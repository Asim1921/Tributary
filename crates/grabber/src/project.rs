//! A saved grabber project: what to explore, what to keep, and where.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use engine::wildcard_match;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::crawl::Crawl;
use crate::offline;

const SECONDS_PER_HOUR: i64 = 3600;

/// The most pages a project can read, however high it's set.
pub const MAX_PAGES_LIMIT: usize = 20_000;

const PICTURES: [&str; 8] = ["jpg", "jpeg", "png", "gif", "webp", "svg", "bmp", "avif"];
const DOCUMENTS: [&str; 10] = [
    "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "txt", "csv", "odt",
];
const AUDIO_AND_VIDEO: [&str; 10] = [
    "mp3", "flac", "wav", "ogg", "m4a", "mp4", "mkv", "webm", "avi", "mov",
];

/// A starting point for a new project.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Template {
    /// Every file the site links to.
    #[default]
    AllFiles,
    Pictures,
    Documents,
    AudioAndVideo,
    /// The pages themselves, with everything needed to read them with no connection.
    OfflineSite,
}

/// How far from the starting page exploring may go.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Scope {
    /// Only below the starting page's folder.
    Folder,
    /// Anywhere on the same site.
    #[default]
    Site,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Project {
    pub id: u64,
    pub name: String,
    pub start_url: String,
    pub template: Template,
    /// How many links away from the starting page to go.
    pub max_depth: u32,
    pub max_pages: usize,
    pub scope: Scope,
    /// Also take files kept on other sites, as pictures often are.
    pub files_anywhere: bool,
    /// How long to wait between requests.
    pub delay_ms: u64,
    /// Addresses to read. Empty means all of them.
    pub include_pages: Vec<String>,
    pub exclude_pages: Vec<String>,
    /// File extensions to keep. Empty means all of them.
    pub file_types: Vec<String>,
    pub exclude_files: Vec<String>,
    pub min_size: Option<u64>,
    pub max_size: Option<u64>,
    pub folder: PathBuf,
    /// Keep the site's folders, rather than putting every file in one.
    pub keep_folders: bool,
    pub offline: bool,
    /// Runs again this many hours after the last run.
    pub repeat_hours: Option<u32>,
    pub next_run: Option<i64>,
    pub last_run: Option<i64>,
    /// Addresses already saved, so a repeat only fetches what's new.
    pub saved_files: Vec<String>,
    /// How far exploring has got.
    pub crawl: Option<Crawl>,
}

impl Default for Project {
    fn default() -> Self {
        Self {
            id: 0,
            name: String::new(),
            start_url: String::new(),
            template: Template::default(),
            max_depth: 3,
            max_pages: 500,
            scope: Scope::default(),
            files_anywhere: false,
            delay_ms: 1000,
            include_pages: Vec::new(),
            exclude_pages: Vec::new(),
            file_types: Vec::new(),
            exclude_files: Vec::new(),
            min_size: None,
            max_size: None,
            folder: PathBuf::new(),
            keep_folders: false,
            offline: false,
            repeat_hours: None,
            next_run: None,
            last_run: None,
            saved_files: Vec::new(),
            crawl: None,
        }
    }
}

fn list(items: &[&str]) -> Vec<String> {
    items.iter().map(|&item| item.to_owned()).collect()
}

/// The last part of an address's path, if it has an extension.
pub(crate) fn extension_of(url: &Url) -> Option<String> {
    let name = url.path_segments()?.next_back()?;
    let (stem, extension) = name.rsplit_once('.')?;
    (!stem.is_empty() && !extension.is_empty()).then(|| extension.to_ascii_lowercase())
}

/// `www.x.test` and `x.test` are the same site.
fn site_of(url: &Url) -> Option<&str> {
    let host = url.host_str()?;
    Some(host.strip_prefix("www.").unwrap_or(host))
}

fn matches_any(patterns: &[String], url: &str) -> bool {
    patterns
        .iter()
        .any(|pattern| !pattern.trim().is_empty() && wildcard_match(pattern.trim(), url))
}

impl Project {
    /// A new project set up from a template.
    pub fn from_template(template: Template, start_url: &str, folder: &Path) -> Self {
        let name = Url::parse(start_url)
            .ok()
            .and_then(|url| url.host_str().map(str::to_owned))
            .unwrap_or_default();

        let mut project = Self {
            template,
            name,
            start_url: start_url.trim().to_owned(),
            folder: folder.to_owned(),
            ..Self::default()
        };

        match template {
            Template::AllFiles => {}
            Template::Pictures => project.file_types = list(&PICTURES),
            Template::Documents => project.file_types = list(&DOCUMENTS),
            Template::AudioAndVideo => project.file_types = list(&AUDIO_AND_VIDEO),
            Template::OfflineSite => {
                project.offline = true;
                project.keep_folders = true;
            }
        }
        project
    }

    /// Explains what's wrong with a starting address, if anything.
    pub fn check_address(address: &str) -> Result<(), String> {
        let url = Url::parse(address.trim())
            .map_err(|_| "the starting address isn't valid".to_owned())?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err("the starting address has to be a web address".to_owned());
        }

        // Projects are saved as plain text, and a repeating one needs its login again
        // later, so it can't simply be held in memory as a download's is.
        if !url.username().is_empty() || url.password().is_some() {
            return Err(
                "logins can't go in the address. Add the site in Options, under Logins".to_owned(),
            );
        }
        Ok(())
    }

    /// Explains what's wrong with a project, if anything.
    pub fn check(&self) -> Result<(), String> {
        Self::check_address(&self.start_url)?;
        if self.name.trim().is_empty() {
            return Err("the project needs a name".to_owned());
        }
        if self.folder.as_os_str().is_empty() {
            return Err("the project needs a folder to save into".to_owned());
        }
        if self.max_pages == 0 {
            return Err("the project has to be allowed at least one page".to_owned());
        }
        if self
            .min_size
            .zip(self.max_size)
            .is_some_and(|(min, max)| min > max)
        {
            return Err("the smallest size is larger than the largest".to_owned());
        }
        Ok(())
    }

    /// The page limit, which can't be set past what's sensible.
    pub fn page_limit(&self) -> usize {
        self.max_pages.min(MAX_PAGES_LIMIT)
    }

    fn start(&self) -> Option<Url> {
        Url::parse(self.start_url.trim()).ok()
    }

    /// Whether an address is within reach of the starting page.
    pub fn in_scope(&self, url: &Url) -> bool {
        let Some(start) = self.start() else {
            return false;
        };
        if site_of(url) != site_of(&start) {
            return false;
        }

        match self.scope {
            Scope::Site => true,
            Scope::Folder => {
                let path = start.path();
                let folder = &path[..path.rfind('/').map_or(0, |at| at + 1)];
                url.path().starts_with(folder)
            }
        }
    }

    /// Whether a page's address passes the project's filters.
    pub fn wants_page(&self, url: &Url) -> bool {
        let address = url.as_str();
        if matches_any(&self.exclude_pages, address) {
            return false;
        }

        // The starting page always counts, or nothing could ever be explored.
        self.include_pages.iter().all(|item| item.trim().is_empty())
            || matches_any(&self.include_pages, address)
            || self.start().is_some_and(|start| &start == url)
    }

    /// Whether a file passes the project's filters. A site saved for offline reading
    /// needs everything its pages use, so only exclusions apply.
    pub fn wants_file(&self, url: &Url) -> bool {
        if matches_any(&self.exclude_files, url.as_str()) {
            return false;
        }
        if self.offline || self.file_types.is_empty() {
            return true;
        }

        extension_of(url).is_some_and(|extension| {
            self.file_types.iter().any(|kind| {
                kind.trim()
                    .trim_start_matches('.')
                    .eq_ignore_ascii_case(&extension)
            })
        })
    }

    pub fn wants_size(&self, size: u64) -> bool {
        self.min_size.is_none_or(|min| size >= min) && self.max_size.is_none_or(|max| size <= max)
    }

    /// Where a file goes, as its folder and name.
    pub fn destination(&self, url: &Url) -> (PathBuf, String) {
        let local = offline::local_path(url, false);
        let name = local
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned());

        let folder = if self.keep_folders {
            self.folder.join(local.parent().unwrap_or(Path::new("")))
        } else {
            self.folder.clone()
        };
        (folder, name)
    }

    /// Whether a scheduled run is due.
    pub fn is_due(&self, now: i64) -> bool {
        self.repeat_hours.is_some() && self.next_run.is_some_and(|due| due <= now)
    }

    /// Notes that a run happened now, and when the next is.
    pub fn mark_ran(&mut self, now: i64) {
        self.last_run = Some(now);
        self.next_run = self.next_run_after(now);
    }

    /// When the next scheduled run falls, counting from `now`.
    pub fn next_run_after(&self, now: i64) -> Option<i64> {
        self.repeat_hours
            .filter(|hours| *hours > 0)
            .map(|hours| now + i64::from(hours) * SECONDS_PER_HOUR)
    }

    /// Keeps what this project has learned when its settings are saved again. Exploring
    /// starts over if what it would find has changed.
    pub fn keep_progress_from(&mut self, previous: &Self) {
        self.saved_files.clone_from(&previous.saved_files);
        self.last_run = previous.last_run;

        let same_search = self.start_url == previous.start_url
            && self.scope == previous.scope
            && self.max_depth == previous.max_depth
            && self.file_types == previous.file_types
            && self.include_pages == previous.include_pages
            && self.exclude_pages == previous.exclude_pages
            && self.exclude_files == previous.exclude_files
            && self.files_anywhere == previous.files_anywhere
            && self.offline == previous.offline;
        if same_search {
            self.crawl.clone_from(&previous.crawl);
        }
    }
}

/// The list of projects, saved as one file.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProjectFile {
    pub projects: Vec<Project>,
}

impl ProjectFile {
    /// Reads the list. A missing or damaged file gives an empty one.
    pub fn load(path: &Path) -> Self {
        fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// Writes the list. It goes to a new file first, so a crash can't leave half of it.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        let text = serde_json::to_string(self).map_err(io::Error::other)?;
        let temporary = path.with_extension("json.new");
        fs::write(&temporary, text)?;
        fs::rename(&temporary, path)
    }

    pub fn find(&self, id: u64) -> Option<&Project> {
        self.projects.iter().find(|project| project.id == id)
    }

    pub fn find_mut(&mut self, id: u64) -> Option<&mut Project> {
        self.projects.iter_mut().find(|project| project.id == id)
    }

    /// Adds a project, or replaces the one with its id. Returns its id.
    pub fn put(&mut self, mut project: Project) -> u64 {
        if let Some(existing) = self.find_mut(project.id) {
            project.keep_progress_from(existing);
            let id = project.id;
            *existing = project;
            return id;
        }

        let id = self.projects.iter().map(|item| item.id).max().unwrap_or(0) + 1;
        project.id = id;
        self.projects.push(project);
        id
    }

    pub fn remove(&mut self, id: u64) {
        self.projects.retain(|project| project.id != id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> Project {
        Project::from_template(
            Template::AllFiles,
            "https://www.x.test/docs/index.html",
            Path::new(r"D:\Saved"),
        )
    }

    fn url(text: &str) -> Url {
        Url::parse(text).unwrap()
    }

    #[test]
    fn a_template_sets_what_it_is_for() {
        let folder = Path::new(r"D:\Saved");
        let start = "https://x.test/";

        let pictures = Project::from_template(Template::Pictures, start, folder);
        assert!(pictures.file_types.contains(&"png".to_owned()));
        assert!(!pictures.offline);

        let offline = Project::from_template(Template::OfflineSite, start, folder);
        assert!(offline.offline);
        assert!(offline.keep_folders);

        assert!(project().file_types.is_empty());
        assert_eq!(project().name, "www.x.test");
    }

    #[test]
    fn a_project_starts_out_polite() {
        let project = Project::default();

        assert!(project.delay_ms >= 1000);
        assert_eq!(project.scope, Scope::Site);
        assert!(!project.files_anywhere);
    }

    #[test]
    fn mistakes_in_a_project_are_explained() {
        assert!(project().check().is_ok());

        let wrong = |change: fn(&mut Project)| {
            let mut project = project();
            change(&mut project);
            project.check().unwrap_err()
        };

        assert!(wrong(|p| p.name.clear()).contains("name"));
        assert!(wrong(|p| p.start_url = "nonsense".to_owned()).contains("address"));
        assert!(wrong(|p| p.start_url = "ftp://x.test/".to_owned()).contains("web address"));
        assert!(wrong(|p| p.folder = PathBuf::new()).contains("folder"));
        assert!(wrong(|p| p.max_pages = 0).contains("page"));
        assert!(
            wrong(|p| {
                p.min_size = Some(10);
                p.max_size = Some(5);
            })
            .contains("size")
        );
    }

    #[test]
    fn a_login_in_the_starting_address_is_refused_because_projects_are_saved_as_text() {
        let problem = Project::check_address("https://alice:hunter2@x.test/").unwrap_err();
        assert!(problem.contains("Options"), "{problem}");

        assert!(Project::check_address("https://alice@x.test/").is_err());
        assert!(Project::check_address("https://x.test/").is_ok());
    }

    #[test]
    fn the_page_limit_cannot_be_set_past_the_maximum() {
        let project = Project {
            max_pages: usize::MAX,
            ..project()
        };
        assert_eq!(project.page_limit(), MAX_PAGES_LIMIT);
    }

    #[test]
    fn the_same_site_includes_its_www_name() {
        let project = project();

        assert!(project.in_scope(&url("https://www.x.test/other.html")));
        assert!(project.in_scope(&url("https://x.test/other.html")));
        assert!(!project.in_scope(&url("https://y.test/other.html")));
        assert!(!project.in_scope(&url("https://sub.x.test/other.html")));
    }

    #[test]
    fn a_folder_scope_stays_below_the_start() {
        let project = Project {
            scope: Scope::Folder,
            ..project()
        };

        assert!(project.in_scope(&url("https://www.x.test/docs/a/b.html")));
        assert!(!project.in_scope(&url("https://www.x.test/other/b.html")));
        assert!(!project.in_scope(&url("https://www.x.test/docsy/b.html")));
    }

    #[test]
    fn page_filters_include_and_exclude() {
        let project = Project {
            include_pages: vec!["*/docs/*".to_owned()],
            exclude_pages: vec!["*/docs/old/*".to_owned()],
            ..project()
        };

        assert!(project.wants_page(&url("https://www.x.test/docs/a.html")));
        assert!(!project.wants_page(&url("https://www.x.test/blog/a.html")));
        assert!(!project.wants_page(&url("https://www.x.test/docs/old/a.html")));
    }

    #[test]
    fn the_starting_page_always_counts() {
        let project = Project {
            include_pages: vec!["*/blog/*".to_owned()],
            ..project()
        };
        assert!(project.wants_page(&url("https://www.x.test/docs/index.html")));
    }

    #[test]
    fn file_filters_go_by_type_and_exclusions() {
        let project = Project {
            file_types: vec!["PDF".to_owned(), ".zip".to_owned(), " png ".to_owned()],
            exclude_files: vec!["*draft*".to_owned()],
            ..project()
        };

        assert!(project.wants_file(&url("https://x.test/a.pdf")));
        assert!(project.wants_file(&url("https://x.test/a.ZIP?v=1")));
        assert!(project.wants_file(&url("https://x.test/a.png")));
        assert!(!project.wants_file(&url("https://x.test/a.exe")));
        assert!(!project.wants_file(&url("https://x.test/noextension")));
        assert!(!project.wants_file(&url("https://x.test/draft.pdf")));
    }

    #[test]
    fn with_no_types_chosen_every_file_counts() {
        assert!(project().wants_file(&url("https://x.test/noextension")));
    }

    #[test]
    fn an_offline_site_keeps_what_its_pages_use() {
        let project = Project {
            offline: true,
            file_types: vec!["pdf".to_owned()],
            exclude_files: vec!["*.exe".to_owned()],
            ..project()
        };

        assert!(project.wants_file(&url("https://x.test/a.png")));
        assert!(!project.wants_file(&url("https://x.test/a.exe")));
    }

    #[test]
    fn size_limits_include_their_ends() {
        let project = Project {
            min_size: Some(10),
            max_size: Some(20),
            ..project()
        };

        assert!(!project.wants_size(9));
        assert!(project.wants_size(10));
        assert!(project.wants_size(20));
        assert!(!project.wants_size(21));
    }

    #[test]
    fn a_file_keeps_its_folders_only_if_asked() {
        let address = url("https://x.test/a/b/report.pdf");

        let (folder, name) = project().destination(&address);
        assert_eq!(folder, Path::new(r"D:\Saved"));
        assert_eq!(name, "report.pdf");

        let kept = Project {
            keep_folders: true,
            ..project()
        };
        assert_eq!(
            kept.destination(&address).0,
            Path::new(r"D:\Saved\x.test\a\b")
        );
    }

    #[test]
    fn a_repeating_project_comes_due_and_moves_on() {
        let mut project = Project {
            repeat_hours: Some(2),
            next_run: Some(1000),
            ..project()
        };

        assert!(!project.is_due(999));
        assert!(project.is_due(1000));

        project.mark_ran(1000);
        assert_eq!(project.last_run, Some(1000));
        assert_eq!(project.next_run, Some(1000 + 2 * 3600));
        assert!(!project.is_due(1000));
    }

    #[test]
    fn a_project_with_no_schedule_is_never_due() {
        let mut project = Project {
            next_run: Some(0),
            ..project()
        };
        assert!(!project.is_due(i64::MAX));

        project.mark_ran(5);
        assert_eq!(project.next_run, None);
    }

    #[test]
    fn projects_survive_being_saved_and_reopened() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("projects.json");

        let mut file = ProjectFile::default();
        let id = file.put(project());
        file.save(&path).unwrap();

        let loaded = ProjectFile::load(&path);
        assert_eq!(loaded.find(id).unwrap().start_url, project().start_url);
    }

    #[test]
    fn a_damaged_file_gives_an_empty_list() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("projects.json");
        fs::write(&path, "not json").unwrap();

        assert!(ProjectFile::load(&path).projects.is_empty());
        assert!(
            ProjectFile::load(&folder.path().join("none.json"))
                .projects
                .is_empty()
        );
    }

    #[test]
    fn new_projects_get_their_own_ids() {
        let mut file = ProjectFile::default();
        let first = file.put(project());
        let second = file.put(project());
        assert_ne!(first, second);

        file.remove(first);
        assert!(file.find(first).is_none());
        assert!(file.find(second).is_some());
    }

    #[test]
    fn changing_a_project_keeps_what_it_learned() {
        let mut file = ProjectFile::default();
        let id = file.put(project());
        let saved = file.find_mut(id).unwrap();
        saved.saved_files.push("https://x.test/a.zip".to_owned());
        saved.crawl = Some(Crawl::default());

        let renamed = Project {
            id,
            name: "Renamed".to_owned(),
            ..project()
        };
        file.put(renamed);

        let kept = file.find(id).unwrap();
        assert_eq!(kept.name, "Renamed");
        assert_eq!(kept.saved_files.len(), 1);
        assert!(kept.crawl.is_some());
    }

    #[test]
    fn changing_what_to_look_for_starts_exploring_over() {
        let mut file = ProjectFile::default();
        let id = file.put(project());
        let saved = file.find_mut(id).unwrap();
        saved.saved_files.push("https://x.test/a.zip".to_owned());
        saved.crawl = Some(Crawl::default());

        let changed = Project {
            id,
            file_types: vec!["pdf".to_owned()],
            ..project()
        };
        file.put(changed);

        let kept = file.find(id).unwrap();
        assert!(kept.crawl.is_none());
        assert_eq!(kept.saved_files.len(), 1, "saved files are still saved");
    }
}
