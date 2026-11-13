//! Application core: owns settings, the background threads and the command interface (`rpc`)
//! shared by the desktop window (Tauri) and the phone hub (HTTP).
use crate::ai::{self, make_triager};
use crate::mail::Email;
use crate::settings::{is_gmail, AccountCfg, Settings};
use crate::store::Store;
use crate::triage::Triage;
use crate::watcher::{self, Account};
use crate::{commands, hub, jobs, secrets, writeback};
use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Whether the database integrity check has already passed in this launch.
static HEALTH_CHECKED: AtomicBool = AtomicBool::new(false);

pub type Notify = Arc<dyn Fn(&str, &str) + Send + Sync>;

pub struct Core {
    pub dir: PathBuf,
    notify: Notify,
    status: Arc<Mutex<String>>,
    stop: Mutex<Arc<AtomicBool>>,
}

fn s_arg(a: &Value, k: &str) -> String {
    a[k].as_str().unwrap_or("").trim().to_string()
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
    pub fn new(dir: PathBuf, notify: Notify) -> Arc<Self> {
        let _ = std::fs::create_dir_all(&dir);
        Arc::new(Self {
            dir,
            notify,
            status: Arc::new(Mutex::new("starting".into())),
            stop: Mutex::new(Arc::new(AtomicBool::new(true))),
        })
    }

    pub fn settings(&self) -> Settings {
        Settings::load(&self.dir)
    }

    pub fn store(&self) -> Result<Store> {
        Store::open(&self.dir.join("agent.db"))
    }

    /// The sync status as shown in the app (for the support report).
    pub fn status_text(&self) -> String {
        self.status
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    fn set_status(&self, s: &str) {
        *self.status.lock().unwrap_or_else(|e| e.into_inner()) = s.to_string();
    }

    /// `Ok(None)`: not set up, or no saved password. `Err(status)`: the system keychain could not be read.
    fn account(&self, s: &Settings) -> std::result::Result<Option<Account>, String> {
        if !s.onboarded || s.account.host.is_empty() {
            return Ok(None);
        }
        match secrets::lookup("imap-password") {
            Ok(Some(password)) => Ok(Some(Account {
                host: s.account.host.clone(),
                port: s.account.port,
                user: s.account.user.clone(),
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

        // A damaged database is set aside and rebuilt; one from a newer app version is refused, untouched. The check
        // runs once per launch (start() also runs after every settings save). A busy database is retried by the
        // sync itself, so only the "newer version" case stops the app here.
        if !HEALTH_CHECKED.swap(true, Ordering::Relaxed) {
            if let Err(e) = Store::open_recovering(&self.dir.join("agent.db")) {
                HEALTH_CHECKED.store(false, Ordering::Relaxed); // not checked after all: try again on the next start
                self.set_status(&format!("error: {e}"));
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
        let acct = match self.account(&s) {
            Ok(Some(a)) => a,
            Ok(None) => {
                self.set_status(if s.onboarded { "no password" } else { "setup" });
                return;
            }
            Err(message) => {
                self.set_status(&message);
                return;
            }
        };

        // watcher
        {
            let (core, stop, acct) = (self.clone(), new_stop.clone(), acct.clone());
            std::thread::spawn(move || {
                let Ok(store) = core.store() else { return };
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
                let status = move |x: &str| {
                    if x == "idle" {
                        live_w.store(true, Ordering::Relaxed);
                    }
                    st.set_status(x);
                };
                let core2 = core.clone();
                watcher::run(
                    &acct,
                    &store,
                    triager.as_ref(),
                    s.first_run_limit,
                    &stop,
                    &status,
                    &mut |e: &Email, t: &Triage| {
                        // never alert for the very first backfill, only for genuinely new mail
                        core2.on_triaged(e, t, !initial || live.load(Ordering::Relaxed));
                    },
                );
            });
        }
        // write-back
        {
            let (core, stop) = (self.clone(), new_stop);
            std::thread::spawn(move || {
                let Ok(store) = core.store() else { return };
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
    fn on_triaged(self: &Arc<Self>, e: &Email, t: &Triage, alert: bool) {
        if alert {
            self.alert(e, t);
        }
        let s = self.settings();
        let Ok(store) = self.store() else { return };
        if s.writeback && is_gmail(&s.account.host) {
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
                let _ = store.queue(e.uid, "category", &format!("AI/{label}"));
            }
        }
        if let Some(task) = store.get(e.uid) {
            commands::auto_draft(self, task);
        }
    }

    fn state(&self, public: bool) -> Result<Value> {
        let store = self.store()?;
        let s = self.settings();
        // The phone hub only needs to know whether write-back is on; it never sees account or alert details.
        let settings = if public {
            json!({"onboarded": s.onboarded, "writeback": s.writeback})
        } else {
            serde_json::to_value(&s)?
        };
        Ok(json!({
            "settings": settings,
            "tasks": store.all_tasks()?,
            "log": store.recent_log(100).into_iter().map(|(at, msg)| json!({"at": at, "msg": msg})).collect::<Vec<_>>(),
            "status": self.status.lock().unwrap_or_else(|e| e.into_inner()).clone(),
            "pending": store.pending(1000).len(),
            "has_anthropic_key": !public && secrets::get("anthropic-key").is_some(),
            "gmail": is_gmail(&s.account.host),
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
        }))
    }

    /// Commands reachable from the phone hub (everything else is desktop-only).
    pub fn rpc_public(self: &Arc<Self>, cmd: &str, args: &Value) -> Result<Value> {
        match cmd {
            "state" => self.state(true),
            c if commands::PUBLIC.contains(&c) => self.rpc(cmd, args),
            _ => Err(anyhow!("not available from the hub")),
        }
    }

    pub fn rpc(self: &Arc<Self>, cmd: &str, a: &Value) -> Result<Value> {
        if let Some(r) = commands::run(self, cmd, a) {
            return r;
        }
        match cmd {
            "state" => self.state(false),
            "test_account" => {
                let acct = Account {
                    host: s_arg(a, "host"),
                    port: a["port"].as_u64().unwrap_or(993) as u16,
                    user: s_arg(a, "user"),
                    password: s_arg(a, "password").replace(' ', ""),
                };
                let mut sess = watcher::connect(&acct)?;
                let sent = watcher::find_sent_folder(&mut sess)?;
                let _ = sess.logout();
                Ok(json!({"ok": true, "sent": sent, "gmail": is_gmail(&acct.host)}))
            }
            "save_account" => {
                let mut s = self.settings();
                let new = AccountCfg {
                    host: s_arg(a, "host"),
                    port: a["port"].as_u64().unwrap_or(993) as u16,
                    user: s_arg(a, "user"),
                };
                let mut candidate = s.clone();
                candidate.account = new.clone();
                candidate.validate()?;
                if (new.host.as_str(), new.user.as_str())
                    != (s.account.host.as_str(), s.account.user.as_str())
                {
                    self.store()?.reset_for_new_account()?;
                }
                secrets::set("imap-password", &s_arg(a, "password").replace(' ', ""))?;
                s.account = new;
                s.save(&self.dir)?;
                Ok(json!(true))
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
                    || s.onboarded != now.onboarded
                {
                    return Err(anyhow!(
                        "The account is changed with save_account, not with settings"
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
                Ok(json!(ai::ollama_models(&url)))
            }
            "test_ai" => {
                let s = self.settings();
                let t = make_triager(&s.ai, secrets::get("anthropic-key"), vec![]);
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
                self.store()?.wipe_all()?;
                crate::store::remove_leftover_files(&self.dir);
                secrets::delete_all();
                let mut s = self.settings();
                s.account = AccountCfg::default();
                s.onboarded = false;
                s.hub.enabled = false;
                s.hub.token.clear();
                s.save(&self.dir)?;
                self.start();
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

pub fn new_token() -> String {
    use rand::Rng;
    const A: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let mut r = rand::thread_rng();
    (0..32)
        .map(|_| A[r.gen_range(0..A.len())] as char)
        .collect()
}
