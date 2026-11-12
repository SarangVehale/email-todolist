//! Background scheduler: wakes snoozed tasks, sends the daily digest, and decides quiet hours.
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

fn tick(core: &Arc<Core>) {
    let Ok(store) = core.store() else { return };
    let s = core.settings();
    let now = Local::now();
    if let Ok(due) = store.wake_due(now.timestamp() as f64) {
        for t in due {
            core.notify_user(
                "Back from snooze",
                if t.action.is_empty() {
                    &t.subject
                } else {
                    &t.action
                },
            );
        }
    }
    let today = now.format("%Y-%m-%d").to_string();
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
            .filter(|t| t.status == "open" && t.snooze_until <= 0.0)
            .collect();
        if !open.is_empty() {
            let high = open.iter().filter(|t| t.priority >= 4).count();
            let urgent = open.iter().filter(|t| t.urgent).count();
            let drafts = open.iter().filter(|t| t.draft.is_some()).count();
            core.notify_user(
                "Inbox digest",
                &format!(
                    "{} open · {high} high · {urgent} urgent · {drafts} draft{} ready",
                    open.len(),
                    if drafts == 1 { "" } else { "s" }
                ),
            );
        }
    }
}

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
