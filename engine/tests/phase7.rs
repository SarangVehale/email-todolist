//! Phase 7: backup, export and restore (design: docs/designs/phase7.md).
use engine::core::Core;
use engine::rules::Rule;
use engine::settings::{AccountCfg, Settings};
use engine::signals::Signals;
use engine::store::{global_uid, Store};
use engine::triage::{RulesTriager, Triager};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

static KEYCHAIN: Mutex<()> = Mutex::new(());
fn keychain() -> std::sync::MutexGuard<'static, ()> {
    KEYCHAIN.lock().unwrap_or_else(|e| e.into_inner())
}

// ───────────────────────── helpers ─────────────────────────
fn fresh(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ia-p7-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A finished setup with one mailbox, the way the wizard leaves it (a server that refuses at once).
fn set_up(dir: &Path) {
    let s = Settings {
        onboarded: true,
        account: AccountCfg {
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

fn core(name: &str) -> (Arc<Core>, PathBuf) {
    let dir = fresh(name);
    set_up(&dir);
    (Core::new(dir.clone(), Arc::new(|_, _| {})), dir)
}

fn mail(st: &Store, uid: u32, from: &str, subject: &str, body: &str) {
    let raw = format!(
        "From: {from}\r\nSubject: {subject}\r\nMessage-ID: <m{uid}@x>\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n{body}\r\n"
    );
    let e = engine::mail::parse(uid, raw.as_bytes()).unwrap();
    let s = Signals::default();
    st.add(&e, &RulesTriager.triage(&e, &s).unwrap(), &s)
        .unwrap();
}

fn rule(id: &str, field: &str, text: &str, priority: Option<u8>, category: Option<&str>) -> Rule {
    Rule {
        id: id.into(),
        field: field.into(),
        text: text.into(),
        priority,
        category: category.map(String::from),
        file: false,
    }
}

const FORMAT: &str = "inbox-agent-backup";

/// A time `n` days from now, in seconds.
fn days_ahead(n: f64) -> f64 {
    engine::backup::now_secs() + n * 86400.0
}

/// The smallest valid file, plus whatever sections the test adds.
fn doc(extra: Value) -> String {
    let mut v = json!({"format": FORMAT, "version": 1});
    if let (Some(base), Some(add)) = (v.as_object_mut(), extra.as_object()) {
        for (k, x) in add {
            base.insert(k.clone(), x.clone());
        }
    }
    v.to_string()
}

fn restore(c: &Arc<Core>, text: &str, apply: bool, settings: bool) -> anyhow::Result<Value> {
    c.rpc(
        "backup_import",
        &json!({"json": text, "apply": apply, "settings": settings}),
    )
}

/// The text of a backup of this computer (no file: the command that writes one goes to the real Downloads folder).
fn export_text(c: &Core) -> String {
    engine::backup::to_json(&engine::backup::build(c).unwrap()).unwrap()
}

/// A computer with a lot of your work on it. Everything marked SECRET must stay out of a backup file.
fn busy_computer(name: &str) -> (Arc<Core>, PathBuf) {
    let (c, dir) = core(name);
    let st = c.store().unwrap();
    st.set_rules(&[
        rule("r1", "domain", "shop.example", Some(1), None),
        rule("r2", "subject", "invoice", None, Some("finance")),
    ])
    .unwrap();
    mail(
        &st,
        5,
        "Priya <priya@acme.example>",
        "SECRET-SUBJECT-5",
        "SECRET-BODY-5",
    );
    mail(
        &st,
        6,
        "Dana <dana@studio.example>",
        "SECRET-SUBJECT-6",
        "SECRET-BODY-6",
    );
    mail(
        &st,
        7,
        "Lee <lee@club.example>",
        "SECRET-SUBJECT-7",
        "SECRET-BODY-7",
    );
    mail(
        &st,
        8,
        "Kim <kim@nothing.example>",
        "SECRET-SUBJECT-8",
        "SECRET-BODY-8",
    );
    // a follow-up the app made for you (it is made again from your Sent folder, so it is not part of a backup)
    st.add_followup(
        &engine::signals::SentMsg {
            message_id: "<sent1@x>".into(),
            to: "someone@else.example".into(),
            recipients: vec!["someone@else.example".into()],
            subject: "SECRET-FOLLOWUP-SUBJECT".into(),
            sent_at: 1_800_000_000.0,
        },
        1_800_000_000.0,
    )
    .unwrap();
    st.set_note(5, "call back on Tuesday").unwrap();
    st.set_star(5, true).unwrap();
    st.set_draft(5, "Hi Priya, thanks for this.", "user")
        .unwrap();
    st.set_done(6, true).unwrap();
    st.set_snooze(7, days_ahead(100.0)).unwrap();
    let now_p = st.get(5).unwrap().priority;
    st.set_priority(5, if now_p == 2 { 4 } else { 2 }).unwrap(); // a correction of the ranking
    st.set_pref("priya@acme.example", "important").unwrap();
    let own = st
        .add_local_task("Renew passport", 4, "2027-03-01")
        .unwrap()
        .unwrap();
    st.set_reminder(
        own,
        "2099-01-01 09:00",
        "weekly",
        engine::reminders::now_local(),
    )
    .unwrap();
    st.set_note(own, "bring two photos").unwrap();
    st.set_star(own, true).unwrap();
    let chore = st.add_local_task("Old chore", 2, "").unwrap().unwrap();
    st.set_done(chore, true).unwrap();
    // options, some of them secrets or destinations
    let mut s = c.settings();
    s.alerts.quiet_start = "22:00".into();
    s.alerts.quiet_end = "07:00".into();
    s.alerts.digest_time = "08:30".into();
    s.alerts.categories = vec!["work".into()];
    s.alerts.ntfy_topic = "SECRET-NTFY-TOPIC".into();
    s.alerts.ntfy_server = "https://secret-ntfy.example".into();
    s.drafts.signature = "Best, Sam".into();
    s.followups.days = 3;
    s.ai.mode = "ollama".into();
    s.ai.ollama_model = "llama3.2".into();
    s.ai.ollama_url = "http://10.9.9.9:11434".into();
    s.hub.token = "SECRET-HUB-TOKEN".into();
    s.writeback = true;
    s.more_accounts.push(AccountCfg {
        id: 1,
        host: "127.0.0.1".into(),
        port: 1,
        user: "second@work.example".into(),
    });
    s.save(&dir).unwrap();
    engine::secrets::set("anthropic-key", "SECRET-API-KEY").unwrap();
    engine::secrets::set("imap-password-1", "SECRET-PW-1").unwrap();
    (c, dir)
}

// ───────────────────────── what is in the file ─────────────────────────
#[test]
fn a_backup_holds_what_you_did_and_nothing_secret() {
    let _k = keychain();
    let (c, dir) = busy_computer("export");
    let text = export_text(&c);
    let v: Value = serde_json::from_str(&text).expect("the file is JSON");
    assert_eq!(v["format"], FORMAT);
    assert_eq!(v["version"], 1);
    for must in [
        "shop.example",
        "call back on Tuesday",
        "Hi Priya, thanks for this.",
        "Renew passport",
        "2099-01-01 09:00",
        "bring two photos",
        "priya@acme.example",
        "second@work.example",
        "me@example.com",
        "22:00",
        "llama3.2",
        "Best, Sam",
    ] {
        assert!(text.contains(must), "the backup should hold {must:?}");
    }
    for never in [
        "SECRET-NTFY-TOPIC",
        "secret-ntfy.example",
        "SECRET-HUB-TOKEN",
        "SECRET-API-KEY",
        "SECRET-PW-1",
        "pw0",
        "10.9.9.9",
        "SECRET-BODY",
        "SECRET-SUBJECT-6",
        "SECRET-SUBJECT-7",
        "SECRET-SUBJECT-8",
        "SECRET-FOLLOWUP-SUBJECT",
        "someone@else.example",
        "dana@studio.example",
        "lee@club.example",
        "kim@nothing.example",
    ] {
        assert!(!text.contains(never), "a backup must never hold {never:?}");
    }
    // the only place a mail's subject appears is the correction you made of it (the ranker's learning)
    assert!(text.matches("SECRET-SUBJECT-5").count() == 1, "{text}");
    assert!(
        v["mail_state"].as_array().unwrap().len() == 3,
        "mail 5 (note, star, draft), 6 (done) and 7 (snoozed); mail 8 has nothing of yours: {}",
        v["mail_state"]
    );
    // nothing about the options that would send mail or open the network
    assert!(v["settings"].get("hub").is_none() && v["settings"].get("writeback").is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_file_is_new_never_over_another_and_named_by_time() {
    let _k = keychain();
    let (c, dir) = core("file");
    let folder = dir.join("out");
    std::fs::create_dir_all(&folder).unwrap();
    let a = engine::backup::export_in(&c, &folder).unwrap();
    std::fs::write(&a.path, "precious").unwrap();
    let b = engine::backup::export_in(&c, &folder).unwrap();
    assert_ne!(a.path, b.path, "a second backup never replaces the first");
    assert_eq!(std::fs::read_to_string(&a.path).unwrap(), "precious");
    let name = b.path.file_name().unwrap().to_string_lossy().to_string();
    assert!(
        name.starts_with("inbox-agent-backup-") && name.ends_with(".json"),
        "{name}"
    );
    assert_eq!(b.path.parent().unwrap(), folder);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_phone_cannot_make_or_restore_a_backup() {
    let _k = keychain();
    let (c, dir) = core("phone");
    for cmd in ["backup_export", "backup_import"] {
        let e = c
            .rpc_public(cmd, &json!({"json": doc(json!({})), "apply": true}))
            .unwrap_err()
            .to_string();
        assert!(e.contains("not available"), "{cmd}: {e}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

// ───────────────────────── restoring ─────────────────────────
#[test]
fn a_backup_restores_into_a_fresh_install() {
    let _k = keychain();
    let (a, dir_a) = busy_computer("roundtrip-a");
    let text = export_text(&a);
    let (b, dir_b) = core("roundtrip-b");
    let sb = b.store().unwrap();
    // the same messages were read again on the new computer before the restore; mail 5 and 7 come later
    mail(&sb, 6, "Dana <dana@studio.example>", "again", "again");
    mail(&sb, 8, "Kim <kim@nothing.example>", "again", "again");
    let r = restore(&b, &text, true, true).unwrap();
    assert_eq!(r["applied"], true);
    assert_eq!(r["rules_added"], 2);
    assert_eq!(r["sender_choices_added"], 1);
    assert_eq!(r["corrections_added"], 1);
    assert_eq!(r["own_tasks_added"], 2);
    assert_eq!(r["mail_applied"], 1, "mail 6 was there: finished again");
    assert_eq!(
        r["mail_parked"], 2,
        "mail 5 and 7 are not read yet: kept for when they are"
    );
    assert_eq!(r["mailboxes_added"], 1);
    // rules
    let rules = sb.rules();
    assert_eq!(rules.len(), 2);
    assert_eq!(
        (
            rules[0].field.as_str(),
            rules[0].text.as_str(),
            rules[0].priority
        ),
        ("domain", "shop.example", Some(1))
    );
    assert_eq!(rules[1].category.as_deref(), Some("finance"));
    // your own tasks, with their reminder, repeat, note and star
    let all = sb.all_tasks().unwrap();
    let passport = all
        .iter()
        .find(|t| t.subject == "Renew passport")
        .expect("restored");
    assert_eq!(
        (
            passport.priority,
            passport.due.as_str(),
            passport.remind.as_str(),
            passport.repeat.as_str(),
            passport.note.as_str(),
            passport.star
        ),
        (
            4,
            "2027-03-01",
            "2099-01-01 09:00",
            "weekly",
            "bring two photos",
            true
        )
    );
    assert_eq!(
        all.iter()
            .find(|t| t.subject == "Old chore")
            .unwrap()
            .status,
        "done"
    );
    // mail 6 is finished again; mail 8 (nothing of yours in the file) is untouched
    assert_eq!(sb.get(6).unwrap().status, "done");
    assert_eq!(sb.get(8).unwrap().status, "open");
    // the messages that arrive later get what you had done with them
    mail(&sb, 5, "Priya <priya@acme.example>", "again", "again");
    mail(&sb, 7, "Lee <lee@club.example>", "again", "again");
    let t5 = sb.get(5).unwrap();
    assert!(t5.star);
    assert_eq!(t5.note, "call back on Tuesday");
    assert_eq!(
        t5.draft.as_ref().map(|d| d.body.as_str()),
        Some("Hi Priya, thanks for this.")
    );
    assert_eq!(t5.pref, "important", "the sender choice came back too");
    assert!(sb.get(7).unwrap().snooze_until > days_ahead(99.0));
    assert_eq!(sb.parked_count(), 0, "applied once, then forgotten");
    assert_eq!(sb.recent_overrides(10).len(), 1, "the correction came back");
    // options and the second mailbox, without any password
    let s = b.settings();
    assert_eq!(
        (s.alerts.quiet_start.as_str(), s.alerts.digest_time.as_str()),
        ("22:00", "08:30")
    );
    assert_eq!(s.drafts.signature, "Best, Sam");
    assert_eq!(
        (
            s.ai.mode.as_str(),
            s.ai.ollama_model.as_str(),
            s.followups.days
        ),
        ("ollama", "llama3.2", 3)
    );
    assert_eq!(s.more_accounts.len(), 1);
    assert_eq!(s.more_accounts[0].user, "second@work.example");
    // the secrets and destinations of the first computer did not come along
    assert!(s.alerts.ntfy_topic.is_empty() && s.hub.token.is_empty() && !s.writeback);
    assert_eq!(s.alerts.ntfy_server, Settings::default().alerts.ntfy_server);
    assert_eq!(s.ai.ollama_url, Settings::default().ai.ollama_url);
    let _ = std::fs::remove_dir_all(&dir_a);
    let _ = std::fs::remove_dir_all(&dir_b);
}

#[test]
fn restoring_never_removes_or_overwrites_and_a_second_time_changes_nothing() {
    let _k = keychain();
    let (a, dir_a) = busy_computer("additive-a");
    let text = export_text(&a);
    let (b, dir_b) = core("additive-b");
    let sb = b.store().unwrap();
    // what is already here
    sb.set_rules(&[rule("mine", "sender", "boss@work.example", Some(5), None)])
        .unwrap();
    mail(&sb, 6, "Dana <dana@studio.example>", "mine", "mine");
    sb.set_note(6, "my own note").unwrap();
    mail(&sb, 5, "Priya <priya@acme.example>", "mine", "mine");
    sb.set_note(5, "already noted").unwrap();
    sb.set_draft(5, "my draft", "user").unwrap();
    sb.set_pref("priya@acme.example", "mute").unwrap();
    let r = restore(&b, &text, true, false).unwrap();
    assert_eq!(
        sb.rules().len(),
        3,
        "yours first, then the two from the file"
    );
    assert_eq!(sb.rules()[0].id, "mine");
    assert_eq!(sb.get(6).unwrap().note, "my own note", "your note stays");
    assert_eq!(
        sb.get(6).unwrap().status,
        "done",
        "what the file says you finished is finished"
    );
    let t5 = sb.get(5).unwrap();
    assert_eq!(t5.note, "already noted");
    assert_eq!(t5.draft.unwrap().body, "my draft");
    assert_eq!(
        t5.pref, "mute",
        "a choice you already made about a sender is not changed"
    );
    assert_eq!(r["sender_choices_skipped"], 1);
    let tasks_after_first = sb.all_tasks().unwrap().len();
    // the same file again: nothing new
    let r2 = restore(&b, &text, true, false).unwrap();
    for key in [
        "rules_added",
        "sender_choices_added",
        "corrections_added",
        "own_tasks_added",
        "mail_parked",
        "mail_applied",
    ] {
        assert_eq!(r2[key], 0, "{key}: {r2}");
    }
    assert_eq!(r2["own_tasks_skipped"], 2);
    assert_eq!(sb.all_tasks().unwrap().len(), tasks_after_first);
    assert_eq!(sb.rules().len(), 3);
    assert_eq!(
        sb.parked_count(),
        1,
        "mail 7's state is kept once, not twice"
    );
    let _ = std::fs::remove_dir_all(&dir_a);
    let _ = std::fs::remove_dir_all(&dir_b);
}

#[test]
fn a_preview_writes_nothing_and_tells_what_the_restore_then_does() {
    let _k = keychain();
    let (a, dir_a) = busy_computer("preview-a");
    let text = export_text(&a);
    let (b, dir_b) = core("preview-b");
    let sb = b.store().unwrap();
    mail(&sb, 6, "Dana <dana@studio.example>", "mine", "mine");
    let before = (
        sb.all_tasks().unwrap().len(),
        sb.rules().len(),
        sb.parked_count(),
        sb.recent_overrides(10).len(),
        b.settings().more_accounts.len(),
    );
    let preview = restore(&b, &text, false, true).unwrap();
    assert_eq!(preview["applied"], false);
    let after = (
        sb.all_tasks().unwrap().len(),
        sb.rules().len(),
        sb.parked_count(),
        sb.recent_overrides(10).len(),
        b.settings().more_accounts.len(),
    );
    assert_eq!(before, after, "a preview changes nothing");
    assert_eq!(sb.get(6).unwrap().status, "open");
    let real = restore(&b, &text, true, true).unwrap();
    for key in [
        "rules_added",
        "sender_choices_added",
        "corrections_added",
        "own_tasks_added",
        "mail_applied",
        "mail_parked",
        "mail_skipped_no_mailbox",
        "mailboxes_added",
        "reminders_dropped",
        "options_changed",
    ] {
        assert_eq!(
            preview[key], real[key],
            "the preview said {key} = {} but the restore did {}",
            preview[key], real[key]
        );
    }
    let _ = std::fs::remove_dir_all(&dir_a);
    let _ = std::fs::remove_dir_all(&dir_b);
}

#[test]
fn state_of_mail_never_goes_backwards_and_nothing_is_queued_for_the_mailbox() {
    let _k = keychain();
    let (c, dir) = core("monotone");
    let st = c.store().unwrap();
    let mut s = c.settings();
    s.writeback = true;
    s.save(&dir).unwrap();
    mail(&st, 5, "A <a@x.example>", "one", "one");
    mail(&st, 6, "B <b@x.example>", "two", "two");
    mail(&st, 7, "C <c@x.example>", "three", "three");
    st.set_done(6, true).unwrap(); // finished here already
    st.set_star(7, true).unwrap();
    st.set_snooze(7, days_ahead(200.0)).unwrap(); // snoozed later than the file says
    let queued = st.pending(1000).len();
    let file = doc(json!({
        "mailboxes": [{"host": "127.0.0.1", "port": 1, "user": "me@example.com"}],
        "mail_state": [
            {"m": 0, "message_id": "<m5@x>", "done": true, "star": true},
            {"m": 0, "message_id": "<m6@x>", "done": false, "star": false},
            {"m": 0, "message_id": "<m7@x>", "snooze_until": days_ahead(100.0)}
        ]
    }));
    let r = restore(&c, &file, true, false).unwrap();
    assert_eq!(
        r["mail_applied"], 1,
        "only mail 5 had anything to gain: {r}"
    );
    assert_eq!(r["mail_unchanged"], 2, "{r}");
    assert_eq!(st.get(5).unwrap().status, "done");
    assert!(st.get(5).unwrap().star);
    assert_eq!(
        st.get(6).unwrap().status,
        "done",
        "a file that says open does not reopen it"
    );
    assert!(st.get(7).unwrap().star, "a star is not taken away");
    assert!(
        st.get(7).unwrap().snooze_until > days_ahead(199.0),
        "the later snooze wins"
    );
    assert_eq!(
        st.pending(1000).len(),
        queued,
        "restoring 'done' archives nothing in the mailbox"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn entries_for_a_mailbox_that_is_not_set_up_here_are_counted_and_skipped() {
    let _k = keychain();
    let (c, dir) = core("unknown-mailbox");
    let file = doc(json!({
        "mailboxes": [
            {"host": "127.0.0.1", "port": 1, "user": "me@example.com"},
            {"host": "imap.elsewhere.example", "port": 993, "user": "someone@elsewhere.example"}
        ],
        "mail_state": [
            {"m": 0, "message_id": "<a@x>", "done": true},
            {"m": 1, "message_id": "<b@x>", "done": true},
            {"m": 1, "message_id": "<c@x>", "star": true}
        ]
    }));
    let r = restore(&c, &file, true, false).unwrap();
    assert_eq!(r["mail_parked"], 1);
    assert_eq!(r["mail_skipped_no_mailbox"], 2);
    assert_eq!(c.store().unwrap().parked_count(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn parked_state_goes_with_its_mailbox_expires_and_is_bounded() {
    let _k = keychain();
    let (c, dir) = core("parked");
    let st = c.store().unwrap();
    let file = doc(json!({
        "mailboxes": [{"host": "127.0.0.1", "port": 1, "user": "me@example.com"}],
        "mail_state": [{"m": 0, "message_id": "<a@x>", "done": true}]
    }));
    restore(&c, &file, true, false).unwrap();
    assert_eq!(st.parked_count(), 1);
    st.age_parked(100.0).unwrap(); // pretend it was parked 100 days ago
    st.prune(90, 30).unwrap();
    assert_eq!(st.parked_count(), 0, "parked state expires after 90 days");
    restore(&c, &file, true, false).unwrap();
    assert_eq!(st.parked_count(), 1);
    st.remove_mailbox(0).unwrap();
    assert_eq!(st.parked_count(), 0, "and goes with its mailbox");
    restore(&c, &file, true, false).unwrap();
    st.wipe_all().unwrap();
    assert_eq!(st.parked_count(), 0, "and with a sign-out");
    // the bound: 5,000 entries at most in one file, and no more than that waiting
    let many: Vec<Value> = (0..5000)
        .map(|i| json!({"m": 0, "message_id": format!("<n{i}@x>"), "done": true}))
        .collect();
    let r = restore(&c, &doc(json!({"mailboxes": [{"host": "127.0.0.1", "port": 1, "user": "me@example.com"}], "mail_state": many})), true, false).unwrap();
    assert_eq!(r["mail_parked"], 5000);
    let more: Vec<Value> = (0..10)
        .map(|i| json!({"m": 0, "message_id": format!("<extra{i}@x>"), "done": true}))
        .collect();
    let r = restore(&c, &doc(json!({"mailboxes": [{"host": "127.0.0.1", "port": 1, "user": "me@example.com"}], "mail_state": more})), true, false).unwrap();
    assert_eq!(r["mail_parked"], 0, "{r}");
    assert_eq!(st.parked_count(), 5000);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn rules_respect_the_limit_and_skip_what_is_already_there() {
    let _k = keychain();
    let (c, dir) = core("rule-limit");
    let st = c.store().unwrap();
    let mine: Vec<Rule> = (0..48)
        .map(|i| {
            rule(
                &format!("m{i}"),
                "subject",
                &format!("mine {i}"),
                Some(2),
                None,
            )
        })
        .collect();
    st.set_rules(&mine).unwrap();
    let file = doc(json!({"rules": [
        {"id": "a", "field": "subject", "text": "mine 3", "priority": 2},
        {"id": "b", "field": "subject", "text": "new one", "priority": 4},
        {"id": "c", "field": "subject", "text": "newer one", "priority": 4},
        {"id": "d", "field": "subject", "text": "newest one", "priority": 4}
    ]}));
    let r = restore(&c, &file, true, false).unwrap();
    assert_eq!(r["rules_added"], 2, "up to the limit of 50: {r}");
    assert_eq!(
        r["rules_skipped"], 2,
        "one was already there, one did not fit: {r}"
    );
    assert_eq!(st.rules().len(), 50);
    let ids: std::collections::HashSet<String> = st.rules().into_iter().map(|r| r.id).collect();
    assert_eq!(ids.len(), 50, "every rule has its own id");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_reminder_in_the_past_is_dropped_and_a_repeat_moves_on() {
    let _k = keychain();
    let (c, dir) = core("reminders");
    let file = doc(json!({"own_tasks": [
        {"title": "past once", "priority": 3, "remind": "2020-01-01 09:00"},
        {"title": "past weekly", "priority": 3, "remind": "2020-01-01 09:00", "repeat": "weekly", "repeat_base": "2020-01-01 09:00"},
        {"title": "future", "priority": 3, "remind": "2099-05-05 10:30"}
    ]}));
    let r = restore(&c, &file, true, false).unwrap();
    assert_eq!(r["own_tasks_added"], 3);
    assert_eq!(
        r["reminders_dropped"], 1,
        "only the one that cannot repeat: {r}"
    );
    let all = c.store().unwrap().all_tasks().unwrap();
    let get = |t: &str| all.iter().find(|x| x.subject == t).unwrap().clone();
    assert_eq!(get("past once").remind, "");
    let weekly = get("past weekly");
    assert_eq!(weekly.repeat, "weekly");
    assert!(
        weekly.remind.as_str()
            > engine::reminders::fmt_local(engine::reminders::now_local()).as_str(),
        "next time: {}",
        weekly.remind
    );
    assert!(
        weekly.remind.starts_with("20") && &weekly.remind[11..] == "09:00",
        "same time of day: {}",
        weekly.remind
    );
    assert_eq!(get("future").remind, "2099-05-05 10:30");
    let _ = std::fs::remove_dir_all(&dir);
}

// ───────────────────────── options and mailboxes ─────────────────────────
#[test]
fn options_and_mailboxes_come_back_only_when_asked_and_never_a_password() {
    let _k = keychain();
    let (a, dir_a) = busy_computer("options-a");
    let text = export_text(&a);
    let (b, dir_b) = core("options-b");
    engine::secrets::delete("imap-password-1");
    let r = restore(&b, &text, true, false).unwrap();
    assert_eq!(r["mailboxes_added"], 0);
    assert_eq!(r["options_changed"], json!([]));
    assert!(
        b.settings().more_accounts.is_empty(),
        "off unless you tick it"
    );
    assert_eq!(b.settings().alerts.quiet_start, "");
    let r = restore(&b, &text, true, true).unwrap();
    assert_eq!(r["mailboxes_added"], 1);
    assert!(!r["options_changed"].as_array().unwrap().is_empty());
    assert!(
        engine::secrets::get("imap-password-1").is_none(),
        "a restored mailbox has no password until you paste one"
    );
    let st = b.rpc("state", &json!({})).unwrap();
    let list = st["accounts"].as_array().unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[1]["status"], "no password", "{list:?}");
    // again: nothing more to add
    let r = restore(&b, &text, true, true).unwrap();
    assert_eq!(r["mailboxes_added"], 0);
    assert_eq!(r["mailboxes_skipped"], 2, "both are set up now: {r}");
    let _ = std::fs::remove_dir_all(&dir_a);
    let _ = std::fs::remove_dir_all(&dir_b);
}

#[test]
fn restored_mailboxes_respect_the_limit_of_five_and_never_touch_the_first() {
    let _k = keychain();
    let (c, dir) = core("mailbox-limit");
    let boxes: Vec<Value> = (0..7)
        .map(|i| json!({"host": "127.0.0.1", "port": 1, "user": format!("b{i}@work.example")}))
        .collect();
    let r = restore(&c, &doc(json!({"mailboxes": boxes})), true, true);
    // seven mailboxes in a file is more than this app can hold: the file is refused, not trimmed
    assert!(r.is_err(), "{r:?}");
    let boxes: Vec<Value> = [
        json!({"host": "127.0.0.1", "port": 1, "user": "ME@example.com"}), // the first one again, other letter case
        json!({"host": "127.0.0.1", "port": 1, "user": "b1@work.example"}),
        json!({"host": "127.0.0.1", "port": 1, "user": "b2@work.example"}),
        json!({"host": "127.0.0.1", "port": 1, "user": "b3@work.example"}),
        json!({"host": "127.0.0.1", "port": 1, "user": "b4@work.example"}),
    ]
    .into();
    let r = restore(&c, &doc(json!({"mailboxes": boxes})), true, true).unwrap();
    assert_eq!(r["mailboxes_added"], 4);
    assert_eq!(r["mailboxes_skipped"], 1);
    let s = c.settings();
    assert_eq!(
        s.account.user, "me@example.com",
        "the first mailbox is not changed"
    );
    assert_eq!(s.mailboxes().len(), 5);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_file_cannot_turn_on_the_hub_write_back_or_a_network_destination() {
    let _k = keychain();
    let (c, dir) = core("hostile-options");
    let before = c.settings();
    let file = doc(json!({"settings": {
        "onboarded": false,
        "account": {"id": 0, "host": "evil.example", "user": "x@evil.example", "port": 993},
        "more_accounts": [{"id": 1, "host": "evil.example", "user": "y@evil.example", "port": 993}],
        "hub": {"enabled": true, "token": "evil", "port": 9999},
        "writeback": true,
        "first_run_limit": 100000,
        "updates": {"check": false},
        "ai": {"mode": "anthropic", "ollama_url": "http://evil.example:1", "ollama_model": "m", "anthropic_model": "evil-model"},
        "alerts": {"desktop": false, "ntfy_server": "https://evil.example", "ntfy_topic": "evil", "quiet_start": "21:00", "quiet_end": "06:00"},
        "drafts": {"signature": "sig", "daily_cap": 3, "tone": "formal"}
    }}));
    restore(&c, &file, true, true).unwrap();
    let s = c.settings();
    assert!(s.onboarded);
    assert_eq!(s.account.host, before.account.host);
    assert!(s.more_accounts.is_empty());
    assert!(!s.hub.enabled && s.hub.token.is_empty() && s.hub.port == before.hub.port);
    assert!(!s.writeback);
    assert_eq!(s.first_run_limit, before.first_run_limit);
    assert_eq!(s.alerts.ntfy_server, before.alerts.ntfy_server);
    assert_eq!(s.alerts.ntfy_topic, "");
    assert_eq!(s.ai.ollama_url, before.ai.ollama_url);
    assert_eq!(s.ai.anthropic_model, before.ai.anthropic_model);
    // what is on the allow-list did come
    assert_eq!(
        (s.ai.mode.as_str(), s.ai.ollama_model.as_str()),
        ("rules", "m") // the cloud ranking is not something a file can choose
    );
    assert_eq!(
        (s.alerts.desktop, s.alerts.quiet_start.as_str()),
        (false, "21:00")
    );
    assert_eq!(
        (
            s.drafts.signature.as_str(),
            s.drafts.daily_cap,
            s.drafts.tone.as_str()
        ),
        ("sig", 3, "formal")
    );
    assert!(!s.updates.check);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn restoring_needs_setup_to_be_finished() {
    let _k = keychain();
    let dir = fresh("not-set-up");
    let c = Core::new(dir.clone(), Arc::new(|_, _| {}));
    let e = restore(&c, &doc(json!({})), true, false)
        .unwrap_err()
        .to_string();
    assert!(e.contains("Finish setting up"), "{e}");
    let _ = std::fs::remove_dir_all(&dir);
}

// ───────────────────────── a file you cannot trust ─────────────────────────
#[test]
fn a_file_that_is_not_ours_or_is_too_new_is_refused() {
    let _k = keychain();
    let (c, dir) = core("not-ours");
    for (text, why) in [
        ("".to_string(), "empty"),
        ("not json at all".to_string(), "garbage"),
        ("[1,2,3]".to_string(), "a list"),
        ("{}".to_string(), "no format"),
        (
            json!({"format": "something-else", "version": 1}).to_string(),
            "another format",
        ),
        (json!({"format": FORMAT}).to_string(), "no version"),
        (
            json!({"format": FORMAT, "version": 2}).to_string(),
            "a newer version",
        ),
        (
            json!({"format": FORMAT, "version": 0}).to_string(),
            "version 0",
        ),
        (
            json!({"format": FORMAT, "version": "1"}).to_string(),
            "a version that is text",
        ),
        (
            json!({"format": FORMAT, "version": 1, "rules": "x"}).to_string(),
            "a list that is text",
        ),
    ] {
        let e = restore(&c, &text, true, true);
        assert!(e.is_err(), "{why} should be refused, got {e:?}");
    }
    let e = restore(
        &c,
        &json!({"format": FORMAT, "version": 2}).to_string(),
        true,
        true,
    )
    .unwrap_err()
    .to_string();
    assert!(e.contains("newer version"), "{e}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_file_over_a_limit_is_refused_whole() {
    let _k = keychain();
    let (c, dir) = core("limits");
    let big = "x".repeat(8 * 1024 * 1024 + 1);
    assert!(restore(&c, &big, true, true).is_err(), "over 8 MB");
    let rules: Vec<Value> = (0..51).map(|i| json!({"id": format!("r{i}"), "field": "subject", "text": format!("t{i}"), "priority": 2})).collect();
    assert!(
        restore(&c, &doc(json!({"rules": rules})), true, false).is_err(),
        "51 rules"
    );
    let tasks: Vec<Value> = (0..2001)
        .map(|i| json!({"title": format!("t{i}"), "priority": 3}))
        .collect();
    assert!(
        restore(&c, &doc(json!({"own_tasks": tasks})), true, false).is_err(),
        "2,001 tasks"
    );
    let prefs: Vec<Value> = (0..5001)
        .map(|i| json!({"addr": format!("a{i}@x.example"), "level": "mute"}))
        .collect();
    assert!(
        restore(&c, &doc(json!({"sender_choices": prefs})), true, false).is_err(),
        "5,001 sender choices"
    );
    let fixes: Vec<Value> = (0..2001).map(|i| json!({"sender": "s", "subject": format!("s{i}"), "old": 3, "new": 4, "addr": "a@x.example", "at": 1.0})).collect();
    assert!(
        restore(&c, &doc(json!({"corrections": fixes})), true, false).is_err(),
        "2,001 corrections"
    );
    let state: Vec<Value> = (0..5001)
        .map(|i| json!({"m": 0, "message_id": format!("<s{i}@x>"), "done": true}))
        .collect();
    assert!(
        restore(&c, &doc(json!({"mailboxes": [{"host": "127.0.0.1", "port": 1, "user": "me@example.com"}], "mail_state": state})), true, false).is_err(),
        "5,001 mail entries"
    );
    let st = c.store().unwrap();
    assert!(st.rules().is_empty() && st.all_tasks().unwrap().is_empty() && st.parked_count() == 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn one_bad_value_refuses_the_whole_restore_and_nothing_is_half_done() {
    let _k = keychain();
    let (c, dir) = core("atomic");
    let st = c.store().unwrap();
    let good = |extra: Value| -> String {
        let mut v: Value = serde_json::from_str(&doc(json!({
            "rules": [{"id": "ok", "field": "subject", "text": "fine", "priority": 2}],
            "own_tasks": [{"title": "fine task", "priority": 3}],
            "sender_choices": [{"addr": "a@x.example", "level": "mute"}]
        })))
        .unwrap();
        for (k, x) in extra.as_object().unwrap() {
            v[k] = x.clone();
        }
        v.to_string()
    };
    let bad_files = [
        (
            "a rule that does nothing",
            good(
                json!({"rules": [{"id": "ok", "field": "subject", "text": "fine", "priority": 2}, {"id": "bad", "field": "subject", "text": "nothing"}]}),
            ),
        ),
        (
            "a rule with a bad field",
            good(json!({"rules": [{"id": "bad", "field": "body", "text": "x", "priority": 2}]})),
        ),
        (
            "a task with no title",
            good(
                json!({"own_tasks": [{"title": "fine task", "priority": 3}, {"title": "  ", "priority": 3}]}),
            ),
        ),
        (
            "a task with a control character",
            good(json!({"own_tasks": [{"title": "a\u{0}b", "priority": 3}]})),
        ),
        (
            "a title over 200 characters",
            good(json!({"own_tasks": [{"title": "t".repeat(201), "priority": 3}]})),
        ),
        (
            "a priority of 9",
            good(json!({"own_tasks": [{"title": "x", "priority": 9}]})),
        ),
        (
            "a due date that is not a date",
            good(json!({"own_tasks": [{"title": "x", "priority": 3, "due": "2026-02-30"}]})),
        ),
        (
            "a reminder that is not a time",
            good(json!({"own_tasks": [{"title": "x", "priority": 3, "remind": "tomorrow"}]})),
        ),
        (
            "a repeat nobody offers",
            good(
                json!({"own_tasks": [{"title": "x", "priority": 3, "remind": "2099-01-01 09:00", "repeat": "hourly"}]}),
            ),
        ),
        (
            "a note over 2,000 characters",
            good(json!({"own_tasks": [{"title": "x", "priority": 3, "note": "n".repeat(2001)}]})),
        ),
        (
            "a sender choice that is not a choice",
            good(json!({"sender_choices": [{"addr": "a@x.example", "level": "super"}]})),
        ),
        (
            "an empty sender address",
            good(json!({"sender_choices": [{"addr": "", "level": "mute"}]})),
        ),
        (
            "a mailbox entry out of range",
            good(
                json!({"mailboxes": [], "mail_state": [{"m": 3, "message_id": "<a@x>", "done": true}]}),
            ),
        ),
        (
            "a message id with a line break",
            good(
                json!({"mailboxes": [{"host": "127.0.0.1", "port": 1, "user": "me@example.com"}], "mail_state": [{"m": 0, "message_id": "<a\r\nb@x>", "done": true}]}),
            ),
        ),
        (
            "a draft over 8,000 characters",
            good(
                json!({"mailboxes": [{"host": "127.0.0.1", "port": 1, "user": "me@example.com"}], "mail_state": [{"m": 0, "message_id": "<a@x>", "draft": {"body": "d".repeat(8001), "source": "user"}}]}),
            ),
        ),
        (
            "a draft of an unknown source",
            good(
                json!({"mailboxes": [{"host": "127.0.0.1", "port": 1, "user": "me@example.com"}], "mail_state": [{"m": 0, "message_id": "<a@x>", "draft": {"body": "d", "source": "robot"}}]}),
            ),
        ),
        (
            "a correction with a priority of 0",
            good(
                json!({"corrections": [{"sender": "s", "subject": "t", "old": 0, "new": 4, "addr": "a@x.example", "at": 1.0}]}),
            ),
        ),
        (
            "options that fail the settings check",
            good(json!({"settings": {"alerts": {"quiet_start": "25:99", "quiet_end": "07:00"}}})),
        ),
        (
            "a mailbox with a bad server name",
            good(json!({"mailboxes": [{"host": "bad host!", "port": 1, "user": "x@y.example"}]})),
        ),
    ];
    for (why, text) in bad_files {
        let e = restore(&c, &text, true, true);
        assert!(e.is_err(), "{why} should refuse the file, got {e:?}");
        assert!(
            st.rules().is_empty() && st.all_tasks().unwrap().is_empty() && st.parked_count() == 0,
            "{why}: something was applied before the refusal"
        );
        assert!(
            c.settings().alerts.quiet_start.is_empty() && c.settings().more_accounts.is_empty(),
            "{why}"
        );
    }
    // and the same file without the bad part is fine
    assert!(restore(&c, &good(json!({})), true, true).is_ok());
    assert_eq!(st.rules().len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn rules_in_a_file_get_new_ids_so_a_repeated_id_cannot_collide() {
    let _k = keychain();
    let (c, dir) = core("rule-ids");
    let st = c.store().unwrap();
    st.set_rules(&[rule("keep", "subject", "keep me", Some(2), None)])
        .unwrap();
    let file = doc(json!({"rules": [
        {"id": "keep", "field": "subject", "text": "one", "priority": 2},
        {"id": "keep", "field": "subject", "text": "two", "priority": 2}
    ]}));
    let r = restore(&c, &file, true, false).unwrap();
    assert_eq!(r["rules_added"], 2);
    let ids: std::collections::HashSet<String> = st.rules().into_iter().map(|r| r.id).collect();
    assert_eq!(
        ids.len(),
        3,
        "every rule has its own id, even when the file repeated yours"
    );
    assert_eq!(st.rules()[0].text, "keep me");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_failure_in_the_middle_of_the_database_part_rolls_the_whole_part_back() {
    let _k = keychain();
    let (c, dir) = core("rollback");
    let st = c.store().unwrap();
    st.set_rules(&[rule("keep", "subject", "keep me", Some(2), None)])
        .unwrap();
    // make the database refuse one particular task, as a full disk or a locked file could at any step
    rusqlite::Connection::open(dir.join("agent.db"))
        .unwrap()
        .execute_batch(
            "CREATE TRIGGER poison BEFORE INSERT ON tasks WHEN NEW.subject='poison' BEGIN SELECT RAISE(ABORT,'poison'); END;",
        )
        .unwrap();
    let file = doc(json!({
        "rules": [{"id": "a", "field": "subject", "text": "new rule", "priority": 2}],
        "sender_choices": [{"addr": "a@x.example", "level": "mute"}],
        "own_tasks": [{"title": "fine", "priority": 3}, {"title": "poison", "priority": 3}]
    }));
    assert!(restore(&c, &file, true, false).is_err());
    assert_eq!(
        st.rules().len(),
        1,
        "the rule that went in first is gone again"
    );
    assert!(
        st.all_tasks().unwrap().iter().all(|t| t.subject != "fine"),
        "so is the task"
    );
    assert!(st.get(1).is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

// ───────────────────────── the mailbox's own data stays out ─────────────────────────
#[test]
fn state_of_mail_is_only_applied_inside_the_mailbox_it_names() {
    let _k = keychain();
    let (c, dir) = core("block");
    let mut s = c.settings();
    s.more_accounts.push(AccountCfg {
        id: 1,
        host: "127.0.0.1".into(),
        port: 1,
        user: "second@work.example".into(),
    });
    s.save(&dir).unwrap();
    let st = c.store().unwrap();
    mail(&st, 5, "A <a@x.example>", "first mailbox", "x");
    let s1 = c.store_for(1).unwrap();
    mail(&s1, 5, "A <a@x.example>", "second mailbox", "x"); // the same message in both mailboxes
    let file = doc(json!({
        "mailboxes": [{"host": "127.0.0.1", "port": 1, "user": "second@work.example"}],
        "mail_state": [{"m": 0, "message_id": "<m5@x>", "done": true}]
    }));
    let r = restore(&c, &file, true, false).unwrap();
    assert_eq!(r["mail_applied"], 1);
    assert_eq!(
        st.get(global_uid(1, 5)).unwrap().status,
        "done",
        "the second mailbox's copy"
    );
    assert_eq!(
        st.get(5).unwrap().status,
        "open",
        "the first mailbox's copy is not touched"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// ───────────────────────── gaps the mutation checks found ─────────────────────────
#[test]
fn a_big_file_that_is_valid_json_is_still_refused() {
    let _k = keychain();
    let (c, dir) = core("big-valid");
    let text =
        json!({"format": FORMAT, "version": 1, "padding": "x".repeat(8 * 1024 * 1024)}).to_string();
    assert!(text.len() > 8 * 1024 * 1024);
    let e = restore(&c, &text, true, false).unwrap_err().to_string();
    assert!(e.contains("too large"), "{e}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn bad_values_are_refused_whether_or_not_options_were_asked_for() {
    let _k = keychain();
    let (c, dir) = core("bad-either-way");
    for with_options in [false, true] {
        for (why, file) in [
            (
                "a mailbox with a bad server name",
                doc(
                    json!({"mailboxes": [{"host": "bad host!", "port": 1, "user": "x@y.example"}]}),
                ),
            ),
            (
                "a mailbox with no address",
                doc(json!({"mailboxes": [{"host": "imap.example.com", "port": 993, "user": ""}]})),
            ),
            (
                "an option no settings screen accepts",
                doc(
                    json!({"settings": {"alerts": {"quiet_start": "25:99", "quiet_end": "07:00"}}}),
                ),
            ),
            (
                "a sender choice that is not an address",
                doc(json!({"sender_choices": [{"addr": "nobody", "level": "mute"}]})),
            ),
            (
                "a sender choice with a space in it",
                doc(json!({"sender_choices": [{"addr": "a b@x.example", "level": "mute"}]})),
            ),
        ] {
            assert!(
                restore(&c, &file, true, with_options).is_err(),
                "{why} (options: {with_options})"
            );
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_finished_task_of_yours_keeps_no_reminder_and_finished_mail_is_not_snoozed() {
    let _k = keychain();
    let (c, dir) = core("finished");
    let st = c.store().unwrap();
    mail(&st, 9, "A <a@x.example>", "nine", "nine");
    let file = doc(json!({
        "mailboxes": [{"host": "127.0.0.1", "port": 1, "user": "me@example.com"}],
        "own_tasks": [{"title": "done with a reminder", "priority": 3, "remind": "2099-05-05 10:30", "repeat": "weekly", "status": "done"}],
        "mail_state": [{"m": 0, "message_id": "<m9@x>", "done": true, "snooze_until": days_ahead(100.0)}]
    }));
    restore(&c, &file, true, false).unwrap();
    let t = st
        .all_tasks()
        .unwrap()
        .into_iter()
        .find(|t| t.subject == "done with a reminder")
        .unwrap();
    assert_eq!(
        (t.status.as_str(), t.remind.as_str(), t.repeat.as_str()),
        ("done", "", "")
    );
    let m = st.get(9).unwrap();
    assert_eq!(m.status, "done");
    assert_eq!(m.snooze_until, 0.0, "finished mail is not also snoozed");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn entries_with_nothing_of_yours_in_them_are_not_kept() {
    let _k = keychain();
    let (c, dir) = core("empty-entries");
    let file = doc(json!({
        "mailboxes": [{"host": "127.0.0.1", "port": 1, "user": "me@example.com"}],
        "mail_state": [{"m": 0, "message_id": "<n1@x>"}, {"m": 0, "message_id": "<n2@x>", "snooze_until": 0}]
    }));
    let r = restore(&c, &file, true, false).unwrap();
    assert_eq!(
        (r["mail_parked"].as_u64(), r["mail_unchanged"].as_u64()),
        (Some(0), Some(2)),
        "{r}"
    );
    assert_eq!(c.store().unwrap().parked_count(), 0);
    let _ = std::fs::remove_dir_all(&dir);
}

// ───────────────────────── found by the reviews of this run ─────────────────────────
#[test]
fn a_file_cannot_switch_ranking_to_the_cloud_service_and_the_preview_says_so() {
    let _k = keychain();
    let (c, dir) = core("no-cloud");
    engine::secrets::set("anthropic-key", "SECRET-API-KEY").unwrap();
    let file = doc(
        json!({"settings": {"ai": {"mode": "anthropic", "ollama_model": "llama3.2"}, "drafts": {"auto": true}}}),
    );
    let preview = restore(&c, &file, false, true).unwrap();
    assert_eq!(preview["options_skipped"], json!(["ai.mode"]), "{preview}");
    let real = restore(&c, &file, true, true).unwrap();
    assert_eq!(real["options_skipped"], json!(["ai.mode"]));
    let s = c.settings();
    assert_eq!(
        s.ai.mode, "rules",
        "choosing the paid cloud service is for you to do in Settings"
    );
    assert_eq!(
        s.ai.ollama_model, "llama3.2",
        "the rest of the section came"
    );
    assert!(s.drafts.auto);
    // a local model is fine
    let file = doc(json!({"settings": {"ai": {"mode": "ollama"}}}));
    restore(&c, &file, true, true).unwrap();
    assert_eq!(c.settings().ai.mode, "ollama");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn restoring_a_backup_onto_the_computer_it_came_from_adds_nothing() {
    let _k = keychain();
    let (c, dir) = busy_computer("same-computer");
    let st = c.store().unwrap();
    // the awkward ones: a finished task that still has a reminder to come, and a correction of mail with a very long subject
    let paid = st
        .add_local_task("Pay rent", 3, "2027-01-01")
        .unwrap()
        .unwrap();
    st.set_reminder(paid, "2099-01-01 09:00", "", engine::reminders::now_local())
        .unwrap();
    st.set_done(paid, true).unwrap();
    mail(&st, 20, "Long <long@x.example>", &"s".repeat(700), "b");
    let was = st.get(20).unwrap().priority;
    st.set_priority(20, if was == 2 { 4 } else { 2 }).unwrap();
    let text = export_text(&c);
    let r = restore(&c, &text, true, true).unwrap();
    for key in [
        "rules_added",
        "sender_choices_added",
        "corrections_added",
        "own_tasks_added",
        "mail_applied",
        "mail_parked",
        "mailboxes_added",
    ] {
        assert_eq!(r[key], 0, "{key}: {r}");
    }
    // and a repeating task that is already due moves to its next time once, then stays
    let file = doc(json!({"own_tasks": [
        {"title": "Rent", "priority": 3, "due": "2020-01-03", "remind": "2020-01-01 09:00", "repeat": "monthly", "repeat_base": "2020-01-01 09:00"}
    ]}));
    let r1 = restore(&c, &file, true, false).unwrap();
    assert_eq!(r1["own_tasks_added"], 1);
    let t = st
        .all_tasks()
        .unwrap()
        .into_iter()
        .find(|t| t.subject == "Rent")
        .unwrap();
    assert!(
        t.remind.as_str() > engine::reminders::fmt_local(engine::reminders::now_local()).as_str(),
        "{}",
        t.remind
    );
    assert!(
        t.due.as_str() >= &t.remind[..10],
        "the due date moved with the reminder, so it is not overdue: due {} remind {}",
        t.due,
        t.remind
    );
    let r2 = restore(&c, &file, true, false).unwrap();
    assert_eq!(
        (
            r2["own_tasks_added"].as_u64(),
            r2["own_tasks_skipped"].as_u64()
        ),
        (Some(0), Some(1)),
        "{r2}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_correction_dated_far_ahead_is_refused_and_one_from_now_is_not() {
    let _k = keychain();
    let (c, dir) = core("future-correction-2");
    let at = |t: f64| {
        doc(
            json!({"corrections": [{"sender": "S", "subject": "T", "old": 2, "new": 4, "addr": "a@x.example", "at": t}]}),
        )
    };
    assert!(restore(&c, &at(4_000_000_000.0), true, false).is_err());
    let now = engine::backup::now_secs();
    restore(&c, &at(now - 5.0), true, false).unwrap();
    let r = restore(&c, &at(now - 5.0), true, false).unwrap();
    assert_eq!(r["corrections_added"], 0, "the same one is not added again");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_restored_correction_belongs_to_no_mailbox_so_removing_one_keeps_it() {
    let _k = keychain();
    let (c, dir) = core("correction-owner");
    let file = doc(
        json!({"corrections": [{"sender": "S", "subject": "T", "old": 2, "new": 4, "addr": "a@x.example", "at": 1_700_000_000.0}]}),
    );
    restore(&c, &file, true, false).unwrap();
    let st = c.store().unwrap();
    assert_eq!(st.recent_overrides(10).len(), 1);
    st.remove_mailbox(0).unwrap();
    assert_eq!(
        st.recent_overrides(10).len(),
        1,
        "what you taught the ranker is not one mailbox's"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn only_reply_drafts_you_edited_go_in_a_backup() {
    let _k = keychain();
    let (c, dir) = core("drafts");
    let st = c.store().unwrap();
    mail(&st, 5, "A <a@x.example>", "five", "five");
    mail(&st, 6, "B <b@x.example>", "six", "six");
    mail(&st, 7, "C <c@x.example>", "seven", "seven");
    st.set_draft(5, "WRITTEN-BY-THE-MODEL about the contract", "ai")
        .unwrap();
    st.set_draft(6, "TEMPLATE-FOR-Bea", "template").unwrap();
    st.set_draft(7, "MY OWN WORDS", "user").unwrap();
    let text = export_text(&c);
    assert!(text.contains("MY OWN WORDS"));
    assert!(
        !text.contains("WRITTEN-BY-THE-MODEL") && !text.contains("TEMPLATE-FOR-Bea"),
        "{text}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_restored_mailbox_starts_clean_even_if_something_was_left_in_its_number() {
    let _k = keychain();
    let (c, dir) = core("restored-clean");
    let ghost = c.store_for(1).unwrap();
    mail(&ghost, 5, "X <x@y.example>", "left behind", "x");
    ghost.set_last_uid(500).unwrap();
    let file = doc(json!({
        "settings": {"alerts": {"quiet_start": "22:00", "quiet_end": "07:00"}},
        "mailboxes": [{"host": "127.0.0.1", "port": 1, "user": "me@example.com"}, {"host": "127.0.0.1", "port": 1, "user": "second@work.example"}],
        "mail_state": [{"m": 1, "message_id": "<w@x>", "done": true}]
    }));
    let r = restore(&c, &file, true, true).unwrap();
    assert_eq!(
        (r["mailboxes_added"].as_u64(), r["mail_parked"].as_u64()),
        (Some(1), Some(1)),
        "{r}"
    );
    let fresh = c.store_for(1).unwrap();
    assert_eq!(
        fresh.last_uid(),
        0,
        "its whole inbox is read, like any new mailbox"
    );
    assert!(fresh.get(global_uid(1, 5)).is_none());
    assert_eq!(
        fresh.parked_count(),
        1,
        "and what was restored for it is there"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_snooze_from_a_file_is_limited_like_one_you_type() {
    let _k = keychain();
    let (c, dir) = core("snooze-cap");
    let at = |t: f64| {
        doc(json!({
            "mailboxes": [{"host": "127.0.0.1", "port": 1, "user": "me@example.com"}],
            "mail_state": [{"m": 0, "message_id": "<a@x>", "snooze_until": t}]
        }))
    };
    let now = engine::backup::now_secs();
    assert!(restore(&c, &at(1e15), true, false).is_err());
    assert!(
        restore(&c, &at(now + 400.0 * 86400.0), true, false).is_err(),
        "more than a year ahead"
    );
    assert!(restore(&c, &at(now + 300.0 * 86400.0), true, false).is_ok());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_file_exactly_at_every_limit_is_accepted() {
    let _k = keychain();
    let (c, dir) = core("at-limits");
    let rules: Vec<Value> = (0..50).map(|i| json!({"id": format!("r{i}"), "field": "subject", "text": format!("t{i}"), "priority": 2})).collect();
    let choices: Vec<Value> = (0..5000)
        .map(|i| json!({"addr": format!("a{i}@x.example"), "level": "mute"}))
        .collect();
    let fixes: Vec<Value> = (0..2000).map(|i| json!({"sender": "s", "subject": format!("s{i}"), "old": 3, "new": 4, "addr": "a@x.example", "at": 1.0 + i as f64})).collect();
    let tasks: Vec<Value> = (0..2000)
        .map(|i| json!({"title": format!("task {i}"), "priority": 3}))
        .collect();
    let state: Vec<Value> = (0..5000)
        .map(|i| json!({"m": 0, "message_id": format!("<s{i}@x>"), "done": true}))
        .collect();
    let boxes: Vec<Value> =
        std::iter::once(json!({"host": "127.0.0.1", "port": 1, "user": "me@example.com"}))
            .chain((1..5).map(
                |i| json!({"host": "127.0.0.1", "port": 1, "user": format!("b{i}@work.example")}),
            ))
            .collect();
    let file = doc(
        json!({"rules": rules, "sender_choices": choices, "corrections": fixes, "own_tasks": tasks, "mail_state": state, "mailboxes": boxes}),
    );
    let r = restore(&c, &file, true, true).unwrap();
    assert_eq!(
        (
            r["rules_added"].as_u64(),
            r["sender_choices_added"].as_u64(),
            r["corrections_added"].as_u64(),
            r["own_tasks_added"].as_u64(),
            r["mail_parked"].as_u64(),
            r["mailboxes_added"].as_u64()
        ),
        (
            Some(50),
            Some(5000),
            Some(2000),
            Some(2000),
            Some(5000),
            Some(4)
        ),
        "{r}"
    );
    let st = c.store().unwrap();
    assert!(
        !st.recent_log(300).iter().any(|l| l.1.contains("sender a")),
        "5,000 sender choices do not write 5,000 lines in the activity log"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_full_size_restore_is_quick_even_with_a_big_database() {
    let _k = keychain();
    let (c, dir) = core("fast");
    c.store().unwrap(); // makes the database
    let db = rusqlite::Connection::open(dir.join("agent.db")).unwrap();
    db.execute_batch(
        "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i+1 FROM n WHERE i<30000)
         INSERT INTO tasks(uid,message_id,sender,addr,subject,received,priority,category,action,urgent,excerpt,why,signals,body,needs_reply,due,reply_to,refs,auth_fail)
         SELECT i, '<big'||i||'@x>', 'S', 'a'||i||'@old.example', 's', 0, 3, 'work', '', 0, '', '', '{}', '', 0, '', '', '', 0 FROM n;",
    )
    .unwrap();
    drop(db);
    let choices: Vec<Value> = (0..5000)
        .map(|i| json!({"addr": format!("z{i}@new.example"), "level": "mute"}))
        .collect();
    let state: Vec<Value> = (0..5000)
        .map(|i| json!({"m": 0, "message_id": format!("<other{i}@x>"), "done": true}))
        .collect();
    let file = doc(
        json!({"sender_choices": choices, "mailboxes": [{"host": "127.0.0.1", "port": 1, "user": "me@example.com"}], "mail_state": state}),
    );
    let started = std::time::Instant::now();
    let preview = restore(&c, &file, false, false).unwrap();
    let real = restore(&c, &file, true, false).unwrap();
    let took = started.elapsed();
    assert_eq!(
        (
            preview["mail_parked"].as_u64(),
            real["mail_parked"].as_u64()
        ),
        (Some(5000), Some(5000))
    );
    assert!(
        took.as_secs() < 20,
        "a preview and a restore of the biggest file took {took:?}: lookups need an index"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_preview_names_the_mailboxes_and_the_rules_it_would_add() {
    let _k = keychain();
    let (c, dir) = core("preview-names");
    let file = doc(json!({
        "mailboxes": [{"host": "127.0.0.1", "port": 1, "user": "me@example.com"}, {"host": "imap.evil.example", "port": 993, "user": "victim@gmail.com"}],
        "rules": [
            {"id": "a", "field": "domain", "text": "promo.example", "file": true},
            {"id": "b", "field": "subject", "text": "invoice", "priority": 4, "category": "finance"}
        ]
    }));
    let p = restore(&c, &file, false, true).unwrap();
    let boxes = p["mailboxes_new"].as_array().unwrap();
    assert_eq!(boxes.len(), 1);
    assert!(
        boxes[0].as_str().unwrap().contains("victim@gmail.com")
            && boxes[0].as_str().unwrap().contains("imap.evil.example"),
        "{boxes:?}"
    );
    let rules: Vec<String> = p["rules_new"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert_eq!(rules.len(), 2);
    assert!(
        rules[0].contains("promo.example") && rules[0].to_lowercase().contains("file"),
        "{rules:?}"
    );
    assert!(
        rules[1].contains("invoice") && rules[1].contains('4') && rules[1].contains("finance"),
        "{rules:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn what_is_in_the_options_of_a_backup_is_exactly_the_allow_list() {
    let _k = keychain();
    let (c, dir) = busy_computer("allow-list");
    let v: Value = serde_json::from_str(&export_text(&c)).unwrap();
    let keys = |x: &Value| -> Vec<String> {
        let mut k: Vec<String> = x.as_object().unwrap().keys().cloned().collect();
        k.sort();
        k
    };
    assert_eq!(
        keys(&v["settings"]),
        ["ai", "alerts", "drafts", "followups", "updates"]
    );
    assert_eq!(keys(&v["settings"]["ai"]), ["mode", "ollama_model"]);
    assert_eq!(
        keys(&v["settings"]["alerts"]),
        [
            "categories",
            "desktop",
            "digest_time",
            "quiet_end",
            "quiet_start"
        ]
    );
    assert_eq!(
        keys(&v["settings"]["drafts"]),
        ["auto", "daily_cap", "min_priority", "signature", "tone"]
    );
    assert_eq!(keys(&v["settings"]["followups"]), ["days"]);
    assert_eq!(keys(&v["settings"]["updates"]), ["check"]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn oversized_values_are_refused_wherever_they_appear() {
    let _k = keychain();
    let (c, dir) = core("oversized");
    let one = |section: &str, item: Value| {
        doc(
            json!({"mailboxes": [{"host": "127.0.0.1", "port": 1, "user": "me@example.com"}], section: [item]}),
        )
    };
    let fix = |sender: String, subject: String, addr: String| json!({"sender": sender, "subject": subject, "old": 2, "new": 4, "addr": addr, "at": 1.0});
    for (why, file) in [
        (
            "a sender name over 300",
            one(
                "corrections",
                fix("s".repeat(301), "t".into(), "a@x.example".into()),
            ),
        ),
        (
            "a subject over 600",
            one(
                "corrections",
                fix("s".into(), "t".repeat(601), "a@x.example".into()),
            ),
        ),
        (
            "an address over 320",
            one(
                "corrections",
                fix(
                    "s".into(),
                    "t".into(),
                    format!("{}@x.example", "a".repeat(320)),
                ),
            ),
        ),
        (
            "a message id over 998",
            one(
                "mail_state",
                json!({"m": 0, "message_id": format!("<{}@x>", "a".repeat(998)), "done": true}),
            ),
        ),
        (
            "a repeat anchor that is not a time",
            one(
                "own_tasks",
                json!({"title": "x", "priority": 3, "due": "2030-01-01", "repeat": "weekly", "repeat_base": "soon"}),
            ),
        ),
        (
            "a repeat with nothing to repeat from",
            one(
                "own_tasks",
                json!({"title": "x", "priority": 3, "repeat": "weekly"}),
            ),
        ),
    ] {
        assert!(restore(&c, &file, true, false).is_err(), "{why}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_backup_says_what_it_had_to_leave_out() {
    let _k = keychain();
    let (c, dir) = core("left-out");
    c.store().unwrap(); // makes the database
    let db = rusqlite::Connection::open(dir.join("agent.db")).unwrap();
    db.execute_batch(
        "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i+1 FROM n WHERE i<2100)
         INSERT INTO overrides(sender,subject,old_priority,new_priority,at,addr,uid) SELECT 's', 'subject '||i, 2, 4, i, 'a@x.example', 0 FROM n;",
    )
    .unwrap();
    drop(db);
    let folder = dir.join("out");
    std::fs::create_dir_all(&folder).unwrap();
    let out = engine::backup::export_in(&c, &folder).unwrap();
    assert_eq!(out.counts["corrections"], 2000);
    assert_eq!(out.counts["left_out"]["corrections"], 100, "{}", out.counts);
    let v: Value = serde_json::from_str(&std::fs::read_to_string(&out.path).unwrap()).unwrap();
    let subjects: Vec<&str> = v["corrections"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["subject"].as_str().unwrap())
        .collect();
    assert!(
        subjects.contains(&"subject 2100") && !subjects.contains(&"subject 100"),
        "the newest are kept"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// ───────────────────────── found by the Opus review loop ─────────────────────────
#[test]
fn a_restored_mailbox_never_signs_in_with_a_password_left_over_from_another_one() {
    let _k = keychain();
    let (c, dir) = core("stale-password");
    // what an earlier install, or a mailbox removed in a hurry, can leave in the keychain under the number the new one takes
    engine::secrets::set("imap-password-1", "OLD-PASSWORD-OF-ANOTHER-SERVER").unwrap();
    let file = doc(json!({
        "settings": {"alerts": {"quiet_start": "22:00", "quiet_end": "07:00"}},
        "mailboxes": [{"host": "127.0.0.1", "port": 1, "user": "me@example.com"}, {"host": "imap.other.example", "port": 993, "user": "second@other.example"}]
    }));
    let r = restore(&c, &file, false, true).unwrap();
    assert_eq!(r["mailboxes_added"], 1);
    assert!(
        engine::secrets::get("imap-password-1").is_some(),
        "a preview changes nothing"
    );
    restore(&c, &file, true, true).unwrap();
    assert!(
        engine::secrets::get("imap-password-1").is_none(),
        "the old password is gone, so the new mailbox asks for its own"
    );
    let st = c.rpc("state", &json!({})).unwrap();
    assert_eq!(st["accounts"][1]["status"], "no password", "{st}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_old_backup_does_not_outweigh_what_you_have_taught_the_ranker_since() {
    let _k = keychain();
    let (c, dir) = core("old-corrections");
    let st = c.store().unwrap();
    let now = engine::backup::now_secs();
    // this week: the sender was raised three times (the ranker looks at the newest three)
    let db = rusqlite::Connection::open(dir.join("agent.db")).unwrap();
    for i in 0..3 {
        db.execute(
            "INSERT INTO overrides(sender,subject,old_priority,new_priority,at,addr,uid) VALUES('Bob','this week',2,4,?1,'bob@x.example',0)",
            [now - 3600.0 * (i + 1) as f64],
        )
        .unwrap();
    }
    drop(db);
    let before = st.learned_shift("bob@x.example");
    assert!(before > 0, "{before}");
    // a backup from long ago, in which the same sender was lowered three times
    let old: Vec<Value> = (0..3)
        .map(|i| json!({"sender": "Bob", "subject": format!("long ago {i}"), "old": 4, "new": 2, "addr": "bob@x.example", "at": now - 300.0 * 86400.0 + i as f64}))
        .collect();
    restore(&c, &doc(json!({"corrections": old})), true, false).unwrap();
    assert_eq!(
        st.learned_shift("bob@x.example"),
        before,
        "the newest corrections count, whatever order they were stored in"
    );
    let recent = st.recent_overrides(2);
    assert!(
        recent.iter().all(|o| o.subject == "this week"),
        "and the ones the model is shown are the newest too"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_restore_waits_for_a_sync_that_is_writing_and_then_succeeds() {
    let _k = keychain();
    let (c, dir) = core("busy-database");
    c.store().unwrap(); // makes the database
    let path = dir.join("agent.db");
    let (started, wait) = std::sync::mpsc::channel();
    let other = std::thread::spawn(move || {
        // another connection (a sync saving mail) holds the write lock for a moment and commits
        let db = rusqlite::Connection::open(&path).unwrap();
        db.execute_batch("BEGIN IMMEDIATE").unwrap();
        db.execute(
            "INSERT INTO tasks(uid,message_id,sender,addr,subject,received,priority,category,action,urgent,excerpt,why,signals,body,needs_reply,due,reply_to,refs,auth_fail)
             VALUES(77,'<w@x>','S','s@x.example','sync',0,3,'work','',0,'','','{}','',0,'','','',0)",
            [],
        )
        .unwrap();
        started.send(()).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(700));
        db.execute_batch("COMMIT").unwrap();
    });
    wait.recv().unwrap();
    let file = doc(json!({
        "rules": [{"id": "a", "field": "subject", "text": "late", "priority": 2}],
        "own_tasks": [{"title": "restored while a sync wrote", "priority": 3}]
    }));
    let r = restore(&c, &file, true, false)
        .expect("a restore waits its turn instead of failing with 'database is locked'");
    assert_eq!(r["rules_added"], 1);
    other.join().unwrap();
    let st = c.store().unwrap();
    assert!(st.get(77).is_some(), "the sync's own write is there too");
    assert!(st
        .all_tasks()
        .unwrap()
        .iter()
        .any(|t| t.subject == "restored while a sync wrote"));
    let _ = std::fs::remove_dir_all(&dir);
}
