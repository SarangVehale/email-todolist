//! Protocol-level tests: the real watcher and write-back code run against a scripted mock IMAP server
//! (plain TCP on localhost). They check what the client actually sends and how it parses real responses.
use engine::mail;
use engine::signals::SentIndex;
use engine::store::Store;
use engine::triage::RulesTriager;
use engine::watcher::{
    build_sent_index, connect_plain, find_sent_folder, find_special, process_new, reconcile_open,
    refresh_sent_index, sent_or_empty, try_find_sent_folder,
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
    refuse_fetch: bool,
    omit_uid: bool,
    refuse_login: bool,
    no_body_for: Option<u32>,
    refuse_search: bool,
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
            "LOGIN" => {
                if s.refuse_login {
                    let _ = w.write_all(
                        format!("{tag} NO [AUTHENTICATIONFAILED] Invalid credentials\r\n")
                            .as_bytes(),
                    );
                    continue;
                }
            }
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
                        if s.refuse_search {
                            let _ = w.write_all(format!("{tag} NO search refused\r\n").as_bytes());
                            continue;
                        }
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
                        } else if let Some(at) = a2.find("OR HEADER In-Reply-To ") {
                            // [NOT FROM "x"]... [NOT HEADER Auto-Submitted "auto-"] OR HEADER In-Reply-To "<id>" HEADER References "<id>"
                            let (front, v) =
                                (&a2[..at], &a2[at + "OR HEADER In-Reply-To ".len()..]);
                            let needle =
                                unquote(v.split(" HEADER References ").next().unwrap_or(""))
                                    .to_lowercase();
                            let not_from: Vec<String> = front
                                .split("NOT FROM ")
                                .skip(1)
                                .map(|x| {
                                    unquote(x.split(" NOT ").next().unwrap_or("")).to_lowercase()
                                })
                                .collect();
                            let no_auto = front.contains("NOT HEADER Auto-Submitted \"auto-\"");
                            f.msgs
                                .iter()
                                .filter(|m| {
                                    let head =
                                        String::from_utf8_lossy(header_of(&m.raw)).to_lowercase();
                                    let lines: Vec<&str> = head.lines().collect();
                                    let from = lines
                                        .iter()
                                        .find(|h| h.starts_with("from:"))
                                        .copied()
                                        .unwrap_or("");
                                    let auto = lines.iter().any(|h| {
                                        h.starts_with("auto-submitted:") && h.contains("auto-")
                                    });
                                    lines.iter().any(|h| {
                                        (h.starts_with("in-reply-to:")
                                            || h.starts_with("references:"))
                                            && h.contains(&needle)
                                    }) && !not_from.iter().any(|a| from.contains(a.as_str()))
                                        && !(no_auto && auto)
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
                        if s.refuse_fetch {
                            let _ = w.write_all(format!("{tag} NO fetch refused\r\n").as_bytes());
                            continue;
                        }
                        let omit_uid = s.omit_uid;
                        let no_body_for = s.no_body_for;
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
                            let uid_item = if omit_uid {
                                String::new()
                            } else {
                                format!("UID {uid} ")
                            };
                            // a server that answers the fetch but sends no content for this message
                            if part.is_empty() && no_body_for == Some(uid) {
                                let _ = w.write_all(
                                    format!(
                                        "* {seq} FETCH ({uid_item}FLAGS ({}))\r\n",
                                        m.flags.join(" ")
                                    )
                                    .as_bytes(),
                                );
                                continue;
                            }
                            let _ = w.write_all(
                                format!("* {seq} FETCH ({uid_item}FLAGS ({}) ", m.flags.join(" "))
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
    assert_eq!(
        try_find_sent_folder(&mut s).unwrap().as_deref(),
        Some("[Gmail]/Sent Mail")
    );
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

// ───────────────── tasks that close themselves ─────────────────
fn with_sent(inbox: Vec<Msg>, sent: Vec<Msg>) -> Vec<Folder> {
    vec![
        folder("INBOX", "", inbox),
        folder("[Gmail]/All Mail", "\\All", vec![]),
        folder("[Gmail]/Sent Mail", "\\Sent", sent),
        folder("[Gmail]/Drafts", "\\Drafts", vec![]),
    ]
}

fn sent_reply(uid: u32, to_id: &str, subject: &str) -> Msg {
    msg(
        uid,
        raw_mail(
            &format!("From: me@example.com\nTo: priya@acme.com\nSubject: {subject}\nMessage-ID: <sent{uid}@me>\nIn-Reply-To: {to_id}\nReferences: {to_id}"),
            "ok",
        ),
    )
}

/// Syncs the inbox into a store and returns it with the Sent index of the same session.
fn synced(srv: &Server) -> (Store, SentIndex) {
    let store = Store::memory().unwrap();
    let mut s = srv.session();
    let folder = find_sent_folder(&mut s).unwrap();
    let idx = build_sent_index(&mut s, &folder, 500).unwrap();
    process_new(&mut s, &store, &RulesTriager, &idx, 50, &mut |_, _| {}).unwrap();
    (store, idx)
}

fn no_stop() -> std::sync::atomic::AtomicBool {
    std::sync::atomic::AtomicBool::new(false)
}

fn set_inbox(srv: &Server, f: impl FnOnce(&mut Folder)) {
    let mut st = srv.st.lock().unwrap();
    f(st.folders.iter_mut().find(|f| f.name == "INBOX").unwrap());
}

#[test]
fn a_task_closes_itself_when_you_replied_and_nothing_is_queued_or_changed() {
    let srv = Server::start(with_sent(
        vec![
            inbox_msg(1, "<a@acme>", "question"),
            inbox_msg(2, "<b@acme>", "other"),
        ],
        vec![sent_reply(1, "<a@acme>", "Re: question")],
    ));
    let (store, idx) = synced(&srv);
    let mut s = srv.session();
    let r = reconcile_open(&mut s, &store, &idx, &no_stop()).unwrap();
    assert_eq!(r.replied, 1);
    let t = store.get(1).unwrap();
    assert_eq!(
        (t.status.as_str(), t.closed_by.as_str()),
        ("done", "replied")
    );
    assert_eq!(store.get(2).unwrap().status, "open");
    assert!(
        store.pending(10).is_empty(),
        "closing because of a reply never queues a mailbox change"
    );
    assert!(
        !srv.log()
            .iter()
            .any(|l| l.contains("STORE") || l.contains("MOVE")),
        "{:?}",
        srv.log()
    );
}

#[test]
fn forwarding_a_message_is_not_replying_to_it() {
    for subject in ["Fwd: question", "FW: question", "fwd:question"] {
        let srv = Server::start(with_sent(
            vec![inbox_msg(1, "<a@acme>", "question")],
            vec![sent_reply(1, "<a@acme>", subject)],
        ));
        let (store, idx) = synced(&srv);
        let mut s = srv.session();
        assert_eq!(
            reconcile_open(&mut s, &store, &idx, &no_stop())
                .unwrap()
                .replied,
            0,
            "{subject}"
        );
        assert_eq!(store.get(1).unwrap().status, "open");
    }
}

#[test]
fn a_reopened_task_stays_open_and_does_not_ask_the_mailbox_to_unarchive() {
    let srv = Server::start(with_sent(
        vec![inbox_msg(1, "<a@acme>", "question")],
        vec![sent_reply(1, "<a@acme>", "Re: question")],
    ));
    let (store, idx) = synced(&srv);
    let mut s = srv.session();
    reconcile_open(&mut s, &store, &idx, &no_stop()).unwrap();
    assert_eq!(store.get(1).unwrap().status, "done");
    store.set_done(1, false).unwrap(); // the user says: this still needs me
    assert_eq!(store.get(1).unwrap().status, "open");
    assert!(
        store.pending(10).is_empty(),
        "reopening an automatic close must not queue an unarchive"
    );
    reconcile_open(&mut s, &store, &idx, &no_stop()).unwrap();
    assert_eq!(
        store.get(1).unwrap().status,
        "open",
        "the next pass does not close it again"
    );
    assert_eq!(store.get(1).unwrap().closed_by, "");
}

#[test]
fn closing_by_hand_still_queues_the_archive_as_before() {
    let srv = Server::start(with_sent(vec![inbox_msg(1, "<a@acme>", "q")], vec![]));
    let (store, _) = synced(&srv);
    store.set_done(1, true).unwrap();
    assert_eq!(store.get(1).unwrap().closed_by, "");
    assert_eq!(store.pending(10).len(), 1);
}

#[test]
fn mail_that_left_the_inbox_is_marked_but_stays_open_and_comes_back_if_it_returns() {
    let srv = Server::start(with_sent(
        vec![
            inbox_msg(1, "<a@acme>", "one"),
            inbox_msg(2, "<b@acme>", "two"),
        ],
        vec![],
    ));
    let (store, idx) = synced(&srv);
    let mut s = srv.session();
    let mut taken = None;
    set_inbox(&srv, |f| taken = Some(f.msgs.remove(0))); // archived somewhere else
    let r = reconcile_open(&mut s, &store, &idx, &no_stop()).unwrap();
    assert_eq!(r.gone, 1);
    let t = store.get(1).unwrap();
    assert!(
        t.gone && t.status == "open",
        "marked, never closed, so nothing is lost"
    );
    assert!(!store.get(2).unwrap().gone);
    set_inbox(&srv, |f| f.msgs.insert(0, taken.take().unwrap())); // moved back
    reconcile_open(&mut s, &store, &idx, &no_stop()).unwrap();
    assert!(!store.get(1).unwrap().gone);
}

#[test]
fn a_server_error_means_unknown_and_nothing_changes() {
    let srv = Server::start(with_sent(vec![inbox_msg(1, "<a@acme>", "one")], vec![]));
    let (store, idx) = synced(&srv);
    srv.st.lock().unwrap().refuse_fetch = true;
    let mut s = srv.session();
    let r = reconcile_open(&mut s, &store, &idx, &no_stop());
    assert!(r.is_ok(), "an error from the server must not stop syncing");
    assert!(!store.get(1).unwrap().gone);
}

#[test]
fn a_reply_without_uids_does_not_read_as_everything_gone() {
    let srv = Server::start(with_sent(
        vec![
            inbox_msg(1, "<a@acme>", "one"),
            inbox_msg(2, "<b@acme>", "two"),
        ],
        vec![],
    ));
    let (store, idx) = synced(&srv);
    srv.st.lock().unwrap().omit_uid = true;
    let mut s = srv.session();
    reconcile_open(&mut s, &store, &idx, &no_stop()).unwrap();
    assert!(!store.get(1).unwrap().gone && !store.get(2).unwrap().gone);
}

#[test]
fn a_renumbered_inbox_changes_nothing_here() {
    let srv = Server::start(with_sent(vec![inbox_msg(1, "<a@acme>", "one")], vec![]));
    let (store, idx) = synced(&srv);
    set_inbox(&srv, |f| f.uidvalidity = 999);
    let mut s = srv.session();
    reconcile_open(&mut s, &store, &idx, &no_stop()).unwrap();
    assert!(
        !store.get(1).unwrap().gone,
        "the old numbers mean nothing now; the sync handles it"
    );
}

#[test]
fn a_large_batch_vanishing_while_the_inbox_is_not_empty_is_treated_as_unknown() {
    let inbox: Vec<Msg> = (1..=6)
        .map(|u| inbox_msg(u, &format!("<m{u}@acme>"), "x"))
        .collect();
    let srv = Server::start(with_sent(inbox, vec![]));
    let (store, idx) = synced(&srv);
    set_inbox(&srv, |f| {
        f.msgs = vec![inbox_msg(100, "<new@acme>", "unrelated")]
    });
    let mut s = srv.session();
    reconcile_open(&mut s, &store, &idx, &no_stop()).unwrap();
    assert!(
        (1..=6).all(|u| !store.get(u).unwrap().gone),
        "a server that answers oddly cannot hide the whole list"
    );
}

#[test]
fn an_inbox_that_really_is_empty_marks_everything_gone_but_closes_nothing() {
    let inbox: Vec<Msg> = (1..=6)
        .map(|u| inbox_msg(u, &format!("<m{u}@acme>"), "x"))
        .collect();
    let srv = Server::start(with_sent(inbox, vec![]));
    let (store, idx) = synced(&srv);
    set_inbox(&srv, |f| f.msgs.clear());
    let mut s = srv.session();
    reconcile_open(&mut s, &store, &idx, &no_stop()).unwrap();
    assert!((1..=6).all(|u| {
        let t = store.get(u).unwrap();
        t.gone && t.status == "open"
    }));
}

#[test]
fn a_stopped_pass_does_nothing() {
    let srv = Server::start(with_sent(
        vec![inbox_msg(1, "<a@acme>", "q")],
        vec![sent_reply(1, "<a@acme>", "Re: q")],
    ));
    let (store, idx) = synced(&srv);
    let mut s = srv.session();
    let stop = std::sync::atomic::AtomicBool::new(true);
    reconcile_open(&mut s, &store, &idx, &stop).unwrap();
    assert_eq!(store.get(1).unwrap().status, "open");
}

#[test]
fn the_sent_index_picks_up_new_replies_without_starting_over() {
    let srv = Server::start(with_sent(
        vec![
            inbox_msg(1, "<a@acme>", "q"),
            inbox_msg(2, "<b@acme>", "q2"),
        ],
        vec![sent_reply(1, "<a@acme>", "Re: q")],
    ));
    let (_store, mut idx) = synced(&srv);
    assert!(idx.replied_to.contains("<a@acme>") && !idx.replied_to.contains("<b@acme>"));
    // the user replies to the second message from their phone
    srv.st
        .lock()
        .unwrap()
        .folders
        .iter_mut()
        .find(|f| f.name == "[Gmail]/Sent Mail")
        .unwrap()
        .msgs
        .push(sent_reply(2, "<b@acme>", "Re: q2"));
    let mut s = srv.session();
    let folder = find_sent_folder(&mut s).unwrap();
    refresh_sent_index(&mut s, &folder, &mut idx).unwrap();
    assert!(idx.replied_to.contains("<b@acme>"));
    let log_before = srv.log().len();
    refresh_sent_index(&mut s, &folder, &mut idx).unwrap();
    let fetches = srv.log()[log_before..]
        .iter()
        .filter(|l| l.contains("FETCH"))
        .count();
    assert_eq!(
        fetches, 0,
        "nothing new in Sent means nothing is downloaded"
    );
}

#[test]
fn a_renumbered_sent_folder_is_read_again_from_scratch() {
    let srv = Server::start(with_sent(
        vec![inbox_msg(1, "<a@acme>", "q")],
        vec![sent_reply(1, "<a@acme>", "Re: q")],
    ));
    let (_, mut idx) = synced(&srv);
    {
        let mut st = srv.st.lock().unwrap();
        let sent = st
            .folders
            .iter_mut()
            .find(|f| f.name == "[Gmail]/Sent Mail")
            .unwrap();
        sent.uidvalidity = 7777;
        sent.msgs = vec![sent_reply(5, "<z@acme>", "Re: z")];
    }
    let mut s = srv.session();
    let folder = find_sent_folder(&mut s).unwrap();
    refresh_sent_index(&mut s, &folder, &mut idx).unwrap();
    assert!(idx.replied_to.contains("<z@acme>"));
}

// ───────── findings from the review council ─────────

/// Folders with no `\Sent` flag and no English name, as a localized server presents them.
fn localized_folders(inbox: Vec<Msg>) -> Vec<Folder> {
    vec![
        folder("INBOX", "", inbox),
        folder("Gesendet", "", vec![]),
        folder("Entwürfe", "", vec![]),
        folder("Papierkorb", "", vec![]),
    ]
}

#[test]
fn a_mailbox_whose_sent_folder_cannot_be_found_still_loads_all_the_mail() {
    // a German, French or custom-named Sent folder with no \Sent flag must not stop the whole sync
    let srv = Server::start(localized_folders(vec![
        inbox_msg(1, "<a@acme>", "one"),
        inbox_msg(2, "<b@acme>", "two"),
    ]));
    let store = Store::memory().unwrap();
    let mut s = srv.session();
    assert!(
        find_sent_folder(&mut s).is_err(),
        "the folder really is unfindable"
    );
    // "Test connection" asks the other way: no Sent folder is an answer (null), not a refusal of the mailbox
    assert_eq!(try_find_sent_folder(&mut s).unwrap(), None);

    let (folder, idx) = sent_or_empty(&mut s, &store);
    assert!(folder.is_none());
    assert!(idx.replied_to.is_empty() && idx.recipients.is_empty());
    let n = process_new(&mut s, &store, &RulesTriager, &idx, 50, &mut |_, _| {}).unwrap();
    assert_eq!(n, 2, "the mail loads anyway");
    let log: Vec<String> = store.recent_log(10).into_iter().map(|l| l.1).collect();
    assert!(
        log.iter().any(|l| l.contains("Sent folder")),
        "and the user is told why replies are not detected: {log:?}"
    );
}

#[test]
fn a_wrong_password_is_told_apart_from_a_server_that_is_simply_down() {
    let srv = Server::start(gmail_folders(vec![], vec![]));
    srv.st.lock().unwrap().refuse_login = true;
    let e = connect_plain(srv.addr, "me@example.com", "wrong").expect_err("login is refused");
    assert!(
        engine::watcher::is_sign_in_problem(&e),
        "a refused password: {e}"
    );

    // nothing listening: an ordinary network problem, not a reason to stop trying
    let dead = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = dead.local_addr().unwrap();
    drop(dead);
    let e = connect_plain(addr, "me@example.com", "pw").expect_err("nothing is listening");
    assert!(!engine::watcher::is_sign_in_problem(&e), "a dead port: {e}");
}

#[test]
fn a_message_the_server_returns_without_a_body_is_retried_then_given_up_on() {
    let srv = Server::start(gmail_folders(
        vec![
            inbox_msg(1, "<a@acme>", "one"),
            inbox_msg(2, "<b@acme>", "two"),
            inbox_msg(3, "<c@acme>", "three"),
        ],
        vec![],
    ));
    srv.st.lock().unwrap().no_body_for = Some(2);
    let store = Store::memory().unwrap();

    // first pass: 1 and 3 are stored, and the cursor does not jump past the message that came back empty
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
    assert!(store.seen(1) && store.seen(3));
    assert!(!store.seen(2));
    assert!(
        store.last_uid() < 2,
        "the cursor waits for it, got {}",
        store.last_uid()
    );

    // the server recovers: the message arrives on the next sync, nothing is lost
    srv.st.lock().unwrap().no_body_for = None;
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
    assert!(
        store.seen(2),
        "the message appears once the server sends it"
    );
    assert_eq!(store.last_uid(), 3);
}

#[test]
fn a_message_that_never_arrives_does_not_block_the_sync_for_ever() {
    let srv = Server::start(gmail_folders(
        vec![
            inbox_msg(1, "<a@acme>", "one"),
            inbox_msg(2, "<b@acme>", "two"),
        ],
        vec![],
    ));
    srv.st.lock().unwrap().no_body_for = Some(1);
    let store = Store::memory().unwrap();
    for _ in 0..4 {
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
    }
    assert!(store.seen(2));
    assert_eq!(store.last_uid(), 2, "after a few tries the cursor moves on");
    let log: Vec<String> = store.recent_log(20).into_iter().map(|l| l.1).collect();
    assert!(
        log.iter()
            .any(|l| l.contains("message 1") && l.contains("no content")),
        "and it is logged, not silent: {log:?}"
    );
}

// ───────────────────────── follow-ups (Phase 4) ─────────────────────────
mod followups {
    use super::*;
    use engine::followup::check;
    use std::sync::atomic::AtomicBool;

    const DAY: f64 = 86400.0;

    fn now() -> f64 {
        chrono::Utc::now().timestamp() as f64
    }

    /// A message you sent `ago_days` ago that starts a conversation.
    fn sent_msg(uid: u32, id: &str, to: &str, ago_days: f64) -> Msg {
        let date = chrono::DateTime::from_timestamp((now() - ago_days * DAY) as i64, 0)
            .unwrap()
            .to_rfc2822();
        msg(
            uid,
            raw_mail(
                &format!("From: me@example.com\nTo: {to}\nSubject: Quote for {id}\nMessage-ID: {id}\nDate: {date}"),
                "Could you send a quote?",
            ),
        )
    }

    fn reply_msg(uid: u32, to_id: &str) -> Msg {
        msg(
            uid,
            raw_mail(
                &format!("From: Bob <bob@x.com>\nSubject: Re: Quote\nMessage-ID: <reply{uid}@x>\nIn-Reply-To: {to_id}\nReferences: {to_id}\nDate: Tue, 10 Nov 2026 09:00:00 +0000"),
                "Here it is.",
            ),
        )
    }

    fn server(sent: Vec<Msg>, inbox: Vec<Msg>, all_mail: Vec<Msg>) -> Server {
        let mut f = gmail_folders(inbox, all_mail);
        f[2].msgs = sent;
        Server::start(f)
    }

    fn index(srv: &Server) -> SentIndex {
        build_sent_index(&mut srv.session(), "[Gmail]/Sent Mail", 500).unwrap()
    }

    fn searches(srv: &Server) -> Vec<String> {
        srv.log()
            .into_iter()
            .filter(|l| l.contains("SEARCH") && l.contains("OR HEADER In-Reply-To"))
            .collect()
    }

    #[test]
    fn an_unanswered_conversation_you_started_becomes_a_task_and_nothing_is_written() {
        let srv = server(
            vec![sent_msg(1, "<s1@me>", "Bob <bob@x.com>", 5.0)],
            vec![inbox_msg(1, "<m1@acme>", "unrelated")],
            vec![],
        );
        let (store, idx) = (Store::memory().unwrap(), index(&srv));
        let r = check(
            &mut srv.session(),
            &store,
            &idx,
            "me@example.com",
            3,
            now(),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!((r.created, r.answered, r.closed), (1, 0, 0));
        let t = store.all_tasks().unwrap();
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].kind, "followup");
        assert!(t[0].excerpt.contains("bob@x.com"), "{}", t[0].excerpt);
        let s = searches(&srv);
        assert_eq!(s.len(), 2, "INBOX and All Mail are both asked: {s:?}");
        assert!(
            s[0].contains("\"<s1@me>\" HEADER References \"<s1@me>\""),
            "the id is sent quoted: {s:?}"
        );
        assert!(
            s[0].contains("NOT FROM \"me@example.com\"")
                && s[0].contains("NOT HEADER Auto-Submitted \"auto-\""),
            "your own mail and automatic replies are left out of the search: {s:?}"
        );
        let log = srv.log();
        assert!(
            log.iter().all(|l| !l.starts_with("SELECT")
                && !l.contains("STORE")
                && !l.contains("APPEND")
                && !l.contains("COPY")
                && !l.contains("MOVE")
                && !l.contains("EXPUNGE")),
            "a follow-up check only reads: {log:?}"
        );
    }

    #[test]
    fn a_conversation_that_was_answered_makes_no_task_and_is_not_searched_again() {
        let srv = server(
            vec![sent_msg(1, "<s1@me>", "bob@x.com", 5.0)],
            vec![reply_msg(1, "<s1@me>")],
            vec![],
        );
        let (store, idx) = (Store::memory().unwrap(), index(&srv));
        let r = check(
            &mut srv.session(),
            &store,
            &idx,
            "me@example.com",
            3,
            now(),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!((r.created, r.answered), (0, 1));
        assert!(store.all_tasks().unwrap().is_empty());
        assert_eq!(store.followup_state("<s1@me>").as_deref(), Some("answered"));
        let before = searches(&srv).len();
        check(
            &mut srv.session(),
            &store,
            &idx,
            "me@example.com",
            3,
            now(),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(
            searches(&srv).len(),
            before,
            "an answered message is never searched twice"
        );
    }

    #[test]
    fn a_reply_you_archived_is_found_in_all_mail() {
        let srv = server(
            vec![sent_msg(1, "<s1@me>", "bob@x.com", 5.0)],
            vec![],
            vec![reply_msg(9, "<s1@me>")],
        );
        let (store, idx) = (Store::memory().unwrap(), index(&srv));
        let r = check(
            &mut srv.session(),
            &store,
            &idx,
            "me@example.com",
            3,
            now(),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!((r.created, r.answered), (0, 1));
    }

    #[test]
    fn a_server_that_cannot_search_is_unknown_never_a_false_followup() {
        let srv = server(
            vec![sent_msg(1, "<s1@me>", "bob@x.com", 5.0)],
            vec![],
            vec![],
        );
        let (store, idx) = (Store::memory().unwrap(), index(&srv));
        srv.st.lock().unwrap().refuse_search = true;
        assert!(check(
            &mut srv.session(),
            &store,
            &idx,
            "me@example.com",
            3,
            now(),
            &AtomicBool::new(false)
        )
        .is_err());
        assert!(store.all_tasks().unwrap().is_empty());
        assert!(
            store.followup_state("<s1@me>").is_none(),
            "nothing is remembered, so it is tried again"
        );
        srv.st.lock().unwrap().refuse_search = false;
        let r = check(
            &mut srv.session(),
            &store,
            &idx,
            "me@example.com",
            3,
            now(),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(
            r.created, 1,
            "the next pass, with a working server, decides"
        );
    }

    #[test]
    fn an_open_followup_closes_itself_when_the_reply_is_read_and_asks_the_server_nothing() {
        let srv = server(
            vec![sent_msg(1, "<s1@me>", "bob@x.com", 5.0)],
            vec![],
            vec![],
        );
        let (store, idx) = (Store::memory().unwrap(), index(&srv));
        check(
            &mut srv.session(),
            &store,
            &idx,
            "me@example.com",
            3,
            now(),
            &AtomicBool::new(false),
        )
        .unwrap();
        let uid = store.open_followups()[0].0;
        // the reply arrives in the inbox and is read like any other new mail
        srv.st.lock().unwrap().folders[0]
            .msgs
            .push(reply_msg(5, "<s1@me>"));
        let mut s = srv.session();
        process_new(&mut s, &store, &RulesTriager, &idx, 50, &mut |_, _| {}).unwrap();
        let before = searches(&srv).len();
        let r = check(
            &mut s,
            &store,
            &idx,
            "me@example.com",
            3,
            now(),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(r.closed, 1);
        assert_eq!(searches(&srv).len(), before, "closing needs no search");
        let t = store.get(uid).unwrap();
        assert_eq!(
            (t.status.as_str(), t.closed_by.as_str()),
            ("done", "replied")
        );
        assert_eq!(store.followup_state("<s1@me>").as_deref(), Some("answered"));
    }

    #[test]
    fn off_means_no_commands_at_all() {
        let srv = server(
            vec![sent_msg(1, "<s1@me>", "bob@x.com", 5.0)],
            vec![],
            vec![],
        );
        let (store, idx) = (Store::memory().unwrap(), index(&srv));
        let mut s = srv.session();
        let before = srv.log().len();
        let r = check(
            &mut s,
            &store,
            &idx,
            "me@example.com",
            0,
            now(),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!((r.created, r.answered, r.closed), (0, 0, 0));
        assert_eq!(srv.log().len(), before, "nothing was sent to the server");
    }

    #[test]
    fn turning_it_off_stops_even_the_checks_on_follow_ups_that_are_already_open() {
        let srv = server(
            vec![sent_msg(1, "<s1@me>", "bob@x.com", 5.0)],
            vec![],
            vec![],
        );
        let (store, idx) = (Store::memory().unwrap(), index(&srv));
        check(
            &mut srv.session(),
            &store,
            &idx,
            "me@example.com",
            3,
            now(),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(store.open_followups().len(), 1, "set-up: one is open");
        srv.st.lock().unwrap().folders[0]
            .msgs
            .push(reply_msg(5, "<s1@me>"));
        let mut s = srv.session();
        let before = srv.log().len();
        let r = check(
            &mut s,
            &store,
            &idx,
            "me@example.com",
            0,
            now(),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!((r.created, r.answered, r.closed), (0, 0, 0));
        assert_eq!(srv.log().len(), before, "off means no commands at all");
        assert_eq!(store.open_followups().len(), 1, "and nothing was closed");
    }

    #[test]
    fn at_most_ten_messages_are_checked_in_one_pass_and_the_rest_next_time() {
        let sent: Vec<Msg> = (1..=12)
            .map(|u| sent_msg(u, &format!("<s{u}@me>"), "bob@x.com", 5.0))
            .collect();
        let srv = server(sent, vec![], vec![]);
        let (store, idx) = (Store::memory().unwrap(), index(&srv));
        let r = check(
            &mut srv.session(),
            &store,
            &idx,
            "me@example.com",
            3,
            now(),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(r.created, 10);
        let r = check(
            &mut srv.session(),
            &store,
            &idx,
            "me@example.com",
            3,
            now(),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(r.created, 2);
        assert_eq!(store.all_tasks().unwrap().len(), 12);
    }

    #[test]
    fn mail_to_yourself_and_replies_you_sent_are_not_searched_for() {
        let mut reply = sent_msg(2, "<s2@me>", "bob@x.com", 5.0);
        reply.raw = raw_mail(
            "From: me@example.com\nTo: bob@x.com\nSubject: Re: hi\nMessage-ID: <s2@me>\nIn-Reply-To: <q@x>\nDate: Mon, 09 Nov 2026 09:00:00 +0000",
            "ok",
        );
        let srv = server(
            vec![sent_msg(1, "<s1@me>", "me@example.com", 5.0), reply],
            vec![],
            vec![],
        );
        let (store, idx) = (Store::memory().unwrap(), index(&srv));
        let r = check(
            &mut srv.session(),
            &store,
            &idx,
            "me@example.com",
            3,
            now(),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!((r.created, r.answered), (0, 0));
        assert!(searches(&srv).is_empty(), "no candidate, no search");
    }

    fn raw_from(uid: u32, from: &str, extra: &str, to_id: &str) -> Msg {
        msg(
            uid,
            raw_mail(
                &format!("From: {from}\nSubject: Re: Quote\nMessage-ID: <x{uid}@x>\nIn-Reply-To: {to_id}\nReferences: {to_id}\nDate: Tue, 10 Nov 2026 09:00:00 +0000{extra}"),
                "text",
            ),
        )
    }

    #[test]
    fn your_own_nudge_in_all_mail_is_not_a_reply() {
        let srv = server(
            vec![sent_msg(1, "<s1@me>", "bob@x.com", 5.0)],
            vec![],
            vec![raw_from(9, "me@example.com", "", "<s1@me>")],
        );
        let (store, idx) = (Store::memory().unwrap(), index(&srv));
        let r = check(
            &mut srv.session(),
            &store,
            &idx,
            "me@example.com",
            3,
            now(),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(
            (r.created, r.answered),
            (1, 0),
            "Gmail's All Mail holds your own mail too"
        );
    }

    #[test]
    fn an_out_of_office_reply_is_not_a_reply() {
        let srv = server(
            vec![sent_msg(1, "<s1@me>", "bob@x.com", 5.0)],
            vec![raw_from(
                4,
                "Bob <bob@x.com>",
                "\nAuto-Submitted: auto-replied",
                "<s1@me>",
            )],
            vec![],
        );
        let (store, idx) = (Store::memory().unwrap(), index(&srv));
        let r = check(
            &mut srv.session(),
            &store,
            &idx,
            "me@example.com",
            3,
            now(),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!((r.created, r.answered), (1, 0));
    }

    #[test]
    fn a_reply_filed_in_an_archive_folder_is_found_on_servers_without_all_mail() {
        let mut sent = folder(
            "Sent",
            "\\Sent",
            vec![sent_msg(1, "<s1@me>", "bob@x.com", 5.0)],
        );
        sent.uidvalidity = 7;
        let srv = Server::start(vec![
            folder("INBOX", "", vec![]),
            sent,
            folder("Archive", "\\Archive", vec![reply_msg(3, "<s1@me>")]),
        ]);
        let (store, idx) = (
            Store::memory().unwrap(),
            build_sent_index(&mut srv.session(), "Sent", 500).unwrap(),
        );
        let r = check(
            &mut srv.session(),
            &store,
            &idx,
            "me@example.com",
            3,
            now(),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!((r.created, r.answered), (0, 1));
    }

    #[test]
    fn a_reply_the_app_already_read_is_found_without_asking_the_server() {
        let srv = server(
            vec![sent_msg(1, "<s1@me>", "bob@x.com", 5.0)],
            vec![],
            vec![],
        );
        let (store, idx) = (Store::memory().unwrap(), index(&srv));
        // the reply was read earlier and the user has since pressed X on it (it may now live in an Archive folder)
        let e = mail::parse(
            7,
            b"From: Bob <bob@x.com>\r\nSubject: Re: Quote\r\nMessage-ID: <r7@x>\r\nIn-Reply-To: <s1@me>\r\nReferences: <s1@me>\r\n\r\nhere\r\n",
        )
        .unwrap();
        let sig = engine::signals::Signals::default();
        use engine::triage::Triager;
        store
            .add(&e, &RulesTriager.triage(&e, &sig).unwrap(), &sig)
            .unwrap();
        store.set_done(7, true).unwrap();
        let before = searches(&srv).len();
        let r = check(
            &mut srv.session(),
            &store,
            &idx,
            "me@example.com",
            3,
            now(),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!((r.created, r.answered), (0, 1));
        assert_eq!(searches(&srv).len(), before, "no search was needed");
    }

    #[test]
    fn check_does_nothing_once_it_is_told_to_stop() {
        let srv = server(
            vec![sent_msg(1, "<s1@me>", "bob@x.com", 5.0)],
            vec![],
            vec![],
        );
        let (store, idx) = (Store::memory().unwrap(), index(&srv));
        let mut s = srv.session();
        let before = srv.log().len();
        let r = check(
            &mut s,
            &store,
            &idx,
            "me@example.com",
            3,
            now(),
            &AtomicBool::new(true),
        )
        .unwrap();
        assert_eq!((r.created, r.answered, r.closed), (0, 0, 0));
        assert_eq!(srv.log().len(), before, "no command was sent");
        assert!(store.all_tasks().unwrap().is_empty() && store.followup_state("<s1@me>").is_none());
    }

    #[test]
    fn check_closes_nothing_once_it_is_told_to_stop() {
        let srv = server(
            vec![sent_msg(1, "<s1@me>", "bob@x.com", 5.0)],
            vec![],
            vec![],
        );
        let (store, idx) = (Store::memory().unwrap(), index(&srv));
        let mut s = srv.session();
        check(
            &mut s,
            &store,
            &idx,
            "me@example.com",
            3,
            now(),
            &AtomicBool::new(false),
        )
        .unwrap();
        srv.st.lock().unwrap().folders[0]
            .msgs
            .push(reply_msg(5, "<s1@me>"));
        process_new(&mut s, &store, &RulesTriager, &idx, 50, &mut |_, _| {}).unwrap();
        let r = check(
            &mut s,
            &store,
            &idx,
            "me@example.com",
            3,
            now(),
            &AtomicBool::new(true),
        )
        .unwrap();
        assert_eq!(r.closed, 0);
        assert_eq!(
            store.open_followups().len(),
            1,
            "a sign-out must not be followed by writes"
        );
        let r = check(
            &mut s,
            &store,
            &idx,
            "me@example.com",
            3,
            now(),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(r.closed, 1);
    }

    #[test]
    fn follow_ups_beyond_the_tenth_still_close_and_no_more_than_twenty_are_open() {
        let sent: Vec<Msg> = (1..=25)
            .map(|u| sent_msg(u, &format!("<s{u}@me>"), "bob@x.com", 5.0))
            .collect();
        let srv = server(sent, vec![], vec![]);
        let (store, idx) = (Store::memory().unwrap(), index(&srv));
        let mut s = srv.session();
        for _ in 0..4 {
            check(
                &mut s,
                &store,
                &idx,
                "me@example.com",
                3,
                now(),
                &AtomicBool::new(false),
            )
            .unwrap();
        }
        assert_eq!(store.open_followups().len(), 20, "the cap");
        let unseen = (1..=25)
            .filter(|u| store.followup_state(&format!("<s{u}@me>")).is_none())
            .count();
        assert_eq!(unseen, 5, "the rest wait until some are closed");
        // replies to the 11th and 15th sent messages are read; both close, whatever their position in the queue
        let inbox: Vec<Msg> = [11u32, 15]
            .iter()
            .map(|n| reply_msg(*n, &format!("<s{n}@me>")))
            .collect();
        srv.st.lock().unwrap().folders[0].msgs = inbox;
        process_new(&mut s, &store, &RulesTriager, &idx, 50, &mut |_, _| {}).unwrap();
        let r = check(
            &mut s,
            &store,
            &idx,
            "me@example.com",
            3,
            now(),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(r.closed, 2);
    }
}

// ───────────────────────── several mailboxes (Phase 6) ─────────────────────────
mod mailboxes {
    use super::*;
    use engine::store::{global_uid, MAILBOX_SPAN};

    fn fresh_db(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("ia-p6-flow-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("agent.db")
    }

    fn inbox(n: u32) -> Vec<Msg> {
        (1..=n)
            .map(|u| inbox_msg(u, &format!("<m{u}@acme>"), &format!("Mail {u}")))
            .collect()
    }

    #[test]
    fn a_sync_for_one_mailbox_numbers_its_tasks_inside_that_mailboxs_block() {
        let srv = Server::start(gmail_folders(inbox(3), vec![]));
        let store = Store::memory().unwrap().scoped(1);
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
        assert_eq!(n, 3);
        for uid in 1..=3 {
            assert!(
                store.get(global_uid(1, uid)).is_some(),
                "message {uid} is task {}",
                global_uid(1, uid)
            );
            assert!(store.get(uid).is_none(), "and not task {uid}");
        }
        assert_eq!(store.last_uid(), 3, "the cursor is this mailbox's own");
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
            "nothing is read twice"
        );
    }

    #[test]
    fn two_servers_with_the_same_message_numbers_feed_one_list() {
        let path = fresh_db("two-servers");
        let (a, b) = (
            Server::start(gmail_folders(inbox(2), vec![])),
            Server::start(gmail_folders(inbox(3), vec![])),
        );
        let (s0, s1) = (
            Store::open(&path).unwrap().scoped(0),
            Store::open(&path).unwrap().scoped(1),
        );
        let (mut c0, mut c1) = (a.session(), b.session());
        let idx = SentIndex::default();
        assert_eq!(
            process_new(&mut c0, &s0, &RulesTriager, &idx, 50, &mut |_, _| {}).unwrap(),
            2
        );
        assert_eq!(
            process_new(&mut c1, &s1, &RulesTriager, &idx, 50, &mut |_, _| {}).unwrap(),
            3
        );
        let all = Store::open(&path).unwrap().all_tasks().unwrap();
        let mut ids: Vec<u32> = all.iter().map(|t| t.uid).collect();
        ids.sort_unstable();
        assert_eq!(
            ids,
            [1, 2, global_uid(1, 1), global_uid(1, 2), global_uid(1, 3)],
            "five different tasks, two of them with the same IMAP number as two others"
        );
        assert_eq!((s0.last_uid(), s1.last_uid()), (2, 3));
        // new mail in one mailbox is read by that mailbox's sync only
        b.st.lock().unwrap().folders[0]
            .msgs
            .push(inbox_msg(4, "<m4@acme>", "Mail 4"));
        assert_eq!(
            process_new(&mut c0, &s0, &RulesTriager, &idx, 50, &mut |_, _| {}).unwrap(),
            0
        );
        assert_eq!(
            process_new(&mut c1, &s1, &RulesTriager, &idx, 50, &mut |_, _| {}).unwrap(),
            1
        );
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_message_number_past_the_id_layout_is_skipped_with_a_log_line_and_the_rest_load() {
        let mut msgs = inbox(2);
        msgs.push(inbox_msg(
            MAILBOX_SPAN + 1,
            "<huge@acme>",
            "numbered too high",
        ));
        let srv = Server::start(gmail_folders(msgs, vec![]));
        let store = Store::memory().unwrap().scoped(2);
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
        assert_eq!(n, 2, "the two that fit");
        let log: Vec<String> = store.recent_log(20).into_iter().map(|l| l.1).collect();
        assert!(
            log.iter()
                .any(|l| l.contains("too large to track") || l.contains("too large for this app")),
            "{log:?}"
        );
        assert!(
            store.last_uid() > MAILBOX_SPAN,
            "the cursor moves on, so it is not asked for again"
        );
        assert_eq!(store.all_tasks().unwrap().len(), 2);
    }

    #[test]
    fn write_back_of_one_mailbox_reaches_only_its_own_server_and_leaves_the_others_queue() {
        let path = fresh_db("writeback");
        let (s0, s1) = (
            Store::open(&path).unwrap().scoped(0),
            Store::open(&path).unwrap().scoped(1),
        );
        task(&s0, 7, "<a@x>", "priya@acme.com");
        task(&s1, 7, "<b@x>", "priya@acme.com");
        let all = Store::open(&path).unwrap();
        all.set_done(7, true).unwrap();
        all.set_done(global_uid(1, 7), true).unwrap();
        assert_eq!(all.pending(10).len(), 2);
        // the second mailbox's server holds only its own message
        let srv = Server::start(gmail_folders(
            vec![],
            vec![inbox_msg(2, "<b@x>", "mailbox two")],
        ));
        let mut s = srv.session();
        let n = drain_session(&mut s, "imap.gmail.com", "me@example.com", &s1).unwrap();
        assert_eq!(n, 1, "only its own item");
        let log = srv.log().join("\n");
        assert!(log.contains("<b@x>"), "{log}");
        assert!(
            !log.contains("<a@x>"),
            "the other mailbox's message is never asked about: {log}"
        );
        let left = all.pending(10);
        assert_eq!(left.len(), 1);
        assert_eq!(
            left[0].uid, 7,
            "the first mailbox's change is still waiting for its own server"
        );
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn replies_and_departures_are_judged_inside_the_mailbox_they_belong_to() {
        let path = fresh_db("reconcile");
        let (s0, s1) = (
            Store::open(&path).unwrap().scoped(0),
            Store::open(&path).unwrap().scoped(1),
        );
        task(&s0, 7, "<same@x>", "priya@acme.com");
        task(&s1, 7, "<same@x>", "priya@acme.com");
        // mailbox 1's server still has message 7 in its inbox; mailbox 0's server has nothing
        let srv = Server::start(gmail_folders(
            vec![inbox_msg(7, "<same@x>", "kept")],
            vec![],
        ));
        let mut s = srv.session();
        let idx = SentIndex::default();
        engine::watcher::reconcile_open(
            &mut s,
            &s1,
            &idx,
            &std::sync::atomic::AtomicBool::new(false),
        )
        .unwrap();
        let all = Store::open(&path).unwrap();
        assert!(
            !all.get(global_uid(1, 7)).unwrap().gone,
            "present in its own mailbox"
        );
        assert!(
            !all.get(7).unwrap().gone,
            "the first mailbox's task was not looked at"
        );
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_reply_you_sent_from_one_mailbox_closes_that_mailboxs_task_and_not_the_other() {
        let path = fresh_db("replied");
        let (s0, s1) = (
            Store::open(&path).unwrap().scoped(0),
            Store::open(&path).unwrap().scoped(1),
        );
        task(&s0, 7, "<same@x>", "priya@acme.com");
        task(&s1, 7, "<same@x>", "priya@acme.com");
        // the second mailbox's Sent folder shows an answer to <same@x>; the first mailbox's does not
        let mut sent = SentIndex::default();
        sent.replied_to.insert("<same@x>".to_string());
        let srv = Server::start(gmail_folders(
            vec![inbox_msg(7, "<same@x>", "kept")],
            vec![],
        ));
        let mut sess = srv.session();
        let out = engine::watcher::reconcile_open(
            &mut sess,
            &s1,
            &sent,
            &std::sync::atomic::AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(out.replied, 1);
        let all = Store::open(&path).unwrap();
        let (mine, other) = (all.get(global_uid(1, 7)).unwrap(), all.get(7).unwrap());
        assert_eq!(
            (mine.status.as_str(), mine.closed_by.as_str()),
            ("done", "replied")
        );
        assert_eq!(
            other.status, "open",
            "the first mailbox's task was not answered from its own mailbox"
        );
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    /// Ranks like the rules do, but raises the stop flag first: the app is restarted, or the mailbox removed, while the
    /// message is being ranked.
    struct StopWhileRanking(Arc<std::sync::atomic::AtomicBool>);
    impl engine::triage::Triager for StopWhileRanking {
        fn triage(
            &self,
            e: &mail::Email,
            s: &engine::signals::Signals,
        ) -> anyhow::Result<engine::triage::Triage> {
            self.0.store(true, std::sync::atomic::Ordering::Relaxed);
            engine::triage::Triager::triage(&RulesTriager, e, s)
        }
    }

    #[test]
    fn a_stop_while_the_last_message_of_a_batch_is_ranked_does_not_move_the_cursor_past_it() {
        let srv = Server::start(gmail_folders(inbox(1), vec![]));
        let store = Store::memory().unwrap().scoped(1);
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let mut s = srv.session();
        let n = engine::watcher::process_new_until(
            &mut s,
            &store,
            &StopWhileRanking(stop.clone()),
            &SentIndex::default(),
            50,
            &mut |_, _| {},
            &stop,
        )
        .unwrap();
        assert_eq!(n, 0, "nothing was stored once the stop came");
        assert_eq!(
            store.last_uid(),
            0,
            "the message was not stored, so the cursor must not say it was read: it would never be read"
        );
        // the next start reads it
        let again = std::sync::atomic::AtomicBool::new(false);
        let mut s = srv.session();
        let n = engine::watcher::process_new_until(
            &mut s,
            &store,
            &RulesTriager,
            &SentIndex::default(),
            50,
            &mut |_, _| {},
            &again,
        )
        .unwrap();
        assert_eq!(n, 1);
        assert!(store.get(global_uid(1, 1)).is_some());
    }
}
