//! The desktop app: connects the download engine to the window and the tray.

mod after_queue;
mod commands;
mod rules;
mod settings;

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use engine::{Config, Engine, Event, QueueId};
use serde::Serialize;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{App, AppHandle, Emitter, Manager, WindowEvent};
use tauri_plugin_autostart::MacosLauncher;
use tokio::sync::broadcast::error::RecvError;

use crate::after_queue::{AfterAction, SHUTDOWN_DELAY_SECONDS, WakeTimer};
use crate::commands::{AppState, apply_to_engine, state_name};
use crate::settings::Settings;

/// How often the wake timer is brought up to date.
const WAKE_TIMER_REFRESH: Duration = Duration::from_secs(60);

/// Passed when Windows starts the app at sign-in, so it opens in the tray.
const MINIMIZED_FLAG: &str = "--minimized";
const MAIN_WINDOW: &str = "main";

/// Keeps data in another folder, so automated tests never touch real downloads.
const DATA_DIR_VARIABLE: &str = "TRIBUTARY_DATA_DIR";

/// Lets an automated test run beside the user's own copy. Only honoured together with
/// [`DATA_DIR_VARIABLE`], because a single copy is what protects the shared database.
const SECOND_COPY_VARIABLE: &str = "TRIBUTARY_ALLOW_SECOND_COPY";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProgressPayload {
    id: i64,
    downloaded: u64,
    total: Option<u64>,
    bytes_per_second: u64,
    segments: Vec<SegmentPayload>,
}

#[derive(Debug, Clone, Serialize)]
struct SegmentPayload {
    start: u64,
    end: u64,
    written: u64,
}

#[derive(Debug, Clone, Serialize)]
struct StatePayload {
    id: i64,
    state: &'static str,
}

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Saves progress on every download before leaving.
fn quit(app: &AppHandle) {
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let _ = handle.state::<AppState>().stop_all().await;
        handle.exit(0);
    });
}

fn stop_all(app: &AppHandle) {
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let _ = handle.state::<AppState>().stop_all().await;
    });
}

fn build_tray(app: &App) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open Tributary", true, None::<&str>)?;
    let stop = MenuItem::with_id(app, "stop-all", "Stop all downloads", true, None::<&str>)?;
    let exit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &stop, &exit])?;

    let mut tray = TrayIconBuilder::with_id("main")
        .tooltip("Tributary")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_main_window(app),
            "stop-all" => stop_all(app),
            "quit" => quit(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        });

    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

/// Carries out what a queue was set to do when it ends.
fn run_after_action(app: &AppHandle, id: QueueId) {
    let state = app.state::<AppState>();
    let action = state
        .engine
        .queues()
        .ok()
        .and_then(|queues| queues.into_iter().find(|info| info.queue.id == id))
        .map(|info| AfterAction::parse(&info.queue.after_action))
        .unwrap_or_default();

    if !action.program.trim().is_empty() {
        let _ = after_queue::run_program(&action.program);
    }
    if action.shutdown && after_queue::start_shutdown(action.force).is_ok() {
        show_main_window(app);
        let _ = app.emit("shutdown-pending", SHUTDOWN_DELAY_SECONDS);
    }
}

/// Forwards engine events to the window for as long as the app runs.
fn forward_events(app: AppHandle, engine: &Engine) {
    let mut events = engine.subscribe();

    tauri::async_runtime::spawn(async move {
        loop {
            match events.recv().await {
                Ok(Event::Progress {
                    id,
                    downloaded,
                    total,
                    bytes_per_second,
                    segments,
                }) => {
                    let _ = app.emit(
                        "download-progress",
                        ProgressPayload {
                            id: id.0,
                            downloaded,
                            total,
                            bytes_per_second,
                            segments: segments
                                .into_iter()
                                .map(|segment| SegmentPayload {
                                    start: segment.start,
                                    end: segment.end,
                                    written: segment.written,
                                })
                                .collect(),
                        },
                    );
                }
                Ok(Event::State { id, state }) => {
                    let _ = app.emit(
                        "download-state",
                        StatePayload {
                            id: id.0,
                            state: state_name(state),
                        },
                    );
                }
                Ok(Event::Queues) => {
                    let _ = app.emit("queues-changed", ());
                }
                Ok(Event::QueueFinished { id }) => {
                    run_after_action(&app, id);
                    let _ = app.emit("queues-changed", ());
                }
                // A burst of events was missed. The next ones bring things up to date.
                Err(RecvError::Lagged(_)) => {}
                Err(RecvError::Closed) => return,
            }
        }
    });
}

fn setup(app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    let paths = app.path();
    let (data_dir, config_dir) = match std::env::var_os(DATA_DIR_VARIABLE) {
        Some(folder) => (PathBuf::from(&folder), PathBuf::from(folder)),
        None => (paths.app_data_dir()?, paths.app_config_dir()?),
    };
    let settings_path = config_dir.join("settings.json");
    let default_download_dir = paths.download_dir()?;
    std::fs::create_dir_all(&data_dir)?;

    let settings = Settings::load(&settings_path, &default_download_dir);
    let engine = Engine::open(&data_dir.join("downloads.db"), Config::default())?;
    apply_to_engine(&engine, &settings);
    forward_events(app.handle().clone(), &engine);

    let scheduler = engine.clone();
    tauri::async_runtime::spawn(async move { scheduler.run_scheduler().await });

    app.manage(AppState {
        engine,
        settings: Mutex::new(settings),
        settings_path,
        wake_timer: WakeTimer::new(),
    });
    build_tray(app)?;

    let handle = app.handle().clone();
    tauri::async_runtime::spawn(async move {
        loop {
            handle.state::<AppState>().update_wake_timer();
            tokio::time::sleep(WAKE_TIMER_REFRESH).await;
        }
    });

    if !std::env::args().any(|arg| arg == MINIMIZED_FLAG) {
        show_main_window(app.handle());
    }
    Ok(())
}

pub fn run() {
    let mut builder = tauri::Builder::default();

    let separate_copy = std::env::var_os(DATA_DIR_VARIABLE).is_some()
        && std::env::var_os(SECOND_COPY_VARIABLE).is_some();
    if !separate_copy {
        // Registered first so a second launch never reaches the shared database.
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main_window(app);
        }));
    }

    let result = builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec![MINIMIZED_FLAG]),
        ))
        .setup(setup)
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let app = window.app_handle();
                let to_tray = app
                    .state::<AppState>()
                    .settings
                    .lock()
                    .map(|settings| settings.close_to_tray)
                    .unwrap_or(true);

                api.prevent_close();
                if to_tray {
                    let _ = window.hide();
                } else {
                    quit(app);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_downloads,
            commands::inspect_url,
            commands::add_download,
            commands::pause_download,
            commands::resume_download,
            commands::stop_all,
            commands::remove_download,
            commands::edit_download,
            commands::open_file,
            commands::show_in_folder,
            commands::get_settings,
            commands::save_settings,
            commands::suggest_category,
            commands::list_queues,
            commands::save_queue,
            commands::delete_queue,
            commands::start_queue,
            commands::stop_queue,
            commands::set_download_queue,
            commands::reorder_queue,
            commands::cancel_shutdown,
        ])
        .run(tauri::generate_context!());

    if let Err(error) = result {
        eprintln!("Tributary stopped: {error}");
    }
}
