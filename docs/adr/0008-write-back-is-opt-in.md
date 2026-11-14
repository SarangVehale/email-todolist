# ADR-0008: Changing the mailbox is opt-in

> **Status:** accepted · **Date:** 2026-11-12 · **Deciders:** the owner and Claude · **Last reviewed:** 2026-11-14

## Context

Finishing, starring or re-ranking a task could also archive, flag or label the message in the real mailbox, which is what makes
the list and the mailbox agree. It is also the riskiest thing the app does: the first security review found that a crafted
Message-ID could inject IMAP commands, and that a substring search could match and archive many messages at once
([SECURITY.md](../../SECURITY.md), findings 1 and 2). A person trying the app should not have their mailbox changed before
they trust it.

## Decision

We never change the mailbox unless "Let Inbox Agent file mail" (`writeback` in the settings, `false` by default) is on. Every
change is queued in the `outbox` table and applied by a separate, writable IMAP connection per mailbox (`writeback::drain`),
which finds the message by Message-ID because an archived message loses its INBOX UID. The sync connection stays read-only.

## Consequences

* Trying the app, and every mailbox's first sync, leaves the mailbox exactly as it was.
* A message is touched only when its exact Message-ID matches at most three messages; labels are only `AI/...`
  ([security model](../architecture/security-model.md#untrusted-inputs-and-their-bounds)).
* While the switch is off, changes still queue, and switching it on applies the ones still waiting ([PRIVACY.md](../../PRIVACY.md)).
* Saving a reply draft to the Drafts folder needs the switch on.
* Mail without a usable Message-ID cannot be changed; the log says so. Gmail and other servers behave differently (labels or an
  Archive folder), and write-back has been tested against a scripted server, not a real one.
* Revisit when write-back has been tested against real servers (listed as not done in SECURITY.md).

## Alternatives considered

| Option | Why it was not chosen |
| --- | --- |
| Write-back on by default | A mistake or a forged Message-ID could change the mailbox before the user chose to trust the app. |
| Find messages by UID | An archived message loses its INBOX UID (`writeback.rs` module comment). |
| Use the sync's connection | The sync examines the inbox read-only (`EXAMINE`), so a reading bug cannot write. |

## References

* `engine/src/writeback.rs`, `engine/src/core.rs` (`spawn_mailbox`), `engine/src/store.rs` (`queue`, `pending`)
* [Life of a message](../architecture/message-lifecycle.md#from-new-mail-to-done)
