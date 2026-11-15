//! Signing in with a Google or Microsoft account instead of an app password (design: docs/designs/phase12.md; the safety list is
//! section 3 of docs/designs/phase3.md; the decision is ADR-0015).
//!
//! * **Google** uses the loopback flow with PKCE: the person's browser is sent to Google, Google sends it back to a one-shot listener on
//!   `127.0.0.1` with a code, and the code is exchanged for tokens.
//! * **Microsoft** uses the device-code flow: the app shows a short code, the person types it on Microsoft's page, and the app polls until
//!   Microsoft says yes.
//!
//! Both are unavailable until the person who builds the app puts a client ID in the constants below (see
//! docs/runbooks/register-sign-in.md); the window then shows no button. Tokens are only ever sent in a POST body or an IMAP command, never
//! in an address, and no error text from a provider is kept: a failure is one of a fixed list of codes. The refresh token lives in the
//! system keychain beside the app passwords; an access token only in memory.
use crate::secrets;
use crate::watcher::SignInRefused;
use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

/// The Google client ID of this build, made at console.cloud.google.com ("Desktop app"). Empty means "Sign in with Google" is not offered.
pub const GOOGLE_CLIENT_ID: &str = "";
/// The matching secret. Google gives every desktop client one and requires it, and says it is not confidential.
pub const GOOGLE_CLIENT_SECRET: &str = "";
/// The Microsoft application (client) ID of this build, a public client made in Microsoft Entra. Empty means "Sign in with Microsoft" is not offered.
pub const MICROSOFT_CLIENT_ID: &str = "";

const GOOGLE_AUTH: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const GOOGLE_TOKEN: &str = "https://oauth2.googleapis.com/token";
const GOOGLE_REVOKE: &str = "https://oauth2.googleapis.com/revoke";
const GOOGLE_SCOPE: &str = "https://mail.google.com/";
const MS_DEVICE: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode";
const MS_TOKEN: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
const MS_SCOPE: &str = "https://outlook.office.com/IMAP.AccessAsUser.All offline_access";

/// How long a person has to finish signing in before the flow gives up.
const FLOW_SECS: u64 = 300;
/// A token with less than this long to live is refreshed before it is used.
const SKEW: Duration = Duration::from_secs(60);
/// After a provider says the sign-in was withdrawn, nothing asks it again for this long.
const DEAD_FOR: Duration = Duration::from_secs(300);

/// Which account the mailbox signs in with.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Provider {
    Google,
    Microsoft,
}
impl Provider {
    /// `google` or `microsoft`, as the settings keep it.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "google" => Some(Self::Google),
            "microsoft" => Some(Self::Microsoft),
            _ => None,
        }
    }
    /// `google` or `microsoft`: how the settings and the window name it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Google => "google",
            Self::Microsoft => "microsoft",
        }
    }
    /// What a person calls it.
    pub fn label(self) -> &'static str {
        match self {
            Self::Google => "Google",
            Self::Microsoft => "Microsoft",
        }
    }
    /// The only mail server a sign-in of this kind is ever sent to, whatever the settings say.
    pub fn host(self) -> &'static str {
        match self {
            Self::Google => "imap.gmail.com",
            Self::Microsoft => "outlook.office365.com",
        }
    }
}

/// Where the provider's pages are. A test points them at a local server; nothing else can (see `use_test_endpoints`).
#[derive(Clone, Debug)]
pub struct Endpoints {
    pub google_auth: String,
    pub google_token: String,
    pub google_revoke: String,
    pub ms_device: String,
    pub ms_token: String,
    pub google_id: String,
    pub google_secret: String,
    pub ms_id: String,
}
static TEST: Mutex<Option<Endpoints>> = Mutex::new(None);

/// Points the sign-in at local servers and gives it client IDs. Only a test of this crate can do it: anywhere else this does nothing and says so.
pub fn use_test_endpoints(e: Option<Endpoints>) -> bool {
    if !secrets::under_cargo_test() {
        return false;
    }
    *TEST.lock().unwrap_or_else(|e| e.into_inner()) = e;
    true
}
fn endpoints() -> (Endpoints, bool) {
    if let Some(e) = TEST.lock().unwrap_or_else(|e| e.into_inner()).clone() {
        return (e, true);
    }
    (
        Endpoints {
            google_auth: GOOGLE_AUTH.into(),
            google_token: GOOGLE_TOKEN.into(),
            google_revoke: GOOGLE_REVOKE.into(),
            ms_device: MS_DEVICE.into(),
            ms_token: MS_TOKEN.into(),
            google_id: GOOGLE_CLIENT_ID.into(),
            google_secret: GOOGLE_CLIENT_SECRET.into(),
            ms_id: MICROSOFT_CLIENT_ID.into(),
        },
        false,
    )
}

/// Is "Sign in with …" offered in this build? Only when it has a client ID.
pub fn available(p: Provider) -> bool {
    let (e, _) = endpoints();
    !match p {
        Provider::Google => e.google_id,
        Provider::Microsoft => e.ms_id,
    }
    .is_empty()
}

// ───────────────────────── small pieces: SHA-256, base64url, percent-encoding ─────────────────────────

/// SHA-256 (FIPS 180-4). Used only to make the PKCE challenge from the verifier; written out here because nothing else in the app needs a
/// hash and a dependency for one function is more to review than forty lines with the standard's test values.
pub fn sha256(data: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&((data.len() as u64) * 8).to_be_bytes());
    for chunk in msg.chunks(64) {
        let mut w = [0u32; 64];
        for (i, b) in chunk.chunks(4).enumerate() {
            w[i] = u32::from_be_bytes([b[0], b[1], b[2], b[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (x, y) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
            *x = x.wrapping_add(y);
        }
    }
    let mut out = [0u8; 32];
    for (i, x) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&x.to_be_bytes());
    }
    out
}

/// The PKCE challenge (RFC 7636, method S256) for a verifier.
pub fn pkce_challenge(verifier: &str) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(sha256(verifier.as_bytes()))
}

fn random_string(n: usize, alphabet: &[u8]) -> String {
    use rand::RngExt;
    let mut r = rand::rng();
    (0..n)
        .map(|_| alphabet[r.random_range(0..alphabet.len())] as char)
        .collect()
}

/// Percent-encodes everything but the unreserved characters (RFC 3986).
fn pct(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            out.push(b as char);
        } else {
            out += &format!("%{b:02X}");
        }
    }
    out
}

fn pct_decode(s: &str) -> String {
    let b = s.as_bytes();
    let (mut out, mut i) = (Vec::new(), 0);
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < b.len()
                && b[i + 1].is_ascii_hexdigit()
                && b[i + 2].is_ascii_hexdigit() =>
            {
                out.push(u8::from_str_radix(&s[i + 1..i + 3], 16).unwrap_or(b'?'));
                i += 2;
            }
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn same(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |d, (x, y)| d | (x ^ y)) == 0
}

// ───────────────────────── talking to the token endpoints ─────────────────────────

/// Why a call to a provider failed, as one of a fixed list: the provider's own text is never kept (an address in it could end up in a log
/// that the phone hub can read).
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Fail {
    /// The provider says the sign-in is no longer valid (revoked, expired, password changed).
    Withdrawn,
    /// Not yet: the person has not finished (device flow).
    Pending,
    /// Slow down (device flow).
    SlowDown,
    /// The person did not allow it, or the code ran out.
    Denied,
    Expired,
    /// The provider refused this app's registration (a wrong or missing client ID, a wrong redirect).
    Registration,
    Network,
    Other,
}
fn code_of(v: &Value) -> Fail {
    match v["error"].as_str().unwrap_or("") {
        "invalid_grant" => Fail::Withdrawn,
        "authorization_pending" => Fail::Pending,
        "slow_down" => Fail::SlowDown,
        "access_denied" | "authorization_declined" => Fail::Denied,
        "expired_token" => Fail::Expired,
        "invalid_client"
        | "unauthorized_client"
        | "invalid_request"
        | "invalid_scope"
        | "redirect_uri_mismatch" => Fail::Registration,
        _ => Fail::Other,
    }
}
/// The sentence a person is shown for a failure.
fn say(f: Fail, p: Provider) -> String {
    let n = p.label();
    match f {
        Fail::Withdrawn => format!("{n} says this sign-in is no longer valid. Sign in again."),
        Fail::Pending | Fail::SlowDown | Fail::Other => format!("{n} did not finish the sign-in. Try again."),
        Fail::Denied => "You chose not to allow Inbox Agent, so nothing was connected.".into(),
        Fail::Expired => "The sign-in took too long. Try again.".into(),
        Fail::Registration => format!("{n} did not accept this app's sign-in settings, so this copy of Inbox Agent cannot sign in with {n}. Use an app password instead."),
        Fail::Network => format!("Couldn't reach {n}. Check that this computer is online, then try again."),
    }
}

/// POSTs a form and reads the JSON answer. A status of 400 or 401 carries an `error` code and is read as one; nothing else of the answer is kept.
fn post_form(url: &str, form: &[(&str, &str)]) -> std::result::Result<Value, Fail> {
    match crate::ai::http_agent(20).post(url).send_form(form) {
        Ok(r) => r.into_json::<Value>().map_err(|_| Fail::Other),
        Err(ureq::Error::Status(_, r)) => Err(r
            .into_json::<Value>()
            .map(|v| code_of(&v))
            .unwrap_or(Fail::Other)),
        Err(_) => Err(Fail::Network),
    }
}

struct Tokens {
    access: String,
    refresh: Option<String>,
    expires_in: u64,
    scope: String,
}
fn read_tokens(v: &Value) -> std::result::Result<Tokens, Fail> {
    let access = v["access_token"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or(Fail::Other)?;
    Ok(Tokens {
        access: access.to_string(),
        refresh: v["refresh_token"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(str::to_string),
        expires_in: v["expires_in"].as_u64().unwrap_or(3600).clamp(60, 86_400),
        scope: v["scope"].as_str().unwrap_or("").to_string(),
    })
}

fn client_form(p: Provider, e: &Endpoints) -> Vec<(&'static str, String)> {
    match p {
        Provider::Google => {
            let mut v = vec![("client_id", e.google_id.clone())];
            if !e.google_secret.is_empty() {
                v.push(("client_secret", e.google_secret.clone()));
            }
            v
        }
        Provider::Microsoft => vec![("client_id", e.ms_id.clone())],
    }
}
fn token_url(p: Provider, e: &Endpoints) -> String {
    match p {
        Provider::Google => e.google_token.clone(),
        Provider::Microsoft => e.ms_token.clone(),
    }
}
fn token_post(p: Provider, extra: &[(&str, &str)]) -> std::result::Result<Tokens, Fail> {
    let (e, _) = endpoints();
    let base = client_form(p, &e);
    let mut form: Vec<(&str, &str)> = base.iter().map(|(k, v)| (*k, v.as_str())).collect();
    form.extend_from_slice(extra);
    read_tokens(&post_form(&token_url(p, &e), &form)?)
}

// ───────────────────────── the tokens of the mailboxes ─────────────────────────

struct Cached {
    token: String,
    until: Instant,
}
#[derive(Default)]
struct Store {
    cache: HashMap<u32, Cached>,
    /// When a provider last said a mailbox's sign-in was withdrawn.
    dead: HashMap<u32, Instant>,
}
fn store() -> &'static Mutex<Store> {
    static S: OnceLock<Mutex<Store>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(Store::default()))
}
fn lock_store() -> std::sync::MutexGuard<'static, Store> {
    store().lock().unwrap_or_else(|e| e.into_inner())
}
/// One refresh at a time, so the sync, the write-back and a restart of the settings cannot all ask a provider at once.
static REFRESHING: Mutex<()> = Mutex::new(());

/// Held while a mailbox's refresh token is put in place of another (a sign-in again): no refresh of the old one can then be in flight to write over it.
pub fn lock_refresh() -> std::sync::MutexGuard<'static, ()> {
    REFRESHING.lock().unwrap_or_else(|e| e.into_inner())
}

fn fresh(id: u32) -> Option<String> {
    lock_store()
        .cache
        .get(&id)
        .filter(|c| c.until > Instant::now() + SKEW)
        .map(|c| c.token.clone())
}

/// Remembers an access token that has just been given out.
pub fn prime(id: u32, access: &str, expires_in: u64) {
    let mut s = lock_store();
    s.dead.remove(&id);
    s.cache.insert(
        id,
        Cached {
            token: access.to_string(),
            until: Instant::now() + Duration::from_secs(expires_in),
        },
    );
}

/// Forgets the access token held for a mailbox (the server refused it: the next call asks the provider for a new one).
pub fn drop_cached(id: u32) {
    lock_store().cache.remove(&id);
}

/// An access token for the mailbox: the one in memory if it has time left, else a new one from the provider using the refresh token in the
/// keychain. When the provider says the sign-in was withdrawn it is not asked again for five minutes, so a mailbox in that state does not
/// hammer it every half minute.
pub fn access_token(id: u32, p: Provider) -> Result<String> {
    if let Some(t) = fresh(id) {
        return Ok(t);
    }
    let _one_at_a_time = REFRESHING.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(t) = fresh(id) {
        return Ok(t); // another thread refreshed while this one waited
    }
    if lock_store()
        .dead
        .get(&id)
        .is_some_and(|at| at.elapsed() < DEAD_FOR)
    {
        return Err(anyhow::Error::new(SignInRefused(say(Fail::Withdrawn, p))));
    }
    let refresh = secrets::lookup(&secrets::oauth_key(id))?.ok_or_else(|| {
        anyhow::Error::new(SignInRefused("no sign-in is saved for this mailbox".into()))
    })?;
    match token_post(
        p,
        &[("grant_type", "refresh_token"), ("refresh_token", &refresh)],
    ) {
        Ok(t) => {
            // the mailbox may have been removed, or signed in again, while the provider was being asked: then nothing is written back
            match secrets::lookup(&secrets::oauth_key(id))? {
                None => {
                    return Err(anyhow::Error::new(SignInRefused(
                        "this sign-in was removed".into(),
                    )))
                }
                Some(now) if now != refresh => {
                    return Err(anyhow!(
                        "this sign-in was replaced while it was being refreshed"
                    ))
                }
                Some(_) => {}
            }
            if let Some(new_refresh) = t.refresh.as_deref().filter(|n| *n != refresh) {
                secrets::set(&secrets::oauth_key(id), new_refresh)?; // Microsoft hands out a new one each time
            }
            prime(id, &t.access, t.expires_in);
            Ok(t.access)
        }
        Err(Fail::Withdrawn) => {
            lock_store().dead.insert(id, Instant::now());
            Err(anyhow::Error::new(SignInRefused(say(Fail::Withdrawn, p))))
        }
        Err(f) => Err(anyhow!(say(f, p))),
    }
}

/// Tells Google to cancel a refresh token (best effort; the token is in the body of the request, never in an address). Microsoft has no such call for personal accounts.
pub fn revoke(p: Provider, refresh: &str) {
    if p == Provider::Google {
        let (e, _) = endpoints();
        let _ = crate::ai::http_agent(5)
            .post(&e.google_revoke)
            .send_form(&[("token", refresh)]);
    }
}

/// Throws away a sign-in that was finished but not used (the mail server refused it, or the mailbox could not be saved): Google is told, and nothing is kept.
pub fn discard(id: u32, d: &Done) {
    revoke(d.provider, &d.refresh);
    drop_cached(id);
}

/// Forgets every token held in memory and ends any sign-in that is waiting (signing out).
pub fn forget_all() {
    // not while a refresh is in flight: it would write a new token back after this
    let _one_at_a_time = REFRESHING.lock().unwrap_or_else(|e| e.into_inner());
    let mut s = lock_store();
    s.cache.clear();
    s.dead.clear();
    drop(s);
    cancel();
}

/// Takes a mailbox's sign-in away: tells Google to revoke it (best effort, with the token in the body of the request), then deletes it from the
/// keychain and from memory.
pub fn sign_out_mailbox(id: u32, p: Option<Provider>) {
    // not while a refresh is in flight: it would write a rotated token back after this
    let _one_at_a_time = REFRESHING.lock().unwrap_or_else(|e| e.into_inner());
    if let (Some(p), Ok(Some(refresh))) = (p, secrets::lookup(&secrets::oauth_key(id))) {
        revoke(p, &refresh);
    }
    secrets::delete(&secrets::oauth_key(id));
    let mut s = lock_store();
    s.cache.remove(&id);
    s.dead.remove(&id);
}

// ───────────────────────── one sign-in at a time ─────────────────────────

/// A sign-in that has worked and is waiting for the window to say which mailbox it is for.
pub struct Done {
    pub provider: Provider,
    pub user: String,
    pub access: String,
    pub refresh: String,
    pub expires_in: u64,
    /// When the sign-in finished: one that is not used within ten minutes is forgotten, so tokens do not wait in memory for ever.
    at: Instant,
}
/// How long a finished sign-in waits for the window to save it (a test makes it short).
fn keep() -> Duration {
    if endpoints().1 {
        Duration::from_secs(2)
    } else {
        Duration::from_secs(600)
    }
}
enum St {
    Idle,
    Waiting { device: Option<(String, String)> },
    Done(Done),
    Failed(String),
}
struct Flow {
    gen: u64,
    st: St,
    cancel: Arc<AtomicBool>,
}
fn flow() -> &'static Mutex<Flow> {
    static F: OnceLock<Mutex<Flow>> = OnceLock::new();
    F.get_or_init(|| {
        Mutex::new(Flow {
            gen: 0,
            st: St::Idle,
            cancel: Arc::new(AtomicBool::new(false)),
        })
    })
}
fn lock_flow() -> std::sync::MutexGuard<'static, Flow> {
    flow().lock().unwrap_or_else(|e| e.into_inner())
}
/// A running flow records its end only if it is still the current one (a person who starts again must not have the first one's result arrive).
fn finish_flow(gen: u64, st: St) {
    let mut f = lock_flow();
    if f.gen == gen && !f.cancel.load(Ordering::Relaxed) {
        f.st = st;
    }
}

/// Where the sign-in stands, for the window to poll: `idle`, `waiting` (with the code to type, for Microsoft), `done` or `failed`. Never a token.
pub fn poll() -> Value {
    let mut f = lock_flow();
    if matches!(&f.st, St::Done(d) if d.at.elapsed() > keep()) {
        if let St::Done(d) = std::mem::replace(&mut f.st, St::Idle) {
            drop(f); // (the request to Google is made without the lock)
            revoke(d.provider, &d.refresh);
            return json!({"state": "idle"});
        }
    }
    match &f.st {
        St::Idle => json!({"state": "idle"}),
        St::Waiting {
            device: Some((code, url)),
        } => json!({"state": "waiting", "code": code, "url": url}),
        St::Waiting { device: None } => json!({"state": "waiting"}),
        St::Done(d) => json!({"state": "done", "user": d.user, "provider": d.provider.name()}),
        St::Failed(m) => json!({"state": "failed", "error": m}),
    }
}

/// Stops a sign-in that is waiting, and forgets how the last one ended.
pub fn cancel() {
    let old = {
        let mut f = lock_flow();
        f.cancel.store(true, Ordering::Relaxed);
        std::mem::replace(&mut f.st, St::Idle)
    };
    // a sign-in that had finished and was never saved is cancelled at Google too: no copy of it is left anywhere
    if let St::Done(d) = old {
        revoke(d.provider, &d.refresh);
    }
}

/// Who the finished sign-in is for, without using it up (so that what is wrong with the mailbox can be refused before the tokens are spent).
pub fn peek() -> Result<(Provider, String)> {
    match &lock_flow().st {
        St::Done(d) if d.at.elapsed() <= keep() => Ok((d.provider, d.user.clone())),
        St::Done(_) => Err(anyhow!(
            "That sign-in was too long ago to use. Sign in again."
        )),
        _ => Err(anyhow!(
            "There is no finished sign-in to use. Sign in again."
        )),
    }
}

/// The finished sign-in, once: the caller now owns the tokens.
pub fn take_done() -> Result<Done> {
    let mut f = lock_flow();
    match std::mem::replace(&mut f.st, St::Idle) {
        St::Done(d) if d.at.elapsed() <= keep() => Ok(d),
        St::Done(d) => {
            drop(f);
            revoke(d.provider, &d.refresh);
            Err(anyhow!(
                "That sign-in was too long ago to use. Sign in again."
            ))
        }
        other => {
            f.st = other;
            Err(anyhow!(
                "There is no finished sign-in to use. Sign in again."
            ))
        }
    }
}

/// Opens a page in the person's browser; in a test only remembers it (the test's own provider address is not on the list of real hosts).
fn open_page(url: &str, test: bool) -> Result<()> {
    if test {
        crate::webpage::note(url);
        Ok(())
    } else {
        crate::webpage::open(url)
    }
}

/// Starts signing in to `p` for the address `user` (it becomes the login hint, and is the user name the mail server is given). Returns what the window
/// shows while it waits: nothing for Google, whose page opens in the browser, and the code and the page to type it on for Microsoft.
pub fn start(p: Provider, user: &str) -> Result<Value> {
    if !available(p) {
        return Err(anyhow!(
            "Signing in with {} is not set up in this build",
            p.label()
        ));
    }
    let user = user.trim();
    if user.is_empty()
        || user.len() > 320
        || !user.contains('@')
        || !crate::settings::is_printable(user, 320)
    {
        return Err(anyhow!("That email address isn't valid"));
    }
    let (e, test) = endpoints();
    let (gen, cancel, old) = {
        let mut f = lock_flow();
        f.cancel.store(true, Ordering::Relaxed); // a flow still waiting is over
        f.gen += 1;
        f.cancel = Arc::new(AtomicBool::new(false));
        let old = std::mem::replace(&mut f.st, St::Waiting { device: None });
        (f.gen, f.cancel.clone(), old)
    };
    if let St::Done(d) = old {
        revoke(d.provider, &d.refresh); // a finished sign-in that was never saved, and is now replaced
    }
    let started: Result<Value> = match p {
        Provider::Google => start_google(gen, cancel, &e, test, user),
        Provider::Microsoft => start_microsoft(gen, cancel, &e, test, user),
    };
    if started.is_err() {
        let mut f = lock_flow();
        if f.gen == gen {
            f.st = St::Idle;
        }
    }
    started
}

fn start_google(
    gen: u64,
    cancel: Arc<AtomicBool>,
    e: &Endpoints,
    test: bool,
    user: &str,
) -> Result<Value> {
    // 127.0.0.1 and never "localhost" (which can mean another address), a port the system picks, and one flow at a time
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .map_err(|_| anyhow!("Couldn't start the sign-in on this computer"))?;
    listener.set_nonblocking(true)?;
    let port = listener.local_addr()?.port();
    let verifier = random_string(
        64,
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~",
    );
    let state = random_string(32, b"abcdefghijklmnopqrstuvwxyz0123456789");
    let redirect = format!("http://127.0.0.1:{port}");
    let url = format!(
        "{}?client_id={}&redirect_uri={}&response_type=code&scope={}&code_challenge={}&code_challenge_method=S256&state={}&access_type=offline&prompt=consent&login_hint={}",
        e.google_auth, pct(&e.google_id), pct(&redirect), pct(GOOGLE_SCOPE), pct_challenge(&verifier), state, pct(user)
    );
    open_page(&url, test).map_err(|_| anyhow!("Couldn't open your web browser"))?;
    let user2 = user.to_string();
    std::thread::Builder::new()
        .name("oauth-google".into())
        .spawn(move || {
            let r = loopback(
                &listener, &cancel, &state, &redirect, &verifier, test, &user2,
            );
            if !cancel.load(Ordering::Relaxed) {
                finish_flow(gen, r.map_or_else(St::Failed, St::Done));
            }
        })?;
    Ok(json!({"kind": "google"}))
}
fn pct_challenge(verifier: &str) -> String {
    pkce_challenge(verifier) // base64url has only unreserved characters
}

/// Waits for the browser to come back with the code: keeps accepting until the right `state` arrives (a browser opens idle spare connections),
/// 5 seconds and 8 KB for each connection, 404 for anything that is not the one request expected, a fixed page.
#[allow(clippy::too_many_arguments)]
fn loopback(
    l: &TcpListener,
    cancel: &AtomicBool,
    state: &str,
    redirect: &str,
    verifier: &str,
    test: bool,
    user: &str,
) -> std::result::Result<Done, String> {
    let p = Provider::Google;
    let deadline = Instant::now() + Duration::from_secs(if test { 20 } else { FLOW_SECS });
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(String::new());
        }
        if Instant::now() > deadline {
            return Err(say(Fail::Expired, p));
        }
        let mut conn = match l.accept() {
            Ok((c, _)) => c,
            Err(_) => {
                std::thread::sleep(Duration::from_millis(25));
                continue;
            }
        };
        let _ = conn.set_nonblocking(false);
        let _ = conn.set_read_timeout(Some(Duration::from_secs(5)));
        let _ = conn.set_write_timeout(Some(Duration::from_secs(5)));
        let Some(params) = read_callback(&mut conn) else {
            let _ = conn.write_all(
                b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            );
            continue;
        };
        let get = |k: &str| params.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
        if !get("state").is_some_and(|s| same(s, state)) {
            let _ = conn.write_all(
                b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            );
            continue;
        }
        let body = "<!doctype html><meta charset=utf-8><title>Inbox Agent</title><p style=\"font:20px sans-serif;margin:3em\">You can close this tab and go back to Inbox Agent.</p>";
        let _ = conn.write_all(
            format!("HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes(),
        );
        if get("error").is_some() {
            return Err(say(Fail::Denied, p));
        }
        let Some(code) = get("code").filter(|c| !c.is_empty()) else {
            return Err(say(Fail::Other, p));
        };
        let t = token_post(
            p,
            &[
                ("grant_type", "authorization_code"),
                ("code", code),
                ("redirect_uri", redirect),
                ("code_verifier", verifier),
            ],
        )
        .map_err(|f| say(f, p))?;
        // the person can untick the permission to read mail on Google's page: check what was really given
        if !t.scope.split(' ').any(|s| s == GOOGLE_SCOPE) {
            return Err("Mail access was not allowed. Sign in again and keep the box that lets Inbox Agent read your email ticked.".into());
        }
        let Some(refresh) = t.refresh else {
            return Err("Google did not give Inbox Agent lasting access. Remove Inbox Agent at myaccount.google.com/permissions, then sign in again.".into());
        };
        return Ok(Done {
            provider: p,
            user: user.to_string(),
            access: t.access,
            refresh,
            expires_in: t.expires_in,
            at: Instant::now(),
        });
    }
}

/// Reads one HTTP request from the browser and returns the query of `GET /?…`, or `None` for anything else (another method or path, a request
/// over 8 KB, one that stalls or is not finished in 5 seconds).
fn read_callback(c: &mut TcpStream) -> Option<Vec<(String, String)>> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 1024];
    let started = Instant::now();
    while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
        if buf.len() >= 8192 || started.elapsed() > Duration::from_secs(5) {
            return None;
        }
        match c.read(&mut chunk) {
            Ok(0) | Err(_) => return None,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
        }
    }
    let head = String::from_utf8_lossy(&buf);
    let line = head.lines().next()?;
    let mut parts = line.split(' ');
    let (method, target) = (parts.next()?, parts.next()?);
    if method != "GET" {
        return None;
    }
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    if path != "/" {
        return None;
    }
    Some(
        query
            .split('&')
            .filter(|kv| !kv.is_empty())
            .map(|kv| {
                let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
                (pct_decode(k), pct_decode(v))
            })
            .collect(),
    )
}

fn start_microsoft(
    gen: u64,
    cancel: Arc<AtomicBool>,
    e: &Endpoints,
    test: bool,
    user: &str,
) -> Result<Value> {
    let p = Provider::Microsoft;
    let v = post_form(
        &e.ms_device,
        &[("client_id", &e.ms_id), ("scope", MS_SCOPE)],
    )
    .map_err(|f| anyhow!(say(f, p)))?;
    let (Some(device), Some(code), Some(url)) = (
        v["device_code"].as_str(),
        v["user_code"].as_str(),
        v["verification_uri"].as_str(),
    ) else {
        return Err(anyhow!(say(Fail::Other, p)));
    };
    let (device, code, url) = (device.to_string(), code.to_string(), url.to_string());
    let (interval, lives) = (
        v["interval"].as_u64().unwrap_or(5).clamp(1, 30),
        v["expires_in"].as_u64().unwrap_or(FLOW_SECS).clamp(30, 900),
    );
    {
        // a sign-in that was cancelled or replaced while Microsoft was being asked for the code does not come back to life
        let mut f = lock_flow();
        if f.gen != gen || cancel.load(Ordering::Relaxed) {
            return Err(anyhow!("The sign-in was cancelled."));
        }
        f.st = St::Waiting {
            device: Some((code.clone(), url.clone())),
        };
    }
    let _ = open_page(&url, test); // if the address is not one that may be opened, the window shows it to be typed
    let user = user.to_string();
    std::thread::Builder::new()
        .name("oauth-microsoft".into())
        .spawn(move || {
            let r = device_poll(&cancel, &device, interval, lives, test, &user);
            if !cancel.load(Ordering::Relaxed) {
                finish_flow(gen, r.map_or_else(St::Failed, St::Done));
            }
        })?;
    Ok(json!({"kind": "microsoft", "code": code, "url": url}))
}

/// Asks Microsoft every `interval` seconds whether the code has been typed (more slowly when it says "slow down"), until yes, no, or it runs out.
fn device_poll(
    cancel: &AtomicBool,
    device: &str,
    mut interval: u64,
    lives: u64,
    test: bool,
    user: &str,
) -> std::result::Result<Done, String> {
    let p = Provider::Microsoft;
    let deadline = Instant::now() + Duration::from_secs(if test { 20 } else { lives });
    // a test does not wait whole seconds
    let tick = |secs: u64| {
        if test {
            Duration::from_millis(secs * 20)
        } else {
            Duration::from_secs(secs)
        }
    };
    loop {
        let until = Instant::now() + tick(interval);
        while Instant::now() < until {
            if cancel.load(Ordering::Relaxed) {
                return Err(String::new());
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        if Instant::now() > deadline {
            return Err(say(Fail::Expired, p));
        }
        match token_post(
            p,
            &[
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("device_code", device),
            ],
        ) {
            Ok(t) => {
                let Some(refresh) = t.refresh else {
                    return Err(say(Fail::Other, p));
                };
                return Ok(Done {
                    provider: p,
                    user: user.to_string(),
                    access: t.access,
                    refresh,
                    expires_in: t.expires_in,
                    at: Instant::now(),
                });
            }
            Err(Fail::Pending) => {}
            Err(Fail::SlowDown) => interval += 5,
            Err(f) => return Err(say(f, p)),
        }
    }
}

/// The XOAUTH2 first response (the user name and the token), as the IMAP command carries it before base64.
pub fn xoauth2_response(user: &str, token: &str) -> String {
    format!("user={user}\x01auth=Bearer {token}\x01\x01")
}
