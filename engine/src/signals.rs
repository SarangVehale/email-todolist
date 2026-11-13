//! Rule-based signals from headers and the user's Sent history.
use crate::mail::{addresses, Email};
use mailparse::MailHeaderMap;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Default)]
pub struct SentIndex {
    pub recipients: HashSet<String>,
    pub message_ids: HashSet<String>,
    /// Message-IDs the user has answered: everything named in the In-Reply-To or References of a sent message.
    /// Forwards are left out, since forwarding a message is not the same as dealing with it.
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
            let subject = headers.get_first_value("Subject").unwrap_or_default();
            // Only In-Reply-To names the message that was answered. References lists the whole conversation, and
            // replying to the newest message does not answer the older ones in it.
            if !is_forward(&subject) {
                for v in headers.get_all_values("In-Reply-To") {
                    self.replied_to.extend(crate::mail::header_ids(&v));
                }
            }
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Signals {
    pub known: bool,
    pub reply: bool,
    pub bulk: bool,
}

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
