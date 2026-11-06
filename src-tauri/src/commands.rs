//! What the window can ask the app to do, and the shapes it sends and receives.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use engine::{Download, DownloadEdit, DownloadId, Engine, NewDownload, State};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State as Managed};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_opener::OpenerExt as _;

use crate::settings::{MAX_CONNECTIONS, MIN_CONNECTIONS, Settings};

/// Everything the commands share.
pub struct AppState {
    pub engine: Engine,
    pub settings: Mutex<Settings>,
    pub settings_path: PathBuf,
}

impl AppState {
    fn settings(&self) -> MutexGuard<'_, Settings> {
        self.settings.lock().unwrap_or_else(PoisonError::into_inner)
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
pub fn apply_to_engine(engine: &Engine, settings: &Settings) {
    engine.set_connections(settings.connections);
    engine.set_temp_dir(settings.temp_dir.clone());
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
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileInfoView {
    file_name: String,
    size: Option<u64>,
    resumable: bool,
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
) -> Result<FileInfoView, String> {
    let info = state.engine.inspect(url.trim(), &[]).await.map_err(text)?;
    Ok(FileInfoView {
        file_name: info.file_name,
        size: info.size,
        resumable: info.resumable,
    })
}

#[tauri::command]
pub async fn add_download(
    state: Managed<'_, AppState>,
    request: AddRequest,
) -> Result<i64, String> {
    let url = request.url.trim().to_owned();
    let dest_dir = request
        .dest_dir
        .filter(|folder| !folder.as_os_str().is_empty())
        .unwrap_or_else(|| state.settings().download_dir.clone());

    let id = state
        .engine
        .add(NewDownload {
            url: url.clone(),
            dest_dir,
            file_name: blank_to_none(request.file_name),
            description: blank_to_none(request.description),
            headers: Vec::new(),
            paused: request.paused,
        })
        .map_err(text)?;

    let mut settings = state.settings();
    settings.remember_url(&url);
    settings.save(&state.settings_path).map_err(text)?;
    Ok(id.0)
}

#[tauri::command]
pub async fn pause_download(state: Managed<'_, AppState>, id: i64) -> Result<(), String> {
    state.engine.pause(DownloadId(id)).await.map_err(text)
}

#[tauri::command]
pub async fn resume_download(state: Managed<'_, AppState>, id: i64) -> Result<(), String> {
    state.engine.resume(DownloadId(id), &[]).map_err(text)
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

    settings.save(&state.settings_path).map_err(text)?;
    apply_to_engine(&state.engine, &settings);
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
    fn a_requests_optional_fields_can_be_left_out() {
        let request: AddRequest = serde_json::from_str(r#"{ "url": "http://x.test/a" }"#).unwrap();
        assert_eq!(request.url, "http://x.test/a");
        assert!(!request.paused);
    }
}
