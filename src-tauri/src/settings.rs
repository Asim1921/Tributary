//! User settings, kept as a small JSON file.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::rules::{Category, default_categories};

pub const MIN_CONNECTIONS: usize = 1;
pub const MAX_CONNECTIONS: usize = 32;
const DEFAULT_CONNECTIONS: usize = 8;
const URL_HISTORY_LIMIT: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolbarSize {
    #[default]
    Large,
    Small,
}

/// How one column of the download list is shown. The list of columns itself
/// belongs to the UI, so an empty `columns` setting means "use the defaults".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Column {
    pub id: String,
    pub visible: bool,
    pub width: u32,
}

/// Every field has a default, so settings saved by an older version still load.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub download_dir: PathBuf,
    pub temp_dir: Option<PathBuf>,
    pub connections: usize,
    pub theme: Theme,
    pub show_start_dialog: bool,
    pub show_complete_dialog: bool,
    pub close_to_tray: bool,
    pub start_with_windows: bool,
    pub columns: Vec<Column>,
    /// Toolbar button ids in display order. Empty means the default set.
    pub toolbar: Vec<String>,
    pub toolbar_size: ToolbarSize,
    pub url_history: Vec<String>,
    pub categories: Vec<Category>,
    /// Offers the last folder used instead of the default one.
    pub remember_last_folder: bool,
    pub last_folder: Option<PathBuf>,
    pub speed_limit_enabled: bool,
    pub speed_limit_kb: u64,
    pub quota_enabled: bool,
    pub quota_mb: u64,
    pub quota_hours: u64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            download_dir: PathBuf::new(),
            temp_dir: None,
            connections: DEFAULT_CONNECTIONS,
            theme: Theme::default(),
            show_start_dialog: true,
            show_complete_dialog: true,
            close_to_tray: true,
            start_with_windows: false,
            columns: Vec::new(),
            toolbar: Vec::new(),
            toolbar_size: ToolbarSize::default(),
            url_history: Vec::new(),
            categories: default_categories(),
            remember_last_folder: false,
            last_folder: None,
            speed_limit_enabled: false,
            speed_limit_kb: 1024,
            quota_enabled: false,
            quota_mb: 1024,
            quota_hours: 1,
        }
    }
}

impl Settings {
    /// Loads settings, filling gaps with defaults. A corrupt file is set aside rather
    /// than deleted, so nothing is lost and the app still starts.
    pub fn load(path: &Path, default_download_dir: &Path) -> Self {
        let mut settings = match fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|_| {
                let _ = fs::rename(path, path.with_extension("json.corrupt"));
                Self::default()
            }),
            Err(_) => Self::default(),
        };

        if settings.download_dir.as_os_str().is_empty() {
            settings.download_dir = default_download_dir.to_owned();
        }
        settings.connections = settings.connections.clamp(MIN_CONNECTIONS, MAX_CONNECTIONS);
        settings
    }

    /// Writes to a temporary file first, so a crash can't leave a half-written file.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(folder) = path.parent() {
            fs::create_dir_all(folder)?;
        }
        let text = serde_json::to_string_pretty(self).map_err(io::Error::other)?;
        let temporary = path.with_extension("json.tmp");
        fs::write(&temporary, text)?;
        fs::rename(&temporary, path)
    }

    /// The folder a new download should use when nothing more specific applies.
    pub fn default_folder(&self) -> &Path {
        self.last_folder
            .as_deref()
            .filter(|_| self.remember_last_folder)
            .unwrap_or(&self.download_dir)
    }

    /// Puts a URL at the front of the history, without repeats.
    pub fn remember_url(&mut self, url: &str) {
        self.url_history.retain(|existing| existing != url);
        self.url_history.insert(0, url.to_owned());
        self.url_history.truncate(URL_HISTORY_LIMIT);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOWNLOADS: &str = r"C:\Users\me\Downloads";

    fn load(path: &Path) -> Settings {
        Settings::load(path, Path::new(DOWNLOADS))
    }

    #[test]
    fn the_default_folder_follows_the_setting() {
        let mut settings = Settings {
            download_dir: PathBuf::from(r"C:\Downloads"),
            last_folder: Some(PathBuf::from(r"D:\Last")),
            ..Settings::default()
        };
        assert_eq!(settings.default_folder(), Path::new(r"C:\Downloads"));

        settings.remember_last_folder = true;
        assert_eq!(settings.default_folder(), Path::new(r"D:\Last"));

        settings.last_folder = None;
        assert_eq!(settings.default_folder(), Path::new(r"C:\Downloads"));
    }

    #[test]
    fn new_settings_come_with_the_standard_categories() {
        assert_eq!(Settings::default().categories.len(), 5);
    }

    #[test]
    fn a_user_can_remove_every_category() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("settings.json");
        let mut settings = load(&path);
        settings.categories.clear();
        settings.save(&path).unwrap();

        assert!(load(&path).categories.is_empty());
    }

    #[test]
    fn a_missing_file_gives_defaults() {
        let folder = tempfile::tempdir().unwrap();
        let settings = load(&folder.path().join("settings.json"));

        assert_eq!(settings.download_dir, PathBuf::from(DOWNLOADS));
        assert_eq!(settings.connections, DEFAULT_CONNECTIONS);
        assert!(settings.show_start_dialog);
    }

    #[test]
    fn settings_survive_saving_and_loading() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("nested").join("settings.json");

        let mut settings = load(&path);
        settings.connections = 16;
        settings.theme = Theme::Dark;
        settings.temp_dir = Some(PathBuf::from(r"D:\temp"));
        settings.columns = vec![Column {
            id: "size".to_owned(),
            visible: false,
            width: 90,
        }];
        settings.save(&path).unwrap();

        assert_eq!(load(&path), settings);
    }

    #[test]
    fn saving_leaves_no_temporary_file() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("settings.json");

        Settings::default().save(&path).unwrap();

        assert_eq!(fs::read_dir(folder.path()).unwrap().count(), 1);
    }

    #[test]
    fn fields_missing_from_an_older_file_get_defaults() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("settings.json");
        fs::write(&path, r#"{ "connections": 4 }"#).unwrap();

        let settings = load(&path);

        assert_eq!(settings.connections, 4);
        assert!(settings.show_complete_dialog);
    }

    #[test]
    fn a_corrupt_file_is_kept_and_the_app_still_starts() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("settings.json");
        fs::write(&path, "{ not json").unwrap();

        let settings = load(&path);

        assert_eq!(settings.connections, DEFAULT_CONNECTIONS);
        let kept = fs::read_to_string(path.with_extension("json.corrupt")).unwrap();
        assert_eq!(kept, "{ not json");
    }

    #[test]
    fn out_of_range_connections_are_corrected() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("settings.json");

        fs::write(&path, r#"{ "connections": 0 }"#).unwrap();
        assert_eq!(load(&path).connections, MIN_CONNECTIONS);

        fs::write(&path, r#"{ "connections": 9999 }"#).unwrap();
        assert_eq!(load(&path).connections, MAX_CONNECTIONS);
    }

    #[test]
    fn url_history_moves_repeats_to_the_front_and_stays_bounded() {
        let mut settings = Settings::default();
        settings.remember_url("a");
        settings.remember_url("b");
        settings.remember_url("a");
        assert_eq!(settings.url_history, ["a", "b"]);

        for number in 0..100 {
            settings.remember_url(&number.to_string());
        }
        assert_eq!(settings.url_history.len(), URL_HISTORY_LIMIT);
        assert_eq!(settings.url_history[0], "99");
    }
}
