//! "Is there a newer version?" Once a day the app can ask GitHub for the latest release of one repository and, if it is
//! newer, tell the user. It never downloads or runs anything. The check is off until `UPDATE_REPO` names a repository.
use anyhow::{anyhow, Result};
use std::io::Read;

/// The GitHub repository releases are published to, as `owner/name`. Empty until the project has one: while it is
/// empty there is no update check at all (no network call, no setting shown). Set it, release, and it switches on.
pub const UPDATE_REPO: &str = "";

const MAX_RESPONSE_BYTES: u64 = 256 * 1024;

/// A newer release that was found: its tag, its plain version number and the release page.
#[derive(Debug, Clone, PartialEq)]
pub struct Update {
    /// The release tag exactly as GitHub published it, so the stored value and the page address agree.
    pub tag: String,
    /// Version number without the leading "v".
    pub version: String,
    /// The release page, always on github.com under the configured repository.
    pub url: String,
}

/// `x.y.z` (a leading "v" is allowed; "-suffix" marks a pre-release; "+build" is ignored). Anything else is not a version.
fn parse(v: &str) -> Option<((u32, u32, u32), bool)> {
    let v = v.trim().trim_start_matches(['v', 'V']);
    let v = v.split('+').next()?;
    let (core, pre) = match v.split_once('-') {
        Some((c, _)) => (c, true),
        None => (v, false),
    };
    let mut parts = core.split('.');
    let mut num = || {
        let p = parts.next()?;
        (!p.is_empty() && p.len() <= 9 && p.bytes().all(|b| b.is_ascii_digit()))
            .then(|| p.parse::<u32>().ok())
            .flatten()
    };
    let t = (num()?, num()?, num()?);
    parts.next().is_none().then_some((t, pre))
}

/// Is `tag` a newer version than `current`? A pre-release tag only counts when the running version is a pre-release too.
pub fn newer(current: &str, tag: &str) -> bool {
    match (parse(current), parse(tag)) {
        // a final release is newer than its own pre-releases: (1,0,0) final beats (1,0,0)-rc.1
        (Some((c, c_pre)), Some((t, t_pre))) => (t, !t_pre) > (c, !c_pre) && (!t_pre || c_pre),
        _ => false,
    }
}

fn valid_repo(repo: &str) -> bool {
    let ok = |s: &str| {
        (1..=100).contains(&s.len())
            && !s.starts_with('.')
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
    };
    repo.split_once('/')
        .is_some_and(|(o, n)| ok(o) && ok(n) && !n.contains('/'))
}

/// Asks `base` (`https://api.github.com` in the app) for the latest release of `repo`.
/// `Ok(None)` means up to date. Errors are for the caller to ignore until tomorrow.
pub fn check(base: &str, repo: &str, current: &str) -> Result<Option<Update>> {
    if !valid_repo(repo) {
        return Err(anyhow!("not a repository name"));
    }
    let resp = crate::ai::http_agent(5)
        .get(&format!(
            "{}/repos/{repo}/releases/latest",
            base.trim_end_matches('/')
        ))
        .set("User-Agent", &format!("InboxAgent/{current}"))
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| anyhow!("update check failed: {e}"))?;
    let mut body = String::new();
    resp.into_reader()
        .take(MAX_RESPONSE_BYTES)
        .read_to_string(&mut body)?;
    let j: serde_json::Value = serde_json::from_str(&body)?;
    let tag = j["tag_name"].as_str().unwrap_or("");
    if j["draft"].as_bool() == Some(true) {
        return Ok(None);
    }
    Ok(release(repo, current, tag))
}

/// A release tag that is a plain version: `v1.2.3`, optionally with a short alphanumeric pre-release suffix. This is
/// the only text from the response that is ever used, and it is checked before it goes into an address.
fn tag_ok(tag: &str) -> bool {
    let core = tag.split_once('-').map_or(tag, |(c, _)| c);
    let suffix_ok = tag.split_once('-').is_none_or(|(_, s)| {
        (1..=20).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.')
    });
    // parse() is forgiving about leading "v"s and spaces; this is not: at most one "v", then digits, dots and an
    // optional hyphen-suffix, so the stored tag and the page address can never disagree.
    let v_count = tag.chars().take_while(|c| *c == 'v' || *c == 'V').count();
    v_count <= 1
        && tag.len() <= 40
        && tag
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
        && suffix_ok
        && parse(core).is_some()
}

/// The update for `tag`, if it is a safe tag and newer than `current`. The page address is built here from the
/// configured repository and the checked tag; nothing else from the response (not even its own `html_url`) is
/// trusted, so a renamed or taken-over repository cannot point users somewhere else.
pub fn release(repo: &str, current: &str, tag: &str) -> Option<Update> {
    if !valid_repo(repo) || !tag_ok(tag) || !newer(current, tag) {
        return None;
    }
    Some(Update {
        tag: tag.to_string(),
        version: tag.trim_start_matches(['v', 'V']).to_string(),
        url: format!("https://github.com/{repo}/releases/tag/{tag}"),
    })
}

/// Reads back what the daily check stored (the newest tag, or empty) and drops it if the running version has caught up.
pub fn pending(stored: Option<&str>, repo: &str, current: &str) -> Option<Update> {
    release(repo, current, stored?.trim())
}
