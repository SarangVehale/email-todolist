# Phase 11: close the gaps the documentation and the first CI run found

> **Status:** current (built in Run 8) · **Audience:** maintainers · **Last reviewed:** 2026-11-14

Not a feature. Writing the documentation (Phase 10) found places where the engine took a number or a string it should have
refused, and the first push to GitHub started CI for the first time and found what no run on Windows had: a workflow GitHub could
not read, a race in the database layer, and tests that depended on how fast a machine is. This phase closes them, test first.

## What was found, and what changes

| Found by | Gap | Change |
| --- | --- | --- |
| Documentation | A task number, a priority and a mail server port were cut to 32, 8 and 16 bits (`as u32`, `as u8`, `as u16`): task 2^32 + n acted on task n, priority 261 was read as 5, port 66529 as 993 | `try_from`: a number that does not fit names no task, no priority and no port; ports outside 1 to 65535 are refused with a message |
| Documentation | The snooze error said "from tomorrow" while today is accepted before 09:00 | The message says what is accepted |
| Documentation | `save_settings` accepted any ranking mode and let a request set the phone hub's access code to any string | The mode must be rules, ollama or anthropic; the access code can only be made by the app (`rotate_hub_token`, or switching the hub on) |
| This phase's own review | With the mode checked, a settings file with a mode that is none of the three (hand-edited) would make every later save fail, even a password change | A saved mode that is not one of the three reads as "rules" |
| Documentation | `detect_ollama` and `test_account` took whatever they were given | They check the address (not empty, no control character, at most 320 bytes), the host name and the password before doing anything, as saving a mailbox does |
| Documentation | `save_account` stored an empty password (which reads as "needs a new app password") and, for the first mailbox, an empty server or address; `finish_setup` finished setup with no mailbox, no address or no password in the keychain | All refuse; a keychain that cannot be read gives its own error, not "add your first mailbox" |
| Documentation | The diagnostics report named a "Google sign-in" option from a feature that was never built | Removed |
| First CI run (Linux) | `database is locked` (code 517) when a sync committed between a write transaction's first read and its first write; the busy timeout cannot help a deferred transaction that has read | Write transactions that may read first take the write lock at once (`BEGIN IMMEDIATE`): `set_rules`, the follow-up task, `clear_mailbox`, `delete_mail_rows` (the restore already did) |
| First CI run (macOS) | The hub tests paused 600 ms and then connected; the hub was not up yet | They wait until it answers |
| First CI run (Linux) | A test expected "no password" to outrank an error; the first mailbox's refused connection is an error at once on Linux, a moment later on Windows | The test accepts the real order (an error outranks "no password"; `worst_status`) |
| First CI run | `ci.yml` was not valid YAML (a step name with a colon and a space), so GitHub refused it | Quoted (already fixed); `scripts/check_docs.py` checks workflow files; the test steps put failing test names into annotations, because a job log needs a sign-in to read |

## Out of scope

`test_account` still refused a mailbox with no Sent folder when this was written (the sync itself copes with none; Phase 12 changed it: the test passes and says so); the flaky behaviour of anything that
was not seen in CI; macOS and Linux runs of the window's tests beyond what CI shows; changing what the window does with the new
messages (it shows an engine message as it is).

## Test plan

`engine/tests/phase11.rs`: one test for each row of the first group, red before the fix, and each new guard put back to its old
behaviour once (23 mutants over two rounds, all caught). The database change cannot be shown by a test that is not timing-dependent: it is checked by
the Phase 6 tests passing on Linux in CI, and the hub and status fixes by CI on all three systems. The first CI run, and the one
after the fixes, are recorded in the review.
