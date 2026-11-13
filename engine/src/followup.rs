//! Follow-ups: conversations you started that nobody has answered. Read-only: it looks at what is in Sent and asks the
//! server (a search, never a download of mail) whether anything replied. Design: docs/designs/phase4.md.
use crate::signals::{SentIndex, SentMsg};
use crate::store::Store;
use crate::watcher::find_special;
use anyhow::{anyhow, Result};
use imap::Session;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};

/// A sent message older than this is not looked at: too old to still be worth nagging about.
pub const WINDOW_DAYS: f64 = 14.0;
/// At most this many sent messages are asked about in one pass.
const PER_PASS: usize = 10;
/// No more than this many follow-ups are open at once. More than that is a list nobody reads; the rest wait until
/// some are closed.
pub const MAX_OPEN: usize = 20;
/// How many of your own addresses are put in one search command.
const MAX_OWN_IN_SEARCH: usize = 5;
const DAY: f64 = 86400.0;

/// The waits offered in Settings; 0 is off.
pub fn valid_days(d: u32) -> bool {
    matches!(d, 0 | 2 | 3 | 5 | 7)
}

/// What one pass did.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct FollowReport {
    /// Tasks made for messages nobody answered.
    pub created: usize,
    /// Sent messages that turned out to have a reply (remembered, so they are not searched again).
    pub answered: usize,
    /// Open follow-ups closed because a reply arrived.
    pub closed: usize,
}

/// Your own addresses, lower-case: the login name when it is an address, and every address your sent mail came from.
pub fn own_addresses(sent: &SentIndex, login: &str) -> Vec<String> {
    let mut own: Vec<String> = sent.from_addrs.iter().cloned().collect();
    let login = login.trim().to_lowercase();
    if crate::mail::valid_address(&login) && !own.contains(&login) {
        own.push(login);
    }
    own.sort();
    own
}

/// The sent messages worth asking about, newest first: conversations you started, to someone other than yourself, sent
/// between `WINDOW_DAYS` and `days` days ago, and not looked at before. A `Date` in the future is too recent to count.
pub fn candidates(
    sent: &SentIndex,
    own: &[String],
    days: u32,
    now: f64,
    store: &Store,
) -> Vec<SentMsg> {
    if days == 0 {
        return vec![];
    }
    let mut out: Vec<SentMsg> = sent
        .started
        .iter()
        .filter_map(|m| {
            let age = now - m.sent_at;
            if age < f64::from(days) * DAY || age > WINDOW_DAYS * DAY {
                return None;
            }
            if store.followup_state(&m.message_id).is_some() {
                return None;
            }
            let to = m.recipients.iter().find(|r| !own.contains(r))?.clone();
            Some(SentMsg { to, ..m.clone() })
        })
        .collect();
    out.sort_by(|a, b| b.sent_at.total_cmp(&a.sent_at));
    out
}

/// INBOX, and the server's "All Mail" and "Archive" folders when it has them (a reply you filed away lives there).
fn search_folders<T: Read + Write>(s: &mut Session<T>) -> Vec<String> {
    let mut folders = vec!["INBOX".to_string()];
    for attr in ["\\All", "\\Archive"] {
        if let Some(f) = find_special(s, attr) {
            if !folders.contains(&f) {
                folders.push(f);
            }
        }
    }
    folders
}

/// Did anyone else reply to this message? Mail from your own addresses (a nudge you sent in the same thread, which
/// Gmail's All Mail also holds) and automatic replies (`Auto-Submitted`: out-of-office notices) are not answers. Any
/// server error is an error, never "no": an answer we could not get must not become a false follow-up.
fn has_reply<T: Read + Write>(
    s: &mut Session<T>,
    folders: &[String],
    id: &str,
    own: &[String],
) -> Result<bool> {
    // the id goes into a command: only a clean `<token>` of visible ASCII with no quote or backslash is ever sent
    if crate::mail::clean_message_id(id) != id {
        return Err(anyhow!("a message id that is not safe to search for"));
    }
    let not_from: String = own
        .iter()
        .filter(|a| crate::mail::valid_address(a))
        .take(MAX_OWN_IN_SEARCH)
        .map(|a| format!("NOT FROM \"{a}\" "))
        .collect();
    let query = format!(
        "{not_from}NOT HEADER Auto-Submitted \"auto-\" OR HEADER In-Reply-To \"{id}\" HEADER References \"{id}\""
    );
    for f in folders {
        s.examine(f)?;
        if !s.uid_search(&query)?.is_empty() {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Closes follow-ups whose reply has arrived, and looks at new candidates. Does nothing (and sends nothing) when
/// `days` is 0, and stops as soon as `stop` is set (the app is signing out: nothing may be written into a database that
/// has just been wiped). An error means "unknown": whatever was decided before it stays decided, and the rest is tried
/// again next time.
///
/// Open follow-ups are closed from the mail the app has already read, which needs no round trip to the server: a reply
/// that arrives in your inbox is stored like any other mail.
pub fn check<T: Read + Write>(
    s: &mut Session<T>,
    store: &Store,
    sent: &SentIndex,
    login: &str,
    days: u32,
    now: f64,
    stop: &AtomicBool,
) -> Result<FollowReport> {
    let mut report = FollowReport::default();
    if days == 0 || stop.load(Ordering::Relaxed) {
        return Ok(report);
    }
    let own = own_addresses(sent, login);
    let mut open = 0;
    for (uid, id) in store.open_followups() {
        if store.has_local_reply(&id, &own) {
            if stop.load(Ordering::Relaxed) {
                return Ok(report);
            }
            if store.close_followup(uid)? {
                report.closed += 1;
                continue;
            }
        }
        open += 1;
    }
    let mut room = MAX_OPEN.saturating_sub(open);
    let todo = candidates(sent, &own, days, now, store);
    if todo.is_empty() || room == 0 {
        return Ok(report);
    }
    let mut folders: Option<Vec<String>> = None; // looked up only when the server really has to be asked
    for m in todo.into_iter().take(PER_PASS) {
        if stop.load(Ordering::Relaxed) {
            return Ok(report);
        }
        let answered = store.has_local_reply(&m.message_id, &own) || {
            if folders.is_none() {
                folders = Some(search_folders(s));
            }
            has_reply(
                s,
                folders.as_deref().unwrap_or_default(),
                &m.message_id,
                &own,
            )?
        };
        if stop.load(Ordering::Relaxed) {
            return Ok(report);
        }
        if answered {
            store.mark_followup(&m.message_id, "answered", now)?;
            report.answered += 1;
        } else {
            if room == 0 {
                break;
            }
            if store.add_followup(&m, now)?.is_some() {
                report.created += 1;
                room -= 1;
            }
        }
    }
    Ok(report)
}
