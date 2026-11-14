//! Phone-hub edge cases: odd or hostile HTTP, header handling, and recovery from connection floods.
use engine::core::Core;
use serde_json::json;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::Arc;
use std::time::{Duration, Instant};

struct Hub {
    core: Arc<Core>,
    port: u16,
    token: String,
    dir: std::path::PathBuf,
}

fn hub(name: &str) -> Hub {
    let dir = std::env::temp_dir().join(format!("inbox-hubedge-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let core = Core::new(dir.clone(), Arc::new(|_, _| {}));
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
        .max(21000);
    core.rpc(
        "save_settings",
        &json!({"patch": {"hub": {"enabled": true, "port": port}}}),
    )
    .unwrap();
    let token = core.settings().hub.token;
    // wait until the hub answers: a fixed pause is too short on a slow machine (macOS CI started it later than 600 ms)
    let up = Instant::now();
    while TcpStream::connect(("127.0.0.1", port)).is_err() {
        assert!(
            up.elapsed() < Duration::from_secs(30),
            "the hub did not start"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    Hub {
        core,
        port,
        token,
        dir,
    }
}

impl Drop for Hub {
    fn drop(&mut self) {
        let _ = self.core.rpc(
            "save_settings",
            &json!({"patch": {"hub": {"enabled": false}}}),
        );
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn raw(port: u16, request: &[u8]) -> String {
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(8))).unwrap();
    let _ = s.write_all(request);
    let mut out = Vec::new();
    let _ = s.read_to_end(&mut out);
    String::from_utf8_lossy(&out).to_string()
}

fn status(r: &str) -> &str {
    r.split(' ').nth(1).unwrap_or("")
}

fn rpc(h: &Hub, auth: &str, body: &str) -> String {
    raw(
        h.port,
        format!(
            "POST /api/rpc HTTP/1.1\r\nHost: x\r\n{auth}Content-Length: {}\r\n\r\n{body}",
            body.len()
        )
        .as_bytes(),
    )
}

const STATE: &str = r#"{"cmd":"state"}"#;

#[test]
fn the_bearer_scheme_is_case_insensitive_and_other_schemes_are_refused() {
    let h = hub("scheme");
    let t = &h.token;
    for ok in [
        format!("Authorization: Bearer {t}\r\n"),
        format!("Authorization: bearer {t}\r\n"),
        format!("authorization: BEARER {t}\r\n"),
        format!("Authorization: Bearer   {t}  \r\n"),
    ] {
        assert_eq!(status(&rpc(&h, &ok, STATE)), "200", "{ok:?}");
    }
    for bad in [
        format!("Authorization: Basic {t}\r\n"),
        format!("Authorization: {t}\r\n"),
        "Authorization: Bearer \r\n".to_string(),
        "Authorization: \r\n".to_string(),
        String::new(),
        format!("Authorization: Bearer {}x\r\n", t),
        format!("X-Authorization: Bearer {t}\r\n"),
    ] {
        assert_eq!(status(&rpc(&h, &bad, STATE)), "401", "{bad:?}");
    }
}

#[test]
fn broken_content_lengths_and_truncated_bodies_are_refused() {
    let h = hub("lengths");
    let auth = format!("Authorization: Bearer {}\r\n", h.token);
    for cl in ["-1", "abc", "1e3", "", "99999999999999999999"] {
        let r = raw(
            h.port,
            format!("POST /api/rpc HTTP/1.1\r\nHost: x\r\n{auth}Content-Length: {cl}\r\n\r\n{{}}")
                .as_bytes(),
        );
        assert!(
            matches!(status(&r), "400" | "413"),
            "Content-Length {cl:?} gave {}",
            status(&r)
        );
    }
    // fewer bytes than promised, then the client hangs up
    let r = raw(
        h.port,
        format!("POST /api/rpc HTTP/1.1\r\nHost: x\r\n{auth}Content-Length: 500\r\n\r\n{{\"cmd\":")
            .as_bytes(),
    );
    assert!(
        r.is_empty() || status(&r) == "400" || status(&r) == "408",
        "{r}"
    );
    assert_eq!(
        status(&rpc(&h, &auth, STATE)),
        "200",
        "the hub still works afterwards"
    );
}

#[test]
fn nonsense_json_with_a_valid_token_is_a_clean_error() {
    let h = hub("badjson");
    let auth = format!("Authorization: Bearer {}\r\n", h.token);
    for body in [
        "",
        "{",
        "null",
        "[]",
        "\"state\"",
        "{\"cmd\": 5}",
        "{\"cmd\": \"state\", \"args\": 7}",
        "\u{0}\u{0}",
        "{\"cmd\":\"set_priority\",\"args\":{\"uid\":\"x\",\"priority\":{}}}",
    ] {
        let r = rpc(&h, &auth, body);
        assert!(
            matches!(status(&r), "200" | "400"),
            "{body:?} gave {}",
            status(&r)
        );
        assert!(!r.contains("panicked"), "{body:?}");
    }
    assert_eq!(status(&rpc(&h, &auth, STATE)), "200");
}

#[test]
fn unusual_methods_versions_and_request_lines() {
    let h = hub("methods");
    assert_eq!(
        status(&raw(h.port, b"GET / HTTP/1.0\r\n\r\n")),
        "200",
        "HTTP/1.0 clients work"
    );
    assert_eq!(
        status(&raw(h.port, b"GET / HTTP/1.1\r\nHost: x\r\n\r\n")),
        "200"
    );
    assert_eq!(
        status(&raw(h.port, b"PUT /api/rpc HTTP/1.1\r\nHost: x\r\n\r\n")),
        "405"
    );
    assert_eq!(
        status(&raw(h.port, b"DELETE / HTTP/1.1\r\nHost: x\r\n\r\n")),
        "404"
    );
    assert_eq!(
        status(&raw(h.port, b"OPTIONS / HTTP/1.1\r\nHost: x\r\n\r\n")),
        "404"
    );
    assert_eq!(
        status(&raw(h.port, b"GET /api/rpc HTTP/1.1\r\nHost: x\r\n\r\n")),
        "404"
    );
    for bad in [
        &b"GET\t/ HTTP/1.1\r\n\r\n"[..],
        b"GET  HTTP/1.1\r\n\r\n",
        b"GET / FTP/1.1\r\n\r\n",
        b"\r\n\r\n",
        b"GET relative HTTP/1.1\r\n\r\n",
        b"GET / HTTP/1.1\r\nbroken header line\r\n\r\n",
        b"\x16\x03\x01\x02\x00\x01\x00\x01\xfc\x03\x03",
    ] {
        let r = raw(h.port, bad);
        assert!(
            r.is_empty() || matches!(status(&r), "400" | "408"),
            "{:?} gave {}",
            String::from_utf8_lossy(bad),
            status(&r)
        ); // 408: an incomplete request times out
    }
}

#[test]
fn the_hub_recovers_after_a_flood_of_idle_connections() {
    let h = hub("flood");
    let idle: Vec<TcpStream> = (0..60)
        .filter_map(|_| TcpStream::connect(("127.0.0.1", h.port)).ok())
        .collect();
    assert!(idle.len() >= 40, "the flood was opened");
    // idle connections hold their slots only until the 5 second read timeout
    let t = Instant::now();
    let mut ok = false;
    while t.elapsed() < Duration::from_secs(12) {
        if status(&raw(
            h.port,
            b"GET /manifest.json HTTP/1.1\r\nHost: x\r\n\r\n",
        )) == "200"
        {
            ok = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    assert!(
        ok,
        "the hub never recovered from {} idle connections",
        idle.len()
    );
    assert!(
        t.elapsed() < Duration::from_secs(11),
        "recovery took {:?}",
        t.elapsed()
    );
    drop(idle);
}
