//! Secrets live in the OS keychain (Windows Credential Manager, macOS Keychain, Secret Service).
use anyhow::Result;

const SERVICE: &str = "InboxAgent";

pub fn set(name: &str, value: &str) -> Result<()> {
    keyring::Entry::new(SERVICE, name)?.set_password(value)?;
    Ok(())
}

pub fn get(name: &str) -> Option<String> {
    keyring::Entry::new(SERVICE, name)
        .ok()?
        .get_password()
        .ok()
        .filter(|s| !s.is_empty())
}

pub fn delete(name: &str) {
    if let Ok(e) = keyring::Entry::new(SERVICE, name) {
        let _ = e.delete_credential();
    }
}
