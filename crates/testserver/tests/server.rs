//! Exercises the server over real sockets, the way a download client would.

use std::time::Instant;

use testserver::content;
use testserver::{Config, Outcome, TestServer};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

struct Reply {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Reply {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(candidate, _)| candidate.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

async fn start() -> TestServer {
    TestServer::start(Config::default()).await.unwrap()
}

async fn send(server: &TestServer, method: &str, target: &str, headers: &[(&str, &str)]) -> Reply {
    let mut stream = TcpStream::connect(server.addr()).await.unwrap();
    let mut request = format!("{method} {target} HTTP/1.1\r\nHost: test\r\nConnection: close\r\n");
    for (name, value) in headers {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    request.push_str("\r\n");
    stream.write_all(request.as_bytes()).await.unwrap();

    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).await.unwrap();

    let end = raw
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .unwrap();
    let head = String::from_utf8(raw[..end].to_vec()).unwrap();
    let mut lines = head.split("\r\n");
    let status = lines
        .next()
        .unwrap()
        .split(' ')
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();
    let headers = lines
        .map(|line| {
            let (name, value) = line.split_once(": ").unwrap();
            (name.to_owned(), value.to_owned())
        })
        .collect();

    Reply {
        status,
        headers,
        body: raw[end + 4..].to_vec(),
    }
}

async fn get(server: &TestServer, target: &str, headers: &[(&str, &str)]) -> Reply {
    send(server, "GET", target, headers).await
}

fn expected(seed: u64, start: u64, length: usize) -> Vec<u8> {
    let mut bytes = vec![0u8; length];
    content::fill(seed, start, &mut bytes);
    bytes
}

/// Removes the chunk framing from a chunked body.
fn dechunk(mut body: &[u8]) -> Vec<u8> {
    let mut decoded = Vec::new();
    loop {
        let line_end = body
            .windows(2)
            .position(|window| window == b"\r\n")
            .unwrap();
        let size =
            usize::from_str_radix(std::str::from_utf8(&body[..line_end]).unwrap(), 16).unwrap();
        if size == 0 {
            return decoded;
        }
        let start = line_end + 2;
        decoded.extend_from_slice(&body[start..start + size]);
        body = &body[start + size + 2..];
    }
}

#[tokio::test]
async fn serves_a_whole_file_that_matches_its_checksum() {
    let server = start().await;
    let reply = get(&server, "/file/100K.bin?seed=9", &[]).await;

    assert_eq!(reply.status, 200);
    assert_eq!(reply.header("Accept-Ranges"), Some("bytes"));
    assert_eq!(reply.header("Content-Length"), Some("102400"));
    assert_eq!(reply.body, expected(9, 0, 102_400));
}

#[tokio::test]
async fn serves_exact_bytes_for_a_range() {
    let server = start().await;
    let reply = get(&server, "/file/1K.bin", &[("Range", "bytes=100-199")]).await;

    assert_eq!(reply.status, 206);
    assert_eq!(reply.header("Content-Range"), Some("bytes 100-199/1024"));
    assert_eq!(reply.body, expected(1, 100, 100));
}

#[tokio::test]
async fn answers_416_past_the_end_of_the_file() {
    let server = start().await;
    let reply = get(&server, "/file/1K.bin", &[("Range", "bytes=5000-")]).await;

    assert_eq!(reply.status, 416);
    assert_eq!(reply.header("Content-Range"), Some("bytes */1024"));
}

#[tokio::test]
async fn head_sends_headers_without_a_body() {
    let server = start().await;
    let reply = send(&server, "HEAD", "/file/1K.bin", &[]).await;

    assert_eq!(reply.status, 200);
    assert_eq!(reply.header("Content-Length"), Some("1024"));
    assert!(reply.body.is_empty());
}

#[tokio::test]
async fn ignores_ranges_when_the_scenario_disables_them() {
    let server = start().await;
    let reply = get(&server, "/file/1K.bin?ranges=0", &[("Range", "bytes=0-9")]).await;

    assert_eq!(reply.status, 200);
    assert_eq!(reply.header("Accept-Ranges"), None);
    assert_eq!(reply.body.len(), 1024);
}

#[tokio::test]
async fn sends_chunked_bodies_with_no_content_length() {
    let server = start().await;
    let reply = get(&server, "/file/200K.bin?length=0", &[]).await;

    assert_eq!(reply.header("Content-Length"), None);
    assert_eq!(reply.header("Transfer-Encoding"), Some("chunked"));
    assert_eq!(dechunk(&reply.body), expected(1, 0, 200 << 10));
}

#[tokio::test]
async fn if_range_with_an_old_validator_returns_the_whole_file() {
    let server = start().await;
    let current = get(&server, "/file/1K.bin", &[]).await;
    let etag = current.header("ETag").unwrap();

    let matching = get(
        &server,
        "/file/1K.bin",
        &[("Range", "bytes=0-9"), ("If-Range", etag)],
    )
    .await;
    assert_eq!(matching.status, 206);

    let stale = get(
        &server,
        "/file/1K.bin",
        &[("Range", "bytes=0-9"), ("If-Range", "\"old\"")],
    )
    .await;
    assert_eq!(stale.status, 200);
    assert_eq!(stale.body.len(), 1024);
}

#[tokio::test]
async fn omits_validators_when_asked() {
    let server = start().await;
    let reply = get(&server, "/file/1K.bin?validators=0", &[]).await;

    assert_eq!(reply.header("ETag"), None);
    assert_eq!(reply.header("Last-Modified"), None);
}

#[tokio::test]
async fn fails_the_requested_number_of_times_then_recovers() {
    let server = start().await;
    let target = "/file/1K.bin?fail=503&fail_times=2&retry_after=7";

    for _ in 0..2 {
        let reply = get(&server, target, &[]).await;
        assert_eq!(reply.status, 503);
        assert_eq!(reply.header("Retry-After"), Some("7"));
    }
    assert_eq!(get(&server, target, &[]).await.status, 200);
}

#[tokio::test]
async fn cuts_the_first_response_then_serves_normally() {
    let server = start().await;
    let target = "/file/100K.bin?drop_after=10K";

    let cut = get(&server, target, &[]).await;
    assert_eq!(cut.header("Content-Length"), Some("102400"));
    assert_eq!(cut.body, expected(1, 0, 10 << 10));

    let retried = get(&server, target, &[]).await;
    assert_eq!(retried.body.len(), 102_400);

    let outcomes: Vec<_> = server
        .records()
        .iter()
        .map(|record| record.outcome)
        .collect();
    assert_eq!(outcomes, [Outcome::Dropped, Outcome::Complete]);
}

#[tokio::test]
async fn a_drop_past_the_end_of_a_range_is_not_used_up() {
    let server = start().await;
    let target = "/file/100K.bin?drop_after=50K";

    let short = get(&server, target, &[("Range", "bytes=0-99")]).await;
    assert_eq!(short.body.len(), 100);

    let long = get(&server, target, &[]).await;
    assert_eq!(long.body.len(), 50 << 10);
}

#[tokio::test]
async fn follows_a_redirect_chain_to_the_file() {
    let server = start().await;
    let first = get(&server, "/file/1K.bin?redirects=2", &[]).await;
    assert_eq!(first.status, 302);

    let second = get(&server, first.header("Location").unwrap(), &[]).await;
    assert_eq!(second.status, 302);

    let last = get(&server, second.header("Location").unwrap(), &[]).await;
    assert_eq!(last.status, 200);
    assert_eq!(last.body.len(), 1024);
}

#[tokio::test]
async fn replaces_the_file_after_the_limit() {
    let server = start().await;
    let target = "/file/1K.bin?change_after=1";

    let before = get(&server, target, &[]).await;
    let after = get(&server, target, &[]).await;

    assert_ne!(before.header("ETag"), after.header("ETag"));
    assert_ne!(
        before.header("Last-Modified"),
        after.header("Last-Modified")
    );
    assert_eq!(after.body, expected(2, 0, 1024));
}

#[tokio::test]
async fn requires_the_exact_cookie() {
    let server = start().await;
    let target = "/file/1K.bin?require_cookie=session%3Dabc";

    assert_eq!(get(&server, target, &[]).await.status, 403);
    assert_eq!(
        get(&server, target, &[("Cookie", "session=xyz")])
            .await
            .status,
        403
    );
    assert_eq!(
        get(&server, target, &[("Cookie", "a=1; session=abc")])
            .await
            .status,
        200
    );
}

#[tokio::test]
async fn names_files_with_content_disposition() {
    let server = start().await;
    let reply = get(&server, "/file/1K?filename=my%20report.zip", &[]).await;

    assert_eq!(
        reply.header("Content-Disposition"),
        Some("attachment; filename=\"my report.zip\"")
    );
}

#[tokio::test]
async fn throttling_slows_the_transfer_to_the_requested_rate() {
    let server = start().await;
    let began = Instant::now();
    let reply = get(&server, "/file/40K.bin?rate=100K", &[]).await;

    assert_eq!(reply.body.len(), 40 << 10);
    let elapsed = began.elapsed().as_secs_f64();
    assert!(elapsed > 0.3, "40 KiB at 100 KiB/s took only {elapsed:.3}s");
}

#[tokio::test]
async fn rejects_a_mistyped_option_instead_of_ignoring_it() {
    let server = start().await;
    let reply = get(&server, "/file/1K.bin?drop_aftr=5", &[]).await;

    assert_eq!(reply.status, 400);
    assert!(String::from_utf8_lossy(&reply.body).contains("drop_aftr"));
}

#[tokio::test]
async fn refuses_requests_over_the_connection_limit_until_a_slot_frees() {
    let server = start().await;
    let target = "/file/1M.bin?limit=1&rate=100K";

    let mut first = TcpStream::connect(server.addr()).await.unwrap();
    let request = format!("GET {target} HTTP/1.1\r\nHost: test\r\n\r\n");
    first.write_all(request.as_bytes()).await.unwrap();
    let mut byte = [0u8; 1];
    first.read_exact(&mut byte).await.unwrap();

    assert_eq!(get(&server, target, &[]).await.status, 503);

    drop(first);
    let mut status = 503;
    for _ in 0..50 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        status = send(&server, "HEAD", target, &[]).await.status;
        if status == 200 {
            break;
        }
    }
    assert_eq!(status, 200);
}

#[tokio::test]
async fn can_delay_its_answer() {
    let server = start().await;
    let began = Instant::now();

    let reply = get(&server, "/file/1K.bin?delay=300", &[]).await;

    assert_eq!(reply.status, 200);
    assert!(began.elapsed().as_millis() >= 280);
}

#[tokio::test]
async fn refuses_methods_that_are_not_downloads() {
    let server = start().await;
    let reply = send(&server, "POST", "/file/1K.bin", &[]).await;

    assert_eq!(reply.status, 405);
    assert_eq!(reply.header("Allow"), Some("GET, HEAD"));
}

#[tokio::test]
async fn logs_exact_headers_and_connection_reuse() {
    let server = start().await;
    let mut stream = TcpStream::connect(server.addr()).await.unwrap();

    let request =
        "GET /file/10.bin HTTP/1.1\r\nHost: test\r\nX-MiXeD-CaSe: 1\r\nRange: bytes=0-4\r\n\r\n";
    for _ in 0..2 {
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut raw = vec![0u8; 4096];
        let mut filled = 0;
        while !raw[..filled].ends_with(&expected(1, 0, 5)) {
            filled += stream.read(&mut raw[filled..]).await.unwrap();
        }
    }
    drop(stream);

    let records = server.records();
    assert_eq!(records.len(), 2);
    assert_eq!(
        records[0].headers[1],
        ("X-MiXeD-CaSe".to_owned(), "1".to_owned())
    );
    assert_eq!(records[0].connection, records[1].connection);
    assert_eq!(records[1].request_on_connection, 2);
    assert_eq!(records[0].content_range.as_deref(), Some("bytes 0-4/10"));
    assert_eq!(records[0].body_bytes_sent, 5);
}

#[tokio::test]
async fn writes_one_json_object_per_request_to_the_log_file() {
    let path = std::env::temp_dir().join(format!("testserver-log-{}.jsonl", std::process::id()));
    let _ = std::fs::remove_file(&path);

    let server = TestServer::start(Config {
        port: 0,
        log_path: Some(path.clone()),
    })
    .await
    .unwrap();
    get(&server, "/file/1K.bin", &[]).await;
    get(&server, "/missing", &[]).await;
    drop(server);

    let text = std::fs::read_to_string(&path).unwrap();
    std::fs::remove_file(&path).unwrap();

    let lines: Vec<serde_json::Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0]["target"], "/file/1K.bin");
    assert_eq!(lines[1]["status"], 404);
}
