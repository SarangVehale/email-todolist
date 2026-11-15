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
        oauth: None,
    };
    let limit = std::env::var("PROBE_LIMIT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(20);
    let mut s = watcher::connect(&a)?;
    let store = Store::memory()?; // throwaway: nothing is saved
                                  // as the sync does: a mailbox with no Sent folder we can find is read all the same (replies will not close tasks)
    let (folder, sent) = watcher::sent_or_empty(&mut s, &store);
    match folder {
        Some(f) => println!("connected; Sent folder: {f}"),
        None => println!("connected; no Sent folder found (replies you send will not close tasks)"),
    }
    println!(
        "sent index: {} recipients, {} message ids",
        sent.recipients.len(),
        sent.message_ids.len()
    );
    let n = watcher::process_new(&mut s, &store, &RulesTriager, &sent, limit, &mut |e, t| {
        println!(
            "P{} {:<10} {} | {}",
            t.priority, t.category, e.sender_addr, e.subject
        );
    })?;
    println!("{n} messages triaged (read-only; nothing changed in your mailbox)");
    Ok(())
}
