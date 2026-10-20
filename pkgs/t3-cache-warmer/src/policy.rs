//! Pure gating and cache-attribution logic, kept free of I/O so it is cheap
//! to test from a fixed clock.

use jiff::{Timestamp, Zoned, tz::TimeZone};
use std::time::Duration;

use crate::config::Config;
use crate::state::{State, ThreadState};
use crate::t3::{CompletedRun, ThreadView};

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
    CacheExpired,
    PendingWarm,
}

impl SkipReason {
    /// Stable label for logs. The exhaustive match keeps every variant named
    /// as variants come and go.
    pub fn label(self) -> &'static str {
        match self {
            Self::Settled => "settled",
            Self::Snoozed => "snoozed",
            Self::Subagent => "subagent",
            Self::Running => "running",
            Self::NoRule => "no-rule",
            Self::OutsideHours => "outside-hours",
            Self::IdleTooLong => "idle-too-long",
            Self::BudgetSpent => "budget-spent",
            Self::CacheFresh => "cache-fresh",
            Self::CacheExpired => "cache-expired",
            Self::PendingWarm => "pending-warm",
        }
    }
}

/// Decides whether one thread should be warmed now.
pub fn decide(config: &Config, state: &ThreadState, now: Timestamp) -> Decision {
    let due = match refresh_at(config, state, &now.to_zoned(TimeZone::system())) {
        Ok(due) => due,
        Err(reason) => return Decision::Skip { reason },
    };
    if now < due {
        return Decision::Skip {
            reason: SkipReason::CacheFresh,
        };
    }
    Decision::Warm
}

/// Gates shared by run-history discovery, decisions, scheduling, and status.
pub fn warming_ttl(config: &Config, state: &ThreadState, local: &Zoned) -> Result<u64, SkipReason> {
    if state.settled {
        return Err(SkipReason::Settled);
    }
    if state.snoozed {
        return Err(SkipReason::Snoozed);
    }
    if state.subagent && config.skip_subagents {
        return Err(SkipReason::Subagent);
    }
    if !is_quiescent(&state.status) {
        return Err(SkipReason::Running);
    }
    if !config.day_window.contains(local.time()) {
        return Err(SkipReason::OutsideHours);
    }
    rule_ttl(config, &state.provider, &state.model).ok_or(SkipReason::NoRule)
}

fn refresh_at(
    config: &Config,
    state: &ThreadState,
    local: &Zoned,
) -> Result<Timestamp, SkipReason> {
    let ttl = warming_ttl(config, state, local)?;
    let now = local.timestamp();
    let last_cache = state.last_cache_at().ok_or(SkipReason::IdleTooLong)?;
    let Some(last_activity) = state.last_activity_at else {
        return Err(SkipReason::IdleTooLong);
    };
    if now.as_second() - last_activity.as_second() > config.active_window_minutes as i64 * 60 {
        return Err(SkipReason::IdleTooLong);
    }
    if state
        .pending_warm_at
        .is_some_and(|sent| now < deadline_after(sent, config.send_grace_seconds))
    {
        // An accepted send may not appear in T3's listing immediately. Its
        // acknowledgement window, not the old cache timestamp, blocks resends.
        return Err(SkipReason::PendingWarm);
    }
    if state.refreshes >= config.max_refreshes_per_activity {
        return Err(SkipReason::BudgetSpent);
    }
    // Keep an existing cache alive; never pay to recreate one after expiry.
    let expires_at = deadline_after(last_cache, ttl);
    if now >= expires_at {
        return Err(SkipReason::CacheExpired);
    }
    Ok(expires_at
        .checked_sub(Duration::from_secs(config.lead_seconds))
        .expect("refresh deadline in timestamp range"))
}

fn deadline_after(at: Timestamp, seconds: u64) -> Timestamp {
    at.checked_add(Duration::from_secs(seconds))
        .expect("deadline in timestamp range")
}

/// Statuses in which a thread is doing nothing and may receive a keepalive.
/// `completed` is T3's state for a thread whose last run finished; anything
/// else (`running`, `preparing`, `failed`, …) means hands off.
fn is_quiescent(status: &str) -> bool {
    matches!(status, "idle" | "completed")
}

/// The TTL of the first matching rule, or `None` when nothing matches.
fn rule_ttl(config: &Config, provider: &str, model: &str) -> Option<u64> {
    config
        .rules
        .iter()
        .find(|rule| {
            let provider_matches = rule.provider.as_deref().is_none_or(|want| want == provider);
            provider_matches && model.contains(&rule.model_contains)
        })
        .map(|rule| rule.ttl_seconds)
}

/// Discovery is periodic; known refresh deadlines can wake the daemon sooner.
/// Overdue eligible threads need a retry after a failed send, while running
/// and pending threads wait for the next discovery pass.
pub fn next_wait(state: &State, config: &Config, now: Timestamp) -> Duration {
    let local = now.to_zoned(TimeZone::system());
    if !config.day_window.contains(local.time()) {
        return config
            .day_window
            .next_start(&local)
            .duration_since(now)
            .unsigned_abs();
    }
    let cap = Duration::from_secs(config.tick_seconds);
    let Some(due) = state
        .threads
        .values()
        .filter_map(|thread| refresh_at(config, thread, &local).ok())
        .min()
    else {
        return cap;
    };
    if due <= now {
        return Duration::from_secs(15).min(cap);
    }
    due.duration_since(now).unsigned_abs().min(cap)
}

/// Observe metadata and expire pending sends before attributing any new run.
/// Metadata changes, including visits, never refresh the cache.
pub fn observe(state: &mut ThreadState, view: &ThreadView, now: Timestamp, grace_seconds: u64) {
    if let Some(sent) = state.pending_warm_at
        && now >= deadline_after(sent, grace_seconds)
    {
        // A failed history read must not turn an already completed keepalive
        // into user activity. Retain its attribution until that run is read;
        // the expired grace window no longer blocks sends in refresh_at.
        if view
            .latest_run_id
            .as_deref()
            .is_none_or(|run_id| state.has_run(run_id))
        {
            state.pending_warm_at = None;
        }
        state.pending_seen_running = false;
    }
    state.title.clone_from(&view.title);
    state.provider.clone_from(&view.provider);
    state.model.clone_from(&view.model);
    state.status.clone_from(&view.status);
    state.settled = view.settled;
    state.snoozed = view.snoozed;
    state.subagent = view.subagent;
    if state.pending_warm_at.is_some() && !is_quiescent(&view.status) {
        state.pending_seen_running = true;
    }
    state.seen_updated_at = Some(view.updated_at);
    if view.latest_run_id.is_none() {
        state.last_run = None;
        state.last_activity_at = None;
    }
}

/// Only a newly completed run can refresh the cache or reset the user budget.
pub fn observe_run(state: &mut ThreadState, run: CompletedRun, grace_seconds: u64) {
    if state.has_run(&run.run_id) {
        return;
    }
    let pending = state
        .pending_warm_at
        .filter(|&sent| run.completed_at >= sent);
    let ours = pending.is_some_and(|sent| {
        run.completed_at.as_second() <= sent.as_second() + grace_seconds as i64
            || state.pending_seen_running
    });
    if !ours {
        state.last_activity_at = Some(run.completed_at);
        state.refreshes = 0;
    }
    state.last_run = Some(run);
    if pending.is_some() {
        state.pending_seen_running = false;
        state.pending_warm_at = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, Rule};

    #[test]
    fn skip_labels_are_distinct() {
        let reasons = [
            SkipReason::Settled,
            SkipReason::Snoozed,
            SkipReason::Subagent,
            SkipReason::Running,
            SkipReason::NoRule,
            SkipReason::OutsideHours,
            SkipReason::IdleTooLong,
            SkipReason::BudgetSpent,
            SkipReason::CacheFresh,
            SkipReason::CacheExpired,
            SkipReason::PendingWarm,
        ];
        let mut labels: Vec<&str> = reasons.iter().copied().map(SkipReason::label).collect();
        labels.sort_unstable();
        let count = labels.len();
        labels.dedup();
        assert_eq!(labels.len(), count, "labels must be distinct");
        assert!(labels.iter().all(|label| !label.is_empty()));
    }

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
            provider: "opencode".to_owned(),
            model: "openai/gpt-6.1-sol".to_owned(),
            updated_at: timestamp(updated_at),
            latest_run_id: Some(format!("run-{updated_at}")),
            subagent: false,
        }
    }

    fn config() -> Config {
        Config {
            lead_seconds: 30,
            rules: vec![
                Rule {
                    provider: Some("opencode".to_owned()),
                    model_contains: "openai/gpt".to_owned(),
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
        let mut state = ThreadState::default();
        observe(&mut state, &view(updated_at), timestamp(updated_at), 300);
        observe_run(&mut state, completed_run(updated_at), 300);
        state
    }

    fn completed_run(at: &str) -> CompletedRun {
        CompletedRun {
            run_id: format!("run-{at}"),
            completed_at: timestamp(at),
        }
    }

    #[test]
    fn warms_only_in_the_refresh_window() {
        let config = config();
        let state = warm_state("2026-10-14T09:00:00.250Z");
        for (at, expected) in [
            (
                "2026-10-14T09:20:00Z",
                Decision::Skip {
                    reason: SkipReason::CacheFresh,
                },
            ),
            (
                "2026-10-14T09:29:30.249999999Z",
                Decision::Skip {
                    reason: SkipReason::CacheFresh,
                },
            ),
            ("2026-10-14T09:29:30.250Z", Decision::Warm),
            ("2026-10-14T09:29:45Z", Decision::Warm),
            ("2026-10-14T09:30:00.249999999Z", Decision::Warm),
            (
                "2026-10-14T09:30:00.250Z",
                Decision::Skip {
                    reason: SkipReason::CacheExpired,
                },
            ),
            (
                "2026-10-14T09:45:00Z",
                Decision::Skip {
                    reason: SkipReason::CacheExpired,
                },
            ),
        ] {
            assert_eq!(decide(&config, &state, timestamp(at)), expected, "{at}");
        }
    }

    #[test]
    fn next_wait_picks_the_earliest_due_thread() {
        let mut config = config();
        config.tick_seconds = 3600;
        let mut state = State::default();
        // Due 09:29:30 and 09:39:30 respectively.
        state
            .threads
            .insert("a".to_owned(), warm_state("2026-10-14T09:00:00Z"));
        state
            .threads
            .insert("b".to_owned(), warm_state("2026-10-14T09:10:00Z"));
        let wait = next_wait(&state, &config, timestamp("2026-10-14T09:05:00Z"));
        assert_eq!(wait, Duration::from_secs(24 * 60 + 30));
    }

    #[test]
    fn scheduler_only_schedules_warmable_threads() {
        let mut config = config();
        config.tick_seconds = 3600;
        let mut state = State::default();
        state
            .threads
            .insert("a".to_owned(), warm_state("2026-10-14T09:00:00Z"));
        let now = timestamp("2026-10-14T09:05:00Z");
        let wake = now.checked_add(next_wait(&state, &config, now)).unwrap();
        let thread = state.threads.get("a").expect("tracked");
        assert_eq!(decide(&config, thread, wake), Decision::Warm);
    }

    #[test]
    fn pending_blocks_resends_but_does_not_revive_an_expired_cache() {
        let config = config();
        let mut state = warm_state("2026-10-14T09:00:00Z");
        state.pending_warm_at = Some(timestamp("2026-10-14T09:29:30Z"));
        assert_eq!(
            decide(&config, &state, timestamp("2026-10-14T09:34:29.999Z")),
            Decision::Skip {
                reason: SkipReason::PendingWarm
            },
            "the old cache is overdue but the send is still pending"
        );
        assert_eq!(
            decide(&config, &state, timestamp("2026-10-14T09:34:30Z")),
            Decision::Skip {
                reason: SkipReason::CacheExpired
            },
            "missing acknowledgement is not evidence of a fresh cache"
        );
    }

    #[test]
    fn observe_updates_thread_identity_status_and_parking_flags() {
        let mut seen = view("2026-10-14T09:00:00Z");
        seen.settled = true;
        seen.snoozed = true;
        seen.subagent = true;
        seen.status = "running".to_owned();
        let mut state = ThreadState::default();
        observe(&mut state, &seen, seen.updated_at, 300);
        assert!(state.settled && state.snoozed && state.subagent);
        assert_eq!(state.status, "running");
        assert_eq!(state.provider, "opencode");
        assert_eq!(state.model, "openai/gpt-6.1-sol");
    }

    #[test]
    fn next_wait_ignores_unwarmable_threads() {
        let mut config = config();
        config.tick_seconds = 3600;
        let now = timestamp("2026-10-14T09:05:00Z");
        let cases: Vec<(&str, ThreadState)> = vec![
            ("pending", {
                let mut thread = warm_state("2026-10-14T09:00:00Z");
                thread.pending_warm_at = Some(timestamp("2026-10-14T09:04:00Z"));
                thread
            }),
            ("spent budget", {
                let mut thread = warm_state("2026-10-14T09:00:00Z");
                thread.refreshes = config.max_refreshes_per_activity;
                thread
            }),
            ("stale activity", warm_state("2026-10-14T02:00:00Z")),
            ("expired cache", warm_state("2026-10-14T08:35:00Z")),
            ("settled", {
                let mut thread = warm_state("2026-10-14T09:00:00Z");
                thread.settled = true;
                thread
            }),
            ("running", {
                let mut thread = warm_state("2026-10-14T09:00:00Z");
                thread.status = "running".to_owned();
                thread
            }),
            ("unknown status", {
                let mut thread = warm_state("2026-10-14T09:00:00Z");
                thread.status.clear();
                thread
            }),
            ("snoozed", {
                let mut thread = warm_state("2026-10-14T09:00:00Z");
                thread.snoozed = true;
                thread
            }),
            ("subagent", {
                let mut thread = warm_state("2026-10-14T09:00:00Z");
                thread.subagent = true;
                thread
            }),
            ("no rule", {
                let mut thread = warm_state("2026-10-14T09:00:00Z");
                thread.model = "unknown-model".to_owned();
                thread
            }),
        ];
        for (name, thread) in cases {
            let mut state = State::default();
            state.threads.insert("thread".to_owned(), thread);
            assert_eq!(
                next_wait(&state, &config, now),
                Duration::from_secs(3600),
                "{name}"
            );
        }
        assert_eq!(
            next_wait(&State::default(), &config, now),
            Duration::from_secs(3600),
            "no threads"
        );
    }

    #[test]
    fn next_wait_outside_hours_waits_for_the_window_to_reopen() {
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
        config.day_window.start = jiff::civil::time((start / 60) as i8, (start % 60) as i8, 0, 0);
        config.day_window.end = jiff::civil::time((end / 60) as i8, (end % 60) as i8, 0, 0);
        assert!(!config.day_window.contains(zoned.time()));
        let mut expected = (start - minutes).rem_euclid(1440) as i64 * 60;
        if expected == 0 {
            expected = 86_400;
        }
        assert_eq!(
            next_wait(&State::default(), &config, now),
            Duration::from_secs(expected as u64)
        );
    }

    #[test]
    fn discovery_interval_does_not_delay_a_known_refresh_deadline() {
        let mut config = config();
        config.tick_seconds = 120;
        let mut state = State::default();
        state
            .threads
            .insert("a".to_owned(), warm_state("2026-10-14T09:00:00.250Z"));
        assert_eq!(
            next_wait(&state, &config, timestamp("2026-10-14T09:28:00.500Z")),
            Duration::from_millis(89_750),
            "wake exactly at the refresh deadline, preserving fractional seconds"
        );
        assert_eq!(
            next_wait(&state, &config, timestamp("2026-10-14T09:05:00Z")),
            Duration::from_secs(120),
            "discover new activity before distant deadlines"
        );
    }

    #[test]
    fn overdue_retries_only_wake_eligible_threads() {
        let mut config = config();
        config.tick_seconds = 120;
        let now = timestamp("2026-10-14T09:29:45Z");
        let mut state = State::default();
        state
            .threads
            .insert("a".to_owned(), warm_state("2026-10-14T09:00:00Z"));
        assert_eq!(next_wait(&state, &config, now), Duration::from_secs(15));

        state.threads.get_mut("a").unwrap().pending_warm_at = Some(now);
        assert_eq!(next_wait(&state, &config, now), Duration::from_secs(120));

        let thread = state.threads.get_mut("a").unwrap();
        thread.pending_warm_at = None;
        thread.status = "running".to_owned();
        assert_eq!(next_wait(&state, &config, now), Duration::from_secs(120));
        state.threads.get_mut("a").unwrap().status = "completed".to_owned();
        assert_eq!(
            next_wait(&state, &config, timestamp("2026-10-14T09:30:00Z")),
            Duration::from_secs(120),
            "expired caches wait for discovery, not overdue-send retries"
        );
        assert_eq!(
            next_wait(&State::default(), &config, now),
            Duration::from_secs(120)
        );
    }

    #[test]
    fn skips_without_matching_rule() {
        let config = config();
        let mut state = warm_state("2026-10-14T09:00:00Z");
        state.model = "deepseek-v4.1-flash".to_owned();
        let decision = decide(&config, &state, timestamp("2026-10-14T10:00:00Z"));
        assert_eq!(
            decision,
            Decision::Skip {
                reason: SkipReason::NoRule
            }
        );
    }

    #[test]
    fn first_rule_wins() {
        let mut config = config();
        config.rules.push(Rule {
            provider: None,
            model_contains: "gpt".to_owned(),
            ttl_seconds: 300,
        });
        assert_eq!(
            rule_ttl(&config, "opencode", "openai/gpt-6.1-sol"),
            Some(1800)
        );
        assert_eq!(
            rule_ttl(&config, "other-provider", "openai/gpt-6.1-sol"),
            Some(300)
        );
        assert_eq!(rule_ttl(&config, "opencode", "claude-5"), Some(300));
    }

    #[test]
    fn treats_completed_threads_as_warmable_but_not_failed_ones() {
        let config = config();
        let now = timestamp("2026-10-14T09:29:45Z");

        let mut completed = warm_state("2026-10-14T09:00:00Z");
        completed.status = "completed".to_owned();
        assert_eq!(decide(&config, &completed, now), Decision::Warm);

        let mut failed = warm_state("2026-10-14T09:00:00Z");
        failed.status = "failed".to_owned();
        assert_eq!(
            decide(&config, &failed, now),
            Decision::Skip {
                reason: SkipReason::Running
            }
        );
    }

    #[test]
    fn skips_busy_or_parked_threads() {
        let config = config();
        let now = timestamp("2026-10-14T10:00:00Z");

        let mut thread = warm_state("2026-10-14T09:00:00Z");
        thread.status = "running".to_owned();
        assert_eq!(
            decide(&config, &thread, now),
            Decision::Skip {
                reason: SkipReason::Running
            }
        );

        let mut thread = warm_state("2026-10-14T09:00:00Z");
        thread.settled = true;
        assert_eq!(
            decide(&config, &thread, now),
            Decision::Skip {
                reason: SkipReason::Settled
            }
        );

        let mut thread = warm_state("2026-10-14T09:00:00Z");
        thread.snoozed = true;
        assert_eq!(
            decide(&config, &thread, now),
            Decision::Skip {
                reason: SkipReason::Snoozed
            }
        );

        let mut thread = warm_state("2026-10-14T09:00:00Z");
        thread.subagent = true;
        assert_eq!(
            decide(&config, &thread, now),
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
            decide(&config, &state, now),
            Decision::Skip {
                reason: SkipReason::IdleTooLong
            }
        );

        let mut spent = warm_state("2026-10-14T14:30:15Z");
        spent.refreshes = config.max_refreshes_per_activity;
        assert_eq!(
            decide(&config, &spent, now),
            Decision::Skip {
                reason: SkipReason::BudgetSpent
            }
        );
    }

    #[test]
    fn counts_our_keepalive_and_its_reply_as_ours() {
        let mut state = ThreadState {
            last_activity_at: Some(timestamp("2026-10-14T09:00:00Z")),
            last_run: Some(completed_run("2026-10-14T09:00:00Z")),
            seen_updated_at: Some(timestamp("2026-10-14T09:00:00Z")),
            refreshes: 1,
            pending_warm_at: Some(timestamp("2026-10-14T09:40:00Z")),
            ..ThreadState::default()
        };

        let mut running = view("2026-10-14T09:40:01Z");
        running.status = "running".to_owned();
        observe(&mut state, &running, running.updated_at, 300);
        assert!(state.pending_seen_running);
        assert_eq!(state.refreshes, 1, "budget untouched");
        assert_eq!(
            state.last_activity_at,
            Some(timestamp("2026-10-14T09:00:00Z")),
            "real activity untouched"
        );

        // The model's reply appears, and the thread is idle again.
        observe(
            &mut state,
            &view("2026-10-14T09:40:30Z"),
            timestamp("2026-10-14T09:40:30Z"),
            300,
        );
        observe_run(&mut state, completed_run("2026-10-14T09:40:30Z"), 300);
        assert_eq!(
            state.last_cache_at(),
            Some(timestamp("2026-10-14T09:40:30Z"))
        );
        assert!(state.pending_warm_at.is_none());
        assert!(!state.pending_seen_running);
        assert_eq!(state.refreshes, 1);

        // Real activity afterwards resets the budget.
        observe(
            &mut state,
            &view("2026-10-14T10:30:00Z"),
            timestamp("2026-10-14T10:30:00Z"),
            300,
        );
        observe_run(&mut state, completed_run("2026-10-14T10:30:00Z"), 300);
        assert_eq!(state.refreshes, 0);
        assert_eq!(
            state.last_activity_at,
            Some(timestamp("2026-10-14T10:30:00Z"))
        );
    }

    #[test]
    fn a_reply_past_the_grace_window_counts_as_fresh_activity() {
        let mut state = ThreadState {
            last_activity_at: Some(timestamp("2026-10-14T09:00:00Z")),
            last_run: Some(completed_run("2026-10-14T09:00:00Z")),
            seen_updated_at: Some(timestamp("2026-10-14T09:00:00Z")),
            refreshes: 1,
            pending_warm_at: Some(timestamp("2026-10-14T09:40:00Z")),
            ..ThreadState::default()
        };
        let mut running = view("2026-10-14T09:40:05Z");
        running.status = "starting".to_owned();
        observe(&mut state, &running, timestamp("2026-10-14T09:40:05Z"), 300);
        assert!(state.pending_seen_running);

        // The reply lands well outside the grace window. The pending keepalive
        // expires with this observation, so the reply reads as fresh activity
        // and re-arms the budget; a turn genuinely still in flight is
        // protected by the Running gate in `decide`, not by holding the
        // budget hostage.
        observe(
            &mut state,
            &view("2026-10-14T09:55:00Z"),
            timestamp("2026-10-14T09:55:00Z"),
            300,
        );
        observe_run(&mut state, completed_run("2026-10-14T09:55:00Z"), 300);
        assert_eq!(state.refreshes, 0);
        assert_eq!(
            state.last_activity_at,
            Some(timestamp("2026-10-14T09:55:00Z"))
        );
        assert_eq!(state.pending_warm_at, None);
        assert!(!state.pending_seen_running);
    }

    #[test]
    fn pending_expires_with_the_observation_past_its_grace_window() {
        let mut unchanged = view("2026-10-14T09:40:00Z");
        unchanged.latest_run_id = None;
        let mut state = ThreadState {
            pending_warm_at: Some(timestamp("2026-10-14T09:40:00Z")),
            ..ThreadState::default()
        };
        observe(
            &mut state,
            &unchanged,
            timestamp("2026-10-14T09:42:00Z"),
            300,
        );
        assert!(state.pending_warm_at.is_some(), "still inside grace");
        observe(
            &mut state,
            &unchanged,
            timestamp("2026-10-14T09:50:00Z"),
            300,
        );
        assert!(state.pending_warm_at.is_none(), "grace passed");

        // Even a keepalive whose run was seen must not pin the budget forever:
        // the process may have missed the run finishing.
        let mut stuck = ThreadState {
            pending_warm_at: Some(timestamp("2026-10-14T09:40:00Z")),
            pending_seen_running: true,
            ..ThreadState::default()
        };
        observe(
            &mut stuck,
            &unchanged,
            timestamp("2026-10-14T09:50:00Z"),
            300,
        );
        assert!(stuck.pending_warm_at.is_none());
        assert!(!stuck.pending_seen_running);
    }

    #[test]
    fn seeding_treats_current_state_as_fresh() {
        let state = warm_state("2026-10-14T09:00:00Z");
        assert_eq!(
            state.last_activity_at,
            Some(timestamp("2026-10-14T09:00:00Z"))
        );
        assert_eq!(state.last_cache_at(), state.last_activity_at);
        assert_eq!(state.refreshes, 0);
    }

    #[test]
    fn visits_and_provider_updates_do_not_refresh_the_cache_or_budget() {
        let mut config = config();
        config.rules[0].ttl_seconds = 1680;
        let mut thread = view("2026-10-19T14:37:59.204Z");
        let mut state = warm_state("2026-10-19T14:37:59.204Z");
        state.refreshes = 1;
        thread.updated_at = timestamp("2026-10-19T15:12:51.540Z");
        observe(&mut state, &thread, thread.updated_at, 300);
        assert_eq!(
            state.last_cache_at(),
            Some(timestamp("2026-10-19T14:37:59.204Z"))
        );
        assert_eq!(state.last_activity_at, state.last_cache_at());
        assert_eq!(state.refreshes, 1);
        assert_eq!(
            decide(&config, &state, thread.updated_at),
            Decision::Skip {
                reason: SkipReason::CacheExpired
            }
        );
    }

    #[test]
    fn a_new_run_rearms_warming_after_the_previous_cache_expired() {
        let config = config();
        let mut state = warm_state("2026-10-14T09:00:00Z");
        assert_eq!(
            decide(&config, &state, timestamp("2026-10-14T10:00:00Z")),
            Decision::Skip {
                reason: SkipReason::CacheExpired
            }
        );
        observe_run(&mut state, completed_run("2026-10-14T10:00:00Z"), 300);
        assert_eq!(
            decide(&config, &state, timestamp("2026-10-14T10:00:00Z")),
            Decision::Skip {
                reason: SkipReason::CacheFresh
            }
        );
        assert_eq!(
            decide(&config, &state, timestamp("2026-10-14T10:29:30Z")),
            Decision::Warm
        );
    }

    #[test]
    fn only_completed_runs_establish_cache_freshness() {
        let mut state = ThreadState {
            last_activity_at: Some(timestamp("2026-10-19T15:12:51.540Z")),
            seen_updated_at: Some(timestamp("2026-10-19T15:12:51.540Z")),
            ..ThreadState::default()
        };
        let mut thread = view("2026-10-19T15:12:51.540Z");
        observe(&mut state, &thread, thread.updated_at, 300);
        assert!(state.last_cache_at().is_none());
        assert_eq!(
            decide(&config(), &state, thread.updated_at),
            Decision::Skip {
                reason: SkipReason::IdleTooLong
            }
        );
        observe_run(&mut state, completed_run("2026-10-19T14:37:59.204Z"), 300);
        assert_eq!(
            state.last_cache_at(),
            Some(timestamp("2026-10-19T14:37:59.204Z"))
        );
        assert_eq!(state.last_activity_at, state.last_cache_at());
        thread.latest_run_id = None;
        observe(&mut state, &thread, thread.updated_at, 300);
        assert!(state.last_cache_at().is_none());
        assert!(state.last_activity_at.is_none());
    }

    #[test]
    fn repeated_run_observations_preserve_the_budget() {
        let mut state = warm_state("2026-10-14T09:00:00Z");
        state.refreshes = 2;
        observe_run(&mut state, completed_run("2026-10-14T09:00:00Z"), 300);
        assert_eq!(state.refreshes, 2);
    }
}
