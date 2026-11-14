//! Background scheduler: wakes snoozed tasks, announces reminders, runs the daily clean-up and the update check, sends the daily
//! digest, and decides quiet hours.
//! The decisions are pure functions of the clock so they can be tested without waiting.
use crate::core::Core;
use chrono::{Datelike, Duration as Span, Local, NaiveTime, TimeZone, Weekday};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// "HH:MM" to minutes since midnight.
pub fn clock_minutes(t: &str) -> Option<u32> {
    let (h, m) = t.split_once(':')?;
    let (h, m): (u32, u32) = (h.parse().ok()?, m.parse().ok()?);
    (h < 24 && m < 60).then_some(h * 60 + m)
}

/// True when `now` (minutes since midnight) falls inside the quiet window. Windows may wrap past midnight.
/// An empty or invalid window is never quiet.
pub fn in_quiet_window(start: &str, end: &str, now: u32) -> bool {
    let (Some(s), Some(e)) = (clock_minutes(start), clock_minutes(end)) else {
        return false;
    };
    match s.cmp(&e) {
        std::cmp::Ordering::Equal => false,
        std::cmp::Ordering::Less => now >= s && now < e,
        std::cmp::Ordering::Greater => now >= s || now < e,
    }
}

/// The digest goes out once per day, at or after the chosen time.
pub fn digest_due(digest_time: &str, now: u32, last_sent_date: Option<&str>, today: &str) -> bool {
    clock_minutes(digest_time).is_some_and(|t| now >= t) && last_sent_date != Some(today)
}

/// The clean-up of old data runs once a day, not only when the app starts: a session left open for weeks must not
/// keep finished tasks, log lines and follow-up records past what PRIVACY.md says.
pub fn prune_due(last_run_date: Option<&str>, today: &str) -> bool {
    last_run_date != Some(today)
}

/// The local time now as minutes since midnight, in the same form `clock_minutes` gives.
pub fn now_minutes() -> u32 {
    use chrono::Timelike;
    let n = Local::now();
    n.hour() * 60 + n.minute()
}

/// When a snooze preset ends, as unix seconds. `from` is the current time.
pub fn snooze_until(preset: &str, from: chrono::DateTime<Local>) -> Option<f64> {
    let at_nine = |d: chrono::NaiveDate| {
        Local
            .from_local_datetime(&d.and_time(NaiveTime::from_hms_opt(9, 0, 0)?))
            .earliest()
            .map(|t| t.timestamp() as f64)
    };
    match preset {
        "3h" => Some((from + Span::hours(3)).timestamp() as f64),
        "tomorrow" => at_nine(from.date_naive() + Span::days(1)),
        "nextweek" => {
            let mut d = from.date_naive() + Span::days(1);
            while d.weekday() != Weekday::Mon {
                d += Span::days(1);
            }
            at_nine(d)
        }
        "clear" => Some(0.0),
        _ => None,
    }
}

/// Once a day (an attempt, not a success: a rate limit or a deleted repository must not be retried all day, and the
/// scheduler restarts whenever settings are saved) asks GitHub for the latest release, if the project has a repository
/// and the user has not turned the check off. Runs on its own thread so a slow network never delays a snooze wake-up.
fn check_for_update(
    core: &Arc<Core>,
    store: &crate::store::Store,
    s: &crate::settings::Settings,
    today: &str,
) {
    static RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    let now = Local::now().timestamp();
    let waiting = store
        .meta_get("update_next_try")
        .and_then(|v| v.parse::<i64>().ok())
        .is_some_and(|t| now < t);
    if crate::updates::UPDATE_REPO.is_empty()
        || !s.updates.check
        || waiting
        || store.meta_get("update_checked").as_deref() == Some(today)
        || RUNNING.swap(true, Ordering::SeqCst)
    {
        return;
    }
    let (core, today) = (core.clone(), today.to_string());
    std::thread::spawn(move || {
        let found = crate::updates::check(
            "https://api.github.com",
            crate::updates::UPDATE_REPO,
            env!("CARGO_PKG_VERSION"),
        );
        if let Ok(store) = core.store() {
            match found {
                // the server answered: that is this day's check, whatever it said
                Ok(found) => {
                    let tag = found.map(|u| u.tag).unwrap_or_default();
                    let _ = store.meta_set("update_available", &tag);
                    let _ = store.meta_set("update_checked", &today);
                    let _ = store.meta_set("update_next_try", "0");
                }
                // no answer (no network yet after a reboot, a sleeping machine): try again in an hour, not tomorrow
                Err(_) => {
                    let _ = store.meta_set(
                        "update_next_try",
                        &(Local::now().timestamp() + 3600).to_string(),
                    );
                }
            }
        }
        RUNNING.store(false, Ordering::SeqCst);
    });
}

/// One line per conversation (not per message) for tasks whose snooze just ended.
pub fn wake_notices(due: &[crate::store::Task]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    due.iter()
        .filter(|t| seen.insert((t.mailbox, t.thread.clone()))) // a conversation is one mailbox's
        .map(|t| {
            if t.action.is_empty() {
                t.subject.clone()
            } else {
                t.action.clone()
            }
        })
        .collect()
}

/// Wake time for "snooze until this date": 09:00 local on that day. The date must be a real day, no earlier than
/// today (and today only while 09:00 is still ahead) and no more than a year away.
pub fn snooze_on(date: &str, from: chrono::DateTime<Local>) -> Option<f64> {
    if !crate::dates::valid_iso(date) {
        return None;
    }
    let day = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
    if day > from.date_naive() + Span::days(366) {
        return None;
    }
    let at = Local
        .from_local_datetime(&day.and_time(NaiveTime::from_hms_opt(9, 0, 0)?))
        .earliest()?;
    (at > from).then(|| at.timestamp() as f64)
}

fn tick(core: &Arc<Core>) {
    let Ok(store) = core.store() else { return };
    let s = core.settings();
    let now = Local::now();
    if let Ok(due) = store.wake_due(now.timestamp() as f64) {
        for label in wake_notices(&due) {
            core.notify_user("Back from snooze", &label);
        }
    }
    // reminders you set: announced once each, whatever the quiet hours (you chose the time)
    if let Ok(due) = store.due_reminders(crate::reminders::now_local()) {
        for (title, body) in crate::reminders::notices(&due) {
            core.notify_user(&title, &body);
        }
    }
    let today = now.format("%Y-%m-%d").to_string();
    if prune_due(store.meta_get("last_prune").as_deref(), &today) {
        let _ = store.prune(90, 30);
        let _ = store.meta_set("last_prune", &today);
    }
    check_for_update(core, &store, &s, &today);
    if digest_due(
        &s.alerts.digest_time,
        now_minutes(),
        store.meta_get("last_digest").as_deref(),
        &today,
    ) {
        let _ = store.meta_set("last_digest", &today);
        let open: Vec<_> = store
            .all_tasks()
            .unwrap_or_default()
            .into_iter()
            .filter(|t| t.status == "open" && t.snooze_until <= 0.0 && !t.gone)
            .collect();
        if !open.is_empty() {
            let today = now.format("%Y-%m-%d").to_string();
            let conversations = open
                .iter()
                .map(|t| (t.mailbox, t.thread.as_str()))
                .collect::<std::collections::HashSet<_>>()
                .len();
            let high = open.iter().filter(|t| t.priority >= 4).count();
            let urgent = open.iter().filter(|t| t.urgent).count();
            let due = open
                .iter()
                .filter(|t| !t.due.is_empty() && t.due <= today)
                .count();
            let drafts = open.iter().filter(|t| t.draft.is_some()).count();
            core.notify_user(
                "Inbox digest",
                &format!(
                    "{conversations} open · {high} high · {due} due{} · {drafts} draft{} ready",
                    if urgent > 0 {
                        format!(" · {urgent} urgent")
                    } else {
                        String::new()
                    },
                    if drafts == 1 { "" } else { "s" }
                ),
            );
        }
    }
}

/// The scheduler loop: wakes snoozed tasks, announces reminders, prunes old data once a day, checks for updates and sends the digest, about every 20 seconds until `stop` is set.
/// It blocks, so `Core::start` runs it on its own thread.
pub fn run(core: Arc<Core>, stop: Arc<AtomicBool>) {
    while !stop.load(Ordering::Relaxed) {
        tick(&core);
        for _ in 0..20 {
            if stop.load(Ordering::Relaxed) {
                return;
            }
            std::thread::sleep(Duration::from_secs(1));
        }
    }
}
