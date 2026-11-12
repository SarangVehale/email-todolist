//! Read-only check against a real mailbox. Prints sender/subject/priority only (never bodies).
//!   set IMAP_HOST=imap.gmail.com  IMAP_USER=you@gmail.com  IMAP_PASS=<app password>  [PROBE_LIMIT=20]
//!   cargo run --example probe --release
use engine::store::Store;
use engine::triage::RulesTriager;
use engine::watcher::{self, Account};

fn main() -> anyhow::Result<()> {
    let var = |k: &str| std::env::var(k).map_err(|_| anyhow::anyhow!("set {k}"));
    let a = Account {
        host: var("IMAP_HOST")?,
        port: std::env::var("IMAP_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(993),
        user: var("IMAP_USER")?,
        password: var("IMAP_PASS")?,
    };
    let limit = std::env::var("PROBE_LIMIT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(20);
    let mut s = watcher::connect(&a)?;
    let folder = watcher::find_sent_folder(&mut s)?;
    println!("connected; Sent folder: {folder}");
    let sent = watcher::build_sent_index(&mut s, &folder, 500)?;
    println!(
        "sent index: {} recipients, {} message ids",
        sent.recipients.len(),
        sent.message_ids.len()
    );
    let store = Store::memory()?; // throwaway: nothing is saved
    let n = watcher::process_new(&mut s, &store, &RulesTriager, &sent, limit, &mut |e, t| {
        println!(
            "P{} {:<10} {} | {}",
            t.priority, t.category, e.sender_addr, e.subject
        );
    })?;
    println!("{n} messages triaged (read-only; nothing changed in your mailbox)");
    Ok(())
}
