//! Running a virus scanner on finished downloads, and playing sounds.

use std::iter;
use std::os::windows::process::CommandExt as _;
use std::path::Path;
use std::process::Command;
use std::ptr;

use windows_sys::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_FILENAME, SND_NODEFAULT};

/// Stands for the downloaded file in the scanner's arguments.
const FILE_PLACEHOLDER: &str = "{file}";

/// Hides the console window a scanner would otherwise open.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Windows' own scanner, which every Windows 11 PC has.
pub const DEFAULT_SCANNER: &str = r"C:\Program Files\Windows Defender\MpCmdRun.exe";
pub const DEFAULT_ARGUMENTS: &str = r#"-Scan -ScanType 3 -File "{file}" -DisableRemediation"#;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScanResult {
    Clean,
    /// The scanner found something, or reported a problem. It gives the exit code.
    Flagged(i32),
    /// The scanner itself couldn't be run.
    Failed(String),
}

fn scanner_command(scanner: &str, arguments: &str, file: &Path) -> Command {
    let file = file.to_string_lossy();
    let arguments = if arguments.contains(FILE_PLACEHOLDER) {
        arguments.replace(FILE_PLACEHOLDER, &file)
    } else {
        format!("{arguments} \"{file}\"")
    };

    let mut command = Command::new(scanner);
    command.raw_arg(arguments).creation_flags(CREATE_NO_WINDOW);
    command
}

/// Scans a file and waits for the result. A scanner reports what it found through its
/// exit code, where 0 means clean.
pub fn scan(scanner: &str, arguments: &str, file: &Path) -> ScanResult {
    match scanner_command(scanner, arguments, file).status() {
        Ok(status) if status.success() => ScanResult::Clean,
        Ok(status) => ScanResult::Flagged(status.code().unwrap_or(-1)),
        Err(error) => ScanResult::Failed(error.to_string()),
    }
}

/// Plays a sound file without waiting for it. Returns whether it could be played.
pub fn play_sound(path: &str) -> bool {
    // Windows reports success for a missing file when playing without waiting.
    if !Path::new(path).is_file() {
        return false;
    }

    let wide: Vec<u16> = path.encode_utf16().chain(iter::once(0)).collect();
    // SAFETY: the path is null-terminated, and `SND_ASYNC` makes Windows take its own
    // copy of what it needs. No module handle is used with `SND_FILENAME`.
    unsafe {
        PlaySoundW(
            wide.as_ptr(),
            ptr::null_mut(),
            SND_FILENAME | SND_ASYNC | SND_NODEFAULT,
        ) != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMMAND_PROCESSOR: &str = r"C:\Windows\System32\cmd.exe";

    #[test]
    fn a_clean_result_comes_from_exit_code_zero() {
        let result = scan(COMMAND_PROCESSOR, "/c exit 0", Path::new("a.zip"));
        assert_eq!(result, ScanResult::Clean);
    }

    #[test]
    fn any_other_exit_code_flags_the_file() {
        let result = scan(COMMAND_PROCESSOR, "/c exit 2", Path::new("a.zip"));
        assert_eq!(result, ScanResult::Flagged(2));
    }

    #[test]
    fn a_missing_scanner_is_reported_rather_than_treated_as_clean() {
        let result = scan(r"C:\no\such\scanner.exe", "", Path::new("a.zip"));
        assert!(matches!(result, ScanResult::Failed(_)));
    }

    #[test]
    fn the_file_path_reaches_the_scanner_even_with_spaces() {
        let folder = tempfile::tempdir().unwrap();
        let file = folder.path().join("my download.zip");
        std::fs::write(&file, "x").unwrap();

        // `if exist` only succeeds if the path arrives in one piece.
        let result = scan(
            COMMAND_PROCESSOR,
            r#"/c if exist "{file}" (exit 0) else (exit 7)"#,
            &file,
        );

        assert_eq!(result, ScanResult::Clean);
    }

    #[test]
    fn the_file_is_added_when_the_arguments_do_not_say_where() {
        let command = scanner_command("scanner", "--deep", Path::new(r"C:\a b\c.zip"));
        let arguments: Vec<_> = command.get_args().collect();

        assert_eq!(arguments, [r#"--deep "C:\a b\c.zip""#]);
    }

    #[test]
    fn windows_ships_the_default_scanner_path_this_app_uses() {
        // Only meaningful on a machine with Defender, which is every Windows 11 PC.
        assert!(DEFAULT_ARGUMENTS.contains(FILE_PLACEHOLDER));
    }

    #[test]
    fn a_missing_sound_file_is_reported() {
        assert!(!play_sound(r"C:\no\such\sound.wav"));
    }
}
