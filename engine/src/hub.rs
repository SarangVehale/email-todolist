//! Phone hub: a small purpose-built HTTP/1.1 server that serves the UI and a token-protected
//! command endpoint to the user's phone or tablet.
//!
//! It is deliberately minimal and defensive: one request per connection, hard size and time
//! limits (so a stalled client cannot hold the server), a cap on concurrent connections,
//! per-address lockout after repeated bad tokens, exact-match routes only (no filesystem access),
//! and hardening headers on every response. It speaks plain HTTP: use it on a trusted network or
//! over a private VPN.
use crate::core::Core;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{ErrorKind, Read, Write};
use std::net::{IpAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const MAX_HEAD: usize = 16 * 1024;
const MAX_BODY: usize = 64 * 1024;
const MAX_CONNECTIONS: usize = 32;
const IO_TIMEOUT: Duration = Duration::from_secs(5);
const LOCKOUT_AFTER: u32 = 10;
const LOCKOUT_WINDOW: Duration = Duration::from_secs(60);

/// Everything under app/ui that the hub may serve. A test checks this list against the directory.
const FILES: &[(&str, &str, &[u8])] = &[
    (
        "/",
        "text/html; charset=utf-8",
        include_bytes!("../../app/ui/index.html"),
    ),
    (
        "/index.html",
        "text/html; charset=utf-8",
        include_bytes!("../../app/ui/index.html"),
    ),
    (
        "/style.css",
        "text/css; charset=utf-8",
        include_bytes!("../../app/ui/style.css"),
    ),
    (
        "/main.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../app/ui/main.js"),
    ),
    (
        "/drafts.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../app/ui/drafts.js"),
    ),
    (
        "/setup.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../app/ui/setup.js"),
    ),
    (
        "/mock.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../app/ui/mock.js"),
    ),
    (
        "/boot.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../app/ui/boot.js"),
    ),
    (
        "/manifest.json",
        "application/manifest+json",
        include_bytes!("../../app/ui/manifest.json"),
    ),
    (
        "/icon.svg",
        "image/svg+xml",
        include_bytes!("../../app/ui/icon.svg"),
    ),
    (
        "/fonts/bodoni-moda-latin-wght-normal.woff2",
        "font/woff2",
        include_bytes!("../../app/ui/fonts/bodoni-moda-latin-wght-normal.woff2"),
    ),
    (
        "/fonts/bodoni-moda-latin-wght-italic.woff2",
        "font/woff2",
        include_bytes!("../../app/ui/fonts/bodoni-moda-latin-wght-italic.woff2"),
    ),
    (
        "/fonts/ibm-plex-mono-latin-400-normal.woff2",
        "font/woff2",
        include_bytes!("../../app/ui/fonts/ibm-plex-mono-latin-400-normal.woff2"),
    ),
    (
        "/fonts/ibm-plex-mono-latin-500-normal.woff2",
        "font/woff2",
        include_bytes!("../../app/ui/fonts/ibm-plex-mono-latin-500-normal.woff2"),
    ),
    (
        "/fonts/space-grotesk-latin-wght-normal.woff2",
        "font/woff2",
        include_bytes!("../../app/ui/fonts/space-grotesk-latin-wght-normal.woff2"),
    ),
];

const CSP: &str = "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; \
connect-src 'self'; object-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'";

type Limiter = Arc<Mutex<HashMap<IpAddr, (u32, Instant)>>>;

fn ct_eq(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |d, (x, y)| d | (x ^ y)) == 0
}

struct Request {
    method: String,
    path: String,
    bearer: String,
    body: Vec<u8>,
}

enum ReadError {
    Status(u16),
    Closed,
}

fn read_request(stream: &mut TcpStream) -> Result<Request, ReadError> {
    let mut buf: Vec<u8> = Vec::with_capacity(2048);
    let mut chunk = [0u8; 2048];
    let head_end = loop {
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break i;
        }
        if buf.len() > MAX_HEAD {
            return Err(ReadError::Status(431));
        }
        match stream.read(&mut chunk) {
            Ok(0) => return Err(ReadError::Closed),
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                return Err(ReadError::Status(408))
            }
            Err(_) => return Err(ReadError::Closed),
        }
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
    let mut lines = head.split("\r\n");
    let mut first = lines.next().unwrap_or("").split(' ');
    let (method, target, version) = (
        first.next().unwrap_or(""),
        first.next().unwrap_or(""),
        first.next().unwrap_or(""),
    );
    if method.is_empty() || !target.starts_with('/') || !version.starts_with("HTTP/1.") {
        return Err(ReadError::Status(400));
    }
    let (mut bearer, mut length) = (String::new(), 0usize);
    for l in lines {
        let Some((k, v)) = l.split_once(':') else {
            return Err(ReadError::Status(400));
        };
        let (k, v) = (k.trim().to_ascii_lowercase(), v.trim());
        if k == "authorization" {
            bearer = match v.get(..7) {
                Some(p) if p.eq_ignore_ascii_case("bearer ") => v[7..].trim().to_string(),
                _ => String::new(),
            };
        } else if k == "content-length" {
            length = v.parse().map_err(|_| ReadError::Status(400))?;
        }
    }
    if length > MAX_BODY {
        return Err(ReadError::Status(413));
    }
    let mut body = buf[head_end + 4..].to_vec();
    while body.len() < length {
        match stream.read(&mut chunk) {
            Ok(0) => return Err(ReadError::Status(400)),
            Ok(n) => body.extend_from_slice(&chunk[..n]),
            Err(_) => return Err(ReadError::Status(408)),
        }
    }
    body.truncate(length);
    Ok(Request {
        method: method.to_string(),
        path: target.split('?').next().unwrap_or("/").to_string(),
        bearer,
        body,
    })
}

fn reason(code: u16) -> &'static str {
    match code {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        413 => "Payload Too Large",
        429 => "Too Many Requests",
        431 => "Request Header Fields Too Large",
        _ => "Service Unavailable",
    }
}

fn respond(stream: &mut TcpStream, code: u16, ctype: &str, body: &[u8]) {
    let head = format!(
        "HTTP/1.1 {code} {}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n\
X-Content-Type-Options: nosniff\r\nX-Frame-Options: DENY\r\nReferrer-Policy: no-referrer\r\nCross-Origin-Resource-Policy: same-origin\r\n\
Content-Security-Policy: {CSP}\r\n\r\n",
        reason(code),
        body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body);
    let _ = stream.flush();
}

fn json_reply(stream: &mut TcpStream, code: u16, v: Value) {
    respond(stream, code, "application/json", v.to_string().as_bytes());
}

fn locked_out(limiter: &Limiter, ip: IpAddr) -> bool {
    let map = limiter.lock().unwrap_or_else(|e| e.into_inner());
    matches!(map.get(&ip), Some((n, t)) if *n >= LOCKOUT_AFTER && t.elapsed() < LOCKOUT_WINDOW)
}

fn record_failure(limiter: &Limiter, ip: IpAddr) {
    let mut map = limiter.lock().unwrap_or_else(|e| e.into_inner());
    if map.len() > 4096 {
        map.retain(|_, (_, t)| t.elapsed() < LOCKOUT_WINDOW);
    }
    let e = map.entry(ip).or_insert((0, Instant::now()));
    if e.1.elapsed() >= LOCKOUT_WINDOW {
        *e = (0, Instant::now());
    }
    e.0 += 1;
}

fn handle(mut stream: TcpStream, core: &Arc<Core>, token: &str, limiter: &Limiter) {
    let _ = stream.set_read_timeout(Some(IO_TIMEOUT));
    let _ = stream.set_write_timeout(Some(IO_TIMEOUT));
    let ip = stream
        .peer_addr()
        .map(|a| a.ip())
        .unwrap_or(IpAddr::from([0, 0, 0, 0]));
    let req = match read_request(&mut stream) {
        Ok(r) => r,
        Err(ReadError::Status(c)) => return json_reply(&mut stream, c, json!({"ok": false})),
        Err(ReadError::Closed) => return,
    };
    match (req.method.as_str(), req.path.as_str()) {
        ("GET", p) => match FILES.iter().find(|(path, _, _)| *path == p) {
            Some((_, ctype, bytes)) => respond(&mut stream, 200, ctype, bytes),
            None => json_reply(&mut stream, 404, json!({"ok": false, "error": "not found"})),
        },
        ("POST", "/api/rpc") => {
            if locked_out(limiter, ip) {
                return json_reply(
                    &mut stream,
                    429,
                    json!({"ok": false, "error": "too many attempts"}),
                );
            }
            if token.is_empty() || !ct_eq(&req.bearer, token) {
                record_failure(limiter, ip);
                return json_reply(
                    &mut stream,
                    401,
                    json!({"ok": false, "error": "unauthorized"}),
                );
            }
            let v: Value = serde_json::from_slice(&req.body).unwrap_or_default();
            match core.rpc_public(v["cmd"].as_str().unwrap_or(""), &v["args"]) {
                Ok(d) => json_reply(&mut stream, 200, json!({"ok": true, "data": d})),
                Err(e) => json_reply(
                    &mut stream,
                    400,
                    json!({"ok": false, "error": e.to_string()}),
                ),
            }
        }
        (_, "/api/rpc") => json_reply(&mut stream, 405, json!({"ok": false})),
        _ => json_reply(&mut stream, 404, json!({"ok": false, "error": "not found"})),
    }
}

/// Loopback, private (RFC 1918), link-local, shared-address (100.64/10, used by Tailscale) and IPv6 local ranges.
/// The hub listens on every interface so a phone can reach it, but it answers only peers on such networks, so a
/// laptop on a public or hotspot interface is not exposed to the wider internet.
pub fn local_peer(ip: std::net::IpAddr) -> bool {
    use std::net::IpAddr;
    match ip {
        IpAddr::V4(v) => {
            let o = v.octets();
            v.is_loopback()
                || v.is_private()
                || v.is_link_local()
                || (o[0] == 100 && (64..=127).contains(&o[1]))
        }
        IpAddr::V6(v) => {
            if let Some(m) = v.to_ipv4_mapped() {
                return local_peer(IpAddr::V4(m));
            }
            let seg = v.segments();
            v.is_loopback() || (seg[0] & 0xfe00) == 0xfc00 || (seg[0] & 0xffc0) == 0xfe80
        }
    }
}

fn bind(port: u16, stop: &AtomicBool) -> Option<TcpListener> {
    // A previous hub thread may still hold the port for a moment after a settings save.
    for _ in 0..60 {
        if stop.load(Ordering::Relaxed) {
            return None;
        }
        if let Ok(l) = TcpListener::bind(("0.0.0.0", port)) {
            return Some(l);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    None
}

pub fn serve(core: Arc<Core>, stop: Arc<AtomicBool>) {
    let s = core.settings();
    let Some(listener) = bind(s.hub.port, &stop) else {
        if let Ok(st) = core.store() {
            let _ = st.log(&format!(
                "phone hub could not listen on port {}",
                s.hub.port
            ));
        }
        return;
    };
    let _ = listener.set_nonblocking(true);
    let token: Arc<str> = s.hub.token.into();
    let limiter: Limiter = Arc::new(Mutex::new(HashMap::new()));
    let active = Arc::new(AtomicUsize::new(0));
    while !stop.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, peer)) => {
                if !local_peer(peer.ip()) {
                    continue; // not a local network: drop the connection
                }
                if active.load(Ordering::Relaxed) >= MAX_CONNECTIONS {
                    continue; // dropping the stream refuses the connection
                }
                let _ = stream.set_nonblocking(false);
                active.fetch_add(1, Ordering::Relaxed);
                let (core, token, limiter, active) =
                    (core.clone(), token.clone(), limiter.clone(), active.clone());
                std::thread::spawn(move || {
                    handle(stream, &core, &token, &limiter);
                    active.fetch_sub(1, Ordering::Relaxed);
                });
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(30))
            }
            Err(_) => std::thread::sleep(Duration::from_millis(100)),
        }
    }
}
