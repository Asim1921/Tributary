//! Downloads the test server's streams through the engine.
//!
//! The main check throughout: whatever goes wrong on the way, the finished file's SHA-256
//! must match the stream's parts joined in order.

use std::fs::{self, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use engine::{
    Config, Download, DownloadId, Engine, Login, NewDownload, QueueId, RetryPolicy, State,
};
use media::{Part, Quality, discover};
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use testserver::TestServer;
use testserver::streams::{self, Piece};

struct Fixture {
    server: TestServer,
    engine: Engine,
    folder: TempDir,
}

fn fast_config() -> Config {
    Config {
        retry: RetryPolicy {
            attempts: 5,
            base_delay: Duration::from_millis(10),
            max_delay: Duration::from_millis(100),
        },
        checkpoint_interval: Duration::from_millis(50),
        progress_interval: Duration::from_millis(25),
        ..Config::default()
    }
}

async fn fixture() -> Fixture {
    let folder = tempfile::tempdir().unwrap();
    Fixture {
        server: TestServer::start(testserver::Config::default())
            .await
            .unwrap(),
        engine: Engine::open(&folder.path().join("downloads.db"), fast_config()).unwrap(),
        folder,
    }
}

fn sha256_of(path: &Path) -> String {
    let mut hasher = Sha256::new();
    hasher.update(fs::read(path).unwrap());
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[track_caller]
fn assert_saved(download: &Download, pieces: &[Piece]) {
    assert_eq!(
        download.state,
        State::Completed,
        "error: {:?}",
        download.error
    );
    let path = download.final_path.as_ref().unwrap();
    let size: u64 = pieces.iter().map(|piece| piece.size_kib * 1024).sum();

    assert_eq!(fs::metadata(path).unwrap().len(), size);
    assert_eq!(sha256_of(path), streams::sha256(pieces), "wrong content");
    assert_eq!(download.downloaded, size);
    assert_eq!(download.size, Some(size));
}

#[track_caller]
fn assert_saved_track(download: &Download, track: &str) {
    assert_saved(download, &streams::track(track).unwrap());
}

impl Fixture {
    fn out(&self) -> PathBuf {
        self.folder.path().join("out")
    }

    fn database(&self) -> PathBuf {
        self.folder.path().join("downloads.db")
    }

    async fn qualities(&self, target: &str) -> Vec<Quality> {
        let engine = self.engine.clone();
        let fetch = move |url: String| {
            let engine = engine.clone();
            async move { engine.fetch_text(&url, &[], None).await }
        };
        discover(&self.server.url(target), fetch).await.unwrap()
    }

    /// The parts of a stream's best quality.
    async fn parts(&self, target: &str) -> Vec<Part> {
        self.qualities(target).await.remove(0).video.parts
    }

    fn new_stream(&self, parts: Vec<Part>) -> NewDownload {
        NewDownload {
            url: self.server.url("/media/stream"),
            dest_dir: self.out(),
            file_name: Some("video.ts".to_owned()),
            parts: Some(parts),
            ..NewDownload::default()
        }
    }

    fn start(&self, parts: Vec<Part>) -> DownloadId {
        self.engine.add(self.new_stream(parts)).unwrap()
    }

    async fn save(&self, parts: Vec<Part>) -> Download {
        let id = self.start(parts);
        self.engine.wait(id).await.unwrap()
    }

    /// The `.part` file of a stream.
    fn part_file(&self, id: DownloadId) -> PathBuf {
        self.out().join(format!("video.ts.{}.part", id.0))
    }

    /// Waits until at least this many bytes are confirmed on disk.
    async fn wait_for_progress(&self, id: DownloadId, bytes: u64) {
        let deadline = Instant::now() + Duration::from_secs(30);
        while self.engine.get(id).unwrap().downloaded < bytes {
            assert!(Instant::now() < deadline, "timed out waiting for progress");
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }

    fn requests_for(&self, seed: u64) -> usize {
        let wanted = format!("seed={seed}");
        self.server
            .records()
            .iter()
            .filter(|record| {
                record
                    .target
                    .split(['?', '&'])
                    .any(|option| option == wanted)
            })
            .count()
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_stream_joins_up_into_the_right_file() {
    let fixture = fixture().await;
    let parts = fixture.parts("/media/low.m3u8").await;

    assert_saved_track(&fixture.save(parts).await, "low");

    let names: Vec<_> = fs::read_dir(fixture.out())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(names, ["video.ts"], "no .part file should remain");
}

#[tokio::test(flavor = "multi_thread")]
async fn each_quality_saves_its_own_parts() {
    let fixture = fixture().await;
    let qualities = fixture.qualities("/media/master.m3u8").await;

    let best = fixture.save(qualities[0].video.parts.clone()).await;
    assert_saved_track(&best, "high");

    let mut smaller = fixture.new_stream(qualities[1].video.parts.clone());
    smaller.file_name = Some("small.ts".to_owned());
    let id = fixture.engine.add(smaller).unwrap();
    assert_saved_track(&fixture.engine.wait(id).await.unwrap(), "low");
}

#[tokio::test(flavor = "multi_thread")]
async fn fragmented_video_keeps_its_initialisation_first() {
    let fixture = fixture().await;
    let parts = fixture.parts("/media/fmp4.m3u8").await;
    assert_saved_track(&fixture.save(parts).await, "fmp4");
}

#[tokio::test(flavor = "multi_thread")]
async fn dash_video_and_sound_are_each_saved_whole() {
    let fixture = fixture().await;
    let qualities = fixture.qualities("/media/dash.mpd").await;

    let video = fixture.save(qualities[1].video.parts.clone()).await;
    assert_saved_track(&video, "v360");

    let mut sound = fixture.new_stream(qualities[1].audio.clone().unwrap().parts);
    sound.file_name = Some("sound.m4a".to_owned());
    let id = fixture.engine.add(sound).unwrap();
    assert_saved_track(&fixture.engine.wait(id).await.unwrap(), "a128");
}

#[tokio::test(flavor = "multi_thread")]
async fn dash_timelines_and_lists_download_correctly() {
    let fixture = fixture().await;

    let timeline = fixture.parts("/media/dash-timeline.mpd").await;
    assert_saved_track(&fixture.save(timeline).await, "t360");

    let mut list = fixture.new_stream(fixture.parts("/media/dash-list.mpd").await);
    list.file_name = Some("list.mp4".to_owned());
    let id = fixture.engine.add(list).unwrap();
    assert_saved_track(&fixture.engine.wait(id).await.unwrap(), "l360");
}

#[tokio::test(flavor = "multi_thread")]
async fn ranges_of_one_file_join_back_into_it() {
    let fixture = fixture().await;
    let parts = fixture.parts("/media/byterange.m3u8").await;
    assert_saved_track(&fixture.save(parts).await, "byterange");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_server_that_ignores_ranges_still_gives_the_right_bytes() {
    let fixture = fixture().await;
    let parts = fixture.parts("/media/byterange.m3u8?ranges=0").await;
    assert_saved_track(&fixture.save(parts).await, "byterange");
}

#[tokio::test(flavor = "multi_thread")]
async fn temporary_failures_are_retried() {
    let fixture = fixture().await;
    let parts = fixture
        .parts("/media/low.m3u8?fail=503&fail_times=2&run=retry")
        .await;

    assert_saved_track(&fixture.save(parts).await, "low");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_connection_cut_mid_part_fetches_that_part_again() {
    let fixture = fixture().await;
    let parts = fixture
        .parts("/media/low.m3u8?drop_after=5K&run=drop")
        .await;

    assert_saved_track(&fixture.save(parts).await, "low");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_part_that_is_gone_fails_and_says_which() {
    let fixture = fixture().await;
    let mut parts = fixture.parts("/media/low.m3u8").await;
    parts[3].url.push_str("&fail=404&fail_times=99");

    let failed = fixture.save(parts).await;

    assert_eq!(failed.state, State::Failed);
    let error = failed.error.unwrap();
    assert!(error.contains("part 4 of 6"), "{error}");
    assert!(error.contains("404"), "{error}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failing_part_gives_up_after_the_allowed_retries() {
    let fixture = fixture().await;
    let mut parts = fixture.parts("/media/low.m3u8").await;
    parts[1].url.push_str("&fail=503&fail_times=99");

    let failed = fixture.save(parts).await;

    assert_eq!(failed.state, State::Failed);
    assert!(failed.error.unwrap().contains("part 2 of 6"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_web_page_is_never_joined_into_the_video() {
    let fixture = fixture().await;
    let mut parts = fixture.parts("/media/low.m3u8").await;
    parts[2].url = fixture.server.url("/");

    let failed = fixture.save(parts).await;

    assert_eq!(failed.state, State::Failed);
    assert!(failed.error.unwrap().contains("web page"));
}

#[tokio::test(flavor = "multi_thread")]
async fn pausing_keeps_finished_parts_and_resuming_fetches_only_the_rest() {
    let fixture = fixture().await;
    let parts = fixture.parts("/media/high.m3u8?rate=150K").await;
    let track = streams::track("high").unwrap();

    let id = fixture.start(parts);
    fixture
        .wait_for_progress(id, track[0].size_kib * 1024)
        .await;
    fixture.engine.pause(id).await.unwrap();

    let paused = fixture.engine.get(id).unwrap();
    assert_eq!(paused.state, State::Paused);
    assert!(paused.parts_done >= 1);
    assert!(fixture.part_file(id).exists());

    fixture.engine.resume(id, &[]).unwrap();
    assert_saved_track(&fixture.engine.wait(id).await.unwrap(), "high");

    assert_eq!(
        fixture.requests_for(track[0].seed),
        1,
        "a finished part must not be fetched again"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_restart_carries_on_from_the_saved_parts() {
    let fixture = fixture().await;
    let parts = fixture.parts("/media/high.m3u8?rate=150K").await;
    let track = streams::track("high").unwrap();

    let id = fixture.start(parts);
    fixture
        .wait_for_progress(id, track[0].size_kib * 1024)
        .await;
    fixture.engine.pause(id).await.unwrap();

    let reopened = Engine::open(&fixture.database(), fast_config()).unwrap();
    reopened.resume(id, &[]).unwrap();
    assert_saved_track(&reopened.wait(id).await.unwrap(), "high");

    assert_eq!(fixture.requests_for(track[0].seed), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn data_past_the_last_checkpoint_is_cut_off_after_a_crash() {
    let fixture = fixture().await;
    let parts = fixture.parts("/media/high.m3u8?rate=150K").await;
    let track = streams::track("high").unwrap();

    let id = fixture.start(parts);
    fixture
        .wait_for_progress(id, track[0].size_kib * 1024)
        .await;
    fixture.engine.pause(id).await.unwrap();

    // A crash can leave written data that the database doesn't count yet. This is longer
    // than everything still to come, so the rest of the stream can't simply overwrite it.
    let mut file = OpenOptions::new()
        .append(true)
        .open(fixture.part_file(id))
        .unwrap();
    file.write_all(&vec![0xEE; 4 << 20]).unwrap();
    drop(file);

    let reopened = Engine::open(&fixture.database(), fast_config()).unwrap();
    reopened.resume(id, &[]).unwrap();
    assert_saved_track(&reopened.wait(id).await.unwrap(), "high");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_part_file_shorter_than_expected_starts_again_rather_than_leaving_a_gap() {
    let fixture = fixture().await;
    let parts = fixture.parts("/media/high.m3u8?rate=150K").await;
    let track = streams::track("high").unwrap();

    let id = fixture.start(parts);
    fixture
        .wait_for_progress(id, track[0].size_kib * 1024)
        .await;
    fixture.engine.pause(id).await.unwrap();

    let file = OpenOptions::new()
        .write(true)
        .open(fixture.part_file(id))
        .unwrap();
    file.set_len(100).unwrap();
    drop(file);

    fixture.engine.resume(id, &[]).unwrap();
    assert_saved_track(&fixture.engine.wait(id).await.unwrap(), "high");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_part_file_that_has_gone_starts_again() {
    let fixture = fixture().await;
    let parts = fixture.parts("/media/high.m3u8?rate=150K").await;
    let track = streams::track("high").unwrap();

    let id = fixture.start(parts);
    fixture
        .wait_for_progress(id, track[0].size_kib * 1024)
        .await;
    fixture.engine.pause(id).await.unwrap();
    fs::remove_file(fixture.part_file(id)).unwrap();

    fixture.engine.resume(id, &[]).unwrap();
    assert_saved_track(&fixture.engine.wait(id).await.unwrap(), "high");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_long_stream_of_tiny_parts_stays_in_order() {
    let fixture = fixture().await;
    let pieces: Vec<Piece> = (0..600)
        .map(|index| Piece {
            size_kib: 1 + index % 5,
            seed: 20_000 + index,
        })
        .collect();
    let parts: Vec<Part> = pieces
        .iter()
        .map(|piece| Part {
            url: fixture
                .server
                .url(&format!("/file/{}K.ts?seed={}", piece.size_kib, piece.seed)),
            range: None,
        })
        .collect();

    assert_saved(&fixture.save(parts).await, &pieces);
}

#[tokio::test(flavor = "multi_thread")]
async fn one_slow_part_does_not_let_the_rest_pile_up_in_memory() {
    let fixture = fixture().await;
    let pieces: Vec<Piece> = (0..40)
        .map(|index| Piece {
            size_kib: 1,
            seed: 30_000 + index,
        })
        .collect();
    let mut parts: Vec<Part> = pieces
        .iter()
        .map(|piece| Part {
            url: fixture
                .server
                .url(&format!("/file/1K.ts?seed={}", piece.seed)),
            range: None,
        })
        .collect();
    parts[0].url.push_str("&delay=700");

    let id = fixture.start(parts);
    tokio::time::sleep(Duration::from_millis(400)).await;

    let fetched = fixture.server.records().len();
    assert!(fetched > 0, "the other connections should keep working");
    assert!(fetched <= 8, "{fetched} parts fetched ahead of a stuck one");

    assert_saved(&fixture.engine.wait(id).await.unwrap(), &pieces);
}

#[tokio::test(flavor = "multi_thread")]
async fn cookies_reach_every_part_and_are_never_saved() {
    let fixture = fixture().await;
    let parts = fixture
        .parts("/media/low.m3u8?require_cookie=secret%3Dtoken123")
        .await;

    let id = fixture
        .engine
        .add(NewDownload {
            headers: vec![("Cookie".to_owned(), "secret=token123".to_owned())],
            ..fixture.new_stream(parts)
        })
        .unwrap();

    assert_saved_track(&fixture.engine.wait(id).await.unwrap(), "low");

    let database = fs::read(fixture.database()).unwrap();
    let text = String::from_utf8_lossy(&database);
    assert!(!text.contains("secret=token123"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_stream_behind_a_login_uses_it() {
    let fixture = fixture().await;
    let parts = fixture.parts("/media/low.m3u8?basic=user%3Apass").await;

    let id = fixture
        .engine
        .add(NewDownload {
            login: Some(Login {
                username: "user".to_owned(),
                password: "pass".to_owned(),
            }),
            ..fixture.new_stream(parts)
        })
        .unwrap();

    assert_saved_track(&fixture.engine.wait(id).await.unwrap(), "low");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_stream_needing_a_login_says_so() {
    let fixture = fixture().await;
    let parts = fixture.parts("/media/low.m3u8?basic=user%3Apass").await;

    let failed = fixture.save(parts).await;

    assert_eq!(failed.state, State::Failed);
    assert!(failed.error.unwrap().contains(engine::LOGIN_NEEDED));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_stream_runs_in_a_queue_like_any_download() {
    let fixture = fixture().await;
    let parts = fixture.parts("/media/low.m3u8").await;

    let id = fixture
        .engine
        .add(NewDownload {
            paused: true,
            queue: Some(QueueId::MAIN),
            ..fixture.new_stream(parts)
        })
        .unwrap();

    fixture.engine.start_queue(QueueId::MAIN).unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    while fixture.engine.get(id).unwrap().state != State::Completed {
        assert!(Instant::now() < deadline, "the queue never finished it");
        fixture.engine.tick(engine::Moment::now()).await.unwrap();
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    assert_saved_track(&fixture.engine.get(id).unwrap(), "low");
}

#[tokio::test(flavor = "multi_thread")]
async fn removing_a_stream_deletes_its_part_file() {
    let fixture = fixture().await;
    let parts = fixture.parts("/media/high.m3u8?rate=150K").await;

    let id = fixture.start(parts);
    fixture.wait_for_progress(id, 1).await;
    let part_file = fixture.part_file(id);
    assert!(part_file.exists());

    fixture.engine.remove(id, false).await.unwrap();

    assert!(!part_file.exists());
    assert!(fixture.engine.get(id).is_err());
}

#[tokio::test(flavor = "multi_thread")]
async fn the_estimated_total_settles_on_the_real_size() {
    let fixture = fixture().await;
    let mut events = fixture.engine.subscribe();
    let parts = fixture.parts("/media/high.m3u8?rate=300K").await;
    let id = fixture.start(parts);

    let real: u64 = streams::track("high")
        .unwrap()
        .iter()
        .map(|piece| piece.size_kib * 1024)
        .sum();

    let mut last_estimate = None;
    loop {
        match events.recv().await.unwrap() {
            engine::Event::Progress { total, .. } => last_estimate = total.or(last_estimate),
            engine::Event::State { id: changed, state }
                if changed == id && state != State::Running =>
            {
                break;
            }
            _ => {}
        }
    }

    let estimate = last_estimate.expect("no estimate was ever reported");
    let difference = estimate.abs_diff(real) as f64 / real as f64;
    assert!(difference < 0.5, "estimated {estimate}, really {real}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_playlist_can_be_fetched_past_a_temporary_failure() {
    let fixture = fixture().await;
    let url = fixture
        .server
        .url("/file/1K.txt?fail=503&fail_times=2&run=text");

    let text = fixture.engine.fetch_text(&url, &[], None).await.unwrap();
    assert!(!text.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_playlist_behind_a_login_is_fetched_with_it() {
    let fixture = fixture().await;
    let url = fixture.server.url("/file/1K.txt?digest=user%3Apass");
    let login = Login {
        username: "user".to_owned(),
        password: "pass".to_owned(),
    };

    assert!(fixture.engine.fetch_text(&url, &[], None).await.is_err());
    assert!(
        fixture
            .engine
            .fetch_text(&url, &[], Some(login))
            .await
            .is_ok()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_huge_file_is_not_read_as_a_playlist() {
    let fixture = fixture().await;
    let url = fixture.server.url("/file/64M.txt");

    let error = fixture
        .engine
        .fetch_text(&url, &[], None)
        .await
        .unwrap_err();
    assert!(error.contains("too large"), "{error}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_missing_playlist_says_so() {
    let fixture = fixture().await;
    let url = fixture.server.url("/media/nothing.m3u8");

    let error = fixture
        .engine
        .fetch_text(&url, &[], None)
        .await
        .unwrap_err();
    assert!(error.contains("404"), "{error}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_web_page_is_read_with_its_type() {
    let fixture = fixture().await;
    let url = fixture.server.url("/site/about.html");

    let page = fixture.engine.fetch_page(&url, &[], None).await.unwrap();

    assert!(page.found);
    assert!(page.content_type.unwrap().starts_with("text/html"));
    assert!(page.text.unwrap().contains("About this site"));
}

#[tokio::test(flavor = "multi_thread")]
async fn reading_a_page_follows_redirects_and_says_where_it_ended_up() {
    let fixture = fixture().await;
    let url = fixture.server.url("/site/moved.html");

    let page = fixture.engine.fetch_page(&url, &[], None).await.unwrap();

    assert!(page.url.ends_with("/site/about.html"), "{}", page.url);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_large_file_is_not_read_when_looking_for_a_page() {
    let fixture = fixture().await;
    let url = fixture.server.url("/file/200M.zip");

    let started = Instant::now();
    let page = fixture.engine.fetch_page(&url, &[], None).await.unwrap();

    assert!(page.found);
    assert_eq!(page.text, None);
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_missing_page_is_an_answer_rather_than_a_failure() {
    let fixture = fixture().await;
    let url = fixture.server.url("/site/broken.html");

    let page = fixture.engine.fetch_page(&url, &[], None).await.unwrap();

    assert!(!page.found);
    assert_eq!(page.text, None);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_busy_server_is_tried_again_when_reading_a_page() {
    let fixture = fixture().await;
    let url = fixture
        .server
        .url("/file/1K.txt?fail=503&fail_times=2&run=page");

    let page = fixture.engine.fetch_page(&url, &[], None).await.unwrap();

    assert!(page.found);
    assert!(page.text.is_some());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_server_that_stays_down_is_reported_when_reading_a_page() {
    let fixture = fixture().await;
    let url = fixture
        .server
        .url("/file/1K.txt?fail=503&fail_times=99&run=down");

    assert!(fixture.engine.fetch_page(&url, &[], None).await.is_err());
}

/// Everything the database has written, including its journal.
fn database_text(fixture: &Fixture) -> String {
    let mut text = String::new();
    for entry in fs::read_dir(fixture.folder.path()).unwrap() {
        let path = entry.unwrap().path();
        if path.is_file() {
            text.push_str(&String::from_utf8_lossy(&fs::read(path).unwrap()));
        }
    }
    text
}

#[tokio::test(flavor = "multi_thread")]
async fn a_password_in_a_parts_address_is_used_but_never_saved() {
    let fixture = fixture().await;
    let mut parts = fixture.parts("/media/low.m3u8?basic=alice%3Ahunter2").await;
    for part in &mut parts {
        part.url = part.url.replace("http://", "http://alice:hunter2@");
    }

    let saved = fixture.save(parts).await;

    assert_saved_track(&saved, "low");
    let stored = database_text(&fixture);
    assert!(
        !stored.contains("alice:hunter2"),
        "a login reached the database"
    );
    assert!(
        stored.contains("basic=alice"),
        "the parts should be there to search"
    );
}
