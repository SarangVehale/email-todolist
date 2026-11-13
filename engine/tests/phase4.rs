//! Phase 4: your own rules, and follow-ups ("waiting on a reply"). See docs/designs/phase4.md.
use chrono::DateTime;
use engine::core::Core;
use engine::followup::candidates;
use engine::mail;
use engine::rules::{first_match, matches, validate, Rule};
use engine::signals::{SentIndex, Signals};
use engine::store::{Store, LOCAL_UID_BASE};
use engine::triage::{RulesTriager, Triage, Triager};
use serde_json::json;
use std::sync::Arc;

// ───────────────────────── helpers ─────────────────────────
fn own() -> Vec<String> {
    vec!["me@example.com".into()]
}

fn rule(id: &str, field: &str, text: &str) -> Rule {
    Rule {
        id: id.into(),
        field: field.into(),
        text: text.into(),
        priority: None,
        category: None,
        file: false,
    }
}
fn pri(mut r: Rule, p: u8) -> Rule {
    r.priority = Some(p);
    r
}
fn cat(mut r: Rule, c: &str) -> Rule {
    r.category = Some(c.into());
    r
}
fn filing(mut r: Rule) -> Rule {
    r.file = true;
    r
}

fn email(uid: u32, from: &str, subject: &str, extra: &str) -> mail::Email {
    let raw = format!(
        "From: {from}\r\nSubject: {subject}\r\nMessage-ID: <m{uid}@x>\r\n{extra}\r\n\r\nhello\r\n"
    );
    mail::parse(uid, raw.as_bytes()).unwrap()
}
fn add(st: &Store, e: &mail::Email) -> Triage {
    let s = Signals::default();
    st.add(e, &RulesTriager.triage(e, &s).unwrap(), &s).unwrap()
}

// ───────────────────────── matching ─────────────────────────
#[test]
fn rule_text_matches_by_field_without_regard_to_case() {
    let sender = rule("a", "sender", "Dana@Studio.example");
    assert!(matches(&sender, "dana@studio.example", "x"));
    assert!(matches(&sender, "DANA@STUDIO.EXAMPLE", "x"));
    // the sender rule is the exact address: no look-alike, no longer address, no bare name
    for no in [
        "dana@studio.example.evil.example",
        "xdana@studio.example",
        "dana@studio.example.com",
        "dana",
        "",
    ] {
        assert!(!matches(&sender, no, "dana@studio.example"), "{no}");
    }
    let subj = rule("b", "subject", "INVOICE");
    assert!(matches(&subj, "a@b.c", "Re: invoice 2291"));
    assert!(!matches(&subj, "invoice@b.c", "hello"));
}

#[test]
fn a_domain_rule_matches_that_domain_and_its_subdomains_but_not_look_alikes() {
    let d = rule("a", "domain", "acme.com");
    assert!(matches(&d, "p@acme.com", ""));
    assert!(matches(&d, "p@mail.acme.com", ""));
    assert!(matches(&d, "P@ACME.COM", ""));
    for no in [
        "p@notacme.com",
        "p@acme.com.evil.example",
        "p@evilacme.com",
        "acme.com",
        "",
    ] {
        assert!(!matches(&d, no, ""), "{no}");
    }
    let at = rule("b", "domain", "@acme.com");
    assert!(matches(&at, "p@acme.com", ""), "a leading @ is tolerated");
}

#[test]
fn the_first_matching_rule_is_the_one_used() {
    let rules = vec![
        pri(rule("1", "domain", "acme.com"), 5),
        pri(rule("2", "sender", "newsletter@acme.com"), 1),
        pri(rule("3", "subject", "weekly"), 2),
    ];
    assert_eq!(
        first_match(&rules, "newsletter@acme.com", "").unwrap().id,
        "1"
    );
    assert_eq!(
        first_match(&rules, "x@other.org", "Weekly news")
            .unwrap()
            .id,
        "3"
    );
    assert!(first_match(&rules, "x@y.z", "hello").is_none());
}

#[test]
fn an_empty_rule_text_never_matches_everything() {
    // validation refuses it, and matching is safe even if one got through
    assert!(!matches(&rule("a", "subject", ""), "a@b.c", "anything"));
    assert!(!matches(&rule("a", "subject", "   "), "a@b.c", "anything"));
    assert!(!matches(&rule("a", "sender", ""), "a@b.c", "anything"));
}

// ───────────────────────── validation ─────────────────────────
#[test]
fn bad_rules_are_refused_and_good_ones_pass() {
    let ok = pri(rule("a1", "sender", "dana"), 4);
    validate(std::slice::from_ref(&ok)).unwrap();
    validate(&[]).unwrap();
    let long = "x".repeat(101);
    let many: Vec<Rule> = (0..51)
        .map(|i| pri(rule(&format!("r{i}"), "sender", "x"), 3))
        .collect();
    let bad: Vec<(&str, Vec<Rule>)> = vec![
        ("empty text", vec![pri(rule("a", "sender", ""), 3)]),
        ("blank text", vec![pri(rule("a", "sender", "  "), 3)]),
        ("101 chars", vec![pri(rule("a", "sender", &long), 3)]),
        ("control char", vec![pri(rule("a", "subject", "a\nb"), 3)]),
        ("unknown field", vec![pri(rule("a", "body", "x"), 3)]),
        ("priority 0", vec![pri(rule("a", "sender", "x"), 0)]),
        ("priority 6", vec![pri(rule("a", "sender", "x"), 6)]),
        (
            "unknown category",
            vec![cat(rule("a", "sender", "x"), "nope")],
        ),
        ("no outcome", vec![rule("a", "sender", "x")]),
        (
            "file with a priority",
            vec![filing(pri(rule("a", "sender", "x"), 2))],
        ),
        (
            "duplicate id",
            vec![
                pri(rule("a", "sender", "x"), 3),
                pri(rule("a", "sender", "y"), 3),
            ],
        ),
        ("empty id", vec![pri(rule("", "sender", "x"), 3)]),
        ("odd id", vec![pri(rule("a b", "sender", "x"), 3)]),
        ("51 rules", many),
    ];
    for (why, rules) in bad {
        assert!(validate(&rules).is_err(), "accepted: {why}");
    }
    validate(&[filing(rule("a", "domain", "spam.example"))]).unwrap();
    validate(&[cat(rule("a", "subject", "invoice"), "finance")]).unwrap();
}

#[test]
fn rules_keep_their_order_in_the_store_and_a_bad_set_changes_nothing() {
    let st = Store::memory().unwrap();
    assert!(st.rules().is_empty());
    let good = vec![
        pri(rule("b", "sender", "two"), 2),
        pri(rule("a", "sender", "one"), 1),
        cat(rule("c", "subject", "bill"), "finance"),
    ];
    st.set_rules(&good).unwrap();
    let ids: Vec<String> = st.rules().into_iter().map(|r| r.id).collect();
    assert_eq!(ids, ["b", "a", "c"]);
    assert!(st.set_rules(&[pri(rule("x", "sender", ""), 3)]).is_err());
    let after: Vec<String> = st.rules().into_iter().map(|r| r.id).collect();
    assert_eq!(
        after,
        ["b", "a", "c"],
        "a refused set must leave the old one"
    );
    st.set_rules(&[]).unwrap();
    assert!(st.rules().is_empty());
}

// ───────────────────────── ranking interactions ─────────────────────────
#[test]
fn a_rule_sets_the_priority_and_category_of_new_mail() {
    let st = Store::memory().unwrap();
    st.set_rules(&[cat(
        pri(rule("a", "domain", "accountant.example"), 5),
        "finance",
    )])
    .unwrap();
    let t = add(&st, &email(1, "Bo <bo@accountant.example>", "hello", ""));
    assert_eq!((t.priority, t.category.as_str()), (5, "finance"));
    let stored = st.get(1).unwrap();
    assert_eq!((stored.priority, stored.category.as_str()), (5, "finance"));
    assert!(stored.why.contains("rule"), "{}", stored.why);
    let other = add(&st, &email(2, "Cy <cy@elsewhere.example>", "hello", ""));
    assert_ne!(other.priority, 0);
    assert!(!st.get(2).unwrap().why.contains("rule"));
}

#[test]
fn a_rule_that_lowers_the_priority_also_removes_urgent() {
    let st = Store::memory().unwrap();
    st.set_rules(&[pri(rule("a", "sender", "shop@deals.example"), 2)])
        .unwrap();
    let e = email(1, "shop@deals.example", "Last chance", "");
    let s = Signals::default();
    let loud = Triage {
        priority: 5,
        category: "promo".into(),
        action: "x".into(),
        urgent: true,
        why: "model".into(),
        needs_reply: true,
        due: String::new(),
    };
    let t = st.add(&e, &loud, &s).unwrap();
    assert_eq!(t.priority, 2);
    assert!(
        !t.urgent,
        "urgent bypasses quiet hours; a low rule must clear it"
    );
    assert!(!st.get(1).unwrap().urgent);
}

#[test]
fn a_sender_preference_beats_a_rule() {
    let st = Store::memory().unwrap();
    st.set_pref("noisy@acme.com", "mute").unwrap();
    st.set_rules(&[pri(rule("a", "domain", "acme.com"), 5)])
        .unwrap();
    let t = add(&st, &email(1, "noisy@acme.com", "hello", ""));
    assert_eq!(t.priority, 1, "the muted sender stays muted");
    assert!(!st.get(1).unwrap().why.contains("rule of yours"));
}

#[test]
fn a_rule_that_sets_the_priority_replaces_the_learned_shift() {
    let st = Store::memory().unwrap();
    // two earlier corrections of this sender, both upward
    for (uid, subj) in [(1u32, "one"), (2, "two")] {
        add(&st, &email(uid, "Sam <sam@learn.example>", subj, ""));
        st.set_priority(uid, 4).unwrap();
    }
    assert!(st.learned_shift("sam@learn.example") > 0);
    st.set_rules(&[pri(rule("a", "sender", "sam@learn.example"), 2)])
        .unwrap();
    add(&st, &email(3, "Sam <sam@learn.example>", "three", ""));
    let t = st.get(3).unwrap();
    assert_eq!((t.priority, t.learned), (2, 0));
}

#[test]
fn mail_that_failed_its_sender_check_only_gets_rules_that_lower_it() {
    let st = Store::memory().unwrap();
    st.set_rules(&[
        pri(rule("a", "domain", "boss.example"), 5),
        cat(rule("b", "domain", "other.example"), "finance"),
        pri(rule("c", "domain", "low.example"), 1),
    ])
    .unwrap();
    let forged = "Authentication-Results: mx; dkim=fail";
    let t = add(&st, &email(1, "ceo@boss.example", "wire money", forged));
    assert!(
        t.priority < 5,
        "a forged From must not use an 'important' rule"
    );
    let t = add(&st, &email(2, "x@other.example", "bill", forged));
    assert_eq!(t.category, "finance", "a category change is harmless");
    let t = add(&st, &email(3, "y@low.example", "hello", forged));
    assert_eq!(t.priority, 1, "lowering is always allowed");
}

#[test]
fn a_filing_rule_leaves_nothing_that_could_raise_an_alert_or_start_a_draft() {
    let st = Store::memory().unwrap();
    st.set_rules(&[filing(rule("a", "domain", "spam.example"))])
        .unwrap();
    let e = email(1, "x@spam.example", "WIN NOW?", "");
    let s = Signals::default();
    let loud = Triage {
        priority: 5,
        category: "promo".into(),
        action: "x".into(),
        urgent: true,
        why: "model".into(),
        needs_reply: true,
        due: String::new(),
    };
    let t = st.add(&e, &loud, &s).unwrap();
    assert!(t.priority <= 2 && !t.urgent && !t.needs_reply, "{t:?}");
    let stored = st.get(1).unwrap();
    assert_eq!(stored.status, "done");
    assert!(!stored.needs_reply && !stored.urgent && stored.priority <= 2);
    assert!(stored.why.contains("Filed"), "{}", stored.why);
    assert!(
        st.pending(10)
            .iter()
            .any(|i| i.op == "archive" && i.uid == 1),
        "with write-back on this archives the message, like pressing X"
    );
}

// ───────────────────────── applying rules to the existing list ─────────────────────────
#[test]
fn applying_changes_open_mail_and_a_dry_run_changes_nothing() {
    let st = Store::memory().unwrap();
    for (uid, from) in [(1, "a@acme.com"), (2, "b@acme.com"), (3, "c@else.com")] {
        add(&st, &email(uid, from, "hello", ""));
    }
    st.set_rules(&[pri(rule("a", "domain", "acme.com"), 5)])
        .unwrap();
    let dry = st.apply_rules(true).unwrap();
    assert_eq!((dry.matched, dry.changed, dry.filed), (2, 2, 0));
    assert_ne!(st.get(1).unwrap().priority, 5, "a dry run changes nothing");
    let real = st.apply_rules(false).unwrap();
    assert_eq!((real.matched, real.changed), (2, 2));
    assert_eq!(st.get(1).unwrap().priority, 5);
    assert_eq!(st.get(2).unwrap().priority, 5);
    assert_ne!(st.get(3).unwrap().priority, 5);
    assert_eq!(
        st.apply_rules(false).unwrap().changed,
        0,
        "second run: nothing left"
    );
}

#[test]
fn applying_leaves_alone_what_the_user_already_decided() {
    let st = Store::memory().unwrap();
    add(&st, &email(1, "pref@acme.com", "has a preference", ""));
    add(
        &st,
        &email(2, "fixed@acme.com", "was corrected by hand", ""),
    );
    add(&st, &email(3, "plain@acme.com", "untouched", ""));
    st.set_pref("pref@acme.com", "mute").unwrap();
    st.set_priority(2, 2).unwrap();
    let mine = st.add_local_task("mine", 3, "").unwrap().unwrap();
    st.set_rules(&[pri(rule("a", "domain", "acme.com"), 5)])
        .unwrap();
    let dry = st.apply_rules(true).unwrap();
    assert_eq!(dry.matched, 1, "only the untouched task is a match");
    st.apply_rules(false).unwrap();
    assert_eq!(st.get(1).unwrap().priority, 1, "a muted sender stays muted");
    assert_eq!(st.get(2).unwrap().priority, 2, "a hand correction stays");
    assert_eq!(st.get(3).unwrap().priority, 5);
    assert_eq!(st.get(mine).unwrap().priority, 3);
}

#[test]
fn applying_a_priority_rule_clears_the_learned_shift_it_replaces() {
    let st = Store::memory().unwrap();
    for (uid, subj) in [(1u32, "one"), (2, "two")] {
        add(&st, &email(uid, "Sam <sam@learn.example>", subj, ""));
        st.set_priority(uid, 4).unwrap();
    }
    add(&st, &email(3, "Sam <sam@learn.example>", "three", ""));
    assert!(
        st.get(3).unwrap().learned > 0,
        "set-up: the shift applies to new mail"
    );
    st.set_rules(&[pri(rule("a", "sender", "sam@learn.example"), 2)])
        .unwrap();
    st.apply_rules(false).unwrap();
    let t = st.get(3).unwrap();
    assert_eq!((t.priority, t.learned), (2, 0));
}

#[test]
fn applying_a_category_rule_leaves_urgent_alone() {
    let st = Store::memory().unwrap();
    let e = email(1, "bo@acme.com", "bill", "");
    let s = Signals::default();
    let urgent = Triage {
        priority: 3,
        category: "other".into(),
        action: "x".into(),
        urgent: true,
        why: "model".into(),
        needs_reply: false,
        due: String::new(),
    };
    st.add(&e, &urgent, &s).unwrap();
    st.set_rules(&[cat(rule("a", "domain", "acme.com"), "finance")])
        .unwrap();
    st.apply_rules(false).unwrap();
    let t = st.get(1).unwrap();
    assert_eq!(
        (t.category.as_str(), t.urgent, t.priority),
        ("finance", true, 3)
    );
}

#[test]
fn applying_rules_never_teaches_the_ranker() {
    let st = Store::memory().unwrap();
    for uid in 1..=3 {
        add(&st, &email(uid, "s@acme.com", &format!("m{uid}"), ""));
    }
    st.set_rules(&[pri(rule("a", "domain", "acme.com"), 5)])
        .unwrap();
    st.apply_rules(false).unwrap();
    assert!(
        st.recent_overrides(10).is_empty(),
        "a rule is not a correction"
    );
    assert_eq!(st.learned_shift("s@acme.com"), 0);
}

#[test]
fn applying_a_filing_rule_marks_done_and_skips_finished_tasks() {
    let st = Store::memory().unwrap();
    add(&st, &email(1, "x@spam.example", "a", ""));
    add(&st, &email(2, "y@spam.example", "b", ""));
    st.set_done(2, true).unwrap();
    st.set_rules(&[filing(rule("a", "domain", "spam.example"))])
        .unwrap();
    let dry = st.apply_rules(true).unwrap();
    assert_eq!(dry.filed, 1, "the done task is not counted");
    let r = st.apply_rules(false).unwrap();
    assert_eq!(r.filed, 1);
    assert_eq!(st.get(1).unwrap().status, "done");
}

// ───────────────────────── commands ─────────────────────────
fn core(name: &str) -> (Arc<Core>, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("ia-p4-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    (Core::new(dir.clone(), Arc::new(|_, _| {})), dir)
}

#[test]
fn rules_are_desktop_only_and_validated_by_the_command() {
    let (c, dir) = core("rules");
    let good = json!([{"id":"a","field":"domain","text":"acme.com","priority":5,"category":null,"file":false}]);
    c.rpc("rules_save", &json!({"rules": good})).unwrap();
    let st = c.rpc("state", &json!({})).unwrap();
    assert_eq!(st["rules"][0]["text"], "acme.com");
    let public = c.rpc_public("state", &json!({})).unwrap();
    assert!(
        public.get("rules").is_none(),
        "the phone never sees the rules"
    );
    for cmd in ["rules_save", "rules_apply"] {
        let e = c
            .rpc_public(cmd, &json!({"rules": [], "dry": true}))
            .unwrap_err()
            .to_string();
        assert!(e.contains("not available from the hub"), "{cmd}: {e}");
    }
    let bad =
        json!([{"id":"a","field":"body","text":"x","priority":5,"category":null,"file":false}]);
    assert!(c.rpc("rules_save", &json!({"rules": bad})).is_err());
    assert_eq!(
        c.rpc("state", &json!({})).unwrap()["rules"][0]["text"],
        "acme.com",
        "a refused save leaves the saved rules"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn rules_apply_reports_counts_through_the_command() {
    let (c, dir) = core("apply");
    add(&c.store().unwrap(), &email(1, "a@acme.com", "hi", ""));
    c.rpc("rules_save", &json!({"rules": [{"id":"a","field":"domain","text":"acme.com","priority":5,"category":null,"file":false}]})).unwrap();
    let dry = c.rpc("rules_apply", &json!({"dry": true})).unwrap();
    assert_eq!(
        (dry["matched"].as_u64(), dry["changed"].as_u64()),
        (Some(1), Some(1))
    );
    assert_ne!(c.store().unwrap().get(1).unwrap().priority, 5);
    c.rpc("rules_apply", &json!({"dry": false})).unwrap();
    assert_eq!(c.store().unwrap().get(1).unwrap().priority, 5);
    let _ = std::fs::remove_dir_all(&dir);
}

// ───────────────────────── follow-ups: choosing candidates ─────────────────────────
const DAY: f64 = 86400.0;
const NOW: f64 = 1_800_000_000.0;

fn rfc2822(ts: f64) -> String {
    DateTime::from_timestamp(ts as i64, 0).unwrap().to_rfc2822()
}
fn sent_raw(id: &str, to: &str, subject: &str, ago_days: f64, extra: &str) -> Vec<u8> {
    format!(
        "From: me@example.com\r\nTo: {to}\r\nSubject: {subject}\r\nMessage-ID: {id}\r\nDate: {}\r\n{extra}\r\n\r\n",
        rfc2822(NOW - ago_days * DAY)
    )
    .into_bytes()
}
fn index(msgs: Vec<Vec<u8>>) -> SentIndex {
    let mut i = SentIndex::default();
    for m in msgs {
        i.add_headers(&m);
    }
    i
}
fn ids(v: &[engine::signals::SentMsg]) -> Vec<String> {
    v.iter().map(|m| m.message_id.clone()).collect()
}

#[test]
fn a_thread_you_started_and_nobody_answered_is_a_candidate() {
    let st = Store::memory().unwrap();
    let idx = index(vec![sent_raw(
        "<a@x>",
        "Dana <d@studio.example>",
        "Quote?",
        5.0,
        "",
    )]);
    let c = candidates(&idx, &own(), 3, NOW, &st);
    assert_eq!(ids(&c), ["<a@x>"]);
    assert_eq!(c[0].to, "d@studio.example");
    assert_eq!(c[0].subject, "Quote?");
}

#[test]
fn only_old_enough_and_not_too_old_messages_count() {
    let st = Store::memory().unwrap();
    let idx = index(vec![
        sent_raw("<young@x>", "d@s.example", "s", 1.0, ""),
        sent_raw("<ok@x>", "d@s.example", "s", 4.0, ""),
        sent_raw("<ancient@x>", "d@s.example", "s", 20.0, ""),
        sent_raw("<future@x>", "d@s.example", "s", -3.0, ""),
    ]);
    assert_eq!(ids(&candidates(&idx, &own(), 3, NOW, &st)), ["<ok@x>"]);
    // a longer wait moves the line
    assert!(candidates(&idx, &own(), 5, NOW, &st).is_empty());
    assert_eq!(candidates(&idx, &own(), 2, NOW, &st).len(), 1);
    // off means off
    assert!(candidates(&idx, &own(), 0, NOW, &st).is_empty());
}

#[test]
fn replies_forwards_self_mail_and_mail_without_an_id_are_never_candidates() {
    let st = Store::memory().unwrap();
    let idx = index(vec![
        sent_raw("<reply@x>", "d@s.example", "Re: hi", 5.0, "In-Reply-To: <q@x>\r\n"),
        sent_raw("<fwd@x>", "d@s.example", "Fwd: hi", 5.0, ""),
        sent_raw("<wg@x>", "d@s.example", "WG: hallo", 5.0, ""),
        sent_raw("<self@x>", "me@example.com", "note to self", 5.0, ""),
        sent_raw("<cap@x>", "ME@Example.com", "note to self 2", 5.0, ""),
        b"From: me@example.com\r\nTo: d@s.example\r\nSubject: no id\r\nDate: Tue, 03 Nov 2026 10:00:00 +0000\r\n\r\n".to_vec(),
        sent_raw("<noto@x>", "", "nobody", 5.0, ""),
        sent_raw("<good@x>", "d@s.example", "keeps", 5.0, ""),
    ]);
    assert_eq!(ids(&candidates(&idx, &own(), 3, NOW, &st)), ["<good@x>"]);
}

#[test]
fn a_message_id_that_could_break_a_search_command_is_dropped() {
    let st = Store::memory().unwrap();
    let idx = index(vec![
        sent_raw("<a\"b@x>", "d@s.example", "quote", 5.0, ""),
        sent_raw("<a\\b@x>", "d@s.example", "slash", 5.0, ""),
        sent_raw("<fine@x>", "d@s.example", "fine", 5.0, ""),
    ]);
    assert_eq!(ids(&candidates(&idx, &own(), 3, NOW, &st)), ["<fine@x>"]);
}

#[test]
fn one_of_several_recipients_being_you_does_not_hide_the_conversation() {
    let st = Store::memory().unwrap();
    let idx = index(vec![sent_raw(
        "<a@x>",
        "me@example.com, Dana <d@s.example>",
        "team",
        5.0,
        "",
    )]);
    let c = candidates(&idx, &own(), 3, NOW, &st);
    assert_eq!(c[0].to, "d@s.example", "the first recipient who is not you");
}

#[test]
fn candidates_come_newest_first_and_subjects_are_cleaned() {
    let st = Store::memory().unwrap();
    let idx = index(vec![
        sent_raw("<old@x>", "d@s.example", "older", 9.0, ""),
        sent_raw(
            "<new@x>",
            "d@s.example",
            "=?utf-8?q?bell=07=0Aline?=",
            4.0,
            "",
        ),
    ]);
    let c = candidates(&idx, &own(), 3, NOW, &st);
    assert_eq!(ids(&c), ["<new@x>", "<old@x>"]);
    assert!(
        !c[0].subject.chars().any(char::is_control),
        "{:?}",
        c[0].subject
    );
}

#[test]
fn the_sent_index_keeps_a_bounded_number_of_started_threads() {
    let mut i = SentIndex::default();
    for n in 0..400 {
        i.add_headers(&sent_raw(&format!("<m{n}@x>"), "d@s.example", "s", 5.0, ""));
    }
    assert!(i.started.len() <= 200, "{}", i.started.len());
}

// ───────────────────────── follow-ups: the task ─────────────────────────
fn msg(id: &str, to: &str, subject: &str, ago_days: f64) -> engine::signals::SentMsg {
    engine::signals::SentMsg {
        message_id: id.into(),
        to: to.into(),
        recipients: vec![to.into()],
        subject: subject.into(),
        sent_at: NOW - ago_days * DAY,
    }
}

#[test]
fn a_followup_is_one_task_of_your_own_and_is_made_only_once() {
    let st = Store::memory().unwrap();
    let m = msg("<a@x>", "d@s.example", "Quote?", 5.0);
    let uid = st.add_followup(&m, NOW).unwrap().expect("created");
    assert!(uid >= LOCAL_UID_BASE);
    let t = st.get(uid).unwrap();
    assert_eq!(t.kind, "followup");
    assert_eq!(t.status, "open");
    assert_eq!(t.priority, 3);
    assert!(
        t.subject.starts_with("Follow up") && t.subject.contains("Quote?"),
        "{}",
        t.subject
    );
    assert!(t.excerpt.contains("d@s.example"), "{}", t.excerpt);
    assert_eq!(st.followup_state("<a@x>").as_deref(), Some("waiting"));
    assert!(st.add_followup(&m, NOW).unwrap().is_none(), "never twice");
    assert!(
        st.pending(10).is_empty(),
        "a task of your own has no mailbox message to change"
    );
}

#[test]
fn a_task_you_closed_is_never_made_again() {
    let st = Store::memory().unwrap();
    let m = msg("<a@x>", "d@s.example", "Quote?", 5.0);
    let uid = st.add_followup(&m, NOW).unwrap().unwrap();
    st.set_done(uid, true).unwrap();
    let idx = index(vec![sent_raw("<a@x>", "d@s.example", "Quote?", 5.0, "")]);
    assert!(candidates(&idx, &own(), 3, NOW, &st).is_empty());
    assert!(st.add_followup(&m, NOW).unwrap().is_none());
}

#[test]
fn an_answered_message_is_remembered_so_it_is_not_searched_again() {
    let st = Store::memory().unwrap();
    st.mark_followup("<a@x>", "answered", NOW).unwrap();
    let idx = index(vec![sent_raw("<a@x>", "d@s.example", "Quote?", 5.0, "")]);
    assert!(candidates(&idx, &own(), 3, NOW, &st).is_empty());
}

#[test]
fn a_reply_closes_an_open_followup_but_not_one_you_reopened() {
    let st = Store::memory().unwrap();
    let a = st
        .add_followup(&msg("<a@x>", "d@s.example", "A", 5.0), NOW)
        .unwrap()
        .unwrap();
    let b = st
        .add_followup(&msg("<b@x>", "e@s.example", "B", 5.0), NOW)
        .unwrap()
        .unwrap();
    let open: Vec<_> = st.open_followups();
    assert_eq!(open.len(), 2);
    assert!(st.close_followup(a).unwrap());
    let ta = st.get(a).unwrap();
    assert_eq!(
        (ta.status.as_str(), ta.closed_by.as_str()),
        ("done", "replied")
    );
    // the user closes, then reopens the other: the app must not close it again
    st.set_done(b, true).unwrap();
    st.set_done(b, false).unwrap();
    assert!(!st.close_followup(b).unwrap());
    assert_eq!(st.get(b).unwrap().status, "open");
    // never touches mail tasks
    add(&st, &email(7, "x@y.example", "mail", ""));
    assert!(!st.close_followup(7).unwrap());
    assert_eq!(st.get(7).unwrap().status, "open");
}

#[test]
fn followup_rows_older_than_sixty_days_are_forgotten_by_pruning() {
    let st = Store::memory().unwrap();
    st.mark_followup("<old@x>", "answered", NOW - 70.0 * DAY)
        .unwrap();
    st.mark_followup("<new@x>", "answered", NOW - 10.0 * DAY)
        .unwrap();
    st.prune_followups(NOW).unwrap();
    assert!(st.followup_state("<old@x>").is_none());
    assert!(st.followup_state("<new@x>").is_some());
}

#[test]
fn signing_out_forgets_followups_and_rules_stay_with_the_user_when_the_account_changes() {
    let st = Store::memory().unwrap();
    st.add_followup(&msg("<a@x>", "d@s.example", "A", 5.0), NOW)
        .unwrap();
    st.set_rules(&[pri(rule("a", "sender", "x"), 3)]).unwrap();
    st.reset_for_new_account().unwrap();
    assert!(
        st.followup_state("<a@x>").is_none(),
        "tied to the old mailbox"
    );
    assert_eq!(
        st.rules().len(),
        1,
        "rules are the user's, not the mailbox's"
    );
    st.wipe_all().unwrap();
    assert!(st.rules().is_empty(), "sign-out removes everything");
}

#[test]
fn followup_settings_accept_only_the_offered_waits() {
    use engine::settings::Settings;
    for d in [0u32, 2, 3, 5, 7] {
        Settings::default()
            .patched(&json!({"followups": {"days": d}}))
            .unwrap()
            .validate()
            .unwrap();
    }
    for d in [1u32, 4, 6, 8, 30, 365] {
        assert!(
            Settings::default()
                .patched(&json!({"followups": {"days": d}}))
                .unwrap()
                .validate()
                .is_err(),
            "{d}"
        );
    }
    assert_eq!(Settings::default().followups.days, 0, "off by default");
}

// ───────────────────────── review fixes (council and drift passes) ─────────────────────────
#[test]
fn a_stranger_cannot_climb_with_a_look_alike_address_or_a_display_name() {
    let st = Store::memory().unwrap();
    st.set_rules(&[pri(rule("a", "sender", "dana@studio.example"), 5)])
        .unwrap();
    let real = add(&st, &email(1, "Dana <dana@studio.example>", "hi", ""));
    assert_eq!(real.priority, 5);
    // valid SPF and DKIM on the attacker's own domain: no failed check to catch, only the match itself
    let sub = add(
        &st,
        &email(2, "x@dana@studio.example.evil.example", "hi", ""),
    );
    let sub2 = add(&st, &email(3, "x@studio.example.evil.example", "hi", ""));
    let named = add(
        &st,
        &email(4, "\"dana@studio.example\" <x@evil.example>", "hi", ""),
    );
    for t in [sub, sub2, named] {
        assert!(t.priority < 5, "{t:?}");
    }
}

#[test]
fn a_subject_rule_raises_only_mail_from_someone_you_write_to() {
    let st = Store::memory().unwrap();
    st.set_rules(&[pri(rule("a", "subject", "invoice"), 5)])
        .unwrap();
    let stranger = add(&st, &email(1, "x@evil.example", "Invoice attached", ""));
    assert!(
        stranger.priority < 5,
        "anyone can write 'invoice' in a subject"
    );
    let known = Signals {
        known: true,
        ..Default::default()
    };
    let e = email(2, "bo@acme.example", "Invoice 2291", "");
    let t = st
        .add(&e, &RulesTriager.triage(&e, &known).unwrap(), &known)
        .unwrap();
    assert_eq!(t.priority, 5, "someone you write to");
    // lowering by subject needs no trust
    st.set_rules(&[pri(rule("b", "subject", "unsubscribe"), 1)])
        .unwrap();
    let low = add(&st, &email(3, "x@evil.example", "How to unsubscribe", ""));
    assert_eq!(low.priority, 1);
}

#[test]
fn applying_rules_keeps_the_same_guards_as_new_mail() {
    let st = Store::memory().unwrap();
    add(
        &st,
        &email(
            1,
            "ceo@boss.example",
            "wire money",
            "Authentication-Results: mx; dkim=fail",
        ),
    );
    add(&st, &email(2, "x@evil.example", "Invoice attached", ""));
    let e = email(3, "bo@acme.example", "Invoice 2291", "");
    let known = Signals {
        known: true,
        ..Default::default()
    };
    st.add(&e, &RulesTriager.triage(&e, &known).unwrap(), &known)
        .unwrap();
    st.set_rules(&[
        pri(rule("a", "domain", "boss.example"), 5),
        pri(rule("b", "subject", "invoice"), 5),
    ])
    .unwrap();
    let before: Vec<u8> = (1..=3).map(|u| st.get(u).unwrap().priority).collect();
    st.apply_rules(false).unwrap();
    assert_eq!(
        st.get(1).unwrap().priority,
        before[0],
        "failed sender check: not raised"
    );
    assert_eq!(
        st.get(2).unwrap().priority,
        before[1],
        "a stranger's subject: not raised"
    );
    assert_eq!(
        st.get(3).unwrap().priority,
        5,
        "someone you write to: raised"
    );
}

#[test]
fn what_a_rule_leaves_in_the_reason_never_repeats_the_rules_own_text() {
    // the reason is part of the task, and the phone sees tasks; the rules themselves it must never see
    let st = Store::memory().unwrap();
    st.set_rules(&[pri(rule("a", "subject", "zebra-secret-phrase"), 2)])
        .unwrap();
    add(
        &st,
        &email(1, "x@y.example", "about the zebra-secret-phrase", ""),
    );
    assert!(!st.get(1).unwrap().why.contains("zebra-secret-phrase"));
    st.set_rules(&[filing(rule("b", "subject", "zebra-secret-phrase"))])
        .unwrap();
    add(
        &st,
        &email(2, "x@y.example", "again zebra-secret-phrase", ""),
    );
    assert!(!st.get(2).unwrap().why.contains("zebra-secret-phrase"));
    st.set_rules(&[cat(rule("c", "subject", "zebra-secret-phrase"), "finance")])
        .unwrap();
    add(
        &st,
        &email(3, "x@y.example", "third zebra-secret-phrase", ""),
    );
    assert!(!st.get(3).unwrap().why.contains("zebra-secret-phrase"));
}

#[test]
fn applying_a_calming_rule_takes_urgent_off_even_when_the_priority_already_matches() {
    let st = Store::memory().unwrap();
    let e = email(1, "bo@acme.com", "bill", "");
    let s = Signals::default();
    let urgent = Triage {
        priority: 3,
        category: "other".into(),
        action: "x".into(),
        urgent: true,
        why: "model".into(),
        needs_reply: false,
        due: String::new(),
    };
    st.add(&e, &urgent, &s).unwrap();
    st.set_rules(&[pri(rule("a", "domain", "acme.com"), 3)])
        .unwrap();
    assert_eq!(st.apply_rules(true).unwrap().changed, 1);
    st.apply_rules(false).unwrap();
    assert!(
        !st.get(1).unwrap().urgent,
        "the same as the mail arriving new"
    );
}

// ───────────────────────── follow-ups: what was already read, and housekeeping ─────────────────────────
fn reply_to(st: &Store, uid: u32, from: &str, to_id: &str, extra: &str) {
    let raw = format!(
        "From: {from}\r\nSubject: Re: quote\r\nMessage-ID: <rep{uid}@x>\r\nIn-Reply-To: {to_id}\r\nReferences: {to_id}\r\n{extra}\r\n\r\nhere\r\n"
    );
    let e = mail::parse(uid, raw.as_bytes()).unwrap();
    let s = engine::signals::compute(&e, &SentIndex::default());
    st.add(&e, &RulesTriager.triage(&e, &s).unwrap(), &s)
        .unwrap();
}

#[test]
fn the_local_reply_check_counts_replies_and_ignores_copies_nudges_and_auto_replies() {
    let st = Store::memory().unwrap();
    let me = vec!["me@example.com".to_string()];
    assert!(!st.has_local_reply("<s1@me>", &me));
    // your own nudge in the same thread, an out-of-office notice, and the Bcc copy of the message itself
    reply_to(&st, 1, "me@example.com", "<s1@me>", "");
    reply_to(
        &st,
        2,
        "Bob <bob@x.com>",
        "<s1@me>",
        "Auto-Submitted: auto-replied\r\n",
    );
    let copy = mail::parse(
        3,
        b"From: me@example.com\r\nSubject: Quote?\r\nMessage-ID: <s1@me>\r\n\r\nhi\r\n",
    )
    .unwrap();
    let s = Signals::default();
    st.add(&copy, &RulesTriager.triage(&copy, &s).unwrap(), &s)
        .unwrap();
    assert!(
        !st.has_local_reply("<s1@me>", &me),
        "none of those is an answer"
    );
    // a real reply is, even after the user pressed X on it, and even deep in a thread (References only)
    reply_to(&st, 4, "Bob <bob@x.com>", "<s1@me>", "");
    st.set_done(4, true).unwrap();
    assert!(st.has_local_reply("<s1@me>", &me));
    assert!(!st.has_local_reply("<other@me>", &me));
    // an id that is only part of another id is not the same id
    assert!(!st.has_local_reply("<s1@m>", &me));
}

#[test]
fn the_sent_messages_own_copy_is_never_its_own_answer_even_when_no_address_is_known_to_be_yours() {
    // a Bcc to yourself arrives in the inbox with the very Message-ID that was sent
    let st = Store::memory().unwrap();
    let copy = mail::parse(
        3,
        b"From: alias@example.org\r\nSubject: Quote?\r\nMessage-ID: <s1@me>\r\n\r\nhi\r\n",
    )
    .unwrap();
    let s = Signals::default();
    st.add(&copy, &RulesTriager.triage(&copy, &s).unwrap(), &s)
        .unwrap();
    assert!(!st.has_local_reply("<s1@me>", &[]));
}

#[test]
fn a_login_that_is_not_an_address_still_hides_notes_you_sent_to_yourself() {
    let st = Store::memory().unwrap();
    let idx = index(vec![
        sent_raw("<self@x>", "me@example.com", "note to self", 5.0, ""),
        sent_raw("<ok@x>", "d@s.example", "keeps", 5.0, ""),
    ]);
    // the login is "jsmith": the From of the sent mail says which address is yours
    let own = engine::followup::own_addresses(&idx, "jsmith");
    assert_eq!(own, ["me@example.com"]);
    assert_eq!(ids(&candidates(&idx, &own, 3, NOW, &st)), ["<ok@x>"]);
}

#[test]
fn old_followup_records_are_forgotten_by_the_routine_clean_up_and_the_scheduler_runs_it_daily() {
    let st = Store::memory().unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64();
    st.mark_followup("<old@x>", "answered", now - 70.0 * DAY)
        .unwrap();
    st.mark_followup("<new@x>", "answered", now - 10.0 * DAY)
        .unwrap();
    st.prune(90, 30).unwrap();
    assert!(
        st.followup_state("<old@x>").is_none(),
        "the app's own clean-up removes it"
    );
    assert!(st.followup_state("<new@x>").is_some());
    assert!(engine::jobs::prune_due(None, "2026-11-13"));
    assert!(engine::jobs::prune_due(Some("2026-11-12"), "2026-11-13"));
    assert!(!engine::jobs::prune_due(Some("2026-11-13"), "2026-11-13"));
}

#[test]
fn tasks_added_at_the_same_moment_from_different_threads_never_collide() {
    let dir = std::env::temp_dir().join(format!("ia-p4-uid-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("agent.db");
    Store::open(&path).unwrap();
    let workers: Vec<_> = (0..6)
        .map(|_| {
            let p = path.clone();
            std::thread::spawn(move || {
                let st = Store::open(&p).unwrap();
                (0..15)
                    .map(|i| {
                        if i % 2 == 0 {
                            st.add_local_task(&format!("t{i}"), 3, "").unwrap().unwrap()
                        } else {
                            let m = engine::signals::SentMsg {
                                message_id: format!("<{i}-{:?}@x>", std::thread::current().id()),
                                to: "d@s.example".into(),
                                recipients: vec!["d@s.example".into()],
                                subject: "s".into(),
                                sent_at: NOW,
                            };
                            st.add_followup(&m, NOW).unwrap().unwrap()
                        }
                    })
                    .collect::<Vec<u32>>()
            })
        })
        .collect();
    let mut all: Vec<u32> = workers
        .into_iter()
        .flat_map(|w| w.join().unwrap())
        .collect();
    let n = all.len();
    all.sort_unstable();
    all.dedup();
    assert_eq!(all.len(), n, "every task got its own id");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn applying_a_raise_that_is_refused_leaves_urgent_alone_and_is_not_counted() {
    // a stranger's subject cannot raise anything, so the rule changes nothing here, urgent flag included
    let st = Store::memory().unwrap();
    let e = email(1, "x@evil.example", "Invoice attached", "");
    let s = Signals::default();
    let urgent = Triage {
        priority: 3,
        category: "other".into(),
        action: "x".into(),
        urgent: true,
        why: "model".into(),
        needs_reply: false,
        due: String::new(),
    };
    st.add(&e, &urgent, &s).unwrap();
    st.set_rules(&[pri(rule("a", "subject", "invoice"), 5)])
        .unwrap();
    assert_eq!(st.apply_rules(true).unwrap().changed, 0);
    st.apply_rules(false).unwrap();
    let t = st.get(1).unwrap();
    assert_eq!((t.priority, t.urgent), (3, true));
}

#[test]
fn a_category_rule_leaves_the_same_reason_on_arrival_and_when_applied_to_the_list() {
    let st = Store::memory().unwrap();
    st.set_rules(&[cat(rule("a", "domain", "acme.example"), "travel")])
        .unwrap();
    add(&st, &email(1, "a@acme.example", "hello", ""));
    st.set_rules(&[]).unwrap();
    add(&st, &email(2, "b@acme.example", "hello", ""));
    st.set_rules(&[cat(rule("a", "domain", "acme.example"), "travel")])
        .unwrap();
    st.apply_rules(false).unwrap();
    let (a, b) = (st.get(1).unwrap(), st.get(2).unwrap());
    assert_eq!(b.category, "travel");
    assert!(
        a.why.contains("category set by a rule of yours")
            && b.why.contains("category set by a rule of yours"),
        "{} / {}",
        a.why,
        b.why
    );
}
