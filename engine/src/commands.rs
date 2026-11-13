//! Task commands: everything the list does to a task (done, star, priority, snooze, sender preference,
//! reply drafts, export). Shared by the desktop window and, for the safe subset, the phone hub.
use crate::ai::make_triager;
use crate::core::Core;
use crate::drafts;
use crate::jobs::{snooze_on, snooze_until};
use crate::mail::{valid_address, Email};
use crate::secrets;
use crate::store::Task;
use crate::triage::DraftCtx;
use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

/// Commands a phone may use. Account, settings and secrets stay desktop-only, and so does anything that
/// spends AI quota or writes to the mailbox (a stolen access code must not be able to do either).
pub const PUBLIC: &[&str] = &[
    "set_done",
    "set_star",
    "set_priority",
    "set_snooze",
    "set_pref",
    "draft_update",
    "draft_discard",
    "set_note",
    "add_task",
    "set_due",
    "set_title",
];

/// Task commands that only the desktop window may use.
const DESKTOP_ONLY: &[&str] = &[
    "draft_generate",
    "draft_to_mailbox",
    "export_markdown",
    "read_body",
    "diagnostics",
    "rules_save",
    "rules_apply",
];

/// Automatic drafting runs one message at a time so the daily budget cannot be overshot.
static AUTO_DRAFT: Mutex<()> = Mutex::new(());

/// Free text typed by the user for the model: short, one paragraph, no control characters.
fn clean_instruction(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_control())
        .take(200)
        .collect::<String>()
        .trim()
        .to_string()
}

fn draft_ctx(core: &Core, instruction: &str) -> DraftCtx {
    let d = core.settings().drafts;
    DraftCtx {
        tone: d.tone,
        signature: d.signature,
        instruction: clean_instruction(instruction),
    }
}

/// Writes (or rewrites) the draft for a task. Returns the draft text and its source.
pub fn generate_draft(core: &Core, uid: u32, instruction: &str) -> Result<(String, &'static str)> {
    let store = core.store()?;
    let t = store.get(uid).ok_or_else(|| anyhow!("no such task"))?;
    if uid >= crate::store::LOCAL_UID_BASE {
        return Err(anyhow!(
            "This task did not come from an email, so there is nothing to reply to"
        ));
    }
    let email = Email {
        uid,
        message_id: t.message_id.clone(),
        sender_name: t.sender.clone(),
        sender_addr: t.addr.clone(),
        subject: t.subject.clone(),
        received: t.received,
        excerpt: t.excerpt.clone(),
        body: store.body(uid),
        in_reply_to: vec![],
        reply_to: t.reply_to.clone(),
        references: store.references(uid),
        bulk_headers: false,
        auth_fail: false,
        attachment_count: 0,
        attachments: vec![],
        unsubscribe: String::new(),
        thread: String::new(),
    };
    let s = core.settings();
    let triager = make_triager(&s.ai, secrets::get("anthropic-key"), vec![]);
    let (body, source) = drafts::generate(triager.as_ref(), &email, &draft_ctx(core, instruction));
    if !store.set_draft(uid, &body, source)? {
        return Err(anyhow!("could not save the draft"));
    }
    Ok((body, source))
}

/// Called for each newly triaged message: writes a draft in the background when the user turned that on,
/// the message needs a reply, a model is configured and today's budget allows it. Never sends anything.
pub fn auto_draft(core: &Arc<Core>, task: Task) {
    let s = core.settings();
    if !s.drafts.auto
        || !task.needs_reply
        || task.priority < s.drafts.min_priority
        || s.ai.mode == "rules"
        || task.draft.is_some()
    {
        return;
    }
    let core = core.clone();
    std::thread::spawn(move || {
        let _one_at_a_time = AUTO_DRAFT.lock().unwrap_or_else(|e| e.into_inner());
        let Ok(store) = core.store() else { return };
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let cap = core.settings().drafts.daily_cap as usize;
        if !store.reserve_ai_draft(cap, &today).unwrap_or(false) {
            let _ = store.log("daily draft limit reached; no draft written");
            return;
        }
        // Only keep it when a model actually wrote it; template drafts are never made automatically.
        if let Ok((_, source)) = generate_draft(&core, task.uid, "") {
            if source != "ai" {
                let _ = store.delete_draft(task.uid);
            }
        }
    });
}

/// Markdown-safe text: control characters dropped and markup characters escaped, so a hostile subject
/// or sender cannot add list items or links to the copied export.
fn md(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_control())
        .flat_map(|c| {
            if "\\`*_[]()<>#|!".contains(c) {
                vec!['\\', c]
            } else {
                vec![c]
            }
        })
        .collect()
}

fn markdown_export(tasks: &[Task]) -> String {
    let mut out = String::from("# To-do\n");
    for t in tasks
        .iter()
        .filter(|t| t.status == "open" && t.snooze_until <= 0.0)
    {
        out.push_str(&format!(
            "\n- [ ] **P{}** {} — {} ({}){}{}",
            t.priority,
            md(if t.action.is_empty() {
                &t.subject
            } else {
                &t.action
            }),
            md(&t.sender),
            md(&t.category),
            if t.due.is_empty() {
                String::new()
            } else {
                format!(", due {}", md(&t.due))
            },
            if t.draft.is_some() {
                ", draft ready"
            } else {
                ""
            }
        ));
    }
    out.push('\n');
    out
}

/// Handles the task commands. Returns None when `cmd` is not one of them.
pub fn run(core: &Arc<Core>, cmd: &str, a: &Value) -> Option<Result<Value>> {
    if !PUBLIC.contains(&cmd) && !DESKTOP_ONLY.contains(&cmd) {
        return None;
    }
    let uid = a["uid"].as_u64().unwrap_or(0) as u32;
    let res = (|| -> Result<Value> {
        let store = core.store()?;
        match cmd {
            "set_done" => Ok(json!(
                store.set_done(uid, a["done"].as_bool().unwrap_or(true))?
            )),
            "set_star" => Ok(json!(
                store.set_star(uid, a["star"].as_bool().unwrap_or(true))?
            )),
            "set_priority" => Ok(json!(
                store.set_priority(uid, a["priority"].as_u64().unwrap_or(0) as u8)?
            )),
            "set_snooze" => {
                let now = chrono::Local::now();
                let until = match a["date"].as_str() {
                    Some(d) => snooze_on(d, now)
                        .ok_or_else(|| anyhow!("Pick a day from tomorrow to a year ahead"))?,
                    None => snooze_until(a["preset"].as_str().unwrap_or(""), now)
                        .ok_or_else(|| anyhow!("unknown snooze option"))?,
                };
                Ok(json!(store.set_snooze(uid, until)?))
            }
            "set_due" => Ok(json!(store.set_due(uid, a["due"].as_str().unwrap_or("x"))?)),
            "set_title" => Ok(json!(
                store.set_title(uid, a["title"].as_str().unwrap_or(""))?
            )),
            "read_body" => {
                if store.get(uid).is_none() {
                    return Err(anyhow!("no such task"));
                }
                Ok(json!(store.body(uid)))
            }
            "set_pref" => {
                let t = store.get(uid).ok_or_else(|| anyhow!("no such task"))?;
                Ok(json!(
                    store.set_pref(&t.addr, a["level"].as_str().unwrap_or(""))?
                ))
            }
            "draft_generate" => {
                let edited = store
                    .get(uid)
                    .and_then(|t| t.draft)
                    .is_some_and(|d| d.source == "user");
                if edited && !a["force"].as_bool().unwrap_or(false) {
                    return Err(anyhow!("This draft has your edits. Confirm to replace it."));
                }
                let (body, source) =
                    generate_draft(core, uid, a["instruction"].as_str().unwrap_or(""))?;
                Ok(json!({"body": body, "source": source}))
            }
            "draft_update" => {
                let body = a["body"].as_str().unwrap_or("");
                if !store.set_draft(uid, body, "user")? {
                    return Err(anyhow!(
                        "that draft is too long or the task no longer exists"
                    ));
                }
                Ok(json!(true))
            }
            "draft_discard" => Ok(json!(store.delete_draft(uid)?)),
            "set_note" => Ok(json!(store.set_note(uid, a["note"].as_str().unwrap_or(""))?)),
            "add_task" => {
                let uid = store
                    .add_local_task(
                        a["title"].as_str().unwrap_or(""),
                        a["priority"].as_u64().unwrap_or(3).min(255) as u8,
                        a["due"].as_str().unwrap_or(""),
                    )?
                    .ok_or_else(|| anyhow!("Give the task a title (up to 200 characters), a priority from 1 to 5, and a date like 2026-11-20 if you set one"))?;
                Ok(json!(uid))
            }
            "draft_to_mailbox" => {
                let s = core.settings();
                if !s.writeback {
                    return Err(anyhow!("Turn on “Let Inbox Agent file mail” in Settings to save drafts to your mailbox"));
                }
                if !valid_address(&s.account.user) {
                    return Err(anyhow!("Your login name isn't an email address, so drafts can't be saved to your mailbox"));
                }
                let t = store.get(uid).ok_or_else(|| anyhow!("no such task"))?;
                if t.draft.is_none() || !valid_address(&t.reply_to) {
                    return Err(anyhow!("there is no draft or no valid reply address"));
                }
                if !store
                    .pending(1000)
                    .iter()
                    .any(|i| i.op == "draft" && i.uid == uid)
                {
                    store.queue(uid, "draft", "")?;
                }
                Ok(json!(true))
            }
            "rules_save" => {
                let rules: Vec<crate::rules::Rule> = serde_json::from_value(a["rules"].clone())
                    .map_err(|_| anyhow!("Those rules couldn't be read"))?;
                store.set_rules(&rules)?;
                Ok(json!(true))
            }
            "rules_apply" => Ok(serde_json::to_value(
                store.apply_rules(a["dry"].as_bool().unwrap_or(true))?,
            )?),
            "export_markdown" => Ok(json!(markdown_export(&store.all_tasks()?))),
            "diagnostics" => {
                let s = core.settings();
                let tasks = store.all_tasks()?;
                let open = tasks.iter().filter(|t| t.status == "open").count();
                let done = tasks.iter().filter(|t| t.status == "done").count();
                let gone = tasks
                    .iter()
                    .filter(|t| t.status == "open" && t.gone)
                    .count();
                let log: Vec<String> = store.recent_log(30).into_iter().map(|l| l.1).collect();
                let crash: Vec<String> = std::fs::read_to_string(core.dir.join("crash.log"))
                    .unwrap_or_default()
                    .lines()
                    .rev()
                    .take(20)
                    .map(String::from)
                    .collect();
                let status = core.status_text();
                Ok(json!(crate::diag::report(&crate::diag::ReportInput {
                    version: env!("CARGO_PKG_VERSION"),
                    os: &format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
                    status: &status,
                    open,
                    done,
                    gone,
                    ai_mode: &s.ai.mode,
                    writeback: s.writeback,
                    hub: s.hub.enabled,
                    quiet_hours: !s.alerts.quiet_start.is_empty(),
                    google: false,
                    host: &s.account.host,
                    log: &log,
                    crash: &crash,
                })))
            }
            _ => Err(anyhow!("unknown command: {cmd}")),
        }
    })();
    Some(res)
}
