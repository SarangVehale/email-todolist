# Phase 6 review record, 2026-11-13

Run 3 of the roadmap: several mailboxes in one list (design: `docs/designs/phase6.md`). Three gates, each by models other than
the one that wrote the code. Every claim was checked against the code before anything changed; fixes got a failing test first (the
two exceptions are named below); the guards were broken one at a time to confirm a test noticed (mutation checks).

## Gate 1: council (two seats, blind to each other)

| Seat | Verdict | Findings |
| --- | --- | --- |
| Opus | ship with caveats | removing a mailbox while it ranks a message left its cursor behind, so a later mailbox with the same number skipped its whole inbox (the same code lost a message for good after any restart, before this change too); the phone's state carried every mailbox's login through the activity log; after the first mailbox is removed only four could be added; a stopped sync could still report a status for a removed mailbox |
| Sonnet | ship with caveats | a snooze in one mailbox was cancelled by the same message arriving in another; `save_account` reset a mailbox before stopping the syncs; **the engine's tests used the real OS keychain when cargo was started from the top folder** (and so could overwrite or delete the passwords of whoever ran them); adding a mailbox logged "a mailbox was removed"; the duplicate-mailbox check missed `imap.gmail.com.` and `googlemail.com`; the UI kept read message text and undo closures under task numbers that a new mailbox can reuse; Cancel during Test/Add threw and focus was dropped; removal leaves its subjects in the 30-day log (undocumented); stale test counts |

The seats agreed on the facts. Every finding was verified before it was acted on, and all were fixed except the log lines after a
removal, which are documented in PRIVACY.md instead (the log is a record, not mailbox data).

## Gate 2: drift passes

| Pass | Confirmed | Outcome |
| --- | --- | --- |
| Design documents and pseudocode (Sonnet) | 12 disagreements, 3 test gaps. **Three were code bugs**: a follow-up record of an already-answered message was filed under mailbox 0; a correction unlinked by a renumbering was parked inside mailbox 0's id range and deleted with it; the phone's log carried the login. The rest were documents: renumbering vs removal and what each deletes, the status words, which follow-up checks are per mailbox, where the id layout lives, the tag text, notes on removal, the start order, diagnostics naming only the first mailbox, `draft_to_mailbox` using the message's own login | code fixed with tests; documents corrected; the missing tests added (a reply closes only its own mailbox's task; an old database opens unchanged) |
| User-facing promises (Haiku) | 5: "up to five" was false after the first mailbox was removed (a code bug); the tag shows a name, not the address; sign-out also deletes the Claude API key; removal deletes more than the README said; saving a password reconnects every mailbox | code fixed; wording corrected in PRIVACY, README, CHANGELOG and the button text |

## Gate 3: sol-loop with Opus as the single reviewer (3 rounds)

Round 1 (ledger of the council and drift findings given up front, so the reviewer looked for new things):

| Id | Severity | Outcome |
| --- | --- | --- |
| R1 | should | **fixed**: a follow-up open for more than 60 days lost its record, so it lost its mailbox and survived the mailbox's removal; the clean-up now keeps a record while its task exists |
| R2 | should | **fixed by construction**: `remove_account` and `sign_out` left every thread stopped when they failed halfway; they now restart whatever happens (not unit-testable: the stop flag is private and a failure cannot be forced reliably) |
| R3 | nit | **fixed**: the digest and wake notices merged one message that reached two mailboxes into one conversation; the list shows two |
| R4 | nit | **fixed**: keeping the cursor on "the same conversation" could jump to the other mailbox's copy |
| R5 | nit | **fixed**: the demo backend did not reuse number 0 like the engine |
| R6 | nit | **fixed**: the first place accepted an empty address or server |

Round 2: R1 to R6 verified; one new finding, **fixed**: a follow-up task that an upgraded database holds without a record (the old
clean-up deleted them at 60 days) had no mailbox and survived removal; such a task now counts as the first mailbox's, the only
one that could make follow-ups before this change.

Round 3: **APPROVE**. The reviewer checked that `clear_mailbox(0)` cannot reach another mailbox's follow-up (a task and its record are written
and deleted together, `task_uid` is never null or shared), that the new `COALESCE` changes nothing for tasks that have records, that
sign-out needs no change, and found nothing further. Budget used: 3 of 3 rounds. The skill names OpenAI's GPT Sol; at the owner's
instruction Opus was used in its place.

## Mutation checks

Every guard in the store and in `core.rs` (id layout, per-mailbox counters, block ranges, outbox and follow-up scoping, correction
links, removal of corrections and follow-up records, keychain deletion on remove, the number a new mailbox takes, sign-out
clearing the extras, the settings guard on `more_accounts`, the id of what `on_triaged` queues, a mailbox with no password) was
broken one at a time and a test failed each time. The tests written after the reviews each failed before their fix.

## An incident worth recording

While adding the keychain test, one run was started from the repository's top folder, the case the Sonnet seat had warned about.
The engine's tests used the real Windows credential store and wrote two dummy entries (`imap-password`, `imap-password-1`),
possibly replacing an entry a person had saved. They were deleted by hand. The engine now recognises its own cargo test runs
(`secrets::in_memory`) wherever they are started, and a test pins it; `engine/.cargo/config.toml` is no longer needed and was removed.

## What these gates could not cover

Nothing was run against a live mailbox, so a real second login, real IMAP servers' number reuse, and Gmail's
`/mail/u/<address>/` link form are manual tests (`docs/TESTCASES.manual.md` MT-99 to MT-111). macOS and Linux keychains were not
exercised. The window was driven in a browser against the demo backend with two and three mailboxes (add, change a password,
remove, filter, Escape, Cancel during a test); the packaged app's own suite checks the state shape, the refusals and the phone's
view, because adding a mailbox there needs a reachable server. The stop-before-reset in `save_account` and the failure path of
`remove_account`/`sign_out` rest on construction and review, not on a race test.
