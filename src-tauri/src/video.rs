//! Saving video streams: choosing a quality, and joining separate picture and sound.
//!
//! The parts of each quality stay here in Rust. The window only sees a summary of each
//! choice, and picks one by its position in the list.

use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use engine::{Download, DownloadId, Engine, Login, State};
use media::ytdlp::Subtitle;
use media::{Quality, Track};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State as Managed};
use url::Url;

use crate::capture::Captured;
use crate::commands::{AddRequest, AppState, add_download_now};
use crate::finishing::{self, FinishPlan, PlannedSubtitle, beside, move_file, put_extras_beside};
use crate::ytdlp;

const FFMPEG_FILE: &str = "ffmpeg.exe";
const DEFAULT_NAME: &str = "video";

/// The largest cover picture fetched.
const MAX_COVER_BYTES: usize = 10 << 20;

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
    /// Sound with no picture.
    audio_only: bool,
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
            audio_only: quality.audio_only,
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
    /// The subtitles offered, by position. Their addresses stay here in Rust.
    subtitles: Vec<SubtitleChoice>,
    /// Whether the video has a cover picture to save.
    has_cover: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleChoice {
    language: String,
    name: String,
    /// Made by speech recognition rather than written by someone.
    automatic: bool,
}

/// What the video dialog offers beyond the qualities.
#[derive(Debug, Default)]
pub struct VideoExtras {
    subtitles: Vec<Subtitle>,
    thumbnail: Option<String>,
    title: Option<String>,
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
    /// Which of the offered subtitles to save, by position.
    #[serde(default)]
    subtitles: Vec<usize>,
    /// Saves the cover picture too.
    #[serde(default)]
    cover: bool,
    /// Turns sound saved on its own into MP3.
    #[serde(default)]
    mp3: bool,
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
    subtitles: Vec<Subtitle>,
    thumbnail: Option<String>,
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
            Ok(found) => {
                // Sound alone stays on offer beside the stream's pictures.
                let alone: Vec<Quality> = qualities
                    .into_iter()
                    .filter(|quality| quality.audio_only)
                    .collect();
                qualities = found;
                qualities.extend(alone);
            }
            Err(problem) if qualities.is_empty() => return Err(problem),
            // The plain files are still there to offer.
            Err(_) => {}
        }
    }
    Ok(Offer {
        qualities,
        title: page.title,
        subtitles: page.subtitles,
        thumbnail: page.thumbnail,
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
            subtitles: Vec::new(),
            thumbnail: None,
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
    /// The queue for later, when it isn't the main one.
    queue: Option<i64>,
    gallery: bool,
    headers: &'a [(String, String)],
    /// What to do to the video once it's in, saved as a plan.
    finish_plan: Option<String>,
}

fn add_track(
    state: &AppState,
    to: &Destination,
    track: &Track,
    file_name: String,
    finish_plan: Option<String>,
) -> Result<i64, String> {
    // A quality that's simply a file gets the full treatment an ordinary download has.
    let (url, parts) = match track.parts.as_slice() {
        [only] if track.is_single_file() => (only.url.clone(), None),
        _ => (to.url.to_owned(), Some(track.parts.clone())),
    };

    let request = AddRequest::for_track(url, to.folder.clone(), file_name, parts)
        .with_finish_plan(finish_plan);
    let request = if to.later {
        request.later().in_queue(to.queue)
    } else {
        request
    };
    add_download_now(state, request.in_gallery(to.gallery), to.headers.to_vec())
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
    let video = add_track(
        state,
        to,
        &quality.video,
        video_name,
        to.finish_plan.clone(),
    )?;

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
        queue: to.queue,
        gallery: to.gallery,
        headers: to.headers,
        finish_plan: None,
    };
    let sound = add_track(state, &sound_to, audio, audio_name, None)?;

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
    let subtitles = offer
        .subtitles
        .iter()
        .map(|subtitle| SubtitleChoice {
            language: subtitle.language.clone(),
            name: subtitle.name.clone(),
            automatic: subtitle.automatic,
        })
        .collect();
    let has_cover = offer.thumbnail.is_some();
    *state.video_extras() = VideoExtras {
        subtitles: offer.subtitles,
        thumbnail: offer.thumbnail,
        title: offer.title.clone(),
    };

    // The site's own title beats the tab's, which has the site's name tacked on.
    let named = offer.title.as_deref().or(file_name.as_deref());
    Ok(VideoDetails {
        choices,
        can_join: ffmpeg_for(&state).is_some(),
        suggested_name: suggested_name(named, url),
        subtitles,
        has_cover,
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
    if request.mp3 && !quality.audio_only {
        return Err("only sound saved on its own can be made into an MP3".to_owned());
    }
    if request.mp3 && ffmpeg_for(&state).is_none() {
        return Err("Making an MP3 needs ffmpeg. Set it up under Options, Video.".to_owned());
    }
    let (plan, missed) = fetch_extras(&state, &request, quality.audio_only).await;

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
        queue: None,
        gallery: false,
        headers: &headers,
        finish_plan: (!plan.is_empty()).then(|| plan.write()),
    };
    let ids = save_quality(&state, &quality, &to).inspect_err(|_| {
        for file in plan.fetched_files() {
            let _ = fs::remove_file(file);
        }
    })?;
    finish_all_ready(&app, &ids);
    if !missed.is_empty() {
        let _ = app.emit(
            "video-problem",
            format!(
                "Tributary couldn't fetch {}, so the video is being saved without {}.",
                missed.join(" or "),
                if missed.len() == 1 { "it" } else { "them" }
            ),
        );
    }
    Ok(())
}

/// A name no other fetched file has.
fn unique_stamp() -> String {
    static COUNT: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or_default();
    format!("{nanos}-{}", COUNT.fetch_add(1, Ordering::Relaxed))
}

/// The kind of picture an address names, for a cover's file name.
fn picture_extension(url: &str) -> &'static str {
    let path = Url::parse(url)
        .map(|url| url.path().to_ascii_lowercase())
        .unwrap_or_default();
    if path.ends_with(".png") {
        "png"
    } else if path.ends_with(".webp") {
        "webp"
    } else {
        "jpg"
    }
}

/// Fetches the subtitles and cover asked for into Tributary's own folder, now rather than
/// later, because their addresses can expire before a video saved for later starts. No
/// cookies go with them. Returns the plan for the video, and what couldn't be fetched.
async fn fetch_extras(
    state: &AppState,
    request: &AddVideoRequest,
    audio_only: bool,
) -> (FinishPlan, Vec<String>) {
    let (subtitles, thumbnail, title) = {
        let extras = state.video_extras();
        let subtitles: Vec<Subtitle> = request
            .subtitles
            .iter()
            .filter_map(|index| extras.subtitles.get(*index).cloned())
            .collect();
        (subtitles, extras.thumbnail.clone(), extras.title.clone())
    };
    let mut plan = FinishPlan {
        mp3: request.mp3 && audio_only,
        audio_only,
        title,
        ..FinishPlan::default()
    };
    let mut missed = Vec::new();
    let wanted_cover = thumbnail.filter(|_| request.cover);
    // Subtitles are for pictures.
    let subtitles = if audio_only { Vec::new() } else { subtitles };
    if subtitles.is_empty() && wanted_cover.is_none() {
        return (plan, missed);
    }

    let folder = sound_folder(&state.settings_path);
    let _ = fs::create_dir_all(&folder);
    let stamp = unique_stamp();
    for (number, subtitle) in subtitles.iter().enumerate() {
        let path = folder.join(format!("{stamp}-{number}.{}", subtitle.extension));
        match state.engine.fetch_text(&subtitle.url, &[], None).await {
            Ok(text) if fs::write(&path, &text).is_ok() => plan.subtitles.push(PlannedSubtitle {
                path,
                language: subtitle.language.clone(),
                name: subtitle.name.clone(),
            }),
            _ => missed.push(format!("the {} subtitles", subtitle.name)),
        }
    }
    if let Some(url) = wanted_cover {
        let path = folder.join(format!("{stamp}-cover.{}", picture_extension(&url)));
        match state.engine.fetch_bytes(&url, &[], MAX_COVER_BYTES).await {
            Ok(bytes) if fs::write(&path, &bytes).is_ok() => plan.cover = Some(path),
            _ => missed.push("the cover picture".to_owned()),
        }
    }
    (plan, missed)
}

/// Finishes videos whose files are already in. A small video's two files can both be done
/// before they're recorded as belonging together, so completion alone would miss it.
fn finish_all_ready(app: &AppHandle, ids: &[i64]) {
    for id in ids {
        finish_if_ready(app, DownloadId(*id));
    }
}

/// Saves the best quality with no questions, for a command line asking for silence.
/// Saves the best quality of a stream, or of the video on a page, without asking. A
/// problem shows as a message.
pub async fn add_best(app: AppHandle, file: Captured, later: bool, quit_when_done: bool) {
    match save_best(&app, file, later).await {
        Ok(ids) if quit_when_done => {
            let state = app.state::<AppState>();
            ids.iter().for_each(|id| state.quit_after(*id));
        }
        Ok(_) => {}
        Err(problem) => {
            let _ = app.emit("video-problem", problem);
        }
    }
}

/// Saves the best quality of a stream, or of the video on a page. Returns the downloads
/// added, or why there's nothing to save.
pub async fn save_best(app: &AppHandle, file: Captured, later: bool) -> Result<Vec<i64>, String> {
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
                queue: file.queue,
                gallery: file.gallery,
                headers: own.as_deref().unwrap_or(&headers),
                finish_plan: None,
            };
            offer
                .qualities
                .first()
                .ok_or_else(|| "the stream lists nothing to save".to_owned())
                .and_then(|best| save_quality(&state, best, &to))
        }
        Err(problem) => Err(problem),
    };

    if let Ok(ids) = &saved {
        finish_all_ready(app, ids);
    }
    saved
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

/// A video ready to be finished: its picture, its separate sound if it has one, and what
/// else was asked for.
struct Job {
    video: Download,
    sound: Option<Download>,
    plan: FinishPlan,
}

/// What's to be done to a video, once everything it needs has finished.
fn job_for(state: &AppState, id: DownloadId) -> Option<Job> {
    let this = state.engine.get(id).ok()?;
    let (video, sound) = match this.companion {
        // The picture is added first, so it has the lower id.
        Some(other) => {
            let other = state.engine.get(other).ok()?;
            if this.id < other.id {
                (this, Some(other))
            } else {
                (other, Some(this))
            }
        }
        None => (this, None),
    };
    let ready = video.state == State::Completed
        && sound
            .as_ref()
            .is_none_or(|sound| sound.state == State::Completed);
    let plan = video
        .finish_plan
        .as_deref()
        .and_then(FinishPlan::read)
        .unwrap_or_default();
    (ready && (sound.is_some() || !plan.is_empty())).then_some(Job { video, sound, plan })
}

/// Finishes a video once everything it needs is in: joins its separate sound, adds the
/// subtitles and cover asked for, or makes an MP3. Without ffmpeg, or if it fails, what
/// couldn't go in is put beside the video, so nothing is lost.
pub fn finish_if_ready(app: &AppHandle, id: DownloadId) {
    let state = app.state::<AppState>();
    let Some(job) = job_for(&state, id) else {
        return;
    };
    let label = if job.sound.is_some() {
        "Adding sound…"
    } else {
        job.plan.label()
    };
    if !state.start_joining(job.video.id.0, label) {
        return;
    }

    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let finished = finish(&handle, &job).await;
        let state = handle.state::<AppState>();
        let _ = state.engine.set_finish_plan(job.video.id, None);
        // Whatever was fetched for the video and found no place is Tributary's to tidy.
        for file in job.plan.fetched_files() {
            let _ = fs::remove_file(file);
        }
        state.finish_joining(job.video.id.0);
        crate::notices::finished(&handle, job.video.id);
        if let Err(problem) = finished {
            let _ = handle.emit("video-problem", problem);
        }
        let _ = handle.emit("queues-changed", ());
    });
}

async fn finish(app: &AppHandle, job: &Job) -> Result<(), String> {
    let state = app.state::<AppState>();
    let video_path = job
        .video
        .final_path
        .clone()
        .ok_or("the video has no file")?;

    let Some(ffmpeg) = ffmpeg_for(&state) else {
        // The dialog said they'd stay apart, so that's no surprise worth a message.
        let placed = put_extras_beside(&video_path, &job.plan, false);
        if let Some(sound) = &job.sound {
            keep_apart(&state, &video_path, sound).await?;
        }
        placed?;
        return if job.plan.mp3 {
            Err("ffmpeg isn't set up any more, so the sound is saved as it came".to_owned())
        } else {
            Ok(())
        };
    };

    let finished = tauri::async_runtime::spawn_blocking({
        let video_path = video_path.clone();
        let sound_path = job
            .sound
            .as_ref()
            .and_then(|sound| sound.final_path.clone());
        let plan = job.plan.clone();
        move || finishing::finish_files(&ffmpeg, &video_path, sound_path.as_deref(), &plan)
    })
    .await
    .map_err(|error| error.to_string())
    .and_then(|result| result);

    match finished {
        Ok(finished) => {
            let recorded = if finished.path == video_path {
                state.engine.file_replaced(job.video.id, finished.size)
            } else {
                state
                    .engine
                    .file_moved(job.video.id, &finished.path, finished.size)
            };
            recorded.map_err(|error| error.to_string())?;
            if let Some(sound) = &job.sound {
                state
                    .engine
                    .remove(sound.id, false)
                    .await
                    .map_err(|error| error.to_string())?;
            }
            // The video is fine, but the user should know a file was left behind.
            match finished.leftover {
                Some(reason) => Err(format!(
                    "The video is joined, but the separate sound file couldn't be deleted: {reason}"
                )),
                None => Ok(()),
            }
        }
        // Nothing is lost: what couldn't go in goes beside the video, and the message says
        // where.
        Err(problem) => {
            let placed = put_extras_beside(&video_path, &job.plan, false);
            let mut message = problem;
            if let Some(sound) = &job.sound {
                match keep_apart(&state, &video_path, sound).await {
                    Ok(kept) => {
                        message = format!(
                            "{message}. The sound is saved beside the video, as {}",
                            kept.file_name().unwrap_or_default().to_string_lossy()
                        );
                    }
                    Err(also) => message = format!("{message}. Also, {also}"),
                }
            }
            match placed {
                Ok(placed) if !placed.is_empty() => {
                    message.push_str(". The subtitles and cover are saved beside it");
                }
                Ok(_) => {}
                Err(also) => message = format!("{message}. Also, {also}"),
            }
            Err(message)
        }
    }
}

/// Finishes any video whose finishing was cut short, say by the app closing.
pub fn finish_leftovers(app: &AppHandle) {
    let state = app.state::<AppState>();
    let Ok(downloads) = state.engine.list() else {
        return;
    };
    for download in downloads.iter().filter(|download| {
        download.state == State::Completed
            && (download.companion.is_some() || download.finish_plan.is_some())
    }) {
        finish_if_ready(app, download.id);
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::time::Duration;

    use crate::finishing::remove_patiently;

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
}
