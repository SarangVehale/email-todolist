//! The parsers that read text nobody here wrote (a mail message, a calendar invitation, a phrase in a message) must not panic and must not
//! take long on damaged input. Each is fed thousands of damaged variants of valid samples (bytes flipped, inserted, deleted, a piece repeated
//! many times, the end cut off) and some pure noise, from a fixed seed so a failure can be repeated.
//!
//! `FUZZ_FACTOR=20 FUZZ_SEED=3 cargo test --test fuzz_inputs` looks longer and from other seeds.
//!
//! This is not a coverage-guided fuzzer: it finds the shallow panics (an index, a slice at a character boundary, an unwrap, an
//! explosion in time or memory) that hostile or merely broken mail makes. A panic in the sync would otherwise be a crash on every start.
use chrono::NaiveDate;
use engine::{calendar, dates, mail, reminders, rules, signals};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::{Duration, Instant};

/// xorshift64*: a small generator with a fixed seed (no dependency, the same inputs on every machine).
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545F4914F6CDD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

/// Bytes that parsers tend to trip on.
const AWKWARD: &[&[u8]] = &[
    b"\r\n",
    b"\n",
    b"\r",
    b"\0",
    b":",
    b";",
    b"=",
    b"<",
    b">",
    b"\"",
    b"'",
    b"\\",
    b"%",
    b"&",
    b"(",
    b")",
    b"[",
    b"]",
    b",",
    b" ",
    b"\t",
    b"=?utf-8?B?",
    b"=?utf-8?Q?",
    b"?=",
    b"=C3=A9",
    b"\xff\xfe",
    b"\xc3",
    b"\xe2\x82",
    b"\xf0\x9f\x98",
    b"\xed\xa0\x80",
    b"%00",
    b"&#x110000;",
    b"boundary=",
    b"--",
    b"Content-Type: multipart/mixed; boundary=\"",
    b"BEGIN:VEVENT\r\n",
    b"END:VEVENT\r\n",
    b"DTSTART:",
    b"TZID=",
    b"RRULE:FREQ=DAILY",
    b"<script>",
    b"</",
    b"<!--",
    b"-->",
    b"<![CDATA[",
    b"<a href=\"",
    b"<br/>",
    b"&nbsp;",
    b"9999999999999999999",
    b"-1",
    b"2026-02-30",
    b"31/12/9999",
    b"00:00:60",
    b"\xe2\x80\xae",
];

fn damage(rng: &mut Rng, sample: &[u8]) -> Vec<u8> {
    let mut v = sample.to_vec();
    for _ in 0..1 + rng.below(5) {
        match rng.below(7) {
            0 if !v.is_empty() => {
                let i = rng.below(v.len());
                v[i] = rng.next() as u8;
            }
            1 => {
                let i = rng.below(v.len() + 1);
                let a = AWKWARD[rng.below(AWKWARD.len())];
                v.splice(i..i, a.iter().copied());
            }
            2 if !v.is_empty() => {
                let i = rng.below(v.len());
                let j = (i + 1 + rng.below(40)).min(v.len());
                v.drain(i..j);
            }
            3 if !v.is_empty() => {
                // a piece repeated many times: nested, long or quadratic input
                let i = rng.below(v.len());
                let j = (i + 1 + rng.below(60)).min(v.len());
                let piece = v[i..j].to_vec();
                let times = 2 + rng.below(300);
                let mut out = v[..j].to_vec();
                for _ in 0..times {
                    out.extend_from_slice(&piece);
                }
                out.extend_from_slice(&v[j..]);
                v = out;
            }
            4 if !v.is_empty() => {
                let keep = rng.below(v.len());
                v.truncate(keep);
            }
            5 => {
                let n = rng.below(200);
                for _ in 0..n {
                    v.push(rng.next() as u8);
                }
            }
            _ => {
                if v.len() > 4 {
                    let i = rng.below(v.len() - 2);
                    v.swap(i, i + 1);
                }
            }
        }
    }
    v
}

/// Runs `f` on `rounds` damaged variants of the samples and reports the first panic or slow call.
fn hammer(name: &str, samples: &[&[u8]], rounds: usize, seed: u64, f: impl Fn(&[u8])) {
    // FUZZ_FACTOR=20 makes every run twenty times as long, FUZZ_SEED=7 starts from other seeds: for a deeper look than the everyday test
    let env = |k: &str, d: u64| {
        std::env::var(k)
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(d)
    };
    let (rounds, seed) = (
        rounds * env("FUZZ_FACTOR", 1) as usize,
        seed + 1000 * env("FUZZ_SEED", 0),
    );
    let mut rng = Rng(seed);
    let limit = Duration::from_secs(2);
    let mut slowest = (Duration::ZERO, 0usize);
    for round in 0..rounds {
        let input = if round % 10 == 9 {
            (0..rng.below(300)).map(|_| rng.next() as u8).collect() // pure noise
        } else {
            {
                let pick = rng.below(samples.len());
                damage(&mut rng, samples[pick])
            }
        };
        let began = Instant::now();
        let outcome = catch_unwind(AssertUnwindSafe(|| f(&input)));
        let took = began.elapsed();
        if took > slowest.0 {
            slowest = (took, input.len());
        }
        if outcome.is_err() {
            let shown: String = String::from_utf8_lossy(&input).chars().take(400).collect();
            panic!(
                "{name} panicked on round {round} (seed {seed}), input of {} bytes: {shown:?}",
                input.len()
            );
        }
        assert!(
            took < limit,
            "{name} took {took:?} on round {round} (seed {seed}) for an input of {} bytes",
            input.len()
        );
    }
    eprintln!(
        "{name}: {rounds} inputs, slowest {:?} for {} bytes",
        slowest.0, slowest.1
    );
}

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

const PLAIN: &[u8] = b"From: Pat Lee <pat@example.org>\r\nTo: me@example.com\r\nSubject: Lunch on Friday?\r\nDate: Thu, 12 Nov 2026 09:30:00 +0100\r\nMessage-ID: <abc123@example.org>\r\nIn-Reply-To: <prev@example.com>\r\nReferences: <first@example.org> <prev@example.com>\r\nList-Unsubscribe: <https://example.org/u?id=1>, <mailto:u@example.org>\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Transfer-Encoding: quoted-printable\r\n\r\nCan you do lunch on Friday? Please reply by 20 November.=0D=0A=C3=A9t=C3=A9\r\n";
const MULTIPART: &[u8] = b"From: =?utf-8?B?w4lsaXNl?= <elise@example.org>\r\nSubject: =?utf-8?Q?Invoice_=E2=82=AC40?=\r\nDate: Fri, 13 Nov 2026 10:00:00 +0000\r\nMessage-ID: <inv@example.org>\r\nMIME-Version: 1.0\r\nContent-Type: multipart/mixed; boundary=\"BOUND\"\r\n\r\n--BOUND\r\nContent-Type: multipart/alternative; boundary=\"ALT\"\r\n\r\n--ALT\r\nContent-Type: text/plain; charset=iso-8859-1\r\n\r\nPlease pay by 2026-12-01.\r\n--ALT\r\nContent-Type: text/html; charset=utf-8\r\n\r\n<html><body><p>Please <b>pay</b> by 2026-12-01.</p><a href=\"https://example.org/pay\">Pay</a><script>x()</script></body></html>\r\n--ALT--\r\n--BOUND\r\nContent-Type: application/pdf; name=\"inv.pdf\"\r\nContent-Disposition: attachment; filename=\"inv.pdf\"\r\nContent-Transfer-Encoding: base64\r\n\r\nJVBERi0xLjQK\r\n--BOUND\r\nContent-Type: text/calendar; method=REQUEST\r\n\r\nBEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nSUMMARY:Review\r\nDTSTART:20261120T140000Z\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n--BOUND--\r\n";
const NO_HEADERS_END: &[u8] = b"Subject: no blank line\r\nFrom: x@y.example";
const ICS: &[u8] = b"BEGIN:VCALENDAR\r\nVERSION:2.0\r\nMETHOD:REQUEST\r\nBEGIN:VEVENT\r\nUID:abc@example.org\r\nDTSTAMP:20261112T090000Z\r\nDTSTART;TZID=Europe/Berlin:20261120T140000\r\nDTEND;TZID=Europe/Berlin:20261120T150000\r\nSUMMARY:Quarterly review\r\nLOCATION:Room 4\\, Floor 2\r\nDESCRIPTION:Bring the report.\\nAnd the slides.\r\nORGANIZER;CN=Pat Lee:mailto:pat@example.org\r\nATTENDEE;PARTSTAT=NEEDS-ACTION:mailto:me@example.com\r\nRRULE:FREQ=WEEKLY;COUNT=4\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
const ICS_ALL_DAY: &[u8] = b"BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nDTSTART;VALUE=DATE:20261224\r\nSUMMARY:Holiday\r\n a folded\r\n line\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
const HTML: &[u8] = b"<html><head><style>p{color:red}</style></head><body><h1>Hello &amp; welcome</h1><p>Line one<br>Line two &nbsp;&#169;&#x1F600;</p><ul><li>a</li><li>b</li></ul><table><tr><td>1</td></tr></table><a href=\"https://example.org\">link</a><!-- c --><script>alert(1)</script></body></html>";

#[test]
fn a_mail_message_never_panics_the_parser() {
    hammer(
        "mail::parse",
        &[PLAIN, MULTIPART, NO_HEADERS_END, b""],
        6000,
        1,
        |b| {
            let _ = mail::parse(7, b);
        },
    );
}

#[test]
fn the_text_helpers_never_panic() {
    let hdr: &[&[u8]] = &[
        b"Pat Lee <pat@example.org>, \"Lee, Pat\" <p2@example.org>, undisclosed-recipients:;",
        b"<https://example.org/u?id=1>, <mailto:u@example.org?subject=unsubscribe>",
        b"<a@b> <c@d> <e@f>",
    ];
    hammer("mail::html_to_text", &[HTML], 3000, 2, |b| {
        let _ = mail::html_to_text(&text(b));
    });
    hammer(
        "mail::addresses and header_ids and unsubscribe_link",
        hdr,
        5000,
        3,
        |b| {
            let t = text(b);
            let _ = mail::addresses(&t);
            let _ = mail::header_ids(&t);
            let _ = mail::unsubscribe_link(&t);
            let _ = mail::clean_message_id(&t);
            let _ = mail::valid_address(&t);
        },
    );
}

#[test]
fn a_calendar_invitation_never_panics_the_reader() {
    hammer(
        "calendar::parse",
        &[ICS, ICS_ALL_DAY, b"BEGIN:VCALENDAR\r\nEND:VCALENDAR\r\n"],
        6000,
        4,
        |b| {
            if let Some(e) = calendar::parse(&text(b)) {
                let _ = calendar::day(&e);
                let _ = calendar::day_at_offset(&e, 3600);
                let _ = calendar::upcoming_day(&e, "2026-11-15");
                let _ = calendar::file_name(&e);
                let stamp = NaiveDate::from_ymd_opt(2026, 11, 15)
                    .unwrap()
                    .and_hms_opt(9, 0, 0)
                    .unwrap();
                let _ = calendar::to_ics(&e, stamp, "seed");
            }
        },
    );
}

#[test]
fn a_date_in_a_message_or_a_reminder_never_panics_the_readers() {
    let received = NaiveDate::from_ymd_opt(2026, 11, 12).unwrap();
    let phrases: &[&[u8]] = &[
        b"Please pay by 20 November",
        b"due 2026-12-01 at the latest",
        b"deadline: Friday 13th",
        b"by next Tuesday, or 12/11/2026",
        b"on 5 Jan, then 31 Dec 2026",
        b"2026-02-30",
        b"2026-11-20 14:30",
    ];
    hammer("dates::extract_due and friends", phrases, 8000, 5, |b| {
        let t = text(b);
        let _ = dates::extract_due(&t, received);
        let _ = dates::valid_iso(&t);
        let _ = dates::parse_iso(&t);
        let _ = dates::due_within_a_day(&t);
        let _ = reminders::parse_local(&t);
        let _ = reminders::valid_repeat(&t);
    });
}

#[test]
fn rule_matching_and_subject_reading_never_panic() {
    let subjects: &[&[u8]] = &[
        b"Re: Fwd: Invoice 4021",
        b"FW: [ext] Lunch?",
        b"=?utf-8?B?w4k=?= update",
    ];
    let rule = |field: &str, text: &str| -> rules::Rule {
        serde_json::from_value(
            serde_json::json!({"id": "r", "field": field, "text": text, "priority": 2}),
        )
        .unwrap()
    };
    hammer(
        "rules::matches and signals::is_forward",
        subjects,
        5000,
        6,
        |b| {
            let t = text(b);
            for field in ["domain", "sender", "subject"] {
                let r = rule(field, &t);
                let _ = rules::matches(&r, "pat@example.org", &t);
                let _ = rules::matches(&r, &t, "Subject");
                let _ = rules::validate(&[r]);
            }
            let _ = signals::is_forward(&t);
        },
    );
}

// A check that cannot fail proves nothing: the same harness must notice a parser that panics and one that is too slow.
#[test]
#[should_panic(expected = "panicked on round")]
fn the_harness_notices_a_panic() {
    hammer(
        "a parser that panics on a NUL",
        &[b"abc\0def"],
        400,
        9,
        |b| {
            if b.contains(&0) {
                panic!("a NUL");
            }
        },
    );
}

#[test]
#[should_panic(expected = "took")]
fn the_harness_notices_a_slow_input() {
    hammer("a parser that stalls", &[b"abc"], 3, 9, |_| {
        std::thread::sleep(Duration::from_millis(2100))
    });
}
