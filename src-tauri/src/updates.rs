//! Keeping Tributary up to date. It looks for a newer release once a day, fetches it in the
//! background, and installs it only when the user says so and nothing is downloading.
//! Every release is signed, and one whose signature doesn't match the key built into the
//! app is refused.

use std::env;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use engine::State;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Url};
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::commands::AppState;

/// How long after starting the first look waits, so it never slows the app opening.
const FIRST_LOOK: Duration = Duration::from_secs(60);
const BETWEEN_LOOKS: Duration = Duration::from_secs(24 * 60 * 60);

/// Points the updater somewhere else, for tests.
const ADDRESS_VARIABLE: &str = "TRIBUTARY_UPDATE_URL";

/// What the window is told about a release waiting to be installed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub notes: String,
}

/// A release that's been fetched and checked, waiting for the user.
#[derive(Default)]
pub struct Pending {
    ready: Option<(Update, Vec<u8>)>,
}

impl Pending {
    fn info(&self) -> Option<UpdateInfo> {
        self.ready.as_ref().map(|(update, _)| UpdateInfo {
            version: update.version.clone(),
            notes: update.body.clone().unwrap_or_default(),
        })
    }
}

fn pending(app: &AppHandle) -> MutexGuard<'_, Pending> {
    app.state::<Mutex<Pending>>()
        .inner()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

/// Looks for a newer release now and then, while that's turned on.
pub fn watch(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST_LOOK).await;
        loop {
            let wanted = app
                .state::<AppState>()
                .settings_snapshot()
                .check_for_updates;
            if wanted {
                // A failed look, say with no connection, just waits for the next one.
                let _ = look(&app).await;
            }
            tokio::time::sleep(BETWEEN_LOOKS).await;
        }
    });
}

/// Looks for a newer release, and fetches it if there is one. Returns what's waiting.
async fn look(app: &AppHandle) -> Result<Option<UpdateInfo>, String> {
    if let Some(info) = pending(app).info() {
        return Ok(Some(info));
    }
    let updater = match env::var(ADDRESS_VARIABLE) {
        Ok(address) => {
            let address = Url::parse(&address).map_err(|error| error.to_string())?;
            app.updater_builder()
                .endpoints(vec![address])
                .and_then(|builder| builder.build())
        }
        Err(_) => app.updater(),
    }
    .map_err(|error| error.to_string())?;

    let Some(update) = updater.check().await.map_err(|error| error.to_string())? else {
        return Ok(None);
    };
    // Checked against the signature as it arrives, so a tampered file is never kept.
    let bytes = update
        .download(|_, _| {}, || {})
        .await
        .map_err(|error| error.to_string())?;
    let mut waiting = pending(app);
    waiting.ready = Some((update, bytes));
    let info = waiting.info();
    drop(waiting);
    if let Some(info) = &info {
        let _ = app.emit("update-ready", info.clone());
    }
    Ok(info)
}

/// What's running that an update would cut short.
fn busy(state: &AppState) -> bool {
    state.is_joining()
        || state.engine.list().is_ok_and(|downloads| {
            downloads
                .iter()
                .any(|download| download.state == State::Running)
        })
}

/// Looks for a newer release now, as Options' "Check now" asks.
#[tauri::command]
pub async fn check_for_update(app: AppHandle) -> Result<Option<UpdateInfo>, String> {
    look(&app).await
}

/// A release already waiting, for a window that opens after it was fetched.
#[tauri::command]
pub async fn waiting_update(app: AppHandle) -> Result<Option<UpdateInfo>, String> {
    Ok(pending(&app).info())
}

/// The version running now.
#[tauri::command]
pub async fn app_version(app: AppHandle) -> Result<String, String> {
    Ok(app.package_info().version.to_string())
}

/// Installs the waiting release and starts the new version. Never while files are
/// downloading or videos are being finished, so nothing is cut short.
#[tauri::command]
pub async fn restart_to_update(app: AppHandle) -> Result<(), String> {
    if busy(&app.state::<AppState>()) {
        return Err(BUSY.to_owned());
    }
    let ready = pending(&app).ready.take();
    let Some((update, bytes)) = ready else {
        return Err(text!("There's no update waiting."));
    };
    if let Err(error) = update.install(&bytes) {
        // Kept, so the user can try again.
        pending(&app).ready = Some((update, bytes));
        return Err(text!("The update couldn't be installed: {error}"));
    }
    app.restart();
}

pub const BUSY: &str = en!(
    "Tributary will update once nothing is downloading. Stop the downloads running now, or wait for them to finish, then restart to update."
);
