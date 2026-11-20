//! Getting a fresh link for a download whose link has expired, as signed links do. The
//! user asks for one, the page the file came from opens in the browser, and the next time
//! the browser hands over a file of the same name, Tributary carries on with it from where
//! it stopped rather than starting again.

use std::collections::HashMap;
use std::env;

use engine::{DownloadEdit, DownloadId, State};
use tauri::{AppHandle, Emitter, Manager, State as Managed};
use tauri_plugin_opener::OpenerExt as _;

use crate::capture::Captured;
use crate::commands::AppState;

/// Set in tests, so no browser opens.
const NO_BROWSER_VARIABLE: &str = "TRIBUTARY_NO_BROWSER";

/// Whether a download failed because its link no longer works, rather than for a reason a
/// fresh link wouldn't fix.
pub fn link_expired(error: &str) -> bool {
    ["answered 403", "answered 404", "answered 410"]
        .iter()
        .any(|answer| error.contains(answer))
}

/// Downloads waiting for a fresh link, by the name of their file.
#[derive(Debug, Default)]
pub struct Awaiting {
    names: HashMap<i64, String>,
}

impl Awaiting {
    pub fn wait(&mut self, id: i64, name: &str) {
        self.names.insert(id, name.to_owned());
    }

    pub fn contains(&self, id: i64) -> bool {
        self.names.contains_key(&id)
    }

    pub fn forget(&mut self, id: i64) {
        self.names.remove(&id);
    }

    /// The download waiting for a file of this name, which stops waiting.
    pub fn take_match(&mut self, name: &str) -> Option<i64> {
        let id = self
            .names
            .iter()
            .filter(|(_, waiting)| waiting.eq_ignore_ascii_case(name))
            .map(|(id, _)| *id)
            .min()?;
        self.names.remove(&id);
        Some(id)
    }
}

/// The last part of an address's path, which is what a file is usually called.
fn name_in(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|url| {
            url.path_segments()?
                .next_back()
                .filter(|name| !name.is_empty())
                .map(|name| {
                    percent_encoding::percent_decode_str(name)
                        .decode_utf8_lossy()
                        .into_owned()
                })
        })
        .unwrap_or_default()
}

/// Waits for a fresh link for a download, opening the page it came from. Returns that page,
/// or nothing if the download doesn't say which page it was, when the user has to find it.
#[tauri::command]
pub async fn get_fresh_link(
    app: AppHandle,
    state: Managed<'_, AppState>,
    id: i64,
) -> Result<Option<String>, String> {
    let download = state
        .engine
        .get(DownloadId(id))
        .map_err(|error| error.to_string())?;
    if matches!(download.state, State::Running | State::Completed) {
        return Err("Only a download that has stopped needs a fresh link.".to_owned());
    }
    let name = download
        .file_name
        .clone()
        .unwrap_or_else(|| name_in(&download.url));
    state.awaiting_link().wait(id, &name);
    let page = download
        .headers
        .iter()
        .find(|(header, _)| header.eq_ignore_ascii_case("referer"))
        .map(|(_, value)| value.clone())
        .filter(|page| page.starts_with("https://") || page.starts_with("http://"));
    if let Some(page) = &page
        && env::var_os(NO_BROWSER_VARIABLE).is_none()
    {
        app.opener()
            .open_url(page, None::<&str>)
            .map_err(|error| error.to_string())?;
    }
    let _ = app.emit("queues-changed", ());
    Ok(page)
}

/// Stops waiting for a fresh link.
#[tauri::command]
pub async fn stop_waiting_for_link(
    app: AppHandle,
    state: Managed<'_, AppState>,
    id: i64,
) -> Result<(), String> {
    state.awaiting_link().forget(id);
    let _ = app.emit("queues-changed", ());
    Ok(())
}

/// Hands a file the browser sent to a download waiting for a fresh link, if one is waiting
/// for a file of that name. It carries on from where it stopped, with the browser's
/// cookies for the new link, or starts again if the server says the file has changed.
/// True if the file was taken this way.
pub fn carry_on_with(app: &AppHandle, file: &Captured) -> Result<bool, String> {
    let state = app.state::<AppState>();
    let name = file
        .file_name
        .clone()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| name_in(&file.url));
    let Some(id) = state.awaiting_link().take_match(&name) else {
        return Ok(false);
    };
    let id = DownloadId(id);
    state
        .engine
        .edit(
            id,
            DownloadEdit {
                url: Some(file.url.clone()),
                ..DownloadEdit::default()
            },
        )
        .map_err(|error| error.to_string())?;
    // Cookies are kept in memory only, as for any captured download.
    let secret: Vec<(String, String)> = file
        .headers()
        .into_iter()
        .filter(|(header, _)| header.eq_ignore_ascii_case("cookie"))
        .collect();
    state
        .engine
        .resume(id, &secret)
        .map_err(|error| error.to_string())?;
    let _ = app.emit("queues-changed", ());
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refusals_a_fresh_link_can_fix_are_told_apart() {
        assert!(link_expired("the server answered 403 Forbidden"));
        assert!(link_expired("the server answered 410 Gone"));
        assert!(link_expired("the server answered 404 Not Found"));
        assert!(!link_expired(
            "the server answered 500 Internal Server Error"
        ));
        assert!(!link_expired("could not connect"));
        assert!(!link_expired("the file is larger than expected"));
    }

    #[test]
    fn a_file_of_the_same_name_is_matched_once() {
        let mut awaiting = Awaiting::default();
        awaiting.wait(7, "Film.MKV");
        awaiting.wait(3, "other.zip");

        assert_eq!(awaiting.take_match("film.mkv"), Some(7));
        assert_eq!(awaiting.take_match("film.mkv"), None);
        assert!(awaiting.contains(3));
        awaiting.forget(3);
        assert_eq!(awaiting.take_match("other.zip"), None);
    }

    #[test]
    fn the_oldest_of_two_waiting_downloads_goes_first() {
        let mut awaiting = Awaiting::default();
        awaiting.wait(9, "a.iso");
        awaiting.wait(4, "a.iso");
        assert_eq!(awaiting.take_match("a.iso"), Some(4));
        assert_eq!(awaiting.take_match("a.iso"), Some(9));
    }

    #[test]
    fn a_name_is_read_from_the_address() {
        assert_eq!(
            name_in("https://x.test/files/My%20Film.mkv?sig=1"),
            "My Film.mkv"
        );
        assert_eq!(name_in("https://x.test/"), "");
        assert_eq!(name_in("not an address"), "");
    }
}
