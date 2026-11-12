//! Rule-based signals from headers and the user's Sent history.
use crate::mail::{addresses, Email};
use mailparse::MailHeaderMap;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Default)]
pub struct SentIndex {
    pub recipients: HashSet<String>,
    pub message_ids: HashSet<String>,
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
        }
    }
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
