//! Secrets live in the OS keychain (Windows Credential Manager, macOS Keychain, Secret Service).
//!
//! With `INBOX_AGENT_KEYCHAIN=memory` in the environment they live in memory instead. The engine's own tests use memory whatever
//! the environment says (see `in_memory`), so a test run never overwrites a password saved by the real app on the same computer.
//! The built app does not set it.
use anyhow::Result;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

const SERVICE: &str = "InboxAgent";

/// The most mailboxes the app reads at once; their passwords are the entries `imap_key(0..MAX_MAILBOXES)`.
pub const MAX_MAILBOXES: u32 = 5;

/// The keychain entry holding the password of mailbox `id`. The first mailbox keeps the name it always had.
pub fn imap_key(id: u32) -> String {
    if id == 0 {
        "imap-password".to_string()
    } else {
        format!("imap-password-{id}")
    }
}

/// Every entry this app ever writes, so "sign out" can remove all of them.
fn all_names() -> Vec<String> {
    let mut v: Vec<String> = (0..MAX_MAILBOXES).map(imap_key).collect();
    v.push("anthropic-key".to_string());
    v
}

/// Is the keychain a table in memory instead of the operating system's? Yes when `INBOX_AGENT_KEYCHAIN=memory` asks for it,
/// and always for this crate's own tests, wherever cargo was started: a test that saves a password or signs out must never
/// touch the passwords of the person running it. (A cargo test binary is recognised by the variables cargo gives it, or by
/// living in a `deps` folder; the installed app and `tauri dev` are neither.)
pub fn in_memory() -> bool {
    std::env::var_os("INBOX_AGENT_KEYCHAIN").is_some_and(|v| v == "memory")
        || (std::env::var_os("CARGO_PKG_NAME").is_some_and(|v| v == "engine")
            && std::env::var_os("CARGO_MANIFEST_DIR").is_some())
        || std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.ends_with("deps")))
            .unwrap_or(false)
}

fn memory() -> Option<&'static Mutex<HashMap<String, String>>> {
    static MEM: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    in_memory().then(|| MEM.get_or_init(|| Mutex::new(HashMap::new())))
}

fn lock(m: &Mutex<HashMap<String, String>>) -> std::sync::MutexGuard<'_, HashMap<String, String>> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Stores `value` under `name` in the keychain, replacing any earlier value. Fails if the keychain cannot be written.
pub fn set(name: &str, value: &str) -> Result<()> {
    if let Some(m) = memory() {
        lock(m).insert(name.to_string(), value.to_string());
        return Ok(());
    }
    keyring::Entry::new(SERVICE, name)?.set_password(value)?;
    Ok(())
}

/// The saved value, or `None` when there is none, it is empty, or the keychain could not be read.
/// Use `lookup` when a keychain failure must be told apart from a missing entry.
pub fn get(name: &str) -> Option<String> {
    lookup(name).ok().flatten()
}

/// `Ok(None)` means there is no such entry. `Err` means the keychain itself could not be read (locked, no
/// service running, access refused), which is a different problem and must not look like a missing password.
pub fn lookup(name: &str) -> Result<Option<String>> {
    if let Some(m) = memory() {
        return Ok(lock(m).get(name).filter(|s| !s.is_empty()).cloned());
    }
    classify(keyring::Entry::new(SERVICE, name).and_then(|e| e.get_password()))
}

/// Turns a keychain read into `lookup`'s answer: an empty value or a missing entry is `Ok(None)`; any other error stays an error.
pub fn classify(r: keyring::Result<String>) -> Result<Option<String>> {
    match r {
        Ok(s) if s.is_empty() => Ok(None),
        Ok(s) => Ok(Some(s)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Removes the entry `name`. It never fails: an entry that is not there, or a keychain that refuses, is ignored.
pub fn delete(name: &str) {
    if let Some(m) = memory() {
        lock(m).remove(name);
        return;
    }
    if let Ok(e) = keyring::Entry::new(SERVICE, name) {
        let _ = e.delete_credential();
    }
}

/// Removes every entry the app writes (all mailbox passwords and the API key), for signing out.
pub fn delete_all() {
    for n in all_names() {
        delete(&n);
    }
}
