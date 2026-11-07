//! The Windows clipboard, dragging files out, the command line, and registering with
//! browsers.

use std::fs;
use std::io;
use std::iter;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::ptr;
use std::slice;

use drag::{DragItem, Image, Options};
use serde_json::json;
use tauri::{WebviewWindow, Window};
use url::Url;
use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Settings4;
use windows_core::Interface as _;
use windows_sys::Win32::Foundation::{ERROR_SUCCESS, HANDLE};
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, GetClipboardData, GetClipboardSequenceNumber, OpenClipboard,
};
use windows_sys::Win32::System::Memory::{GlobalLock, GlobalUnlock};
use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, REG_SZ, RegSetKeyValueW};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON, VK_RBUTTON};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_SWAPBUTTON};

/// The clipboard format for text.
const CF_UNICODETEXT: u32 = 13;

/// The picture beside the cursor during a drag.
const DRAG_IMAGE: &[u8] = include_bytes!("../icons/32x32.png");

const HOST_NAME: &str = "app.tributary.capture";

/// Fixed by the public key in the extension's manifest.
pub const EXTENSION_ID: &str = "khhpmnkkeejcjhbnknnghhahncphbobk";

/// Where each browser looks for native messaging hosts, under the current user.
const BROWSER_KEYS: [&str; 3] = [
    r"Software\Google\Chrome\NativeMessagingHosts",
    r"Software\Microsoft\Edge\NativeMessagingHosts",
    r"Software\BraveSoftware\Brave-Browser\NativeMessagingHosts",
];

/// Stops the browser engine remembering what's typed into the window. It would otherwise
/// keep it in a plain file of its own, and an address can hold a password. It ignores
/// `autocomplete="off"`, so this has to be done here. All the windows share this setting,
/// and a new one resets it, so every window has to be given this when it's created.
pub fn stop_remembering_typed_text(window: &WebviewWindow) {
    let _ = window.with_webview(|webview| {
        // SAFETY: plain calls on a live webview, made on the thread that owns it.
        let _ = unsafe {
            webview
                .controller()
                .CoreWebView2()
                .and_then(|core| core.Settings())
                .and_then(|settings| settings.cast::<ICoreWebView2Settings4>())
                .and_then(|settings| {
                    settings.SetIsGeneralAutofillEnabled(false)?;
                    settings.SetIsPasswordAutosaveEnabled(false)
                })
        };
    });
}

/// Whether the main mouse button is down, allowing for the buttons being swapped.
fn main_button_held() -> bool {
    // SAFETY: these calls only read input state.
    unsafe {
        let button = if GetSystemMetrics(SM_SWAPBUTTON) == 0 {
            VK_LBUTTON
        } else {
            VK_RBUTTON
        };
        GetAsyncKeyState(i32::from(button)) < 0
    }
}

/// Drags files out of the window, returning when they're dropped or the drag is cancelled.
/// Has to run on the main thread.
pub fn drag_out(window: &Window, files: Vec<PathBuf>) -> Result<(), String> {
    // A drag with no button held counts as finished, so the files would land on whatever
    // is under the cursor. That's what a quick flick that has already ended would do.
    if !main_button_held() {
        return Ok(());
    }

    // The library panics if Windows can't describe the files, and this thread must survive
    // that because the app's downloads depend on it.
    let dragged = panic::catch_unwind(AssertUnwindSafe(|| {
        drag::start_drag(
            window,
            DragItem::Files(files),
            Image::Raw(DRAG_IMAGE.to_vec()),
            |_, _| {},
            Options::default(),
        )
    }));

    match dragged {
        Ok(result) => result.map_err(|error| error.to_string()),
        Err(_) => Err("Windows couldn't start the drag".to_owned()),
    }
}

/// Changes whenever anything copies to the clipboard.
pub fn clipboard_sequence() -> u32 {
    // SAFETY: this call takes no arguments.
    unsafe { GetClipboardSequenceNumber() }
}

/// The text on the clipboard, if there is any.
pub fn clipboard_text() -> Option<String> {
    // SAFETY: a null owner is allowed, and the clipboard is closed again below.
    if unsafe { OpenClipboard(ptr::null_mut()) } == 0 {
        return None;
    }

    // SAFETY: the clipboard is open. The handle stays valid until it's closed, and text
    // in this format is null-terminated.
    unsafe {
        let handle: HANDLE = GetClipboardData(CF_UNICODETEXT);
        let mut text = None;
        if !handle.is_null() {
            let locked = GlobalLock(handle).cast::<u16>();
            if !locked.is_null() {
                let mut length = 0;
                while *locked.add(length) != 0 {
                    length += 1;
                }
                text = Some(String::from_utf16_lossy(slice::from_raw_parts(
                    locked, length,
                )));
                GlobalUnlock(handle);
            }
        }
        CloseClipboard();
        text
    }
}

/// Whether copied text is a link to a file type that's worth offering to download.
pub fn download_link(text: &str, file_types: &[String]) -> Option<String> {
    let text = text.trim();
    if text.chars().any(char::is_whitespace) {
        return None;
    }

    let url = Url::parse(text).ok()?;
    if !matches!(url.scheme(), "http" | "https" | "ftp") {
        return None;
    }

    let (_, extension) = url.path().rsplit('/').next()?.rsplit_once('.')?;
    file_types
        .iter()
        .any(|known| known.eq_ignore_ascii_case(extension))
        .then(|| text.to_owned())
}

/// What the command line asked for.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Invocation {
    pub add: Option<CommandLineAdd>,
    pub start_queue: bool,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct CommandLineAdd {
    pub url: String,
    pub folder: Option<PathBuf>,
    pub name: Option<String>,
    /// Adds it to the main queue instead of starting it.
    pub later: bool,
    /// Skips the start dialog.
    pub silent: bool,
    /// Closes the app once this download has finished.
    pub quit_when_done: bool,
}

/// Reads the arguments, without the program's own name.
pub fn parse_arguments(arguments: &[String]) -> Result<Invocation, String> {
    let mut invocation = Invocation::default();
    let mut add = CommandLineAdd::default();
    let mut remaining = arguments.iter();

    while let Some(argument) = remaining.next() {
        let mut value = |flag: &str| {
            remaining
                .next()
                .cloned()
                .ok_or_else(|| format!("{flag} needs a value"))
        };

        match argument.as_str() {
            "--add" => add.url = value("--add")?,
            "--folder" => add.folder = Some(PathBuf::from(value("--folder")?)),
            "--name" => add.name = Some(value("--name")?),
            "--later" => add.later = true,
            "--silent" => add.silent = true,
            "--quit-when-done" => add.quit_when_done = true,
            "--start-queue" => invocation.start_queue = true,
            "--minimized" => {}
            other => return Err(format!("unknown option {other}")),
        }
    }

    // Without these the download might never start, and the app would wait for ever.
    if add.quit_when_done && (add.url.is_empty() || !add.silent || add.later) {
        return Err("--quit-when-done needs --add and --silent".to_owned());
    }

    if !add.url.is_empty() {
        invocation.add = Some(add);
    }
    Ok(invocation)
}

/// Takes the `\\?\` off a path. Windows puts it on some paths it hands back, but not
/// everything accepts it: browsers start their host through `cmd.exe`, and it would be
/// baffling in a folder someone is told to pick.
pub fn plain_path(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    match text.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with(r"UNC\") => PathBuf::from(rest),
        _ => path.to_owned(),
    }
}

fn host_manifest(host_path: &Path) -> String {
    json!({
        "name": HOST_NAME,
        "description": "Tributary download manager",
        "path": plain_path(host_path),
        "type": "stdio",
        "allowed_origins": [format!("chrome-extension://{EXTENSION_ID}/")],
    })
    .to_string()
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(iter::once(0)).collect()
}

/// Tells browsers where the host program is. Safe to repeat, and only changes the
/// current user's settings.
pub fn register_with_browsers(data_dir: &Path, host_path: &Path) -> io::Result<()> {
    let manifest_path = data_dir.join("native-host.json");
    fs::write(&manifest_path, host_manifest(host_path))?;

    let value = wide(&manifest_path.to_string_lossy());
    for key in BROWSER_KEYS {
        let subkey = wide(&format!(r"{key}\{HOST_NAME}"));
        // SAFETY: the strings are null-terminated, and the size covers the whole value.
        let status = unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                subkey.as_ptr(),
                ptr::null(),
                REG_SZ,
                value.as_ptr().cast(),
                (value.len() * size_of::<u16>()) as u32,
            )
        };
        if status != ERROR_SUCCESS {
            return Err(io::Error::from_raw_os_error(status as i32));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn types() -> Vec<String> {
        ["zip", "mp4"].map(str::to_owned).to_vec()
    }

    fn arguments(text: &[&str]) -> Vec<String> {
        text.iter().map(|&item| item.to_owned()).collect()
    }

    #[test]
    fn a_link_to_a_listed_file_type_is_offered() {
        let link = download_link("  https://x.test/a/b.ZIP?token=1 \n", &types());
        assert_eq!(link.as_deref(), Some("https://x.test/a/b.ZIP?token=1"));
    }

    #[test]
    fn other_copied_text_is_ignored() {
        for text in [
            "https://x.test/page.html",
            "https://x.test/",
            "just some words",
            "see https://x.test/a.zip for details",
            "file:///C:/a.zip",
            "",
        ] {
            assert_eq!(download_link(text, &types()), None, "{text}");
        }
    }

    #[test]
    fn reads_a_full_command_line() {
        let invocation = parse_arguments(&arguments(&[
            "--add",
            "https://x.test/a.zip",
            "--folder",
            r"D:\Files",
            "--name",
            "b.zip",
            "--silent",
        ]))
        .unwrap();

        assert_eq!(
            invocation.add,
            Some(CommandLineAdd {
                url: "https://x.test/a.zip".to_owned(),
                folder: Some(PathBuf::from(r"D:\Files")),
                name: Some("b.zip".to_owned()),
                later: false,
                silent: true,
                quit_when_done: false,
            })
        );
    }

    #[test]
    fn the_app_can_close_after_a_download_that_starts_by_itself() {
        let invocation = parse_arguments(&arguments(&[
            "--add",
            "https://x.test/a.zip",
            "--silent",
            "--quit-when-done",
        ]))
        .unwrap();

        assert!(invocation.add.unwrap().quit_when_done);
    }

    #[test]
    fn closing_afterwards_is_refused_if_the_download_might_never_start() {
        let url = "https://x.test/a.zip";
        let cases: [&[&str]; 3] = [
            &["--quit-when-done"],
            &["--add", url, "--quit-when-done"],
            &["--add", url, "--silent", "--later", "--quit-when-done"],
        ];

        for case in cases {
            assert!(parse_arguments(&arguments(case)).is_err(), "{case:?}");
        }
    }

    #[test]
    fn starting_a_queue_needs_no_download() {
        let invocation = parse_arguments(&arguments(&["--start-queue"])).unwrap();

        assert!(invocation.start_queue);
        assert_eq!(invocation.add, None);
    }

    #[test]
    fn opening_at_sign_in_is_not_an_error() {
        assert_eq!(
            parse_arguments(&arguments(&["--minimized"])).unwrap(),
            Invocation::default()
        );
    }

    #[test]
    fn bad_command_lines_are_refused() {
        assert!(parse_arguments(&arguments(&["--add"])).is_err());
        assert!(parse_arguments(&arguments(&["--nonsense"])).is_err());
    }

    #[test]
    fn the_long_path_prefix_is_taken_off() {
        assert_eq!(
            plain_path(Path::new(r"\\?\C:\Users\me\host.exe")),
            Path::new(r"C:\Users\me\host.exe")
        );
        assert_eq!(plain_path(Path::new(r"C:\a.exe")), Path::new(r"C:\a.exe"));
    }

    #[test]
    fn a_network_path_keeps_its_prefix_because_it_would_mean_something_else_without_it() {
        let path = Path::new(r"\\?\UNC\server\share\a.exe");
        assert_eq!(plain_path(path), path);
    }

    #[test]
    fn the_host_manifest_gives_browsers_a_path_they_can_start() {
        let text = host_manifest(Path::new(r"\\?\C:\Program\tributary-host.exe"));
        let manifest: serde_json::Value = serde_json::from_str(&text).unwrap();

        assert_eq!(manifest["path"], r"C:\Program\tributary-host.exe");
    }

    #[test]
    fn the_host_manifest_only_allows_our_extension() {
        let text = host_manifest(Path::new(r"C:\Program\tributary-host.exe"));
        let manifest: serde_json::Value = serde_json::from_str(&text).unwrap();

        assert_eq!(manifest["name"], HOST_NAME);
        assert_eq!(manifest["path"], r"C:\Program\tributary-host.exe");
        assert_eq!(
            manifest["allowed_origins"],
            json!(["chrome-extension://khhpmnkkeejcjhbnknnghhahncphbobk/"])
        );
    }

    #[test]
    fn the_extension_id_matches_the_key_in_its_manifest() {
        let text = include_str!("../../extension/manifest.json");
        let manifest: serde_json::Value = serde_json::from_str(text).unwrap();
        assert!(manifest["key"].as_str().is_some_and(|key| key.len() > 300));
        assert_eq!(EXTENSION_ID.len(), 32);
    }
}
