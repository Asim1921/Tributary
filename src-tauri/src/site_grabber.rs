//! Running site grabber projects, and the commands behind their window.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use engine::{Engine, Login, QueueId};
use grabber::{Crawl, Ended, Fetched, Found, Project, ProjectFile, Source, Template, explore};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State as Managed};
use tokio_util::sync::CancellationToken;
use url::Url;

use crate::commands::{AddRequest, AppState, add_download_now};

/// How often repeating projects are checked.
const SCHEDULE_CHECK_INTERVAL: Duration = Duration::from_secs(30);

/// How many pages go by between saving progress.
const PAGES_PER_SAVE: usize = 20;

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs() as i64)
}

/// The saved projects, and which are exploring right now.
pub struct Grabber {
    path: PathBuf,
    file: Mutex<ProjectFile>,
    running: Mutex<HashMap<u64, CancellationToken>>,
}

impl Grabber {
    pub fn open(path: PathBuf) -> Self {
        Self {
            file: Mutex::new(ProjectFile::load(&path)),
            path,
            running: Mutex::default(),
        }
    }

    fn file(&self) -> MutexGuard<'_, ProjectFile> {
        self.file.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn running(&self) -> MutexGuard<'_, HashMap<u64, CancellationToken>> {
        self.running.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn project(&self, id: u64) -> Result<Project, String> {
        self.file()
            .find(id)
            .cloned()
            .ok_or_else(|| "that project no longer exists".to_owned())
    }

    fn save(&self, file: &ProjectFile) -> Result<(), String> {
        file.save(&self.path).map_err(|error| error.to_string())
    }

    /// Records how far a project has got. A project deleted meanwhile stays deleted.
    fn store_progress(&self, id: u64, crawl: &Crawl) {
        let mut file = self.file();
        if let Some(project) = file.find_mut(id) {
            project.crawl = Some(crawl.clone());
            let _ = self.save(&file);
        }
    }
}

/// Reads pages through the engine, so proxies and other settings apply.
struct EngineSource {
    engine: Engine,
    login: Option<Login>,
}

impl Source for EngineSource {
    async fn fetch(&self, url: &str) -> Result<Fetched, String> {
        let page = self.engine.fetch_page(url, &[], self.login.clone()).await?;
        Ok(Fetched {
            url: page.url,
            found: page.found,
            content_type: page.content_type,
            text: page.text,
        })
    }

    async fn size(&self, url: &str) -> Option<u64> {
        let info = self.engine.inspect(url, &[], self.login.clone()).await;
        info.ok()?.size
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    id: u64,
    pages_read: usize,
    pages_failed: usize,
    pages_refused: usize,
    pages_waiting: usize,
    files_found: usize,
    finished: bool,
}

impl Progress {
    fn of(id: u64, crawl: &Crawl) -> Self {
        Self {
            id,
            pages_read: crawl.pages_read,
            pages_failed: crawl.pages_failed,
            pages_refused: crawl.pages_refused,
            pages_waiting: crawl.pages_waiting(),
            files_found: crawl.found.len(),
            finished: crawl.finished,
        }
    }
}

/// Queues files that haven't been saved yet, and starts the queue.
fn download_files(state: &AppState, id: u64, wanted: &[Found]) -> Result<usize, String> {
    let project = state.grabber.project(id)?;
    let mut added = Vec::new();

    for file in wanted {
        if project.saved_files.contains(&file.url) {
            continue;
        }
        let Ok(url) = Url::parse(&file.url) else {
            continue;
        };

        let (folder, name) = project.destination(&url);
        let request = AddRequest::for_track(file.url.clone(), Some(folder), name, None).later();
        // Sites often only serve their files to visitors coming from their own pages.
        let headers = vec![("Referer".to_owned(), file.from.clone())];
        add_download_now(state, request, headers)?;
        added.push(file.url.clone());
    }

    if !added.is_empty() {
        let mut projects = state.grabber.file();
        if let Some(project) = projects.find_mut(id) {
            project.saved_files.extend(added.iter().cloned());
        }
        state.grabber.save(&projects)?;
    }

    // Even with nothing new, files from an earlier run may still be waiting in the queue.
    state
        .engine
        .start_queue(QueueId::MAIN)
        .map_err(|error| error.to_string())?;
    Ok(added.len())
}

/// Explores a project, carrying on from where it stopped. `save_everything` is for
/// scheduled runs, where nobody is there to choose.
async fn run_project(app: AppHandle, id: u64, save_everything: bool) {
    let state = app.state::<AppState>();
    let Ok(project) = state.grabber.project(id) else {
        return;
    };

    let stop = CancellationToken::new();
    {
        let mut running = state.grabber.running();
        if running.contains_key(&id) {
            return;
        }
        running.insert(id, stop.clone());
    }

    let mut crawl = project
        .crawl
        .clone()
        .filter(|crawl| !crawl.finished)
        .unwrap_or_else(|| Crawl::start(&project));

    let source = EngineSource {
        engine: state.engine.clone(),
        login: state.site_login_for(&project.start_url),
    };

    let reporter = app.clone();
    let mut pages_since_save = 0;
    let report = |progress: &Crawl| {
        let _ = reporter.emit("grabber-progress", Progress::of(id, progress));

        pages_since_save += 1;
        if pages_since_save >= PAGES_PER_SAVE {
            pages_since_save = 0;
            reporter
                .state::<AppState>()
                .grabber
                .store_progress(id, progress);
        }
    };

    let ended = explore(&project, &mut crawl, &source, &stop, report).await;

    state.grabber.store_progress(id, &crawl);
    state.grabber.running().remove(&id);

    let problem = if ended == Ended::Finished && (save_everything || project.offline) {
        download_files(&state, id, &crawl.found).err()
    } else {
        None
    };
    let _ = app.emit("grabber-finished", id);
    if let Some(problem) = problem {
        let _ = app.emit("video-problem", problem);
    }
}

/// Runs repeating projects when they come due.
pub fn watch_schedule(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            let state = app.state::<AppState>();
            let due: Vec<u64> = {
                let mut file = state.grabber.file();
                let time = now();
                let due: Vec<u64> = file
                    .projects
                    .iter_mut()
                    .filter(|project| project.is_due(time))
                    .map(|project| {
                        // Each run looks at the site afresh.
                        project.mark_ran(time);
                        project.crawl = None;
                        project.id
                    })
                    .collect();
                if !due.is_empty() {
                    let _ = state.grabber.save(&file);
                }
                due
            };

            for id in due {
                tauri::async_runtime::spawn(run_project(app.clone(), id, true));
            }
            tokio::time::sleep(SCHEDULE_CHECK_INTERVAL).await;
        }
    });
}

/// A project as the window sees it, without the bulk of its exploring.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectView {
    #[serde(flatten)]
    project: Project,
    pages_read: usize,
    files_found: usize,
    explored: bool,
    running: bool,
}

#[tauri::command]
pub async fn list_projects(state: Managed<'_, AppState>) -> Result<Vec<ProjectView>, String> {
    let running = state.grabber.running().keys().copied().collect::<Vec<_>>();
    let file = state.grabber.file();

    Ok(file
        .projects
        .iter()
        .map(|project| {
            let crawl = project.crawl.as_ref();
            ProjectView {
                project: Project {
                    crawl: None,
                    saved_files: Vec::new(),
                    ..project.clone()
                },
                pages_read: crawl.map_or(0, |crawl| crawl.pages_read),
                files_found: crawl.map_or(0, |crawl| crawl.found.len()),
                explored: crawl.is_some_and(|crawl| crawl.finished),
                running: running.contains(&project.id),
            }
        })
        .collect())
}

/// A new project set up from a template, not saved yet.
#[tauri::command]
pub async fn new_project(
    state: Managed<'_, AppState>,
    template: Template,
    start_url: String,
) -> Result<Project, String> {
    Project::check_address(&start_url)?;
    let folder = state.settings_snapshot().download_dir;
    Ok(Project::from_template(template, &start_url, &folder))
}

#[tauri::command]
pub async fn save_project(
    state: Managed<'_, AppState>,
    mut project: Project,
) -> Result<u64, String> {
    project.check()?;
    project.start_url = project.start_url.trim().to_owned();
    project.crawl = None;

    project.next_run = project.next_run_after(now());
    let mut file = state.grabber.file();
    let id = file.put(project);
    state.grabber.save(&file)?;
    Ok(id)
}

#[tauri::command]
pub async fn delete_project(state: Managed<'_, AppState>, id: u64) -> Result<(), String> {
    if let Some(stop) = state.grabber.running().get(&id) {
        stop.cancel();
    }
    let mut file = state.grabber.file();
    file.remove(id);
    state.grabber.save(&file)
}

#[tauri::command]
pub async fn start_exploring(app: AppHandle, id: u64) -> Result<(), String> {
    tauri::async_runtime::spawn(run_project(app, id, false));
    Ok(())
}

#[tauri::command]
pub async fn stop_exploring(state: Managed<'_, AppState>, id: u64) -> Result<(), String> {
    if let Some(stop) = state.grabber.running().get(&id) {
        stop.cancel();
    }
    Ok(())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileView {
    url: String,
    from: String,
    size: Option<u64>,
    saved: bool,
}

/// What exploring has found so far.
#[tauri::command]
pub async fn project_files(state: Managed<'_, AppState>, id: u64) -> Result<Vec<FileView>, String> {
    let project = state.grabber.project(id)?;
    let found = project.crawl.map_or_else(Vec::new, |crawl| crawl.found);

    Ok(found
        .into_iter()
        .map(|file| FileView {
            saved: project.saved_files.contains(&file.url),
            url: file.url,
            from: file.from,
            size: file.size,
        })
        .collect())
}

/// Saves the chosen files. Returns how many were added.
#[tauri::command]
pub async fn download_project_files(
    state: Managed<'_, AppState>,
    id: u64,
    urls: Vec<String>,
) -> Result<usize, String> {
    let project = state.grabber.project(id)?;
    let found = project.crawl.map_or_else(Vec::new, |crawl| crawl.found);
    let wanted: Vec<Found> = found
        .into_iter()
        .filter(|file| urls.contains(&file.url))
        .collect();

    download_files(&state, id, &wanted)
}
