//! Protocol-level tests: the real watcher and write-back code run against a scripted mock IMAP server
//! (plain TCP on localhost). They check what the client actually sends and how it parses real responses.
use engine::mail;
use engine::signals::SentIndex;
use engine::store::Store;
use engine::triage::RulesTriager;
use engine::watcher::{
    build_sent_index, connect_plain, find_sent_folder, find_special, process_new,
};
use engine::writeback::drain_session;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

// ───────────────────────── mock server ─────────────────────────
#[derive(Clone, Debug)]
struct Msg {
    uid: u32,
    raw: Vec<u8>,
    flags: Vec<String>,
    labels: Vec<String>,
    fake_size: Option<u32>,
}

#[derive(Clone, Default)]
struct Folder {
    name: String,
    attr: String,
    uidvalidity: u32,
    msgs: Vec<Msg>,
}

#[derive(Default)]
struct State {
    folders: Vec<Folder>,
    log: Vec<String>,
    refuse_move: bool,
    refuse_store: bool,
}

struct Server {
    addr: SocketAddr,
    st: Arc<Mutex<State>>,
}

fn raw_mail(headers: &str, body: &str) -> Vec<u8> {
    format!("{}\r\n\r\n{}\r\n", headers.replace('\n', "\r\n"), body).into_bytes()
}

fn msg(uid: u32, raw: Vec<u8>) -> Msg {
    Msg {
        uid,
        raw,
        flags: vec![],
        labels: vec!["\\Inbox".into()],
        fake_size: None,
    }
}

fn folder(name: &str, attr: &str, msgs: Vec<Msg>) -> Folder {
    Folder {
        name: name.into(),
        attr: attr.into(),
        uidvalidity: 111,
        msgs,
    }
}

impl Server {
    fn start(folders: Vec<Folder>) -> Server {
        let st = Arc::new(Mutex::new(State {
            folders,
            ..Default::default()
        }));
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        let s2 = st.clone();
        std::thread::spawn(move || {
            for c in l.incoming().flatten() {
                let s3 = s2.clone();
                std::thread::spawn(move || serve(c, s3));
            }
        });
        Server { addr, st }
    }
    fn log(&self) -> Vec<String> {
        self.st.lock().unwrap().log.clone()
    }
    fn folder(&self, name: &str) -> Folder {
        self.st
            .lock()
            .unwrap()
            .folders
            .iter()
            .find(|f| f.name == name)
            .cloned()
            .unwrap()
    }
    fn session(&self) -> imap::Session<TcpStream> {
        connect_plain(self.addr, "me@example.com", "pw").unwrap()
    }
}

fn unquote(s: &str) -> String {
    s.trim()
        .trim_matches('"')
        .replace("\\\"", "\"")
        .replace("\\\\", "\\")
}

/// Items of a parenthesised list: quoted strings and bare atoms.
fn list_items(s: &str) -> Vec<String> {
    let inner = s
        .split_once('(')
        .and_then(|(_, r)| r.rsplit_once(')'))
        .map(|(i, _)| i)
        .unwrap_or("");
    let (mut out, mut cur, mut q) = (vec![], String::new(), false);
    for c in inner.chars() {
        match c {
            '"' => q = !q,
            ' ' if !q => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            _ => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn uid_set(s: &str) -> Vec<u32> {
    s.split(',').filter_map(|x| x.trim().parse().ok()).collect()
}

fn header_of(raw: &[u8]) -> &[u8] {
    let i = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|i| i + 4)
        .unwrap_or(raw.len());
    &raw[..i]
}

fn serve(stream: TcpStream, st: Arc<Mutex<State>>) {
    let mut w = stream.try_clone().unwrap();
    let mut r = BufReader::new(stream);
    let _ = w.write_all(b"* OK mock IMAP ready\r\n");
    let mut sel: Option<String> = None;
    let mut line = String::new();
    loop {
        line.clear();
        if r.read_line(&mut line).unwrap_or(0) == 0 {
            return;
        }
        let l = line.trim_end().to_string();
        let (tag, rest) = l.split_once(' ').unwrap_or((&l, ""));
        let (cmd, args) = rest.split_once(' ').unwrap_or((rest, ""));
        let cmd = cmd.to_uppercase();
        let mut s = st.lock().unwrap();
        s.log.push(format!("{cmd} {args}").trim().to_string());
        let mut out = String::new();
        match cmd.as_str() {
            "CAPABILITY" => out += "* CAPABILITY IMAP4rev1 IDLE UIDPLUS MOVE X-GM-EXT-1\r\n",
            "LOGIN" => {}
            "LIST" => {
                for f in &s.folders {
                    let attr = if f.attr.is_empty() {
                        String::new()
                    } else {
                        format!(" {}", f.attr)
                    };
                    out += &format!("* LIST (\\HasNoChildren{attr}) \"/\" \"{}\"\r\n", f.name);
                }
            }
            "SELECT" | "EXAMINE" => {
                let name = unquote(args);
                match s.folders.iter().find(|f| f.name == name) {
                    Some(f) => {
                        sel = Some(name);
                        let next = f.msgs.iter().map(|m| m.uid).max().unwrap_or(0) + 1;
                        out += &format!("* FLAGS (\\Seen \\Flagged \\Draft \\Deleted)\r\n* {} EXISTS\r\n* 0 RECENT\r\n* OK [UIDVALIDITY {}] ok\r\n* OK [UIDNEXT {next}] ok\r\n", f.msgs.len(), f.uidvalidity);
                        let _ = w.write_all(out.as_bytes());
                        let mode = if cmd == "EXAMINE" {
                            "READ-ONLY"
                        } else {
                            "READ-WRITE"
                        };
                        let _ = w.write_all(format!("{tag} OK [{mode}] done\r\n").as_bytes());
                        continue;
                    }
                    None => {
                        let _ = w.write_all(format!("{tag} NO no such mailbox\r\n").as_bytes());
                        continue;
                    }
                }
            }
            "UID" => {
                let (sub, a2) = args.split_once(' ').unwrap_or((args, ""));
                let folder_name = sel.clone().unwrap_or_default();
                match sub.to_uppercase().as_str() {
                    "SEARCH" => {
                        let f = s
                            .folders
                            .iter()
                            .find(|f| f.name == folder_name)
                            .cloned()
                            .unwrap_or_default();
                        let hit: Vec<u32> = if a2.eq_ignore_ascii_case("ALL") {
                            f.msgs.iter().map(|m| m.uid).collect()
                        } else if let Some(n) = a2.strip_prefix("UID ") {
                            let n: u32 = n.split(':').next().unwrap_or("0").parse().unwrap_or(0);
                            let mut v: Vec<u32> =
                                f.msgs.iter().map(|m| m.uid).filter(|u| *u >= n).collect();
                            if v.is_empty() {
                                v = f.msgs.iter().map(|m| m.uid).max().into_iter().collect();
                            }
                            v
                        } else if let Some(v) = a2.strip_prefix("HEADER Message-ID ") {
                            let needle = unquote(v).to_lowercase();
                            f.msgs
                                .iter()
                                .filter(|m| {
                                    String::from_utf8_lossy(header_of(&m.raw)).lines().any(|h| {
                                        h.to_lowercase().starts_with("message-id:")
                                            && h.to_lowercase().contains(&needle)
                                    })
                                })
                                .map(|m| m.uid)
                                .collect()
                        } else {
                            vec![]
                        };
                        out += &format!(
                            "* SEARCH{}\r\n",
                            hit.iter().map(|u| format!(" {u}")).collect::<String>()
                        );
                    }
                    "FETCH" => {
                        let (set, items) = a2.split_once(' ').unwrap_or((a2, ""));
                        let f = s
                            .folders
                            .iter()
                            .find(|f| f.name == folder_name)
                            .cloned()
                            .unwrap_or_default();
                        for uid in uid_set(set) {
                            let Some((i, m)) =
                                f.msgs.iter().enumerate().find(|(_, m)| m.uid == uid)
                            else {
                                continue;
                            };
                            let seq = i + 1;
                            let mut part = String::new();
                            if items.contains("RFC822.SIZE") {
                                part = format!(
                                    "RFC822.SIZE {}",
                                    m.fake_size.unwrap_or(m.raw.len() as u32)
                                );
                            }
                            let _ = w.write_all(
                                format!("* {seq} FETCH (UID {uid} FLAGS ({}) ", m.flags.join(" "))
                                    .as_bytes(),
                            );
                            if !part.is_empty() {
                                let _ = w.write_all(format!("{part})\r\n").as_bytes());
                            } else {
                                let (label, bytes) = if items.contains("HEADER") {
                                    ("BODY[HEADER]", header_of(&m.raw).to_vec())
                                } else {
                                    ("BODY[]", m.raw.clone())
                                };
                                let _ = w.write_all(
                                    format!("{label} {{{}}}\r\n", bytes.len()).as_bytes(),
                                );
                                let _ = w.write_all(&bytes);
                                let _ = w.write_all(b")\r\n");
                            }
                        }
                    }
                    "STORE" => {
                        if s.refuse_store {
                            let _ = w.write_all(format!("{tag} NO store refused\r\n").as_bytes());
                            continue;
                        }
                        let (set, spec) = a2.split_once(' ').unwrap_or((a2, ""));
                        let items = list_items(spec);
                        let up = spec.to_uppercase();
                        let add = up.starts_with('+');
                        let labels = up.contains("X-GM-LABELS");
                        if let Some(f) = s.folders.iter_mut().find(|f| f.name == folder_name) {
                            for m in f.msgs.iter_mut().filter(|m| uid_set(set).contains(&m.uid)) {
                                let target = if labels { &mut m.labels } else { &mut m.flags };
                                for it in &items {
                                    if add {
                                        if !target.contains(it) {
                                            target.push(it.clone());
                                        }
                                    } else {
                                        target.retain(|x| x != it);
                                    }
                                }
                            }
                        }
                    }
                    "MOVE" | "COPY" => {
                        let (set, dest) = a2.split_once(' ').unwrap_or((a2, ""));
                        let dest = unquote(dest);
                        if !s.folders.iter().any(|f| f.name == dest) {
                            let _ = w.write_all(
                                format!("{tag} NO [TRYCREATE] no such mailbox\r\n").as_bytes(),
                            );
                            continue;
                        }
                        if sub.eq_ignore_ascii_case("MOVE") && s.refuse_move {
                            let _ = w.write_all(
                                format!("{tag} NO [CANNOT] MOVE not supported\r\n").as_bytes(),
                            );
                            continue;
                        }
                        let moving: Vec<Msg> = s
                            .folders
                            .iter()
                            .find(|f| f.name == folder_name)
                            .map(|f| {
                                f.msgs
                                    .iter()
                                    .filter(|m| uid_set(set).contains(&m.uid))
                                    .cloned()
                                    .collect()
                            })
                            .unwrap_or_default();
                        if sub.eq_ignore_ascii_case("MOVE") {
                            if let Some(f) = s.folders.iter_mut().find(|f| f.name == folder_name) {
                                f.msgs.retain(|m| !uid_set(set).contains(&m.uid));
                            }
                        }
                        if let Some(d) = s.folders.iter_mut().find(|f| f.name == dest) {
                            for mut m in moving {
                                m.uid = d.msgs.iter().map(|x| x.uid).max().unwrap_or(0) + 1;
                                d.msgs.push(m);
                            }
                        }
                    }
                    "EXPUNGE" => {
                        if let Some(f) = s.folders.iter_mut().find(|f| f.name == folder_name) {
                            f.msgs.retain(|m| {
                                !(uid_set(a2).contains(&m.uid)
                                    && m.flags.iter().any(|x| x == "\\Deleted"))
                            });
                        }
                    }
                    _ => {}
                }
            }
            "APPEND" => {
                // tag APPEND "Folder" (\Draft) {len}
                let name = unquote(
                    args.split(" (")
                        .next()
                        .unwrap_or("")
                        .split(" {")
                        .next()
                        .unwrap_or(""),
                );
                let flags = list_items(args);
                let n: usize = args
                    .rsplit_once('{')
                    .and_then(|(_, r)| r.trim_end_matches('}').parse().ok())
                    .unwrap_or(0);
                let _ = w.write_all(b"+ Ready for literal data\r\n");
                let mut buf = vec![0u8; n];
                r.read_exact(&mut buf).unwrap();
                let mut tail = String::new();
                let _ = r.read_line(&mut tail);
                if let Some(f) = s.folders.iter_mut().find(|f| f.name == name) {
                    let uid = f.msgs.iter().map(|m| m.uid).max().unwrap_or(0) + 1;
                    f.msgs.push(Msg {
                        uid,
                        raw: buf,
                        flags,
                        labels: vec![],
                        fake_size: None,
                    });
                }
                let _ = w
                    .write_all(format!("{tag} OK [APPENDUID 1 1] APPEND completed\r\n").as_bytes());
                continue;
            }
            "IDLE" => {
                let _ = w.write_all(b"+ idling\r\n");
                let mut d = String::new();
                let _ = r.read_line(&mut d);
            }
            "LOGOUT" => {
                let _ = w.write_all(format!("* BYE bye\r\n{tag} OK done\r\n").as_bytes());
                return;
            }
            _ => {}
        }
        out += &format!("{tag} OK {cmd} completed\r\n");
        let _ = w.write_all(out.as_bytes());
    }
}

// ───────────────────────── helpers ─────────────────────────
fn inbox_msg(uid: u32, mid: &str, subject: &str) -> Msg {
    msg(uid, raw_mail(&format!("From: Priya <priya@acme.com>\nSubject: {subject}\nMessage-ID: {mid}\nDate: Tue, 10 Nov 2026 09:00:00 +0000"), "Can you sign today?"))
}

fn gmail_folders(inbox: Vec<Msg>, all_mail: Vec<Msg>) -> Vec<Folder> {
    vec![
        folder("INBOX", "", inbox),
        folder("[Gmail]/All Mail", "\\All", all_mail),
        folder("[Gmail]/Sent Mail", "\\Sent", vec![]),
        folder("[Gmail]/Drafts", "\\Drafts", vec![]),
    ]
}

fn task(store: &Store, uid: u32, mid: &str, reply_to: &str) {
    let e = mail::parse(uid, format!("From: Priya <priya@acme.com>\r\nReply-To: {reply_to}\r\nSubject: Q4 contract?\r\nMessage-ID: {mid}\r\nReferences: <r0@acme>\r\n\r\nCan you sign today?\r\n").as_bytes()).unwrap();
    use engine::triage::Triager;
    let s = engine::signals::Signals {
        known: true,
        ..Default::default()
    };
    store
        .add(&e, &RulesTriager.triage(&e, &s).unwrap(), &s)
        .unwrap();
}

// ───────────────────────── tests ─────────────────────────
#[test]
fn special_use_folders_are_found_by_their_flags() {
    let srv = Server::start(gmail_folders(vec![], vec![]));
    let mut s = srv.session();
    assert_eq!(
        find_special(&mut s, "\\All").as_deref(),
        Some("[Gmail]/All Mail")
    );
    assert_eq!(
        find_special(&mut s, "\\Drafts").as_deref(),
        Some("[Gmail]/Drafts")
    );
    assert_eq!(find_sent_folder(&mut s).unwrap(), "[Gmail]/Sent Mail");
}

#[test]
fn the_sent_history_teaches_who_you_write_to() {
    let mut f = gmail_folders(vec![], vec![]);
    f[2].msgs = vec![
        msg(
            1,
            raw_mail(
                "From: me@example.com\nTo: Bob <bob@x.com>\nCc: carol@y.com\nMessage-ID: <s1@me>",
                "hi",
            ),
        ),
        msg(
            2,
            raw_mail(
                "From: me@example.com\nTo: dave@z.com\nMessage-ID: <s2@me>",
                "hi",
            ),
        ),
    ];
    let srv = Server::start(f);
    let mut s = srv.session();
    let idx = build_sent_index(&mut s, "[Gmail]/Sent Mail", 500).unwrap();
    for a in ["bob@x.com", "carol@y.com", "dave@z.com"] {
        assert!(idx.recipients.contains(a), "{a} missing");
    }
    assert!(idx.message_ids.contains("<s1@me>") && idx.message_ids.contains("<s2@me>"));
    assert!(
        srv.log().iter().any(|l| l.contains("EXAMINE"))
            && !srv.log().iter().any(|l| l.starts_with("SELECT")),
        "the sent folder is opened read-only"
    );
}

#[test]
fn first_sync_backfills_the_latest_messages_then_only_new_ones_and_never_modifies_the_mailbox() {
    let inbox: Vec<Msg> = (1..=5)
        .map(|u| inbox_msg(u, &format!("<m{u}@acme>"), &format!("Mail {u}")))
        .collect();
    let srv = Server::start(gmail_folders(inbox, vec![]));
    let store = Store::memory().unwrap();
    let mut s = srv.session();
    let idx = SentIndex::default();
    let mut seen = vec![];
    let n = process_new(&mut s, &store, &RulesTriager, &idx, 3, &mut |e, _| {
        seen.push(e.uid)
    })
    .unwrap();
    assert_eq!(n, 3);
    seen.sort();
    assert_eq!(seen, vec![3, 4, 5], "only the latest three are backfilled");
    assert_eq!(store.last_uid(), 5);
    assert_eq!(
        process_new(&mut s, &store, &RulesTriager, &idx, 3, &mut |_, _| {}).unwrap(),
        0,
        "nothing new on the second pass"
    );
    srv.st.lock().unwrap().folders[0]
        .msgs
        .push(inbox_msg(6, "<m6@acme>", "Mail 6"));
    assert_eq!(
        process_new(&mut s, &store, &RulesTriager, &idx, 3, &mut |_, _| {}).unwrap(),
        1
    );
    assert_eq!(store.all_tasks().unwrap().len(), 4);
    let log = srv.log();
    assert!(
        log.iter().all(|l| !l.starts_with("SELECT")
            && !l.contains("STORE")
            && !l.contains("APPEND")
            && !l.contains("EXPUNGE")),
        "read-only run must not write: {log:?}"
    );
    assert!(
        log.iter().any(|l| l.contains("BODY.PEEK[]")),
        "messages are fetched with PEEK so they stay unread: {log:?}"
    );
    assert!(
        !log.iter()
            .any(|l| l.contains("BODY[]") || l.contains("RFC822 ")),
        "{log:?}"
    );
}

#[test]
fn oversized_and_hostile_messages_are_skipped_without_stopping_the_sync() {
    let mut big = inbox_msg(1, "<big@acme>", "huge attachment");
    big.fake_size = Some(50 * 1024 * 1024);
    let mut nest = String::from(
        "From: a@b.com\nSubject: nest\nContent-Type: multipart/mixed; boundary=b0\n\n",
    );
    for i in 0..300 {
        nest.push_str(&format!(
            "--b{i}\nContent-Type: multipart/mixed; boundary=b{}\n\n",
            i + 1
        ));
    }
    let hostile = msg(2, nest.replace('\n', "\r\n").into_bytes());
    let fine = inbox_msg(3, "<ok@acme>", "a normal message");
    let srv = Server::start(gmail_folders(vec![big, hostile, fine], vec![]));
    let store = Store::memory().unwrap();
    let mut s = srv.session();
    let n = process_new(
        &mut s,
        &store,
        &RulesTriager,
        &SentIndex::default(),
        50,
        &mut |_, _| {},
    )
    .unwrap();
    assert_eq!(n, 1, "only the normal message is stored");
    assert_eq!(
        store.last_uid(),
        3,
        "the cursor still moves past the skipped ones"
    );
    let log: Vec<String> = store.recent_log(20).into_iter().map(|l| l.1).collect();
    assert!(log.iter().any(|l| l.contains("larger than")), "{log:?}");
    assert!(log.iter().any(|l| l.contains("could not parse")), "{log:?}");
}

/// A triager that panics on one subject, standing in for any bug a hostile message could trigger.
struct Boom;
impl engine::triage::Triager for Boom {
    fn triage(
        &self,
        e: &engine::mail::Email,
        s: &engine::signals::Signals,
    ) -> anyhow::Result<engine::triage::Triage> {
        if e.subject.contains("BOOM") {
            panic!("simulated bug on {}", e.subject);
        }
        RulesTriager.triage(e, s)
    }
}

#[test]
fn a_message_that_panics_is_skipped_and_the_rest_of_the_sync_continues() {
    let srv = Server::start(gmail_folders(
        vec![
            inbox_msg(1, "<a@acme>", "first"),
            inbox_msg(2, "<b@acme>", "BOOM"),
            inbox_msg(3, "<c@acme>", "third"),
        ],
        vec![],
    ));
    let store = Store::memory().unwrap();
    let mut s = srv.session();
    let n = process_new(
        &mut s,
        &store,
        &Boom,
        &SentIndex::default(),
        50,
        &mut |_, _| {},
    )
    .expect("a panic inside one message must not abort the sync");
    assert_eq!(n, 2);
    assert!(store.seen(1) && !store.seen(2) && store.seen(3));
    assert_eq!(store.last_uid(), 3, "the cursor moves past the bad message");
    let log: Vec<String> = store.recent_log(20).into_iter().map(|l| l.1).collect();
    assert!(
        log.iter()
            .any(|l| l.contains("message 2") && l.contains("skipped")),
        "{log:?}"
    );
    assert!(store.in_flight().is_none(), "no marker is left behind");
}

#[test]
fn a_message_that_was_in_flight_during_a_crash_is_tried_once_more_then_skipped() {
    let store = Store::memory().unwrap();
    // first attempt starts, then the process dies: the marker stays
    assert_eq!(store.begin_attempt(7).unwrap(), 1);
    // after the restart the same message begins again: second attempt
    assert_eq!(store.begin_attempt(7).unwrap(), 2);
    // a different message resets the count
    assert_eq!(store.begin_attempt(8).unwrap(), 1);
    store.end_attempt().unwrap();
    assert!(store.in_flight().is_none());
    assert_eq!(store.begin_attempt(8).unwrap(), 1);
}

#[test]
fn a_poison_message_that_keeps_killing_the_process_is_dropped_after_two_tries() {
    let srv = Server::start(gmail_folders(
        vec![
            inbox_msg(1, "<a@acme>", "first"),
            inbox_msg(2, "<b@acme>", "killer"),
            inbox_msg(3, "<c@acme>", "third"),
        ],
        vec![],
    ));
    let store = Store::memory().unwrap();
    task(&store, 1, "<a@acme>", "priya@acme.com"); // already handled before the crashes
                                                   // two earlier runs died while handling message 2
    store.begin_attempt(2).unwrap();
    store.begin_attempt(2).unwrap();
    let mut s = srv.session();
    let n = process_new(
        &mut s,
        &store,
        &RulesTriager,
        &SentIndex::default(),
        50,
        &mut |_, _| {},
    )
    .unwrap();
    assert_eq!(
        n, 1,
        "message 3 is stored, message 2 is not retried a third time"
    );
    assert!(store.seen(3) && !store.seen(2));
    let log: Vec<String> = store.recent_log(20).into_iter().map(|l| l.1).collect();
    assert!(
        log.iter()
            .any(|l| l.contains("message 2") && l.contains("crashed")),
        "{log:?}"
    );
}

#[test]
fn a_database_error_is_not_counted_as_a_crash_so_the_message_is_not_lost() {
    // regression: an ordinary store error left the in-flight marker, and the third retry skipped the message for good
    let dir = std::env::temp_dir().join(format!("inbox-dberr-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("agent.db");
    let store = Store::open(&path).unwrap();
    let side = rusqlite::Connection::open(&path).unwrap();
    side.execute_batch("CREATE TRIGGER nope BEFORE INSERT ON tasks WHEN NEW.uid=2 BEGIN SELECT RAISE(ABORT,'disk full'); END;").unwrap();
    let srv = Server::start(gmail_folders(
        vec![
            inbox_msg(1, "<a@acme>", "first"),
            inbox_msg(2, "<b@acme>", "second"),
        ],
        vec![],
    ));
    for round in 1..=4 {
        let mut s = srv.session();
        let r = process_new(
            &mut s,
            &store,
            &RulesTriager,
            &SentIndex::default(),
            50,
            &mut |_, _| {},
        );
        assert!(r.is_err(), "round {round}: the store error is reported");
        assert!(
            store.in_flight().is_none(),
            "round {round}: it does not leave a crash marker"
        );
    }
    side.execute_batch("DROP TRIGGER nope").unwrap();
    let mut s = srv.session();
    process_new(
        &mut s,
        &store,
        &RulesTriager,
        &SentIndex::default(),
        50,
        &mut |_, _| {},
    )
    .unwrap();
    assert!(store.seen(2), "message 2 is stored once the fault clears");
    drop(side);
    drop(store);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_message_with_an_absurd_date_is_still_stored() {
    let weird = msg(
        1,
        raw_mail(
            "From: Priya <priya@acme.com>\nSubject: dated oddly\nMessage-ID: <d@acme>\nDate: Mon, 1 Jan 262143 12:00:00 +1400",
            "please reply by tomorrow",
        ),
    );
    let srv = Server::start(gmail_folders(vec![weird], vec![]));
    let store = Store::memory().unwrap();
    let mut s = srv.session();
    let n = process_new(
        &mut s,
        &store,
        &RulesTriager,
        &SentIndex::default(),
        50,
        &mut |_, _| {},
    )
    .unwrap();
    assert_eq!(n, 1);
    assert!(store.seen(1));
}

#[test]
fn a_stopped_sync_stores_nothing_new() {
    let srv = Server::start(gmail_folders(
        vec![inbox_msg(1, "<a@acme>", "first")],
        vec![],
    ));
    let store = Store::memory().unwrap();
    let mut s = srv.session();
    let stop = std::sync::atomic::AtomicBool::new(true);
    let n = engine::watcher::process_new_until(
        &mut s,
        &store,
        &RulesTriager,
        &SentIndex::default(),
        50,
        &mut |_, _| {},
        &stop,
    )
    .unwrap();
    assert_eq!(n, 0);
    assert!(
        !store.seen(1),
        "signing out must not be followed by a late write"
    );
}

#[test]
fn a_renumbered_mailbox_restarts_the_sync_instead_of_losing_mail() {
    let srv = Server::start(gmail_folders(
        vec![
            inbox_msg(1, "<a@acme>", "one"),
            inbox_msg(2, "<b@acme>", "two"),
        ],
        vec![],
    ));
    let store = Store::memory().unwrap();
    let mut s = srv.session();
    process_new(
        &mut s,
        &store,
        &RulesTriager,
        &SentIndex::default(),
        50,
        &mut |_, _| {},
    )
    .unwrap();
    assert_eq!(store.last_uid(), 2);
    {
        let mut st = srv.st.lock().unwrap();
        st.folders[0].uidvalidity = 222;
        st.folders[0].msgs = vec![inbox_msg(1, "<new@acme>", "after renumbering")];
    }
    let n = process_new(
        &mut s,
        &store,
        &RulesTriager,
        &SentIndex::default(),
        50,
        &mut |_, _| {},
    )
    .unwrap();
    assert_eq!(n, 1, "the new numbering is read from the start");
    let all = store.all_tasks().unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].subject, "after renumbering");
}

#[test]
fn gmail_actions_touch_exactly_the_right_message_and_nothing_else() {
    // uid 1 is a decoy whose Message-ID merely CONTAINS ours (an IMAP substring search returns it too)
    let all_mail = vec![
        inbox_msg(1, "<<a@x>>", "decoy"),
        inbox_msg(2, "<a@x>", "the real one"),
    ];
    let srv = Server::start(gmail_folders(vec![], all_mail));
    let store = Store::memory().unwrap();
    task(&store, 7, "<a@x>", "priya@acme.com");
    store.set_done(7, true).unwrap();
    store.set_priority(7, 5).unwrap(); // label + star
    store.queue(7, "category", "AI/Work").unwrap();
    let mut s = srv.session();
    let n = drain_session(&mut s, "imap.gmail.com", "me@example.com", &store).unwrap();
    assert_eq!(n, 4, "archive, label, star and category are applied");
    assert!(
        store.pending(10).is_empty(),
        "the queue is empty afterwards"
    );
    let am = srv.folder("[Gmail]/All Mail");
    let (decoy, real) = (&am.msgs[0], &am.msgs[1]);
    assert_eq!(
        decoy.labels,
        vec!["\\Inbox".to_string()],
        "the decoy must be untouched: {decoy:?}"
    );
    assert!(decoy.flags.is_empty());
    assert!(
        !real.labels.contains(&"\\Inbox".to_string()),
        "archived: Inbox label removed: {real:?}"
    );
    for l in ["AI/Done", "AI/P5", "AI/Work"] {
        assert!(
            real.labels.contains(&l.to_string()),
            "missing {l}: {real:?}"
        );
    }
    assert!(
        real.flags.contains(&"\\Flagged".to_string()),
        "starred: {real:?}"
    );
    assert!(
        srv.log()
            .iter()
            .all(|l| !(l.contains("STORE 1 ") || l.contains("STORE 1,"))),
        "{:?}",
        srv.log()
    );
}

#[test]
fn a_lookup_with_no_exact_match_changes_nothing() {
    let decoys: Vec<Msg> = (1..=8)
        .map(|u| inbox_msg(u, &format!("<<a@x>{u}>"), "decoy"))
        .collect();
    let srv = Server::start(gmail_folders(vec![], decoys));
    let store = Store::memory().unwrap();
    task(&store, 7, "<a@x>", "priya@acme.com");
    store.set_done(7, true).unwrap();
    let mut s = srv.session();
    drain_session(&mut s, "imap.gmail.com", "me@example.com", &store).unwrap();
    assert!(
        srv.log().iter().all(|l| !l.contains("STORE")),
        "no STORE may be issued: {:?}",
        srv.log()
    );
}

#[test]
fn a_reply_draft_is_appended_to_drafts_flagged_and_threaded_and_nothing_is_sent() {
    let srv = Server::start(gmail_folders(vec![], vec![]));
    let store = Store::memory().unwrap();
    task(&store, 7, "<a@x>", "priya.work@acme.com");
    store
        .set_draft(7, "Hi Priya,\n\nSigned and returned.\n\nBest,\nSam", "user")
        .unwrap();
    store.queue(7, "draft", "").unwrap();
    let mut s = srv.session();
    assert_eq!(
        drain_session(&mut s, "imap.gmail.com", "me@example.com", &store).unwrap(),
        1
    );
    let drafts = srv.folder("[Gmail]/Drafts");
    assert_eq!(drafts.msgs.len(), 1);
    assert!(
        drafts.msgs[0].flags.contains(&"\\Draft".to_string()),
        "flagged as a draft: {:?}",
        drafts.msgs[0].flags
    );
    use mailparse::MailHeaderMap;
    let p = mailparse::parse_mail(&drafts.msgs[0].raw).unwrap();
    assert_eq!(
        p.headers.get_first_value("To").unwrap(),
        "priya.work@acme.com"
    );
    assert_eq!(p.headers.get_first_value("From").unwrap(), "me@example.com");
    assert_eq!(p.headers.get_first_value("In-Reply-To").unwrap(), "<a@x>");
    assert!(p
        .headers
        .get_first_value("References")
        .unwrap()
        .contains("<r0@acme>"));
    assert_eq!(
        p.headers.get_first_value("Subject").unwrap(),
        "Re: Q4 contract?"
    );
    assert!(p
        .get_body()
        .unwrap()
        .replace("\r\n", "\n")
        .contains("Signed and returned."));
    assert!(
        store.get(7).unwrap().draft.unwrap().saved,
        "the task remembers it was saved"
    );
    for f in ["INBOX", "[Gmail]/All Mail", "[Gmail]/Sent Mail"] {
        assert!(
            srv.folder(f).msgs.is_empty(),
            "{f} must not receive the draft"
        );
    }
}

#[test]
fn other_providers_archive_by_moving_and_fall_back_to_copy_and_delete_without_move() {
    for refuse_move in [false, true] {
        let folders = vec![
            folder("INBOX", "", vec![inbox_msg(1, "<a@x>", "mail")]),
            folder("Archive", "\\Archive", vec![]),
        ];
        let srv = Server::start(folders);
        srv.st.lock().unwrap().refuse_move = refuse_move;
        let store = Store::memory().unwrap();
        task(&store, 7, "<a@x>", "priya@acme.com");
        store.set_done(7, true).unwrap();
        let mut s = srv.session();
        drain_session(&mut s, "imap.example.com", "me@example.com", &store).unwrap();
        assert!(
            srv.folder("INBOX").msgs.is_empty(),
            "refuse_move={refuse_move}: left the inbox ({:?})",
            srv.log()
        );
        assert_eq!(
            srv.folder("Archive").msgs.len(),
            1,
            "refuse_move={refuse_move}: arrived in Archive"
        );
        let log = srv.log();
        assert!(log.iter().any(|l| l.contains("MOVE")));
        assert_eq!(
            log.iter().any(|l| l.contains("COPY")),
            refuse_move,
            "fallback used only when MOVE is refused: {log:?}"
        );
    }
}

#[test]
fn a_failing_server_is_retried_a_few_times_then_the_item_is_dropped_and_logged() {
    let srv = Server::start(gmail_folders(vec![], vec![inbox_msg(1, "<a@x>", "mail")]));
    srv.st.lock().unwrap().refuse_store = true;
    let store = Store::memory().unwrap();
    task(&store, 7, "<a@x>", "priya@acme.com");
    store.set_star(7, true).unwrap();
    for round in 1..=5 {
        let mut s = srv.session();
        drain_session(&mut s, "imap.gmail.com", "me@example.com", &store).unwrap();
        let left = store.pending(10).len();
        assert_eq!(left, if round < 5 { 1 } else { 0 }, "after attempt {round}");
    }
    let log: Vec<String> = store.recent_log(30).into_iter().map(|l| l.1).collect();
    assert!(log.iter().any(|l| l.contains("gave up")), "{log:?}");
}

// ───────────────────────── edge cases ─────────────────────────
#[test]
fn a_message_without_a_message_id_is_not_guessed_at() {
    let srv = Server::start(gmail_folders(
        vec![],
        vec![inbox_msg(1, "<a@x>", "other mail")],
    ));
    let store = Store::memory().unwrap();
    task(&store, 7, "", "priya@acme.com"); // no usable Message-ID
    store.set_done(7, true).unwrap();
    let mut s = srv.session();
    drain_session(&mut s, "imap.gmail.com", "me@example.com", &store).unwrap();
    assert!(
        srv.log().iter().all(|l| !l.contains("STORE")),
        "nothing may be modified: {:?}",
        srv.log()
    );
    assert!(
        store.pending(10).is_empty(),
        "the item is not retried forever"
    );
    let log: Vec<String> = store.recent_log(20).into_iter().map(|l| l.1).collect();
    assert!(
        log.iter().any(|l| l.contains("no usable Message-ID")),
        "the user is told why: {log:?}"
    );
}

#[test]
fn duplicate_copies_of_the_same_message_are_all_updated() {
    let srv = Server::start(gmail_folders(
        vec![],
        vec![
            inbox_msg(1, "<a@x>", "copy one"),
            inbox_msg(2, "<a@x>", "copy two"),
            inbox_msg(3, "<b@x>", "unrelated"),
        ],
    ));
    let store = Store::memory().unwrap();
    task(&store, 7, "<a@x>", "priya@acme.com");
    store.set_star(7, true).unwrap();
    let mut s = srv.session();
    drain_session(&mut s, "imap.gmail.com", "me@example.com", &store).unwrap();
    let am = srv.folder("[Gmail]/All Mail");
    assert!(
        am.msgs[0].flags.contains(&"\\Flagged".to_string())
            && am.msgs[1].flags.contains(&"\\Flagged".to_string())
    );
    assert!(
        am.msgs[2].flags.is_empty(),
        "the unrelated message is untouched"
    );
}

#[test]
fn an_empty_inbox_is_fine() {
    let srv = Server::start(gmail_folders(vec![], vec![]));
    let store = Store::memory().unwrap();
    let mut s = srv.session();
    assert_eq!(
        process_new(
            &mut s,
            &store,
            &RulesTriager,
            &SentIndex::default(),
            50,
            &mut |_, _| {}
        )
        .unwrap(),
        0
    );
    assert_eq!(store.last_uid(), 0);
    assert_eq!(
        process_new(
            &mut s,
            &store,
            &RulesTriager,
            &SentIndex::default(),
            50,
            &mut |_, _| {}
        )
        .unwrap(),
        0,
        "and stays fine on repeat"
    );
}

#[test]
fn uids_with_gaps_are_handled_and_the_cursor_tracks_the_highest() {
    let inbox = vec![
        inbox_msg(10, "<a@x>", "a"),
        inbox_msg(20, "<b@x>", "b"),
        inbox_msg(30, "<c@x>", "c"),
    ];
    let srv = Server::start(gmail_folders(inbox, vec![]));
    let store = Store::memory().unwrap();
    let mut s = srv.session();
    let mut seen = vec![];
    process_new(
        &mut s,
        &store,
        &RulesTriager,
        &SentIndex::default(),
        2,
        &mut |e, _| seen.push(e.uid),
    )
    .unwrap();
    seen.sort();
    assert_eq!(seen, vec![20, 30]);
    assert_eq!(store.last_uid(), 30);
    srv.st.lock().unwrap().folders[0]
        .msgs
        .push(inbox_msg(45, "<d@x>", "d"));
    assert_eq!(
        process_new(
            &mut s,
            &store,
            &RulesTriager,
            &SentIndex::default(),
            2,
            &mut |_, _| {}
        )
        .unwrap(),
        1
    );
    assert_eq!(store.last_uid(), 45);
}

#[test]
fn a_missing_archive_folder_never_loses_the_message() {
    let srv = Server::start(vec![folder(
        "INBOX",
        "",
        vec![inbox_msg(1, "<a@x>", "mail")],
    )]); // no Archive folder at all
    let store = Store::memory().unwrap();
    task(&store, 7, "<a@x>", "priya@acme.com");
    store.set_done(7, true).unwrap();
    for _ in 0..5 {
        let mut s = srv.session();
        drain_session(&mut s, "imap.example.com", "me@example.com", &store).unwrap();
    }
    assert_eq!(
        srv.folder("INBOX").msgs.len(),
        1,
        "the message must still be in the inbox"
    );
    assert!(
        store.pending(10).is_empty(),
        "the item is given up on after its retries"
    );
    let log: Vec<String> = store.recent_log(40).into_iter().map(|l| l.1).collect();
    assert!(
        log.iter().any(|l| l.contains("failed")) && log.iter().any(|l| l.contains("gave up")),
        "{log:?}"
    );
    assert!(
        store.get(7).unwrap().status == "done",
        "the local state still reflects what the user did"
    );
}
