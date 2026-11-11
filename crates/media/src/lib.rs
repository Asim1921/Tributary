//! Reads HLS playlists and DASH manifests, and works out the parts of each quality.
//!
//! Nothing here touches the network. [`discover`] is given a function to fetch text with,
//! so the caller decides how requests are made.
//!
//! Protected streams are refused, and that check reads the raw text rather than the
//! parsed result, so a declaration the parsers don't model is still caught. A stream is
//! refused whole if any playlist it uses declares protection, even if others don't.

mod dash;
mod hls;
pub mod ytdlp;

use std::future::Future;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

/// More than any real video has. A manifest comes from a web page, so this stops one
/// from describing an endless list.
const MAX_PARTS: usize = 200_000;

/// How many qualities are read from one stream.
const MAX_QUALITIES: usize = 32;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum Error {
    #[error("this video is protected, so it can't be saved")]
    Protected,
    #[error("this is a live stream, so it can't be saved as a file")]
    Live,
    #[error(
        "this video needs a login or a paid account, which Tributary doesn't use for video sites"
    )]
    LoginRequired,
    #[error("this address isn't a video stream")]
    NotAStream,
    #[error(
        "this address is a list of videos, not one video. Open the video you want and use the button beside its player, or paste that video's own address"
    )]
    Playlist,
    #[error("the stream couldn't be read: {0}")]
    Invalid(String),
    #[error("{0}")]
    Fetch(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ByteRange {
    pub start: u64,
    pub length: u64,
}

/// One request's worth of a stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Part {
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<ByteRange>,
}

/// Parts that join up into one file.
#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    pub parts: Vec<Part>,
    /// The file extension that suits the joined parts, without a dot.
    pub extension: String,
    pub seconds: f64,
}

impl Track {
    /// True if this is simply a file, which an ordinary download handles better.
    pub fn is_single_file(&self) -> bool {
        matches!(self.parts.as_slice(), [Part { range: None, .. }])
    }
}

/// One choice of what to save.
#[derive(Debug, Clone, PartialEq)]
pub struct Quality {
    pub label: String,
    pub width: Option<u64>,
    pub height: Option<u64>,
    /// Bits per second.
    pub bandwidth: Option<u64>,
    pub video: Track,
    /// Set when the sound is separate from the picture.
    pub audio: Option<Track>,
    /// The request headers to send, when the source says exactly which. `None` leaves it
    /// to whatever the browser sent.
    pub headers: Option<Vec<(String, String)>>,
}

impl Quality {
    /// Roughly how many bytes this will take, from its declared bit rate.
    pub fn estimated_bytes(&self) -> Option<u64> {
        let bandwidth = self.bandwidth? as f64;
        Some((bandwidth * self.video.seconds / 8.0) as u64)
    }
}

/// Reads a stream, using `fetch` for its playlists. The best quality comes first.
pub async fn discover<F, Fut>(url: &str, fetch: F) -> Result<Vec<Quality>, Error>
where
    F: Fn(String) -> Fut,
    Fut: Future<Output = Result<String, String>>,
{
    let text = fetch(url.to_owned()).await.map_err(Error::Fetch)?;
    let start = text.trim_start_matches('\u{feff}').trim_start();

    let mut qualities = if start.starts_with("#EXTM3U") {
        hls::discover(url, start, &fetch).await?
    } else if start.contains("<MPD") {
        dash::read(url, start)?
    } else {
        return Err(Error::NotAStream);
    };

    if qualities.is_empty() {
        return Err(Error::Invalid("it lists nothing to save".to_owned()));
    }
    qualities.sort_by_key(|quality| {
        std::cmp::Reverse((quality.height.unwrap_or(0), quality.bandwidth.unwrap_or(0)))
    });
    Ok(qualities)
}

/// Whether an address or content type looks like a stream rather than a plain file.
pub fn is_stream(url: &str, content_type: Option<&str>) -> bool {
    let kind = content_type.unwrap_or_default().to_ascii_lowercase();
    if kind.contains("mpegurl") || kind.contains("dash+xml") {
        return true;
    }
    let path = url
        .split(['?', '#'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    path.ends_with(".m3u8") || path.ends_with(".mpd")
}

/// Makes a link in a playlist absolute. Only web addresses are accepted, because a
/// playlist must not be able to point the app at local files.
fn resolve(base: &Url, link: &str) -> Result<Url, Error> {
    let url = base
        .join(link.trim())
        .map_err(|_| Error::Invalid(format!("it has a bad address: {link}")))?;
    if matches!(url.scheme(), "http" | "https") {
        Ok(url)
    } else {
        Err(Error::Invalid(format!("it has a bad address: {link}")))
    }
}

fn parse_base(url: &str) -> Result<Url, Error> {
    Url::parse(url).map_err(|_| Error::Invalid(format!("it has a bad address: {url}")))
}

fn too_many_parts() -> Error {
    Error::Invalid("it lists too many parts".to_owned())
}

fn extension_of(url: &str) -> Option<String> {
    let path = Url::parse(url).ok()?.path().to_ascii_lowercase();
    let (_, extension) = path.rsplit('/').next()?.rsplit_once('.')?;
    Some(extension.to_owned())
}

/// The extension for parts like this one once joined. A segment's own extension is often
/// one no player opens, such as `.m4s`.
fn joined_extension(url: &str, fragmented: bool, audio: bool) -> String {
    let found = extension_of(url).unwrap_or_default();
    let mp4 = if audio { "m4a" } else { "mp4" };
    match found.as_str() {
        "m4s" | "mp4" | "m4v" | "m4a" | "cmfv" | "cmfa" | "fmp4" => mp4.to_owned(),
        "ts" | "aac" | "mp3" | "webm" | "mkv" | "ogg" | "opus" => found,
        _ if fragmented => mp4.to_owned(),
        _ => "ts".to_owned(),
    }
}

/// The name a picture's sharpness goes by, as video sites give it: the height it would have
/// at 16:9. A film cropped to 3840×2026 is "2160p" and an upright 1080×1920 one is "1080p",
/// because that's how sharp they are, though neither is that many pixels tall.
fn picture_class(width: Option<u64>, height: Option<u64>) -> Option<u64> {
    let height = height.filter(|height| *height > 0)?;
    let Some(width) = width.filter(|width| *width > 0) else {
        return Some(height);
    };
    let (short, long) = (width.min(height), width.max(height));
    // The long side's 16:9 height, rounded to the nearest pixel.
    let sixteen_nine = long.saturating_mul(9).saturating_add(8) / 16;
    Some(short.max(sixteen_nine))
}

fn video_label(width: Option<u64>, height: Option<u64>, bandwidth: Option<u64>) -> String {
    match (picture_class(width, height), bandwidth) {
        (Some(class), _) => format!("{class}p"),
        (None, Some(bandwidth)) => format!("{:.1} Mbps", bandwidth as f64 / 1_000_000.0),
        (None, None) => "Video".to_owned(),
    }
}

fn audio_label(bandwidth: Option<u64>) -> String {
    match bandwidth {
        Some(bandwidth) => format!("Audio {} kbps", bandwidth / 1000),
        None => "Audio".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pictures_are_named_by_how_sharp_they_are_not_their_height() {
        // Sizes two real YouTube videos came in, and the names YouTube gives them.
        for (width, height, name) in [
            (256, 138, "144p"),
            (426, 230, "240p"),
            (640, 346, "360p"),
            (854, 462, "480p"),
            (1280, 692, "720p"),
            (256, 136, "144p"),
            (426, 224, "240p"),
            (640, 338, "360p"),
            (854, 450, "480p"),
            (1280, 676, "720p"),
            (1920, 1012, "1080p"),
            (2560, 1350, "1440p"),
            (3840, 2026, "2160p"),
        ] {
            assert_eq!(
                video_label(Some(width), Some(height), None),
                name,
                "{width}x{height}"
            );
        }
    }

    #[test]
    fn ordinary_upright_square_and_older_shapes_keep_their_usual_names() {
        for (width, height, name) in [
            (1920, 1080, "1080p"),
            (1280, 720, "720p"),
            (1080, 1920, "1080p"),
            (720, 1280, "720p"),
            (1080, 1080, "1080p"),
            (640, 480, "480p"),
            (1440, 1080, "1080p"),
            (1920, 800, "1080p"),
        ] {
            assert_eq!(
                video_label(Some(width), Some(height), None),
                name,
                "{width}x{height}"
            );
        }
    }

    #[test]
    fn a_picture_with_a_missing_or_odd_size_still_gets_a_name() {
        assert_eq!(video_label(None, Some(720), None), "720p");
        assert_eq!(video_label(Some(0), Some(720), None), "720p");
        assert_eq!(video_label(Some(1280), None, Some(2_500_000)), "2.5 Mbps");
        assert_eq!(video_label(Some(1280), Some(0), None), "Video");
        // Nothing a source says can make this overflow.
        assert!(video_label(Some(u64::MAX), Some(u64::MAX), None).ends_with('p'));
    }

    #[tokio::test]
    async fn a_widescreen_stream_is_named_by_how_sharp_it_is() {
        let master =
            "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=5000000,RESOLUTION=1920x800\nwide.m3u8\n";
        let media = "#EXTM3U\n#EXT-X-TARGETDURATION:4\n#EXTINF:4.0,\na.ts\n#EXT-X-ENDLIST\n";
        let fetch = |url: String| async move {
            Ok::<_, String>(
                if url.ends_with("master.m3u8") {
                    master
                } else {
                    media
                }
                .to_owned(),
            )
        };

        let qualities = discover("https://x.test/master.m3u8", fetch).await.unwrap();
        assert_eq!(qualities[0].label, "1080p");
        assert_eq!(
            qualities[0].height,
            Some(800),
            "the real size is kept as it is"
        );
    }

    #[test]
    fn recognises_streams_by_address_or_type() {
        assert!(is_stream("https://x.test/a.m3u8?token=1", None));
        assert!(is_stream("https://x.test/A.MPD#t=1", None));
        assert!(is_stream(
            "https://x.test/play",
            Some("application/vnd.apple.mpegurl")
        ));
        assert!(is_stream(
            "https://x.test/play",
            Some("application/x-mpegURL")
        ));
        assert!(is_stream(
            "https://x.test/play",
            Some("application/dash+xml")
        ));
        assert!(!is_stream("https://x.test/a.mp4", Some("video/mp4")));
        assert!(!is_stream("https://x.test/m3u8/a.mp4", None));
    }

    #[test]
    fn a_playlist_cannot_point_at_local_files() {
        let base = Url::parse("https://x.test/a/b.m3u8").unwrap();

        assert!(resolve(&base, "file:///C:/Windows/win.ini").is_err());
        assert!(resolve(&base, "ftp://x.test/a.ts").is_err());
        assert_eq!(
            resolve(&base, "../c.ts").unwrap().as_str(),
            "https://x.test/c.ts"
        );
        assert_eq!(
            resolve(&base, "https://y.test/d.ts").unwrap().as_str(),
            "https://y.test/d.ts"
        );
    }

    #[test]
    fn segments_get_an_extension_a_player_can_open() {
        assert_eq!(joined_extension("https://x/a.m4s", true, false), "mp4");
        assert_eq!(joined_extension("https://x/a.m4s", true, true), "m4a");
        assert_eq!(joined_extension("https://x/a.ts?x=1", false, false), "ts");
        assert_eq!(joined_extension("https://x/a.webm", false, false), "webm");
        assert_eq!(joined_extension("https://x/segment", false, false), "ts");
        assert_eq!(joined_extension("https://x/segment", true, false), "mp4");
    }

    #[test]
    fn only_a_lone_whole_file_counts_as_a_single_file() {
        let part = |range| Part {
            url: "https://x.test/a.mp4".to_owned(),
            range,
        };
        let track = |parts| Track {
            parts,
            extension: "mp4".to_owned(),
            seconds: 1.0,
        };

        assert!(track(vec![part(None)]).is_single_file());
        assert!(!track(vec![part(None), part(None)]).is_single_file());
        let range = ByteRange {
            start: 0,
            length: 10,
        };
        assert!(!track(vec![part(Some(range))]).is_single_file());
    }

    #[test]
    fn the_size_estimate_comes_from_the_bit_rate() {
        let quality = Quality {
            label: "720p".to_owned(),
            width: None,
            height: Some(720),
            bandwidth: Some(8_000_000),
            video: Track {
                parts: Vec::new(),
                extension: "ts".to_owned(),
                seconds: 10.0,
            },
            audio: None,
            headers: None,
        };

        assert_eq!(quality.estimated_bytes(), Some(10_000_000));
    }
}
