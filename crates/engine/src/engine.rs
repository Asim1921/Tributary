//! The public face of the engine: adding, pausing, resuming and listing downloads.

use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use reqwest::header::{ACCEPT_ENCODING, HeaderMap, HeaderName, HeaderValue};
use reqwest::{Client, Url, redirect};
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tokio::time::MissedTickBehavior;
use tokio_util::sync::CancellationToken;

use crate::disk::{self, PartFile};
use crate::error::Error;
use crate::name;
use crate::remote::{self, Problem, Remote};
use crate::segments::SegmentMap;
use crate::store::{Download, DownloadId, State, Store};
use crate::transfer::{self, Ended, Job, RetryPolicy};

const USER_AGENT: &str = concat!("Tributary/", env!("CARGO_PKG_VERSION"));
const MAX_REDIRECTS: usize = 10;
const EVENT_BUFFER: usize = 1024;

/// How many times a download restarts because the server's file changed.
const MAX_RESTARTS: u32 = 3;

/// How much each new measurement moves the reported speed.
const SPEED_SMOOTHING: f64 = 0.3;

/// Headers that must never be saved. The caller supplies them again to resume.
const SECRET_HEADERS: [&str; 3] = ["cookie", "authorization", "proxy-authorization"];

#[derive(Debug, Clone)]
pub struct Config {
    /// Connections per download.
    pub connections: usize,
    /// The smallest piece a file is split into.
    pub min_split: u64,
    pub retry: RetryPolicy,
    pub connect_timeout: Duration,
    /// A connection that delivers nothing for this long counts as failed.
    pub read_timeout: Duration,
    /// How often progress is made safe on disk. A crash loses at most about this much.
    pub checkpoint_interval: Duration,
    pub progress_interval: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            connections: 8,
            min_split: 1 << 20,
            retry: RetryPolicy {
                attempts: 6,
                base_delay: Duration::from_secs(1),
                max_delay: Duration::from_secs(30),
            },
            connect_timeout: Duration::from_secs(15),
            read_timeout: Duration::from_secs(30),
            checkpoint_interval: Duration::from_secs(1),
            progress_interval: Duration::from_millis(250),
        }
    }
}

/// A download to start.
#[derive(Debug, Clone, Default)]
pub struct NewDownload {
    pub url: String,
    pub dest_dir: PathBuf,
    /// Overrides the name from the server.
    pub file_name: Option<String>,
    /// Extra request headers. `Cookie` and similar are used but never saved.
    pub headers: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    State {
        id: DownloadId,
        state: State,
    },
    Progress {
        id: DownloadId,
        downloaded: u64,
        total: Option<u64>,
        bytes_per_second: u64,
    },
}

struct Running {
    stop: CancellationToken,
    task: JoinHandle<()>,
}

struct Inner {
    store: Arc<Store>,
    client: Client,
    config: Config,
    events: broadcast::Sender<Event>,
    running: Mutex<HashMap<DownloadId, Running>>,
}

impl Inner {
    fn running(&self) -> MutexGuard<'_, HashMap<DownloadId, Running>> {
        self.running.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn emit(&self, event: Event) {
        // Having nobody listening is normal.
        let _ = self.events.send(event);
    }
}

/// Runs downloads and remembers them. Must be used inside a Tokio runtime.
pub struct Engine {
    inner: Arc<Inner>,
}

impl Engine {
    /// Opens the database at `path`. Downloads interrupted by a crash come back paused.
    pub fn open(path: &Path, config: Config) -> Result<Self, Error> {
        let mut default_headers = HeaderMap::new();
        // Ranges count bytes of the stored file, so compression must stay off.
        default_headers.insert(ACCEPT_ENCODING, HeaderValue::from_static("identity"));

        let client = Client::builder()
            .user_agent(USER_AGENT)
            .default_headers(default_headers)
            .redirect(redirect::Policy::limited(MAX_REDIRECTS))
            .connect_timeout(config.connect_timeout)
            .read_timeout(config.read_timeout)
            .build()?;

        let (events, _) = broadcast::channel(EVENT_BUFFER);
        Ok(Self {
            inner: Arc::new(Inner {
                store: Arc::new(Store::open(path)?),
                client,
                config,
                events,
                running: Mutex::default(),
            }),
        })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.inner.events.subscribe()
    }

    /// Saves a download and starts it.
    pub fn add(&self, new: NewDownload) -> Result<DownloadId, Error> {
        let url = Url::parse(&new.url).map_err(|_| Error::InvalidUrl(new.url.clone()))?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err(Error::InvalidUrl(new.url));
        }

        let (secret, saved): (Vec<_>, Vec<_>) = new
            .headers
            .into_iter()
            .partition(|(name, _)| is_secret(name));
        let secret = to_header_map(&secret)?;
        to_header_map(&saved)?;

        let file_name = new.file_name.as_deref().map(name::sanitize);
        let id =
            self.inner
                .store
                .insert(url.as_str(), &new.dest_dir, file_name.as_deref(), &saved)?;

        self.start(id, secret)?;
        Ok(id)
    }

    /// Stops a running download, keeping its progress. Does nothing if it isn't running.
    pub async fn pause(&self, id: DownloadId) -> Result<(), Error> {
        let running = self.inner.running().remove(&id);
        if let Some(running) = running {
            running.stop.cancel();
            let _ = running.task.await;
        }
        Ok(())
    }

    /// Continues a paused or failed download. Secret headers aren't saved, so a
    /// download that needs them must be given them again.
    pub fn resume(&self, id: DownloadId, secret_headers: &[(String, String)]) -> Result<(), Error> {
        let download = self.get(id)?;
        if download.state == State::Completed {
            return Err(Error::AlreadyCompleted(id));
        }
        self.start(id, to_header_map(secret_headers)?)
    }

    /// Forgets a download and deletes its unfinished `.part` file. A completed file stays.
    pub async fn remove(&self, id: DownloadId) -> Result<(), Error> {
        self.pause(id).await?;
        let download = self.get(id)?;

        if let Some(file_name) = &download.file_name {
            let path = part_path(&download.dest_dir, file_name, id);
            match std::fs::remove_file(path) {
                Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                    return Err(error.into());
                }
                _ => {}
            }
        }
        self.inner.store.delete(id)?;
        Ok(())
    }

    pub fn get(&self, id: DownloadId) -> Result<Download, Error> {
        self.inner.store.get(id)?.ok_or(Error::NotFound(id))
    }

    pub fn list(&self) -> Result<Vec<Download>, Error> {
        Ok(self.inner.store.list()?)
    }

    /// Waits until a download is no longer running and returns it.
    pub async fn wait(&self, id: DownloadId) -> Result<Download, Error> {
        let mut events = self.subscribe();
        loop {
            let download = self.get(id)?;
            if download.state != State::Running {
                return Ok(download);
            }
            if let Err(broadcast::error::RecvError::Closed) = events.recv().await {
                return self.get(id);
            }
        }
    }

    fn start(&self, id: DownloadId, secret_headers: HeaderMap) -> Result<(), Error> {
        let mut running = self.inner.running();
        if running.contains_key(&id) {
            return Err(Error::AlreadyRunning(id));
        }

        self.inner.store.set_state(id, State::Running, None)?;
        let stop = CancellationToken::new();
        let task = tokio::spawn(run(
            Arc::clone(&self.inner),
            id,
            secret_headers,
            stop.clone(),
        ));
        running.insert(id, Running { stop, task });

        self.inner.emit(Event::State {
            id,
            state: State::Running,
        });
        Ok(())
    }
}

fn is_secret(name: &str) -> bool {
    SECRET_HEADERS
        .iter()
        .any(|secret| name.eq_ignore_ascii_case(secret))
}

fn to_header_map(pairs: &[(String, String)]) -> Result<HeaderMap, Error> {
    let mut headers = HeaderMap::new();
    for (name, value) in pairs {
        let invalid = || Error::InvalidHeader(name.clone());
        let mut header_value = HeaderValue::from_str(value).map_err(|_| invalid())?;
        header_value.set_sensitive(is_secret(name));
        let header_name = HeaderName::from_bytes(name.as_bytes()).map_err(|_| invalid())?;
        headers.append(header_name, header_value);
    }
    Ok(headers)
}

fn part_path(dest_dir: &Path, file_name: &str, id: DownloadId) -> PathBuf {
    dest_dir.join(format!("{file_name}.{}.part", id.0))
}

/// How one run of a download ended, once restarts are out of the way.
enum Finish {
    Completed { path: PathBuf, size: u64 },
    Paused,
}

enum Attempt {
    Done(Finish),
    FileChanged,
}

/// Runs a download to its end and records the result.
async fn run(inner: Arc<Inner>, id: DownloadId, secrets: HeaderMap, stop: CancellationToken) {
    let result = drive(&inner, id, &secrets, &stop).await;

    let saved = match &result {
        Ok(Finish::Completed { path, size }) => inner
            .store
            .set_completed(id, path, *size)
            .map(|()| State::Completed),
        Ok(Finish::Paused) => inner
            .store
            .set_state(id, State::Paused, None)
            .map(|()| State::Paused),
        Err(message) => inner
            .store
            .set_state(id, State::Failed, Some(message))
            .map(|()| State::Failed),
    };

    inner.running().remove(&id);
    if let Ok(state) = saved {
        inner.emit(Event::State { id, state });
    }
}

async fn drive(
    inner: &Inner,
    id: DownloadId,
    secrets: &HeaderMap,
    stop: &CancellationToken,
) -> Result<Finish, String> {
    for _ in 0..=MAX_RESTARTS {
        match attempt(inner, id, secrets, stop).await? {
            Attempt::Done(finish) => return Ok(finish),
            Attempt::FileChanged => {
                inner
                    .store
                    .save_segments(id, &[])
                    .map_err(|error| error.to_string())?;
            }
        }
    }
    Err("the file kept changing on the server".to_owned())
}

/// Asks the server about the file, retrying temporary failures.
async fn probe(
    inner: &Inner,
    url: &str,
    headers: &HeaderMap,
    stop: &CancellationToken,
) -> Result<Remote, Problem> {
    let policy = inner.config.retry;
    let mut failures = 0;

    loop {
        let result = tokio::select! {
            () = stop.cancelled() => return Err(Problem::Stopped),
            result = remote::probe(&inner.client, url, headers) => result,
        };

        let (reason, wait) = match result {
            Err(Problem::Retry { reason, wait }) => (reason, wait),
            other => return other,
        };

        failures += 1;
        if failures > policy.attempts {
            return Err(Problem::Fatal(reason));
        }
        tokio::select! {
            () = stop.cancelled() => return Err(Problem::Stopped),
            () = tokio::time::sleep(wait.unwrap_or(policy.delay(failures))) => {}
        }
    }
}

/// One pass at a download: checks the server, then transfers.
async fn attempt(
    inner: &Inner,
    id: DownloadId,
    secrets: &HeaderMap,
    stop: &CancellationToken,
) -> Result<Attempt, String> {
    let download = inner
        .store
        .get(id)
        .map_err(|error| error.to_string())?
        .ok_or("the download no longer exists")?;

    let mut headers = to_header_map(&download.headers).map_err(|error| error.to_string())?;
    headers.extend(secrets.clone());

    let remote = match probe(inner, &download.url, &headers, stop).await {
        Ok(remote) => remote,
        Err(Problem::Stopped) => return Ok(Attempt::Done(Finish::Paused)),
        Err(Problem::Fatal(reason) | Problem::Retry { reason, .. }) => return Err(reason),
        Err(Problem::Changed) => return Ok(Attempt::FileChanged),
    };

    let file_name = match &download.file_name {
        Some(file_name) => file_name.clone(),
        None => {
            let suggested = remote.suggested_name.as_deref().unwrap_or_default();
            let file_name = name::sanitize(suggested);
            inner
                .store
                .set_name(id, &file_name)
                .map_err(|error| error.to_string())?;
            file_name
        }
    };

    std::fs::create_dir_all(&download.dest_dir)
        .map_err(|error| format!("could not create the folder: {error}"))?;
    let path = part_path(&download.dest_dir, &file_name, id);

    let segmented = remote.ranged && remote.size.is_some_and(|size| size > 0);
    let resuming =
        segmented && download.downloaded > 0 && remote.same_file(&download) && path.exists();

    let min_split = inner.config.min_split;
    let (file, map) = if resuming {
        let saved = inner
            .store
            .segments(id)
            .map_err(|error| error.to_string())?;
        let file = PartFile::open(&path);
        (file, SegmentMap::from_saved(&saved, min_split))
    } else {
        let size = remote.size.unwrap_or(0);
        let map = SegmentMap::new(size, inner.config.connections, min_split);
        let planned = if segmented {
            map.snapshot()
        } else {
            Vec::new()
        };
        inner
            .store
            .begin_file(id, &remote, &planned)
            .map_err(|error| error.to_string())?;
        (PartFile::create(&path, remote.size), map)
    };
    let file = file.map_err(|error| format!("could not open the file: {error}"))?;

    let job = Arc::new(Job {
        client: inner.client.clone(),
        url: remote.url.clone(),
        headers,
        if_range: remote.if_range().map(str::to_owned),
        etag: remote.etag.clone(),
        size: remote.size,
        file: Arc::new(file),
        transferred: AtomicU64::new(map.total_written()),
        map: Mutex::new(map),
        stop: stop.child_token(),
        policy: inner.config.retry,
    });

    let transfer: Pin<Box<dyn Future<Output = Ended> + Send>> = if segmented {
        Box::pin(transfer::run_segments(
            Arc::clone(&job),
            inner.config.connections,
        ))
    } else {
        Box::pin(transfer::run_stream(Arc::clone(&job)))
    };

    let ended = watch(inner, id, &job, segmented, transfer).await;
    if segmented {
        checkpoint(inner, id, &job).await?;
    }

    let transferred = job.transferred.load(Ordering::Relaxed);
    drop(job);

    match ended {
        Ended::Finished => {
            let target = disk::finish(&path, &download.dest_dir, &file_name)
                .map_err(|error| format!("could not finish the file: {error}"))?;
            Ok(Attempt::Done(Finish::Completed {
                path: target,
                size: remote.size.unwrap_or(transferred),
            }))
        }
        Ended::Stopped => Ok(Attempt::Done(Finish::Paused)),
        Ended::Changed => Ok(Attempt::FileChanged),
        Ended::Failed(reason) => Err(reason),
    }
}

/// Reports progress and saves checkpoints until the transfer ends.
async fn watch(
    inner: &Inner,
    id: DownloadId,
    job: &Job,
    segmented: bool,
    mut transfer: Pin<Box<dyn Future<Output = Ended> + Send>>,
) -> Ended {
    let mut ticker = tokio::time::interval(inner.config.progress_interval);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

    let mut last_checkpoint = Instant::now();
    let mut last_tick = Instant::now();
    let mut last_bytes = job.transferred.load(Ordering::Relaxed);
    let mut speed = 0.0;

    loop {
        tokio::select! {
            ended = &mut transfer => return ended,
            _ = ticker.tick() => {
                let bytes = job.transferred.load(Ordering::Relaxed);
                let elapsed = last_tick.elapsed().as_secs_f64().max(0.001);
                let measured = bytes.saturating_sub(last_bytes) as f64 / elapsed;
                speed += SPEED_SMOOTHING * (measured - speed);
                last_tick = Instant::now();
                last_bytes = bytes;

                inner.emit(Event::Progress {
                    id,
                    downloaded: bytes,
                    total: job.size,
                    bytes_per_second: speed as u64,
                });

                if segmented && last_checkpoint.elapsed() >= inner.config.checkpoint_interval {
                    // A missed checkpoint only means a little more to fetch after a
                    // crash, and the next one tries again.
                    let _ = checkpoint(inner, id, job).await;
                    last_checkpoint = Instant::now();
                }
            }
        }
    }
}

/// Makes progress safe. The order matters: the snapshot is taken first, so everything
/// in it was written before the flush, and it's saved only after the flush succeeds.
/// The database can then never claim bytes the file doesn't have.
async fn checkpoint(inner: &Inner, id: DownloadId, job: &Job) -> Result<(), String> {
    let snapshot = job.map().snapshot();
    let store = Arc::clone(&inner.store);
    let file = Arc::clone(&job.file);

    tokio::task::spawn_blocking(move || {
        file.sync()
            .map_err(|error| format!("could not flush the file: {error}"))?;
        store
            .save_segments(id, &snapshot)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())?
}
