//! The one-time notice that closing the window does not quit the app.
//!
//! Closing the window hides it and the app goes on watching the mailbox from its icon in the tray (the shell does that). A person who has
//! never seen that may think it quit, or that it is broken when mail alerts arrive. The shell shows one notification the first time the
//! window is closed; this remembers that it did, in a small file in the app's data folder.
use std::path::Path;

/// The file whose presence means the notice has been shown.
const MARKER: &str = "tray-told";

/// Is this the first time? True once, and it records that it was: the second call and every later one is false. If the record cannot be written (a
/// read-only or missing folder) it is false too, so a problem with the disk never turns into a notice on every close.
pub fn told_once(dir: &Path) -> bool {
    let marker = dir.join(MARKER);
    if marker.exists() {
        return false;
    }
    std::fs::write(
        &marker,
        b"The notice that closing the window keeps the app running has been shown.\n",
    )
    .is_ok()
}
