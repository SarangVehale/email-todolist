//! SQLite store: tasks, reply drafts, user corrections, activity log and the mailbox write-back outbox.
//! Each thread opens its own connection (WAL + busy timeout), so no shared locking is needed.
use crate::mail::Email;
use crate::reminders;
use crate::rules::{self, Rule};
use crate::signals::{SentMsg, Signals};
use crate::triage::Triage;
use chrono::{NaiveDateTime, Timelike};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::path::Path;

pub const MAX_DRAFT_CHARS: usize = 8000;
pub const MAX_NOTE_CHARS: usize = 2000;
/// Bumped whenever the database layout changes in a way an older app cannot read.
pub const SCHEMA_VERSION: i32 = 2;
/// Tasks you add yourself live above this number, so they never collide with a mail server's UIDs.
pub const LOCAL_UID_BASE: u32 = 2_000_000_000;
/// The most mailboxes the app reads at once.
pub const MAX_MAILBOXES: u32 = 5;
/// Each mailbox owns a block of this many task ids: mailbox `n` numbers its tasks `n * MAILBOX_SPAN + IMAP UID`. The first
/// mailbox's ids are therefore its IMAP UIDs, exactly as before, and five blocks end where your own tasks begin. A mailbox
/// whose UIDs pass this number (400 million messages) cannot be tracked and its messages are skipped with a log line.
pub const MAILBOX_SPAN: u32 = 400_000_000;

/// The task id of IMAP message `uid` in mailbox `mailbox`. `uid` must be below `MAILBOX_SPAN`.
pub fn global_uid(mailbox: u32, uid: u32) -> u32 {
    mailbox * MAILBOX_SPAN + uid
}

/// Which mailbox a mail task's id belongs to.
pub fn mailbox_of(id: u32) -> u32 {
    id / MAILBOX_SPAN
}

/// The IMAP UID inside the mailbox for a mail task's id.
pub fn imap_uid(id: u32) -> u32 {
    id % MAILBOX_SPAN
}

/// What a mailbox keeps in `meta`, under a name that carries the mailbox number (`last_uid@2`; the first mailbox keeps
/// the plain name).
const PER_MAILBOX_META: [&str; 5] = [
    "last_uid",
    "uid_validity",
    "attempt",
    "no_content",
    "followup_error",
];

#[derive(Debug, Clone, Serialize)]
pub struct Draft {
    pub body: String,
    /// "ai", "template" or "user"
    pub source: String,
    /// A copy was saved to the mailbox's Drafts folder.
    pub saved: bool,
    pub updated: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Task {
    pub uid: u32,
    pub message_id: String,
    pub sender: String,
    pub addr: String,
    pub subject: String,
    pub received: f64,
    pub priority: u8,
    pub category: String,
    pub action: String,
    pub urgent: bool,
    pub status: String,
    pub star: bool,
    pub excerpt: String,
    pub why: String,
    pub signals: Signals,
    pub needs_reply: bool,
    /// ISO date (YYYY-MM-DD) found in the message, or empty.
    pub due: String,
    /// Unix seconds; hidden from the to-do list until then. 0 = not snoozed.
    pub snooze_until: f64,
    pub reply_to: String,
    /// Replies would go somewhere the sender did not expect: the Reply-To domain differs from the
    /// sender's, or the message failed DKIM/SPF/DMARC. The UI asks for a second click before saving a draft.
    pub reply_warn: bool,
    /// Up to five attachment names, and how many files the message carries in total.
    pub attachments: Vec<String>,
    pub attachment_count: u32,
    /// A safe https/mailto unsubscribe link from the message, or empty.
    pub unsubscribe: String,
    /// Free text the user wrote on the task.
    pub note: String,
    /// Key shared by every message of one conversation. Mail with no usable id and tasks you added get `uid:<id>`,
    /// so they are never grouped with anything.
    pub thread: String,
    /// "replied" when the app closed this task because it found your reply in Sent; empty otherwise.
    pub closed_by: String,
    /// The message is no longer in your inbox.
    pub gone: bool,
    /// Steps added to (or taken from) the priority because of your corrections of this sender; 0 = none.
    pub learned: i32,
    pub draft: Option<Draft>,
    /// "important", "mute" or empty (normal) for this sender.
    pub pref: String,
    /// "followup" for a task the app made because nobody answered something you sent; empty otherwise.
    pub kind: String,
    /// Local "YYYY-MM-DD HH:MM" at which to remind you about a task you added; empty = none.
    pub remind: String,
    /// "daily", "weekly" or "monthly" for a task that comes back after you complete it; empty = never.
    pub repeat: String,
    /// The calendar event the message invited you to, if it carried one.
    pub event: Option<crate::calendar::Event>,
    /// The mailbox (0 to 4) a mail task, or a follow-up, belongs to; none for a task you added yourself.
    pub mailbox: Option<u32>,
}

/// What `apply_rules` did, or with a dry run would do.
#[derive(Debug, Default, Clone, Copy, PartialEq, Serialize)]
pub struct RuleReport {
    /// Open mail tasks a rule applies to.
    pub matched: usize,
    /// Of those, the ones whose priority, category or status would change.
    pub changed: usize,
    /// Of those, the ones that would be marked done.
    pub filed: usize,
}

#[derive(Debug, Clone)]
pub struct OutboxItem {
    pub id: i64,
    pub uid: u32,
    pub op: String,
    pub arg: String,
    pub tries: i64,
}

#[derive(Debug, Clone)]
pub struct Override {
    pub sender: String,
    pub subject: String,
    pub old: u8,
    pub new: u8,
}

pub struct Store {
    db: Connection,
    /// The mailbox this store works for, or None for the whole app. A plain store (the window, the commands, the phone,
    /// every test) is None: it sees every mailbox's tasks, and what it writes for "the mailbox" is the first one's, which
    /// is how a single-mailbox app has always behaved. The sync and write-back of mailbox `n` use `scoped(n)`.
    scope: Option<u32>,
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS tasks (
  uid INTEGER PRIMARY KEY, message_id TEXT, sender TEXT, addr TEXT, subject TEXT, received REAL,
  priority INTEGER, category TEXT, action TEXT, urgent INTEGER, status TEXT DEFAULT 'open',
  star INTEGER DEFAULT 0, excerpt TEXT DEFAULT '', why TEXT DEFAULT '', signals TEXT DEFAULT '{}'
);
CREATE TABLE IF NOT EXISTS drafts (
  uid INTEGER PRIMARY KEY, body TEXT NOT NULL, source TEXT NOT NULL, saved INTEGER DEFAULT 0, updated REAL
);
CREATE TABLE IF NOT EXISTS sender_prefs (addr TEXT PRIMARY KEY, level TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS overrides (
  id INTEGER PRIMARY KEY AUTOINCREMENT, sender TEXT, subject TEXT, old_priority INTEGER, new_priority INTEGER, at REAL
);
CREATE TABLE IF NOT EXISTS log (id INTEGER PRIMARY KEY AUTOINCREMENT, at REAL, msg TEXT);
CREATE TABLE IF NOT EXISTS outbox (
  id INTEGER PRIMARY KEY AUTOINCREMENT, uid INTEGER, op TEXT, arg TEXT DEFAULT '', tries INTEGER DEFAULT 0, at REAL
);
CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT);
CREATE TABLE IF NOT EXISTS rules (
  id TEXT PRIMARY KEY, ord INTEGER NOT NULL, field TEXT NOT NULL, text TEXT NOT NULL,
  priority INTEGER, category TEXT, file INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS followups (
  message_id TEXT PRIMARY KEY, state TEXT NOT NULL, at REAL NOT NULL, task_uid INTEGER NOT NULL DEFAULT 0,
  acct INTEGER NOT NULL DEFAULT 0
);
";

/// Full-text search over what a task holds, kept in step by triggers (so anything that adds, edits or deletes a task is
/// reflected, "Sign out and reset" included). External-content index: the words live only in the index, the text only in
/// `tasks`. Created after the columns it reads exist, so it is separate from `SCHEMA`.
const FTS_SCHEMA: &str = "
CREATE VIRTUAL TABLE IF NOT EXISTS tasks_fts USING fts5(
  subject, sender, addr, body, note, content='tasks', content_rowid='uid', tokenize='unicode61 remove_diacritics 2'
);
CREATE TRIGGER IF NOT EXISTS tasks_fts_ai AFTER INSERT ON tasks BEGIN
  INSERT INTO tasks_fts(rowid, subject, sender, addr, body, note)
  VALUES (new.uid, new.subject, new.sender, new.addr, new.body, new.note);
END;
CREATE TRIGGER IF NOT EXISTS tasks_fts_ad AFTER DELETE ON tasks BEGIN
  INSERT INTO tasks_fts(tasks_fts, rowid, subject, sender, addr, body, note)
  VALUES ('delete', old.uid, old.subject, old.sender, old.addr, old.body, old.note);
END;
CREATE TRIGGER IF NOT EXISTS tasks_fts_au AFTER UPDATE OF subject, sender, addr, body, note ON tasks BEGIN
  INSERT INTO tasks_fts(tasks_fts, rowid, subject, sender, addr, body, note)
  VALUES ('delete', old.uid, old.subject, old.sender, old.addr, old.body, old.note);
  INSERT INTO tasks_fts(rowid, subject, sender, addr, body, note)
  VALUES (new.uid, new.subject, new.sender, new.addr, new.body, new.note);
END;
";

/// What you type, turned into a query the index cannot misread: only letters and digits survive, each word is quoted, the
/// last one also matches as a prefix. At most 100 characters and 8 words; None when nothing is left. A column name, `OR`,
/// `NEAR`, a quote or a `*` in the input is just text.
pub fn fts_query(q: &str) -> Option<String> {
    let text: String = q.chars().take(100).collect();
    let words: Vec<&str> = text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .take(8)
        .collect();
    let (last, rest) = words.split_last()?;
    let mut out: Vec<String> = rest.iter().map(|w| format!("\"{w}\"")).collect();
    out.push(format!("\"{last}\"*"));
    Some(out.join(" "))
}

/// Columns added after the first release. Existing databases are upgraded in place.
const ADDED_COLUMNS: &[(&str, &str)] = &[
    ("body", "TEXT DEFAULT ''"),
    ("needs_reply", "INTEGER DEFAULT 0"),
    ("due", "TEXT DEFAULT ''"),
    ("snooze_until", "REAL DEFAULT 0"),
    ("reply_to", "TEXT DEFAULT ''"),
    ("refs", "TEXT DEFAULT ''"),
    ("auth_fail", "INTEGER DEFAULT 0"),
    ("done_at", "REAL DEFAULT 0"),
    ("attachments", "TEXT DEFAULT '[]'"),
    ("attachment_count", "INTEGER DEFAULT 0"),
    ("unsubscribe", "TEXT DEFAULT ''"),
    ("note", "TEXT DEFAULT ''"),
    ("thread", "TEXT DEFAULT ''"),
    // "replied" when the app closed the task because a reply was found in Sent; empty when the user closed it
    ("closed_by", "TEXT DEFAULT ''"),
    // the user reopened an automatically closed task: do not close it again
    ("keep_open", "INTEGER DEFAULT 0"),
    // the message is no longer in the inbox (moved, archived or deleted somewhere else)
    ("gone", "INTEGER DEFAULT 0"),
    // priority steps added or removed because of the user's earlier corrections of this sender
    ("learned", "INTEGER DEFAULT 0"),
    // "followup" for tasks the app made because a message you sent got no reply
    ("kind", "TEXT DEFAULT ''"),
    // reminders: the time, the repeat, the first occurrence (repeats are counted from it) and the time already announced
    ("remind", "TEXT DEFAULT ''"),
    ("repeat", "TEXT DEFAULT ''"),
    ("repeat_base", "TEXT DEFAULT ''"),
    ("reminded", "TEXT DEFAULT ''"),
    // the first calendar event of the message, as JSON; empty when there is none
    ("event", "TEXT DEFAULT ''"),
];

/// And the follow-up records, which learned which mailbox they belong to.
const ADDED_FOLLOWUP_COLUMNS: &[(&str, &str)] = &[("acct", "INTEGER NOT NULL DEFAULT 0")];

/// Same idea for the corrections table.
const ADDED_OVERRIDE_COLUMNS: &[(&str, &str)] =
    &[("addr", "TEXT DEFAULT ''"), ("uid", "INTEGER DEFAULT 0")];

const SELECT: &str = "SELECT t.uid,t.message_id,t.sender,t.addr,t.subject,t.received,t.priority,t.category,t.action,t.urgent,t.status,t.star,\
t.excerpt,t.why,t.signals,t.needs_reply,t.due,t.snooze_until,t.reply_to,d.body,d.source,d.saved,d.updated,p.level,t.auth_fail,t.attachments,t.attachment_count,t.unsubscribe,t.note,t.thread,t.closed_by,t.gone,t.learned,t.kind,t.remind,t.repeat,t.event,\
CASE WHEN t.kind='followup' THEN COALESCE((SELECT f.acct FROM followups f WHERE f.task_uid=t.uid), 0) END \
FROM tasks t LEFT JOIN drafts d ON d.uid=t.uid LEFT JOIN sender_prefs p ON p.addr=t.addr";

fn domain(a: &str) -> String {
    a.rsplit('@').next().unwrap_or("").to_lowercase()
}

fn row(r: &rusqlite::Row) -> rusqlite::Result<Task> {
    let addr: String = r.get(3)?;
    let reply_to: String = r.get(18)?;
    let auth_fail = r.get::<_, i32>(24)? != 0;
    let attachments: Vec<String> =
        serde_json::from_str(&r.get::<_, String>(25)?).unwrap_or_default();
    let uid: u32 = r.get(0)?;
    let thread = match r.get::<_, String>(29)? {
        t if t.is_empty() => format!("uid:{uid}"),
        t => t,
    };
    let reply_warn = auth_fail || (!reply_to.is_empty() && domain(&reply_to) != domain(&addr));
    let draft = match r.get::<_, Option<String>>(19)? {
        Some(body) => Some(Draft {
            body,
            source: r.get(20)?,
            saved: r.get::<_, i32>(21)? != 0,
            updated: r.get::<_, Option<f64>>(22)?.unwrap_or(0.0),
        }),
        None => None,
    };
    Ok(Task {
        uid: r.get(0)?,
        message_id: r.get(1)?,
        sender: r.get(2)?,
        addr,
        subject: r.get(4)?,
        received: r.get(5)?,
        priority: r.get(6)?,
        category: r.get(7)?,
        action: r.get(8)?,
        urgent: r.get::<_, i32>(9)? != 0,
        status: r.get(10)?,
        star: r.get::<_, i32>(11)? != 0,
        excerpt: r.get(12)?,
        why: r.get(13)?,
        signals: serde_json::from_str(&r.get::<_, String>(14)?).unwrap_or_default(),
        needs_reply: r.get::<_, i32>(15)? != 0,
        due: r.get(16)?,
        snooze_until: r.get(17)?,
        reply_to,
        reply_warn,
        attachments,
        attachment_count: r.get(26)?,
        unsubscribe: r.get(27)?,
        note: r.get(28)?,
        thread,
        closed_by: r.get(30)?,
        gone: r.get::<_, i32>(31)? != 0,
        learned: r.get(32)?,
        draft,
        pref: r.get::<_, Option<String>>(23)?.unwrap_or_default(),
        kind: r.get(33)?,
        remind: r.get(34)?,
        repeat: r.get(35)?,
        event: serde_json::from_str(&r.get::<_, Option<String>>(36)?.unwrap_or_default()).ok(),
        mailbox: if uid < LOCAL_UID_BASE {
            Some(mailbox_of(uid))
        } else {
            r.get::<_, Option<u32>>(37)?
        },
    })
}

/// A delete in an FTS5 index only writes a tombstone: the removed words stay readable in the file until a later merge,
/// which for an old index may never come. With `secure-delete` the index overwrites them. The setting lives in the index's
/// own config table, so it is written once, not on every open (an open that writes can fail when another thread holds
/// the database).
fn ensure_secure_delete(db: &Connection) -> anyhow::Result<()> {
    let set: Option<i64> = db
        .query_row(
            "SELECT v FROM tasks_fts_config WHERE k='secure-delete'",
            [],
            |r| r.get(0),
        )
        .optional()?;
    if set != Some(1) {
        db.execute(
            "INSERT INTO tasks_fts(tasks_fts, rank) VALUES('secure-delete', 1)",
            [],
        )?;
    }
    Ok(())
}

/// Adds a column unless it is there. Every mailbox's sync opens its own connection, so two of them can reach a database that
/// needs the column at the same moment; the loser's `ALTER` then fails with "duplicate column name", which is the answer
/// it was after.
fn add_column(db: &Connection, table: &str, name: &str, def: &str) -> anyhow::Result<()> {
    match db.execute(&format!("ALTER TABLE {table} ADD COLUMN {name} {def}"), []) {
        Err(e) if e.to_string().contains("duplicate column name") => Ok(()),
        r => r.map(|_| ()).map_err(Into::into),
    }
}

fn has_index(db: &Connection) -> bool {
    db.query_row(
        "SELECT 1 FROM sqlite_master WHERE name='tasks_fts'",
        [],
        |_| Ok(()),
    )
    .is_ok()
}

fn migrate(db: &Connection) -> anyhow::Result<()> {
    let have: Vec<String> = db
        .prepare("PRAGMA table_info(tasks)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(Result::ok)
        .collect();
    for (name, def) in ADDED_COLUMNS {
        if !have.iter().any(|h| h == name) {
            add_column(db, "tasks", name, def)?;
        }
    }
    let have: Vec<String> = db
        .prepare("PRAGMA table_info(followups)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(Result::ok)
        .collect();
    for (name, def) in ADDED_FOLLOWUP_COLUMNS {
        if !have.iter().any(|h| h == name) {
            add_column(db, "followups", name, def)?;
        }
    }
    let have: Vec<String> = db
        .prepare("PRAGMA table_info(overrides)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(Result::ok)
        .collect();
    for (name, def) in ADDED_OVERRIDE_COLUMNS {
        if !have.iter().any(|h| h == name) {
            add_column(db, "overrides", name, def)?;
        }
    }
    Ok(())
}

/// The database was written by a newer version of the app. It is refused, never "repaired".
#[derive(Debug)]
pub struct TooNew(pub i32);
impl std::fmt::Display for TooNew {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "This data was created by a newer version of Inbox Agent (format {}). Update the app, or move the data folder aside to start fresh.",
            self.0
        )
    }
}
impl std::error::Error for TooNew {}

/// The integrity check found damage.
#[derive(Debug)]
struct Corrupt(String);
impl std::fmt::Display for Corrupt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "the local database failed its integrity check: {}",
            self.0
        )
    }
}
impl std::error::Error for Corrupt {}

/// Damage to the file itself, as opposed to a busy, locked, full or read-only database.
fn is_corruption(e: &anyhow::Error) -> bool {
    use rusqlite::ErrorCode::{DatabaseCorrupt, NotADatabase};
    e.downcast_ref::<Corrupt>().is_some()
        || e.downcast_ref::<rusqlite::Error>().is_some_and(|r| {
            matches!(r, rusqlite::Error::SqliteFailure(f, _) if matches!(f.code, DatabaseCorrupt | NotADatabase))
        })
}

/// Removes files the app leaves beside the database that can hold mail text: the pre-upgrade backup, damaged
/// databases that were set aside, and the crash log. Used by "Sign out and reset".
pub fn remove_leftover_files(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let n = e.file_name().to_string_lossy().to_string();
        if n == "crash.log" || n.starts_with("agent.db.") {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

impl Store {
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        // Every mailbox's sync, its write-back, the scheduler and the window open their own connection, often at the same
        // moment (at start-up, and after every settings save). Getting a database ready (switching it to WAL, creating
        // tables, adding columns, building the index) is where connections trip over each other, so that part takes turns.
        // Ordinary reads and writes afterwards do not.
        static SETUP: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _turn = SETUP.lock().unwrap_or_else(|e| e.into_inner());
        let db = Connection::open(path)?;
        db.busy_timeout(std::time::Duration::from_secs(5))?;
        let _ = db.query_row("PRAGMA journal_mode=WAL", [], |_| Ok(()));
        // deleted rows are overwritten with zeros, so a wipe does not leave mail text in free pages
        db.pragma_update(None, "secure_delete", "ON")?;
        // the app replaces rows (INSERT OR REPLACE); a replace must delete the old row's index entry too
        db.pragma_update(None, "recursive_triggers", "ON")?;
        let version: i32 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version > SCHEMA_VERSION {
            return Err(TooNew(version).into());
        }
        if version < SCHEMA_VERSION {
            let has_data = db
                .query_row("SELECT 1 FROM sqlite_master WHERE name='tasks'", [], |_| {
                    Ok(())
                })
                .is_ok();
            if has_data {
                // keep one copy of the data as it was before this upgrade touches it
                let _ = db.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()));
                let mut bak = path.as_os_str().to_owned();
                bak.push(".bak");
                let _ = std::fs::copy(path, bak);
            }
        }
        let had_index = has_index(&db);
        db.execute_batch(SCHEMA)?;
        migrate(&db)?;
        db.execute_batch(FTS_SCHEMA)?;
        ensure_secure_delete(&db)?;
        if !had_index {
            // a database from before the index existed: index what is already there
            db.execute("INSERT INTO tasks_fts(tasks_fts) VALUES('rebuild')", [])?;
        }
        if version != SCHEMA_VERSION {
            db.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        }
        Ok(Self { db, scope: None })
    }

    /// Opens the database; if the file is damaged, moves it (and its journal files) aside as
    /// `<name>.corrupt-<time>` and starts a fresh one. Returns the new location of the damaged file when that
    /// happened. Only real corruption is treated this way: a busy or locked database, a full disk or a read-only
    /// folder is an ordinary error and the file is left exactly where it is. A database from a newer app version
    /// is an error and is never moved.
    pub fn open_recovering(path: &Path) -> anyhow::Result<(Self, Option<std::path::PathBuf>)> {
        let checked = Self::open(path).and_then(|s| {
            let verdict: String = s.db.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
            if verdict == "ok" {
                Ok(s)
            } else {
                Err(Corrupt(verdict).into())
            }
        });
        match checked {
            Ok(s) => Ok((s, None)),
            Err(e) if is_corruption(&e) && path.exists() => {
                let stamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_secs());
                let name = |suffix: &str| {
                    let mut p = path.as_os_str().to_owned();
                    p.push(suffix);
                    std::path::PathBuf::from(p)
                };
                let moved = name(&format!(".corrupt-{stamp}"));
                std::fs::rename(path, &moved)?;
                for ext in ["-wal", "-shm"] {
                    let _ = std::fs::rename(name(ext), name(&format!(".corrupt-{stamp}{ext}")));
                }
                let fresh = Self::open(path)?;
                fresh.log("the local database was damaged; it was set aside and rebuilt, and your mail will sync again")?;
                Ok((fresh, Some(moved)))
            }
            Err(e) => Err(e),
        }
    }

    pub fn memory() -> anyhow::Result<Self> {
        let db = Connection::open_in_memory()?;
        db.pragma_update(None, "recursive_triggers", "ON")?;
        db.execute_batch(SCHEMA)?;
        migrate(&db)?;
        db.execute_batch(FTS_SCHEMA)?;
        ensure_secure_delete(&db)?;
        Ok(Self { db, scope: None })
    }

    /// Ids of the tasks that match what the user typed, best match first, at most 500. See `fts_query`.
    pub fn search(&self, q: &str) -> anyhow::Result<Vec<u32>> {
        let Some(query) = fts_query(q) else {
            return Ok(vec![]);
        };
        let mut st = self.db.prepare(
            "SELECT rowid FROM tasks_fts WHERE tasks_fts MATCH ?1 ORDER BY rank LIMIT 500",
        )?;
        let rows = st.query_map([query], |r| r.get::<_, u32>(0))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// The same database, working for mailbox `mailbox`: the numbers the sync passes in and gets back are IMAP UIDs of that
    /// mailbox, its counters are its own, and what it resets or lists is that mailbox's alone.
    pub fn scoped(mut self, mailbox: u32) -> Self {
        self.scope = Some(mailbox);
        self
    }

    fn mb(&self) -> u32 {
        self.scope.unwrap_or(0)
    }

    /// The `meta` name for one of this mailbox's counters (`last_uid`, or `last_uid@2` for the third mailbox).
    pub fn mkey(&self, base: &str) -> String {
        match self.mb() {
            0 => base.to_string(),
            n => format!("{base}@{n}"),
        }
    }

    /// The task id of IMAP message `uid` in this store's mailbox; an error when the number is too large for the layout.
    fn gid(&self, uid: u32) -> anyhow::Result<u32> {
        if uid >= MAILBOX_SPAN {
            anyhow::bail!("message number {uid} is too large for this app to track");
        }
        Ok(global_uid(self.mb(), uid))
    }

    /// The block of task ids that belong to mailbox `n`.
    fn block(n: u32) -> (u32, u32) {
        (n * MAILBOX_SPAN, (n + 1) * MAILBOX_SPAN)
    }

    pub fn seen(&self, uid: u32) -> bool {
        let Ok(id) = self.gid(uid) else {
            return false;
        };
        self.db
            .query_row("SELECT 1 FROM tasks WHERE uid=?1", [id], |_| Ok(()))
            .is_ok()
    }

    pub fn get(&self, uid: u32) -> Option<Task> {
        self.db
            .query_row(&format!("{SELECT} WHERE t.uid=?1"), [uid], row)
            .optional()
            .ok()
            .flatten()
    }

    /// Full text of the message, kept only to draft replies. Not part of `Task` so it is never sent to the UI.
    pub fn body(&self, uid: u32) -> String {
        self.db
            .query_row("SELECT body FROM tasks WHERE uid=?1", [uid], |r| r.get(0))
            .unwrap_or_default()
    }

    /// Threading headers used when a reply draft is saved to the mailbox.
    pub fn references(&self, uid: u32) -> String {
        self.db
            .query_row("SELECT refs FROM tasks WHERE uid=?1", [uid], |r| r.get(0))
            .unwrap_or_default()
    }

    /// Stores a triaged message. The sender's saved preference (important / mute) overrides the
    /// model's priority. Returns the triage as stored, so alerts use the final priority.
    pub fn add(&self, e: &Email, t: &Triage, s: &Signals) -> anyhow::Result<Triage> {
        let uid = self.gid(e.uid)?; // the task id: this mailbox's block plus the IMAP UID
        let mut t = t.clone();
        let pref = self.pref(&e.sender_addr);
        match pref.as_deref() {
            Some("important") if t.priority < 4 && !e.auth_fail => {
                t.priority = 4;
                t.why = format!("{} (sender marked important)", t.why.trim_end_matches('.'));
            }
            Some("mute") => {
                t.priority = 1;
                t.urgent = false;
                t.needs_reply = false;
                t.why = "Sender is muted.".into();
            }
            _ => {}
        }
        // The user's own rules. A sender with a saved preference skips them (that is the more specific instruction).
        // Mail that failed its sender check only receives what lowers it, so a forged From address cannot use an
        // "this address matters" rule to climb.
        let (mut by_rule_priority, mut file) = (false, false);
        if pref.is_none() {
            let all = self.rules();
            if let Some(r) = rules::first_match(&all, &e.sender_addr, &e.subject) {
                let mut note = String::new();
                if let Some(p) = r.priority {
                    if p <= t.priority || rules::may_raise(r, e.auth_fail, s.known || s.reply) {
                        t.priority = p;
                        if p <= 3 {
                            t.urgent = false; // urgent bypasses quiet hours; "this is unimportant" must not leave it on
                        }
                        by_rule_priority = true;
                        note = format!("A rule of yours set the priority to {p}.");
                    }
                }
                if let Some(c) = &r.category {
                    t.category = c.clone();
                    if note.is_empty() {
                        note = format!(
                            "{} (category set by a rule of yours).",
                            t.why.trim_end_matches('.')
                        );
                    }
                }
                if r.file {
                    // filed mail is ranked low and never alerts or gets an automatic draft
                    t.priority = t.priority.min(2);
                    t.urgent = false;
                    t.needs_reply = false;
                    file = true;
                    note = "Filed by a rule of yours.".into();
                }
                if !note.is_empty() {
                    t.why = note;
                }
            }
        }
        // What the user taught by correcting this sender before. Only the rules ranker is shifted (a model already
        // sees the corrections in its prompt), never a sender with an explicit preference or a rule that set the
        // priority, never mail that failed its sender check, and it never makes anything urgent.
        let mut learned = 0;
        if pref.is_none()
            && !by_rule_priority
            && !file
            && !e.auth_fail
            && t.why.starts_with("Rule-based")
        {
            let shift = self.learned_shift(&e.sender_addr);
            if shift != 0 {
                let new = (t.priority as i32 + shift).clamp(1, 5) as u8;
                learned = new as i32 - t.priority as i32;
                if learned != 0 {
                    t.priority = new;
                    t.why = format!("{} Adjusted from your corrections.", t.why.trim_end());
                }
            }
        }
        // an invitation with no other date gives the task the day of the event, when that is still ahead
        if t.due.is_empty() {
            let today = chrono::Local::now().format("%Y-%m-%d").to_string();
            if let Some(day) = e
                .event
                .as_ref()
                .and_then(|ev| crate::calendar::upcoming_day(ev, &today))
            {
                t.due = day;
            }
        }
        let event_json = match &e.event {
            Some(ev) => serde_json::to_string(ev)?,
            None => String::new(),
        };
        self.db.execute(
            "INSERT OR REPLACE INTO tasks(uid,message_id,sender,addr,subject,received,priority,category,action,urgent,excerpt,why,signals,body,needs_reply,due,reply_to,refs,auth_fail,attachments,attachment_count,unsubscribe,thread,learned,event)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24,?25)",
            params![
                uid, e.message_id, e.sender_name, e.sender_addr, e.subject, e.received, t.priority, t.category,
                t.action, t.urgent as i32, e.excerpt, t.why, serde_json::to_string(s)?, e.body,
                t.needs_reply as i32, t.due, e.reply_to, e.references, e.auth_fail as i32,
                serde_json::to_string(&e.attachments)?, e.attachment_count as i64, e.unsubscribe, e.thread,
                learned, event_json
            ],
        )?;
        // news in a snoozed conversation wakes it: the snooze was for the old state of the thread
        if !e.thread.is_empty() && !e.auth_fail {
            let (lo, hi) = Self::block(mailbox_of(uid)); // a conversation belongs to one mailbox
            self.db.execute(
                "UPDATE tasks SET snooze_until=0 WHERE thread=?1 AND status='open' AND snooze_until>0 AND uid<>?2 AND uid>=?3 AND uid<?4",
                params![e.thread, uid, lo, hi],
            )?;
        }
        self.log(&format!(
            "triaged “{}” → P{}, {}{}{}",
            e.subject,
            t.priority,
            t.category,
            if t.urgent { ", urgent" } else { "" },
            if t.needs_reply { ", needs reply" } else { "" }
        ))?;
        if file {
            self.set_done(uid, true)?; // exactly what pressing X does, including the archive with write-back on
        }
        Ok(t)
    }

    // ---- your rules ----
    pub fn rules(&self) -> Vec<Rule> {
        let Ok(mut q) = self
            .db
            .prepare("SELECT id,field,text,priority,category,file FROM rules ORDER BY ord")
        else {
            return vec![];
        };
        q.query_map([], |r| {
            Ok(Rule {
                id: r.get(0)?,
                field: r.get(1)?,
                text: r.get(2)?,
                priority: r.get(3)?,
                category: r.get(4)?,
                file: r.get::<_, i32>(5)? != 0,
            })
        })
        .map(|rows| rows.filter_map(Result::ok).collect())
        .unwrap_or_default()
    }

    /// Replaces the whole rule list, in order. A list that fails validation changes nothing.
    pub fn set_rules(&self, list: &[Rule]) -> anyhow::Result<()> {
        rules::validate(list)?;
        let tx = self.db.unchecked_transaction()?;
        tx.execute("DELETE FROM rules", [])?;
        for (i, r) in list.iter().enumerate() {
            tx.execute(
                "INSERT INTO rules(id,ord,field,text,priority,category,file) VALUES(?1,?2,?3,?4,?5,?6,?7)",
                params![r.id, i as i64, r.field, r.text.trim(), r.priority, r.category, r.file as i32],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    fn auth_failed(&self, uid: u32) -> bool {
        self.db
            .query_row("SELECT auth_fail FROM tasks WHERE uid=?1", [uid], |r| {
                r.get::<_, i32>(0)
            })
            .unwrap_or(0)
            != 0
    }

    /// Runs the rules over the open mail in the list. `dry` only counts. Tasks with a sender preference, tasks whose
    /// priority the user already corrected by hand and tasks you added yourself are left alone, and nothing here is
    /// recorded as a correction, so rules never teach the ranker.
    pub fn apply_rules(&self, dry: bool) -> anyhow::Result<RuleReport> {
        let mut report = RuleReport::default();
        let all = self.rules();
        if all.is_empty() {
            return Ok(report);
        }
        let corrected: std::collections::HashSet<u32> = {
            let mut q = self
                .db
                .prepare("SELECT DISTINCT uid FROM overrides WHERE uid<>0")?;
            let rows = q.query_map([], |r| r.get::<_, u32>(0))?;
            rows.filter_map(Result::ok).collect()
        };
        for t in self.all_tasks()? {
            if t.status != "open"
                || t.uid >= LOCAL_UID_BASE
                || !t.pref.is_empty()
                || corrected.contains(&t.uid)
            {
                continue;
            }
            let Some(r) = rules::first_match(&all, &t.addr, &t.subject) else {
                continue;
            };
            report.matched += 1;
            let forged = self.auth_failed(t.uid);
            let mut priority = t.priority;
            let mut applied = false; // the rule's priority was allowed to take effect
            if let Some(p) = r.priority {
                if p <= t.priority
                    || rules::may_raise(r, forged, t.signals.known || t.signals.reply)
                {
                    priority = p;
                    applied = true;
                }
            }
            let category = r.category.clone().unwrap_or_else(|| t.category.clone());
            if r.file {
                priority = priority.min(2);
            }
            // a rule that sets a priority of 3 or less takes the urgent flag off, as it does for new mail
            let calms = applied && priority <= 3 && t.urgent;
            if priority == t.priority && category == t.category && !r.file && !calms {
                continue;
            }
            report.changed += 1;
            report.filed += r.file as usize;
            if dry {
                continue;
            }
            let why = if r.file {
                "Filed by a rule of yours.".to_string()
            } else if priority != t.priority {
                format!("A rule of yours set the priority to {priority}.")
            } else if category != t.category {
                format!(
                    "{} (category set by a rule of yours).",
                    t.why.trim_end_matches('.')
                )
            } else {
                t.why.clone()
            };
            self.db.execute(
                "UPDATE tasks SET priority=?1, category=?2, why=?3, learned=CASE WHEN ?1<>?4 THEN 0 ELSE learned END, \
                 urgent=CASE WHEN ?7=1 OR ?5=1 THEN 0 ELSE urgent END, \
                 needs_reply=CASE WHEN ?5=1 THEN 0 ELSE needs_reply END WHERE uid=?6",
                params![priority, category, why, t.priority, r.file as i32, t.uid, calms as i32],
            )?;
            if priority != t.priority {
                self.queue(t.uid, "label", &format!("AI/P{priority}"))?;
            }
            if r.file {
                self.set_done(t.uid, true)?;
            }
        }
        if !dry && report.changed > 0 {
            self.log(&format!(
                "your rules were applied to the list: {} changed, {} filed",
                report.changed, report.filed
            ))?;
        }
        Ok(report)
    }

    pub fn all_tasks(&self) -> anyhow::Result<Vec<Task>> {
        let mut st = self.db.prepare(&format!(
            "{SELECT} ORDER BY t.priority DESC, t.received ASC"
        ))?;
        let rows = st.query_map([], row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    fn label(&self, uid: u32) -> String {
        self.get(uid)
            .map(|t| {
                if t.action.is_empty() {
                    t.subject
                } else {
                    t.action
                }
            })
            .unwrap_or_default()
    }

    // ---- user actions: update locally, log, queue the mailbox change ----
    pub fn set_done(&self, uid: u32, done: bool) -> anyhow::Result<bool> {
        self.set_done_at(uid, done, reminders::now_local())
    }

    /// `set_done` with the clock given. Completing a repeating task of your own does not close it: it moves to the next
    /// occurrence and stays open.
    pub fn set_done_at(&self, uid: u32, done: bool, now: NaiveDateTime) -> anyhow::Result<bool> {
        let Some(t) = self.get(uid) else {
            return Ok(false);
        };
        if done && uid >= LOCAL_UID_BASE && !t.repeat.is_empty() && t.status == "open" {
            return self.advance_repeat(&t, now);
        }
        // Reopening a task the app closed by itself (you had replied): the mailbox was never changed, so there is
        // nothing to unarchive, and the task must not be closed again by the next pass.
        let auto = !done && !t.closed_by.is_empty();
        // a follow-up you reopen stays open, whoever closed it: the app must not close it again when a reply turns up
        let keep = auto || (!done && t.kind == "followup");
        self.db.execute(
            "UPDATE tasks SET status=?1, snooze_until=0, closed_by='', keep_open=CASE WHEN ?3=1 THEN 1 ELSE keep_open END, done_at=CASE WHEN ?1='done' THEN CAST(strftime('%s','now') AS INTEGER) ELSE 0 END WHERE uid=?2",
            params![if done { "done" } else { "open" }, uid, keep as i32],
        )?;
        if !auto {
            self.queue(uid, if done { "archive" } else { "unarchive" }, "")?;
        }
        self.log(&format!(
            "{} “{}”",
            if done { "archived" } else { "reopened" },
            self.label(uid)
        ))?;
        Ok(true)
    }

    pub fn set_star(&self, uid: u32, on: bool) -> anyhow::Result<bool> {
        if self.get(uid).is_none() {
            return Ok(false);
        }
        self.db.execute(
            "UPDATE tasks SET star=?1 WHERE uid=?2",
            params![on as i32, uid],
        )?;
        self.queue(uid, if on { "star" } else { "unstar" }, "")?;
        self.log(&format!(
            "{} “{}”",
            if on { "starred" } else { "unstarred" },
            self.label(uid)
        ))?;
        Ok(true)
    }

    pub fn set_priority(&self, uid: u32, new: u8) -> anyhow::Result<bool> {
        let Some(t) = self.get(uid) else {
            return Ok(false);
        };
        if !(1..=5).contains(&new) {
            return Ok(false);
        }
        if t.priority != new {
            self.db.execute(
                "UPDATE tasks SET priority=?1 WHERE uid=?2",
                params![new, uid],
            )?;
            let forged: bool = self
                .db
                .query_row("SELECT auth_fail FROM tasks WHERE uid=?1", [uid], |r| {
                    r.get::<_, i32>(0)
                })
                .unwrap_or(0)
                != 0;
            // Corrections teach by sender ADDRESS, one per action. A correction of mail that failed its sender
            // check teaches nothing, and changing a task back to what it was (undo) removes the correction it made
            // instead of recording an opposite one.
            if !forged && !t.addr.is_empty() {
                // One correction per task: the priority it had at first, and the priority it has now. Nudging the
                // same mail twice is still one correction, and walking it back to where it started removes it.
                let existing: Option<(i64, u8)> = self
                    .db
                    .query_row(
                        "SELECT id, old_priority FROM overrides WHERE uid=?1 AND uid<>0 AND addr=?2 ORDER BY id DESC LIMIT 1",
                        params![uid, t.addr],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )
                    .optional()?;
                match existing {
                    Some((id, first)) if first == new => {
                        self.db.execute("DELETE FROM overrides WHERE id=?1", [id])?;
                    }
                    Some((id, _)) => {
                        self.db.execute(
                            "UPDATE overrides SET new_priority=?1, at=strftime('%s','now') WHERE id=?2",
                            params![new, id],
                        )?;
                    }
                    None => {
                        self.db.execute(
                            "INSERT INTO overrides(sender,subject,old_priority,new_priority,at,addr,uid) VALUES(?1,?2,?3,?4,strftime('%s','now'),?5,?6)",
                            params![t.sender, t.subject, t.priority, new, t.addr, uid],
                        )?;
                    }
                }
            }
            self.queue(uid, "label", &format!("AI/P{new}"))?;
            self.log(&format!(
                "P{}→P{} “{}” saved as a correction",
                t.priority,
                new,
                self.label(uid)
            ))?;
            if new >= 4 && !t.star {
                self.set_star(uid, true)?;
            }
        }
        Ok(true)
    }

    // ---- snooze ----
    /// Hides a task from the to-do list until `until` (unix seconds). 0 brings it back now.
    pub fn set_snooze(&self, uid: u32, until: f64) -> anyhow::Result<bool> {
        if self.get(uid).is_none() || until < 0.0 {
            return Ok(false);
        }
        self.db.execute(
            "UPDATE tasks SET snooze_until=?1 WHERE uid=?2",
            params![until, uid],
        )?;
        self.log(&format!(
            "{} “{}”",
            if until > 0.0 { "snoozed" } else { "unsnoozed" },
            self.label(uid)
        ))?;
        Ok(true)
    }

    /// Open tasks whose snooze has expired. Their snooze is cleared so each wakes exactly once.
    pub fn wake_due(&self, now: f64) -> anyhow::Result<Vec<Task>> {
        let due: Vec<Task> = self
            .all_tasks()?
            .into_iter()
            .filter(|t| t.status == "open" && t.snooze_until > 0.0 && t.snooze_until <= now)
            .collect();
        for t in &due {
            self.db
                .execute("UPDATE tasks SET snooze_until=0 WHERE uid=?1", [t.uid])?;
            self.log(&format!(
                "“{}” is back from snooze",
                if t.action.is_empty() {
                    &t.subject
                } else {
                    &t.action
                }
            ))?;
        }
        Ok(due)
    }

    // ---- sender preferences ----
    pub fn pref(&self, addr: &str) -> Option<String> {
        self.db
            .query_row(
                "SELECT level FROM sender_prefs WHERE addr=?1",
                [addr],
                |r| r.get(0),
            )
            .optional()
            .ok()
            .flatten()
    }

    /// "important" lifts this sender's mail to at least P4, "mute" drops it to P1 and silences alerts,
    /// "normal" forgets the preference. Open tasks from the sender are updated straight away.
    pub fn set_pref(&self, addr: &str, level: &str) -> anyhow::Result<bool> {
        if addr.is_empty() || !matches!(level, "important" | "mute" | "normal") {
            return Ok(false);
        }
        if level == "normal" {
            self.db
                .execute("DELETE FROM sender_prefs WHERE addr=?1", [addr])?;
            // "Normal" is also the way to start over: whatever was learned about this sender is forgotten
            self.db
                .execute("DELETE FROM overrides WHERE addr=?1", [addr])?;
        } else {
            self.db.execute(
                "INSERT OR REPLACE INTO sender_prefs VALUES(?1,?2)",
                params![addr, level],
            )?;
            let sql = if level == "important" {
                "UPDATE tasks SET priority=MAX(priority,4) WHERE addr=?1 AND status='open'"
            } else {
                "UPDATE tasks SET priority=1, urgent=0 WHERE addr=?1 AND status='open'"
            };
            self.db.execute(sql, [addr])?;
        }
        self.log(&format!("sender {addr} set to {level}"))?;
        Ok(true)
    }

    // ---- drafts ----
    pub fn set_draft(&self, uid: u32, body: &str, source: &str) -> anyhow::Result<bool> {
        if self.get(uid).is_none()
            || body.chars().count() > MAX_DRAFT_CHARS
            || !matches!(source, "ai" | "template" | "user")
        {
            return Ok(false);
        }
        self.db.execute(
            "INSERT OR REPLACE INTO drafts(uid,body,source,saved,updated) VALUES(?1,?2,?3,0,strftime('%s','now'))",
            params![uid, body, source],
        )?;
        self.log(&format!(
            "{} reply draft for “{}”",
            if source == "user" { "edited" } else { "wrote" },
            self.label(uid)
        ))?;
        Ok(true)
    }

    pub fn delete_draft(&self, uid: u32) -> anyhow::Result<bool> {
        let n = self.db.execute("DELETE FROM drafts WHERE uid=?1", [uid])?;
        Ok(n > 0)
    }

    pub fn mark_draft_saved(&self, uid: u32) -> anyhow::Result<()> {
        self.db
            .execute("UPDATE drafts SET saved=1 WHERE uid=?1", [uid])?;
        Ok(())
    }

    /// Takes one slot from today's automatic-draft budget. Returns false when the cap is reached. The count lives in
    /// `meta`, so discarding or editing a draft cannot hand the budget back.
    pub fn reserve_ai_draft(&self, cap: usize, today: &str) -> anyhow::Result<bool> {
        let key = format!("drafts_{today}");
        let used: usize = self
            .meta_get(&key)
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        if used >= cap {
            return Ok(false);
        }
        self.meta_set(&key, &(used + 1).to_string())?;
        Ok(true)
    }

    // ---- tasks that close themselves ----
    /// Open mail tasks of this store's mailbox (not the ones you added) with their Message-ID, for the reply and
    /// left-the-inbox checks: (IMAP uid, message_id, keep_open, gone). Newest first.
    pub fn open_mail_tasks(&self, limit: i64) -> Vec<(u32, String, bool, bool)> {
        let (lo, hi) = Self::block(self.mb());
        let Ok(mut q) = self.db.prepare(
            "SELECT uid, message_id, keep_open, gone FROM tasks WHERE status='open' AND uid>=?1 AND uid<?2 AND message_id<>'' ORDER BY received DESC LIMIT ?3",
        ) else {
            return vec![];
        };
        q.query_map(params![lo, hi, limit], |r| {
            Ok((
                imap_uid(r.get(0)?),
                r.get(1)?,
                r.get::<_, i32>(2)? != 0,
                r.get::<_, i32>(3)? != 0,
            ))
        })
        .map(|rows| rows.filter_map(Result::ok).collect())
        .unwrap_or_default()
    }

    /// Closes a task because the mailbox shows it is dealt with (`by` = "replied"). Never touches the mailbox, and
    /// leaves tasks the user reopened alone. Returns whether anything changed.
    pub fn auto_close(&self, uid: u32, by: &str) -> anyhow::Result<bool> {
        let uid = self.gid(uid)?;
        let n = self.db.execute(
            "UPDATE tasks SET status='done', snooze_until=0, closed_by=?1, done_at=CAST(strftime('%s','now') AS INTEGER) WHERE uid=?2 AND status='open' AND keep_open=0 AND uid<?3",
            params![by, uid, LOCAL_UID_BASE],
        )?;
        if n > 0 {
            self.log(&format!("closed “{}”: you replied", self.label(uid)))?;
        }
        Ok(n > 0)
    }

    /// Marks a task whose message is no longer in the inbox (or clears the mark if it is back).
    pub fn set_gone(&self, uid: u32, gone: bool) -> anyhow::Result<()> {
        let uid = self.gid(uid)?;
        self.db.execute(
            "UPDATE tasks SET gone=?1 WHERE uid=?2 AND gone<>?1",
            params![gone as i32, uid],
        )?;
        Ok(())
    }

    /// The inbox UIDVALIDITY the stored tasks were numbered under, if known.
    pub fn stored_uid_validity(&self) -> Option<u32> {
        self.meta_get(&self.mkey("uid_validity"))?.parse().ok()
    }

    // ---- editable fields ----
    /// Sets or clears (empty string) the due date of any task. Only a real `YYYY-MM-DD` is accepted.
    pub fn set_due(&self, uid: u32, due: &str) -> anyhow::Result<bool> {
        let Some(t) = self.get(uid) else {
            return Ok(false);
        };
        if !(due.is_empty() || crate::dates::valid_iso(due)) {
            return Ok(false);
        }
        self.db
            .execute("UPDATE tasks SET due=?1 WHERE uid=?2", params![due, uid])?;
        // a repeat that has no reminder counts from the due date, so it follows the date you chose
        if uid >= LOCAL_UID_BASE && !t.repeat.is_empty() && t.remind.is_empty() {
            if due.is_empty() {
                self.db.execute(
                    "UPDATE tasks SET repeat='', repeat_base='' WHERE uid=?1",
                    [uid],
                )?;
            } else {
                self.db.execute(
                    "UPDATE tasks SET repeat_base=?1 WHERE uid=?2",
                    params![format!("{due} 00:00"), uid],
                )?;
            }
        }
        Ok(true)
    }

    /// Renames a task you added yourself. A message's subject is not ours to rewrite, so mail tasks are refused.
    pub fn set_title(&self, uid: u32, title: &str) -> anyhow::Result<bool> {
        let title = title.trim();
        if uid < LOCAL_UID_BASE
            || self.get(uid).is_none()
            || title.is_empty()
            || title.chars().count() > 200
            || title.chars().any(|c| c.is_control())
        {
            return Ok(false);
        }
        self.db.execute(
            "UPDATE tasks SET subject=?1, action=?1 WHERE uid=?2",
            params![title, uid],
        )?;
        Ok(true)
    }

    // ---- notes and tasks you add yourself ----
    /// Saves the free-text note on a task (control characters other than line breaks are dropped).
    pub fn set_note(&self, uid: u32, note: &str) -> anyhow::Result<bool> {
        if self.get(uid).is_none() || note.chars().count() > MAX_NOTE_CHARS {
            return Ok(false);
        }
        let clean: String = note
            .chars()
            .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
            .collect();
        self.db
            .execute("UPDATE tasks SET note=?1 WHERE uid=?2", params![clean, uid])?;
        Ok(true)
    }

    /// A task that did not come from an email. Returns its id, or None when the input is not acceptable.
    pub fn add_local_task(
        &self,
        title: &str,
        priority: u8,
        due: &str,
    ) -> anyhow::Result<Option<u32>> {
        let title = title.trim();
        if title.is_empty()
            || title.chars().count() > 200
            || title.chars().any(|c| c.is_control())
            || !(1..=5).contains(&priority)
            || !(due.is_empty() || crate::dates::valid_iso(due))
        {
            return Ok(None);
        }
        // The id is chosen inside the INSERT, so two threads adding at the same moment cannot pick the same one.
        self.db.execute(
            "INSERT INTO tasks(uid,message_id,sender,addr,subject,received,priority,category,action,urgent,excerpt,why,signals,body,needs_reply,due,reply_to,refs,auth_fail)
             VALUES(COALESCE((SELECT MAX(uid)+1 FROM tasks WHERE uid>=?4), ?4),'','You','',?1,strftime('%s','now'),?2,'personal',?1,0,'','Added by you','{}','',0,?3,'','',0)",
            params![title, priority, due, LOCAL_UID_BASE],
        )?;
        let uid = self.db.last_insert_rowid() as u32;
        self.log(&format!("added task “{title}”"))?;
        Ok(Some(uid))
    }

    // ---- reminders and repeats (your own tasks) ----
    /// Sets or clears the reminder and the repeat of a task you added. `at` is a local "YYYY-MM-DD HH:MM" that has not
    /// passed (the minute `now` is in still counts); empty clears it, and a repeat with it. A repeat needs a reminder, or a
    /// due date when `at` is empty (it then only moves the due date).
    pub fn set_reminder(
        &self,
        uid: u32,
        at: &str,
        repeat: &str,
        now: NaiveDateTime,
    ) -> anyhow::Result<()> {
        use anyhow::{anyhow, bail};
        let t = self.get(uid).ok_or_else(|| anyhow!("no such task"))?;
        if uid < LOCAL_UID_BASE {
            bail!("Reminders are for tasks you added yourself. To see a message later, snooze it");
        }
        if t.kind == "followup" {
            bail!("Reminders are for tasks you added yourself, not the ones the app made for you");
        }
        if !reminders::valid_repeat(repeat) {
            bail!("A task can repeat daily, weekly or monthly");
        }
        if at.is_empty() {
            if repeat.is_empty() {
                self.db.execute(
                    "UPDATE tasks SET remind='', repeat='', repeat_base='', reminded='' WHERE uid=?1",
                    [uid],
                )?;
                return Ok(());
            }
            if t.due.is_empty() {
                bail!("A repeat needs a reminder time or a due date");
            }
            self.db.execute(
                "UPDATE tasks SET remind='', repeat=?1, repeat_base=?2, reminded='' WHERE uid=?3",
                params![repeat, format!("{} 00:00", t.due), uid],
            )?;
            return Ok(());
        }
        let when = reminders::parse_local(at).ok_or_else(|| anyhow!("Pick a date and a time"))?;
        if !reminders::year_ok(when) {
            bail!("Pick a date between 2000 and 2100");
        }
        let this_minute = now
            .with_second(0)
            .and_then(|n| n.with_nanosecond(0))
            .unwrap_or(now);
        // choosing the time the task already has (to change only its repeat, say) is not a new request: it may be in the
        // past, and it is not announced again
        let unchanged = t.remind == at;
        if when < this_minute && !unchanged {
            bail!("Pick a time that has not passed");
        }
        let stored: String = self
            .db
            .query_row("SELECT repeat_base FROM tasks WHERE uid=?1", [uid], |r| {
                r.get(0)
            })
            .unwrap_or_default();
        let base = if repeat.is_empty() {
            String::new()
        } else if unchanged && t.repeat == repeat && !stored.is_empty() {
            stored
        } else {
            at.to_string()
        };
        self.db.execute(
            "UPDATE tasks SET remind=?1, repeat=?2, repeat_base=?3, reminded=CASE WHEN remind=?1 THEN reminded ELSE '' END WHERE uid=?4",
            params![at, repeat, base, uid],
        )?;
        Ok(())
    }

    /// Open tasks of yours whose reminder time has come and has not been announced for that time yet. Each is marked
    /// announced before it is returned, so a restart never repeats a notice.
    pub fn due_reminders(&self, now: NaiveDateTime) -> anyhow::Result<Vec<Task>> {
        let stamp = reminders::fmt_local(now);
        let due: Vec<(u32, String)> = {
            let mut q = self.db.prepare(
                "SELECT uid, remind FROM tasks WHERE uid>=?1 AND status='open' AND remind<>'' AND remind<=?2 AND reminded<>remind ORDER BY remind, uid",
            )?;
            let rows = q.query_map(params![LOCAL_UID_BASE, stamp], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })?;
            rows.filter_map(Result::ok).collect()
        };
        let mut out = vec![];
        for (uid, remind) in due {
            // only the time that was selected is marked: if another thread changed it in between, the new time is
            // left alone and gets its own announcement
            let n = self.db.execute(
                "UPDATE tasks SET reminded=remind WHERE uid=?1 AND remind=?2 AND reminded<>remind",
                params![uid, remind],
            )?;
            if n == 0 {
                continue;
            }
            if let Some(t) = self.get(uid) {
                out.push(t);
            }
        }
        Ok(out)
    }

    /// Completing a repeating task: it stays open, and its reminder (and due date, if it had one) move to the first
    /// occurrence after both now and the current reminder.
    fn advance_repeat(&self, t: &Task, now: NaiveDateTime) -> anyhow::Result<bool> {
        let stored: String = self
            .db
            .query_row("SELECT repeat_base FROM tasks WHERE uid=?1", [t.uid], |r| {
                r.get(0)
            })
            .unwrap_or_default();
        let current = reminders::parse_local(&t.remind);
        let anchor = reminders::parse_local(&stored)
            .or(current)
            .or_else(|| reminders::parse_local(&format!("{} 00:00", t.due)));
        let Some(base) = anchor else {
            return Ok(false);
        };
        // the current reminder, or for a repeat that has only a due date that date, is the floor: finishing early
        // moves past it, and finishing late moves past now
        let floor = current.or_else(|| reminders::parse_local(&format!("{} 00:00", t.due)));
        let after = floor.filter(|c| *c > now).unwrap_or(now);
        let Some(next) = reminders::next_occurrence(base, &t.repeat, after) else {
            return Ok(false);
        };
        // a due date that sat a few days after the reminder keeps that distance
        let due = match (crate::dates::parse_iso(&t.due), current) {
            (Some(d), Some(c)) => (next.date() + (d - c.date()))
                .format("%Y-%m-%d")
                .to_string(),
            (Some(_), None) => next.date().format("%Y-%m-%d").to_string(),
            (None, _) => String::new(),
        };
        let remind = if t.remind.is_empty() {
            String::new()
        } else {
            reminders::fmt_local(next)
        };
        self.db.execute(
            "UPDATE tasks SET remind=?1, due=?2, reminded='', snooze_until=0 WHERE uid=?3",
            params![remind, due, t.uid],
        )?;
        self.log(&format!(
            "“{}” done for now; next on {}",
            self.label(t.uid),
            next.format(if t.remind.is_empty() {
                "%-d %b"
            } else {
                "%-d %b %H:%M"
            })
        ))?;
        Ok(true)
    }

    // ---- follow-ups: "waiting on a reply" ----
    /// What was decided about a sent message: "waiting" (a task was made), "answered", or None (never looked at).
    pub fn followup_state(&self, message_id: &str) -> Option<String> {
        self.db
            .query_row(
                "SELECT state FROM followups WHERE message_id=?1",
                [message_id],
                |r| r.get(0),
            )
            .optional()
            .ok()
            .flatten()
    }

    /// Remembers the outcome for a sent message so it is never looked at again. `at` is when it was first seen.
    pub fn mark_followup(&self, message_id: &str, state: &str, at: f64) -> anyhow::Result<()> {
        self.db.execute(
            "INSERT INTO followups(message_id,state,at,acct) VALUES(?1,?2,?3,?4) ON CONFLICT(message_id) DO UPDATE SET state=excluded.state",
            params![message_id, state, at, self.mb()],
        )?;
        Ok(())
    }

    /// Makes the task for a sent message nobody answered. Returns None when this message was looked at before, so a
    /// task you closed is never made again.
    pub fn add_followup(&self, m: &SentMsg, now: f64) -> anyhow::Result<Option<u32>> {
        if self.followup_state(&m.message_id).is_some() {
            return Ok(None);
        }
        let subject: String = m.subject.chars().take(150).collect();
        let title = format!("Follow up: {subject}");
        let day = chrono::DateTime::from_timestamp(m.sent_at as i64, 0)
            .map(|d| d.with_timezone(&chrono::Local).format("%-d %b").to_string())
            .unwrap_or_default();
        let excerpt = format!(
            "You wrote to {} on {day} about “{subject}”. Nobody has replied yet.",
            m.to
        );
        // the task and its record are written together or not at all
        let tx = self.db.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO tasks(uid,message_id,sender,addr,subject,received,priority,category,action,urgent,excerpt,why,signals,body,needs_reply,due,reply_to,refs,auth_fail,kind)
             VALUES(COALESCE((SELECT MAX(uid)+1 FROM tasks WHERE uid>=?4), ?4),'','You','',?1,?2,3,'personal',?1,0,?3,'Nobody has replied to something you sent','{}','',0,'','','',0,'followup')",
            params![title, now, excerpt, LOCAL_UID_BASE],
        )?;
        let uid = tx.last_insert_rowid() as u32;
        tx.execute(
            "INSERT INTO followups(message_id,state,at,task_uid,acct) VALUES(?1,'waiting',?2,?3,?4)",
            params![m.message_id, now, uid, self.mb()],
        )?;
        tx.commit()?;
        self.log(&format!("waiting on a reply: “{subject}”"))?;
        Ok(Some(uid))
    }

    /// Has a reply to this sent message already arrived and been stored? Mail the app has read is the best evidence it
    /// has: it covers a reply that was archived or moved since (by the app's own "done" included), and costs the server
    /// nothing. Not counted: the sent message's own copy (a Bcc to yourself), anything from one of your own addresses
    /// (a nudge you sent in the same thread), and automatic replies (out-of-office notices carry bulk headers).
    pub fn has_local_reply(&self, message_id: &str, own: &[String]) -> bool {
        let Ok(mut q) = self.db.prepare(
            "SELECT refs, thread, signals, addr FROM tasks WHERE uid<?1 AND message_id<>?2 AND (thread=?2 OR instr(refs, ?2)>0)",
        ) else {
            return false;
        };
        q.query_map(params![LOCAL_UID_BASE, message_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })
        .map(|rows| {
            rows.filter_map(Result::ok)
                .any(|(refs, thread, signals, addr)| {
                    (thread == message_id || refs.split(' ').any(|t| t == message_id))
                        && !own.contains(&addr)
                        && !serde_json::from_str::<Signals>(&signals).is_ok_and(|g| g.bulk)
                })
        })
        .unwrap_or(false)
    }

    /// Open follow-up tasks the app may still close by itself: (task id, Message-ID of the sent message).
    pub fn open_followups(&self) -> Vec<(u32, String)> {
        let Ok(mut q) = self.db.prepare(
            "SELECT t.uid, f.message_id FROM tasks t JOIN followups f ON f.task_uid=t.uid \
             WHERE t.kind='followup' AND t.status='open' AND t.keep_open=0 AND (?1 IS NULL OR f.acct=?1) ORDER BY t.uid",
        ) else {
            return vec![];
        };
        q.query_map([self.scope], |r| Ok((r.get(0)?, r.get(1)?)))
            .map(|rows| rows.filter_map(Result::ok).collect())
            .unwrap_or_default()
    }

    /// Closes a follow-up because a reply arrived. Leaves tasks you reopened, and anything that is not a follow-up,
    /// alone. Never touches the mailbox.
    pub fn close_followup(&self, uid: u32) -> anyhow::Result<bool> {
        let n = self.db.execute(
            "UPDATE tasks SET status='done', snooze_until=0, closed_by='replied', done_at=CAST(strftime('%s','now') AS INTEGER) \
             WHERE uid=?1 AND kind='followup' AND status='open' AND keep_open=0",
            [uid],
        )?;
        if n > 0 {
            self.db.execute(
                "UPDATE followups SET state='answered' WHERE task_uid=?1",
                [uid],
            )?;
            self.log(&format!("closed “{}”: they replied", self.label(uid)))?;
        }
        Ok(n > 0)
    }

    /// Forgets follow-up records older than 60 days. A sent message that old is outside the 14-day window anyway. A record
    /// whose task still exists stays as long as the task does: it says which mailbox the task belongs to, and it is how the
    /// task is found again when that mailbox is removed.
    pub fn prune_followups(&self, now: f64) -> anyhow::Result<usize> {
        Ok(self.db.execute(
            "DELETE FROM followups WHERE at < ?1 AND (task_uid = 0 OR task_uid NOT IN (SELECT uid FROM tasks))",
            [now - 60.0 * 86400.0],
        )?)
    }

    // ---- outbox ----
    pub fn queue(&self, uid: u32, op: &str, arg: &str) -> anyhow::Result<()> {
        if uid >= LOCAL_UID_BASE {
            return Ok(()); // your own tasks have no message in a mailbox to change
        }
        self.db.execute(
            "INSERT INTO outbox(uid,op,arg,at) VALUES(?1,?2,?3,strftime('%s','now'))",
            params![uid, op, arg],
        )?;
        Ok(())
    }

    pub fn pending(&self, limit: i64) -> Vec<OutboxItem> {
        // a store for one mailbox sees only that mailbox's items; the plain store sees them all
        let (lo, hi) = match self.scope {
            Some(n) => Self::block(n),
            None => (0, u32::MAX),
        };
        let Ok(mut st) = self.db.prepare(
            "SELECT id,uid,op,arg,tries FROM outbox WHERE uid>=?2 AND uid<?3 ORDER BY id LIMIT ?1",
        ) else {
            return vec![];
        };
        st.query_map(params![limit, lo, hi], |r| {
            Ok(OutboxItem {
                id: r.get(0)?,
                uid: r.get(1)?,
                op: r.get(2)?,
                arg: r.get(3)?,
                tries: r.get(4)?,
            })
        })
        .map(|rows| rows.filter_map(Result::ok).collect())
        .unwrap_or_default()
    }

    pub fn ack(&self, id: i64) -> anyhow::Result<()> {
        self.db.execute("DELETE FROM outbox WHERE id=?1", [id])?;
        Ok(())
    }

    pub fn fail(&self, id: i64) -> anyhow::Result<()> {
        self.db
            .execute("UPDATE outbox SET tries=tries+1 WHERE id=?1", [id])?;
        Ok(())
    }

    // ---- log / overrides / cursor / meta ----
    pub fn log(&self, msg: &str) -> anyhow::Result<()> {
        self.db.execute(
            "INSERT INTO log(at,msg) VALUES(strftime('%s','now'),?1)",
            [msg],
        )?;
        Ok(())
    }

    pub fn recent_log(&self, n: i64) -> Vec<(f64, String)> {
        let Ok(mut st) = self
            .db
            .prepare("SELECT at,msg FROM log ORDER BY id DESC LIMIT ?1")
        else {
            return vec![];
        };
        st.query_map([n], |r| Ok((r.get::<_, f64>(0)?, r.get::<_, String>(1)?)))
            .map(|rows| rows.filter_map(Result::ok).collect())
            .unwrap_or_default()
    }

    /// Priority steps to add to later mail from `addr`, learned from the user's last three corrections of it.
    /// Needs at least two, all in the same direction; the size is their rounded average, between 1 and 2 steps.
    pub fn learned_shift(&self, addr: &str) -> i32 {
        if addr.is_empty() {
            return 0;
        }
        let Ok(mut q) = self.db.prepare(
            "SELECT new_priority-old_priority FROM overrides WHERE addr=?1 ORDER BY id DESC LIMIT 3",
        ) else {
            return 0;
        };
        let deltas: Vec<i32> = q
            .query_map([addr], |r| r.get(0))
            .map(|rows| rows.filter_map(Result::ok).collect())
            .unwrap_or_default();
        if deltas.len() < 2 || !(deltas.iter().all(|d| *d > 0) || deltas.iter().all(|d| *d < 0)) {
            return 0;
        }
        let mean = deltas.iter().sum::<i32>() as f64 / deltas.len() as f64;
        let steps = (mean.abs().round() as i32).clamp(1, 2);
        if mean > 0.0 {
            steps
        } else {
            -steps
        }
    }

    pub fn recent_overrides(&self, n: i64) -> Vec<Override> {
        let Ok(mut st) = self
            .db
            .prepare("SELECT sender,subject,old_priority,new_priority FROM overrides ORDER BY id DESC LIMIT ?1")
        else {
            return vec![];
        };
        st.query_map([n], |r| {
            Ok(Override {
                sender: r.get(0)?,
                subject: r.get(1)?,
                old: r.get(2)?,
                new: r.get(3)?,
            })
        })
        .map(|rows| rows.filter_map(Result::ok).collect())
        .unwrap_or_default()
    }

    pub fn meta_get(&self, key: &str) -> Option<String> {
        self.db
            .query_row("SELECT value FROM meta WHERE key=?1", [key], |r| r.get(0))
            .optional()
            .ok()
            .flatten()
    }

    pub fn meta_set(&self, key: &str, value: &str) -> anyhow::Result<()> {
        self.db.execute(
            "INSERT OR REPLACE INTO meta VALUES(?1,?2)",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn last_uid(&self) -> u32 {
        self.meta_get(&self.mkey("last_uid"))
            .and_then(|v| v.parse().ok())
            .unwrap_or(0)
    }

    pub fn set_last_uid(&self, uid: u32) -> anyhow::Result<()> {
        self.meta_set(&self.mkey("last_uid"), &uid.to_string())
    }

    /// Marks a message as "being handled right now" and returns how many times in a row this same message has
    /// been started without finishing. A crash leaves the marker behind, so the next start sees a count above 1.
    pub fn begin_attempt(&self, uid: u32) -> anyhow::Result<u32> {
        let tries = match self
            .meta_get(&self.mkey("attempt"))
            .as_deref()
            .and_then(|v| v.split_once(':'))
        {
            Some((u, n)) if u.parse() == Ok(uid) => n.parse::<u32>().unwrap_or(0) + 1,
            _ => 1,
        };
        self.meta_set(&self.mkey("attempt"), &format!("{uid}:{tries}"))?;
        Ok(tries)
    }

    /// Clears the marker once a message has been handled (stored, rejected or skipped).
    pub fn end_attempt(&self) -> anyhow::Result<()> {
        self.meta_set(&self.mkey("attempt"), "")
    }

    /// How many times in a row the server has answered for `uid` without sending its content. Kept apart from the
    /// crash counter, which is a single slot that any other message resets.
    pub fn no_content_attempt(&self, uid: u32) -> anyhow::Result<u32> {
        let tries = match self
            .meta_get(&self.mkey("no_content"))
            .as_deref()
            .and_then(|v| v.split_once(':'))
        {
            Some((u, n)) if u.parse() == Ok(uid) => n.parse::<u32>().unwrap_or(0) + 1,
            _ => 1,
        };
        self.meta_set(&self.mkey("no_content"), &format!("{uid}:{tries}"))?;
        Ok(tries)
    }

    /// Forgets the no-content counter once a fetch came back whole.
    pub fn clear_no_content(&self) -> anyhow::Result<()> {
        self.meta_set(&self.mkey("no_content"), "")
    }

    /// The message a previous run was in the middle of, if it never finished.
    pub fn in_flight(&self) -> Option<u32> {
        self.meta_get(&self.mkey("attempt"))?
            .split_once(':')?
            .0
            .parse()
            .ok()
    }

    /// Data minimisation: drop finished tasks (and their drafts) and log lines past their retention.
    /// Open tasks are never touched.
    pub fn prune(&self, done_days: i64, log_days: i64) -> anyhow::Result<(usize, usize)> {
        let tasks = self.db.execute(
            "DELETE FROM tasks WHERE status='done' AND COALESCE(NULLIF(done_at,0), received) < CAST(strftime('%s','now') AS INTEGER) - ?1 * 86400",
            [done_days],
        )?;
        self.db.execute(
            "DELETE FROM drafts WHERE uid NOT IN (SELECT uid FROM tasks)",
            [],
        )?;
        let logs = self.db.execute(
            "DELETE FROM log WHERE at < CAST(strftime('%s','now') AS INTEGER) - ?1 * 86400",
            [log_days],
        )?;
        self.db.execute(
            "DELETE FROM overrides WHERE at < CAST(strftime('%s','now') AS INTEGER) - 365 * 86400",
            [],
        )?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0.0, |d| d.as_secs_f64());
        self.prune_followups(now)?;
        Ok((tasks, logs))
    }

    /// IMAP UIDs are only meaningful within one UIDVALIDITY. If the server reports a new value the
    /// stored cursor and UIDs are stale, so start over. Returns true when a reset happened.
    pub fn reconcile_uid_validity(&self, v: u32) -> anyhow::Result<bool> {
        let key = self.mkey("uid_validity");
        let reset = matches!(self.meta_get(&key), Some(c) if c != v.to_string());
        if reset {
            self.reset_mailbox_data()?;
            // what was learned about senders still holds, but the links to the old UIDs mean nothing now. An unlinked
            // correction is parked on the first number of this mailbox's block (no message has it: IMAP numbers start at 1),
            // so it still belongs to this mailbox and goes with it, and with no other
            let (lo, hi) = Self::block(self.mb());
            self.db.execute(
                "UPDATE overrides SET uid=?1 WHERE uid>=?1 AND uid<?2",
                params![lo, hi],
            )?;
        }
        self.meta_set(&key, &v.to_string())?;
        Ok(reset)
    }

    /// Sign-out: forget everything this app stored about the mailbox and the user's corrections.
    pub fn wipe_all(&self) -> anyhow::Result<()> {
        self.db.execute_batch(
            "DELETE FROM tasks; DELETE FROM drafts; DELETE FROM outbox; DELETE FROM meta; DELETE FROM log; DELETE FROM overrides; DELETE FROM sender_prefs; DELETE FROM rules; DELETE FROM followups; INSERT INTO tasks_fts(tasks_fts) VALUES('delete-all');",
        )?;
        // Best effort: fold the journal into the file and rewrite it, so no copy of the deleted rows is left
        // in the write-ahead log or in unused pages. A busy database skips this; secure_delete already zeroed rows.
        let _ = self
            .db
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()));
        let _ = self.db.execute_batch("VACUUM");
        let _ = self
            .db
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()));
        Ok(())
    }

    /// For tests: pretend a task was finished at `at` (unix seconds).
    #[doc(hidden)]
    pub fn backdate_done(&self, uid: u32, at: i64) -> anyhow::Result<()> {
        self.db
            .execute("UPDATE tasks SET done_at=?1 WHERE uid=?2", params![at, uid])?;
        Ok(())
    }

    /// The first mailbox's account changed (or any scoped store's): everything tied to that mailbox goes, including what was
    /// learned from its messages and the follow-ups about mail sent from it. Sender preferences, your rules, your own tasks
    /// and the other mailboxes stay.
    pub fn reset_for_new_account(&self) -> anyhow::Result<()> {
        self.clear_mailbox(self.mb())
    }

    /// Removes a mailbox: its tasks, their drafts, its queued changes, the corrections made on its messages, its follow-ups,
    /// its counters. Nothing in the mailbox itself is touched, and nothing of any other mailbox or of yours.
    pub fn remove_mailbox(&self, n: u32) -> anyhow::Result<()> {
        self.clear_mailbox(n)?;
        self.log("a mailbox was removed from the app")?;
        Ok(())
    }

    /// `remove_mailbox` without the log line: for clearing a number before a new mailbox takes it.
    pub fn clear_mailbox(&self, n: u32) -> anyhow::Result<()> {
        self.delete_mail_rows(n)?;
        let (lo, hi) = Self::block(n);
        let tx = self.db.unchecked_transaction()?;
        tx.execute(
            "DELETE FROM overrides WHERE uid>=?1 AND uid<?2",
            params![lo, hi],
        )?;
        tx.execute(
            "DELETE FROM tasks WHERE kind='followup' AND uid IN (SELECT task_uid FROM followups WHERE acct=?1)",
            [n],
        )?;
        if n == 0 {
            // a follow-up whose record the previous version pruned (it deleted records at 60 days whatever their task was
            // doing): only the first mailbox could make follow-ups then, so it is the first mailbox's
            tx.execute(
                "DELETE FROM tasks WHERE kind='followup' AND uid NOT IN (SELECT task_uid FROM followups)",
                [],
            )?;
        }
        tx.execute("DELETE FROM followups WHERE acct=?1", [n])?;
        tx.commit()?;
        Ok(())
    }

    /// The mailbox's tasks, drafts, queued changes and counters; the rest of `meta` (the update check, the digest date, the
    /// draft budget) is not the mailbox's and stays.
    fn delete_mail_rows(&self, n: u32) -> anyhow::Result<()> {
        let (lo, hi) = Self::block(n);
        let tx = self.db.unchecked_transaction()?;
        for table in ["tasks", "drafts", "outbox"] {
            tx.execute(
                &format!("DELETE FROM {table} WHERE uid>=?1 AND uid<?2"),
                params![lo, hi],
            )?;
        }
        for base in PER_MAILBOX_META {
            let key = if n == 0 {
                base.to_string()
            } else {
                format!("{base}@{n}")
            };
            tx.execute("DELETE FROM meta WHERE key=?1", [key])?;
        }
        tx.commit()?;
        Ok(())
    }

    /// The sync found its mailbox renumbered: only that mailbox's mail and counters are forgotten.
    pub fn reset_mailbox_data(&self) -> anyhow::Result<()> {
        self.delete_mail_rows(self.mb())
    }
}
