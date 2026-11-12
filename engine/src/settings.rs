//! User settings (JSON in the app data dir). Secrets are NOT stored here; see `secrets`.
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct AccountCfg {
    pub host: String,
    pub port: u16,
    pub user: String,
}
impl Default for AccountCfg {
    fn default() -> Self {
        Self {
            host: String::new(),
            port: 993,
            user: String::new(),
        }
    }
}

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
        }
    }
}

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

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Settings {
    pub onboarded: bool,
    pub account: AccountCfg,
    pub ai: AiCfg,
    pub alerts: AlertCfg,
    pub hub: HubCfg,
    pub drafts: DraftCfg,
    /// Apply done/star/priority changes to the real mailbox. Off until the user opts in.
    pub writeback: bool,
    pub first_run_limit: usize,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            onboarded: false,
            account: AccountCfg::default(),
            ai: AiCfg::default(),
            alerts: AlertCfg::default(),
            hub: HubCfg::default(),
            drafts: DraftCfg::default(),
            writeback: false,
            first_run_limit: 50,
        }
    }
}

impl Settings {
    pub fn load(dir: &Path) -> Self {
        std::fs::read_to_string(dir.join("settings.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

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

pub fn is_gmail(host: &str) -> bool {
    let h = host.to_lowercase();
    h.ends_with("gmail.com") || h.ends_with("googlemail.com")
}

fn is_printable(s: &str, max: usize) -> bool {
    s.len() <= max && !s.chars().any(|c| c.is_control())
}

fn valid_host(h: &str) -> bool {
    h.len() <= 253
        && h.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
}

fn valid_http_url(u: &str) -> bool {
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

impl Settings {
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
        if !valid_http_url(&self.ai.ollama_url) {
            bail!("Ollama address must be an http(s) address");
        }
        if !valid_model(&self.ai.ollama_model) || !valid_model(&self.ai.anthropic_model) {
            bail!("Model name isn't valid");
        }
        let al = &self.alerts;
        if ![&al.quiet_start, &al.quiet_end, &al.digest_time]
            .iter()
            .all(|t| valid_clock(t))
            || al.quiet_start.is_empty() != al.quiet_end.is_empty()
        {
            bail!("Times must look like 22:30, and quiet hours need both a start and an end");
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
        if self.hub.port < 1024 {
            bail!("Phone hub port must be 1024 or higher");
        }
        if self.first_run_limit == 0 || self.first_run_limit > 1000 {
            bail!("First-run limit must be between 1 and 1000");
        }
        Ok(())
    }
}
