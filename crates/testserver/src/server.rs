//! The server: accepts connections, applies scenarios and records what happened.

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{self, Write};
use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use md5::{Digest as _, Md5};
use serde::Serialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::{JoinHandle, JoinSet};

use crate::content;
use crate::http::{RangeRequest, RequestHead, parse_range, reason};
use crate::scenario::{Scenario, with_redirects};

const MAX_HEAD_BYTES: usize = 64 << 10;
const MAX_CHUNK_BYTES: u64 = 64 << 10;
const MIN_CHUNK_BYTES: u64 = 1 << 10;
const THROTTLE_STEPS_PER_SECOND: u64 = 20;

/// The two `Last-Modified` values a file can have: original and replaced.
const LAST_MODIFIED: [&str; 2] = [
    "Wed, 21 Oct 2026 07:28:00 GMT",
    "Thu, 22 Oct 2026 09:15:00 GMT",
];

const INDEX_PAGE: &str = include_str!("index.html");

const DIGEST_REALM: &str = "test";
const DIGEST_NONCE: &str = "test-nonce";

/// Where to listen and where to write the request log.
#[derive(Debug, Clone, Default)]
pub struct Config {
    /// Port on 127.0.0.1. Zero picks a free one.
    pub port: u16,
    /// Appends one JSON object per request to this file.
    pub log_path: Option<PathBuf>,
    /// A `user:password` a client must give when using this server as a proxy.
    pub proxy_login: Option<String>,
}

/// How a response ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// Everything planned was sent.
    Complete,
    /// The server cut the connection, as the scenario asked.
    Dropped,
    /// The client stopped reading before the response finished.
    ClientClosed,
}

/// One request and how it was answered.
#[derive(Debug, Clone, Serialize)]
pub struct Record {
    /// Order of arrival across all connections, from 1.
    pub sequence: u64,
    /// When the request arrived, in milliseconds since the server started.
    pub at_ms: u64,
    pub duration_ms: u64,
    /// Identifies the TCP connection, from 1.
    pub connection: u64,
    pub peer_port: u16,
    /// Position of this request on its connection, from 1.
    pub request_on_connection: u32,
    pub method: String,
    pub target: String,
    pub version: String,
    /// In the exact order and case the client sent.
    pub headers: Vec<(String, String)>,
    pub status: u16,
    pub content_range: Option<String>,
    pub body_bytes_planned: Option<u64>,
    pub body_bytes_sent: u64,
    pub outcome: Outcome,
}

#[derive(Default)]
struct Counters {
    visits: u32,
    drops: u32,
    active: u32,
    /// Digest request counts already used, to catch replayed requests.
    digest_counts: HashSet<u32>,
}

/// Holds one of a URL's concurrent response slots until dropped.
struct Slot<'a> {
    state: &'a State,
    target: &'a str,
}

impl Drop for Slot<'_> {
    fn drop(&mut self) {
        let mut counters = self
            .state
            .counters
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(entry) = counters.get_mut(self.target) {
            entry.active -= 1;
        }
    }
}

struct State {
    started: Instant,
    next_connection: AtomicU64,
    next_request: AtomicU64,
    counters: Mutex<HashMap<String, Counters>>,
    records: Mutex<Vec<Record>>,
    log: Option<Mutex<File>>,
    proxy_login: Option<String>,
}

impl State {
    /// Counts a visit to this exact URL and returns its number, from 1.
    fn visit(&self, target: &str) -> u32 {
        let mut counters = self.counters.lock().unwrap_or_else(PoisonError::into_inner);
        let entry = counters.entry(target.to_owned()).or_default();
        entry.visits += 1;
        entry.visits
    }

    /// Takes a response slot for this URL, unless `limit` are already in use.
    fn enter<'a>(&'a self, target: &'a str, limit: Option<u32>) -> Option<Slot<'a>> {
        let mut counters = self.counters.lock().unwrap_or_else(PoisonError::into_inner);
        let entry = counters.entry(target.to_owned()).or_default();
        if limit.is_some_and(|limit| entry.active >= limit) {
            return None;
        }
        entry.active += 1;
        Some(Slot {
            state: self,
            target,
        })
    }

    /// Checks a Digest login, written out independently of the client under test. A
    /// count can't be used twice. Parallel requests may arrive in any order.
    fn digest_accepted(&self, head: &RequestHead, expected: &str) -> bool {
        let Some(value) = head.header("authorization") else {
            return false;
        };
        let Some(parameters) = value.strip_prefix("Digest ") else {
            return false;
        };
        let find = |wanted: &str| {
            parameters.split(", ").find_map(|pair| {
                let (name, value) = pair.split_once('=')?;
                (name == wanted).then(|| value.trim_matches('"').to_owned())
            })
        };

        let (Some(uri), Some(response), Some(count), Some(client_nonce)) =
            (find("uri"), find("response"), find("nc"), find("cnonce"))
        else {
            return false;
        };
        let Some((user, password)) = expected.split_once(':') else {
            return false;
        };

        let hash = |text: String| {
            Md5::digest(text)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        };
        let secret = hash(format!("{user}:{DIGEST_REALM}:{password}"));
        let request = hash(format!("GET:{uri}"));
        let correct = hash(format!(
            "{secret}:{DIGEST_NONCE}:{count}:{client_nonce}:auth:{request}"
        ));
        if response != correct || uri != head.target {
            return false;
        }

        let Ok(count) = u32::from_str_radix(&count, 16) else {
            return false;
        };
        let mut counters = self.counters.lock().unwrap_or_else(PoisonError::into_inner);
        let entry = counters.entry(head.target.clone()).or_default();
        entry.digest_counts.insert(count)
    }

    /// Claims one of the scenario's drops, if any are left for this URL.
    fn claim_drop(&self, target: &str, allowed: u32) -> bool {
        let mut counters = self.counters.lock().unwrap_or_else(PoisonError::into_inner);
        let entry = counters.entry(target.to_owned()).or_default();
        let available = entry.drops < allowed;
        if available {
            entry.drops += 1;
        }
        available
    }

    fn record(&self, record: Record) {
        if let Some(log) = &self.log {
            let mut file = log.lock().unwrap_or_else(PoisonError::into_inner);
            let written = serde_json::to_string(&record)
                .map_err(io::Error::from)
                .and_then(|line| writeln!(file, "{line}"));
            if let Err(error) = written {
                eprintln!("could not write to the request log: {error}");
            }
        }
        self.records
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(record);
    }
}

/// A running server. Dropping it stops the server and closes its connections.
pub struct TestServer {
    addr: SocketAddr,
    state: Arc<State>,
    accept_loop: JoinHandle<()>,
}

impl TestServer {
    /// Starts listening on 127.0.0.1. This server is never reachable from other machines.
    pub async fn start(config: Config) -> io::Result<Self> {
        let log = config
            .log_path
            .map(|path| File::options().create(true).append(true).open(path))
            .transpose()?
            .map(Mutex::new);

        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, config.port)).await?;
        let addr = listener.local_addr()?;
        let state = Arc::new(State {
            started: Instant::now(),
            next_connection: AtomicU64::new(1),
            next_request: AtomicU64::new(1),
            counters: Mutex::default(),
            records: Mutex::default(),
            log,
            proxy_login: config.proxy_login,
        });

        let accept_loop = tokio::spawn(accept_connections(listener, Arc::clone(&state)));
        Ok(Self {
            addr,
            state,
            accept_loop,
        })
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Returns the full URL of a path on this server.
    pub fn url(&self, target: &str) -> String {
        format!("http://{}{target}", self.addr)
    }

    /// Returns the requests that have finished, in the order they finished.
    pub fn records(&self) -> Vec<Record> {
        self.state
            .records
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.accept_loop.abort();
    }
}

async fn accept_connections(listener: TcpListener, state: Arc<State>) {
    // Held here so that aborting this task also aborts every open connection.
    let mut connections = JoinSet::new();

    loop {
        tokio::select! {
            accepted = listener.accept() => match accepted {
                Ok((stream, peer)) => {
                    connections.spawn(serve_connection(Arc::clone(&state), stream, peer));
                }
                Err(error) => eprintln!("could not accept a connection: {error}"),
            },
            Some(_) = connections.join_next() => {}
        }
    }
}

async fn serve_connection(state: Arc<State>, mut stream: TcpStream, peer: SocketAddr) {
    let connection = state.next_connection.fetch_add(1, Ordering::Relaxed);
    let _ = stream.set_nodelay(true);
    let mut pending = Vec::new();
    let mut request_on_connection = 0;

    while let Ok(Some(text)) = read_head(&mut stream, &mut pending).await {
        let Some(head) = RequestHead::parse(&text) else {
            let _ = stream
                .write_all(
                    b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .await;
            return;
        };

        request_on_connection += 1;
        let sequence = state.next_request.fetch_add(1, Ordering::Relaxed);
        let arrived = Instant::now();
        let at_ms = arrived.duration_since(state.started).as_millis() as u64;

        let served = respond(&state, &mut stream, &head).await;
        let keep_open = served.keep_open && served.outcome == Outcome::Complete;

        state.record(Record {
            sequence,
            at_ms,
            duration_ms: arrived.elapsed().as_millis() as u64,
            connection,
            peer_port: peer.port(),
            request_on_connection,
            method: head.method,
            target: head.target,
            version: head.version,
            headers: head.headers,
            status: served.status,
            content_range: served.content_range,
            body_bytes_planned: served.body_bytes_planned,
            body_bytes_sent: served.body_bytes_sent,
            outcome: served.outcome,
        });

        if !keep_open {
            return;
        }
    }
}

/// Reads up to the blank line that ends a request head. `None` means the client closed.
async fn read_head(stream: &mut TcpStream, pending: &mut Vec<u8>) -> io::Result<Option<String>> {
    loop {
        if let Some(end) = pending.windows(4).position(|window| window == b"\r\n\r\n") {
            let text = String::from_utf8_lossy(&pending[..end]).into_owned();
            pending.drain(..end + 4);
            return Ok(Some(text));
        }
        if pending.len() > MAX_HEAD_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "request head too large",
            ));
        }

        let mut chunk = [0u8; 4096];
        let count = stream.read(&mut chunk).await?;
        if count == 0 {
            return Ok(None);
        }
        pending.extend_from_slice(&chunk[..count]);
    }
}

/// The parts of a response that end up in its [`Record`].
struct Served {
    status: u16,
    content_range: Option<String>,
    body_bytes_planned: Option<u64>,
    body_bytes_sent: u64,
    outcome: Outcome,
    keep_open: bool,
}

async fn respond(state: &State, stream: &mut TcpStream, head: &RequestHead) -> Served {
    if head.method != "GET" && head.method != "HEAD" {
        let mut served = send_small(stream, head, 405, &[("Allow", "GET, HEAD".into())], b"").await;
        served.keep_open = false;
        return served;
    }

    // A client using this server as a proxy sends the full address.
    let target = match head.target.strip_prefix("http://") {
        Some(rest) => {
            if let Some(expected) = &state.proxy_login {
                let wanted = format!("Basic {}", STANDARD.encode(expected));
                if head.header("proxy-authorization") != Some(wanted.as_str()) {
                    let headers = [("Proxy-Authenticate", "Basic realm=\"proxy\"".to_owned())];
                    return send_small(stream, head, 407, &headers, b"").await;
                }
            }
            rest.find('/').map_or("/", |index| &rest[index..])
        }
        None => &head.target,
    };

    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    match path {
        "/" => {
            let headers = [("Content-Type", "text/html; charset=utf-8".to_owned())];
            send_small(stream, head, 200, &headers, INDEX_PAGE.as_bytes()).await
        }
        "/login" => {
            let headers = [
                ("Set-Cookie", "tributary_test=1; Path=/".to_owned()),
                ("Location", "/".to_owned()),
            ];
            send_small(stream, head, 302, &headers, b"").await
        }
        _ => match path.strip_prefix("/file/") {
            Some(name) => serve_file(state, stream, head, name, query).await,
            None => send_small(stream, head, 404, &[], b"not found\n").await,
        },
    }
}

async fn serve_file(
    state: &State,
    stream: &mut TcpStream,
    head: &RequestHead,
    name: &str,
    query: &str,
) -> Served {
    let scenario = match Scenario::parse(name, query) {
        Ok(scenario) => scenario,
        Err(message) => {
            return send_small(stream, head, 400, &[], format!("{message}\n").as_bytes()).await;
        }
    };
    let visit = state.visit(&head.target);

    if let Some(milliseconds) = scenario.delay {
        tokio::time::sleep(Duration::from_millis(milliseconds)).await;
    }

    if let Some(required) = &scenario.require_cookie {
        let cookies = head.header("cookie").unwrap_or_default();
        if !cookies.split(';').any(|cookie| cookie.trim() == required) {
            return send_small(stream, head, 403, &[], b"cookie required\n").await;
        }
    }

    if let Some(expected) = &scenario.basic {
        let wanted = format!("Basic {}", STANDARD.encode(expected));
        if head.header("authorization") != Some(wanted.as_str()) {
            let headers = [("WWW-Authenticate", "Basic realm=\"test\"".to_owned())];
            return send_small(stream, head, 401, &headers, b"login required\n").await;
        }
    }

    if let Some(expected) = &scenario.digest
        && !state.digest_accepted(head, expected)
    {
        let challenge =
            format!("Digest realm=\"{DIGEST_REALM}\", nonce=\"{DIGEST_NONCE}\", qop=\"auth\"");
        let headers = [("WWW-Authenticate", challenge)];
        return send_small(stream, head, 401, &headers, b"login required\n").await;
    }

    if let Some(status) = scenario.fail.filter(|_| visit <= scenario.fail_times) {
        let retry_after = scenario
            .retry_after
            .map(|seconds| ("Retry-After", seconds.to_string()));
        return send_small(stream, head, status, retry_after.as_slice(), b"").await;
    }

    if scenario.redirects > 0 {
        let location = format!(
            "/file/{name}?{}",
            with_redirects(query, scenario.redirects - 1)
        );
        return send_small(stream, head, 302, &[("Location", location)], b"").await;
    }

    let Some(_slot) = state.enter(&head.target, scenario.limit) else {
        return send_small(stream, head, 503, &[], b"too many connections\n").await;
    };

    let version = scenario.version_at(visit);
    let seed = scenario.seed.wrapping_add(version);
    let etag = format!("\"{seed:x}-{:x}\"", scenario.size);
    let last_modified = LAST_MODIFIED[version as usize];

    let validators_match = head
        .header("if-range")
        .is_none_or(|value| value == etag || value == last_modified);
    let range = match head.header("range") {
        Some(value) if scenario.ranges && validators_match => parse_range(value, scenario.size),
        _ => RangeRequest::Full,
    };

    let (status, start, length, content_range) = match range {
        RangeRequest::Full => (200, 0, scenario.size, None),
        RangeRequest::Partial { start, end } => (
            206,
            start,
            end - start + 1,
            Some(format!("bytes {start}-{end}/{}", scenario.size)),
        ),
        RangeRequest::Unsatisfiable => {
            let headers = [("Content-Range", format!("bytes */{}", scenario.size))];
            return send_small(stream, head, 416, &headers, b"").await;
        }
    };

    let mut headers = vec![("Content-Type", content_type(name).to_owned())];
    if scenario.ranges {
        headers.push(("Accept-Ranges", "bytes".to_owned()));
    }
    if scenario.validators {
        headers.push(("ETag", etag));
        headers.push(("Last-Modified", last_modified.to_owned()));
    }
    if let Some(filename) = &scenario.filename {
        headers.push((
            "Content-Disposition",
            format!("attachment; filename=\"{filename}\""),
        ));
    }
    if let Some(value) = &content_range {
        headers.push(("Content-Range", value.clone()));
    }
    if scenario.length {
        headers.push(("Content-Length", length.to_string()));
    } else {
        headers.push(("Transfer-Encoding", "chunked".to_owned()));
    }

    let keep_alive = head.keep_alive();
    let mut served = Served {
        status,
        content_range,
        body_bytes_planned: Some(length),
        body_bytes_sent: 0,
        outcome: Outcome::Complete,
        keep_open: keep_alive,
    };

    if stream
        .write_all(render_head(status, &headers, keep_alive).as_bytes())
        .await
        .is_err()
    {
        served.outcome = Outcome::ClientClosed;
        return served;
    }
    if head.method == "HEAD" {
        served.body_bytes_planned = Some(0);
        return served;
    }

    let drop_at = scenario
        .drop_after
        .filter(|&limit| limit < length && state.claim_drop(&head.target, scenario.drop_times));
    let plan = BodyPlan {
        seed,
        start,
        length,
        chunked: !scenario.length,
        rate: scenario.rate_from(start),
        drop_at,
    };
    (served.body_bytes_sent, served.outcome) = send_body(stream, plan).await;
    served
}

struct BodyPlan {
    seed: u64,
    start: u64,
    length: u64,
    chunked: bool,
    rate: Option<u64>,
    drop_at: Option<u64>,
}

async fn send_body(stream: &mut TcpStream, plan: BodyPlan) -> (u64, Outcome) {
    let chunk_limit = plan.rate.map_or(MAX_CHUNK_BYTES, |rate| {
        (rate / THROTTLE_STEPS_PER_SECOND).clamp(MIN_CHUNK_BYTES, MAX_CHUNK_BYTES)
    });
    let mut buffer = vec![0u8; chunk_limit as usize];
    let began = Instant::now();
    let mut sent = 0;

    while sent < plan.length {
        if plan.drop_at == Some(sent) {
            return (sent, Outcome::Dropped);
        }

        let until_stop = plan.drop_at.unwrap_or(plan.length).min(plan.length) - sent;
        let count = until_stop.min(chunk_limit) as usize;
        content::fill(plan.seed, plan.start + sent, &mut buffer[..count]);

        let written = if plan.chunked {
            write_chunk(stream, &buffer[..count]).await
        } else {
            stream.write_all(&buffer[..count]).await
        };
        if written.is_err() {
            return (sent, Outcome::ClientClosed);
        }
        sent += count as u64;

        if let Some(rate) = plan.rate {
            let due = Duration::from_secs_f64(sent as f64 / rate as f64);
            if let Some(wait) = due.checked_sub(began.elapsed()) {
                tokio::time::sleep(wait).await;
            }
        }
    }

    if plan.drop_at == Some(sent) {
        return (sent, Outcome::Dropped);
    }
    if plan.chunked && stream.write_all(b"0\r\n\r\n").await.is_err() {
        return (sent, Outcome::ClientClosed);
    }
    (sent, Outcome::Complete)
}

async fn write_chunk(stream: &mut TcpStream, data: &[u8]) -> io::Result<()> {
    stream
        .write_all(format!("{:x}\r\n", data.len()).as_bytes())
        .await?;
    stream.write_all(data).await?;
    stream.write_all(b"\r\n").await
}

/// Sends a response whose body fits in memory.
async fn send_small(
    stream: &mut TcpStream,
    head: &RequestHead,
    status: u16,
    headers: &[(&str, String)],
    body: &[u8],
) -> Served {
    let keep_alive = head.keep_alive();
    let mut all_headers = headers.to_vec();
    all_headers.push(("Content-Length", body.len().to_string()));

    let mut bytes = render_head(status, &all_headers, keep_alive).into_bytes();
    let body_length = if head.method == "HEAD" { 0 } else { body.len() };
    bytes.extend_from_slice(&body[..body_length]);

    let outcome = match stream.write_all(&bytes).await {
        Ok(()) => Outcome::Complete,
        Err(_) => Outcome::ClientClosed,
    };
    let sent = if outcome == Outcome::Complete {
        body_length as u64
    } else {
        0
    };

    Served {
        status,
        content_range: None,
        body_bytes_planned: Some(body_length as u64),
        body_bytes_sent: sent,
        outcome,
        keep_open: keep_alive,
    }
}

fn render_head(status: u16, headers: &[(&str, String)], keep_alive: bool) -> String {
    let mut text = format!(
        "HTTP/1.1 {status} {}\r\nServer: tributary-testserver\r\n",
        reason(status)
    );
    for (name, value) in headers {
        text.push_str(&format!("{name}: {value}\r\n"));
    }
    if !keep_alive {
        text.push_str("Connection: close\r\n");
    }
    text.push_str("\r\n");
    text
}

fn content_type(name: &str) -> &'static str {
    match name
        .rsplit('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "zip" => "application/zip",
        "pdf" => "application/pdf",
        "exe" => "application/vnd.microsoft.portable-executable",
        "mp4" => "video/mp4",
        "mp3" => "audio/mpeg",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}
