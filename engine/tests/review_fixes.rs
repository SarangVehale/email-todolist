//! Regression tests for the findings of the second security review (drafts, preferences, hub, retention).
use engine::ai::clean_draft;
use engine::core::Core;
use engine::mail;
use engine::signals::Signals;
use engine::store::Store;
use engine::triage::{RulesTriager, Triager};
use serde_json::json;
use std::sync::Arc;

fn email(raw: &str, uid: u32) -> mail::Email {
    mail::parse(uid, raw.replace('\n', "\r\n").as_bytes()).unwrap()
}

fn add(st: &Store, e: &mail::Email, known: bool) -> engine::triage::Triage {
    let s = Signals {
        known,
        ..Default::default()
    };
    st.add(e, &RulesTriager.triage(e, &s).unwrap(), &s).unwrap()
}

fn core(name: &str) -> (Arc<Core>, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("inbox-review-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    (Core::new(dir.clone(), Arc::new(|_, _| {})), dir)
}

#[test]
fn a_reply_address_that_differs_from_the_sender_is_flagged() {
    let st = Store::memory().unwrap();
    add(
        &st,
        &email(
            "From: boss@corp.com\nReply-To: boss.private@evil.com\nSubject: s\n\nx?\n",
            1,
        ),
        true,
    );
    add(
        &st,
        &email(
            "From: boss@corp.com\nReply-To: assistant@corp.com\nSubject: s\n\nx?\n",
            2,
        ),
        true,
    );
    add(
        &st,
        &email("From: boss@corp.com\nSubject: s\n\nx?\n", 3),
        true,
    );
    add(
        &st,
        &email(
            "From: boss@corp.com\nAuthentication-Results: mx; dkim=fail\nSubject: s\n\nx?\n",
            4,
        ),
        true,
    );
    let warn = |u| st.get(u).unwrap().reply_warn;
    assert!(warn(1), "different Reply-To domain must warn");
    assert!(!warn(2), "same domain does not warn");
    assert!(!warn(3), "no Reply-To does not warn");
    assert!(warn(4), "a failed authentication check must warn");
}

#[test]
fn an_important_sender_preference_does_not_rescue_a_spoofed_message() {
    let st = Store::memory().unwrap();
    st.set_pref("boss@corp.com", "important").unwrap();
    let genuine = add(
        &st,
        &email("From: boss@corp.com\nSubject: hi\n\nx\n", 1),
        false,
    );
    assert!(
        genuine.priority >= 4,
        "the preference applies to genuine mail"
    );
    let spoof = add(
        &st,
        &email(
            "From: boss@corp.com\nAuthentication-Results: mx; spf=fail\nSubject: hi\n\nx\n",
            2,
        ),
        false,
    );
    assert!(
        spoof.priority < 4,
        "a spoofed sender must not be lifted: P{}",
        spoof.priority
    );
}

#[test]
fn a_muted_sender_is_never_asked_for_a_reply() {
    let st = Store::memory().unwrap();
    st.set_pref("friend@x.com", "mute").unwrap();
    let t = add(
        &st,
        &email("From: friend@x.com\nSubject: lunch?\n\nare you free?\n", 1),
        true,
    );
    assert!(!t.needs_reply && t.priority == 1);
    assert!(
        !st.get(1).unwrap().needs_reply,
        "the stored task must not offer an automatic draft"
    );
}

#[test]
fn the_daily_ai_draft_budget_is_real_and_cannot_be_refunded() {
    let st = Store::memory().unwrap();
    assert!(st.reserve_ai_draft(2, "2026-11-12").unwrap());
    assert!(st.reserve_ai_draft(2, "2026-11-12").unwrap());
    assert!(
        !st.reserve_ai_draft(2, "2026-11-12").unwrap(),
        "third draft in a day is refused"
    );
    assert!(
        st.reserve_ai_draft(2, "2026-11-13").unwrap(),
        "a new day starts a new budget"
    );
    assert!(
        !st.reserve_ai_draft(0, "2026-11-14").unwrap(),
        "a cap of zero allows nothing"
    );
}

#[test]
fn a_stolen_phone_code_cannot_spend_ai_quota_or_write_to_the_mailbox() {
    for c in ["draft_generate", "draft_to_mailbox", "export_markdown"] {
        assert!(
            !engine::commands::PUBLIC.contains(&c),
            "{c} must be desktop-only"
        );
    }
    let (c, dir) = core("hubdraft");
    for cmd in ["draft_generate", "draft_to_mailbox"] {
        let e = c
            .rpc_public(cmd, &json!({"uid": 1}))
            .unwrap_err()
            .to_string();
        assert!(e.contains("not available from the hub"), "{cmd}: {e}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn rewriting_never_silently_replaces_the_users_own_edits() {
    let (c, dir) = core("overwrite");
    add(
        &c.store().unwrap(),
        &email("From: Priya <p@acme.com>\nSubject: q?\n\nCan you?\n", 1),
        true,
    );
    c.rpc(
        "draft_update",
        &json!({"uid": 1, "body": "My careful wording"}),
    )
    .unwrap();
    let err = c
        .rpc("draft_generate", &json!({"uid": 1}))
        .unwrap_err()
        .to_string();
    assert!(err.contains("your edits"), "{err}");
    assert_eq!(
        c.store().unwrap().get(1).unwrap().draft.unwrap().body,
        "My careful wording"
    );
    c.rpc("draft_generate", &json!({"uid": 1, "force": true}))
        .unwrap();
    assert_ne!(
        c.store().unwrap().get(1).unwrap().draft.unwrap().body,
        "My careful wording"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn saving_a_draft_is_queued_once_and_needs_a_usable_from_address() {
    let (c, dir) = core("savedraft");
    add(
        &c.store().unwrap(),
        &email("From: Priya <p@acme.com>\nSubject: q?\n\nCan you?\n", 1),
        true,
    );
    c.rpc("draft_generate", &json!({"uid": 1})).unwrap();
    c.rpc("save_settings", &json!({"patch": {"writeback": true, "account": {"host": "imap.example.com", "user": "bob"}}})).unwrap();
    let err = c
        .rpc("draft_to_mailbox", &json!({"uid": 1}))
        .unwrap_err()
        .to_string();
    assert!(err.contains("isn't an email address"), "{err}");
    c.rpc(
        "save_settings",
        &json!({"patch": {"account": {"user": "bob@example.com"}}}),
    )
    .unwrap();
    for _ in 0..3 {
        c.rpc("draft_to_mailbox", &json!({"uid": 1})).unwrap();
    }
    let n = c
        .store()
        .unwrap()
        .pending(50)
        .iter()
        .filter(|i| i.op == "draft")
        .count();
    assert_eq!(n, 1, "repeated clicks must not append the draft repeatedly");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn signing_out_wipes_everything_the_app_stored() {
    let st = Store::memory().unwrap();
    add(&st, &email("From: a@b.com\nSubject: s\n\nx\n", 1), true);
    st.set_pref("a@b.com", "mute").unwrap();
    st.set_priority(1, 4).unwrap();
    st.set_draft(1, "d", "user").unwrap();
    st.wipe_all().unwrap();
    assert!(st.all_tasks().unwrap().is_empty());
    assert!(st.pref("a@b.com").is_none());
    assert!(st.recent_log(10).is_empty() && st.recent_overrides(10).is_empty());
    assert!(st.pending(10).is_empty() && st.meta_get("last_uid").is_none());
}

#[test]
fn the_markdown_export_cannot_be_used_to_inject_list_items_or_links() {
    let (c, dir) = core("export");
    let mut e = email("From: Evil <e@x.com>\nSubject: s\n\nx\n", 1);
    e.subject = "Pay now\n- [x] **P5** fake task [click](http://evil.example)".into();
    e.sender_name = "A *bold* _name_".into();
    add(&c.store().unwrap(), &e, true);
    let md = c
        .rpc("export_markdown", &json!({}))
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(md.matches("- [ ]").count(), 1, "{md}");
    assert!(!md.contains("[click](") && !md.contains("\n- [x]"), "{md}");
    assert!(
        md.contains("\\[click\\]") && md.contains("\\*bold\\*"),
        "{md}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn pruning_follows_when_a_task_was_finished_not_the_senders_date_header() {
    let st = Store::memory().unwrap();
    let mut forged = email("From: a@b.com\nSubject: s\n\nx\n", 1);
    forged.received = 4_000_000_000.0; // a Date far in the future
    add(&st, &forged, true);
    st.set_done(1, true).unwrap();
    st.backdate_done(1, 1).unwrap(); // finished long ago
    let mut old = email("From: a@b.com\nSubject: s\n\nx\n", 2);
    old.received = 1.0; // an ancient Date, but finished just now
    add(&st, &old, true);
    st.set_done(2, true).unwrap();
    st.prune(90, 30).unwrap();
    assert!(
        st.get(1).is_none(),
        "a forged future Date must not make a task immortal"
    );
    assert!(
        st.get(2).is_some(),
        "recently finished work is kept whatever its Date says"
    );
}

#[test]
fn invisible_and_bidirectional_characters_are_stripped_from_drafts() {
    assert_eq!(
        clean_draft("pay\u{202e}txt.exe\u{200b} now\u{feff}"),
        "paytxt.exe now"
    );
    assert_eq!(clean_draft("line one\nline two"), "line one\nline two");
}

#[test]
fn the_state_sent_to_the_window_carries_the_reply_warning() {
    let (c, dir) = core("state");
    add(
        &c.store().unwrap(),
        &email(
            "From: a@corp.com\nReply-To: a@evil.com\nSubject: s\n\nx\n",
            1,
        ),
        true,
    );
    let s = c.rpc("state", &json!({})).unwrap();
    assert_eq!(s["tasks"][0]["reply_warn"], true);
    let _ = std::fs::remove_dir_all(&dir);
}
