//! User settings (JSON in the app data dir). Secrets are NOT stored here; see `secrets`.
use serde::{Deserialize, Serialize};
use std::path::Path;

/// One mailbox: its IMAP server, port and address. The password is never kept here.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct AccountCfg {
    /// The mailbox's number: 0 for the first (`Settings::account`), 1 to 4 for `more_accounts`. It is part of every task
    /// id the mailbox makes, so it never changes while the mailbox exists.
    pub id: u32,
    pub host: String,
    pub port: u16,
    pub user: String,
}
impl Default for AccountCfg {
    fn default() -> Self {
        Self {
            id: 0,
            host: String::new(),
            port: 993,
            user: String::new(),
        }
    }
}

/// Which AI helps with triage and drafts ("rules" means none) and where to find the model. A key for the API mode is kept in the keychain, not here.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct AiCfg {
    /// "rules" | "ollama" | "anthropic"
    pub mode: String,
    pub ollama_url: String,
    pub ollama_model: String,
    pub anthropic_model: String,
}
impl Default for AiCfg {
    fn default() -> Self {
        Self {
            mode: "rules".into(),
            ollama_url: "http://127.0.0.1:11434".into(),
            ollama_model: String::new(),
            anthropic_model: "claude-haiku-5-5".into(),
        }
    }
}

/// How and when to alert: desktop notifications, the ntfy server and topic for phone alerts, quiet hours, the daily digest time and which categories alert.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct AlertCfg {
    pub desktop: bool,
    pub ntfy_server: String,
    pub ntfy_topic: String,
    /// Local time "HH:MM". Both set = no alerts in that window (urgent mail still gets through). Empty = off.
    pub quiet_start: String,
    pub quiet_end: String,
    /// Local time "HH:MM" for a once-a-day summary notification. Empty = off.
    pub digest_time: String,
    /// Only these categories raise alerts (urgent mail always does). Empty = every category.
    pub categories: Vec<String>,
}
impl Default for AlertCfg {
    fn default() -> Self {
        Self {
            desktop: true,
            ntfy_server: "https://ntfy.sh".into(),
            ntfy_topic: String::new(),
            quiet_start: String::new(),
            quiet_end: String::new(),
            digest_time: String::new(),
            categories: vec![],
        }
    }
}

/// How reply drafts are written: whether they are made automatically, the tone, the signature and the limits on automatic drafting.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct DraftCfg {
    /// Write a draft automatically for mail that needs a reply (needs an AI model; never sent).
    pub auto: bool,
    /// "friendly" | "concise" | "formal"
    pub tone: String,
    pub signature: String,
    /// Only auto-draft for messages at or above this priority.
    pub min_priority: u8,
    /// At most this many AI drafts per 24 hours.
    pub daily_cap: u32,
}
impl Default for DraftCfg {
    fn default() -> Self {
        Self {
            auto: false,
            tone: "friendly".into(),
            signature: String::new(),
            min_priority: 3,
            daily_cap: 25,
        }
    }
}

/// The phone hub: whether it runs, the port it listens on (1024 or higher) and its access code, which is made fresh each time the hub is switched on.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct HubCfg {
    pub enabled: bool,
    pub port: u16,
    pub token: String,
}
impl Default for HubCfg {
    fn default() -> Self {
        Self {
            enabled: false,
            port: 8787,
            token: String::new(),
        }
    }
}

/// Whether the app checks for a newer version of itself.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct UpdateCfg {
    /// Once a day, ask GitHub whether a newer version exists. Only has an effect once the project names its repository.
    pub check: bool,
}
impl Default for UpdateCfg {
    fn default() -> Self {
        Self { check: true }
    }
}

/// How long to wait for an answer to mail you sent before a follow-up task is made.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct FollowCfg {
    /// Make a task for a conversation you started when nobody has answered after this many days: 2, 3, 5 or 7.
    /// 0 turns the feature off.
    pub days: u32,
}

/// Everything the user sets, saved as `settings.json` in the app data folder: mailboxes, AI, alerts, drafts, phone hub, updates and follow-ups.
/// Passwords and keys are not in it.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Settings {
    pub onboarded: bool,
    /// The first mailbox (number 0).
    pub account: AccountCfg,
    /// Up to four more, numbered 1 to 4. Their passwords are in the keychain, never here.
    pub more_accounts: Vec<AccountCfg>,
    pub ai: AiCfg,
    pub alerts: AlertCfg,
    pub hub: HubCfg,
    pub drafts: DraftCfg,
    pub updates: UpdateCfg,
    pub followups: FollowCfg,
    /// Apply done/star/priority changes to the real mailbox. Off until the user opts in.
    pub writeback: bool,
    pub first_run_limit: usize,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            onboarded: false,
            account: AccountCfg::default(),
            more_accounts: vec![],
            ai: AiCfg::default(),
            alerts: AlertCfg::default(),
            hub: HubCfg::default(),
            drafts: DraftCfg::default(),
            updates: UpdateCfg::default(),
            followups: FollowCfg::default(),
            writeback: false,
            first_run_limit: 50,
        }
    }
}

impl Settings {
    /// Every mailbox that is set up, the first one (when it has a server) and then the others, each with its number.
    pub fn mailboxes(&self) -> Vec<AccountCfg> {
        let mut all = vec![];
        if !self.account.host.is_empty() {
            all.push(AccountCfg {
                id: 0,
                ..self.account.clone()
            });
        }
        all.extend(self.more_accounts.iter().cloned());
        all
    }

    /// The settings saved in `dir`. A missing or unreadable file gives the defaults; this never fails. A ranking mode that is
    /// not one of the three (a hand-edited file) reads as "rules": `validate` refuses it, and would then refuse every later save.
    pub fn load(dir: &Path) -> Self {
        let mut s: Self = std::fs::read_to_string(dir.join("settings.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        if !valid_mode(&s.ai.mode) {
            s.ai.mode = "rules".into();
        }
        s
    }

    /// Writes the settings to `dir`, through a temporary file and a rename so a crash cannot leave half a file.
    /// It does not call `validate`.
    pub fn save(&self, dir: &Path) -> anyhow::Result<()> {
        std::fs::create_dir_all(dir)?;
        let tmp = dir.join("settings.json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(tmp, dir.join("settings.json"))?;
        Ok(())
    }

    /// Merge a JSON patch (only known fields survive deserialisation).
    pub fn patched(&self, patch: &serde_json::Value) -> anyhow::Result<Self> {
        let mut base = serde_json::to_value(self)?;
        merge(&mut base, patch);
        Ok(serde_json::from_value(base)?)
    }
}

fn merge(a: &mut serde_json::Value, b: &serde_json::Value) {
    match (a, b) {
        (serde_json::Value::Object(x), serde_json::Value::Object(y)) => {
            for (k, v) in y {
                merge(x.entry(k.clone()).or_insert(serde_json::Value::Null), v);
            }
        }
        (a, b) => *a = b.clone(),
    }
}

/// Gmail's own hosts: the name itself or a sub-domain of it. A look-alike such as `evilgmail.com` or
/// `imap.gmail.com.evil.example` is not Gmail.
pub fn is_gmail(host: &str) -> bool {
    let h = host.trim().trim_end_matches('.').to_lowercase();
    ["gmail.com", "googlemail.com"]
        .iter()
        .any(|d| h == *d || h.ends_with(&format!(".{d}")))
}

/// Is `s` at most `max` bytes with no control character? (A mailbox's address, a topic: what is shown or sent as one line.)
pub(crate) fn is_printable(s: &str, max: usize) -> bool {
    s.len() <= max && !s.chars().any(|c| c.is_control())
}

/// The ranking modes there are: the built-in rules, a local Ollama, or Anthropic's API.
fn valid_mode(m: &str) -> bool {
    matches!(m, "rules" | "ollama" | "anthropic")
}

pub(crate) fn valid_host(h: &str) -> bool {
    h.len() <= 253
        && h.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
}

pub(crate) fn valid_http_url(u: &str) -> bool {
    (u.starts_with("http://") || u.starts_with("https://"))
        && u.len() <= 200
        && !u.chars().any(|c| c.is_control() || c.is_whitespace())
}

fn valid_topic(t: &str) -> bool {
    t.len() <= 64
        && t.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn valid_clock(t: &str) -> bool {
    t.is_empty()
        || (t.len() == 5
            && t.as_bytes()[2] == b':'
            && t[..2].parse::<u32>().is_ok_and(|h| h < 24)
            && t[3..].parse::<u32>().is_ok_and(|m| m < 60))
}

fn valid_model(m: &str) -> bool {
    is_printable(m, 100) && !m.chars().any(char::is_whitespace)
}

/// Which mailbox this is: its server and address in a normal form. The same server written another way (letter case, a
/// trailing dot, Google's other name for it) is the same mailbox.
pub fn mailbox_key(a: &AccountCfg) -> (String, String) {
    let host = a.host.trim().trim_end_matches('.').to_lowercase();
    let host = match host.strip_suffix("googlemail.com") {
        Some(front) => format!("{front}gmail.com"),
        None => host,
    };
    (host, a.user.trim().to_lowercase())
}

impl AccountCfg {
    /// A mailbox that names a real server and address, as every mailbox after the first must.
    pub fn check(&self) -> anyhow::Result<()> {
        use anyhow::bail;
        if self.host.is_empty() || !valid_host(&self.host) {
            bail!("That mail server name isn't valid");
        }
        if self.port == 0 {
            bail!("That mail server port isn't valid");
        }
        if self.user.trim().is_empty() || !is_printable(&self.user, 320) {
            bail!("That email address isn't valid");
        }
        Ok(())
    }
}

impl Settings {
    /// The extra mailboxes: at most four, numbers 1 to 4 each used once, a real server and user each, and no mailbox (server
    /// and user, ignoring case) twice, the first one included.
    fn validate_mailboxes(&self) -> anyhow::Result<()> {
        use anyhow::bail;
        if self.more_accounts.len() > (crate::store::MAX_MAILBOXES - 1) as usize {
            bail!("At most five mailboxes can be read at once");
        }
        let key = mailbox_key;
        let mut seen_pairs = std::collections::HashSet::new();
        if !self.account.host.is_empty() {
            seen_pairs.insert(key(&self.account));
        }
        let mut seen_ids = std::collections::HashSet::new();
        for a in &self.more_accounts {
            if !(1..crate::store::MAX_MAILBOXES).contains(&a.id) || !seen_ids.insert(a.id) {
                bail!("Every extra mailbox needs its own number from 1 to 4");
            }
            a.check()?;
            if !seen_pairs.insert(key(a)) {
                bail!("That mailbox is already in the app");
            }
        }
        Ok(())
    }

    /// Rejects values that could inject into URLs, headers or IMAP commands. Called before every save.
    pub fn validate(&self) -> anyhow::Result<()> {
        use anyhow::bail;
        let a = &self.account;
        if !a.host.is_empty() && !valid_host(&a.host) {
            bail!("That mail server name isn't valid");
        }
        if a.port == 0 {
            bail!("That mail server port isn't valid");
        }
        if !is_printable(&a.user, 320) {
            bail!("That email address isn't valid");
        }
        if !valid_topic(&self.alerts.ntfy_topic) {
            bail!("Alert topic can only use letters, digits, - and _");
        }
        if !valid_http_url(&self.alerts.ntfy_server) {
            bail!("Alert server must be an http(s) address");
        }
        if !valid_mode(&self.ai.mode) {
            bail!("Ranking mode must be rules, ollama or anthropic");
        }
        if !valid_http_url(&self.ai.ollama_url) {
            bail!("Ollama address must be an http(s) address");
        }
        if !valid_model(&self.ai.ollama_model) || !valid_model(&self.ai.anthropic_model) {
            bail!("Model name isn't valid");
        }
        self.validate_mailboxes()?;
        let al = &self.alerts;
        if ![&al.quiet_start, &al.quiet_end, &al.digest_time]
            .iter()
            .all(|t| valid_clock(t))
            || al.quiet_start.is_empty() != al.quiet_end.is_empty()
        {
            bail!("Times must look like 22:30, and quiet hours need both a start and an end");
        }
        if al.categories.len() > 8
            || al
                .categories
                .iter()
                .any(|c| !crate::ai::CATEGORIES.contains(&c.as_str()))
        {
            bail!("Unknown alert category");
        }
        let d = &self.drafts;
        if !matches!(d.tone.as_str(), "friendly" | "concise" | "formal") {
            bail!("Draft tone must be friendly, concise or formal");
        }
        if d.signature.chars().count() > 500
            || d.signature.chars().any(|c| c.is_control() && c != '\n')
        {
            bail!("Signature is too long or contains control characters");
        }
        if !(1..=5).contains(&d.min_priority) || d.daily_cap > 200 {
            bail!("Draft limits are out of range");
        }
        if !crate::followup::valid_days(self.followups.days) {
            bail!("Follow-up reminders can wait 2, 3, 5 or 7 days, or be off");
        }
        if self.hub.port < 1024 {
            bail!("Phone hub port must be 1024 or higher");
        }
        if self.first_run_limit == 0 || self.first_run_limit > 1000 {
            bail!("First-run limit must be between 1 and 1000");
        }
        Ok(())
    }
}
