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

    Some(Candidate {
        kind,
        url,
        extension: text(format, "ext").unwrap_or("mp4"),
        height: number(format, "height").unwrap_or(0.0) as u64,
        width: number(format, "width").map(|width| width as u64),
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
    let mut label = video_label(Some(item.height).filter(|height| *height > 0), None);
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

    // One format for each height, frame rate and range, keeping the best-playing one.
    pictures.sort_by(|left, right| {
        (left.height, left.fps > 30, left.hdr)
            .cmp(&(right.height, right.fps > 30, right.hdr))
            .then(compatibility(right).cmp(&compatibility(left)))
            .then(left.bitrate.total_cmp(&right.bitrate))
    });
    pictures.reverse();
    let mut seen = Vec::new();
    pictures.retain(|item| {
        let key = (item.height, item.fps > 30, item.hdr);
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
        });
    }

    // Sound alone, for a page that has no picture.
    if qualities.is_empty()
        && let Some(sound) = sounds.iter().max_by(|a, b| a.bitrate.total_cmp(&b.bitrate))
    {
        let bits = Some((sound.bitrate * 1000.0) as u64).filter(|bits| *bits > 0);
        qualities.push(Quality {
            label: crate::audio_label(bits),
            width: None,
            height: None,
            bandwidth: bits,
            video: track(sound, seconds, sound.extension),
            audio: None,
            headers: Some(safe_headers(sound.headers)),
        });
    }

    if qualities.is_empty() {
        return Err(Error::Invalid(
            "it has no video that can be fetched as a plain file".into(),
        ));
    }

    Ok(Page {
        title: text(&page, "title")
            .map(str::to_owned)
            .filter(|title| !title.trim().is_empty()),
        qualities,
    })
}

/// Turns yt-dlp's complaint into something to show.
pub fn explain_failure(stderr: &str) -> Error {
    let lower = stderr.to_ascii_lowercase();
    let says = |words: &[&str]| words.iter().any(|word| lower.contains(word));

    if says(&["drm"]) {
        Error::Protected
    } else if says(&["live event", "is live", "premieres in", "will begin in"]) {
        Error::Live
    } else if says(&[
        "sign in",
        "log in",
        "login",
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
        Error::Invalid("there's no video here that can be saved".into())
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
    fn heights_come_best_first_with_the_title() {
        let found = parse(&page(standard())).unwrap();

        let labels: Vec<&str> = found
            .qualities
            .iter()
            .map(|item| item.label.as_str())
            .collect();
        assert_eq!(labels, ["2160p", "1080p", "720p"]);
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
        assert_eq!(labels.len(), 3);
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
        assert_eq!(found.qualities.len(), 1);
        assert_eq!(found.qualities[0].label, "720p");
    }

    #[test]
    fn a_page_with_nothing_fetchable_says_so() {
        let mut stream = picture("hls", 1080, "mp4", "avc1", 4000.0);
        stream["protocol"] = json!("m3u8_native");
        assert!(matches!(parse(&page(vec![stream])), Err(Error::Invalid(_))));
        assert!(matches!(parse(&page(Vec::new())), Err(Error::Invalid(_))));
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

        assert_eq!(
            error("ERROR: [youtube] x: Sign in to confirm you're not a bot"),
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
        assert!(matches!(
            error("ERROR: Unsupported URL: https://x.test/"),
            Error::Invalid(_)
        ));

        let other = error("WARNING: slow\nERROR: [x] something odd happened");
        assert_eq!(
            other,
            Error::Fetch("yt-dlp couldn't read this page: [x] something odd happened".into())
        );
    }
}
