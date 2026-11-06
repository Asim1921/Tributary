//! The speed limit and quota shared by every connection.

use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

/// The share of a second's allowance that can be used in one burst.
const BURST_SECONDS: f64 = 0.1;
const MIN_BURST_BYTES: f64 = 16.0 * 1024.0;

/// At most `bytes` per `period`. Once used up, downloads wait for the next period.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Quota {
    pub bytes: u64,
    pub period: Duration,
}

#[derive(Debug)]
struct State {
    rate: Option<u64>,
    /// May go negative, which is what makes later callers wait their turn.
    tokens: f64,
    updated: Instant,
    quota: Option<Quota>,
    period_started: Option<Instant>,
    used: u64,
}

#[derive(Debug)]
pub struct Limits {
    state: Mutex<State>,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            state: Mutex::new(State {
                rate: None,
                tokens: 0.0,
                updated: Instant::now(),
                quota: None,
                period_started: None,
                used: 0,
            }),
        }
    }
}

impl Limits {
    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Sets the total bytes per second across all downloads, or removes the limit.
    pub fn set_rate(&self, rate: Option<u64>) {
        let mut state = self.state();
        state.rate = rate.filter(|&rate| rate > 0);
        state.tokens = 0.0;
        state.updated = Instant::now();
    }

    pub fn set_quota(&self, quota: Option<Quota>) {
        let mut state = self.state();
        if state.quota != quota {
            state.quota = quota;
            state.period_started = None;
            state.used = 0;
        }
    }

    /// Records bytes just received and says how long to wait before reading more.
    pub fn charge(&self, bytes: u64) -> Duration {
        self.charge_at(bytes, Instant::now())
    }

    fn charge_at(&self, bytes: u64, now: Instant) -> Duration {
        let mut state = self.state();
        let rate_wait = rate_wait(&mut state, bytes, now);
        let quota_wait = quota_wait(&mut state, bytes, now);
        rate_wait.max(quota_wait)
    }
}

fn rate_wait(state: &mut State, bytes: u64, now: Instant) -> Duration {
    let Some(rate) = state.rate else {
        return Duration::ZERO;
    };

    let rate = rate as f64;
    let elapsed = now.saturating_duration_since(state.updated).as_secs_f64();
    let burst = (rate * BURST_SECONDS).max(MIN_BURST_BYTES);

    state.tokens = (state.tokens + elapsed * rate).min(burst) - bytes as f64;
    state.updated = now;

    if state.tokens < 0.0 {
        Duration::from_secs_f64(-state.tokens / rate)
    } else {
        Duration::ZERO
    }
}

fn quota_wait(state: &mut State, bytes: u64, now: Instant) -> Duration {
    let Some(quota) = state.quota else {
        return Duration::ZERO;
    };

    let started = *state.period_started.get_or_insert(now);
    if now.saturating_duration_since(started) >= quota.period {
        state.period_started = Some(now);
        state.used = 0;
    }

    state.used += bytes;
    if state.used < quota.bytes {
        return Duration::ZERO;
    }

    let period_ends = state.period_started.unwrap_or(now) + quota.period;
    period_ends.saturating_duration_since(now)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIB: u64 = 1 << 20;

    #[test]
    fn no_limit_means_no_waiting() {
        let limits = Limits::default();
        assert_eq!(limits.charge(10 * MIB), Duration::ZERO);
    }

    #[test]
    fn waiting_matches_the_rate() {
        let limits = Limits::default();
        limits.set_rate(Some(MIB));
        let now = Instant::now();

        let wait = limits.charge_at(MIB, now);

        assert!((wait.as_secs_f64() - 1.0).abs() < 0.05, "waited {wait:?}");
    }

    #[test]
    fn several_connections_share_one_limit() {
        let limits = Limits::default();
        limits.set_rate(Some(MIB));
        let now = Instant::now();

        limits.charge_at(MIB, now);
        let second = limits.charge_at(MIB, now);

        assert!(
            (second.as_secs_f64() - 2.0).abs() < 0.05,
            "waited {second:?}"
        );
    }

    #[test]
    fn time_passing_earns_the_allowance_back() {
        let limits = Limits::default();
        limits.set_rate(Some(MIB));
        let now = Instant::now();

        limits.charge_at(MIB, now);
        let later = limits.charge_at(MIB, now + Duration::from_secs(1));

        assert!((later.as_secs_f64() - 1.0).abs() < 0.05, "waited {later:?}");
    }

    #[test]
    fn idle_time_only_saves_up_a_small_burst() {
        let limits = Limits::default();
        limits.set_rate(Some(MIB));
        let later = Instant::now() + Duration::from_secs(3600);

        let wait = limits.charge_at(MIB, later);

        assert!(
            wait.as_secs_f64() > 0.85,
            "a long idle gave a big burst: {wait:?}"
        );
    }

    #[test]
    fn a_rate_of_zero_means_no_limit() {
        let limits = Limits::default();
        limits.set_rate(Some(0));
        assert_eq!(limits.charge(MIB), Duration::ZERO);
    }

    #[test]
    fn removing_the_limit_stops_the_waiting() {
        let limits = Limits::default();
        limits.set_rate(Some(1024));
        limits.charge(MIB);
        limits.set_rate(None);
        assert_eq!(limits.charge(MIB), Duration::ZERO);
    }

    #[test]
    fn a_used_up_quota_waits_for_the_next_period() {
        let limits = Limits::default();
        let period = Duration::from_secs(3600);
        limits.set_quota(Some(Quota {
            bytes: 10 * MIB,
            period,
        }));
        let now = Instant::now();

        assert_eq!(limits.charge_at(9 * MIB, now), Duration::ZERO);

        let half_way = now + period / 2;
        let wait = limits.charge_at(MIB, half_way);
        assert_eq!(wait, period / 2);
    }

    #[test]
    fn a_new_period_starts_fresh() {
        let limits = Limits::default();
        let period = Duration::from_secs(3600);
        limits.set_quota(Some(Quota { bytes: MIB, period }));
        let now = Instant::now();

        assert!(limits.charge_at(MIB, now) > Duration::ZERO);
        assert_eq!(limits.charge_at(1, now + period), Duration::ZERO);
    }

    #[test]
    fn changing_the_quota_starts_a_new_count() {
        let limits = Limits::default();
        let period = Duration::from_secs(3600);
        limits.set_quota(Some(Quota { bytes: MIB, period }));
        assert!(limits.charge(MIB) > Duration::ZERO);

        limits.set_quota(Some(Quota {
            bytes: 2 * MIB,
            period,
        }));

        assert_eq!(limits.charge(MIB), Duration::ZERO);
    }

    #[test]
    fn the_longer_of_the_two_waits_applies() {
        let limits = Limits::default();
        limits.set_rate(Some(MIB));
        limits.set_quota(Some(Quota {
            bytes: MIB,
            period: Duration::from_secs(600),
        }));

        let wait = limits.charge_at(MIB, Instant::now());

        assert!(wait > Duration::from_secs(500), "waited {wait:?}");
    }
}
