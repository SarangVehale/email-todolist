# ADR-0007: Task ids encode the mailbox

> **Status:** accepted · **Date:** 2026-11-13 · **Deciders:** the owner and Claude · **Last reviewed:** 2026-11-14

## Context

Until Phase 6 the app read one mailbox and a task's id was the message's IMAP UID, which is unique only inside one mailbox. The
id is used everywhere: the database key, every command, the window's comparisons and the phone. Reading up to five mailboxes
into one list needed ids that never collide. The first draft of the design used 64-bit ids, and that turned out to need a change
in every place the window compares an id ([Phase 6](../designs/phase6.md)).

## Decision

We keep one `u32` id and cut it into blocks: the task id of IMAP message `uid` in mailbox `n` (0 to 4) is
`n * 400,000,000 + uid` (`MAILBOX_SPAN`), and tasks you add and follow-ups start at 2,000,000,000 (`LOCAL_UID_BASE`). The helpers
are `global_uid`, `mailbox_of` and `imap_uid` in `engine/src/store.rs`.

## Consequences

* Mailbox 0's ids are its IMAP UIDs, so an existing database needed no migration and every id a window or phone held stayed valid.
* Which mailbox a task belongs to is arithmetic on its id; each mailbox's sync uses a store scoped to its block (`Store::scoped`).
* At most five mailboxes. A UID of 400,000,000 or more cannot be tracked and is skipped with a log line.
* A mailbox's number never changes while it exists; a removed mailbox's number is cleared before it is used again (`clear_mailbox`).
* Revisit if more than five mailboxes are needed, or a real mailbox's UIDs approach 400 million.

## Alternatives considered

| Option | Why it was not chosen |
| --- | --- |
| A 64-bit id (the first draft) | Needed a change in every place the window compares an id ([Phase 6](../designs/phase6.md), status line). |
| One database per mailbox | Rules, sender choices, corrections and your own tasks are shared by all mailboxes, so they would need a third home. |

## References

* [Phase 6 design](../designs/phase6.md), section "A task's id says which mailbox it came from"
* [Data model](../architecture/data-model.md#task-ids)
