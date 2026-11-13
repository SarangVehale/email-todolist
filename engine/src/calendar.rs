//! Calendar invitations found in mail, and the small `.ics` file the app can save from one. Everything in an invitation
//! is attacker-controlled, so it is read defensively (size limits, one event, no nesting, plain-text fields only) and the
//! file is BUILT by the app from those fields, never copied from the message. Design: docs/designs/phase5.md.
use chrono::{Duration, Local, NaiveDate, NaiveDateTime, NaiveTime, Offset, TimeZone};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};

/// A calendar part larger than this is ignored.
pub const MAX_ICS_BYTES: usize = 64 * 1024;
const MAX_TEXT_CHARS: usize = 200;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub summary: String,
    /// `YYYYMMDD` (all day), `YYYYMMDDTHHMMSS` (local or in `tzid`) or `YYYYMMDDTHHMMSSZ` (UTC).
    pub start: String,
    /// Same shapes as `start`, of the same kind; empty when unknown.
    pub end: String,
    /// A plain zone name such as `Europe/Berlin`; empty for UTC, all-day and floating times.
    pub tzid: String,
    pub location: String,
    pub all_day: bool,
    pub cancelled: bool,
    /// The time zone the invitation named when it is not one the app can pass on (so the time is floating): shown so the
    /// reader knows the hour may be somebody else's. Empty otherwise.
    #[serde(default)]
    pub zone_name: String,
    /// The invitation repeats. Only its first time is kept.
    #[serde(default)]
    pub recurring: bool,
}

/// Control characters, line and paragraph separators, zero-width and bidirectional marks: nothing a calendar field or a
/// file name has any use for, and some of them end a line for programs that read the file (U+2028) or reorder what a reader
/// sees (U+202E).
fn unsafe_char(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            '\u{2028}' | '\u{2029}'
                | '\u{200b}'..='\u{200f}'
                | '\u{202a}'..='\u{202e}'
                | '\u{2060}'..='\u{2064}'
                | '\u{2066}'..='\u{2069}'
                | '\u{feff}'
        )
}

/// Text from a calendar field: nothing from `unsafe_char`, trimmed, cut to a reasonable length.
fn clean(s: &str) -> String {
    s.chars()
        .filter(|c| !unsafe_char(*c))
        .collect::<String>()
        .trim()
        .chars()
        .take(MAX_TEXT_CHARS)
        .collect()
}

/// RFC 5545 TEXT unescaping. A line break becomes a space: the app shows one line.
fn unescape(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c == '\\' {
            match it.next() {
                Some('n') | Some('N') => out.push(' '),
                Some(o) => out.push(o),
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// A zone name safe to pass on: `UTC`, `GMT`, or an `Area/Place` name from the IANA list's areas (`Europe/Berlin`,
/// `America/Argentina/Buenos_Aires`), made of letters, digits and `_ + -` between slashes, at most 64 characters. A
/// Windows name such as "W. Europe Standard Time" is not one, and the time is then written as a floating local time
/// rather than in a wrong zone.
fn plain_zone(z: &str) -> bool {
    const AREAS: [&str; 11] = [
        "Africa",
        "America",
        "Antarctica",
        "Arctic",
        "Asia",
        "Atlantic",
        "Australia",
        "Europe",
        "Indian",
        "Pacific",
        "Etc",
    ];
    if matches!(z, "UTC" | "GMT") {
        return true;
    }
    let mut parts = z.split('/');
    let area = parts.next().unwrap_or("");
    let places: Vec<&str> = parts.collect();
    z.len() <= 64
        && AREAS.contains(&area)
        && !places.is_empty()
        && places.iter().all(|p| {
            !p.is_empty()
                && !p.starts_with('-')
                && p.chars()
                    .all(|c| c.is_ascii_alphanumeric() || "_+-".contains(c))
        })
}

/// Zone names Outlook and Exchange write (Windows names), with the IANA name each stands for. A name that is not here is
/// kept for display only and the time is then a floating local time. Source: the Unicode CLDR `windowsZones` list, the
/// territory-001 entry of each zone.
const WINDOWS_ZONES: &[(&str, &str)] = &[
    ("Dateline Standard Time", "Etc/GMT+12"),
    ("UTC-11", "Etc/GMT+11"),
    ("Hawaiian Standard Time", "Pacific/Honolulu"),
    ("Alaskan Standard Time", "America/Anchorage"),
    ("Pacific Standard Time", "America/Los_Angeles"),
    ("Pacific Standard Time (Mexico)", "America/Tijuana"),
    ("US Mountain Standard Time", "America/Phoenix"),
    ("Mountain Standard Time", "America/Denver"),
    ("Mountain Standard Time (Mexico)", "America/Chihuahua"),
    ("Central America Standard Time", "America/Guatemala"),
    ("Central Standard Time", "America/Chicago"),
    ("Central Standard Time (Mexico)", "America/Mexico_City"),
    ("Mexico Standard Time", "America/Mexico_City"),
    ("Canada Central Standard Time", "America/Regina"),
    ("Eastern Standard Time", "America/New_York"),
    ("US Eastern Standard Time", "America/Indianapolis"),
    ("SA Pacific Standard Time", "America/Bogota"),
    ("Atlantic Standard Time", "America/Halifax"),
    ("Venezuela Standard Time", "America/Caracas"),
    ("SA Western Standard Time", "America/La_Paz"),
    ("Pacific SA Standard Time", "America/Santiago"),
    ("Newfoundland Standard Time", "America/St_Johns"),
    ("E. South America Standard Time", "America/Sao_Paulo"),
    ("Argentina Standard Time", "America/Argentina/Buenos_Aires"),
    ("SA Eastern Standard Time", "America/Cayenne"),
    ("Montevideo Standard Time", "America/Montevideo"),
    ("Azores Standard Time", "Atlantic/Azores"),
    ("Cape Verde Standard Time", "Atlantic/Cape_Verde"),
    ("UTC", "UTC"),
    ("GMT Standard Time", "Europe/London"),
    ("Greenwich Standard Time", "Atlantic/Reykjavik"),
    ("W. Europe Standard Time", "Europe/Berlin"),
    ("Central Europe Standard Time", "Europe/Budapest"),
    ("Romance Standard Time", "Europe/Paris"),
    ("Central European Standard Time", "Europe/Warsaw"),
    ("W. Central Africa Standard Time", "Africa/Lagos"),
    ("GTB Standard Time", "Europe/Bucharest"),
    ("Middle East Standard Time", "Asia/Beirut"),
    ("Egypt Standard Time", "Africa/Cairo"),
    ("South Africa Standard Time", "Africa/Johannesburg"),
    ("FLE Standard Time", "Europe/Kiev"),
    ("Israel Standard Time", "Asia/Jerusalem"),
    ("Turkey Standard Time", "Europe/Istanbul"),
    ("Arabic Standard Time", "Asia/Baghdad"),
    ("Arab Standard Time", "Asia/Riyadh"),
    ("E. Africa Standard Time", "Africa/Nairobi"),
    ("Iran Standard Time", "Asia/Tehran"),
    ("Arabian Standard Time", "Asia/Dubai"),
    ("Russian Standard Time", "Europe/Moscow"),
    ("Pakistan Standard Time", "Asia/Karachi"),
    ("India Standard Time", "Asia/Kolkata"),
    ("Sri Lanka Standard Time", "Asia/Colombo"),
    ("Nepal Standard Time", "Asia/Kathmandu"),
    ("Bangladesh Standard Time", "Asia/Dhaka"),
    ("SE Asia Standard Time", "Asia/Bangkok"),
    ("China Standard Time", "Asia/Shanghai"),
    ("Singapore Standard Time", "Asia/Singapore"),
    ("W. Australia Standard Time", "Australia/Perth"),
    ("Taipei Standard Time", "Asia/Taipei"),
    ("Tokyo Standard Time", "Asia/Tokyo"),
    ("Korea Standard Time", "Asia/Seoul"),
    ("Cen. Australia Standard Time", "Australia/Adelaide"),
    ("AUS Central Standard Time", "Australia/Darwin"),
    ("E. Australia Standard Time", "Australia/Brisbane"),
    ("AUS Eastern Standard Time", "Australia/Sydney"),
    ("Tasmania Standard Time", "Australia/Hobart"),
    ("New Zealand Standard Time", "Pacific/Auckland"),
    ("Fiji Standard Time", "Pacific/Fiji"),
];

/// The zone of a start or end time: the IANA name to write (when the name is one, or a known Windows name), and for any
/// other name the cleaned text, kept only so the app can say the time's zone is unknown.
fn resolve_zone(raw: &str) -> (String, String) {
    if plain_zone(raw) {
        return (raw.to_string(), String::new());
    }
    if let Some((_, iana)) = WINDOWS_ZONES
        .iter()
        .find(|(w, _)| w.eq_ignore_ascii_case(raw))
    {
        return (iana.to_string(), String::new());
    }
    (String::new(), clean(raw).chars().take(64).collect())
}

/// An ISO 8601 duration (`PT2H`, `P1DT2H30M`, `P1W`) as a length of time; None for anything else, for zero, and for more
/// than a year. An all-day event takes whole days only.
fn duration_of(v: &str, all_day: bool) -> Option<Duration> {
    let rest = v.strip_prefix('P')?;
    let (date_part, time_part) = match rest.split_once('T') {
        Some((d, t)) => (d, Some(t)),
        None => (rest, None),
    };
    let mut secs: i64 = 0;
    let mut add = |part: &str, units: &[(char, i64)]| -> Option<()> {
        let mut digits = String::new();
        for c in part.chars() {
            if c.is_ascii_digit() {
                digits.push(c);
                continue;
            }
            let (_, mult) = units.iter().find(|(u, _)| *u == c)?;
            let n: i64 = digits.parse().ok()?;
            digits.clear();
            secs = secs.checked_add(n.checked_mul(*mult)?)?;
        }
        digits.is_empty().then_some(())
    };
    add(date_part, &[('W', 7 * 86_400), ('D', 86_400)])?;
    if let Some(t) = time_part {
        if all_day || t.is_empty() {
            return None;
        }
        add(t, &[('H', 3_600), ('M', 60), ('S', 1)])?;
    }
    (1..=366 * 86_400)
        .contains(&secs)
        .then(|| Duration::seconds(secs))
}

/// The value `start` plus `d`, in the same shape and kind, if it is still a valid value.
fn plus(start: &str, d: Duration, all_day: bool, utc: bool) -> Option<String> {
    let t = as_datetime(start)?.checked_add_signed(d)?;
    let text = if all_day {
        t.format("%Y%m%d").to_string()
    } else {
        format!(
            "{}{}",
            t.format("%Y%m%dT%H%M%S"),
            if utc { "Z" } else { "" }
        )
    };
    value(&text).map(|(n, _, _)| n)
}

/// A date or date-time value: (normalised text, date only, UTC).
fn value(v: &str) -> Option<(String, bool, bool)> {
    let b = v.as_bytes();
    let digits =
        |r: std::ops::Range<usize>| b.get(r).is_some_and(|s| s.iter().all(u8::is_ascii_digit));
    let num = |r: std::ops::Range<usize>| v.get(r).and_then(|s| s.parse::<u32>().ok());
    let date = || -> Option<NaiveDate> {
        let y = v.get(0..4)?.parse::<i32>().ok()?;
        if !(2000..=2100).contains(&y) {
            return None;
        }
        NaiveDate::from_ymd_opt(y, num(4..6)?, num(6..8)?)
    };
    match v.len() {
        8 if digits(0..8) => date().map(|_| (v.to_string(), true, false)),
        15 | 16 if digits(0..8) && b[8] == b'T' && digits(9..15) => {
            let utc = v.len() == 16;
            if utc && b[15] != b'Z' {
                return None;
            }
            date()?;
            NaiveTime::from_hms_opt(num(9..11)?, num(11..13)?, num(13..15)?)?;
            Some((v.to_string(), false, utc))
        }
        _ => None,
    }
}

fn as_datetime(v: &str) -> Option<NaiveDateTime> {
    let (norm, date_only, _) = value(v)?;
    if date_only {
        NaiveDate::parse_from_str(&norm, "%Y%m%d")
            .ok()?
            .and_hms_opt(0, 0, 0)
    } else {
        NaiveDateTime::parse_from_str(norm.trim_end_matches('Z'), "%Y%m%dT%H%M%S").ok()
    }
}

/// The first colon that is not inside double quotes (a parameter value may be quoted and may contain one).
fn split_name_value(line: &str) -> Option<(&str, &str)> {
    let mut quoted = false;
    for (i, c) in line.char_indices() {
        match c {
            '"' => quoted = !quoted,
            ':' if !quoted => return Some((&line[..i], &line[i + 1..])),
            _ => {}
        }
    }
    None
}

fn param<'a>(params: &'a str, key: &str) -> Option<&'a str> {
    params.split(';').skip(1).find_map(|p| {
        let (k, v) = p.split_once('=')?;
        k.eq_ignore_ascii_case(key).then(|| v.trim_matches('"'))
    })
}

/// The first event of an iCalendar text, or None when there is none, it is a reply, or anything about its start is wrong.
pub fn parse(text: &str) -> Option<Event> {
    if text.len() > MAX_ICS_BYTES {
        return None;
    }
    // unfold: a line that starts with a space or a tab continues the one before it
    let mut lines: Vec<String> = vec![];
    for raw in text.split('\n') {
        let raw = raw.strip_suffix('\r').unwrap_or(raw);
        match (raw.strip_prefix([' ', '\t']), lines.last_mut()) {
            (Some(rest), Some(last)) => last.push_str(rest),
            _ => lines.push(raw.to_string()),
        }
    }
    let (mut in_cal, mut method, mut depth_skip) = (false, String::new(), 0usize);
    let mut in_event = false;
    let mut done = false;
    let (mut summary, mut start, mut end, mut location, mut status) =
        (None, None, None, None, None);
    let (mut duration, mut recurring) = (None, false);
    for line in &lines {
        let Some((head, val)) = split_name_value(line) else {
            continue;
        };
        let name = head
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_uppercase();
        match name.as_str() {
            "BEGIN" => {
                let kind = val.trim().to_ascii_uppercase();
                match kind.as_str() {
                    "VCALENDAR" => in_cal = true,
                    "VEVENT" if in_cal && !done && !in_event => in_event = true,
                    _ if in_event || depth_skip > 0 => depth_skip += 1, // an alarm or anything nested: skipped
                    _ => {}
                }
            }
            "END" => {
                let kind = val.trim().to_ascii_uppercase();
                if depth_skip > 0 {
                    depth_skip -= 1;
                } else if kind == "VEVENT" && in_event {
                    in_event = false;
                    done = true;
                }
            }
            "METHOD" if in_cal && !in_event && depth_skip == 0 => {
                method = val.trim().to_ascii_uppercase()
            }
            "SUMMARY" if in_event && depth_skip == 0 && summary.is_none() => {
                summary = Some(unescape(val))
            }
            "LOCATION" if in_event && depth_skip == 0 && location.is_none() => {
                location = Some(unescape(val))
            }
            "STATUS" if in_event && depth_skip == 0 && status.is_none() => {
                status = Some(val.trim().to_ascii_uppercase())
            }
            "DTSTART" if in_event && depth_skip == 0 && start.is_none() => {
                start = Some((head.to_string(), val.trim().to_string()))
            }
            "DTEND" if in_event && depth_skip == 0 && end.is_none() => {
                end = Some((head.to_string(), val.trim().to_string()))
            }
            "DURATION" if in_event && depth_skip == 0 && duration.is_none() => {
                duration = Some(val.trim().to_string())
            }
            "RRULE" | "RDATE" if in_event && depth_skip == 0 => recurring = true,
            _ => {}
        }
    }
    if method == "REPLY" || !done {
        return None;
    }
    let (head, v) = start?;
    let (start, all_day, utc) = value(&v)?;
    let start_zone = param(&head, "TZID").map(str::to_ascii_lowercase);
    // an end in another zone cannot be put in the start's zone without a time-zone database, so it is dropped
    let mut end = end
        .and_then(|(eh, ev)| {
            let (e, d, u) = value(&ev)?;
            let same_zone = param(&eh, "TZID").map(str::to_ascii_lowercase) == start_zone;
            (d == all_day && u == utc && same_zone && as_datetime(&e) >= as_datetime(&start))
                .then_some(e)
        })
        .unwrap_or_default();
    if end.is_empty() {
        if let Some(d) = duration.as_deref().and_then(|v| duration_of(v, all_day)) {
            end = plus(&start, d, all_day, utc).unwrap_or_default();
        }
    }
    let (tzid, zone_name) = match param(&head, "TZID") {
        Some(z) if !utc && !all_day => resolve_zone(z),
        _ => (String::new(), String::new()),
    };
    let summary = clean(&summary.unwrap_or_default());
    Some(Event {
        summary: if summary.is_empty() {
            "(no title)".into()
        } else {
            summary
        },
        start,
        end,
        tzid,
        location: clean(&location.unwrap_or_default()),
        all_day,
        cancelled: method == "CANCEL" || status.as_deref() == Some("CANCELLED"),
        zone_name,
        recurring,
    })
}

/// The event's day as `YYYY-MM-DD` for someone `offset_secs` ahead of UTC. A UTC time falls on the day it is where
/// that person is (23:30Z on the 14th is already the 15th in India); an all-day, floating or zoned time is the day it was
/// written, wherever the reader is.
pub fn day_at_offset(e: &Event, offset_secs: i32) -> Option<String> {
    let d = if e.start.ends_with('Z') {
        as_datetime(&e.start)?
            .checked_add_signed(Duration::seconds(offset_secs.into()))?
            .date()
    } else {
        NaiveDate::parse_from_str(e.start.get(0..8)?, "%Y%m%d").ok()?
    };
    Some(d.format("%Y-%m-%d").to_string())
}

/// The event's day on this computer: see `day_at_offset`.
pub fn day(e: &Event) -> Option<String> {
    let offset = if e.start.ends_with('Z') {
        as_datetime(&e.start)
            .map(|t| Local.offset_from_utc_datetime(&t).fix().local_minus_utc())
            .unwrap_or(0)
    } else {
        0
    };
    day_at_offset(e, offset)
}

/// The event's day when it is still ahead (today counts) and the event is not cancelled; what a task's due date may
/// come from.
pub fn upcoming_day(e: &Event, today: &str) -> Option<String> {
    if e.cancelled {
        return None;
    }
    day(e).filter(|d| d.as_str() >= today)
}

// ───────────────────────── the file ─────────────────────────
/// RFC 5545 TEXT escaping. Control characters are dropped and a line break can only become the two characters `\n`, so a
/// value can never start a new property.
fn escape(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            ';' => out.push_str("\\;"),
            ',' => out.push_str("\\,"),
            '\n' | '\r' => out.push_str("\\n"),
            c if unsafe_char(c) => {}
            c => out.push(c),
        }
    }
    out
}

/// Folds a content line at 75 octets, never inside a character; continuation lines start with one space.
fn fold(line: &str) -> String {
    let mut out = String::new();
    let (mut used, mut first) = (0usize, true);
    for c in line.chars() {
        let limit = if first { 75 } else { 74 };
        if used + c.len_utf8() > limit {
            out.push_str("\r\n ");
            used = 0;
            first = false;
        }
        out.push(c);
        used += c.len_utf8();
    }
    out
}

/// A stable id for the event: saving the same message twice must not make two events in a calendar.
fn unique_id(seed: &str, e: &Event) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in seed
        .bytes()
        .chain([0])
        .chain(e.summary.bytes())
        .chain([0])
        .chain(e.start.bytes())
    {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}@inbox-agent")
}

fn time_property(name: &str, v: &str, e: &Event) -> String {
    if e.all_day {
        format!("{name};VALUE=DATE:{v}")
    } else if !e.tzid.is_empty() && plain_zone(&e.tzid) {
        format!("{name};TZID={}:{v}", e.tzid)
    } else {
        format!("{name}:{v}")
    }
}

/// The `.ics` text for an event, built from its fields. `stamp` is the current UTC time; `seed` identifies the message.
pub fn to_ics(e: &Event, stamp: NaiveDateTime, seed: &str) -> String {
    let mut lines = vec![
        "BEGIN:VCALENDAR".to_string(),
        "VERSION:2.0".into(),
        "PRODID:-//Inbox Agent//Event from an email//EN".into(),
        "CALSCALE:GREGORIAN".into(),
        "METHOD:PUBLISH".into(),
        "BEGIN:VEVENT".into(),
        format!("UID:{}", unique_id(seed, e)),
        format!("DTSTAMP:{}Z", stamp.format("%Y%m%dT%H%M%S")),
        time_property("DTSTART", &e.start, e),
    ];
    let end = if !e.end.is_empty() {
        Some(e.end.clone())
    } else if e.all_day {
        // an all-day event with no end lasts that one day; the end date is the day after
        NaiveDate::parse_from_str(&e.start, "%Y%m%d")
            .ok()
            .map(|d| (d + Duration::days(1)).format("%Y%m%d").to_string())
    } else {
        None
    };
    if let Some(end) = end {
        lines.push(time_property("DTEND", &end, e));
    }
    lines.push(format!("SUMMARY:{}", escape(&e.summary)));
    if !e.location.is_empty() {
        lines.push(format!("LOCATION:{}", escape(&e.location)));
    }
    lines.push("END:VEVENT".into());
    lines.push("END:VCALENDAR".into());
    lines.iter().map(|l| fold(l) + "\r\n").collect()
}

/// A file name that is plain ASCII whatever the summary says: `invite-<words>-<date>.ics`.
pub fn file_name(e: &Event) -> String {
    let mut slug = String::new();
    for c in e.summary.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
        } else if !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
    }
    let slug: String = slug.trim_matches('-').chars().take(40).collect();
    let slug = slug.trim_matches('-');
    let date: String = e
        .start
        .chars()
        .take(8)
        .filter(char::is_ascii_digit)
        .collect();
    format!(
        "invite-{}-{}.ics",
        if slug.is_empty() { "event" } else { slug },
        date
    )
}

/// Writes the event as a NEW file in `dir`: an existing file is never replaced (the name gets `-2`, `-3`...), and the name
/// can never leave the folder. Returns the path written.
pub fn save_to(
    dir: &Path,
    e: &Event,
    stamp: NaiveDateTime,
    seed: &str,
) -> std::io::Result<PathBuf> {
    let text = to_ics(e, stamp, seed);
    let name = file_name(e);
    let stem = name.trim_end_matches(".ics");
    for n in 1..=99 {
        let candidate = if n == 1 {
            name.clone()
        } else {
            format!("{stem}-{n}.ics")
        };
        let path = dir.join(candidate);
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut f) => {
                f.write_all(text.as_bytes())?;
                return Ok(path);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "there are already 99 files with this name in the folder",
    ))
}

/// The Downloads folder under `home`, or `home` itself when there is none; None when `home` is not a folder.
pub fn downloads_dir_in(home: Option<&Path>) -> Option<PathBuf> {
    let home = home?;
    let downloads = home.join("Downloads");
    if downloads.is_dir() {
        Some(downloads)
    } else if home.is_dir() {
        Some(home.to_path_buf())
    } else {
        None
    }
}

/// The user's Downloads folder.
pub fn downloads_dir() -> Option<PathBuf> {
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
    downloads_dir_in(Some(Path::new(&home)))
}
