//! Parsed view of one RECEIVED message. Every field here is attacker-controlled, so parsing
//! drops anything that could be abused downstream (IMAP commands, deep MIME, forged headers).
use crate::calendar::{self, Event};
use mailparse::{addrparse, DispositionType, MailAddr, MailHeaderMap, ParsedMail};

/// More multipart containers than any real message has; beyond this we refuse to parse (stack-depth DoS).
const MAX_MULTIPART_MARKERS: usize = 200;
const PARSE_STACK_BYTES: usize = 64 << 20;
const MAX_TEXT_DEPTH: usize = 24;
/// Characters of message text kept for drafting replies.
const MAX_BODY_CHARS: usize = 3000;
/// HTML beyond this many bytes is ignored when rendering to text.
const MAX_HTML_BYTES: usize = 256 * 1024;
const MAX_REFERENCES: usize = 10;

/// One received message after parsing: sender, subject, received time, a short excerpt, the capped body text, thread ids and attachment names.
/// Every field is attacker-controlled; parsing has already cleaned or capped the ones that reach IMAP commands, headers or the screen.
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
    /// Total number of attached files (inline images such as logos are not counted).
    pub attachment_count: usize,
    /// Up to five attachment file names, cleaned for display.
    pub attachments: Vec<String>,
    /// A safe `https:` or `mailto:` link from the List-Unsubscribe header, or empty.
    pub unsubscribe: String,
    /// The conversation this message belongs to: the first id in References (the oldest message), else the id it
    /// replies to, else its own Message-ID. Empty when the message has no usable id at all.
    pub thread: String,
    /// The first event of a calendar invitation in the message (`text/calendar`, `application/ics` or a `.ics` file), if
    /// there is one and it is readable. See `calendar::parse`.
    pub event: Option<Event>,
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

/// The (display name, lower-cased address) pairs in an address header such as From or Reply-To, with groups flattened.
/// A value that cannot be parsed gives an empty list. The addresses are not checked with `valid_address`.
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

/// Every `<...>` token in a header value, with or without spaces between them (`<a@x><b@x>` is two ids).
fn ids(value: &str) -> Vec<String> {
    let mut out = vec![];
    let mut cur: Option<String> = None;
    for c in value.chars() {
        match (c, cur.as_mut()) {
            ('<', _) => cur = Some(String::from("<")),
            ('>', Some(s)) => {
                s.push('>');
                out.push(cur.take().unwrap_or_default());
            }
            (c, Some(s)) if !c.is_whitespace() => s.push(c),
            (_, Some(_)) => cur = None, // whitespace inside <...> is not an id
            _ => {}
        }
        if out.len() >= 200 {
            break; // a header cannot make this list unbounded
        }
    }
    out
}

/// The clean Message-IDs a header value (In-Reply-To, References) refers to.
pub fn header_ids(value: &str) -> Vec<String> {
    ids(value)
        .iter()
        .map(|i| clean_message_id(i))
        .filter(|i| !i.is_empty())
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
fn drop_blocks(html: &str, truncated: bool) -> String {
    const KINDS: [(&str, &str); 4] = [
        ("<script", "</script>"),
        ("<style", "</style>"),
        ("<title", "</title>"),
        ("<!--", "-->"),
    ];
    // One lowercase copy and one left-to-right pass: time is linear in the input however many blocks it has.
    let lower = html.to_ascii_lowercase();
    let mut out = String::with_capacity(html.len());
    let mut pos = 0;
    // next[k] = where kind k opens at or after `pos`; None once it has no more openers or no closer is left
    let mut next: [Option<usize>; 4] = [None; 4];
    let mut stale = [true; 4];
    loop {
        for (k, (open, _)) in KINDS.iter().enumerate() {
            if stale[k] || next[k].is_some_and(|n| n < pos) {
                next[k] = lower[pos..].find(open).map(|i| i + pos);
                stale[k] = false;
            }
        }
        let Some((k, a)) = next
            .iter()
            .enumerate()
            .filter_map(|(k, n)| n.map(|n| (k, n)))
            .min_by_key(|&(_, n)| n)
        else {
            break;
        };
        match lower[a..].find(KINDS[k].1) {
            Some(i) => {
                out.push_str(&html[pos..a]);
                out.push(' ');
                pos = a + i + KINDS[k].1.len();
            }
            // The text was cut inside this block, so the rest of it is block content (CSS, script), not mail text.
            None if truncated => {
                out.push_str(&html[pos..a]);
                pos = html.len();
                break;
            }
            // an opener with no closer: leave it as text, and no later opener of this kind can close either
            None => next[k] = None,
        }
    }
    out.push_str(&html[pos..]);
    out
}

/// A readable plain-text rendering of an HTML body: block tags become line breaks, other tags vanish,
/// entities are decoded. Malformed markup degrades to text, never to an error.
pub fn html_to_text(html: &str) -> String {
    // Only the start of a message is ever shown or stored, so a huge body is cut before any work is done.
    let mut cut = html.len().min(MAX_HTML_BYTES);
    while !html.is_char_boundary(cut) {
        cut -= 1;
    }
    let s = drop_blocks(&html[..cut], cut < html.len());
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

/// A file name that is safe to show: no path, no control characters, at most 60 characters.
fn clean_file_name(s: &str) -> String {
    let base = s.rsplit(['/', '\\']).next().unwrap_or("");
    base.chars()
        .filter(|c| !c.is_control())
        .take(60)
        .collect::<String>()
        .trim()
        .to_string()
}

/// The first readable event in any part of the message. A calendar part over the size limit is ignored (it is decoded first, which the 5 MB message limit bounds).
fn find_event(m: &ParsedMail, depth: usize) -> Option<Event> {
    if depth > MAX_TEXT_DEPTH {
        return None;
    }
    if !m.subparts.is_empty() {
        return m.subparts.iter().find_map(|p| find_event(p, depth + 1));
    }
    let named_ics = m
        .get_content_disposition()
        .params
        .get("filename")
        .or_else(|| m.ctype.params.get("name"))
        .is_some_and(|n| n.to_ascii_lowercase().ends_with(".ics"));
    if !(matches!(
        m.ctype.mimetype.as_str(),
        "text/calendar" | "application/ics"
    ) || named_ics)
    {
        return None;
    }
    calendar::parse(&m.get_body().ok()?) // refuses anything over the size limit itself
}

fn collect_attachments(m: &ParsedMail, depth: usize, names: &mut Vec<String>, count: &mut usize) {
    if depth > MAX_TEXT_DEPTH {
        return;
    }
    if m.subparts.is_empty() {
        let d = m.get_content_disposition();
        let file = d
            .params
            .get("filename")
            .or_else(|| m.ctype.params.get("name"));
        let is_attachment = d.disposition == DispositionType::Attachment
            || (d.disposition != DispositionType::Inline && file.is_some());
        if is_attachment {
            *count += 1;
            if names.len() < 5 {
                let n = clean_file_name(file.map(String::as_str).unwrap_or(""));
                names.push(if n.is_empty() {
                    m.ctype.mimetype.clone()
                } else {
                    n
                });
            }
        }
        return;
    }
    for p in &m.subparts {
        collect_attachments(p, depth + 1, names, count);
    }
}

/// The first https link, else the first mailto link, in a List-Unsubscribe header. Anything else (http, javascript,
/// data, links with spaces or control characters, very long links) is ignored.
pub fn unsubscribe_link(header: &str) -> String {
    let links: Vec<&str> = header
        .split('<')
        .skip(1)
        .filter_map(|p| p.split_once('>').map(|(link, _)| link)) // only a complete <...> counts
        .map(str::trim)
        .collect();
    let plain = |l: &str| {
        l.len() <= 2000
            && l.is_ascii()
            && !l
                .chars()
                .any(|c| c.is_control() || c.is_whitespace() || matches!(c, '<' | '>' | '\\' | '"'))
    };
    links
        .iter()
        .find(|l| plain(l) && https_ok(l))
        .or_else(|| links.iter().find(|l| plain(l) && mailto_ok(l)))
        .map(|l| l.to_string())
        .unwrap_or_default()
}

/// `https://host[:port][/...]` that a browser's URL parser will accept: a real DNS-style host, no user info,
/// no IPv6 brackets, a numeric port in range. Anything odd is rejected rather than repaired.
fn https_ok(l: &str) -> bool {
    let Some(rest) = l
        .get(..8)
        .filter(|p| p.eq_ignore_ascii_case("https://"))
        .map(|_| &l[8..])
    else {
        return false;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p)) => (h, Some(p)),
        None => (authority, None),
    };
    let port_ok = port.is_none_or(|p| {
        !p.is_empty()
            && p.len() <= 5
            && p.bytes().all(|b| b.is_ascii_digit())
            && p.parse::<u32>().is_ok_and(|n| (1..=65535).contains(&n))
    });
    let host = host.strip_suffix('.').unwrap_or(host); // "example.com." is the same host
    let labels: Vec<&str> = host.split('.').collect();
    port_ok
        && !host.is_empty()
        && host.len() <= 253
        // a public name has at least two labels and a non-numeric ending: this refuses "localhost", "10.0.0.1",
        // "169.254.169.254" and the decimal form "2130706433"
        && labels.len() >= 2
        && !labels
            .last()
            .is_some_and(|l| l.bytes().all(|b| b.is_ascii_digit()))
        && labels.iter().all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with(['-', '_'])
                && !label.ends_with(['-', '_'])
                && label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        })
}

/// `mailto:address[?query]` where the address is a plausible single mailbox.
fn mailto_ok(l: &str) -> bool {
    let Some(rest) = l
        .get(..7)
        .filter(|p| p.eq_ignore_ascii_case("mailto:"))
        .map(|_| &l[7..])
    else {
        return false;
    };
    let addr = rest.split('?').next().unwrap_or("");
    valid_address(addr)
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
    let reference_ids = ids(&get("References"));
    refs.extend(reference_ids.iter().cloned());
    let message_id = clean_message_id(&get("Message-ID"));
    // taken before the list is cut to its last entries, so a long thread keeps its root
    let thread = reference_ids
        .iter()
        .chain(in_reply_to.iter())
        .map(|r| clean_message_id(r))
        .find(|r| !r.is_empty())
        .unwrap_or_else(|| message_id.clone());
    let mut chain: Vec<String> = refs
        .iter()
        .map(|r| clean_message_id(r))
        .filter(|r| !r.is_empty())
        .collect();
    chain.dedup();
    if chain.len() > MAX_REFERENCES {
        chain.drain(..chain.len() - MAX_REFERENCES);
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
    let (mut attachments, mut attachment_count) = (vec![], 0);
    collect_attachments(&m, 0, &mut attachments, &mut attachment_count);
    let unsubscribe = unsubscribe_link(&get("List-Unsubscribe"));
    let event = find_event(&m, 0);
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
    // a Date outside 2000..2100 is a forgery or a typo; treat it as unknown rather than trusting it
    let received = mailparse::dateparse(&get("Date"))
        .map(|t| t as f64)
        .ok()
        .filter(|t| (946_684_800.0..4_102_444_800.0).contains(t))
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
        references: chain.join(" "),
        bulk_headers,
        auth_fail,
        attachment_count,
        attachments,
        unsubscribe,
        thread,
        event,
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
