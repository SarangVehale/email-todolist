//! Reminders and repeats for the tasks you add yourself. A reminder time is a LOCAL date and time written
//! "YYYY-MM-DD HH:MM" and kept as that text: it means "9 o'clock on the wall" whatever the clock does, so a repeating task
//! stays at 9 across a daylight-saving change. Design: docs/designs/phase5.md.
use crate::store::Task;
use chrono::{Datelike, Duration, Local, Months, NaiveDateTime};

const FORMAT: &str = "%Y-%m-%d %H:%M";

/// The text exactly as `fmt_local` writes it, or None. Anything else (a stray space, a one-digit hour, seconds) is
/// refused, so a stored time can always be compared as text.
pub fn parse_local(s: &str) -> Option<NaiveDateTime> {
    let d = NaiveDateTime::parse_from_str(s, FORMAT).ok()?;
    (d.format(FORMAT).to_string() == s).then_some(d)
}

pub fn fmt_local(d: NaiveDateTime) -> String {
    d.format(FORMAT).to_string()
}

pub fn now_local() -> NaiveDateTime {
    Local::now().naive_local()
}

pub fn valid_repeat(r: &str) -> bool {
    matches!(r, "" | "daily" | "weekly" | "monthly")
}

/// Reminders are accepted between these years; a date outside them is a typo.
pub fn year_ok(d: NaiveDateTime) -> bool {
    (2000..=2100).contains(&d.year())
}

/// The first occurrence of a repeating task that is later than `after`. Occurrences are counted from `base` (the first
/// one), never from the previous one: a monthly task set for the 31st is on the 28th in February and on the 31st again in
/// March. `None` for anything that does not repeat.
pub fn next_occurrence(
    base: NaiveDateTime,
    repeat: &str,
    after: NaiveDateTime,
) -> Option<NaiveDateTime> {
    if !matches!(repeat, "daily" | "weekly" | "monthly") {
        return None;
    }
    if base > after {
        return Some(base);
    }
    match repeat {
        "monthly" => {
            let diff =
                (after.year() - base.year()) * 12 + after.month() as i32 - base.month() as i32;
            let mut n = diff.max(0) as u32;
            loop {
                let candidate = base.checked_add_months(Months::new(n))?;
                if candidate > after {
                    return Some(candidate);
                }
                n += 1;
            }
        }
        _ => {
            let step = if repeat == "daily" { 1 } else { 7 };
            let mut n = (after - base).num_days() / step;
            loop {
                let candidate = base.checked_add_signed(Duration::days(n * step))?;
                if candidate > after {
                    return Some(candidate);
                }
                n += 1;
            }
        }
    }
}

/// What to tell the user about reminders that just came due: one notice each, or one for all of them when there are more
/// than three (after a long absence a barrage helps nobody).
pub fn notices(due: &[Task]) -> Vec<(String, String)> {
    match due.len() {
        0 => vec![],
        1..=3 => due
            .iter()
            .map(|t| {
                let title = if t.action.is_empty() {
                    &t.subject
                } else {
                    &t.action
                };
                ("Reminder".to_string(), title.clone())
            })
            .collect(),
        n => vec![("Reminders".to_string(), format!("{n} reminders are due"))],
    }
}
