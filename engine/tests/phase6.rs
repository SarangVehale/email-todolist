//! Phase 6: several mailboxes in one list. See docs/designs/phase6.md.
use engine::core::{worst_status, Core};
use engine::settings::Settings;
use engine::signals::{SentMsg, Signals};
use engine::store::{
    global_uid, imap_uid, mailbox_of, Store, LOCAL_UID_BASE, MAILBOX_SPAN, MAX_MAILBOXES,
};
use engine::triage::{RulesTriager, Triager};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// The in-memory keychain is one per process, and tests run side by side: those that save or read passwords take turns.
static KEYCHAIN: Mutex<()> = Mutex::new(());
fn keychain() -> std::sync::MutexGuard<'static, ()> {
    KEYCHAIN.lock().unwrap_or_else(|e| e.into_inner())
}

// ───────────────────────── helpers ─────────────────────────
fn fresh(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ia-p6-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn email(uid: u32, from: &str, subject: &str) -> engine::mail::Email {
    let raw = format!(
        "From: {from}\r\nSubject: {subject}\r\nMessage-ID: <m{uid}@x>\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nbody of {subject}\r\n"
    );
    engine::mail::parse(uid, raw.as_bytes()).unwrap()
}

fn add(st: &Store, uid: u32, from: &str, subject: &str) -> anyhow::Result<()> {
    let e = email(uid, from, subject);
    let s = Signals::default();
    st.add(&e, &RulesTriager.triage(&e, &s)?, &s).map(|_| ())
}

fn core(name: &str) -> (Arc<Core>, PathBuf) {
    let dir = fresh(name);
    (Core::new(dir.clone(), Arc::new(|_, _| {})), dir)
}

fn sent(id: &str, to: &str) -> SentMsg {
    SentMsg {
        message_id: id.into(),
        to: to.into(),
        recipients: vec![to.into()],
        subject: "Quote".into(),
        sent_at: 1_800_000_000.0,
    }
}

// ───────────────────────── ids ─────────────────────────
#[test]
fn task_ids_say_which_mailbox_they_came_from_and_the_first_mailbox_keeps_its_ids() {
    assert_eq!(global_uid(0, 5), 5, "nothing already stored changes");
    assert_eq!(global_uid(1, 5), MAILBOX_SPAN + 5);
    for n in 0..MAX_MAILBOXES {
        for uid in [1u32, 777, MAILBOX_SPAN - 1] {
            let id = global_uid(n, uid);
            assert_eq!(
                (mailbox_of(id), imap_uid(id)),
                (n, uid),
                "mailbox {n}, uid {uid}"
            );
            assert!(
                id < LOCAL_UID_BASE,
                "a mailbox's tasks never reach the ids of your own tasks"
            );
        }
    }
    assert!(u64::from(MAX_MAILBOXES) * u64::from(MAILBOX_SPAN) <= u64::from(LOCAL_UID_BASE));
}

// ───────────────────────── a store that knows its mailbox ─────────────────────────
#[test]
fn two_mailboxes_with_the_same_imap_uid_make_two_tasks() {
    let dir = fresh("same-uid");
    let path = dir.join("agent.db");
    let (s0, s1) = (
        Store::open(&path).unwrap(),
        Store::open(&path).unwrap().scoped(1),
    );
    add(&s0, 7, "a@work.example", "from the first").unwrap();
    add(&s1, 7, "b@home.example", "from the second").unwrap();
    let all = Store::open(&path).unwrap().all_tasks().unwrap();
    let mut ids: Vec<u32> = all.iter().map(|t| t.uid).collect();
    ids.sort_unstable();
    assert_eq!(ids, [7, MAILBOX_SPAN + 7]);
    let unscoped = Store::open(&path).unwrap();
    assert_eq!(unscoped.get(7).unwrap().subject, "from the first");
    assert_eq!(
        unscoped.get(MAILBOX_SPAN + 7).unwrap().subject,
        "from the second"
    );
    assert!(s0.seen(7) && s1.seen(7), "each mailbox knows its own uid 7");
    assert!(!s0.seen(8) && !s1.seen(8));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_imap_uid_too_large_for_the_id_layout_is_refused_and_stores_nothing() {
    let st = Store::memory().unwrap().scoped(1);
    assert!(add(&st, MAILBOX_SPAN, "a@b.example", "huge").is_err());
    assert!(add(&st, u32::MAX, "a@b.example", "huge").is_err());
    assert!(Store::memory().unwrap().all_tasks().unwrap().is_empty());
    assert!(st.all_tasks().unwrap().is_empty(), "nothing was written");
    add(
        &st,
        MAILBOX_SPAN - 1,
        "a@b.example",
        "the largest that fits",
    )
    .unwrap();
}

#[test]
fn cursors_validity_and_attempt_counters_are_kept_per_mailbox() {
    let dir = fresh("cursors");
    let path = dir.join("agent.db");
    let (s0, s1) = (
        Store::open(&path).unwrap(),
        Store::open(&path).unwrap().scoped(1),
    );
    s0.set_last_uid(10).unwrap();
    s1.set_last_uid(3).unwrap();
    assert_eq!((s0.last_uid(), s1.last_uid()), (10, 3));
    assert_eq!(
        Store::open(&path).unwrap().last_uid(),
        10,
        "the plain store is the first mailbox"
    );
    assert_eq!(s0.begin_attempt(7).unwrap(), 1);
    assert_eq!(s0.begin_attempt(7).unwrap(), 2);
    assert_eq!(
        s1.begin_attempt(7).unwrap(),
        1,
        "another mailbox's message 7 is another message"
    );
    assert_eq!((s0.in_flight(), s1.in_flight()), (Some(7), Some(7)));
    s1.end_attempt().unwrap();
    assert_eq!((s0.in_flight(), s1.in_flight()), (Some(7), None));
    assert_eq!(s0.no_content_attempt(9).unwrap(), 1);
    assert_eq!(s1.no_content_attempt(9).unwrap(), 1);
    assert_eq!(s0.no_content_attempt(9).unwrap(), 2);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_renumbered_mailbox_loses_only_its_own_mail_and_nothing_else() {
    let dir = fresh("renumber");
    let path = dir.join("agent.db");
    let (s0, s1) = (
        Store::open(&path).unwrap(),
        Store::open(&path).unwrap().scoped(1),
    );
    for uid in [1u32, 2] {
        add(&s0, uid, "a@work.example", &format!("first {uid}")).unwrap();
        add(&s1, uid, "b@home.example", &format!("second {uid}")).unwrap();
    }
    let all = Store::open(&path).unwrap();
    all.set_draft(global_uid(1, 1), "a draft", "user").unwrap();
    all.queue(global_uid(1, 1), "star", "").unwrap();
    all.queue(global_uid(0, 1), "star", "").unwrap();
    let mine = all.add_local_task("my own", 3, "").unwrap().unwrap();
    all.set_rules(&[]).unwrap();
    all.set_pref("a@work.example", "mute").unwrap();
    all.meta_set("update_checked", "2026-11-13").unwrap();
    s0.set_last_uid(2).unwrap();
    s1.set_last_uid(2).unwrap();
    assert!(
        !s1.reconcile_uid_validity(5).unwrap(),
        "first sight of a number is not a change"
    );
    assert!(s1.reconcile_uid_validity(6).unwrap());
    // the second mailbox starts over
    assert!(all.get(global_uid(1, 1)).is_none() && all.get(global_uid(1, 2)).is_none());
    assert_eq!(s1.last_uid(), 0);
    assert_eq!(
        all.pending(10).len(),
        1,
        "only the other mailbox's queued change is left"
    );
    // everything else is where it was
    assert!(
        all.get(1).is_some() && all.get(2).is_some(),
        "the first mailbox is untouched"
    );
    assert_eq!(s0.last_uid(), 2);
    assert!(all.get(mine).is_some(), "so are the tasks you added");
    assert_eq!(all.pref("a@work.example").as_deref(), Some("mute"));
    assert_eq!(
        all.meta_get("update_checked").as_deref(),
        Some("2026-11-13"),
        "a renumbered mailbox used to erase the update-check date along with its cursor"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn queued_mailbox_changes_go_to_the_mailbox_they_belong_to() {
    let dir = fresh("outbox");
    let path = dir.join("agent.db");
    let all = Store::open(&path).unwrap();
    let (s0, s1) = (
        Store::open(&path).unwrap().scoped(0),
        Store::open(&path).unwrap().scoped(1),
    );
    add(&s0, 3, "a@b.example", "zero").unwrap();
    add(&s1, 3, "a@b.example", "one").unwrap();
    all.queue(global_uid(0, 3), "star", "").unwrap();
    all.queue(global_uid(1, 3), "archive", "").unwrap();
    let ops = |s: &Store| -> Vec<(u32, String)> {
        s.pending(10).into_iter().map(|i| (i.uid, i.op)).collect()
    };
    assert_eq!(ops(&s0), [(3, "star".to_string())]);
    assert_eq!(ops(&s1), [(global_uid(1, 3), "archive".to_string())]);
    assert_eq!(
        all.pending(10).len(),
        2,
        "the window counts what is waiting for any of them"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_calls_the_sync_makes_speak_imap_numbers_and_stay_inside_the_mailbox() {
    let dir = fresh("imap-numbers");
    let path = dir.join("agent.db");
    let (s0, s1) = (
        Store::open(&path).unwrap(),
        Store::open(&path).unwrap().scoped(1),
    );
    add(&s0, 7, "a@b.example", "zero").unwrap();
    add(&s1, 7, "a@b.example", "one").unwrap();
    let open: Vec<u32> = s1.open_mail_tasks(10).into_iter().map(|t| t.0).collect();
    assert_eq!(
        open,
        [7],
        "the sync of mailbox 1 sees its own message by its IMAP number"
    );
    assert_eq!(s0.open_mail_tasks(10).len(), 1);
    s1.set_gone(7, true).unwrap();
    let all = Store::open(&path).unwrap();
    assert!(all.get(global_uid(1, 7)).unwrap().gone);
    assert!(
        !all.get(7).unwrap().gone,
        "message 7 of the first mailbox is another message"
    );
    assert!(s1.auto_close(7, "replied").unwrap());
    assert_eq!(all.get(global_uid(1, 7)).unwrap().status, "done");
    assert_eq!(all.get(7).unwrap().status, "open");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn follow_ups_belong_to_the_mailbox_whose_sent_mail_made_them() {
    let dir = fresh("followups");
    let path = dir.join("agent.db");
    let (s0, s1) = (
        Store::open(&path).unwrap().scoped(0),
        Store::open(&path).unwrap().scoped(1),
    );
    s0.add_followup(&sent("<a@x>", "d@s.example"), 1_800_000_000.0)
        .unwrap();
    s1.add_followup(&sent("<b@x>", "e@s.example"), 1_800_000_000.0)
        .unwrap();
    assert_eq!(s0.open_followups().len(), 1);
    assert_eq!(s1.open_followups().len(), 1);
    assert_eq!(s1.open_followups()[0].1, "<b@x>");
    assert_eq!(Store::open(&path).unwrap().open_followups().len(), 2);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn every_task_says_which_mailbox_it_belongs_to_and_your_own_tasks_say_none() {
    let dir = fresh("tasks-say");
    let path = dir.join("agent.db");
    let (s0, s2) = (
        Store::open(&path).unwrap().scoped(0),
        Store::open(&path).unwrap().scoped(2),
    );
    add(&s0, 5, "a@work.example", "first mailbox mail").unwrap();
    add(&s2, 5, "b@home.example", "third mailbox mail").unwrap();
    let fu = s2
        .add_followup(&sent("<b@x>", "e@s.example"), 1_800_000_000.0)
        .unwrap()
        .expect("a follow-up task");
    let mine = s0.add_local_task("Renew passport", 3, "").unwrap().unwrap();
    let all = Store::open(&path).unwrap();
    let of = |uid: u32| all.get(uid).unwrap().mailbox;
    assert_eq!(of(5), Some(0), "mail of the first mailbox");
    assert_eq!(of(global_uid(2, 5)), Some(2), "mail of the third mailbox");
    assert_eq!(
        of(fu),
        Some(2),
        "a follow-up belongs to the mailbox that sent the message"
    );
    assert_eq!(of(mine), None, "a task you added belongs to no mailbox");
    let json = serde_json::to_value(all.get(fu).unwrap()).unwrap();
    assert_eq!(json["mailbox"], 2, "and the list is told");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn removing_a_mailbox_removes_everything_of_it_and_nothing_else() {
    let dir = fresh("remove");
    let path = dir.join("agent.db");
    let all = Store::open(&path).unwrap();
    let (s0, s1) = (
        Store::open(&path).unwrap(),
        Store::open(&path).unwrap().scoped(1),
    );
    add(&s0, 1, "a@work.example", "keepword first").unwrap();
    add(&s1, 1, "b@home.example", "dropword second").unwrap();
    all.set_draft(global_uid(1, 1), "a draft", "user").unwrap();
    all.queue(global_uid(1, 1), "star", "").unwrap();
    all.queue(global_uid(0, 1), "star", "").unwrap();
    all.set_priority(global_uid(1, 1), 5).unwrap(); // makes a correction on a message of mailbox 1
    all.set_priority(global_uid(0, 1), 5).unwrap();
    s1.set_last_uid(9).unwrap();
    s0.set_last_uid(4).unwrap();
    s0.add_followup(&sent("<a@x>", "d@s.example"), 1_800_000_000.0)
        .unwrap();
    s1.add_followup(&sent("<b@x>", "e@s.example"), 1_800_000_000.0)
        .unwrap();
    let mine = all.add_local_task("my own", 3, "").unwrap().unwrap();
    all.set_pref("x@y.example", "important").unwrap();
    all.set_rules(&[]).unwrap();

    all.remove_mailbox(1).unwrap();

    assert!(all.get(global_uid(1, 1)).is_none());
    assert!(
        all.search("dropword").unwrap().is_empty(),
        "its words left the search index"
    );
    assert_eq!(s1.last_uid(), 0);
    let left: Vec<u32> = all
        .pending(100)
        .into_iter()
        .map(|i| mailbox_of(i.uid))
        .collect();
    assert!(
        !left.is_empty() && left.iter().all(|m| *m == 0),
        "its queued changes are gone, the other mailbox's are not: {left:?}"
    );
    assert_eq!(s1.open_followups().len(), 0, "its follow-ups are gone");
    assert!(
        all.followup_state("<b@x>").is_none(),
        "and so is the record that remembered them"
    );
    assert!(
        all.followup_state("<a@x>").is_some(),
        "the other mailbox's record stays"
    );
    assert_eq!(
        all.recent_overrides(10).len(),
        1,
        "the correction made on its message is gone, the other stays"
    );
    // everything else is where it was
    assert!(all.get(1).is_some());
    assert_eq!(all.search("keepword").unwrap(), [1]);
    assert_eq!(s0.last_uid(), 4);
    assert_eq!(s0.open_followups().len(), 1);
    assert!(all.get(mine).is_some());
    assert_eq!(all.pref("x@y.example").as_deref(), Some("important"));
    // the first mailbox can go too, while another stays
    add(&s1, 2, "b@home.example", "second again").unwrap();
    all.remove_mailbox(0).unwrap();
    assert!(all.get(1).is_none() && all.get(global_uid(1, 2)).is_some());
    assert!(
        all.get(mine).is_some(),
        "your own tasks live outside every mailbox's range"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn changing_the_first_mailbox_resets_only_the_first_mailbox() {
    let dir = fresh("reset-first");
    let path = dir.join("agent.db");
    let (s0, s1) = (
        Store::open(&path).unwrap().scoped(0),
        Store::open(&path).unwrap().scoped(1),
    );
    add(&s0, 1, "a@b.example", "zero").unwrap();
    add(&s1, 1, "a@b.example", "one").unwrap();
    s0.add_followup(&sent("<a@x>", "d@s.example"), 1_800_000_000.0)
        .unwrap();
    s1.add_followup(&sent("<b@x>", "e@s.example"), 1_800_000_000.0)
        .unwrap();
    s0.reset_for_new_account().unwrap();
    let all = Store::open(&path).unwrap();
    assert!(all.get(1).is_none() && all.get(global_uid(1, 1)).is_some());
    assert_eq!(
        (s0.open_followups().len(), s1.open_followups().len()),
        (0, 1)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// ───────────────────────── settings, passwords, status ─────────────────────────
#[test]
fn a_settings_file_from_the_previous_version_is_the_first_mailbox() {
    let s: Settings = serde_json::from_str(
        r#"{"onboarded":true,"account":{"host":"imap.example.com","port":993,"user":"me@example.com"}}"#,
    )
    .unwrap();
    assert!(s.more_accounts.is_empty());
    let m = s.mailboxes();
    assert_eq!(
        (m.len(), m[0].id, m[0].user.as_str()),
        (1, 0, "me@example.com")
    );
    assert!(
        Settings::default().mailboxes().is_empty(),
        "no server, no mailbox"
    );
}

fn with_extras(extras: serde_json::Value) -> anyhow::Result<Settings> {
    let mut s = Settings::default();
    s.account.host = "imap.example.com".into();
    s.account.user = "me@example.com".into();
    let s = s.patched(&json!({ "more_accounts": extras }))?;
    s.validate()?;
    Ok(s)
}

fn extra(id: u32, host: &str, user: &str) -> serde_json::Value {
    json!({"id": id, "host": host, "port": 993, "user": user})
}

#[test]
fn up_to_four_more_mailboxes_each_with_its_own_number() {
    let s = with_extras(json!([
        extra(1, "imap.gmail.com", "a@gmail.com"),
        extra(2, "imap.fastmail.com", "b@fastmail.com"),
        extra(4, "imap.mail.me.com", "c@icloud.com"),
    ]))
    .unwrap();
    let ids: Vec<u32> = s.mailboxes().iter().map(|m| m.id).collect();
    assert_eq!(ids, [0, 1, 2, 4]);
    with_extras(json!([
        extra(1, "h1.example", "a@x"),
        extra(2, "h2.example", "a@x"),
        extra(3, "h3.example", "a@x"),
        extra(4, "h4.example", "a@x"),
    ]))
    .unwrap();
}

#[test]
fn bad_mailbox_lists_are_refused() {
    let cases = [
        (
            "five extras",
            json!([
                extra(1, "a.example", "u"),
                extra(2, "b.example", "u"),
                extra(3, "c.example", "u"),
                extra(4, "d.example", "u"),
                extra(5, "e.example", "u")
            ]),
        ),
        ("number 0", json!([extra(0, "a.example", "u")])),
        ("number 5", json!([extra(5, "a.example", "u")])),
        (
            "same number twice",
            json!([extra(1, "a.example", "u"), extra(1, "b.example", "u")]),
        ),
        (
            "same mailbox as the first",
            json!([extra(1, "imap.example.com", "me@example.com")]),
        ),
        (
            "same mailbox as the first, in capitals",
            json!([extra(1, "IMAP.Example.com", "ME@example.com")]),
        ),
        (
            "the same mailbox twice",
            json!([extra(1, "a.example", "u"), extra(2, "A.example", "U")]),
        ),
        ("a bad host", json!([extra(1, "a b.example", "u")])),
        ("an empty host", json!([extra(1, "", "u")])),
        ("an empty user", json!([extra(1, "a.example", "")])),
        (
            "a control character in the user",
            json!([extra(1, "a.example", "u\nv")]),
        ),
        (
            "a port of 0",
            json!([{"id": 1, "host": "a.example", "port": 0, "user": "u"}]),
        ),
    ];
    for (why, list) in cases {
        assert!(with_extras(list).is_err(), "accepted: {why}");
    }
}

#[test]
fn every_mailbox_has_its_own_keychain_entry_and_sign_out_forgets_them_all() {
    let _keys = keychain();
    use engine::secrets;
    assert_eq!(
        secrets::imap_key(0),
        "imap-password",
        "the first keeps the name it always had"
    );
    assert_eq!(secrets::imap_key(3), "imap-password-3");
    for id in 0..secrets::MAX_MAILBOXES {
        secrets::set(&secrets::imap_key(id), &format!("pw{id}")).unwrap();
    }
    secrets::set("anthropic-key", "k").unwrap();
    assert_eq!(secrets::get("imap-password-4").as_deref(), Some("pw4"));
    secrets::delete_all();
    for id in 0..secrets::MAX_MAILBOXES {
        assert!(secrets::get(&secrets::imap_key(id)).is_none(), "{id}");
    }
    assert!(secrets::get("anthropic-key").is_none());
}

#[test]
fn the_status_shown_is_the_worst_of_the_mailboxes() {
    let w = |v: &[&str]| worst_status(&v.iter().map(|s| s.to_string()).collect::<Vec<_>>());
    assert_eq!(w(&["idle", "idle"]), "idle");
    assert_eq!(w(&["idle", "syncing"]), "syncing");
    assert_eq!(w(&["connecting", "idle"]), "connecting");
    assert_eq!(w(&["syncing", "connecting"]), "connecting");
    assert_eq!(w(&["idle", "error: refused"]), "error: refused");
    assert_eq!(
        w(&["error: first", "error: second", "idle"]),
        "error: first"
    );
    assert_eq!(w(&["idle", "no password"]), "no password");
    assert_eq!(w(&["idle"]), "idle");
    assert_eq!(w(&[]), "setup");
}

// ───────────────────────── commands ─────────────────────────
fn set_up(dir: &Path) {
    // a first mailbox the way the wizard leaves it (a server that refuses at once, so nothing waits on the network)
    let s = Settings {
        onboarded: true,
        account: engine::settings::AccountCfg {
            id: 0,
            host: "127.0.0.1".into(),
            port: 1,
            user: "me@example.com".into(),
        },
        ..Default::default()
    };
    s.save(dir).unwrap();
    engine::secrets::set("imap-password", "pw0").unwrap();
}

#[test]
fn another_mailbox_can_be_added_listed_and_removed() {
    let _keys = keychain();
    let (c, dir) = core("accounts");
    set_up(&dir);
    let id = c
        .rpc(
            "add_account",
            &json!({"host": "127.0.0.1", "port": 1, "user": "me@work.example", "password": "pw1"}),
        )
        .unwrap();
    assert_eq!(id, json!(1));
    let st = c.rpc("state", &json!({})).unwrap();
    let list = st["accounts"].as_array().unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(
        (list[1]["id"].as_u64(), list[1]["user"].as_str()),
        (Some(1), Some("me@work.example"))
    );
    assert!(list.iter().all(|a| a.get("password").is_none()));
    let file = std::fs::read_to_string(dir.join("settings.json")).unwrap();
    assert!(
        !file.contains("pw1") && !file.contains("pw0"),
        "passwords never go in a file"
    );
    assert_eq!(
        engine::secrets::get("imap-password-1").as_deref(),
        Some("pw1")
    );
    // the phone sees no mailbox names or addresses
    let public = c.rpc_public("state", &json!({})).unwrap();
    assert!(public.get("accounts").is_none());
    // mail of the second mailbox, then remove it
    let s1 = c.store_for(1).unwrap();
    add(&s1, 5, "x@y.example", "second mailbox mail").unwrap();
    let mine = c
        .store()
        .unwrap()
        .add_local_task("mine", 3, "")
        .unwrap()
        .unwrap();
    c.rpc("remove_account", &json!({"id": 1})).unwrap();
    assert!(c.store().unwrap().get(global_uid(1, 5)).is_none());
    assert!(c.store().unwrap().get(mine).is_some());
    assert_eq!(
        c.rpc("state", &json!({})).unwrap()["accounts"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(
        engine::secrets::get("imap-password-1").is_none(),
        "its password left the keychain"
    );
    assert!(c.settings().more_accounts.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_mailbox_with_no_password_says_so_and_does_not_stop_the_other_from_starting() {
    let _keys = keychain();
    let (c, dir) = core("one-without-password");
    set_up(&dir);
    let mut s = Settings::load(&dir);
    s.more_accounts.push(engine::settings::AccountCfg {
        id: 1,
        host: "127.0.0.1".into(),
        port: 1,
        user: "me@work.example".into(),
    });
    s.save(&dir).unwrap();
    engine::secrets::delete("imap-password-1");
    c.start();
    let st = c.rpc("state", &json!({})).unwrap();
    let list = st["accounts"].as_array().unwrap();
    assert_eq!(list[1]["status"], "no password", "{list:?}");
    assert_ne!(
        list[0]["status"], "no password",
        "the first mailbox has a password and is being read: {list:?}"
    );
    assert_eq!(
        st["status"], "no password",
        "and the one status the window shows is the worst of the two"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn after_the_first_mailbox_is_removed_its_place_is_taken_by_the_next_one_added() {
    let _keys = keychain();
    let (c, dir) = core("reuse-first");
    set_up(&dir);
    let add = |user: &str, pw: &str| {
        c.rpc(
            "add_account",
            &json!({"host": "127.0.0.1", "port": 1, "user": user, "password": pw}),
        )
    };
    for n in 1..=4 {
        assert_eq!(add(&format!("m{n}@work.example"), "pw").unwrap(), json!(n));
    }
    assert!(add("six@work.example", "pw").is_err(), "five are in use");
    c.rpc("remove_account", &json!({"id": 0})).unwrap();
    let count = || {
        c.rpc("state", &json!({})).unwrap()["accounts"]
            .as_array()
            .unwrap()
            .len()
    };
    assert_eq!(count(), 4);
    // five can be read again: the new one takes the empty first place, with the first place's keychain entry
    assert_eq!(add("again@home.example", "pwx").unwrap(), json!(0));
    assert_eq!(count(), 5);
    assert_eq!(c.settings().account.user, "again@home.example");
    assert_eq!(
        engine::secrets::get("imap-password").as_deref(),
        Some("pwx")
    );
    assert!(
        add("sixth@home.example", "pw").is_err(),
        "and still no more than five"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_phone_never_sees_the_login_of_a_mailbox_not_even_in_the_log() {
    let _keys = keychain();
    let (c, dir) = core("phone-log");
    set_up(&dir);
    let st = c.store().unwrap();
    st.log("connected as me@example.com; Sent folder 'Sent'")
        .unwrap();
    c.store_for(1)
        .unwrap()
        .log("connected as second@work.example; Sent folder 'Sent Items'")
        .unwrap();
    let phone = c.rpc_public("state", &json!({})).unwrap().to_string();
    assert!(
        !phone.contains("me@example.com") && !phone.contains("second@work.example"),
        "{phone}"
    );
    assert!(
        phone.contains("Sent Items"),
        "the rest of the line is still there for the phone"
    );
    let desk = c.rpc("state", &json!({})).unwrap().to_string();
    assert!(
        desk.contains("second@work.example"),
        "the desktop window still sees its own log in full"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_new_mailbox_starts_clean_even_if_a_stopped_sync_left_something_in_its_number() {
    let _keys = keychain();
    let (c, dir) = core("leftovers");
    set_up(&dir);
    // what a sync that was still running when its mailbox was removed can leave behind
    let ghost = c.store_for(1).unwrap();
    add(&ghost, 5, "x@y.example", "left behind").unwrap();
    ghost.set_last_uid(5).unwrap();
    let id = c
        .rpc(
            "add_account",
            &json!({"host": "127.0.0.1", "port": 1, "user": "new@work.example", "password": "pw"}),
        )
        .unwrap();
    assert_eq!(id, json!(1));
    let fresh = c.store_for(1).unwrap();
    assert_eq!(
        fresh.last_uid(),
        0,
        "so its whole inbox is read, as for any new mailbox"
    );
    assert!(
        fresh.get(global_uid(1, 5)).is_none(),
        "and no stranger's task is in its list"
    );
    assert!(
        !c.store()
            .unwrap()
            .recent_log(50)
            .iter()
            .any(|l| l.1.contains("removed")),
        "and the log does not claim a mailbox was removed when one was added"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn news_in_one_mailbox_does_not_wake_a_snooze_in_another() {
    let dir = fresh("snooze-other");
    let path = dir.join("agent.db");
    let (s0, s1) = (
        Store::open(&path).unwrap().scoped(0),
        Store::open(&path).unwrap().scoped(1),
    );
    add(&s0, 5, "a@x.example", "same message").unwrap();
    let all = Store::open(&path).unwrap();
    all.set_snooze(5, 4_000_000_000.0).unwrap();
    // the same message also reaches the second mailbox (a copy to both of your addresses)
    add(&s1, 5, "a@x.example", "same message").unwrap();
    assert!(
        all.get(5).unwrap().snooze_until > 0.0,
        "a conversation is one mailbox's: news in another does not cancel this snooze"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_same_server_written_another_way_is_still_the_same_mailbox() {
    let mailbox = |id: u32, host: &str, user: &str| engine::settings::AccountCfg {
        id,
        host: host.into(),
        port: 993,
        user: user.into(),
    };
    let with = |extra: engine::settings::AccountCfg| Settings {
        account: mailbox(0, "imap.gmail.com", "me@gmail.com"),
        more_accounts: vec![extra],
        ..Default::default()
    };
    assert!(
        with(mailbox(1, "imap.gmail.com.", "me@gmail.com"))
            .validate()
            .is_err(),
        "a trailing dot"
    );
    assert!(
        with(mailbox(1, "IMAP.GMAIL.COM", " Me@Gmail.com "))
            .validate()
            .is_err(),
        "letter case and spaces"
    );
    assert!(
        with(mailbox(1, "imap.googlemail.com", "me@gmail.com"))
            .validate()
            .is_err(),
        "Google's other name for the same server"
    );
    assert!(
        with(mailbox(1, "imap.gmail.com", "other@gmail.com"))
            .validate()
            .is_ok(),
        "another account is fine"
    );
}

#[test]
fn the_keychain_is_never_the_real_one_when_the_tests_run() {
    // however cargo was started (inside engine/ or from the repository's top folder), a test that saves a password or
    // signs out must not touch the passwords of the person running it
    assert!(engine::secrets::in_memory());
}

#[test]
fn a_database_from_before_several_mailboxes_opens_unchanged() {
    let dir = fresh("old-database");
    let path = dir.join("agent.db");
    {
        let st = Store::open(&path).unwrap();
        add(&st, 7, "a@work.example", "an old task").unwrap();
        st.set_last_uid(7).unwrap();
        st.add_followup(&sent("<old@x>", "d@s.example"), 1_800_000_000.0)
            .unwrap();
    }
    // make it look like the file the previous version wrote: no mailbox number on the follow-up records
    {
        let db = rusqlite::Connection::open(&path).unwrap();
        db.execute("ALTER TABLE followups DROP COLUMN acct", [])
            .unwrap();
    }
    let st = Store::open(&path).unwrap().scoped(0);
    let t = st.get(7).expect("the old task is still task 7");
    assert_eq!((t.subject.as_str(), t.mailbox), ("an old task", Some(0)));
    assert_eq!(st.last_uid(), 7, "the cursor is where it was");
    assert_eq!(
        st.open_followups().len(),
        1,
        "the old follow-up belongs to the first mailbox"
    );
    assert_eq!(
        st.all_tasks().unwrap().len(),
        2,
        "the task and the follow-up task"
    );
    // and it can take a second mailbox beside it
    let s1 = Store::open(&path).unwrap().scoped(1);
    add(&s1, 7, "b@home.example", "a new one").unwrap();
    assert!(st.get(7).is_some() && st.get(global_uid(1, 7)).is_some());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_follow_up_still_open_after_sixty_days_keeps_its_mailbox_and_goes_with_it() {
    let dir = fresh("old-followup");
    let path = dir.join("agent.db");
    let s1 = Store::open(&path).unwrap().scoped(1);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64();
    let task = s1
        .add_followup(&sent("<old@x>", "d@s.example"), now - 70.0 * 86400.0)
        .unwrap()
        .unwrap();
    let all = Store::open(&path).unwrap();
    all.prune(90, 30).unwrap(); // the daily clean-up
    assert_eq!(
        all.get(task)
            .expect("an open follow-up is not pruned at 60 days")
            .mailbox,
        Some(1),
        "it still knows which mailbox it belongs to"
    );
    all.remove_mailbox(1).unwrap();
    assert!(all.get(task).is_none(), "and it goes when its mailbox does");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_follow_up_whose_record_the_old_version_pruned_belongs_to_the_first_mailbox() {
    let dir = fresh("recordless-followup");
    let path = dir.join("agent.db");
    let task = {
        let st = Store::open(&path).unwrap();
        st.add_followup(&sent("<old@x>", "d@s.example"), 1_800_000_000.0)
            .unwrap()
            .unwrap()
    };
    // the previous version deleted every record older than 60 days, whatever its task was doing
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute("DELETE FROM followups", [])
        .unwrap();
    let all = Store::open(&path).unwrap();
    assert_eq!(
        all.get(task).unwrap().mailbox,
        Some(0),
        "only the first mailbox could make follow-ups then"
    );
    all.remove_mailbox(1).unwrap();
    assert!(
        all.get(task).is_some(),
        "another mailbox's removal is none of its business"
    );
    all.remove_mailbox(0).unwrap();
    assert!(
        all.get(task).is_none(),
        "and it goes with the first mailbox, as it always did"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_notices_and_the_digest_count_a_conversation_once_per_mailbox() {
    let dir = fresh("notices");
    let path = dir.join("agent.db");
    let (s0, s1) = (
        Store::open(&path).unwrap().scoped(0),
        Store::open(&path).unwrap().scoped(1),
    );
    add(&s0, 5, "a@x.example", "same message").unwrap();
    add(&s1, 5, "a@x.example", "same message").unwrap(); // the same message reached both mailboxes
    let tasks = Store::open(&path).unwrap().all_tasks().unwrap();
    assert_eq!(tasks.len(), 2);
    assert_eq!(
        engine::jobs::wake_notices(&tasks).len(),
        2,
        "the list shows two rows, so a wake-up notice names each of them"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_mailbox_in_the_first_place_needs_an_address_and_a_server_like_any_other() {
    let _keys = keychain();
    let (c, dir) = core("first-place-checked");
    set_up(&dir);
    let add = |host: &str, user: &str| {
        c.rpc(
            "add_account",
            &json!({"host": host, "port": 1, "user": user, "password": "pw"}),
        )
    };
    for n in 1..=4 {
        add("127.0.0.1", &format!("m{n}@work.example")).unwrap();
    }
    c.rpc("remove_account", &json!({"id": 0})).unwrap();
    assert!(add("127.0.0.1", "").is_err(), "no address");
    assert!(add("127.0.0.1", "   ").is_err(), "a blank address");
    assert!(add("", "x@y.example").is_err(), "no server");
    assert_eq!(c.settings().mailboxes().len(), 4, "nothing was half-added");
    assert!(add("127.0.0.1", "ok@work.example").is_ok());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn adding_a_mailbox_is_refused_when_it_should_be() {
    let _keys = keychain();
    let (c, dir) = core("accounts-refused");
    // nothing set up yet
    let e = c
        .rpc(
            "add_account",
            &json!({"host": "127.0.0.1", "port": 1, "user": "a@b.example", "password": "p"}),
        )
        .unwrap_err()
        .to_string();
    assert!(e.contains("first mailbox"), "{e}");
    set_up(&dir);
    let add = |host: &str, user: &str, pw: &str| {
        c.rpc(
            "add_account",
            &json!({"host": host, "port": 1, "user": user, "password": pw}),
        )
    };
    assert!(
        add("127.0.0.1", "ME@example.com", "p").is_err(),
        "the first mailbox again"
    );
    assert!(add("a b", "x@y.example", "p").is_err(), "a bad server name");
    assert!(add("127.0.0.1", "x@y.example", "").is_err(), "no password");
    for (i, user) in ["a@x.example", "b@x.example", "c@x.example", "d@x.example"]
        .iter()
        .enumerate()
    {
        assert_eq!(add("127.0.0.1", user, "p").unwrap(), json!(i + 1));
    }
    let e = add("127.0.0.1", "e@x.example", "p")
        .unwrap_err()
        .to_string();
    assert!(e.contains("at most"), "{e}");
    // a removed number is reused
    c.rpc("remove_account", &json!({"id": 2})).unwrap();
    assert_eq!(add("127.0.0.1", "f@x.example", "p").unwrap(), json!(2));
    // the phone may not add or remove anything
    for cmd in ["add_account", "remove_account"] {
        let e = c
            .rpc_public(cmd, &json!({"host": "127.0.0.1", "port": 1, "user": "z@x.example", "password": "p", "id": 1}))
            .unwrap_err()
            .to_string();
        assert!(e.contains("not available from the hub"), "{cmd}: {e}");
        assert!(!engine::commands::PUBLIC.contains(&cmd));
    }
    // a settings patch cannot add one behind the command's back
    let e = c
        .rpc("save_settings", &json!({"patch": {"more_accounts": []}}))
        .unwrap_err()
        .to_string();
    assert!(
        e.contains("save_account") || e.contains("add_account") || e.contains("account"),
        "{e}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn removing_the_first_mailbox_keeps_the_others_and_removing_the_last_asks_for_setup_again() {
    let _keys = keychain();
    let (c, dir) = core("accounts-first");
    set_up(&dir);
    c.rpc(
        "add_account",
        &json!({"host": "127.0.0.1", "port": 1, "user": "b@x.example", "password": "p1"}),
    )
    .unwrap();
    add(
        &c.store_for(0).unwrap(),
        1,
        "a@y.example",
        "first mailbox mail",
    )
    .unwrap();
    add(
        &c.store_for(1).unwrap(),
        1,
        "a@y.example",
        "second mailbox mail",
    )
    .unwrap();
    let mine = c
        .store()
        .unwrap()
        .add_local_task("mine", 3, "")
        .unwrap()
        .unwrap();
    c.store().unwrap().set_rules(&[]).unwrap();
    c.store().unwrap().set_pref("x@y.example", "mute").unwrap();
    c.rpc("remove_account", &json!({"id": 0})).unwrap();
    let s = c.settings();
    assert!(s.onboarded, "another mailbox is still there");
    assert_eq!(s.mailboxes().len(), 1);
    assert_eq!(s.mailboxes()[0].user, "b@x.example");
    assert!(c.store().unwrap().get(1).is_none());
    assert!(c.store().unwrap().get(global_uid(1, 1)).is_some());
    // the last one
    c.rpc("remove_account", &json!({"id": 1})).unwrap();
    let s = c.settings();
    assert!(
        !s.onboarded && s.mailboxes().is_empty(),
        "setup is asked for again"
    );
    let st = c.store().unwrap();
    assert!(st.get(mine).is_some(), "your own tasks stay");
    assert_eq!(
        st.pref("x@y.example").as_deref(),
        Some("mute"),
        "so do your choices about senders"
    );
    assert!(
        c.rpc("remove_account", &json!({"id": 3})).is_err(),
        "there is no such mailbox"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn changing_the_first_mailbox_leaves_the_others_alone() {
    let _keys = keychain();
    let (c, dir) = core("accounts-change-first");
    set_up(&dir);
    c.rpc(
        "add_account",
        &json!({"host": "127.0.0.1", "port": 1, "user": "b@x.example", "password": "p1"}),
    )
    .unwrap();
    add(&c.store_for(0).unwrap(), 1, "a@y.example", "first").unwrap();
    add(&c.store_for(1).unwrap(), 1, "a@y.example", "second").unwrap();
    c.rpc("save_account", &json!({"host": "127.0.0.1", "port": 1, "user": "someone.else@example.com", "password": "new"}))
        .unwrap();
    assert!(
        c.store().unwrap().get(1).is_none(),
        "another address starts clean"
    );
    assert!(
        c.store().unwrap().get(global_uid(1, 1)).is_some(),
        "the second mailbox is untouched"
    );
    assert_eq!(
        engine::secrets::get("imap-password-1").as_deref(),
        Some("p1")
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn signing_out_removes_every_mailbox_and_every_password() {
    let _keys = keychain();
    let (c, dir) = core("accounts-signout");
    set_up(&dir);
    c.rpc(
        "add_account",
        &json!({"host": "127.0.0.1", "port": 1, "user": "b@x.example", "password": "p1"}),
    )
    .unwrap();
    add(&c.store_for(1).unwrap(), 1, "a@y.example", "second").unwrap();
    c.rpc("sign_out", &json!({})).unwrap();
    let s = c.settings();
    assert!(s.more_accounts.is_empty() && s.mailboxes().is_empty() && !s.onboarded);
    assert!(c.store().unwrap().all_tasks().unwrap().is_empty());
    assert!(
        engine::secrets::get("imap-password-1").is_none()
            && engine::secrets::get("imap-password").is_none()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn many_connections_opening_a_database_that_needs_migrating_at_once_all_succeed() {
    // every mailbox's sync opens its own connection; a fresh database used to make one of them fail with
    // "duplicate column name" and that mailbox never started
    for round in 0..6 {
        let dir = fresh(&format!("migrate-race-{round}"));
        let path = dir.join("agent.db");
        let go = Arc::new(std::sync::Barrier::new(8));
        let workers: Vec<_> = (0..8)
            .map(|i| {
                let (p, g) = (path.clone(), go.clone());
                std::thread::spawn(move || {
                    g.wait();
                    Store::open(&p).map(|s| s.scoped(i % 5)).map(|_| ())
                })
            })
            .collect();
        for w in workers {
            w.join()
                .unwrap()
                .expect("a connection failed to open the database");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn a_renumbered_mailbox_cuts_the_links_of_its_own_corrections_only() {
    let dir = fresh("renumber-links");
    let path = dir.join("agent.db");
    let (s0, s1) = (
        Store::open(&path).unwrap().scoped(0),
        Store::open(&path).unwrap().scoped(1),
    );
    add(&s0, 1, "a@work.example", "first").unwrap();
    let all = Store::open(&path).unwrap();
    let original = all.get(1).unwrap().priority;
    assert_ne!(original, 2);
    all.set_priority(1, 2).unwrap(); // a correction on a message of the first mailbox
    assert_eq!(all.recent_overrides(10).len(), 1);
    s1.reconcile_uid_validity(5).unwrap();
    s1.reconcile_uid_validity(6).unwrap(); // the second mailbox is renumbered
                                           // putting the first mailbox's message back where it was withdraws the correction, which only works while it is linked
    all.set_priority(1, original).unwrap();
    assert_eq!(
        all.recent_overrides(10).len(),
        0,
        "the first mailbox's correction was still linked to its message"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_correction_cut_loose_by_a_renumbering_goes_only_with_its_own_mailbox() {
    let dir = fresh("detached-corrections");
    let path = dir.join("agent.db");
    let (s0, s1) = (
        Store::open(&path).unwrap().scoped(0),
        Store::open(&path).unwrap().scoped(1),
    );
    add(&s1, 1, "b@home.example", "second").unwrap();
    let all = Store::open(&path).unwrap();
    all.set_priority(global_uid(1, 1), 1).unwrap(); // a correction on a message of the second mailbox
    s1.reconcile_uid_validity(5).unwrap();
    s1.reconcile_uid_validity(6).unwrap(); // the second mailbox is renumbered: the correction stays, unlinked
    assert_eq!(all.recent_overrides(10).len(), 1);
    add(&s0, 1, "a@work.example", "first").unwrap();
    all.remove_mailbox(0).unwrap(); // the first mailbox goes
    assert_eq!(
        all.recent_overrides(10).len(),
        1,
        "what was learned in the second mailbox is not the first mailbox's to delete"
    );
    all.remove_mailbox(1).unwrap();
    assert_eq!(
        all.recent_overrides(10).len(),
        0,
        "and it goes with the second mailbox"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_record_of_an_already_answered_message_belongs_to_the_mailbox_that_saw_it() {
    let dir = fresh("answered-record");
    let path = dir.join("agent.db");
    let (s0, s1) = (
        Store::open(&path).unwrap().scoped(0),
        Store::open(&path).unwrap().scoped(1),
    );
    s0.mark_followup("<a@x>", "answered", 1_800_000_000.0)
        .unwrap();
    s1.mark_followup("<b@x>", "answered", 1_800_000_000.0)
        .unwrap();
    let all = Store::open(&path).unwrap();
    all.remove_mailbox(0).unwrap();
    assert!(all.followup_state("<a@x>").is_none(), "its own record went");
    assert!(
        all.followup_state("<b@x>").is_some(),
        "the second mailbox's record is not touched by removing the first"
    );
    all.remove_mailbox(1).unwrap();
    assert!(
        all.followup_state("<b@x>").is_none(),
        "and goes with its own mailbox"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn what_follows_a_new_message_is_queued_for_that_mailboxs_own_task() {
    let (c, dir) = core("on-triaged");
    let s = Settings {
        writeback: true,
        ..Default::default()
    };
    s.save(&dir).unwrap();
    let s1 = c.store_for(1).unwrap();
    add(&s1, 5, "a@b.example", "second mailbox mail").unwrap();
    let e = email(5, "a@b.example", "second mailbox mail");
    let t = engine::triage::Triage {
        priority: 3,
        category: "work".into(),
        action: "x".into(),
        urgent: false,
        why: "r".into(),
        needs_reply: false,
        due: String::new(),
    };
    c.on_triaged(1, true, &e, &t, false);
    let items: Vec<(u32, String)> = c
        .store()
        .unwrap()
        .pending(10)
        .into_iter()
        .map(|i| (i.uid, i.op))
        .collect();
    assert_eq!(
        items,
        [(global_uid(1, 5), "category".to_string())],
        "the label is queued for the second mailbox's task, not for task 5 of the first"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn another_mailbox_can_get_a_new_password_without_losing_its_list_and_a_new_address_resets_only_it()
{
    let _keys = keychain();
    let (c, dir) = core("accounts-password");
    set_up(&dir);
    c.rpc(
        "add_account",
        &json!({"host": "127.0.0.1", "port": 1, "user": "b@x.example", "password": "old"}),
    )
    .unwrap();
    add(&c.store_for(0).unwrap(), 1, "a@y.example", "first mailbox").unwrap();
    add(&c.store_for(1).unwrap(), 1, "a@y.example", "second mailbox").unwrap();
    // the same mailbox, a new password: everything stays
    c.rpc(
        "save_account",
        &json!({"id": 1, "host": "127.0.0.1", "port": 1, "user": "b@x.example", "password": "new"}),
    )
    .unwrap();
    assert_eq!(
        engine::secrets::get("imap-password-1").as_deref(),
        Some("new")
    );
    assert_eq!(
        engine::secrets::get("imap-password").as_deref(),
        Some("pw0"),
        "the first mailbox's password is its own"
    );
    let st = c.store().unwrap();
    assert!(st.get(global_uid(1, 1)).is_some() && st.get(1).is_some());
    assert_eq!(c.settings().more_accounts.len(), 1);
    assert_eq!(c.settings().more_accounts[0].id, 1, "it keeps its number");
    // another address: that mailbox starts clean, and only that one
    c.rpc("save_account", &json!({"id": 1, "host": "127.0.0.1", "port": 1, "user": "c@x.example", "password": "newer"}))
        .unwrap();
    assert!(
        st.get(global_uid(1, 1)).is_none(),
        "the second mailbox starts clean"
    );
    assert!(st.get(1).is_some(), "the first is untouched");
    assert_eq!(c.settings().more_accounts[0].user, "c@x.example");
    // there is no mailbox 3, and a mailbox cannot be changed into one that already exists
    assert!(c.rpc("save_account", &json!({"id": 3, "host": "127.0.0.1", "port": 1, "user": "z@x.example", "password": "p"})).is_err());
    assert!(c.rpc("save_account", &json!({"id": 1, "host": "127.0.0.1", "port": 1, "user": "me@example.com", "password": "p"})).is_err());
    // the phone cannot do any of this
    assert!(c.rpc_public("save_account", &json!({"id": 1, "host": "127.0.0.1", "port": 1, "user": "q@x.example", "password": "p"})).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}
