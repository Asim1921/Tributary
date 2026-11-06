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
    #[error("this address isn't a video stream")]
    NotAStream,
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

fn video_label(height: Option<u64>, bandwidth: Option<u64>) -> String {
    match (height, bandwidth) {
        (Some(height), _) => format!("{height}p"),
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
        };

        assert_eq!(quality.estimated_bytes(), Some(10_000_000));
    }
}
