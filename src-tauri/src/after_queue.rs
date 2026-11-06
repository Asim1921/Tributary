//! What happens when a queue ends, and waking the computer for a scheduled start.

use std::ffi::c_void;
use std::os::windows::process::CommandExt as _;
use std::process::Command;
use std::ptr;

use engine::{Moment, Queue};
use serde::{Deserialize, Serialize};
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::System::Threading::{
    CancelWaitableTimer, CreateWaitableTimerW, SetWaitableTimer,
};

/// How long the user has to cancel a shutdown.
pub const SHUTDOWN_DELAY_SECONDS: u32 = 60;

const MINUTES_PER_DAY: i64 = 24 * 60;

/// Saved with each queue. The engine stores it without looking inside.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AfterAction {
    pub shutdown: bool,
    /// Closes programs even if they have unsaved work.
    pub force: bool,
    /// A file to open or a command to run.
    pub program: String,
    /// Wakes the computer from sleep for this queue's start time.
    pub wake: bool,
}

impl AfterAction {
    pub fn parse(text: &str) -> Self {
        serde_json::from_str(text).unwrap_or_default()
    }

    pub fn to_text(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

/// The arguments for a delayed shutdown.
pub fn shutdown_arguments(force: bool) -> Vec<String> {
    let mut arguments = vec![
        "/s".to_owned(),
        "/t".to_owned(),
        SHUTDOWN_DELAY_SECONDS.to_string(),
        "/c".to_owned(),
        "Tributary finished a queue. Open Tributary to cancel.".to_owned(),
    ];
    if force {
        arguments.push("/f".to_owned());
    }
    arguments
}

pub fn start_shutdown(force: bool) -> std::io::Result<()> {
    Command::new("shutdown")
        .args(shutdown_arguments(force))
        .spawn()
        .map(drop)
}

pub fn cancel_shutdown() -> std::io::Result<()> {
    Command::new("shutdown").arg("/a").spawn().map(drop)
}

/// Builds the command for what the user set up. It goes through the command processor
/// so it can be a plain file, a batch file or a command line.
///
/// The text is passed exactly as typed, because Rust's normal argument quoting would
/// break paths the user has already quoted. `/S` with outer quotes tells the command
/// processor to keep everything between them unchanged.
fn program_command(program: &str) -> Command {
    let mut command = Command::new("cmd");
    command.raw_arg(format!("/S /C \"{program}\""));
    command
}

pub fn run_program(program: &str) -> std::io::Result<()> {
    program_command(program).spawn().map(drop)
}

/// Seconds until a queue's next scheduled start, if it has one.
pub fn seconds_until_start(queue: &Queue, now: Moment) -> Option<i64> {
    let start = i64::from(queue.start_minute?);
    let minute = i64::from(now.minute);

    (0..=7).find_map(|days_ahead| {
        let weekday = (i64::from(now.weekday) + days_ahead) % 7;
        let enabled = queue.days & (1 << weekday) != 0;
        let ahead = days_ahead * MINUTES_PER_DAY + start - minute;
        (enabled && ahead > 0).then_some(ahead * 60)
    })
}

/// A timer that wakes the computer from sleep. Windows must allow wake timers, which
/// is a setting in the power plan.
pub struct WakeTimer {
    handle: HANDLE,
}

// SAFETY: a waitable timer handle can be used from any thread.
unsafe impl Send for WakeTimer {}
// SAFETY: as above, and every call below only passes the handle to Windows.
unsafe impl Sync for WakeTimer {}

impl WakeTimer {
    pub fn new() -> Option<Self> {
        // SAFETY: null security attributes and a null name are both valid here.
        let handle = unsafe { CreateWaitableTimerW(ptr::null(), 0, ptr::null()) };
        (!handle.is_null()).then_some(Self { handle })
    }

    /// Sets the timer, replacing any earlier time. Returns whether Windows accepted it.
    pub fn set(&self, seconds_from_now: i64) -> bool {
        // Negative means relative, in units of 100 nanoseconds.
        let due: i64 = -seconds_from_now.max(1) * 10_000_000;
        let no_callback: Option<unsafe extern "system" fn(*const c_void, u32, u32)> = None;

        // SAFETY: the handle is open, `due` outlives the call, and no callback is used.
        // The final 1 asks Windows to wake the computer when the timer fires.
        unsafe { SetWaitableTimer(self.handle, &due, 0, no_callback, ptr::null(), 1) != 0 }
    }

    pub fn cancel(&self) {
        // SAFETY: the handle is open.
        unsafe {
            CancelWaitableTimer(self.handle);
        }
    }
}

impl Drop for WakeTimer {
    fn drop(&mut self) {
        // SAFETY: the handle is open and is not used after this.
        unsafe {
            CloseHandle(self.handle);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn moment(weekday: u8, minute: u16) -> Moment {
        Moment {
            seconds: 0,
            day: 100,
            weekday,
            minute,
        }
    }

    fn scheduled(start: u16, days: u8) -> Queue {
        Queue {
            start_minute: Some(start),
            days,
            ..Queue::new("test")
        }
    }

    #[test]
    fn a_shutdown_always_has_a_delay_to_cancel_in() {
        let arguments = shutdown_arguments(false);

        assert_eq!(arguments[0], "/s");
        assert_eq!(arguments[1], "/t");
        assert_eq!(arguments[2], "60");
        assert!(!arguments.contains(&"/f".to_owned()));
    }

    #[test]
    fn forcing_is_only_added_when_asked_for() {
        assert!(shutdown_arguments(true).contains(&"/f".to_owned()));
    }

    #[test]
    fn actions_survive_being_saved_with_a_queue() {
        let action = AfterAction {
            shutdown: true,
            force: false,
            program: r"C:\scripts\done.bat".to_owned(),
            wake: true,
        };

        assert_eq!(AfterAction::parse(&action.to_text()), action);
    }

    #[test]
    fn a_blank_or_broken_action_does_nothing() {
        assert_eq!(AfterAction::parse(""), AfterAction::default());
        assert_eq!(AfterAction::parse("{ not json"), AfterAction::default());
        assert!(!AfterAction::parse("").shutdown);
    }

    #[test]
    fn finds_the_next_start_today() {
        let queue = scheduled(10 * 60, 0b111_1111);
        assert_eq!(seconds_until_start(&queue, moment(0, 9 * 60)), Some(3600));
    }

    #[test]
    fn a_start_already_passed_means_tomorrow() {
        let queue = scheduled(9 * 60, 0b111_1111);
        assert_eq!(
            seconds_until_start(&queue, moment(0, 10 * 60)),
            Some(23 * 3600)
        );
    }

    #[test]
    fn skips_to_the_next_enabled_day() {
        // Only Wednesday, asked on a Monday.
        let queue = scheduled(9 * 60, 0b000_0100);
        assert_eq!(
            seconds_until_start(&queue, moment(0, 9 * 60)),
            Some(2 * 24 * 3600)
        );
    }

    #[test]
    fn wraps_around_the_end_of_the_week() {
        // Only Monday, asked on a Sunday.
        let queue = scheduled(9 * 60, 0b000_0001);
        assert_eq!(
            seconds_until_start(&queue, moment(6, 9 * 60)),
            Some(24 * 3600)
        );
    }

    #[test]
    fn no_start_time_or_no_days_means_no_answer() {
        assert_eq!(seconds_until_start(&Queue::new("x"), moment(0, 0)), None);
        assert_eq!(seconds_until_start(&scheduled(60, 0), moment(0, 0)), None);
    }

    #[test]
    fn a_program_runs_from_a_path_with_spaces() {
        let folder = tempfile::tempdir().unwrap();
        let spaced = folder.path().join("my folder");
        std::fs::create_dir(&spaced).unwrap();

        let script = spaced.join("run me.bat");
        let output = spaced.join("it ran.txt");
        std::fs::write(&script, format!("@echo ok> \"{}\"", output.display())).unwrap();

        let status = program_command(&format!("\"{}\"", script.display()))
            .status()
            .unwrap();

        assert!(status.success());
        assert!(output.exists(), "the script should have written its file");
    }

    #[test]
    fn a_command_line_with_quotes_and_arguments_runs_unchanged() {
        let folder = tempfile::tempdir().unwrap();
        let output = folder.path().join("out put.txt");

        let status = program_command(&format!("echo hello> \"{}\"", output.display()))
            .status()
            .unwrap();

        assert!(status.success());
        let text = std::fs::read_to_string(&output).unwrap();
        assert_eq!(text.trim(), "hello");
    }

    #[test]
    fn the_wake_timer_can_be_set_and_cancelled() {
        let timer = WakeTimer::new().expect("Windows should create a timer");

        assert!(timer.set(3600));
        timer.cancel();
    }
}
