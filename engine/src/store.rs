//! SQLite store: tasks, reply drafts, user corrections, activity log and the mailbox write-back outbox.
//! Each thread opens its own connection (WAL + busy timeout), so no shared locking is needed.
use crate::mail::Email;
use crate::signals::Signals;
use crate::triage::Triage;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::path::Path;

pub const MAX_DRAFT_CHARS: usize = 8000;
pub const MAX_NOTE_CHARS: usize = 2000;
/// Bumped whenever the database layout changes in a way an older app cannot read.
pub const SCHEMA_VERSION: i32 = 1;
/// Tasks you add yourself live above this number, so they never collide with a mail server's UIDs.
pub const LOCAL_UID_BASE: u32 = 2_000_000_000;

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
";

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
];

/// Same idea for the corrections table.
const ADDED_OVERRIDE_COLUMNS: &[(&str, &str)] =
    &[("addr", "TEXT DEFAULT ''"), ("uid", "INTEGER DEFAULT 0")];

const SELECT: &str = "SELECT t.uid,t.message_id,t.sender,t.addr,t.subject,t.received,t.priority,t.category,t.action,t.urgent,t.status,t.star,\
t.excerpt,t.why,t.signals,t.needs_reply,t.due,t.snooze_until,t.reply_to,d.body,d.source,d.saved,d.updated,p.level,t.auth_fail,t.attachments,t.attachment_count,t.unsubscribe,t.note,t.thread,t.closed_by,t.gone,t.learned \
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
    })
}

fn migrate(db: &Connection) -> anyhow::Result<()> {
    let have: Vec<String> = db
        .prepare("PRAGMA table_info(tasks)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(Result::ok)
        .collect();
    for (name, def) in ADDED_COLUMNS {
        if !have.iter().any(|h| h == name) {
            db.execute(&format!("ALTER TABLE tasks ADD COLUMN {name} {def}"), [])?;
        }
    }
    let have: Vec<String> = db
        .prepare("PRAGMA table_info(overrides)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(Result::ok)
        .collect();
    for (name, def) in ADDED_OVERRIDE_COLUMNS {
        if !have.iter().any(|h| h == name) {
            db.execute(
                &format!("ALTER TABLE overrides ADD COLUMN {name} {def}"),
                [],
            )?;
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
        let db = Connection::open(path)?;
        db.busy_timeout(std::time::Duration::from_secs(5))?;
        let _ = db.query_row("PRAGMA journal_mode=WAL", [], |_| Ok(()));
        // deleted rows are overwritten with zeros, so a wipe does not leave mail text in free pages
        db.pragma_update(None, "secure_delete", "ON")?;
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
        db.execute_batch(SCHEMA)?;
        migrate(&db)?;
        if version != SCHEMA_VERSION {
            db.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        }
        Ok(Self { db })
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
        db.execute_batch(SCHEMA)?;
        migrate(&db)?;
        Ok(Self { db })
    }

    pub fn seen(&self, uid: u32) -> bool {
        self.db
            .query_row("SELECT 1 FROM tasks WHERE uid=?1", [uid], |_| Ok(()))
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
        // What the user taught by correcting this sender before. Only the rules ranker is shifted (a model already
        // sees the corrections in its prompt), never a sender with an explicit preference, never mail that failed its
        // sender check, and it never makes anything urgent.
        let mut learned = 0;
        if pref.is_none() && !e.auth_fail && t.why.starts_with("Rule-based") {
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
        self.db.execute(
            "INSERT OR REPLACE INTO tasks(uid,message_id,sender,addr,subject,received,priority,category,action,urgent,excerpt,why,signals,body,needs_reply,due,reply_to,refs,auth_fail,attachments,attachment_count,unsubscribe,thread,learned)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24)",
            params![
                e.uid, e.message_id, e.sender_name, e.sender_addr, e.subject, e.received, t.priority, t.category,
                t.action, t.urgent as i32, e.excerpt, t.why, serde_json::to_string(s)?, e.body,
                t.needs_reply as i32, t.due, e.reply_to, e.references, e.auth_fail as i32,
                serde_json::to_string(&e.attachments)?, e.attachment_count as i64, e.unsubscribe, e.thread,
                learned
            ],
        )?;
        // news in a snoozed conversation wakes it: the snooze was for the old state of the thread
        if !e.thread.is_empty() && !e.auth_fail {
            self.db.execute(
                "UPDATE tasks SET snooze_until=0 WHERE thread=?1 AND status='open' AND snooze_until>0 AND uid<>?2",
                params![e.thread, e.uid],
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
        Ok(t)
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
        let Some(t) = self.get(uid) else {
            return Ok(false);
        };
        // Reopening a task the app closed by itself (you had replied): the mailbox was never changed, so there is
        // nothing to unarchive, and the task must not be closed again by the next pass.
        let auto = !done && !t.closed_by.is_empty();
        self.db.execute(
            "UPDATE tasks SET status=?1, snooze_until=0, closed_by='', keep_open=CASE WHEN ?3=1 THEN 1 ELSE keep_open END, done_at=CASE WHEN ?1='done' THEN CAST(strftime('%s','now') AS INTEGER) ELSE 0 END WHERE uid=?2",
            params![if done { "done" } else { "open" }, uid, auto as i32],
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
    /// Open mail tasks (not the ones you added) with their Message-ID, for the reply and left-the-inbox checks:
    /// (uid, message_id, keep_open, gone). Newest first.
    pub fn open_mail_tasks(&self, limit: i64) -> Vec<(u32, String, bool, bool)> {
        let Ok(mut q) = self.db.prepare(
            "SELECT uid, message_id, keep_open, gone FROM tasks WHERE status='open' AND uid<?1 AND message_id<>'' ORDER BY received DESC LIMIT ?2",
        ) else {
            return vec![];
        };
        q.query_map(params![LOCAL_UID_BASE, limit], |r| {
            Ok((
                r.get(0)?,
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
        self.db.execute(
            "UPDATE tasks SET gone=?1 WHERE uid=?2 AND gone<>?1",
            params![gone as i32, uid],
        )?;
        Ok(())
    }

    /// The inbox UIDVALIDITY the stored tasks were numbered under, if known.
    pub fn stored_uid_validity(&self) -> Option<u32> {
        self.meta_get("uid_validity")?.parse().ok()
    }

    // ---- editable fields ----
    /// Sets or clears (empty string) the due date of any task. Only a real `YYYY-MM-DD` is accepted.
    pub fn set_due(&self, uid: u32, due: &str) -> anyhow::Result<bool> {
        if self.get(uid).is_none() || !(due.is_empty() || crate::dates::valid_iso(due)) {
            return Ok(false);
        }
        self.db
            .execute("UPDATE tasks SET due=?1 WHERE uid=?2", params![due, uid])?;
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
        let top: Option<u32> = self
            .db
            .query_row(
                "SELECT MAX(uid) FROM tasks WHERE uid >= ?1",
                [LOCAL_UID_BASE],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        let uid = top.map_or(LOCAL_UID_BASE, |t| t + 1);
        self.db.execute(
            "INSERT INTO tasks(uid,message_id,sender,addr,subject,received,priority,category,action,urgent,excerpt,why,signals,body,needs_reply,due,reply_to,refs,auth_fail)
             VALUES(?1,'','You','',?2,strftime('%s','now'),?3,'personal',?2,0,'','Added by you','{}','',0,?4,'','',0)",
            params![uid, title, priority, due],
        )?;
        self.log(&format!("added task “{title}”"))?;
        Ok(Some(uid))
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
        let Ok(mut st) = self
            .db
            .prepare("SELECT id,uid,op,arg,tries FROM outbox ORDER BY id LIMIT ?1")
        else {
            return vec![];
        };
        st.query_map([limit], |r| {
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
        self.meta_get("last_uid")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0)
    }

    pub fn set_last_uid(&self, uid: u32) -> anyhow::Result<()> {
        self.meta_set("last_uid", &uid.to_string())
    }

    /// Marks a message as "being handled right now" and returns how many times in a row this same message has
    /// been started without finishing. A crash leaves the marker behind, so the next start sees a count above 1.
    pub fn begin_attempt(&self, uid: u32) -> anyhow::Result<u32> {
        let tries = match self
            .meta_get("attempt")
            .as_deref()
            .and_then(|v| v.split_once(':'))
        {
            Some((u, n)) if u.parse() == Ok(uid) => n.parse::<u32>().unwrap_or(0) + 1,
            _ => 1,
        };
        self.meta_set("attempt", &format!("{uid}:{tries}"))?;
        Ok(tries)
    }

    /// Clears the marker once a message has been handled (stored, rejected or skipped).
    pub fn end_attempt(&self) -> anyhow::Result<()> {
        self.meta_set("attempt", "")
    }

    /// The message a previous run was in the middle of, if it never finished.
    pub fn in_flight(&self) -> Option<u32> {
        self.meta_get("attempt")?.split_once(':')?.0.parse().ok()
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
        Ok((tasks, logs))
    }

    /// IMAP UIDs are only meaningful within one UIDVALIDITY. If the server reports a new value the
    /// stored cursor and UIDs are stale, so start over. Returns true when a reset happened.
    pub fn reconcile_uid_validity(&self, v: u32) -> anyhow::Result<bool> {
        let reset = matches!(self.meta_get("uid_validity"), Some(c) if c != v.to_string());
        if reset {
            self.reset_mailbox_data()?;
            // what was learned about senders still holds, but the links to the old UIDs mean nothing now
            self.db.execute("UPDATE overrides SET uid=0", [])?;
        }
        self.meta_set("uid_validity", &v.to_string())?;
        Ok(reset)
    }

    /// Sign-out: forget everything this app stored about the mailbox and the user's corrections.
    pub fn wipe_all(&self) -> anyhow::Result<()> {
        self.db.execute_batch(
            "DELETE FROM tasks; DELETE FROM drafts; DELETE FROM outbox; DELETE FROM meta; DELETE FROM log; DELETE FROM overrides; DELETE FROM sender_prefs;",
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

    /// Forget everything tied to a mailbox (used when the account changes). Sender preferences stay.
    /// The account changed: everything tied to the old mailbox goes, including what was learned about its senders.
    pub fn reset_for_new_account(&self) -> anyhow::Result<()> {
        self.reset_mailbox_data()?;
        self.db.execute("DELETE FROM overrides", [])?;
        Ok(())
    }

    pub fn reset_mailbox_data(&self) -> anyhow::Result<()> {
        self.db.execute_batch(
            "DELETE FROM tasks WHERE uid < 2000000000; DELETE FROM drafts WHERE uid < 2000000000; DELETE FROM outbox; DELETE FROM meta;",
        )?;
        Ok(())
    }
}
