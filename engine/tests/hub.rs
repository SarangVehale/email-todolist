//! Phone-hub security tests, run against a real listening socket.
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
    let dir = std::env::temp_dir().join(format!("inbox-hub-{name}-{}", std::process::id()));
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

fn raw(port: u16, request: &str) -> String {
    // a refused connection (the hub restarts whenever settings are saved, and a slow machine takes a moment to bind again) is an
    // empty answer, so a test that waits for the hub can try again
    let Ok(mut s) = TcpStream::connect(("127.0.0.1", port)) else {
        return String::new();
    };
    s.set_read_timeout(Some(Duration::from_secs(8))).unwrap();
    let _ = s.write_all(request.as_bytes());
    let mut out = Vec::new();
    let _ = s.read_to_end(&mut out);
    String::from_utf8_lossy(&out).to_string()
}

fn rpc(h: &Hub, token: &str, body: &str) -> String {
    raw(h.port, &format!("POST /api/rpc HTTP/1.1\r\nHost: x\r\nAuthorization: Bearer {token}\r\nContent-Length: {}\r\n\r\n{body}", body.len()))
}

fn head(r: &str) -> &str {
    &r[..r.len().min(60)]
}

#[test]
fn responses_carry_hardening_headers() {
    let h = hub("headers");
    // every way the hub can answer goes through one function, and each must carry the headers: a page, a missing page, a refusal, a wrong method,
    // a malformed request. (A new error path that wrote its own reply would be caught here.)
    let requests = [
        "GET / HTTP/1.1\r\nHost: x\r\n\r\n".to_string(),
        "GET /nothing-here HTTP/1.1\r\nHost: x\r\n\r\n".to_string(),
        "POST /api/rpc HTTP/1.1\r\nHost: x\r\nAuthorization: Bearer wrong\r\nContent-Length: 2\r\n\r\n{}".to_string(),
        "PUT /api/rpc HTTP/1.1\r\nHost: x\r\n\r\n".to_string(),
        "GET / HTTP/1.1\r\nHost: x\r\nContent-Length: nine\r\n\r\n".to_string(),
    ];
    for request in requests {
        let r = raw(h.port, &request).to_lowercase();
        assert!(r.starts_with("http/1.1 "), "no reply to {request:?}");
        for want in [
            "x-frame-options: deny",
            "referrer-policy: no-referrer",
            "x-content-type-options: nosniff",
            "content-security-policy:",
            "frame-ancestors 'none'",
            "cache-control: no-store",
            "cross-origin-opener-policy: same-origin",
            "cross-origin-resource-policy: same-origin",
            "permissions-policy: camera=()",
        ] {
            assert!(
                r.contains(want),
                "missing {want} in the reply to {request:?}:\n{r}"
            );
        }
        assert!(
            !r.contains("script-src 'self' 'unsafe-inline'"),
            "inline scripts must not be allowed"
        );
    }
}

#[test]
fn guessing_tokens_gets_an_address_locked_out() {
    let h = hub("ratelimit");
    let mut saw_429 = false;
    for i in 0..25 {
        let r = rpc(&h, &format!("wrong{i}"), r#"{"cmd":"state"}"#);
        assert!(
            r.starts_with("HTTP/1.1 401") || r.starts_with("HTTP/1.1 429"),
            "{}",
            head(&r)
        );
        saw_429 |= r.starts_with("HTTP/1.1 429");
    }
    assert!(saw_429, "brute force was never throttled");
    let r = rpc(&h, &h.token, r#"{"cmd":"state"}"#);
    assert!(
        r.starts_with("HTTP/1.1 429"),
        "a locked-out address stays locked out: {}",
        head(&r)
    );
}

#[test]
fn oversized_and_malformed_requests_are_refused() {
    let h = hub("oversize");
    let r = raw(h.port, &format!("POST /api/rpc HTTP/1.1\r\nHost: x\r\nAuthorization: Bearer {}\r\nContent-Length: 99999999\r\n\r\n", h.token));
    assert!(r.starts_with("HTTP/1.1 413"), "{}", head(&r));
    assert!(raw(h.port, "garbage\r\n\r\n").starts_with("HTTP/1.1 400"));
    let huge = format!(
        "GET / HTTP/1.1\r\nHost: x\r\nX: {}\r\n\r\n",
        "a".repeat(40_000)
    );
    let r = raw(h.port, &huge);
    assert!(
        r.starts_with("HTTP/1.1 431") || r.starts_with("HTTP/1.1 400"),
        "{}",
        head(&r)
    );
}

#[test]
fn stalled_clients_cannot_block_other_clients() {
    let h = hub("slowloris");
    let mut stuck: Vec<TcpStream> = (0..6)
        .map(|_| TcpStream::connect(("127.0.0.1", h.port)).unwrap())
        .collect();
    for s in &mut stuck {
        s.write_all(b"POST /api/rpc HTTP/1.1\r\nHost: x\r\nContent-Length: 500\r\n\r\npartial")
            .unwrap();
    }
    let t = Instant::now();
    let r = raw(h.port, "GET / HTTP/1.1\r\nHost: x\r\n\r\n");
    assert!(r.starts_with("HTTP/1.1 200"), "{}", head(&r));
    assert!(
        t.elapsed() < Duration::from_secs(3),
        "served only after {:?}",
        t.elapsed()
    );
}

#[test]
fn hub_survives_repeated_settings_saves() {
    let h = hub("restart");
    for i in 0..4 {
        h.core
            .rpc(
                "save_settings",
                &json!({"patch": {"alerts": {"desktop": i % 2 == 0}}}),
            )
            .unwrap();
        let mut up = false;
        for _ in 0..50 {
            if raw(h.port, "GET /manifest.json HTTP/1.1\r\nHost: x\r\n\r\n")
                .starts_with("HTTP/1.1 200")
            {
                up = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        assert!(up, "hub was not serving after settings save #{i}");
    }
}

#[test]
fn rotating_the_token_kills_the_old_one() {
    let h = hub("rotate");
    let old = h.token.clone();
    h.core.rpc("rotate_hub_token", &json!({})).unwrap();
    let new = h.core.settings().hub.token;
    assert_ne!(old, new);
    let mut ok = false;
    for _ in 0..50 {
        if rpc(&h, &new, r#"{"cmd":"state"}"#).starts_with("HTTP/1.1 200") {
            ok = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(ok, "new token never worked");
    assert!(rpc(&h, &old, r#"{"cmd":"state"}"#).starts_with("HTTP/1.1 401"));
}

#[test]
fn public_state_leaks_neither_account_nor_alert_secrets() {
    let h = hub("publicstate");
    h.core
        .rpc(
            "save_settings",
            &json!({"patch": {"alerts": {"ntfy_topic": "secret-topic-123"}}}),
        )
        .unwrap();
    let mut r = String::new();
    for _ in 0..50 {
        r = rpc(&h, &h.core.settings().hub.token, r#"{"cmd":"state"}"#);
        if r.starts_with("HTTP/1.1 200") {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(r.starts_with("HTTP/1.1 200"), "{}", head(&r));
    assert!(
        !r.contains("secret-topic-123") && !r.contains(&h.token),
        "a secret leaked through the hub state"
    );
}

#[test]
fn every_ui_file_is_served_and_nothing_else() {
    let h = hub("static");
    let ui = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../app/ui");
    let mut stack = vec![ui.clone()];
    let mut n = 0;
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x != "txt")
                // the demo backend and its preview page are for development only and are deliberately
                // not shipped to the network
                && p
                    .file_name()
                    .is_some_and(|f| f != "mock.js" && f != "demo.html")
            {
                let rel = p
                    .strip_prefix(&ui)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                let r = raw(h.port, &format!("GET /{rel} HTTP/1.1\r\nHost: x\r\n\r\n"));
                assert!(
                    r.starts_with("HTTP/1.1 200"),
                    "/{rel} is not served by the hub"
                );
                n += 1;
            }
        }
    }
    assert!(n >= 6, "only {n} files checked");
    for bad in [
        "/../Cargo.toml",
        "/%2e%2e/Cargo.toml",
        "/fonts/../index.html%00",
        "//etc/passwd",
        "/settings.json",
        "/agent.db",
    ] {
        assert!(
            !raw(h.port, &format!("GET {bad} HTTP/1.1\r\nHost: x\r\n\r\n"))
                .starts_with("HTTP/1.1 200"),
            "{bad} was served"
        );
    }
}

#[test]
fn the_demo_backend_is_never_served_to_the_network() {
    // A visitor with no access code must not be handed the demo backend: with it, the page shows the setup wizard,
    // asks for an email address and an app password, and answers "Connected" to anything typed. That is a credential
    // form served from the user's own machine to everyone on the network.
    let h = hub("nomock");
    for path in ["/mock.js", "/demo.html"] {
        let r = raw(h.port, &format!("GET {path} HTTP/1.1\r\nHost: x\r\n\r\n"));
        assert!(r.starts_with("HTTP/1.1 404"), "{path}: {}", head(&r));
    }
    // and the page it would have been loaded from must not reference it either
    let page = raw(h.port, "GET /index.html HTTP/1.1\r\nHost: x\r\n\r\n");
    assert!(
        !page.contains("mock.js"),
        "index.html still loads the demo backend"
    );
}
