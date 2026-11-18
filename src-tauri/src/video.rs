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

/// The page holding an embedded player, when it's worth reading instead of the player. A
/// refusal on principle, such as a protected or live video, would only be repeated.
fn host_to_try<'a>(url: &str, host: Option<&'a str>, problem: &media::Error) -> Option<&'a str> {
    let principled = matches!(
        problem,
        media::Error::Protected
            | media::Error::Live
            | media::Error::LoginRequired
            | media::Error::Playlist
    );
    host.map(str::trim)
        .filter(|host| !principled && !host.is_empty() && *host != url.trim())
}

/// Reads a web page's video through yt-dlp. No cookies or logins go with it, so nothing the
/// browser held can reach it. A player embedded in another page is asked about with that
/// page as the referer, and the page itself is read when the player can't be.
async fn read_page(
    state: &AppState,
    url: &str,
    referer: Option<&str>,
    host: Option<&str>,
) -> Result<Offer, String> {
    let program = ytdlp::program_for(state).ok_or_else(|| ytdlp::MISSING.to_owned())?;
    let page = match ytdlp::inspect(&program, url, referer).await {
        Ok(page) => page,
        // A player that can't be read on its own may be found in the page holding it.
        Err(problem) => match host_to_try(url, host, &problem) {
            Some(host) => ytdlp::inspect(&program, host, None)
                .await
                // The player's own answer says more about what went wrong.
                .map_err(|_| problem.to_string())?,
            None => return Err(problem.to_string()),
        },
    };

    // A page whose best is only offered as HLS: Tributary reads the stream itself, which
    // also refuses it if it's encrypted, and asks for it with only the headers yt-dlp named.
    let mut qualities = page.qualities;
    if let Some(stream) = page.stream {
        match read_stream(&state.engine, &stream).await {
            Ok(found) => qualities = found,
            Err(problem) if qualities.is_empty() => return Err(problem),
            // The plain files are still there to offer.
            Err(_) => {}
        }
    }
    Ok(Offer {
        qualities,
        title: page.title,
    })
}

/// Reads a stream spotted in a page. One that can't be read on its own, say because its
/// site turns away anything but a browser, is looked up through the page holding it. A
/// refusal on principle, such as protection, isn't.
async fn read_spotted(
    state: &AppState,
    url: &str,
    headers: &[(String, String)],
    login: Option<Login>,
    host: Option<&str>,
) -> Result<Offer, String> {
    let fetch = |address: String| {
        let engine = state.engine.clone();
        let headers = headers.to_vec();
        let login = login.clone();
        async move { engine.fetch_text(&address, &headers, login).await }
    };
    match media::discover(url, fetch).await {
        Ok(qualities) => Ok(Offer {
            qualities,
            title: None,
        }),
        Err(problem) => match host_to_try(url, host, &problem) {
            // The stream's own answer says more about what went wrong.
            Some(host) => read_page(state, host, None, None)
                .await
                .map_err(|_| problem.to_string()),
            None => Err(problem.to_string()),
        },
    }
}

async fn discover_stream(
    engine: &Engine,
    url: &str,
    headers: &[(String, String)],
) -> Result<Vec<Quality>, media::Error> {
    let fetch = |address: String| {
        let engine = engine.clone();
        let headers = headers.to_vec();
        async move { engine.fetch_text(&address, &headers, None).await }
    };
    media::discover(url, fetch).await
}

/// Reads an HLS stream yt-dlp found, with only the headers it named. Some sites turn away
/// anything but a browser at the list of every quality, while each quality's own list,
/// which yt-dlp has already read, stays open, so those are read one by one instead.
/// Tributary doesn't pretend to be a browser. A protected quality refuses the whole stream.
async fn read_stream(
    engine: &Engine,
    stream: &media::ytdlp::Stream,
) -> Result<Vec<Quality>, String> {
    let mut found = match discover_stream(engine, &stream.url, &stream.headers).await {
        Ok(found) => found,
        Err(media::Error::Protected) => return Err(media::Error::Protected.to_string()),
        Err(problem) => {
            let mut found = Vec::new();
            for variant in &stream.variants {
                match discover_stream(engine, &variant.url, &stream.headers).await {
                    Ok(mut one) if !one.is_empty() => {
                        let mut quality = one.remove(0);
                        quality.label = variant.label.clone();
                        quality.height = Some(variant.class);
                        quality.bandwidth = variant.bandwidth.or(quality.bandwidth);
                        found.push(quality);
                    }
                    Err(media::Error::Protected) => return Err(media::Error::Protected.to_string()),
                    // One quality that can't be read leaves the others.
                    _ => {}
                }
            }
            if found.is_empty() {
                return Err(problem.to_string());
            }
            found
        }
    };
    for quality in &mut found {
        quality.headers = Some(stream.headers.clone());
    }
    Ok(found)
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
/// Where a video's sound waits to be joined to its picture, out of the user's way. It
/// only ever reaches the user's folder if the two can't be joined.
pub fn sound_folder(settings_path: &Path) -> PathBuf {
    settings_path
        .parent()
        .unwrap_or(Path::new("."))
        .join(SOUND_FOLDER)
}

const SOUND_FOLDER: &str = "sound";

fn save_quality(state: &AppState, quality: &Quality, to: &Destination) -> Result<Vec<i64>, String> {
    let video_name = format!("{}.{}", to.name, quality.video.extension);
    let video = add_track(state, to, &quality.video, video_name)?;

    let Some(audio) = &quality.audio else {
        return Ok(vec![video]);
    };
    let working = sound_folder(&state.settings_path);
    fs::create_dir_all(&working)
        .map_err(|error| format!("couldn't make a folder for the sound: {error}"))?;
    let audio_name = format!("{}.audio.{}", to.name, audio.extension);
    let sound_to = Destination {
        url: to.url,
        name: to.name,
        folder: Some(working),
        later: to.later,
        headers: to.headers,
    };
    let sound = add_track(state, &sound_to, audio, audio_name)?;

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
        let (referer, host) = capture_token
            .map(|token| state.pending().page_context(token))
            .unwrap_or_default();
        read_page(&state, url, referer.as_deref(), host.as_deref()).await?
    } else {
        let headers = capture_token.map_or_else(Vec::new, |token| state.pending().headers(token));
        let (_, host) = capture_token
            .map(|token| state.pending().page_context(token))
            .unwrap_or_default();
        let login = state.site_login_for(url);
        read_spotted(&state, url, &headers, login, host.as_deref()).await?
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
        read_page(
            &state,
            &file.url,
            file.referrer.as_deref(),
            file.host.as_deref(),
        )
        .await
    } else {
        read_spotted(&state, &file.url, &headers, login, file.host.as_deref()).await
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

/// Where a sound that can't be joined is put: beside its video, under a name nothing
/// there already has.
fn beside(video: &Path, sound_name: &str) -> PathBuf {
    let folder = video.parent().unwrap_or(Path::new("."));
    let wanted = folder.join(sound_name);
    if !wanted.exists() {
        return wanted;
    }
    let name = Path::new(sound_name);
    let stem = name.file_stem().unwrap_or_default().to_string_lossy();
    let extension = name
        .extension()
        .map(|extension| format!(".{}", extension.to_string_lossy()));
    (2..)
        .map(|count| {
            folder.join(format!(
                "{stem} ({count}){}",
                extension.as_deref().unwrap_or("")
            ))
        })
        .find(|path| !path.exists())
        .unwrap_or(wanted)
}

/// Moves a file, copying it when the two places are on different drives.
fn move_file(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::rename(from, to).or_else(|_| {
        fs::copy(from, to)?;
        fs::remove_file(from)
    })
}

/// Puts a sound that can't be joined beside its video, so nothing is lost, and lets the
/// list forget it, since the video stands for both.
async fn keep_apart(
    state: &AppState,
    video_path: &Path,
    audio: &engine::Download,
) -> Result<PathBuf, String> {
    let from = audio.final_path.clone().ok_or("the sound has no file")?;
    let name = from
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "sound".to_owned());
    let to = if from.parent() == video_path.parent() {
        from.clone()
    } else {
        let to = beside(video_path, &name);
        move_file(&from, &to)
            .map_err(|error| format!("the sound couldn't be put beside the video: {error}"))?;
        to
    };
    state
        .engine
        .remove(audio.id, false)
        .await
        .map_err(|error| error.to_string())?;
    Ok(to)
}

/// Joins a video's picture and sound once both are ready. Without ffmpeg, or if they can't
/// be joined, the sound is put beside the video instead, so nothing is lost.
pub fn join_if_ready(app: &AppHandle, id: DownloadId) {
    let state = app.state::<AppState>();
    let Some((video, audio)) = finished_pair(&state, id) else {
        return;
    };
    let (Some(video_path), Some(audio_path)) = (video.final_path.clone(), audio.final_path.clone())
    else {
        return;
    };
    if !state.start_joining(video.id.0) {
        return;
    }
    let handle = app.clone();
    let Some(ffmpeg) = ffmpeg_for(&state) else {
        // The dialog said they'd stay as two files, so that's no surprise worth a message.
        tauri::async_runtime::spawn(async move {
            let state = handle.state::<AppState>();
            let kept = keep_apart(&state, &video_path, &audio).await;
            state.finish_joining(video.id.0);
            if let Err(problem) = kept {
                let _ = handle.emit("video-problem", problem);
            }
            let _ = handle.emit("queues-changed", ());
        });
        return;
    };

    tauri::async_runtime::spawn(async move {
        let joined = tauri::async_runtime::spawn_blocking({
            let video_path = video_path.clone();
            move || join_files(&ffmpeg, &video_path, &audio_path)
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
            // Nothing is lost: the sound goes beside the video, and the message says where.
            Err(problem) => match keep_apart(&state, &video_path, &audio).await {
                Ok(kept) => Err(format!(
                    "{problem}. The sound is saved beside the video, as {}",
                    kept.file_name().unwrap_or_default().to_string_lossy()
                )),
                Err(also) => Err(format!("{problem}. Also, {also}")),
            },
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
    fn a_videos_sound_waits_in_tributarys_own_folder() {
        let settings =
            Path::new(r"C:\Users\me\AppData\Roaming\app.tributary.desktop\settings.json");
        assert_eq!(
            sound_folder(settings),
            Path::new(r"C:\Users\me\AppData\Roaming\app.tributary.desktop\sound")
        );
    }

    #[test]
    fn a_sound_kept_apart_goes_beside_its_video_under_a_free_name() {
        let folder = tempfile::tempdir().unwrap();
        let video = folder.path().join("film.mp4");

        assert_eq!(
            beside(&video, "film.audio.m4a"),
            folder.path().join("film.audio.m4a")
        );
        fs::write(folder.path().join("film.audio.m4a"), b"older").unwrap();
        assert_eq!(
            beside(&video, "film.audio.m4a"),
            folder.path().join("film.audio (2).m4a")
        );
        fs::write(folder.path().join("film.audio (2).m4a"), b"older").unwrap();
        assert_eq!(
            beside(&video, "film.audio.m4a"),
            folder.path().join("film.audio (3).m4a")
        );
    }

    #[test]
    fn a_kept_sound_is_moved_whole() {
        let working = tempfile::tempdir().unwrap();
        let downloads = tempfile::tempdir().unwrap();
        let from = working.path().join("film.audio.m4a");
        fs::write(&from, b"the sound").unwrap();
        let to = downloads.path().join("film.audio.m4a");

        move_file(&from, &to).unwrap();
        assert!(!from.exists());
        assert_eq!(fs::read(&to).unwrap(), b"the sound");
    }

    #[test]
    fn the_page_holding_a_player_is_tried_when_the_player_cannot_be_read() {
        let player = "https://player.test/embed/1";
        let host = Some("https://host.test/film");
        let refused = media::Error::Fetch(media::ytdlp::REFUSED.into());

        assert_eq!(host_to_try(player, host, &refused), host);
        assert_eq!(host_to_try(player, host, &media::Error::NoVideo), host);
    }

    #[test]
    fn a_refusal_on_principle_is_not_tried_again_on_the_host_page() {
        let host = Some("https://host.test/film");
        for problem in [
            media::Error::Protected,
            media::Error::Live,
            media::Error::LoginRequired,
            media::Error::Playlist,
        ] {
            assert_eq!(
                host_to_try("https://player.test/1", host, &problem),
                None,
                "{problem:?}"
            );
        }
    }

    #[test]
    fn there_is_nothing_to_try_without_a_different_host_page() {
        let player = "https://player.test/embed/1";
        let problem = media::Error::NoVideo;

        assert_eq!(host_to_try(player, None, &problem), None);
        assert_eq!(host_to_try(player, Some("  "), &problem), None);
        assert_eq!(host_to_try(player, Some(player), &problem), None);
    }

    #[test]
    fn a_missing_program_is_explained() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("a.mp4");

        let problem = join_files(Path::new(r"C:\no\such\ffmpeg.exe"), &path, &path).unwrap_err();
        assert!(problem.contains("couldn't start"), "{problem}");
    }
}
