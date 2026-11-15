//! Pluggable triage. `RulesTriager` is the always-available default; AI providers
//! (local model, API key, ...) implement the same trait and fall back to rules on error.
use crate::mail::Email;
use crate::signals::{base_score, Signals};

/// The verdict on one message: a priority from 1 (ignore) to 5 (within the hour), a category, a short to-do line, whether it is urgent and needs a reply, and any due date.
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

/// Something that can triage a message and draft a reply: the rules, or a model (local or API).
/// `triage` may fail, and callers then fall back to the rules (see `triage_with_fallback`); `draft` fails unless the provider has a model.
pub trait Triager: Send {
    fn triage(&self, email: &Email, sig: &Signals) -> anyhow::Result<Triage>;

    /// Writes the body of a reply. Providers without a model return an error and the caller uses a template.
    fn draft(&self, _email: &Email, _ctx: &DraftCtx) -> anyhow::Result<String> {
        anyhow::bail!("no AI model is configured")
    }
}

/// The triager that needs no model: it scores from the signals, sorts the category by keywords, and finds due dates in the text.
/// It never fails and never marks a message urgent, so it is the fallback for every other triager.
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
        let due = due_in(e);
        let needs_reply = !s.bulk && (s.reply || s.known) && e.body.contains('?');
        let mut priority = base_score(s);
        let mut why = format!("Rule-based: {why}.");
        // Someone who knows you is waiting for an answer and the date is today, tomorrow or past. This is the one
        // thing rules promote to 5. It never sets "urgent" (that flag bypasses quiet hours), and the words in a
        // message ("URGENT", "ASAP") count for nothing, because anyone can write them.
        if needs_reply && !e.auth_fail && !due.is_empty() && crate::dates::due_within_a_day(&due) {
            priority = 5;
            why = "Rule-based: someone you write to is waiting for an answer and it is due.".into();
        }
        Ok(Triage {
            priority,
            category: rule_category(e, s).into(),
            action: e.subject.chars().take(80).collect(),
            urgent: false,
            why,
            needs_reply,
            due,
        })
    }
}

/// A cued due date in the subject or the start of the body, resolved against the day the message arrived.
fn due_in(e: &Email) -> String {
    let text = format!(
        "{} {}",
        e.subject,
        e.body.chars().take(1500).collect::<String>()
    );
    crate::dates::extract_due(&text, crate::dates::received_date(e.received)).unwrap_or_default()
}

/// Run `primary`; on any error use the rules, so a failing AI never loses mail.
pub fn triage_with_fallback(primary: &dyn Triager, e: &Email, s: &Signals) -> Triage {
    let mut t = primary
        .triage(e, s)
        .or_else(|_| RulesTriager.triage(e, s))
        // the rules never fail, but a message must not be lost to a panic if that ever changes: a neutral verdict instead
        .unwrap_or_else(|_| Triage {
            priority: 3,
            category: "other".into(),
            action: String::new(),
            urgent: false,
            why: "no ranking was possible".into(),
            needs_reply: false,
            due: String::new(),
        });
    if t.due.is_empty() {
        t.due = due_in(e); // a model that found no date may still have missed an obvious one
    }
    t
}
