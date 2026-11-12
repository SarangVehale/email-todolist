//! Applies queued user actions to the real mailbox over a separate, writable IMAP connection.
//! Messages are found by Message-ID because an archived message loses its INBOX UID.
//! Gmail: archive = remove \Inbox label, priority = `AI/P#` labels. Other IMAP servers: archive =
//! move to the Archive folder, star = \Flagged, no labels.
use crate::drafts;
use crate::mail::clean_message_id;
use crate::settings::is_gmail;
use crate::store::{OutboxItem, Store};
use crate::watcher::{connect, find_special, Account};
use anyhow::Result;
use imap::Session;
use mailparse::MailHeaderMap;
use std::io::{Read, Write};

fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// IMAP `HEADER` search is a substring match, so every hit is re-checked against its real header.
const MAX_SEARCH_HITS: usize = 50;
const MAX_EXACT_MATCHES: usize = 3;

fn message_id_of(raw: &[u8]) -> String {
    mailparse::parse_headers(raw)
        .ok()
        .and_then(|(h, _)| h.get_first_value("Message-ID"))
        .map(|v| clean_message_id(&v))
        .unwrap_or_default()
}

/// UIDs whose own Message-ID header equals `mid` exactly.
pub fn exact_matches(headers: &[(u32, Vec<u8>)], mid: &str) -> Vec<u32> {
    headers
        .iter()
        .filter(|(_, h)| message_id_of(h) == mid)
        .map(|(u, _)| *u)
        .collect()
}

/// Selects `folder` and returns the UID set of the one message with this Message-ID, if present.
/// Refuses (returns None) for an invalid ID, or when the answer looks ambiguous or forged.
fn locate<T: Read + Write>(s: &mut Session<T>, folder: &str, mid: &str) -> Result<Option<String>> {
    if mid.is_empty() || clean_message_id(mid) != mid {
        return Ok(None);
    }
    s.select(folder)?;
    let mut hits: Vec<u32> = s
        .uid_search(format!("HEADER Message-ID {}", quote(mid)))?
        .into_iter()
        .collect();
    hits.sort_unstable();
    if hits.is_empty() || hits.len() > MAX_SEARCH_HITS {
        return Ok(None);
    }
    let set = hits
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let mut headers = vec![];
    for m in s.uid_fetch(set, "BODY.PEEK[HEADER]")?.iter() {
        if let (Some(uid), Some(h)) = (m.uid, m.header()) {
            headers.push((uid, h.to_vec()));
        }
    }
    let mut exact = exact_matches(&headers, mid);
    exact.sort_unstable();
    if exact.is_empty() || exact.len() > MAX_EXACT_MATCHES {
        return Ok(None);
    }
    Ok(Some(
        exact
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(","),
    ))
}

struct Folders {
    gmail: bool,
    all_mail: String,
    archive: String,
    drafts: String,
}

fn apply<T: Read + Write>(
    s: &mut Session<T>,
    f: &Folders,
    mid: &str,
    op: &str,
    arg: &str,
) -> Result<bool> {
    if f.gmail {
        let Some(set) = locate(s, &f.all_mail, mid)? else {
            return Ok(false);
        };
        match op {
            "archive" => {
                s.uid_store(&set, "-X-GM-LABELS (\\Inbox)")?;
                s.uid_store(&set, "+X-GM-LABELS (\"AI/Done\")")?;
            }
            "unarchive" => {
                s.uid_store(&set, "+X-GM-LABELS (\\Inbox)")?;
                s.uid_store(&set, "-X-GM-LABELS (\"AI/Done\")")?;
            }
            "star" => {
                s.uid_store(&set, "+FLAGS (\\Flagged)")?;
            }
            "unstar" => {
                s.uid_store(&set, "-FLAGS (\\Flagged)")?;
            }
            "category" if valid_ai_label(arg) => {
                s.uid_store(&set, format!("+X-GM-LABELS ({})", quote(arg)))?;
            }
            "label" if valid_ai_label(arg) => {
                let others: Vec<String> = (1..=5)
                    .map(|n| format!("AI/P{n}"))
                    .filter(|l| l != arg)
                    .map(|l| quote(&l))
                    .collect();
                s.uid_store(&set, format!("-X-GM-LABELS ({})", others.join(" ")))?;
                s.uid_store(&set, format!("+X-GM-LABELS ({})", quote(arg)))?;
            }
            _ => {}
        }
        return Ok(true);
    }
    match op {
        "archive" => {
            let Some(set) = locate(s, "INBOX", mid)? else {
                return Ok(false);
            };
            if s.uid_mv(&set, &f.archive).is_err() {
                s.uid_copy(&set, &f.archive)?;
                s.uid_store(&set, "+FLAGS (\\Deleted)")?;
                let _ = s.uid_expunge(&set);
            }
        }
        "unarchive" => {
            let Some(set) = locate(s, &f.archive, mid)? else {
                return Ok(false);
            };
            if s.uid_mv(&set, "INBOX").is_err() {
                s.uid_copy(&set, "INBOX")?;
                s.uid_store(&set, "+FLAGS (\\Deleted)")?;
                let _ = s.uid_expunge(&set);
            }
        }
        "star" | "unstar" => {
            let flag = if op == "star" {
                "+FLAGS (\\Flagged)"
            } else {
                "-FLAGS (\\Flagged)"
            };
            for folder in ["INBOX", f.archive.as_str()] {
                if let Some(set) = locate(s, folder, mid)? {
                    s.uid_store(&set, flag)?;
                    return Ok(true);
                }
            }
            return Ok(false);
        }
        _ => {} // labels do not exist outside Gmail
    }
    Ok(true)
}

/// Processes pending items in order over a fresh connection.
pub fn drain(a: &Account, store: &Store) -> Result<usize> {
    if store.pending(1).is_empty() {
        return Ok(0);
    }
    let mut s = connect(a)?;
    let done = drain_session(&mut s, &a.host, &a.user, store);
    let _ = s.logout();
    done
}

/// Stops at the first failure to keep ordering; gives up on an item after 5 tries so one bad message
/// cannot block the queue forever.
pub fn drain_session<T: Read + Write>(
    s: &mut Session<T>,
    host: &str,
    from: &str,
    store: &Store,
) -> Result<usize> {
    if store.pending(1).is_empty() {
        return Ok(0);
    }
    let gmail = is_gmail(host);
    let f = Folders {
        gmail,
        all_mail: find_special(s, "\\All").unwrap_or_else(|| "[Gmail]/All Mail".into()),
        archive: find_special(s, "\\Archive").unwrap_or_else(|| "Archive".into()),
        drafts: find_special(s, "\\Drafts")
            .unwrap_or_else(|| if gmail { "[Gmail]/Drafts" } else { "Drafts" }.into()),
    };
    let mut done = 0;
    for item in store.pending(50) {
        match run_item(s, &f, store, &item, from) {
            Ok(()) => {
                store.ack(item.id)?;
                done += 1;
            }
            Err(e) => {
                store.fail(item.id)?;
                store.log(&format!("mailbox update “{}” failed: {e}", item.op))?;
                if item.tries + 1 >= 5 {
                    store.log(&format!(
                        "gave up on “{}” for message {}",
                        item.op, item.uid
                    ))?;
                    store.ack(item.id)?;
                }
                break;
            }
        }
    }
    Ok(done)
}

/// Labels this app creates are always `AI/<Name>`; anything else is refused.
fn valid_ai_label(l: &str) -> bool {
    l.strip_prefix("AI/").is_some_and(|n| {
        (1..=24).contains(&n.len()) && n.chars().all(|c| c.is_ascii_alphanumeric())
    })
}

/// Appends the task's reply draft to the Drafts folder, flagged as a draft. Nothing is sent.
fn save_draft<T: Read + Write>(
    s: &mut Session<T>,
    f: &Folders,
    store: &Store,
    uid: u32,
    from: &str,
) -> Result<()> {
    let Some(t) = store.get(uid) else {
        return Ok(());
    };
    let Some(d) = t.draft.as_ref() else {
        return Ok(());
    };
    let msg = drafts::build_message(&drafts::DraftMessage {
        from,
        to: &t.reply_to,
        subject: &t.subject,
        in_reply_to: &t.message_id,
        references: &store.references(uid),
        body: &d.body,
    })?;
    s.append_with_flags(&f.drafts, &msg, &[imap::types::Flag::Draft])?;
    store.mark_draft_saved(uid)?;
    store.log(&format!(
        "saved a reply draft to {} for “{}”",
        f.drafts, t.subject
    ))?;
    Ok(())
}

fn run_item<T: Read + Write>(
    s: &mut Session<T>,
    f: &Folders,
    store: &Store,
    item: &OutboxItem,
    from: &str,
) -> Result<()> {
    if item.op == "draft" {
        return save_draft(s, f, store, item.uid, from);
    }
    if let Some(t) = store.get(item.uid) {
        if t.message_id.is_empty() {
            store.log(&format!(
                "“{}” has no usable Message-ID, so your mailbox could not be updated for {}",
                t.subject, item.op
            ))?;
        } else if !apply(s, f, &t.message_id, &item.op, &item.arg)? {
            store.log(&format!(
                "message “{}” not found for {}",
                t.subject, item.op
            ))?;
        }
    }
    Ok(())
}
