//! The public face of the engine: adding, pausing, resuming and listing downloads.

use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use media::Part;
use percent_encoding::percent_decode_str;
use reqwest::header::{ACCEPT_ENCODING, CONTENT_TYPE, HeaderMap, HeaderName, HeaderValue};
use reqwest::{Client, NoProxy, Proxy, Response, Url, redirect};
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tokio::time::MissedTickBehavior;
use tokio_util::sync::CancellationToken;

use crate::auth::{Authenticator, Login};
use crate::disk::{self, PartFile};
use crate::error::Error;
use crate::ftp::{self, FtpTarget};
use crate::limits::{Limits, Quota};
use crate::name;
use crate::parts::{self, PartsJob, Position};
use crate::remote::{self, Problem, Remote};
use crate::scheduler::{Command, Moment, Queue, QueueId, Scheduler};
use crate::segments::{Segment, SegmentMap};
use crate::store::{Download, DownloadId, State, Store};
use crate::transfer::{self, Ended, Job, RetryPolicy};
use crate::wildcard::wildcard_match;

const USER_AGENT: &str = concat!("Tributary/", env!("CARGO_PKG_VERSION"));
const MAX_REDIRECTS: usize = 10;
const EVENT_BUFFER: usize = 1024;

/// How many times a download restarts because the server's file changed.
const MAX_RESTARTS: u32 = 3;

/// How much each new measurement moves the reported speed.
const SPEED_SMOOTHING: f64 = 0.3;

/// The most that's read of a playlist. More than any real one, but it comes from a web
/// page, so it mustn't be able to fill memory.
const MAX_TEXT_BYTES: usize = 32 << 20;

/// Headers that must never be saved. They're kept in memory while the program runs.
/// After a restart, the caller has to supply them again.
const SECRET_HEADERS: [&str; 3] = ["cookie", "authorization", "proxy-authorization"];

/// How often the scheduler looks at the queues.
const SCHEDULER_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Debug, Clone)]
pub struct Config {
    /// Connections per download.
    pub connections: usize,
    /// Where `.part` files go. `None` keeps them next to the destination, which
    /// avoids a slow copy between drives when a download finishes.
    pub temp_dir: Option<PathBuf>,
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
            temp_dir: None,
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
    pub description: Option<String>,
    /// Extra request headers. `Cookie` and similar are used but never saved.
    pub headers: Vec<(String, String)>,
    /// Saves the download without starting it.
    pub paused: bool,
    /// Adds it to the end of this queue.
    pub queue: Option<QueueId>,
    pub category: Option<String>,
    /// Used if the server asks for one. Kept in memory only.
    pub login: Option<Login>,
    /// Makes this a stream: these parts are joined in order into one file.
    pub parts: Option<Vec<Part>>,
}

/// A proxy the user has set up by hand.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ManualProxy {
    /// Such as `http://host:8080` or `socks5://host:1080`.
    pub url: String,
    pub login: Option<Login>,
    /// Hosts to reach directly, separated by commas.
    pub exceptions: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ProxyMode {
    /// Follows Windows' proxy settings.
    #[default]
    System,
    Direct,
    Manual(ManualProxy),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkOptions {
    /// Replaces the program's own name in requests.
    pub user_agent: Option<String>,
    pub proxy: ProxyMode,
    pub ftp_passive: bool,
}

impl Default for NetworkOptions {
    fn default() -> Self {
        Self {
            user_agent: None,
            proxy: ProxyMode::default(),
            ftp_passive: true,
        }
    }
}

/// How many connections to use for downloads from particular servers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerLimit {
    /// A host pattern such as `*.example.com`.
    pub pattern: String,
    pub connections: usize,
}

/// Changes to a download that isn't running. `None` leaves a field as it is.
#[derive(Debug, Clone, Default)]
pub struct DownloadEdit {
    /// A new address for the same file, for example to replace an expired link.
    /// If it turns out to be a different file, the download starts again.
    pub url: Option<String>,
    pub dest_dir: Option<PathBuf>,
    pub description: Option<String>,
}

/// What a server says about a file before downloading it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileInfo {
    pub file_name: String,
    pub size: Option<u64>,
    pub resumable: bool,
    /// The address leads to a web page rather than a file.
    pub is_web_page: bool,
    pub content_type: Option<String>,
}

/// What a server gave for a web page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchedPage {
    /// Where it really came from, after any redirects.
    pub url: String,
    /// False if the server said there's no such page.
    pub found: bool,
    pub content_type: Option<String>,
    /// Empty if it wasn't text.
    pub text: Option<String>,
}

/// A queue and how it's doing right now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueInfo {
    pub queue: Queue,
    pub active: bool,
    pub files: Vec<DownloadId>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    State {
        id: DownloadId,
        state: State,
    },
    /// A queue or its files changed.
    Queues,
    /// A queue ended on its own, so its after-queue action should run.
    QueueFinished {
        id: QueueId,
    },
    Progress {
        id: DownloadId,
        downloaded: u64,
        total: Option<u64>,
        bytes_per_second: u64,
        /// Each part of the file and how far it has got. Empty on one connection.
        segments: Vec<Segment>,
    },
}

/// Settings that can change while the engine runs. A change applies to downloads
/// started afterwards.
#[derive(Debug, Clone)]
struct LiveOptions {
    connections: usize,
    temp_dir: Option<PathBuf>,
    server_limits: Vec<ServerLimit>,
    client: Client,
    ftp_passive: bool,
}

impl LiveOptions {
    /// How many connections to use with a server.
    fn connections_for(&self, host: &str) -> usize {
        self.server_limits
            .iter()
            .find(|limit| wildcard_match(&limit.pattern, host))
            .map_or(self.connections, |limit| limit.connections.max(1))
    }
}

struct Running {
    stop: CancellationToken,
    task: JoinHandle<()>,
}

struct Inner {
    store: Arc<Store>,
    config: Config,
    live: Mutex<LiveOptions>,
    limits: Arc<Limits>,
    events: broadcast::Sender<Event>,
    running: Mutex<HashMap<DownloadId, Running>>,
    scheduler: Mutex<Scheduler>,
    /// Secret headers, in memory only.
    secrets: Mutex<HashMap<DownloadId, HeaderMap>>,
    /// Logins, in memory only.
    logins: Mutex<HashMap<DownloadId, Login>>,
}

impl Inner {
    fn running(&self) -> MutexGuard<'_, HashMap<DownloadId, Running>> {
        self.running.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn scheduler(&self) -> MutexGuard<'_, Scheduler> {
        self.scheduler
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn secrets(&self) -> MutexGuard<'_, HashMap<DownloadId, HeaderMap>> {
        self.secrets.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn logins(&self) -> MutexGuard<'_, HashMap<DownloadId, Login>> {
        self.logins.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn client(&self) -> Client {
        self.live().client.clone()
    }

    fn ftp_target(&self, url: &Url, login: Option<Login>) -> FtpTarget {
        FtpTarget {
            url: url.clone(),
            login,
            passive: self.live().ftp_passive,
            connect_timeout: self.config.connect_timeout,
            read_timeout: self.config.read_timeout,
        }
    }

    fn live(&self) -> MutexGuard<'_, LiveOptions> {
        self.live.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn emit(&self, event: Event) {
        // Having nobody listening is normal.
        let _ = self.events.send(event);
    }
}

/// Runs downloads and remembers them. Must be used inside a Tokio runtime. Clones
/// share the same downloads.
#[derive(Clone)]
pub struct Engine {
    inner: Arc<Inner>,
}

impl Engine {
    /// Opens the database at `path`. Downloads interrupted by a crash come back paused.
    pub fn open(path: &Path, config: Config) -> Result<Self, Error> {
        let network = NetworkOptions::default();
        let client = build_client(&config, &network)?;

        let (events, _) = broadcast::channel(EVENT_BUFFER);
        let live = LiveOptions {
            connections: config.connections,
            temp_dir: config.temp_dir.clone(),
            server_limits: Vec::new(),
            client,
            ftp_passive: network.ftp_passive,
        };
        Ok(Self {
            inner: Arc::new(Inner {
                store: Arc::new(Store::open(path)?),
                config,
                live: Mutex::new(live),
                limits: Arc::default(),
                events,
                running: Mutex::default(),
                scheduler: Mutex::default(),
                secrets: Mutex::default(),
                logins: Mutex::default(),
            }),
        })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.inner.events.subscribe()
    }

    /// Sets the user agent, proxy and FTP mode for downloads started from now on.
    pub fn set_network(&self, options: &NetworkOptions) -> Result<(), Error> {
        let client = build_client(&self.inner.config, options)?;
        let mut live = self.inner.live();
        live.client = client;
        live.ftp_passive = options.ftp_passive;
        Ok(())
    }

    /// Sets connection counts for particular servers, for downloads started from now on.
    pub fn set_server_limits(&self, limits: Vec<ServerLimit>) {
        self.inner.live().server_limits = limits;
    }

    /// Gives a download a login, kept in memory only.
    pub fn set_login(&self, id: DownloadId, login: Login) {
        self.inner.logins().insert(id, login);
    }

    /// Sets the total bytes per second across all downloads. Applies straight away.
    pub fn set_speed_limit(&self, bytes_per_second: Option<u64>) {
        self.inner.limits.set_rate(bytes_per_second);
    }

    /// Sets how much can be downloaded per period. Applies straight away.
    pub fn set_quota(&self, quota: Option<Quota>) {
        self.inner.limits.set_quota(quota);
    }

    /// Looks after the queues for as long as the returned future runs.
    pub async fn run_scheduler(&self) {
        let mut ticker = tokio::time::interval(SCHEDULER_INTERVAL);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            ticker.tick().await;
            // A failure here is usually temporary, and the next tick tries again.
            let _ = self.tick(Moment::now()).await;
        }
    }

    /// Does whatever the queues need at this moment.
    pub async fn tick(&self, now: Moment) -> Result<(), Error> {
        let queues = self
            .inner
            .store
            .queues()?
            .into_iter()
            .map(|queue| {
                let files = self.inner.store.queue_files(queue.id)?;
                Ok((queue, files))
            })
            .collect::<Result<Vec<_>, Error>>()?;

        let commands = self.inner.scheduler().tick(now, &queues);
        let changed = !commands.is_empty();
        for command in commands {
            self.execute(command).await?;
        }
        if changed {
            self.inner.emit(Event::Queues);
        }
        Ok(())
    }

    async fn execute(&self, command: Command) -> Result<(), Error> {
        match command {
            Command::Start(id) => match self.start(id) {
                Err(Error::AlreadyRunning(_)) => {}
                other => other?,
            },
            Command::Pause(id) => self.stop_running(id).await,
            Command::MoveToEnd(id) => {
                let queue = self.get(id)?.queue;
                self.inner.store.set_queue(id, queue)?;
            }
            Command::LeaveQueue(id) => self.inner.store.set_queue(id, None)?,
            Command::CheckForUpdate(id) => {
                // A server that can't be reached is simply checked again next time.
                let _ = self.check_for_update(id).await;
            }
            Command::Finished(id) => self.inner.emit(Event::QueueFinished { id }),
        }
        Ok(())
    }

    /// Asks the server whether a finished file has changed, and if so marks it to be
    /// fetched again. The old file stays until the new one is complete.
    async fn check_for_update(&self, id: DownloadId) -> Result<(), Error> {
        let download = self.get(id)?;
        // A stream's address is its playlist, which says nothing about the saved file.
        if download.is_stream {
            return Ok(());
        }

        let mut headers = to_header_map(&download.headers)?;
        if let Some(secrets) = self.inner.secrets().get(&id) {
            headers.extend(secrets.clone());
        }
        let login = self.inner.logins().get(&id).cloned();

        let stop = CancellationToken::new();
        let (remote, _) = probe(&self.inner, &download.url, &headers, login, &stop)
            .await
            .map_err(|problem| Error::Request(problem.into_reason()))?;

        if !remote.same_file(&download) {
            self.inner.store.prepare_refresh(id)?;
            self.inner.emit(Event::State {
                id,
                state: State::Paused,
            });
        }
        Ok(())
    }

    pub fn queues(&self) -> Result<Vec<QueueInfo>, Error> {
        let scheduler = self.inner.scheduler();
        self.inner
            .store
            .queues()?
            .into_iter()
            .map(|queue| {
                let files = self.inner.store.queue_files(queue.id)?;
                Ok(QueueInfo {
                    active: scheduler.is_active(queue.id),
                    files: files.into_iter().map(|file| file.id).collect(),
                    queue,
                })
            })
            .collect()
    }

    /// Creates a queue, or changes one that exists. Returns its id.
    pub fn save_queue(&self, queue: &Queue) -> Result<QueueId, Error> {
        let id = self.inner.store.save_queue(queue)?;
        self.inner.emit(Event::Queues);
        Ok(id)
    }

    /// Deletes a queue, stopping it first. Its files stay in the list.
    pub async fn delete_queue(&self, id: QueueId) -> Result<(), Error> {
        if id == QueueId::MAIN {
            return Err(Error::MainQueue);
        }
        self.stop_queue(id).await?;
        self.inner.store.delete_queue(id)?;
        self.inner.emit(Event::Queues);
        Ok(())
    }

    /// Starts a queue by hand. Its next steps happen on the following ticks.
    pub fn start_queue(&self, id: QueueId) -> Result<(), Error> {
        let queue = self
            .inner
            .store
            .queues()?
            .into_iter()
            .find(|queue| queue.id == id)
            .ok_or(Error::QueueNotFound(id))?;
        let files = self.inner.store.queue_files(id)?;

        self.inner
            .scheduler()
            .start_now(&queue, &files, Moment::now());
        self.inner.emit(Event::Queues);
        Ok(())
    }

    /// Stops a queue by hand, along with its running files. Its after-queue action
    /// doesn't run.
    pub async fn stop_queue(&self, id: QueueId) -> Result<(), Error> {
        self.inner.scheduler().stop_now(id);
        for file in self.inner.store.queue_files(id)? {
            if file.state == State::Running {
                self.stop_running(file.id).await;
            }
        }
        self.inner.emit(Event::Queues);
        Ok(())
    }

    /// Moves a download to the end of a queue, or out of queues with `None`.
    pub fn set_queue(&self, id: DownloadId, queue: Option<QueueId>) -> Result<(), Error> {
        self.get(id)?;
        self.inner.store.set_queue(id, queue)?;
        self.inner.emit(Event::Queues);
        Ok(())
    }

    /// Sets the order of a queue's files.
    pub fn reorder_queue(&self, ids: &[DownloadId]) -> Result<(), Error> {
        self.inner.store.reorder_queue(ids)?;
        self.inner.emit(Event::Queues);
        Ok(())
    }

    pub fn set_category(&self, id: DownloadId, category: Option<&str>) -> Result<(), Error> {
        self.get(id)?;
        self.inner.store.set_category(id, category)?;
        Ok(())
    }

    /// Sets the connections used by downloads started from now on.
    pub fn set_connections(&self, connections: usize) {
        self.inner.live().connections = connections.max(1);
    }

    /// Sets where new downloads keep their `.part` files.
    pub fn set_temp_dir(&self, temp_dir: Option<PathBuf>) {
        self.inner.live().temp_dir = temp_dir;
    }

    /// Asks the server about a file without downloading it.
    pub async fn inspect(
        &self,
        url: &str,
        headers: &[(String, String)],
        login: Option<Login>,
    ) -> Result<FileInfo, Error> {
        let mut url = parse_url(url)?;
        let login = take_url_login(&mut url).or(login);
        let headers = to_header_map(headers)?;

        // One try only, because someone is waiting for the answer.
        let stop = CancellationToken::new();
        let (remote, _) = probe_once(&self.inner, url.as_str(), &headers, login, &stop)
            .await
            .map_err(|problem| match problem {
                Problem::LoginRequired { .. } => Error::LoginRequired,
                other => Error::Request(other.into_reason()),
            })?;

        let suggested = remote.suggested_name.as_deref().unwrap_or_default();
        Ok(FileInfo {
            file_name: name::sanitize(suggested),
            size: remote.size,
            resumable: remote.ranged,
            is_web_page: remote.is_web_page(),
            content_type: remote.content_type.clone(),
        })
    }

    /// Saves a download and, unless asked not to, starts it.
    pub fn add(&self, new: NewDownload) -> Result<DownloadId, Error> {
        let mut url = parse_url(&new.url)?;
        // A password in the address must not reach the database.
        let mut login = take_url_login(&mut url).or(new.login);

        // A part inherits the login of the playlist it was listed in.
        let parts = new.parts.map(|mut parts| {
            for part in &mut parts {
                if let Ok(mut address) = Url::parse(&part.url)
                    && let Some(found) = take_url_login(&mut address)
                {
                    login.get_or_insert(found);
                    part.url = address.into();
                }
            }
            parts
        });

        let (secret, saved): (Vec<_>, Vec<_>) = new
            .headers
            .into_iter()
            .partition(|(name, _)| is_secret(name));
        let secret = to_header_map(&secret)?;
        to_header_map(&saved)?;

        let file_name = new.file_name.as_deref().map(name::sanitize);
        let id = self.inner.store.insert(
            url.as_str(),
            &new.dest_dir,
            file_name.as_deref(),
            new.description.as_deref(),
            &saved,
        )?;

        if let Some(parts) = &parts {
            self.inner.store.set_stream_parts(id, parts)?;
        }
        if !secret.is_empty() {
            self.inner.secrets().insert(id, secret);
        }
        if let Some(login) = login {
            self.inner.logins().insert(id, login);
        }
        if let Some(queue) = new.queue {
            self.inner.store.set_queue(id, Some(queue))?;
        }
        if new.category.is_some() {
            self.inner.store.set_category(id, new.category.as_deref())?;
        }

        if new.paused {
            self.inner.emit(Event::State {
                id,
                state: State::Paused,
            });
        } else {
            self.start(id)?;
        }
        Ok(id)
    }

    /// Links the picture and sound of a video that's saved as two downloads.
    pub fn link_companions(&self, first: DownloadId, second: DownloadId) -> Result<(), Error> {
        Ok(self.inner.store.link_companions(first, second)?)
    }

    /// Records that a finished file was replaced by one of a different size.
    pub fn file_replaced(&self, id: DownloadId, size: u64) -> Result<(), Error> {
        Ok(self.inner.store.set_size(id, size)?)
    }

    /// Reads a small text file, such as a playlist.
    pub async fn fetch_text(
        &self,
        url: &str,
        headers: &[(String, String)],
        login: Option<Login>,
    ) -> Result<String, String> {
        self.request(url, headers, login, read_text).await
    }

    /// Reads a web page. A body that isn't text is left unread, so a large file costs
    /// nothing here. An answer such as 404 is a result, not an error.
    pub async fn fetch_page(
        &self,
        url: &str,
        headers: &[(String, String)],
        login: Option<Login>,
    ) -> Result<FetchedPage, String> {
        self.request(url, headers, login, read_page).await
    }

    /// Makes a request, retrying temporary failures and logging in if asked to.
    async fn request<T, R, F>(
        &self,
        url: &str,
        headers: &[(String, String)],
        login: Option<Login>,
        read: R,
    ) -> Result<T, String>
    where
        R: Fn(Response) -> F,
        F: Future<Output = Result<T, Problem>>,
    {
        let headers = to_header_map(headers).map_err(|error| error.to_string())?;
        let client = self.inner.client();
        let policy = self.inner.config.retry;
        let mut auth: Option<Arc<Authenticator>> = None;
        let mut failures = 0;

        loop {
            let request = client.get(url).headers(headers.clone());
            let request = remote::authorize(request, auth.as_deref(), url);
            let outcome = match request.send().await {
                Ok(response) => read(response).await,
                Err(error) => Err(Problem::from_request(error)),
            };

            match (outcome, &login) {
                (Ok(text), _) => return Ok(text),
                (
                    Err(Problem::LoginRequired {
                        challenge: Some(challenge),
                    }),
                    Some(login),
                ) if auth.is_none() => {
                    auth = Some(Arc::new(Authenticator::new(login.clone(), challenge)));
                }
                (Err(Problem::Retry { reason, wait }), _) => {
                    failures += 1;
                    if failures > policy.attempts {
                        return Err(reason);
                    }
                    tokio::time::sleep(wait.unwrap_or(policy.delay(failures))).await;
                }
                (Err(problem), _) => return Err(problem.into_reason()),
            }
        }
    }

    /// Changes a download that isn't running.
    pub fn edit(&self, id: DownloadId, edit: DownloadEdit) -> Result<(), Error> {
        if self.inner.running().contains_key(&id) {
            return Err(Error::AlreadyRunning(id));
        }
        self.get(id)?;

        let mut url = edit.url.as_deref().map(parse_url).transpose()?;
        if let Some(login) = url.as_mut().and_then(take_url_login) {
            self.inner.logins().insert(id, login);
        }
        self.inner.store.edit(
            id,
            url.as_ref().map(Url::as_str),
            edit.dest_dir.as_deref(),
            edit.description.as_deref(),
        )?;
        Ok(())
    }

    /// Stops a download by hand, keeping its progress. Its queue leaves it alone until
    /// it's resumed. Does nothing if it isn't running.
    pub async fn pause(&self, id: DownloadId) -> Result<(), Error> {
        self.inner.scheduler().hold(id);
        self.stop_running(id).await;
        Ok(())
    }

    /// Stops a download and waits until its progress is saved.
    async fn stop_running(&self, id: DownloadId) {
        let running = self.inner.running().remove(&id);
        if let Some(running) = running {
            running.stop.cancel();
            let _ = running.task.await;
        }
    }

    /// Continues a paused or failed download. Secret headers are remembered until the
    /// program closes. After that, a download that needs them must be given them again.
    pub fn resume(&self, id: DownloadId, secret_headers: &[(String, String)]) -> Result<(), Error> {
        let download = self.get(id)?;
        if download.state == State::Completed {
            return Err(Error::AlreadyCompleted(id));
        }
        if !secret_headers.is_empty() {
            let headers = to_header_map(secret_headers)?;
            self.inner.secrets().insert(id, headers);
        }
        self.inner.scheduler().release(id);
        self.start(id)
    }

    /// Forgets a download and deletes its unfinished `.part` file. A completed file
    /// stays unless `delete_file` is set.
    pub async fn remove(&self, id: DownloadId, delete_file: bool) -> Result<(), Error> {
        self.pause(id).await?;
        let download = self.get(id)?;

        if let Some(file_name) = &download.file_name {
            remove_if_present(&part_path(&download, file_name))?;
        }
        if let Some(path) = download.final_path.as_deref().filter(|_| delete_file) {
            remove_if_present(path)?;
        }
        self.inner.store.delete(id)?;
        self.inner.secrets().remove(&id);
        self.inner.logins().remove(&id);
        self.inner.scheduler().release(id);
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

    fn start(&self, id: DownloadId) -> Result<(), Error> {
        let mut running = self.inner.running();
        if running.contains_key(&id) {
            return Err(Error::AlreadyRunning(id));
        }

        let secret_headers = self.inner.secrets().get(&id).cloned().unwrap_or_default();
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

async fn read_body(response: &mut Response) -> Result<String, Problem> {
    let mut data = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(Problem::from_request)? {
        data.extend_from_slice(&chunk);
        if data.len() > MAX_TEXT_BYTES {
            return Err(Problem::Fatal("the file is too large to read".to_owned()));
        }
    }
    Ok(String::from_utf8_lossy(&data).into_owned())
}

async fn read_text(mut response: Response) -> Result<String, Problem> {
    let status = response.status();
    if !status.is_success() {
        return Err(Problem::from_status(status, response.headers()));
    }
    read_body(&mut response).await
}

fn is_text(content_type: Option<&str>) -> bool {
    content_type.is_none_or(|kind| {
        let kind = kind.to_ascii_lowercase();
        kind.starts_with("text/") || kind.contains("xhtml") || kind.contains("xml")
    })
}

async fn read_page(mut response: Response) -> Result<FetchedPage, Problem> {
    let status = response.status();
    let url = response.url().to_string();
    let content_type = remote::text(response.headers(), CONTENT_TYPE);

    if !status.is_success() {
        // Only what might go better next time is a failure. The rest is just an answer.
        return match Problem::from_status(status, response.headers()) {
            Problem::Fatal(_) => Ok(FetchedPage {
                url,
                found: false,
                content_type,
                text: None,
            }),
            problem => Err(problem),
        };
    }

    let text = if is_text(content_type.as_deref()) {
        Some(read_body(&mut response).await?)
    } else {
        None
    };
    Ok(FetchedPage {
        url,
        found: true,
        content_type,
        text,
    })
}

fn build_client(config: &Config, network: &NetworkOptions) -> Result<Client, Error> {
    let mut default_headers = HeaderMap::new();
    // Ranges count bytes of the stored file, so compression must stay off.
    default_headers.insert(ACCEPT_ENCODING, HeaderValue::from_static("identity"));

    let mut builder = Client::builder()
        .user_agent(network.user_agent.as_deref().unwrap_or(USER_AGENT))
        .default_headers(default_headers)
        .redirect(redirect::Policy::limited(MAX_REDIRECTS))
        .connect_timeout(config.connect_timeout)
        .read_timeout(config.read_timeout);

    builder = match &network.proxy {
        ProxyMode::System => builder,
        ProxyMode::Direct => builder.no_proxy(),
        ProxyMode::Manual(manual) => {
            let invalid = |_| Error::InvalidProxy(manual.url.clone());
            let mut proxy = Proxy::all(&manual.url).map_err(invalid)?;
            if let Some(login) = &manual.login {
                proxy = proxy.basic_auth(&login.username, &login.password);
            }
            builder.proxy(proxy.no_proxy(NoProxy::from_string(&manual.exceptions)))
        }
    };

    Ok(builder.build()?)
}

fn parse_url(text: &str) -> Result<Url, Error> {
    let invalid = || Error::InvalidUrl(text.to_owned());
    let url = Url::parse(text).map_err(|_| invalid())?;
    if matches!(url.scheme(), "http" | "https" | "ftp") {
        Ok(url)
    } else {
        Err(invalid())
    }
}

/// Takes `user:password@` out of an address, so it isn't saved.
fn take_url_login(url: &mut Url) -> Option<Login> {
    if url.username().is_empty() {
        return None;
    }

    let decode = |text: &str| percent_decode_str(text).decode_utf8_lossy().into_owned();
    let login = Login {
        username: decode(url.username()),
        password: decode(url.password().unwrap_or_default()),
    };
    let _ = url.set_username("");
    let _ = url.set_password(None);
    Some(login)
}

fn remove_if_present(path: &Path) -> Result<(), Error> {
    match std::fs::remove_file(path) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.into()),
        _ => Ok(()),
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

fn part_path(download: &Download, file_name: &str) -> PathBuf {
    download
        .part_dir
        .as_ref()
        .unwrap_or(&download.dest_dir)
        .join(format!("{file_name}.{}.part", download.id.0))
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

/// What a server said about a file, and how to log in for the requests that follow.
type Probed = (Remote, Option<Arc<Authenticator>>);

/// Asks the server about a file once, logging in if it asks and there's a login to use.
async fn probe_once(
    inner: &Inner,
    url: &str,
    headers: &HeaderMap,
    login: Option<Login>,
    stop: &CancellationToken,
) -> Result<Probed, Problem> {
    if url.starts_with("ftp:") {
        let parsed = Url::parse(url).map_err(|error| Problem::Fatal(error.to_string()))?;
        let target = inner.ftp_target(&parsed, login);
        return tokio::select! {
            () = stop.cancelled() => Err(Problem::Stopped),
            remote = ftp::probe(&target) => Ok((remote?, None)),
        };
    }

    let client = inner.client();
    let first = tokio::select! {
        () = stop.cancelled() => return Err(Problem::Stopped),
        result = remote::probe(&client, url, headers, None) => result,
    };

    match (first, login) {
        (
            Err(Problem::LoginRequired {
                challenge: Some(challenge),
            }),
            Some(login),
        ) => {
            let auth = Arc::new(Authenticator::new(login, challenge));
            let second = tokio::select! {
                () = stop.cancelled() => return Err(Problem::Stopped),
                result = remote::probe(&client, url, headers, Some(&auth)) => result,
            };
            Ok((second?, Some(auth)))
        }
        (result, _) => Ok((result?, None)),
    }
}

/// Asks the server about the file, retrying temporary failures.
async fn probe(
    inner: &Inner,
    url: &str,
    headers: &HeaderMap,
    login: Option<Login>,
    stop: &CancellationToken,
) -> Result<Probed, Problem> {
    let policy = inner.config.retry;
    let mut failures = 0;

    loop {
        let result = probe_once(inner, url, headers, login.clone(), stop).await;

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
    let mut download = inner
        .store
        .get(id)
        .map_err(|error| error.to_string())?
        .ok_or("the download no longer exists")?;

    let mut headers = to_header_map(&download.headers).map_err(|error| error.to_string())?;
    headers.extend(secrets.clone());

    if download.is_stream {
        return attempt_stream(inner, id, download, headers, stop).await;
    }

    let login = inner.logins().get(&id).cloned();
    let (remote, auth) = match probe(inner, &download.url, &headers, login.clone(), stop).await {
        Ok(probed) => probed,
        Err(Problem::Stopped) => return Ok(Attempt::Done(Finish::Paused)),
        Err(Problem::Changed) => return Ok(Attempt::FileChanged),
        Err(problem) => return Err(problem.into_reason()),
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

    let live = inner.live().clone();
    let parsed_url = Url::parse(&remote.url).map_err(|error| error.to_string())?;
    let connections = live.connections_for(parsed_url.host_str().unwrap_or_default());
    let path = prepare_folders(inner, &mut download, &live, &file_name)?;
    let client = live.client;

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
        let map = SegmentMap::new(size, connections, min_split);
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

    let ftp = (parsed_url.scheme() == "ftp").then(|| inner.ftp_target(&parsed_url, login));

    let job = Arc::new(Job {
        client,
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
        limits: Arc::clone(&inner.limits),
        auth,
        ftp,
    });

    let transfer: Pin<Box<dyn Future<Output = Ended> + Send>> = if segmented {
        Box::pin(transfer::run_segments(Arc::clone(&job), connections))
    } else {
        Box::pin(transfer::run_stream(Arc::clone(&job)))
    };

    let ended = watch(inner, id, &*job, segmented, transfer).await;
    if segmented {
        job.checkpoint(inner, id).await?;
    }

    let transferred = job.transferred.load(Ordering::Relaxed);
    drop(job);

    let size = remote.size.unwrap_or(transferred);
    conclude(ended, &download, path, file_name, size).await
}

/// One pass at a stream. There's no file on a server to check first, so it starts from
/// however many parts are already safely on disk.
async fn attempt_stream(
    inner: &Inner,
    id: DownloadId,
    mut download: Download,
    headers: HeaderMap,
    stop: &CancellationToken,
) -> Result<Attempt, String> {
    let parts = inner
        .store
        .stream_parts(id)
        .map_err(|error| error.to_string())?;
    let first_url = parts
        .first()
        .map(|part| part.url.clone())
        .ok_or("the video has no parts")?;
    let file_name = download.file_name.clone().ok_or("the video has no name")?;

    let login = inner.logins().get(&id).cloned();
    let auth = match login {
        None => None,
        Some(login) => match probe(inner, &first_url, &headers, Some(login), stop).await {
            Ok((_, auth)) => auth,
            Err(Problem::Stopped) => return Ok(Attempt::Done(Finish::Paused)),
            Err(problem) => return Err(problem.into_reason()),
        },
    };

    let live = inner.live().clone();
    let parsed_url = Url::parse(&first_url).map_err(|error| error.to_string())?;
    let connections = live.connections_for(parsed_url.host_str().unwrap_or_default());
    let path = prepare_folders(inner, &mut download, &live, &file_name)?;

    // Only what the database counts as safe is kept. Anything past it may be incomplete.
    let on_disk = std::fs::metadata(&path).map_or(0, |file| file.len());
    let start = if download.parts_done > 0 && on_disk >= download.downloaded {
        Position {
            parts: download.parts_done,
            bytes: download.downloaded,
        }
    } else {
        Position::default()
    };

    let file = if start.parts > 0 {
        PartFile::open(&path).and_then(|file| file.set_len(start.bytes).map(|()| file))
    } else {
        inner
            .store
            .save_stream_progress(id, 0, 0)
            .map_err(|error| error.to_string())?;
        PartFile::create(&path, None)
    }
    .map_err(|error| format!("could not open the file: {error}"))?;

    let setup = parts::Setup {
        client: live.client,
        parts,
        headers,
        file: Arc::new(file),
        stop: stop.child_token(),
        policy: inner.config.retry,
        limits: Arc::clone(&inner.limits),
        auth,
    };
    let job = Arc::new(PartsJob::new(setup, start));

    let transfer = Box::pin(parts::run(Arc::clone(&job), connections));
    let ended = watch(inner, id, &*job, true, transfer).await;
    job.checkpoint(inner, id).await?;

    let size = job.position().bytes;
    drop(job);
    conclude(ended, &download, path, file_name, size).await
}

/// Decides where `.part` files go, and makes sure the folders exist.
fn prepare_folders(
    inner: &Inner,
    download: &mut Download,
    live: &LiveOptions,
    file_name: &str,
) -> Result<PathBuf, String> {
    if download.part_dir.is_none() {
        let part_dir = live
            .temp_dir
            .clone()
            .unwrap_or_else(|| download.dest_dir.clone());
        inner
            .store
            .set_part_dir(download.id, &part_dir)
            .map_err(|error| error.to_string())?;
        download.part_dir = Some(part_dir);
    }

    let path = part_path(download, file_name);
    for folder in [path.parent(), Some(download.dest_dir.as_path())]
        .into_iter()
        .flatten()
    {
        std::fs::create_dir_all(folder)
            .map_err(|error| format!("could not create the folder: {error}"))?;
    }
    Ok(path)
}

/// Turns the end of a transfer into a result, moving a finished file into place.
async fn conclude(
    ended: Ended,
    download: &Download,
    path: PathBuf,
    file_name: String,
    size: u64,
) -> Result<Attempt, String> {
    match ended {
        Ended::Finished => {
            // Moving between drives copies the whole file, so keep it off the async threads.
            let dest_dir = download.dest_dir.clone();
            let replacing = download
                .final_path
                .clone()
                .filter(|_| download.replace_on_finish);
            let target = tokio::task::spawn_blocking(move || {
                match replacing {
                    Some(target) => disk::replace(&path, &target).map(|()| target),
                    None => disk::finish(&path, &dest_dir, &file_name),
                }
                .map_err(|error| format!("could not finish the file: {error}"))
            })
            .await
            .map_err(|error| error.to_string())??;

            Ok(Attempt::Done(Finish::Completed { path: target, size }))
        }
        Ended::Stopped => Ok(Attempt::Done(Finish::Paused)),
        Ended::Changed => Ok(Attempt::FileChanged),
        Ended::Failed(reason) => Err(reason),
    }
}

/// What progress reports and checkpoints need to know about a running transfer.
trait Progress {
    fn received(&self) -> u64;
    fn total(&self) -> Option<u64>;
    fn segments(&self) -> Vec<Segment>;
    async fn checkpoint(&self, inner: &Inner, id: DownloadId) -> Result<(), String>;
}

impl Progress for Job {
    fn received(&self) -> u64 {
        self.transferred.load(Ordering::Relaxed)
    }

    fn total(&self) -> Option<u64> {
        self.size
    }

    fn segments(&self) -> Vec<Segment> {
        self.map().snapshot()
    }

    /// Makes progress safe. The order matters: the snapshot is taken first, so everything
    /// in it was written before the flush, and it's saved only after the flush succeeds.
    /// The database can then never claim bytes the file doesn't have.
    async fn checkpoint(&self, inner: &Inner, id: DownloadId) -> Result<(), String> {
        let snapshot = self.map().snapshot();
        let store = Arc::clone(&inner.store);
        let file = Arc::clone(&self.file);

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
}

impl Progress for PartsJob {
    fn received(&self) -> u64 {
        PartsJob::received(self)
    }

    fn total(&self) -> Option<u64> {
        self.estimated_total()
    }

    fn segments(&self) -> Vec<Segment> {
        Vec::new()
    }

    /// The same order as for a file: what's saved was written before the flush.
    async fn checkpoint(&self, inner: &Inner, id: DownloadId) -> Result<(), String> {
        let Position { parts, bytes } = self.position();
        let store = Arc::clone(&inner.store);
        let file = self.file();

        tokio::task::spawn_blocking(move || {
            file.sync()
                .map_err(|error| format!("could not flush the file: {error}"))?;
            store
                .save_stream_progress(id, parts, bytes)
                .map_err(|error| error.to_string())
        })
        .await
        .map_err(|error| error.to_string())?
    }
}

/// Reports progress and saves checkpoints until the transfer ends.
async fn watch<P: Progress>(
    inner: &Inner,
    id: DownloadId,
    job: &P,
    checkpoints: bool,
    mut transfer: Pin<Box<dyn Future<Output = Ended> + Send>>,
) -> Ended {
    let mut ticker = tokio::time::interval(inner.config.progress_interval);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

    let mut last_checkpoint = Instant::now();
    let mut last_tick = Instant::now();
    let mut last_bytes = job.received();
    let mut speed = 0.0;

    loop {
        tokio::select! {
            ended = &mut transfer => return ended,
            _ = ticker.tick() => {
                let bytes = job.received();
                let elapsed = last_tick.elapsed().as_secs_f64().max(0.001);
                let measured = bytes.saturating_sub(last_bytes) as f64 / elapsed;
                speed += SPEED_SMOOTHING * (measured - speed);
                last_tick = Instant::now();
                last_bytes = bytes;

                inner.emit(Event::Progress {
                    id,
                    downloaded: bytes,
                    total: job.total(),
                    bytes_per_second: speed as u64,
                    segments: if checkpoints { job.segments() } else { Vec::new() },
                });

                if checkpoints && last_checkpoint.elapsed() >= inner.config.checkpoint_interval {
                    // A missed checkpoint only means a little more to fetch after a
                    // crash, and the next one tries again.
                    let _ = job.checkpoint(inner, id).await;
                    last_checkpoint = Instant::now();
                }
            }
        }
    }
}
