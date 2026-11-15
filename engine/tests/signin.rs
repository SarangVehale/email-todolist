//! Sign in with a Google or Microsoft account (docs/designs/phase12.md, ADR-0015): the loopback flow, the device-code flow, the tokens, XOAUTH2 and what
//! is saved. The provider is a scripted local server and the mail server a scripted IMAP one; nothing leaves the computer.
use base64::Engine as _;
use engine::core::Core;
use engine::oauth::{self, Endpoints, Provider};
use engine::settings::{AccountCfg, Settings};
use engine::{secrets, watcher, webpage};
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// These tests share the flow, the token store and the endpoints, which are one each per process.
static SERIAL: Mutex<()> = Mutex::new(());
/// How long the scripted mail server waits before it answers a sign-in, in milliseconds (0 unless a test wants a slow one).
static IMAP_DELAY_MS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// How many sign-ins the scripted mail server has been asked for (counted before its pause), so a test can wait until one is under way.
static IMAP_ASKED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

// ───────────────────────── the scripted token server ─────────────────────────

type Script = HashMap<String, VecDeque<(u16, String, u64)>>;
/// The requests a server has seen (path and body), shared with the thread that serves them.
type Seen = Arc<Mutex<Vec<(String, String)>>>;
type Lines = Arc<Mutex<Vec<String>>>;
struct Tok {
    addr: SocketAddr,
    seen: Arc<Mutex<Vec<(String, String)>>>,
    script: Arc<Mutex<Script>>,
}
impl Tok {
    fn start() -> Tok {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        let (seen, script): (Seen, Arc<Mutex<Script>>) = Default::default();
        let (s2, c2) = (seen.clone(), script.clone());
        std::thread::spawn(move || {
            for conn in l.incoming().flatten() {
                let (seen, script) = (s2.clone(), c2.clone());
                std::thread::spawn(move || serve_http(conn, &seen, &script));
            }
        });
        Tok { addr, seen, script }
    }
    fn say(&self, path: &str, status: u16, body: Value) {
        self.script
            .lock()
            .unwrap()
            .entry(path.into())
            .or_default()
            .push_back((status, body.to_string(), 0));
    }
    fn say_slowly(&self, path: &str, status: u16, body: Value, ms: u64) {
        self.script
            .lock()
            .unwrap()
            .entry(path.into())
            .or_default()
            .push_back((status, body.to_string(), ms));
    }
    /// Waits until the server has been asked on `path` (the answer may still be on its way: some are slow on purpose), so that a test acts at a known moment
    /// and not after a guess about how fast this computer is.
    fn wait_for(&self, path: &str) {
        let until = Instant::now() + Duration::from_secs(10);
        while self.bodies(path).is_empty() {
            assert!(Instant::now() < until, "{path} was never asked");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    fn bodies(&self, path: &str) -> Vec<String> {
        self.seen
            .lock()
            .unwrap()
            .iter()
            .filter(|(p, _)| p == path)
            .map(|(_, b)| b.clone())
            .collect()
    }
    fn endpoints(&self) -> Endpoints {
        let a = self.addr;
        Endpoints {
            google_auth: format!("http://{a}/gauth"),
            google_token: format!("http://{a}/gtoken"),
            google_revoke: format!("http://{a}/grevoke"),
            ms_device: format!("http://{a}/msdevice"),
            ms_token: format!("http://{a}/mstoken"),
            google_id: "gid".into(),
            google_secret: "gsecret".into(),
            ms_id: "msid".into(),
        }
    }
}
fn serve_http(mut c: TcpStream, seen: &Mutex<Vec<(String, String)>>, script: &Mutex<Script>) {
    let _ = c.set_read_timeout(Some(Duration::from_secs(5)));
    let mut r = BufReader::new(c.try_clone().unwrap());
    let mut line = String::new();
    if r.read_line(&mut line).unwrap_or(0) == 0 {
        return;
    }
    let path = line.split(' ').nth(1).unwrap_or("").to_string();
    let mut len = 0usize;
    loop {
        let mut h = String::new();
        if r.read_line(&mut h).unwrap_or(0) == 0 || h == "\r\n" {
            break;
        }
        if let Some(v) = h.to_ascii_lowercase().strip_prefix("content-length:") {
            len = v.trim().parse().unwrap_or(0);
        }
    }
    let mut body = vec![0u8; len];
    let _ = r.read_exact(&mut body);
    seen.lock()
        .unwrap()
        .push((path.clone(), String::from_utf8_lossy(&body).into_owned()));
    let (status, text, delay) = {
        let mut s = script.lock().unwrap();
        match s.get_mut(&path) {
            Some(q) if q.len() > 1 => q.pop_front().unwrap(),
            Some(q) if q.len() == 1 => q.front().cloned().unwrap(),
            _ => (404, "{\"error\":\"invalid_request\"}".into(), 0),
        }
    };
    std::thread::sleep(Duration::from_millis(delay));
    let _ = c.write_all(format!("HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}", text.len()).as_bytes());
}

// ───────────────────────── the scripted IMAP server (XOAUTH2) ─────────────────────────

struct Imap {
    addr: SocketAddr,
    /// `user|token` of every AUTHENTICATE received, in order.
    log: Arc<Mutex<Vec<String>>>,
    good: Arc<Mutex<Vec<String>>>,
}
fn imap_start(good: &[&str]) -> Imap {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap();
    let log: Lines = Default::default();
    let good: Lines = Arc::new(Mutex::new(good.iter().map(|s| s.to_string()).collect()));
    let (l2, g2) = (log.clone(), good.clone());
    std::thread::spawn(move || {
        for conn in l.incoming().flatten() {
            let (log, good) = (l2.clone(), g2.clone());
            std::thread::spawn(move || imap_conn(conn, &log, &good));
        }
    });
    Imap { addr, log, good }
}
fn imap_conn(mut c: TcpStream, log: &Mutex<Vec<String>>, good: &Mutex<Vec<String>>) {
    let _ = c.set_read_timeout(Some(Duration::from_secs(5)));
    let mut r = BufReader::new(c.try_clone().unwrap());
    let _ = c.write_all(b"* OK ready\r\n");
    loop {
        let mut line = String::new();
        if r.read_line(&mut line).unwrap_or(0) == 0 {
            return;
        }
        let l = line.trim_end();
        let (tag, rest) = l.split_once(' ').unwrap_or((l, ""));
        match rest.split(' ').next().unwrap_or("").to_uppercase().as_str() {
            "CAPABILITY" => {
                let _ = c.write_all(
                    format!("* CAPABILITY IMAP4rev1 AUTH=XOAUTH2\r\n{tag} OK done\r\n").as_bytes(),
                );
            }
            "AUTHENTICATE" => {
                let _ = c.write_all(b"+ \r\n");
                let mut resp = String::new();
                let _ = r.read_line(&mut resp);
                IMAP_ASKED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                std::thread::sleep(Duration::from_millis(
                    IMAP_DELAY_MS.load(std::sync::atomic::Ordering::Relaxed),
                ));
                let raw = base64::engine::general_purpose::STANDARD
                    .decode(resp.trim())
                    .unwrap_or_default();
                let raw = String::from_utf8_lossy(&raw).into_owned();
                let user = raw
                    .strip_prefix("user=")
                    .and_then(|s| s.split('\x01').next())
                    .unwrap_or("")
                    .to_string();
                let token = raw
                    .split("auth=Bearer ")
                    .nth(1)
                    .and_then(|s| s.split('\x01').next())
                    .unwrap_or("")
                    .to_string();
                log.lock().unwrap().push(format!("{user}|{token}"));
                if good.lock().unwrap().contains(&token) {
                    let _ = c.write_all(format!("{tag} OK AUTHENTICATE completed\r\n").as_bytes());
                } else {
                    let err = base64::engine::general_purpose::STANDARD.encode("{\"status\":\"401\",\"schemes\":\"Bearer\",\"scope\":\"https://mail.google.com/\"}");
                    let _ = c.write_all(format!("+ {err}\r\n").as_bytes());
                    let mut empty = String::new();
                    let _ = r.read_line(&mut empty);
                    let _ = c.write_all(
                        format!("{tag} NO [AUTHENTICATIONFAILED] Invalid credentials\r\n")
                            .as_bytes(),
                    );
                }
            }
            "LOGOUT" => {
                let _ = c.write_all(format!("* BYE\r\n{tag} OK bye\r\n").as_bytes());
                return;
            }
            _ => {
                let _ = c.write_all(format!("{tag} BAD not here\r\n").as_bytes());
            }
        }
    }
}

// ───────────────────────── helpers ─────────────────────────

fn fresh(name: &str) -> (Arc<Core>, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("ia-signin-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    (Core::new(dir.clone(), Arc::new(|_, _| {})), dir)
}
/// Sets up the endpoints and a clean token store, and puts them all back at the end.
struct Scene {
    _g: std::sync::MutexGuard<'static, ()>,
}
fn scene(tok: Option<&Tok>) -> Scene {
    let g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    oauth::forget_all();
    secrets::delete_all();
    assert!(oauth::use_test_endpoints(Some(
        tok.map_or_else(no_ids, Tok::endpoints)
    )));
    Scene { _g: g }
}
impl Drop for Scene {
    fn drop(&mut self) {
        IMAP_DELAY_MS.store(0, std::sync::atomic::Ordering::Relaxed);
        oauth::use_test_endpoints(None);
        watcher::use_plain_server(None);
        oauth::forget_all();
        secrets::delete_all();
    }
}
/// Endpoints with no client ID: what a build that has not been registered with Google or Microsoft looks like (whatever the constants in `oauth.rs` are).
fn no_ids() -> Endpoints {
    Endpoints {
        google_auth: String::new(),
        google_token: String::new(),
        google_revoke: String::new(),
        ms_device: String::new(),
        ms_token: String::new(),
        google_id: String::new(),
        google_secret: String::new(),
        ms_id: String::new(),
    }
}
fn http_get(addr: &str, target: &str) -> (u16, String) {
    let mut s = TcpStream::connect(addr).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(8))).unwrap();
    s.write_all(
        format!("GET {target} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n").as_bytes(),
    )
    .unwrap();
    let mut out = String::new();
    let _ = s.read_to_string(&mut out);
    let status = out
        .split(' ')
        .nth(1)
        .and_then(|c| c.parse().ok())
        .unwrap_or(0);
    (status, out)
}
fn query_of(url: &str) -> HashMap<String, String> {
    let q = url.split_once('?').map_or("", |x| x.1);
    q.split('&')
        .filter_map(|kv| kv.split_once('='))
        .map(|(k, v)| {
            let mut out = Vec::new();
            let b = v.as_bytes();
            let mut i = 0;
            while i < b.len() {
                if b[i] == b'%' && i + 2 < b.len() {
                    out.push(u8::from_str_radix(&v[i + 1..i + 3], 16).unwrap());
                    i += 3;
                } else {
                    out.push(b[i]);
                    i += 1;
                }
            }
            (k.to_string(), String::from_utf8(out).unwrap())
        })
        .collect()
}
fn wait_state(c: &Arc<Core>, want: &str) -> Value {
    let until = Instant::now() + Duration::from_secs(12);
    loop {
        let p = c.rpc("oauth_poll", &json!({})).unwrap();
        if p["state"] == want {
            return p;
        }
        assert!(
            Instant::now() < until,
            "still {p} after 12 seconds, wanted {want}"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}
/// Starts a Google sign-in and plays the browser: returns what the browser was sent to, and answers the callback with `callback(redirect, state)`.
fn google_round(
    c: &Arc<Core>,
    user: &str,
    callback: impl Fn(&str, &str) -> String,
) -> (HashMap<String, String>, (u16, String)) {
    let before = webpage::opened().len();
    c.rpc("oauth_start", &json!({"provider": "google", "user": user}))
        .unwrap();
    let url = webpage::opened()
        .into_iter()
        .nth(before)
        .expect("the browser was sent to Google's page");
    let q = query_of(&url);
    let redirect = q["redirect_uri"].clone();
    let answer = http_get(
        redirect.trim_start_matches("http://"),
        &callback(&redirect, &q["state"]),
    );
    (q, answer)
}
fn tokens(access: &str, refresh: Option<&str>, scope: &str) -> Value {
    let mut v =
        json!({"access_token": access, "expires_in": 3600, "scope": scope, "token_type": "Bearer"});
    if let Some(r) = refresh {
        v["refresh_token"] = json!(r);
    }
    v
}
const MAIL: &str = "https://mail.google.com/";

// ───────────────────────── the pieces ─────────────────────────

#[test]
fn sha256_and_the_pkce_challenge_match_the_published_test_values() {
    let hex = |d: [u8; 32]| d.iter().map(|b| format!("{b:02x}")).collect::<String>();
    assert_eq!(
        hex(oauth::sha256(b"")),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        hex(oauth::sha256(b"abc")),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        hex(oauth::sha256(
            b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"
        )),
        "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
    );
    // a message that needs a second block for its length (55, 56 and 64 bytes are the edges of the padding)
    assert_eq!(
        hex(oauth::sha256(&[b'a'; 56])),
        "b35439a4ac6f0948b6d6f9e3c6af0f5f590ce20f1bde7090ef7970686ec6738a"
    );
    // RFC 7636, appendix B
    assert_eq!(
        oauth::pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
        "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
    );
}

#[test]
fn a_build_without_a_client_id_offers_no_sign_in_and_refuses_to_start_one() {
    let _s = scene(None);
    let (c, dir) = fresh("noid");
    let st = c.rpc("state", &json!({})).unwrap();
    assert_eq!(st["sign_in"], json!({"google": false, "microsoft": false}));
    for p in ["google", "microsoft"] {
        let e = c
            .rpc(
                "oauth_start",
                &json!({"provider": p, "user": "me@example.com"}),
            )
            .unwrap_err()
            .to_string();
        assert!(e.contains("not set up in this build"), "{e}");
    }
    assert!(c
        .rpc(
            "oauth_start",
            &json!({"provider": "yahoo", "user": "me@example.com"})
        )
        .unwrap_err()
        .to_string()
        .contains("isn't known"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn google_sign_in_runs_the_loopback_flow_with_pkce_and_saves_a_mailbox_that_signs_in_with_xoauth2()
{
    let tok = Tok::start();
    tok.say("/gtoken", 200, tokens("AT1", Some("RT1"), MAIL));
    let imap = imap_start(&["AT1"]);
    let _s = scene(Some(&tok));
    watcher::use_plain_server(Some(imap.addr));
    let (c, dir) = fresh("google");
    let st = c.rpc("state", &json!({})).unwrap();
    assert_eq!(st["sign_in"], json!({"google": true, "microsoft": true}));

    let (q, answer) = google_round(&c, "me@gmail.com", |_, state| {
        format!("/?code=CODE1&state={state}")
    });
    assert_eq!(answer.0, 200);
    assert!(answer.1.contains("You can close this tab"), "a fixed page");
    assert!(
        answer.1.contains("Referrer-Policy: no-referrer")
            && answer.1.contains("Cache-Control: no-store")
    );
    // what the browser was sent to
    assert_eq!(q["client_id"], "gid");
    assert_eq!(q["response_type"], "code");
    assert_eq!(q["scope"], MAIL);
    assert_eq!(q["code_challenge_method"], "S256");
    assert_eq!(q["access_type"], "offline");
    assert_eq!(q["login_hint"], "me@gmail.com");
    assert!(
        q["redirect_uri"].starts_with("http://127.0.0.1:"),
        "{}",
        q["redirect_uri"]
    );
    assert!(q["state"].len() >= 32);

    let done = wait_state(&c, "done");
    assert_eq!(
        done,
        json!({"state": "done", "user": "me@gmail.com", "provider": "google"}),
        "the answer holds no token"
    );
    // what Google was asked for the code
    let body = tok.bodies("/gtoken").remove(0);
    let form = query_of(&format!("?{body}"));
    assert_eq!(form["grant_type"], "authorization_code");
    assert_eq!(form["code"], "CODE1");
    assert_eq!(form["client_id"], "gid");
    assert_eq!(form["client_secret"], "gsecret");
    assert_eq!(form["redirect_uri"], q["redirect_uri"]);
    assert_eq!(
        oauth::pkce_challenge(&form["code_verifier"]),
        q["code_challenge"],
        "the verifier is the one the challenge was made from"
    );

    // the first mailbox is saved only once the mail server has accepted the token
    secrets::set("imap-password", "an old password").unwrap();
    assert_eq!(c.rpc("oauth_finish", &json!({})).unwrap(), json!(0));
    assert_eq!(
        imap.log.lock().unwrap().clone(),
        vec!["me@gmail.com|AT1"],
        "XOAUTH2 with the user name and the token"
    );
    let s = Settings::load(&dir);
    assert_eq!(
        (
            s.account.host.as_str(),
            s.account.user.as_str(),
            s.account.port
        ),
        ("imap.gmail.com", "me@gmail.com", 993)
    );
    assert_eq!(s.oauth.get(&0).map(String::as_str), Some("google"));
    assert_eq!(secrets::get("oauth-refresh-0").as_deref(), Some("RT1"));
    assert!(
        secrets::get("imap-password").is_none(),
        "the sign-in replaced the password the mailbox had"
    );
    assert!(
        c.rpc("finish_setup", &json!({})).is_ok(),
        "setup can finish with a sign-in in place of a password"
    );
    let st = c.rpc("state", &json!({})).unwrap();
    assert_eq!(st["accounts"][0]["auth"], "google");
    // nothing secret is anywhere a window, a log or a bug report can see
    let everything = format!("{st}{}", c.rpc("diagnostics", &json!({})).unwrap());
    for secret in ["AT1", "RT1", "gsecret"] {
        assert!(
            !everything.contains(secret),
            "{secret} is in the state or the diagnostics"
        );
    }
    assert_eq!(
        c.rpc("oauth_poll", &json!({})).unwrap()["state"],
        "idle",
        "a finished sign-in is used once"
    );
    assert!(c.rpc("oauth_finish", &json!({})).is_err());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_listener_goes_on_waiting_through_requests_that_are_not_the_answer() {
    let tok = Tok::start();
    tok.say("/gtoken", 200, tokens("AT1", Some("RT1"), MAIL));
    let _s = scene(Some(&tok));
    let (c, dir) = fresh("listener");
    let before = webpage::opened().len();
    c.rpc(
        "oauth_start",
        &json!({"provider": "google", "user": "me@gmail.com"}),
    )
    .unwrap();
    let q = query_of(&webpage::opened()[before]);
    let at = q["redirect_uri"].trim_start_matches("http://").to_string();
    // a stray page, a wrong state, another path and a post: all 404, none starts the exchange, and the flow is still waiting
    assert_eq!(http_get(&at, "/").0, 404);
    assert_eq!(http_get(&at, "/?code=EVIL&state=wrong").0, 404);
    assert_eq!(
        http_get(&at, &format!("/other?code=EVIL&state={}", q["state"])).0,
        404
    );
    assert_eq!(
        http_get(&at, &format!("/?code=EVIL&state={}x", q["state"])).0,
        404,
        "the state has to match whole"
    );
    {
        let mut s = TcpStream::connect(&at).unwrap();
        s.write_all(
            format!(
                "POST /?code=EVIL&state={} HTTP/1.1\r\nContent-Length: 0\r\n\r\n",
                q["state"]
            )
            .as_bytes(),
        )
        .unwrap();
        let mut out = String::new();
        s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let _ = s.read_to_string(&mut out);
        assert!(out.starts_with("HTTP/1.1 404"), "{out}");
    }
    {
        // a request over 8 KB is dropped
        let mut s = TcpStream::connect(&at).unwrap();
        // with the right state, so that only the size can be what refuses it
        let _ = s.write_all(
            format!(
                "GET /?x={}&code=EVIL&state={} HTTP/1.1\r\n\r\n",
                "a".repeat(9000),
                q["state"]
            )
            .as_bytes(),
        );
        let mut out = String::new();
        s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let _ = s.read_to_string(&mut out);
        assert!(
            !out.contains("200 OK"),
            "an over-long request was answered: {out}"
        );
    }
    assert!(
        tok.bodies("/gtoken").is_empty(),
        "no code was exchanged for any of them"
    );
    assert_eq!(c.rpc("oauth_poll", &json!({})).unwrap()["state"], "waiting");
    // a browser's idle spare connection does not stop the real one for long (5 seconds at most)
    let _idle = TcpStream::connect(&at).unwrap();
    let started = Instant::now();
    assert_eq!(
        http_get(&at, &format!("/?code=CODE1&state={}", q["state"])).0,
        200
    );
    assert!(
        started.elapsed() < Duration::from_secs(9),
        "took {:?}",
        started.elapsed()
    );
    wait_state(&c, "done");
    assert_eq!(tok.bodies("/gtoken").len(), 1);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_refusal_a_missing_permission_and_a_missing_refresh_token_are_sentences_not_a_half_saved_mailbox(
) {
    let tok = Tok::start();
    let _s = scene(Some(&tok));
    let (c, dir) = fresh("fails");
    // the person pressed Cancel on Google's page
    let (_, a) = google_round(&c, "me@gmail.com", |_, st| {
        format!("/?error=access_denied&state={st}")
    });
    assert_eq!(a.0, 200);
    let f = wait_state(&c, "failed");
    assert_eq!(
        f["error"],
        "You chose not to allow Inbox Agent, so nothing was connected."
    );
    // the person unticked the permission to read mail
    tok.say("/gtoken", 200, tokens("AT1", Some("RT1"), "openid email"));
    google_round(&c, "me@gmail.com", |_, st| format!("/?code=C&state={st}"));
    let f = wait_state(&c, "failed");
    assert!(
        f["error"]
            .as_str()
            .unwrap()
            .starts_with("Mail access was not allowed"),
        "{f}"
    );
    // Google gave no refresh token
    tok.script.lock().unwrap().clear();
    tok.say("/gtoken", 200, tokens("AT1", None, MAIL));
    google_round(&c, "me@gmail.com", |_, st| format!("/?code=C&state={st}"));
    let f = wait_state(&c, "failed");
    assert!(
        f["error"]
            .as_str()
            .unwrap()
            .contains("did not give Inbox Agent lasting access"),
        "{f}"
    );
    // Google refused the app's registration; its own words are not kept
    tok.script.lock().unwrap().clear();
    tok.say(
        "/gtoken",
        401,
        json!({"error": "invalid_client", "error_description": "secret-looking text me@gmail.com"}),
    );
    google_round(&c, "me@gmail.com", |_, st| format!("/?code=C&state={st}"));
    let f = wait_state(&c, "failed");
    assert!(
        f["error"]
            .as_str()
            .unwrap()
            .contains("did not accept this app's sign-in settings"),
        "{f}"
    );
    assert!(
        !f.to_string().contains("secret-looking"),
        "the provider's text is not kept"
    );
    // nothing was finished or saved by any of it
    assert!(c.rpc("oauth_finish", &json!({})).is_err());
    assert!(Settings::load(&dir).account.host.is_empty());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn starting_again_ends_the_first_sign_in_and_cancel_ends_the_one_waiting() {
    let tok = Tok::start();
    let _s = scene(Some(&tok));
    let (c, dir) = fresh("again");
    let before = webpage::opened().len();
    c.rpc(
        "oauth_start",
        &json!({"provider": "google", "user": "me@gmail.com"}),
    )
    .unwrap();
    let first = query_of(&webpage::opened()[before])["redirect_uri"]
        .trim_start_matches("http://")
        .to_string();
    c.rpc(
        "oauth_start",
        &json!({"provider": "google", "user": "me@gmail.com"}),
    )
    .unwrap();
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        TcpStream::connect_timeout(&first.parse().unwrap(), Duration::from_millis(500)).is_err(),
        "the first listener is closed"
    );
    assert_eq!(c.rpc("oauth_poll", &json!({})).unwrap()["state"], "waiting");
    assert_eq!(c.rpc("oauth_cancel", &json!({})).unwrap(), json!(true));
    assert_eq!(c.rpc("oauth_poll", &json!({})).unwrap()["state"], "idle");
    let second = query_of(&webpage::opened()[before + 1])["redirect_uri"]
        .trim_start_matches("http://")
        .to_string();
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        TcpStream::connect_timeout(&second.parse().unwrap(), Duration::from_millis(500)).is_err(),
        "the second is closed too"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn microsoft_sign_in_shows_a_code_waits_is_told_to_slow_down_and_finishes() {
    let tok = Tok::start();
    tok.say("/msdevice", 200, json!({"device_code": "DC", "user_code": "ABCD-EFGH", "verification_uri": "https://www.microsoft.com/link", "expires_in": 900, "interval": 1}));
    tok.say("/mstoken", 400, json!({"error": "authorization_pending"}));
    tok.say("/mstoken", 400, json!({"error": "slow_down"}));
    tok.say(
        "/mstoken",
        200,
        tokens(
            "MT1",
            Some("MR1"),
            "https://outlook.office.com/IMAP.AccessAsUser.All",
        ),
    );
    let imap = imap_start(&["MT1"]);
    let _s = scene(Some(&tok));
    watcher::use_plain_server(Some(imap.addr));
    let (c, dir) = fresh("ms");
    let r = c
        .rpc(
            "oauth_start",
            &json!({"provider": "microsoft", "user": "me@outlook.com"}),
        )
        .unwrap();
    assert_eq!(
        r,
        json!({"kind": "microsoft", "code": "ABCD-EFGH", "url": "https://www.microsoft.com/link"})
    );
    assert!(
        webpage::opened().contains(&"https://www.microsoft.com/link".to_string()),
        "the page where the code is typed was opened"
    );
    let waiting = c.rpc("oauth_poll", &json!({})).unwrap();
    assert_eq!(
        waiting,
        json!({"state": "waiting", "code": "ABCD-EFGH", "url": "https://www.microsoft.com/link"})
    );
    wait_state(&c, "done");
    let asks = tok.bodies("/mstoken");
    assert_eq!(asks.len(), 3, "asked until it said yes");
    assert!(
        asks[0].contains("device_code=DC")
            && asks[0]
                .contains("grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Adevice_code")
            && asks[0].contains("client_id=msid"),
        "{}",
        asks[0]
    );
    assert!(
        tok.bodies("/msdevice")[0].contains("offline_access"),
        "a lasting sign-in was asked for"
    );
    assert_eq!(c.rpc("oauth_finish", &json!({})).unwrap(), json!(0));
    let s = Settings::load(&dir);
    assert_eq!(
        (s.account.host.as_str(), s.account.user.as_str()),
        ("outlook.office365.com", "me@outlook.com")
    );
    assert_eq!(s.oauth.get(&0).map(String::as_str), Some("microsoft"));
    assert_eq!(secrets::get("oauth-refresh-0").as_deref(), Some("MR1"));
    assert_eq!(imap.log.lock().unwrap().clone(), vec!["me@outlook.com|MT1"]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn microsoft_sign_in_ends_with_a_sentence_when_the_code_is_declined_or_runs_out() {
    let tok = Tok::start();
    tok.say("/msdevice", 200, json!({"device_code": "DC", "user_code": "ABCD-EFGH", "verification_uri": "https://www.microsoft.com/link", "expires_in": 900, "interval": 1}));
    let _s = scene(Some(&tok));
    let (c, dir) = fresh("ms-fail");
    tok.say("/mstoken", 400, json!({"error": "authorization_declined"}));
    c.rpc(
        "oauth_start",
        &json!({"provider": "microsoft", "user": "me@outlook.com"}),
    )
    .unwrap();
    assert_eq!(
        wait_state(&c, "failed")["error"],
        "You chose not to allow Inbox Agent, so nothing was connected."
    );
    tok.script.lock().unwrap().remove("/mstoken");
    tok.say("/mstoken", 400, json!({"error": "expired_token"}));
    c.rpc(
        "oauth_start",
        &json!({"provider": "microsoft", "user": "me@outlook.com"}),
    )
    .unwrap();
    assert_eq!(
        wait_state(&c, "failed")["error"],
        "The sign-in took too long. Try again."
    );
    // Microsoft cannot be reached at all
    tok.script.lock().unwrap().remove("/msdevice");
    let e = c
        .rpc(
            "oauth_start",
            &json!({"provider": "microsoft", "user": "me@outlook.com"}),
        )
        .unwrap_err()
        .to_string();
    assert!(
        e.contains("did not finish the sign-in") || e.contains("did not accept"),
        "{e}"
    );
    assert_eq!(
        c.rpc("oauth_poll", &json!({})).unwrap()["state"],
        "idle",
        "a flow that never started is not left waiting"
    );
    let _ = std::fs::remove_dir_all(dir);
}

fn oauth_mailbox(dir: &std::path::Path, id: u32, host: &str, user: &str, kind: &str) {
    let mut s = Settings::load(dir);
    let cfg = AccountCfg {
        id,
        host: host.into(),
        port: 993,
        user: user.into(),
    };
    if id == 0 {
        s.account = cfg;
    } else {
        s.more_accounts.push(cfg);
    }
    s.oauth.insert(id, kind.into());
    s.onboarded = true;
    s.save(dir).unwrap();
}

#[test]
fn many_callers_share_one_refresh_and_a_new_refresh_token_replaces_the_old_one() {
    let tok = Tok::start();
    tok.say_slowly(
        "/mstoken",
        200,
        json!({"access_token": "MT-new", "refresh_token": "MR-rotated", "expires_in": 3600}),
        300,
    );
    let _s = scene(Some(&tok));
    secrets::set("oauth-refresh-0", "MR-old").unwrap();
    let handles: Vec<_> = (0..8)
        .map(|_| std::thread::spawn(|| oauth::access_token(0, Provider::Microsoft).unwrap()))
        .collect();
    for h in handles {
        assert_eq!(h.join().unwrap(), "MT-new");
    }
    let asks = tok.bodies("/mstoken");
    assert_eq!(asks.len(), 1, "eight callers, one request to Microsoft");
    assert!(
        asks[0].contains("grant_type=refresh_token") && asks[0].contains("refresh_token=MR-old"),
        "{}",
        asks[0]
    );
    assert_eq!(
        secrets::get("oauth-refresh-0").as_deref(),
        Some("MR-rotated"),
        "Microsoft hands out a new one each time"
    );
    // the token in memory is used while it has time left
    assert_eq!(
        oauth::access_token(0, Provider::Microsoft).unwrap(),
        "MT-new"
    );
    assert_eq!(tok.bodies("/mstoken").len(), 1);
}

#[test]
fn a_withdrawn_sign_in_is_asked_about_once_and_then_left_alone_until_the_person_signs_in_again() {
    let tok = Tok::start();
    tok.say("/gtoken", 400, json!({"error": "invalid_grant"}));
    let _s = scene(Some(&tok));
    secrets::set("oauth-refresh-0", "RT").unwrap();
    for _ in 0..4 {
        let e = oauth::access_token(0, Provider::Google).unwrap_err();
        assert!(
            watcher::is_sign_in_problem(&e),
            "the watcher treats it as a refused sign-in: {e}"
        );
        assert!(e.to_string().contains("no longer valid"), "{e}");
    }
    assert_eq!(
        tok.bodies("/gtoken").len(),
        1,
        "four tries, one request: the watcher's half-minute retry does not hammer Google"
    );
    // signing in again (a new token for the mailbox) clears it
    oauth::prime(0, "AT-again", 3600);
    assert_eq!(
        oauth::access_token(0, Provider::Google).unwrap(),
        "AT-again"
    );
}

#[test]
fn a_token_the_server_refuses_is_dropped_and_one_more_is_tried_then_the_sign_in_is_refused() {
    let tok = Tok::start();
    tok.say("/gtoken", 200, tokens("GOOD", None, MAIL));
    let imap = imap_start(&["GOOD"]);
    let _s = scene(Some(&tok));
    secrets::set("oauth-refresh-0", "RT").unwrap();
    oauth::prime(0, "STALE", 3600); // revoked since it was given out
    let a = watcher::Account {
        host: "imap.gmail.com".into(),
        port: 993,
        user: "me@gmail.com".into(),
        password: String::new(),
        oauth: Some((0, Provider::Google)),
    };
    let mut s = watcher::connect_plain_account(imap.addr, &a).expect("the second token works");
    let _ = s.logout();
    assert_eq!(
        imap.log.lock().unwrap().clone(),
        vec!["me@gmail.com|STALE", "me@gmail.com|GOOD"],
        "refused once, then the new token"
    );
    assert_eq!(tok.bodies("/gtoken").len(), 1);
    // the provider keeps giving a token the server refuses: two tries in all, then a refused sign-in (never a loop)
    imap.good.lock().unwrap().clear();
    imap.log.lock().unwrap().clear();
    oauth::drop_cached(0);
    let e = match watcher::connect_plain_account(imap.addr, &a) {
        Ok(_) => panic!("signed in with no good token"),
        Err(e) => e,
    };
    assert!(watcher::is_sign_in_problem(&e), "{e}");
    assert_eq!(
        imap.log.lock().unwrap().len(),
        2,
        "two attempts and no more"
    );
}

#[test]
fn nothing_is_saved_for_a_token_the_mail_server_does_not_accept() {
    let tok = Tok::start();
    tok.say("/gtoken", 200, tokens("BAD", Some("RT1"), MAIL));
    let imap = imap_start(&["something else"]);
    let _s = scene(Some(&tok));
    watcher::use_plain_server(Some(imap.addr));
    let (c, dir) = fresh("refused");
    google_round(&c, "me@gmail.com", |_, st| format!("/?code=C&state={st}"));
    wait_state(&c, "done");
    let e = c.rpc("oauth_finish", &json!({})).unwrap_err();
    assert!(watcher::is_sign_in_problem(&e), "{e}");
    assert!(
        e.to_string().contains("Invalid credentials"),
        "it is the server's own refusal that comes back, not a message about a missing token: {e}"
    );
    assert_eq!(
        imap.log.lock().unwrap().clone(),
        vec!["me@gmail.com|BAD"],
        "one attempt, with the new token"
    );
    let s = Settings::load(&dir);
    assert!(
        s.account.host.is_empty() && s.oauth.is_empty(),
        "no mailbox was saved"
    );
    assert!(
        secrets::get("oauth-refresh-0").is_none(),
        "no token was kept"
    );
    // nor is the refused token left in memory for anything to use
    assert!(
        oauth::access_token(0, Provider::Google).is_err(),
        "no token is held for the mailbox that was not saved"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_sign_in_is_only_ever_for_its_providers_server_and_only_the_sign_in_commands_can_change_it() {
    let _s = scene(None);
    let (c, dir) = fresh("rules");
    let ok = |oauth: &[(u32, &str)], host: &str| {
        let mut s = Settings {
            account: AccountCfg {
                id: 0,
                host: host.into(),
                port: 993,
                user: "me@example.com".into(),
            },
            ..Settings::default()
        };
        for (k, v) in oauth {
            s.oauth.insert(*k, v.to_string());
        }
        s.validate()
    };
    assert!(ok(&[(0, "google")], "imap.gmail.com").is_ok());
    assert!(ok(&[(0, "microsoft")], "outlook.office365.com").is_ok());
    assert!(
        ok(&[(0, "google")], "imap.evil.example").is_err(),
        "a Google token is never sent to another server"
    );
    assert!(ok(&[(0, "microsoft")], "imap.gmail.com").is_err());
    assert!(
        ok(&[(0, "yahoo")], "imap.gmail.com").is_err(),
        "a kind that is not known"
    );
    assert!(
        ok(&[(3, "google")], "imap.gmail.com").is_err(),
        "a sign-in for a mailbox that is not there"
    );
    // a settings change cannot add, change or remove one
    Settings {
        account: AccountCfg {
            id: 0,
            host: "imap.gmail.com".into(),
            port: 993,
            user: "me@gmail.com".into(),
        },
        onboarded: true,
        ..Settings::default()
    }
    .save(&dir)
    .unwrap();
    let e = c
        .rpc(
            "save_settings",
            &json!({"patch": {"oauth": {"0": "google"}}}),
        )
        .unwrap_err()
        .to_string();
    assert!(e.contains("changed with save_account"), "{e}");
    oauth_mailbox(&dir, 0, "imap.gmail.com", "me@gmail.com", "google");
    for patch in [
        json!({"oauth": {"0": "microsoft"}}),
        json!({"oauth": {"1": "google"}}),
        json!({"oauth": {"0": null}}),
        json!({"oauth": null}),
    ] {
        assert!(
            c.rpc("save_settings", &json!({ "patch": patch })).is_err(),
            "{patch} changed a sign-in"
        );
    }
    assert_eq!(
        Settings::load(&dir).oauth.get(&0).map(String::as_str),
        Some("google"),
        "it is as it was"
    );
    // saving a password for the mailbox ends the sign-in
    secrets::set("oauth-refresh-0", "RT").unwrap();
    c.rpc("save_account", &json!({"id": 0, "host": "imap.gmail.com", "user": "me@gmail.com", "password": "abcd efgh ijkl mnop"})).unwrap();
    assert!(
        Settings::load(&dir).oauth.is_empty(),
        "a password replaces the sign-in"
    );
    assert!(secrets::get("oauth-refresh-0").is_none() && secrets::get("imap-password").is_some());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn removing_a_mailbox_or_signing_out_deletes_the_sign_in_and_google_is_told_with_the_token_in_the_body(
) {
    let tok = Tok::start();
    tok.say("/grevoke", 200, json!({}));
    let _s = scene(Some(&tok));
    let (c, dir) = fresh("revoke");
    oauth_mailbox(&dir, 0, "imap.gmail.com", "me@gmail.com", "google");
    secrets::set("oauth-refresh-0", "RT-secret-0").unwrap();
    c.rpc("remove_account", &json!({"id": 0})).unwrap();
    assert!(secrets::get("oauth-refresh-0").is_none());
    assert!(Settings::load(&dir).oauth.is_empty());
    let seen = tok.seen.lock().unwrap().clone();
    assert_eq!(seen.iter().filter(|(p, _)| p == "/grevoke").count(), 1);
    assert_eq!(
        seen.iter().find(|(p, _)| p == "/grevoke").unwrap().1,
        "token=RT-secret-0",
        "in the body, never in the address"
    );
    assert!(
        seen.iter().all(|(p, _)| !p.contains("RT-secret")),
        "no token in any address"
    );
    // signing out does the same for every mailbox
    oauth_mailbox(&dir, 0, "imap.gmail.com", "me@gmail.com", "google");
    secrets::set("oauth-refresh-0", "RT-secret-1").unwrap();
    c.rpc("sign_out", &json!({})).unwrap();
    assert!(secrets::get("oauth-refresh-0").is_none());
    assert!(Settings::load(&dir).oauth.is_empty());
    assert!(tok
        .bodies("/grevoke")
        .contains(&"token=RT-secret-1".to_string()));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_second_mailbox_can_sign_in_with_an_account_and_gets_the_lowest_free_number() {
    let tok = Tok::start();
    tok.say("/gtoken", 200, tokens("AT2", Some("RT2"), MAIL));
    let imap = imap_start(&["AT2"]);
    let _s = scene(Some(&tok));
    watcher::use_plain_server(Some(imap.addr));
    let (c, dir) = fresh("second");
    // it needs a first mailbox
    google_round(&c, "two@gmail.com", |_, st| format!("/?code=C&state={st}"));
    wait_state(&c, "done");
    let e = c
        .rpc("oauth_finish", &json!({"add": true}))
        .unwrap_err()
        .to_string();
    assert!(e.contains("Set up your first mailbox"), "{e}");
    secrets::set("imap-password", "pw").unwrap();
    Settings {
        account: AccountCfg {
            id: 0,
            host: "imap.example.com".into(),
            port: 993,
            user: "one@example.com".into(),
        },
        onboarded: true,
        ..Settings::default()
    }
    .save(&dir)
    .unwrap();
    google_round(&c, "two@gmail.com", |_, st| format!("/?code=C&state={st}"));
    wait_state(&c, "done");
    assert_eq!(
        c.rpc("oauth_finish", &json!({"add": true})).unwrap(),
        json!(1)
    );
    let s = Settings::load(&dir);
    assert_eq!(s.more_accounts.len(), 1);
    assert_eq!(
        (s.more_accounts[0].id, s.more_accounts[0].user.as_str()),
        (1, "two@gmail.com")
    );
    assert_eq!(s.oauth.get(&1).map(String::as_str), Some("google"));
    assert_eq!(secrets::get("oauth-refresh-1").as_deref(), Some("RT2"));
    assert_eq!(
        secrets::get("imap-password").as_deref(),
        Some("pw"),
        "the first mailbox keeps its password"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_phone_hub_cannot_start_poll_cancel_or_finish_a_sign_in() {
    let tok = Tok::start();
    let _s = scene(Some(&tok));
    let (c, dir) = fresh("hub");
    let before = webpage::opened().len();
    for (cmd, args) in [
        (
            "oauth_start",
            json!({"provider": "google", "user": "me@gmail.com"}),
        ),
        ("oauth_poll", json!({})),
        ("oauth_cancel", json!({})),
        ("oauth_finish", json!({})),
    ] {
        assert!(
            c.rpc_public(cmd, &args)
                .unwrap_err()
                .to_string()
                .contains("not available from the hub"),
            "{cmd}"
        );
    }
    assert_eq!(
        webpage::opened().len(),
        before,
        "nothing was opened for the hub"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_sign_in_is_offered_exactly_when_a_client_id_is_set_and_each_provider_has_one_server() {
    // (that the test endpoints are honoured only in a test of this crate is a guard that this file cannot reach: it is in `oauth::use_test_endpoints`)
    let _s = scene(None);
    assert!(!oauth::available(Provider::Google) && !oauth::available(Provider::Microsoft));
    let tok = Tok::start();
    assert!(oauth::use_test_endpoints(Some(tok.endpoints())));
    assert!(oauth::available(Provider::Google) && oauth::available(Provider::Microsoft));
    let mut only_google = no_ids();
    only_google.google_id = "gid".into();
    assert!(oauth::use_test_endpoints(Some(only_google)));
    assert!(oauth::available(Provider::Google) && !oauth::available(Provider::Microsoft));
    assert_eq!(Provider::Google.host(), "imap.gmail.com");
    assert_eq!(Provider::Microsoft.host(), "outlook.office365.com");
}

#[test]
fn a_finished_sign_in_that_is_not_used_is_forgotten_instead_of_waiting_in_memory() {
    let tok = Tok::start();
    tok.say("/gtoken", 200, tokens("AT1", Some("RT1"), MAIL));
    let _s = scene(Some(&tok));
    let (c, dir) = fresh("stale");
    google_round(&c, "me@gmail.com", |_, st| format!("/?code=C&state={st}"));
    wait_state(&c, "done");
    std::thread::sleep(Duration::from_millis(2300));
    assert_eq!(
        c.rpc("oauth_poll", &json!({})).unwrap()["state"],
        "idle",
        "it is no longer offered"
    );
    let e = c.rpc("oauth_finish", &json!({})).unwrap_err().to_string();
    assert!(e.contains("There is no finished sign-in"), "{e}");
    // one that is used in time still works (the tests above), and one that was too late is said to be
    google_round(&c, "me@gmail.com", |_, st| format!("/?code=C&state={st}"));
    wait_state(&c, "done");
    std::thread::sleep(Duration::from_millis(2300));
    let e = c.rpc("oauth_finish", &json!({})).unwrap_err().to_string();
    assert!(e.contains("too long ago"), "{e}");
    let _ = std::fs::remove_dir_all(dir);
}

fn gmail_mailbox_with_a_sign_in(dir: &std::path::Path, refresh: &str) {
    oauth_mailbox(dir, 0, "imap.gmail.com", "me@gmail.com", "google");
    secrets::set("oauth-refresh-0", refresh).unwrap();
}

#[test]
fn signing_in_again_proves_the_new_token_not_the_old_grant_and_keeps_the_old_one_when_it_fails() {
    let tok = Tok::start();
    tok.say("/gtoken", 200, tokens("NEW-BAD", Some("RT-new"), MAIL));
    // the old grant still works: a retry that fell back to it would show in the server's log, and the new token would be saved unproved
    let imap = imap_start(&["OLD-ACCESS"]);
    let _s = scene(Some(&tok));
    watcher::use_plain_server(Some(imap.addr));
    let (c, dir) = fresh("again-bad");
    gmail_mailbox_with_a_sign_in(&dir, "RT-old");
    oauth::prime(0, "OLD-ACCESS", 3600);
    google_round(&c, "me@gmail.com", |_, st| format!("/?code=C&state={st}"));
    wait_state(&c, "done");
    let e = c.rpc("oauth_finish", &json!({"id": 0})).unwrap_err();
    assert!(watcher::is_sign_in_problem(&e), "{e}");
    assert_eq!(
        imap.log.lock().unwrap().clone(),
        vec!["me@gmail.com|NEW-BAD"],
        "the old grant was not tried in its place"
    );
    assert_eq!(
        secrets::get("oauth-refresh-0").as_deref(),
        Some("RT-old"),
        "the old sign-in is as it was"
    );
    assert!(
        tok.bodies("/grevoke").contains(&"token=RT-new".to_string()),
        "the grant that was never used is cancelled at Google"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn signing_in_again_with_a_good_token_replaces_the_grant_and_cancels_the_old_one() {
    let tok = Tok::start();
    tok.say("/gtoken", 200, tokens("NEW-GOOD", Some("RT-new"), MAIL));
    tok.say("/grevoke", 200, json!({}));
    let imap = imap_start(&["NEW-GOOD"]);
    let _s = scene(Some(&tok));
    watcher::use_plain_server(Some(imap.addr));
    let (c, dir) = fresh("again-good");
    gmail_mailbox_with_a_sign_in(&dir, "RT-old");
    google_round(&c, "me@gmail.com", |_, st| format!("/?code=C&state={st}"));
    wait_state(&c, "done");
    assert_eq!(c.rpc("oauth_finish", &json!({"id": 0})).unwrap(), json!(0));
    assert_eq!(secrets::get("oauth-refresh-0").as_deref(), Some("RT-new"));
    assert_eq!(
        tok.bodies("/grevoke"),
        vec!["token=RT-old".to_string()],
        "the old grant is cancelled, with the token in the body"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn what_is_wrong_with_the_mailbox_is_refused_before_the_sign_in_is_used_up() {
    let tok = Tok::start();
    tok.say("/gtoken", 200, tokens("AT1", Some("RT1"), MAIL));
    let imap = imap_start(&["AT1"]);
    let _s = scene(Some(&tok));
    watcher::use_plain_server(Some(imap.addr));
    let (c, dir) = fresh("duplicate");
    // the same address is already a mailbox
    Settings {
        account: AccountCfg {
            id: 0,
            host: "imap.gmail.com".into(),
            port: 993,
            user: "me@gmail.com".into(),
        },
        onboarded: true,
        ..Settings::default()
    }
    .save(&dir)
    .unwrap();
    secrets::set("imap-password", "pw").unwrap();
    google_round(&c, "me@gmail.com", |_, st| format!("/?code=C&state={st}"));
    wait_state(&c, "done");
    let e = c
        .rpc("oauth_finish", &json!({"add": true}))
        .unwrap_err()
        .to_string();
    assert!(e.contains("already in the app"), "{e}");
    assert_eq!(
        c.rpc("oauth_poll", &json!({})).unwrap()["state"],
        "done",
        "the sign-in was not spent: the person does not repeat the browser trip"
    );
    assert!(
        imap.log.lock().unwrap().is_empty(),
        "the mail server was not asked"
    );
    assert!(tok.bodies("/grevoke").is_empty());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_token_is_never_sent_to_a_host_the_settings_file_names_instead_of_the_providers() {
    let tok = Tok::start();
    let imap = imap_start(&["AT1"]);
    let _s = scene(Some(&tok));
    secrets::set("oauth-refresh-0", "RT").unwrap();
    oauth::prime(0, "AT1", 3600);
    // a settings file that was edited or damaged: a Google sign-in on another server
    let a = watcher::Account {
        host: "imap.evil.example".into(),
        port: 993,
        user: "me@gmail.com".into(),
        password: String::new(),
        oauth: Some((0, Provider::Google)),
    };
    let e = match watcher::connect_plain_account(imap.addr, &a) {
        Ok(_) => panic!("connected with a token to another server"),
        Err(e) => e,
    };
    assert!(e.to_string().contains("only for imap.gmail.com"), "{e}");
    assert!(imap.log.lock().unwrap().is_empty(), "no token was sent");
    watcher::use_plain_server(Some(imap.addr));
    assert!(watcher::check_token(&a, "AT1").is_err());
    assert!(
        imap.log.lock().unwrap().is_empty(),
        "no token was sent by the check either"
    );
    // the provider's own server, in any letter case, is fine
    let ok = watcher::Account {
        host: "IMAP.Gmail.com".into(),
        ..a
    };
    let mut s = watcher::connect_plain_account(imap.addr, &ok).expect("the provider's own server");
    let _ = s.logout();
}

#[test]
fn a_dropped_connection_is_not_a_refused_sign_in() {
    let tok = Tok::start();
    let _s = scene(Some(&tok));
    // a server that hangs up as soon as it is asked to authenticate
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap();
    std::thread::spawn(move || {
        for mut c in l.incoming().flatten() {
            let _ = c.write_all(b"* OK ready\r\n");
            let mut r = BufReader::new(c.try_clone().unwrap());
            let mut line = String::new();
            let _ = r.read_line(&mut line);
            drop(c); // gone
        }
    });
    secrets::set("oauth-refresh-0", "RT").unwrap();
    oauth::prime(0, "AT1", 3600);
    let a = watcher::Account {
        host: "imap.gmail.com".into(),
        port: 993,
        user: "me@gmail.com".into(),
        password: String::new(),
        oauth: Some((0, Provider::Google)),
    };
    let e = match watcher::connect_plain_account(addr, &a) {
        Ok(_) => panic!("signed in to a server that hung up"),
        Err(e) => e,
    };
    assert!(
        !watcher::is_sign_in_problem(&e),
        "a dropped connection is an ordinary error, tried again soon: {e}"
    );
    assert!(
        tok.bodies("/gtoken").is_empty(),
        "and it did not cost a new token"
    );
}

#[test]
fn removing_a_mailbox_while_its_token_is_being_refreshed_leaves_nothing_behind() {
    let tok = Tok::start();
    tok.say_slowly(
        "/mstoken",
        200,
        json!({"access_token": "MT-new", "refresh_token": "MR-rotated", "expires_in": 3600}),
        500,
    );
    let _s = scene(Some(&tok));
    secrets::set("oauth-refresh-0", "MR-old").unwrap();
    let refresh = std::thread::spawn(|| oauth::access_token(0, Provider::Microsoft));
    tok.wait_for("/mstoken"); // the provider is being asked
    oauth::sign_out_mailbox(0, Some(Provider::Microsoft));
    let _ = refresh.join().unwrap();
    assert!(
        secrets::get("oauth-refresh-0").is_none(),
        "the rotated token was not written back after the removal"
    );
    // and nothing is left in memory to be used for the removed mailbox
    let e = oauth::access_token(0, Provider::Microsoft).unwrap_err();
    assert!(watcher::is_sign_in_problem(&e), "{e}");
}

#[test]
fn a_microsoft_sign_in_cancelled_while_the_code_is_being_asked_for_does_not_come_back() {
    let tok = Tok::start();
    tok.say_slowly("/msdevice", 200, json!({"device_code": "DC", "user_code": "ABCD-EFGH", "verification_uri": "https://www.microsoft.com/link", "expires_in": 900, "interval": 1}), 500);
    tok.say("/mstoken", 400, json!({"error": "authorization_pending"}));
    let _s = scene(Some(&tok));
    let (c, dir) = fresh("ms-cancel");
    let c2 = c.clone();
    let start = std::thread::spawn(move || {
        c2.rpc(
            "oauth_start",
            &json!({"provider": "microsoft", "user": "me@outlook.com"}),
        )
    });
    tok.wait_for("/msdevice");
    c.rpc("oauth_cancel", &json!({})).unwrap();
    let r = start.join().unwrap();
    assert!(r.is_err(), "the start that was cancelled says so: {r:?}");
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(
        c.rpc("oauth_poll", &json!({})).unwrap()["state"],
        "idle",
        "no code is left on screen for a sign-in that was cancelled"
    );
    assert!(
        tok.bodies("/mstoken").is_empty(),
        "and nothing polls for it"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_status_line_tells_a_signed_in_mailbox_to_sign_in_again_not_to_enter_a_password() {
    let pw = watcher::Account {
        host: "imap.example.com".into(),
        port: 993,
        user: "a@example.com".into(),
        password: "x".into(),
        oauth: None,
    };
    let acc = watcher::Account {
        oauth: Some((0, Provider::Google)),
        ..pw.clone()
    };
    assert!(watcher::refused_status(&pw).contains("Enter your password again in Settings"));
    assert!(watcher::refused_status(&acc).contains("Sign in again in Settings"));
    assert!(!watcher::refused_status(&acc).contains("password"));
}

#[test]
fn a_change_made_in_settings_while_the_mail_server_is_asked_is_kept() {
    let tok = Tok::start();
    tok.say("/gtoken", 200, tokens("AT1", Some("RT1"), MAIL));
    let imap = imap_start(&["AT1"]);
    let _s = scene(Some(&tok));
    watcher::use_plain_server(Some(imap.addr));
    let (c, dir) = fresh("lost-update");
    google_round(&c, "me@gmail.com", |_, st| format!("/?code=C&state={st}"));
    wait_state(&c, "done");
    // a setting changes while oauth_finish is waiting for the mail server (the scripted server answers after a pause)
    IMAP_DELAY_MS.store(500, std::sync::atomic::Ordering::Relaxed);
    let c2 = c.clone();
    let asked = IMAP_ASKED.load(std::sync::atomic::Ordering::Relaxed);
    let slow = std::thread::spawn(move || c2.rpc("oauth_finish", &json!({})));
    while IMAP_ASKED.load(std::sync::atomic::Ordering::Relaxed) == asked {
        std::thread::sleep(Duration::from_millis(10)); // the mail server is being asked, and is slow to answer
    }
    c.rpc(
        "save_settings",
        &json!({"patch": {"followups": {"days": 5}}}),
    )
    .unwrap();
    let finished = slow.join().unwrap();
    assert!(finished.is_ok(), "{finished:?}");
    assert_eq!(
        Settings::load(&dir).followups.days,
        5,
        "the change was not undone by the save that followed"
    );
    assert_eq!(Settings::load(&dir).account.user, "me@gmail.com");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_refresh_that_was_overtaken_by_a_new_sign_in_does_not_write_its_token_over_it() {
    let tok = Tok::start();
    tok.say_slowly(
        "/mstoken",
        200,
        json!({"access_token": "MT-old-grant", "refresh_token": "MR-rotated-old", "expires_in": 3600}),
        500,
    );
    let _s = scene(Some(&tok));
    secrets::set("oauth-refresh-0", "MR-old").unwrap();
    let refresh = std::thread::spawn(|| oauth::access_token(0, Provider::Microsoft));
    tok.wait_for("/mstoken"); // the provider is being asked about the old grant
    secrets::set("oauth-refresh-0", "MR-resigned-in").unwrap(); // the person signed in again meanwhile
    let e = refresh.join().unwrap().unwrap_err().to_string();
    assert!(e.contains("replaced"), "{e}");
    assert_eq!(
        secrets::get("oauth-refresh-0").as_deref(),
        Some("MR-resigned-in"),
        "the new sign-in was not overwritten by the old grant's refreshed token"
    );
}

#[test]
fn when_the_settings_cannot_be_saved_the_old_sign_in_keeps_working_and_the_new_grant_is_cancelled()
{
    let tok = Tok::start();
    tok.say("/gtoken", 200, tokens("AT-new", Some("RT-new"), MAIL));
    tok.say("/grevoke", 200, json!({}));
    let imap = imap_start(&["AT-new"]);
    let _s = scene(Some(&tok));
    watcher::use_plain_server(Some(imap.addr));
    let (c, dir) = fresh("save-fails");
    secrets::set("oauth-refresh-0", "RT-old").unwrap();
    secrets::set("imap-password", "pw").unwrap();
    // a settings file that cannot be written (it is a folder)
    std::fs::create_dir_all(dir.join("settings.json")).unwrap();
    google_round(&c, "me@gmail.com", |_, st| format!("/?code=C&state={st}"));
    wait_state(&c, "done");
    let e = c.rpc("oauth_finish", &json!({"id": 0}));
    assert!(e.is_err(), "the save cannot work");
    assert_eq!(
        secrets::get("oauth-refresh-0").as_deref(),
        Some("RT-old"),
        "the keychain is as it was"
    );
    assert_eq!(
        secrets::get("imap-password").as_deref(),
        Some("pw"),
        "no password was deleted"
    );
    let revoked = tok.bodies("/grevoke");
    assert!(
        revoked.contains(&"token=RT-new".to_string()),
        "the grant that was not used is cancelled"
    );
    assert!(
        !revoked.contains(&"token=RT-old".to_string()),
        "the old grant, which still works, is not"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_finished_sign_in_that_is_dropped_is_cancelled_at_google_so_no_grant_is_left_unowned() {
    let tok = Tok::start();
    tok.say("/gtoken", 200, tokens("AT1", Some("RT1"), MAIL));
    tok.say("/grevoke", 200, json!({}));
    let _s = scene(Some(&tok));
    let (c, dir) = fresh("dropped");
    let revoked = |n: usize| {
        let until = Instant::now() + Duration::from_secs(5);
        while tok.bodies("/grevoke").len() < n && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(25));
        }
        tok.bodies("/grevoke")
    };
    // cancelled after it finished (the person pressed Cancel as the answer arrived)
    google_round(&c, "me@gmail.com", |_, st| format!("/?code=C&state={st}"));
    wait_state(&c, "done");
    c.rpc("oauth_cancel", &json!({})).unwrap();
    assert_eq!(revoked(1), vec!["token=RT1".to_string()]);
    // replaced by a new sign-in before it was used
    google_round(&c, "me@gmail.com", |_, st| format!("/?code=C&state={st}"));
    wait_state(&c, "done");
    google_round(&c, "me@gmail.com", |_, st| format!("/?code=C&state={st}"));
    assert_eq!(
        revoked(2).len(),
        2,
        "the first was cancelled when the second began"
    );
    // forgotten because it was not used in time
    wait_state(&c, "done");
    std::thread::sleep(Duration::from_millis(2300));
    assert_eq!(c.rpc("oauth_poll", &json!({})).unwrap()["state"], "idle");
    assert_eq!(revoked(3).len(), 3, "and so was the one that went stale");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_password_ends_the_old_sign_in_only_after_the_settings_that_say_so_are_saved() {
    let tok = Tok::start();
    // the revoke takes a while: what the settings say while it is under way shows the order
    tok.say_slowly("/grevoke", 200, json!({}), 700);
    let _s = scene(Some(&tok));
    let (c, dir) = fresh("order");
    gmail_mailbox_with_a_sign_in(&dir, "RT-old");
    let c2 = c.clone();
    let save = std::thread::spawn(move || {
        c2.rpc(
            "save_account",
            &json!({"id": 0, "host": "imap.gmail.com", "user": "me@gmail.com", "password": "abcd efgh ijkl mnop"}),
        )
    });
    tok.wait_for("/grevoke"); // Google is being told, and the settings are already saved
    assert!(
        Settings::load(&dir).oauth.is_empty(),
        "the settings were saved before the old grant was given up"
    );
    assert!(save.join().unwrap().is_ok());
    assert!(secrets::get("oauth-refresh-0").is_none() && secrets::get("imap-password").is_some());
    let _ = std::fs::remove_dir_all(dir);
}
