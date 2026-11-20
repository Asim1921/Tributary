//! Reads what yt-dlp reports about a web page's video, and applies the same refusals as
//! for any other stream.
//!
//! Only formats that are plain files on a web server are used, so the engine fetches them
//! like any download. Protected, live and login-only videos are refused, and the list of
//! request headers handed on is a short fixed one that can never include a cookie.

use serde_json::Value;

use crate::{Error, Part, Quality, Track, video_label};

/// Request headers worth passing on. Anything else, cookies above all, is dropped.
const PASSED_HEADERS: [&str; 4] = ["user-agent", "referer", "origin", "accept-language"];

/// What yt-dlp says about `availability` when a video can't be had without an account.
const NEEDS_ACCOUNT: [&str; 4] = ["needs_auth", "premium_only", "subscriber_only", "private"];

const MAX_FORMATS: usize = 400;

/// A page's video, as found.
#[derive(Debug, Clone, PartialEq)]
pub struct Page {
    pub title: Option<String>,
    pub qualities: Vec<Quality>,
    /// An HLS stream holding better qualities than any plain file, for Tributary to read
    /// itself. Reading it refuses an encrypted stream, as any HLS address does.
    pub stream: Option<Stream>,
    /// Subtitles people wrote, then automatic ones, each group by name.
    pub subtitles: Vec<Subtitle>,
    /// The video's cover picture.
    pub thumbnail: Option<String>,
}

/// A subtitle track a page offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subtitle {
    /// The language code the site gives, such as `en` or `pt-BR`.
    pub language: String,
    /// What it's called, such as `English`.
    pub name: String,
    pub url: String,
    /// `vtt`, `srt` or `ass`, which ffmpeg can all read.
    pub extension: String,
    /// Made by speech recognition rather than written by someone.
    pub automatic: bool,
}

/// Subtitle formats ffmpeg can put into a video, best first.
const SUBTITLE_FORMATS: [&str; 3] = ["vtt", "srt", "ass"];

/// The most subtitles offered, so a page can't fill the dialog.
const MAX_SUBTITLES: usize = 80;

/// The subtitles a page offers, in a format that can go into a video.
fn subtitles(page: &Value) -> Vec<Subtitle> {
    let mut found = Vec::new();
    for (key, automatic) in [("subtitles", false), ("automatic_captions", true)] {
        let Some(languages) = page.get(key).and_then(Value::as_object) else {
            continue;
        };
        let mut these: Vec<Subtitle> = languages
            .iter()
            // A live stream's chat replay is listed as a subtitle, but isn't one.
            .filter(|(language, _)| language.as_str() != "live_chat")
            .filter_map(|(language, entries)| {
                let entries = entries.as_array()?;
                let entry = SUBTITLE_FORMATS.iter().find_map(|wanted| {
                    entries
                        .iter()
                        .find(|entry| text(entry, "ext") == Some(*wanted))
                })?;
                let url = text(entry, "url").filter(|url| url.starts_with("http"))?;
                let name = text(entry, "name")
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                    .unwrap_or(language);
                Some(Subtitle {
                    language: language.clone(),
                    name: name.to_owned(),
                    url: url.to_owned(),
                    extension: text(entry, "ext")?.to_owned(),
                    automatic,
                })
            })
            .collect();
        if automatic {
            // YouTube offers its automatic captions translated by machine into every
            // language, marked with `tlang` in the address. Only the video's own are worth
            // offering, where those can be told apart.
            let own: Vec<Subtitle> = these
                .iter()
                .filter(|subtitle| !subtitle.url.contains("tlang="))
                .cloned()
                .collect();
            if !own.is_empty() {
                these = own;
            }
        }
        these.sort_by_key(|subtitle| subtitle.name.to_lowercase());
        found.extend(these);
    }
    found.truncate(MAX_SUBTITLES);
    found
}

/// What to call a sound saved on its own, beside the pictures offered.
fn audio_only_label(bits: Option<u64>, extension: &str) -> String {
    let kind = extension.to_ascii_uppercase();
    match bits {
        Some(bits) => format!("Audio only, {} kbps ({kind})", bits / 1000),
        None => format!("Audio only ({kind})"),
    }
}

/// A stream's address, and the only headers to send for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stream {
    pub url: String,
    pub headers: Vec<(String, String)>,
    /// Each quality by its own list, best first, for a site that turns away anything but a
    /// browser at the list of every quality. Only qualities with their sound in them.
    pub variants: Vec<Variant>,
}

/// One quality of an HLS stream, by its own list, as yt-dlp found it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variant {
    pub url: String,
    pub label: String,
    pub class: u64,
    pub bandwidth: Option<u64>,
}

/// The most qualities read one by one.
const MAX_VARIANTS: usize = 12;

fn variants(formats: &[Value]) -> Vec<Variant> {
    let mut found: Vec<Variant> = formats
        .iter()
        .filter(|format| {
            is_hls(format) && has_codec(format, "vcodec") && has_codec(format, "acodec")
        })
        .filter_map(|format| {
            let item = candidate(format)?;
            Some(Variant {
                url: item.url.to_owned(),
                label: label(&item),
                class: item.class,
                bandwidth: Some((item.bitrate * 1000.0) as u64).filter(|bits| *bits > 0),
            })
        })
        .filter(|variant| variant.class > 0)
        .collect();
    found.sort_by_key(|variant| std::cmp::Reverse((variant.class, variant.bandwidth)));
    let mut seen = Vec::new();
    found.retain(|variant| {
        let new = !seen.contains(&variant.label);
        seen.push(variant.label.clone());
        new
    });
    found.truncate(MAX_VARIANTS);
    found
}

fn is_hls(format: &Value) -> bool {
    text(format, "protocol").is_some_and(|protocol| protocol.starts_with("m3u8"))
}

/// The best picture offered as HLS, with the list that holds every quality of it.
fn best_hls(formats: &[Value]) -> Option<(u64, Stream)> {
    formats
        .iter()
        .filter(|format| is_hls(format) && has_codec(format, "vcodec"))
        .filter_map(|format| {
            let height = number(format, "height").map(|height| height as u64);
            let width = number(format, "width").map(|width| width as u64);
            let class = crate::picture_class(width, height)?;
            let url = text(format, "manifest_url")
                .or_else(|| text(format, "url"))
                .filter(|url| url.starts_with("http"))?;
            let headers = safe_headers(format.get("http_headers").unwrap_or(&Value::Null));
            Some((
                class,
                Stream {
                    url: url.to_owned(),
                    headers,
                    variants: variants(formats),
                },
            ))
        })
        .max_by_key(|(class, _)| *class)
}

fn text<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key)?.as_str()
}

fn number(value: &Value, key: &str) -> Option<f64> {
    value.get(key)?.as_f64()
}

/// A codec field, where yt-dlp says `none` for a missing kind of track.
fn has_codec(format: &Value, key: &str) -> bool {
    text(format, key).is_none_or(|codec| codec != "none")
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Video,
    Audio,
    Both,
}

struct Candidate<'a> {
    kind: Kind,
    url: &'a str,
    extension: &'a str,
    height: u64,
    width: Option<u64>,
    /// The height it's known by, such as 2160 for a widescreen film at 3840×2026.
    class: u64,
    fps: u64,
    hdr: bool,
    bitrate: f64,
    codec: &'a str,
    headers: &'a Value,
}

fn is_plain_file(format: &Value) -> bool {
    matches!(text(format, "protocol"), Some("https" | "http"))
}

fn candidate(format: &Value) -> Option<Candidate<'_>> {
    let url = text(format, "url").filter(|url| url.starts_with("http"))?;
    let video = has_codec(format, "vcodec");
    let audio = has_codec(format, "acodec");
    // A format that says it has neither is a storyboard or similar.
    let kind = match (video, audio) {
        (true, true) => Kind::Both,
        (true, false) => Kind::Video,
        (false, true) => Kind::Audio,
        (false, false) => return None,
    };
    if kind != Kind::Audio && number(format, "height").is_none() && text(format, "vcodec").is_some()
    {
        return None;
    }

    let height = number(format, "height").unwrap_or(0.0) as u64;
    let width = number(format, "width").map(|width| width as u64);
    Some(Candidate {
        kind,
        url,
        extension: text(format, "ext").unwrap_or("mp4"),
        height,
        width,
        class: crate::picture_class(width, Some(height)).unwrap_or(0),
        fps: number(format, "fps").unwrap_or(0.0) as u64,
        hdr: text(format, "dynamic_range").is_some_and(|range| range != "SDR"),
        bitrate: number(format, "tbr").unwrap_or(0.0),
        codec: text(format, "vcodec").unwrap_or(""),
        headers: format.get("http_headers").unwrap_or(&Value::Null),
    })
}

/// How well a picture format plays everywhere. `H.264` in `mp4` works on everything, and
/// it's what's kept when a height has several.
fn compatibility(item: &Candidate) -> u8 {
    match (item.extension, item.codec) {
        ("mp4", codec) if codec.starts_with("avc") => 0,
        ("webm", codec) if codec.starts_with("vp9") || codec.starts_with("vp09") => 1,
        ("mp4", codec) if codec.starts_with("av01") => 2,
        _ => 3,
    }
}

/// The best sound for a container: one that can go inside it without re-encoding.
fn sound_for<'a>(video: &Candidate, sounds: &[&'a Candidate<'a>]) -> Option<&'a Candidate<'a>> {
    let fits = |sound: &&&Candidate| match video.extension {
        "mp4" => sound.extension == "m4a",
        "webm" => sound.extension == "webm",
        _ => true,
    };
    let best = |left: &&&Candidate, right: &&&Candidate| left.bitrate.total_cmp(&right.bitrate);

    sounds
        .iter()
        .filter(fits)
        .max_by(best)
        .or_else(|| sounds.iter().max_by(best))
        .copied()
}

/// Only these request headers, whatever yt-dlp lists.
fn safe_headers(headers: &Value) -> Vec<(String, String)> {
    let Some(map) = headers.as_object() else {
        return Vec::new();
    };

    PASSED_HEADERS
        .iter()
        .filter_map(|wanted| {
            map.iter()
                .find(|(name, _)| name.eq_ignore_ascii_case(wanted))
                .and_then(|(name, value)| Some((name.clone(), value.as_str()?.to_owned())))
        })
        .collect()
}

fn track(item: &Candidate, seconds: f64, extension: &str) -> Track {
    Track {
        parts: vec![Part {
            url: item.url.to_owned(),
            range: None,
        }],
        extension: extension.to_owned(),
        seconds,
    }
}

fn label(item: &Candidate) -> String {
    let mut label = video_label(
        item.width,
        Some(item.height).filter(|height| *height > 0),
        None,
    );
    if item.fps > 30 {
        label.push_str(&item.fps.to_string());
    }
    if item.hdr {
        label.push_str(" HDR");
    }
    label
}

/// Reads yt-dlp's `--dump-single-json` output.
pub fn parse(json: &str) -> Result<Page, Error> {
    let page: Value = serde_json::from_str(json)
        .map_err(|_| Error::Invalid("yt-dlp's answer isn't readable".into()))?;

    // A video's own address gives one video even when it's part of a playlist, because
    // yt-dlp is run with `--no-playlist`. A list comes back only for an address that names
    // no video at all, like a playlist's own page or a site's home page.
    if text(&page, "_type") == Some("playlist") {
        return Err(Error::Playlist);
    }
    let live = matches!(text(&page, "live_status"), Some("is_live" | "is_upcoming"))
        || page.get("is_live").and_then(Value::as_bool) == Some(true);
    if live {
        return Err(Error::Live);
    }
    if text(&page, "availability").is_some_and(|state| NEEDS_ACCOUNT.contains(&state)) {
        return Err(Error::LoginRequired);
    }

    let formats = page
        .get("formats")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::Invalid("it lists nothing to save".into()))?;
    if formats.len() > MAX_FORMATS {
        return Err(Error::Invalid("it lists too many formats".into()));
    }

    // Protection is judged on everything listed. A page that offers a protected version
    // is refused whole, rather than quietly picking the unprotected leftovers.
    let drm = |format: &Value| {
        format.get("has_drm").and_then(Value::as_bool) == Some(true)
            || text(format, "format_note")
                .is_some_and(|note| note.to_ascii_lowercase().contains("drm"))
    };
    if formats.iter().any(drm) {
        return Err(Error::Protected);
    }

    let candidates: Vec<Candidate> = formats
        .iter()
        .filter(|format| is_plain_file(format))
        .filter_map(candidate)
        .collect();
    let seconds = number(&page, "duration").unwrap_or(0.0);

    let sounds: Vec<&Candidate> = candidates
        .iter()
        .filter(|item| item.kind == Kind::Audio)
        .collect();
    let mut pictures: Vec<&Candidate> = candidates
        .iter()
        .filter(|item| item.kind != Kind::Audio)
        .collect();

    // One format for each sharpness, frame rate and range, keeping the best-playing one.
    pictures.sort_by(|left, right| {
        (left.class, left.fps > 30, left.hdr)
            .cmp(&(right.class, right.fps > 30, right.hdr))
            .then(compatibility(right).cmp(&compatibility(left)))
            .then(left.bitrate.total_cmp(&right.bitrate))
    });
    pictures.reverse();
    let mut seen = Vec::new();
    pictures.retain(|item| {
        let key = (item.class, item.fps > 30, item.hdr);
        // A format with sound already in it only stands in when nothing separate exists.
        if seen.contains(&key) {
            return false;
        }
        seen.push(key);
        true
    });

    let mut qualities = Vec::new();
    for item in pictures {
        let headers = Some(safe_headers(item.headers));
        let paired = (item.kind == Kind::Video)
            .then(|| sound_for(item, &sounds))
            .flatten();

        let (video_extension, audio) = match paired {
            // Picture and sound that don't fit one container are joined into Matroska.
            Some(sound)
                if sound.extension == "m4a" && item.extension == "mp4"
                    || sound.extension == "webm" && item.extension == "webm" =>
            {
                (item.extension, Some(track(sound, seconds, sound.extension)))
            }
            Some(sound) => ("mkv", Some(track(sound, seconds, sound.extension))),
            None => (item.extension, None),
        };

        let bandwidth = (item.bitrate + paired.map_or(0.0, |sound| sound.bitrate)) * 1000.0;
        qualities.push(Quality {
            label: label(item),
            width: item.width,
            height: Some(item.height).filter(|height| *height > 0),
            bandwidth: Some(bandwidth as u64).filter(|bits| *bits > 0),
            video: track(item, seconds, video_extension),
            audio,
            headers,
            audio_only: false,
        });
    }

    // Sound alone: the only choice for a page with no picture, or the last one beside the
    // pictures, for anyone who only wants to listen. Beside pictures, M4A comes first,
    // because it plays everywhere.
    let best = |a: &&&Candidate, b: &&&Candidate| a.bitrate.total_cmp(&b.bitrate);
    let alone = if qualities.is_empty() {
        sounds.iter().max_by(best)
    } else {
        sounds
            .iter()
            .filter(|sound| sound.extension == "m4a")
            .max_by(best)
            .or_else(|| sounds.iter().max_by(best))
    };
    if let Some(sound) = alone {
        let bits = Some((sound.bitrate * 1000.0) as u64).filter(|bits| *bits > 0);
        let label = if qualities.is_empty() {
            crate::audio_label(bits)
        } else {
            audio_only_label(bits, sound.extension)
        };
        qualities.push(Quality {
            label,
            width: None,
            height: None,
            bandwidth: bits,
            video: track(sound, seconds, sound.extension),
            audio: None,
            headers: Some(safe_headers(sound.headers)),
            audio_only: true,
        });
    }

    // Some sites offer their best, or everything, only as HLS, which Tributary reads itself.
    let best_plain = qualities
        .iter()
        .filter_map(|quality| crate::picture_class(quality.width, quality.height))
        .max()
        .unwrap_or(0);
    let stream = best_hls(formats)
        .filter(|(class, _)| *class > best_plain)
        .map(|(_, stream)| stream);

    if qualities.is_empty() && stream.is_none() {
        return Err(Error::Invalid(
            "it has no video in a form Tributary can fetch".into(),
        ));
    }

    Ok(Page {
        title: text(&page, "title")
            .map(str::to_owned)
            .filter(|title| !title.trim().is_empty()),
        qualities,
        stream,
        subtitles: subtitles(&page),
        thumbnail: text(&page, "thumbnail")
            .filter(|url| url.starts_with("http"))
            .map(str::to_owned),
    })
}

/// What to say when YouTube wants to check that this computer isn't a bot.
pub const BOT_CHECK: &str = "YouTube wants to check this computer isn't a bot, which it does for a while after a lot of videos in a short time. Try again in a few minutes";

/// What to say when a site won't hand a page to anything but its own player.
pub const REFUSED: &str = "the site wouldn't show this page outside its own player. It may only work inside the page it's embedded in, or its link may have expired, so use the button beside the player on that page";

/// Turns yt-dlp's complaint into something to show.
pub fn explain_failure(stderr: &str) -> Error {
    let lower = stderr.to_ascii_lowercase();
    let says = |words: &[&str]| words.iter().any(|word| lower.contains(word));

    if says(&["not a bot", "not a robot"]) {
        // YouTube asks this of a computer that has fetched a lot in a short time. It says
        // "sign in", but the video itself needs no account, so it mustn't read as one that does.
        Error::Fetch(BOT_CHECK.into())
    } else if says(&["drm"]) {
        Error::Protected
    } else if says(&["live event", "is live", "premieres in", "will begin in"]) {
        Error::Live
    } else if says(&[
        "sign in",
        "log in",
        "login",
        "logged in",
        "logged-in",
        "private video",
        "members-only",
        "members only",
        "join this channel",
        "requires payment",
        "premium",
        "age-restricted",
        "confirm your age",
    ]) {
        Error::LoginRequired
    } else if says(&[
        "unsupported url",
        "no video formats",
        "no video could be found",
    ]) {
        Error::NoVideo
    } else if says(&["http error 403", "http error 404", "http error 410"]) {
        // Players embedded in another site often answer only when asked from that site.
        Error::Fetch(REFUSED.into())
    } else {
        let last = stderr
            .lines()
            .rev()
            .find(|line| line.trim_start().starts_with("ERROR"))
            .unwrap_or("it couldn't read this page")
            .trim_start_matches("ERROR:")
            .trim();
        let short: String = last.chars().take(200).collect();
        Error::Fetch(format!("yt-dlp couldn't read this page: {short}"))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    fn picture(id: &str, height: u64, extension: &str, codec: &str, bitrate: f64) -> Value {
        json!({
            "format_id": id, "url": format!("https://v.test/{id}"), "protocol": "https",
            "ext": extension, "vcodec": codec, "acodec": "none", "height": height,
            "width": height * 16 / 9, "tbr": bitrate, "fps": 30,
            "http_headers": {
                "User-Agent": "Agent", "Referer": "https://v.test/", "Cookie": "secret=1",
                "Authorization": "Bearer x", "Accept": "*/*"
            }
        })
    }

    fn sound(id: &str, extension: &str, bitrate: f64) -> Value {
        json!({
            "format_id": id, "url": format!("https://v.test/{id}"), "protocol": "https",
            "ext": extension, "vcodec": "none", "acodec": "x", "tbr": bitrate
        })
    }

    fn page(formats: Vec<Value>) -> String {
        json!({ "title": "A film", "duration": 100.0, "formats": formats }).to_string()
    }

    fn standard() -> Vec<Value> {
        vec![
            picture("p1080a", 1080, "mp4", "avc1.64", 4000.0),
            picture("p1080v", 1080, "webm", "vp9", 3000.0),
            picture("p2160", 2160, "webm", "vp9", 16000.0),
            picture("p720", 720, "mp4", "avc1.4d", 2000.0),
            sound("a-m4a", "m4a", 128.0),
            sound("a-webm", "webm", 160.0),
        ]
    }

    #[test]
    fn a_widescreen_video_is_named_as_the_site_names_it() {
        // Formats as yt-dlp reported them for a real widescreen YouTube video.
        let sized = |id: &str, width: u64, height: u64, codec: &str, extension: &str| {
            json!({
                "format_id": id, "url": format!("https://v.test/{id}"), "protocol": "https",
                "ext": extension, "vcodec": codec, "acodec": "none", "width": width,
                "height": height, "tbr": width as f64, "fps": 25,
            })
        };
        let formats = vec![
            sized("313", 3840, 2026, "vp9", "webm"),
            sized("271", 2560, 1350, "vp9", "webm"),
            sized("137", 1920, 1012, "avc1.640028", "mp4"),
            sized("248", 1920, 1012, "vp9", "webm"),
            sized("136", 1280, 676, "avc1.4d401f", "mp4"),
            sized("135", 854, 450, "avc1.4d401e", "mp4"),
            sound("140", "m4a", 128.0),
        ];

        let found = parse(&page(formats)).unwrap();
        let labels: Vec<&str> = found
            .qualities
            .iter()
            .map(|item| item.label.as_str())
            .collect();
        assert_eq!(
            labels,
            [
                "2160p",
                "1440p",
                "1080p",
                "720p",
                "480p",
                "Audio only, 128 kbps (M4A)"
            ]
        );
        // Two formats of the same sharpness are one choice, the one that plays everywhere.
        assert_eq!(found.qualities[2].video.parts[0].url, "https://v.test/137");
        assert_eq!(
            found.qualities[2].height,
            Some(1012),
            "the real size is kept as it is"
        );
    }

    #[test]
    fn heights_come_best_first_with_the_title() {
        let found = parse(&page(standard())).unwrap();

        let labels: Vec<&str> = found
            .qualities
            .iter()
            .map(|item| item.label.as_str())
            .collect();
        assert_eq!(
            labels,
            ["2160p", "1080p", "720p", "Audio only, 128 kbps (M4A)"]
        );
        assert_eq!(found.title.as_deref(), Some("A film"));
    }

    #[test]
    fn the_format_that_plays_everywhere_is_kept_for_a_height() {
        let found = parse(&page(standard())).unwrap();

        let p1080 = &found.qualities[1];
        assert!(
            p1080.video.parts[0].url.ends_with("p1080a"),
            "mp4 with H.264 wins"
        );
        assert_eq!(p1080.video.extension, "mp4");
    }

    #[test]
    fn sound_that_fits_the_container_is_chosen() {
        let found = parse(&page(standard())).unwrap();

        let mp4 = found.qualities[1].audio.as_ref().unwrap();
        assert_eq!(
            (mp4.extension.as_str(), mp4.parts[0].url.as_str()),
            ("m4a", "https://v.test/a-m4a")
        );

        let webm = found.qualities[0].audio.as_ref().unwrap();
        assert_eq!(
            webm.extension, "webm",
            "a 2160p webm picture takes the webm sound"
        );
    }

    #[test]
    fn a_picture_and_sound_that_cannot_share_a_container_go_into_matroska() {
        let formats = vec![
            picture("p", 1080, "webm", "vp9", 3000.0),
            sound("a", "m4a", 128.0),
        ];
        let found = parse(&page(formats)).unwrap();

        assert_eq!(found.qualities[0].video.extension, "mkv");
        assert_eq!(found.qualities[0].audio.as_ref().unwrap().extension, "m4a");
    }

    #[test]
    fn a_format_with_sound_inside_is_a_quality_of_its_own() {
        let muxed = json!({
            "format_id": "18", "url": "https://v.test/18", "protocol": "https", "ext": "mp4",
            "vcodec": "avc1", "acodec": "mp4a", "height": 360, "width": 640, "tbr": 500.0
        });
        let found = parse(&page(vec![muxed])).unwrap();

        assert_eq!(found.qualities.len(), 1);
        assert!(found.qualities[0].audio.is_none());
    }

    #[test]
    fn only_a_short_fixed_list_of_headers_is_passed_on() {
        let found = parse(&page(standard())).unwrap();
        let headers = found.qualities[0].headers.as_ref().unwrap();

        let names: Vec<String> = headers
            .iter()
            .map(|(name, _)| name.to_ascii_lowercase())
            .collect();
        assert_eq!(names, ["user-agent", "referer"]);
        assert!(!format!("{headers:?}").contains("secret"));
    }

    #[test]
    fn the_estimate_adds_picture_and_sound() {
        let found = parse(&page(standard())).unwrap();
        assert_eq!(found.qualities[1].bandwidth, Some(4_128_000));
    }

    #[test]
    fn frame_rate_and_range_make_separate_choices() {
        let mut fast = picture("fast", 1080, "mp4", "avc1", 6000.0);
        fast["fps"] = json!(60);
        let mut hdr = picture("hdr", 1080, "webm", "vp9.2", 5000.0);
        hdr["dynamic_range"] = json!("HDR10");
        let found = parse(&page(vec![
            picture("a", 1080, "mp4", "avc1", 4000.0),
            fast,
            hdr,
            sound("s", "m4a", 128.0),
        ]))
        .unwrap();

        let labels: Vec<&str> = found
            .qualities
            .iter()
            .map(|item| item.label.as_str())
            .collect();
        assert_eq!(labels.len(), 4);
        assert!(labels.contains(&"1080p60"));
        assert!(labels.contains(&"1080p HDR"));
        assert!(labels.contains(&"1080p"));
    }

    #[test]
    fn formats_that_are_not_plain_files_are_left_out() {
        let mut stream = picture("hls", 1080, "mp4", "avc1", 4000.0);
        stream["protocol"] = json!("m3u8_native");
        let mut dash = picture("dash", 2160, "mp4", "avc1", 9000.0);
        dash["protocol"] = json!("http_dash_segments");

        let found = parse(&page(vec![
            stream,
            dash,
            picture("ok", 720, "mp4", "avc1", 2000.0),
            sound("s", "m4a", 128.0),
        ]))
        .unwrap();
        assert_eq!(found.qualities.len(), 2);
        assert_eq!(found.qualities[0].label, "720p");
        assert!(found.qualities[1].audio_only);
    }

    #[test]
    fn sound_alone_is_offered_last_as_m4a_where_it_can_be() {
        let found = parse(&page(standard())).unwrap();

        let alone = found.qualities.last().unwrap();
        assert!(alone.audio_only);
        // The WebM sound has the higher rate, but M4A plays everywhere.
        assert_eq!(alone.video.parts[0].url, "https://v.test/a-m4a");
        assert_eq!(alone.video.extension, "m4a");
        assert_eq!(alone.audio, None);
        assert_eq!(alone.bandwidth, Some(128_000));
        assert!(
            found
                .qualities
                .iter()
                .rev()
                .skip(1)
                .all(|quality| !quality.audio_only)
        );

        // Without M4A, the best of what there is.
        let found = parse(&page(vec![
            picture("p720", 720, "webm", "vp9", 2000.0),
            sound("opus", "webm", 160.0),
            sound("low", "webm", 64.0),
        ]))
        .unwrap();
        assert_eq!(
            found.qualities.last().unwrap().label,
            "Audio only, 160 kbps (WEBM)"
        );
    }

    #[test]
    fn a_page_with_no_sound_of_its_own_offers_no_sound_alone() {
        let mut whole = picture("whole", 720, "mp4", "avc1", 2000.0);
        whole["acodec"] = json!("mp4a.40.2");
        let found = parse(&page(vec![whole])).unwrap();

        assert_eq!(found.qualities.len(), 1);
        assert!(!found.qualities[0].audio_only);
    }

    #[test]
    fn subtitles_and_the_cover_are_read() {
        let mut answer: Value = serde_json::from_str(&page(standard())).unwrap();
        answer["thumbnail"] = json!("https://i.test/cover.webp");
        answer["subtitles"] = json!({
            "fr": [
                { "ext": "json3", "url": "https://s.test/fr.json3", "name": "French" },
                { "ext": "srt", "url": "https://s.test/fr.srt", "name": "French" },
                { "ext": "vtt", "url": "https://s.test/fr.vtt", "name": "French" }
            ],
            "en": [{ "ext": "vtt", "url": "https://s.test/en.vtt", "name": "English" }],
            "live_chat": [{ "ext": "json", "url": "https://s.test/chat.json" }],
            "de": [{ "ext": "ttml", "url": "https://s.test/de.ttml", "name": "German" }],
            "es": [{ "ext": "srt", "url": "file:///C:/es.srt", "name": "Spanish" }]
        });
        answer["automatic_captions"] = json!({
            "en": [{ "ext": "vtt", "url": "https://s.test/auto?lang=en", "name": "English" }],
            "af": [{ "ext": "vtt", "url": "https://s.test/auto?lang=en&tlang=af", "name": "Afrikaans" }],
            "pt-BR": [{ "ext": "srt", "url": "https://s.test/auto?lang=pt", "name": "" }]
        });

        let found = parse(&answer.to_string()).unwrap();

        assert_eq!(
            found.thumbnail.as_deref(),
            Some("https://i.test/cover.webp")
        );
        let listed: Vec<(&str, &str, &str, bool)> = found
            .subtitles
            .iter()
            .map(|item| {
                (
                    item.language.as_str(),
                    item.name.as_str(),
                    item.url.as_str(),
                    item.automatic,
                )
            })
            .collect();
        assert_eq!(
            listed,
            [
                // Written ones first, by name, each in the best format ffmpeg reads.
                ("en", "English", "https://s.test/en.vtt", false),
                ("fr", "French", "https://s.test/fr.vtt", false),
                // Then automatic ones, without the machine translations.
                ("en", "English", "https://s.test/auto?lang=en", true),
                ("pt-BR", "pt-BR", "https://s.test/auto?lang=pt", true),
            ]
        );
    }

    #[test]
    fn a_page_without_subtitles_or_a_cover_has_none() {
        let found = parse(&page(standard())).unwrap();
        assert!(found.subtitles.is_empty());
        assert_eq!(found.thumbnail, None);
    }

    #[test]
    fn each_quality_with_its_sound_is_kept_by_its_own_list() {
        // As Dailymotion lists them: upright pictures, with the sound in each.
        let master = "https://director.test/master.m3u8";
        let with_sound = |id: &str, width: u64, height: u64, fps: u64, bitrate: f64| {
            let mut format = hls(id, height, Some(master));
            format["width"] = json!(width);
            format["fps"] = json!(fps);
            format["tbr"] = json!(bitrate);
            format["acodec"] = json!("mp4a.40.2");
            format
        };
        let found = parse(&page(vec![
            with_sound("hls-380", 360, 640, 25, 461.0),
            with_sound("hls-1080@60", 1080, 1920, 60, 5717.0),
            with_sound("hls-2160@60", 2160, 3840, 60, 20394.0),
            with_sound("hls-1080-low", 1080, 1920, 60, 3000.0),
        ]))
        .unwrap();

        let stream = found.stream.unwrap();
        let labels: Vec<&str> = stream
            .variants
            .iter()
            .map(|variant| variant.label.as_str())
            .collect();
        assert_eq!(labels, ["2160p60", "1080p60", "360p"]);
        assert_eq!(
            stream.variants[1].url, "https://v.test/hls-1080@60.m3u8",
            "the better of the two 1080p"
        );
        assert_eq!(stream.variants[0].bandwidth, Some(20_394_000));
    }

    #[test]
    fn pictures_without_their_sound_are_not_offered_one_by_one() {
        // As TED lists them: the sound is a list of its own, so only the master will do.
        let found = parse(&page(vec![hls(
            "hls-851",
            360,
            Some("https://v.test/m.m3u8"),
        )]))
        .unwrap();
        let stream = found.stream.unwrap();
        assert_eq!(stream.url, "https://v.test/m.m3u8");
        assert!(stream.variants.is_empty());
    }

    fn hls(id: &str, height: u64, master: Option<&str>) -> Value {
        let mut format = picture(id, height, "mp4", "avc1", 1000.0);
        format["protocol"] = json!("m3u8_native");
        format["url"] = json!(format!("https://v.test/{id}.m3u8"));
        if let Some(master) = master {
            format["manifest_url"] = json!(master);
        }
        format["http_headers"] =
            json!({ "User-Agent": "Agent", "Referer": "https://site.test/", "Cookie": "secret=1" });
        format
    }

    #[test]
    fn a_page_offered_only_as_hls_is_handed_to_tributarys_own_reader() {
        let master = "https://v.test/master.m3u8";
        let found = parse(&page(vec![
            hls("h1", 1080, Some(master)),
            hls("h2", 480, Some(master)),
        ]))
        .unwrap();

        assert!(found.qualities.is_empty());
        let stream = found.stream.unwrap();
        assert_eq!(stream.url, master, "the list that holds every quality");
        assert_eq!(
            stream.headers,
            [
                ("User-Agent".to_owned(), "Agent".to_owned()),
                ("Referer".to_owned(), "https://site.test/".to_owned()),
            ]
        );
    }

    #[test]
    fn hls_is_used_only_when_it_is_better_than_every_plain_file() {
        let better = parse(&page(vec![
            hls("h", 1080, None),
            picture("p", 360, "mp4", "avc1", 500.0),
        ]))
        .unwrap();
        assert_eq!(
            better.stream.map(|stream| stream.url),
            Some("https://v.test/h.m3u8".to_owned())
        );

        let worse = parse(&page(vec![
            hls("h", 720, None),
            picture("p", 1080, "mp4", "avc1", 4000.0),
            sound("s", "m4a", 128.0),
        ]))
        .unwrap();
        assert_eq!(worse.stream, None);
        assert_eq!(worse.qualities[0].label, "1080p");
    }

    #[test]
    fn protected_hls_still_refuses_the_whole_page() {
        let mut locked = hls("h", 1080, None);
        locked["has_drm"] = json!(true);
        assert_eq!(parse(&page(vec![locked])).unwrap_err(), Error::Protected);
    }

    #[test]
    fn a_page_with_nothing_tributary_can_read_says_so() {
        let mut dash = picture("dash", 1080, "mp4", "avc1", 9000.0);
        dash["protocol"] = json!("http_dash_segments");
        assert!(matches!(parse(&page(vec![dash])), Err(Error::Invalid(_))));
    }

    #[test]
    fn a_page_with_nothing_fetchable_says_so() {
        assert!(matches!(parse(&page(Vec::new())), Err(Error::Invalid(_))));
        // HLS alone is no longer nothing: it's handed to Tributary's own reader.
        let mut stream = picture("hls", 1080, "mp4", "avc1", 4000.0);
        stream["protocol"] = json!("m3u8_native");
        assert!(parse(&page(vec![stream])).unwrap().stream.is_some());
    }

    #[test]
    fn sound_alone_is_offered_when_there_is_no_picture() {
        let found = parse(&page(vec![sound("a", "m4a", 128.0)])).unwrap();
        assert_eq!(found.qualities[0].label, "Audio 128 kbps");
    }

    #[test]
    fn any_protected_format_refuses_the_whole_page() {
        let mut protected = picture("drm", 1080, "mp4", "avc1", 4000.0);
        protected["has_drm"] = json!(true);

        let mut formats = standard();
        formats.push(protected);
        assert_eq!(parse(&page(formats)), Err(Error::Protected));
    }

    #[test]
    fn a_format_noted_as_drm_refuses_the_page() {
        let mut noted = picture("drm", 1080, "mp4", "avc1", 4000.0);
        noted["format_note"] = json!("1080p (DRM)");
        assert_eq!(parse(&page(vec![noted])), Err(Error::Protected));
    }

    #[test]
    fn live_and_upcoming_videos_are_refused() {
        for change in [
            ("is_live", json!(true)),
            ("live_status", json!("is_live")),
            ("live_status", json!("is_upcoming")),
        ] {
            let mut value: Value = serde_json::from_str(&page(standard())).unwrap();
            value[change.0] = change.1;
            assert_eq!(parse(&value.to_string()), Err(Error::Live), "{value}");
        }
    }

    #[test]
    fn videos_needing_an_account_are_refused() {
        for state in ["needs_auth", "premium_only", "subscriber_only", "private"] {
            let mut value: Value = serde_json::from_str(&page(standard())).unwrap();
            value["availability"] = json!(state);
            assert_eq!(
                parse(&value.to_string()),
                Err(Error::LoginRequired),
                "{state}"
            );
        }

        let mut open: Value = serde_json::from_str(&page(standard())).unwrap();
        open["availability"] = json!("public");
        assert!(parse(&open.to_string()).is_ok());
    }

    #[test]
    fn an_address_that_names_only_a_list_is_explained() {
        // What yt-dlp gives for a playlist's own page, or YouTube's home page.
        let text = json!({ "_type": "playlist", "id": "recommended", "entries": [] }).to_string();

        let problem = parse(&text).unwrap_err();
        assert_eq!(problem, Error::Playlist);
        assert!(problem.to_string().contains("button beside its player"));
    }

    #[test]
    fn a_video_that_belongs_to_a_playlist_is_one_video() {
        // A watch address with `list=` gives the one video, saying which list it's from.
        let mut video: Value = serde_json::from_str(&page(standard())).unwrap();
        video["_type"] = json!("video");
        video["playlist_id"] = json!("PLD8ifvNZtu-c");
        video["playlist_index"] = json!(2);
        video["webpage_url"] = json!("https://www.youtube.com/watch?v=2Kqrs38vHfQ");

        let found = parse(&video.to_string()).unwrap();
        assert_eq!(found.qualities[0].label, "2160p");
        assert_eq!(found.title.as_deref(), Some("A film"));
    }

    #[test]
    fn unreadable_answers_are_refused() {
        assert!(matches!(parse("not json"), Err(Error::Invalid(_))));
        assert!(matches!(parse("{}"), Err(Error::Invalid(_))));
    }

    #[test]
    fn an_absurd_number_of_formats_is_refused() {
        let formats = (0..500)
            .map(|n| picture(&format!("f{n}"), 360, "mp4", "avc1", 1.0))
            .collect();
        assert!(matches!(parse(&page(formats)), Err(Error::Invalid(_))));
    }

    #[test]
    fn addresses_that_are_not_web_addresses_are_ignored() {
        let mut local = picture("local", 1080, "mp4", "avc1", 4000.0);
        local["url"] = json!("file:///C:/Windows/win.ini");
        assert!(matches!(parse(&page(vec![local])), Err(Error::Invalid(_))));
    }

    #[test]
    fn complaints_are_explained() {
        let error = |text: &str| explain_failure(text);

        // A check on the computer, not a video that needs an account. YouTube writes it with
        // a curly apostrophe.
        for check in [
            "ERROR: [youtube] x: Sign in to confirm you're not a bot",
            "ERROR: [youtube] pBzadnf1Z4M: Sign in to confirm you’re not a bot. Use --cookies-from-browser or --cookies for the authentication.",
        ] {
            assert_eq!(error(check), Error::Fetch(BOT_CHECK.into()), "{check}");
        }
        assert_eq!(
            error("ERROR: Sign in to confirm your age"),
            Error::LoginRequired
        );
        // What Vimeo answers now.
        assert_eq!(
            error(
                "ERROR: [vimeo] 76979871: The web client only works when logged-in. Use --cookies, --username and --password"
            ),
            Error::LoginRequired
        );
        assert_eq!(
            error("ERROR: This video is available to this channel's members only"),
            Error::LoginRequired
        );
        assert_eq!(
            error("ERROR: Private video. Sign in if you've been granted access"),
            Error::LoginRequired
        );
        assert_eq!(
            error("ERROR: This video is DRM protected"),
            Error::Protected
        );
        assert_eq!(
            error("ERROR: This live event will begin in 2 hours"),
            Error::Live
        );
        assert_eq!(
            error("ERROR: Unsupported URL: https://x.test/"),
            Error::NoVideo
        );
        assert!(Error::NoVideo.to_string().contains("press play"));

        // What a player embedded in another site answers when asked on its own.
        for refused in [
            "ERROR: [generic] 1339713?vs=x: Unable to download webpage: HTTP Error 404: Not Found",
            "ERROR: [generic] player: Unable to download webpage: HTTP Error 403: Forbidden",
            "ERROR: Unable to download webpage: HTTP Error 410: Gone",
        ] {
            assert_eq!(error(refused), Error::Fetch(REFUSED.into()), "{refused}");
        }

        let other = error("WARNING: slow\nERROR: [x] something odd happened");
        assert_eq!(
            other,
            Error::Fetch("yt-dlp couldn't read this page: [x] something odd happened".into())
        );
    }
}
