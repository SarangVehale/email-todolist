//! Backup, export and restore (design: docs/designs/phase7.md).
//!
//! A backup is one JSON file with what you did on top of your mail: rules, sender choices, corrections, tasks of your own, what
//! you did with mail (keyed by Message-ID, never its text), the mailboxes and a short allow-list of options. It never holds a
//! password, a key, the phone hub's code, a network destination or any message text. Restoring only adds, in one transaction,
//! and the file is treated as untrusted input.
use crate::core::Core;
use crate::rules::{self, Rule};
use crate::settings::{mailbox_key, AccountCfg, Settings};
use crate::store::{MAX_MAILBOXES, MAX_NOTE_CHARS};
use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const FORMAT: &str = "inbox-agent-backup";
pub const VERSION: u32 = 1;
/// The largest file the app writes or reads.
pub const MAX_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_CHOICES: usize = 5000;
pub const MAX_CORRECTIONS: usize = 2000;
pub const MAX_OWN_TASKS: usize = 2000;
pub const MAX_MAIL_STATE: usize = 5000;

fn p993() -> u16 {
    993
}
fn three() -> u8 {
    3
}
fn open() -> String {
    "open".into()
}

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
#[serde(default)]
pub struct Backup {
    pub format: String,
    pub version: u32,
    pub app_version: String,
    pub created: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings: Option<Options>,
    pub mailboxes: Vec<BoxCfg>,
    pub rules: Vec<Rule>,
    pub sender_choices: Vec<Choice>,
    pub corrections: Vec<Correction>,
    pub own_tasks: Vec<OwnTask>,
    pub mail_state: Vec<MailState>,
}

/// A mailbox as the file names it: server, port and address. Never a password.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct BoxCfg {
    pub host: String,
    #[serde(default = "p993")]
    pub port: u16,
    pub user: String,
}
impl Default for BoxCfg {
    fn default() -> Self {
        Self {
            host: String::new(),
            port: 993,
            user: String::new(),
        }
    }
}
impl BoxCfg {
    pub fn account(&self) -> AccountCfg {
        AccountCfg {
            id: 0,
            host: self.host.clone(),
            port: self.port,
            user: self.user.clone(),
        }
    }
}

/// "Always important" or "mute" for a sender.
#[derive(Serialize, Deserialize, Default, Clone, Debug)]
#[serde(default)]
pub struct Choice {
    pub addr: String,
    pub level: String,
}

/// A correction of the ranking (what the ranker learns from).
#[derive(Serialize, Deserialize, Default, Clone, Debug)]
#[serde(default)]
pub struct Correction {
    pub sender: String,
    pub subject: String,
    pub old: u8,
    pub new: u8,
    pub addr: String,
    pub at: f64,
}

/// A task you added yourself.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct OwnTask {
    pub title: String,
    #[serde(default = "three")]
    pub priority: u8,
    pub due: String,
    pub remind: String,
    pub repeat: String,
    pub repeat_base: String,
    #[serde(default = "open")]
    pub status: String,
    pub star: bool,
    pub note: String,
}
impl Default for OwnTask {
    fn default() -> Self {
        Self {
            title: String::new(),
            priority: 3,
            due: String::new(),
            remind: String::new(),
            repeat: String::new(),
            repeat_base: String::new(),
            status: "open".into(),
            star: false,
            note: String::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
#[serde(default)]
pub struct DraftState {
    pub body: String,
    pub source: String,
}

/// What you did with one message of one mailbox. `m` is a position in the file's `mailboxes`.
#[derive(Serialize, Deserialize, Default, Clone, Debug)]
#[serde(default)]
pub struct MailState {
    pub m: usize,
    pub message_id: String,
    pub done: bool,
    pub star: bool,
    pub snooze_until: f64,
    pub note: String,
    pub draft: Option<DraftState>,
}
impl MailState {
    /// Is there anything of yours to keep?
    pub fn has_content(&self) -> bool {
        self.done
            || self.star
            || self.snooze_until > 0.0
            || !self.note.is_empty()
            || self.draft.is_some()
    }
}

/// The options a backup may carry. An allow-list: a setting that is not named here is never exported or restored, so a new
/// setting (or a secret, or a network destination) cannot slip in.
#[derive(Serialize, Deserialize, Default, Clone, Debug)]
#[serde(default)]
pub struct Options {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ai: Option<AiOpt>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alerts: Option<AlertOpt>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drafts: Option<DraftOpt>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub followups: Option<FollowOpt>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updates: Option<UpdateOpt>,
}
#[derive(Serialize, Deserialize, Default, Clone, Debug)]
#[serde(default)]
pub struct AiOpt {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ollama_model: Option<String>,
}
#[derive(Serialize, Deserialize, Default, Clone, Debug)]
#[serde(default)]
pub struct AlertOpt {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desktop: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub categories: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quiet_start: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quiet_end: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub digest_time: Option<String>,
}
#[derive(Serialize, Deserialize, Default, Clone, Debug)]
#[serde(default)]
pub struct DraftOpt {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tone: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_priority: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub daily_cap: Option<u32>,
}
#[derive(Serialize, Deserialize, Default, Clone, Debug)]
#[serde(default)]
pub struct FollowOpt {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub days: Option<u32>,
}
#[derive(Serialize, Deserialize, Default, Clone, Debug)]
#[serde(default)]
pub struct UpdateOpt {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub check: Option<bool>,
}

impl Options {
    /// The allow-listed options of these settings.
    pub fn of(s: &Settings) -> Self {
        Self {
            ai: Some(AiOpt {
                mode: Some(s.ai.mode.clone()),
                ollama_model: Some(s.ai.ollama_model.clone()),
            }),
            alerts: Some(AlertOpt {
                desktop: Some(s.alerts.desktop),
                categories: Some(s.alerts.categories.clone()),
                quiet_start: Some(s.alerts.quiet_start.clone()),
                quiet_end: Some(s.alerts.quiet_end.clone()),
                digest_time: Some(s.alerts.digest_time.clone()),
            }),
            drafts: Some(DraftOpt {
                auto: Some(s.drafts.auto),
                tone: Some(s.drafts.tone.clone()),
                signature: Some(s.drafts.signature.clone()),
                min_priority: Some(s.drafts.min_priority),
                daily_cap: Some(s.drafts.daily_cap),
            }),
            followups: Some(FollowOpt {
                days: Some(s.followups.days),
            }),
            updates: Some(UpdateOpt {
                check: Some(s.updates.check),
            }),
        }
    }

    /// These options laid over `current`, checked like any settings change, and the names of the ones that differ.
    pub fn apply(&self, current: &Settings) -> Result<(Settings, Vec<String>, Vec<String>)> {
        // Claude with your own key sends details of your mail to a third party and can cost money: it is chosen in Settings,
        // never by a file. (A local model, or rules only, can come back.)
        let mut me = self.clone();
        let mut skipped = vec![];
        if let Some(ai) = me.ai.as_mut() {
            if ai.mode.as_deref() == Some("anthropic") {
                ai.mode = None;
                skipped.push("ai.mode".to_string());
            }
        }
        let patch = serde_json::to_value(&me)?;
        let new = current.patched(&patch)?;
        new.validate()?;
        let (before, after) = (serde_json::to_value(current)?, serde_json::to_value(&new)?);
        let mut changed = vec![];
        if let Some(sections) = patch.as_object() {
            for (section, keys) in sections {
                for key in keys.as_object().into_iter().flatten().map(|(k, _)| k) {
                    if before[section][key] != after[section][key] {
                        changed.push(format!("{section}.{key}"));
                    }
                }
            }
        }
        Ok((new, changed, skipped))
    }
}

/// What a restore (or a preview of one) did.
#[derive(Serialize, Default, Debug, Clone)]
pub struct Report {
    pub applied: bool,
    pub rules_added: usize,
    pub rules_skipped: usize,
    pub sender_choices_added: usize,
    pub sender_choices_skipped: usize,
    pub corrections_added: usize,
    pub corrections_skipped: usize,
    pub own_tasks_added: usize,
    pub own_tasks_skipped: usize,
    pub reminders_dropped: usize,
    pub mail_applied: usize,
    pub mail_unchanged: usize,
    pub mail_parked: usize,
    pub mail_skipped_no_mailbox: usize,
    pub mail_skipped_full: usize,
    pub options_changed: Vec<String>,
    /// Options in the file that are not restored (a file cannot choose the paid cloud ranking).
    pub options_skipped: Vec<String>,
    pub mailboxes_added: usize,
    pub mailboxes_skipped: usize,
    /// "address (server:port)" of each mailbox that would be added, so the server is seen before a password is pasted.
    pub mailboxes_new: Vec<String>,
    /// What each new rule does, in words (the first 20).
    pub rules_new: Vec<String>,
}

// ───────────────────────── checking a file ─────────────────────────
fn clean_text(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .collect()
}

/// A single line of text with no control character and at most `max` characters.
fn line(s: &str, max: usize, what: &str) -> Result<()> {
    if s.chars().count() > max || s.chars().any(|c| c.is_control()) {
        bail!("The file has {what} that is too long or has odd characters in it");
    }
    Ok(())
}

fn too_many(what: &str, max: usize) -> anyhow::Error {
    anyhow!("The file has more {what} than a backup of this app can hold (the limit is {max})")
}

/// Reads and checks a backup. Anything wrong refuses the whole file.
pub fn parse(text: &str) -> Result<Backup> {
    if text.len() > MAX_BYTES {
        bail!("This file is too large to be a backup made by Inbox Agent (the limit is 8 MB)");
    }
    let mut b: Backup = serde_json::from_str(text)
        .map_err(|_| anyhow!("This is not a backup made by Inbox Agent: it could not be read"))?;
    if b.format != FORMAT {
        bail!("This is not a backup made by Inbox Agent");
    }
    if b.version == 0 {
        bail!("This is not a backup made by Inbox Agent: it has no version");
    }
    if b.version > VERSION {
        bail!("This backup was made by a newer version of Inbox Agent. Update the app, then restore it");
    }
    if b.mailboxes.len() > MAX_MAILBOXES as usize {
        return Err(too_many("mailboxes", MAX_MAILBOXES as usize));
    }
    if b.rules.len() > rules::MAX_RULES {
        return Err(too_many("rules", rules::MAX_RULES));
    }
    if b.sender_choices.len() > MAX_CHOICES {
        return Err(too_many("sender choices", MAX_CHOICES));
    }
    if b.corrections.len() > MAX_CORRECTIONS {
        return Err(too_many("corrections", MAX_CORRECTIONS));
    }
    if b.own_tasks.len() > MAX_OWN_TASKS {
        return Err(too_many("tasks of your own", MAX_OWN_TASKS));
    }
    if b.mail_state.len() > MAX_MAIL_STATE {
        return Err(too_many("entries about mail", MAX_MAIL_STATE));
    }
    for m in &b.mailboxes {
        m.account().check()?;
    }
    // the rules go through the same check as rules typed in; ids are made again on restore, so a repeated one is no problem
    let mut copy = b.rules.clone();
    for (i, r) in copy.iter_mut().enumerate() {
        r.id = format!("r{i}");
    }
    rules::validate(&copy)?;
    for c in &mut b.sender_choices {
        c.addr = c.addr.trim().to_lowercase();
        if c.addr.is_empty()
            || c.addr.chars().count() > 320
            || !c.addr.contains('@')
            || c.addr
                .chars()
                .any(|ch| ch.is_control() || ch.is_whitespace())
            || !matches!(c.level.as_str(), "important" | "mute")
        {
            bail!("The file has a sender choice that is not valid");
        }
    }
    for c in &b.corrections {
        line(&c.sender, 300, "a sender name")?;
        line(&c.subject, 600, "a subject")?;
        line(&c.addr, 320, "an address")?;
        if !(1..=5).contains(&c.old)
            || !(1..=5).contains(&c.new)
            || !c.at.is_finite()
            || c.at < 0.0
            || c.at > now_secs() + 86400.0
        {
            bail!("The file has a correction that is not valid");
        }
    }
    let invalid_task = || anyhow!("The file has a task of yours that is not valid");
    for t in &mut b.own_tasks {
        t.title = t.title.trim().to_string();
        if t.title.is_empty()
            || t.title.chars().count() > 200
            || t.title.chars().any(|c| c.is_control())
            || !(1..=5).contains(&t.priority)
            || !(t.due.is_empty() || crate::dates::valid_iso(&t.due))
            || !crate::reminders::valid_repeat(&t.repeat)
            || !matches!(t.status.as_str(), "open" | "done")
            || t.note.chars().count() > MAX_NOTE_CHARS
        {
            return Err(invalid_task());
        }
        for when in [&t.remind, &t.repeat_base] {
            if !when.is_empty()
                && !crate::reminders::parse_local(when).is_some_and(crate::reminders::year_ok)
            {
                return Err(invalid_task());
            }
        }
        if !t.repeat.is_empty() && t.remind.is_empty() && t.due.is_empty() {
            return Err(invalid_task());
        }
        t.note = clean_text(&t.note);
    }
    for s in &mut b.mail_state {
        if s.m >= b.mailboxes.len()
            || s.message_id.is_empty()
            || s.message_id.chars().count() > 998
            || s.message_id.chars().any(|c| c.is_control())
            || !s.snooze_until.is_finite()
            || s.snooze_until < 0.0
            || s.snooze_until > now_secs() + 366.0 * 86400.0 // the furthest a snooze typed in can go
            || s.note.chars().count() > MAX_NOTE_CHARS
        {
            bail!("The file has an entry about a message that is not valid");
        }
        s.note = clean_text(&s.note);
        if let Some(d) = &mut s.draft {
            if d.body.chars().count() > crate::store::MAX_DRAFT_CHARS
                || !matches!(d.source.as_str(), "ai" | "template" | "user")
            {
                bail!("The file has a reply draft that is not valid");
            }
            d.body = clean_text(&d.body);
        }
    }
    if let Some(o) = &b.settings {
        o.apply(&Settings::default())?; // values that no settings screen would accept
    }
    Ok(b)
}

// ───────────────────────── making a file ─────────────────────────
/// A backup of this computer's data, as a value.
pub fn build(core: &Core) -> Result<Backup> {
    let settings = core.settings();
    let store = core.store()?;
    let boxes = settings.mailboxes();
    let ids: Vec<u32> = boxes.iter().map(|m| m.id).collect();
    Ok(Backup {
        format: FORMAT.into(),
        version: VERSION,
        app_version: env!("CARGO_PKG_VERSION").into(),
        created: chrono::Utc::now().to_rfc3339(),
        settings: Some(Options::of(&settings)),
        mailboxes: boxes
            .iter()
            .map(|m| BoxCfg {
                host: m.host.clone(),
                port: m.port,
                user: m.user.clone(),
            })
            .collect(),
        rules: store.rules(),
        sender_choices: store.backup_choices(),
        corrections: store.backup_corrections(),
        own_tasks: store.backup_own_tasks(),
        mail_state: store.backup_mail_state(&ids),
    })
}

pub fn to_json(b: &Backup) -> Result<String> {
    let text = serde_json::to_string_pretty(b)?;
    if text.len() > MAX_BYTES {
        bail!("Your data is too large for one backup file (over 8 MB)");
    }
    Ok(text)
}

/// What `export_in` wrote.
#[derive(Debug, Clone)]
pub struct Exported {
    pub path: PathBuf,
    pub counts: Value,
}

/// Writes `text` as a NEW file in `dir`: an existing file is never replaced (the name gets `-2`, `-3`...). Readable by the
/// owner only where the system has such a thing.
fn write_new(dir: &Path, stem: &str, text: &str) -> std::io::Result<PathBuf> {
    use std::io::Write;
    for n in 1..=99 {
        let name = if n == 1 {
            format!("{stem}.json")
        } else {
            format!("{stem}-{n}.json")
        };
        let path = dir.join(name);
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        match opts.open(&path) {
            Ok(mut f) => {
                f.write_all(text.as_bytes())?;
                return Ok(path);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "there are already 99 backup files with this name in the folder",
    ))
}

fn limit_of(kind: &str) -> usize {
    match kind {
        "sender_choices" => MAX_CHOICES,
        "corrections" => MAX_CORRECTIONS,
        "own_tasks" => MAX_OWN_TASKS,
        _ => MAX_MAIL_STATE,
    }
}

/// Saves a backup of this computer's data into `dir`.
pub fn export_in(core: &Core, dir: &Path) -> Result<Exported> {
    let b = build(core)?;
    let text = to_json(&b)?;
    let stem = format!(
        "inbox-agent-backup-{}",
        chrono::Local::now().format("%Y%m%d-%H%M%S")
    );
    let path =
        write_new(dir, &stem, &text).map_err(|e| anyhow!("Could not save the backup file: {e}"))?;
    // a file holds at most so many of each kind: say what did not fit instead of leaving it out silently
    let (choices, fixes, own, mail) = core.store()?.backup_totals();
    let mut left_out = serde_json::Map::new();
    for (kind, total, kept) in [
        ("sender_choices", choices, b.sender_choices.len()),
        ("corrections", fixes, b.corrections.len()),
        ("own_tasks", own, b.own_tasks.len()),
        ("mail_state", mail, b.mail_state.len()),
    ] {
        if total > kept && kept >= limit_of(kind) {
            left_out.insert(kind.to_string(), json!(total - kept));
        }
    }
    let counts = json!({
        "rules": b.rules.len(),
        "sender_choices": b.sender_choices.len(),
        "corrections": b.corrections.len(),
        "own_tasks": b.own_tasks.len(),
        "mail_state": b.mail_state.len(),
        "mailboxes": b.mailboxes.len(),
        "left_out": left_out,
    });
    let _ = core.store()?.log("saved a backup file");
    Ok(Exported { path, counts })
}

// ───────────────────────── restoring ─────────────────────────
/// Which local mailbox each mailbox of the file is (None: not set up here), and the settings with the new ones added.
struct BoxPlan {
    map: Vec<Option<u32>>,
    settings: Settings,
    added: usize,
    skipped: usize,
    new_ids: Vec<u32>,
    labels: Vec<String>,
}

fn plan_mailboxes(current: &Settings, file: &[BoxCfg], add: bool) -> BoxPlan {
    let mut s = current.clone();
    let mut map = vec![];
    let (mut added, mut skipped) = (0, 0);
    let (mut new_ids, mut labels) = (vec![], vec![]);
    for b in file {
        let acct = b.account();
        let key = mailbox_key(&acct);
        let have = s.mailboxes();
        if let Some(m) = have.iter().find(|m| mailbox_key(m) == key) {
            map.push(Some(m.id));
            skipped += 1;
            continue;
        }
        let used: std::collections::HashSet<u32> = have.iter().map(|m| m.id).collect();
        match (add, (0..MAX_MAILBOXES).find(|n| !used.contains(n))) {
            (true, Some(id)) => {
                let new = AccountCfg { id, ..acct };
                if id == 0 {
                    s.account = new;
                } else {
                    s.more_accounts.push(new);
                }
                map.push(Some(id));
                added += 1;
                new_ids.push(id);
                labels.push(format!("{} ({}:{})", b.user, b.host, b.port));
            }
            _ => {
                map.push(None);
                skipped += 1;
            }
        }
    }
    BoxPlan {
        map,
        settings: s,
        added,
        skipped,
        new_ids,
        labels,
    }
}

/// Checks a file and either reports what restoring it would do (`apply` false: nothing is written) or does it. With
/// `with_options` the allow-listed options come back too, and mailboxes that are not set up are added without a password.
pub fn import(core: &Arc<Core>, text: &str, apply: bool, with_options: bool) -> Result<Report> {
    let current = core.settings();
    if !current.onboarded || current.mailboxes().is_empty() {
        bail!("Finish setting up your first mailbox before restoring a backup");
    }
    let b = parse(text)?;
    let plan = plan_mailboxes(&current, &b.mailboxes, with_options);
    let (new_settings, changed, skipped) = match (&b.settings, with_options) {
        (Some(o), true) => o.apply(&plan.settings)?,
        _ => (plan.settings.clone(), vec![], vec![]),
    };
    new_settings.validate()?;
    let store = core.store()?;
    if apply {
        // a mailbox that is new here starts from nothing: a sync of an earlier mailbox with this number may have been
        // mid-message when that mailbox was removed, and can have left a cursor or a task behind
        // and with no password: one left in the keychain under that number (by an earlier install, or a mailbox removed in a
        // hurry) must not be sent to the server this file names
        for id in &plan.new_ids {
            store.clear_mailbox(*id)?;
            crate::secrets::delete(&crate::secrets::imap_key(*id));
        }
    }
    let mut report = store.restore_apply(
        &b,
        &plan.map,
        now_secs(),
        crate::reminders::now_local(),
        !apply,
    )?;
    report.applied = apply;
    report.options_changed = changed;
    report.options_skipped = skipped;
    report.mailboxes_added = plan.added;
    report.mailboxes_skipped = plan.skipped;
    report.mailboxes_new = plan.labels.clone();
    if apply {
        let _ = store.log("restored a backup file");
        if with_options && (!report.options_changed.is_empty() || plan.added > 0) {
            new_settings.save(&core.dir).map_err(|e| {
                anyhow!(
                    "Your data was restored, but the options and mailboxes could not be saved: {e}"
                )
            })?;
            core.start(); // the new mailboxes ask for their passwords; the options take effect
        }
    }
    Ok(report)
}

pub fn now_secs() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}
