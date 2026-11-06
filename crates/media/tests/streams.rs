//! Reads the test server's streams, checking each ends up as exactly the right parts.

use media::{Error, Part, Quality, discover};
use testserver::streams;

const BASE: &str = "http://test.local";

/// Stands in for the network, answering from the test server's streams.
async fn fetch(url: String) -> Result<String, String> {
    let address = url.strip_prefix(BASE).ok_or("wrong host")?;
    let (path, query) = address.split_once('?').unwrap_or((address, ""));
    let name = path.strip_prefix("/media/").ok_or("not a stream")?;

    streams::render(name, query)
        .map(|(_, text)| text)
        .ok_or_else(|| format!("no stream called {name}"))
}

async fn read(name: &str) -> Result<Vec<Quality>, Error> {
    discover(&format!("{BASE}/media/{name}"), fetch).await
}

fn seed_of(part: &Part) -> u64 {
    let (_, query) = part.url.split_once('?').unwrap();
    let seed = query.split('&').find_map(|pair| pair.strip_prefix("seed="));
    seed.unwrap().parse().unwrap()
}

/// Checks that parts are the track's pieces, in order.
fn assert_matches(parts: &[Part], track: &str) {
    let pieces = streams::track(track).unwrap();
    let expected: Vec<u64> = pieces.iter().map(|piece| piece.seed).collect();
    let found: Vec<u64> = parts.iter().map(seed_of).collect();
    assert_eq!(found, expected, "{track}");
}

#[tokio::test]
async fn qualities_come_best_first_with_their_parts() {
    let qualities = read("master.m3u8").await.unwrap();

    assert_eq!(qualities.len(), 2);
    assert_eq!(qualities[0].label, "720p");
    assert_eq!(qualities[0].height, Some(720));
    assert_eq!(qualities[0].width, Some(1280));
    assert_eq!(qualities[0].bandwidth, Some(2_400_000));
    assert_eq!(qualities[1].label, "360p");

    assert_matches(&qualities[0].video.parts, "high");
    assert_matches(&qualities[1].video.parts, "low");
    assert_eq!(qualities[0].video.extension, "ts");
    assert_eq!(qualities[0].video.seconds, 24.0);
    assert_eq!(qualities[0].audio, None);
}

#[tokio::test]
async fn links_resolve_to_real_server_paths() {
    let qualities = read("low.m3u8").await.unwrap();
    assert!(
        qualities[0].video.parts[0]
            .url
            .starts_with(&format!("{BASE}/file/20K.ts?seed="))
    );
}

#[tokio::test]
async fn a_single_playlist_is_one_quality() {
    let qualities = read("low.m3u8").await.unwrap();

    assert_eq!(qualities.len(), 1);
    assert_eq!(qualities[0].label, "Video");
    assert_matches(&qualities[0].video.parts, "low");
}

#[tokio::test]
async fn separate_sound_is_found() {
    let qualities = read("audio-master.m3u8").await.unwrap();

    let audio = qualities[0].audio.as_ref().unwrap();
    assert_matches(&qualities[0].video.parts, "low");
    assert_matches(&audio.parts, "audio");
}

#[tokio::test]
async fn fragmented_mp4_starts_with_its_initialisation() {
    let qualities = read("fmp4.m3u8").await.unwrap();

    assert_matches(&qualities[0].video.parts, "fmp4");
    assert_eq!(qualities[0].video.extension, "mp4");
}

#[tokio::test]
async fn parts_can_be_ranges_of_one_file() {
    let qualities = read("byterange.m3u8").await.unwrap();

    let parts = &qualities[0].video.parts;
    assert_eq!(parts.len(), 3);

    let mut position = 0;
    for part in parts {
        let range = part.range.unwrap();
        assert_eq!(range.start, position, "ranges must follow on");
        position += range.length;
    }
    assert_eq!(position, 300 * 1024);
}

#[tokio::test]
async fn saying_there_is_no_key_is_fine() {
    assert_matches(&read("key-none.m3u8").await.unwrap()[0].video.parts, "low");
}

#[tokio::test]
async fn every_kind_of_hls_encryption_is_refused() {
    for name in [
        "encrypted.m3u8",
        "sample-aes.m3u8",
        "late-key.m3u8",
        "session-key.m3u8",
    ] {
        assert_eq!(read(name).await, Err(Error::Protected), "{name}");
    }
}

#[tokio::test]
async fn one_encrypted_quality_refuses_the_whole_stream() {
    assert_eq!(read("mixed-master.m3u8").await, Err(Error::Protected));
}

#[tokio::test]
async fn dash_protection_is_refused() {
    assert_eq!(read("dash-protected.mpd").await, Err(Error::Protected));
}

#[tokio::test]
async fn live_streams_are_refused() {
    assert_eq!(read("live.m3u8").await, Err(Error::Live));
    assert_eq!(read("dash-live.mpd").await, Err(Error::Live));
}

/// The server serves these from `/dash/`, so seeds aren't in the addresses.
fn dash_names(parts: &[Part]) -> Vec<String> {
    parts
        .iter()
        .map(|part| part.url.rsplit('/').next().unwrap().to_owned())
        .collect()
}

#[tokio::test]
async fn dash_templates_give_each_quality_its_parts_and_shared_sound() {
    let qualities = read("dash.mpd").await.unwrap();

    assert_eq!(qualities.len(), 2);
    assert_eq!(qualities[0].label, "720p");
    assert_eq!(qualities[0].video.extension, "mp4");
    assert_eq!(qualities[0].video.seconds, 24.0);

    assert_eq!(
        dash_names(&qualities[0].video.parts),
        [
            "init.mp4",
            "seg-001.m4s",
            "seg-002.m4s",
            "seg-003.m4s",
            "seg-004.m4s",
            "seg-005.m4s",
            "seg-006.m4s"
        ]
    );
    assert!(qualities[0].video.parts[0].url.contains("/dash/v720/"));
    assert!(qualities[1].video.parts[0].url.contains("/dash/v360/"));

    let audio = qualities[0].audio.as_ref().unwrap();
    assert!(audio.parts[0].url.contains("/dash/a128/"));
    assert_eq!(audio.extension, "m4a");
    assert_eq!(qualities[1].audio.as_ref(), Some(audio));
}

#[tokio::test]
async fn dash_timelines_name_parts_by_time() {
    let qualities = read("dash-timeline.mpd").await.unwrap();

    assert_eq!(
        dash_names(&qualities[0].video.parts),
        [
            "init.mp4", "0.m4s", "4.m4s", "8.m4s", "12.m4s", "16.m4s", "20.m4s"
        ]
    );
}

#[tokio::test]
async fn dash_lists_give_their_parts_in_order() {
    let qualities = read("dash-list.mpd").await.unwrap();
    assert_matches(&qualities[0].video.parts, "l360");
}

#[tokio::test]
async fn a_dash_quality_can_simply_be_a_file() {
    let qualities = read("dash-base.mpd").await.unwrap();

    assert_eq!(qualities.len(), 2);
    assert!(qualities[0].video.is_single_file());
    assert_matches(&qualities[0].video.parts, "b720");
    assert_matches(&qualities[1].video.parts, "b360");
}

#[tokio::test]
async fn faults_given_to_a_stream_reach_its_parts() {
    let qualities = read("master.m3u8?fail=503&run=x").await.unwrap();

    for part in &qualities[0].video.parts {
        assert!(part.url.contains("fail=503&run=x"), "{}", part.url);
    }
}

#[tokio::test]
async fn escaped_addresses_in_a_manifest_are_read_back() {
    let qualities = read("dash-list.mpd?fail=503&run=x").await.unwrap();

    for part in &qualities[0].video.parts {
        assert!(part.url.contains("fail=503&run=x"), "{}", part.url);
    }
}

#[tokio::test]
async fn a_web_page_is_not_a_stream() {
    let fetch_page = |_| async { Ok("<!doctype html><html></html>".to_owned()) };
    assert_eq!(
        discover("http://x.test/a", fetch_page).await,
        Err(Error::NotAStream)
    );
}

#[tokio::test]
async fn a_failed_fetch_explains_itself() {
    let fail = |_| async { Err("server unavailable".to_owned()) };
    assert_eq!(
        discover("http://x.test/a.m3u8", fail).await,
        Err(Error::Fetch("server unavailable".to_owned()))
    );
}

#[tokio::test]
async fn a_failed_quality_is_reported_rather_than_skipped() {
    let partly = |url: String| async move {
        if url.ends_with("high.m3u8") {
            Err("gone".to_owned())
        } else {
            fetch(url).await
        }
    };
    assert_eq!(
        discover(&format!("{BASE}/media/master.m3u8"), partly).await,
        Err(Error::Fetch("gone".to_owned()))
    );
}

#[tokio::test]
async fn a_playlist_that_points_to_itself_cannot_loop() {
    let looping = |url: String| async move {
        let _ = url;
        Ok("#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=1\nself.m3u8\n".to_owned())
    };
    assert!(matches!(
        discover("http://x.test/self.m3u8", looping).await,
        Err(Error::Invalid(_))
    ));
}
