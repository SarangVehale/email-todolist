# ADR-0010: A backup is one JSON file, and restoring only adds

> **Status:** accepted · **Date:** 2026-11-14 · **Deciders:** the owner and Claude · **Last reviewed:** 2026-11-14

## Context

Everything you tell the app (rules, sender choices, corrections, your own tasks, notes, what you did with mail, options) lives in
one database on one computer. A new computer, a reinstall or "Sign out and reset" loses it, while the mail itself is safe in the
mailbox. A backup file can be edited or planted, so it is untrusted input, and it must not carry a secret or turn on anything
that sends data out ([Phase 7](../designs/phase7.md)).

## Decision

We write a backup as one unencrypted JSON file (`inbox-agent-backup-<date>-<time>.json`, in Downloads, never over another file)
holding only what you did on top of your mail, keyed by Message-ID for mail. Restoring (`backup_import`) is desktop-only, shows a
preview first, only adds what is missing, and runs in one database transaction; the preview is the same code rolled back.

## Consequences

* No password, key, hub code, ntfy topic, Ollama address or message text is in the file; correction rows carry a sender and
  subject, the one exception ([PRIVACY.md](../../PRIVACY.md)).
* A restore can never delete anything, and the same file restored twice changes nothing the second time. Named exceptions: ticked
  options take the file's values, a later snooze wins.
* State for mail not yet read waits in `restored_state` (at most 5,000 rows, 90 days) and is applied when the message arrives.
* A hostile file is bounded (8 MB, list limits, every value checked) and cannot turn on the hub, write-back or the paid ranking.
  A mailbox's server is the one network name it can supply, so the preview lists it.
* The file is not encrypted; you keep it like any private document.
* Revisit if scheduled backups, encryption or syncing between computers are wanted (all out of scope in Phase 7).

## Alternatives considered

| Option | Why it was not chosen |
| --- | --- |
| Copy the database file | It holds message text and is tied to one schema version; a restore could not merge with what is there. |
| A restore that replaces | It would throw away work done since the backup. |
| An encrypted file | Out of scope in Phase 7; the database it comes from is not encrypted either. |

## References

* `engine/src/backup.rs`, `engine/src/store/transfer.rs`
* [Phase 7 design](../designs/phase7.md), [Phase 7 review](../2026-11-14_PHASE7_REVIEW.md)
