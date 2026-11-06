//! Runs the test server until Ctrl+C, or prints the checksum a test file should have.

use std::path::PathBuf;
use std::process::ExitCode;

use testserver::scenario::Scenario;
use testserver::{Config, FtpConfig, FtpServer, TestServer, content, streams};

const USAGE: &str = "usage: testserver [--port N] [--log PATH] [--files FOLDER]\n       \
                     testserver sha256 NAME [SEED]\n       testserver sha256-track NAME";

/// Printed so a test can find the FTP server, which uses a free port.
const FTP_LINE: &str = "ftp on";
const DEFAULT_PORT: u16 = 8787;

/// Returns the SHA-256 of a test file, such as `100M.bin` with seed 1.
fn checksum(mut args: impl Iterator<Item = String>) -> Result<String, String> {
    let name = args
        .next()
        .ok_or("sha256 needs a file name, such as 100M.bin")?;
    let query = args
        .next()
        .map(|seed| format!("seed={seed}"))
        .unwrap_or_default();
    let scenario = Scenario::parse(&name, &query)?;
    Ok(content::sha256(scenario.seed, scenario.size))
}

/// Returns the SHA-256 of a stream's parts joined together, such as `low`.
fn track_checksum(mut args: impl Iterator<Item = String>) -> Result<String, String> {
    let name = args.next().ok_or("sha256-track needs a track name")?;
    let track = streams::track(&name).ok_or(format!("unknown track `{name}`"))?;
    Ok(streams::sha256(&track))
}

fn parse_args(mut args: impl Iterator<Item = String>) -> Result<Config, String> {
    let mut config = Config {
        port: DEFAULT_PORT,
        ..Config::default()
    };

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--port" => {
                let value = args.next().ok_or("--port needs a value")?;
                config.port = value
                    .parse()
                    .map_err(|_| format!("invalid port `{value}`"))?;
            }
            "--log" => {
                config.log_path = Some(PathBuf::from(args.next().ok_or("--log needs a path")?));
            }
            "--files" => {
                let folder = args.next().ok_or("--files needs a folder")?;
                config.static_dir = Some(PathBuf::from(folder));
            }
            other => return Err(format!("unknown argument `{other}`")),
        }
    }
    Ok(config)
}

#[tokio::main]
async fn main() -> ExitCode {
    let mut args = std::env::args().skip(1).peekable();
    let asked = args.peek().cloned().unwrap_or_default();
    if asked == "sha256" || asked == "sha256-track" {
        let answer = if asked == "sha256" {
            checksum(args.skip(1))
        } else {
            track_checksum(args.skip(1))
        };
        return match answer {
            Ok(digest) => {
                println!("{digest}");
                ExitCode::SUCCESS
            }
            Err(message) => {
                eprintln!("{message}\n{USAGE}");
                ExitCode::from(2)
            }
        };
    }

    let config = match parse_args(args) {
        Ok(config) => config,
        Err(message) => {
            eprintln!("{message}\n{USAGE}");
            return ExitCode::from(2);
        }
    };

    let server = match TestServer::start(config).await {
        Ok(server) => server,
        Err(error) => {
            eprintln!("could not start: {error}");
            return ExitCode::FAILURE;
        }
    };

    let ftp = match FtpServer::start(FtpConfig::default()).await {
        Ok(ftp) => ftp,
        Err(error) => {
            eprintln!("could not start FTP: {error}");
            return ExitCode::FAILURE;
        }
    };

    println!("listening on {}", server.url("/"));
    println!("{FTP_LINE} {}", ftp.url("/"));
    match tokio::signal::ctrl_c().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("could not wait for Ctrl+C: {error}");
            ExitCode::FAILURE
        }
    }
}
