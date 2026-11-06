//! Runs the real host program the way a browser does: over stdin and stdout, with a
//! pretend app on the other end of the pipe.

use std::process::Stdio;
use std::time::Duration;

use host::{read_frame, write_frame};
use serde_json::Value;
use tokio::net::windows::named_pipe::ServerOptions;
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

fn unique_pipe(label: &str) -> String {
    format!(r"\\.\pipe\tributary-test-{label}-{}", std::process::id())
}

struct Browser {
    child: Child,
    stdin: ChildStdin,
    stdout: ChildStdout,
}

fn start_host(pipe: &str) -> Browser {
    let mut child = Command::new(env!("CARGO_BIN_EXE_tributary-host"))
        .env("TRIBUTARY_PIPE", pipe)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();

    let stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    Browser {
        child,
        stdin,
        stdout,
    }
}

impl Browser {
    async fn ask(&mut self, message: &[u8]) -> Value {
        write_frame(&mut self.stdin, message).await.unwrap();
        let reply = read_frame(&mut self.stdout).await.unwrap().unwrap();
        serde_json::from_slice(&reply).unwrap()
    }
}

/// A pretend app that answers each message by echoing it back inside `got`.
fn start_app(pipe: &str, messages: usize) {
    let mut server = ServerOptions::new()
        .first_pipe_instance(true)
        .create(pipe)
        .unwrap();
    let name = pipe.to_owned();

    tokio::spawn(async move {
        for _ in 0..messages {
            server.connect().await.unwrap();
            let mut client = server;
            server = ServerOptions::new().create(&name).unwrap();

            let message = read_frame(&mut client).await.unwrap().unwrap();
            let got: Value = serde_json::from_slice(&message).unwrap();
            let reply = serde_json::json!({ "ok": true, "got": got }).to_string();
            write_frame(&mut client, reply.as_bytes()).await.unwrap();
        }
    });
}

#[tokio::test(flavor = "multi_thread")]
async fn a_message_reaches_the_app_and_the_answer_comes_back() {
    let pipe = unique_pipe("basic");
    start_app(&pipe, 1);
    let mut browser = start_host(&pipe);

    let reply = browser.ask(br#"{"type":"ping"}"#).await;

    assert_eq!(reply["ok"], true);
    assert_eq!(reply["got"]["type"], "ping");
}

#[tokio::test(flavor = "multi_thread")]
async fn several_messages_work_in_a_row() {
    let pipe = unique_pipe("several");
    start_app(&pipe, 3);
    let mut browser = start_host(&pipe);

    for number in 0..3 {
        let message = format!(r#"{{"number":{number}}}"#);
        let reply = browser.ask(message.as_bytes()).await;
        assert_eq!(reply["got"]["number"], number);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn no_app_gives_a_clear_failure_so_the_browser_keeps_the_download() {
    // There's no app beside the test program, and nothing on this pipe.
    let pipe = unique_pipe("missing");
    let mut browser = start_host(&pipe);

    let reply = tokio::time::timeout(Duration::from_secs(15), browser.ask(br#"{"type":"ping"}"#))
        .await
        .expect("the host must answer rather than hang");

    assert_eq!(reply["ok"], false);
    assert!(reply["error"].is_string());
}

#[tokio::test(flavor = "multi_thread")]
async fn the_host_exits_when_the_browser_closes_it() {
    let pipe = unique_pipe("close");
    let browser = start_host(&pipe);
    let Browser {
        mut child,
        stdin,
        stdout,
    } = browser;

    drop(stdin);
    drop(stdout);

    let status = tokio::time::timeout(Duration::from_secs(5), child.wait()).await;
    assert!(status.is_ok(), "the host should not linger");
}
