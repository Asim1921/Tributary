//! A small command-line front end to the engine.

use std::path::PathBuf;
use std::process::ExitCode;

use engine::{Config, Download, DownloadId, Engine, Error, Event, NewDownload, State};
use tokio::sync::broadcast::error::RecvError;

const USAGE: &str =
    "usage: tributary-fetch get URL FOLDER DATABASE\n       tributary-fetch resume ID DATABASE";

enum Command {
    Get {
        url: String,
        folder: PathBuf,
        database: PathBuf,
    },
    Resume {
        id: DownloadId,
        database: PathBuf,
    },
}

fn parse_args(args: &[String]) -> Option<Command> {
    match args {
        [command, url, folder, database] if command == "get" => Some(Command::Get {
            url: url.clone(),
            folder: folder.into(),
            database: database.into(),
        }),
        [command, id, database] if command == "resume" => Some(Command::Resume {
            id: DownloadId(id.parse().ok()?),
            database: database.into(),
        }),
        _ => None,
    }
}

async fn run(command: Command) -> Result<Download, Error> {
    match command {
        Command::Get {
            url,
            folder,
            database,
        } => {
            let engine = Engine::open(&database, Config::default())?;
            let id = engine.add(NewDownload {
                url,
                dest_dir: folder,
                ..NewDownload::default()
            })?;
            follow(&engine, id).await
        }
        Command::Resume { id, database } => {
            let engine = Engine::open(&database, Config::default())?;
            engine.resume(id, &[])?;
            follow(&engine, id).await
        }
    }
}

/// Prints progress until the download ends, or pauses it on Ctrl+C.
async fn follow(engine: &Engine, id: DownloadId) -> Result<Download, Error> {
    let mut events = engine.subscribe();
    println!("id {}", id.0);

    loop {
        if engine.get(id)?.state != State::Running {
            return engine.get(id);
        }

        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                engine.pause(id).await?;
                return engine.get(id);
            }
            event = events.recv() => match event {
                Ok(Event::Progress { downloaded, total, bytes_per_second, .. }) => {
                    let megabytes = |bytes: u64| bytes as f64 / 1_048_576.0;
                    match total {
                        Some(total) => println!(
                            "{:.1} of {:.1} MiB at {:.1} MiB/s",
                            megabytes(downloaded),
                            megabytes(total),
                            megabytes(bytes_per_second)
                        ),
                        None => println!(
                            "{:.1} MiB at {:.1} MiB/s",
                            megabytes(downloaded),
                            megabytes(bytes_per_second)
                        ),
                    }
                }
                Ok(Event::State { .. } | Event::Queues | Event::QueueFinished { .. }) | Err(RecvError::Lagged(_)) => {}
                Err(RecvError::Closed) => return engine.get(id),
            },
        }
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(command) = parse_args(&args) else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };

    match run(command).await {
        Ok(download) => match download.state {
            State::Completed => {
                let path = download.final_path.unwrap_or_default();
                println!("completed {}", path.display());
                ExitCode::SUCCESS
            }
            State::Failed => {
                eprintln!("failed: {}", download.error.unwrap_or_default());
                ExitCode::FAILURE
            }
            State::Paused | State::Running => {
                println!("paused at {} bytes", download.downloaded);
                ExitCode::SUCCESS
            }
        },
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
