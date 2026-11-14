//! Rule-based signals from headers and the user's Sent history.
use crate::mail::{addresses, Email};
use mailparse::MailHeaderMap;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// How many of the newest conversations-you-started the index keeps for the follow-up check.
const MAX_STARTED: usize = 200;
/// How many of your own sending addresses are remembered.
const MAX_OWN: usize = 8;

/// A message you sent that began a conversation (it replies to nothing), with enough to ask the server whether anybody
/// answered it. The Message-ID is always a clean `<token>`, so it can be placed in a search command.
#[derive(Debug, Clone, PartialEq)]
pub struct SentMsg {
    pub message_id: String,
    /// The recipient the follow-up is about (lower-case address): the first one who is not you.
    pub to: String,
    /// Every recipient (To and Cc, lower-case, at most ten), so the check can leave out your own address.
    pub recipients: Vec<String>,
    pub subject: String,
    /// Unix seconds from the Date header.
    pub sent_at: f64,
}

/// What the Sent folder says about you: who you wrote to, your own addresses, the Message-IDs you sent, the ones you answered, and the conversations you started.
/// It is topped up from headers only (`add_headers`), so no message text is kept.
#[derive(Default)]
pub struct SentIndex {
    /// Newest first, at most MAX_STARTED; see `SentMsg`.
    pub started: Vec<SentMsg>,
    /// The addresses your sent mail was written from (at most MAX_OWN): a login name is not always an address, and
    /// aliases exist, so the follow-up check needs these to tell your own mail from a reply.
    pub from_addrs: HashSet<String>,
    pub recipients: HashSet<String>,
    pub message_ids: HashSet<String>,
    /// Message-IDs the user has answered: what the In-Reply-To of a sent message names. References is deliberately
    /// not used, because replying to the newest message of a thread does not answer the older ones. Forwards are left
    /// out too, since forwarding a message is not the same as dealing with it.
    pub replied_to: HashSet<String>,
    /// UIDVALIDITY of the Sent folder when the index was built, and the highest UID read so far, so the index can
    /// be topped up with only the new messages (0 = not yet known).
    pub uidvalidity: u32,
    pub last_uid: u32,
}

impl SentIndex {
    /// Feed the raw header block of a sent message (To, Cc, Message-ID).
    pub fn add_headers(&mut self, raw: &[u8]) {
        if let Ok((headers, _)) = mailparse::parse_headers(raw) {
            for k in ["To", "Cc"] {
                for v in headers.get_all_values(k) {
                    for (_, a) in addresses(&v) {
                        self.recipients.insert(a);
                    }
                }
            }
            if let Some(id) = headers.get_first_value("Message-ID") {
                self.message_ids.insert(id.trim().to_string());
            }
            if self.from_addrs.len() < MAX_OWN {
                if let Some((_, a)) =
                    addresses(&headers.get_first_value("From").unwrap_or_default())
                        .into_iter()
                        .next()
                {
                    if crate::mail::valid_address(&a) {
                        self.from_addrs.insert(a);
                    }
                }
            }
            let subject = headers.get_first_value("Subject").unwrap_or_default();
            // Only In-Reply-To names the message that was answered. References lists the whole conversation, and
            // replying to the newest message does not answer the older ones in it.
            if !is_forward(&subject) {
                for v in headers.get_all_values("In-Reply-To") {
                    self.replied_to.extend(crate::mail::header_ids(&v));
                }
                self.note_started(&headers, &subject);
            }
        }
    }

    /// A message with no In-Reply-To is one you started: remember it for the follow-up check.
    fn note_started(&mut self, headers: &[mailparse::MailHeader], subject: &str) {
        if !headers.get_all_values("In-Reply-To").is_empty() {
            return;
        }
        let id = crate::mail::clean_message_id(
            &headers.get_first_value("Message-ID").unwrap_or_default(),
        );
        let recipients: Vec<String> = ["To", "Cc"]
            .iter()
            .flat_map(|k| headers.get_all_values(k))
            .flat_map(|v| addresses(&v))
            .map(|(_, a)| a)
            .filter(|a| crate::mail::valid_address(a))
            .take(10)
            .collect();
        let sent_at =
            mailparse::dateparse(&headers.get_first_value("Date").unwrap_or_default()).ok();
        let (Some(to), Some(sent_at)) = (recipients.first().cloned(), sent_at) else {
            return;
        };
        if id.is_empty() || self.started.iter().any(|m| m.message_id == id) {
            return;
        }
        // The subject comes out of a header the sender chose, and ends up in a task title: no control characters.
        let subject: String = subject
            .chars()
            .filter(|c| !c.is_control())
            .take(150)
            .collect();
        self.started.push(SentMsg {
            message_id: id,
            to,
            recipients,
            subject: subject.trim().to_string(),
            sent_at: sent_at as f64,
        });
        if self.started.len() > MAX_STARTED {
            self.started.sort_by(|a, b| b.sent_at.total_cmp(&a.sent_at));
            self.started.truncate(MAX_STARTED);
        }
    }
}

/// A subject that marks a forward in the languages mail clients commonly use: Fwd/FW, WG (German), TR (French),
/// RV (Spanish), ENC (Portuguese), Doorst (Dutch), VS and VB (Nordic). Forwarding a message is not dealing with it.
pub fn is_forward(subject: &str) -> bool {
    let s = subject.trim_start().to_lowercase();
    s.split_once(':').is_some_and(|(prefix, _)| {
        matches!(
            prefix.trim_end(),
            "fwd" | "fw" | "wg" | "tr" | "rv" | "enc" | "doorst" | "vs" | "vb"
        )
    })
}

/// Three yes/no facts about an incoming message that feed the prior score and the AI prompt: the sender is someone you have written to, the message answers one of yours, and it looks automated.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Signals {
    pub known: bool,
    pub reply: bool,
    pub bulk: bool,
}

/// The signals of one message against the Sent history.
/// A message that failed the server's DKIM, SPF or DMARC check is never counted as `known` or as a `reply`, because its From address may be forged.
pub fn compute(e: &Email, sent: &SentIndex) -> Signals {
    let a = &e.sender_addr;
    let noreply = [
        "noreply",
        "no-reply",
        "no_reply",
        "donotreply",
        "do-not-reply",
        "mailer-daemon",
        "notifications@",
    ]
    .iter()
    .any(|k| a.contains(k));
    Signals {
        // A failed DKIM/SPF/DMARC check means the From address (and reply headers) may be forged.
        known: !e.auth_fail && sent.recipients.contains(a),
        reply: !e.auth_fail && e.in_reply_to.iter().any(|r| sent.message_ids.contains(r)),
        bulk: e.bulk_headers || noreply,
    }
}

/// Prior priority 1-5. Real people and replies matter; bulk mail does not.
pub fn base_score(s: &Signals) -> u8 {
    if s.reply || (s.known && !s.bulk) {
        4
    } else if s.bulk {
        if s.known {
            2
        } else {
            1
        }
    } else {
        3
    }
}
