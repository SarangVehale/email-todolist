//! The database side of backup and restore (design: docs/designs/phase7.md). A child module of `store`, so it can use the
//! connection. Reading what goes into a backup; applying a parsed backup in ONE transaction (a preview is the same code with
//! the transaction rolled back); and the table of what you did with mail that has not been read yet.
use super::{mailbox_of, Store, LOCAL_UID_BASE, MAILBOX_SPAN};
use crate::backup::{
    now_secs, Backup, Choice, Correction, DraftState, MailState, OwnTask, Report, MAX_CHOICES,
    MAX_CORRECTIONS, MAX_MAIL_STATE, MAX_OWN_TASKS,
};
use crate::reminders;
use anyhow::Result;
use chrono::NaiveDateTime;
use rusqlite::{params, OptionalExtension};
use std::collections::HashSet;

/// At most this many entries wait for mail that has not been read yet.
pub const MAX_PARKED: usize = MAX_MAIL_STATE;

/// Mail text that goes in a backup (a correction's sender and subject): one line, no control characters, cut to `max`.
fn one_line(s: &str, max: usize) -> String {
    s.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(max)
        .collect()
}

/// A rule in words, so the preview can show what a file would add.
fn describe_rule(r: &crate::rules::Rule) -> String {
    let text = r.text.trim();
    let when = match r.field.as_str() {
        "sender" => format!("the address is {text}"),
        "domain" => format!("the domain is {text}"),
        _ => format!("the subject contains \"{text}\""),
    };
    let mut does = vec![];
    if r.file {
        does.push("file the mail away (mark it done)".to_string());
    }
    if let Some(p) = r.priority {
        does.push(format!("priority {p}"));
    }
    if let Some(c) = &r.category {
        does.push(format!("category {c}"));
    }
    format!("If {when}: {}", does.join(", "))
}

fn fresh_rule_id(taken: &HashSet<String>) -> String {
    use rand::RngExt;
    const ALPHABET: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";
    let mut rng = rand::rng();
    loop {
        let id: String = (0..8)
            .map(|_| ALPHABET[rng.random_range(0..ALPHABET.len())] as char)
            .collect();
        if !taken.contains(&id) {
            return id;
        }
    }
}

impl Store {
    // ---- reading what goes into a backup ----
    /// The sender choices ("important" or "mute") for a backup, by address, at most `MAX_CHOICES`.
    pub fn backup_choices(&self) -> Vec<Choice> {
        let Ok(mut q) = self
            .db
            .prepare("SELECT addr, level FROM sender_prefs ORDER BY addr LIMIT ?1")
        else {
            return vec![];
        };
        q.query_map([MAX_CHOICES as i64], |r| {
            Ok(Choice {
                addr: r.get(0)?,
                level: r.get(1)?,
            })
        })
        .map(|rows| rows.filter_map(Result::ok).collect())
        .unwrap_or_default()
    }

    /// The corrections you made, oldest first (the newest ones if there are more than a file holds).
    pub fn backup_corrections(&self) -> Vec<Correction> {
        let Ok(mut q) = self.db.prepare(
            "SELECT sender, subject, old_priority, new_priority, COALESCE(addr,''), at FROM overrides ORDER BY id DESC LIMIT ?1",
        ) else {
            return vec![];
        };
        let mut out: Vec<Correction> = q
            .query_map([MAX_CORRECTIONS as i64], |r| {
                Ok(Correction {
                    sender: one_line(&r.get::<_, Option<String>>(0)?.unwrap_or_default(), 300),
                    subject: one_line(&r.get::<_, Option<String>>(1)?.unwrap_or_default(), 600),
                    old: r.get::<_, i64>(2)?.clamp(1, 5) as u8,
                    new: r.get::<_, i64>(3)?.clamp(1, 5) as u8,
                    addr: one_line(&r.get::<_, String>(4)?, 320),
                    at: r.get::<_, Option<f64>>(5)?.unwrap_or(0.0).max(0.0),
                })
            })
            .map(|rows| rows.filter_map(Result::ok).collect())
            .unwrap_or_default();
        out.reverse();
        out
    }

    /// The tasks you added yourself (not the ones the app made for you).
    pub fn backup_own_tasks(&self) -> Vec<OwnTask> {
        let Ok(mut q) = self.db.prepare(
            "SELECT subject, priority, due, remind, repeat, repeat_base, status, star, note FROM tasks WHERE uid>=?1 AND kind='' ORDER BY CASE status WHEN 'open' THEN 0 ELSE 1 END, uid DESC LIMIT ?2",
        ) else {
            return vec![];
        };
        q.query_map(params![LOCAL_UID_BASE, MAX_OWN_TASKS as i64], |r| {
            Ok(OwnTask {
                title: r.get(0)?,
                priority: r.get::<_, i64>(1)?.clamp(1, 5) as u8,
                due: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                remind: r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                repeat: r.get::<_, Option<String>>(4)?.unwrap_or_default(),
                repeat_base: r.get::<_, Option<String>>(5)?.unwrap_or_default(),
                status: if r.get::<_, String>(6)? == "done" {
                    "done".into()
                } else {
                    "open".into()
                },
                star: r.get::<_, i64>(7)? != 0,
                note: r.get::<_, Option<String>>(8)?.unwrap_or_default(),
            })
        })
        .map(|rows| rows.filter_map(Result::ok).collect())
        .unwrap_or_default()
    }

    /// What you did with mail: finished, starred, snoozed, a note, a reply draft. Keyed by Message-ID and the position of its
    /// mailbox in `boxes`; nothing of the message itself.
    pub fn backup_mail_state(&self, boxes: &[u32]) -> Vec<MailState> {
        let Ok(mut q) = self.db.prepare(
            "SELECT t.uid, t.message_id, t.status, t.star, t.snooze_until, COALESCE(t.note,''), d.body, d.source
             FROM tasks t LEFT JOIN drafts d ON d.uid=t.uid AND d.source='user'
             WHERE t.uid<?1 AND t.message_id<>''
               AND (t.status='done' OR t.star<>0 OR t.snooze_until>?2 OR COALESCE(t.note,'')<>'' OR d.body IS NOT NULL)
             ORDER BY t.received DESC LIMIT ?3",
        ) else {
            return vec![];
        };
        let now = now_secs();
        q.query_map(params![LOCAL_UID_BASE, now, MAX_MAIL_STATE as i64], |r| {
            let uid: u32 = r.get(0)?;
            let draft = r.get::<_, Option<String>>(6)?.map(|body| DraftState {
                body,
                source: r
                    .get::<_, Option<String>>(7)
                    .ok()
                    .flatten()
                    .unwrap_or_default(),
            });
            Ok((
                uid,
                MailState {
                    m: 0,
                    message_id: r.get(1)?,
                    done: r.get::<_, String>(2)? == "done",
                    star: r.get::<_, i64>(3)? != 0,
                    snooze_until: r.get::<_, f64>(4).unwrap_or(0.0).max(0.0),
                    note: r.get(5)?,
                    draft,
                },
            ))
        })
        .map(|rows| {
            rows.filter_map(Result::ok)
                .filter_map(|(uid, mut s)| {
                    s.m = boxes.iter().position(|b| *b == mailbox_of(uid))?;
                    if s.snooze_until <= now {
                        s.snooze_until = 0.0;
                    }
                    Some(s)
                })
                .collect()
        })
        .unwrap_or_default()
    }

    /// How many there are in all of: sender choices, corrections, tasks of yours, and mail with something of yours on it.
    pub fn backup_totals(&self) -> (usize, usize, usize, usize) {
        let count = |sql: &str, p: &[&dyn rusqlite::ToSql]| -> usize {
            self.db
                .query_row(sql, p, |r| r.get::<_, i64>(0))
                .unwrap_or(0)
                .max(0) as usize
        };
        (
            count("SELECT COUNT(*) FROM sender_prefs", &[]),
            count("SELECT COUNT(*) FROM overrides", &[]),
            count(
                "SELECT COUNT(*) FROM tasks WHERE uid>=?1 AND kind=''",
                &[&LOCAL_UID_BASE],
            ),
            count(
                "SELECT COUNT(*) FROM tasks t LEFT JOIN drafts d ON d.uid=t.uid AND d.source='user'
                 WHERE t.uid<?1 AND t.message_id<>''
                   AND (t.status='done' OR t.star<>0 OR t.snooze_until>?2 OR COALESCE(t.note,'')<>'' OR d.body IS NOT NULL)",
                &[&LOCAL_UID_BASE, &now_secs()],
            ),
        )
    }

    // ---- state for mail that has not been read yet ----
    /// How many restored entries are waiting for their message to be read by the sync.
    pub fn parked_count(&self) -> usize {
        self.db
            .query_row("SELECT COUNT(*) FROM restored_state", [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap_or(0) as usize
    }

    /// For tests: pretend every parked entry was parked `days` ago.
    #[doc(hidden)]
    pub fn age_parked(&self, days: f64) -> Result<()> {
        self.db
            .execute("UPDATE restored_state SET at = at - ?1 * 86400", [days])?;
        Ok(())
    }

    pub(super) fn prune_parked(&self) -> Result<usize> {
        Ok(self.db.execute(
            "DELETE FROM restored_state WHERE at < ?1",
            [now_secs() - 90.0 * 86400.0],
        )?)
    }

    /// A message that was just read: what you did with it before is applied, once.
    pub(super) fn apply_parked(&self, uid: u32, message_id: &str) -> Result<()> {
        if message_id.is_empty() || uid >= LOCAL_UID_BASE {
            return Ok(());
        }
        let mb = mailbox_of(uid);
        let row: Option<String> = self
            .db
            .query_row(
                "SELECT state FROM restored_state WHERE mailbox=?1 AND message_id=?2",
                params![mb, message_id],
                |r| r.get(0),
            )
            .optional()?;
        let Some(json) = row else { return Ok(()) };
        self.db.execute(
            "DELETE FROM restored_state WHERE mailbox=?1 AND message_id=?2",
            params![mb, message_id],
        )?;
        if let Ok(state) = serde_json::from_str::<MailState>(&json) {
            self.apply_state(uid, &state, now_secs())?;
        }
        Ok(())
    }

    /// Lays what you had done over a task, never taking anything away: finished stays finished, a star stays, the later snooze
    /// wins, your existing note and draft win. Nothing is queued for the mailbox. Returns whether anything changed.
    fn apply_state(&self, uid: u32, s: &MailState, now: f64) -> Result<bool> {
        let Some((status, star, snooze, note)) = self
            .db
            .query_row(
                "SELECT status, star, snooze_until, COALESCE(note,'') FROM tasks WHERE uid=?1",
                [uid],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, i64>(1)? != 0,
                        r.get::<_, f64>(2).unwrap_or(0.0),
                        r.get::<_, String>(3)?,
                    ))
                },
            )
            .optional()?
        else {
            return Ok(false);
        };
        let mut changed = false;
        let mut open = status == "open";
        if s.done && open {
            self.db.execute(
                "UPDATE tasks SET status='done', snooze_until=0, closed_by='', done_at=?2 WHERE uid=?1",
                params![uid, now as i64],
            )?;
            open = false;
            changed = true;
        }
        if s.star && !star {
            self.db
                .execute("UPDATE tasks SET star=1 WHERE uid=?1", [uid])?;
            changed = true;
        }
        if open && s.snooze_until > now && s.snooze_until > snooze + 1.0 {
            self.db.execute(
                "UPDATE tasks SET snooze_until=?2 WHERE uid=?1",
                params![uid, s.snooze_until],
            )?;
            changed = true;
        }
        if !s.note.is_empty() && note.is_empty() {
            self.db.execute(
                "UPDATE tasks SET note=?2 WHERE uid=?1",
                params![uid, s.note],
            )?;
            changed = true;
        }
        if let Some(d) = &s.draft {
            let n = self.db.execute(
                "INSERT OR IGNORE INTO drafts(uid,body,source,saved,updated) VALUES(?1,?2,?3,0,?4)",
                params![uid, d.body, d.source, now],
            )?;
            changed |= n > 0;
        }
        Ok(changed)
    }

    // ---- restoring ----
    /// Applies a parsed backup in one transaction. `boxes[i]` is the local mailbox number of the file's mailbox `i`, if it is
    /// set up here. With `dry` the transaction is rolled back: the report says what a real run does, and nothing is written.
    pub fn restore_apply(
        &self,
        b: &Backup,
        boxes: &[Option<u32>],
        now: f64,
        now_local: NaiveDateTime,
        dry: bool,
    ) -> Result<Report> {
        // the write lock is taken at once (and waited for): a deferred transaction that reads first fails outright if a sync
        // commits before its first write
        let tx = rusqlite::Transaction::new_unchecked(
            &self.db,
            rusqlite::TransactionBehavior::Immediate,
        )?;
        let mut r = Report::default();
        self.restore_rules(b, &mut r)?;
        self.restore_choices(b, &mut r)?;
        self.restore_corrections(b, &mut r)?;
        self.restore_own_tasks(b, now, now_local, &mut r)?;
        self.restore_mail_state(b, boxes, now, &mut r)?;
        if dry {
            tx.rollback()?;
        } else {
            tx.commit()?;
        }
        Ok(r)
    }

    fn restore_rules(&self, b: &Backup, r: &mut Report) -> Result<()> {
        let mut have = self.rules();
        let mut ids: HashSet<String> = have.iter().map(|x| x.id.clone()).collect();
        for rule in &b.rules {
            let same = have.iter().any(|x| {
                x.field == rule.field
                    && x.text.trim().eq_ignore_ascii_case(rule.text.trim())
                    && x.priority == rule.priority
                    && x.category == rule.category
                    && x.file == rule.file
            });
            if same || have.len() >= crate::rules::MAX_RULES {
                r.rules_skipped += 1;
                continue;
            }
            let id = fresh_rule_id(&ids);
            self.db.execute(
                "INSERT INTO rules(id,ord,field,text,priority,category,file) VALUES(?1,?2,?3,?4,?5,?6,?7)",
                params![id, have.len() as i64, rule.field, rule.text.trim(), rule.priority, rule.category, rule.file as i32],
            )?;
            ids.insert(id.clone());
            if r.rules_new.len() < 20 {
                r.rules_new.push(describe_rule(rule));
            }
            let mut kept = rule.clone();
            kept.id = id;
            have.push(kept);
            r.rules_added += 1;
        }
        Ok(())
    }

    fn restore_choices(&self, b: &Backup, r: &mut Report) -> Result<()> {
        for c in &b.sender_choices {
            let exists = self
                .db
                .query_row(
                    "SELECT 1 FROM sender_prefs WHERE addr=?1",
                    [&c.addr],
                    |_| Ok(()),
                )
                .optional()?
                .is_some();
            if exists {
                r.sender_choices_skipped += 1;
            } else {
                // what choosing the sender does (see `set_pref`), without a line in the activity log for each one
                self.db.execute(
                    "INSERT OR REPLACE INTO sender_prefs VALUES(?1,?2)",
                    params![c.addr, c.level],
                )?;
                let sql = if c.level == "important" {
                    "UPDATE tasks SET priority=MAX(priority,4) WHERE addr=?1 AND status='open'"
                } else {
                    "UPDATE tasks SET priority=1, urgent=0 WHERE addr=?1 AND status='open'"
                };
                self.db.execute(sql, [&c.addr])?;
                r.sender_choices_added += 1;
            }
        }
        Ok(())
    }

    fn restore_corrections(&self, b: &Backup, r: &mut Report) -> Result<()> {
        if b.corrections.is_empty() {
            return Ok(());
        }
        // the same correction is one with the same address, sender, subject, priorities and time. What is stored is compared
        // in the form a backup writes it (one line, cut to length), so a backup restored onto the computer it came from
        // finds its own corrections
        type Key = (String, String, String, u8, u8, i64);
        let mut have: HashSet<Key> = HashSet::new();
        {
            let mut q = self.db.prepare(
                "SELECT COALESCE(addr,''), COALESCE(sender,''), COALESCE(subject,''), old_priority, new_priority, COALESCE(at,0) FROM overrides",
            )?;
            let rows = q.query_map([], |x| {
                Ok((
                    one_line(&x.get::<_, String>(0)?, 320),
                    one_line(&x.get::<_, String>(1)?, 300),
                    one_line(&x.get::<_, String>(2)?, 600),
                    x.get::<_, i64>(3)?.clamp(1, 5) as u8,
                    x.get::<_, i64>(4)?.clamp(1, 5) as u8,
                    (x.get::<_, f64>(5)?.max(0.0) * 1000.0).round() as i64,
                ))
            })?;
            for k in rows.flatten() {
                have.insert(k);
            }
        }
        for c in &b.corrections {
            let key: Key = (
                c.addr.clone(),
                c.sender.clone(),
                c.subject.clone(),
                c.old,
                c.new,
                (c.at * 1000.0).round() as i64, // to the millisecond: a time must not differ by a rounding
            );
            if !have.insert(key) {
                r.corrections_skipped += 1;
                continue;
            }
            // what was learned about senders is no one mailbox's: it is filed under a number no mailbox has, so removing
            // a mailbox does not take it
            self.db.execute(
                "INSERT INTO overrides(sender,subject,old_priority,new_priority,at,addr,uid) VALUES(?1,?2,?3,?4,?5,?6,?7)",
                params![c.sender, c.subject, c.old, c.new, c.at, c.addr, u32::MAX],
            )?;
            r.corrections_added += 1;
        }
        Ok(())
    }

    fn restore_own_tasks(
        &self,
        b: &Backup,
        now: f64,
        now_local: NaiveDateTime,
        r: &mut Report,
    ) -> Result<()> {
        use chrono::Timelike;
        let this_minute = now_local
            .with_second(0)
            .and_then(|n| n.with_nanosecond(0))
            .unwrap_or(now_local);
        for t in &b.own_tasks {
            let done = t.status == "done";
            // a finished task keeps no reminder and no repeat, whatever it had: that is how it is stored here
            let (eff_remind, eff_repeat) = if done {
                ("", "")
            } else {
                (t.remind.as_str(), t.repeat.as_str())
            };
            let file_when = reminders::parse_local(eff_remind);
            let past = file_when.is_some_and(|w| w < this_minute);
            // where a repeat counts from: its anchor, else its reminder, else its due date
            let anchor = if eff_repeat.is_empty() {
                None
            } else {
                reminders::parse_local(&t.repeat_base)
                    .or(file_when)
                    .or_else(|| reminders::parse_local(&format!("{} 00:00", t.due)))
            };
            // the same task: a repeating one has the same title, repeat and anchor; any other the same title and due date,
            // unless it has another reminder still to come
            let duplicate = match anchor {
                Some(a) => self
                    .db
                    .query_row(
                        "SELECT 1 FROM tasks WHERE uid>=?1 AND kind='' AND subject=?2 AND repeat=?3 AND repeat_base=?4",
                        params![LOCAL_UID_BASE, t.title, eff_repeat, reminders::fmt_local(a)],
                        |_| Ok(()),
                    )
                    .optional()?
                    .is_some(),
                None => {
                    let mut q = self.db.prepare(
                        "SELECT remind FROM tasks WHERE uid>=?1 AND kind='' AND subject=?2 AND due=?3 AND repeat=''",
                    )?;
                    let reminds: Vec<String> = q
                        .query_map(params![LOCAL_UID_BASE, t.title, t.due], |x| {
                            x.get::<_, Option<String>>(0).map(Option::unwrap_or_default)
                        })?
                        .filter_map(Result::ok)
                        .collect();
                    reminds
                        .iter()
                        .any(|have| eff_remind.is_empty() || past || have == eff_remind)
                }
            };
            if duplicate {
                r.own_tasks_skipped += 1;
                continue;
            }
            // the reminder: kept if it is still to come; a repeating one moves to its next time, and its due date with it;
            // one that has passed and cannot repeat is dropped
            let (mut remind, mut due) = (eff_remind.to_string(), t.due.clone());
            let mut base = String::new();
            if let Some(a) = anchor {
                base = reminders::fmt_local(a);
                if past {
                    if let (Some(was), Some(next)) = (
                        file_when,
                        reminders::next_occurrence(a, eff_repeat, now_local),
                    ) {
                        remind = reminders::fmt_local(next);
                        if let Some(d) = crate::dates::parse_iso(&t.due) {
                            due = (next.date() + (d - was.date()))
                                .format("%Y-%m-%d")
                                .to_string();
                        }
                    }
                }
            } else if past {
                remind.clear();
                r.reminders_dropped += 1;
            }
            self.db.execute(
                "INSERT INTO tasks(uid,message_id,sender,addr,subject,received,priority,category,action,urgent,excerpt,why,signals,body,needs_reply,due,reply_to,refs,auth_fail)
                 VALUES(COALESCE((SELECT MAX(uid)+1 FROM tasks WHERE uid>=?4), ?4),'','You','',?1,?5,?2,'personal',?1,0,'','Added by you','{}','',0,?3,'','',0)",
                params![t.title, t.priority, due, LOCAL_UID_BASE, now],
            )?;
            let uid = self.db.last_insert_rowid() as u32;
            self.db.execute(
                "UPDATE tasks SET star=?2, note=?3, status=?4, done_at=?5, remind=?6, repeat=?7, repeat_base=?8, reminded='' WHERE uid=?1",
                params![uid, t.star as i32, t.note, t.status, if done { now as i64 } else { 0 }, remind, eff_repeat, base],
            )?;
            r.own_tasks_added += 1;
        }
        Ok(())
    }

    fn restore_mail_state(
        &self,
        b: &Backup,
        boxes: &[Option<u32>],
        now: f64,
        r: &mut Report,
    ) -> Result<()> {
        let mut parked = self.parked_count();
        for s in &b.mail_state {
            let Some(Some(mb)) = boxes.get(s.m) else {
                r.mail_skipped_no_mailbox += 1;
                continue;
            };
            if !s.has_content() {
                r.mail_unchanged += 1;
                continue;
            }
            let (lo, hi) = (mb * MAILBOX_SPAN, (mb + 1) * MAILBOX_SPAN);
            let uid: Option<u32> = self
                .db
                .query_row(
                    "SELECT uid FROM tasks WHERE message_id=?1 AND uid>=?2 AND uid<?3 LIMIT 1",
                    params![s.message_id, lo, hi],
                    |x| x.get(0),
                )
                .optional()?;
            if let Some(uid) = uid {
                if self.apply_state(uid, s, now)? {
                    r.mail_applied += 1;
                } else {
                    r.mail_unchanged += 1;
                }
                continue;
            }
            if parked >= MAX_PARKED {
                r.mail_skipped_full += 1;
                continue;
            }
            let n = self.db.execute(
                "INSERT OR IGNORE INTO restored_state(mailbox,message_id,state,at) VALUES(?1,?2,?3,?4)",
                params![mb, s.message_id, serde_json::to_string(s)?, now],
            )?;
            if n > 0 {
                parked += 1;
                r.mail_parked += 1;
            } else {
                r.mail_unchanged += 1;
            }
        }
        Ok(())
    }
}
