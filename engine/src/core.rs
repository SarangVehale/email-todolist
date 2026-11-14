//! Application core: owns settings, the background threads and the command interface (`rpc`)
//! shared by the desktop window (Tauri) and the phone hub (HTTP).
use crate::ai::{self, make_triager};
use crate::mail::Email;
use crate::settings::{is_gmail, AccountCfg, Settings};
use crate::store::{global_uid, Store};
use crate::triage::Triage;
use crate::watcher::{self, Account};
use crate::{commands, hub, jobs, secrets, writeback};
use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Whether the database integrity check has already passed in this launch.
static HEALTH_CHECKED: AtomicBool = AtomicBool::new(false);

/// Shows a desktop notification, given a title and a body. The window shell supplies it; `Core` calls it for alerts when desktop alerts are on.
pub type Notify = Arc<dyn Fn(&str, &str) + Send + Sync>;

/// The running app: the data folder, the sync status of each mailbox, the stop flag of the background threads and the notifier.
/// Settings and the database are read from the folder on each use, so the window and the phone hub always see current values.
pub struct Core {
    pub dir: PathBuf,
    notify: Notify,
    /// One status per mailbox (by number); the one the app shows is the worst of them.
    status: Arc<Mutex<BTreeMap<u32, String>>>,
    stop: Mutex<Arc<AtomicBool>>,
}

fn s_arg(a: &Value, k: &str) -> String {
    a[k].as_str().unwrap_or("").trim().to_string()
}

/// The mail server port in a request: 993 when none is given, else a whole number from 1 to 65535. A number outside that is
/// refused; it used to be cut to 16 bits, so 66529 was read as 993.
fn port_arg(a: &Value) -> Result<u16> {
    match a.get("port") {
        None | Some(Value::Null) => Ok(993),
        Some(v) => v
            .as_u64()
            .and_then(|p| u16::try_from(p).ok())
            .filter(|p| *p != 0)
            .ok_or_else(|| anyhow!("That mail server port isn't valid")),
    }
}

fn ascii(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii() && !c.is_control() {
                c
            } else {
                '?'
            }
        })
        .collect()
}

impl Core {
    /// A core for the data folder `dir`, created if it is missing. It starts no threads; call `start` for that.
    pub fn new(dir: PathBuf, notify: Notify) -> Arc<Self> {
        let _ = std::fs::create_dir_all(&dir);
        Arc::new(Self {
            dir,
            notify,
            status: Arc::new(Mutex::new(BTreeMap::new())),
            stop: Mutex::new(Arc::new(AtomicBool::new(true))),
        })
    }

    /// The saved settings, read from disk each time; unreadable settings give the defaults.
    pub fn settings(&self) -> Settings {
        Settings::load(&self.dir)
    }

    /// A new connection to the database `agent.db` in the data folder. Fails if it cannot be opened, for instance when it was made by a newer app.
    pub fn store(&self) -> Result<Store> {
        Store::open(&self.dir.join("agent.db"))
    }

    /// The database as mailbox `mailbox`'s sync and write-back see it: their numbers are IMAP UIDs, and what they list or
    /// reset is that mailbox's alone.
    pub fn store_for(&self, mailbox: u32) -> Result<Store> {
        Ok(self.store()?.scoped(mailbox))
    }

    /// The sync status as shown in the app (for the support report): the worst of the mailboxes.
    pub fn status_text(&self) -> String {
        let all: Vec<String> = self
            .status
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .cloned()
            .collect();
        if all.is_empty() {
            "starting".into()
        } else {
            worst_status(&all)
        }
    }

    fn set_status(&self, mailbox: u32, s: &str) {
        self.status
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(mailbox, s.to_string());
    }

    fn status_of(&self, mailbox: u32) -> String {
        self.status
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&mailbox)
            .cloned()
            .unwrap_or_else(|| "starting".into())
    }

    /// `Ok(None)`: no saved password. `Err(status)`: the system keychain could not be read.
    fn mailbox_account(&self, m: &AccountCfg) -> std::result::Result<Option<Account>, String> {
        match secrets::lookup(&secrets::imap_key(m.id)) {
            Ok(Some(password)) => Ok(Some(Account {
                host: m.host.clone(),
                port: m.port,
                user: m.user.clone(),
                password,
            })),
            Ok(None) => Ok(None),
            Err(e) => Err(format!(
                "error: your system keychain is not available ({e}). Unlock it and restart the app"
            )),
        }
    }

    /// (Re)starts the background threads from the current settings.
    pub fn start(self: &Arc<Self>) {
        let new_stop = Arc::new(AtomicBool::new(false));
        std::mem::replace(
            &mut *self.stop.lock().unwrap_or_else(|e| e.into_inner()),
            new_stop.clone(),
        )
        .store(true, Ordering::Relaxed);
        let s = self.settings();
        self.status
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();

        // A damaged database is set aside and rebuilt; one from a newer app version is refused, untouched. The check
        // runs once per launch (start() also runs after every settings save). A busy database is retried by the
        // sync itself, so only the "newer version" case stops the app here.
        if !HEALTH_CHECKED.swap(true, Ordering::Relaxed) {
            if let Err(e) = Store::open_recovering(&self.dir.join("agent.db")) {
                HEALTH_CHECKED.store(false, Ordering::Relaxed); // not checked after all: try again on the next start
                self.set_status(0, &format!("error: {e}"));
                if e.downcast_ref::<crate::store::TooNew>().is_some() {
                    return;
                }
            }
        }

        if s.hub.enabled {
            let (core, stop) = (self.clone(), new_stop.clone());
            std::thread::spawn(move || hub::serve(core, stop));
        }
        {
            let (core, stop) = (self.clone(), new_stop.clone());
            std::thread::spawn(move || jobs::run(core, stop));
        }
        let boxes = s.mailboxes();
        if !s.onboarded || boxes.is_empty() {
            self.set_status(0, if s.onboarded { "no password" } else { "setup" });
            return;
        }
        for m in boxes {
            match self.mailbox_account(&m) {
                Ok(Some(acct)) => self.spawn_mailbox(m.id, acct, new_stop.clone()),
                Ok(None) => self.set_status(m.id, "no password"),
                Err(message) => self.set_status(m.id, &message),
            }
        }
    }

    /// The sync and the write-back of one mailbox, each on its own thread with its own connection and a store scoped to
    /// the mailbox.
    fn spawn_mailbox(self: &Arc<Self>, id: u32, acct: Account, stop: Arc<AtomicBool>) {
        let gmail = is_gmail(&acct.host);
        // watcher
        {
            let (core, stop, acct) = (self.clone(), stop.clone(), acct.clone());
            std::thread::spawn(move || {
                let Ok(store) = core.store_for(id) else {
                    return;
                };
                let _ = store.prune(90, 30);
                let s = core.settings();
                let triager = make_triager(
                    &s.ai,
                    secrets::get("anthropic-key"),
                    store.recent_overrides(10),
                );
                let initial = store.last_uid() == 0;
                let live = Arc::new(AtomicBool::new(false));
                let (live_w, st) = (live.clone(), core.clone());
                let stopped = stop.clone();
                let status = move |x: &str| {
                    if x == "idle" {
                        live_w.store(true, Ordering::Relaxed);
                    }
                    // a sync that was stopped (restart, or its mailbox removed) must not report on a mailbox that may be gone
                    if !stopped.load(Ordering::Relaxed) {
                        st.set_status(id, x);
                    }
                };
                let core2 = core.clone();
                watcher::run(
                    &acct,
                    &store,
                    triager.as_ref(),
                    watcher::SyncOptions {
                        first_run_limit: s.first_run_limit,
                        follow_days: s.followups.days,
                    },
                    &stop,
                    &status,
                    &mut |e: &Email, t: &Triage| {
                        // never alert for the very first backfill of a mailbox, only for genuinely new mail
                        core2.on_triaged(id, gmail, e, t, !initial || live.load(Ordering::Relaxed));
                    },
                );
            });
        }
        // write-back
        {
            let core = self.clone();
            std::thread::spawn(move || {
                let Ok(store) = core.store_for(id) else {
                    return;
                };
                while !stop.load(Ordering::Relaxed) {
                    let mut wait = 2;
                    if core.settings().writeback && !store.pending(1).is_empty() {
                        if let Err(e) = writeback::drain(&acct, &store) {
                            let _ = store.log(&format!("could not reach mailbox for updates: {e}"));
                            wait = 15;
                        }
                    }
                    for _ in 0..wait {
                        if stop.load(Ordering::Relaxed) {
                            return;
                        }
                        std::thread::sleep(Duration::from_secs(1));
                    }
                }
            });
        }
    }

    /// Desktop notification plus ntfy, if configured.
    fn deliver(&self, title: &str, body: &str, urgent: bool) {
        let s = self.settings();
        if s.alerts.desktop {
            (self.notify)(title, body);
        }
        if !s.alerts.ntfy_topic.is_empty() {
            let _ = crate::ai::http_agent(10)
                .post(&format!(
                    "{}/{}",
                    s.alerts.ntfy_server.trim_end_matches('/'),
                    s.alerts.ntfy_topic
                ))
                .set("Title", &ascii(title))
                .set("Priority", if urgent { "5" } else { "4" })
                .send_string(body);
        }
    }

    /// A notification that is not about a single message (digest, snooze).
    pub fn notify_user(&self, title: &str, body: &str) {
        self.deliver(title, body, false);
    }

    /// Alerts for important mail. Quiet hours hold back everything except urgent mail.
    fn alert(&self, e: &Email, t: &Triage) {
        if !(t.priority >= 4 || t.urgent) {
            return;
        }
        let s = self.settings();
        if !t.urgent
            && !s.alerts.categories.is_empty()
            && !s.alerts.categories.contains(&t.category)
        {
            return;
        }
        if !t.urgent
            && jobs::in_quiet_window(
                &s.alerts.quiet_start,
                &s.alerts.quiet_end,
                jobs::now_minutes(),
            )
        {
            return;
        }
        let title = format!(
            "{}{}",
            if t.urgent { "URGENT · " } else { "" },
            e.sender_name
        );
        let body = if t.action.is_empty() {
            &e.subject
        } else {
            &t.action
        };
        self.deliver(&title, body, t.urgent);
    }

    /// Everything that follows a newly triaged message: alert, category label, automatic reply draft.
    #[doc(hidden)]
    pub fn on_triaged(
        self: &Arc<Self>,
        mailbox: u32,
        gmail: bool,
        e: &Email,
        t: &Triage,
        alert: bool,
    ) {
        if alert {
            self.alert(e, t);
        }
        let s = self.settings();
        let Ok(store) = self.store() else { return };
        let uid = global_uid(mailbox, e.uid); // the task's id: the message's number in its own mailbox
        if s.writeback && gmail {
            let mut label: String = t
                .category
                .chars()
                .filter(|c| c.is_ascii_alphanumeric())
                .take(24)
                .collect();
            if let Some(first) = label.get_mut(0..1) {
                first.make_ascii_uppercase();
            }
            if !label.is_empty() {
                let _ = store.queue(uid, "category", &format!("AI/{label}"));
            }
        }
        if let Some(task) = store.get(uid).filter(|t| t.status == "open") {
            commands::auto_draft(self, task); // finished mail (a restore can finish it as it arrives) needs no reply
        }
    }

    fn state(&self, public: bool, done_shown: usize) -> Result<Value> {
        let store = self.store()?;
        let s = self.settings();
        // The phone hub only needs to know whether write-back is on; it never sees account or alert details.
        let settings = if public {
            json!({"onboarded": s.onboarded, "writeback": s.writeback})
        } else {
            serde_json::to_value(&s)?
        };
        let (tasks, done_total) = store.tasks_for_window(done_shown)?;
        let mut state = json!({
            "settings": settings,
            "tasks": tasks,
            "done_total": done_total,
            "log": store.recent_log(100).into_iter().map(|(at, msg)| json!({"at": at, "msg": if public { for_the_phone(&msg) } else { msg }})).collect::<Vec<_>>(),
            "status": self.status_text(),
            "pending": store.pending(1000).len(),
            "has_anthropic_key": !public && secrets::get("anthropic-key").is_some(),
            "gmail": s.mailboxes().iter().any(|m| is_gmail(&m.host)),
            "update_feature": !crate::updates::UPDATE_REPO.is_empty(),
            "update": if public || !s.updates.check || crate::updates::UPDATE_REPO.is_empty() {
                None
            } else {
                crate::updates::pending(
                    store.meta_get("update_available").as_deref(),
                    crate::updates::UPDATE_REPO,
                    env!("CARGO_PKG_VERSION"),
                )
                .map(|u| json!({"version": u.version, "url": u.url}))
            },
        });
        // what the user told the app to look for is private to this computer: the phone never sees it
        if !public {
            state["rules"] = serde_json::to_value(store.rules())?;
            // the mailboxes (names, addresses, how each is doing) are for the desktop window only
            state["accounts"] = json!(s
                .mailboxes()
                .iter()
                .map(|m| json!({
                    "id": m.id, "user": m.user, "host": m.host, "gmail": is_gmail(&m.host),
                    "status": self.status_of(m.id),
                }))
                .collect::<Vec<_>>());
        }
        Ok(state)
    }

    /// Commands reachable from the phone hub (everything else is desktop-only).
    pub fn rpc_public(self: &Arc<Self>, cmd: &str, args: &Value) -> Result<Value> {
        match cmd {
            "state" => self.state(true, done_asked(args)),
            c if commands::PUBLIC.contains(&c) => self.rpc(cmd, args),
            _ => Err(anyhow!("not available from the hub")),
        }
    }

    /// Runs one named command with JSON arguments for the desktop window and returns its JSON answer; an unknown command is an error.
    /// This reaches everything, settings and secrets included, so the phone hub must go through `rpc_public` instead.
    pub fn rpc(self: &Arc<Self>, cmd: &str, a: &Value) -> Result<Value> {
        if let Some(r) = commands::run(self, cmd, a) {
            return r;
        }
        match cmd {
            "state" => self.state(false, done_asked(a)),
            "test_account" => {
                let acct = Account {
                    host: s_arg(a, "host"),
                    port: port_arg(a)?,
                    user: s_arg(a, "user"),
                    password: s_arg(a, "password").replace(' ', ""),
                };
                // the same checks as for saving a mailbox, before anything connects
                if acct.host.is_empty() || acct.user.is_empty() {
                    return Err(anyhow!("Enter the mail server and the email address"));
                }
                if !crate::settings::valid_host(&acct.host) {
                    return Err(anyhow!("That mail server name isn't valid"));
                }
                if !crate::settings::is_printable(&acct.user, 320) {
                    return Err(anyhow!("That email address isn't valid"));
                }
                if acct.password.is_empty() {
                    return Err(anyhow!("Enter the app password"));
                }
                let mut sess = watcher::connect(&acct)?;
                // a server with no Sent folder we can find is still a mailbox (replies will not close tasks by themselves): say `null`
                let sent = watcher::try_find_sent_folder(&mut sess)?;
                let _ = sess.logout();
                Ok(json!({"ok": true, "sent": sent, "gmail": is_gmail(&acct.host)}))
            }
            "save_account" => {
                // the first mailbox, or the one named by `id`: a new password, or a new server or address
                let s = self.settings();
                if s_arg(a, "password").replace(' ', "").is_empty() {
                    return Err(anyhow!("Enter the app password for this mailbox"));
                }
                let id = match a.get("id") {
                    None | Some(Value::Null) => 0,
                    Some(v) => u32::try_from(v.as_u64().unwrap_or(u64::MAX)).unwrap_or(u32::MAX),
                };
                let new = AccountCfg {
                    id,
                    host: s_arg(a, "host"),
                    port: port_arg(a)?,
                    user: s_arg(a, "user"),
                };
                if new.host.is_empty() || new.user.is_empty() {
                    return Err(anyhow!("Enter the mail server and the email address"));
                }
                let old = s.mailboxes().into_iter().find(|m| m.id == id);
                let mut candidate = s.clone();
                if id == 0 {
                    candidate.account = new.clone();
                } else {
                    let Some(slot) = candidate.more_accounts.iter_mut().find(|m| m.id == id) else {
                        return Err(anyhow!("There is no such mailbox"));
                    };
                    *slot = new.clone();
                }
                candidate.validate()?;
                // another server or address is another mailbox: this one's data is not the new one's. Only this mailbox's
                // data; every other mailbox is not part of this change.
                let changed = old.is_some_and(|o| {
                    (o.host.as_str(), o.user.as_str()) != (new.host.as_str(), new.user.as_str())
                });
                if changed {
                    // stop the syncs first: one that is mid-batch would write the old mailbox's mail back after the reset
                    self.stop
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .store(true, Ordering::Relaxed);
                }
                let saved = (|| -> Result<()> {
                    if changed {
                        self.store_for(id)?.reset_for_new_account()?;
                    }
                    secrets::set(
                        &secrets::imap_key(id),
                        &s_arg(a, "password").replace(' ', ""),
                    )?;
                    candidate.save(&self.dir)
                })();
                // the syncs hold the old password (or were stopped above): start them again, whatever happened
                if s.onboarded && (changed || saved.is_ok()) {
                    self.start();
                }
                saved?;
                Ok(json!(true))
            }
            "add_account" => {
                let mut s = self.settings();
                if !s.onboarded || s.mailboxes().is_empty() {
                    return Err(anyhow!("Set up your first mailbox before adding another"));
                }
                let password = s_arg(a, "password").replace(' ', "");
                if password.is_empty() {
                    return Err(anyhow!("Enter the app password for this mailbox"));
                }
                let used: std::collections::HashSet<u32> =
                    s.mailboxes().iter().map(|m| m.id).collect();
                // the lowest free number; 0 is the first mailbox's place, free again after it was removed
                let Some(id) = (0..crate::store::MAX_MAILBOXES).find(|n| !used.contains(n)) else {
                    return Err(anyhow!("You can read at most five mailboxes at once"));
                };
                let new = AccountCfg {
                    id,
                    host: s_arg(a, "host"),
                    port: port_arg(a)?,
                    user: s_arg(a, "user"),
                };
                if new.host.trim().is_empty() || new.user.trim().is_empty() {
                    return Err(anyhow!(
                        "Enter the mail server and the email address of this mailbox"
                    ));
                }
                if id == 0 {
                    s.account = new;
                } else {
                    s.more_accounts.push(new);
                }
                s.validate()?;
                // a new mailbox starts from nothing: a sync of an earlier mailbox with this number may have been
                // mid-message when that mailbox was removed, and can have left a cursor or a task behind
                self.store()?.clear_mailbox(id)?;
                secrets::set(&secrets::imap_key(id), &password)?;
                s.save(&self.dir)?;
                self.start();
                Ok(json!(id))
            }
            "remove_account" => {
                let mut s = self.settings();
                let id = u32::try_from(a["id"].as_u64().unwrap_or(u64::MAX)).unwrap_or(u32::MAX);
                if !s.mailboxes().iter().any(|m| m.id == id) {
                    return Err(anyhow!("There is no such mailbox"));
                }
                // stop the sync first, so nothing writes this mailbox's mail back in while it is removed
                self.stop
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .store(true, Ordering::Relaxed);
                let removed = (|| -> Result<()> {
                    self.store()?.remove_mailbox(id)?;
                    secrets::delete(&secrets::imap_key(id));
                    if id == 0 {
                        s.account = AccountCfg::default();
                    } else {
                        s.more_accounts.retain(|m| m.id != id);
                    }
                    if s.mailboxes().is_empty() {
                        s.onboarded = false; // nothing left to read: setup is asked for again
                    }
                    s.save(&self.dir)
                })();
                self.start(); // the syncs were stopped above: start them again even if the removal failed halfway
                removed?;
                Ok(json!(true))
            }
            "backup_export" => {
                // a file in Downloads (or your user folder), never over another one
                let dir = crate::calendar::downloads_dir()
                    .ok_or_else(|| anyhow!("The app could not find your Downloads folder"))?;
                let out = crate::backup::export_in(self, &dir)?;
                Ok(json!({"path": out.path.to_string_lossy(), "counts": out.counts}))
            }
            "backup_import" => {
                // `apply` false only reports what would happen; `settings` also restores options and mailboxes
                let report = crate::backup::import(
                    self,
                    a["json"].as_str().unwrap_or(""),
                    a["apply"].as_bool().unwrap_or(false),
                    a["settings"].as_bool().unwrap_or(false),
                )?;
                Ok(serde_json::to_value(report)?)
            }
            "save_settings" => {
                // The account (server, user, password, and the data tied to them) changes only through
                // save_account, which resets the old mailbox's data, and onboarding only through finish_setup.
                // A patch must be an object: any other shape replaces every setting at once (serde reads a list
                // in field order), which would slip around both.
                let patch = &a["patch"];
                if !patch.is_object() {
                    return Err(anyhow!("A settings change must be an object"));
                }
                let now = self.settings();
                let mut s = now.patched(patch)?;
                if serde_json::to_value(&s.account)? != serde_json::to_value(&now.account)?
                    || serde_json::to_value(&s.more_accounts)?
                        != serde_json::to_value(&now.more_accounts)?
                    || s.onboarded != now.onboarded
                {
                    return Err(anyhow!(
                        "The account is changed with save_account, not with settings"
                    ));
                }
                // the phone hub's access code is made by the app (or by `rotate_hub_token`), never chosen by a request
                if s.hub.token != now.hub.token {
                    return Err(anyhow!(
                        "The access code is made by the app: use Rotate access code"
                    ));
                }
                s.validate()?;
                if !s.hub.enabled {
                    s.hub.token.clear(); // a fresh access code every time the hub is switched on
                } else if s.hub.token.is_empty() {
                    s.hub.token = new_token();
                }
                s.save(&self.dir)?;
                self.start();
                Ok(serde_json::to_value(&s)?)
            }
            "rotate_hub_token" => {
                let mut s = self.settings();
                s.hub.token = new_token();
                s.save(&self.dir)?;
                self.start();
                Ok(json!(true))
            }
            "set_secret" => {
                if s_arg(a, "name") != "anthropic-key" {
                    return Err(anyhow!("unknown secret"));
                }
                let v = s_arg(a, "value");
                if v.is_empty() {
                    secrets::delete("anthropic-key")
                } else {
                    secrets::set("anthropic-key", &v)?
                }
                Ok(json!(true))
            }
            "detect_ollama" => {
                let url = if s_arg(a, "url").is_empty() {
                    self.settings().ai.ollama_url
                } else {
                    s_arg(a, "url")
                };
                if !crate::settings::valid_http_url(&url) {
                    return Err(anyhow!("Ollama address must be an http(s) address"));
                }
                Ok(json!(ai::ollama_models(&url)))
            }
            "test_ai" => {
                let s = self.settings();
                let key = secrets::get("anthropic-key");
                // With no model name or no key `make_triager` quietly gives the rules, which never fail: the test would say it works
                // for a model that was never chosen.
                if s.ai.mode == "ollama" && s.ai.ollama_model.trim().is_empty() {
                    return Err(anyhow!("No model is chosen"));
                }
                if s.ai.mode == "anthropic" && key.is_none() {
                    return Err(anyhow!("No API key is saved"));
                }
                let t = make_triager(&s.ai, key, vec![]);
                let sample = crate::mail::parse(
                    0,
                    b"From: Priya <priya@example.com>\r\nSubject: Contract needs your signature today\r\n\r\nCan you sign by 5pm? Legal has cleared it.\r\n",
                )?;
                let r = t.triage(&sample, &Default::default())?;
                Ok(
                    json!({"priority": r.priority, "category": r.category, "action": r.action, "why": r.why, "needs_reply": r.needs_reply, "due": r.due}),
                )
            }
            "finish_setup" => {
                let mut s = self.settings();
                // setup is finished when the first mailbox is there, with its password
                if s.account.host.is_empty()
                    || s.account.user.is_empty()
                    || secrets::lookup(&secrets::imap_key(0))?.is_none()
                {
                    return Err(anyhow!("Add your first mailbox before finishing setup"));
                }
                s.onboarded = true;
                s.save(&self.dir)?;
                self.start();
                Ok(json!(true))
            }
            "sign_out" => {
                // stop the background threads first so nothing writes mail back in while the data is wiped
                self.stop
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .store(true, Ordering::Relaxed);
                let signed_out = (|| -> Result<()> {
                    self.store()?.wipe_all()?;
                    crate::store::remove_leftover_files(&self.dir);
                    secrets::delete_all();
                    let mut s = self.settings();
                    s.account = AccountCfg::default();
                    s.more_accounts.clear();
                    s.onboarded = false;
                    s.hub.enabled = false;
                    s.hub.token.clear();
                    s.save(&self.dir)
                })();
                self.start(); // the threads were stopped above: start them again even if this failed halfway
                signed_out?;
                Ok(json!(true))
            }
            "hub_info" => {
                let s = self.settings();
                let ip = local_ip_address::local_ip()
                    .map(|i| i.to_string())
                    .unwrap_or_else(|_| "127.0.0.1".into());
                let url = format!("http://{ip}:{}/?token={}", s.hub.port, s.hub.token);
                let svg = qrcode::QrCode::new(url.as_bytes())?
                    .render::<qrcode::render::svg::Color>()
                    .min_dimensions(220, 220)
                    .quiet_zone(true)
                    .build();
                Ok(json!({"url": url, "svg": svg, "enabled": s.hub.enabled}))
            }
            _ => Err(anyhow!("unknown command: {cmd}")),
        }
    }
}

/// How many finished tasks the window and the phone are sent (the newest). The rest are kept, searchable, and pruned as before.
pub const DONE_SHOWN: usize = 200;
/// The most finished tasks one answer holds, however many the window asks for ("Show older" on the Done tab asks for more).
pub const DONE_MAX: usize = 20_000;

/// How many finished tasks the window asks for: at least the default, at most `DONE_MAX`; anything else is the default.
fn done_asked(args: &Value) -> usize {
    args["done"].as_u64().map_or(DONE_SHOWN, |n| {
        (n.min(DONE_MAX as u64) as usize).max(DONE_SHOWN)
    })
}

/// A log line as the phone may see it: the login a mailbox connected as is the mailbox's address, which stays on the computer.
fn for_the_phone(msg: &str) -> String {
    match msg.strip_prefix("connected as ") {
        Some(rest) => format!(
            "connected;{}",
            rest.split_once(';').map_or("", |(_, after)| after)
        ),
        None => msg.to_string(),
    }
}

/// The status to show for several mailboxes, in this order: an error of any of them, then "no password", then "connecting",
/// then "syncing", then any other status that is not "idle", else the first one. No mailbox at all is "setup".
pub fn worst_status(all: &[String]) -> String {
    let find = |f: &dyn Fn(&str) -> bool| all.iter().find(|s| f(s)).cloned();
    find(&|s| s.starts_with("error"))
        .or_else(|| find(&|s| s == "no password"))
        .or_else(|| find(&|s| s == "connecting"))
        .or_else(|| find(&|s| s == "syncing"))
        .or_else(|| find(&|s| s != "idle"))
        .or_else(|| all.first().cloned())
        .unwrap_or_else(|| "setup".to_string())
}

/// A random access code of 32 letters and digits, used for the phone hub.
pub fn new_token() -> String {
    use rand::RngExt;
    const A: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let mut r = rand::rng();
    (0..32)
        .map(|_| A[r.random_range(0..A.len())] as char)
        .collect()
}
