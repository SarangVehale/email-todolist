# ADR-0004: Secrets live in the OS keychain

> **Status:** accepted · **Date:** 2026-11-12 · **Deciders:** the owner and Claude · **Last reviewed:** 2026-11-14

## Context

The engine needs each mailbox's app password at every start, and the API key for the cloud ranking, while it runs unattended in
the tray. `settings.json` is a plain file in the user's profile, and the data folder is not encrypted at rest
([SECURITY.md](../../SECURITY.md), residual risks). A backup file must never hold a secret ([Phase 7](../designs/phase7.md)).

## Decision

We store the mail passwords and the Anthropic API key with the `keyring` crate in the operating system's keychain (Windows
Credential Manager, macOS Keychain, Secret Service), service `InboxAgent`, entries `imap-password`, `imap-password-<n>` and
`anthropic-key` (`engine/src/secrets.rs`). Settings, the database, `state` and backups never hold them.

## Consequences

* A copied data folder or backup file holds no password. "Sign out and reset" deletes every entry (`secrets::delete_all`).
* A keychain that cannot be read stops only the mailbox that needs it, with the keychain's error as its status
  (`Core::mailbox_account`); "no entry" and "cannot read" are kept apart (`secrets::lookup`).
* Linux needs a running Secret Service.
* Tests must not touch the real keychain: since [Phase 6](../designs/phase6.md) the engine's tests use an in-memory one
  (`secrets::in_memory`), so a test run can never read, overwrite or delete a password saved by the real app.
* The phone hub's access code is not in the keychain: it is in `settings.json` (`hub.token`), in plain text. SECURITY.md lists
  this as a residual risk.
* Revisit if the hub code should move to the keychain, or if a platform without a keychain must be supported.

## Alternatives considered

| Option | Why it was not chosen |
| --- | --- |
| The password in `settings.json` | Readable by anything that reads the file, and copied with the folder. |
| An encrypted file with its key beside it | No safer than the plain file. |
| Ask for the password at every start | The app starts at login, minimised, and must sync with nobody at the keyboard. |

## References

* `engine/src/secrets.rs`, `engine/src/core.rs` (`mailbox_account`, `set_secret`, `sign_out`)
* [Security model](../architecture/security-model.md#where-each-secret-lives)
