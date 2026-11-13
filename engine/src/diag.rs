//! A small crash log. A release build has no console, and the activity log lives in the database (which may be the
//! thing that broke), so panics are also written to `crash.log` next to it. Only the panic text and its source
//! location are recorded, cut to 300 characters, never mail content on purpose; see PRIVACY.md.
use std::io::Write;
use std::path::{Path, PathBuf};

const MAX_LOG_BYTES: u64 = 100 * 1024;
const MAX_TEXT: usize = 300;

/// Appends one crash entry. The file restarts when it passes 100 KB, so it can never grow without bound.
pub fn record(dir: &Path, thread: &str, location: &str, message: &str) {
    let path = dir.join("crash.log");
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > MAX_LOG_BYTES) {
        let _ = std::fs::remove_file(&path);
    }
    let clean = |s: &str| -> String {
        s.chars()
            .map(|c| if c.is_control() { ' ' } else { c })
            .take(MAX_TEXT)
            .collect()
    };
    let line = format!(
        "{} [{}] {} {}\n",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
        clean(thread),
        clean(location),
        clean(message)
    );
    let _ = std::fs::create_dir_all(dir);
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = f.write_all(line.as_bytes());
    }
}

/// Makes every panic in the process leave a line in `<dir>/crash.log`, then runs the previous hook.
pub fn install_crash_log(dir: PathBuf) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let msg = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "panic".into());
        let at = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_default();
        let thread = std::thread::current()
            .name()
            .unwrap_or("unnamed")
            .to_string();
        record(&dir, &thread, &at, &msg);
        previous(info);
    }));
}
