//! Finds a due date in message text without any AI: "due 20 Nov", "by Friday", "deadline: 2026-11-20", "before tomorrow".
//! A date only counts when a cue word ("due", "by", "before", "until", "deadline"...) comes just before it, so
//! "see you on the 5th" or an invoice date does not become a deadline. Relative words resolve against the date the
//! message was received, not today.
use chrono::{Datelike, Duration, Local, NaiveDate, TimeZone, Weekday};

const CUES: &[&str] = &[
    "due", "by", "before", "until", "till", "deadline", "on", "expires", "expiry", "expire",
    "ends", "submit", "respond", "reply", "than", "latest", "closes", "close",
];

const MONTHS: &[(&str, u32)] = &[
    ("jan", 1),
    ("january", 1),
    ("feb", 2),
    ("february", 2),
    ("mar", 3),
    ("march", 3),
    ("apr", 4),
    ("april", 4),
    ("may", 5),
    ("jun", 6),
    ("june", 6),
    ("jul", 7),
    ("july", 7),
    ("aug", 8),
    ("august", 8),
    ("sep", 9),
    ("sept", 9),
    ("september", 9),
    ("oct", 10),
    ("october", 10),
    ("nov", 11),
    ("november", 11),
    ("dec", 12),
    ("december", 12),
];

const WEEKDAYS: &[(&str, Weekday)] = &[
    ("mon", Weekday::Mon),
    ("monday", Weekday::Mon),
    ("tue", Weekday::Tue),
    ("tues", Weekday::Tue),
    ("tuesday", Weekday::Tue),
    ("wed", Weekday::Wed),
    ("weds", Weekday::Wed),
    ("wednesday", Weekday::Wed),
    ("thu", Weekday::Thu),
    ("thur", Weekday::Thu),
    ("thurs", Weekday::Thu),
    ("thursday", Weekday::Thu),
    ("fri", Weekday::Fri),
    ("friday", Weekday::Fri),
    ("sat", Weekday::Sat),
    ("saturday", Weekday::Sat),
    ("sun", Weekday::Sun),
    ("sunday", Weekday::Sun),
];

/// `YYYY-MM-DD` that is a real calendar date between 2000 and 2100.
pub fn valid_iso(s: &str) -> bool {
    parse_iso(s).is_some()
}

fn parse_iso(s: &str) -> Option<NaiveDate> {
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' || !s.is_ascii() {
        return None;
    }
    let (y, m, d) = (
        s[0..4].parse::<i32>().ok()?,
        s[5..7].parse::<u32>().ok()?,
        s[8..10].parse::<u32>().ok()?,
    );
    if !(2000..=2100).contains(&y) {
        return None;
    }
    NaiveDate::from_ymd_opt(y, m, d)
}

/// The calendar date (local time) a message was received; today when the Date header was missing.
pub fn received_date(received: f64) -> NaiveDate {
    // outside 2000..2100 the calendar arithmetic below could overflow, so treat it as unknown
    if !(946_684_800.0..4_102_444_800.0).contains(&received) {
        return Local::now().date_naive();
    }
    Local
        .timestamp_opt(received as i64, 0)
        .single()
        .map(|t| t.date_naive())
        .unwrap_or_else(|| Local::now().date_naive())
}

fn clean(token: &str) -> String {
    token
        .trim_matches(|c: char| !c.is_alphanumeric() && c != '-')
        .to_string()
}

fn day_number(t: &str) -> Option<u32> {
    let digits: String = t.chars().take_while(char::is_ascii_digit).collect();
    let rest = &t[digits.len()..];
    if digits.is_empty() || digits.len() > 2 || !matches!(rest, "" | "st" | "nd" | "rd" | "th") {
        return None;
    }
    digits.parse().ok().filter(|d| (1..=31).contains(d))
}

fn year_number(t: &str) -> Option<i32> {
    (t.len() == 4 && t.chars().all(|c| c.is_ascii_digit()))
        .then(|| t.parse().ok())
        .flatten()
        .filter(|y| (2000..=2100).contains(y))
}

fn month_number(t: &str) -> Option<u32> {
    MONTHS.iter().find(|(n, _)| *n == t).map(|(_, m)| *m)
}

/// A date written without a year means the next occurrence, unless it was only a few weeks ago (then it is overdue).
fn with_year(day: u32, month: u32, year: Option<i32>, today: NaiveDate) -> Option<NaiveDate> {
    if let Some(y) = year {
        return NaiveDate::from_ymd_opt(y, month, day);
    }
    let this = NaiveDate::from_ymd_opt(today.year(), month, day)?;
    if this >= today - Duration::days(45) {
        Some(this)
    } else {
        NaiveDate::from_ymd_opt(today.year() + 1, month, day)
    }
}

fn next_weekday(from: NaiveDate, wd: Weekday) -> NaiveDate {
    let mut d = from;
    while d.weekday() != wd {
        d += Duration::days(1);
    }
    d
}

/// The first cued date in `text`, as `YYYY-MM-DD`, or None.
pub fn extract_due(text: &str, received: NaiveDate) -> Option<String> {
    let lower = text.to_lowercase();
    let tokens: Vec<String> = lower
        .split_whitespace()
        .map(clean)
        .filter(|t| !t.is_empty())
        .collect();
    for i in 0..tokens.len() {
        let cued = tokens[i.saturating_sub(3)..i]
            .iter()
            .any(|t| CUES.contains(&t.as_str()));
        if !cued {
            continue;
        }
        let t = tokens[i].as_str();
        let next = |k: usize| tokens.get(i + k).map(String::as_str);
        let date = if let Some(d) = parse_iso(t) {
            Some(d)
        } else if let Some(day) = day_number(t) {
            match next(1).and_then(month_number) {
                Some(m) => with_year(day, m, next(2).and_then(year_number), received),
                None => None,
            }
        } else if let Some(m) = month_number(t) {
            match next(1).and_then(day_number) {
                Some(day) => with_year(day, m, next(2).and_then(year_number), received),
                None => None,
            }
        } else if matches!(t, "today" | "tonight" | "eod" | "cob") {
            Some(received)
        } else if t == "tomorrow" {
            Some(received + Duration::days(1))
        } else {
            WEEKDAYS
                .iter()
                .find(|(n, _)| *n == t)
                .map(|(_, wd)| next_weekday(received, *wd))
        };
        if let Some(d) = date.filter(|d| (2000..=2100).contains(&d.year())) {
            return Some(d.format("%Y-%m-%d").to_string());
        }
    }
    None
}
