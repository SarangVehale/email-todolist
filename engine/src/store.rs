//! SQLite store: tasks, reply drafts, user corrections, activity log and the mailbox write-back outbox.
//! Each thread opens its own connection (WAL + busy timeout), so no shared locking is needed.
use crate::mail::Email;
use crate::signals::Signals;
use crate::triage::Triage;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::path::Path;

pub const MAX_DRAFT_CHARS: usize = 8000;

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
];

const SELECT: &str = "SELECT t.uid,t.message_id,t.sender,t.addr,t.subject,t.received,t.priority,t.category,t.action,t.urgent,t.status,t.star,\
t.excerpt,t.why,t.signals,t.needs_reply,t.due,t.snooze_until,t.reply_to,d.body,d.source,d.saved,d.updated,p.level,t.auth_fail \
FROM tasks t LEFT JOIN drafts d ON d.uid=t.uid LEFT JOIN sender_prefs p ON p.addr=t.addr";

fn domain(a: &str) -> String {
    a.rsplit('@').next().unwrap_or("").to_lowercase()
}

fn row(r: &rusqlite::Row) -> rusqlite::Result<Task> {
    let addr: String = r.get(3)?;
    let reply_to: String = r.get(18)?;
    let auth_fail = r.get::<_, i32>(24)? != 0;
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
    Ok(())
}

impl Store {
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        let db = Connection::open(path)?;
        db.busy_timeout(std::time::Duration::from_secs(5))?;
        let _ = db.query_row("PRAGMA journal_mode=WAL", [], |_| Ok(()));
        db.execute_batch(SCHEMA)?;
        migrate(&db)?;
        Ok(Self { db })
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
        match self.pref(&e.sender_addr).as_deref() {
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
        self.db.execute(
            "INSERT OR REPLACE INTO tasks(uid,message_id,sender,addr,subject,received,priority,category,action,urgent,excerpt,why,signals,body,needs_reply,due,reply_to,refs,auth_fail)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19)",
            params![
                e.uid, e.message_id, e.sender_name, e.sender_addr, e.subject, e.received, t.priority, t.category,
                t.action, t.urgent as i32, e.excerpt, t.why, serde_json::to_string(s)?, e.body,
                t.needs_reply as i32, t.due, e.reply_to, e.references, e.auth_fail as i32
            ],
        )?;
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
        if self.get(uid).is_none() {
            return Ok(false);
        }
        self.db.execute(
            "UPDATE tasks SET status=?1, snooze_until=0, done_at=CASE WHEN ?1='done' THEN CAST(strftime('%s','now') AS INTEGER) ELSE 0 END WHERE uid=?2",
            params![if done { "done" } else { "open" }, uid],
        )?;
        self.queue(uid, if done { "archive" } else { "unarchive" }, "")?;
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
            self.db.execute(
                "INSERT INTO overrides(sender,subject,old_priority,new_priority,at) VALUES(?1,?2,?3,?4,strftime('%s','now'))",
                params![t.sender, t.subject, t.priority, new],
            )?;
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

    // ---- outbox ----
    pub fn queue(&self, uid: u32, op: &str, arg: &str) -> anyhow::Result<()> {
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
        }
        self.meta_set("uid_validity", &v.to_string())?;
        Ok(reset)
    }

    /// Sign-out: forget everything this app stored about the mailbox and the user's corrections.
    pub fn wipe_all(&self) -> anyhow::Result<()> {
        self.db.execute_batch(
            "DELETE FROM tasks; DELETE FROM drafts; DELETE FROM outbox; DELETE FROM meta; DELETE FROM log; DELETE FROM overrides; DELETE FROM sender_prefs;",
        )?;
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
    pub fn reset_mailbox_data(&self) -> anyhow::Result<()> {
        self.db.execute_batch(
            "DELETE FROM tasks; DELETE FROM drafts; DELETE FROM outbox; DELETE FROM meta;",
        )?;
        Ok(())
    }
}
