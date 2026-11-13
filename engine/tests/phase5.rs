//! Phase 5: reminders and repeats, search inside message text, calendar invitations. See docs/designs/phase5.md.
use chrono::NaiveDateTime;
use engine::reminders::{next_occurrence, notices, parse_local};
use engine::store::{Store, LOCAL_UID_BASE};

fn dt(s: &str) -> NaiveDateTime {
    parse_local(s).unwrap_or_else(|| panic!("bad test time {s}"))
}
fn own(st: &Store, title: &str) -> u32 {
    st.add_local_task(title, 3, "").unwrap().unwrap()
}

// ───────────────────────── reminders: setting ─────────────────────────
#[test]
fn a_reminder_needs_a_real_time_that_has_not_passed_and_a_task_of_your_own() {
    let st = Store::memory().unwrap();
    let mine = own(&st, "bins");
    let now = dt("2026-11-13 10:30");
    for bad in [
        "2026-13-01 10:00",
        "2026-02-30 10:00",
        "2026-11-13T10:30",
        "2026-11-13 25:00",
        "2026-11-13 10:60",
        "10:30",
        "tomorrow",
        "2026-11-13 10:30:00",
        "1999-12-31 10:00",
        "2101-01-01 10:00",
        "2026-11-13 10:29", // a minute ago
        "2025-01-01 09:00",
    ] {
        assert!(
            st.set_reminder(mine, bad, "", now).is_err(),
            "accepted {bad:?}"
        );
    }
    assert!(st.get(mine).unwrap().remind.is_empty());
    // the minute you are in still counts
    st.set_reminder(mine, "2026-11-13 10:30", "", now).unwrap();
    assert_eq!(st.get(mine).unwrap().remind, "2026-11-13 10:30");
    st.set_reminder(mine, "2026-11-14 09:00", "", now).unwrap();
    assert_eq!(st.get(mine).unwrap().remind, "2026-11-14 09:00");
    // not for mail, not for a task that is not there
    let raw = b"From: a@b.com\r\nSubject: hi\r\nMessage-ID: <m1@x>\r\n\r\nhello\r\n";
    let e = engine::mail::parse(1, raw).unwrap();
    let s = engine::signals::Signals::default();
    use engine::triage::Triager;
    st.add(
        &e,
        &engine::triage::RulesTriager.triage(&e, &s).unwrap(),
        &s,
    )
    .unwrap();
    assert!(st.set_reminder(1, "2026-11-14 09:00", "", now).is_err());
    assert!(st.set_reminder(999, "2026-11-14 09:00", "", now).is_err());
    assert!(st.get(1).unwrap().remind.is_empty());
}

#[test]
fn a_repeat_needs_a_reminder_or_a_due_date_and_only_three_kinds_exist() {
    let st = Store::memory().unwrap();
    let mine = own(&st, "bins");
    let now = dt("2026-11-13 10:30");
    for bad in ["yearly", "Daily", "hourly", "daily ", "1"] {
        assert!(
            st.set_reminder(mine, "2026-11-14 09:00", bad, now).is_err(),
            "{bad}"
        );
    }
    assert!(
        st.set_reminder(mine, "", "daily", now).is_err(),
        "no reminder and no due date"
    );
    for ok in ["daily", "weekly", "monthly"] {
        st.set_reminder(mine, "2026-11-14 09:00", ok, now).unwrap();
        assert_eq!(st.get(mine).unwrap().repeat, ok);
    }
    // due date only: the repeat only moves the due date
    let dated = st.add_local_task("rent", 3, "2026-12-01").unwrap().unwrap();
    st.set_reminder(dated, "", "monthly", now).unwrap();
    let t = st.get(dated).unwrap();
    assert_eq!((t.remind.as_str(), t.repeat.as_str()), ("", "monthly"));
}

#[test]
fn clearing_a_reminder_clears_its_repeat_too() {
    let st = Store::memory().unwrap();
    let mine = own(&st, "bins");
    let now = dt("2026-11-13 10:30");
    st.set_reminder(mine, "2026-11-14 09:00", "weekly", now)
        .unwrap();
    st.set_reminder(mine, "", "", now).unwrap();
    let t = st.get(mine).unwrap();
    assert_eq!((t.remind.as_str(), t.repeat.as_str()), ("", ""));
}

// ───────────────────────── next occurrence ─────────────────────────
#[test]
fn the_next_occurrence_is_the_first_one_after_the_moment_given() {
    let base = dt("2026-11-13 09:00");
    assert_eq!(
        next_occurrence(base, "daily", dt("2026-11-13 09:00")),
        Some(dt("2026-11-14 09:00"))
    );
    assert_eq!(
        next_occurrence(base, "daily", dt("2026-11-13 08:00")),
        Some(base),
        "the first one is still ahead"
    );
    assert_eq!(
        next_occurrence(base, "daily", dt("2026-11-20 09:30")),
        Some(dt("2026-11-21 09:00"))
    );
    assert_eq!(
        next_occurrence(base, "weekly", dt("2026-11-13 09:00")),
        Some(dt("2026-11-20 09:00"))
    );
    assert_eq!(
        next_occurrence(base, "weekly", dt("2026-12-30 12:00")),
        Some(dt("2027-01-01 09:00"))
    );
    assert_eq!(next_occurrence(base, "", dt("2026-11-13 09:00")), None);
    assert_eq!(
        next_occurrence(base, "yearly", dt("2026-11-13 09:00")),
        None
    );
}

#[test]
fn a_monthly_task_set_for_the_31st_does_not_drift_and_handles_leap_years() {
    let base = dt("2027-01-31 09:00");
    let mut at = base;
    let mut seen = vec![];
    for _ in 0..6 {
        at = next_occurrence(base, "monthly", at).unwrap();
        seen.push(at.format("%Y-%m-%d").to_string());
    }
    assert_eq!(
        seen,
        [
            "2027-02-28",
            "2027-03-31",
            "2027-04-30",
            "2027-05-31",
            "2027-06-30",
            "2027-07-31"
        ]
    );
    let leap = dt("2027-12-31 09:00");
    assert_eq!(
        next_occurrence(leap, "monthly", dt("2028-01-31 09:00")),
        Some(dt("2028-02-29 09:00"))
    );
    assert_eq!(
        next_occurrence(leap, "monthly", dt("2027-12-31 09:00")),
        Some(dt("2028-01-31 09:00")),
        "year roll"
    );
    // a 29 February start lands on the 28th in other years
    let feb = dt("2028-02-29 09:00");
    assert_eq!(
        next_occurrence(feb, "monthly", feb),
        Some(dt("2028-03-29 09:00"))
    );
    assert_eq!(
        next_occurrence(feb, "monthly", dt("2028-12-29 09:00")),
        Some(dt("2029-01-29 09:00"))
    );
}

#[test]
fn a_long_absence_still_finds_the_next_occurrence_quickly() {
    let base = dt("2026-11-13 09:00");
    let far = dt("2090-01-01 00:00");
    assert!(next_occurrence(base, "daily", far).unwrap() > far);
    assert!(next_occurrence(base, "weekly", far).unwrap() > far);
    assert!(next_occurrence(base, "monthly", far).unwrap() > far);
}

#[test]
fn local_time_text_round_trips_and_nothing_else_is_accepted() {
    assert_eq!(
        dt("2026-11-13 09:05").format("%Y-%m-%d %H:%M").to_string(),
        "2026-11-13 09:05"
    );
    for bad in [
        "",
        " 2026-11-13 09:05",
        "2026-11-13 09:05 ",
        "2026-11-13 9:05",
        "2026-1-13 09:05",
        "2026-11-13  09:05",
    ] {
        assert!(parse_local(bad).is_none(), "{bad:?}");
    }
}

// ───────────────────────── firing ─────────────────────────
#[test]
fn a_reminder_fires_once_and_again_only_when_it_is_changed() {
    let st = Store::memory().unwrap();
    let a = own(&st, "bins");
    let now = dt("2026-11-13 08:00");
    st.set_reminder(a, "2026-11-13 09:00", "", now).unwrap();
    assert!(st.due_reminders(dt("2026-11-13 08:59")).unwrap().is_empty());
    let fired = st.due_reminders(dt("2026-11-13 09:00")).unwrap();
    assert_eq!(fired.len(), 1);
    assert_eq!(fired[0].uid, a);
    assert!(
        st.due_reminders(dt("2026-11-13 09:01")).unwrap().is_empty(),
        "never twice"
    );
    assert!(
        st.due_reminders(dt("2026-11-20 09:00")).unwrap().is_empty(),
        "not after a restart either"
    );
    // a new time re-arms it
    st.set_reminder(a, "2026-11-14 09:00", "", dt("2026-11-13 09:05"))
        .unwrap();
    assert!(st.due_reminders(dt("2026-11-13 12:00")).unwrap().is_empty());
    assert_eq!(st.due_reminders(dt("2026-11-14 09:00")).unwrap().len(), 1);
    // choosing the same time again changes nothing, so it is not announced a second time
    st.set_reminder(a, "2026-11-14 09:00", "", dt("2026-11-14 09:00"))
        .unwrap();
    assert!(st.due_reminders(dt("2026-11-14 09:10")).unwrap().is_empty());
}

#[test]
fn editing_the_due_date_of_a_due_only_repeat_moves_its_count_and_clearing_it_drops_the_repeat() {
    let st = Store::memory().unwrap();
    let a = st.add_local_task("rent", 3, "2026-12-01").unwrap().unwrap();
    st.set_reminder(a, "", "monthly", dt("2026-11-13 08:00"))
        .unwrap();
    st.set_due(a, "2026-12-15").unwrap();
    st.set_done_at(a, true, dt("2026-12-15 10:00")).unwrap();
    assert_eq!(
        st.get(a).unwrap().due,
        "2027-01-15",
        "counted from the date you chose"
    );
    st.set_due(a, "").unwrap();
    let t = st.get(a).unwrap();
    assert_eq!(
        (t.repeat.as_str(), t.due.as_str()),
        ("", ""),
        "no date, nothing to repeat"
    );
}

#[test]
fn a_due_date_that_sat_after_the_reminder_keeps_its_distance() {
    let st = Store::memory().unwrap();
    let a = st
        .add_local_task("report", 3, "2026-11-19")
        .unwrap()
        .unwrap();
    st.set_reminder(a, "2026-11-17 09:00", "weekly", dt("2026-11-13 08:00"))
        .unwrap();
    st.set_done_at(a, true, dt("2026-11-17 09:30")).unwrap();
    let t = st.get(a).unwrap();
    assert_eq!(
        (t.remind.as_str(), t.due.as_str()),
        ("2026-11-24 09:00", "2026-11-26")
    );
}

#[test]
fn the_reminder_commands_work_for_your_own_tasks_and_explain_a_refusal() {
    use serde_json::json;
    let dir = std::env::temp_dir().join(format!("ia-p5-cmd-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let c = engine::core::Core::new(dir.clone(), std::sync::Arc::new(|_, _| {}));
    let mine = c
        .store()
        .unwrap()
        .add_local_task("bins", 3, "")
        .unwrap()
        .unwrap();
    let future = (chrono::Local::now() + chrono::Duration::days(3))
        .format("%Y-%m-%d 09:00")
        .to_string();
    // the phone may set one too: it only edits a task of yours, like a due date
    c.rpc_public(
        "set_reminder",
        &json!({"uid": mine, "at": future, "repeat": "weekly"}),
    )
    .unwrap();
    let t = c.store().unwrap().get(mine).unwrap();
    assert_eq!(
        (t.remind.as_str(), t.repeat.as_str()),
        (future.as_str(), "weekly")
    );
    let past = (chrono::Local::now() - chrono::Duration::days(3))
        .format("%Y-%m-%d 09:00")
        .to_string();
    let e = c
        .rpc("set_reminder", &json!({"uid": mine, "at": past}))
        .unwrap_err()
        .to_string();
    assert!(e.contains("has not passed"), "{e}");
    let e = c
        .rpc("set_reminder", &json!({"uid": mine, "at": "soon"}))
        .unwrap_err()
        .to_string();
    assert!(e.contains("date and a time"), "{e}");
    let e = c
        .rpc("set_reminder", &json!({"uid": 5, "at": future}))
        .unwrap_err()
        .to_string();
    assert!(e.contains("no such task"), "{e}");
    c.rpc("set_reminder", &json!({"uid": mine, "at": ""}))
        .unwrap();
    assert!(c.store().unwrap().get(mine).unwrap().remind.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn finished_tasks_and_cleared_reminders_never_fire() {
    let st = Store::memory().unwrap();
    let (a, b, c) = (own(&st, "a"), own(&st, "b"), own(&st, "c"));
    let now = dt("2026-11-13 08:00");
    for t in [a, b, c] {
        st.set_reminder(t, "2026-11-13 09:00", "", now).unwrap();
    }
    st.set_done(a, true).unwrap();
    st.set_reminder(b, "", "", now).unwrap();
    let fired: Vec<u32> = st
        .due_reminders(dt("2026-11-13 09:30"))
        .unwrap()
        .iter()
        .map(|t| t.uid)
        .collect();
    assert_eq!(fired, [c]);
}

#[test]
fn a_reminder_that_came_due_while_the_app_was_closed_fires_when_it_starts() {
    let st = Store::memory().unwrap();
    let a = own(&st, "bins");
    st.set_reminder(a, "2026-11-13 09:00", "", dt("2026-11-13 08:00"))
        .unwrap();
    assert_eq!(st.due_reminders(dt("2026-11-16 18:00")).unwrap().len(), 1);
}

#[test]
fn up_to_three_reminders_get_their_own_notice_and_more_become_one() {
    let st = Store::memory().unwrap();
    let now = dt("2026-11-13 08:00");
    let ids: Vec<u32> = (1..=5).map(|i| own(&st, &format!("task {i}"))).collect();
    for id in &ids {
        st.set_reminder(*id, "2026-11-13 09:00", "", now).unwrap();
    }
    let all = st.due_reminders(dt("2026-11-13 09:00")).unwrap();
    assert_eq!(all.len(), 5);
    let three = notices(&all[..3]);
    assert_eq!(three.len(), 3);
    assert!(three
        .iter()
        .all(|(title, body)| title == "Reminder" && body.starts_with("task ")));
    let many = notices(&all);
    assert_eq!(many.len(), 1);
    assert_eq!(
        many[0],
        ("Reminders".to_string(), "5 reminders are due".to_string())
    );
    assert!(notices(&[]).is_empty());
}

// ───────────────────────── completing a repeating task ─────────────────────────
#[test]
fn completing_a_repeating_task_moves_it_to_the_next_time_and_keeps_it_open() {
    let st = Store::memory().unwrap();
    let a = own(&st, "bins");
    st.set_reminder(a, "2026-11-17 07:00", "weekly", dt("2026-11-13 08:00"))
        .unwrap();
    st.set_due(a, "2026-11-17").unwrap();
    st.set_snooze(a, 4_000_000_000.0).unwrap();
    st.due_reminders(dt("2026-11-17 07:00")).unwrap();
    // done on the day, after it fired
    assert!(st.set_done_at(a, true, dt("2026-11-17 08:00")).unwrap());
    let t = st.get(a).unwrap();
    assert_eq!(t.status, "open", "it never goes to Done");
    assert_eq!(
        (t.remind.as_str(), t.due.as_str()),
        ("2026-11-24 07:00", "2026-11-24")
    );
    assert_eq!(t.repeat, "weekly");
    assert_eq!(t.snooze_until, 0.0);
    assert!(st.due_reminders(dt("2026-11-24 06:59")).unwrap().is_empty());
    assert_eq!(
        st.due_reminders(dt("2026-11-24 07:00")).unwrap().len(),
        1,
        "the next occurrence fires"
    );
}

#[test]
fn completing_early_or_very_late_lands_on_the_first_occurrence_that_is_still_ahead() {
    let st = Store::memory().unwrap();
    let (early, late) = (own(&st, "early"), own(&st, "late"));
    let now = dt("2026-11-13 08:00");
    st.set_reminder(early, "2026-11-20 09:00", "daily", now)
        .unwrap();
    st.set_reminder(late, "2026-11-14 09:00", "daily", now)
        .unwrap();
    // done a week before it was due: the next one is after the one that was set
    st.set_done_at(early, true, dt("2026-11-13 09:00")).unwrap();
    assert_eq!(st.get(early).unwrap().remind, "2026-11-21 09:00");
    // done three weeks late: the missed days are skipped
    st.set_done_at(late, true, dt("2026-12-05 10:00")).unwrap();
    assert_eq!(st.get(late).unwrap().remind, "2026-12-06 09:00");
}

#[test]
fn a_monthly_task_keeps_its_day_across_short_months() {
    let st = Store::memory().unwrap();
    let a = own(&st, "rent");
    st.set_reminder(a, "2027-01-31 09:00", "monthly", dt("2027-01-01 08:00"))
        .unwrap();
    let mut seen = vec![];
    for day in ["2027-01-31 10:00", "2027-02-28 10:00", "2027-03-31 10:00"] {
        st.set_done_at(a, true, dt(day)).unwrap();
        seen.push(st.get(a).unwrap().remind);
    }
    assert_eq!(
        seen,
        ["2027-02-28 09:00", "2027-03-31 09:00", "2027-04-30 09:00"]
    );
}

#[test]
fn a_repeat_with_only_a_due_date_moves_the_due_date() {
    let st = Store::memory().unwrap();
    let a = st.add_local_task("rent", 3, "2026-12-01").unwrap().unwrap();
    st.set_reminder(a, "", "monthly", dt("2026-11-13 08:00"))
        .unwrap();
    st.set_done_at(a, true, dt("2026-12-01 10:00")).unwrap();
    let t = st.get(a).unwrap();
    assert_eq!(
        (t.status.as_str(), t.due.as_str(), t.remind.as_str()),
        ("open", "2027-01-01", "")
    );
}

#[test]
fn an_ordinary_task_still_completes_and_reopens_as_before() {
    let st = Store::memory().unwrap();
    let a = own(&st, "once");
    st.set_reminder(a, "2026-11-14 09:00", "", dt("2026-11-13 08:00"))
        .unwrap();
    assert!(st.set_done(a, true).unwrap());
    assert_eq!(st.get(a).unwrap().status, "done");
    assert!(st.set_done(a, false).unwrap());
    assert_eq!(st.get(a).unwrap().status, "open");
    assert!(a >= LOCAL_UID_BASE);
}

#[test]
fn a_first_occurrence_far_in_the_future_is_returned_as_it_is() {
    let base = dt("2026-12-25 09:00");
    let after = dt("2026-11-13 10:00");
    for repeat in ["daily", "weekly", "monthly"] {
        assert_eq!(next_occurrence(base, repeat, after), Some(base), "{repeat}");
    }
}

// ───────────────────────── search inside message text ─────────────────────────
use engine::signals::Signals;
use engine::triage::{RulesTriager, Triager};

fn mail(st: &Store, uid: u32, from: &str, subject: &str, body: &str) {
    let raw =
        format!("From: {from}\r\nSubject: {subject}\r\nMessage-ID: <s{uid}@x>\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n{body}\r\n");
    let e = engine::mail::parse(uid, raw.as_bytes()).unwrap();
    let s = Signals::default();
    st.add(&e, &RulesTriager.triage(&e, &s).unwrap(), &s)
        .unwrap();
}
fn found(st: &Store, q: &str) -> Vec<u32> {
    let mut v = st.search(q).unwrap();
    v.sort_unstable();
    v
}

#[test]
fn search_finds_words_anywhere_in_a_message_and_wants_all_of_them() {
    let st = Store::memory().unwrap();
    mail(
        &st,
        1,
        "Ana Builder <ana@builder.example>",
        "Quote",
        "The boiler needs a new valve before winter.",
    );
    mail(
        &st,
        2,
        "Bo <bo@x.example>",
        "Heating",
        "Nothing about the other thing here.",
    );
    mail(
        &st,
        3,
        "Cy <cy@y.example>",
        "Boiler service",
        "Annual visit booked.",
    );
    assert_eq!(found(&st, "valve"), [1], "a word that is only in the text");
    assert_eq!(found(&st, "boiler"), [1, 3], "text and subject");
    assert_eq!(found(&st, "builder"), [1], "the sender's name");
    assert_eq!(
        found(&st, "ana@builder.example"),
        [1],
        "an address is split into its words"
    );
    assert_eq!(found(&st, "winter valve"), [1], "all words, in any order");
    assert_eq!(
        found(&st, "valve visit"),
        Vec::<u32>::new(),
        "all of them, not any of them"
    );
    assert!(found(&st, "zebra").is_empty());
}

#[test]
fn only_the_last_word_may_be_a_prefix_and_case_and_accents_do_not_matter() {
    let st = Store::memory().unwrap();
    mail(
        &st,
        1,
        "x@y.example",
        "Menu",
        "We met at the Café on Friday about the boiler.",
    );
    assert_eq!(found(&st, "boil"), [1], "a prefix of the last word");
    assert_eq!(
        found(&st, "boil friday"),
        Vec::<u32>::new(),
        "an earlier word must be whole: boil is not a word here"
    );
    assert_eq!(found(&st, "friday boil"), [1]);
    assert_eq!(found(&st, "CAFE"), [1]);
    assert_eq!(found(&st, "café"), [1]);
    assert_eq!(found(&st, "cafe"), [1]);
}

#[test]
fn what_you_type_is_never_search_syntax() {
    let st = Store::memory().unwrap();
    mail(
        &st,
        1,
        "x@y.example",
        "plain",
        "only the word secret appears here",
    );
    mail(&st, 2, "x@y.example", "both", "body secret together");
    for q in [
        "OR",
        "AND",
        "NOT",
        "NEAR(a b)",
        "NEAR",
        "\"",
        "\"\"",
        "a*",
        "*",
        "-x",
        "^x",
        "(",
        ")",
        "((",
        ":",
        "{body}: x",
        "col:\"x\"",
        "a OR b",
        "x NOT y",
        "'; DROP TABLE tasks; --",
        "\u{0}",
        "body:",
        "subject:x",
    ] {
        assert!(st.search(q).is_ok(), "{q:?} made the search fail");
    }
    // a column filter is just two words
    assert_eq!(
        found(&st, "body:secret"),
        [2],
        "needs the words body AND secret, not a column"
    );
    assert_eq!(found(&st, "secret"), [1, 2]);
    // an operator word is just a word
    assert!(found(&st, "secret OR zebra").is_empty());
    assert!(found(&st, "NOT secret").is_empty());
    assert!(
        st.search("").unwrap().is_empty()
            && st.search("   ").unwrap().is_empty()
            && st.search("*** ---").unwrap().is_empty()
    );
    assert_eq!(st.all_tasks().unwrap().len(), 2, "nothing was dropped");
}

#[test]
fn a_query_is_limited_to_a_hundred_characters_and_eight_words() {
    let st = Store::memory().unwrap();
    mail(
        &st,
        1,
        "x@y.example",
        "s",
        "one two three four five six seven eight nine ten",
    );
    assert_eq!(found(&st, "one two three four five six seven eight"), [1]);
    assert_eq!(
        found(&st, "one two three four five six seven eight zebra"),
        [1],
        "only the first eight words count"
    );
    let long = format!("one {}", "x".repeat(500));
    assert!(st.search(&long).is_ok());
    // words that start after the hundredth character are not part of the query
    let spaced = format!("one{}zebra", " ".repeat(120));
    assert_eq!(found(&st, &spaced), [1]);
    assert_eq!(
        engine::store::fts_query(&"word ".repeat(40))
            .unwrap()
            .matches('"')
            .count(),
        2 * 8
    );
    assert!(engine::store::fts_query("").is_none());
}

#[test]
fn the_index_follows_every_change_to_a_task() {
    let st = Store::memory().unwrap();
    mail(&st, 1, "x@y.example", "old subject", "alpha text");
    assert_eq!(found(&st, "alpha"), [1]);
    // the app replaces rows: the old words must go
    mail(&st, 1, "x@y.example", "new subject", "beta text");
    assert!(
        found(&st, "alpha").is_empty() && found(&st, "old").is_empty(),
        "a replaced row leaves no stale words"
    );
    assert_eq!(found(&st, "beta"), [1]);
    // your note is searchable and follows edits
    st.set_note(1, "call the plumber").unwrap();
    assert_eq!(found(&st, "plumber"), [1]);
    st.set_note(1, "call the electrician").unwrap();
    assert!(found(&st, "plumber").is_empty());
    assert_eq!(found(&st, "electrician"), [1]);
    // a task of your own can be renamed
    let mine = st.add_local_task("buy stamps", 3, "").unwrap().unwrap();
    assert_eq!(found(&st, "stamps"), [mine]);
    st.set_title(mine, "buy envelopes").unwrap();
    assert!(found(&st, "stamps").is_empty());
    assert_eq!(found(&st, "envelopes"), [mine]);
    // status changes do not disturb it
    st.set_done(1, true).unwrap();
    assert_eq!(found(&st, "beta"), [1]);
}

#[test]
fn deleting_tasks_removes_them_from_the_index_whatever_deletes_them() {
    let st = Store::memory().unwrap();
    for (uid, w) in [(1u32, "pruned"), (2, "kept"), (3, "renumbered")] {
        mail(&st, uid, "x@y.example", "s", &format!("word {w}"));
    }
    st.set_done(1, true).unwrap();
    st.backdate_done(1, 1).unwrap();
    st.prune(90, 30).unwrap();
    assert!(found(&st, "pruned").is_empty(), "pruning");
    assert_eq!(found(&st, "kept"), [2]);
    st.reset_mailbox_data().unwrap();
    assert!(
        found(&st, "kept").is_empty() && found(&st, "renumbered").is_empty(),
        "a renumbered mailbox"
    );
}

#[test]
fn signing_out_leaves_no_message_text_in_the_index() {
    let dir = std::env::temp_dir().join(format!("ia-p5-wipe-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("agent.db");
    {
        let st = Store::open(&path).unwrap();
        mail(
            &st,
            1,
            "Ana <ana@builder.example>",
            "Quote",
            "the boiler secret-phrase-xyz",
        );
        st.set_note(1, "note-phrase-abc").unwrap();
        assert_eq!(found(&st, "xyz"), [1]);
        st.wipe_all().unwrap();
        assert!(st.search("boiler").unwrap().is_empty() && st.search("abc").unwrap().is_empty());
    }
    // and nothing is left in the file itself, in the table, the log or free pages
    let bytes: Vec<u8> = ["agent.db", "agent.db-wal"]
        .iter()
        .flat_map(|f| std::fs::read(dir.join(f)).unwrap_or_default())
        .collect();
    for needle in ["boiler", "secret-phrase", "note-phrase", "builder"] {
        assert!(
            !bytes.windows(needle.len()).any(|w| w == needle.as_bytes()),
            "{needle} is still on disk after sign-out"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_index_stays_consistent_after_a_mixed_run_of_changes() {
    let dir = std::env::temp_dir().join(format!("ia-p5-integrity-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("agent.db");
    let st = Store::open(&path).unwrap();
    for uid in 1..=20u32 {
        mail(
            &st,
            uid,
            "x@y.example",
            &format!("subject {uid}"),
            &format!("body number {uid} common"),
        );
    }
    for uid in 1..=20u32 {
        if uid % 3 == 0 {
            mail(&st, uid, "x@y.example", "replaced", "fresh words");
        }
        if uid % 4 == 0 {
            st.set_note(uid, "a note").unwrap();
        }
    }
    st.reset_mailbox_data().unwrap();
    for uid in 1..=5u32 {
        mail(&st, uid, "x@y.example", "again", "again common");
    }
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute(
        "INSERT INTO tasks_fts(tasks_fts, rank) VALUES('integrity-check', 1)",
        [],
    )
    .expect("the full-text index disagrees with the tasks it indexes");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_database_from_the_first_version_gets_its_index_when_it_is_opened() {
    let dir = std::env::temp_dir().join(format!("ia-p5-upgrade-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("agent.db");
    {
        let st = Store::open(&path).unwrap();
        mail(
            &st,
            1,
            "x@y.example",
            "old mail",
            "a legacy-word lives here",
        );
    }
    // turn it back into what version 1 left behind: no index, no triggers, the old version number
    {
        let db = rusqlite::Connection::open(&path).unwrap();
        db.execute_batch(
            "DROP TRIGGER tasks_fts_ai; DROP TRIGGER tasks_fts_ad; DROP TRIGGER tasks_fts_au; DROP TABLE tasks_fts; PRAGMA user_version=1;",
        )
        .unwrap();
    }
    let st = Store::open(&path).unwrap();
    assert_eq!(
        found(&st, "legacy"),
        [1],
        "the existing mail is searchable straight away"
    );
    mail(&st, 2, "x@y.example", "new mail", "a fresh-word arrives");
    assert_eq!(found(&st, "fresh"), [2], "and new mail is indexed");
    let mut bak = path.as_os_str().to_owned();
    bak.push(".bak");
    assert!(
        std::path::Path::new(&bak).exists(),
        "the one-time copy of the old database was made first"
    );
    let v: i32 = rusqlite::Connection::open(&path)
        .unwrap()
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(v, engine::store::SCHEMA_VERSION);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn search_is_a_desktop_command_the_phone_cannot_use() {
    use serde_json::json;
    let dir = std::env::temp_dir().join(format!("ia-p5-hub-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let c = engine::core::Core::new(dir.clone(), std::sync::Arc::new(|_, _| {}));
    mail(
        &c.store().unwrap(),
        1,
        "x@y.example",
        "s",
        "needle in the text",
    );
    let hits = c.rpc("search", &json!({"q": "needle"})).unwrap();
    assert_eq!(hits, json!([1]));
    assert_eq!(
        c.rpc("search", &json!({})).unwrap(),
        json!([]),
        "no query, no hits"
    );
    let e = c
        .rpc_public("search", &json!({"q": "needle"}))
        .unwrap_err()
        .to_string();
    assert!(e.contains("not available from the hub"), "{e}");
    assert!(!engine::commands::PUBLIC.contains(&"search"));
    let _ = std::fs::remove_dir_all(&dir);
}

// ───────────────────────── calendar invitations ─────────────────────────
use engine::calendar::{self, Event};

fn ics(lines: &[&str]) -> String {
    lines.join("\r\n") + "\r\n"
}

fn invite(extra: &[&str]) -> String {
    let mut v = vec![
        "BEGIN:VCALENDAR",
        "VERSION:2.0",
        "METHOD:REQUEST",
        "BEGIN:VEVENT",
        "UID:abc@outlook",
        "SUMMARY:Quarterly planning",
        "DTSTART:20261120T093000Z",
        "DTEND:20261120T103000Z",
        "LOCATION:Room 4",
    ];
    v.extend_from_slice(extra);
    v.extend_from_slice(&["END:VEVENT", "END:VCALENDAR"]);
    ics(&v)
}

fn ev(summary: &str, start: &str) -> Event {
    Event {
        summary: summary.into(),
        start: start.into(),
        end: String::new(),
        tzid: String::new(),
        location: String::new(),
        all_day: start.len() == 8,
        cancelled: false,
        zone_name: String::new(),
        recurring: false,
    }
}

#[test]
fn a_utc_invitation_is_read() {
    let e = calendar::parse(&invite(&[])).unwrap();
    assert_eq!(e.summary, "Quarterly planning");
    assert_eq!(
        (e.start.as_str(), e.end.as_str()),
        ("20261120T093000Z", "20261120T103000Z")
    );
    assert_eq!(
        (e.location.as_str(), e.all_day, e.cancelled),
        ("Room 4", false, false)
    );
    assert_eq!(calendar::day(&e).as_deref(), Some("2026-11-20"));
}

#[test]
fn zones_floating_times_and_all_day_events() {
    let berlin = ics(&[
        "BEGIN:VCALENDAR",
        "BEGIN:VEVENT",
        "SUMMARY:Call",
        "DTSTART;TZID=Europe/Berlin:20261120T093000",
        "DTEND;TZID=Europe/Berlin:20261120T100000",
        "END:VEVENT",
        "END:VCALENDAR",
    ]);
    let e = calendar::parse(&berlin).unwrap();
    assert_eq!(
        (e.start.as_str(), e.tzid.as_str()),
        ("20261120T093000", "Europe/Berlin")
    );
    // a Windows zone name from the list is understood; one that is not becomes floating (and is labelled), never a wrong zone
    let windows = berlin.replace("Europe/Berlin", "\"W. Europe Standard Time\"");
    let e = calendar::parse(&windows).unwrap();
    assert_eq!(
        (e.start.as_str(), e.tzid.as_str()),
        ("20261120T093000", "Europe/Berlin")
    );
    let unknown = berlin.replace("Europe/Berlin", "\"Martian Standard Time\"");
    let e = calendar::parse(&unknown).unwrap();
    assert_eq!(
        (e.start.as_str(), e.tzid.as_str(), e.zone_name.as_str()),
        ("20261120T093000", "", "Martian Standard Time")
    );
    // a zone name that tries something is dropped as well
    for evil in [
        "../../etc",
        "a;b",
        "x\\\\y",
        "Europe/Berlin:evil",
        "Mars/Olympus",
        "europe/berlin",
        "Europe/",
        "Europe//Berlin",
        "Europe/-x",
        "Z".repeat(80).as_str(),
    ] {
        let e = calendar::parse(&berlin.replace("Europe/Berlin", evil));
        assert!(e.is_none_or(|e| e.tzid.is_empty()), "{evil}");
    }
    // names from the list are kept
    for good in [
        "UTC",
        "America/Argentina/Buenos_Aires",
        "Etc/GMT+5",
        "Asia/Kolkata",
    ] {
        let e = calendar::parse(&berlin.replace("Europe/Berlin", good)).unwrap();
        assert_eq!(e.tzid, good);
    }
    let floating = ics(&[
        "BEGIN:VCALENDAR",
        "BEGIN:VEVENT",
        "SUMMARY:x",
        "DTSTART:20261120T093000",
        "END:VEVENT",
        "END:VCALENDAR",
    ]);
    let e = calendar::parse(&floating).unwrap();
    assert_eq!(
        (e.start.as_str(), e.tzid.as_str(), e.all_day),
        ("20261120T093000", "", false)
    );
    let day = ics(&[
        "BEGIN:VCALENDAR",
        "BEGIN:VEVENT",
        "SUMMARY:Holiday",
        "DTSTART;VALUE=DATE:20261224",
        "DTEND;VALUE=DATE:20261227",
        "END:VEVENT",
        "END:VCALENDAR",
    ]);
    let e = calendar::parse(&day).unwrap();
    assert_eq!(
        (e.start.as_str(), e.end.as_str(), e.all_day),
        ("20261224", "20261227", true)
    );
}

#[test]
fn folded_lines_and_escapes_are_undone() {
    let text = ics(&[
        "BEGIN:VCALENDAR",
        "BEGIN:VEVENT",
        "SUMMARY:Team\\, weekly\\nsync \\\\ ok\\; yes",
        "DTSTART:20261120T093000Z",
        "LOCATION:The very long meeting room name that the sending program",
        "  folded onto a second line",
        "\tand a third",
        "END:VEVENT",
        "END:VCALENDAR",
    ]);
    let e = calendar::parse(&text).unwrap();
    assert_eq!(e.summary, "Team, weekly sync \\ ok; yes");
    // unfolding removes exactly one space or tab at the start of a continuation line, and nothing else
    assert_eq!(
        e.location,
        "The very long meeting room name that the sending program folded onto a second lineand a third"
    );
}

#[test]
fn only_the_first_event_counts_and_alarms_are_skipped() {
    let text = ics(&[
        "BEGIN:VCALENDAR",
        "BEGIN:VEVENT",
        "BEGIN:VALARM",
        "ACTION:DISPLAY",
        "SUMMARY:alarm text",
        "DESCRIPTION:x",
        "END:VALARM",
        "SUMMARY:The real one",
        "DTSTART:20261120T093000Z",
        "END:VEVENT",
        "BEGIN:VEVENT",
        "SUMMARY:A second event",
        "DTSTART:20270101T090000Z",
        "END:VEVENT",
        "END:VCALENDAR",
    ]);
    let e = calendar::parse(&text).unwrap();
    assert_eq!(
        (e.summary.as_str(), e.start.as_str()),
        ("The real one", "20261120T093000Z")
    );
}

#[test]
fn a_cancelled_event_is_marked_and_a_reply_is_ignored() {
    assert!(
        calendar::parse(&invite(&[]).replace("METHOD:REQUEST", "METHOD:CANCEL"))
            .unwrap()
            .cancelled
    );
    assert!(
        calendar::parse(&invite(&["STATUS:CANCELLED"]))
            .unwrap()
            .cancelled
    );
    assert!(
        !calendar::parse(&invite(&["STATUS:CONFIRMED"]))
            .unwrap()
            .cancelled
    );
    assert!(calendar::parse(&invite(&[]).replace("METHOD:REQUEST", "METHOD:REPLY")).is_none());
    assert!(calendar::parse(&invite(&[]).replace("METHOD:REQUEST", "METHOD:PUBLISH")).is_some());
}

#[test]
fn nonsense_in_an_invitation_never_becomes_an_event() {
    for bad_start in [
        "20261301T090000Z",
        "20260230",
        "19991231T090000Z",
        "21010101",
        "2026-11-20",
        "20261120T250000Z",
        "20261120T096000Z",
        "20261120 093000",
        "",
        "20261120T0930",
        "2026112",
    ] {
        let t = invite(&[]).replace("DTSTART:20261120T093000Z", &format!("DTSTART:{bad_start}"));
        assert!(
            calendar::parse(&t).is_none(),
            "accepted a start of {bad_start:?}"
        );
    }
    assert!(calendar::parse("").is_none());
    assert!(calendar::parse("BEGIN:VCALENDAR\r\nEND:VCALENDAR\r\n").is_none());
    assert!(
        calendar::parse("BEGIN:VEVENT\r\nSUMMARY:x\r\nDTSTART:20261120T093000Z\r\nEND:VEVENT\r\n")
            .is_none(),
        "an event outside a calendar"
    );
    // an end before the start is dropped, an end of another kind is dropped
    let e =
        calendar::parse(&invite(&[]).replace("DTEND:20261120T103000Z", "DTEND:20261120T083000Z"))
            .unwrap();
    assert_eq!(e.end, "");
    let e = calendar::parse(
        &invite(&[]).replace("DTEND:20261120T103000Z", "DTEND;VALUE=DATE:20261121"),
    )
    .unwrap();
    assert_eq!(e.end, "");
}

#[test]
fn text_is_cleaned_and_limited() {
    let long = "x".repeat(500);
    let t = invite(&[])
        .replace("SUMMARY:Quarterly planning", &format!("SUMMARY:{long}"))
        .replace("LOCATION:Room 4", &format!("LOCATION:{long}"));
    let e = calendar::parse(&t).unwrap();
    assert_eq!(
        (e.summary.chars().count(), e.location.chars().count()),
        (200, 200)
    );
    let t = invite(&[]).replace(
        "SUMMARY:Quarterly planning",
        "SUMMARY:  \u{7}bell\u{1b}[31m red \u{0}  ",
    );
    let e = calendar::parse(&t).unwrap();
    assert!(!e.summary.chars().any(char::is_control), "{:?}", e.summary);
    let t = invite(&[]).replace("SUMMARY:Quarterly planning", "SUMMARY:   ");
    assert_eq!(calendar::parse(&t).unwrap().summary, "(no title)");
    // far too big: not even looked at
    let huge = invite(&[&format!("DESCRIPTION:{}", "d".repeat(70_000))]);
    assert!(calendar::parse(&huge).is_none());
}

#[test]
fn garbage_never_panics_the_parser() {
    let mut seed = 12345u64;
    let mut next = || {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (seed >> 33) as usize
    };
    let alphabet: Vec<char> = "BEGIN:VCALENDAREVNTDSUMRYPOHL;=\r\n\t \\,\"0123456789TZ-/é\u{0}"
        .chars()
        .collect();
    for _ in 0..3000 {
        let len = next() % 400;
        let s: String = (0..len)
            .map(|_| alphabet[next() % alphabet.len()])
            .collect();
        let _ = calendar::parse(&s);
        let _ = calendar::parse(&format!(
            "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\n{s}\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n"
        ));
    }
}

// ───────────────────────── the file the app writes ─────────────────────────
const STAMP: &str = "2026-11-13 12:00";

fn stamp() -> NaiveDateTime {
    dt(STAMP)
}

#[test]
fn the_file_is_built_by_the_app_and_reads_back_as_the_same_event() {
    for e in [
        Event {
            end: "20261120T103000Z".into(),
            location: "Room 4, 2nd floor; east".into(),
            ..ev("Planning, Q4; \\ final", "20261120T093000Z")
        },
        Event {
            tzid: "Europe/Berlin".into(),
            end: "20261120T100000".into(),
            ..ev("Call", "20261120T093000")
        },
        Event {
            end: "20261227".into(),
            ..ev("Holiday", "20261224")
        },
        ev("No end", "20261120T093000"),
    ] {
        let text = calendar::to_ics(&e, stamp(), "msg-1");
        assert!(text.starts_with("BEGIN:VCALENDAR\r\n") && text.ends_with("END:VCALENDAR\r\n"));
        assert!(
            text.contains("\r\nDTSTAMP:20261113T120000Z\r\n")
                && text.contains("\r\nVERSION:2.0\r\n")
        );
        assert!(
            !text.replace("\r\n", "").contains('\n'),
            "only CRLF line ends"
        );
        let back = calendar::parse(&text).unwrap_or_else(|| panic!("cannot read back {text}"));
        let mut want = e.clone();
        if want.all_day && want.end.is_empty() {
            want.end = "20261121".into(); // a one-day event is written with the day after as its end
        }
        assert_eq!(back, want);
    }
}

#[test]
fn a_hostile_summary_or_place_cannot_add_properties_to_the_file() {
    let evil = "Lunch\r\nATTENDEE:mailto:victim@x.example\r\nBEGIN:VALARM\r\nACTION:EXEC\r\nEND:VALARM\r\nX:";
    let e = Event {
        location: "Here\r\nORGANIZER:mailto:boss@x.example".into(),
        ..ev(evil, "20261120T093000Z")
    };
    // the parser has already removed line breaks; also feed the generator the raw values directly
    let text = calendar::to_ics(&e, stamp(), "m");
    let names: Vec<String> = text
        .replace("\r\n ", "")
        .lines()
        .map(|l| l.split([':', ';']).next().unwrap_or("").to_string())
        .collect();
    let allowed = [
        "BEGIN", "END", "VERSION", "PRODID", "CALSCALE", "METHOD", "UID", "DTSTAMP", "DTSTART",
        "DTEND", "SUMMARY", "LOCATION",
    ];
    for n in &names {
        assert!(
            allowed.contains(&n.as_str()),
            "a property that was not ours got in: {n:?} in {text}"
        );
    }
    assert_eq!(names.iter().filter(|n| *n == "BEGIN").count(), 2);
    assert_eq!(names.iter().filter(|n| *n == "END").count(), 2);
}

#[test]
fn long_lines_are_folded_at_75_octets_without_splitting_a_character() {
    let e = ev(&"é".repeat(150), "20261120T093000Z");
    let text = calendar::to_ics(&e, stamp(), "m");
    for line in text.split("\r\n") {
        assert!(line.len() <= 75, "{} octets: {line}", line.len());
    }
    assert_eq!(calendar::parse(&text).unwrap().summary, "é".repeat(150));
}

#[test]
fn the_unique_id_is_stable_for_the_same_message_and_differs_between_events() {
    let a = calendar::to_ics(&ev("A", "20261120T093000Z"), stamp(), "msg-1");
    let b = calendar::to_ics(
        &ev("A", "20261120T093000Z"),
        dt("2027-01-01 00:00"),
        "msg-1",
    );
    let c = calendar::to_ics(&ev("A", "20261120T093000Z"), stamp(), "msg-2");
    let uid = |t: &str| {
        t.lines()
            .find(|l| l.starts_with("UID:"))
            .unwrap()
            .to_string()
    };
    assert_eq!(
        uid(&a),
        uid(&b),
        "saving again must not create a second event in a calendar"
    );
    assert_ne!(uid(&a), uid(&c));
    assert!(uid(&a).ends_with("@inbox-agent"));
}

#[test]
fn file_names_are_plain_ascii_whatever_the_summary_says() {
    let long = "x".repeat(300);
    for summary in [
        "Réunion: Q4/plan?*<>|\"",
        "../../../etc/passwd",
        "CON",
        "",
        "日本語の会議",
        long.as_str(),
    ] {
        let n = calendar::file_name(&ev(summary, "20261120T093000Z"));
        assert!(n.ends_with(".ics") && n.len() <= 64, "{n}");
        assert!(
            n.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.'),
            "{n}"
        );
        assert!(!n.starts_with('.') && !n.contains(".."), "{n}");
    }
    assert_eq!(
        calendar::file_name(&ev("Quarterly Planning!", "20261120T093000Z")),
        "invite-quarterly-planning-20261120.ics"
    );
    assert_eq!(
        calendar::file_name(&ev("", "20261120T093000Z")),
        "invite-event-20261120.ics"
    );
}

#[test]
fn saving_writes_a_new_file_each_time_and_never_overwrites_or_leaves_the_folder() {
    let dir = std::env::temp_dir().join(format!("ia-p5-ics-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let e = ev("../../outside", "20261120T093000Z");
    let a = calendar::save_to(&dir, &e, stamp(), "m").unwrap();
    let b = calendar::save_to(&dir, &e, stamp(), "m").unwrap();
    assert_ne!(a, b, "a second save gets a new name");
    for p in [&a, &b] {
        assert_eq!(
            p.parent().unwrap(),
            dir.as_path(),
            "the file stays in the folder"
        );
        assert_eq!(
            std::fs::read_to_string(p).unwrap(),
            calendar::to_ics(&e, stamp(), "m")
        );
    }
    // an existing file of the same name is never touched
    let name = calendar::file_name(&e);
    std::fs::write(dir.join(&name), "precious").unwrap();
    let c = calendar::save_to(&dir, &e, stamp(), "m").unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.join(&name)).unwrap(),
        "precious"
    );
    assert_ne!(c.file_name().unwrap().to_string_lossy(), name);
    assert!(calendar::save_to(&dir.join("missing"), &e, stamp(), "m").is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_downloads_folder_is_found_without_inventing_one() {
    let dir = std::env::temp_dir().join(format!("ia-p5-home-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("Downloads")).unwrap();
    assert_eq!(
        calendar::downloads_dir_in(Some(&dir)),
        Some(dir.join("Downloads"))
    );
    std::fs::remove_dir_all(dir.join("Downloads")).unwrap();
    assert_eq!(
        calendar::downloads_dir_in(Some(&dir)),
        Some(dir.clone()),
        "no Downloads folder: the home folder"
    );
    assert_eq!(calendar::downloads_dir_in(Some(&dir.join("nowhere"))), None);
    assert_eq!(calendar::downloads_dir_in(None), None);
    let _ = std::fs::remove_dir_all(&dir);
}

// ───────────────────────── from the mail to the task ─────────────────────────
fn mail_with_calendar(
    uid: u32,
    ctype: &str,
    extra_headers: &str,
    calendar_text: &str,
) -> engine::mail::Email {
    let raw = format!(
        "From: Boss <boss@acme.example>\r\nSubject: Invitation: planning\r\nMessage-ID: <c{uid}@x>\r\nMIME-Version: 1.0\r\nContent-Type: multipart/mixed; boundary=\"B\"\r\n\r\n\
--B\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nPlease join.\r\n\
--B\r\nContent-Type: {ctype}\r\n{extra_headers}\r\n{calendar_text}\r\n--B--\r\n"
    );
    engine::mail::parse(uid, raw.as_bytes()).unwrap()
}

fn add_mail(st: &Store, e: &engine::mail::Email) {
    let s = Signals::default();
    st.add(e, &RulesTriager.triage(e, &s).unwrap(), &s).unwrap();
}

#[test]
fn an_invitation_in_a_message_becomes_an_event_on_its_task() {
    let st = Store::memory().unwrap();
    let text = invite(&[]).replace("20261120", "20991120");
    for (uid, ctype, extra) in [
        (1u32, "text/calendar; method=REQUEST; charset=utf-8", ""),
        (
            2,
            "application/ics; name=\"invite.ics\"",
            "Content-Disposition: attachment; filename=\"invite.ics\"\r\n",
        ),
        (
            3,
            "application/octet-stream; name=\"meeting.ICS\"",
            "Content-Disposition: attachment; filename=\"meeting.ICS\"\r\n",
        ),
    ] {
        let e = mail_with_calendar(uid, ctype, extra, &text);
        assert_eq!(
            e.event.as_ref().map(|v| v.summary.as_str()),
            Some("Quarterly planning"),
            "{ctype}"
        );
        add_mail(&st, &e);
        let t = st.get(uid).unwrap();
        let ev = t.event.expect("the task carries the event");
        assert_eq!(ev.start, "20991120T093000Z");
        assert_eq!(t.due, "2099-11-20", "the event's day becomes the due date");
    }
    // a message without one has none
    let plain = engine::mail::parse(
        9,
        b"From: a@b.com\r\nSubject: hi\r\nMessage-ID: <p@x>\r\n\r\nhello\r\n",
    )
    .unwrap();
    assert!(plain.event.is_none());
}

#[test]
fn the_events_day_never_replaces_a_date_the_message_already_gave_and_is_not_set_for_the_past_or_a_cancellation(
) {
    let st = Store::memory().unwrap();
    // a past event and a cancelled one set nothing
    let past = mail_with_calendar(
        1,
        "text/calendar",
        "",
        &invite(&[]).replace("20261120", "20100101"),
    );
    add_mail(&st, &past);
    assert_eq!(st.get(1).unwrap().due, "");
    assert!(st.get(1).unwrap().event.is_some(), "but it is still shown");
    let cancelled = mail_with_calendar(
        2,
        "text/calendar",
        "",
        &invite(&[])
            .replace("20261120", "20991120")
            .replace("REQUEST", "CANCEL"),
    );
    add_mail(&st, &cancelled);
    assert_eq!(st.get(2).unwrap().due, "");
    assert!(st.get(2).unwrap().event.unwrap().cancelled);
    // a date found in the text stays
    let raw = "From: Boss <boss@acme.example>\r\nSubject: Please send it, due 2099-03-05\r\nMessage-ID: <d@x>\r\nMIME-Version: 1.0\r\nContent-Type: multipart/mixed; boundary=\"B\"\r\n\r\n--B\r\nContent-Type: text/plain\r\n\r\nthanks\r\n--B\r\nContent-Type: text/calendar\r\n\r\n".to_string()
        + &invite(&[]).replace("20261120", "20991120")
        + "\r\n--B--\r\n";
    let e = engine::mail::parse(3, raw.as_bytes()).unwrap();
    add_mail(&st, &e);
    assert_eq!(st.get(3).unwrap().due, "2099-03-05");
}

#[test]
fn an_oversized_or_unreadable_calendar_part_is_ignored_and_the_message_still_loads() {
    let big = invite(&[&format!("DESCRIPTION:{}", "d".repeat(70_000))]);
    let e = mail_with_calendar(1, "text/calendar", "", &big);
    assert!(e.event.is_none());
    assert_eq!(e.subject, "Invitation: planning");
    let junk = mail_with_calendar(2, "text/calendar", "", "this is not a calendar at all");
    assert!(junk.event.is_none());
    let reply = mail_with_calendar(
        3,
        "text/calendar; method=REPLY",
        "",
        &invite(&[]).replace("REQUEST", "REPLY"),
    );
    assert!(reply.event.is_none());
}

#[test]
fn saving_an_event_is_a_desktop_command_and_refuses_what_it_should() {
    use serde_json::json;
    let dir = std::env::temp_dir().join(format!("ia-p5-evcmd-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let c = engine::core::Core::new(dir.clone(), std::sync::Arc::new(|_, _| {}));
    let st = c.store().unwrap();
    add_mail(
        &st,
        &mail_with_calendar(
            1,
            "text/calendar",
            "",
            &invite(&[]).replace("20261120", "20991120"),
        ),
    );
    add_mail(
        &st,
        &mail_with_calendar(
            2,
            "text/calendar",
            "",
            &invite(&[]).replace("REQUEST", "CANCEL"),
        ),
    );
    let plain = engine::mail::parse(
        3,
        b"From: a@b.com\r\nSubject: hi\r\nMessage-ID: <p@x>\r\n\r\nhello\r\n",
    )
    .unwrap();
    add_mail(&st, &plain);
    let e = c
        .rpc_public("save_event", &json!({"uid": 1}))
        .unwrap_err()
        .to_string();
    assert!(e.contains("not available from the hub"), "{e}");
    assert!(!engine::commands::PUBLIC.contains(&"save_event"));
    let e = c
        .rpc("save_event", &json!({"uid": 2}))
        .unwrap_err()
        .to_string();
    assert!(e.contains("cancelled"), "{e}");
    let e = c
        .rpc("save_event", &json!({"uid": 3}))
        .unwrap_err()
        .to_string();
    assert!(e.contains("no event"), "{e}");
    let e = c
        .rpc("save_event", &json!({"uid": 99}))
        .unwrap_err()
        .to_string();
    assert!(e.contains("no such task"), "{e}");
    // the writing itself goes to the folder it is given
    let out = std::env::temp_dir().join(format!("ia-p5-evout-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out).unwrap();
    let path = engine::commands::save_event_in(&c, 1, &out).unwrap();
    assert!(path.starts_with(&out) && path.extension().is_some_and(|x| x == "ics"));
    assert!(std::fs::read_to_string(&path)
        .unwrap()
        .contains("SUMMARY:Quarterly planning"));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&out);
}

#[test]
fn control_characters_handed_to_the_file_builder_never_reach_the_file() {
    // the parser already removes them; the builder must not rely on that
    let e = Event {
        location: "door\u{1b}[2J\u{7}\u{0}".into(),
        ..ev(
            "bell\u{7} esc\u{1b}[31m nul\u{0} del\u{7f}",
            "20261120T093000Z",
        )
    };
    let text = calendar::to_ics(&e, stamp(), "m");
    let stripped = text.replace("\r\n", "");
    assert!(
        !stripped.chars().any(char::is_control),
        "a control character got into the file: {text:?}"
    );
    assert!(text.contains("SUMMARY:bell esc[31m nul del\r\n"), "{text}");
}

// ───────────────────────── review fixes (council and drift passes on Run 2) ─────────────────────────
#[test]
fn finishing_a_due_only_repeat_before_its_due_day_moves_it_to_the_next_period() {
    let st = Store::memory().unwrap();
    let a = st.add_local_task("rent", 3, "2026-11-20").unwrap().unwrap();
    st.set_reminder(a, "", "weekly", dt("2026-11-01 08:00"))
        .unwrap();
    // paid a week early: this week's one is done, the next is due a week after the date it had
    st.set_done_at(a, true, dt("2026-11-13 10:00")).unwrap();
    assert_eq!(st.get(a).unwrap().due, "2026-11-27");
    // and again early
    st.set_done_at(a, true, dt("2026-11-14 10:00")).unwrap();
    assert_eq!(st.get(a).unwrap().due, "2026-12-04");
    // on the day, or after it, as before
    st.set_done_at(a, true, dt("2026-12-04 09:00")).unwrap();
    assert_eq!(st.get(a).unwrap().due, "2026-12-11");
    st.set_done_at(a, true, dt("2027-01-20 09:00")).unwrap();
    assert_eq!(
        st.get(a).unwrap().due,
        "2027-01-22",
        "weeks missed are skipped"
    );
}

#[test]
fn changing_only_the_repeat_of_a_reminder_that_has_fired_is_accepted_and_stays_announced() {
    let st = Store::memory().unwrap();
    let a = own(&st, "bins");
    st.set_reminder(a, "2026-11-14 09:00", "weekly", dt("2026-11-13 08:00"))
        .unwrap();
    assert_eq!(st.due_reminders(dt("2026-11-14 09:00")).unwrap().len(), 1);
    let later = dt("2026-11-14 10:00");
    st.set_reminder(a, "2026-11-14 09:00", "daily", later)
        .unwrap();
    assert_eq!(st.get(a).unwrap().repeat, "daily");
    assert!(
        st.due_reminders(later).unwrap().is_empty(),
        "the same time is not announced a second time"
    );
    // adding a repeat to a reminder that is not repeating and has fired works the same way
    let b = own(&st, "once");
    st.set_reminder(b, "2026-11-14 09:00", "", dt("2026-11-13 08:00"))
        .unwrap();
    st.due_reminders(dt("2026-11-14 09:00")).unwrap();
    st.set_reminder(b, "2026-11-14 09:00", "weekly", later)
        .unwrap();
    assert_eq!(st.get(b).unwrap().repeat, "weekly");
    // a different time in the past is still refused
    assert!(st
        .set_reminder(b, "2026-11-14 08:00", "weekly", later)
        .is_err());
    // finishing it then moves it on from where it stood
    st.set_done_at(b, true, later).unwrap();
    assert_eq!(st.get(b).unwrap().remind, "2026-11-21 09:00");
}

#[test]
fn tasks_the_app_made_for_you_take_no_reminders() {
    let st = Store::memory().unwrap();
    let m = engine::signals::SentMsg {
        message_id: "<f@x>".into(),
        to: "d@s.example".into(),
        recipients: vec!["d@s.example".into()],
        subject: "Quote".into(),
        sent_at: 1_800_000_000.0,
    };
    let f = st.add_followup(&m, 1_800_000_000.0).unwrap().unwrap();
    let e = st
        .set_reminder(f, "2026-11-14 09:00", "", dt("2026-11-13 08:00"))
        .unwrap_err()
        .to_string();
    assert!(e.contains("added yourself"), "{e}");
}

#[test]
fn pruned_edited_and_replaced_words_do_not_stay_in_the_database_file() {
    let dir = std::env::temp_dir().join(format!("ia-p5-remanence-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("agent.db");
    {
        let st = Store::open(&path).unwrap();
        // pruned after the retention period
        mail(&st, 1, "x@y.example", "s", "zebrapruned unique words here");
        st.set_done(1, true).unwrap();
        st.backdate_done(1, 1).unwrap();
        // a note edited to remove something private
        mail(&st, 2, "x@y.example", "s", "keeps");
        st.set_note(2, "notesecretzebra").unwrap();
        st.set_note(2, "harmless").unwrap();
        // a row replaced with new text
        mail(&st, 3, "x@y.example", "s", "zebrareplaced old text");
        mail(&st, 3, "x@y.example", "s", "new text");
        // a renumbered mailbox
        mail(&st, 4, "x@y.example", "s", "zebrarenumbered text");
        st.prune(90, 30).unwrap();
        st.reset_mailbox_data().unwrap();
        mail(&st, 5, "x@y.example", "s", "zebrachanged mailbox text");
        st.reset_for_new_account().unwrap();
    }
    let bytes: Vec<u8> = ["agent.db", "agent.db-wal"]
        .iter()
        .flat_map(|f| std::fs::read(dir.join(f)).unwrap_or_default())
        .collect();
    for needle in [
        "zebrapruned",
        "notesecretzebra",
        "zebrareplaced",
        "zebrarenumbered",
        "zebrachanged",
    ] {
        assert!(
            !bytes.windows(needle.len()).any(|w| w == needle.as_bytes()),
            "{needle} is still in the database file"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

// ───────────────────────── calendar: what the file used to lose or get wrong ─────────────────────────
fn one(lines: &[&str]) -> Event {
    let mut v = vec!["BEGIN:VCALENDAR", "BEGIN:VEVENT", "SUMMARY:x"];
    v.extend_from_slice(lines);
    v.extend_from_slice(&["END:VEVENT", "END:VCALENDAR"]);
    calendar::parse(&ics(&v)).expect("an event")
}

#[test]
fn the_day_of_a_utc_invitation_is_the_day_where_you_are() {
    let late = one(&["DTSTART:20261114T233000Z"]);
    assert_eq!(
        calendar::day_at_offset(&late, 0).as_deref(),
        Some("2026-11-14")
    );
    assert_eq!(
        calendar::day_at_offset(&late, 19_800).as_deref(),
        Some("2026-11-15"),
        "India"
    );
    assert_eq!(
        calendar::day_at_offset(&late, 3_600).as_deref(),
        Some("2026-11-15"),
        "Berlin"
    );
    assert_eq!(
        calendar::day_at_offset(&late, -28_800).as_deref(),
        Some("2026-11-14"),
        "California"
    );
    let early = one(&["DTSTART:20261115T020000Z"]);
    assert_eq!(
        calendar::day_at_offset(&early, -28_800).as_deref(),
        Some("2026-11-14"),
        "still the 14th in California"
    );
    // all-day and floating times are the day written, wherever you are
    for text in [
        "DTSTART;VALUE=DATE:20261114",
        "DTSTART:20261114T233000",
        "DTSTART;TZID=Asia/Kolkata:20261114T233000",
    ] {
        let e = one(&[text]);
        assert_eq!(
            calendar::day_at_offset(&e, 36_000).as_deref(),
            Some("2026-11-14"),
            "{text}"
        );
        assert_eq!(
            calendar::day_at_offset(&e, -36_000).as_deref(),
            Some("2026-11-14"),
            "{text}"
        );
    }
}

#[test]
fn the_zone_names_outlook_writes_are_understood_and_unknown_ones_are_labelled() {
    let make = |tz: &str| one(&[&format!("DTSTART;TZID=\"{tz}\":20261120T100000")]);
    for (win, iana) in [
        ("Pacific Standard Time", "America/Los_Angeles"),
        ("Eastern Standard Time", "America/New_York"),
        ("W. Europe Standard Time", "Europe/Berlin"),
        ("GMT Standard Time", "Europe/London"),
        ("India Standard Time", "Asia/Kolkata"),
        ("AUS Eastern Standard Time", "Australia/Sydney"),
        ("Tokyo Standard Time", "Asia/Tokyo"),
    ] {
        let e = make(win);
        assert_eq!((e.tzid.as_str(), e.zone_name.as_str()), (iana, ""), "{win}");
    }
    let unknown = make("Martian Standard Time");
    assert_eq!(
        (unknown.tzid.as_str(), unknown.zone_name.as_str()),
        ("", "Martian Standard Time")
    );
    // what is shown is cleaned like any other text
    let odd = make("Bad\u{202e}Zone\u{7}");
    assert!(
        !odd.zone_name
            .chars()
            .any(|c| c.is_control() || c == '\u{202e}'),
        "{:?}",
        odd.zone_name
    );
    let long = make(&"Z".repeat(300));
    assert!(long.zone_name.chars().count() <= 64);
    // an event in a zone we kept needs no label
    assert_eq!(
        one(&["DTSTART;TZID=Europe/Berlin:20261120T100000"]).zone_name,
        ""
    );
    // UTC and all-day events never have one
    assert_eq!(one(&["DTSTART:20261120T100000Z"]).zone_name, "");
}

#[test]
fn a_duration_gives_the_end_when_there_is_no_end() {
    let end = |lines: &[&str]| one(lines).end;
    assert_eq!(
        end(&["DTSTART:20261120T100000Z", "DURATION:PT2H"]),
        "20261120T120000Z"
    );
    assert_eq!(
        end(&["DTSTART:20261120T100000", "DURATION:PT1H30M"]),
        "20261120T113000"
    );
    assert_eq!(
        end(&[
            "DTSTART;TZID=Europe/Berlin:20261120T230000",
            "DURATION:PT2H"
        ]),
        "20261121T010000"
    );
    assert_eq!(
        end(&["DTSTART;VALUE=DATE:20261120", "DURATION:P2D"]),
        "20261122"
    );
    assert_eq!(
        end(&["DTSTART;VALUE=DATE:20261120", "DURATION:P1W"]),
        "20261127"
    );
    assert_eq!(
        end(&["DTSTART:20261120T100000Z", "DURATION:P1DT2H"]),
        "20261121T120000Z"
    );
    assert_eq!(
        end(&["DTSTART:20261120T100000Z", "DURATION:PT45M"]),
        "20261120T104500Z"
    );
    // an explicit end wins
    assert_eq!(
        end(&[
            "DTSTART:20261120T100000Z",
            "DTEND:20261120T110000Z",
            "DURATION:PT5H"
        ]),
        "20261120T110000Z"
    );
    // nonsense, nothing, a negative one, an all-day event with hours, absurdly long: no end
    for bad in [
        "PT",
        "P",
        "2H",
        "-PT1H",
        "PT0S",
        "PT2X",
        "PT1.5H",
        "P1Y",
        "P400D",
        "PT99999999999H",
    ] {
        assert_eq!(
            end(&["DTSTART:20261120T100000Z", &format!("DURATION:{bad}")]),
            "",
            "{bad}"
        );
    }
    assert_eq!(end(&["DTSTART;VALUE=DATE:20261120", "DURATION:PT2H"]), "");
}

#[test]
fn an_end_written_in_another_zone_is_dropped_rather_than_read_in_the_starts_zone() {
    let e = one(&[
        "DTSTART;TZID=Europe/London:20261120T100000",
        "DTEND;TZID=America/New_York:20261120T130000",
    ]);
    assert_eq!(e.end, "", "3 hours would have been 8");
    let same = one(&[
        "DTSTART;TZID=Europe/London:20261120T100000",
        "DTEND;TZID=Europe/London:20261120T130000",
    ]);
    assert_eq!(same.end, "20261120T130000");
    let floating_end = one(&[
        "DTSTART;TZID=Europe/London:20261120T100000",
        "DTEND:20261120T130000",
    ]);
    assert_eq!(floating_end.end, "");
}

#[test]
fn a_recurring_invitation_is_marked_because_only_its_first_time_is_kept() {
    let e = one(&["DTSTART:20261120T100000Z", "RRULE:FREQ=WEEKLY;COUNT=10"]);
    assert!(e.recurring);
    assert!(!one(&["DTSTART:20261120T100000Z"]).recurring);
    // the file holds the one occurrence and no rule of the sender's
    let text = calendar::to_ics(&e, stamp(), "m");
    assert!(!text.contains("RRULE"), "{text}");
}

#[test]
fn line_and_paragraph_separators_and_bidi_marks_never_reach_the_file_or_the_task() {
    let nasty = "Hello\u{2028}ATTENDEE:mailto:evil@x.example\u{2029}X\u{202e}gnp.exe\u{200b}\u{feff}\u{2066}";
    let text = ics(&[
        "BEGIN:VCALENDAR",
        "BEGIN:VEVENT",
        &format!("SUMMARY:{nasty}"),
        &format!("LOCATION:{nasty}"),
        "DTSTART:20261120T100000Z",
        "END:VEVENT",
        "END:VCALENDAR",
    ]);
    let e = calendar::parse(&text).unwrap();
    let bad = |c: char| {
        c.is_control()
            || matches!(
                c,
                '\u{2028}' | '\u{2029}' | '\u{202e}' | '\u{200b}' | '\u{feff}' | '\u{2066}'
            )
    };
    assert!(
        !e.summary.chars().any(bad) && !e.location.chars().any(bad),
        "{e:?}"
    );
    // and the builder does not rely on the parser
    let raw = Event {
        summary: nasty.into(),
        location: nasty.into(),
        ..ev("x", "20261120T100000Z")
    };
    let out = calendar::to_ics(&raw, stamp(), "m");
    assert!(!out.replace("\r\n", "").chars().any(bad), "{out:?}");
}

#[test]
fn an_event_stored_by_the_previous_build_still_loads() {
    let old = r#"{"summary":"Old","start":"20261120T100000Z","end":"","tzid":"","location":"","all_day":false,"cancelled":false}"#;
    let e: Event = serde_json::from_str(old).unwrap();
    assert_eq!((e.zone_name.as_str(), e.recurring), ("", false));
}

#[test]
fn an_index_made_without_secure_delete_gets_it_when_the_database_is_opened() {
    let dir = std::env::temp_dir().join(format!("ia-p5-sd-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("agent.db");
    drop(Store::open(&path).unwrap());
    let get = |db: &rusqlite::Connection| -> Option<i64> {
        db.query_row(
            "SELECT v FROM tasks_fts_config WHERE k='secure-delete'",
            [],
            |r| r.get(0),
        )
        .ok()
    };
    let db = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(get(&db), Some(1), "set when the index is created");
    // an index from a build that did not set it
    db.execute("DELETE FROM tasks_fts_config WHERE k='secure-delete'", [])
        .unwrap();
    assert_eq!(get(&db), None);
    drop(db);
    drop(Store::open(&path).unwrap());
    assert_eq!(get(&rusqlite::Connection::open(&path).unwrap()), Some(1));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_log_line_for_a_due_date_only_repeat_names_a_day_not_a_time() {
    let st = Store::memory().unwrap();
    let a = st.add_local_task("rent", 3, "2026-12-01").unwrap().unwrap();
    st.set_reminder(a, "", "monthly", dt("2026-11-13 08:00"))
        .unwrap();
    st.set_done_at(a, true, dt("2026-12-01 10:00")).unwrap();
    let log = st
        .recent_log(5)
        .into_iter()
        .map(|l| l.1)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        log.contains("next on 1 Jan") && !log.contains("00:00"),
        "{log}"
    );
    let b = own(&st, "bins");
    st.set_reminder(b, "2026-12-02 07:30", "weekly", dt("2026-11-13 08:00"))
        .unwrap();
    st.set_done_at(b, true, dt("2026-12-02 08:00")).unwrap();
    let log = st
        .recent_log(5)
        .into_iter()
        .map(|l| l.1)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(log.contains("next on 9 Dec 07:30"), "{log}");
}
