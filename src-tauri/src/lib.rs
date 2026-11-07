//! The desktop app: connects the download engine to the window and the tray.

mod after_queue;
mod capture;
mod commands;
mod data;
mod rules;
mod scanner;
mod secrets;
mod settings;
mod site_grabber;
mod system;
mod video;

use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use engine::{Config, DownloadId, Engine, Event, QueueId, State};
use serde::Serialize;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{App, AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use tauri_plugin_autostart::MacosLauncher;
use tokio::sync::broadcast::error::RecvError;

use crate::after_queue::{AfterAction, SHUTDOWN_DELAY_SECONDS, WakeTimer};
use crate::capture::Captured;
use crate::commands::{AddRequest, AppState, add_download_now, apply_to_engine, state_name};
use crate::scanner::ScanResult;
use crate::settings::{Settings, Sounds};
use crate::site_grabber::Grabber;
use crate::system::Invocation;

/// How often the wake timer is brought up to date.
const WAKE_TIMER_REFRESH: Duration = Duration::from_secs(60);

/// How often the clipboard is checked for changes.
const CLIPBOARD_INTERVAL: Duration = Duration::from_secs(1);

const HOST_FILE: &str = "tributary-host.exe";
const DROP_TARGET_WINDOW: &str = "droptarget";

/// Passed when Windows starts the app at sign-in, so it opens in the tray.
const MINIMIZED_FLAG: &str = "--minimized";
const MAIN_WINDOW: &str = "main";

/// Keeps data in another folder, so automated tests never touch real downloads.
const DATA_DIR_VARIABLE: &str = "TRIBUTARY_DATA_DIR";

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

pub(crate) fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Shows or hides the small window links can be dropped on.
pub(crate) fn sync_drop_target(app: &AppHandle, show: bool) {
    match (app.get_webview_window(DROP_TARGET_WINDOW), show) {
        (Some(window), false) => {
            let _ = window.close();
        }
        (None, true) => {
            let _ = WebviewWindowBuilder::new(
                app,
                DROP_TARGET_WINDOW,
                WebviewUrl::App("index.html#drop".into()),
            )
            .title("Tributary drop target")
            .inner_size(84.0, 84.0)
            .decorations(false)
            .resizable(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .disable_drag_drop_handler()
            .build();
        }
        _ => {}
    }
}

/// Carries out what a command line asked for. A command line arrives on the main thread,
/// and starting a download needs the async runtime, which that thread isn't part of.
fn handle_arguments(app: &AppHandle, arguments: &[String]) {
    let Ok(invocation) = system::parse_arguments(arguments) else {
        return;
    };
    let app = app.clone();
    tauri::async_runtime::spawn(async move { carry_out(app, invocation).await });
}

async fn carry_out(handle: AppHandle, invocation: Invocation) {
    let app = &handle;
    let state = app.state::<AppState>();

    if let Some(add) = invocation.add {
        let file = Captured {
            url: add.url,
            file_name: add.name,
            folder: add.folder,
            ..Captured::default()
        };
        if file.is_stream() && (add.later || add.silent) {
            video::add_best(handle.clone(), file, add.later, add.quit_when_done).await;
        } else if add.later {
            let _ = add_download_now(&state, AddRequest::from_captured(file).later(), Vec::new());
        } else if add.silent {
            let added = add_download_now(&state, AddRequest::from_captured(file), Vec::new());
            if let (true, Ok(id)) = (add.quit_when_done, added) {
                state.quit_after(id);
                // A small file can finish before anything was listening for it.
                let id = DownloadId(id);
                if state
                    .engine
                    .get(id)
                    .is_ok_and(|download| download.state == State::Completed)
                {
                    quit_if_asked(app, id);
                }
            }
        } else {
            let _ = capture::receive(app, vec![file], false);
        }
    }

    if invocation.start_queue {
        let _ = state.engine.start_queue(QueueId::MAIN);
    }
}

/// Offers links copied to the clipboard, if that's turned on.
fn watch_clipboard(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut last_sequence = system::clipboard_sequence();
        loop {
            tokio::time::sleep(CLIPBOARD_INTERVAL).await;

            let sequence = system::clipboard_sequence();
            if sequence == last_sequence {
                continue;
            }
            last_sequence = sequence;

            let settings = app.state::<AppState>().settings_snapshot();
            if !settings.watch_clipboard {
                continue;
            }
            let link = system::clipboard_text()
                .and_then(|text| system::download_link(&text, &settings.capture_types));
            if let Some(url) = link {
                let file = Captured {
                    url,
                    ..Captured::default()
                };
                let _ = capture::receive(&app, vec![file], false);
            }
        }
    });
}

/// Saves progress on every download before leaving.
fn quit(app: &AppHandle) {
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let _ = handle.state::<AppState>().stop_all().await;
        handle.exit(0);
    });
}

/// Closes the app once every download a command line was waiting for has finished. It
/// stays open if it's still needed: for another download, or for a virus scan whose
/// warning would otherwise never be seen.
fn quit_if_asked(app: &AppHandle, id: DownloadId) {
    let state = app.state::<AppState>();
    if !state.finished_waiting_for(id.0) {
        return;
    }

    let busy = state
        .engine
        .list()
        .is_ok_and(|downloads| downloads.iter().any(|item| item.state == State::Running));
    if !busy && !state.is_joining() && !state.settings_snapshot().scan_enabled {
        quit(app);
    }
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ScanPayload {
    id: i64,
    file: String,
    /// Empty when the file is clean.
    problem: String,
}

/// Runs the virus scanner on a finished file, if that's turned on.
fn scan_finished_file(app: &AppHandle, id: DownloadId) {
    let state = app.state::<AppState>();
    let settings = state
        .settings
        .lock()
        .map(|settings| settings.clone())
        .unwrap_or_default();
    if !settings.scan_enabled {
        return;
    }
    let Some(path) = state
        .engine
        .get(id)
        .ok()
        .and_then(|download| download.final_path)
    else {
        return;
    };

    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let result = scanner::scan(&settings.scanner_path, &settings.scanner_arguments, &path);
        let problem = match result {
            ScanResult::Clean => String::new(),
            ScanResult::Flagged(code) => format!("the scanner reported a problem (code {code})"),
            ScanResult::Failed(reason) => format!("the scanner couldn't run: {reason}"),
        };
        let _ = handle.emit(
            "scan-finished",
            ScanPayload {
                id: id.0,
                file: path.to_string_lossy().into_owned(),
                problem,
            },
        );
    });
}

/// Plays the sound the user chose for an event, if any.
fn play_event_sound(app: &AppHandle, choose: impl Fn(&Sounds) -> &str) {
    if let Ok(settings) = app.state::<AppState>().settings.lock() {
        let sound = choose(&settings.sounds);
        if !sound.is_empty() {
            scanner::play_sound(sound);
        }
    }
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
                    match state {
                        State::Completed => {
                            play_event_sound(&app, |sounds| &sounds.complete);
                            scan_finished_file(&app, id);
                            video::join_if_ready(&app, id);
                            quit_if_asked(&app, id);
                        }
                        State::Failed => play_event_sound(&app, |sounds| &sounds.failed),
                        State::Running | State::Paused => {}
                    }
                }
                Ok(Event::Queues) => {
                    let _ = app.emit("queues-changed", ());
                }
                Ok(Event::QueueFinished { id }) => {
                    play_event_sound(&app, |sounds| &sounds.queue);
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
    // A bad saved proxy shouldn't stop the app opening. Options can correct it.
    let _ = apply_to_engine(&engine, &settings);
    forward_events(app.handle().clone(), &engine);

    let scheduler = engine.clone();
    tauri::async_runtime::spawn(async move { scheduler.run_scheduler().await });

    app.manage(AppState {
        engine,
        settings: Mutex::new(settings),
        settings_path,
        wake_timer: WakeTimer::new(),
        pending: Mutex::default(),
        quit_after: Mutex::default(),
        video_choices: Mutex::default(),
        joining: Mutex::default(),
        grabber: Grabber::open(data_dir.join("projects.json")),
    });
    app.state::<AppState>().attach_site_logins();
    build_tray(app)?;

    capture::serve(app.handle().clone());
    watch_clipboard(app.handle().clone());
    site_grabber::watch_schedule(app.handle().clone());

    // Only an installed copy registers, so a test can't redirect the real browsers.
    if std::env::var_os(DATA_DIR_VARIABLE).is_none()
        && let Ok(resources) = app.path().resource_dir()
    {
        let host = resources.join(HOST_FILE);
        if host.exists() {
            let _ = system::register_with_browsers(&data_dir, &host);
        }
    }

    let show_drop_target = app.state::<AppState>().settings_snapshot().show_drop_target;
    sync_drop_target(app.handle(), show_drop_target);

    let handle = app.handle().clone();
    tauri::async_runtime::spawn(async move {
        loop {
            handle.state::<AppState>().update_wake_timer();
            tokio::time::sleep(WAKE_TIMER_REFRESH).await;
        }
    });

    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if !arguments.iter().any(|arg| arg == MINIMIZED_FLAG) {
        show_main_window(app.handle());
    }
    handle_arguments(app.handle(), &arguments);
    Ok(())
}

pub fn run() {
    let mut context = tauri::generate_context!();

    // The lock that keeps to one copy follows the identifier. Each test folder gets its
    // own, so a test runs beside the user's copy and still behaves as a single app.
    if let Some(folder) = std::env::var_os(DATA_DIR_VARIABLE) {
        let mut hasher = DefaultHasher::new();
        folder.hash(&mut hasher);
        context.config_mut().identifier = format!("app.tributary.test.{:x}", hasher.finish());
    }

    let result = tauri::Builder::default()
        // Registered first so a second launch never reaches the shared database.
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            let arguments = args.get(1..).unwrap_or_default();
            if arguments.is_empty() {
                show_main_window(app);
            }
            handle_arguments(app, arguments);
        }))
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
            commands::drag_files,
            commands::window_ready,
            video::inspect_video,
            video::add_video,
            site_grabber::list_projects,
            site_grabber::new_project,
            site_grabber::save_project,
            site_grabber::delete_project,
            site_grabber::start_exploring,
            site_grabber::stop_exploring,
            site_grabber::project_files,
            site_grabber::download_project_files,
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
            commands::login_and_resume,
            commands::save_site_login,
            commands::delete_site_login,
            commands::set_proxy_password,
            commands::play_sound,
            commands::export_data,
            commands::import_data,
            commands::discard_capture,
            commands::add_batch,
            commands::extension_folder,
            commands::add_dropped_link,
        ])
        .run(context);

    if let Err(error) = result {
        eprintln!("Tributary stopped: {error}");
    }
}
