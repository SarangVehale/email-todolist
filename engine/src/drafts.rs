//! Reply drafting. A model writes the draft when one is configured; otherwise a quick-reply template is used.
//! Drafts are stored with the task and are NEVER sent automatically. Saving one to the mailbox's Drafts
//! folder is an explicit user action, and the message built here is hardened against header injection.
use crate::mail::{clean_message_id, valid_address, Email};
use crate::triage::{DraftCtx, Triager};
use anyhow::{bail, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use rand::Rng;

/// Quick replies for when no model is available (or as a starting point).
pub const TEMPLATES: &[(&str, &str)] = &[
    (
        "thanks",
        "Thanks for your message. I've received it and will get back to you shortly.",
    ),
    ("confirm", "Thanks, that works for me. Consider it confirmed."),
    (
        "decline",
        "Thank you for thinking of me. Unfortunately I can't make this work, so I'll have to decline.",
    ),
    (
        "later",
        "Thanks for reaching out. I need a little more time and will reply properly by [date].",
    ),
];

/// First word of the sender's name, letters only, or empty when it is an address or not usable.
fn first_name(e: &Email) -> String {
    let w: String = e
        .sender_name
        .split_whitespace()
        .next()
        .unwrap_or("")
        .chars()
        .take_while(|c| c.is_alphabetic() || *c == '\'' || *c == '-')
        .collect();
    if w.is_empty() || e.sender_name.contains('@') {
        String::new()
    } else {
        w
    }
}

fn template_key(instruction: &str) -> &'static str {
    let i = instruction.to_lowercase();
    if i.contains("declin") || i.contains("can't") || i.contains("cannot") {
        "decline"
    } else if i.contains("confirm") || i.contains("yes") || i.contains("accept") {
        "confirm"
    } else if i.contains("later") || i.contains("time") || i.contains("busy") {
        "later"
    } else {
        "thanks"
    }
}

/// A reply made from a fixed quick-reply text, picked from the user's instruction (thanks, confirm, decline or later), with a greeting by first name and the signature if there is one.
/// It never fails, so it is the fallback when no model can draft.
pub fn template_draft(e: &Email, ctx: &DraftCtx) -> String {
    let key = template_key(&ctx.instruction);
    let text = TEMPLATES
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, t)| *t)
        .unwrap_or(TEMPLATES[0].1);
    let name = first_name(e);
    let mut out = format!(
        "{}\n\n{text}",
        if name.is_empty() {
            "Hi,".to_string()
        } else {
            format!("Hi {name},")
        }
    );
    if !ctx.signature.is_empty() {
        out.push_str("\n\n");
        out.push_str(&ctx.signature);
    }
    out
}

/// Returns the draft and where it came from ("ai" or "template").
pub fn generate(t: &dyn Triager, e: &Email, ctx: &DraftCtx) -> (String, &'static str) {
    match t.draft(e, ctx) {
        Ok(mut body) => {
            if !ctx.signature.is_empty() && !body.contains(ctx.signature.trim()) {
                body.push_str("\n\n");
                body.push_str(&ctx.signature);
            }
            (body, "ai")
        }
        Err(_) => (template_draft(e, ctx), "template"),
    }
}

// ───────── building the RFC 822 message for the Drafts folder ─────────

/// The parts of a reply to save into the mailbox's Drafts folder; `build_message` turns them into the message text.
/// The fields are untrusted until `build_message` has checked them.
pub struct DraftMessage<'a> {
    pub from: &'a str,
    pub to: &'a str,
    pub subject: &'a str,
    pub in_reply_to: &'a str,
    pub references: &'a str,
    pub body: &'a str,
}

fn strip_ctl(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).collect()
}

/// RFC 2047 encoded-words (UTF-8, base64), split on character boundaries, for a subject with non-ASCII text.
fn encode_subject(s: &str) -> String {
    if s.chars().all(|c| c.is_ascii() && !c.is_control()) {
        return s.to_string();
    }
    let mut words = vec![];
    let mut cur = String::new();
    for c in s.chars() {
        if cur.len() + c.len_utf8() > 42 {
            words.push(std::mem::take(&mut cur));
        }
        cur.push(c);
    }
    if !cur.is_empty() {
        words.push(cur);
    }
    words
        .iter()
        .map(|w| format!("=?UTF-8?B?{}?=", STANDARD.encode(w)))
        .collect::<Vec<_>>()
        .join("\r\n ")
}

fn reply_subject(s: &str) -> String {
    let s: String = strip_ctl(s).chars().take(200).collect();
    let s = s.trim();
    if s.to_lowercase().starts_with("re:") {
        s.to_string()
    } else {
        format!("Re: {s}")
    }
}

/// Builds a plain-text message flagged as an unsent draft. Rejects invalid addresses; header values can never
/// contain line breaks, so a crafted subject or sender cannot add headers such as Bcc.
pub fn build_message(m: &DraftMessage) -> Result<Vec<u8>> {
    if !valid_address(m.from) || !valid_address(m.to) {
        bail!("the reply address isn't valid");
    }
    let ids = |s: &str| {
        s.split_whitespace()
            .map(clean_message_id)
            .filter(|i| !i.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    };
    let in_reply_to = clean_message_id(m.in_reply_to);
    let mut refs = ids(m.references);
    if !in_reply_to.is_empty() && !refs.contains(&in_reply_to) {
        refs = format!("{refs} {in_reply_to}").trim().to_string();
    }
    let rand: String = (0..24)
        .map(|_| rand::thread_rng().gen_range(b'a'..=b'z') as char)
        .collect();
    let body = m.body.replace("\r\n", "\n").replace('\n', "\r\n");
    let b64 = STANDARD.encode(body.as_bytes());
    let wrapped: Vec<&str> = b64
        .as_bytes()
        .chunks(76)
        .map(|c| std::str::from_utf8(c).unwrap_or(""))
        .collect();
    let mut out = format!(
        "From: {}\r\nTo: {}\r\nSubject: {}\r\nDate: {}\r\nMessage-ID: <{rand}@inbox-agent.invalid>\r\n",
        m.from,
        m.to,
        encode_subject(&reply_subject(m.subject)),
        chrono::Utc::now().to_rfc2822()
    );
    if !in_reply_to.is_empty() {
        out.push_str(&format!("In-Reply-To: {in_reply_to}\r\n"));
    }
    if !refs.is_empty() {
        out.push_str(&format!("References: {refs}\r\n"));
    }
    out.push_str("MIME-Version: 1.0\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Transfer-Encoding: base64\r\nX-Inbox-Agent: draft\r\n\r\n");
    out.push_str(&wrapped.join("\r\n"));
    out.push_str("\r\n");
    Ok(out.into_bytes())
}
