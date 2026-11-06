//! Exporting and importing settings and the download list.
//!
//! Passwords are never included: they stay in Windows Credential Manager, and a site
//! login in the export is only its pattern and user name.

use std::fs;
use std::path::{Path, PathBuf};

use engine::{Engine, NewDownload, State};
use serde::{Deserialize, Serialize};

use crate::settings::Settings;

const FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportedDownload {
    url: String,
    dest_dir: PathBuf,
    file_name: Option<String>,
    description: Option<String>,
    category: Option<String>,
    finished: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Export {
    version: u32,
    settings: Settings,
    downloads: Vec<ExportedDownload>,
}

/// Writes settings and the download list to a file.
pub fn export_to(path: &Path, engine: &Engine, settings: &Settings) -> Result<usize, String> {
    let downloads: Vec<ExportedDownload> = engine
        .list()
        .map_err(|error| error.to_string())?
        .into_iter()
        // An unfinished stream can't come back from just its address: imported, that
        // would save the text of its playlist as if it were the video.
        .filter(|download| !download.is_stream || download.state == State::Completed)
        .map(|download| ExportedDownload {
            finished: download.state == State::Completed,
            url: download.url,
            dest_dir: download.dest_dir,
            file_name: download.file_name,
            description: download.description,
            category: download.category,
        })
        .collect();

    let count = downloads.len();
    let export = Export {
        version: FORMAT_VERSION,
        settings: settings.clone(),
        downloads,
    };
    let text = serde_json::to_string_pretty(&export).map_err(|error| error.to_string())?;
    fs::write(path, text).map_err(|error| error.to_string())?;
    Ok(count)
}

/// Reads an export. Returns its settings, and how many downloads were added. Unfinished
/// downloads come back stopped, and addresses already in the list are skipped.
pub fn import_from(path: &Path, engine: &Engine) -> Result<(Settings, usize), String> {
    let text = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let export: Export =
        serde_json::from_str(&text).map_err(|_| "this isn't a Tributary export file".to_owned())?;

    if export.version > FORMAT_VERSION {
        return Err("this file came from a newer version of Tributary".to_owned());
    }

    let existing: Vec<String> = engine
        .list()
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|download| download.url)
        .collect();

    let mut added = 0;
    for download in export.downloads {
        if download.finished || existing.contains(&download.url) {
            continue;
        }
        engine
            .add(NewDownload {
                url: download.url,
                dest_dir: download.dest_dir,
                file_name: download.file_name,
                description: download.description,
                category: download.category,
                paused: true,
                ..NewDownload::default()
            })
            .map_err(|error| error.to_string())?;
        added += 1;
    }
    Ok((export.settings, added))
}

#[cfg(test)]
mod tests {
    use engine::Config;

    use super::*;

    fn engine(folder: &Path, name: &str) -> Engine {
        Engine::open(&folder.join(name), Config::default()).unwrap()
    }

    fn paused(url: &str, folder: &Path) -> NewDownload {
        NewDownload {
            url: url.to_owned(),
            dest_dir: folder.to_owned(),
            description: Some("note".to_owned()),
            category: Some("Video".to_owned()),
            paused: true,
            ..NewDownload::default()
        }
    }

    #[tokio::test]
    async fn an_unfinished_stream_is_left_out_of_an_export() {
        let folder = tempfile::tempdir().unwrap();
        let file = folder.path().join("export.json");
        let source = engine(folder.path(), "a.db");

        source
            .add(paused("https://x.test/a.zip", folder.path()))
            .unwrap();
        source
            .add(NewDownload {
                file_name: Some("film.ts".to_owned()),
                parts: Some(vec![media::Part {
                    url: "https://x.test/1.ts".to_owned(),
                    range: None,
                }]),
                ..paused("https://x.test/film.m3u8", folder.path())
            })
            .unwrap();

        let count = export_to(&file, &source, &Settings::default()).unwrap();

        assert_eq!(count, 1);
        assert!(!fs::read_to_string(&file).unwrap().contains("film.m3u8"));
    }

    #[tokio::test]
    async fn the_list_and_settings_move_to_another_install() {
        let folder = tempfile::tempdir().unwrap();
        let file = folder.path().join("export.json");

        let source = engine(folder.path(), "a.db");
        source
            .add(paused("https://x.test/a.zip", folder.path()))
            .unwrap();
        let settings = Settings {
            connections: 12,
            ..Settings::default()
        };

        assert_eq!(export_to(&file, &source, &settings).unwrap(), 1);

        let target = engine(folder.path(), "b.db");
        let (imported, added) = import_from(&file, &target).unwrap();

        assert_eq!(added, 1);
        assert_eq!(imported.connections, 12);

        let list = target.list().unwrap();
        assert_eq!(list[0].url, "https://x.test/a.zip");
        assert_eq!(list[0].state, State::Paused);
        assert_eq!(list[0].description.as_deref(), Some("note"));
        assert_eq!(list[0].category.as_deref(), Some("Video"));
    }

    #[tokio::test]
    async fn importing_twice_does_not_duplicate_anything() {
        let folder = tempfile::tempdir().unwrap();
        let file = folder.path().join("export.json");
        let source = engine(folder.path(), "a.db");
        source
            .add(paused("https://x.test/a.zip", folder.path()))
            .unwrap();
        export_to(&file, &source, &Settings::default()).unwrap();

        let target = engine(folder.path(), "b.db");
        import_from(&file, &target).unwrap();
        let (_, added) = import_from(&file, &target).unwrap();

        assert_eq!(added, 0);
        assert_eq!(target.list().unwrap().len(), 1);
    }

    #[test]
    fn a_file_that_is_not_an_export_is_refused() {
        let folder = tempfile::tempdir().unwrap();
        let file = folder.path().join("other.json");
        fs::write(&file, "{ \"something\": 1 }").unwrap();

        let runtime = tokio::runtime::Runtime::new().unwrap();
        let _guard = runtime.enter();
        let target = engine(folder.path(), "b.db");

        assert!(import_from(&file, &target).unwrap_err().contains("export"));
    }

    #[test]
    fn a_newer_format_is_refused_rather_than_misread() {
        let folder = tempfile::tempdir().unwrap();
        let file = folder.path().join("newer.json");
        let export = serde_json::json!({
            "version": FORMAT_VERSION + 1,
            "settings": Settings::default(),
            "downloads": [],
        });
        fs::write(&file, export.to_string()).unwrap();

        let runtime = tokio::runtime::Runtime::new().unwrap();
        let _guard = runtime.enter();
        let target = engine(folder.path(), "b.db");

        assert!(import_from(&file, &target).unwrap_err().contains("newer"));
    }

    #[test]
    fn exports_contain_no_passwords() {
        // Settings hold only a site's pattern and user name, so a password has no field
        // to be in. This fails if one is ever added.
        let text = serde_json::to_string(&Settings::default()).unwrap();
        assert!(!text.to_lowercase().contains("password"));
    }
}
