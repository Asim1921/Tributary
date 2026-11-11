//! Saving video streams: choosing a quality, and joining separate picture and sound.
//!
//! The parts of each quality stay here in Rust. The window only sees a summary of each
//! choice, and picks one by its position in the list.

use std::env;
use std::ffi::OsStr;
use std::fs;
use std::os::windows::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use engine::{Download, DownloadId, Engine, Login, State};
use media::{Quality, Track};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State as Managed};
use url::Url;

use crate::capture::Captured;
use crate::commands::{AddRequest, AppState, add_download_now};
use crate::ytdlp;

const FFMPEG_FILE: &str = "ffmpeg.exe";
const DEFAULT_NAME: &str = "video";

/// Keeps ffmpeg from flashing a console window.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoChoice {
    label: String,
    width: Option<u64>,
    height: Option<u64>,
    estimated_bytes: Option<u64>,
    seconds: f64,
    extension: String,
    separate_audio: bool,
}

impl From<&Quality> for VideoChoice {
    fn from(quality: &Quality) -> Self {
        Self {
            label: quality.label.clone(),
            width: quality.width,
            height: quality.height,
            estimated_bytes: quality.estimated_bytes(),
            seconds: quality.video.seconds,
            extension: quality.video.extension.clone(),
            separate_audio: quality.audio.is_some(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoDetails {
    choices: Vec<VideoChoice>,
    /// Whether separate picture and sound can be joined into one file.
    can_join: bool,
    suggested_name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddVideoRequest {
    url: String,
    /// Which of the offered choices, by position.
    choice: usize,
    dest_dir: PathBuf,
    /// Without an extension, because that depends on the choice.
    name: String,
    #[serde(default)]
    later: bool,
    capture_token: Option<u64>,
}

/// Where to find ffmpeg. A chosen path is the only one tried, so a mistake there shows
/// up rather than a different copy quietly being used.
pub fn find_ffmpeg(chosen: &str, search_path: Option<&OsStr>) -> Option<PathBuf> {
    let chosen = chosen.trim();
    if !chosen.is_empty() {
        return Some(PathBuf::from(chosen)).filter(|path| path.is_file());
    }
    env::split_paths(search_path?)
        .map(|folder| folder.join(FFMPEG_FILE))
        .find(|path| path.is_file())
}

fn ffmpeg_for(state: &AppState) -> Option<PathBuf> {
    let chosen = state.settings_snapshot().ffmpeg_path;
    find_ffmpeg(&chosen, env::var_os("PATH").as_deref())
}

/// A name for the saved file: the page's title if there is one, otherwise the address.
fn suggested_name(title: Option<&str>, url: &str) -> String {
    let title = title.map(str::trim).filter(|title| !title.is_empty());
    if let Some(title) = title {
        return title.to_owned();
    }

    let file = Url::parse(url).ok().and_then(|url| {
        url.path_segments()?
            .next_back()
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
    });
    match file {
        Some(file) => file
            .rsplit_once('.')
            .map_or(file.clone(), |(stem, _)| stem.to_owned()),
        None => DEFAULT_NAME.to_owned(),
    }
}

/// What a page or stream offers, and what to call it.
struct Offer {
    qualities: Vec<Quality>,
    title: Option<String>,
}

/// Reads a web page's video through yt-dlp. No headers or logins go with it, so a cookie
/// the browser held can never reach it.
async fn read_page(state: &AppState, url: &str) -> Result<Offer, String> {
    let program = ytdlp::program_for(state).ok_or_else(|| ytdlp::MISSING.to_owned())?;
    let page = ytdlp::inspect(&program, url)
        .await
        .map_err(|error| error.to_string())?;
    Ok(Offer {
        qualities: page.qualities,
        title: page.title,
    })
}

async fn read_qualities(
    engine: &Engine,
    url: &str,
    headers: &[(String, String)],
    login: Option<Login>,
) -> Result<Vec<Quality>, String> {
    let fetch = |address: String| {
        let engine = engine.clone();
        let headers = headers.to_vec();
        let login = login.clone();
        async move { engine.fetch_text(&address, &headers, login).await }
    };
    media::discover(url, fetch)
        .await
        .map_err(|error| error.to_string())
}

/// Where one quality is going.
struct Destination<'a> {
    url: &'a str,
    name: &'a str,
    folder: Option<PathBuf>,
    later: bool,
    headers: &'a [(String, String)],
}

fn add_track(
    state: &AppState,
    to: &Destination,
    track: &Track,
    file_name: String,
) -> Result<i64, String> {
    // A quality that's simply a file gets the full treatment an ordinary download has.
    let (url, parts) = match track.parts.as_slice() {
        [only] if track.is_single_file() => (only.url.clone(), None),
        _ => (to.url.to_owned(), Some(track.parts.clone())),
    };

    let request = AddRequest::for_track(url, to.folder.clone(), file_name, parts);
    let request = if to.later { request.later() } else { request };
    add_download_now(state, request, to.headers.to_vec())
}

/// Adds a quality to the list, as two linked downloads if its sound is separate.
fn save_quality(state: &AppState, quality: &Quality, to: &Destination) -> Result<Vec<i64>, String> {
    let video_name = format!("{}.{}", to.name, quality.video.extension);
    let video = add_track(state, to, &quality.video, video_name)?;

    let Some(audio) = &quality.audio else {
        return Ok(vec![video]);
    };
    let audio_name = format!("{}.audio.{}", to.name, audio.extension);
    let sound = add_track(state, to, audio, audio_name)?;

    state
        .engine
        .link_companions(DownloadId(video), DownloadId(sound))
        .map_err(|error| error.to_string())?;
    Ok(vec![video, sound])
}

/// Reads a stream for the video dialog.
#[tauri::command]
pub async fn inspect_video(
    state: Managed<'_, AppState>,
    url: String,
    file_name: Option<String>,
    capture_token: Option<u64>,
    page: Option<bool>,
) -> Result<VideoDetails, String> {
    let url = url.trim();

    let offer = if page == Some(true) {
        read_page(&state, url).await?
    } else {
        let headers = capture_token.map_or_else(Vec::new, |token| state.pending().headers(token));
        let login = state.site_login_for(url);
        Offer {
            qualities: read_qualities(&state.engine, url, &headers, login).await?,
            title: None,
        }
    };

    let choices = offer.qualities.iter().map(VideoChoice::from).collect();
    *state.video_choices() = offer.qualities;

    // The site's own title beats the tab's, which has the site's name tacked on.
    let named = offer.title.as_deref().or(file_name.as_deref());
    Ok(VideoDetails {
        choices,
        can_join: ffmpeg_for(&state).is_some(),
        suggested_name: suggested_name(named, url),
    })
}

/// Saves the chosen quality.
#[tauri::command]
pub async fn add_video(
    app: AppHandle,
    state: Managed<'_, AppState>,
    request: AddVideoRequest,
) -> Result<(), String> {
    let quality = state
        .video_choices()
        .get(request.choice)
        .cloned()
        .ok_or("that quality is no longer available")?;

    let captured = request
        .capture_token
        .and_then(|token| state.pending().take(token))
        .and_then(|files| files.into_iter().next())
        .map(|file| file.headers())
        .unwrap_or_default();
    // A source that names its own headers gets exactly those. What the browser held,
    // cookies included, belongs to the page and never goes to another server.
    let headers = quality.headers.clone().unwrap_or(captured);

    let to = Destination {
        url: request.url.trim(),
        name: &request.name,
        folder: Some(request.dest_dir.clone()),
        later: request.later,
        headers: &headers,
    };
    let ids = save_quality(&state, &quality, &to)?;
    join_finished(&app, &ids);
    Ok(())
}

/// Joins picture and sound that already finished. A small video's two files can both be
/// done before they're recorded as belonging together, so completion alone would miss it.
fn join_finished(app: &AppHandle, ids: &[i64]) {
    for id in ids {
        join_if_ready(app, DownloadId(*id));
    }
}

/// Saves the best quality with no questions, for a command line asking for silence.
pub async fn add_best(app: AppHandle, file: Captured, later: bool, quit_when_done: bool) {
    let state = app.state::<AppState>();
    let headers = file.headers();
    let login = state.site_login_for(&file.url);

    let read = if file.page {
        read_page(&state, &file.url).await
    } else {
        read_qualities(&state.engine, &file.url, &headers, login)
            .await
            .map(|qualities| Offer {
                qualities,
                title: None,
            })
    };

    let saved = match read {
        Ok(offer) => {
            let name = suggested_name(
                offer.title.as_deref().or(file.file_name.as_deref()),
                &file.url,
            );
            let own = offer
                .qualities
                .first()
                .and_then(|best| best.headers.clone());
            let to = Destination {
                url: &file.url,
                name: &name,
                folder: file.folder.clone(),
                later,
                headers: own.as_deref().unwrap_or(&headers),
            };
            offer
                .qualities
                .first()
                .ok_or_else(|| "the stream lists nothing to save".to_owned())
                .and_then(|best| save_quality(&state, best, &to))
        }
        Err(problem) => Err(problem),
    };

    match saved {
        Ok(ids) => {
            if quit_when_done {
                ids.iter().for_each(|id| state.quit_after(*id));
            }
            join_finished(&app, &ids);
        }
        Err(problem) => {
            let _ = app.emit("video-problem", problem);
        }
    }
}

/// Joins picture and sound without re-encoding, then replaces the picture file with the
/// result and removes the sound file. Returns the new size. On failure both files stay.
/// How the joining went. The video is joined whatever happens to the sound file.
#[derive(Debug)]
struct Joined {
    size: u64,
    /// Set when the sound file couldn't be deleted afterwards.
    leftover: Option<String>,
}

/// How long a file that was just read may stay locked, as it can be while a virus scanner
/// looks at it.
const DELETE_ATTEMPTS: u32 = 30;
const DELETE_WAIT: Duration = Duration::from_millis(100);

/// Deletes a file, trying again for a moment if something else still has it open.
fn remove_patiently(path: &Path, attempts: u32, wait: Duration) -> Result<(), String> {
    let mut last = String::new();
    for attempt in 0..attempts.max(1) {
        match fs::remove_file(path) {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => last = error.to_string(),
        }
        if attempt + 1 < attempts {
            std::thread::sleep(wait);
        }
    }
    Err(last)
}

fn join_files(ffmpeg: &Path, video: &Path, audio: &Path) -> Result<Joined, String> {
    let extension = video
        .extension()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    // ffmpeg picks the format from the name, so the extension has to stay last.
    let output = video.with_extension(format!("joining.{extension}"));

    let status = Command::new(ffmpeg)
        .args(["-y", "-loglevel", "error", "-i"])
        .arg(video)
        .arg("-i")
        .arg(audio)
        .args(["-map", "0:v:0", "-map", "1:a:0", "-c", "copy"])
        .arg(&output)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map_err(|error| format!("ffmpeg couldn't start: {error}"))?;

    if !status.success() {
        let _ = fs::remove_file(&output);
        return Err("ffmpeg couldn't join the picture and sound, so they stay as two files".into());
    }

    let size = fs::metadata(&output)
        .map_err(|error| format!("the joined file can't be read: {error}"))?
        .len();
    fs::rename(&output, video).map_err(|error| format!("could not save the file: {error}"))?;
    let leftover = remove_patiently(audio, DELETE_ATTEMPTS, DELETE_WAIT).err();
    Ok(Joined { size, leftover })
}

/// Both halves of a video, once both have finished. The picture is added first, so it
/// has the lower id.
fn finished_pair(state: &AppState, id: DownloadId) -> Option<(Download, Download)> {
    let this = state.engine.get(id).ok()?;
    let other = state.engine.get(this.companion?).ok()?;
    if this.state != State::Completed || other.state != State::Completed {
        return None;
    }
    Some(if this.id < other.id {
        (this, other)
    } else {
        (other, this)
    })
}

/// Joins a video's picture and sound if both are ready and ffmpeg is available. Without
/// ffmpeg they simply stay as two files.
pub fn join_if_ready(app: &AppHandle, id: DownloadId) {
    let state = app.state::<AppState>();
    let Some((video, audio)) = finished_pair(&state, id) else {
        return;
    };
    let (Some(video_path), Some(audio_path)) = (video.final_path, audio.final_path) else {
        return;
    };
    let Some(ffmpeg) = ffmpeg_for(&state) else {
        return;
    };
    if !state.start_joining(video.id.0) {
        return;
    }

    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let joined = tauri::async_runtime::spawn_blocking(move || {
            join_files(&ffmpeg, &video_path, &audio_path)
        })
        .await
        .map_err(|error| error.to_string())
        .and_then(|result| result);

        let state = handle.state::<AppState>();
        let finished = match joined {
            Ok(joined) => {
                let replaced = state.engine.file_replaced(video.id, joined.size);
                let removed = state.engine.remove(audio.id, false).await;
                replaced
                    .and(removed)
                    .map_err(|error| error.to_string())
                    // The video is fine, but the user should know a file was left behind.
                    .and_then(|()| match joined.leftover {
                        Some(reason) => Err(format!(
                            "The video is joined, but the separate sound file couldn't be deleted: {reason}"
                        )),
                        None => Ok(()),
                    })
            }
            Err(problem) => Err(problem),
        };

        state.finish_joining(video.id.0);
        if let Err(problem) = finished {
            let _ = handle.emit("video-problem", problem);
        }
        let _ = handle.emit("queues-changed", ());
    });
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::*;

    #[test]
    fn a_page_title_makes_the_best_name() {
        assert_eq!(
            suggested_name(Some("  My film "), "https://x.test/a/master.m3u8"),
            "My film"
        );
    }

    #[test]
    fn without_a_title_the_name_comes_from_the_address() {
        let name = |url| suggested_name(None, url);

        assert_eq!(name("https://x.test/a/master.m3u8?t=1"), "master");
        assert_eq!(name("https://x.test/stream"), "stream");
        assert_eq!(suggested_name(Some("   "), "https://x.test/b.mpd"), "b");
        assert_eq!(name("https://x.test/"), DEFAULT_NAME);
        assert_eq!(name("not an address"), DEFAULT_NAME);
    }

    #[test]
    fn a_chosen_ffmpeg_is_the_only_one_tried() {
        let folder = tempfile::tempdir().unwrap();
        let found = folder.path().join(FFMPEG_FILE);
        fs::write(&found, b"").unwrap();
        let search = OsString::from(folder.path());

        assert_eq!(find_ffmpeg("", Some(&search)), Some(found.clone()));
        assert_eq!(
            find_ffmpeg(&found.to_string_lossy(), None),
            Some(found.clone())
        );
        assert_eq!(find_ffmpeg(r"C:\no\such\ffmpeg.exe", Some(&search)), None);
    }

    #[test]
    fn ffmpeg_may_simply_not_be_there() {
        let empty = tempfile::tempdir().unwrap();
        let search = OsString::from(empty.path());

        assert_eq!(find_ffmpeg("", Some(&search)), None);
        assert_eq!(find_ffmpeg("", None), None);
    }

    #[test]
    fn a_failed_join_keeps_both_files() {
        let folder = tempfile::tempdir().unwrap();
        let video = folder.path().join("a.mp4");
        let audio = folder.path().join("a.audio.m4a");
        fs::write(&video, b"picture").unwrap();
        fs::write(&audio, b"sound").unwrap();

        // Any program that fails stands in for ffmpeg giving up.
        let failing = PathBuf::from(env::var_os("ComSpec").unwrap());
        assert!(join_files(&failing, &video, &audio).is_err());

        assert_eq!(fs::read(&video).unwrap(), b"picture");
        assert_eq!(fs::read(&audio).unwrap(), b"sound");
    }

    #[test]
    fn a_file_still_open_elsewhere_is_deleted_once_it_is_let_go() {
        use std::os::windows::fs::OpenOptionsExt as _;

        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("sound.m4a");
        fs::write(&path, b"sound").unwrap();

        // Nothing else may delete or share it while this handle is open.
        let holder = fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&path)
            .unwrap();
        let release = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(250));
            drop(holder);
        });

        let removed = remove_patiently(&path, 50, Duration::from_millis(20));
        release.join().unwrap();

        assert_eq!(removed, Ok(()));
        assert!(!path.exists());
    }

    #[test]
    fn a_file_that_stays_locked_is_reported_rather_than_ignored() {
        use std::os::windows::fs::OpenOptionsExt as _;

        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("sound.m4a");
        fs::write(&path, b"sound").unwrap();
        let _holder = fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&path)
            .unwrap();

        let removed = remove_patiently(&path, 3, Duration::from_millis(10));

        assert!(removed.is_err());
        assert!(path.exists());
    }

    #[test]
    fn a_file_that_is_already_gone_counts_as_deleted() {
        let folder = tempfile::tempdir().unwrap();
        assert_eq!(
            remove_patiently(&folder.path().join("none"), 3, Duration::from_millis(1)),
            Ok(())
        );
    }

    #[test]
    fn a_missing_program_is_explained() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("a.mp4");

        let problem = join_files(Path::new(r"C:\no\such\ffmpeg.exe"), &path, &path).unwrap_err();
        assert!(problem.contains("couldn't start"), "{problem}");
    }
}
