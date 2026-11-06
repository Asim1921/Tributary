//! Decides what each queue does next.
//!
//! Nothing here touches the network, the disk or a clock. The time is passed in and
//! the decisions come back as [`Command`]s, so every rule can be tested exactly.

use std::collections::{HashMap, HashSet};

use chrono::{Datelike, Local, NaiveDateTime, Timelike};

use crate::store::{DownloadId, State};

/// How long a failed file waits before its next try.
pub const RETRY_DELAY_SECONDS: i64 = 30;

const MINUTES_PER_DAY: i64 = 24 * 60;

/// All seven days, one bit each, with Monday as the lowest bit.
pub const EVERY_DAY: u8 = 0b111_1111;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct QueueId(pub i64);

impl QueueId {
    /// The queue that always exists and can't be deleted.
    pub const MAIN: Self = Self(1);
    /// Used in a new [`Queue`] that hasn't been saved yet.
    pub const NEW: Self = Self(0);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueKind {
    /// Finished files leave the queue.
    Download,
    /// Files stay, and are fetched again whenever they change on the server.
    Sync,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Queue {
    pub id: QueueId,
    pub name: String,
    pub kind: QueueKind,
    /// Files downloading at once.
    pub concurrency: u32,
    /// Extra tries for a file before it's left as failed.
    pub retries: u32,
    /// Minutes after midnight.
    pub start_minute: Option<u16>,
    pub stop_minute: Option<u16>,
    /// Days a scheduled start applies. See [`EVERY_DAY`].
    pub days: u8,
    /// For a sync queue, the minutes between checks.
    pub sync_minutes: u32,
    /// What to do when the queue ends. Stored for the app, which is what carries it out.
    pub after_action: String,
}

impl Queue {
    pub fn new(name: &str) -> Self {
        Self {
            id: QueueId::NEW,
            name: name.to_owned(),
            kind: QueueKind::Download,
            concurrency: 2,
            retries: 5,
            start_minute: None,
            stop_minute: None,
            days: EVERY_DAY,
            sync_minutes: 60,
            after_action: String::new(),
        }
    }

    fn runs_on(&self, weekday: u8) -> bool {
        self.days & (1 << weekday) != 0
    }
}

/// A file in a queue, as the scheduler needs to see it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueueFile {
    pub id: DownloadId,
    pub state: State,
}

/// A point in local time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Moment {
    /// For measuring gaps between moments.
    pub seconds: i64,
    pub day: i64,
    /// 0 is Monday.
    pub weekday: u8,
    /// Minutes after midnight.
    pub minute: u16,
}

impl Moment {
    pub fn now() -> Self {
        Self::from_local(Local::now().naive_local())
    }

    pub fn from_local(time: NaiveDateTime) -> Self {
        Self {
            seconds: time.and_utc().timestamp(),
            day: i64::from(time.date().num_days_from_ce()),
            weekday: time.date().weekday().num_days_from_monday() as u8,
            minute: (time.hour() * 60 + time.minute()) as u16,
        }
    }

    fn absolute_minute(self) -> i64 {
        self.day * MINUTES_PER_DAY + i64::from(self.minute)
    }
}

/// Something the engine should do. The scheduler itself does nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Start(DownloadId),
    Pause(DownloadId),
    MoveToEnd(DownloadId),
    LeaveQueue(DownloadId),
    CheckForUpdate(DownloadId),
    /// The queue ended on its own, so its after-queue action should run.
    Finished(QueueId),
}

#[derive(Debug, Default)]
struct Run {
    active: bool,
    /// An empty queue must never trigger an action such as shutting down.
    started_any: bool,
    /// For a sync queue, when the next check is due.
    next_sync: Option<i64>,
    sync_day: Option<i64>,
    /// A file that was alone when the queue started retries without limit, which suits
    /// waiting out a busy server. A file merely left over after the others finished
    /// keeps its limit, or one broken link would stop the queue from ever ending.
    started_alone: Option<bool>,
}

#[derive(Debug, Default, Clone, Copy)]
struct Retry {
    failures: u32,
    counted: bool,
    not_before: i64,
}

#[derive(Debug, Default)]
pub struct Scheduler {
    runs: HashMap<QueueId, Run>,
    retries: HashMap<DownloadId, Retry>,
    /// Files stopped by hand. A queue leaves them alone until they're resumed.
    held: HashSet<DownloadId>,
    last_minute: Option<i64>,
}

impl Scheduler {
    pub fn is_active(&self, queue: QueueId) -> bool {
        self.runs.get(&queue).is_some_and(|run| run.active)
    }

    pub fn hold(&mut self, id: DownloadId) {
        self.held.insert(id);
    }

    pub fn release(&mut self, id: DownloadId) {
        self.held.remove(&id);
    }

    /// Starts a queue by hand. Its files get a fresh set of tries.
    pub fn start_now(&mut self, queue: &Queue, files: &[QueueFile], now: Moment) {
        for file in files {
            self.retries.remove(&file.id);
            self.held.remove(&file.id);
        }
        self.activate(queue, now);
    }

    /// Stops a queue by hand. Its after-queue action doesn't run.
    pub fn stop_now(&mut self, queue: QueueId) {
        self.runs.remove(&queue);
    }

    fn activate(&mut self, queue: &Queue, now: Moment) {
        let sync = queue.kind == QueueKind::Sync;
        self.runs.insert(
            queue.id,
            Run {
                active: true,
                started_any: false,
                next_sync: sync.then_some(now.seconds),
                sync_day: sync.then_some(now.day),
                started_alone: None,
            },
        );
    }

    /// Works out what should happen at this moment.
    pub fn tick(&mut self, now: Moment, queues: &[(Queue, Vec<QueueFile>)]) -> Vec<Command> {
        let previous = self.last_minute.replace(now.absolute_minute());
        let mut commands = Vec::new();

        for (queue, files) in queues {
            self.apply_schedule(queue, files, now, previous, &mut commands);
            if self.is_active(queue.id) {
                self.advance(queue, files, now, &mut commands);
            }
        }
        commands
    }

    fn apply_schedule(
        &mut self,
        queue: &Queue,
        files: &[QueueFile],
        now: Moment,
        previous: Option<i64>,
        commands: &mut Vec<Command>,
    ) {
        let reached = |minute: u16| crossed(previous, now, minute);

        if self.is_active(queue.id) {
            let stop_reached = queue.stop_minute.is_some_and(reached);
            let day_over = queue.kind == QueueKind::Sync
                && queue.stop_minute.is_none()
                && self
                    .runs
                    .get(&queue.id)
                    .and_then(|run| run.sync_day)
                    .is_some_and(|day| day != now.day);

            if stop_reached || day_over {
                self.end(queue, files, commands);
            }
            return;
        }

        let Some(start) = queue.start_minute.filter(|_| queue.runs_on(now.weekday)) else {
            return;
        };
        // Opening the app part way through a window still runs that day's schedule.
        let opened_in_window =
            previous.is_none() && inside_window(now.minute, start, queue.stop_minute);
        if reached(start) || opened_in_window {
            self.activate(queue, now);
        }
    }

    /// Ends a run on its own, as opposed to being stopped by hand.
    fn end(&mut self, queue: &Queue, files: &[QueueFile], commands: &mut Vec<Command>) {
        commands.extend(
            files
                .iter()
                .filter(|file| file.state == State::Running)
                .map(|file| Command::Pause(file.id)),
        );

        let started_any = self
            .runs
            .remove(&queue.id)
            .is_some_and(|run| run.started_any);
        if started_any {
            commands.push(Command::Finished(queue.id));
        }
    }

    fn advance(
        &mut self,
        queue: &Queue,
        files: &[QueueFile],
        now: Moment,
        commands: &mut Vec<Command>,
    ) {
        if queue.kind == QueueKind::Sync {
            let Some(run) = self.runs.get_mut(&queue.id) else {
                return;
            };
            match run.next_sync {
                Some(due) if now.seconds < due => return,
                Some(_) => {
                    run.next_sync = None;
                    run.started_any = true;
                    commands.extend(
                        files
                            .iter()
                            .filter(|file| file.state == State::Completed)
                            .map(|file| Command::CheckForUpdate(file.id)),
                    );
                    // The checks run next, and any changed files are picked up after them.
                    return;
                }
                None => {}
            }
        }

        self.count_failures(files, now, commands);
        if queue.kind == QueueKind::Download {
            for file in files.iter().filter(|file| file.state == State::Completed) {
                self.retries.remove(&file.id);
                commands.push(Command::LeaveQueue(file.id));
            }
        }

        let unfinished = files
            .iter()
            .filter(|file| file.state != State::Completed && !self.held.contains(&file.id))
            .count();
        let lone = self
            .runs
            .get_mut(&queue.id)
            .is_some_and(|run| *run.started_alone.get_or_insert(unfinished == 1));

        let running = files
            .iter()
            .filter(|file| file.state == State::Running)
            .count();
        let free = (queue.concurrency.max(1) as usize).saturating_sub(running);

        let startable: Vec<DownloadId> = files
            .iter()
            .filter(|file| self.can_start(file, queue, now, lone))
            .map(|file| file.id)
            .take(free)
            .collect();

        for id in &startable {
            self.retries.entry(*id).or_default().counted = false;
            commands.push(Command::Start(*id));
        }
        if !startable.is_empty()
            && let Some(run) = self.runs.get_mut(&queue.id)
        {
            run.started_any = true;
        }

        let waiting = files
            .iter()
            .any(|file| self.waiting_to_retry(file, queue, now, lone));
        if running > 0 || !startable.is_empty() || waiting {
            return;
        }

        match queue.kind {
            QueueKind::Download => {
                let started_any = self
                    .runs
                    .remove(&queue.id)
                    .is_some_and(|run| run.started_any);
                if started_any {
                    commands.push(Command::Finished(queue.id));
                }
            }
            QueueKind::Sync => {
                if let Some(run) = self.runs.get_mut(&queue.id) {
                    run.next_sync = Some(now.seconds + i64::from(queue.sync_minutes.max(1)) * 60);
                }
            }
        }
    }

    /// Notes each new failure once, and sends the file to the back of the queue.
    fn count_failures(&mut self, files: &[QueueFile], now: Moment, commands: &mut Vec<Command>) {
        for file in files.iter().filter(|file| file.state == State::Failed) {
            let retry = self.retries.entry(file.id).or_default();
            if !retry.counted {
                retry.counted = true;
                retry.failures += 1;
                retry.not_before = now.seconds + RETRY_DELAY_SECONDS;
                commands.push(Command::MoveToEnd(file.id));
            }
        }
    }

    fn tries_left(&self, id: DownloadId, queue: &Queue, lone: bool) -> bool {
        lone || self
            .retries
            .get(&id)
            .is_none_or(|retry| retry.failures <= queue.retries)
    }

    fn can_start(&self, file: &QueueFile, queue: &Queue, now: Moment, lone: bool) -> bool {
        if self.held.contains(&file.id) {
            return false;
        }
        match file.state {
            State::Paused => true,
            State::Failed => {
                self.tries_left(file.id, queue, lone)
                    && self
                        .retries
                        .get(&file.id)
                        .is_none_or(|retry| retry.not_before <= now.seconds)
            }
            State::Running | State::Completed => false,
        }
    }

    fn waiting_to_retry(&self, file: &QueueFile, queue: &Queue, now: Moment, lone: bool) -> bool {
        file.state == State::Failed
            && !self.held.contains(&file.id)
            && self.tries_left(file.id, queue, lone)
            && self
                .retries
                .get(&file.id)
                .is_some_and(|retry| retry.not_before > now.seconds)
    }
}

/// Whether a time of day was passed since the previous tick. Checking by range rather
/// than for an exact match means a computer that was asleep still catches it.
fn crossed(previous: Option<i64>, now: Moment, minute: u16) -> bool {
    let Some(previous) = previous else {
        return false;
    };
    [now.day, now.day - 1].iter().any(|day| {
        let target = day * MINUTES_PER_DAY + i64::from(minute);
        previous < target && target <= now.absolute_minute()
    })
}

/// Whether `minute` is between start and stop, allowing for a window across midnight.
fn inside_window(minute: u16, start: u16, stop: Option<u16>) -> bool {
    match stop {
        Some(stop) if start <= stop => (start..stop).contains(&minute),
        Some(stop) => minute >= start || minute < stop,
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(day: i64, hour: i64, minute: i64) -> Moment {
        Moment {
            seconds: day * 86_400 + hour * 3600 + minute * 60,
            day,
            weekday: (day % 7) as u8,
            minute: (hour * 60 + minute) as u16,
        }
    }

    fn later(moment: Moment, seconds: i64) -> Moment {
        Moment {
            seconds: moment.seconds + seconds,
            ..moment
        }
    }

    fn file(id: i64, state: State) -> QueueFile {
        QueueFile {
            id: DownloadId(id),
            state,
        }
    }

    fn queue() -> Queue {
        Queue {
            id: QueueId(1),
            ..Queue::new("test")
        }
    }

    fn started(commands: &[Command]) -> Vec<i64> {
        commands
            .iter()
            .filter_map(|command| match command {
                Command::Start(id) => Some(id.0),
                _ => None,
            })
            .collect()
    }

    fn finished(commands: &[Command]) -> bool {
        commands
            .iter()
            .any(|command| matches!(command, Command::Finished(_)))
    }

    /// A scheduler that has run once, so the next tick can detect crossed times.
    fn warmed_up(moment: Moment) -> Scheduler {
        let mut scheduler = Scheduler::default();
        scheduler.tick(moment, &[]);
        scheduler
    }

    #[test]
    fn a_queue_does_nothing_until_started() {
        let mut scheduler = Scheduler::default();
        let queues = [(queue(), vec![file(1, State::Paused)])];

        assert!(scheduler.tick(at(1, 9, 0), &queues).is_empty());
    }

    #[test]
    fn starts_up_to_its_concurrency_in_order() {
        let mut scheduler = Scheduler::default();
        let files = vec![
            file(3, State::Paused),
            file(1, State::Paused),
            file(2, State::Paused),
        ];
        let now = at(1, 9, 0);
        scheduler.start_now(&queue(), &files, now);

        let commands = scheduler.tick(now, &[(queue(), files)]);

        assert_eq!(started(&commands), [3, 1]);
    }

    #[test]
    fn running_files_use_up_the_concurrency() {
        let mut scheduler = Scheduler::default();
        let files = vec![
            file(1, State::Running),
            file(2, State::Running),
            file(3, State::Paused),
        ];
        let now = at(1, 9, 0);
        scheduler.start_now(&queue(), &files, now);

        assert!(started(&scheduler.tick(now, &[(queue(), files)])).is_empty());
    }

    #[test]
    fn a_free_slot_goes_to_the_next_file() {
        let mut scheduler = Scheduler::default();
        let files = vec![
            file(1, State::Completed),
            file(2, State::Running),
            file(3, State::Paused),
        ];
        let now = at(1, 9, 0);
        scheduler.start_now(&queue(), &files, now);

        assert_eq!(started(&scheduler.tick(now, &[(queue(), files)])), [3]);
    }

    #[test]
    fn finished_files_leave_a_download_queue() {
        let mut scheduler = Scheduler::default();
        let files = vec![file(1, State::Completed), file(2, State::Running)];
        let now = at(1, 9, 0);
        scheduler.start_now(&queue(), &files, now);

        let commands = scheduler.tick(now, &[(queue(), files)]);

        assert!(commands.contains(&Command::LeaveQueue(DownloadId(1))));
    }

    #[test]
    fn ends_and_reports_when_nothing_is_left() {
        let mut scheduler = Scheduler::default();
        let now = at(1, 9, 0);
        let files = vec![file(1, State::Paused)];
        scheduler.start_now(&queue(), &files, now);
        scheduler.tick(now, &[(queue(), files)]);

        let commands = scheduler.tick(now, &[(queue(), vec![file(1, State::Completed)])]);

        assert!(finished(&commands));
        assert!(!scheduler.is_active(QueueId(1)));
    }

    #[test]
    fn an_empty_queue_never_reports_finished() {
        let mut scheduler = Scheduler::default();
        let now = at(1, 9, 0);
        scheduler.start_now(&queue(), &[], now);

        let commands = scheduler.tick(now, &[(queue(), Vec::new())]);

        assert!(!finished(&commands), "this could shut the computer down");
        assert!(!scheduler.is_active(QueueId(1)));
    }

    #[test]
    fn a_failed_file_goes_to_the_back_and_waits() {
        let mut scheduler = Scheduler::default();
        let files = vec![file(1, State::Failed), file(2, State::Paused)];
        let now = at(1, 9, 0);
        scheduler.start_now(&queue(), &files, now);

        let commands = scheduler.tick(now, &[(queue(), files)]);

        assert!(commands.contains(&Command::MoveToEnd(DownloadId(1))));
        assert_eq!(started(&commands), [2], "the failed file must wait");
    }

    #[test]
    fn a_failed_file_is_retried_after_the_delay() {
        let mut scheduler = Scheduler::default();
        let files = vec![file(1, State::Failed), file(2, State::Running)];
        let now = at(1, 9, 0);
        scheduler.start_now(&queue(), &files, now);
        scheduler.tick(now, &[(queue(), files.clone())]);

        let too_soon = scheduler.tick(later(now, 10), &[(queue(), files.clone())]);
        assert!(started(&too_soon).is_empty());

        let on_time = scheduler.tick(later(now, RETRY_DELAY_SECONDS), &[(queue(), files)]);
        assert_eq!(started(&on_time), [1]);
    }

    #[test]
    fn a_failure_is_counted_once_however_many_ticks_see_it() {
        let mut scheduler = Scheduler::default();
        let files = vec![file(1, State::Failed), file(2, State::Running)];
        let now = at(1, 9, 0);
        scheduler.start_now(&queue(), &files, now);

        let first = scheduler.tick(now, &[(queue(), files.clone())]);
        let second = scheduler.tick(later(now, 1), &[(queue(), files)]);

        assert!(first.contains(&Command::MoveToEnd(DownloadId(1))));
        assert!(!second.contains(&Command::MoveToEnd(DownloadId(1))));
    }

    /// Fails a file again and again, the way a broken link would.
    fn fail_repeatedly(scheduler: &mut Scheduler, queue: &Queue, others: &[QueueFile], times: u32) {
        let mut now = at(1, 9, 0);
        for _ in 0..times {
            let mut files = vec![file(1, State::Failed)];
            files.extend_from_slice(others);
            scheduler.tick(now, &[(queue.clone(), files.clone())]);
            now = later(now, RETRY_DELAY_SECONDS);
            scheduler.tick(now, &[(queue.clone(), files)]);
            now = later(now, 1);
        }
    }

    #[test]
    fn gives_up_on_a_file_after_its_retries() {
        let mut scheduler = Scheduler::default();
        let mut limited = queue();
        limited.retries = 2;
        let other = [file(2, State::Running)];
        scheduler.start_now(&limited, &[], at(1, 9, 0));

        fail_repeatedly(&mut scheduler, &limited, &other, 3);

        let mut files = vec![file(1, State::Failed)];
        files.extend_from_slice(&other);
        let queues = [(limited, files)];

        // The second tick is past the retry delay, so only the limit can stop it.
        scheduler.tick(at(5, 9, 0), &queues);
        let commands = scheduler.tick(later(at(5, 9, 0), RETRY_DELAY_SECONDS), &queues);
        assert!(started(&commands).is_empty());
    }

    #[test]
    fn a_queue_with_only_given_up_files_ends() {
        let mut scheduler = Scheduler::default();
        let mut limited = queue();
        limited.retries = 0;
        let now = at(1, 9, 0);
        let files = vec![file(1, State::Failed), file(2, State::Failed)];
        scheduler.start_now(&limited, &files, now);
        scheduler.tick(now, &[(limited.clone(), files.clone())]);

        // Each has failed once and has no retries, so nothing is left to do.
        scheduler.tick(later(now, 60), &[(limited.clone(), files)]);

        assert!(!scheduler.is_active(limited.id));
    }

    #[test]
    fn a_lone_file_keeps_retrying_however_often_it_fails() {
        let mut scheduler = Scheduler::default();
        let mut limited = queue();
        limited.retries = 1;
        scheduler.start_now(&limited, &[], at(1, 9, 0));

        fail_repeatedly(&mut scheduler, &limited, &[], 10);

        let queues = [(limited.clone(), vec![file(1, State::Failed)])];
        scheduler.tick(at(5, 9, 0), &queues);
        let commands = scheduler.tick(later(at(5, 9, 0), RETRY_DELAY_SECONDS), &queues);

        assert_eq!(started(&commands), [1]);
        assert!(scheduler.is_active(limited.id));
    }

    #[test]
    fn a_file_left_over_after_the_others_finish_still_gives_up() {
        let mut scheduler = Scheduler::default();
        let mut limited = queue();
        limited.retries = 1;
        let now = at(1, 9, 0);
        let both = vec![file(1, State::Failed), file(2, State::Running)];
        scheduler.start_now(&limited, &both, now);
        scheduler.tick(now, &[(limited.clone(), both)]);

        // The other file has finished and gone, so this one is now alone in the queue.
        let alone = vec![file(1, State::Failed)];
        let mut time = now;
        for _ in 0..5 {
            time = later(time, RETRY_DELAY_SECONDS + 1);
            scheduler.tick(time, &[(limited.clone(), alone.clone())]);
        }

        assert!(
            !scheduler.is_active(limited.id),
            "the queue must be able to end"
        );
    }

    #[test]
    fn starting_by_hand_gives_files_fresh_tries() {
        let mut scheduler = Scheduler::default();
        let mut limited = queue();
        limited.retries = 0;
        let files = vec![file(1, State::Failed), file(2, State::Running)];
        let now = at(1, 9, 0);
        scheduler.start_now(&limited, &files, now);
        scheduler.tick(now, &[(limited.clone(), files.clone())]);

        scheduler.start_now(&limited, &files, later(now, 60));

        let commands = scheduler.tick(later(now, 120), &[(limited, files)]);
        assert!(commands.contains(&Command::MoveToEnd(DownloadId(1))));
    }

    #[test]
    fn a_file_stopped_by_hand_is_left_alone() {
        let mut scheduler = Scheduler::default();
        let files = vec![file(1, State::Paused), file(2, State::Paused)];
        let now = at(1, 9, 0);
        scheduler.start_now(&queue(), &files, now);
        scheduler.hold(DownloadId(1));

        assert_eq!(started(&scheduler.tick(now, &[(queue(), files)])), [2]);
    }

    #[test]
    fn a_resumed_file_rejoins_the_queue() {
        let mut scheduler = Scheduler::default();
        let files = vec![file(1, State::Paused), file(2, State::Running)];
        let now = at(1, 9, 0);
        scheduler.start_now(&queue(), &files, now);
        scheduler.hold(DownloadId(1));
        scheduler.tick(now, &[(queue(), files.clone())]);

        scheduler.release(DownloadId(1));

        assert_eq!(started(&scheduler.tick(now, &[(queue(), files)])), [1]);
    }

    #[test]
    fn a_held_file_does_not_keep_the_queue_open() {
        let mut scheduler = Scheduler::default();
        let files = vec![file(1, State::Paused)];
        let now = at(1, 9, 0);
        scheduler.start_now(&queue(), &files, now);
        scheduler.hold(DownloadId(1));

        scheduler.tick(now, &[(queue(), files)]);

        assert!(!scheduler.is_active(QueueId(1)));
    }

    #[test]
    fn stopping_by_hand_skips_the_finished_report() {
        let mut scheduler = Scheduler::default();
        let files = vec![file(1, State::Running)];
        let now = at(1, 9, 0);
        scheduler.start_now(&queue(), &files, now);
        scheduler.tick(now, &[(queue(), files.clone())]);

        scheduler.stop_now(QueueId(1));

        let commands = scheduler.tick(later(now, 1), &[(queue(), files)]);
        assert!(!finished(&commands));
        assert!(!scheduler.is_active(QueueId(1)));
    }

    fn scheduled(start: u16, stop: Option<u16>) -> Queue {
        Queue {
            start_minute: Some(start),
            stop_minute: stop,
            ..queue()
        }
    }

    #[test]
    fn starts_when_its_time_arrives() {
        let mut scheduler = warmed_up(at(1, 8, 59));
        let queues = [(scheduled(9 * 60, None), vec![file(1, State::Paused)])];

        assert_eq!(started(&scheduler.tick(at(1, 9, 0), &queues)), [1]);
    }

    #[test]
    fn does_not_start_early() {
        let mut scheduler = warmed_up(at(1, 8, 58));
        let queues = [(scheduled(9 * 60, None), vec![file(1, State::Paused)])];

        assert!(scheduler.tick(at(1, 8, 59), &queues).is_empty());
    }

    #[test]
    fn a_computer_that_slept_through_the_time_still_starts() {
        let mut scheduler = warmed_up(at(1, 8, 0));
        let queues = [(scheduled(9 * 60, None), vec![file(1, State::Paused)])];

        assert_eq!(started(&scheduler.tick(at(1, 11, 30), &queues)), [1]);
    }

    #[test]
    fn starting_only_happens_once_per_day() {
        let mut scheduler = warmed_up(at(1, 8, 59));
        let plan = scheduled(9 * 60, None);

        scheduler.tick(at(1, 9, 0), &[(plan.clone(), vec![file(1, State::Paused)])]);
        scheduler.tick(
            at(1, 9, 1),
            &[(plan.clone(), vec![file(1, State::Completed)])],
        );

        let commands = scheduler.tick(at(1, 9, 2), &[(plan, vec![file(2, State::Paused)])]);
        assert!(commands.is_empty());
    }

    #[test]
    fn runs_again_the_next_day() {
        let mut scheduler = warmed_up(at(1, 23, 59));
        let plan = scheduled(9 * 60, None);

        scheduler.tick(
            at(2, 8, 59),
            &[(plan.clone(), vec![file(1, State::Paused)])],
        );
        let commands = scheduler.tick(at(2, 9, 0), &[(plan, vec![file(1, State::Paused)])]);

        assert_eq!(started(&commands), [1]);
    }

    #[test]
    fn skips_days_it_is_not_set_for() {
        let mut scheduler = warmed_up(at(1, 8, 59));
        let plan = Queue {
            days: 0,
            ..scheduled(9 * 60, None)
        };

        let commands = scheduler.tick(at(1, 9, 0), &[(plan, vec![file(1, State::Paused)])]);

        assert!(commands.is_empty());
    }

    #[test]
    fn opening_the_app_inside_the_window_starts_the_queue() {
        let mut scheduler = Scheduler::default();
        let queues = [(
            scheduled(8 * 60, Some(18 * 60)),
            vec![file(1, State::Paused)],
        )];

        assert_eq!(started(&scheduler.tick(at(1, 12, 0), &queues)), [1]);
    }

    #[test]
    fn opening_the_app_outside_the_window_does_not() {
        let mut scheduler = Scheduler::default();
        let queues = [(
            scheduled(8 * 60, Some(18 * 60)),
            vec![file(1, State::Paused)],
        )];

        assert!(scheduler.tick(at(1, 19, 0), &queues).is_empty());
    }

    #[test]
    fn opening_the_app_after_a_start_time_with_no_stop_time_waits_for_tomorrow() {
        let mut scheduler = Scheduler::default();
        let queues = [(scheduled(8 * 60, None), vec![file(1, State::Paused)])];

        assert!(scheduler.tick(at(1, 12, 0), &queues).is_empty());
    }

    #[test]
    fn the_stop_time_pauses_running_files_and_ends_the_queue() {
        let mut scheduler = warmed_up(at(1, 8, 59));
        let plan = scheduled(9 * 60, Some(10 * 60));

        scheduler.tick(at(1, 9, 0), &[(plan.clone(), vec![file(1, State::Paused)])]);
        let commands = scheduler.tick(
            at(1, 10, 0),
            &[(plan.clone(), vec![file(1, State::Running)])],
        );

        assert!(commands.contains(&Command::Pause(DownloadId(1))));
        assert!(finished(&commands));
        assert!(!scheduler.is_active(plan.id));
    }

    #[test]
    fn a_stop_time_applies_to_a_queue_started_by_hand() {
        let mut scheduler = warmed_up(at(1, 9, 59));
        let plan = Queue {
            stop_minute: Some(10 * 60),
            ..queue()
        };
        let files = vec![file(1, State::Running)];
        scheduler.start_now(&plan, &files, at(1, 9, 59));

        let commands = scheduler.tick(at(1, 10, 0), &[(plan, files)]);

        assert!(commands.contains(&Command::Pause(DownloadId(1))));
    }

    #[test]
    fn a_queue_started_after_its_stop_time_runs_until_the_next_one() {
        let mut scheduler = warmed_up(at(1, 11, 0));
        let plan = Queue {
            stop_minute: Some(10 * 60),
            ..queue()
        };
        let files = vec![file(1, State::Running)];
        scheduler.start_now(&plan, &files, at(1, 11, 0));

        let commands = scheduler.tick(at(1, 11, 1), &[(plan.clone(), files)]);

        assert!(!commands.contains(&Command::Pause(DownloadId(1))));
        assert!(scheduler.is_active(plan.id));
    }

    #[test]
    fn a_window_across_midnight_stops_the_next_morning() {
        let mut scheduler = warmed_up(at(1, 22, 59));
        let plan = scheduled(23 * 60, Some(6 * 60));

        let begun = scheduler.tick(
            at(1, 23, 0),
            &[(plan.clone(), vec![file(1, State::Paused)])],
        );
        assert_eq!(started(&begun), [1]);

        let running = vec![file(1, State::Running)];
        let night = scheduler.tick(at(2, 3, 0), &[(plan.clone(), running.clone())]);
        assert!(!night.contains(&Command::Pause(DownloadId(1))));

        let morning = scheduler.tick(at(2, 6, 0), &[(plan, running)]);
        assert!(morning.contains(&Command::Pause(DownloadId(1))));
    }

    fn sync_queue() -> Queue {
        Queue {
            kind: QueueKind::Sync,
            sync_minutes: 30,
            ..queue()
        }
    }

    #[test]
    fn a_sync_queue_checks_its_finished_files() {
        let mut scheduler = Scheduler::default();
        let files = vec![file(1, State::Completed)];
        let now = at(1, 9, 0);
        scheduler.start_now(&sync_queue(), &files, now);

        let commands = scheduler.tick(now, &[(sync_queue(), files)]);

        assert_eq!(commands, [Command::CheckForUpdate(DownloadId(1))]);
    }

    #[test]
    fn a_sync_queue_keeps_its_files() {
        let mut scheduler = Scheduler::default();
        let files = vec![file(1, State::Completed)];
        let now = at(1, 9, 0);
        scheduler.start_now(&sync_queue(), &files, now);
        scheduler.tick(now, &[(sync_queue(), files.clone())]);

        let commands = scheduler.tick(later(now, 1), &[(sync_queue(), files)]);

        assert!(!commands.contains(&Command::LeaveQueue(DownloadId(1))));
    }

    #[test]
    fn a_sync_queue_downloads_files_that_changed() {
        let mut scheduler = Scheduler::default();
        let now = at(1, 9, 0);
        scheduler.start_now(&sync_queue(), &[], now);
        scheduler.tick(now, &[(sync_queue(), vec![file(1, State::Completed)])]);

        // The check found a change, so the engine marked the file as ready to fetch.
        let commands = scheduler.tick(
            later(now, 1),
            &[(sync_queue(), vec![file(1, State::Paused)])],
        );

        assert_eq!(started(&commands), [1]);
    }

    #[test]
    fn a_sync_queue_checks_again_after_its_interval() {
        let mut scheduler = Scheduler::default();
        let files = vec![file(1, State::Completed)];
        let now = at(1, 9, 0);
        scheduler.start_now(&sync_queue(), &files, now);
        scheduler.tick(now, &[(sync_queue(), files.clone())]);
        scheduler.tick(later(now, 1), &[(sync_queue(), files.clone())]);

        assert!(
            scheduler
                .tick(later(now, 60), &[(sync_queue(), files.clone())])
                .is_empty()
        );
        assert!(scheduler.is_active(QueueId(1)), "it should keep waiting");

        let due = scheduler.tick(later(now, 1 + 30 * 60), &[(sync_queue(), files)]);
        assert_eq!(due, [Command::CheckForUpdate(DownloadId(1))]);
    }

    #[test]
    fn a_sync_queue_with_no_stop_time_ends_with_the_day() {
        let mut scheduler = warmed_up(at(1, 8, 59));
        let files = vec![file(1, State::Completed)];
        scheduler.start_now(&sync_queue(), &files, at(1, 9, 0));
        scheduler.tick(at(1, 9, 0), &[(sync_queue(), files.clone())]);

        let commands = scheduler.tick(at(2, 0, 1), &[(sync_queue(), files)]);

        assert!(finished(&commands));
        assert!(!scheduler.is_active(QueueId(1)));
    }

    #[test]
    fn moments_read_local_time_correctly() {
        let time =
            NaiveDateTime::parse_from_str("2026-11-09 14:05:30", "%Y-%m-%d %H:%M:%S").unwrap();
        let moment = Moment::from_local(time);

        assert_eq!(moment.weekday, 0, "9 November 2026 is a Monday");
        assert_eq!(moment.minute, 14 * 60 + 5);
    }
}
