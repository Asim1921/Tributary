//! Checks how long things take.
//!
//! Kept apart from the other tests, because Cargo runs test files one at a time. Dozens
//! of downloads running alongside would make these measurements meaningless. For the
//! same reason, everything here is a single test run in sequence.

use std::time::{Duration, Instant};

use engine::{Config, Engine, NewDownload, State};
use testserver::TestServer;

const MIB: u64 = 1 << 20;

/// Downloads a file under a speed limit and returns how many seconds it took.
async fn timed_download(target: &str, limit: u64) -> f64 {
    let server = TestServer::start(testserver::Config::default())
        .await
        .unwrap();
    let folder = tempfile::tempdir().unwrap();
    let config = Config {
        min_split: 64 << 10,
        ..Config::default()
    };
    let engine = Engine::open(&folder.path().join("downloads.db"), config).unwrap();
    engine.set_speed_limit(Some(limit));

    let began = Instant::now();
    let id = engine
        .add(NewDownload {
            url: server.url(target),
            dest_dir: folder.path().join("out"),
            ..NewDownload::default()
        })
        .unwrap();
    let download = engine.wait(id).await.unwrap();

    assert_eq!(download.state, State::Completed, "{:?}", download.error);
    began.elapsed().as_secs_f64()
}

#[tokio::test(flavor = "multi_thread")]
async fn speed_limits_are_accurate_to_within_ten_percent() {
    let cases = [
        ("/file/16M.bin", 8 * MIB, "many connections"),
        ("/file/8M.bin?ranges=0", 4 * MIB, "one connection"),
        ("/file/32M.bin", 16 * MIB, "a higher limit"),
    ];

    for (target, limit, label) in cases {
        let seconds = timed_download(target, limit).await;
        assert!(
            (1.8..=2.2).contains(&seconds),
            "{label}: took {seconds:.2}s, expected about 2s"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn no_limit_is_much_faster_than_a_limit() {
    let server = TestServer::start(testserver::Config::default())
        .await
        .unwrap();
    let folder = tempfile::tempdir().unwrap();
    let engine = Engine::open(&folder.path().join("downloads.db"), Config::default()).unwrap();

    let began = Instant::now();
    let id = engine
        .add(NewDownload {
            url: server.url("/file/16M.bin"),
            dest_dir: folder.path().join("out"),
            ..NewDownload::default()
        })
        .unwrap();
    engine.wait(id).await.unwrap();

    assert!(began.elapsed() < Duration::from_millis(1500));
}
