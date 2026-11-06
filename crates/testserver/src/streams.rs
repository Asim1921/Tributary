//! Test streams: HLS playlists and DASH manifests built on the generated files.
//!
//! `/media/NAME` serves a playlist or manifest, and anything in its query is added to
//! every part it lists, so the fault options in [`crate::scenario`] apply to a stream
//! too. For example, `/media/low.m3u8?fail=503&fail_times=1&run=a` fails each part once.
//!
//! | Name                  | What it is                                                  |
//! |-----------------------|-------------------------------------------------------------|
//! | `master.m3u8`         | Two qualities, `low.m3u8` and `high.m3u8`                   |
//! | `play`                | The same, at an address that doesn't show what it is        |
//! | `audio-master.m3u8`   | One quality, with its audio in `audio.m3u8`                 |
//! | `fmp4.m3u8`           | Fragmented MP4, with an initialisation part                 |
//! | `byterange.m3u8`      | Parts that are ranges of a single file                      |
//! | `key-none.m3u8`       | Declares `METHOD=NONE`, so it isn't encrypted               |
//! | `encrypted.m3u8`      | AES-128 encrypted                                           |
//! | `sample-aes.m3u8`     | SAMPLE-AES encrypted                                        |
//! | `late-key.m3u8`       | Only its later parts are encrypted                          |
//! | `session-key.m3u8`    | A master declaring a session key                            |
//! | `mixed-master.m3u8`   | One plain quality and one encrypted one                     |
//! | `live.m3u8`           | A live stream, with no end                                  |
//! | `dash.mpd`            | Two video qualities and audio, from a `SegmentTemplate`     |
//! | `dash-timeline.mpd`   | Parts listed by a `SegmentTimeline`, named by `$Time$`      |
//! | `dash-list.mpd`       | Parts listed one by one in a `SegmentList`                  |
//! | `dash-base.mpd`       | Each quality is one whole file                              |
//! | `dash-protected.mpd`  | Declares `ContentProtection`                                |
//! | `dash-live.mpd`       | A live stream, `type="dynamic"`                             |

use sha2::{Digest, Sha256};

use crate::content;

const HLS_TYPE: &str = "application/vnd.apple.mpegurl";
const DASH_TYPE: &str = "application/dash+xml";

const PIECE_SECONDS: u64 = 4;
const PIECES_PER_TRACK: u64 = 6;

/// One part of a stream. Each has its own size and content, so joining them in the wrong
/// order can't go unnoticed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Piece {
    pub size_kib: u64,
    pub seed: u64,
}

impl Piece {
    fn file_name(self, extension: &str) -> String {
        format!("{}K.{extension}", self.size_kib)
    }

    fn query(self, extra: &str) -> String {
        match extra {
            "" => format!("seed={}", self.seed),
            extra => format!("seed={}&{extra}", self.seed),
        }
    }

    /// Where this part is, relative to a playlist in `/media/`.
    fn link(self, extension: &str, extra: &str) -> String {
        format!(
            "../file/{}?{}",
            self.file_name(extension),
            self.query(extra)
        )
    }
}

fn pieces(first_seed: u64, scale: u64) -> Vec<Piece> {
    (0..PIECES_PER_TRACK)
        .map(|index| Piece {
            size_kib: (20 + 7 * index) * scale,
            seed: first_seed + index,
        })
        .collect()
}

/// Puts an initialisation part in front of a track.
fn with_init(first_seed: u64, scale: u64) -> Vec<Piece> {
    let init = Piece {
        size_kib: 3,
        seed: first_seed + PIECES_PER_TRACK,
    };
    let mut all = vec![init];
    all.extend(pieces(first_seed, scale));
    all
}

/// The parts of a track, in the order they join up.
pub fn track(name: &str) -> Option<Vec<Piece>> {
    Some(match name {
        "low" => pieces(1000, 1),
        "high" => pieces(2000, 3),
        "audio" => pieces(3000, 1),
        "fmp4" => with_init(4000, 1),
        "byterange" => vec![Piece {
            size_kib: 300,
            seed: 5000,
        }],
        "v360" => with_init(6000, 1),
        "v720" => with_init(7000, 3),
        "a128" => with_init(8000, 1),
        "t360" => with_init(9000, 1),
        "l360" => with_init(10_000, 1),
        "b360" => vec![Piece {
            size_kib: 200,
            seed: 11_000,
        }],
        "b720" => vec![Piece {
            size_kib: 500,
            seed: 12_000,
        }],
        _ => return None,
    })
}

/// The SHA-256 of a track's parts joined together.
pub fn sha256(pieces: &[Piece]) -> String {
    let mut hasher = Sha256::new();
    for piece in pieces {
        let mut data = vec![0; piece.size_kib as usize * 1024];
        content::fill(piece.seed, 0, &mut data);
        hasher.update(&data);
    }
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn playlist_start(extra_lines: &str) -> String {
    format!(
        "#EXTM3U\n#EXT-X-VERSION:7\n#EXT-X-TARGETDURATION:{PIECE_SECONDS}\n\
         #EXT-X-MEDIA-SEQUENCE:0\n{extra_lines}"
    )
}

fn entries(pieces: &[Piece], extension: &str, query: &str) -> String {
    pieces
        .iter()
        .map(|piece| {
            format!(
                "#EXTINF:{PIECE_SECONDS}.0,\n{}\n",
                piece.link(extension, query)
            )
        })
        .collect()
}

/// A finished playlist of a whole track.
fn simple_playlist(name: &str, query: &str, extra_lines: &str) -> String {
    let track = track(name).unwrap_or_default();
    format!(
        "{}{}#EXT-X-ENDLIST\n",
        playlist_start(extra_lines),
        entries(&track, "ts", query)
    )
}

fn fmp4_playlist(query: &str) -> String {
    let all = track("fmp4").unwrap_or_default();
    let map = format!("#EXT-X-MAP:URI=\"{}\"\n", all[0].link("mp4", query));
    format!(
        "{}{}#EXT-X-ENDLIST\n",
        playlist_start(&map),
        entries(&all[1..], "m4s", query)
    )
}

fn byterange_playlist(query: &str) -> String {
    let file = track("byterange").unwrap_or_default()[0].link("ts", query);
    let (first, second, third) = (100 * 1024, 120 * 1024, 80 * 1024);
    format!(
        "{}#EXTINF:4.0,\n#EXT-X-BYTERANGE:{first}@0\n{file}\n\
         #EXTINF:4.0,\n#EXT-X-BYTERANGE:{second}\n{file}\n\
         #EXTINF:4.0,\n#EXT-X-BYTERANGE:{third}\n{file}\n#EXT-X-ENDLIST\n",
        playlist_start("")
    )
}

/// Half of the parts are plain and the rest encrypted.
fn late_key_playlist(query: &str) -> String {
    let all = track("low").unwrap_or_default();
    let (plain, encrypted) = all.split_at(all.len() / 2);
    format!(
        "{}{}#EXT-X-KEY:METHOD=AES-128,URI=\"key.bin\"\n{}#EXT-X-ENDLIST\n",
        playlist_start(""),
        entries(plain, "ts", query),
        entries(encrypted, "ts", query)
    )
}

fn live_playlist(query: &str) -> String {
    let all = track("low").unwrap_or_default();
    format!("{}{}", playlist_start(""), entries(&all, "ts", query))
}

/// A link to another playlist in `/media/`, keeping the query.
fn sibling(name: &str, query: &str) -> String {
    match query {
        "" => name.to_owned(),
        query => format!("{name}?{query}"),
    }
}

fn master(lines: &str, low: &str, high: &str, query: &str) -> String {
    format!(
        "#EXTM3U\n#EXT-X-VERSION:7\n{lines}\
         #EXT-X-STREAM-INF:BANDWIDTH=800000,RESOLUTION=640x360,CODECS=\"avc1.4d401e,mp4a.40.2\"\n\
         {}\n\
         #EXT-X-STREAM-INF:BANDWIDTH=2400000,RESOLUTION=1280x720,CODECS=\"avc1.4d401f,mp4a.40.2\"\n\
         {}\n",
        sibling(low, query),
        sibling(high, query)
    )
}

fn audio_master(query: &str) -> String {
    format!(
        "#EXTM3U\n#EXT-X-VERSION:7\n\
         #EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"sound\",NAME=\"English\",DEFAULT=YES,URI=\"{}\"\n\
         #EXT-X-STREAM-INF:BANDWIDTH=800000,RESOLUTION=640x360,AUDIO=\"sound\"\n{}\n",
        sibling("audio.m3u8", query),
        sibling("low.m3u8", query)
    )
}

fn mpd(attributes: &str, body: &str) -> String {
    let seconds = PIECE_SECONDS * PIECES_PER_TRACK;
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <MPD xmlns=\"urn:mpeg:dash:schema:mpd:2011\" profiles=\"urn:mpeg:dash:profile:isoff-on-demand:2011\" \
         minBufferTime=\"PT2S\" mediaPresentationDuration=\"PT{seconds}S\" {attributes}>\n\
         <Period>\n{body}</Period>\n</MPD>\n"
    )
}

/// Escapes a query for use inside an XML attribute.
fn xml_query(query: &str) -> String {
    match query {
        "" => String::new(),
        query => format!("?{}", query.replace('&', "&amp;")),
    }
}

fn template(query: &str) -> String {
    let query = xml_query(query);
    format!(
        "<SegmentTemplate timescale=\"1\" duration=\"{PIECE_SECONDS}\" startNumber=\"1\" \
         initialization=\"../dash/$RepresentationID$/init.mp4{query}\" \
         media=\"../dash/$RepresentationID$/seg-$Number%03d$.m4s{query}\"/>\n"
    )
}

fn dash_template(attributes: &str, video_lines: &str, query: &str) -> String {
    let template = template(query);
    mpd(
        attributes,
        &format!(
            "<AdaptationSet contentType=\"video\" mimeType=\"video/mp4\">\n{video_lines}{template}\
             <Representation id=\"v360\" bandwidth=\"800000\" width=\"640\" height=\"360\" codecs=\"avc1.4d401e\"/>\n\
             <Representation id=\"v720\" bandwidth=\"2400000\" width=\"1280\" height=\"720\" codecs=\"avc1.4d401f\"/>\n\
             </AdaptationSet>\n\
             <AdaptationSet contentType=\"audio\" mimeType=\"audio/mp4\" lang=\"en\">\n{template}\
             <Representation id=\"a128\" bandwidth=\"128000\" codecs=\"mp4a.40.2\"/>\n\
             </AdaptationSet>\n"
        ),
    )
}

fn dash_timeline(query: &str) -> String {
    let query = xml_query(query);
    let repeats = PIECES_PER_TRACK - 2;
    mpd(
        "type=\"static\"",
        &format!(
            "<AdaptationSet contentType=\"video\" mimeType=\"video/mp4\">\n\
             <SegmentTemplate timescale=\"1\" initialization=\"../dash/t360/init.mp4{query}\" \
             media=\"../dash/t360/$Time$.m4s{query}\">\n\
             <SegmentTimeline><S t=\"0\" d=\"{PIECE_SECONDS}\" r=\"{repeats}\"/><S d=\"{PIECE_SECONDS}\"/></SegmentTimeline>\n\
             </SegmentTemplate>\n\
             <Representation id=\"t360\" bandwidth=\"800000\" width=\"640\" height=\"360\"/>\n\
             </AdaptationSet>\n"
        ),
    )
}

fn dash_list(query: &str) -> String {
    let all = track("l360").unwrap_or_default();
    let link = |piece: &Piece, extension| piece.link(extension, query).replace('&', "&amp;");
    let parts: String = all[1..]
        .iter()
        .map(|piece| format!("<SegmentURL media=\"{}\"/>\n", link(piece, "m4s")))
        .collect();
    mpd(
        "type=\"static\"",
        &format!(
            "<AdaptationSet contentType=\"video\" mimeType=\"video/mp4\">\n\
             <Representation id=\"l360\" bandwidth=\"800000\" width=\"640\" height=\"360\">\n\
             <SegmentList timescale=\"1\" duration=\"{PIECE_SECONDS}\">\n\
             <Initialization sourceURL=\"{}\"/>\n{parts}</SegmentList>\n\
             </Representation>\n</AdaptationSet>\n",
            link(&all[0], "mp4")
        ),
    )
}

fn dash_base(query: &str) -> String {
    let file = |name| {
        track(name).unwrap_or_default()[0]
            .link("mp4", query)
            .replace('&', "&amp;")
    };
    mpd(
        "type=\"static\"",
        &format!(
            "<AdaptationSet contentType=\"video\" mimeType=\"video/mp4\">\n\
             <Representation id=\"b360\" bandwidth=\"800000\" width=\"640\" height=\"360\">\n\
             <BaseURL>{}</BaseURL>\n</Representation>\n\
             <Representation id=\"b720\" bandwidth=\"2400000\" width=\"1280\" height=\"720\">\n\
             <BaseURL>{}</BaseURL>\n</Representation>\n\
             </AdaptationSet>\n",
            file("b360"),
            file("b720")
        ),
    )
}

/// Builds a playlist or manifest. Returns its content type and text.
pub fn render(name: &str, query: &str) -> Option<(&'static str, String)> {
    let aes = "#EXT-X-KEY:METHOD=AES-128,URI=\"key.bin\"\n";
    let sample_aes = "#EXT-X-KEY:METHOD=SAMPLE-AES,URI=\"key.bin\"\n";
    let session_key = "#EXT-X-SESSION-KEY:METHOD=AES-128,URI=\"key.bin\"\n";
    let protection = "<ContentProtection schemeIdUri=\"urn:mpeg:dash:mp4protection:2011\" \
                      value=\"cenc\"/>\n";

    let hls = |text| Some((HLS_TYPE, text));
    let dash = |text| Some((DASH_TYPE, text));

    match name {
        "master.m3u8" | "play" => hls(master("", "low.m3u8", "high.m3u8", query)),
        "mixed-master.m3u8" => hls(master("", "low.m3u8", "encrypted.m3u8", query)),
        "session-key.m3u8" => hls(master(session_key, "low.m3u8", "high.m3u8", query)),
        "audio-master.m3u8" => hls(audio_master(query)),
        "low.m3u8" => hls(simple_playlist("low", query, "")),
        "high.m3u8" => hls(simple_playlist("high", query, "")),
        "audio.m3u8" => hls(simple_playlist("audio", query, "")),
        "key-none.m3u8" => hls(simple_playlist("low", query, "#EXT-X-KEY:METHOD=NONE\n")),
        "encrypted.m3u8" => hls(simple_playlist("low", query, aes)),
        "sample-aes.m3u8" => hls(simple_playlist("low", query, sample_aes)),
        "late-key.m3u8" => hls(late_key_playlist(query)),
        "live.m3u8" => hls(live_playlist(query)),
        "fmp4.m3u8" => hls(fmp4_playlist(query)),
        "byterange.m3u8" => hls(byterange_playlist(query)),
        "dash.mpd" => dash(dash_template("type=\"static\"", "", query)),
        "dash-protected.mpd" => dash(dash_template("type=\"static\"", protection, query)),
        "dash-live.mpd" => dash(dash_template("type=\"dynamic\"", "", query)),
        "dash-timeline.mpd" => dash(dash_timeline(query)),
        "dash-list.mpd" => dash(dash_list(query)),
        "dash-base.mpd" => dash(dash_base(query)),
        _ => None,
    }
}

/// Finds the file behind a `/dash/TRACK/PART` address. Returns the file's name and query.
pub fn dash_piece(path: &str, query: &str) -> Option<(String, String)> {
    let (track_name, part) = path.strip_prefix("/dash/")?.split_once('/')?;
    let all = track(track_name)?;

    let (index, extension) = if part == "init.mp4" {
        (0, "mp4")
    } else if let Some(number) = part
        .strip_prefix("seg-")
        .and_then(|rest| rest.strip_suffix(".m4s"))
    {
        (number.parse().ok()?, "m4s")
    } else {
        let time: u64 = part.strip_suffix(".m4s")?.parse().ok()?;
        ((time / PIECE_SECONDS) as usize + 1, "m4s")
    };

    let piece = all.get(index)?;
    Some((piece.file_name(extension), piece.query(query)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const EVERY_NAME: [&str; 20] = [
        "master.m3u8",
        "mixed-master.m3u8",
        "session-key.m3u8",
        "audio-master.m3u8",
        "low.m3u8",
        "high.m3u8",
        "audio.m3u8",
        "key-none.m3u8",
        "encrypted.m3u8",
        "sample-aes.m3u8",
        "late-key.m3u8",
        "live.m3u8",
        "fmp4.m3u8",
        "byterange.m3u8",
        "dash.mpd",
        "dash-protected.mpd",
        "dash-live.mpd",
        "dash-timeline.mpd",
        "dash-list.mpd",
        "dash-base.mpd",
    ];

    #[test]
    fn every_stream_can_be_built() {
        for name in EVERY_NAME {
            assert!(render(name, "").is_some(), "{name}");
        }
        assert!(render("missing.m3u8", "").is_none());
    }

    #[test]
    fn the_parts_of_a_track_all_differ() {
        for name in ["low", "high", "audio", "v360", "v720", "a128"] {
            let all = track(name).unwrap();
            let mut seeds: Vec<u64> = all.iter().map(|piece| piece.seed).collect();
            let mut sizes: Vec<u64> = all.iter().map(|piece| piece.size_kib).collect();
            seeds.dedup();
            sizes.dedup();
            assert_eq!(seeds.len(), all.len(), "{name}");
            assert_eq!(sizes.len(), all.len(), "{name}");
        }
    }

    #[test]
    fn joining_parts_in_another_order_changes_the_checksum() {
        let mut all = track("low").unwrap();
        let right = sha256(&all);
        all.swap(1, 2);
        assert_ne!(sha256(&all), right);
    }

    #[test]
    fn faults_reach_every_part_of_a_stream() {
        let (_, text) = render("low.m3u8", "fail=503&run=a").unwrap();
        let links = text.lines().filter(|line| !line.starts_with('#'));
        assert!(links.clone().count() > 1);
        assert!(links.clone().all(|link| link.ends_with("&fail=503&run=a")));
    }

    #[test]
    fn faults_stay_valid_xml_in_a_manifest() {
        for name in [
            "dash.mpd",
            "dash-timeline.mpd",
            "dash-list.mpd",
            "dash-base.mpd",
        ] {
            let (_, text) = render(name, "fail=503&run=a").unwrap();
            let bare = text.replace("&amp;", "");
            assert!(!bare.contains('&'), "{name} has a bare ampersand");
            assert!(text.contains("fail=503&amp;run=a"), "{name}");
        }
    }

    #[test]
    fn finds_the_files_behind_dash_addresses() {
        let all = track("v360").unwrap();

        let (name, query) = dash_piece("/dash/v360/init.mp4", "").unwrap();
        assert_eq!(name, format!("{}K.mp4", all[0].size_kib));
        assert_eq!(query, format!("seed={}", all[0].seed));

        let (_, query) = dash_piece("/dash/v360/seg-002.m4s", "fail=503").unwrap();
        assert_eq!(query, format!("seed={}&fail=503", all[2].seed));

        let (_, query) = dash_piece("/dash/t360/4.m4s", "").unwrap();
        assert_eq!(query, format!("seed={}", track("t360").unwrap()[2].seed));

        assert!(dash_piece("/dash/nothing/init.mp4", "").is_none());
        assert!(dash_piece("/dash/v360/seg-099.m4s", "").is_none());
    }
}
