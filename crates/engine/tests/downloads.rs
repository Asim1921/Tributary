//! Runs whole downloads against the fault-injecting test server.
//!
//! The main check throughout: the finished file's SHA-256 must match the source.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use engine::{
    Config, Download, DownloadEdit, DownloadId, Engine, Error, Event, Moment, NewDownload, Queue,
    QueueId, QueueKind, Quota, RETRY_DELAY_SECONDS, RetryPolicy, State,
};
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use testserver::{Record, TestServer, content};

const MIB: u64 = 1 << 20;

struct Fixture {
    server: TestServer,
    engine: Engine,
    folder: TempDir,
}

fn fast_config() -> Config {
    Config {
        min_split: 64 << 10,
        retry: RetryPolicy {
            attempts: 5,
            base_delay: Duration::from_millis(10),
            max_delay: Duration::from_millis(100),
        },
        checkpoint_interval: Duration::from_millis(100),
        progress_interval: Duration::from_millis(25),
        ..Config::default()
    }
}

async fn fixture_with(config: Config) -> Fixture {
    let folder = tempfile::tempdir().unwrap();
    let database = folder.path().join("downloads.db");
    Fixture {
        server: TestServer::start(testserver::Config::default())
            .await
            .unwrap(),
        engine: Engine::open(&database, config).unwrap(),
        folder,
    }
}

async fn fixture() -> Fixture {
    fixture_with(fast_config()).await
}

impl Fixture {
    fn downloads_dir(&self) -> PathBuf {
        self.folder.path().join("out")
    }

    fn new_download(&self, target: &str) -> NewDownload {
        NewDownload {
            url: self.server.url(target),
            dest_dir: self.downloads_dir(),
            ..NewDownload::default()
        }
    }

    fn start(&self, target: &str) -> DownloadId {
        self.engine.add(self.new_download(target)).unwrap()
    }

    async fn download(&self, target: &str) -> Download {
        let id = self.start(target);
        self.engine.wait(id).await.unwrap()
    }

    /// Waits until at least `bytes` are confirmed on disk.
    async fn wait_for_progress(&self, id: DownloadId, bytes: u64) {
        let deadline = Instant::now() + Duration::from_secs(30);
        while self.engine.get(id).unwrap().downloaded < bytes {
            assert!(Instant::now() < deadline, "timed out waiting for progress");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    fn files(&self) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(self.downloads_dir())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
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
fn assert_completed(download: &Download, seed: u64, size: u64) {
    assert_eq!(
        download.state,
        State::Completed,
        "error: {:?}",
        download.error
    );
    let path = download.final_path.as_ref().unwrap();
    assert_eq!(fs::metadata(path).unwrap().len(), size);
    assert_eq!(
        sha256_of(path),
        content::sha256(seed, size),
        "wrong content"
    );
    assert_eq!(download.downloaded, size);
}

fn range_of(record: &Record) -> Option<&str> {
    record
        .headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("range"))
        .map(|(_, value)| value.as_str())
}

/// The first byte a record asked for, ignoring the one-byte probes.
fn range_start(record: &Record) -> Option<u64> {
    let range = range_of(record)?;
    if range == "bytes=0-0" {
        return None;
    }
    range
        .strip_prefix("bytes=")?
        .split('-')
        .next()?
        .parse()
        .ok()
}

/// Requests made after the `count`th probe.
fn records_after_probe(records: &[Record], count: usize) -> Vec<Record> {
    let mut sorted = records.to_vec();
    sorted.sort_by_key(|record| record.sequence);
    let boundary = sorted
        .iter()
        .filter(|record| range_of(record) == Some("bytes=0-0"))
        .nth(count - 1)
        .expect("the probe should have been recorded")
        .sequence;
    sorted.retain(|record| record.sequence > boundary);
    sorted
}

#[tokio::test(flavor = "multi_thread")]
async fn downloads_a_file_over_several_connections() {
    let fixture = fixture().await;
    let download = fixture.download("/file/8M.bin").await;

    assert_completed(&download, 1, 8 * MIB);
    assert_eq!(download.file_name.as_deref(), Some("8M.bin"));
    assert!(download.ranged);

    let records = fixture.server.records();
    let connections: std::collections::HashSet<_> = records
        .iter()
        .filter(|record| record.status == 206)
        .map(|record| record.connection)
        .collect();
    assert!(
        connections.len() >= 8,
        "used {} connections",
        connections.len()
    );
    assert_eq!(fixture.files(), ["8M.bin"], "no .part file should remain");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_tiny_file_works() {
    let fixture = fixture().await;
    assert_completed(&fixture.download("/file/10.bin").await, 1, 10);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_empty_file_works() {
    let fixture = fixture().await;
    assert_completed(&fixture.download("/file/0.bin").await, 1, 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn falls_back_to_one_connection_without_range_support() {
    let fixture = fixture().await;
    let download = fixture.download("/file/3M.bin?ranges=0").await;

    assert_completed(&download, 1, 3 * MIB);
    assert!(!download.ranged);
}

#[tokio::test(flavor = "multi_thread")]
async fn handles_a_server_that_never_states_the_size() {
    let fixture = fixture().await;
    let download = fixture.download("/file/3M.bin?length=0").await;

    assert_completed(&download, 1, 3 * MIB);
}

#[tokio::test(flavor = "multi_thread")]
async fn survives_many_dropped_connections() {
    let fixture = fixture().await;
    let target = "/file/8M.bin?drop_after=100K&drop_times=20";

    assert_completed(&fixture.download(target).await, 1, 8 * MIB);

    let dropped = fixture
        .server
        .records()
        .iter()
        .filter(|record| record.outcome == testserver::Outcome::Dropped)
        .count();
    assert_eq!(dropped, 20);
}

#[tokio::test(flavor = "multi_thread")]
async fn restarts_a_failed_single_connection_download() {
    let fixture = fixture().await;
    let target = "/file/3M.bin?ranges=0&drop_after=1M&drop_times=2";

    assert_completed(&fixture.download(target).await, 1, 3 * MIB);
}

#[tokio::test(flavor = "multi_thread")]
async fn gives_up_on_a_single_connection_server_that_keeps_cutting_off() {
    let fixture = fixture().await;
    let target = "/file/3M.bin?ranges=0&drop_after=1M&drop_times=100";

    let download = fixture.download(target).await;

    // Nothing survives a failed attempt here, so bytes fetched during one aren't
    // progress. Probe, first try and 5 retries.
    let requests = fixture.server.records().len();
    let final_size = download
        .final_path
        .as_ref()
        .map(|path| fs::metadata(path).unwrap().len());
    assert_eq!(
        (download.state, requests),
        (State::Failed, 7),
        "final file size: {final_size:?}"
    );
    assert_eq!(fixture.files(), ["3M.bin.1.part"], "no final file");
}

#[tokio::test(flavor = "multi_thread")]
async fn retries_temporary_server_errors() {
    let fixture = fixture().await;
    let target = "/file/2M.bin?fail=503&fail_times=3";

    assert_completed(&fixture.download(target).await, 1, 2 * MIB);
}

#[tokio::test(flavor = "multi_thread")]
async fn waits_as_long_as_retry_after_says() {
    let fixture = fixture().await;
    let began = Instant::now();

    let download = fixture
        .download("/file/1M.bin?fail=503&fail_times=1&retry_after=1")
        .await;

    assert_completed(&download, 1, MIB);
    assert!(began.elapsed() >= Duration::from_secs(1));
}

#[tokio::test(flavor = "multi_thread")]
async fn gives_up_after_too_many_temporary_failures() {
    let fixture = fixture().await;
    let download = fixture
        .download("/file/1M.bin?fail=503&fail_times=1000")
        .await;

    assert_eq!(download.state, State::Failed);
    assert!(download.error.unwrap().contains("503"));
    assert_eq!(
        fixture.server.records().len(),
        6,
        "first try plus 5 retries"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn does_not_retry_a_missing_file() {
    let fixture = fixture().await;
    let download = fixture.download("/missing").await;

    assert_eq!(download.state, State::Failed);
    assert!(download.error.unwrap().contains("404"));
    assert_eq!(fixture.server.records().len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn follows_redirects() {
    let fixture = fixture().await;
    let download = fixture.download("/file/2M.bin?redirects=3").await;

    assert_completed(&download, 1, 2 * MIB);
}

#[tokio::test(flavor = "multi_thread")]
async fn adapts_to_a_server_that_limits_connections() {
    let fixture = fixture().await;
    let download = fixture.download("/file/8M.bin?limit=2&rate=8M").await;

    assert_completed(&download, 1, 8 * MIB);

    let refused = fixture
        .server
        .records()
        .iter()
        .filter(|record| record.status == 503)
        .count();
    assert!(refused > 0, "the limit should have been hit");
}

#[tokio::test(flavor = "multi_thread")]
async fn restarts_cleanly_when_the_file_changes_mid_download() {
    let fixture = fixture().await;
    let download = fixture.download("/file/4M.bin?change_after=3").await;

    // Seed 2 is the replaced file. A mix of the two would fail this check.
    assert_completed(&download, 2, 4 * MIB);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_finished_connection_takes_over_part_of_a_slow_one() {
    let fixture = fixture_with(Config {
        connections: 2,
        ..fast_config()
    })
    .await;

    let download = fixture.download("/file/8M.bin?rate=4M&slow_from=4M").await;
    assert_completed(&download, 1, 8 * MIB);

    let extra_starts = fixture
        .server
        .records()
        .iter()
        .filter_map(range_start)
        .filter(|&start| start > 4 * MIB)
        .count();
    assert!(extra_starts > 0, "nothing was split off the slow half");
}

#[tokio::test(flavor = "multi_thread")]
async fn pause_keeps_progress_and_resume_fetches_only_the_rest() {
    let fixture = fixture().await;
    let size = 32 * MIB;
    let id = fixture.start("/file/32M.bin?rate=2M");

    fixture.wait_for_progress(id, 8 * MIB).await;
    fixture.engine.pause(id).await.unwrap();

    let paused = fixture.engine.get(id).unwrap();
    assert_eq!(paused.state, State::Paused);
    assert!(paused.downloaded >= 8 * MIB);
    assert_eq!(fixture.files(), ["32M.bin.1.part"]);

    fixture.engine.resume(id, &[]).unwrap();
    let finished = fixture.engine.wait(id).await.unwrap();
    assert_completed(&finished, 1, size);

    let resumed = records_after_probe(&fixture.server.records(), 2);
    let sent: u64 = resumed.iter().map(|record| record.body_bytes_sent).sum();
    let limit = size - paused.downloaded / 2;
    assert!(sent < limit, "fetched {sent} bytes again, limit {limit}");
}

#[tokio::test(flavor = "multi_thread")]
async fn pausing_something_that_is_not_running_is_harmless() {
    let fixture = fixture().await;
    let download = fixture.download("/file/1K.bin").await;

    fixture.engine.pause(download.id).await.unwrap();

    assert_completed(&fixture.engine.get(download.id).unwrap(), 1, 1024);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_completed_download_cannot_be_resumed() {
    let fixture = fixture().await;
    let download = fixture.download("/file/1K.bin").await;

    assert!(matches!(
        fixture.engine.resume(download.id, &[]),
        Err(Error::AlreadyCompleted(_))
    ));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_running_download_cannot_be_started_twice() {
    let fixture = fixture().await;
    let id = fixture.start("/file/32M.bin?rate=1M");

    assert!(matches!(
        fixture.engine.resume(id, &[]),
        Err(Error::AlreadyRunning(_))
    ));
    fixture.engine.pause(id).await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_download_can_be_resumed() {
    let fixture = fixture().await;
    // Enough failures to exhaust the first run's retries but not a second run's.
    let download = fixture.download("/file/1M.bin?fail=503&fail_times=7").await;
    assert_eq!(download.state, State::Failed);

    fixture.engine.resume(download.id, &[]).unwrap();

    let finished = fixture.engine.wait(download.id).await.unwrap();
    assert_completed(&finished, 1, MIB);
    assert_eq!(finished.error, None);
}

#[tokio::test(flavor = "multi_thread")]
async fn resuming_after_the_file_changed_starts_again() {
    let fixture = fixture().await;
    // The probe is visit 1, so the file changes shortly after this starts.
    let id = fixture.start("/file/32M.bin?rate=2M&change_after=9");

    fixture.wait_for_progress(id, MIB).await;
    fixture.engine.pause(id).await.unwrap();
    fixture.engine.resume(id, &[]).unwrap();

    let finished = fixture.engine.wait(id).await.unwrap();
    assert_completed(&finished, 2, 32 * MIB);
}

#[tokio::test(flavor = "multi_thread")]
async fn removing_a_download_deletes_its_part_file_only() {
    let fixture = fixture().await;
    let id = fixture.start("/file/32M.bin?rate=1M");
    fixture.wait_for_progress(id, MIB).await;

    fixture.engine.remove(id, false).await.unwrap();

    assert!(fixture.files().is_empty());
    assert!(matches!(fixture.engine.get(id), Err(Error::NotFound(_))));

    let kept = fixture.download("/file/1K.bin").await;
    fixture.engine.remove(kept.id, false).await.unwrap();
    assert_eq!(fixture.files(), ["1K.bin"], "completed files stay");
}

#[tokio::test(flavor = "multi_thread")]
async fn removing_can_also_delete_the_finished_file() {
    let fixture = fixture().await;
    let download = fixture.download("/file/1K.bin").await;

    fixture.engine.remove(download.id, true).await.unwrap();

    assert!(fixture.files().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_download_can_be_added_without_starting() {
    let fixture = fixture().await;
    let id = fixture
        .engine
        .add(NewDownload {
            paused: true,
            description: Some("for later".to_owned()),
            ..fixture.new_download("/file/1M.bin")
        })
        .unwrap();

    let saved = fixture.engine.get(id).unwrap();
    assert_eq!(saved.state, State::Paused);
    assert_eq!(saved.description.as_deref(), Some("for later"));
    assert!(saved.added_at > 0);
    assert!(
        fixture.server.records().is_empty(),
        "nothing should be sent"
    );

    fixture.engine.resume(id, &[]).unwrap();
    let finished = fixture.engine.wait(id).await.unwrap();
    assert_completed(&finished, 1, MIB);
    assert!(finished.completed_at.unwrap() >= saved.added_at);
}

#[tokio::test(flavor = "multi_thread")]
async fn inspecting_reports_the_file_without_downloading_it() {
    let fixture = fixture().await;

    let info = fixture
        .engine
        .inspect(&fixture.server.url("/file/3M.bin"), &[])
        .await
        .unwrap();
    assert_eq!(info.file_name, "3M.bin");
    assert_eq!(info.size, Some(3 * MIB));
    assert!(info.resumable);

    let single = fixture
        .engine
        .inspect(&fixture.server.url("/file/3M.bin?ranges=0"), &[])
        .await
        .unwrap();
    assert!(!single.resumable);

    let missing = fixture
        .engine
        .inspect(&fixture.server.url("/missing"), &[])
        .await;
    assert!(matches!(missing, Err(Error::Request(reason)) if reason.contains("404")));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_new_link_for_the_same_file_continues_the_download() {
    let fixture = fixture().await;
    let size = 32 * MIB;
    let id = fixture.start("/file/32M.bin?rate=2M&run=old-link");

    fixture.wait_for_progress(id, 8 * MIB).await;
    fixture.engine.pause(id).await.unwrap();
    let paused = fixture.engine.get(id).unwrap();

    fixture
        .engine
        .edit(
            id,
            DownloadEdit {
                url: Some(fixture.server.url("/file/32M.bin?run=new-link")),
                ..DownloadEdit::default()
            },
        )
        .unwrap();
    fixture.engine.resume(id, &[]).unwrap();

    let finished = fixture.engine.wait(id).await.unwrap();
    assert_completed(&finished, 1, size);

    let records = fixture.server.records();
    let new_link_bytes: u64 = records
        .iter()
        .filter(|record| record.target.contains("new-link"))
        .map(|record| record.body_bytes_sent)
        .sum();
    assert!(
        new_link_bytes < size - paused.downloaded / 2,
        "fetched {new_link_bytes} bytes from the new link"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_new_link_to_a_different_file_starts_again() {
    let fixture = fixture().await;
    let id = fixture.start("/file/32M.bin?rate=2M");

    fixture.wait_for_progress(id, MIB).await;
    fixture.engine.pause(id).await.unwrap();

    fixture
        .engine
        .edit(
            id,
            DownloadEdit {
                url: Some(fixture.server.url("/file/32M.bin?seed=5")),
                ..DownloadEdit::default()
            },
        )
        .unwrap();
    fixture.engine.resume(id, &[]).unwrap();

    assert_completed(&fixture.engine.wait(id).await.unwrap(), 5, 32 * MIB);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_running_download_cannot_be_edited() {
    let fixture = fixture().await;
    let id = fixture.start("/file/32M.bin?rate=1M");

    let result = fixture.engine.edit(id, DownloadEdit::default());

    assert!(matches!(result, Err(Error::AlreadyRunning(_))));
    fixture.engine.pause(id).await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn changing_the_folder_mid_download_keeps_progress() {
    let fixture = fixture().await;
    let id = fixture.start("/file/32M.bin?rate=2M");

    fixture.wait_for_progress(id, 4 * MIB).await;
    fixture.engine.pause(id).await.unwrap();
    let paused = fixture.engine.get(id).unwrap();

    let new_folder = fixture.folder.path().join("moved");
    fixture
        .engine
        .edit(
            id,
            DownloadEdit {
                dest_dir: Some(new_folder.clone()),
                ..DownloadEdit::default()
            },
        )
        .unwrap();
    fixture.engine.resume(id, &[]).unwrap();

    let finished = fixture.engine.wait(id).await.unwrap();
    assert_completed(&finished, 1, 32 * MIB);
    assert_eq!(finished.final_path.unwrap(), new_folder.join("32M.bin"));

    let resumed = records_after_probe(&fixture.server.records(), 2);
    let sent: u64 = resumed.iter().map(|record| record.body_bytes_sent).sum();
    assert!(
        sent < 32 * MIB - paused.downloaded / 2,
        "fetched {sent} bytes"
    );
    assert!(
        fixture.files().is_empty(),
        "the .part file should have moved"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_temporary_folder_on_another_drive_works() {
    // The system temp folder is on C: and the build folder is on D: on this machine,
    // so the final move has to copy between drives.
    let temp = tempfile::tempdir().unwrap();
    let fixture = fixture_with(Config {
        temp_dir: Some(temp.path().to_owned()),
        ..fast_config()
    })
    .await;
    let dest = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();

    let id = fixture
        .engine
        .add(NewDownload {
            dest_dir: dest.path().to_owned(),
            ..fixture.new_download("/file/4M.bin")
        })
        .unwrap();
    let finished = fixture.engine.wait(id).await.unwrap();

    assert_completed(&finished, 1, 4 * MIB);
    assert_eq!(finished.final_path.unwrap(), dest.path().join("4M.bin"));
    assert_eq!(
        fs::read_dir(temp.path()).unwrap().count(),
        0,
        "nothing left behind"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_new_connection_count_applies_to_the_next_download() {
    let fixture = fixture().await;
    fixture.engine.set_connections(2);

    let download = fixture.download("/file/8M.bin").await;

    assert_completed(&download, 1, 8 * MIB);
    let first_wave = fixture
        .server
        .records()
        .iter()
        .filter_map(range_start)
        .filter(|&start| start == 0 || start == 4 * MIB)
        .count();
    assert_eq!(first_wave, 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn never_overwrites_an_existing_file() {
    let fixture = fixture().await;
    fs::create_dir_all(fixture.downloads_dir()).unwrap();
    let existing = fixture.downloads_dir().join("1K.bin");
    fs::write(&existing, b"precious").unwrap();

    let download = fixture.download("/file/1K.bin").await;

    assert_completed(&download, 1, 1024);
    assert_eq!(
        download.final_path.unwrap().file_name().unwrap(),
        "1K (1).bin"
    );
    assert_eq!(fs::read(existing).unwrap(), b"precious");
}

#[tokio::test(flavor = "multi_thread")]
async fn uses_the_name_the_server_gives() {
    let fixture = fixture().await;
    let download = fixture.download("/file/1K?filename=my%20report.zip").await;

    assert_completed(&download, 1, 1024);
    assert_eq!(fixture.files(), ["my report.zip"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_hostile_server_name_cannot_leave_the_download_folder() {
    let fixture = fixture().await;
    let download = fixture
        .download("/file/1K?filename=..%2F..%2F..%2Fevil.exe")
        .await;

    assert_completed(&download, 1, 1024);
    assert_eq!(fixture.files(), ["evil.exe"]);
    assert!(!fixture.folder.path().join("evil.exe").exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn sends_cookies_but_never_saves_them() {
    let fixture = fixture().await;
    // The value is escaped here so that the saved URL doesn't contain it either.
    let target = "/file/1M.bin?require_cookie=session%3D%73ecret-value-123";

    let id = fixture
        .engine
        .add(NewDownload {
            headers: vec![
                ("Cookie".to_owned(), "session=secret-value-123".to_owned()),
                ("Referer".to_owned(), "http://site.test/page".to_owned()),
            ],
            ..fixture.new_download(target)
        })
        .unwrap();
    let download = fixture.engine.wait(id).await.unwrap();

    assert_completed(&download, 1, MIB);
    assert_eq!(
        download.headers,
        [("Referer".to_owned(), "http://site.test/page".to_owned())]
    );

    let mut saved = Vec::new();
    for entry in fs::read_dir(fixture.folder.path()).unwrap() {
        let path = entry.unwrap().path();
        if path.is_file() {
            saved.extend(fs::read(path).unwrap());
        }
    }
    let needle = b"secret-value-123";
    assert!(!saved.is_empty());
    assert!(
        !saved.windows(needle.len()).any(|window| window == needle),
        "the cookie reached the database"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn cookies_survive_a_pause_but_not_a_restart() {
    let fixture = fixture().await;
    let cookie = ("Cookie".to_owned(), "session=abc".to_owned());
    let target = "/file/32M.bin?rate=2M&require_cookie=session%3Dabc";

    let id = fixture
        .engine
        .add(NewDownload {
            headers: vec![cookie.clone()],
            ..fixture.new_download(target)
        })
        .unwrap();
    fixture.wait_for_progress(id, MIB).await;
    fixture.engine.pause(id).await.unwrap();

    // Within a session, resuming needs nothing extra.
    fixture.engine.resume(id, &[]).unwrap();
    fixture.wait_for_progress(id, 2 * MIB).await;
    fixture.engine.pause(id).await.unwrap();

    // After a restart the cookie is gone, because it was never saved.
    let reopened =
        Engine::open(&fixture.folder.path().join("downloads.db"), fast_config()).unwrap();
    reopened.resume(id, &[]).unwrap();
    let refused = reopened.wait(id).await.unwrap();
    assert_eq!(refused.state, State::Failed);
    assert!(refused.error.unwrap().contains("403"));

    reopened.resume(id, &[cookie]).unwrap();
    assert_completed(&reopened.wait(id).await.unwrap(), 1, 32 * MIB);
}

#[tokio::test(flavor = "multi_thread")]
async fn removing_the_speed_limit_takes_effect_on_a_running_download() {
    let fixture = fixture().await;
    fixture.engine.set_speed_limit(Some(MIB));
    let id = fixture.start("/file/32M.bin");
    fixture.wait_for_progress(id, MIB).await;

    fixture.engine.set_speed_limit(None);

    let began = Instant::now();
    assert_completed(&fixture.engine.wait(id).await.unwrap(), 1, 32 * MIB);
    assert!(began.elapsed() < Duration::from_secs(10), "still limited");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_quota_pauses_and_resumes_on_its_own() {
    let fixture = fixture().await;
    fixture.engine.set_quota(Some(Quota {
        bytes: MIB,
        period: Duration::from_secs(1),
    }));
    let began = Instant::now();

    let download = fixture.download("/file/3M.bin").await;

    assert_completed(&download, 1, 3 * MIB);
    assert!(
        began.elapsed() >= Duration::from_millis(1900),
        "3 MiB at 1 MiB per second took only {:?}",
        began.elapsed()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn stopping_during_a_speed_limit_wait_is_prompt() {
    let fixture = fixture().await;
    fixture.engine.set_speed_limit(Some(100 << 10));
    let id = fixture.start("/file/32M.bin");
    tokio::time::sleep(Duration::from_millis(300)).await;

    let began = Instant::now();
    fixture.engine.pause(id).await.unwrap();

    assert!(began.elapsed() < Duration::from_secs(2));
    assert_eq!(fixture.engine.get(id).unwrap().state, State::Paused);
}

/// Adds a file to a queue without starting it.
fn queued(fixture: &Fixture, target: &str, queue: QueueId) -> DownloadId {
    fixture
        .engine
        .add(NewDownload {
            paused: true,
            queue: Some(queue),
            ..fixture.new_download(target)
        })
        .unwrap()
}

fn queue_is_active(fixture: &Fixture, queue: QueueId) -> bool {
    fixture
        .engine
        .queues()
        .unwrap()
        .iter()
        .any(|info| info.queue.id == queue && info.active)
}

/// Steps the scheduler until the queue ends, `step_seconds` of pretend time per tick.
async fn run_queue(fixture: &Fixture, queue: QueueId, step_seconds: i64) {
    let mut now = Moment::now();
    for _ in 0..3000 {
        fixture.engine.tick(now).await.unwrap();
        if !queue_is_active(fixture, queue) {
            return;
        }
        now.seconds += step_seconds;
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the queue never ended");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_new_database_has_a_main_queue() {
    let fixture = fixture().await;
    let queues = fixture.engine.queues().unwrap();

    assert_eq!(queues.len(), 1);
    assert_eq!(queues[0].queue.id, QueueId::MAIN);
    assert_eq!(queues[0].queue.name, "Main queue");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_queue_downloads_everything_then_reports_finished() {
    let fixture = fixture().await;
    let mut events = fixture.engine.subscribe();
    let ids = [
        queued(&fixture, "/file/1M.bin?seed=1", QueueId::MAIN),
        queued(&fixture, "/file/1M.bin?seed=2", QueueId::MAIN),
        queued(&fixture, "/file/1M.bin?seed=3", QueueId::MAIN),
    ];

    fixture.engine.start_queue(QueueId::MAIN).unwrap();
    run_queue(&fixture, QueueId::MAIN, 1).await;

    for (index, id) in ids.iter().enumerate() {
        assert_completed(&fixture.engine.get(*id).unwrap(), index as u64 + 1, MIB);
    }
    assert!(
        fixture.engine.queues().unwrap()[0].files.is_empty(),
        "finished files should leave the queue"
    );

    let mut finished = 0;
    while let Ok(event) = events.try_recv() {
        if event == (Event::QueueFinished { id: QueueId::MAIN }) {
            finished += 1;
        }
    }
    assert_eq!(finished, 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_queue_keeps_to_its_concurrency_and_order() {
    let fixture = fixture().await;
    let mut queue = Queue::new("one at a time");
    queue.concurrency = 1;
    let queue = fixture.engine.save_queue(&queue).unwrap();

    let first = queued(&fixture, "/file/2M.bin?seed=1&rate=4M", queue);
    let second = queued(&fixture, "/file/2M.bin?seed=2&rate=4M", queue);
    let third = queued(&fixture, "/file/2M.bin?seed=3&rate=4M", queue);
    fixture
        .engine
        .reorder_queue(&[third, first, second])
        .unwrap();

    fixture.engine.start_queue(queue).unwrap();
    let mut now = Moment::now();
    let mut most_running = 0;
    while queue_is_active(&fixture, queue) {
        fixture.engine.tick(now).await.unwrap();
        let running = fixture
            .engine
            .list()
            .unwrap()
            .iter()
            .filter(|download| download.state == State::Running)
            .count();
        most_running = most_running.max(running);
        now.seconds += 1;
        tokio::time::sleep(Duration::from_millis(5)).await;
    }

    assert_eq!(most_running, 1);
    let finish_order: Vec<i64> = [first, second, third]
        .iter()
        .map(|id| fixture.engine.get(*id).unwrap().id.0)
        .collect();
    assert_eq!(finish_order.len(), 3);

    let mut records = fixture.server.records();
    records.sort_by_key(|record| record.sequence);
    let first_seen = records
        .iter()
        .find(|record| record.target.contains("seed="))
        .unwrap();
    assert!(
        first_seen.target.contains("seed=3"),
        "the reordered file goes first"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_file_is_retried_and_the_queue_carries_on() {
    let fixture = fixture().await;
    let flaky = queued(
        &fixture,
        "/file/1M.bin?seed=1&fail=404&fail_times=1",
        QueueId::MAIN,
    );
    let fine = queued(&fixture, "/file/1M.bin?seed=2", QueueId::MAIN);

    fixture.engine.start_queue(QueueId::MAIN).unwrap();
    run_queue(&fixture, QueueId::MAIN, RETRY_DELAY_SECONDS + 1).await;

    assert_completed(&fixture.engine.get(flaky).unwrap(), 1, MIB);
    assert_completed(&fixture.engine.get(fine).unwrap(), 2, MIB);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_queue_gives_up_on_a_file_that_keeps_failing() {
    let fixture = fixture().await;
    let mut queue = Queue::new("short");
    queue.retries = 1;
    let queue = fixture.engine.save_queue(&queue).unwrap();

    let broken = queued(&fixture, "/missing/a", queue);
    let fine = queued(&fixture, "/file/1M.bin", queue);

    fixture.engine.start_queue(queue).unwrap();
    run_queue(&fixture, queue, RETRY_DELAY_SECONDS + 1).await;

    assert_eq!(fixture.engine.get(broken).unwrap().state, State::Failed);
    assert_completed(&fixture.engine.get(fine).unwrap(), 1, MIB);
}

#[tokio::test(flavor = "multi_thread")]
async fn stopping_a_queue_stops_its_files_without_reporting_finished() {
    let fixture = fixture().await;
    let mut events = fixture.engine.subscribe();
    let id = queued(&fixture, "/file/32M.bin?rate=1M", QueueId::MAIN);

    fixture.engine.start_queue(QueueId::MAIN).unwrap();
    fixture.engine.tick(Moment::now()).await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;

    fixture.engine.stop_queue(QueueId::MAIN).await.unwrap();

    assert_eq!(fixture.engine.get(id).unwrap().state, State::Paused);
    assert!(!queue_is_active(&fixture, QueueId::MAIN));
    while let Ok(event) = events.try_recv() {
        assert!(!matches!(event, Event::QueueFinished { .. }));
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_file_stopped_by_hand_stays_stopped_while_its_queue_runs() {
    let fixture = fixture().await;
    let stopped = queued(&fixture, "/file/32M.bin?rate=1M&seed=1", QueueId::MAIN);
    let other = queued(&fixture, "/file/1M.bin?seed=2", QueueId::MAIN);

    fixture.engine.start_queue(QueueId::MAIN).unwrap();
    fixture.engine.tick(Moment::now()).await.unwrap();
    fixture.engine.pause(stopped).await.unwrap();

    run_queue(&fixture, QueueId::MAIN, 1).await;

    assert_eq!(fixture.engine.get(stopped).unwrap().state, State::Paused);
    assert_completed(&fixture.engine.get(other).unwrap(), 2, MIB);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_scheduled_queue_starts_at_its_time() {
    let fixture = fixture().await;
    let mut queue = Queue::new("night");
    queue.start_minute = Some(120);
    let queue = fixture.engine.save_queue(&queue).unwrap();
    let id = queued(&fixture, "/file/1M.bin", queue);

    let moment = |minute: u16| Moment {
        seconds: i64::from(minute) * 60,
        day: 1000,
        weekday: 0,
        minute,
    };

    fixture.engine.tick(moment(119)).await.unwrap();
    assert_eq!(fixture.engine.get(id).unwrap().state, State::Paused);

    fixture.engine.tick(moment(120)).await.unwrap();
    assert_completed(&fixture.engine.wait(id).await.unwrap(), 1, MIB);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_sync_queue_refetches_a_changed_file_in_place() {
    let fixture = fixture().await;
    fixture.engine.set_connections(1);
    let mut queue = Queue::new("sync");
    queue.kind = QueueKind::Sync;
    queue.sync_minutes = 1;
    let queue = fixture.engine.save_queue(&queue).unwrap();

    // The probe and the download are visits 1 and 2, so the next check sees a new file.
    let id = queued(&fixture, "/file/512K.bin?change_after=2", queue);
    fixture.engine.start_queue(queue).unwrap();

    let mut now = Moment::now();
    for _ in 0..600 {
        fixture.engine.tick(now).await.unwrap();
        let download = fixture.engine.get(id).unwrap();
        let changed = download.final_path.as_ref().is_some_and(|path| {
            download.state == State::Completed && sha256_of(path) == content::sha256(2, 512 << 10)
        });
        if changed {
            break;
        }
        now.seconds += 61;
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    assert_completed(&fixture.engine.get(id).unwrap(), 2, 512 << 10);
    assert_eq!(
        fixture.files(),
        ["512K.bin"],
        "the file is replaced, not duplicated"
    );
    assert_eq!(
        fixture.engine.queues().unwrap()[1].files,
        [id],
        "sync files stay queued"
    );
    fixture.engine.stop_queue(queue).await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_sync_queue_leaves_an_unchanged_file_alone() {
    let fixture = fixture().await;
    fixture.engine.set_connections(1);
    let mut queue = Queue::new("sync");
    queue.kind = QueueKind::Sync;
    queue.sync_minutes = 1;
    let queue = fixture.engine.save_queue(&queue).unwrap();

    let id = queued(&fixture, "/file/512K.bin", queue);
    fixture.engine.start_queue(queue).unwrap();

    let mut now = Moment::now();
    for _ in 0..30 {
        fixture.engine.tick(now).await.unwrap();
        now.seconds += 61;
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    assert_completed(&fixture.engine.get(id).unwrap(), 1, 512 << 10);
    let full_fetches = fixture
        .server
        .records()
        .iter()
        .filter(|record| record.body_bytes_sent >= 512 << 10)
        .count();
    assert_eq!(full_fetches, 1, "it should only be downloaded once");
    fixture.engine.stop_queue(queue).await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn deleting_a_queue_keeps_its_files() {
    let fixture = fixture().await;
    let queue = fixture.engine.save_queue(&Queue::new("temporary")).unwrap();
    let id = queued(&fixture, "/file/1M.bin", queue);

    fixture.engine.delete_queue(queue).await.unwrap();

    assert_eq!(fixture.engine.queues().unwrap().len(), 1);
    assert_eq!(fixture.engine.get(id).unwrap().queue, None);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_main_queue_cannot_be_deleted() {
    let fixture = fixture().await;

    let result = fixture.engine.delete_queue(QueueId::MAIN).await;

    assert!(matches!(result, Err(Error::MainQueue)));
}

#[tokio::test(flavor = "multi_thread")]
async fn queues_and_categories_survive_a_restart() {
    let fixture = fixture().await;
    let mut queue = Queue::new("saved");
    queue.start_minute = Some(90);
    queue.stop_minute = Some(600);
    queue.kind = QueueKind::Sync;
    queue.concurrency = 3;
    queue.days = 0b001_0101;
    queue.after_action = "{\"shutdown\":true}".to_owned();
    let saved_id = fixture.engine.save_queue(&queue).unwrap();

    let id = fixture
        .engine
        .add(NewDownload {
            paused: true,
            queue: Some(saved_id),
            category: Some("Music".to_owned()),
            ..fixture.new_download("/file/1M.bin")
        })
        .unwrap();

    let reopened =
        Engine::open(&fixture.folder.path().join("downloads.db"), fast_config()).unwrap();
    let info = reopened
        .queues()
        .unwrap()
        .into_iter()
        .find(|info| info.queue.id == saved_id)
        .unwrap();

    queue.id = saved_id;
    assert_eq!(info.queue, queue);
    assert_eq!(info.files, [id]);
    assert_eq!(reopened.get(id).unwrap().category.as_deref(), Some("Music"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_download_can_move_between_queues_and_leave_them() {
    let fixture = fixture().await;
    let other = fixture.engine.save_queue(&Queue::new("other")).unwrap();
    let id = queued(&fixture, "/file/1M.bin", QueueId::MAIN);

    fixture.engine.set_queue(id, Some(other)).unwrap();
    assert_eq!(fixture.engine.get(id).unwrap().queue, Some(other));

    fixture.engine.set_queue(id, None).unwrap();
    assert_eq!(fixture.engine.get(id).unwrap().queue, None);
}

#[tokio::test(flavor = "multi_thread")]
async fn inspecting_a_web_page_says_so() {
    let fixture = fixture().await;

    let page = fixture
        .engine
        .inspect(&fixture.server.url("/"), &[])
        .await
        .unwrap();
    assert!(page.is_web_page);

    let file = fixture
        .engine
        .inspect(&fixture.server.url("/file/1K.zip"), &[])
        .await
        .unwrap();
    assert!(!file.is_web_page);
}

#[tokio::test(flavor = "multi_thread")]
async fn reports_progress_and_state_changes() {
    let fixture = fixture().await;
    let mut events = fixture.engine.subscribe();

    let download = fixture.download("/file/8M.bin?rate=8M").await;
    assert_completed(&download, 1, 8 * MIB);

    let mut seen_progress = false;
    let mut states = Vec::new();
    while let Ok(event) = events.try_recv() {
        match event {
            Event::Progress {
                total, segments, ..
            } => {
                assert_eq!(total, Some(8 * MIB));
                assert_eq!(
                    segments
                        .iter()
                        .map(|part| part.end - part.start)
                        .sum::<u64>(),
                    8 * MIB
                );
                seen_progress = true;
            }
            Event::State { state, .. } => states.push(state),
            Event::Queues | Event::QueueFinished { .. } => {}
        }
    }
    assert!(seen_progress);
    assert_eq!(states, [State::Running, State::Completed]);
}

#[tokio::test(flavor = "multi_thread")]
async fn rejects_urls_it_cannot_download() {
    let fixture = fixture().await;
    for url in ["not a url", "ftp://x.test/a", "file:///C:/Windows/win.ini"] {
        let result = fixture.engine.add(NewDownload {
            url: url.to_owned(),
            ..NewDownload::default()
        });
        assert!(matches!(result, Err(Error::InvalidUrl(_))), "{url}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn rejects_invalid_headers() {
    let fixture = fixture().await;
    let result = fixture.engine.add(NewDownload {
        headers: vec![("Referer".to_owned(), "a\r\nInjected: yes".to_owned())],
        ..fixture.new_download("/file/1K.bin")
    });
    assert!(matches!(result, Err(Error::InvalidHeader(_))));
}

#[tokio::test(flavor = "multi_thread")]
async fn survives_the_process_being_killed_mid_download() {
    let server = TestServer::start(testserver::Config::default())
        .await
        .unwrap();
    let folder = tempfile::tempdir().unwrap();
    let database = folder.path().join("downloads.db");
    let out = folder.path().join("out");
    let size = 48 * MIB;

    let mut child = Command::new(env!("CARGO_BIN_EXE_tributary-fetch"))
        .arg("get")
        .arg(server.url("/file/48M.bin?rate=2M"))
        .arg(&out)
        .arg(&database)
        .stdout(Stdio::null())
        .spawn()
        .unwrap();

    // Long enough for at least one checkpoint, too short to finish.
    tokio::time::sleep(Duration::from_millis(1800)).await;
    child.kill().unwrap();
    child.wait().unwrap();

    let engine = Engine::open(&database, fast_config()).unwrap();
    let id = DownloadId(1);

    let interrupted = engine.get(id).unwrap();
    assert_eq!(interrupted.state, State::Paused, "should recover as paused");
    assert!(interrupted.downloaded > 0, "no checkpoint was saved");
    assert!(interrupted.downloaded < size, "it finished before the kill");

    engine.resume(id, &[]).unwrap();
    let finished = engine.wait(id).await.unwrap();
    assert_completed(&finished, 1, size);

    let resumed = records_after_probe(&server.records(), 2);
    let sent: u64 = resumed.iter().map(|record| record.body_bytes_sent).sum();
    assert!(
        sent < size - interrupted.downloaded / 2,
        "fetched {sent} bytes after the kill"
    );
}
