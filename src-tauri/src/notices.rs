//! Windows notifications for finished and failed downloads, shown while Tributary is out
//! of sight. A batch or a queue says so once rather than once a file.

use std::collections::HashMap;
use std::time::Duration;

use engine::{DownloadId, QueueId};
use tauri::{AppHandle, Manager};
use tauri_plugin_opener::OpenerExt as _;
use tauri_winrt_notification::Toast;

use crate::commands::AppState;

/// The app's ID, which its Start menu shortcut carries. Windows shows notifications under
/// the name and icon that go with it.
const APP_ID: &str = "app.tributary.desktop";

/// Set by the test suites. A test copy shares the installed app's ID, so its notifications
/// would otherwise land in the user's own notification list.
const QUIET_VARIABLE: &str = "TRIBUTARY_NO_NOTIFICATIONS";

/// How long to wait for more downloads to finish, so they're told of together.
const GATHER_TIME: Duration = Duration::from_secs(2);

/// What a notification says, and what its buttons do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub title: String,
    pub text: String,
    /// Each button's label and what it does, such as `open:12`.
    pub buttons: Vec<(&'static str, String)>,
}

/// A download that finished, as a notification describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finish {
    pub id: i64,
    pub name: String,
    /// The kind of checksum it was checked against, and whether it matched.
    pub checked: Option<(&'static str, bool)>,
}

/// What to do about something that happened.
#[derive(Debug, PartialEq, Eq)]
pub enum Step {
    /// Say this now.
    Now(Notice),
    /// Wait a moment for others, then say them together.
    Gather,
    /// Nothing yet: a queue says it when it ends.
    Quiet,
}

/// Downloads waiting to be told of, gathered so many don't make many notifications.
#[derive(Debug, Default)]
pub struct Notices {
    /// Files finished in each running queue, told of when the queue ends.
    queues: HashMap<i64, usize>,
    finished: Vec<Finish>,
    /// Each failed file's name and why.
    failed: Vec<(String, String)>,
}

impl Notices {
    /// A download finished. `queue` is the queue it ran in, if that queue is running.
    pub fn finished(&mut self, finish: Finish, queue: Option<i64>) -> Step {
        // A file that doesn't match its checksum is worth saying at once, on its own.
        if let Some((kind, false)) = finish.checked {
            let name = &finish.name;
            return Step::Now(Notice {
                title: text!("Checksum doesn't match"),
                text: text!(
                    "{name} doesn't match its {kind} checksum. It may be damaged, so download it again before opening it."
                ),
                buttons: vec![(en!("Show in folder"), format!("folder:{}", finish.id))],
            });
        }
        if let Some(queue) = queue {
            *self.queues.entry(queue).or_default() += 1;
            return Step::Quiet;
        }
        self.finished.push(finish);
        if self.finished.len() == 1 && self.failed.is_empty() {
            Step::Gather
        } else {
            Step::Quiet
        }
    }

    pub fn failed(&mut self, name: String, reason: String) -> Step {
        self.failed.push((name, reason));
        if self.failed.len() == 1 && self.finished.is_empty() {
            Step::Gather
        } else {
            Step::Quiet
        }
    }

    /// What was gathered, said together.
    pub fn flush(&mut self) -> Vec<Notice> {
        let mut notices = Vec::new();
        match self.finished.as_slice() {
            [] => {}
            [one] => {
                let verified = match one.checked {
                    Some((kind, true)) => text!("Its {kind} checksum matches."),
                    _ => String::new(),
                };
                notices.push(Notice {
                    title: text!("Download complete"),
                    text: [one.name.as_str(), verified.as_str()]
                        .into_iter()
                        .filter(|line| !line.is_empty())
                        .collect::<Vec<_>>()
                        .join("\n"),
                    buttons: vec![
                        (en!("Open"), format!("open:{}", one.id)),
                        (en!("Show in folder"), format!("folder:{}", one.id)),
                    ],
                });
            }
            [first, rest @ ..] => notices.push(Notice {
                title: text!("{count} downloads complete", count = rest.len() + 1),
                text: text!(
                    "{name} and {count} more",
                    name = first.name,
                    count = rest.len()
                ),
                buttons: Vec::new(),
            }),
        }
        match self.failed.as_slice() {
            [] => {}
            [(name, reason)] => notices.push(Notice {
                title: text!("Download failed"),
                text: format!("{name}: {reason}"),
                buttons: Vec::new(),
            }),
            many => notices.push(Notice {
                title: text!("{count} downloads failed", count = many.len()),
                text: text!("Open Tributary to see why."),
                buttons: Vec::new(),
            }),
        }
        self.finished.clear();
        self.failed.clear();
        notices
    }

    /// A queue ended, which tells of the files it finished.
    pub fn queue_finished(&mut self, queue: i64, name: &str) -> Option<Notice> {
        let count = self.queues.remove(&queue).filter(|count| *count > 0)?;
        Some(Notice {
            title: text!("{name} is done"),
            text: if count == 1 {
                text!("1 file saved")
            } else {
                text!("{count} files saved")
            },
            buttons: Vec::new(),
        })
    }
}

/// Whether Tributary is out of sight: hidden in the tray, minimized, or behind another
/// window. While it's in front, its own list and messages say what happened.
fn out_of_sight(app: &AppHandle) -> bool {
    app.get_webview_window(crate::MAIN_WINDOW)
        .is_none_or(|window| {
            !window.is_visible().unwrap_or(false)
                || window.is_minimized().unwrap_or(false)
                || !window.is_focused().unwrap_or(false)
        })
}

fn show(app: &AppHandle, notice: Notice) {
    let wanted = app
        .state::<AppState>()
        .settings_snapshot()
        .notify_in_background;
    if !wanted || std::env::var_os(QUIET_VARIABLE).is_some() || !out_of_sight(app) {
        return;
    }
    // Made in English, as the tests read it, and said in the user's language here.
    let say = crate::messages::describe;
    let mut toast = Toast::new(APP_ID)
        .title(&say(&notice.title))
        .text1(&say(&notice.text));
    for (label, action) in &notice.buttons {
        toast = toast.add_button(&say(label), action);
    }
    let handle = app.clone();
    let toast = toast.on_activated(move |action| {
        act(&handle, action.as_deref());
        Ok(())
    });
    // Without the Start menu shortcut, as when Tributary isn't installed, Windows refuses
    // to show it. That's no reason to bother anyone.
    let _ = toast.show();
}

/// Does what a notification's button asks, or brings Tributary forward.
fn act(app: &AppHandle, action: Option<&str>) {
    let state = app.state::<AppState>();
    let file = |id: &str| {
        let id = DownloadId(id.parse().ok()?);
        state
            .engine
            .get(id)
            .ok()?
            .final_path
            .filter(|path| path.exists())
    };
    match action.and_then(|action| action.split_once(':')) {
        Some(("open", id)) => {
            if let Some(path) = file(id) {
                let _ = app.opener().open_path(path.to_string_lossy(), None::<&str>);
            }
        }
        Some(("folder", id)) => {
            if let Some(path) = file(id) {
                let _ = app.opener().reveal_item_in_dir(path);
            }
        }
        _ => crate::show_main_window(app),
    }
}

fn carry_out(app: &AppHandle, step: Step) {
    match step {
        Step::Now(notice) => show(app, notice),
        Step::Gather => {
            let handle = app.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(GATHER_TIME).await;
                let notices = handle.state::<AppState>().notices().flush();
                for notice in notices {
                    show(&handle, notice);
                }
            });
        }
        Step::Quiet => {}
    }
}

fn display_name(download: &engine::Download) -> String {
    download
        .final_path
        .as_deref()
        .and_then(|path| path.file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .or_else(|| download.file_name.clone())
        .unwrap_or_else(|| download.url.clone())
}

/// A download finished for good: checked, and made into what was asked for.
pub fn finished(app: &AppHandle, id: DownloadId) {
    let state = app.state::<AppState>();
    let Ok(download) = state.engine.get(id) else {
        return;
    };
    let running = |queue: QueueId| {
        state.engine.queues().is_ok_and(|queues| {
            queues
                .iter()
                .any(|info| info.queue.id == queue && info.active)
        })
    };
    let queue = download
        .queue
        .filter(|queue| running(*queue))
        .map(|queue| queue.0);
    let finish = Finish {
        id: id.0,
        name: display_name(&download),
        checked: download
            .checksum
            .as_ref()
            .zip(download.checksum_matches)
            .map(|(checksum, matches)| (checksum.algorithm.label(), matches)),
    };
    let step = state.notices().finished(finish, queue);
    carry_out(app, step);
}

/// A download just finished. Told of now, unless it's still being checked or finished, or
/// it's half of a video, which is told of once that's done.
pub fn completed(app: &AppHandle, id: DownloadId) {
    let state = app.state::<AppState>();
    let busy = state.is_checking(id.0) || state.finishing_label(id.0).is_some();
    let half = state
        .engine
        .get(id)
        .is_ok_and(|download| download.companion.is_some());
    if !busy && !half {
        finished(app, id);
    }
}

pub fn failed(app: &AppHandle, id: DownloadId) {
    let state = app.state::<AppState>();
    let Ok(download) = state.engine.get(id) else {
        return;
    };
    // The notice is said in the user's language, the reason with it.
    let reason = crate::messages::describe(
        download
            .error
            .as_deref()
            .unwrap_or(en!("it couldn't be downloaded")),
    );
    let step = state.notices().failed(display_name(&download), reason);
    carry_out(app, step);
}

pub fn queue_finished(app: &AppHandle, id: QueueId) {
    let state = app.state::<AppState>();
    let name = state
        .engine
        .queues()
        .ok()
        .and_then(|queues| queues.into_iter().find(|info| info.queue.id == id))
        .map(|info| info.queue.name)
        .unwrap_or_else(|| text!("A queue"));
    let notice = state.notices().queue_finished(id.0, &name);
    if let Some(notice) = notice {
        show(app, notice);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finish(id: i64, name: &str) -> Finish {
        Finish {
            id,
            name: name.to_owned(),
            checked: None,
        }
    }

    #[test]
    fn one_download_is_told_of_with_its_buttons() {
        let mut notices = Notices::default();
        assert_eq!(notices.finished(finish(7, "a.zip"), None), Step::Gather);

        assert_eq!(
            notices.flush(),
            [Notice {
                title: "Download complete".to_owned(),
                text: "a.zip".to_owned(),
                buttons: vec![
                    ("Open", "open:7".to_owned()),
                    ("Show in folder", "folder:7".to_owned()),
                ],
            }]
        );
        assert!(notices.flush().is_empty(), "nothing is said twice");
    }

    #[test]
    fn many_at_once_are_one_notification() {
        let mut notices = Notices::default();
        assert_eq!(notices.finished(finish(1, "a.jpg"), None), Step::Gather);
        for id in 2..=40 {
            assert_eq!(notices.finished(finish(id, "more.jpg"), None), Step::Quiet);
        }
        assert_eq!(
            notices.failed("b.zip".into(), "the server said no".into()),
            Step::Quiet
        );

        let said = notices.flush();
        assert_eq!(said.len(), 2);
        assert_eq!(said[0].title, "40 downloads complete");
        assert_eq!(said[0].text, "a.jpg and 39 more");
        assert_eq!(said[1].title, "Download failed");
        assert_eq!(said[1].text, "b.zip: the server said no");
    }

    #[test]
    fn a_queue_tells_of_its_files_when_it_ends() {
        let mut notices = Notices::default();
        for id in 1..=300 {
            assert_eq!(
                notices.finished(finish(id, "photo.jpg"), Some(5)),
                Step::Quiet
            );
        }
        assert!(notices.flush().is_empty());

        let said = notices.queue_finished(5, "Page media").unwrap();
        assert_eq!(said.title, "Page media is done");
        assert_eq!(said.text, "300 files saved");
        // A queue that finished nothing has nothing to say.
        assert_eq!(notices.queue_finished(5, "Page media"), None);
        assert_eq!(notices.queue_finished(9, "Other"), None);
    }

    #[test]
    fn a_checksum_is_mentioned_and_a_mismatch_is_said_at_once() {
        let mut notices = Notices::default();
        let checked = |matches| Finish {
            checked: Some(("SHA-256", matches)),
            ..finish(3, "linux.iso")
        };

        match notices.finished(checked(false), Some(1)) {
            Step::Now(notice) => {
                assert_eq!(notice.title, "Checksum doesn't match");
                assert!(
                    notice
                        .text
                        .contains("linux.iso doesn't match its SHA-256 checksum")
                );
                assert_eq!(notice.buttons, [("Show in folder", "folder:3".to_owned())]);
            }
            other => panic!("{other:?}"),
        }

        notices.finished(checked(true), None);
        assert_eq!(
            notices.flush()[0].text,
            "linux.iso\nIts SHA-256 checksum matches."
        );
    }

    #[test]
    fn many_failures_are_one_notification() {
        let mut notices = Notices::default();
        assert_eq!(notices.failed("a".into(), "x".into()), Step::Gather);
        assert_eq!(notices.failed("b".into(), "y".into()), Step::Quiet);

        let said = notices.flush();
        assert_eq!(said.len(), 1);
        assert_eq!(said[0].title, "2 downloads failed");
    }
}
