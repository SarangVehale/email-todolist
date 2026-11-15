//! Shared by the fuzz tests: a small seeded generator and the ways of damaging a sample.
#![allow(dead_code)] // each test file uses part of this module

/// xorshift64*: a small generator with a fixed seed (no dependency, the same inputs on every machine).
pub struct Rng(pub u64);
impl Rng {
    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545F4914F6CDD1D)
    }
    pub fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

/// FUZZ_FACTOR makes every run that many times as long, FUZZ_SEED starts from other seeds: for a deeper look than the everyday test.
pub fn factor() -> usize {
    std::env::var("FUZZ_FACTOR")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1)
}
pub fn seeded(seed: u64) -> u64 {
    seed.wrapping_add(
        1000u64.wrapping_mul(
            std::env::var("FUZZ_SEED")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0),
        ),
    )
}

/// Bytes that parsers tend to trip on.
pub const AWKWARD: &[&[u8]] = &[
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

pub fn damage(rng: &mut Rng, sample: &[u8]) -> Vec<u8> {
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
