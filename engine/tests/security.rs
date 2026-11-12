//! Security regression tests. Each one models an attacker-controlled field of a RECEIVED email.
use engine::mail;
use engine::signals::{compute, SentIndex};

fn parse(raw: &str) -> mail::Email {
    mail::parse(1, raw.replace('\n', "\r\n").as_bytes()).unwrap()
}

#[test]
fn message_id_with_encoded_crlf_cannot_carry_imap_commands() {
    // RFC 2047 decoding turns =0D=0A into real CR LF after header unfolding.
    let e = parse("From: a@b.com\nSubject: x\nMessage-ID: =?utf-8?q?<x>=0D=0AA9_UID_STORE_1:*_+FLAGS_(\\Deleted)=0D=0A?=\n\nbody\n");
    assert!(
        !e.message_id.contains('\r') && !e.message_id.contains('\n'),
        "control characters survived: {:?}",
        e.message_id
    );
    assert_eq!(
        e.message_id, "",
        "a malformed Message-ID must be dropped, not sanitised"
    );
}

#[test]
fn message_id_accepts_only_a_plain_angle_addr() {
    assert_eq!(
        parse("From: a@b.com\nMessage-ID: <abc.123@example.com>\n\nx\n").message_id,
        "<abc.123@example.com>"
    );
    for bad in ["<a\"b@x>", "<a\\b@x>", "abc@x", "<>", "<a b@x>", "<é@x>"] {
        let e = parse(&format!("From: a@b.com\nMessage-ID: {bad}\n\nx\n"));
        assert_eq!(e.message_id, "", "accepted {bad:?}");
    }
    let long = format!("<{}@x>", "a".repeat(300));
    assert_eq!(
        parse(&format!("From: a@b.com\nMessage-ID: {long}\n\nx\n")).message_id,
        ""
    );
}

#[test]
fn deeply_nested_multipart_does_not_crash_the_process() {
    let depth = 4000;
    let mut raw = String::from(
        "From: a@b.com\nSubject: nest\nContent-Type: multipart/mixed; boundary=b0\n\n",
    );
    for i in 0..depth {
        raw.push_str(&format!(
            "--b{i}\nContent-Type: multipart/mixed; boundary=b{}\n\n",
            i + 1
        ));
    }
    raw.push_str("deep\n");
    // Either a clean parse or a clean error is fine; a stack overflow would abort this test binary.
    let _ = mail::parse(1, raw.replace('\n', "\r\n").as_bytes());
}

#[test]
fn failed_dkim_or_spf_cancels_known_sender_trust() {
    let mut sent = SentIndex::default();
    sent.add_headers(b"To: boss@corp.com\r\nMessage-ID: <m1@me>\r\n\r\n");
    let spoof = parse("From: boss@corp.com\nSubject: wire money\nAuthentication-Results: mx.example; dkim=fail; spf=fail\nIn-Reply-To: <m1@me>\n\nx\n");
    let s = compute(&spoof, &sent);
    assert!(
        !s.known && !s.reply,
        "spoofed sender must not be trusted: {s:?}"
    );
    let genuine = parse("From: boss@corp.com\nSubject: hi\nAuthentication-Results: mx.example; dkim=pass; spf=pass\n\nx\n");
    assert!(
        compute(&genuine, &sent).known,
        "authentic mail must stay trusted"
    );
    let unsigned = parse("From: boss@corp.com\nSubject: hi\n\nx\n");
    assert!(
        compute(&unsigned, &sent).known,
        "no auth header: keep today's behaviour"
    );
}
