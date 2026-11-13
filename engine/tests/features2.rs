//! Tests for due-date extraction, attachments, unsubscribe links, notes, your own tasks and alert categories.
use chrono::NaiveDate;
use engine::dates::{extract_due, valid_iso};
use engine::mail::{self, unsubscribe_link};
use engine::settings::Settings;
use engine::signals::Signals;
use engine::store::{Store, LOCAL_UID_BASE};
use engine::triage::{RulesTriager, Triager};

fn day(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

fn due(text: &str) -> Option<String> {
    // Thursday 12 November 2026
    extract_due(text, day(2026, 11, 12))
}

fn email(raw: &str, uid: u32) -> mail::Email {
    mail::parse(uid, raw.replace('\n', "\r\n").as_bytes()).unwrap()
}

// ── due dates ──
#[test]
fn cued_dates_are_found_in_common_forms() {
    assert_eq!(due("Please pay by 20 Nov").as_deref(), Some("2026-11-20"));
    assert_eq!(
        due("Deadline: November 20th, 2026").as_deref(),
        Some("2026-11-20")
    );
    assert_eq!(due("due 2026-12-01").as_deref(), Some("2026-12-01"));
    assert_eq!(due("reply before tomorrow").as_deref(), Some("2026-11-13"));
    assert_eq!(due("needed by eod").as_deref(), Some("2026-11-12"));
    assert_eq!(due("by Friday please").as_deref(), Some("2026-11-13"));
}

#[test]
fn dates_without_a_cue_are_ignored() {
    assert_eq!(due("Invoice dated 20 Nov 2026"), None);
    assert_eq!(due("see you Friday"), None);
    assert_eq!(due("nothing here"), None);
    assert_eq!(due(""), None);
}

#[test]
fn impossible_dates_are_rejected() {
    assert_eq!(due("due 30 Feb"), None);
    assert_eq!(due("due 2026-02-30"), None);
    assert_eq!(due("due 32 Nov"), None);
    assert_eq!(due("due 2026-13-01"), None);
}

#[test]
fn year_is_inferred_sensibly() {
    // a date a few days ago is overdue, so it stays this year; one long past means next year
    assert_eq!(due("due 5 Nov").as_deref(), Some("2026-11-05"));
    assert_eq!(due("due 5 Jan").as_deref(), Some("2027-01-05"));
}

#[test]
fn weekday_names_resolve_from_the_day_received() {
    // received Thursday: "by Thursday" is the same day, not next week
    assert_eq!(due("by Thursday").as_deref(), Some("2026-11-12"));
    assert_eq!(due("by monday").as_deref(), Some("2026-11-16"));
}

#[test]
fn iso_validator_is_strict() {
    assert!(valid_iso("2026-11-20"));
    assert!(!valid_iso("2026-11-2"));
    assert!(!valid_iso("1999-01-01"));
    assert!(!valid_iso("2026/11/20"));
    assert!(!valid_iso("２０２６-11-20"));
    assert!(!valid_iso("2026-02-29"));
}

#[test]
fn hostile_text_does_not_panic() {
    let junk = "due ".repeat(5000) + "\u{0}\u{202e}by 99999999999 nov 💥 due";
    let _ = due(&junk);
    let _ = due("by \u{1F600}\u{1F600} 20");
}

#[test]
fn rules_triage_fills_the_due_date() {
    let e = email("From: a@b.com\nSubject: Report due 20 Nov\n\nhello\n", 1);
    let t = RulesTriager.triage(&e, &Signals::default()).unwrap();
    assert!(t.due.ends_with("-11-20"), "got {:?}", t.due);
}

// ── attachments ──
const MULTI: &str = "From: a@b.com\nSubject: files\nMIME-Version: 1.0\nContent-Type: multipart/mixed; boundary=\"X\"\n\n--X\nContent-Type: text/plain\n\nSee attached.\n--X\nContent-Type: application/pdf; name=\"report.pdf\"\nContent-Disposition: attachment; filename=\"report.pdf\"\nContent-Transfer-Encoding: base64\n\nJVBERg==\n--X\nContent-Type: image/png\nContent-Disposition: inline; filename=\"logo.png\"\nContent-ID: <l>\nContent-Transfer-Encoding: base64\n\niVBORw==\n--X--\n";

#[test]
fn attachments_are_counted_and_inline_images_are_not() {
    let e = email(MULTI, 1);
    assert_eq!(e.attachment_count, 1);
    assert_eq!(e.attachments, vec!["report.pdf".to_string()]);
}

#[test]
fn attachment_names_are_stripped_of_paths_and_capped() {
    let long = "a".repeat(200);
    let raw = format!("From: a@b.com\nMIME-Version: 1.0\nContent-Type: multipart/mixed; boundary=\"X\"\n\n--X\nContent-Type: text/plain\n\nhi\n--X\nContent-Type: application/octet-stream\nContent-Disposition: attachment; filename=\"..\\..\\evil/{long}.exe\"\n\nAA==\n--X--\n");
    let e = email(&raw, 1);
    assert_eq!(e.attachment_count, 1);
    assert!(e.attachments[0].chars().count() <= 60);
    assert!(!e.attachments[0].contains('/') && !e.attachments[0].contains('\\'));
}

#[test]
fn only_five_names_are_kept_but_all_are_counted() {
    let mut raw = String::from("From: a@b.com\nMIME-Version: 1.0\nContent-Type: multipart/mixed; boundary=\"X\"\n\n--X\nContent-Type: text/plain\n\nhi\n");
    for i in 0..8 {
        raw += &format!("--X\nContent-Type: text/plain\nContent-Disposition: attachment; filename=\"f{i}.txt\"\n\nx\n");
    }
    raw += "--X--\n";
    let e = email(&raw, 1);
    assert_eq!(e.attachment_count, 8);
    assert_eq!(e.attachments.len(), 5);
}

#[test]
fn plain_mail_has_no_attachments() {
    let e = email("From: a@b.com\n\nhi\n", 1);
    assert_eq!(e.attachment_count, 0);
    assert!(e.attachments.is_empty());
}

// ── unsubscribe ──
#[test]
fn unsubscribe_prefers_https_then_mailto_and_rejects_the_rest() {
    assert_eq!(
        unsubscribe_link("<mailto:u@x.com>, <https://x.com/u?id=1>"),
        "https://x.com/u?id=1"
    );
    assert_eq!(unsubscribe_link("<mailto:u@x.com>"), "mailto:u@x.com");
    assert_eq!(unsubscribe_link("<http://x.com/u>"), "");
    assert_eq!(unsubscribe_link("<javascript:alert(1)>"), "");
    assert_eq!(unsubscribe_link("<https://x.com/a b>"), "");
    assert_eq!(unsubscribe_link("<data:text/html,hi>"), "");
    assert_eq!(unsubscribe_link(""), "");
    assert_eq!(unsubscribe_link("<https://>"), "");
    let long = format!("<https://x.com/{}>", "a".repeat(2100));
    assert_eq!(
        unsubscribe_link(&long),
        "",
        "links past 2000 characters are refused"
    );
}

#[test]
fn unsubscribe_links_must_be_real_urls_the_browser_can_parse() {
    // regression: "https://[" passed the old prefix check and made new URL() throw in the UI, blanking the list
    for bad in [
        "<https://[>",
        "<https://[::1>",
        "<https://>",
        "<https:///path>",
        "<https://exa mple.com/x>",
        "<https://user:pw@evil.com/x>",
        "<https://exa<mple.com>",
        "<https://example.com:99999/x>",
        "<https://example.com:abc/x>",
        "<https://.>",
        "<https://-bad.com/x>",
        "<https://exa\\mple.com/x>",
        "<mailto:>",
        "<mailto:no-at-sign>",
        "<mailto:?subject=x>",
    ] {
        assert_eq!(unsubscribe_link(bad), "", "{bad} must be rejected");
    }
    assert_eq!(
        unsubscribe_link("<https://u.example.com:8443/out?id=1#x>"),
        "https://u.example.com:8443/out?id=1#x"
    );
    assert_eq!(
        unsubscribe_link("<mailto:unsub@x.com?subject=unsubscribe>"),
        "mailto:unsub@x.com?subject=unsubscribe"
    );
    // a bad first link must not hide a good second one
    assert_eq!(
        unsubscribe_link("<https://[>, <https://ok.example.com/u>"),
        "https://ok.example.com/u"
    );
}

#[test]
fn unsubscribe_hosts_must_look_public_and_real_long_links_are_kept() {
    for bad in [
        "<https://localhost/u>",
        "<https://127.0.0.1:8080/u>",
        "<https://169.254.169.254/latest>",
        "<https://2130706433/u>",
        "<https://10.0.0.1/u>",
        "<https://intranet/u>",
    ] {
        assert_eq!(unsubscribe_link(bad), "", "{bad} must be rejected");
    }
    assert_eq!(
        unsubscribe_link("<https://mail_tracker.example.com/u>"),
        "https://mail_tracker.example.com/u"
    );
    assert_eq!(
        unsubscribe_link("<https://example.com./u>"),
        "https://example.com./u"
    );
    let long = format!(
        "https://u.example.com/unsubscribe?token={}",
        "a".repeat(900)
    );
    assert_eq!(
        unsubscribe_link(&format!("<{long}>")),
        long,
        "long tracking links from big senders are kept"
    );
    assert_eq!(
        unsubscribe_link(&format!("<https://u.example.com/{}>", "a".repeat(2100))),
        "",
        "but not without limit"
    );
}

#[test]
fn unsubscribe_is_read_from_the_header() {
    let e = email(
        "From: n@x.com\nList-Unsubscribe: <https://x.com/out>\n\nhi\n",
        1,
    );
    assert_eq!(e.unsubscribe, "https://x.com/out");
}

#[test]
fn attachments_and_unsubscribe_survive_storage() {
    let st = Store::memory().unwrap();
    let mut raw = String::from(
        "From: n@x.com\nSubject: s\nMessage-ID: <q@x>\nList-Unsubscribe: <https://x.com/out>\n",
    );
    raw += MULTI.split_once("Subject: files\n").unwrap().1;
    let e = email(&raw, 7);
    let s = Signals::default();
    st.add(&e, &RulesTriager.triage(&e, &s).unwrap(), &s)
        .unwrap();
    let t = st.get(7).unwrap();
    assert_eq!(t.unsubscribe, "https://x.com/out");
    assert_eq!(t.attachment_count, 1);
    assert_eq!(t.attachments, vec!["report.pdf".to_string()]);
}

// ── notes ──
fn stored(st: &Store, uid: u32) {
    let e = email("From: a@b.com\nSubject: s\n\nhi\n", uid);
    let s = Signals::default();
    st.add(&e, &RulesTriager.triage(&e, &s).unwrap(), &s)
        .unwrap();
}

#[test]
fn notes_are_saved_cleaned_and_bounded() {
    let st = Store::memory().unwrap();
    stored(&st, 1);
    assert!(st.set_note(1, "call back\nask about \u{7}price").unwrap());
    assert_eq!(st.get(1).unwrap().note, "call back\nask about price");
    assert!(!st.set_note(1, &"x".repeat(2001)).unwrap());
    assert_eq!(
        st.get(1).unwrap().note,
        "call back\nask about price",
        "an oversized note changes nothing"
    );
    assert!(!st.set_note(99, "ghost").unwrap(), "unknown task");
    assert!(st.set_note(1, "").unwrap());
    assert_eq!(st.get(1).unwrap().note, "");
}

// ── your own tasks ──
#[test]
fn own_tasks_validate_input() {
    let st = Store::memory().unwrap();
    assert!(st.add_local_task("", 3, "").unwrap().is_none());
    assert!(st.add_local_task("   ", 3, "").unwrap().is_none());
    assert!(st
        .add_local_task(&"t".repeat(201), 3, "")
        .unwrap()
        .is_none());
    assert!(st.add_local_task("bell\u{7}", 3, "").unwrap().is_none());
    assert!(st.add_local_task("ok", 0, "").unwrap().is_none());
    assert!(st.add_local_task("ok", 6, "").unwrap().is_none());
    assert!(st.add_local_task("ok", 3, "2026-02-30").unwrap().is_none());
    assert!(st.add_local_task("ok", 3, "tomorrow").unwrap().is_none());
}

#[test]
fn own_tasks_get_distinct_ids_and_fields() {
    let st = Store::memory().unwrap();
    let a = st.add_local_task("Buy stamps", 2, "").unwrap().unwrap();
    let b = st
        .add_local_task("File taxes", 5, "2026-12-01")
        .unwrap()
        .unwrap();
    assert!(a >= LOCAL_UID_BASE && b == a + 1);
    let t = st.get(b).unwrap();
    assert_eq!(
        (t.subject.as_str(), t.priority, t.due.as_str()),
        ("File taxes", 5, "2026-12-01")
    );
    assert_eq!(t.addr, "");
}

#[test]
fn own_tasks_never_queue_mailbox_changes() {
    let st = Store::memory().unwrap();
    let a = st.add_local_task("Buy stamps", 2, "").unwrap().unwrap();
    st.queue(a, "done", "").unwrap();
    assert!(st.pending(10).is_empty());
}

#[test]
fn own_tasks_survive_a_mailbox_reset_but_not_sign_out() {
    let st = Store::memory().unwrap();
    stored(&st, 5);
    let a = st.add_local_task("Mine", 3, "").unwrap().unwrap();
    st.reset_mailbox_data().unwrap();
    assert!(st.get(5).is_none());
    assert!(st.get(a).is_some());
    st.wipe_all().unwrap();
    assert!(st.get(a).is_none());
}

// ── alert categories ──
#[test]
fn alert_categories_are_validated() {
    let mut s = Settings::default();
    s.alerts.categories = vec!["finance".into(), "work".into()];
    assert!(s.validate().is_ok());
    s.alerts.categories = vec!["bogus".into()];
    assert!(s.validate().is_err());
    s.alerts.categories = vec!["work".to_string(); 9];
    assert!(s.validate().is_err());
}
