//! Opening a page in the person's own browser: the provider's page where a mail password is made, or a message in its own website.
//!
//! The window cannot open a link by itself, and a command that opens whatever address it is given would let a bug or a hostile mail send a
//! person anywhere, so only `https` addresses on a short list of hosts are opened, with no user name in them and no port.
use anyhow::{anyhow, Result};
use std::sync::Mutex;

/// The hosts a page may be opened on: where a Google, Apple, Yahoo, Fastmail or Microsoft account makes an app password or signs in, and their webmail.
const HOSTS: &[&str] = &[
    "myaccount.google.com",
    "accounts.google.com",
    "mail.google.com",
    "login.yahoo.com",
    "mail.yahoo.com",
    "account.apple.com",
    "appleid.apple.com",
    "www.icloud.com",
    "app.fastmail.com",
    "login.microsoftonline.com",
    "login.live.com",
    "account.live.com",
    "account.microsoft.com",
    "microsoft.com",
    "www.microsoft.com",
    "outlook.live.com",
    "outlook.office.com",
    "outlook.office365.com",
];

/// Addresses "opened" while recording (in tests, and when `INBOX_AGENT_OPEN=record` asks for it): nothing is launched.
static OPENED: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// May this address be opened? `https://`, a host on the list (nothing before it, no port), and nothing in it that is not plain printable text.
pub fn allowed(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    if url.len() > 2048
        || url
            .chars()
            .any(|c| c.is_control() || c.is_whitespace() || c == '\\')
    {
        return false;
    }
    let host_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let host = &rest[..host_end];
    host.bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
        && HOSTS.contains(&host.to_ascii_lowercase().as_str())
}

/// Opens the page in the default browser. Refuses an address that is not allowed. While recording, only remembers it.
pub fn open(url: &str) -> Result<()> {
    if !allowed(url) {
        return Err(anyhow!("That page can't be opened from here"));
    }
    if recording() {
        OPENED
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(url.to_string());
        return Ok(());
    }
    open::that_detached(url).map_err(|e| anyhow!("Couldn't open your browser: {e}"))
}

/// Is opening only being recorded? In this crate's own tests always, so no test opens a browser on the computer of the person running it.
fn recording() -> bool {
    std::env::var_os("INBOX_AGENT_OPEN").is_some_and(|v| v == "record")
        || crate::secrets::under_cargo_test()
}

/// Remembers an address as if it had been opened, without checking it or opening it. Only while recording (in tests), so a sign-in test can see where
/// it would have sent the browser; anywhere else this does nothing.
pub fn note(url: &str) {
    if recording() {
        OPENED
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(url.to_string());
    }
}

/// The addresses recorded so far, oldest first (only while recording).
pub fn opened() -> Vec<String> {
    OPENED.lock().unwrap_or_else(|e| e.into_inner()).clone()
}
