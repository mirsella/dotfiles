//! Pure gating and cache-attribution logic, kept free of I/O so it is cheap
//! to test from a fixed clock.

use jiff::{Span, Timestamp, tz::TimeZone};

use crate::config::{Config, parse_hhmm};
use crate::state::{State, ThreadState};

/// One thread as reported by `t3_thread_list`.
#[derive(Debug, Clone)]
pub struct ThreadView {
    pub id: String,
    pub title: String,
    pub settled: bool,
    pub snoozed: bool,
    /// Run status; `idle` means nothing is running.
    pub status: String,
    pub provider: String,
    pub model: String,
    pub updated_at: Timestamp,
    pub subagent: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Warm,
    Skip { reason: SkipReason },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    Settled,
    Snoozed,
    Subagent,
    Running,
    NoRule,
    OutsideHours,
    IdleTooLong,
    BudgetSpent,
    CacheFresh,
}

/// Seeds the state of a thread we have never seen. Until the first refresh
/// cycle the current activity is treated as the source of a fresh cache.
pub fn seed(view: &ThreadView) -> ThreadState {
    ThreadState {
        title: view.title.clone(),
        provider: view.provider.clone(),
        model: view.model.clone(),
        last_activity_at: Some(view.updated_at),
        last_cache_at: Some(view.updated_at),
        seen_updated_at: Some(view.updated_at),
        ..ThreadState::default()
    }
}

/// Decides whether one thread should be warmed now.
pub fn decide(config: &Config, view: &ThreadView, state: &ThreadState, now: Timestamp) -> Decision {
    let skip = |reason| Decision::Skip { reason };
    if view.settled {
        return skip(SkipReason::Settled);
    }
    if view.snoozed {
        return skip(SkipReason::Snoozed);
    }
    if view.subagent && config.skip_subagents {
        return skip(SkipReason::Subagent);
    }
    if !is_quiescent(&view.status) {
        return skip(SkipReason::Running);
    }
    if !within_day_window(config, now) {
        return skip(SkipReason::OutsideHours);
    }
    let (ttl, last_cache) = match stored_gate(config, &view.provider, &view.model, state, now) {
        Ok(gate) => gate,
        Err(reason) => return skip(reason),
    };
    if now < refresh_due_after(last_cache, ttl, config.lead_seconds) {
        return skip(SkipReason::CacheFresh);
    }
    Decision::Warm
}

/// Eligibility from stored beliefs alone: the rule TTL and cache base when a
/// pass could warm this thread once quiescent and in-hours, or the reason it
/// would skip. Shared by [`decide`] and [`next_wakeup`] so the scheduler
/// never wakes for threads no pass would warm.
fn stored_gate(
    config: &Config,
    provider: &str,
    model: &str,
    state: &ThreadState,
    now: Timestamp,
) -> Result<(u64, Timestamp), SkipReason> {
    let Some(ttl) = rule_ttl(config, provider, model) else {
        return Err(SkipReason::NoRule);
    };
    let Some(last_activity) = state.last_activity_at else {
        return Err(SkipReason::IdleTooLong);
    };
    if now.as_second() - last_activity.as_second() > config.active_window_minutes as i64 * 60 {
        return Err(SkipReason::IdleTooLong);
    }
    if state.pending_warm_at.is_some() {
        // A keepalive is still in flight; let it land before deciding again.
        return Err(SkipReason::CacheFresh);
    }
    if state.refreshes >= config.max_refreshes_per_activity {
        return Err(SkipReason::BudgetSpent);
    }
    Ok((ttl, state.last_cache_at.unwrap_or(last_activity)))
}

/// The moment a cache written at `last_cache` should be refreshed: its TTL
/// minus the configured lead time. This is the single canonical definition
/// of "due", shared by [`decide`] and [`next_wakeup`].
pub fn refresh_due_after(
    last_cache: Timestamp,
    ttl_seconds: u64,
    lead_seconds: u64,
) -> Timestamp {
    let threshold = (ttl_seconds as i64 - lead_seconds as i64).max(0);
    Timestamp::from_second(last_cache.as_second() + threshold)
        .expect("cache due timestamp in range")
}

/// Statuses in which a thread is doing nothing and may receive a keepalive.
/// `completed` is T3's state for a thread whose last run finished; anything
/// else (`running`, `preparing`, `failed`, …) means hands off.
pub fn is_quiescent(status: &str) -> bool {
    matches!(status, "idle" | "completed")
}

/// The TTL of the first matching rule, or `None` when nothing matches.
pub fn rule_ttl(config: &Config, provider: &str, model: &str) -> Option<u64> {
    config
        .rules
        .iter()
        .find(|rule| {
            let provider_matches = rule.provider.as_deref().is_none_or(|want| want == provider);
            provider_matches && model.contains(&rule.model_contains)
        })
        .map(|rule| rule.ttl_seconds)
}

/// Whether the local wall clock is inside the configured window.
pub fn within_day_window(config: &Config, now: Timestamp) -> bool {
    let (Ok(start), Ok(end)) = (
        parse_hhmm(&config.day_window.start, "start"),
        parse_hhmm(&config.day_window.end, "end"),
    ) else {
        return false;
    };
    let zoned = now.to_zoned(TimeZone::system());
    let minutes = i16::from(zoned.hour()) * 60 + i16::from(zoned.minute());
    within_window(minutes, start as i16, end as i16)
}

fn within_window(minutes: i16, start: i16, end: i16) -> bool {
    if start <= end {
        minutes >= start && minutes < end
    } else {
        minutes >= start || minutes < end
    }
}

/// The next moment `run` should wake: the earliest refresh due among threads
/// a pass could warm, or the day window reopening when outside it. `None`
/// means nothing is scheduled and the caller should fall back to its cap.
pub fn next_wakeup(state: &State, config: &Config, now: Timestamp) -> Option<Timestamp> {
    if !within_day_window(config, now) {
        return next_window_start(config, now);
    }
    state
        .threads
        .values()
        .filter_map(|thread| {
            if thread.settled || thread.snoozed {
                return None;
            }
            if config.skip_subagents && thread.subagent {
                return None;
            }
            let (ttl, last_cache) =
                stored_gate(config, &thread.provider, &thread.model, thread, now).ok()?;
            Some(refresh_due_after(last_cache, ttl, config.lead_seconds))
        })
        .min()
}

/// Next local occurrence of the day window's start after `now`.
fn next_window_start(config: &Config, now: Timestamp) -> Option<Timestamp> {
    let start = parse_hhmm(&config.day_window.start, "start").ok()?;
    let zoned = now.to_zoned(TimeZone::system());
    let today = zoned
        .date()
        .at((start / 60) as i8, (start % 60) as i8, 0, 0)
        .to_zoned(TimeZone::system())
        .ok()?;
    if today.timestamp() > now {
        return Some(today.timestamp());
    }
    Some(today.checked_add(Span::new().days(1)).ok()?.timestamp())
}

/// Folds a new observation of a thread into our belief about its cache.
///
/// Activity that lands within a pending keepalive's grace window, or while
/// that keepalive's turn was seen running, is attributed to the keepalive: it
/// refreshes the cache without resetting the refresh budget or counting as
/// the user returning. Anything else is real activity.
///
/// Call this once per thread per tick, for every thread the list reports.
pub fn observe(state: &mut ThreadState, view: &ThreadView, grace_seconds: u64) {
    state.settled = view.settled;
    state.snoozed = view.snoozed;
    state.subagent = view.subagent;
    if state.pending_warm_at.is_some() && !is_quiescent(&view.status) {
        state.pending_seen_running = true;
    }
    if state.seen_updated_at != Some(view.updated_at) {
        let ours = state.pending_warm_at.is_some_and(|sent| {
            view.updated_at.as_second() <= sent.as_second() + grace_seconds as i64
                || state.pending_seen_running
        });
        state.seen_updated_at = Some(view.updated_at);
        state.last_cache_at = Some(view.updated_at);
        if !ours {
            state.last_activity_at = Some(view.updated_at);
            state.refreshes = 0;
        }
    }
    if state.pending_warm_at.is_some() && state.pending_seen_running && is_quiescent(&view.status) {
        // Our keepalive turn finished; stop attributing later activity to it.
        state.pending_seen_running = false;
        state.pending_warm_at = None;
    }
}

/// Drops a pending keepalive once its grace window has passed, whether or not
/// its turn was ever observed. A keepalive run is a one-word reply: if it has
/// not settled by then, either it failed or this process missed the run, and
/// holding the thread's budget hostage forever would be worse than warming it
/// once more. Anything still running is skipped by [`decide`] regardless.
pub fn expire_pending(state: &mut ThreadState, now: Timestamp, grace_seconds: u64) {
    if let Some(sent) = state.pending_warm_at
        && now.as_second() > sent.as_second() + grace_seconds as i64
    {
        state.pending_warm_at = None;
        state.pending_seen_running = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, Rule};

    fn timestamp(text: &str) -> Timestamp {
        text.parse().expect("timestamp")
    }

    fn view(updated_at: &str) -> ThreadView {
        ThreadView {
            id: "thread-1".to_owned(),
            title: "Test".to_owned(),
            settled: false,
            snoozed: false,
            status: "idle".to_owned(),
            provider: "codex".to_owned(),
            model: "gpt-6.1-sol".to_owned(),
            updated_at: timestamp(updated_at),
            subagent: false,
        }
    }

    fn config() -> Config {
        Config {
            lead_seconds: 30,
            rules: vec![
                Rule {
                    provider: Some("codex".to_owned()),
                    model_contains: "gpt".to_owned(),
                    ttl_seconds: 1800,
                },
                Rule {
                    provider: None,
                    model_contains: "claude".to_owned(),
                    ttl_seconds: 300,
                },
            ],
            ..Config::default()
        }
    }

    fn warm_state(updated_at: &str) -> ThreadState {
        ThreadState {
            last_activity_at: Some(timestamp(updated_at)),
            last_cache_at: Some(timestamp(updated_at)),
            seen_updated_at: Some(timestamp(updated_at)),
            ..ThreadState::default()
        }
    }

    #[test]
    fn warms_when_due_and_active() {
        let config = config();
        let state = warm_state("2026-10-14T09:00:00Z");
        let decision = decide(
            &config,
            &view("2026-10-14T09:00:00Z"),
            &state,
            timestamp("2026-10-14T09:45:00Z"),
        );
        assert_eq!(decision, Decision::Warm);
    }

    #[test]
    fn skips_until_ttl_elapsed() {
        let config = config();
        let state = warm_state("2026-10-14T09:00:00Z");
        let decision = decide(
            &config,
            &view("2026-10-14T09:00:00Z"),
            &state,
            timestamp("2026-10-14T09:20:00Z"),
        );
        assert_eq!(
            decision,
            Decision::Skip {
                reason: SkipReason::CacheFresh
            }
        );
    }

    #[test]
    fn warms_at_the_lead_time_not_the_ttl() {
        // codex/gpt rule: ttl 1800, lead 30, so due 1770s after the write.
        let config = config();
        let state = warm_state("2026-10-14T09:00:00Z");
        let fresh = decide(
            &config,
            &view("2026-10-14T09:00:00Z"),
            &state,
            timestamp("2026-10-14T09:29:29Z"),
        );
        assert_eq!(
            fresh,
            Decision::Skip {
                reason: SkipReason::CacheFresh
            }
        );
        let due = decide(
            &config,
            &view("2026-10-14T09:00:00Z"),
            &state,
            timestamp("2026-10-14T09:29:30Z"),
        );
        assert_eq!(due, Decision::Warm);
    }

    #[test]
    fn refresh_due_subtracts_the_lead_time() {
        let base = timestamp("2026-10-14T09:00:00Z");
        assert_eq!(
            refresh_due_after(base, 1800, 30),
            timestamp("2026-10-14T09:29:30Z")
        );
        assert_eq!(
            refresh_due_after(base, 20, 30),
            base,
            "a lead past the ttl means due now"
        );
    }

    fn tracked(cache_at: &str) -> ThreadState {
        ThreadState {
            provider: "codex".to_owned(),
            model: "gpt-6.1-sol".to_owned(),
            ..warm_state(cache_at)
        }
    }

    #[test]
    fn next_wakeup_picks_the_earliest_due_thread() {
        let config = config();
        let mut state = State::default();
        // Due 09:29:30 and 09:39:30 respectively.
        state.threads.insert("a".to_owned(), tracked("2026-10-14T09:00:00Z"));
        state.threads.insert("b".to_owned(), tracked("2026-10-14T09:10:00Z"));
        let wake = next_wakeup(&state, &config, timestamp("2026-10-14T09:05:00Z"));
        assert_eq!(wake, Some(timestamp("2026-10-14T09:29:30Z")));
    }

    #[test]
    fn scheduler_only_schedules_warmable_threads() {
        // A thread `next_wakeup` wakes for must be one `decide` would warm
        // once quiescent: both go through the same stored gates.
        let config = config();
        let mut state = State::default();
        state.threads.insert("a".to_owned(), tracked("2026-10-14T09:00:00Z"));
        let now = timestamp("2026-10-14T09:05:00Z");
        let wake = next_wakeup(&state, &config, now).expect("scheduled");
        let thread = state.threads.get("a").expect("tracked");
        assert_eq!(
            decide(&config, &view("2026-10-14T09:00:00Z"), thread, wake),
            Decision::Warm
        );
    }

    #[test]
    fn next_wakeup_ignores_unwarmable_threads() {
        let config = config();
        let now = timestamp("2026-10-14T09:05:00Z");
        let cases: Vec<(&str, ThreadState)> = vec![
            ("pending", {
                let mut thread = tracked("2026-10-14T09:00:00Z");
                thread.pending_warm_at = Some(timestamp("2026-10-14T09:04:00Z"));
                thread
            }),
            ("spent budget", {
                let mut thread = tracked("2026-10-14T09:00:00Z");
                thread.refreshes = config.max_refreshes_per_activity;
                thread
            }),
            ("stale activity", tracked("2026-10-14T02:00:00Z")),
            ("settled", {
                let mut thread = tracked("2026-10-14T09:00:00Z");
                thread.settled = true;
                thread
            }),
            ("no rule", {
                let mut thread = tracked("2026-10-14T09:00:00Z");
                thread.model = "unknown-model".to_owned();
                thread
            }),
        ];
        for (name, thread) in cases {
            let mut state = State::default();
            state.threads.insert("thread".to_owned(), thread);
            assert_eq!(next_wakeup(&state, &config, now), None, "{name}");
        }
        assert_eq!(
            next_wakeup(&State::default(), &config, now),
            None,
            "no threads"
        );
    }

    #[test]
    fn next_wakeup_outside_hours_waits_for_the_window_to_reopen() {
        let mut config = config();
        let now = timestamp("2026-10-14T09:05:00Z");
        let zoned = now.to_zoned(TimeZone::system());
        let minutes = i16::from(zoned.hour()) * 60 + i16::from(zoned.minute());
        // A two-hour window entirely in the past or future today, so `now`
        // is outside it without any midnight wrap to reason about.
        let (start, end) = if minutes >= 240 {
            (minutes - 240, minutes - 120)
        } else {
            (minutes + 120, minutes + 240)
        };
        config.day_window.start = format!("{:02}:{:02}", start / 60, start % 60);
        config.day_window.end = format!("{:02}:{:02}", end / 60, end % 60);
        assert!(!within_day_window(&config, now));
        let wake = next_wakeup(&State::default(), &config, now).expect("reopening is scheduled");
        assert!(wake > now);
        let mut expected = (start - minutes).rem_euclid(1440) as i64 * 60;
        if expected == 0 {
            expected = 86_400;
        }
        assert_eq!(wake.as_second() - now.as_second(), expected);
    }

    #[test]
    fn skips_without_matching_rule() {
        let config = config();
        let mut thread = view("2026-10-14T09:00:00Z");
        thread.model = "deepseek-v4.1-flash".to_owned();
        let state = warm_state("2026-10-14T09:00:00Z");
        let decision = decide(&config, &thread, &state, timestamp("2026-10-14T10:00:00Z"));
        assert_eq!(
            decision,
            Decision::Skip {
                reason: SkipReason::NoRule
            }
        );
    }

    #[test]
    fn first_rule_wins() {
        let config = config();
        assert_eq!(rule_ttl(&config, "codex", "gpt-6.1-sol"), Some(1800));
        assert_eq!(rule_ttl(&config, "opencode", "gpt-6.1-sol"), None);
        assert_eq!(rule_ttl(&config, "opencode", "claude-5"), Some(300));
    }

    #[test]
    fn treats_completed_threads_as_warmable_but_not_failed_ones() {
        let config = config();
        let state = warm_state("2026-10-14T09:00:00Z");
        let now = timestamp("2026-10-14T10:00:00Z");

        let mut completed = view("2026-10-14T09:00:00Z");
        completed.status = "completed".to_owned();
        assert_eq!(decide(&config, &completed, &state, now), Decision::Warm);

        let mut failed = view("2026-10-14T09:00:00Z");
        failed.status = "failed".to_owned();
        assert_eq!(
            decide(&config, &failed, &state, now),
            Decision::Skip {
                reason: SkipReason::Running
            }
        );
    }

    #[test]
    fn skips_busy_or_parked_threads() {
        let config = config();
        let state = warm_state("2026-10-14T09:00:00Z");
        let now = timestamp("2026-10-14T10:00:00Z");

        let mut thread = view("2026-10-14T09:00:00Z");
        thread.status = "running".to_owned();
        assert_eq!(
            decide(&config, &thread, &state, now),
            Decision::Skip {
                reason: SkipReason::Running
            }
        );

        let mut thread = view("2026-10-14T09:00:00Z");
        thread.settled = true;
        assert_eq!(
            decide(&config, &thread, &state, now),
            Decision::Skip {
                reason: SkipReason::Settled
            }
        );

        let mut thread = view("2026-10-14T09:00:00Z");
        thread.snoozed = true;
        assert_eq!(
            decide(&config, &thread, &state, now),
            Decision::Skip {
                reason: SkipReason::Snoozed
            }
        );

        let mut thread = view("2026-10-14T09:00:00Z");
        thread.subagent = true;
        assert_eq!(
            decide(&config, &thread, &state, now),
            Decision::Skip {
                reason: SkipReason::Subagent
            }
        );
    }

    #[test]
    fn skips_stale_threads_and_spent_budget() {
        let config = config();
        let now = timestamp("2026-10-14T15:00:00Z");
        let state = warm_state("2026-10-14T09:00:00Z");
        assert_eq!(
            decide(&config, &view("2026-10-14T09:00:00Z"), &state, now),
            Decision::Skip {
                reason: SkipReason::IdleTooLong
            }
        );

        let mut spent = warm_state("2026-10-14T14:30:00Z");
        spent.refreshes = config.max_refreshes_per_activity;
        assert_eq!(
            decide(&config, &view("2026-10-14T14:30:00Z"), &spent, now),
            Decision::Skip {
                reason: SkipReason::BudgetSpent
            }
        );
    }

    #[test]
    fn counts_our_keepalive_and_its_reply_as_ours() {
        let mut state = ThreadState {
            last_activity_at: Some(timestamp("2026-10-14T09:00:00Z")),
            last_cache_at: Some(timestamp("2026-10-14T09:00:00Z")),
            seen_updated_at: Some(timestamp("2026-10-14T09:00:00Z")),
            refreshes: 1,
            pending_warm_at: Some(timestamp("2026-10-14T09:40:00Z")),
            ..ThreadState::default()
        };

        // Our send appears.
        observe(&mut state, &view("2026-10-14T09:40:01Z"), 300);
        assert_eq!(state.refreshes, 1, "budget untouched");
        assert_eq!(
            state.last_activity_at,
            Some(timestamp("2026-10-14T09:00:00Z")),
            "real activity untouched"
        );

        // The model's reply appears, and the thread is idle again.
        observe(&mut state, &view("2026-10-14T09:40:30Z"), 300);
        assert_eq!(state.last_cache_at, Some(timestamp("2026-10-14T09:40:30Z")));
        assert_eq!(
            state.pending_warm_at,
            Some(timestamp("2026-10-14T09:40:00Z")),
            "cleared only once the turn was seen running"
        );
        assert_eq!(state.refreshes, 1);

        // Real activity afterwards resets the budget.
        observe(&mut state, &view("2026-10-14T10:30:00Z"), 300);
        assert_eq!(state.refreshes, 0);
        assert_eq!(
            state.last_activity_at,
            Some(timestamp("2026-10-14T10:30:00Z"))
        );
    }

    #[test]
    fn counts_a_slow_keepalive_turn_as_ours_and_clears_it() {
        let mut state = ThreadState {
            last_activity_at: Some(timestamp("2026-10-14T09:00:00Z")),
            last_cache_at: Some(timestamp("2026-10-14T09:00:00Z")),
            seen_updated_at: Some(timestamp("2026-10-14T09:00:00Z")),
            refreshes: 1,
            pending_warm_at: Some(timestamp("2026-10-14T09:40:00Z")),
            ..ThreadState::default()
        };
        let mut running = view("2026-10-14T09:40:05Z");
        running.status = "starting".to_owned();
        observe(&mut state, &running, 300);
        assert!(state.pending_seen_running);

        // The reply lands well outside the grace window, then the run finishes.
        observe(&mut state, &view("2026-10-14T09:55:00Z"), 300);
        assert_eq!(state.refreshes, 1);
        assert_eq!(
            state.last_activity_at,
            Some(timestamp("2026-10-14T09:00:00Z"))
        );
        assert_eq!(state.pending_warm_at, None, "turn finished");
        assert!(!state.pending_seen_running);
    }

    #[test]
    fn expires_a_pending_keepalive_after_its_grace_window() {
        let mut state = ThreadState {
            pending_warm_at: Some(timestamp("2026-10-14T09:40:00Z")),
            ..ThreadState::default()
        };
        expire_pending(&mut state, timestamp("2026-10-14T09:42:00Z"), 300);
        assert!(state.pending_warm_at.is_some(), "still inside grace");
        expire_pending(&mut state, timestamp("2026-10-14T09:50:00Z"), 300);
        assert!(state.pending_warm_at.is_none(), "grace passed");

        // Even a keepalive whose run was seen must not pin the budget forever:
        // the process may have missed the run finishing.
        let mut stuck = ThreadState {
            pending_warm_at: Some(timestamp("2026-10-14T09:40:00Z")),
            pending_seen_running: true,
            ..ThreadState::default()
        };
        expire_pending(&mut stuck, timestamp("2026-10-14T09:50:00Z"), 300);
        assert!(stuck.pending_warm_at.is_none());
        assert!(!stuck.pending_seen_running);
    }

    #[test]
    fn window_boundaries_and_wrap() {
        assert!(within_window(600, 480, 1380), "10:00 inside 08:00-23:00");
        assert!(within_window(480, 480, 1380), "start inclusive");
        assert!(!within_window(1380, 480, 1380), "end exclusive");
        assert!(within_window(1400, 1320, 360), "23:20 inside 22:00-06:00");
        assert!(within_window(60, 1320, 360), "01:00 inside 22:00-06:00");
        assert!(!within_window(600, 1320, 360), "10:00 outside 22:00-06:00");
    }

    #[test]
    fn seeding_treats_current_state_as_fresh() {
        let state = seed(&view("2026-10-14T09:00:00Z"));
        assert_eq!(state.last_activity_at, Some(timestamp("2026-10-14T09:00:00Z")));
        assert_eq!(state.last_cache_at, state.last_activity_at);
        assert_eq!(state.refreshes, 0);
    }
}
