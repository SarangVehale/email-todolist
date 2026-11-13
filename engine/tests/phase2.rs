//! Phase 2: due dates you can edit, reading the stored text, snooze until a date.
use chrono::{Local, TimeZone};
use engine::jobs::snooze_on;
use engine::signals::Signals;
use engine::store::{Store, LOCAL_UID_BASE};
use engine::triage::{RulesTriager, Triager};

fn mail_task(st: &Store, uid: u32, subject: &str, body: &str) {
    let raw =
        format!("From: a@b.com\r\nSubject: {subject}\r\nMessage-ID: <m{uid}@x>\r\n\r\n{body}\r\n");
    let e = engine::mail::parse(uid, raw.as_bytes()).unwrap();
    let s = Signals::default();
    st.add(&e, &RulesTriager.triage(&e, &s).unwrap(), &s)
        .unwrap();
}

// ── editable due dates ──
#[test]
fn a_due_date_can_be_set_changed_and_cleared_on_any_task() {
    let st = Store::memory().unwrap();
    mail_task(&st, 1, "hello", "x");
    assert!(st.set_due(1, "2026-12-24").unwrap());
    assert_eq!(st.get(1).unwrap().due, "2026-12-24");
    assert!(st.set_due(1, "").unwrap());
    assert_eq!(st.get(1).unwrap().due, "");
    let local = st.add_local_task("mine", 3, "").unwrap().unwrap();
    assert!(st.set_due(local, "2027-01-02").unwrap());
}

#[test]
fn bad_due_dates_and_unknown_tasks_are_refused_without_changing_anything() {
    let st = Store::memory().unwrap();
    mail_task(&st, 1, "hello", "x");
    st.set_due(1, "2026-12-24").unwrap();
    for bad in [
        "tomorrow",
        "2026-02-30",
        "2026-13-01",
        "12/24/2026",
        "2026-12-24 ",
        "1999-01-01",
        "2026-1-5",
    ] {
        assert!(!st.set_due(1, bad).unwrap(), "{bad}");
    }
    assert_eq!(st.get(1).unwrap().due, "2026-12-24");
    assert!(!st.set_due(999, "2026-12-24").unwrap());
}

// ── editable titles, only for tasks you added ──
#[test]
fn only_your_own_tasks_can_be_renamed() {
    let st = Store::memory().unwrap();
    mail_task(&st, 1, "from the outside", "x");
    let mine = st.add_local_task("Buy stamps", 2, "").unwrap().unwrap();
    assert!(mine >= LOCAL_UID_BASE);
    assert!(st.set_title(mine, "Buy first-class stamps").unwrap());
    let t = st.get(mine).unwrap();
    assert_eq!(
        (t.subject.as_str(), t.action.as_str()),
        ("Buy first-class stamps", "Buy first-class stamps")
    );
    assert!(
        !st.set_title(1, "renamed mail").unwrap(),
        "a message's subject is not ours to rewrite"
    );
    assert_eq!(st.get(1).unwrap().subject, "from the outside");
    for bad in ["", "   ", "bell\u{7}", &"x".repeat(201)] {
        assert!(!st.set_title(mine, bad).unwrap());
    }
    assert_eq!(st.get(mine).unwrap().subject, "Buy first-class stamps");
}

// ── reading the stored text ──
#[test]
fn the_stored_text_can_be_read_back() {
    let st = Store::memory().unwrap();
    mail_task(&st, 1, "hello", "line one\n\nline two");
    let body = st.body(1);
    assert!(body.contains("line one") && body.contains("line two"));
    assert_eq!(st.body(404), "");
}

// ── snooze until a date ──
#[test]
fn snooze_until_a_date_wakes_at_nine_that_morning() {
    let now = Local.with_ymd_and_hms(2026, 11, 13, 15, 30, 0).unwrap();
    let t = snooze_on("2026-11-20", now).expect("a future date is fine");
    let at = Local.timestamp_opt(t as i64, 0).unwrap();
    assert_eq!(at.format("%Y-%m-%d %H:%M").to_string(), "2026-11-20 09:00");
    // tomorrow works, and so does today if 09:00 has not passed
    assert!(snooze_on("2026-11-14", now).is_some());
    let early = Local.with_ymd_and_hms(2026, 11, 13, 7, 0, 0).unwrap();
    assert!(snooze_on("2026-11-13", early).is_some());
}

#[test]
fn snooze_dates_in_the_past_or_far_away_or_malformed_are_refused() {
    let now = Local.with_ymd_and_hms(2026, 11, 13, 15, 30, 0).unwrap();
    for bad in [
        "2026-11-13",
        "2026-11-12",
        "2025-01-01",
        "2028-01-01",
        "2026-02-30",
        "soon",
        "",
        "2026-11-20T09:00",
    ] {
        assert!(snooze_on(bad, now).is_none(), "{bad}");
    }
}

// ── threads ──
fn mail_with(uid: u32, headers: &str) -> engine::mail::Email {
    let raw = format!("From: a@b.com\r\nSubject: s\r\n{headers}\r\n\r\nbody\r\n");
    engine::mail::parse(uid, raw.as_bytes()).unwrap()
}

fn store_mail(st: &Store, e: &engine::mail::Email) {
    let s = Signals::default();
    st.add(e, &RulesTriager.triage(e, &s).unwrap(), &s).unwrap();
}

#[test]
fn a_reply_chain_shares_one_thread_key_rooted_at_the_first_message() {
    let m1 = mail_with(1, "Message-ID: <m1@x>");
    let m2 = mail_with(
        2,
        "Message-ID: <m2@x>\r\nIn-Reply-To: <m1@x>\r\nReferences: <m1@x>",
    );
    let m3 = mail_with(
        3,
        "Message-ID: <m3@x>\r\nIn-Reply-To: <m2@x>\r\nReferences: <m1@x> <m2@x>",
    );
    assert_eq!(m1.thread, "<m1@x>");
    assert_eq!(m2.thread, "<m1@x>");
    assert_eq!(m3.thread, "<m1@x>");
}

#[test]
fn the_root_survives_a_very_long_references_header() {
    let refs: Vec<String> = (1..=40).map(|i| format!("<r{i}@x>")).collect();
    let m = mail_with(
        9,
        &format!(
            "Message-ID: <last@x>\r\nIn-Reply-To: <r40@x>\r\nReferences: {}",
            refs.join(" ")
        ),
    );
    assert_eq!(
        m.thread, "<r1@x>",
        "the stored reference list is cut to 10, the root is not"
    );
}

#[test]
fn references_written_without_spaces_are_still_read() {
    let m = mail_with(1, "Message-ID: <m2@x>\r\nReferences: <a@x><b@x>\r\n\t<c@x>");
    assert_eq!(m.thread, "<a@x>");
    assert!(m.references.contains("<c@x>"));
}

#[test]
fn a_reply_without_references_uses_in_reply_to_and_a_lone_message_uses_its_own_id() {
    let r = mail_with(1, "Message-ID: <r@x>\r\nIn-Reply-To: <p@x>");
    assert_eq!(r.thread, "<p@x>");
    let alone = mail_with(2, "Message-ID: <alone@x>");
    assert_eq!(alone.thread, "<alone@x>");
}

#[test]
fn mail_with_no_usable_id_and_tasks_you_added_never_share_a_thread() {
    let st = Store::memory().unwrap();
    store_mail(&st, &mail_with(1, "Message-ID: not-an-id"));
    store_mail(&st, &mail_with(2, ""));
    let a = st.add_local_task("one", 3, "").unwrap().unwrap();
    let b = st.add_local_task("two", 3, "").unwrap().unwrap();
    let keys: std::collections::HashSet<String> = [1, 2, a, b]
        .iter()
        .map(|u| st.get(*u).unwrap().thread)
        .collect();
    assert_eq!(keys.len(), 4, "{keys:?}");
    assert!(st.get(1).unwrap().thread.starts_with("uid:"));
}

#[test]
fn the_thread_key_is_stored_with_the_task() {
    let st = Store::memory().unwrap();
    store_mail(
        &st,
        &mail_with(5, "Message-ID: <m2@x>\r\nReferences: <m1@x>"),
    );
    assert_eq!(st.get(5).unwrap().thread, "<m1@x>");
}

#[test]
fn a_new_message_in_a_snoozed_thread_wakes_it() {
    let st = Store::memory().unwrap();
    store_mail(&st, &mail_with(1, "Message-ID: <m1@x>"));
    store_mail(&st, &mail_with(2, "Message-ID: <other@x>"));
    st.set_snooze(1, 4_000_000_000.0).unwrap();
    st.set_snooze(2, 4_000_000_000.0).unwrap();
    store_mail(
        &st,
        &mail_with(3, "Message-ID: <m3@x>\r\nReferences: <m1@x>"),
    );
    assert_eq!(st.get(1).unwrap().snooze_until, 0.0, "the thread has news");
    assert!(
        st.get(2).unwrap().snooze_until > 0.0,
        "an unrelated snooze is untouched"
    );
}

// ── learning from corrections (rules mode) ──
fn from(uid: u32, addr: &str, name: &str, extra: &str) -> engine::mail::Email {
    let raw = format!(
        "From: {name} <{addr}>\r\nSubject: s{uid}\r\nMessage-ID: <l{uid}@x>\r\n{extra}\r\n\r\nbody\r\n"
    );
    engine::mail::parse(uid, raw.as_bytes()).unwrap()
}

fn stored_priority(st: &Store, uid: u32) -> u8 {
    st.get(uid).unwrap().priority
}

/// Adds mail from `addr` at the plain rules priority (3) and returns the stored priority.
fn arrive(st: &Store, uid: u32, addr: &str, name: &str) -> u8 {
    let e = from(uid, addr, name, "");
    let s = Signals::default(); // unknown, not bulk: rules say 3
    st.add(&e, &RulesTriager.triage(&e, &s).unwrap(), &s)
        .unwrap();
    stored_priority(st, uid)
}

#[test]
fn two_corrections_in_the_same_direction_shift_later_mail_from_that_address() {
    let st = Store::memory().unwrap();
    assert_eq!(arrive(&st, 1, "boss@x.com", "Boss"), 3);
    assert_eq!(arrive(&st, 2, "boss@x.com", "Boss"), 3);
    st.set_priority(1, 4).unwrap();
    assert_eq!(
        arrive(&st, 3, "boss@x.com", "Boss"),
        3,
        "one correction is not a pattern yet"
    );
    st.set_priority(2, 4).unwrap();
    let p = arrive(&st, 4, "boss@x.com", "Boss");
    assert_eq!(
        p, 4,
        "two upward corrections raise the next message one step"
    );
    let t = st.get(4).unwrap();
    assert_eq!(t.learned, 1);
    assert!(t.why.contains("corrections"), "{}", t.why);
}

#[test]
fn downward_corrections_lower_later_mail_but_never_below_one() {
    let st = Store::memory().unwrap();
    for u in 1..=3 {
        arrive(&st, u, "news@x.com", "News");
    }
    st.set_priority(1, 1).unwrap();
    st.set_priority(2, 1).unwrap();
    let p = arrive(&st, 4, "news@x.com", "News");
    assert!((1..=2).contains(&p), "{p}");
    assert!(st.get(4).unwrap().learned < 0);
    for u in 5..=7 {
        assert!(arrive(&st, u, "news@x.com", "News") >= 1);
    }
}

#[test]
fn mixed_directions_do_not_shift_anything() {
    let st = Store::memory().unwrap();
    for u in 1..=2 {
        arrive(&st, u, "mix@x.com", "Mix");
    }
    st.set_priority(1, 5).unwrap();
    st.set_priority(2, 1).unwrap();
    assert_eq!(arrive(&st, 3, "mix@x.com", "Mix"), 3);
    assert_eq!(st.get(3).unwrap().learned, 0);
}

#[test]
fn learning_is_keyed_on_the_address_not_the_display_name() {
    let st = Store::memory().unwrap();
    for u in 1..=2 {
        arrive(&st, u, "real@x.com", "Priya Nair");
    }
    st.set_priority(1, 5).unwrap();
    st.set_priority(2, 5).unwrap();
    assert_eq!(
        arrive(&st, 3, "impostor@evil.com", "Priya Nair"),
        3,
        "claiming a trusted display name earns nothing"
    );
    assert!(
        arrive(&st, 4, "real@x.com", "Someone Else") > 3,
        "the address is what counts"
    );
}

#[test]
fn undo_removes_the_correction_instead_of_adding_an_opposite_one() {
    let st = Store::memory().unwrap();
    for u in 1..=3 {
        arrive(&st, u, "u@x.com", "U");
    }
    st.set_priority(1, 5).unwrap();
    st.set_priority(1, 3).unwrap(); // undo: back to what it was
    assert!(
        st.recent_overrides(10).is_empty(),
        "an undone correction leaves no trace"
    );
    st.set_priority(2, 5).unwrap();
    assert_eq!(
        arrive(&st, 4, "u@x.com", "U"),
        3,
        "only one real correction exists"
    );
}

#[test]
fn a_sender_preference_wins_over_learning() {
    let st = Store::memory().unwrap();
    for u in 1..=2 {
        arrive(&st, u, "p@x.com", "P");
    }
    st.set_priority(1, 5).unwrap();
    st.set_priority(2, 5).unwrap();
    st.set_pref("p@x.com", "mute").unwrap();
    assert_eq!(arrive(&st, 3, "p@x.com", "P"), 1, "muted stays muted");
}

#[test]
fn setting_a_sender_back_to_normal_forgets_what_was_learned() {
    let st = Store::memory().unwrap();
    for u in 1..=2 {
        arrive(&st, u, "r@x.com", "R");
    }
    st.set_priority(1, 5).unwrap();
    st.set_priority(2, 5).unwrap();
    assert!(arrive(&st, 3, "r@x.com", "R") > 3);
    st.set_pref("r@x.com", "normal").unwrap();
    assert_eq!(
        arrive(&st, 4, "r@x.com", "R"),
        3,
        "the user can always start over"
    );
}

#[test]
fn mail_that_failed_its_sender_check_neither_teaches_nor_learns() {
    let st = Store::memory().unwrap();
    for u in 1..=2 {
        arrive(&st, u, "f@x.com", "F");
    }
    st.set_priority(1, 5).unwrap();
    st.set_priority(2, 5).unwrap();
    let forged = from(3, "f@x.com", "F", "Authentication-Results: mx; dkim=fail");
    let s = Signals::default();
    st.add(&forged, &RulesTriager.triage(&forged, &s).unwrap(), &s)
        .unwrap();
    assert_eq!(
        stored_priority(&st, 3),
        3,
        "a forged message does not inherit trust"
    );
    // and correcting a forged message records nothing
    let before = st.recent_overrides(10).len();
    st.set_priority(3, 1).unwrap();
    assert_eq!(st.recent_overrides(10).len(), before);
}

#[test]
fn learning_never_makes_anything_urgent() {
    let st = Store::memory().unwrap();
    for u in 1..=2 {
        arrive(&st, u, "o@x.com", "O");
    }
    st.set_priority(1, 5).unwrap();
    st.set_priority(2, 5).unwrap();
    arrive(&st, 3, "o@x.com", "O");
    assert!(!st.get(3).unwrap().urgent);
}

#[test]
fn a_shift_is_never_larger_than_two_steps() {
    let st = Store::memory().unwrap();
    for u in 1..=4 {
        arrive(&st, u, "big@x.com", "Big");
    }
    for u in 1..=3 {
        st.set_priority(u, 1).unwrap(); // 3 -> 1 is two steps each time
    }
    let t = arrive(&st, 9, "big@x.com", "Big");
    assert_eq!(t, 1);
    assert!(st.get(9).unwrap().learned >= -2);
}

// ── priority 5 from a due date, in rules mode ──
fn rules(subject: &str, body: &str, extra: &str, s: Signals) -> engine::triage::Triage {
    let raw = format!("From: Pat <pat@x.com>\r\nSubject: {subject}\r\nMessage-ID: <p@x>\r\n{extra}\r\n\r\n{body}\r\n");
    let e = engine::mail::parse(1, raw.as_bytes()).unwrap();
    RulesTriager.triage(&e, &s).unwrap()
}

fn known() -> Signals {
    Signals {
        known: true,
        ..Default::default()
    }
}

#[test]
fn a_person_waiting_for_an_answer_that_is_overdue_gets_priority_five_but_never_urgent() {
    let t = rules(
        "Contract",
        "Can you sign this? It is due 2026-01-05.",
        "",
        known(),
    );
    assert_eq!(t.priority, 5);
    assert!(
        !t.urgent,
        "rules never raise the alert-through-quiet-hours flag"
    );
    assert!(t.why.to_lowercase().contains("due"), "{}", t.why);
}

#[test]
fn it_needs_a_question_a_known_sender_and_a_near_date() {
    let q = "Can you sign this? Due 2026-01-05.";
    assert_eq!(
        rules("a", "Please sign this, due 2026-01-05.", "", known()).priority,
        4,
        "no question, so not waiting"
    );
    assert_eq!(
        rules("a", q, "", Signals::default()).priority,
        3,
        "an unknown sender is not enough"
    );
    assert_eq!(
        rules("a", "Can you sign this? Due 2099-01-05.", "", known()).priority,
        4,
        "far away is not pressing"
    );
    assert_eq!(
        rules("a", "Can you sign this?", "", known()).priority,
        4,
        "no date, no boost"
    );
    let bulk = Signals {
        known: true,
        bulk: true,
        ..Default::default()
    };
    assert!(
        rules("a", q, "", bulk).priority < 5,
        "bulk mail is never promoted"
    );
    let replying = Signals {
        reply: true,
        ..Default::default()
    };
    assert_eq!(
        rules("a", q, "", replying).priority,
        5,
        "a reply to my own mail counts"
    );
}

#[test]
fn a_failed_sender_check_blocks_the_boost() {
    let t = rules(
        "a",
        "Can you sign this? Due 2026-01-05.",
        "Authentication-Results: mx; spf=fail",
        known(),
    );
    assert!(t.priority < 5);
}

#[test]
fn a_phisher_writing_urgent_words_gets_nothing_from_them() {
    let t = rules(
        "URGENT action required immediately",
        "URGENT: asap, final notice, time-sensitive",
        "",
        Signals::default(),
    );
    assert_eq!(t.priority, 3);
    assert!(!t.urgent);
}

// ── through the command layer ──
fn core(name: &str) -> (std::sync::Arc<engine::core::Core>, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("inbox-p2-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    (
        engine::core::Core::new(dir.clone(), std::sync::Arc::new(|_, _| {})),
        dir,
    )
}

#[test]
fn a_task_of_your_own_has_nothing_to_reply_to() {
    let (c, dir) = core("localdraft");
    let uid = c
        .rpc(
            "add_task",
            &serde_json::json!({"title": "Buy stamps", "priority": 3, "due": ""}),
        )
        .unwrap();
    let err = c
        .rpc(
            "draft_generate",
            &serde_json::json!({"uid": uid, "instruction": ""}),
        )
        .unwrap_err()
        .to_string();
    assert!(err.contains("did not come from an email"), "{err}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn reading_the_stored_text_is_desktop_only_and_due_dates_work_from_the_phone() {
    let (c, dir) = core("body");
    {
        let st = c.store().unwrap();
        mail_task(&st, 1, "hello", "private words in the body");
    }
    let desktop = c.rpc("read_body", &serde_json::json!({"uid": 1})).unwrap();
    assert!(desktop.as_str().unwrap().contains("private words"));
    assert!(
        c.rpc_public("read_body", &serde_json::json!({"uid": 1}))
            .is_err(),
        "a phone gets the excerpt only"
    );
    assert!(c
        .rpc("read_body", &serde_json::json!({"uid": 404}))
        .is_err());
    let ok = c
        .rpc_public(
            "set_due",
            &serde_json::json!({"uid": 1, "due": "2026-12-24"}),
        )
        .unwrap();
    assert_eq!(ok, serde_json::json!(true));
    let bad = c
        .rpc_public("set_due", &serde_json::json!({"uid": 1}))
        .unwrap();
    assert_eq!(
        bad,
        serde_json::json!(false),
        "a missing date is not a clear request"
    );
    assert_eq!(c.store().unwrap().get(1).unwrap().due, "2026-12-24");
    let _ = std::fs::remove_dir_all(dir);
}

// ── fixes from the independent review of Phase 2 ──
fn sent_headers(subject: &str, extra: &str) -> Vec<u8> {
    format!("From: me@x.com\r\nTo: boss@x.com\r\nSubject: {subject}\r\nMessage-ID: <s@me>\r\n{extra}\r\n\r\n").into_bytes()
}

#[test]
fn a_reply_to_one_message_does_not_answer_the_older_messages_it_quotes() {
    // the boss asks (A); a colleague replies-all (B); you reply to B: References are [A, B] but you answered B only
    let mut idx = engine::signals::SentIndex::default();
    idx.add_headers(&sent_headers(
        "Re: report",
        "In-Reply-To: <b@x>\r\nReferences: <a@x> <b@x>",
    ));
    assert!(idx.replied_to.contains("<b@x>"));
    assert!(
        !idx.replied_to.contains("<a@x>"),
        "the boss's request is still unanswered"
    );
}

#[test]
fn forwards_in_other_languages_are_not_replies_either() {
    for subject in [
        "Fwd: x",
        "FW: x",
        "fwd:x",
        "Fwd : x",
        "WG: x",
        "wg: x",
        "TR: x",
        "RV: x",
        "ENC: x",
        "Doorst: x",
        "VS: x",
        "VB: x",
        "  FW: x",
    ] {
        let mut idx = engine::signals::SentIndex::default();
        idx.add_headers(&sent_headers(subject, "In-Reply-To: <a@x>"));
        assert!(idx.replied_to.is_empty(), "{subject:?} is a forward");
    }
    for subject in [
        "Re: x",
        "RE: x",
        "AW: x",
        "SV: x",
        "Returned: x",
        "Fwdx",
        "Forward the report",
        "Frank: x",
    ] {
        let mut idx = engine::signals::SentIndex::default();
        idx.add_headers(&sent_headers(subject, "In-Reply-To: <a@x>"));
        assert!(
            idx.replied_to.contains("<a@x>"),
            "{subject:?} is a real reply"
        );
    }
}

#[test]
fn moving_one_mail_up_twice_is_one_correction_not_a_pattern() {
    let st = Store::memory().unwrap();
    for u in 1..=3 {
        arrive(&st, u, "x@x.com", "X");
    }
    st.set_priority(1, 4).unwrap();
    st.set_priority(1, 5).unwrap(); // the same mail, a second nudge
    assert_eq!(
        st.recent_overrides(10).len(),
        1,
        "one mail, one correction: first priority to latest"
    );
    assert_eq!(
        st.learned_shift("x@x.com"),
        0,
        "a single mail is not a pattern"
    );
    assert_eq!(arrive(&st, 4, "x@x.com", "X"), 3);
}

#[test]
fn walking_a_mail_back_to_where_it_started_leaves_no_correction() {
    let st = Store::memory().unwrap();
    for u in 1..=3 {
        arrive(&st, u, "w@x.com", "W");
    }
    st.set_priority(1, 5).unwrap();
    st.set_priority(1, 4).unwrap();
    st.set_priority(1, 3).unwrap();
    assert!(
        st.recent_overrides(10).is_empty(),
        "3 -> 5 -> 4 -> 3 nets to nothing"
    );
    st.set_priority(2, 1).unwrap();
    assert_eq!(
        st.learned_shift("w@x.com"),
        0,
        "one real correction is still not a pattern"
    );
}

#[test]
fn a_correction_only_ever_matches_its_own_sender_and_task() {
    let st = Store::memory().unwrap();
    arrive(&st, 1, "old@x.com", "Old");
    st.set_priority(1, 5).unwrap();
    st.reset_for_new_account().unwrap(); // the account changed: the numbers mean something else now
    assert!(
        st.recent_overrides(10).is_empty(),
        "nothing learned about one mailbox carries to another"
    );
    arrive(&st, 1, "new@x.com", "New"); // the same uid number, a different sender
    st.set_priority(1, 4).unwrap();
    assert_eq!(
        st.recent_overrides(10).len(),
        1,
        "this is a fresh correction, not the undo of an old one"
    );
}

#[test]
fn a_task_you_reopened_stays_yours_even_after_a_stray_close_and_undo() {
    let st = Store::memory().unwrap();
    mail_task(&st, 1, "q", "x");
    assert!(st.auto_close(1, "replied").unwrap());
    st.set_done(1, false).unwrap(); // you reopen it
    st.set_done(1, true).unwrap(); // a stray click closes it...
    st.set_done(1, false).unwrap(); // ...and undo reopens it
    assert!(
        !st.auto_close(1, "replied").unwrap(),
        "the app must never close it again"
    );
}

#[test]
fn a_forged_message_cannot_wake_a_snoozed_conversation() {
    let st = Store::memory().unwrap();
    store_mail(&st, &mail_with(1, "Message-ID: <m1@x>"));
    st.set_snooze(1, 4_000_000_000.0).unwrap();
    store_mail(
        &st,
        &mail_with(
            2,
            "Message-ID: <fake@x>\r\nReferences: <m1@x>\r\nAuthentication-Results: mx; dkim=fail",
        ),
    );
    assert!(st.get(1).unwrap().snooze_until > 0.0);
}

#[test]
fn waking_a_conversation_notifies_once_not_once_per_message() {
    let st = Store::memory().unwrap();
    store_mail(&st, &mail_with(1, "Message-ID: <m1@x>"));
    store_mail(
        &st,
        &mail_with(2, "Message-ID: <m2@x>\r\nReferences: <m1@x>"),
    );
    store_mail(&st, &mail_with(3, "Message-ID: <other@x>"));
    for u in [1, 2, 3] {
        st.set_snooze(u, 1.0).unwrap();
    }
    let due = st.wake_due(10.0).unwrap();
    assert_eq!(due.len(), 3);
    assert_eq!(
        engine::jobs::wake_notices(&due).len(),
        2,
        "one for the conversation, one for the other task"
    );
}

#[test]
fn a_renumbered_inbox_keeps_what_was_learned_but_forgets_which_task_it_came_from() {
    let st = Store::memory().unwrap();
    st.reconcile_uid_validity(100).unwrap();
    for u in 1..=2 {
        arrive(&st, u, "keep@x.com", "Keep");
    }
    st.set_priority(1, 5).unwrap();
    st.set_priority(2, 5).unwrap();
    assert!(
        st.reconcile_uid_validity(200).unwrap(),
        "the server renumbered"
    );
    assert_eq!(
        st.recent_overrides(10).len(),
        2,
        "the corrections about the sender survive"
    );
    arrive(&st, 1, "other@x.com", "Other"); // a new message that happens to get the old number 1
    st.set_priority(1, 4).unwrap();
    assert_eq!(
        st.recent_overrides(10).len(),
        3,
        "it is a new correction, not the undo of an old one"
    );
}
