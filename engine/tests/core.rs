use engine::ai::parse_reply;
use engine::mail;
use engine::settings::{is_gmail, Settings};
use engine::signals::Signals;
use engine::store::Store;
use engine::triage::{RulesTriager, Triager};
use serde_json::json;

fn task_store() -> Store {
    let st = Store::memory().unwrap();
    for (uid, raw) in [
        (
            1u32,
            "From: a@x.com\r\nSubject: one\r\nMessage-ID: <1@x>\r\n\r\nhi\r\n",
        ),
        (2, "From: b@x.com\r\nSubject: two\r\n\r\nhi\r\n"),
    ] {
        let e = mail::parse(uid, raw.as_bytes()).unwrap();
        let s = Signals::default();
        let t = RulesTriager.triage(&e, &s).unwrap();
        st.add(&e, &t, &s).unwrap();
    }
    st
}

#[test]
fn settings_patch_merges_nested_fields() {
    let s = Settings::default();
    let p = s
        .patched(&json!({"hub": {"enabled": true}, "ai": {"mode": "ollama"}, "bogus": 1}))
        .unwrap();
    assert!(p.hub.enabled && p.hub.port == 8787);
    assert_eq!(p.ai.mode, "ollama");
    assert_eq!(p.ai.anthropic_model, "claude-haiku-5-5");
    assert!(!p.writeback, "write-back must default to off");
}

#[test]
fn gmail_detection() {
    assert!(is_gmail("imap.gmail.com") && is_gmail("IMAP.GOOGLEMAIL.COM"));
    assert!(!is_gmail("outlook.office365.com"));
}

#[test]
fn parses_model_replies() {
    let t = parse_reply("Sure! ```json\n{\"priority\": 9, \"category\":\"work\",\"action\":\"Sign it\",\"urgent\":true,\"why\":\"deadline\"}\n```").unwrap();
    assert_eq!(
        (t.priority, t.urgent, t.category.as_str()),
        (5, true, "work")
    ); // clamped to 5
    assert!(parse_reply("no json here").is_err());
    assert!(parse_reply("{\"category\":\"x\"}").is_err());
}

#[test]
fn actions_update_state_and_queue_mailbox_changes() {
    let st = task_store();
    assert!(st.set_priority(1, 5).unwrap());
    let t = st.get(1).unwrap();
    assert_eq!(t.priority, 5);
    assert!(t.star, "P4+ auto-stars");
    assert_eq!(st.recent_overrides(5).len(), 1);
    assert!(st.set_done(2, true).unwrap());
    assert_eq!(st.get(2).unwrap().status, "done");
    let ops: Vec<String> = st
        .pending(10)
        .iter()
        .map(|i| format!("{}:{}", i.uid, i.op))
        .collect();
    assert_eq!(ops, ["1:label", "1:star", "2:archive"]);
    let first = st.pending(1)[0].id;
    st.ack(first).unwrap();
    assert_eq!(st.pending(10).len(), 2);
    assert!(!st.set_done(99, true).unwrap());
    assert!(!st.set_priority(1, 9).unwrap());
}

#[test]
fn reset_clears_mailbox_data() {
    let st = task_store();
    st.set_last_uid(5).unwrap();
    st.reset_mailbox_data().unwrap();
    assert!(st.all_tasks().unwrap().is_empty() && st.last_uid() == 0 && st.pending(5).is_empty());
}
