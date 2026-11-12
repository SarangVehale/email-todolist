//! Edge cases: odd real-world mail, malformed model output, hostile or corrupt settings, calendar and daylight-saving
//! boundaries, store invariants, and draft building at its limits.
use chrono::{Datelike, Local, TimeZone, Timelike, Weekday};
use engine::ai::{clean_draft, parse_reply};
use engine::drafts::{build_message, template_draft, DraftMessage};
use engine::jobs::{clock_minutes, digest_due, in_quiet_window, snooze_until};
use engine::mail::{self, html_to_text};
use engine::settings::Settings;
use engine::signals::{compute, SentIndex, Signals};
use engine::store::Store;
use engine::triage::{DraftCtx, RulesTriager, Triager};
use mailparse::MailHeaderMap;
use serde_json::json;

fn crlf(raw: &str) -> Vec<u8> {
    raw.replace('\n', "\r\n").into_bytes()
}

fn parse(raw: &str) -> mail::Email {
    mail::parse(1, &crlf(raw)).unwrap()
}

fn add(st: &Store, e: &mail::Email) {
    let s = Signals::default();
    st.add(e, &RulesTriager.triage(e, &s).unwrap(), &s).unwrap();
}

// ───────────────────────── message parsing ─────────────────────────
#[test]
fn html_only_mail_becomes_readable_text() {
    let e = parse("From: a@b.com\nSubject: s\nContent-Type: text/html; charset=utf-8\n\n<html><head><style>p{color:red}</style><title>Secret title</title></head><body><p>Hello&nbsp;<b>Priya</b></p><p>Sign &amp; return by 5pm &#8212; thanks</p><script>alert(1)</script></body></html>");
    assert!(e.body.contains("Hello Priya"), "{:?}", e.body);
    assert!(
        e.body.contains("Sign & return by 5pm — thanks"),
        "{:?}",
        e.body
    );
    for hidden in ["alert", "color:red", "<b>", "Secret title", "&amp;"] {
        assert!(
            !e.body.contains(hidden),
            "{hidden} leaked into {:?}",
            e.body
        );
    }
    assert!(e.excerpt.starts_with("Hello Priya"), "{:?}", e.excerpt);
    assert!(
        e.body.lines().count() >= 2,
        "paragraphs stay on separate lines"
    );
}

#[test]
fn multipart_alternative_prefers_the_plain_part() {
    let e = parse("From: a@b.com\nContent-Type: multipart/alternative; boundary=X\n\n--X\nContent-Type: text/plain\n\nplain wins\n--X\nContent-Type: text/html\n\n<p>html loses</p>\n--X--\n");
    assert_eq!(e.body, "plain wins");
}

#[test]
fn html_conversion_survives_malformed_markup() {
    for bad in [
        "<p>a</p><script>never closed",
        "<p>unclosed <b>tags <i>everywhere",
        "a < b and c > d",
        "<<<>>>",
        "&notanentity; & alone &#xZZ; &#99999999999; &#; &",
        "<style>",
        "<!-- unterminated comment",
        "",
        "<a href=\"x>broken attribute",
    ] {
        let out = html_to_text(bad);
        assert!(
            out.len() <= bad.len() + 8,
            "output grew unreasonably for {bad:?}"
        );
    }
    assert_eq!(html_to_text("a < b and c > d").trim(), "a < b and c > d");
    assert!(html_to_text("<p>a</p><script>never closed").contains('a'));
}

#[test]
fn block_tags_make_line_breaks() {
    let t = html_to_text("one<br/>two<br />three<p>four</p><div>five</div><li>six</li>");
    let lines: Vec<&str> = t.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    assert_eq!(
        lines,
        ["one", "two", "three", "four", "five", "six"],
        "{t:?}"
    );
}

#[test]
fn other_character_sets_and_transfer_encodings_decode() {
    let latin = [b"From: a@b.com\r\nContent-Type: text/plain; charset=ISO-8859-1\r\nContent-Transfer-Encoding: 8bit\r\n\r\ncaf".as_slice(), &[0xE9], b"\r\n"].concat();
    assert!(mail::parse(1, &latin).unwrap().body.contains("café"));
    assert_eq!(parse("From: a@b.com\nContent-Type: text/plain\nContent-Transfer-Encoding: base64\n\nSGVsbG8gd29ybGQ=\n").body, "Hello world");
    assert!(parse("From: a@b.com\nContent-Type: text/plain; charset=utf-8\nContent-Transfer-Encoding: quoted-printable\n\ncaf=C3=A9 au lait\n").body.contains("café au lait"));
}

#[test]
fn encoded_words_in_headers_decode() {
    assert_eq!(
        parse(
            "From: =?UTF-8?B?w4lxdWlwZQ==?= <team@x.com>\nSubject: =?UTF-8?B?w4lxdWlwZQ==?=\n\nx\n"
        )
        .subject,
        "Équipe"
    );
    let e = parse("From: \"Nair, Priya\" <priya@acme.com>\nSubject: s\n\nx\n");
    assert_eq!(
        (e.sender_name.as_str(), e.sender_addr.as_str()),
        ("Nair, Priya", "priya@acme.com")
    );
}

#[test]
fn missing_or_strange_headers_never_panic() {
    let none = parse("Subject: no sender at all\n\nbody\n");
    assert_eq!(none.sender_addr, "");
    assert_eq!(none.reply_to, "", "no usable reply address");
    assert!(!compute(&none, &SentIndex::default()).known);
    assert_eq!(parse("From: a@b.com\n\nbody\n").subject, "(no subject)");
    assert_eq!(
        parse("From: a@b.com\nDate: not a date at all\nSubject: s\n\nx\n").received,
        0.0
    );
    assert_eq!(
        parse("From: undisclosed-recipients:;\nSubject: s\n\nx\n").sender_addr,
        ""
    );
    let _ = mail::parse(1, b"");
    let _ = mail::parse(1, b"\r\n\r\n");
    let _ = mail::parse(
        1,
        b"\x00\x00\x00From: a@b.com\r\nSubject: nul\x00bytes\r\n\r\nbo\x00dy",
    );
    let e = parse("From: Boss@Corp.COM\nSubject: s\n\nx\n");
    assert_eq!(
        e.sender_addr, "boss@corp.com",
        "addresses are lowercased so sender matching is reliable"
    );
}

#[test]
fn attachment_only_mail_has_no_text_but_is_still_a_task() {
    let e = parse("From: a@b.com\nSubject: photo\nContent-Type: multipart/mixed; boundary=X\n\n--X\nContent-Type: image/png\nContent-Transfer-Encoding: base64\n\niVBORw0KGgo=\n--X--\n");
    assert_eq!((e.body.as_str(), e.excerpt.as_str()), ("", ""));
    let st = Store::memory().unwrap();
    add(&st, &e);
    assert_eq!(st.all_tasks().unwrap().len(), 1);
}

#[test]
fn huge_subjects_and_long_threads_are_bounded() {
    let subject = "x".repeat(20_000);
    let e = parse(&format!("From: a@b.com\nSubject: {subject}\n\nhi\n"));
    let st = Store::memory().unwrap();
    add(&st, &e); // stored whole, no panic
    let m = build_message(&DraftMessage {
        from: "me@x.com",
        to: "a@b.com",
        subject: &e.subject,
        in_reply_to: "",
        references: "",
        body: "b",
    })
    .unwrap();
    let p = mailparse::parse_mail(&m).unwrap();
    assert!(
        p.headers
            .get_first_value("Subject")
            .unwrap()
            .chars()
            .count()
            <= 210,
        "the reply subject is capped"
    );
    let refs: String = (1..=15)
        .map(|i| format!("<r{i}@x>"))
        .collect::<Vec<_>>()
        .join(" ");
    let e = parse(&format!(
        "From: a@b.com\nSubject: s\nReferences: {refs}\n\nx\n"
    ));
    assert_eq!(
        e.references.split(' ').count(),
        10,
        "only the 10 most recent thread ids are kept"
    );
    assert!(e.references.ends_with("<r15@x>"));
}

// ───────────────────────── model output ─────────────────────────
#[test]
fn model_replies_in_many_shapes_are_understood_or_refused_safely() {
    let p = |t: &str| parse_reply(t).map(|r| r.priority);
    assert_eq!(p(r#"{"priority": 4}"#).unwrap(), 4);
    assert_eq!(p(r#"{"priority": 4.0}"#).unwrap(), 4);
    assert_eq!(p(r#"{"priority": "4"}"#).unwrap(), 4);
    assert_eq!(p(r#"{"priority": 7}"#).unwrap(), 5, "clamped down");
    assert_eq!(p(r#"{"priority": -3}"#).unwrap(), 1, "clamped up");
    assert!(p(r#"{"priority": "high"}"#).is_err());
    assert!(p(r#"{"category": "work"}"#).is_err(), "no priority at all");
    assert!(p(r#"{"priority": 4, "categ"#).is_err(), "truncated output");
    assert!(
        p("").is_err()
            && p("I cannot help with that.").is_err()
            && p("{").is_err()
            && p("}{").is_err()
    );
    assert_eq!(
        p("Sure! Here you go:\n```json\n{\"priority\": 2}\n```\nHope that helps.").unwrap(),
        2
    );
    assert_eq!(
        p(r#"{"priority": 3} {"priority": 5}"#).unwrap(),
        3,
        "the first object wins"
    );
    assert_eq!(
        p(r#"Use {curly} braces freely. {"priority": 2, "why": "x"}"#).unwrap(),
        2,
        "prose braces before the JSON"
    );
    assert_eq!(
        p(r#"{"priority": 4, "why": "has } and { inside"}"#).unwrap(),
        4
    );
}

#[test]
fn loosely_typed_flags_are_read_sensibly() {
    let t = parse_reply(r#"{"priority":3,"urgent":"yes","needs_reply":1}"#).unwrap();
    assert!(t.urgent && t.needs_reply);
    let t = parse_reply(r#"{"priority":3,"urgent":"false","needs_reply":0}"#).unwrap();
    assert!(!t.urgent && !t.needs_reply);
    let t = parse_reply(r#"{"priority":3,"urgent":null}"#).unwrap();
    assert!(!t.urgent);
}

#[test]
fn drafts_from_models_are_cleaned_at_the_extremes() {
    assert_eq!(clean_draft(""), "");
    assert_eq!(clean_draft("   \n\t  "), "");
    assert_eq!(clean_draft("```"), "");
    assert_eq!(
        clean_draft("Subject: only a subject"),
        "Subject: only a subject"
    );
    assert_eq!(clean_draft("Subject: Re: x\nHello"), "Hello");
    assert_eq!(clean_draft("😀 hello 你好"), "😀 hello 你好");
}

// ───────────────────────── settings ─────────────────────────
#[test]
fn bad_settings_patches_are_errors_not_crashes() {
    let s = Settings::default();
    for bad in [
        json!({"writeback": "yes"}),
        json!({"hub": {"port": 70000}}),
        json!({"hub": {"port": -1}}),
        json!({"alerts": null}),
        json!({"account": {"port": "993"}}),
        json!({"first_run_limit": -5}),
        json!([1, 2, 3]),
        json!("a string"),
    ] {
        let r = s.patched(&bad).and_then(|p| p.validate().map(|_| p));
        assert!(r.is_err(), "accepted {bad}");
    }
    let same = s.patched(&json!({})).unwrap();
    assert_eq!(
        serde_json::to_value(&same).unwrap(),
        serde_json::to_value(&s).unwrap(),
        "an empty patch changes nothing"
    );
    assert!(
        s.patched(&json!({"unknown_field": 1, "alerts": {"also_unknown": 2}}))
            .is_ok(),
        "unknown fields are ignored"
    );
    for n in [1, 1000] {
        assert!(s
            .patched(&json!({"first_run_limit": n}))
            .unwrap()
            .validate()
            .is_ok());
    }
    for n in [0, 1001] {
        assert!(s
            .patched(&json!({"first_run_limit": n}))
            .unwrap()
            .validate()
            .is_err());
    }
}

#[test]
fn corrupt_or_old_settings_files_fall_back_safely() {
    let dir = std::env::temp_dir().join(format!("inbox-edge-settings-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let f = dir.join("settings.json");
    for garbage in [
        "",
        "{",
        "not json",
        "[1,2]",
        "{\"hub\": \"oops\"}",
        "\u{0}\u{0}",
    ] {
        std::fs::write(&f, garbage).unwrap();
        let s = Settings::load(&dir);
        assert!(
            !s.onboarded && !s.writeback && s.account.port == 993,
            "{garbage:?} must load as defaults"
        );
    }
    std::fs::write(
        &f,
        r#"{"onboarded": true, "account": {"host": "imap.x.com"}}"#,
    )
    .unwrap(); // an older, smaller file
    let s = Settings::load(&dir);
    assert!(
        s.onboarded
            && s.account.host == "imap.x.com"
            && s.account.port == 993
            && s.alerts.ntfy_server == "https://ntfy.sh"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// ───────────────────────── dates, times, daylight saving ─────────────────────────
fn at(y: i32, m: u32, d: u32, h: u32, min: u32) -> chrono::DateTime<Local> {
    Local
        .with_ymd_and_hms(y, m, d, h, min, 0)
        .earliest()
        .expect("valid local time")
}

fn when(secs: f64) -> chrono::DateTime<Local> {
    Local.timestamp_opt(secs as i64, 0).unwrap()
}

#[test]
fn snooze_presets_across_month_year_and_leap_boundaries() {
    let tomorrow = |from| when(snooze_until("tomorrow", from).unwrap());
    let t = tomorrow(at(2026, 12, 31, 14, 0));
    assert_eq!((t.year(), t.month(), t.day(), t.hour()), (2027, 1, 1, 9));
    let t = tomorrow(at(2028, 2, 28, 23, 59));
    assert_eq!((t.month(), t.day()), (2, 29), "leap year");
    let t = tomorrow(at(2027, 2, 28, 8, 0));
    assert_eq!((t.month(), t.day()), (3, 1), "non-leap year");
    let t = tomorrow(at(2026, 11, 12, 3, 0));
    assert_eq!(
        (t.day(), t.hour()),
        (13, 9),
        "a 3am snooze for tomorrow means tomorrow, not today"
    );
}

#[test]
fn next_week_always_lands_on_a_monday_in_the_future() {
    for (y, m, d) in [
        (2026, 11, 14),
        (2026, 11, 15),
        (2026, 11, 16),
        (2026, 11, 17),
        (2026, 12, 28),
        (2026, 12, 31),
    ] {
        let from = at(y, m, d, 12, 0);
        let t = when(snooze_until("nextweek", from).unwrap());
        assert_eq!(t.weekday(), Weekday::Mon, "from {y}-{m}-{d}");
        assert!(
            t > from && (t - from).num_days() <= 7,
            "from {y}-{m}-{d} landed {}",
            t
        );
        assert_eq!(t.hour(), 9);
    }
    assert_eq!(
        when(snooze_until("nextweek", at(2026, 11, 16, 12, 0)).unwrap()).day(),
        23,
        "from a Monday means the following Monday"
    );
    assert_eq!(
        when(snooze_until("nextweek", at(2026, 11, 15, 12, 0)).unwrap()).day(),
        16,
        "from a Sunday means tomorrow"
    );
}

#[test]
fn three_hours_crosses_midnight_and_is_exactly_three_hours() {
    let from = at(2026, 11, 12, 23, 30);
    let t = when(snooze_until("3h", from).unwrap());
    assert_eq!((t.day(), t.hour(), t.minute()), (13, 2, 30));
    assert_eq!(
        snooze_until("3h", from).unwrap() - from.timestamp() as f64,
        10_800.0
    );
    for bad in ["", "3H", " 3h", "tomorrow ", "TOMORROW", "1d", "999h"] {
        assert!(snooze_until(bad, from).is_none(), "accepted {bad:?}");
    }
}

#[test]
fn digest_and_quiet_hours_at_the_boundaries() {
    assert!(
        digest_due("00:00", 0, None, "2026-11-12"),
        "a midnight digest is due at 00:00"
    );
    assert!(digest_due("23:59", 1439, None, "2026-11-12"));
    assert!(!digest_due("23:59", 1438, None, "2026-11-12"));
    assert!(
        digest_due("09:00", 9 * 60, Some("2026-11-11"), "2026-11-12"),
        "a new day re-arms it"
    );
    assert!(!digest_due("not a time", 600, None, "2026-11-12"));
    assert!(in_quiet_window("00:00", "23:59", 600) && !in_quiet_window("00:00", "23:59", 1439));
    assert!(
        in_quiet_window("23:00", "00:30", 1439)
            && in_quiet_window("23:00", "00:30", 10)
            && !in_quiet_window("23:00", "00:30", 40)
    );
    assert_eq!(
        (clock_minutes("00:00"), clock_minutes("23:59")),
        (Some(0), Some(1439))
    );
    for bad in ["", "24:00", "12:60", "ab:cd", "12", "-1:00", "12:xx"] {
        assert_eq!(clock_minutes(bad), None, "{bad:?}");
    }
}

// ───────────────────────── store invariants ─────────────────────────
#[test]
fn hostile_and_unicode_text_is_stored_verbatim_and_safely() {
    let st = Store::memory().unwrap();
    let mut e = parse("From: a@b.com\nSubject: s\n\nx\n");
    e.subject = "'); DROP TABLE tasks; --  😀 你好 \u{202e}rtl".into();
    e.sender_name = "Robert'); DROP TABLE tasks;--".into();
    add(&st, &e);
    let t = st.get(1).unwrap();
    assert_eq!(t.subject, e.subject);
    assert_eq!(t.sender, e.sender_name);
    assert_eq!(st.all_tasks().unwrap().len(), 1, "the table is still there");
}

#[test]
fn re_adding_a_message_keeps_its_draft_and_a_no_op_priority_change_writes_nothing() {
    let st = Store::memory().unwrap();
    let e = parse("From: a@b.com\nSubject: s\n\nx?\n");
    add(&st, &e);
    st.set_draft(1, "my draft", "user").unwrap();
    add(&st, &e); // the same uid seen again (for example after a resync)
    assert_eq!(st.all_tasks().unwrap().len(), 1);
    assert_eq!(st.get(1).unwrap().draft.unwrap().body, "my draft");
    let p = st.get(1).unwrap().priority;
    let before = st.pending(100).len();
    assert!(st.set_priority(1, p).unwrap());
    assert_eq!(
        st.pending(100).len(),
        before,
        "setting the same priority queues nothing"
    );
    assert!(
        st.recent_overrides(10).is_empty(),
        "and records no correction"
    );
}

#[test]
fn snooze_edge_cases() {
    let st = Store::memory().unwrap();
    add(&st, &parse("From: a@b.com\nSubject: one\n\nx\n"));
    assert!(
        !st.set_snooze(1, -5.0).unwrap(),
        "a negative time is refused"
    );
    assert!(!st.set_snooze(99, 10.0).unwrap(), "so is a missing task");
    st.set_snooze(1, 1.0).unwrap(); // in the distant past
    assert_eq!(
        st.wake_due(10.0).unwrap().len(),
        1,
        "an already-expired snooze wakes immediately"
    );
    assert!(st.wake_due(10.0).unwrap().is_empty());
}

#[test]
fn sender_preferences_survive_a_mailbox_reset_but_not_a_sign_out() {
    let st = Store::memory().unwrap();
    st.set_pref("boss@corp.com", "important").unwrap();
    assert!(
        !st.set_pref("", "important").unwrap(),
        "an empty address is refused"
    );
    st.reset_mailbox_data().unwrap();
    assert_eq!(st.pref("boss@corp.com").as_deref(), Some("important"));
    st.wipe_all().unwrap();
    assert!(st.pref("boss@corp.com").is_none());
}

#[test]
fn uidvalidity_is_stable_across_repeated_identical_values() {
    let st = Store::memory().unwrap();
    for _ in 0..3 {
        assert!(!st.reconcile_uid_validity(42).unwrap());
    }
    add(&st, &parse("From: a@b.com\nSubject: s\n\nx\n"));
    st.set_last_uid(9).unwrap();
    assert!(!st.reconcile_uid_validity(42).unwrap());
    assert_eq!(st.last_uid(), 9);
    assert!(st.reconcile_uid_validity(u32::MAX).unwrap());
    assert!(
        st.all_tasks().unwrap().is_empty() && st.pending(5).is_empty(),
        "pending actions about old UIDs are dropped too"
    );
}

// ───────────────────────── drafts at their limits ─────────────────────────
fn subject_of(raw: Vec<u8>) -> String {
    mailparse::parse_mail(&raw)
        .unwrap()
        .headers
        .get_first_value("Subject")
        .unwrap()
}

fn body_of(raw: Vec<u8>) -> String {
    mailparse::parse_mail(&raw)
        .unwrap()
        .get_body()
        .unwrap()
        .replace("\r\n", "\n")
}

fn msg<'a>(subject: &'a str, body: &'a str) -> DraftMessage<'a> {
    DraftMessage {
        from: "me@example.com",
        to: "a@b.com",
        subject,
        in_reply_to: "",
        references: "",
        body,
    }
}

#[test]
fn draft_subjects_and_bodies_at_the_extremes_round_trip() {
    let long_unicode: String = "日本語のとても長い件名😀".repeat(30);
    let m = build_message(&msg(&long_unicode, "")).unwrap();
    let text = String::from_utf8(m.clone()).unwrap();
    for l in text.lines() {
        assert!(l.len() <= 78, "header line too long ({}): {l}", l.len());
    }
    let p = mailparse::parse_mail(&m).unwrap();
    let subj = p.headers.get_first_value("Subject").unwrap();
    assert!(
        subj.starts_with("Re: 日本語") && subj.chars().count() <= 204 && !subj.contains('\u{FFFD}'),
        "{subj}"
    );
    assert!(build_message(&msg("", "")).is_ok());
    assert!(subject_of(
        build_message(&msg(
            "", "   
  "
        ))
        .unwrap()
    )
    .starts_with("Re:"));
    assert_eq!(
        subject_of(build_message(&msg("RE: loud", "x")).unwrap()),
        "RE: loud",
        "no double prefix whatever the case"
    );
    assert_eq!(
        subject_of(build_message(&msg("Fwd: x", "x")).unwrap()),
        "Re: Fwd: x"
    );
    let big = "line\n".repeat(1500);
    assert_eq!(body_of(build_message(&msg("s", &big)).unwrap()), big);
}

#[test]
fn template_drafts_handle_odd_names_and_signatures() {
    let name = |n: &str| {
        let mut e = parse("From: a@b.com\nSubject: s\n\nx\n");
        e.sender_name = n.into();
        template_draft(&e, &DraftCtx::default())
    };
    assert!(name("O'Brien-Smith, Pat").starts_with("Hi O'Brien-Smith,"));
    assert!(name("😀 Sam").starts_with("Hi,"), "an emoji is not a name");
    assert!(
        name("priya@acme.com").starts_with("Hi,"),
        "an address is not a name"
    );
    assert!(name("").starts_with("Hi,"));
    assert!(name("   ").starts_with("Hi,"));
    let ctx = DraftCtx {
        signature: "— Sam\nAcme".into(),
        ..Default::default()
    };
    assert!(
        template_draft(&parse("From: a@b.com\nSubject: s\n\nx\n"), &ctx).ends_with("— Sam\nAcme")
    );
}
