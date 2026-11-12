//! Pluggable triage. `RulesTriager` is the always-available default; AI providers
//! (local model, API key, ...) implement the same trait and fall back to rules on error.
use crate::mail::Email;
use crate::signals::{base_score, Signals};

#[derive(Debug, Clone, PartialEq)]
pub struct Triage {
    pub priority: u8,
    pub category: String,
    pub action: String,
    pub urgent: bool,
    pub why: String,
    /// The sender is waiting for an answer from the user.
    pub needs_reply: bool,
    /// ISO date (YYYY-MM-DD) the message says something is due, or empty.
    pub due: String,
}

/// What a reply draft should sound like. `instruction` is a short user request such as "decline politely".
#[derive(Debug, Clone, Default)]
pub struct DraftCtx {
    pub tone: String,
    pub signature: String,
    pub instruction: String,
}

pub trait Triager: Send {
    fn triage(&self, email: &Email, sig: &Signals) -> anyhow::Result<Triage>;

    /// Writes the body of a reply. Providers without a model return an error and the caller uses a template.
    fn draft(&self, _email: &Email, _ctx: &DraftCtx) -> anyhow::Result<String> {
        anyhow::bail!("no AI model is configured")
    }
}

pub struct RulesTriager;

const CATEGORIES: &[(&str, &[&str])] = &[
    (
        "finance",
        &[
            "invoice",
            "statement",
            "payment",
            "receipt",
            "bank",
            "bill",
            "tax",
            "refund",
            "transaction",
        ],
    ),
    (
        "travel",
        &[
            "flight",
            "booking",
            "itinerary",
            "boarding",
            "reservation",
            "hotel",
            "check-in",
        ],
    ),
    (
        "promo",
        &[
            "% off",
            "sale",
            "discount",
            "offer",
            "deal",
            "coupon",
            "newsletter",
            "unsubscribe",
        ],
    ),
    (
        "work",
        &[
            "meeting",
            "deadline",
            "contract",
            "review",
            "pull request",
            "project",
            "report",
            "standup",
        ],
    ),
];

fn rule_category(e: &Email, s: &Signals) -> &'static str {
    let hay = format!("{} {}", e.subject, e.sender_addr).to_lowercase();
    if let Some((name, _)) = CATEGORIES
        .iter()
        .find(|(_, words)| words.iter().any(|w| hay.contains(w)))
    {
        if *name != "promo" || s.bulk {
            return name;
        }
    }
    if s.bulk {
        "notification"
    } else if s.known {
        "personal"
    } else {
        "other"
    }
}

impl Triager for RulesTriager {
    fn triage(&self, e: &Email, s: &Signals) -> anyhow::Result<Triage> {
        let why = if s.reply {
            "Reply to your mail"
        } else if s.known && !s.bulk {
            "From someone you write to"
        } else if s.bulk {
            "Automated or bulk"
        } else {
            "Unknown sender"
        };
        Ok(Triage {
            priority: base_score(s),
            category: rule_category(e, s).into(),
            action: e.subject.chars().take(80).collect(),
            urgent: false,
            why: format!("Rule-based: {why}."),
            needs_reply: !s.bulk && (s.reply || s.known) && e.body.contains('?'),
            due: String::new(),
        })
    }
}

/// Run `primary`; on any error use the rules, so a failing AI never loses mail.
pub fn triage_with_fallback(primary: &dyn Triager, e: &Email, s: &Signals) -> Triage {
    primary
        .triage(e, s)
        .or_else(|_| RulesTriager.triage(e, s))
        .expect("rules triage is infallible")
}
