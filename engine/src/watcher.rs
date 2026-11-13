//! IMAP connection, Sent-history indexing, backfill and IDLE loop. Read-only (EXAMINE + BODY.PEEK).
use crate::mail::{self, Email};
use crate::signals::{self, SentIndex};
use crate::store::Store;
use crate::triage::{triage_with_fallback, Triage, Triager};
use anyhow::{anyhow, Result};
use imap::types::NameAttribute;
use imap::Session;
use native_tls::{TlsConnector, TlsStream};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

pub type Sess = Session<TlsStream<TcpStream>>;

/// Messages larger than this are skipped (memory and parse-time DoS).
const MAX_MESSAGE_BYTES: u32 = 5 * 1024 * 1024;
/// How many times one message may be started without finishing (a crash each time) before it is skipped.
const MAX_ATTEMPTS: u32 = 2;

#[derive(Clone)]
pub struct Account {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
}

/// Opens a TLS connection with hard connect and read/write time limits, so a dead network can never hang a thread forever.
pub fn connect(a: &Account) -> Result<Sess> {
    connect_with_socket(a).map(|(s, _)| s)
}

/// Like `connect`, and also returns a handle to the same socket. The IMAP library clears the read time limit after
/// every IDLE, so the sync loop uses this handle to put the 60-second limit back; without it a server that goes silent
/// after an IDLE would hang the thread for good.
pub fn connect_with_socket(a: &Account) -> Result<(Sess, TcpStream)> {
    use std::net::ToSocketAddrs;
    let addr = (a.host.as_str(), a.port)
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| anyhow!("could not find the server {}", a.host))?;
    let tcp = TcpStream::connect_timeout(&addr, Duration::from_secs(15))?;
    tcp.set_read_timeout(Some(Duration::from_secs(60)))?;
    tcp.set_write_timeout(Some(Duration::from_secs(60)))?;
    let socket = tcp.try_clone()?;
    let tls = TlsConnector::builder().build()?;
    let stream = tls
        .connect(&a.host, tcp)
        .map_err(|e| anyhow!("secure connection failed: {e}"))?;
    let mut client = imap::Client::new(stream);
    client.read_greeting()?;
    let session = client
        .login(&a.user, &a.password)
        .map_err(|e| anyhow!("login failed: {}", e.0))?;
    Ok((session, socket))
}

/// Plain-TCP connection for tests against a local mock server. Never used with a real mailbox.
#[doc(hidden)]
pub fn connect_plain(
    addr: std::net::SocketAddr,
    user: &str,
    password: &str,
) -> Result<Session<TcpStream>> {
    let tcp = TcpStream::connect_timeout(&addr, Duration::from_secs(5))?;
    tcp.set_read_timeout(Some(Duration::from_secs(10)))?;
    let mut client = imap::Client::new(tcp);
    client.read_greeting()?;
    client
        .login(user, password)
        .map_err(|e| anyhow!("login failed: {}", e.0))
}

/// Folder carrying an RFC 6154 special-use flag such as `\Sent`, `\All` or `\Archive`.
pub fn find_special<T: Read + Write>(s: &mut Session<T>, attr: &str) -> Option<String> {
    let names = s.list(Some(""), Some("*")).ok()?;
    names
        .iter()
        .find(|n| {
            n.attributes()
                .iter()
                .any(|a| matches!(a, NameAttribute::Custom(c) if c.eq_ignore_ascii_case(attr)))
        })
        .map(|n| n.name().to_string())
}

pub fn find_sent_folder<T: Read + Write>(s: &mut Session<T>) -> Result<String> {
    if let Some(f) = find_special(s, "\\Sent") {
        return Ok(f);
    }
    let names = s.list(Some(""), Some("*"))?;
    let all: Vec<String> = names.iter().map(|n| n.name().to_string()).collect();
    for guess in [
        "[Gmail]/Sent Mail",
        "Sent",
        "Sent Items",
        "Sent Messages",
        "INBOX.Sent",
    ] {
        if all.iter().any(|n| n == guess) {
            return Ok(guess.to_string());
        }
    }
    Err(anyhow!("no Sent folder found among {} folders", all.len()))
}

fn uid_set(uids: &[u32]) -> String {
    uids.iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

pub fn build_sent_index<T: Read + Write>(
    s: &mut Session<T>,
    folder: &str,
    limit: usize,
) -> Result<SentIndex> {
    let mut idx = SentIndex::default();
    let mailbox = s.examine(folder)?;
    idx.uidvalidity = mailbox.uid_validity.unwrap_or(0);
    let mut uids: Vec<u32> = s.uid_search("ALL")?.into_iter().collect();
    uids.sort_unstable();
    idx.last_uid = uids.last().copied().unwrap_or(0);
    let start = uids.len().saturating_sub(limit);
    for chunk in uids[start..].chunks(100) {
        for m in s.uid_fetch(uid_set(chunk), "BODY.PEEK[HEADER]")?.iter() {
            if let Some(h) = m.header() {
                idx.add_headers(h);
            }
        }
    }
    Ok(idx)
}

/// Adds messages that arrived in Sent since the index was built. If the folder was renumbered, it is read again
/// from scratch. Nothing is downloaded when nothing is new.
pub fn refresh_sent_index<T: Read + Write>(
    s: &mut Session<T>,
    folder: &str,
    idx: &mut SentIndex,
) -> Result<()> {
    let mailbox = s.examine(folder)?;
    if idx.uidvalidity != 0 && mailbox.uid_validity.is_some_and(|v| v != idx.uidvalidity) {
        *idx = build_sent_index(s, folder, 500)?;
        return Ok(());
    }
    let last = idx.last_uid;
    let mut fresh: Vec<u32> = s
        .uid_search(format!("UID {}:*", last + 1))?
        .into_iter()
        .filter(|u| *u > last)
        .collect();
    fresh.sort_unstable();
    let start = fresh.len().saturating_sub(500);
    for chunk in fresh[start..].chunks(100) {
        for m in s.uid_fetch(uid_set(chunk), "BODY.PEEK[HEADER]")?.iter() {
            if let Some(h) = m.header() {
                idx.add_headers(h);
            }
        }
    }
    if let Some(max) = fresh.last() {
        idx.last_uid = *max;
    }
    Ok(())
}

/// What one pass of `reconcile_open` changed.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Reconciled {
    /// Tasks closed because a reply was found in Sent.
    pub replied: usize,
    /// Tasks newly marked as no longer in the inbox.
    pub gone: usize,
}

/// Looks at the open mail tasks and compares them with the mailbox:
/// * a reply in Sent (not a forward) closes the task as "replied";
/// * a message that is no longer in INBOX is marked `gone` (it stays open: it may have been archived to deal with
///   later, or filtered; the list shows these in their own fold) and the mark is removed if it comes back.
///
/// Anything unclear counts as "unknown" and changes nothing: a server error, a reply without UIDs, a renumbered
/// inbox, or a whole batch vanishing while the inbox still holds other mail. It never changes the mailbox.
pub fn reconcile_open<T: Read + Write>(
    s: &mut Session<T>,
    store: &Store,
    sent: &SentIndex,
    stop: &AtomicBool,
) -> Result<Reconciled> {
    let mut out = Reconciled::default();
    if stop.load(Ordering::Relaxed) {
        return Ok(out);
    }
    let tasks = store.open_mail_tasks(500);
    let mut still_open = vec![];
    for (uid, id, keep_open, gone) in tasks {
        if !keep_open && sent.replied_to.contains(&id) && store.auto_close(uid, "replied")? {
            out.replied += 1;
        } else {
            still_open.push((uid, gone));
        }
    }
    let Ok(mailbox) = s.examine("INBOX") else {
        return Ok(out);
    };
    // the stored UIDs mean something else after a renumbering; process_new deals with that
    if let (Some(now), Some(then)) = (mailbox.uid_validity, store.stored_uid_validity()) {
        if now != then {
            return Ok(out);
        }
    }
    for chunk in still_open.chunks(200) {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        let uids: Vec<u32> = chunk.iter().map(|(u, _)| *u).collect();
        let Ok(fetched) = s.uid_fetch(uid_set(&uids), "RFC822.SIZE") else {
            return Ok(out); // the server said no: unknown
        };
        let present: std::collections::HashSet<u32> =
            fetched.iter().filter_map(|m| m.uid).collect();
        if !fetched.is_empty() && present.is_empty() {
            return Ok(out); // a reply that names no UIDs says nothing
        }
        if present.is_empty() && mailbox.exists > 0 && uids.len() >= 5 {
            return Ok(out); // a whole batch vanished while the inbox still has mail: do not believe it
        }
        for (uid, was_gone) in chunk {
            let is_gone = !present.contains(uid);
            if is_gone != *was_gone {
                store.set_gone(*uid, is_gone)?;
                if is_gone {
                    out.gone += 1;
                }
            }
        }
    }
    Ok(out)
}

/// Fetches and triages INBOX messages newer than the stored cursor. Returns how many were new.
pub fn process_new<T: Read + Write>(
    s: &mut Session<T>,
    store: &Store,
    triager: &dyn Triager,
    sent: &SentIndex,
    first_run_limit: usize,
    on_new: &mut dyn FnMut(&Email, &Triage),
) -> Result<usize> {
    process_new_until(
        s,
        store,
        triager,
        sent,
        first_run_limit,
        on_new,
        &AtomicBool::new(false),
    )
}

/// Like `process_new`, but gives up promptly once `stop` is set (the app is signing out or restarting), so a
/// sync that was waiting on the network cannot write mail into a database that has just been wiped.
pub fn process_new_until<T: Read + Write>(
    s: &mut Session<T>,
    store: &Store,
    triager: &dyn Triager,
    sent: &SentIndex,
    first_run_limit: usize,
    on_new: &mut dyn FnMut(&Email, &Triage),
    stop: &AtomicBool,
) -> Result<usize> {
    let stopped = || stop.load(Ordering::Relaxed);
    if stopped() {
        return Ok(0);
    }
    let mailbox = s.examine("INBOX")?;
    if let Some(v) = mailbox.uid_validity {
        if store.reconcile_uid_validity(v)? {
            store.log("the mail server renumbered your inbox; starting a fresh sync")?;
        }
    }
    let last = store.last_uid();
    let mut uids: Vec<u32> = if last > 0 {
        s.uid_search(format!("UID {}:*", last + 1))?
            .into_iter()
            .filter(|u| *u > last)
            .collect()
    } else {
        s.uid_search("ALL")?.into_iter().collect()
    };
    uids.sort_unstable();
    if last == 0 {
        let start = uids.len().saturating_sub(first_run_limit);
        uids = uids.split_off(start);
    }
    let mut n = 0;
    for chunk in uids.chunks(20) {
        if stopped() {
            return Ok(n);
        }
        let mut wanted: Vec<u32> = vec![];
        for m in s.uid_fetch(uid_set(chunk), "RFC822.SIZE")?.iter() {
            let Some(uid) = m.uid else { continue };
            if m.size.unwrap_or(u32::MAX) <= MAX_MESSAGE_BYTES {
                wanted.push(uid);
            } else {
                store.log(&format!(
                    "skipped message {uid}: larger than {} MB",
                    MAX_MESSAGE_BYTES >> 20
                ))?;
            }
        }
        if wanted.is_empty() {
            if let Some(max) = chunk.last() {
                store.set_last_uid(store.last_uid().max(*max))?;
            }
            continue;
        }
        for m in s.uid_fetch(uid_set(&wanted), "BODY.PEEK[]")?.iter() {
            let (Some(uid), Some(body)) = (m.uid, m.body()) else {
                continue;
            };
            if stopped() {
                return Ok(n);
            }
            if store.seen(uid) {
                continue;
            }
            // A message that keeps taking the app down must not do it on every start: it gets two tries.
            let tries = store.begin_attempt(uid)?;
            if tries > MAX_ATTEMPTS {
                store.log(&format!(
                    "skipped message {uid}: handling it crashed the app {} times",
                    tries - 1
                ))?;
                store.end_attempt()?;
                continue;
            }
            // A bug triggered by one message (a panic) is contained to that message.
            let handled =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<bool> {
                    let email = match mail::parse(uid, body) {
                        Ok(e) => e,
                        Err(e) => {
                            store.log(&format!("could not parse message {uid}: {e}"))?;
                            return Ok(false);
                        }
                    };
                    let sig = signals::compute(&email, sent);
                    let t = triage_with_fallback(triager, &email, &sig);
                    if stopped() {
                        return Ok(false); // signing out: do not store anything new
                    }
                    let t = store.add(&email, &t, &sig)?; // sender preferences may change the priority
                    on_new(&email, &t);
                    Ok(true)
                }));
            match handled {
                Ok(Ok(stored)) => {
                    if stored {
                        n += 1;
                    }
                }
                // an ordinary error (database busy, disk full) is not a crash of this message: do not count it
                Ok(Err(e)) => {
                    let _ = store.end_attempt();
                    return Err(e);
                }
                Err(_) => {
                    store.log(&format!("skipped message {uid}: processing it crashed"))?;
                }
            }
            store.end_attempt()?;
        }
        if let Some(max) = chunk.last() {
            store.set_last_uid(store.last_uid().max(*max))?;
        }
    }
    Ok(n)
}

/// How often an idle connection looks for replies sent and mail moved elsewhere.
const RECONCILE_EVERY: Duration = Duration::from_secs(120);

/// Brings the Sent index up to date and lets finished tasks close. A failure here is logged and ignored: it must
/// never take down the connection that delivers new mail.
fn maintain<T: Read + Write>(
    s: &mut Session<T>,
    store: &Store,
    folder: &str,
    sent: &mut SentIndex,
    stop: &AtomicBool,
) {
    if let Err(e) = refresh_sent_index(s, folder, sent) {
        let _ = store.log(&format!("could not read your Sent folder this time: {e}"));
        return;
    }
    match reconcile_open(s, store, sent, stop) {
        Ok(r) if r.replied > 0 || r.gone > 0 => {
            let _ = store.log(&format!(
                "checked your mailbox: {} closed because you replied, {} no longer in the inbox",
                r.replied, r.gone
            ));
        }
        Err(e) => {
            let _ = store.log(&format!("could not check for finished mail this time: {e}"));
        }
        _ => {}
    }
}

/// Runs until `stop` is set. Reconnects after errors. IDLE is re-issued every minute so `stop` is
/// honoured promptly.
pub fn run(
    a: &Account,
    store: &Store,
    triager: &dyn Triager,
    first_run_limit: usize,
    stop: &AtomicBool,
    status: &dyn Fn(&str),
    on_new: &mut dyn FnMut(&Email, &Triage),
) {
    while !stop.load(Ordering::Relaxed) {
        status("connecting");
        // a panic anywhere in a connection attempt becomes an ordinary error and goes through the retry path
        let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<()> {
            let (mut s, socket) = connect_with_socket(a)?;
            let folder = find_sent_folder(&mut s)?;
            let mut sent = build_sent_index(&mut s, &folder, 500)?;
            store.log(&format!("connected as {}; Sent folder “{folder}”", a.user))?;
            status("syncing");
            process_new_until(&mut s, store, triager, &sent, first_run_limit, on_new, stop)?;
            maintain(&mut s, store, &folder, &mut sent, stop);
            let mut last_check = std::time::Instant::now();
            status("idle");
            while !stop.load(Ordering::Relaxed) {
                s.examine("INBOX")?; // read-only; IDLE works on an examined mailbox
                let outcome = s.idle()?.wait_with_timeout(Duration::from_secs(60))?;
                // the library cleared the read limit; restore it before anything else is sent or read
                let _ = socket.set_read_timeout(Some(Duration::from_secs(60)));
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                if matches!(outcome, imap::extensions::idle::WaitOutcome::MailboxChanged) {
                    status("syncing");
                    process_new_until(
                        &mut s,
                        store,
                        triager,
                        &sent,
                        first_run_limit,
                        on_new,
                        stop,
                    )?;
                    // a busy mailbox changes all the time: do the extra checks at most every 20 seconds then
                    if last_check.elapsed() >= Duration::from_secs(20) {
                        maintain(&mut s, store, &folder, &mut sent, stop);
                        last_check = std::time::Instant::now();
                    }
                    status("idle");
                } else if last_check.elapsed() >= RECONCILE_EVERY {
                    // nothing arrived, but you may have replied or archived elsewhere
                    maintain(&mut s, store, &folder, &mut sent, stop);
                    last_check = std::time::Instant::now();
                }
            }
            Ok(())
        }));
        let result = attempt.unwrap_or_else(|_| {
            Err(anyhow::anyhow!(
                "internal error, see crash.log in the app's data folder"
            ))
        });
        if let Err(e) = result {
            status(&format!("error: {e}"));
            let _ = store.log(&format!("connection lost ({e}); retrying in 30s"));
            for _ in 0..30 {
                if stop.load(Ordering::Relaxed) {
                    return;
                }
                std::thread::sleep(Duration::from_secs(1));
            }
        }
    }
}
