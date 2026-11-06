//! Downloads arriving from outside the app: the browser, the clipboard and the command
//! line.
//!
//! Cookies stay here in Rust. The window only ever sees a token standing for a captured
//! file, so a problem in the web interface can't expose them.

use std::collections::HashMap;
use std::ffi::c_void;
use std::io;
use std::iter;
use std::mem;
use std::path::PathBuf;
use std::ptr;

use host::{pipe_name, read_frame, write_frame};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager};
use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};
use url::Url;
use windows_sys::Win32::Foundation::LocalFree;
use windows_sys::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows_sys::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};

use crate::commands::{AddRequest, AppState, add_download_now};

/// How many captures can wait for the user. Past this, the browser keeps its downloads,
/// so a page can't pile up dialogs.
pub const MAX_PENDING: usize = 50;

/// Full access for the pipe's owner and the system, and nobody else.
const OWNER_ONLY: &str = "D:P(A;;GA;;;OW)(A;;GA;;;SY)";

/// A file sent from outside the app.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Captured {
    pub url: String,
    pub referrer: Option<String>,
    pub user_agent: Option<String>,
    pub cookies: Option<String>,
    pub file_name: Option<String>,
    /// Only set from the command line.
    #[serde(skip)]
    pub folder: Option<PathBuf>,
}

impl Captured {
    pub fn is_valid(&self) -> bool {
        Url::parse(&self.url).is_ok_and(|url| matches!(url.scheme(), "http" | "https" | "ftp"))
    }

    /// The request headers a server may expect, because the browser would have sent them.
    pub fn headers(&self) -> Vec<(String, String)> {
        [
            ("Referer", &self.referrer),
            ("User-Agent", &self.user_agent),
            ("Cookie", &self.cookies),
        ]
        .into_iter()
        .filter_map(|(name, value)| {
            value
                .as_ref()
                .filter(|value| !value.is_empty())
                .map(|value| (name.to_owned(), value.clone()))
        })
        .collect()
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Request {
    Ping,
    Settings,
    Download(Captured),
    Batch { items: Vec<Captured> },
}

/// Captures waiting for the user to decide.
#[derive(Debug, Default)]
pub struct Pending {
    next: u64,
    waiting: HashMap<u64, Vec<Captured>>,
    window_ready: bool,
    /// Events from before the window was listening.
    undelivered: Vec<CaptureEvent>,
}

impl Pending {
    /// Holds files until the user decides. `None` means too many are already waiting.
    pub fn hold(&mut self, files: Vec<Captured>) -> Option<u64> {
        if self.waiting.len() >= MAX_PENDING {
            return None;
        }
        self.next += 1;
        self.waiting.insert(self.next, files);
        Some(self.next)
    }

    /// Returns an event if the window can take it now, and otherwise keeps it. The app
    /// may have been started just to receive this file, and by the time it's here the
    /// browser has already let go of its own copy, so losing it would lose the download.
    pub fn deliver(&mut self, event: CaptureEvent) -> Option<CaptureEvent> {
        if self.window_ready {
            return Some(event);
        }
        self.undelivered.push(event);
        None
    }

    /// Notes that the window is listening, and returns what arrived before it was.
    pub fn window_ready(&mut self) -> Vec<CaptureEvent> {
        self.window_ready = true;
        mem::take(&mut self.undelivered)
    }

    pub fn take(&mut self, token: u64) -> Option<Vec<Captured>> {
        self.waiting.remove(&token)
    }

    /// The headers of a held file, for checking it before the user decides.
    pub fn headers(&self, token: u64) -> Vec<(String, String)> {
        self.waiting
            .get(&token)
            .and_then(|files| files.first())
            .map(Captured::headers)
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureItem {
    url: String,
    file_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CaptureEvent {
    token: u64,
    items: Vec<CaptureItem>,
    /// Whether the user is choosing from several files.
    batch: bool,
}

/// Takes in files from outside the app. Returns whether they were accepted. If not, the
/// browser keeps the download.
pub fn receive(app: &AppHandle, files: Vec<Captured>, batch: bool) -> Result<(), String> {
    let files: Vec<Captured> = files.into_iter().filter(Captured::is_valid).collect();
    if files.is_empty() {
        return Err("there's nothing here that can be downloaded".to_owned());
    }

    let state = app.state::<AppState>();
    if !batch && !state.settings_snapshot().show_start_dialog {
        for file in files {
            let headers = file.headers();
            let request = AddRequest::from_captured(file);
            add_download_now(&state, request, headers)?;
        }
        return Ok(());
    }

    let items = files
        .iter()
        .map(|file| CaptureItem {
            url: file.url.clone(),
            file_name: file.file_name.clone(),
        })
        .collect();

    let event = {
        let mut pending = state.pending();
        let token = pending
            .hold(files)
            .ok_or("too many downloads are waiting")?;
        pending.deliver(CaptureEvent {
            token,
            items,
            batch,
        })
    };

    crate::show_main_window(app);
    match event {
        Some(event) => app
            .emit("capture", event)
            .map_err(|error| error.to_string()),
        None => Ok(()),
    }
}

/// What the extension needs in order to decide what to capture.
fn settings_reply(state: &AppState) -> Value {
    let settings = state.settings_snapshot();
    json!({
        "ok": true,
        "enabled": settings.capture_enabled,
        "fileTypes": settings.capture_types,
        "exclusions": settings.capture_exclusions,
        "forceKey": settings.force_key,
        "skipKey": settings.skip_key,
    })
}

fn answer(app: &AppHandle, message: &[u8]) -> Value {
    let request = match serde_json::from_slice::<Request>(message) {
        Ok(request) => request,
        Err(_) => return json!({ "ok": false, "error": "unreadable message" }),
    };

    let result = match request {
        Request::Ping => Ok(()),
        Request::Settings => return settings_reply(&app.state::<AppState>()),
        Request::Download(file) => receive(app, vec![file], false),
        Request::Batch { items } => receive(app, items, true),
    };

    match result {
        Ok(()) => json!({ "ok": true }),
        Err(error) => json!({ "ok": false, "error": error }),
    }
}

async fn serve_client(mut client: NamedPipeServer, app: AppHandle) {
    while let Ok(Some(message)) = read_frame(&mut client).await {
        let reply = answer(&app, &message).to_string();
        if write_frame(&mut client, reply.as_bytes()).await.is_err() {
            return;
        }
    }
}

/// Creates one instance of the pipe, open only to this user and refusing other machines.
fn create_pipe(name: &str, first: bool) -> io::Result<NamedPipeServer> {
    let description: Vec<u16> = OWNER_ONLY.encode_utf16().chain(iter::once(0)).collect();
    let mut descriptor: PSECURITY_DESCRIPTOR = ptr::null_mut();

    // SAFETY: the text is null-terminated, and `descriptor` receives memory that's
    // released below.
    let converted = unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            description.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            ptr::null_mut(),
        )
    };
    if converted == 0 {
        return Err(io::Error::last_os_error());
    }

    let mut attributes = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor,
        bInheritHandle: 0,
    };

    // SAFETY: the attributes and the descriptor they point to outlive this call.
    let pipe = unsafe {
        ServerOptions::new()
            .first_pipe_instance(first)
            .reject_remote_clients(true)
            .create_with_security_attributes_raw(name, (&raw mut attributes).cast::<c_void>())
    };

    // SAFETY: `descriptor` came from the conversion above and isn't used after this.
    unsafe {
        LocalFree(descriptor.cast());
    }
    pipe
}

/// Listens for the browser for as long as the app runs.
pub fn serve(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let name = pipe_name();
        // This fails if another copy already owns the pipe, which is fine.
        let Ok(mut server) = create_pipe(&name, true) else {
            return;
        };

        loop {
            if server.connect().await.is_err() {
                continue;
            }
            let client = server;
            server = match create_pipe(&name, false) {
                Ok(next) => next,
                Err(_) => return,
            };
            tauri::async_runtime::spawn(serve_client(client, app.clone()));
        }
    });
}

#[cfg(test)]
mod tests {
    use tokio::net::windows::named_pipe::ClientOptions;

    use super::*;

    fn file(url: &str) -> Captured {
        Captured {
            url: url.to_owned(),
            ..Captured::default()
        }
    }

    #[test]
    fn reads_a_message_from_the_extension() {
        let message = br#"{ "type": "download", "url": "https://x.test/a.zip",
            "referrer": "https://x.test/", "userAgent": "Browser", "cookies": "a=1",
            "fileName": "a.zip", "size": 100, "mime": "application/zip" }"#;

        match serde_json::from_slice::<Request>(message).unwrap() {
            Request::Download(file) => {
                assert_eq!(file.url, "https://x.test/a.zip");
                assert_eq!(file.cookies.as_deref(), Some("a=1"));
            }
            other => panic!("read as {other:?}"),
        }
    }

    #[test]
    fn a_page_cannot_set_the_download_folder() {
        // The folder is only for the command line, so a message can't choose it.
        let message = br#"{ "type": "download", "url": "https://x.test/a.zip",
            "folder": "C:\\Windows\\System32" }"#;

        match serde_json::from_slice::<Request>(message).unwrap() {
            Request::Download(file) => assert_eq!(file.folder, None),
            other => panic!("read as {other:?}"),
        }
    }

    #[test]
    fn unreadable_messages_are_refused() {
        assert!(serde_json::from_slice::<Request>(b"not json").is_err());
        assert!(serde_json::from_slice::<Request>(br#"{"type":"unknown"}"#).is_err());
        assert!(serde_json::from_slice::<Request>(br#"{"type":"download"}"#).is_err());
    }

    #[test]
    fn only_downloadable_addresses_are_accepted() {
        assert!(file("https://x.test/a.zip").is_valid());
        assert!(file("ftp://x.test/a.zip").is_valid());
        assert!(!file("file:///C:/Windows/win.ini").is_valid());
        assert!(!file("javascript:alert(1)").is_valid());
        assert!(!file("not a url").is_valid());
    }

    #[test]
    fn sends_what_the_browser_would_have_sent() {
        let captured = Captured {
            referrer: Some("https://x.test/".to_owned()),
            user_agent: Some("Browser".to_owned()),
            cookies: Some("a=1".to_owned()),
            ..file("https://x.test/a.zip")
        };

        let names: Vec<String> = captured
            .headers()
            .into_iter()
            .map(|(name, _)| name)
            .collect();

        assert_eq!(names, ["Referer", "User-Agent", "Cookie"]);
    }

    #[test]
    fn empty_values_are_left_out() {
        let captured = Captured {
            cookies: Some(String::new()),
            ..file("https://x.test/a.zip")
        };

        assert!(captured.headers().is_empty());
    }

    #[test]
    fn captures_wait_until_taken() {
        let mut pending = Pending::default();
        let token = pending.hold(vec![file("https://x.test/a")]).unwrap();

        assert_eq!(pending.take(token).unwrap().len(), 1);
        assert!(pending.take(token).is_none(), "a token only works once");
    }

    fn event(token: u64) -> CaptureEvent {
        CaptureEvent {
            token,
            items: Vec::new(),
            batch: false,
        }
    }

    #[test]
    fn a_capture_from_before_the_window_was_listening_is_not_lost() {
        let mut pending = Pending::default();

        assert!(pending.deliver(event(1)).is_none());
        assert!(pending.deliver(event(2)).is_none());

        let tokens: Vec<u64> = pending
            .window_ready()
            .iter()
            .map(|event| event.token)
            .collect();
        assert_eq!(tokens, [1, 2]);
    }

    #[test]
    fn captures_go_straight_to_a_listening_window_and_are_not_repeated() {
        let mut pending = Pending::default();
        pending.deliver(event(1));
        pending.window_ready();

        assert_eq!(pending.deliver(event(2)).map(|event| event.token), Some(2));
        assert!(pending.window_ready().is_empty());
    }

    #[test]
    fn a_page_cannot_pile_up_endless_captures() {
        let mut pending = Pending::default();
        for _ in 0..MAX_PENDING {
            assert!(pending.hold(vec![file("https://x.test/a")]).is_some());
        }

        assert!(pending.hold(vec![file("https://x.test/a")]).is_none());
    }

    #[test]
    fn a_held_files_headers_can_be_read_without_taking_it() {
        let mut pending = Pending::default();
        let captured = Captured {
            cookies: Some("a=1".to_owned()),
            ..file("https://x.test/a")
        };
        let token = pending.hold(vec![captured]).unwrap();

        assert_eq!(pending.headers(token).len(), 1);
        assert!(pending.take(token).is_some());
        assert!(pending.headers(token).is_empty());
    }

    #[tokio::test]
    async fn this_user_can_connect_to_the_restricted_pipe() {
        let name = format!(r"\\.\pipe\tributary-test-capture-{}", std::process::id());
        let server = create_pipe(&name, true).unwrap();

        let connecting = server.connect();
        let client = ClientOptions::new().open(&name);

        assert!(client.is_ok(), "{:?}", client.err());
        connecting.await.unwrap();
    }

    #[tokio::test]
    async fn a_second_owner_of_the_pipe_is_refused() {
        let name = format!(r"\\.\pipe\tributary-test-owner-{}", std::process::id());
        let _first = create_pipe(&name, true).unwrap();

        assert!(create_pipe(&name, true).is_err());
    }
}
