//! Downloads a stream: many small parts that join up into one file.
//!
//! Parts are fetched at the same time but always written in order, so the file only ever
//! holds a finished beginning. Progress is then simply how many parts are safely on disk,
//! and resuming cuts the file back to that point and carries on.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use media::Part;
use reqwest::header::{CONTENT_TYPE, HeaderMap, RANGE};
use reqwest::{Client, Response, StatusCode};
use tokio::sync::Notify;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use crate::auth::Authenticator;
use crate::disk::PartFile;
use crate::limits::Limits;
use crate::remote::{LOGIN_NEEDED, Problem, authorize, text};
use crate::transfer::{Ended, RetryPolicy};

/// Connections per stream. Fewer than for a file, because each holds a whole part in
/// memory until its turn to be written.
pub const MAX_CONNECTIONS: usize = 4;

/// How many parts past the next one to write may be fetched. This limits what waits in
/// memory behind one slow part.
const PARTS_AHEAD: usize = MAX_CONNECTIONS + 2;

/// Far more than a real part. A larger one is refused rather than held in memory.
const MAX_PART_BYTES: usize = 64 << 20;

/// How much of a stream is written, in parts and in bytes.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub parts: usize,
    pub bytes: u64,
}

#[derive(Default)]
struct Joined {
    position: Position,
    next_to_claim: usize,
    /// Finished parts waiting for the ones before them.
    ready: BTreeMap<usize, Vec<u8>>,
}

/// What a stream's transfer needs from the engine.
pub struct Setup {
    pub client: Client,
    pub parts: Vec<Part>,
    pub headers: HeaderMap,
    pub file: Arc<PartFile>,
    pub stop: CancellationToken,
    pub policy: RetryPolicy,
    pub limits: Arc<Limits>,
    pub auth: Option<Arc<Authenticator>>,
}

/// One run of one stream, shared by its connections.
pub struct PartsJob {
    setup: Setup,
    received: AtomicU64,
    joined: Mutex<Joined>,
    /// Keeps writes in order, across `await`.
    writing: tokio::sync::Mutex<()>,
    changed: Notify,
}

impl PartsJob {
    pub fn new(setup: Setup, start: Position) -> Self {
        Self {
            setup,
            received: AtomicU64::new(start.bytes),
            joined: Mutex::new(Joined {
                position: start,
                next_to_claim: start.parts,
                ready: BTreeMap::new(),
            }),
            writing: tokio::sync::Mutex::new(()),
            changed: Notify::new(),
        }
    }

    fn joined(&self) -> MutexGuard<'_, Joined> {
        self.joined.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn position(&self) -> Position {
        self.joined().position
    }

    pub fn file(&self) -> Arc<PartFile> {
        Arc::clone(&self.setup.file)
    }

    /// Bytes received so far, including parts not written yet.
    pub fn received(&self) -> u64 {
        self.received.load(Ordering::Relaxed)
    }

    /// The final size, estimated from the parts finished so far.
    pub fn estimated_total(&self) -> Option<u64> {
        let Position { parts, bytes } = self.position();
        (parts > 0).then(|| {
            let estimate = bytes / parts as u64 * self.setup.parts.len() as u64;
            estimate.max(self.received())
        })
    }

    fn is_complete(&self) -> bool {
        self.position().parts == self.setup.parts.len()
    }

    /// Takes the next part once there's room for it. `None` means nothing is left to do.
    async fn claim(&self) -> Option<usize> {
        loop {
            let changed = self.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();

            {
                let mut joined = self.joined();
                if joined.next_to_claim >= self.setup.parts.len() {
                    return None;
                }
                if joined.next_to_claim <= joined.position.parts + PARTS_AHEAD {
                    joined.next_to_claim += 1;
                    return Some(joined.next_to_claim - 1);
                }
            }

            tokio::select! {
                () = self.setup.stop.cancelled() => return None,
                () = &mut changed => {}
            }
        }
    }

    async fn open(&self, part: &Part) -> Result<Response, Problem> {
        let mut request = self
            .setup
            .client
            .get(&part.url)
            .headers(self.setup.headers.clone());

        if let Some(range) = part.range {
            let last = range.start + range.length.saturating_sub(1);
            request = request.header(RANGE, format!("bytes={}-{last}", range.start));
        }
        let request = authorize(request, self.setup.auth.as_deref(), &part.url);

        let response = tokio::select! {
            () = self.setup.stop.cancelled() => return Err(Problem::Stopped),
            response = request.send() => response.map_err(Problem::from_request)?,
        };

        let status = response.status();
        if !status.is_success() {
            return Err(Problem::from_status(status, response.headers()));
        }

        // Usually an expired link sending the visitor to a login page. Joining that in
        // would quietly ruin the file.
        let content_type = text(response.headers(), CONTENT_TYPE).unwrap_or_default();
        if content_type.starts_with("text/html") {
            return Err(Problem::Fatal(
                "the server sent a web page instead of video".to_owned(),
            ));
        }
        Ok(response)
    }

    /// Fetches one part. `counted` tracks what this attempt added to `received`.
    async fn read(&self, part: &Part, counted: &mut u64) -> Result<Vec<u8>, Problem> {
        let mut response = self.open(part).await?;

        // A server that ignores the range sends the whole file, so it's cut out here.
        let mut skip = match part.range {
            Some(range) if response.status() != StatusCode::PARTIAL_CONTENT => range.start,
            _ => 0,
        };
        let wanted = part.range.map(|range| range.length as usize);
        let mut data = Vec::new();

        loop {
            let chunk = tokio::select! {
                () = self.setup.stop.cancelled() => return Err(Problem::Stopped),
                chunk = response.chunk() => chunk.map_err(Problem::from_request)?,
            };
            let Some(chunk) = chunk else {
                break;
            };

            let length = chunk.len() as u64;
            let wait = self.setup.limits.charge(length);
            if !wait.is_zero() && !self.wait(wait).await {
                return Err(Problem::Stopped);
            }
            self.received.fetch_add(length, Ordering::Relaxed);
            *counted += length;

            let dropped = skip.min(length);
            skip -= dropped;
            data.extend_from_slice(&chunk[dropped as usize..]);

            if data.len() > MAX_PART_BYTES {
                return Err(Problem::Fatal(
                    "a part of the video is too large".to_owned(),
                ));
            }
            if wanted.is_some_and(|wanted| data.len() >= wanted) {
                break;
            }
        }

        if let Some(wanted) = wanted {
            if data.len() < wanted {
                return Err(Problem::retry("the server sent less than it should"));
            }
            data.truncate(wanted);
        }
        Ok(data)
    }

    /// Waits before a retry. Returns false if stopped meanwhile.
    async fn wait(&self, delay: Duration) -> bool {
        tokio::select! {
            () = self.setup.stop.cancelled() => false,
            () = tokio::time::sleep(delay) => true,
        }
    }

    /// Fetches a part, retrying temporary failures.
    async fn fetch(&self, index: usize) -> Result<Vec<u8>, Ended> {
        let part = &self.setup.parts[index];
        let policy = self.setup.policy;
        let mut failures = 0;

        loop {
            let mut counted = 0;
            let result = self.read(part, &mut counted).await;
            if result.is_err() {
                self.received.fetch_sub(counted, Ordering::Relaxed);
            }

            match result {
                Ok(data) => return Ok(data),
                Err(Problem::Stopped) => return Err(Ended::Stopped),
                Err(Problem::LoginRequired { .. }) => {
                    return Err(Ended::Failed(LOGIN_NEEDED.to_owned()));
                }
                Err(Problem::Retry { reason, wait }) => {
                    failures += 1;
                    if failures > policy.attempts {
                        return Err(Ended::Failed(self.describe(index, &reason)));
                    }
                    if !self.wait(wait.unwrap_or(policy.delay(failures))).await {
                        return Err(Ended::Stopped);
                    }
                }
                Err(problem) => {
                    return Err(Ended::Failed(self.describe(index, &problem.into_reason())));
                }
            }
        }
    }

    fn describe(&self, index: usize, reason: &str) -> String {
        format!(
            "part {} of {} failed: {reason}",
            index + 1,
            self.setup.parts.len()
        )
    }

    /// Hands over a finished part, writing everything that's now next in line.
    async fn commit(&self, index: usize, data: Vec<u8>) -> Result<(), Ended> {
        self.joined().ready.insert(index, data);
        let _writing = self.writing.lock().await;

        loop {
            let (data, offset) = {
                let mut joined = self.joined();
                let next = joined.position.parts;
                match joined.ready.remove(&next) {
                    Some(data) => (data, joined.position.bytes),
                    None => break,
                }
            };

            let length = data.len() as u64;
            let file = self.file();
            tokio::task::spawn_blocking(move || file.write_at(&data, offset))
                .await
                .map_err(|error| Ended::Failed(format!("disk write stopped: {error}")))?
                .map_err(|error| Ended::Failed(format!("could not write the file: {error}")))?;

            let mut joined = self.joined();
            joined.position.parts += 1;
            joined.position.bytes += length;
        }

        self.changed.notify_waiters();
        Ok(())
    }
}

/// One connection: takes parts until none are left.
async fn worker(job: Arc<PartsJob>) -> Ended {
    loop {
        if job.setup.stop.is_cancelled() {
            return Ended::Stopped;
        }
        let Some(index) = job.claim().await else {
            return if job.setup.stop.is_cancelled() {
                Ended::Stopped
            } else {
                Ended::Finished
            };
        };

        let data = match job.fetch(index).await {
            Ok(data) => data,
            Err(ended) => return ended,
        };
        if let Err(ended) = job.commit(index, data).await {
            return ended;
        }
    }
}

/// Runs up to `connections` connections until the stream is complete or something stops it.
pub async fn run(job: Arc<PartsJob>, connections: usize) -> Ended {
    let mut workers = JoinSet::new();
    for _ in 0..connections.clamp(1, MAX_CONNECTIONS) {
        workers.spawn(worker(Arc::clone(&job)));
    }

    let mut first_problem = None;
    while let Some(joined) = workers.join_next().await {
        let ended = joined.unwrap_or_else(|error| Ended::Failed(format!("{error}")));
        if ended != Ended::Finished && first_problem.is_none() {
            first_problem = Some(ended);
            job.setup.stop.cancel();
        }
    }

    if job.is_complete() {
        return Ended::Finished;
    }
    first_problem.unwrap_or_else(|| Ended::Failed("the transfer ended early".to_owned()))
}
