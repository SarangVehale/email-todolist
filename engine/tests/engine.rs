use engine::mail;
use engine::signals::{base_score, compute, SentIndex};
use engine::store::Store;
use engine::triage::{triage_with_fallback, RulesTriager, Triage, Triager};

fn email(raw: &str, uid: u32) -> mail::Email {
    mail::parse(uid, raw.replace('\n', "\r\n").as_bytes()).unwrap()
}

fn sent() -> SentIndex {
    let mut s = SentIndex::default();
    s.add_headers(b"To: Bob <bob@x.com>\r\nMessage-ID: <m1@me>\r\n\r\n");
    s
}

#[test]
fn parses_basic_message() {
    let e = email(
        "From: Bob Smith <Bob@X.com>\nSubject: Lunch?\nDate: Tue, 10 Nov 2026 09:00:00 +0000\nMessage-ID: <a@x>\n\nFree at noon?\n",
        7,
    );
    assert_eq!(
        (e.sender_name.as_str(), e.sender_addr.as_str()),
        ("Bob Smith", "bob@x.com")
    );
    assert_eq!(e.subject, "Lunch?");
    assert_eq!(e.excerpt, "Free at noon?");
    assert!(e.received > 1.7e9);
}

#[test]
fn reply_to_my_mail_is_high_priority() {
    let e = email(
        "From: bob@x.com\nSubject: Re: hi\nIn-Reply-To: <m1@me>\n\nok\n",
        1,
    );
    let s = compute(&e, &sent());
    assert!(s.reply && s.known);
    assert_eq!(base_score(&s), 4);
}

#[test]
fn bulk_mail_is_low_priority() {
    let e = email(
        "From: noreply@shop.com\nSubject: Sale\nList-Unsubscribe: <x>\n\nbuy\n",
        2,
    );
    let s = compute(&e, &sent());
    assert!(s.bulk);
    assert_eq!(base_score(&s), 1);
}

#[test]
fn stranger_gets_middle_score() {
    let e = email("From: new@person.com\nSubject: hello\n\nhi\n", 3);
    assert_eq!(base_score(&compute(&e, &sent())), 3);
}

#[test]
fn multipart_picks_plain_text() {
    let raw = "From: a@b.com\nSubject: m\nContent-Type: multipart/alternative; boundary=XX\n\n--XX\nContent-Type: text/plain\n\nplain body\n--XX\nContent-Type: text/html\n\n<p>html</p>\n--XX--\n";
    assert_eq!(email(raw, 4).excerpt, "plain body");
}

struct Failing;
impl Triager for Failing {
    fn triage(&self, _: &mail::Email, _: &engine::signals::Signals) -> anyhow::Result<Triage> {
        anyhow::bail!("model unavailable")
    }
}

#[test]
fn failing_ai_falls_back_to_rules() {
    let e = email("From: new@person.com\nSubject: hello\n\nhi\n", 5);
    let s = compute(&e, &sent());
    let t = triage_with_fallback(&Failing, &e, &s);
    assert_eq!(t, RulesTriager.triage(&e, &s).unwrap());
}

#[test]
fn store_roundtrip() {
    let st = Store::memory().unwrap();
    let e = email(
        "From: bob@x.com\nSubject: Re: hi\nIn-Reply-To: <m1@me>\n\nok\n",
        9,
    );
    let s = compute(&e, &sent());
    let t = RulesTriager.triage(&e, &s).unwrap();
    assert!(!st.seen(9));
    st.add(&e, &t, &s).unwrap();
    st.set_last_uid(9).unwrap();
    assert!(st.seen(9) && st.last_uid() == 9);
    let all = st.all_tasks().unwrap();
    assert_eq!(all.len(), 1);
    assert!(all[0].signals.reply && all[0].priority == 4);
}
