//! Running yt-dlp for sites that feed their video player by script, and installing it.
//!
//! yt-dlp is only ever started with a fixed list of arguments and no shell, with a time
//! limit and a cap on how much it may print. It is never given cookies, and it only reads
//! about a page. The files themselves are downloaded by Tributary's own engine.

use std::env;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use engine::{Engine, NewDownload, State};
use media::Error;
use media::ytdlp::{self, Page};
use sha2::{Digest, Sha256};
use tauri::State as Managed;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;
use url::Url;

use crate::commands::AppState;

/// What the window shows when yt-dlp isn't there, and recognises to offer installing it.
pub const MISSING: &str = "yt-dlp is needed for this site";

const PROGRAM: &str = "yt-dlp.exe";
const TOOLS_FOLDER: &str = "tools";
const RELEASE: &str = "https://github.com/yt-dlp/yt-dlp/releases/latest/download";
const DOWNLOAD_NAME: &str = "yt-dlp.download";

/// Reading one page can take a while, because it works out the video's addresses.
const TIME_LIMIT: Duration = Duration::from_secs(120);

/// The answer for a long video is a few megabytes. This is far more than any real one.
const MAX_ANSWER_BYTES: u64 = 64 << 20;
const MAX_COMPLAINT_BYTES: u64 = 1 << 20;

/// Keeps a console window from flashing up.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Where the copy Tributary installs lives, beside its settings.
pub fn tools_folder(settings_path: &Path) -> PathBuf {
    settings_path
        .parent()
        .unwrap_or(Path::new("."))
        .join(TOOLS_FOLDER)
}

/// Where to find yt-dlp. A chosen path is the only one tried, so a mistake shows up
/// rather than another copy quietly being used.
pub fn find_program(
    chosen: &str,
    tools: &Path,
    search_path: Option<&std::ffi::OsStr>,
) -> Option<PathBuf> {
    let chosen = chosen.trim();
    if !chosen.is_empty() {
        return Some(PathBuf::from(chosen)).filter(|path| path.is_file());
    }

    let own = tools.join(PROGRAM);
    if own.is_file() {
        return Some(own);
    }
    env::split_paths(search_path?)
        .map(|folder| folder.join(PROGRAM))
        .find(|path| path.is_file())
}

pub fn program_for(state: &AppState) -> Option<PathBuf> {
    let chosen = state.settings_snapshot().ytdlp_path;
    find_program(
        &chosen,
        &tools_folder(&state.settings_path),
        env::var_os("PATH").as_deref(),
    )
}

/// Only web pages are given to yt-dlp.
fn check_page(address: &str) -> Result<Url, Error> {
    let url = Url::parse(address.trim())
        .map_err(|_| Error::Invalid("that isn't a web address".into()))?;
    if matches!(url.scheme(), "http" | "https") && url.host_str().is_some() {
        Ok(url)
    } else {
        Err(Error::Invalid("that isn't a web address".into()))
    }
}

/// The whole command line. Nothing in a page's address can become an option, because it
/// comes after \`--\`. Nothing here reads a configuration file or a cookie file.
fn arguments(url: &Url) -> Vec<String> {
    [
        "--ignore-config",
        "--no-playlist",
        "--no-warnings",
        "--no-call-home",
        "--dump-single-json",
        "--socket-timeout",
        "20",
        "--",
    ]
    .iter()
    .map(|argument| (*argument).to_owned())
    .chain([url.to_string()])
    .collect()
}

async fn read_limited(stream: Option<impl AsyncRead + Unpin>, limit: u64) -> Vec<u8> {
    let mut data = Vec::new();
    if let Some(stream) = stream {
        let _ = stream.take(limit).read_to_end(&mut data).await;
    }
    data
}

/// Asks yt-dlp what video a page holds.
pub async fn inspect(program: &Path, address: &str) -> Result<Page, Error> {
    let url = check_page(address)?;

    let mut child = Command::new(program)
        .args(arguments(&url))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|error| Error::Fetch(format!("yt-dlp couldn't start: {error}")))?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let run = async {
        let (answer, complaint, status) = tokio::join!(
            read_limited(stdout, MAX_ANSWER_BYTES),
            read_limited(stderr, MAX_COMPLAINT_BYTES),
            child.wait()
        );
        (answer, complaint, status)
    };
    let Ok((answer, complaint, status)) = tokio::time::timeout(TIME_LIMIT, run).await else {
        // Dropping the child ends it.
        return Err(Error::Fetch(
            "yt-dlp took too long to read this page".into(),
        ));
    };

    let succeeded = status.is_ok_and(|status| status.success());
    if !succeeded {
        return Err(ytdlp::explain_failure(&String::from_utf8_lossy(&complaint)));
    }
    ytdlp::parse(&String::from_utf8_lossy(&answer))
}

/// The checksum a release lists for a file.
fn checksum_for(sums: &str, file: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let (hash, name) = line.split_once(char::is_whitespace)?;
        let name = name.trim().trim_start_matches('*');
        let valid = hash.len() == 64 && hash.chars().all(|digit| digit.is_ascii_hexdigit());
        (valid && name == file).then(|| hash.to_ascii_lowercase())
    })
}

fn sha256_of(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher)?;
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

/// Downloads the latest yt-dlp from its own releases, checks it against the checksum
/// published beside it, and puts it in place. Doing this again updates it.
pub async fn install_into(engine: &Engine, tools: &Path) -> Result<PathBuf, String> {
    std::fs::create_dir_all(tools).map_err(|error| format!("couldn't make the folder: {error}"))?;

    let sums = engine
        .fetch_text(&format!("{RELEASE}/SHA2-256SUMS"), &[], None)
        .await
        .map_err(|error| format!("couldn't read the published checksums: {error}"))?;
    let wanted =
        checksum_for(&sums, PROGRAM).ok_or("the release doesn't list a checksum for yt-dlp")?;

    let _ = std::fs::remove_file(tools.join(DOWNLOAD_NAME));
    let id = engine
        .add(NewDownload {
            url: format!("{RELEASE}/{PROGRAM}"),
            dest_dir: tools.to_path_buf(),
            file_name: Some(DOWNLOAD_NAME.to_owned()),
            ..NewDownload::default()
        })
        .map_err(|error| error.to_string())?;
    let done = engine.wait(id).await.map_err(|error| error.to_string())?;

    // It was only in the list to show progress. The file stays.
    let _ = engine.remove(id, false).await;

    if done.state != State::Completed {
        return Err(format!(
            "the download failed: {}",
            done.error.unwrap_or_else(|| "it didn't finish".into())
        ));
    }
    let downloaded = done.final_path.ok_or("the download has no file")?;

    let found = tokio::task::spawn_blocking({
        let downloaded = downloaded.clone();
        move || sha256_of(&downloaded)
    })
    .await
    .map_err(|error| error.to_string())?
    .map_err(|error| format!("couldn't check the download: {error}"))?;

    if found != wanted {
        let _ = std::fs::remove_file(&downloaded);
        return Err(
            "the download doesn't match its published checksum, so it was thrown away".into(),
        );
    }

    let target = tools.join(PROGRAM);
    std::fs::rename(&downloaded, &target)
        .map_err(|error| format!("couldn't put it in place (is it running?): {error}"))?;
    Ok(target)
}

/// Installs or updates yt-dlp, for the button in the window.
#[tauri::command]
pub async fn install_ytdlp(state: Managed<'_, AppState>) -> Result<String, String> {
    let tools = tools_folder(&state.settings_path);
    install_into(&state.engine, &tools)
        .await
        .map(|path| path.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::fs;

    use super::*;

    #[test]
    fn a_pages_address_can_never_become_an_option() {
        let url = Url::parse("https://x.test/watch?v=--exec=calc").unwrap();
        let all = arguments(&url);

        let separator = all.iter().position(|item| item == "--").unwrap();
        assert_eq!(all.len(), separator + 2, "only the address follows");
        assert_eq!(all.last().unwrap(), url.as_str());
    }

    #[test]
    fn no_cookie_or_configuration_is_ever_read() {
        let all = arguments(&Url::parse("https://x.test/").unwrap()).join(" ");

        assert!(all.contains("--ignore-config"));
        assert!(!all.to_ascii_lowercase().contains("cookie"));
        assert!(!all.contains("--exec"));
        assert!(all.contains("--no-playlist"));
    }

    #[test]
    fn only_web_pages_are_accepted() {
        assert!(check_page("https://x.test/watch?v=1").is_ok());
        assert!(check_page(" http://x.test/ ").is_ok());
        for bad in [
            "file:///C:/Windows/win.ini",
            "ftp://x.test/a",
            "javascript:alert(1)",
            "-o x",
            "",
            "x.test",
        ] {
            assert!(check_page(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_chosen_program_is_the_only_one_tried() {
        let folder = tempfile::tempdir().unwrap();
        let tools = folder.path().join("tools");
        fs::create_dir_all(&tools).unwrap();
        fs::write(tools.join(PROGRAM), b"").unwrap();

        let elsewhere = folder.path().join("mine.exe");
        fs::write(&elsewhere, b"").unwrap();

        let found = find_program(&elsewhere.to_string_lossy(), &tools, None);
        assert_eq!(found, Some(elsewhere));
        assert_eq!(find_program(r"C:\no\such\yt-dlp.exe", &tools, None), None);
    }

    #[test]
    fn the_installed_copy_comes_before_the_search_path() {
        let folder = tempfile::tempdir().unwrap();
        let tools = folder.path().join("tools");
        let other = folder.path().join("other");
        fs::create_dir_all(&tools).unwrap();
        fs::create_dir_all(&other).unwrap();
        fs::write(tools.join(PROGRAM), b"").unwrap();
        fs::write(other.join(PROGRAM), b"").unwrap();
        let search = OsString::from(&other);

        assert_eq!(
            find_program("", &tools, Some(&search)),
            Some(tools.join(PROGRAM))
        );
        fs::remove_file(tools.join(PROGRAM)).unwrap();
        assert_eq!(
            find_program("", &tools, Some(&search)),
            Some(other.join(PROGRAM))
        );
    }

    #[test]
    fn it_may_simply_not_be_there() {
        let folder = tempfile::tempdir().unwrap();
        let search = OsString::from(folder.path());

        assert_eq!(find_program("", folder.path(), Some(&search)), None);
        assert_eq!(find_program("", folder.path(), None), None);
    }

    #[test]
    fn a_published_checksum_is_found_by_file_name() {
        let hash = "f".repeat(64);
        let sums = format!(
            "{} yt-dlp\n{hash}  yt-dlp.exe\n{} *yt-dlp_x86.exe\n",
            "0".repeat(64),
            "1".repeat(64)
        );

        assert_eq!(checksum_for(&sums, "yt-dlp.exe"), Some(hash));
        assert_eq!(checksum_for(&sums, "yt-dlp_x86.exe"), Some("1".repeat(64)));
        assert_eq!(checksum_for(&sums, "missing.exe"), None);
    }

    #[test]
    fn a_line_that_is_not_a_checksum_is_ignored() {
        let sums = "short yt-dlp.exe\nnot-hex-not-hex-not-hex-not-hex-not-hex-not-hex-not-hex-not-hex yt-dlp.exe\n";
        assert_eq!(checksum_for(sums, "yt-dlp.exe"), None);
    }

    #[test]
    fn a_files_checksum_is_computed() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("a");
        fs::write(&path, b"abc").unwrap();

        assert_eq!(
            sha256_of(&path).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[tokio::test]
    async fn a_missing_program_is_explained() {
        let problem = inspect(Path::new(r"C:\no\such\yt-dlp.exe"), "https://x.test/").await;
        assert!(matches!(problem, Err(Error::Fetch(text)) if text.contains("couldn't start")));
    }

    #[tokio::test]
    async fn a_page_that_is_not_a_web_address_never_starts_anything() {
        let problem = inspect(
            Path::new(r"C:\no\such\yt-dlp.exe"),
            "file:///C:/Windows/win.ini",
        )
        .await;
        assert!(matches!(problem, Err(Error::Invalid(_))), "{problem:?}");
    }
}
