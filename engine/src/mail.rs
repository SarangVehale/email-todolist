//! Parsed view of one RECEIVED message. Every field here is attacker-controlled, so parsing
//! drops anything that could be abused downstream (IMAP commands, deep MIME, forged headers).
use mailparse::{addrparse, MailAddr, MailHeaderMap, ParsedMail};

/// More multipart containers than any real message has; beyond this we refuse to parse (stack-depth DoS).
const MAX_MULTIPART_MARKERS: usize = 200;
const PARSE_STACK_BYTES: usize = 64 << 20;
const MAX_TEXT_DEPTH: usize = 24;
/// Characters of message text kept for drafting replies.
const MAX_BODY_CHARS: usize = 3000;
const MAX_REFERENCES: usize = 10;

#[derive(Debug, Clone)]
pub struct Email {
    pub uid: u32,
    /// Empty unless it is a plain `<printable-ascii>` token (see `clean_message_id`).
    pub message_id: String,
    pub sender_name: String,
    pub sender_addr: String,
    pub subject: String,
    pub received: f64, // unix seconds
    pub excerpt: String,
    /// Plain-text content with line breaks kept (capped), used only to draft replies.
    pub body: String,
    pub in_reply_to: Vec<String>,
    /// Where a reply should go: Reply-To if it is a valid address, otherwise the sender.
    pub reply_to: String,
    /// Space-separated clean Message-IDs of the thread so far, for the References header of a reply.
    pub references: String,
    pub bulk_headers: bool,
    /// The receiving server reported a failed DKIM/SPF/DMARC check, so the From address is not trustworthy.
    pub auth_fail: bool,
}

/// A Message-ID is used in IMAP SEARCH commands, so only a strict `<token>` of visible ASCII is kept.
/// RFC 2047 decoding can produce CR/LF, quotes and backslashes, which would break out of the command.
pub fn clean_message_id(s: &str) -> String {
    let s = s.trim();
    let ok = (3..=200).contains(&s.len())
        && s.starts_with('<')
        && s.ends_with('>')
        && s.bytes()
            .all(|b| (0x21..=0x7e).contains(&b) && b != b'"' && b != b'\\');
    if ok {
        s.to_string()
    } else {
        String::new()
    }
}

/// A bare mailbox address safe to place in a header: no spaces, control characters, quotes or list separators.
pub fn valid_address(a: &str) -> bool {
    a.len() <= 320
        && a.matches('@').count() == 1
        && !a.starts_with('@')
        && !a.ends_with('@')
        && !a
            .chars()
            .any(|c| c.is_control() || c.is_whitespace() || "<>,;\"\\()".contains(c))
}

pub fn addresses(value: &str) -> Vec<(String, String)> {
    let mut out = vec![];
    if let Ok(list) = addrparse(value) {
        for a in list.iter() {
            match a {
                MailAddr::Single(s) => out.push((
                    s.display_name.clone().unwrap_or_default(),
                    s.addr.to_lowercase(),
                )),
                MailAddr::Group(g) => {
                    for s in &g.addrs {
                        out.push((
                            s.display_name.clone().unwrap_or_default(),
                            s.addr.to_lowercase(),
                        ))
                    }
                }
            }
        }
    }
    out
}

fn ids(value: &str) -> Vec<String> {
    value
        .split_whitespace()
        .filter(|t| t.starts_with('<') && t.ends_with('>'))
        .map(String::from)
        .collect()
}

fn plain_text(m: &ParsedMail, depth: usize) -> String {
    if depth > MAX_TEXT_DEPTH {
        return String::new();
    }
    if m.subparts.is_empty() {
        return if m.ctype.mimetype == "text/plain" {
            m.get_body().unwrap_or_default()
        } else {
            String::new()
        };
    }
    m.subparts
        .iter()
        .map(|p| plain_text(p, depth + 1))
        .find(|t| !t.is_empty())
        .unwrap_or_default()
}

fn entity(name: &str) -> Option<char> {
    match name {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        "nbsp" => Some(' '),
        "ndash" | "mdash" => Some('-'),
        "hellip" => Some('.'),
        _ => {
            let n = name.strip_prefix('#')?;
            let code = match n.strip_prefix(['x', 'X']) {
                Some(h) => u32::from_str_radix(h, 16).ok()?,
                None => n.parse().ok()?,
            };
            char::from_u32(code).filter(|c| !c.is_control() || *c == '\n')
        }
    }
}

/// Removes `<tag ...> ... </tag>` blocks (script, style, title) and comments. Works on ASCII-lowercased
/// copies so byte offsets stay identical; unmatched openers are left alone, so it cannot loop or panic.
fn drop_blocks(html: &str) -> String {
    let mut s = html.to_string();
    for (open, close) in [
        ("<script", "</script>"),
        ("<style", "</style>"),
        ("<title", "</title>"),
        ("<!--", "-->"),
    ] {
        let mut from = 0;
        loop {
            let lower = s.to_ascii_lowercase();
            let Some(a) = lower[from..].find(open).map(|i| i + from) else {
                break;
            };
            let Some(b) = lower[a..].find(close).map(|i| i + a + close.len()) else {
                break;
            };
            s.replace_range(a..b, " ");
            from = a;
        }
    }
    s
}

/// A readable plain-text rendering of an HTML body: block tags become line breaks, other tags vanish,
/// entities are decoded. Malformed markup degrades to text, never to an error.
pub fn html_to_text(html: &str) -> String {
    let s = drop_blocks(html);
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len() / 2);
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '<'
            && chars
                .get(i + 1)
                .is_some_and(|n| n.is_ascii_alphabetic() || matches!(n, '/' | '!' | '?'))
        {
            let end = chars[i..]
                .iter()
                .position(|&x| x == '>')
                .map(|p| i + p)
                .unwrap_or(chars.len() - 1);
            let name: String = chars[i + 1..end]
                .iter()
                .take_while(|x| !x.is_whitespace())
                .collect::<String>()
                .to_ascii_lowercase();
            let name = name.trim_matches('/');
            if matches!(
                name,
                "br" | "p"
                    | "div"
                    | "tr"
                    | "li"
                    | "h1"
                    | "h2"
                    | "h3"
                    | "h4"
                    | "h5"
                    | "h6"
                    | "blockquote"
                    | "table"
                    | "ul"
                    | "ol"
            ) {
                out.push('\n');
            } else if matches!(name, "td" | "th") {
                out.push(' ');
            }
            i = end + 1;
        } else if c == '&' {
            let semi = chars[i + 1..].iter().take(10).position(|&x| x == ';');
            match semi.and_then(|p| {
                entity(&chars[i + 1..i + 1 + p].iter().collect::<String>()).map(|ch| (ch, p))
            }) {
                Some((ch, p)) => {
                    out.push(ch);
                    i += p + 2;
                }
                None => {
                    out.push('&');
                    i += 1;
                }
            }
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

fn html_text(m: &ParsedMail, depth: usize) -> String {
    if depth > MAX_TEXT_DEPTH {
        return String::new();
    }
    if m.subparts.is_empty() {
        return if m.ctype.mimetype == "text/html" {
            html_to_text(&m.get_body().unwrap_or_default())
        } else {
            String::new()
        };
    }
    m.subparts
        .iter()
        .map(|p| html_text(p, depth + 1))
        .find(|t| !t.trim().is_empty())
        .unwrap_or_default()
}

/// Keeps paragraph breaks, drops quoted reply lines, collapses runs of blank lines and caps the length.
fn tidy_body(text: &str) -> String {
    let mut out = String::new();
    let mut blank = 0;
    for line in text.replace("\r\n", "\n").lines() {
        let l = line.trim_end();
        if l.trim_start().starts_with('>') {
            continue;
        }
        if l.is_empty() {
            blank += 1;
            if blank > 1 {
                continue;
            }
        } else {
            blank = 0;
        }
        out.push_str(l);
        out.push('\n');
    }
    out.trim().chars().take(MAX_BODY_CHARS).collect()
}

fn parse_inner(uid: u32, raw: &[u8]) -> anyhow::Result<Email> {
    let m = mailparse::parse_mail(raw)?;
    let h = m.headers.as_slice();
    let get = |k: &str| h.get_first_value(k).unwrap_or_default();
    let (name, addr) = addresses(&get("From"))
        .into_iter()
        .next()
        .unwrap_or_default();
    let mut refs = ids(&get("In-Reply-To"));
    let in_reply_to = refs.clone();
    refs.extend(ids(&get("References")));
    let message_id = clean_message_id(&get("Message-ID"));
    let mut thread: Vec<String> = refs
        .iter()
        .map(|r| clean_message_id(r))
        .filter(|r| !r.is_empty())
        .collect();
    thread.dedup();
    if thread.len() > MAX_REFERENCES {
        thread.drain(..thread.len() - MAX_REFERENCES);
    }
    let prec = get("Precedence").to_lowercase();
    let auto = get("Auto-Submitted");
    let bulk_headers = !get("List-Unsubscribe").is_empty()
        || !get("List-Id").is_empty()
        || matches!(prec.as_str(), "bulk" | "list" | "junk")
        || (!auto.is_empty() && auto.to_lowercase() != "no");
    let auth_fail = h.get_all_values("Authentication-Results").iter().any(|v| {
        let v = v.to_lowercase();
        ["dkim=fail", "spf=fail", "dmarc=fail"]
            .iter()
            .any(|f| v.contains(f))
    });
    let plain = plain_text(&m, 0);
    // HTML-only mail is the norm today: fall back to a text rendering of the HTML part
    let text = if plain.trim().is_empty() {
        html_text(&m, 0)
    } else {
        plain
    };
    let body = tidy_body(&text);
    let excerpt: String = body
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(400)
        .collect();
    let received = mailparse::dateparse(&get("Date"))
        .map(|t| t as f64)
        .unwrap_or(0.0);
    let subject = get("Subject");
    let reply_to = addresses(&get("Reply-To"))
        .into_iter()
        .map(|(_, a)| a)
        .find(|a| valid_address(a))
        .unwrap_or_else(|| {
            if valid_address(&addr) {
                addr.clone()
            } else {
                String::new()
            }
        });
    Ok(Email {
        uid,
        message_id,
        sender_name: if name.is_empty() { addr.clone() } else { name },
        sender_addr: addr,
        subject: if subject.is_empty() {
            "(no subject)".into()
        } else {
            subject
        },
        received,
        excerpt,
        body,
        in_reply_to,
        reply_to,
        references: thread.join(" "),
        bulk_headers,
        auth_fail,
    })
}

/// Parses on a dedicated large-stack thread so a hostile message can only fail, never abort the app.
pub fn parse(uid: u32, raw: &[u8]) -> anyhow::Result<Email> {
    if raw
        .windows(10)
        .filter(|w| w.eq_ignore_ascii_case(b"multipart/"))
        .count()
        > MAX_MULTIPART_MARKERS
    {
        anyhow::bail!("message nests too many MIME parts");
    }
    std::thread::scope(|s| {
        std::thread::Builder::new()
            .stack_size(PARSE_STACK_BYTES)
            .spawn_scoped(s, || parse_inner(uid, raw))?
            .join()
            .map_err(|_| anyhow::anyhow!("parser panicked"))?
    })
}
