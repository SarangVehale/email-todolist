//! The phone hub is the one part of the app that listens on a network. Damaged requests (bytes flipped, inserted, cut, a piece repeated, the wrong
//! length, noise) are sent to a real hub over a real socket from a fixed seed, and three things must hold: no connection thread panics, every
//! reply is a well-formed HTTP reply (or nothing, when the request was cut off), and the hub still answers a plain request afterwards.
//!
//! This is a separate file so that it can count panics with a panic hook of its own: a panic in a connection thread ends that thread only, and
//! the hub carries on, so without the count it would go unseen. `FUZZ_FACTOR` and `FUZZ_SEED` run it longer from other seeds.
mod common;
use common::{damage, factor, seeded, Rng};
use engine::core::Core;
use serde_json::json;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

static PANICS: AtomicUsize = AtomicUsize::new(0);

/// One exchange: the bytes are sent, the sending side is closed, and what comes back is read (at most 64 KB, for at most 3 seconds).
fn exchange(port: u16, bytes: &[u8]) -> Option<Vec<u8>> {
    let mut s = TcpStream::connect(("127.0.0.1", port)).ok()?;
    s.set_read_timeout(Some(Duration::from_secs(3))).ok()?;
    s.set_write_timeout(Some(Duration::from_secs(3))).ok()?;
    let _ = s.write_all(bytes);
    let _ = s.shutdown(Shutdown::Write);
    let mut out = Vec::new();
    let _ = s.take(64 * 1024).read_to_end(&mut out);
    Some(out)
}

fn well_formed(reply: &[u8]) -> bool {
    reply.is_empty()
        || (reply.starts_with(b"HTTP/1.1 ")
            && reply.len() > 12
            && reply[9..12].iter().all(|b| b.is_ascii_digit()))
}

fn post(token: &str, body: &str) -> Vec<u8> {
    format!(
        "POST /api/rpc HTTP/1.1\r\nHost: phone\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .into_bytes()
}

#[test]
fn damaged_requests_never_panic_the_hub_or_stop_it_answering() {
    std::panic::set_hook(Box::new(|info| {
        PANICS.fetch_add(1, Ordering::SeqCst);
        eprintln!("PANIC in the hub: {info}");
    }));
    let dir = std::env::temp_dir().join(format!("ia-fuzz-hub-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let core = Core::new(dir.clone(), Arc::new(|_, _| {}));
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
        .max(20000);
    core.rpc(
        "save_settings",
        &json!({"patch": {"hub": {"enabled": true, "port": port}}}),
    )
    .unwrap();
    let token = core.settings().hub.token;
    let up = Instant::now();
    while TcpStream::connect(("127.0.0.1", port)).is_err() {
        assert!(
            up.elapsed() < Duration::from_secs(30),
            "the hub did not start"
        );
        std::thread::sleep(Duration::from_millis(50));
    }

    let samples: Vec<Vec<u8>> = vec![
        b"GET / HTTP/1.1\r\nHost: phone\r\nConnection: close\r\n\r\n".to_vec(),
        b"GET /manifest.json HTTP/1.1\r\nHost: phone\r\nAccept: */*\r\nConnection: close\r\n\r\n"
            .to_vec(),
        b"GET /../../etc/passwd HTTP/1.1\r\nHost: phone\r\n\r\n".to_vec(),
        post(&token, r#"{"cmd":"state","args":{}}"#),
        post(
            &token,
            r#"{"cmd":"set_priority","args":{"uid":1,"priority":4}}"#,
        ),
        post("wrong-code", r#"{"cmd":"state","args":{}}"#),
        post(&token, r#"{"cmd":"search","args":{"q":"lunch"}}"#),
    ];
    // the undamaged samples get proper replies
    for s in &samples {
        let reply = exchange(port, s).expect("the hub answers");
        assert!(
            well_formed(&reply) && !reply.is_empty(),
            "an undamaged sample gets a reply: {:?}",
            String::from_utf8_lossy(&reply)
        );
    }

    let mut rng = Rng(seeded(21));
    let rounds = 600 * factor();
    let mut replies = 0usize;
    for round in 0..rounds {
        let pick = rng.below(samples.len());
        let bytes = if round % 12 == 11 {
            (0..rng.below(400)).map(|_| rng.next() as u8).collect()
        } else {
            damage(&mut rng, &samples[pick])
        };
        if let Some(reply) = exchange(port, &bytes) {
            assert!(
                well_formed(&reply),
                "round {round}: a reply that is not HTTP: {:?} (to {:?})",
                String::from_utf8_lossy(&reply[..reply.len().min(120)]),
                String::from_utf8_lossy(&bytes[..bytes.len().min(200)])
            );
            replies += usize::from(!reply.is_empty());
        }
        if round % 25 == 24 {
            let alive = exchange(port, &samples[1]).unwrap_or_default();
            assert!(
                alive.starts_with(b"HTTP/1.1 200"),
                "round {round}: the hub stopped answering a plain request: {:?}",
                String::from_utf8_lossy(&alive[..alive.len().min(120)])
            );
        }
    }
    assert_eq!(
        PANICS.load(Ordering::SeqCst),
        0,
        "a connection thread of the hub panicked (see PANIC above)"
    );
    assert!(
        replies > rounds / 10,
        "most damaged requests still get an answer, an error at least ({replies} of {rounds})"
    );

    let _ = core.rpc(
        "save_settings",
        &json!({"patch": {"hub": {"enabled": false}}}),
    );
    let _ = std::fs::remove_dir_all(dir);
}
