//! Torrents: magnet links and `.torrent` files, downloaded with librqbit.
//!
//! Nothing torrent-related touches the network until the user adds a torrent: the session,
//! with its listening port and DHT, starts then, or at start-up only if torrents were left
//! in the list. By default a torrent shares only while it downloads, and is paused once it
//! has finished; a torrent can be set to keep sharing until it has sent as much as it got.
//! Torrents appear in the main list with ids below zero, which keeps them apart from the
//! engine's downloads.

use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::net::{Ipv4Addr, SocketAddr};
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use librqbit::api::TorrentIdOrHash;
use librqbit::limits::LimitsConfig;
use librqbit::{
    AddTorrent, AddTorrentOptions, AddTorrentResponse, ListenerOptions, ManagedTorrent, Session,
    SessionOptions, SessionPersistenceConfig, TorrentStatsState,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Mutex as AsyncMutex;

use crate::commands::AppState;

/// The largest `.torrent` file read.
const MAX_TORRENT_BYTES: usize = 10 << 20;

/// How long reading a torrent's file list may take, which for a magnet link means finding
/// peers who have it.
const LIST_TIME: Duration = Duration::from_secs(90);

/// For tests: no DHT, so nothing reaches the internet.
const NO_DHT_VARIABLE: &str = "TRIBUTARY_NO_DHT";
/// For tests: peers to try first, such as a local seeder, as `host:port,host:port`.
const PEERS_VARIABLE: &str = "TRIBUTARY_TORRENT_PEERS";

/// A torrent's row id in the list, below zero so it can't meet a download's.
pub fn row_id(torrent: usize) -> i64 {
    -(torrent as i64) - 1
}

/// The torrent a row id stands for, if it stands for one.
pub fn torrent_of(row: i64) -> Option<usize> {
    (row < 0).then(|| (-row - 1) as usize)
}

/// Whether an address is a torrent: a magnet link, or a `.torrent` file on the web.
pub fn is_torrent_address(text: &str) -> bool {
    let text = text.trim();
    if text
        .get(..7)
        .is_some_and(|start| start.eq_ignore_ascii_case("magnet:"))
    {
        return text.to_ascii_lowercase().contains("xt=urn:btih:");
    }
    url::Url::parse(text).is_ok_and(|url| {
        matches!(url.scheme(), "http" | "https")
            && url.path().to_ascii_lowercase().ends_with(".torrent")
    })
}

/// What to do with a torrent that has finished.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AfterFinishing {
    /// Stop sharing: pause it.
    Pause,
    /// Keep sharing for now.
    Share,
}

/// A finished torrent is paused, unless it was set to keep sharing and hasn't yet sent as
/// much as it got.
pub fn after_finishing(keep_sharing: bool, uploaded: u64, size: u64) -> AfterFinishing {
    if keep_sharing && uploaded < size {
        AfterFinishing::Share
    } else {
        AfterFinishing::Pause
    }
}

/// A file in a torrent, as the dialog lists it.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TorrentFile {
    pub name: String,
    pub size: u64,
}

/// What a torrent holds, read before it's added.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TorrentPreview {
    pub name: String,
    pub files: Vec<TorrentFile>,
}

/// A torrent as the list shows it.
#[derive(Debug, Clone)]
pub struct TorrentRow {
    pub id: i64,
    /// Its magnet link, which finds it again from its peers.
    pub address: String,
    pub name: String,
    pub folder: PathBuf,
    pub added_at: i64,
    pub size: u64,
    pub done: u64,
    pub state: &'static str,
    pub error: Option<String>,
}

/// The torrent being looked at in the dialog: its file, and peers already found for it.
struct Pending {
    bytes: bytes::Bytes,
    peers: Vec<SocketAddr>,
    name: String,
    /// More than one file, which go in a folder of the torrent's name.
    several: bool,
}

pub struct Torrents {
    folder: PathBuf,
    session: AsyncMutex<Option<Arc<Session>>>,
    pending: Mutex<Option<Pending>>,
    /// Torrents set to keep sharing until they've sent as much as they got.
    sharing: Mutex<HashSet<usize>>,
    /// Torrents already told of as finished.
    finished: Mutex<HashSet<usize>>,
    /// Where each torrent saves, which librqbit doesn't say.
    folders: Mutex<HashMap<usize, PathBuf>>,
    /// When each torrent was added, in seconds since 1970, for the list's Added column.
    added: Mutex<HashMap<usize, i64>>,
}

/// Tributary's own files beside librqbit's in the torrents folder.
const OWN_FILES: [&str; 3] = ["sharing.json", "folders.json", "added.json"];

fn read_json<T: serde::de::DeserializeOwned + Default>(path: &Path) -> T {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn seconds_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs() as i64)
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Torrents {
    pub fn new(data_dir: &Path) -> Self {
        let folder = data_dir.join("torrents");
        Self {
            session: AsyncMutex::new(None),
            pending: Mutex::new(None),
            sharing: Mutex::new(read_json(&folder.join("sharing.json"))),
            finished: Mutex::new(HashSet::new()),
            folders: Mutex::new(read_json(&folder.join("folders.json"))),
            added: Mutex::new(read_json(&folder.join("added.json"))),
            folder,
        }
    }

    /// Saves which torrents keep sharing, where each saves and when each was added.
    fn save_sharing(&self) {
        let sharing = serde_json::to_string(&*lock(&self.sharing)).unwrap_or_default();
        let folders = serde_json::to_string(&*lock(&self.folders)).unwrap_or_default();
        let added = serde_json::to_string(&*lock(&self.added)).unwrap_or_default();
        let _ = fs::create_dir_all(&self.folder);
        for (name, text) in OWN_FILES.into_iter().zip([sharing, folders, added]) {
            let _ = fs::write(self.folder.join(name), text);
        }
    }

    fn added_at(&self, id: usize) -> i64 {
        lock(&self.added).get(&id).copied().unwrap_or_default()
    }

    fn folder_of(&self, id: usize) -> PathBuf {
        lock(&self.folders).get(&id).cloned().unwrap_or_default()
    }

    /// Whether torrents were left in the list last time, so the session should start now.
    pub fn has_saved_torrents(&self) -> bool {
        fs::read_dir(&self.folder).is_ok_and(|entries| {
            entries.flatten().any(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                name.ends_with(".json") && !OWN_FILES.contains(&name.as_str())
            })
        })
    }

    /// The session, started the first time it's needed.
    pub async fn session(
        &self,
        download_dir: PathBuf,
        speed_limit: Option<u64>,
    ) -> Result<Arc<Session>, String> {
        let mut session = self.session.lock().await;
        if let Some(session) = session.as_ref() {
            return Ok(Arc::clone(session));
        }
        let started =
            Session::new_with_opts(download_dir, session_options(&self.folder, speed_limit))
                .await
                .map_err(|error| format!("torrents couldn't start: {error:#}"))?;
        *session = Some(Arc::clone(&started));
        Ok(started)
    }

    async fn running_session(&self) -> Option<Arc<Session>> {
        self.session.lock().await.clone()
    }
}

/// The session's settings: its saved list beside Tributary's data, a port of its own
/// choosing, no port forwarding and no announcing on the local network.
fn session_options(folder: &Path, speed_limit: Option<u64>) -> SessionOptions {
    let limit = speed_limit
        .and_then(|bytes| u32::try_from(bytes).ok())
        .and_then(NonZeroU32::new);
    SessionOptions {
        dht: if env::var_os(NO_DHT_VARIABLE).is_some() {
            None
        } else {
            Some(Default::default())
        },
        persistence: Some(SessionPersistenceConfig::Json {
            folder: Some(folder.to_owned()),
        }),
        fastresume: true,
        listen: Some(ListenerOptions {
            listen_addr: SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0)),
            enable_upnp_port_forwarding: false,
            ipv4_only: true,
            ..ListenerOptions::default()
        }),
        disable_local_service_discovery: true,
        ratelimits: LimitsConfig {
            download_bps: limit,
            upload_bps: limit,
        },
        ..SessionOptions::default()
    }
}

/// Peers to try first, from the test setting.
fn extra_peers() -> Vec<SocketAddr> {
    env::var(PEERS_VARIABLE)
        .map(|text| {
            text.split(',')
                .filter_map(|peer| peer.trim().parse().ok())
                .collect()
        })
        .unwrap_or_default()
}

fn settings_for_session(app: &AppHandle) -> (PathBuf, Option<u64>) {
    let settings = app.state::<AppState>().settings_snapshot();
    let limit = settings
        .speed_limit_enabled
        .then_some(settings.speed_limit_kb * 1024);
    (settings.download_dir.clone(), limit)
}

async fn session_for(app: &AppHandle) -> Result<Arc<Session>, String> {
    let (folder, limit) = settings_for_session(app);
    app.state::<Torrents>().session(folder, limit).await
}

/// Reads what a torrent holds without adding it: the files of a `.torrent` file at an
/// address, or of a magnet link, whose file list comes from the peers who have it.
#[tauri::command]
pub async fn inspect_torrent(app: AppHandle, url: String) -> Result<TorrentPreview, String> {
    let url = url.trim().to_owned();
    if !is_torrent_address(&url) {
        return Err("That isn't a magnet link or a .torrent file.".to_owned());
    }
    let session = session_for(&app).await?;
    let add = if url.to_ascii_lowercase().starts_with("magnet:") {
        AddTorrent::from_url(url.clone())
    } else {
        // Fetched without cookies, like any page Tributary reads for itself.
        let bytes = app
            .state::<AppState>()
            .engine
            .fetch_bytes(&url, &[], MAX_TORRENT_BYTES)
            .await?;
        AddTorrent::from_bytes(bytes)
    };
    let options = AddTorrentOptions {
        list_only: true,
        initial_peers: Some(extra_peers()).filter(|peers| !peers.is_empty()),
        ..AddTorrentOptions::default()
    };
    let listed = tokio::time::timeout(LIST_TIME, session.add_torrent(add, Some(options)))
        .await
        .map_err(|_| "No one sharing this torrent could be found in time.".to_owned())?
        .map_err(|error| format!("the torrent couldn't be read: {error:#}"))?;
    let list = match listed {
        AddTorrentResponse::ListOnly(list) => list,
        AddTorrentResponse::AlreadyManaged(..) | AddTorrentResponse::Added(..) => {
            return Err("This torrent is already in your list.".to_owned());
        }
    };
    let files: Vec<TorrentFile> = list
        .info
        .iter_file_details()
        .map(|file| TorrentFile {
            name: file.filename.to_vec().join("\\"),
            size: file.len,
        })
        .collect();
    let name = list
        .info
        .name()
        .map(|name| name.into_owned())
        .unwrap_or_else(|| "torrent".to_owned());
    let mut peers = list.seen_peers;
    peers.extend(extra_peers());
    *lock(&app.state::<Torrents>().pending) = Some(Pending {
        bytes: list.torrent_bytes,
        peers,
        name: name.clone(),
        several: files.len() > 1,
    });
    Ok(TorrentPreview { name, files })
}

/// Adds the torrent last looked at, with only the files chosen. Returns its row id.
#[tauri::command]
pub async fn add_torrent(
    app: AppHandle,
    chosen: Vec<usize>,
    dest_dir: PathBuf,
    keep_sharing: bool,
    later: bool,
) -> Result<i64, String> {
    if chosen.is_empty() {
        return Err("No files were chosen.".to_owned());
    }
    let torrents = app.state::<Torrents>();
    let pending = lock(&torrents.pending)
        .take()
        .ok_or("Read the torrent first.")?;
    let session = session_for(&app).await?;
    // A torrent of several files keeps them together in a folder of its name.
    let dest_dir = if pending.several {
        dest_dir.join(engine::name::sanitize(&pending.name))
    } else {
        dest_dir
    };
    let options = AddTorrentOptions {
        only_files: Some(chosen),
        output_folder: Some(dest_dir.to_string_lossy().into_owned()),
        paused: later,
        initial_peers: Some(pending.peers).filter(|peers| !peers.is_empty()),
        ..AddTorrentOptions::default()
    };
    let added = session
        .add_torrent(AddTorrent::from_bytes(pending.bytes), Some(options))
        .await
        .map_err(|error| format!("the torrent couldn't be added: {error:#}"))?;
    let handle = added.into_handle().ok_or("the torrent couldn't be added")?;
    if keep_sharing {
        lock(&torrents.sharing).insert(handle.id());
    }
    lock(&torrents.folders).insert(handle.id(), dest_dir);
    lock(&torrents.added)
        .entry(handle.id())
        .or_insert_with(seconds_now);
    torrents.save_sharing();
    let _ = app.emit("queues-changed", ());
    Ok(row_id(handle.id()))
}

fn state_of(torrent: &ManagedTorrent) -> (&'static str, Option<String>, u64, u64, u64, u64) {
    let stats = torrent.stats();
    let speed = stats
        .live
        .as_ref()
        .map(|live| (live.download_speed.mbps * 125_000.0) as u64)
        .unwrap_or_default();
    // librqbit counts whole pieces, and a piece can hold the end of a chosen file and the
    // start of one left out: the files' own lengths say what the user asked for.
    let lengths: Vec<u64> = torrent
        .with_metadata(|metadata| metadata.file_infos.iter().map(|file| file.len).collect())
        .unwrap_or_default();
    let (size, done) = if lengths.is_empty() {
        (stats.total_bytes, stats.progress_bytes)
    } else {
        let (size, done) = chosen_bytes(
            &lengths,
            &stats.file_progress,
            torrent.only_files().as_deref(),
        );
        // While it's still checking what's on disk there's no count per file yet.
        let done = if stats.file_progress.is_empty() {
            stats.progress_bytes.min(size)
        } else {
            done
        };
        (size, done)
    };
    let state = match (stats.state, stats.finished) {
        (TorrentStatsState::Error, _) => "failed",
        (_, true) => "completed",
        (TorrentStatsState::Paused, false) => "paused",
        (TorrentStatsState::Live | TorrentStatsState::Initializing { .. }, false) => "running",
    };
    (state, stats.error, size, done, stats.uploaded_bytes, speed)
}

/// The size of the files chosen, and how much of them is here, from each file's length and
/// bytes held. `None` means every file was chosen.
fn chosen_bytes(lengths: &[u64], have: &[u64], only: Option<&[usize]>) -> (u64, u64) {
    let chosen = |index: usize| only.is_none_or(|only| only.contains(&index));
    lengths
        .iter()
        .enumerate()
        .filter(|(index, _)| chosen(*index))
        .fold((0, 0), |(size, done), (index, length)| {
            let held = have.get(index).copied().unwrap_or_default().min(*length);
            (size + length, done + held)
        })
}

/// The torrents for the list.
pub async fn rows(app: &AppHandle) -> Vec<TorrentRow> {
    let torrents = app.state::<Torrents>();
    let Some(session) = torrents.running_session().await else {
        return Vec::new();
    };
    session.with_torrents(|all| {
        all.map(|(id, torrent)| {
            let (state, error, size, done, _, _) = state_of(torrent);
            TorrentRow {
                id: row_id(id),
                address: magnet_for(&torrent.info_hash().as_string()),
                name: torrent.name().unwrap_or_else(|| "torrent".to_owned()),
                folder: torrents.folder_of(id),
                added_at: torrents.added_at(id),
                size,
                done,
                state,
                error,
            }
        })
        .collect()
    })
}

/// The magnet link for a torrent's info hash, written in hex.
fn magnet_for(info_hash: &str) -> String {
    format!("magnet:?xt=urn:btih:{info_hash}")
}

/// Starts the session at start-up if torrents were left in the list, then reports their
/// progress every second, pauses those that have finished sharing, and says when one is done.
pub fn watch(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let torrents = app.state::<Torrents>();
        if torrents.has_saved_torrents() {
            let _ = session_for(&app).await;
        }
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            let Some(session) = torrents.running_session().await else {
                continue;
            };
            let handles: Vec<(usize, Arc<ManagedTorrent>)> = session
                .with_torrents(|all| all.map(|(id, handle)| (id, Arc::clone(handle))).collect());
            for (id, handle) in handles {
                let (state, _, size, done, uploaded, speed) = state_of(&handle);
                if state == "running" {
                    let _ = app.emit(
                        "download-progress",
                        serde_json::json!({
                            "id": row_id(id), "downloaded": done, "total": size,
                            "bytesPerSecond": speed, "segments": [],
                        }),
                    );
                }
                if state != "completed" {
                    continue;
                }
                let keep = lock(&torrents.sharing).contains(&id);
                if !handle.is_paused()
                    && after_finishing(keep, uploaded, size) == AfterFinishing::Pause
                {
                    let _ = session.pause(&handle).await;
                    if keep {
                        lock(&torrents.sharing).remove(&id);
                        torrents.save_sharing();
                    }
                }
                if lock(&torrents.finished).insert(id) {
                    let _ = app.emit(
                        "download-state",
                        serde_json::json!({ "id": row_id(id), "state": "completed" }),
                    );
                    let _ = app.emit("queues-changed", ());
                }
            }
        }
    });
}

async fn with_handle(
    app: &AppHandle,
    row: i64,
) -> Result<(Arc<Session>, Arc<ManagedTorrent>), String> {
    let id = torrent_of(row).ok_or("not a torrent")?;
    let session = app
        .state::<Torrents>()
        .running_session()
        .await
        .ok_or("torrents aren't running")?;
    let handle = session
        .get(TorrentIdOrHash::Id(id))
        .ok_or("that torrent is no longer in the list")?;
    Ok((session, handle))
}

pub async fn pause(app: &AppHandle, row: i64) -> Result<(), String> {
    let (session, handle) = with_handle(app, row).await?;
    session
        .pause(&handle)
        .await
        .map_err(|error| error.to_string())
}

pub async fn resume(app: &AppHandle, row: i64) -> Result<(), String> {
    let (session, handle) = with_handle(app, row).await?;
    if !handle.is_paused() {
        return Ok(());
    }
    session
        .unpause(&handle)
        .await
        .map_err(|error| error.to_string())
}

pub async fn remove(app: &AppHandle, row: i64, delete_files: bool) -> Result<(), String> {
    let (session, handle) = with_handle(app, row).await?;
    let id = handle.id();
    session
        .delete(TorrentIdOrHash::Id(id), delete_files)
        .await
        .map_err(|error| error.to_string())?;
    let torrents = app.state::<Torrents>();
    lock(&torrents.sharing).remove(&id);
    lock(&torrents.finished).remove(&id);
    lock(&torrents.folders).remove(&id);
    lock(&torrents.added).remove(&id);
    torrents.save_sharing();
    Ok(())
}

/// The folder a torrent saves into, for Show in folder.
pub async fn folder_of(app: &AppHandle, row: i64) -> Result<PathBuf, String> {
    let (_, handle) = with_handle(app, row).await?;
    Ok(app.state::<Torrents>().folder_of(handle.id()))
}

/// Opens Windows' Default apps page at Tributary, so the user can choose it for magnet links.
#[tauri::command]
pub async fn choose_for_magnet_links(app: AppHandle) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt as _;
    app.opener()
        .open_url(
            "ms-settings:defaultapps?registeredAppUser=Tributary",
            None::<&str>,
        )
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use librqbit::{CreateTorrentOptions, create_torrent};
    use sha2::{Digest, Sha256};

    use super::*;

    #[test]
    fn torrents_have_ids_of_their_own() {
        for id in [0usize, 1, 7, 100_000] {
            assert!(row_id(id) < 0);
            assert_eq!(torrent_of(row_id(id)), Some(id));
        }
        assert_eq!(torrent_of(0), None);
        assert_eq!(torrent_of(12), None);
    }

    #[test]
    fn torrent_addresses_are_recognised() {
        assert!(is_torrent_address(
            "magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567&dn=a"
        ));
        assert!(is_torrent_address(
            "MAGNET:?dn=x&xt=urn:btih:0123456789abcdef0123456789abcdef01234567"
        ));
        assert!(is_torrent_address(
            "https://releases.test/ubuntu.iso.torrent?x=1"
        ));
        assert!(!is_torrent_address("magnet:?dn=no-hash"));
        assert!(!is_torrent_address("https://x.test/a.iso"));
        assert!(!is_torrent_address("file:///C:/a.torrent"));
        // A torrent row's own address can be added again.
        let own = magnet_for("0123456789abcdef0123456789abcdef01234567");
        assert!(is_torrent_address(&own), "{own}");
    }

    #[test]
    fn only_the_chosen_files_count_towards_size_and_progress() {
        // b.bin was left out, though a piece it shares with a.bin has arrived.
        assert_eq!(
            chosen_bytes(&[1_500, 700], &[1_500, 200], Some(&[0])),
            (1_500, 1_500)
        );
        assert_eq!(chosen_bytes(&[1_500, 700], &[300, 0], None), (2_200, 300));
        assert_eq!(chosen_bytes(&[10, 20, 30], &[], Some(&[1, 2])), (50, 0));
    }

    #[test]
    fn a_finished_torrent_stops_sharing_unless_asked_to_keep_on() {
        assert_eq!(after_finishing(false, 0, 100), AfterFinishing::Pause);
        assert_eq!(after_finishing(true, 40, 100), AfterFinishing::Share);
        assert_eq!(after_finishing(true, 100, 100), AfterFinishing::Pause);
    }

    fn sha256_of(path: &Path) -> String {
        Sha256::digest(fs::read(path).unwrap())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    fn quiet_session_options(folder: &Path) -> SessionOptions {
        SessionOptions {
            dht: None,
            persistence: Some(SessionPersistenceConfig::Json {
                folder: Some(folder.to_owned()),
            }),
            listen: Some(ListenerOptions {
                listen_addr: SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
                enable_upnp_port_forwarding: false,
                ipv4_only: true,
                ..ListenerOptions::default()
            }),
            disable_local_service_discovery: true,
            disable_trackers: true,
            ..SessionOptions::default()
        }
    }

    /// A seeder and a downloader on this computer only: the chosen file arrives whole, and
    /// the one left out never does.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_torrent_downloads_only_the_files_chosen() {
        let root = tempfile::tempdir().unwrap();
        let shared = root.path().join("seed").join("album");
        fs::create_dir_all(&shared).unwrap();
        let wanted: Vec<u8> = (0..3_000_000u32)
            .map(|index| (index * 7 % 251) as u8)
            .collect();
        fs::write(shared.join("wanted.bin"), &wanted).unwrap();
        fs::write(shared.join("skipped.bin"), vec![9u8; 2_000_000]).unwrap();

        let made = create_torrent(
            &shared,
            CreateTorrentOptions::default(),
            &librqbit::spawn_utils::BlockingSpawner::new(2),
        )
        .await
        .unwrap();
        let bytes = made.as_bytes().unwrap();

        let seeder = Session::new_with_opts(
            root.path().join("seed"),
            quiet_session_options(&root.path().join("seed-state")),
        )
        .await
        .unwrap();
        let seeding = seeder
            .add_torrent(
                AddTorrent::from_bytes(bytes.clone()),
                Some(AddTorrentOptions {
                    overwrite: true,
                    // Given a folder, the files go straight into it, so the seeder points
                    // at the folder that holds them.
                    output_folder: Some(shared.to_string_lossy().into_owned()),
                    ..AddTorrentOptions::default()
                }),
            )
            .await
            .unwrap()
            .into_handle()
            .unwrap();
        tokio::time::timeout(Duration::from_secs(30), seeding.wait_until_completed())
            .await
            .expect("the seeder never found its own files")
            .unwrap();
        let seeder_at = seeder.listen_addr().unwrap();

        let leecher = Session::new_with_opts(
            root.path().join("out"),
            quiet_session_options(&root.path().join("out-state")),
        )
        .await
        .unwrap();
        let listed = match leecher
            .add_torrent(
                AddTorrent::from_bytes(bytes.clone()),
                Some(AddTorrentOptions {
                    list_only: true,
                    ..AddTorrentOptions::default()
                }),
            )
            .await
            .unwrap()
        {
            AddTorrentResponse::ListOnly(list) => list,
            _ => panic!("expected only a list"),
        };
        let names: Vec<String> = listed
            .info
            .iter_file_details()
            .map(|file| file.filename.to_vec().join("\\"))
            .collect();
        let wanted_index = names.iter().position(|name| name == "wanted.bin").unwrap();

        let handle = leecher
            .add_torrent(
                AddTorrent::from_bytes(bytes),
                Some(AddTorrentOptions {
                    only_files: Some(vec![wanted_index]),
                    output_folder: Some(root.path().join("out").to_string_lossy().into_owned()),
                    initial_peers: Some(vec![SocketAddr::from((
                        Ipv4Addr::LOCALHOST,
                        seeder_at.port(),
                    ))]),
                    ..AddTorrentOptions::default()
                }),
            )
            .await
            .unwrap()
            .into_handle()
            .unwrap();
        let began = Instant::now();
        tokio::time::timeout(Duration::from_secs(60), handle.wait_until_completed())
            .await
            .expect("the download never finished")
            .unwrap();
        assert!(began.elapsed() < Duration::from_secs(60));

        let out = root.path().join("out");
        assert_eq!(sha256_of(&out.join("wanted.bin")), {
            Sha256::digest(&wanted)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        });
        // A piece that straddles two files is fetched whole, so a file left out can get the
        // edge of one piece, but no more.
        let skipped = out.join("skipped.bin");
        let piece = u64::from(listed.info.info().piece_length);
        let fetched = if skipped.exists() {
            fs::read(&skipped)
                .unwrap()
                .iter()
                .filter(|byte| **byte == 9)
                .count() as u64
        } else {
            0
        };
        assert!(
            fetched <= piece,
            "{fetched} bytes of the skipped file came, a piece is {piece}"
        );
        assert_eq!(state_of(&handle).0, "completed");
    }
}
