//! What the window can ask the app to do, and the shapes it sends and receives.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use engine::{
    Download, DownloadEdit, DownloadId, Engine, Login, ManualProxy, Moment, NetworkOptions,
    NewDownload, ProxyMode, Queue, QueueId, QueueInfo, QueueKind, Quota, ServerLimit, State,
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State as Managed};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_opener::OpenerExt as _;

use crate::after_queue::{self, AfterAction, WakeTimer, seconds_until_start};
use crate::rules::{match_category, site_matches};
use crate::settings::{MAX_CONNECTIONS, MIN_CONNECTIONS, ProxyChoice, Settings, SiteLogin};
use crate::{data, scanner, secrets};

const SECONDS_PER_HOUR: u64 = 3600;
const PROXY_KEY: &str = "proxy";

fn site_key(pattern: &str) -> String {
    format!("site/{pattern}")
}

/// Everything the commands share.
pub struct AppState {
    pub engine: Engine,
    pub settings: Mutex<Settings>,
    pub settings_path: PathBuf,
    pub wake_timer: Option<WakeTimer>,
}

impl AppState {
    fn settings(&self) -> MutexGuard<'_, Settings> {
        self.settings.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The saved login for a URL, if a site rule covers it.
    fn site_login_for(&self, url: &str) -> Option<Login> {
        let settings = self.settings();
        let site = settings
            .site_logins
            .iter()
            .find(|site| site_matches(&site.pattern, url))?;
        let (username, password) = secrets::read(&site_key(&site.pattern))?;
        Some(Login { username, password })
    }

    /// Gives every unfinished download its saved login, so queues can use them too.
    pub fn attach_site_logins(&self) {
        let Ok(downloads) = self.engine.list() else {
            return;
        };
        for download in downloads {
            if download.state != State::Completed
                && let Some(login) = self.site_login_for(&download.url)
            {
                self.engine.set_login(download.id, login);
            }
        }
    }

    /// Points the wake timer at the earliest start that asked for it.
    pub fn update_wake_timer(&self) {
        let Some(timer) = &self.wake_timer else {
            return;
        };
        let now = Moment::now();
        let next = self.engine.queues().ok().and_then(|queues| {
            queues
                .iter()
                .filter(|info| AfterAction::parse(&info.queue.after_action).wake)
                .filter_map(|info| seconds_until_start(&info.queue, now))
                .min()
        });

        match next {
            Some(seconds) => {
                timer.set(seconds);
            }
            None => timer.cancel(),
        }
    }

    /// Stops every running download, saving progress. Used by Stop All and on exit.
    pub async fn stop_all(&self) -> Result<(), engine::Error> {
        for download in self.engine.list()? {
            if download.state == State::Running {
                self.engine.pause(download.id).await?;
            }
        }
        Ok(())
    }
}

/// Passes settings the engine needs through to it.
pub fn apply_to_engine(engine: &Engine, settings: &Settings) -> Result<(), engine::Error> {
    engine.set_network(&network_options(settings))?;
    engine.set_server_limits(
        settings
            .server_rules
            .iter()
            .map(|rule| ServerLimit {
                pattern: rule.pattern.clone(),
                connections: rule.connections,
            })
            .collect(),
    );
    engine.set_connections(settings.connections);
    engine.set_temp_dir(settings.temp_dir.clone());
    engine.set_speed_limit(
        settings
            .speed_limit_enabled
            .then_some(settings.speed_limit_kb * 1024),
    );
    engine.set_quota(settings.quota_enabled.then_some(Quota {
        bytes: settings.quota_mb * 1024 * 1024,
        period: Duration::from_secs(settings.quota_hours.max(1) * SECONDS_PER_HOUR),
    }));
    Ok(())
}

fn network_options(settings: &Settings) -> NetworkOptions {
    let proxy = match settings.proxy_choice {
        ProxyChoice::System => ProxyMode::System,
        ProxyChoice::Direct => ProxyMode::Direct,
        ProxyChoice::Manual => ProxyMode::Manual(ManualProxy {
            url: settings.proxy_url.trim().to_owned(),
            login: (!settings.proxy_username.is_empty()).then(|| Login {
                username: settings.proxy_username.clone(),
                password: secrets::read(PROXY_KEY)
                    .map(|(_, password)| password)
                    .unwrap_or_default(),
            }),
            exceptions: settings.proxy_exceptions.clone(),
        }),
    };

    NetworkOptions {
        user_agent: Some(settings.user_agent.trim().to_owned()).filter(|text| !text.is_empty()),
        proxy,
        ftp_passive: settings.ftp_passive,
    }
}

pub fn state_name(state: State) -> &'static str {
    match state {
        State::Running => "running",
        State::Paused => "paused",
        State::Completed => "completed",
        State::Failed => "failed",
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadView {
    id: i64,
    url: String,
    file_name: Option<String>,
    dest_dir: PathBuf,
    final_path: Option<PathBuf>,
    size: Option<u64>,
    downloaded: u64,
    state: &'static str,
    error: Option<String>,
    description: Option<String>,
    resumable: bool,
    added_at: i64,
    completed_at: Option<i64>,
    queue_id: Option<i64>,
    category: Option<String>,
}

impl From<Download> for DownloadView {
    fn from(download: Download) -> Self {
        Self {
            id: download.id.0,
            url: download.url,
            file_name: download.file_name,
            dest_dir: download.dest_dir,
            final_path: download.final_path,
            size: download.size,
            downloaded: download.downloaded,
            state: state_name(download.state),
            error: download.error,
            description: download.description,
            resumable: download.ranged,
            added_at: download.added_at,
            completed_at: download.completed_at,
            queue_id: download.queue.map(|queue| queue.0),
            category: download.category,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileInfoView {
    file_name: String,
    size: Option<u64>,
    resumable: bool,
    is_web_page: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddRequest {
    url: String,
    dest_dir: Option<PathBuf>,
    file_name: Option<String>,
    description: Option<String>,
    #[serde(default)]
    paused: bool,
    queue_id: Option<i64>,
    category: Option<String>,
    username: Option<String>,
    password: Option<String>,
    #[serde(default)]
    remember_login: bool,
}

/// A queue as the window sees it. Zero as an id means a new queue.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueView {
    id: i64,
    name: String,
    sync: bool,
    concurrency: u32,
    retries: u32,
    start_minute: Option<u16>,
    stop_minute: Option<u16>,
    days: u8,
    sync_minutes: u32,
    after: AfterAction,
    #[serde(default)]
    active: bool,
    #[serde(default)]
    files: Vec<i64>,
}

impl From<QueueInfo> for QueueView {
    fn from(info: QueueInfo) -> Self {
        let queue = info.queue;
        Self {
            id: queue.id.0,
            sync: queue.kind == QueueKind::Sync,
            concurrency: queue.concurrency,
            retries: queue.retries,
            start_minute: queue.start_minute,
            stop_minute: queue.stop_minute,
            days: queue.days,
            sync_minutes: queue.sync_minutes,
            after: AfterAction::parse(&queue.after_action),
            name: queue.name,
            active: info.active,
            files: info.files.iter().map(|id| id.0).collect(),
        }
    }
}

impl From<QueueView> for Queue {
    fn from(view: QueueView) -> Self {
        Self {
            id: QueueId(view.id),
            name: view.name,
            kind: if view.sync {
                QueueKind::Sync
            } else {
                QueueKind::Download
            },
            concurrency: view.concurrency.max(1),
            retries: view.retries,
            start_minute: view.start_minute,
            stop_minute: view.stop_minute,
            days: view.days,
            sync_minutes: view.sync_minutes.max(1),
            after_action: view.after.to_text(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryChoice {
    name: Option<String>,
    folder: PathBuf,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditRequest {
    url: Option<String>,
    dest_dir: Option<PathBuf>,
    description: Option<String>,
}

fn text(error: impl ToString) -> String {
    error.to_string()
}

fn blank_to_none(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

#[tauri::command]
pub async fn list_downloads(state: Managed<'_, AppState>) -> Result<Vec<DownloadView>, String> {
    let downloads = state.engine.list().map_err(text)?;
    Ok(downloads.into_iter().map(DownloadView::from).collect())
}

#[tauri::command]
pub async fn inspect_url(
    state: Managed<'_, AppState>,
    url: String,
    username: Option<String>,
    password: Option<String>,
) -> Result<FileInfoView, String> {
    let url = url.trim();
    let login = login_from(username, password).or_else(|| state.site_login_for(url));
    let info = state.engine.inspect(url, &[], login).await.map_err(text)?;
    Ok(FileInfoView {
        file_name: info.file_name,
        size: info.size,
        resumable: info.resumable,
        is_web_page: info.is_web_page,
    })
}

fn login_from(username: Option<String>, password: Option<String>) -> Option<Login> {
    let username = blank_to_none(username)?;
    Some(Login {
        username,
        password: password.unwrap_or_default(),
    })
}

/// The host of a URL, which is what a remembered login applies to.
fn host_of(url: &str) -> Option<String> {
    url::Url::parse(url).ok()?.host_str().map(str::to_owned)
}

/// Saves a login: the password in Credential Manager, the rest in the settings.
fn remember_site(state: &AppState, pattern: &str, login: &Login) -> Result<(), String> {
    secrets::store(&site_key(pattern), &login.username, &login.password).map_err(text)?;

    let mut settings = state.settings();
    settings.site_logins.retain(|site| site.pattern != pattern);
    settings.site_logins.push(SiteLogin {
        pattern: pattern.to_owned(),
        username: login.username.clone(),
    });
    settings.save(&state.settings_path).map_err(text)
}

/// Picks a category and folder for a file, from the category rules and settings.
fn choose_category(settings: &Settings, file_name: &str, url: &str) -> CategoryChoice {
    let found = match_category(&settings.categories, file_name, url);
    CategoryChoice {
        name: found.map(|category| category.name.clone()),
        folder: found
            .and_then(|category| category.folder.clone())
            .unwrap_or_else(|| settings.default_folder().to_owned()),
    }
}

#[tauri::command]
pub async fn suggest_category(
    state: Managed<'_, AppState>,
    file_name: String,
    url: String,
) -> Result<CategoryChoice, String> {
    Ok(choose_category(&state.settings(), &file_name, &url))
}

#[tauri::command]
pub async fn add_download(
    state: Managed<'_, AppState>,
    request: AddRequest,
) -> Result<i64, String> {
    let url = request.url.trim().to_owned();
    let file_name = blank_to_none(request.file_name);

    // A download added without a dialog is sorted by its address alone.
    let name_for_rules = file_name.clone().unwrap_or_else(|| url_file_name(&url));
    let chosen = choose_category(&state.settings(), &name_for_rules, &url);
    let category = blank_to_none(request.category).or(chosen.name);
    let dest_dir = request
        .dest_dir
        .filter(|folder| !folder.as_os_str().is_empty())
        .unwrap_or(chosen.folder);

    let typed_login = login_from(request.username, request.password);
    if let (true, Some(login), Some(host)) = (request.remember_login, &typed_login, host_of(&url)) {
        remember_site(&state, &host, login)?;
    }
    let login = typed_login.or_else(|| state.site_login_for(&url));

    let id = state
        .engine
        .add(NewDownload {
            url: url.clone(),
            dest_dir: dest_dir.clone(),
            file_name,
            description: blank_to_none(request.description),
            headers: Vec::new(),
            paused: request.paused,
            queue: request.queue_id.map(QueueId),
            category,
            login,
        })
        .map_err(text)?;

    let mut settings = state.settings();
    settings.remember_url(&url);
    settings.last_folder = Some(dest_dir);
    settings.save(&state.settings_path).map_err(text)?;
    Ok(id.0)
}

/// The part of a URL's path after the last slash, ignoring any query.
fn url_file_name(url: &str) -> String {
    url.split(['?', '#'])
        .next()
        .and_then(|path| path.rsplit('/').next())
        .unwrap_or_default()
        .to_owned()
}

#[tauri::command]
pub async fn list_queues(state: Managed<'_, AppState>) -> Result<Vec<QueueView>, String> {
    let queues = state.engine.queues().map_err(text)?;
    Ok(queues.into_iter().map(QueueView::from).collect())
}

#[tauri::command]
pub async fn save_queue(state: Managed<'_, AppState>, queue: QueueView) -> Result<i64, String> {
    let id = state.engine.save_queue(&queue.into()).map_err(text)?;
    state.update_wake_timer();
    Ok(id.0)
}

#[tauri::command]
pub async fn delete_queue(state: Managed<'_, AppState>, id: i64) -> Result<(), String> {
    state.engine.delete_queue(QueueId(id)).await.map_err(text)?;
    state.update_wake_timer();
    Ok(())
}

#[tauri::command]
pub async fn start_queue(state: Managed<'_, AppState>, id: i64) -> Result<(), String> {
    state.engine.start_queue(QueueId(id)).map_err(text)
}

#[tauri::command]
pub async fn stop_queue(state: Managed<'_, AppState>, id: i64) -> Result<(), String> {
    state.engine.stop_queue(QueueId(id)).await.map_err(text)
}

#[tauri::command]
pub async fn set_download_queue(
    state: Managed<'_, AppState>,
    id: i64,
    queue_id: Option<i64>,
) -> Result<(), String> {
    state
        .engine
        .set_queue(DownloadId(id), queue_id.map(QueueId))
        .map_err(text)
}

#[tauri::command]
pub async fn reorder_queue(state: Managed<'_, AppState>, ids: Vec<i64>) -> Result<(), String> {
    let ids: Vec<DownloadId> = ids.into_iter().map(DownloadId).collect();
    state.engine.reorder_queue(&ids).map_err(text)
}

#[tauri::command]
pub async fn cancel_shutdown() -> Result<(), String> {
    after_queue::cancel_shutdown().map_err(text)
}

#[tauri::command]
pub async fn pause_download(state: Managed<'_, AppState>, id: i64) -> Result<(), String> {
    state.engine.pause(DownloadId(id)).await.map_err(text)
}

#[tauri::command]
pub async fn resume_download(state: Managed<'_, AppState>, id: i64) -> Result<(), String> {
    state.attach_site_logins();
    state.engine.resume(DownloadId(id), &[]).map_err(text)
}

/// Gives a download a login and tries it again.
#[tauri::command]
pub async fn login_and_resume(
    state: Managed<'_, AppState>,
    id: i64,
    username: String,
    password: String,
    remember: bool,
) -> Result<(), String> {
    let download = state.engine.get(DownloadId(id)).map_err(text)?;
    let login = Login { username, password };

    if remember && let Some(host) = host_of(&download.url) {
        remember_site(&state, &host, &login)?;
    }
    state.engine.set_login(DownloadId(id), login);
    state.engine.resume(DownloadId(id), &[]).map_err(text)
}

#[tauri::command]
pub async fn save_site_login(
    state: Managed<'_, AppState>,
    pattern: String,
    username: String,
    password: String,
) -> Result<(), String> {
    remember_site(&state, pattern.trim(), &Login { username, password })?;
    state.attach_site_logins();
    Ok(())
}

#[tauri::command]
pub async fn delete_site_login(
    state: Managed<'_, AppState>,
    pattern: String,
) -> Result<(), String> {
    secrets::delete(&site_key(&pattern));
    let mut settings = state.settings();
    settings.site_logins.retain(|site| site.pattern != pattern);
    settings.save(&state.settings_path).map_err(text)
}

#[tauri::command]
pub async fn set_proxy_password(
    state: Managed<'_, AppState>,
    password: String,
) -> Result<(), String> {
    let username = state.settings().proxy_username.clone();
    secrets::store(PROXY_KEY, &username, &password).map_err(text)?;
    let settings = state.settings().clone();
    apply_to_engine(&state.engine, &settings).map_err(text)
}

#[tauri::command]
pub async fn play_sound(path: String) -> Result<bool, String> {
    Ok(scanner::play_sound(&path))
}

#[tauri::command]
pub async fn export_data(state: Managed<'_, AppState>, path: PathBuf) -> Result<usize, String> {
    let settings = state.settings().clone();
    data::export_to(&path, &state.engine, &settings)
}

#[tauri::command]
pub async fn import_data(state: Managed<'_, AppState>, path: PathBuf) -> Result<usize, String> {
    let (imported, added) = data::import_from(&path, &state.engine)?;
    apply_to_engine(&state.engine, &imported).map_err(text)?;
    imported.save(&state.settings_path).map_err(text)?;
    *state.settings() = imported;
    Ok(added)
}

#[tauri::command]
pub async fn stop_all(state: Managed<'_, AppState>) -> Result<(), String> {
    state.stop_all().await.map_err(text)
}

#[tauri::command]
pub async fn remove_download(
    state: Managed<'_, AppState>,
    id: i64,
    delete_file: bool,
) -> Result<(), String> {
    state
        .engine
        .remove(DownloadId(id), delete_file)
        .await
        .map_err(text)
}

#[tauri::command]
pub async fn edit_download(
    state: Managed<'_, AppState>,
    id: i64,
    edit: EditRequest,
) -> Result<(), String> {
    state
        .engine
        .edit(
            DownloadId(id),
            DownloadEdit {
                url: blank_to_none(edit.url),
                dest_dir: edit
                    .dest_dir
                    .filter(|folder| !folder.as_os_str().is_empty()),
                description: edit.description,
            },
        )
        .map_err(text)
}

/// The finished file of a download, confirming it still exists.
fn finished_file(state: &AppState, id: i64) -> Result<PathBuf, String> {
    let download = state.engine.get(DownloadId(id)).map_err(text)?;
    download
        .final_path
        .filter(|path| Path::new(path).exists())
        .ok_or_else(|| "the file is no longer there".to_owned())
}

#[tauri::command]
pub async fn open_file(
    app: AppHandle,
    state: Managed<'_, AppState>,
    id: i64,
) -> Result<(), String> {
    let path = finished_file(&state, id)?;
    app.opener()
        .open_path(path.to_string_lossy(), None::<&str>)
        .map_err(text)
}

#[tauri::command]
pub async fn show_in_folder(
    app: AppHandle,
    state: Managed<'_, AppState>,
    id: i64,
) -> Result<(), String> {
    let download = state.engine.get(DownloadId(id)).map_err(text)?;
    match finished_file(&state, id) {
        Ok(path) => app.opener().reveal_item_in_dir(path).map_err(text),
        Err(_) => app
            .opener()
            .open_path(download.dest_dir.to_string_lossy(), None::<&str>)
            .map_err(text),
    }
}

#[tauri::command]
pub async fn get_settings(state: Managed<'_, AppState>) -> Result<Settings, String> {
    Ok(state.settings().clone())
}

#[tauri::command]
pub async fn save_settings(
    app: AppHandle,
    state: Managed<'_, AppState>,
    mut settings: Settings,
) -> Result<(), String> {
    settings.connections = settings.connections.clamp(MIN_CONNECTIONS, MAX_CONNECTIONS);

    let autolaunch = app.autolaunch();
    if autolaunch.is_enabled().unwrap_or(false) != settings.start_with_windows {
        let changed = if settings.start_with_windows {
            autolaunch.enable()
        } else {
            autolaunch.disable()
        };
        changed.map_err(text)?;
    }

    // Applied first, so a bad proxy address is refused before anything is saved.
    apply_to_engine(&state.engine, &settings).map_err(text)?;
    settings.save(&state.settings_path).map_err(text)?;
    *state.settings() = settings;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_text_counts_as_nothing() {
        assert_eq!(blank_to_none(None), None);
        assert_eq!(blank_to_none(Some("   ".to_owned())), None);
        assert_eq!(blank_to_none(Some(" a ".to_owned())).as_deref(), Some("a"));
    }

    #[test]
    fn file_names_come_from_the_url_path() {
        assert_eq!(url_file_name("https://x.test/a/song.mp3?t=1#x"), "song.mp3");
        assert_eq!(url_file_name("https://x.test/"), "");
    }

    #[test]
    fn a_category_folder_wins_over_the_default() {
        let mut settings = Settings {
            download_dir: PathBuf::from(r"C:\Downloads"),
            ..Settings::default()
        };
        if let Some(music) = settings.categories.iter_mut().find(|c| c.name == "Music") {
            music.folder = Some(PathBuf::from(r"D:\Music"));
        }

        let song = choose_category(&settings, "a.mp3", "https://x.test/a.mp3");
        assert_eq!(song.name.as_deref(), Some("Music"));
        assert_eq!(song.folder, PathBuf::from(r"D:\Music"));

        let other = choose_category(&settings, "a.xyz", "https://x.test/a.xyz");
        assert_eq!(other.name, None);
        assert_eq!(other.folder, PathBuf::from(r"C:\Downloads"));
    }

    #[test]
    fn queues_keep_their_settings_through_the_window() {
        let json = r#"{ "id": 3, "name": "Night", "sync": true, "concurrency": 0, "retries": 2,
            "startMinute": 60, "stopMinute": null, "days": 31, "syncMinutes": 0,
            "after": { "shutdown": true } }"#;
        let view: QueueView = serde_json::from_str(json).unwrap();
        let queue = Queue::from(view);

        assert_eq!(queue.id, QueueId(3));
        assert_eq!(queue.kind, QueueKind::Sync);
        assert_eq!(
            queue.concurrency, 1,
            "a queue must be able to run something"
        );
        assert_eq!(queue.sync_minutes, 1);
        assert!(AfterAction::parse(&queue.after_action).shutdown);
    }

    #[test]
    fn a_requests_optional_fields_can_be_left_out() {
        let request: AddRequest = serde_json::from_str(r#"{ "url": "http://x.test/a" }"#).unwrap();
        assert_eq!(request.url, "http://x.test/a");
        assert!(!request.paused);
    }
}
