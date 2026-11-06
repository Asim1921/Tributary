//! Started by the browser. Passes each message from the extension to the app, starting
//! the app if it isn't running, and passes the answer back.
//!
//! Stays out of the way of the protocol: stdin and stdout belong to the browser, so
//! nothing else may ever be written to stdout.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::io;
use std::process::Command;
use std::time::{Duration, Instant};

use host::{pipe_name, read_frame, write_frame};
use serde_json::json;
use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeClient};

const APP_FILE: &str = "tributary.exe";
const MINIMIZED_FLAG: &str = "--minimized";
const APP_START_TIMEOUT: Duration = Duration::from_secs(8);
const RETRY_INTERVAL: Duration = Duration::from_millis(50);

/// The Windows error for a pipe with no free instance.
const ERROR_PIPE_BUSY: i32 = 231;

/// Starts the app that was installed alongside this program.
fn start_app() -> io::Result<()> {
    let path = std::env::current_exe()?
        .parent()
        .ok_or(io::ErrorKind::NotFound)?
        .join(APP_FILE);
    Command::new(path).arg(MINIMIZED_FLAG).spawn().map(drop)
}

async fn connect() -> io::Result<NamedPipeClient> {
    let name = pipe_name();
    let mut started = false;
    let deadline = Instant::now() + APP_START_TIMEOUT;

    loop {
        match ClientOptions::new().open(&name) {
            Ok(client) => return Ok(client),
            Err(error) if Instant::now() >= deadline => return Err(error),
            // Busy means the app is there but between connections, so it only needs a
            // moment. Anything else means it isn't running.
            Err(error) if error.raw_os_error() == Some(ERROR_PIPE_BUSY) => {}
            Err(_) if !started => {
                start_app()?;
                started = true;
            }
            Err(_) => {}
        }
        tokio::time::sleep(RETRY_INTERVAL).await;
    }
}

async fn forward(message: &[u8]) -> io::Result<Vec<u8>> {
    let mut client = connect().await?;
    write_frame(&mut client, message).await?;
    read_frame(&mut client)
        .await?
        .ok_or_else(|| io::ErrorKind::UnexpectedEof.into())
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let mut stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();

    // The browser closes stdin when it's finished with this program.
    while let Ok(Some(message)) = read_frame(&mut stdin).await {
        let reply = forward(&message).await.unwrap_or_else(|error| {
            json!({ "ok": false, "error": error.to_string() })
                .to_string()
                .into_bytes()
        });

        if write_frame(&mut stdout, &reply).await.is_err() {
            return;
        }
    }
}
