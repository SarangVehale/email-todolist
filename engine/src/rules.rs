//! Rules the user writes: "if the sender / domain / subject contains this, then set the priority, set the category, or
//! file it". Plain case-insensitive text only: there are no regular expressions, so no rule can be made to run for a
//! long time. The first rule that matches is the one used. Where rules sit in ranking is described in
//! docs/designs/phase4.md.
use serde::{Deserialize, Serialize};

/// The most rules the user may have; `validate` refuses a longer list.
pub const MAX_RULES: usize = 50;
/// The most characters in a rule's match text (and at least one is needed).
pub const MAX_TEXT: usize = 100;

/// One user rule: text to look for in the sender, domain or subject, and what to do on a match (set a priority, set a category, or mark done).
/// Matching is plain case-insensitive text; the first rule that matches is the one used.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Rule {
    /// Chosen by the caller; only used to tell rules apart, so it must be unique.
    pub id: String,
    /// "sender" (the address is exactly this), "domain" (that domain or a sub-domain) or "subject" (contains).
    /// A display name is never matched: anyone can write any name, and a look-alike address is free to register.
    pub field: String,
    pub text: String,
    /// Set the priority to 1-5.
    pub priority: Option<u8>,
    /// Set the category to one of the app's categories.
    pub category: Option<String>,
    /// Mark the task done as soon as it arrives. Cannot be combined with a priority.
    pub file: bool,
}

impl Default for Rule {
    fn default() -> Self {
        Self {
            id: String::new(),
            field: "sender".into(),
            text: String::new(),
            priority: None,
            category: None,
            file: false,
        }
    }
}

fn valid_id(id: &str) -> bool {
    (1..=32).contains(&id.len())
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Refuses a rule set that is too big, has a rule with nothing to do, or has text that could cause trouble.
pub fn validate(rules: &[Rule]) -> anyhow::Result<()> {
    use anyhow::bail;
    if rules.len() > MAX_RULES {
        bail!("At most {MAX_RULES} rules are allowed");
    }
    let mut seen = std::collections::HashSet::new();
    for r in rules {
        if !valid_id(&r.id) || !seen.insert(r.id.as_str()) {
            bail!("Every rule needs its own short id");
        }
        if !matches!(r.field.as_str(), "sender" | "domain" | "subject") {
            bail!("A rule looks at the sender, the domain or the subject");
        }
        let t = r.text.trim();
        if t.is_empty()
            || r.text.chars().count() > MAX_TEXT
            || r.text.chars().any(|c| c.is_control())
        {
            bail!("A rule needs text of 1 to {MAX_TEXT} characters");
        }
        if r.priority.is_some_and(|p| !(1..=5).contains(&p)) {
            bail!("A rule's priority is from 1 to 5");
        }
        if r.category
            .as_deref()
            .is_some_and(|c| !crate::ai::CATEGORIES.contains(&c))
        {
            bail!("Unknown category in a rule");
        }
        if r.priority.is_none() && r.category.is_none() && !r.file {
            bail!("A rule has to do something: set a priority, set a category, or file the mail");
        }
        if r.file && r.priority.is_some() {
            bail!("Filing mail and setting its priority don't go together");
        }
    }
    Ok(())
}

/// Does this rule match a message from `addr` with `subject`? An empty rule text matches nothing.
pub fn matches(rule: &Rule, addr: &str, subject: &str) -> bool {
    let needle = rule.text.trim().to_lowercase();
    if needle.is_empty() {
        return false;
    }
    let addr = addr.to_lowercase();
    match rule.field.as_str() {
        "sender" => addr == needle,
        "domain" => {
            let domain = needle.trim_start_matches('@');
            let Some((_, host)) = addr.rsplit_once('@') else {
                return false;
            };
            !domain.is_empty() && (host == domain || host.ends_with(&format!(".{domain}")))
        }
        "subject" => subject.to_lowercase().contains(&needle),
        _ => false,
    }
}

/// The first rule in the list that matches, if any.
pub fn first_match<'a>(rules: &'a [Rule], addr: &str, subject: &str) -> Option<&'a Rule> {
    rules.iter().find(|r| matches(r, addr, subject))
}

/// May this rule RAISE a message's priority? Never for mail that failed its sender check (a forged From address must
/// not use an "this address matters" rule), and a rule on the subject only for someone you write to: anyone can put
/// any words in a subject, so a stranger must not be able to climb by choosing them.
pub fn may_raise(rule: &Rule, failed_sender_check: bool, known_sender: bool) -> bool {
    !failed_sender_check && (rule.field != "subject" || known_sender)
}
