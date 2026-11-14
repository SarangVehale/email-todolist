# ADR-0003: IMAP only, and the app never sends mail

> **Status:** accepted · **Date:** 2026-11-12 · **Deciders:** the owner and Claude · **Last reviewed:** 2026-11-14

## Context

The owner set two constraints: the app never sends mail, and it works with any IMAP provider through an app password. Reply
drafts are written by a model or a template from text an attacker controls, so a send path would be one prompt injection away
from mail sent in your name ([SECURITY.md](../../SECURITY.md), finding 11). Provider sign-in is not free: reading Gmail over
IMAP with Google sign-in needs a restricted scope with a paid yearly security assessment ([Phase 3](../designs/phase3.md)), and
Microsoft turned off password sign-in for IMAP.

## Decision

We read mail over IMAP only, with `EXAMINE` and `BODY.PEEK` so reading never marks anything as read, and change the mailbox only
through IMAP on the opt-in write-back connection ([ADR-0008](0008-write-back-is-opt-in.md)). There is no SMTP client in the
code. A reply draft can only be appended to the Drafts folder (`writeback::save_draft`), by an explicit click.

## Consequences

* No code path can send mail on your behalf, whatever a model or a message says.
* Any provider that offers IMAP with an app password works; Gmail's labels are used through `X-GM-LABELS`.
* You send the reply from your own mail program. Gmail needs 2-step verification and an app password. Microsoft accounts are not
  supported.
* Revisit if the owner wants the app to send, or if Google sign-in becomes possible without a paid review (the steps a safe
  version needs are kept in [Phase 3](../designs/phase3.md)).

## Alternatives considered

| Option | Why it was not chosen |
| --- | --- |
| SMTP for sending replies | Against the owner's constraint, and it would let hostile text reach a send button. |
| Provider APIs with OAuth (Gmail API, Microsoft Graph) | One integration per provider; Google's restricted scope needs a paid assessment for a public app. |

## References

* `engine/src/watcher.rs` (read-only), `engine/src/writeback.rs`, `engine/src/drafts.rs`
* [Life of a message](../architecture/message-lifecycle.md)
