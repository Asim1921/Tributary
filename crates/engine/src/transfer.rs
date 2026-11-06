//! Moves bytes from the network to the file: several connections when the server
//! supports ranges, one otherwise.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use bytes::Bytes;
use reqwest::header::{ETAG, HeaderMap, IF_RANGE, RANGE};
use reqwest::{Client, RequestBuilder, Response, StatusCode};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use crate::auth::Authenticator;
use crate::disk::PartFile;
use crate::ftp::{FtpBody, FtpTarget};
use crate::limits::Limits;
use crate::remote::{ContentRange, LOGIN_NEEDED, Problem, authorize, text};
use crate::segments::{Claim, SegmentMap};

/// How long to wait for the end of a response so its connection can be used again.
const REUSE_WAIT: Duration = Duration::from_millis(200);

/// Data is gathered to this size first, because network reads are far smaller than
/// what's efficient to hand to a disk.
const WRITE_BUFFER_BYTES: usize = 1 << 20;

/// How persistent to be. On a segmented download, progress that was kept resets the count.
#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy {
    /// Failures in a row before giving up.
    pub attempts: u32,
    pub base_delay: Duration,
    pub max_delay: Duration,
}

impl RetryPolicy {
    /// Doubles after each failure, up to `max_delay`. `failures` starts at 1.
    pub fn delay(&self, failures: u32) -> Duration {
        self.base_delay
            .saturating_mul(1u32 << failures.saturating_sub(1).min(16))
            .min(self.max_delay)
    }
}

/// How a transfer ended.
#[derive(Debug, PartialEq, Eq)]
pub enum Ended {
    Finished,
    Stopped,
    Changed,
    Failed(String),
}

/// One run of one download, shared by its connections.
pub struct Job {
    pub client: Client,
    pub url: String,
    pub headers: HeaderMap,
    pub if_range: Option<String>,
    pub etag: Option<String>,
    pub size: Option<u64>,
    pub file: Arc<PartFile>,
    /// Unused when the server doesn't support ranges.
    pub map: Mutex<SegmentMap>,
    /// Bytes written so far, for progress reports.
    pub transferred: AtomicU64,
    pub stop: CancellationToken,
    pub policy: RetryPolicy,
    /// Shared by every download.
    pub limits: Arc<Limits>,
    pub auth: Option<Arc<Authenticator>>,
    /// Set for an FTP download.
    pub ftp: Option<FtpTarget>,
}

/// The bytes of a file arriving from a server.
enum Body {
    Http(Response),
    Ftp(Box<FtpBody>),
}

impl Body {
    async fn next(&mut self) -> Result<Option<Bytes>, Problem> {
        match self {
            Self::Http(response) => response.chunk().await.map_err(Problem::from_request),
            Self::Ftp(body) => body.next().await,
        }
    }

    /// Reads to the end of a response that has delivered everything, because a
    /// connection can only be reused once its response is finished.
    async fn finish(&mut self) {
        if let Self::Http(response) = self {
            let _ = tokio::time::timeout(REUSE_WAIT, response.chunk()).await;
        }
    }
}

impl Job {
    pub fn map(&self) -> MutexGuard<'_, SegmentMap> {
        self.map.lock().unwrap_or_else(PoisonError::into_inner)
    }

    async fn send(&self, request: RequestBuilder) -> Result<Response, Problem> {
        tokio::select! {
            () = self.stop.cancelled() => Err(Problem::Stopped),
            response = request.send() => response.map_err(Problem::from_request),
        }
    }

    /// Waits before a retry. Returns false if stopped meanwhile.
    async fn wait(&self, delay: Duration) -> bool {
        tokio::select! {
            () = self.stop.cancelled() => false,
            () = tokio::time::sleep(delay) => true,
        }
    }

    /// Counts received bytes against the limits, waiting if needed. Not reading is
    /// what slows the server down. Returns false if stopped meanwhile.
    async fn throttle(&self, bytes: usize) -> bool {
        let wait = self.limits.charge(bytes as u64);
        wait.is_zero() || self.wait(wait).await
    }

    /// Writes out `buffer` at `offset`, off the async threads, leaving it empty.
    async fn write(&self, buffer: &mut Vec<u8>, offset: u64) -> Result<(), Problem> {
        let file = Arc::clone(&self.file);
        let data = std::mem::take(buffer);
        let count = data.len() as u64;

        let result = tokio::task::spawn_blocking(move || {
            let written = file.write_at(&data, offset);
            (written, data)
        })
        .await;

        let (written, data) =
            result.map_err(|error| Problem::Fatal(format!("disk write stopped: {error}")))?;
        written.map_err(|error| Problem::Fatal(format!("could not write the file: {error}")))?;

        *buffer = data;
        buffer.clear();
        self.transferred.fetch_add(count, Ordering::Relaxed);
        Ok(())
    }
}

/// Runs up to `connections` connections until the file is complete or something stops it.
pub async fn run_segments(job: Arc<Job>, connections: usize) -> Ended {
    let mut workers = JoinSet::new();
    for _ in 0..connections.max(1) {
        workers.spawn(worker(Arc::clone(&job)));
    }

    let mut first_problem = None;
    while let Some(joined) = workers.join_next().await {
        let ended = joined.unwrap_or_else(|error| Ended::Failed(format!("{error}")));
        if ended != Ended::Finished && first_problem.is_none() {
            first_problem = Some(ended);
            job.stop.cancel();
        }
    }

    if job.map().is_complete() {
        return Ended::Finished;
    }
    first_problem.unwrap_or_else(|| Ended::Failed("the transfer ended early".to_owned()))
}

/// One connection: takes work until none is left.
async fn worker(job: Arc<Job>) -> Ended {
    let mut failures = 0;

    loop {
        if job.stop.is_cancelled() {
            return Ended::Stopped;
        }
        let Some(claim) = job.map().claim() else {
            return Ended::Finished;
        };

        let before = job.map().written(claim.index);
        let result = fetch_segment(&job, claim).await;
        let (progressed, others_active) = {
            let mut map = job.map();
            let progressed = map.written(claim.index) > before;
            map.release(claim.index);
            (progressed, map.has_owners())
        };

        match result {
            Ok(()) => failures = 0,
            Err(Problem::Stopped) => return Ended::Stopped,
            Err(Problem::Changed) => return Ended::Changed,
            Err(Problem::Fatal(reason)) => return Ended::Failed(reason),
            Err(Problem::LoginRequired { .. }) => {
                return Ended::Failed(LOGIN_NEEDED.to_owned());
            }
            Err(Problem::Retry { reason, wait }) => {
                failures = if progressed { 1 } else { failures + 1 };
                if failures > job.policy.attempts {
                    // Some servers cap connections per client. Leaving quietly lets
                    // the connections that do work finish this part later. Only the
                    // last one reports failure.
                    return if others_active {
                        Ended::Finished
                    } else {
                        Ended::Failed(reason)
                    };
                }
                if !job.wait(wait.unwrap_or(job.policy.delay(failures))).await {
                    return Ended::Stopped;
                }
            }
        }
    }
}

/// Opens one part of the file, by whichever protocol it uses.
async fn open_range(job: &Job, claim: Claim) -> Result<Body, Problem> {
    if let Some(target) = &job.ftp {
        let opening = FtpBody::open(target, claim.from);
        return tokio::select! {
            () = job.stop.cancelled() => Err(Problem::Stopped),
            body = opening => Ok(Body::Ftp(Box::new(body?))),
        };
    }

    let mut request = job
        .client
        .get(&job.url)
        .headers(job.headers.clone())
        .header(RANGE, format!("bytes={}-{}", claim.from, claim.end - 1));
    if let Some(validator) = &job.if_range {
        request = request.header(IF_RANGE, validator);
    }
    let request = authorize(request, job.auth.as_deref(), &job.url);

    let response = job.send(request).await?;
    check_partial(job, &response, claim)?;
    Ok(Body::Http(response))
}

async fn fetch_segment(job: &Job, claim: Claim) -> Result<(), Problem> {
    let mut body = open_range(job, claim).await?;

    let mut buffer = Vec::with_capacity(WRITE_BUFFER_BYTES);
    let mut position = claim.from;

    loop {
        let next = tokio::select! {
            () = job.stop.cancelled() => {
                flush(job, claim, &mut buffer, &mut position).await?;
                return Err(Problem::Stopped);
            }
            next = body.next() => next,
        };

        match next {
            Ok(Some(bytes)) => {
                let (taken, done) = {
                    let mut map = job.map();
                    let taken = map.accept(claim.index, bytes.len());
                    (taken, map.unfetched(claim.index) == 0)
                };
                buffer.extend_from_slice(&bytes[..taken]);

                if done || buffer.len() >= WRITE_BUFFER_BYTES {
                    flush(job, claim, &mut buffer, &mut position).await?;
                }
                // Before the `done` check, so a segment's last bytes count as well.
                let allowed = job.throttle(taken).await;
                if done {
                    if taken == bytes.len() {
                        body.finish().await;
                    }
                    return Ok(());
                }
                if !allowed {
                    flush(job, claim, &mut buffer, &mut position).await?;
                    return Err(Problem::Stopped);
                }
            }
            Ok(None) => {
                flush(job, claim, &mut buffer, &mut position).await?;
                return Err(Problem::retry("the server ended the response early"));
            }
            Err(problem) => {
                // Bytes that did arrive are good, so keep them.
                flush(job, claim, &mut buffer, &mut position).await?;
                return Err(problem);
            }
        }
    }
}

async fn flush(
    job: &Job,
    claim: Claim,
    buffer: &mut Vec<u8>,
    position: &mut u64,
) -> Result<(), Problem> {
    let count = buffer.len() as u64;
    if count == 0 {
        return Ok(());
    }
    job.write(buffer, *position).await?;
    job.map().commit(claim.index, count);
    *position += count;
    Ok(())
}

/// Confirms a response really is the requested part of the same file.
fn check_partial(job: &Job, response: &Response, claim: Claim) -> Result<(), Problem> {
    match response.status() {
        StatusCode::PARTIAL_CONTENT => {}
        // A Digest nonce can expire during a long download. With a fresh one from the
        // server, the login still works, so this connection only needs another try.
        StatusCode::UNAUTHORIZED => {
            let problem = Problem::from_status(StatusCode::UNAUTHORIZED, response.headers());
            return match (&job.auth, problem) {
                (
                    Some(auth),
                    Problem::LoginRequired {
                        challenge: Some(fresh),
                    },
                ) => {
                    auth.renew(fresh);
                    Err(Problem::retry("the login needs renewing"))
                }
                (_, problem) => Err(problem),
            };
        }
        // A whole file, or a range past the end, means the plan no longer fits the
        // server. That's what `If-Range` answers when the file has changed.
        StatusCode::OK | StatusCode::RANGE_NOT_SATISFIABLE => return Err(Problem::Changed),
        status => return Err(Problem::from_status(status, response.headers())),
    }

    let headers = response.headers();
    if let (Some(current), Some(expected)) = (text(headers, ETAG), &job.etag)
        && current != *expected
    {
        return Err(Problem::Changed);
    }

    let Some(range) = ContentRange::from_headers(headers) else {
        return Err(Problem::retry("the server sent no usable Content-Range"));
    };
    if range.total.is_some() && range.total != job.size {
        return Err(Problem::Changed);
    }
    if range.start != Some(claim.from) {
        return Err(Problem::retry("the server sent a different range"));
    }
    Ok(())
}

/// Fetches the file on one connection. Any failure restarts from the beginning,
/// because the server can't send just the missing part.
pub async fn run_stream(job: Arc<Job>) -> Ended {
    let mut failures = 0;

    loop {
        job.transferred.store(0, Ordering::Relaxed);

        match fetch_whole(&job).await {
            Ok(()) => return Ended::Finished,
            Err(Problem::Stopped) => return Ended::Stopped,
            Err(Problem::Changed) => return Ended::Changed,
            Err(Problem::Fatal(reason)) => return Ended::Failed(reason),
            Err(Problem::LoginRequired { .. }) => {
                return Ended::Failed(LOGIN_NEEDED.to_owned());
            }
            Err(Problem::Retry { reason, wait }) => {
                // Unlike with segments, nothing survives a failed attempt, so bytes
                // fetched during one are not progress. Counting them as progress
                // would retry forever against a server that always cuts off.
                failures += 1;
                if failures > job.policy.attempts {
                    return Ended::Failed(reason);
                }
                if !job.wait(wait.unwrap_or(job.policy.delay(failures))).await {
                    return Ended::Stopped;
                }
            }
        }
    }
}

/// Opens the whole file, by whichever protocol it uses.
async fn open_whole(job: &Job) -> Result<Body, Problem> {
    if let Some(target) = &job.ftp {
        let opening = FtpBody::open(target, 0);
        return tokio::select! {
            () = job.stop.cancelled() => Err(Problem::Stopped),
            body = opening => Ok(Body::Ftp(Box::new(body?))),
        };
    }

    let request = job.client.get(&job.url).headers(job.headers.clone());
    let request = authorize(request, job.auth.as_deref(), &job.url);
    let response = job.send(request).await?;
    if response.status() != StatusCode::OK {
        return Err(Problem::from_status(response.status(), response.headers()));
    }
    Ok(Body::Http(response))
}

async fn fetch_whole(job: &Job) -> Result<(), Problem> {
    let mut body = open_whole(job).await?;

    let mut buffer = Vec::with_capacity(WRITE_BUFFER_BYTES);
    let mut position = 0;

    loop {
        let next = tokio::select! {
            () = job.stop.cancelled() => return Err(Problem::Stopped),
            next = body.next() => next,
        };

        match next? {
            Some(bytes) => {
                buffer.extend_from_slice(&bytes);
                if buffer.len() >= WRITE_BUFFER_BYTES {
                    let count = buffer.len() as u64;
                    job.write(&mut buffer, position).await?;
                    position += count;
                }
                if !job.throttle(bytes.len()).await {
                    return Err(Problem::Stopped);
                }
            }
            None => break,
        }
    }

    let count = buffer.len() as u64;
    job.write(&mut buffer, position).await?;
    position += count;

    if job.size.is_some_and(|size| size != position) {
        return Err(Problem::retry("the server ended the response early"));
    }
    job.file
        .set_len(position)
        .map_err(|error| Problem::Fatal(format!("could not write the file: {error}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delays_double_up_to_the_limit() {
        let policy = RetryPolicy {
            attempts: 5,
            base_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(10),
        };

        let delays: Vec<u64> = (1..=6).map(|n| policy.delay(n).as_secs()).collect();
        assert_eq!(delays, [1, 2, 4, 8, 10, 10]);
    }

    #[test]
    fn huge_failure_counts_do_not_overflow() {
        let policy = RetryPolicy {
            attempts: 5,
            base_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(10),
        };
        assert_eq!(policy.delay(u32::MAX), Duration::from_secs(10));
    }
}
