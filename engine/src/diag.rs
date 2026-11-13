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

// ───────────────────────── support report ─────────────────────────

/// How an error line may continue after its first ": ". The rest of the line is kept only when it STARTS with one of
/// these (they carry no user data, and what follows is a number); anything else — server replies, parse errors, names,
/// folder and label names — is replaced by an ellipsis.
const SAFE_TAILS: &[&str] = &[
    "processing it crashed",
    "larger than",
    "handling it crashed the app",
    "you replied",
];

/// Everything from the first opening quote to the LAST matching closing quote becomes an ellipsis, so a subject that
/// itself contains quotation marks cannot leak its tail. Curly quotes, straight quotes and backticks (Rust's panic
/// messages quote strings in backticks) all count. An unterminated quote swallows the rest of the line.
fn collapse_quotes(s: &str) -> String {
    let closer = |c: char| match c {
        '“' => Some('”'),
        '"' => Some('"'),
        '`' => Some('`'),
        _ => None,
    };
    let Some((i, open)) = s.char_indices().find(|(_, c)| closer(*c).is_some()) else {
        return s.to_string();
    };
    let end = closer(open).unwrap_or(open);
    let rest = &s[i + open.len_utf8()..];
    match rest.rfind(end) {
        Some(j) => format!(
            "{}{open}…{end}{}",
            &s[..i],
            collapse_quotes(&rest[j + end.len_utf8()..])
        ),
        None => format!("{}{open}…{end}", &s[..i]),
    }
}

/// Removes what follows an error colon or sits inside "connection lost (...)", and the login name in "connected as".
fn drop_error_details(s: &str) -> String {
    if let Some(rest) = s.strip_prefix("connected as ") {
        let after = rest.split_once(';').map_or("", |(_, a)| a);
        return format!("connected;{after}");
    }
    if let Some(i) = s.find("connection lost (") {
        let tail = s.rfind(')').map_or("", |j| &s[j + 1..]);
        return format!("{}connection lost (…){tail}", &s[..i]);
    }
    match s.split_once(": ") {
        Some((head, tail)) if !SAFE_TAILS.iter().any(|t| tail.starts_with(t)) => {
            format!("{head}: …")
        }
        _ => s.to_string(),
    }
}

/// Makes one log line safe to paste into a bug report: quoted text, error details, addresses and home-folder paths are
/// replaced, control characters (and so line breaks) become spaces, and the line is cut to 200 characters.
pub fn redact(line: &str) -> String {
    let s: String = line
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let s = drop_error_details(&collapse_quotes(&s));
    // Addresses: any whitespace-delimited piece that holds an "@" goes whole. Local parts can hold quotes, slashes,
    // braces and "=" (VERP bounce addresses encode the recipient that way), so nothing less than the whole piece is safe.
    let out = s
        .split(' ')
        .map(|t| if t.contains('@') { "<address>" } else { t })
        .collect::<Vec<_>>()
        .join(" ");
    // from the first path under a home folder to the end of the line (a folder name can contain spaces)
    let mut kept: Vec<&str> = vec![];
    for t in out.split(' ') {
        let l = t.to_lowercase();
        if l.contains("\\users\\") || l.contains("/users/") || l.contains("/home/") {
            kept.push("<path>");
            break;
        }
        // any other piece that looks like a path or a mail folder ("Drafts/Jane-Private") is a name someone chose
        kept.push(if t.contains('/') || t.contains('\\') {
            "<name>"
        } else {
            t
        });
    }
    let out = kept.join(" ").trim().to_string();
    if out.chars().count() > 200 {
        let mut cut: String = out.chars().take(199).collect();
        cut.push('…');
        cut
    } else {
        out
    }
}

/// A crash-log line is "<date> <time> [<thread>] <file:line> <panic text>". The panic text can quote user data
/// (Rust quotes strings it fails to slice), so only the first four parts are kept.
pub fn redact_crash(line: &str) -> String {
    let parts: Vec<&str> = line.split_whitespace().collect();
    // One piece is this program's own source location (src/mail.rs:10), which is code, not the user's data; it is
    // recognised by its shape, so a line with or without the time field keeps it.
    let is_location = |p: &str| p.contains(".rs:") && !p.contains('@');
    let head = parts
        .iter()
        .take(4)
        .map(|p| {
            if is_location(p) {
                (*p).to_string()
            } else {
                redact(p)
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    if parts.len() > 4 {
        format!("{head} (message removed)")
    } else {
        head
    }
}

/// Mail providers whose name tells nobody anything about the user. Any other server (a company, a small host, a LAN
/// address) is reported as "custom server": the domain alone can identify a person.
const KNOWN_HOSTS: &[&str] = &[
    "imap.gmail.com",
    "imap.mail.me.com",
    "imap.mail.yahoo.com",
    "imap.fastmail.com",
    "outlook.office365.com",
    "imap.zoho.com",
    "imap.aol.com",
    "imap.gmx.com",
    "imap.mail.com",
    "imappro.zoho.eu",
    "imap.protonmail.ch",
];

fn server_name(host: &str) -> String {
    let h = host.trim().to_lowercase();
    if h.is_empty() {
        "not set".into()
    } else if KNOWN_HOSTS.contains(&h.as_str()) {
        h
    } else {
        "custom server".into()
    }
}

/// What the support report is built from.
pub struct ReportInput<'a> {
    pub version: &'a str,
    pub os: &'a str,
    pub status: &'a str,
    pub open: usize,
    pub done: usize,
    pub gone: usize,
    pub ai_mode: &'a str,
    pub writeback: bool,
    pub hub: bool,
    pub quiet_hours: bool,
    pub google: bool,
    pub host: &'a str,
    pub log: &'a [String],
    pub crash: &'a [String],
}

/// Plain text the user can read, check and paste into a bug report. It holds counts, option names, the server's
/// host name, and redacted log lines: no addresses, subjects, message text, keys or access codes.
pub fn report(i: &ReportInput) -> String {
    let on = |b: bool| if b { "on" } else { "off" };
    let lines = |v: &[String], f: &dyn Fn(&str) -> String| {
        if v.is_empty() {
            "  none\n".to_string()
        } else {
            v.iter().map(|l| format!("  {}\n", f(l))).collect()
        }
    };
    format!(
        "Inbox Agent diagnostics\nVersion: {}\nSystem: {}\nStatus: {}\nTasks: {} open, {} done, {} left the inbox\nOptions: ranking {} · write-back {} · phone hub {} · quiet hours {} · Google sign-in {}\nMail server: {}\n\nRecent activity (newest first, private text removed):\n{}\nCrash log (last lines, private text removed):\n{}",
        redact(i.version),
        redact(i.os),
        redact(i.status),
        i.open,
        i.done,
        i.gone,
        redact(i.ai_mode),
        on(i.writeback),
        on(i.hub),
        on(i.quiet_hours),
        on(i.google),
        server_name(i.host),
        lines(i.log, &redact),
        lines(i.crash, &redact_crash)
    )
}
