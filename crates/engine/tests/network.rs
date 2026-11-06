//! FTP, logins, proxies and other network options, against real local servers.

use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use engine::{
    Config, Download, DownloadId, Engine, Error, LOGIN_NEEDED, Login, ManualProxy, NetworkOptions,
    NewDownload, ProxyMode, RetryPolicy, ServerLimit, State,
};
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use testserver::{FtpConfig, FtpServer, SocksProxy, TestServer, content};

const MIB: u64 = 1 << 20;

fn config() -> Config {
    Config {
        min_split: 64 << 10,
        retry: RetryPolicy {
            attempts: 4,
            base_delay: Duration::from_millis(10),
            max_delay: Duration::from_millis(50),
        },
        checkpoint_interval: Duration::from_millis(100),
        ..Config::default()
    }
}

struct Setup {
    engine: Engine,
    folder: TempDir,
}

fn setup() -> Setup {
    let folder = tempfile::tempdir().unwrap();
    let engine = Engine::open(&folder.path().join("downloads.db"), config()).unwrap();
    Setup { engine, folder }
}

async fn http() -> TestServer {
    TestServer::start(testserver::Config::default())
        .await
        .unwrap()
}

impl Setup {
    fn out(&self) -> std::path::PathBuf {
        self.folder.path().join("out")
    }

    fn new_download(&self, url: String) -> NewDownload {
        NewDownload {
            url,
            dest_dir: self.out(),
            ..NewDownload::default()
        }
    }

    async fn download(&self, url: String) -> Download {
        let id = self.engine.add(self.new_download(url)).unwrap();
        self.engine.wait(id).await.unwrap()
    }

    async fn download_with_login(&self, url: String, login: Login) -> Download {
        let id = self
            .engine
            .add(NewDownload {
                login: Some(login),
                ..self.new_download(url)
            })
            .unwrap();
        self.engine.wait(id).await.unwrap()
    }

    /// Everything the program saved, for checking that a secret isn't in it.
    fn saved_bytes(&self) -> Vec<u8> {
        let mut saved = Vec::new();
        for entry in fs::read_dir(self.folder.path()).unwrap() {
            let path = entry.unwrap().path();
            if path.is_file() {
                saved.extend(fs::read(path).unwrap());
            }
        }
        saved
    }
}

fn login(username: &str, password: &str) -> Login {
    Login {
        username: username.to_owned(),
        password: password.to_owned(),
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
fn assert_completed(download: &Download, size: u64) {
    assert_eq!(
        download.state,
        State::Completed,
        "error: {:?}",
        download.error
    );
    let path = download.final_path.as_ref().unwrap();
    assert_eq!(sha256_of(path), content::sha256(1, size), "wrong content");
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

// Connections

#[tokio::test(flavor = "multi_thread")]
async fn the_probe_connection_is_reused_for_the_download() {
    let server = http().await;
    let setup = setup();
    setup.engine.set_connections(1);

    assert_completed(
        &setup.download(server.url("/file/512K.bin")).await,
        512 << 10,
    );

    let records = server.records();
    assert_eq!(records.len(), 2);
    assert_eq!(
        records[0].connection, records[1].connection,
        "same connection"
    );
    assert_eq!(records[1].request_on_connection, 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_finished_part_lets_its_connection_take_the_next_one() {
    let server = http().await;
    let setup = setup();
    setup.engine.set_connections(2);

    // The first half finishes at once and then helps with the slow half.
    let url = server.url("/file/8M.bin?rate=8M&slow_from=4M");
    assert_completed(&setup.download(url).await, 8 * MIB);

    let reused = server
        .records()
        .iter()
        .filter(|record| record.request_on_connection > 1)
        .count();
    assert!(reused > 0, "no connection was used twice");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_server_rule_sets_the_connection_count() {
    let server = http().await;
    let setup = setup();
    setup.engine.set_server_limits(vec![ServerLimit {
        pattern: "127.0.0.*".to_owned(),
        connections: 2,
    }]);

    assert_completed(&setup.download(server.url("/file/8M.bin")).await, 8 * MIB);

    let first_wave: HashSet<String> = server
        .records()
        .iter()
        .filter_map(|record| {
            record
                .headers
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case("range"))
                .map(|(_, value)| value.clone())
        })
        .filter(|range| range.starts_with("bytes=0-") && range != "bytes=0-0")
        .collect();
    assert_eq!(
        first_wave.into_iter().collect::<Vec<_>>(),
        [format!("bytes=0-{}", 4 * MIB - 1)],
        "two connections means the first part is half the file"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_server_rule_for_another_host_changes_nothing() {
    let server = http().await;
    let setup = setup();
    setup.engine.set_server_limits(vec![ServerLimit {
        pattern: "*.other.test".to_owned(),
        connections: 1,
    }]);

    assert_completed(&setup.download(server.url("/file/8M.bin")).await, 8 * MIB);

    let parts = server
        .records()
        .iter()
        .filter(|record| record.status == 206)
        .count();
    assert!(parts >= 8);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_custom_user_agent_reaches_the_server() {
    let server = http().await;
    let setup = setup();
    setup
        .engine
        .set_network(&NetworkOptions {
            user_agent: Some("My Browser/1.0".to_owned()),
            ..NetworkOptions::default()
        })
        .unwrap();

    assert_completed(&setup.download(server.url("/file/1K.bin")).await, 1024);

    for record in server.records() {
        let agent = record
            .headers
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case("user-agent"))
            .map(|(_, value)| value.as_str());
        assert_eq!(agent, Some("My Browser/1.0"));
    }
}

// Proxies

#[tokio::test(flavor = "multi_thread")]
async fn downloads_through_an_http_proxy() {
    let proxy = http().await;
    let setup = setup();
    setup
        .engine
        .set_network(&NetworkOptions {
            proxy: ProxyMode::Manual(ManualProxy {
                url: format!("http://{}", proxy.addr()),
                ..ManualProxy::default()
            }),
            ..NetworkOptions::default()
        })
        .unwrap();

    // This host doesn't exist, so success proves the proxy did the work.
    let download = setup
        .download("http://tributary.invalid/file/2M.bin".to_owned())
        .await;

    assert_completed(&download, 2 * MIB);
    assert!(
        proxy.records()[0]
            .target
            .starts_with("http://tributary.invalid")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_proxy_login_is_sent() {
    let proxy = TestServer::start(testserver::Config {
        proxy_login: Some("proxyuser:proxypass".to_owned()),
        ..testserver::Config::default()
    })
    .await
    .unwrap();
    let setup = setup();
    setup
        .engine
        .set_network(&NetworkOptions {
            proxy: ProxyMode::Manual(ManualProxy {
                url: format!("http://{}", proxy.addr()),
                login: Some(login("proxyuser", "proxypass")),
                ..ManualProxy::default()
            }),
            ..NetworkOptions::default()
        })
        .unwrap();

    let download = setup
        .download("http://tributary.invalid/file/1M.bin".to_owned())
        .await;

    assert_completed(&download, MIB);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_host_on_the_exceptions_list_skips_the_proxy() {
    let server = http().await;
    let proxy = http().await;
    let setup = setup();
    setup
        .engine
        .set_network(&NetworkOptions {
            proxy: ProxyMode::Manual(ManualProxy {
                url: format!("http://{}", proxy.addr()),
                exceptions: "127.0.0.1".to_owned(),
                ..ManualProxy::default()
            }),
            ..NetworkOptions::default()
        })
        .unwrap();

    assert_completed(&setup.download(server.url("/file/1K.bin")).await, 1024);

    assert!(proxy.records().is_empty(), "the proxy shouldn't be used");
    assert!(!server.records().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn downloads_through_a_socks_proxy() {
    let server = http().await;
    let proxy = SocksProxy::start(server.addr()).await.unwrap();
    let setup = setup();
    setup
        .engine
        .set_network(&NetworkOptions {
            proxy: ProxyMode::Manual(ManualProxy {
                url: proxy.url(),
                ..ManualProxy::default()
            }),
            ..NetworkOptions::default()
        })
        .unwrap();

    let download = setup
        .download("http://tributary.invalid/file/2M.bin".to_owned())
        .await;

    assert_completed(&download, 2 * MIB);
    assert!(proxy.connections() > 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_bad_proxy_address_is_refused_up_front() {
    let setup = setup();

    let result = setup.engine.set_network(&NetworkOptions {
        proxy: ProxyMode::Manual(ManualProxy {
            url: "not a proxy address".to_owned(),
            ..ManualProxy::default()
        }),
        ..NetworkOptions::default()
    });

    assert!(matches!(result, Err(Error::InvalidProxy(_))));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_proxy_that_cannot_be_reached_fails_the_download_cleanly() {
    let setup = setup();
    setup
        .engine
        .set_network(&NetworkOptions {
            proxy: ProxyMode::Manual(ManualProxy {
                url: "http://127.0.0.1:9".to_owned(),
                ..ManualProxy::default()
            }),
            ..NetworkOptions::default()
        })
        .unwrap();

    let download = setup
        .download("http://tributary.invalid/a.bin".to_owned())
        .await;

    assert_eq!(download.state, State::Failed);
}

// Logins

#[tokio::test(flavor = "multi_thread")]
async fn downloads_with_a_basic_login() {
    let server = http().await;
    let setup = setup();
    let url = server.url(
        "/file/4M.bin?basic=alice%3Dx:secret"
            .replace("%3Dx", "")
            .as_str(),
    );

    let download = setup
        .download_with_login(url, login("alice", "secret"))
        .await;

    assert_completed(&download, 4 * MIB);
}

#[tokio::test(flavor = "multi_thread")]
async fn downloads_with_a_digest_login_over_several_connections() {
    let server = http().await;
    let setup = setup();
    let url = server.url("/file/8M.bin?digest=bob:hunter2");

    let download = setup
        .download_with_login(url, login("bob", "hunter2"))
        .await;

    assert_completed(&download, 8 * MIB);

    let parts = server
        .records()
        .iter()
        .filter(|record| record.status == 206)
        .count();
    assert!(
        parts >= 8,
        "only {parts} parts: the login should work on every connection"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_missing_login_is_reported_as_such() {
    let server = http().await;
    let setup = setup();

    let download = setup.download(server.url("/file/1M.bin?basic=a:b")).await;

    assert_eq!(download.state, State::Failed);
    assert_eq!(download.error.as_deref(), Some(LOGIN_NEEDED));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_wrong_password_is_reported_and_not_retried_forever() {
    let server = http().await;
    let setup = setup();
    let url = server.url("/file/1M.bin?digest=a:right");

    let download = setup.download_with_login(url, login("a", "wrong")).await;

    assert_eq!(download.state, State::Failed);
    assert_eq!(download.error.as_deref(), Some(LOGIN_NEEDED));
    assert!(
        server.records().len() <= 3,
        "{} requests",
        server.records().len()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_download_can_be_given_a_login_and_resumed() {
    let server = http().await;
    let setup = setup();
    let failed = setup.download(server.url("/file/1M.bin?basic=a:b")).await;

    setup.engine.set_login(failed.id, login("a", "b"));
    setup.engine.resume(failed.id, &[]).unwrap();

    assert_completed(&setup.engine.wait(failed.id).await.unwrap(), MIB);
}

#[tokio::test(flavor = "multi_thread")]
async fn inspecting_a_protected_file_asks_for_a_login() {
    let server = http().await;
    let setup = setup();
    let url = server.url("/file/1M.bin?basic=a:b");

    let without = setup.engine.inspect(&url, &[], None).await;
    assert!(matches!(without, Err(Error::LoginRequired)));

    let with = setup.engine.inspect(&url, &[], Some(login("a", "b"))).await;
    assert_eq!(with.unwrap().size, Some(MIB));
}

#[tokio::test(flavor = "multi_thread")]
async fn passwords_never_reach_the_disk() {
    let server = http().await;
    let setup = setup();
    let url = server.url("/file/1M.bin?digest=user:%70assword-value-123");

    let download = setup
        .download_with_login(url, login("user", "password-value-123"))
        .await;

    assert_completed(&download, MIB);
    assert!(!contains(&setup.saved_bytes(), b"password-value-123"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_login_in_the_address_is_used_but_not_saved() {
    let server = http().await;
    let setup = setup();
    let address = format!(
        "http://user:address-secret-99@{}/file/1M.bin?basic=user:address%2Dsecret%2D99",
        server.addr()
    );

    let download = setup.download(address).await;

    assert_completed(&download, MIB);
    assert!(!download.url.contains('@'), "saved as {}", download.url);
    assert!(!contains(&setup.saved_bytes(), b"address-secret-99"));
}

// FTP

async fn ftp(config: FtpConfig) -> FtpServer {
    FtpServer::start(config).await.unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn downloads_over_ftp_with_several_connections() {
    let server = ftp(FtpConfig::default()).await;
    let setup = setup();

    let download = setup.download(server.url("/pub/8M.bin")).await;

    assert_completed(&download, 8 * MIB);
    assert_eq!(download.file_name.as_deref(), Some("8M.bin"));

    let offsets: HashSet<u64> = server.retrievals().iter().map(|item| item.offset).collect();
    assert!(offsets.len() >= 8, "only {} parts", offsets.len());
    assert!(server.sessions() >= 8, "each part needs its own session");
}

#[tokio::test(flavor = "multi_thread")]
async fn ftp_works_in_active_mode() {
    let server = ftp(FtpConfig::default()).await;
    let setup = setup();
    setup
        .engine
        .set_network(&NetworkOptions {
            ftp_passive: false,
            ..NetworkOptions::default()
        })
        .unwrap();

    assert_completed(&setup.download(server.url("/pub/2M.bin")).await, 2 * MIB);

    assert!(server.retrievals().iter().all(|item| item.active));
    assert!(
        server
            .commands()
            .iter()
            .any(|line| line.starts_with("PORT"))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn ftp_uses_passive_mode_by_default() {
    let server = ftp(FtpConfig::default()).await;
    let setup = setup();

    assert_completed(&setup.download(server.url("/pub/1M.bin")).await, MIB);

    assert!(server.retrievals().iter().all(|item| !item.active));
}

#[tokio::test(flavor = "multi_thread")]
async fn ftp_falls_back_to_one_connection_without_resume_support() {
    let server = ftp(FtpConfig {
        allow_resume: false,
        ..FtpConfig::default()
    })
    .await;
    let setup = setup();

    let download = setup.download(server.url("/pub/2M.bin")).await;

    assert_completed(&download, 2 * MIB);
    assert_eq!(server.retrievals().len(), 1);
    assert!(!download.ranged);
}

#[tokio::test(flavor = "multi_thread")]
async fn ftp_logs_in_with_the_address_login_without_saving_it() {
    let server = ftp(FtpConfig {
        login: Some(("alice".to_owned(), "ftp-secret-42".to_owned())),
        ..FtpConfig::default()
    })
    .await;
    let setup = setup();

    let download = setup
        .download(server.url_with_login("alice", "ftp-secret-42", "/pub/1M.bin"))
        .await;

    assert_completed(&download, MIB);
    assert!(!download.url.contains("ftp-secret-42"));
    assert!(!contains(&setup.saved_bytes(), b"ftp-secret-42"));
}

#[tokio::test(flavor = "multi_thread")]
async fn ftp_reports_a_wrong_login() {
    let server = ftp(FtpConfig {
        login: Some(("alice".to_owned(), "right".to_owned())),
        ..FtpConfig::default()
    })
    .await;
    let setup = setup();

    let download = setup
        .download_with_login(server.url("/pub/1M.bin"), login("alice", "wrong"))
        .await;

    assert_eq!(download.state, State::Failed);
    assert_eq!(download.error.as_deref(), Some(LOGIN_NEEDED));
}

#[tokio::test(flavor = "multi_thread")]
async fn ftp_reports_a_missing_file_without_retrying() {
    let server = ftp(FtpConfig::default()).await;
    let setup = setup();

    let download = setup.download(server.url("/missing/file.bin")).await;

    assert_eq!(download.state, State::Failed);
    assert!(download.error.unwrap().contains("550"));
    assert_eq!(server.sessions(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn ftp_survives_dropped_transfers() {
    let server = ftp(FtpConfig {
        drop_after: Some(100 << 10),
        drop_times: 12,
        ..FtpConfig::default()
    })
    .await;
    let setup = setup();

    assert_completed(&setup.download(server.url("/pub/8M.bin")).await, 8 * MIB);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_dropped_single_connection_ftp_transfer_is_not_mistaken_for_complete() {
    let server = ftp(FtpConfig {
        allow_resume: false,
        drop_after: Some(100 << 10),
        drop_times: 1,
        ..FtpConfig::default()
    })
    .await;
    let setup = setup();

    assert_completed(&setup.download(server.url("/pub/1M.bin")).await, MIB);
    assert_eq!(
        server.retrievals().len(),
        2,
        "the cut transfer must be retried"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn ftp_downloads_pause_and_resume_where_they_stopped() {
    let server = ftp(FtpConfig {
        rate: Some(2 * MIB),
        ..FtpConfig::default()
    })
    .await;
    let setup = setup();
    let size = 32 * MIB;

    let id = setup
        .engine
        .add(setup.new_download(server.url("/pub/32M.bin")))
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    while setup.engine.get(id).unwrap().downloaded < 8 * MIB {
        assert!(Instant::now() < deadline, "no progress");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    setup.engine.pause(id).await.unwrap();
    let paused = setup.engine.get(id).unwrap();
    assert_eq!(paused.state, State::Paused);
    let before = server.retrievals().len();

    setup.engine.resume(id, &[]).unwrap();
    assert_completed(&setup.engine.wait(id).await.unwrap(), size);

    let resumed: u64 = server.retrievals()[before..]
        .iter()
        .map(|item| item.sent)
        .sum();
    assert!(
        resumed < size - paused.downloaded / 2,
        "fetched {resumed} bytes again"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_command_hidden_in_an_ftp_address_is_refused() {
    let server = ftp(FtpConfig::default()).await;
    let setup = setup();

    let download = setup
        .download(server.url("/pub/1M.bin%0D%0ADELE%20x"))
        .await;

    assert_eq!(download.state, State::Failed);
    assert!(
        !server
            .commands()
            .iter()
            .any(|line| line.starts_with("DELE")),
        "a command got through"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn inspecting_an_ftp_file_works() {
    let server = ftp(FtpConfig::default()).await;
    let setup = setup();

    let info = setup
        .engine
        .inspect(&server.url("/pub/3M.iso"), &[], None)
        .await
        .unwrap();

    assert_eq!(info.file_name, "3M.iso");
    assert_eq!(info.size, Some(3 * MIB));
    assert!(info.resumable);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_download_id_never_shows_a_password_in_its_error() {
    let setup = setup();

    let download = setup
        .download("ftp://user:leaky-secret@127.0.0.1:9/a.bin".to_owned())
        .await;

    assert_eq!(download.state, State::Failed);
    assert!(!download.error.unwrap_or_default().contains("leaky-secret"));
    assert_eq!(download.id, DownloadId(1));
}
