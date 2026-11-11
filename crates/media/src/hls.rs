//! HLS playlists.

use std::collections::HashSet;
use std::future::Future;

use m3u8_rs::{
    AlternativeMedia, AlternativeMediaType, MasterPlaylist, MediaPlaylist, MediaPlaylistType,
    Playlist, VariantStream, parse_playlist_res,
};
use url::Url;

use crate::{
    ByteRange, Error, MAX_PARTS, MAX_QUALITIES, Part, Quality, Track, audio_label,
    joined_extension, parse_base, resolve, too_many_parts, video_label,
};

/// Refuses a playlist that declares any encryption. `METHOD=NONE` is the one value that
/// means there's none.
fn check_protection(text: &str) -> Result<(), Error> {
    for line in text.lines().map(str::trim) {
        let upper = line.to_ascii_uppercase();

        if let Some(attributes) = upper
            .strip_prefix("#EXT-X-KEY:")
            .or_else(|| upper.strip_prefix("#EXT-X-SESSION-KEY:"))
        {
            let plain = attributes
                .split(',')
                .any(|attribute| attribute.trim() == "METHOD=NONE");
            if !plain {
                return Err(Error::Protected);
            }
        }

        // Asks for a protected connection to the screen, which only applies to DRM.
        if upper.contains("HDCP-LEVEL=") && !upper.contains("HDCP-LEVEL=NONE") {
            return Err(Error::Protected);
        }
    }
    Ok(())
}

fn parse(text: &str) -> Result<Playlist, Error> {
    parse_playlist_res(text.as_bytes())
        .map_err(|_| Error::Invalid("the playlist isn't valid".to_owned()))
}

fn part_range(range: Option<&m3u8_rs::ByteRange>, next_start: u64) -> Option<ByteRange> {
    range.map(|range| ByteRange {
        start: range.offset.unwrap_or(next_start),
        length: range.length,
    })
}

/// Turns a playlist into the parts to fetch, in order.
fn track(base: &Url, playlist: &MediaPlaylist, audio: bool) -> Result<Track, Error> {
    if !playlist.end_list && playlist.playlist_type != Some(MediaPlaylistType::Vod) {
        return Err(Error::Live);
    }
    if playlist.segments.len() > MAX_PARTS {
        return Err(too_many_parts());
    }

    let mut parts = Vec::with_capacity(playlist.segments.len());
    let mut current_map = None;
    let mut last_url = String::new();
    let mut next_start = 0;
    let mut seconds = 0.0;
    let mut first_media_url = None;

    for segment in &playlist.segments {
        // Each new initialisation section goes in just before the parts that use it.
        if let Some(map) = &segment.map {
            let key = (map.uri.as_str(), map.byte_range.as_ref());
            if current_map != Some(key) {
                parts.push(Part {
                    url: resolve(base, &map.uri)?.into(),
                    range: part_range(map.byte_range.as_ref(), 0),
                });
                current_map = Some(key);
            }
        }

        let url: String = resolve(base, &segment.uri)?.into();
        if url != last_url {
            next_start = 0;
        }
        let range = part_range(segment.byte_range.as_ref(), next_start);
        if let Some(range) = range {
            next_start = range.start + range.length;
        }
        last_url.clone_from(&url);
        first_media_url.get_or_insert_with(|| url.clone());

        seconds += f64::from(segment.duration);
        parts.push(Part { url, range });
    }

    let extension = joined_extension(
        first_media_url.as_deref().unwrap_or_default(),
        current_map.is_some(),
        audio,
    );
    Ok(Track {
        parts,
        extension,
        seconds,
    })
}

/// Fetches a playlist and reads it as a list of parts.
async fn fetch_track<F, Fut>(url: &Url, audio: bool, fetch: &F) -> Result<Track, Error>
where
    F: Fn(String) -> Fut,
    Fut: Future<Output = Result<String, String>>,
{
    let text = fetch(url.to_string()).await.map_err(Error::Fetch)?;
    check_protection(&text)?;

    match parse(&text)? {
        Playlist::MediaPlaylist(playlist) => track(url, &playlist, audio),
        Playlist::MasterPlaylist(_) => Err(Error::Invalid(
            "a playlist pointed to another list of qualities".to_owned(),
        )),
    }
}

/// The separate sound for a quality, if it has one. Otherwise the sound is in its parts.
fn audio_for<'a>(
    master: &'a MasterPlaylist,
    variant: &VariantStream,
) -> Option<&'a AlternativeMedia> {
    let group = variant.audio.as_deref()?;
    let mut choices = master
        .alternatives
        .iter()
        .filter(|media| media.media_type == AlternativeMediaType::Audio)
        .filter(|media| media.group_id == group && media.uri.is_some());

    let first = choices.clone().next();
    choices.find(|media| media.default).or(first)
}

/// True for a quality that has sound but no picture.
fn is_audio_only(variant: &VariantStream) -> bool {
    let codecs = variant.codecs.as_deref().unwrap_or_default();
    variant.resolution.is_none() && codecs.contains("mp4a") && !codecs.contains("avc")
}

async fn read_master<F, Fut>(
    base: &Url,
    master: &MasterPlaylist,
    fetch: &F,
) -> Result<Vec<Quality>, Error>
where
    F: Fn(String) -> Fut,
    Fut: Future<Output = Result<String, String>>,
{
    let mut seen = HashSet::new();
    let mut qualities = Vec::new();

    let variants = master
        .variants
        .iter()
        .filter(|variant| !variant.is_i_frame)
        .filter(|variant| seen.insert(variant.uri.as_str()));

    for variant in variants.take(MAX_QUALITIES) {
        let audio_only = is_audio_only(variant);
        let video = fetch_track(&resolve(base, &variant.uri)?, audio_only, fetch).await?;

        let audio = match audio_for(master, variant).and_then(|media| media.uri.as_ref()) {
            Some(link) => Some(fetch_track(&resolve(base, link)?, true, fetch).await?),
            None => None,
        };

        let height = variant.resolution.as_ref().map(|size| size.height);
        let bandwidth = Some(variant.bandwidth);
        qualities.push(Quality {
            label: if audio_only {
                audio_label(bandwidth)
            } else {
                video_label(
                    variant.resolution.as_ref().map(|size| size.width),
                    height,
                    bandwidth,
                )
            },
            width: variant.resolution.as_ref().map(|size| size.width),
            height,
            bandwidth,
            video,
            audio,
            headers: None,
        });
    }
    Ok(qualities)
}

pub(crate) async fn discover<F, Fut>(
    url: &str,
    text: &str,
    fetch: &F,
) -> Result<Vec<Quality>, Error>
where
    F: Fn(String) -> Fut,
    Fut: Future<Output = Result<String, String>>,
{
    check_protection(text)?;
    let base = parse_base(url)?;

    match parse(text)? {
        Playlist::MasterPlaylist(master) => read_master(&base, &master, fetch).await,
        Playlist::MediaPlaylist(playlist) => Ok(vec![Quality {
            label: "Video".to_owned(),
            width: None,
            height: None,
            bandwidth: None,
            video: track(&base, &playlist, false)?,
            audio: None,
            headers: None,
        }]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_declared_key_is_refused_however_it_is_written() {
        for line in [
            "#EXT-X-KEY:METHOD=AES-128,URI=\"k\"",
            "#EXT-X-KEY:METHOD=SAMPLE-AES,URI=\"k\"",
            "#EXT-X-KEY:METHOD=SAMPLE-AES-CTR,URI=\"k\"",
            "#EXT-X-KEY:URI=\"k\",METHOD=AES-128",
            "#ext-x-key:method=aes-128,uri=\"k\"",
            "   #EXT-X-KEY:METHOD=SOMETHING-NEW,URI=\"k\"",
            "#EXT-X-SESSION-KEY:METHOD=AES-128,URI=\"k\"",
            "#EXT-X-KEY:URI=\"METHOD=NONE\"",
            "#EXT-X-STREAM-INF:BANDWIDTH=1,HDCP-LEVEL=TYPE-0",
            "#EXT-X-STREAM-INF:BANDWIDTH=1,HDCP-LEVEL=TYPE-1",
        ] {
            assert_eq!(check_protection(line), Err(Error::Protected), "{line}");
        }
    }

    #[test]
    fn saying_there_is_no_encryption_is_allowed() {
        for line in [
            "#EXT-X-KEY:METHOD=NONE",
            "#EXT-X-STREAM-INF:BANDWIDTH=1,HDCP-LEVEL=NONE",
            "#EXTINF:4.0,",
            "key.ts",
        ] {
            assert_eq!(check_protection(line), Ok(()), "{line}");
        }
    }

    fn read(text: &str) -> Result<Track, Error> {
        let base = Url::parse("https://x.test/a/list.m3u8").unwrap();
        match parse(text)? {
            Playlist::MediaPlaylist(playlist) => track(&base, &playlist, false),
            Playlist::MasterPlaylist(_) => panic!("read as a master playlist"),
        }
    }

    #[test]
    fn a_playlist_without_an_end_is_live() {
        let text = "#EXTM3U\n#EXT-X-TARGETDURATION:4\n#EXTINF:4.0,\na.ts\n";
        assert_eq!(read(text), Err(Error::Live));
    }

    #[test]
    fn a_video_on_demand_playlist_counts_as_finished() {
        let text = "#EXTM3U\n#EXT-X-PLAYLIST-TYPE:VOD\n#EXT-X-TARGETDURATION:4\n\
                    #EXTINF:4.0,\na.ts\n";
        assert_eq!(read(text).unwrap().parts.len(), 1);
    }

    #[test]
    fn a_range_with_no_start_follows_the_one_before_it() {
        let text = "#EXTM3U\n#EXT-X-TARGETDURATION:4\n\
                    #EXTINF:4.0,\n#EXT-X-BYTERANGE:100@50\na.ts\n\
                    #EXTINF:4.0,\n#EXT-X-BYTERANGE:200\na.ts\n\
                    #EXTINF:4.0,\n#EXT-X-BYTERANGE:300\nb.ts\n\
                    #EXT-X-ENDLIST\n";
        let parts = read(text).unwrap().parts;

        let range = |index: usize| parts[index].range.unwrap();
        assert_eq!((range(0).start, range(0).length), (50, 100));
        assert_eq!((range(1).start, range(1).length), (150, 200));
        assert_eq!((range(2).start, range(2).length), (0, 300), "a new file");
    }

    #[test]
    fn a_new_initialisation_section_goes_in_where_it_starts() {
        let text = "#EXTM3U\n#EXT-X-TARGETDURATION:4\n#EXT-X-MAP:URI=\"one.mp4\"\n\
                    #EXTINF:4.0,\na.m4s\n#EXTINF:4.0,\nb.m4s\n\
                    #EXT-X-DISCONTINUITY\n#EXT-X-MAP:URI=\"two.mp4\"\n\
                    #EXTINF:4.0,\nc.m4s\n#EXT-X-ENDLIST\n";
        let track = read(text).unwrap();

        let names: Vec<&str> = track
            .parts
            .iter()
            .map(|part| part.url.rsplit('/').next().unwrap())
            .collect();
        assert_eq!(names, ["one.mp4", "a.m4s", "b.m4s", "two.mp4", "c.m4s"]);
        assert_eq!(track.extension, "mp4");
    }

    #[test]
    fn links_are_resolved_against_the_playlist() {
        let text = "#EXTM3U\n#EXT-X-TARGETDURATION:4\n\
                    #EXTINF:4.0,\nb.ts?token=1\n#EXTINF:4.0,\n/c.ts\n#EXT-X-ENDLIST\n";
        let parts = read(text).unwrap().parts;

        assert_eq!(parts[0].url, "https://x.test/a/b.ts?token=1");
        assert_eq!(parts[1].url, "https://x.test/c.ts");
    }

    #[test]
    fn durations_add_up() {
        let text = "#EXTM3U\n#EXT-X-TARGETDURATION:4\n\
                    #EXTINF:4.0,\na.ts\n#EXTINF:2.5,\nb.ts\n#EXT-X-ENDLIST\n";
        assert_eq!(read(text).unwrap().seconds, 6.5);
    }

    #[test]
    fn nonsense_is_not_a_playlist() {
        assert!(matches!(
            parse("#EXTM3U\n#EXT"),
            Ok(_) | Err(Error::Invalid(_))
        ));
        assert!(matches!(parse("not a playlist"), Err(Error::Invalid(_))));
    }
}
