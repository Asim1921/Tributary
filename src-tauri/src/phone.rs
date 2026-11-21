//! Sending links from a phone to Tributary over the home network. Off until turned on.
//!
//! A small page on the PC, reached by a QR code holding the PC's address on the home
//! network and a pairing code, lets a phone add a link, which the PC then asks about just
//! as it does a link from the browser, and see how downloads are getting on. Nothing else
//! can be reached from it: no files, no settings. Every request but the page itself needs
//! the pairing code, sent in a header that another site's page can't add.

use std::convert::Infallible;
use std::env;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use engine::State;
use http_body_util::{BodyExt, Full, Limited};
use hyper::body::{Bytes, Incoming};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::{AppHandle, Manager, State as Managed};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

use crate::capture::{self, Captured};
use crate::commands::AppState;

/// Binds somewhere else, for tests: `127.0.0.1` keeps it on this computer.
const ADDRESS_VARIABLE: &str = "TRIBUTARY_PHONE_ADDRESS";

const TOKEN_HEADER: &str = "x-tributary-token";

/// The most a request may carry: a link is small.
const MAX_BODY_BYTES: usize = 8 << 10;

/// Requests allowed each second, from everyone together.
const MAX_REQUESTS_PER_SECOND: u64 = 20;

/// How long a phone may take to send a request's headers.
const HEADER_TIME: Duration = Duration::from_secs(10);

/// The most downloads listed for the phone.
const MAX_LISTED: usize = 100;

const PAGE: &str = include_str!("phone.html");

/// The server while it runs.
#[derive(Default)]
pub struct Phone {
    running: Mutex<Option<Running>>,
}

struct Running {
    address: SocketAddr,
    stop: CancellationToken,
}

impl Phone {
    fn running(&self) -> MutexGuard<'_, Option<Running>> {
        self.running.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// A new pairing code: 32 random bytes, as hex.
pub fn new_token() -> String {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("Windows always has random numbers");
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Compares in the same time whatever the answer, so timing gives nothing away.
fn same(given: &str, token: &str) -> bool {
    given.len() == token.len()
        && given
            .bytes()
            .zip(token.bytes())
            .fold(0u8, |difference, (a, b)| difference | (a ^ b))
            == 0
}

/// Whether an address belongs to a home or office network, rather than the internet.
pub fn is_home_network(address: Ipv4Addr) -> bool {
    address.is_private()
}

/// This computer's address on the home network: the one it would use to reach the
/// internet, found without sending anything. None if that isn't a home network address.
pub fn home_address() -> Option<Ipv4Addr> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    // Connecting a UDP socket only picks a route; nothing is sent. 192.0.2.1 is reserved
    // for documentation, so it's never a real host.
    socket.connect((Ipv4Addr::new(192, 0, 2, 1), 9)).ok()?;
    match socket.local_addr().ok()?.ip() {
        IpAddr::V4(address) if is_home_network(address) => Some(address),
        _ => None,
    }
}

/// Where to listen: the home network address, or the test override.
fn listen_address() -> Result<Ipv4Addr, String> {
    if let Ok(chosen) = env::var(ADDRESS_VARIABLE) {
        return chosen
            .parse()
            .map_err(|_| format!("{ADDRESS_VARIABLE} isn't an address"));
    }
    home_address().ok_or_else(|| {
        "This computer isn't on a home or office network, so a phone can't reach it.".to_owned()
    })
}

/// Whether a phone at `peer` may connect: one on the same kind of network.
fn peer_allowed(peer: IpAddr, listening: Ipv4Addr) -> bool {
    match peer {
        IpAddr::V4(peer) => {
            is_home_network(peer) || (peer.is_loopback() && listening.is_loopback())
        }
        IpAddr::V6(_) => false,
    }
}

/// Counts requests in the current second, refusing past the limit.
struct RateLimit {
    second: AtomicU64,
    count: AtomicU64,
}

impl RateLimit {
    const fn new() -> Self {
        Self {
            second: AtomicU64::new(0),
            count: AtomicU64::new(0),
        }
    }

    fn allow(&self, now_seconds: u64) -> bool {
        if self.second.swap(now_seconds, Ordering::Relaxed) != now_seconds {
            self.count.store(0, Ordering::Relaxed);
        }
        self.count.fetch_add(1, Ordering::Relaxed) < MAX_REQUESTS_PER_SECOND
    }
}

static LIMIT: RateLimit = RateLimit::new();

fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or_default()
}

fn reply(status: StatusCode, content_type: &str, body: impl Into<Bytes>) -> Response<Full<Bytes>> {
    Response::builder()
        .status(status)
        .header("Content-Type", content_type)
        .header("Cache-Control", "no-store")
        .header("X-Content-Type-Options", "nosniff")
        .header("Referrer-Policy", "no-referrer")
        .header(
            "Content-Security-Policy",
            "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; connect-src 'self'; img-src data:",
        )
        .body(Full::new(body.into()))
        .unwrap_or_default()
}

fn json_reply(status: StatusCode, value: serde_json::Value) -> Response<Full<Bytes>> {
    reply(status, "application/json", value.to_string())
}

#[derive(Deserialize)]
struct AddBody {
    url: String,
}

/// A download as the phone sees it: its name and how far it's got.
#[derive(Serialize)]
struct Listed {
    name: String,
    state: &'static str,
    percent: Option<u8>,
}

async fn handle(
    app: AppHandle,
    token: String,
    request: Request<Incoming>,
) -> Response<Full<Bytes>> {
    if !LIMIT.allow(now_seconds()) {
        return json_reply(
            StatusCode::TOO_MANY_REQUESTS,
            json!({ "error": "too many requests" }),
        );
    }
    let path = request.uri().path().to_owned();
    let method = request.method().clone();
    if method == Method::GET && path == "/" {
        return reply(StatusCode::OK, "text/html; charset=utf-8", PAGE);
    }

    let given = request
        .headers()
        .get(TOKEN_HEADER)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    if !same(given, &token) {
        return json_reply(
            StatusCode::FORBIDDEN,
            json!({ "error": "this phone isn't paired" }),
        );
    }

    match (method, path.as_str()) {
        (Method::POST, "/add") => {
            let body = match Limited::new(request.into_body(), MAX_BODY_BYTES)
                .collect()
                .await
            {
                Ok(body) => body.to_bytes(),
                Err(_) => {
                    return json_reply(
                        StatusCode::PAYLOAD_TOO_LARGE,
                        json!({ "error": "too large" }),
                    );
                }
            };
            let Ok(AddBody { url }) = serde_json::from_slice(&body) else {
                return json_reply(StatusCode::BAD_REQUEST, json!({ "error": "send a link" }));
            };
            let file = Captured {
                url: url.trim().to_owned(),
                ..Captured::default()
            };
            if !file.is_valid() {
                return json_reply(
                    StatusCode::BAD_REQUEST,
                    json!({ "error": "that isn't a web or FTP address" }),
                );
            }
            // Asked about on the PC, as a link from the browser is.
            match capture::receive(&app, vec![file], false) {
                Ok(()) => json_reply(StatusCode::ACCEPTED, json!({ "ok": true })),
                Err(problem) => json_reply(StatusCode::BAD_REQUEST, json!({ "error": problem })),
            }
        }
        (Method::GET, "/list") => {
            let state = app.state::<AppState>();
            let mut downloads = state.engine.list().unwrap_or_default();
            downloads.sort_by_key(|download| std::cmp::Reverse(download.added_at));
            let listed: Vec<Listed> = downloads
                .iter()
                // A video's separate sound is part of its video, not a download of its own.
                .filter(|download| download.companion.is_none_or(|other| other > download.id))
                .take(MAX_LISTED)
                .map(|download| Listed {
                    name: download
                        .file_name
                        .clone()
                        .unwrap_or_else(|| "a new download".to_owned()),
                    state: match download.state {
                        State::Running => "downloading",
                        State::Paused => "stopped",
                        State::Completed => "complete",
                        State::Failed => "failed",
                    },
                    percent: download
                        .size
                        .filter(|size| *size > 0)
                        .map(|size| ((download.downloaded.min(size) * 100) / size) as u8),
                })
                .collect();
            json_reply(StatusCode::OK, json!(listed))
        }
        _ => json_reply(StatusCode::NOT_FOUND, json!({ "error": "not here" })),
    }
}

/// Starts listening, if it isn't already. Returns where.
async fn start(app: &AppHandle) -> Result<SocketAddr, String> {
    let state = app.state::<AppState>();
    let phone = app.state::<Phone>();
    if let Some(running) = phone.running().as_ref() {
        return Ok(running.address);
    }
    let token = state.settings_snapshot().phone_token;
    if token.len() != 64 {
        return Err("There's no pairing code yet.".to_owned());
    }
    let address = listen_address()?;
    let listener = TcpListener::bind((address, state.settings_snapshot().phone_port))
        .await
        .or(TcpListener::bind((address, 0)).await)
        .map_err(|error| format!("couldn't listen on the home network: {error}"))?;
    let bound = listener.local_addr().map_err(|error| error.to_string())?;
    let stop = CancellationToken::new();
    *phone.running() = Some(Running {
        address: bound,
        stop: stop.clone(),
    });

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            let accepted = tokio::select! {
                () = stop.cancelled() => return,
                accepted = listener.accept() => accepted,
            };
            let Ok((stream, peer)) = accepted else {
                continue;
            };
            if !peer_allowed(peer.ip(), address) {
                continue;
            }
            let app = app.clone();
            let token = token.clone();
            let stop = stop.clone();
            tauri::async_runtime::spawn(async move {
                let service = service_fn(move |request| {
                    let app = app.clone();
                    let token = token.clone();
                    async move { Ok::<_, Infallible>(handle(app, token, request).await) }
                });
                let mut builder = http1::Builder::new();
                builder
                    .timer(hyper_util::rt::TokioTimer::new())
                    .header_read_timeout(HEADER_TIME)
                    .keep_alive(false);
                let connection = builder.serve_connection(TokioIo::new(stream), service);
                tokio::select! {
                    () = stop.cancelled() => {}
                    _ = connection => {}
                }
            });
        }
    });
    Ok(bound)
}

fn stop(app: &AppHandle) {
    if let Some(running) = app.state::<Phone>().running().take() {
        running.stop.cancel();
    }
}

/// Starts the server when the app starts, if it was left on.
pub fn start_if_wanted(app: &AppHandle) {
    if !app.state::<AppState>().settings_snapshot().phone_enabled {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let _ = start(&app).await;
    });
}

/// What Options shows: whether it's on, the address for the phone with its pairing code,
/// and that address as a QR code.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhoneStatus {
    running: bool,
    address: Option<String>,
    /// An SVG picture of the address, made here from nothing but the address.
    qr_code: Option<String>,
    problem: Option<String>,
}

fn status_of(app: &AppHandle, problem: Option<String>) -> PhoneStatus {
    let running = app
        .state::<Phone>()
        .running()
        .as_ref()
        .map(|running| running.address);
    let token = app.state::<AppState>().settings_snapshot().phone_token;
    let address = running.map(|address| format!("http://{address}/#{token}"));
    let qr_code = address.as_deref().and_then(|address| {
        qrcode::QrCode::new(address.as_bytes()).ok().map(|code| {
            code.render::<qrcode::render::svg::Color>()
                .min_dimensions(200, 200)
                .build()
        })
    });
    PhoneStatus {
        running: running.is_some(),
        address,
        qr_code,
        problem,
    }
}

#[tauri::command]
pub async fn phone_status(app: AppHandle) -> Result<PhoneStatus, String> {
    Ok(status_of(&app, None))
}

/// Turns sending from a phone on or off, and remembers the choice.
#[tauri::command]
pub async fn set_phone(
    app: AppHandle,
    state: Managed<'_, AppState>,
    on: bool,
) -> Result<PhoneStatus, String> {
    {
        let mut settings = state.settings_snapshot();
        settings.phone_enabled = on;
        if settings.phone_token.len() != 64 {
            settings.phone_token = new_token();
        }
        state.replace_settings(settings)?;
    }
    if !on {
        stop(&app);
        return Ok(status_of(&app, None));
    }
    let problem = start(&app).await.err();
    if let (None, Some(running)) = (&problem, app.state::<Phone>().running().as_ref()) {
        // The same port next time, so a bookmark on the phone keeps working.
        let mut settings = state.settings_snapshot();
        settings.phone_port = running.address.port();
        state.replace_settings(settings)?;
    }
    Ok(status_of(&app, problem))
}

/// Makes a new pairing code, so phones paired before have to scan again.
#[tauri::command]
pub async fn new_phone_code(
    app: AppHandle,
    state: Managed<'_, AppState>,
) -> Result<PhoneStatus, String> {
    let was_running = app.state::<Phone>().running().is_some();
    stop(&app);
    let mut settings = state.settings_snapshot();
    settings.phone_token = new_token();
    state.replace_settings(settings)?;
    let problem = if was_running {
        start(&app).await.err()
    } else {
        None
    };
    Ok(status_of(&app, problem))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairing_codes_are_long_and_different() {
        let (first, second) = (new_token(), new_token());
        assert_eq!(first.len(), 64);
        assert!(first.chars().all(|character| character.is_ascii_hexdigit()));
        assert_ne!(first, second);
    }

    #[test]
    fn a_code_must_match_exactly() {
        let token = "a".repeat(64);
        assert!(same(&token, &token));
        assert!(!same(&"a".repeat(63), &token));
        assert!(!same(&format!("{}b", "a".repeat(63)), &token));
        assert!(!same("", &token));
    }

    #[test]
    fn only_home_network_addresses_count() {
        for home in ["192.168.1.20", "10.0.0.5", "172.20.3.4"] {
            assert!(is_home_network(home.parse().unwrap()), "{home}");
        }
        for not_home in ["8.8.8.8", "172.32.0.1", "127.0.0.1", "169.254.1.1"] {
            assert!(!is_home_network(not_home.parse().unwrap()), "{not_home}");
        }
    }

    #[test]
    fn only_phones_on_the_home_network_may_connect() {
        let home: Ipv4Addr = "192.168.1.20".parse().unwrap();
        assert!(peer_allowed("192.168.1.30".parse().unwrap(), home));
        assert!(!peer_allowed("8.8.8.8".parse().unwrap(), home));
        assert!(!peer_allowed("127.0.0.1".parse().unwrap(), home));
        assert!(!peer_allowed("::1".parse().unwrap(), home));
        // A test bound to this computer only takes this computer.
        assert!(peer_allowed(
            "127.0.0.1".parse().unwrap(),
            Ipv4Addr::LOCALHOST
        ));
    }

    #[test]
    fn too_many_requests_in_a_second_are_refused() {
        let limit = RateLimit::new();
        let allowed = (0..30).filter(|_| limit.allow(1000)).count();
        assert_eq!(allowed as u64, MAX_REQUESTS_PER_SECOND);
        assert!(limit.allow(1001), "a new second starts afresh");
    }
}
