//! Matching downloads to categories, and wildcard patterns.

use std::path::PathBuf;

use engine::wildcard_match;
use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Category {
    pub name: String,
    /// Where its files go. `None` uses the default folder.
    pub folder: Option<PathBuf>,
    /// File extensions, without the dot.
    pub extensions: Vec<String>,
    /// Site patterns such as `*.example.com`. These take priority over extensions.
    pub sites: Vec<String>,
}

fn category(name: &str, extensions: &[&str]) -> Category {
    Category {
        name: name.to_owned(),
        folder: None,
        extensions: extensions.iter().map(|&text| text.to_owned()).collect(),
        sites: Vec::new(),
    }
}

pub fn default_categories() -> Vec<Category> {
    vec![
        category(
            "Compressed",
            &["zip", "rar", "7z", "gz", "tar", "bz2", "xz", "iso"],
        ),
        category(
            "Documents",
            &[
                "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "txt", "epub",
            ],
        ),
        category("Music", &["mp3", "wav", "flac", "aac", "ogg", "m4a", "wma"]),
        category(
            "Programs",
            &["exe", "msi", "msix", "apk", "dmg", "deb", "rpm"],
        ),
        category(
            "Video",
            &["mp4", "mkv", "avi", "mov", "webm", "wmv", "flv", "m4v"],
        ),
    ]
}

/// Whether a site pattern covers a URL. A pattern can name just the host, or include
/// a path.
pub fn site_matches(pattern: &str, url: &str) -> bool {
    let pattern = pattern.trim();
    if pattern.is_empty() {
        return false;
    }

    let Ok(parsed) = Url::parse(url) else {
        return wildcard_match(pattern, url);
    };
    let host = parsed.host_str().unwrap_or_default();
    wildcard_match(pattern, host)
        || wildcard_match(pattern, &format!("{host}{}", parsed.path()))
        || wildcard_match(pattern, url)
}

fn extension(file_name: &str) -> Option<&str> {
    file_name.rsplit_once('.').map(|(_, extension)| extension)
}

/// Finds the category for a file: a matching site first, then a matching extension.
pub fn match_category<'a>(
    categories: &'a [Category],
    file_name: &str,
    url: &str,
) -> Option<&'a Category> {
    categories
        .iter()
        .find(|category| category.sites.iter().any(|site| site_matches(site, url)))
        .or_else(|| {
            let extension = extension(file_name)?;
            categories.iter().find(|category| {
                category.extensions.iter().any(|known| {
                    known
                        .trim_start_matches('.')
                        .eq_ignore_ascii_case(extension)
                })
            })
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn site_patterns_match_hosts_and_paths() {
        let url = "https://files.example.com/music/song.mp3?token=1";

        assert!(site_matches("*.example.com", url));
        assert!(site_matches("files.example.com", url));
        assert!(site_matches("*example.com/music/*", url));
        assert!(!site_matches("*.other.com", url));
        assert!(!site_matches("", url));
        assert!(!site_matches("   ", url));
    }

    #[test]
    fn a_host_pattern_cannot_be_fooled_by_the_path() {
        let url = "https://evil.test/files.example.com";
        assert!(!site_matches("files.example.com", url));
    }

    #[test]
    fn files_are_matched_by_extension() {
        let categories = default_categories();
        let name = |file: &str| {
            match_category(&categories, file, "https://x.test/").map(|c| c.name.as_str())
        };

        assert_eq!(name("song.MP3"), Some("Music"));
        assert_eq!(name("film.mkv"), Some("Video"));
        assert_eq!(name("setup.exe"), Some("Programs"));
        assert_eq!(name("archive.tar.gz"), Some("Compressed"));
        assert_eq!(name("notes.xyz"), None);
        assert_eq!(name("no-extension"), None);
    }

    #[test]
    fn site_rules_win_over_extensions() {
        let mut categories = default_categories();
        categories.push(Category {
            sites: vec!["*.work.test".to_owned()],
            ..category("Work", &[])
        });

        let found = match_category(&categories, "song.mp3", "https://files.work.test/song.mp3");

        assert_eq!(found.map(|c| c.name.as_str()), Some("Work"));
    }

    #[test]
    fn an_extension_written_with_a_dot_still_matches() {
        let categories = vec![category("Custom", &[".abc"])];

        assert!(match_category(&categories, "file.abc", "https://x.test/").is_some());
    }
}
