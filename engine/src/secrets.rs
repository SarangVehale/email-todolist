//! Secrets live in the OS keychain (Windows Credential Manager, macOS Keychain, Secret Service).
use anyhow::Result;

const SERVICE: &str = "InboxAgent";

/// Every entry this app ever writes, so "sign out" can remove all of them.
pub const ALL: &[&str] = &["imap-password", "anthropic-key"];

pub fn set(name: &str, value: &str) -> Result<()> {
    keyring::Entry::new(SERVICE, name)?.set_password(value)?;
    Ok(())
}

pub fn get(name: &str) -> Option<String> {
    lookup(name).ok().flatten()
}

/// `Ok(None)` means there is no such entry. `Err` means the keychain itself could not be read (locked, no
/// service running, access refused), which is a different problem and must not look like a missing password.
pub fn lookup(name: &str) -> Result<Option<String>> {
    classify(keyring::Entry::new(SERVICE, name).and_then(|e| e.get_password()))
}

pub fn classify(r: keyring::Result<String>) -> Result<Option<String>> {
    match r {
        Ok(s) if s.is_empty() => Ok(None),
        Ok(s) => Ok(Some(s)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn delete(name: &str) {
    if let Ok(e) = keyring::Entry::new(SERVICE, name) {
        let _ = e.delete_credential();
    }
}

pub fn delete_all() {
    for n in ALL {
        delete(n);
    }
}
