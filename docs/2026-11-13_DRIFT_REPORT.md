# Drift report, 2026-11-13

Documents checked against the code at commit `583abbe`, one document at a time, by two independent passes that were
given the documents and the source but none of the history behind them. Everything below was confirmed in the code
before it was changed. Items marked **fixed** were corrected the same day; the rest are recorded as accepted.

## docs/designs/phase2.md

| Drift | Resolution |
| --- | --- |
| "Bulk mail at priority 1 or 2 with no due date folds into one row" — the code also requires no star, no urgent flag and no draft anywhere in the conversation, and the UI is a button, not a row | **fixed** (document) |
| "Tasks due today or overdue sort as at least priority 4" — the code also requires someone to be waiting on an answer, so a newsletter cannot climb | **fixed** (document) |
| "The daily digest counts them" — the digest shows due tasks as their own figure, not folded into "high" | **fixed** (document) |
| "Learning applies only to the rules ranker" — it also applies when a model failed and the rules took over | **fixed** (document) |
| A new message wakes a snoozed conversation only if it passed its sender check; the document did not say so | **fixed** (document) |
| The test plan calls the learning and thread-key tests "unit tests on the store and triage code"; they live in `engine/tests/phase2.rs` | **fixed** (document) |
| `signals.rs` still said `replied_to` holds ids from "In-Reply-To or References" — the exact thing Phase 2 changed | **fixed** (code comment) |
| `prune` (done tasks 90 days, log 30, corrections 365) runs at every watcher start and no design document mentioned it | **fixed** (added to PSEUDOCODE §3) |

## docs/designs/phase3.md

| Drift | Resolution |
| --- | --- |
| "One attempt a day, an attempt not a success" — the day is recorded only when the server answers; a try that got no answer repeats in an hour | **fixed** (document) |
| Diagnostics "the mail server's host name" — only well-known providers are named; anything else reads "custom server" | **fixed** (document) |
| Two redaction rules were missing from the list: path-and-folder names become `<name>`, and a "connected as" line loses the login name | **fixed** (document) |
| The test plan described OAuth and AUTHENTICATE test infrastructure that does not exist, because Google sign-in was deferred | **fixed** (document) |

## docs/PSEUDOCODE.md

| Drift | Resolution |
| --- | --- |
| §11 repeated the wrong update-check rule (attempt versus answer, no mention of the hourly retry) | **fixed** |
| §11 diagnostics "server host name" | **fixed** |
| §9 digest line was the old format; the real one counts conversations, shows a "due" figure, hides "urgent" at zero, and excludes snoozed and left-the-inbox tasks | **fixed** |
| §4 unsubscribe limit said 500 characters; the code allows 2,000 and also requires a real public host | **fixed** |
| §2 showed no maintenance step; the code runs one after the first sync, after a mailbox change at most every 20 s, and every 120 s when idle | **fixed** |
| §3 omitted the pruning that runs at watcher start | **fixed** |
| §1 did not mention that the scheduler also runs the update check | **fixed** |

## README.md

| Drift | Resolution |
| --- | --- |
| Shortcuts missing from the list: `e` (done), the arrow keys, Enter and Space (open), `,` (settings) | **fixed** |
| `r` does nothing on tasks you added yourself; the list implied otherwise | **fixed** |
| Shortcuts did not work with Caps Lock or Shift held, while the in-app card shows capitals | **fixed in the code**: single-character keys are lowercased before matching |

## PRIVACY.md

These are promises a user could rely on, so they are listed in full.

| Drift | Resolution |
| --- | --- |
| The storage list omitted corrections (sender, subject, priorities, kept 365 days), sender preferences and queued mailbox changes | **fixed** |
| The 3,000-character body was described as "used only to write replies"; it is also shown on demand by "Read message" | **fixed** |
| Anthropic ranking also sends the sender names and subjects of the ten most recent corrected messages | **fixed** |
| ntfy also carries the daily digest and "back from snooze" notices, at any priority, not only high-priority mail | **fixed** |
| Switching write-back on applies the changes made while it was off; the wizard said "leave off to keep your mailbox untouched" | **fixed** in both the document and the wizard text |
| Sign-out deletes more than was stated (drafts, log, corrections, sender choices, backup, crash log) and keeps the ntfy topic and AI choice | **fixed** |

## SECURITY.md

| Drift | Resolution |
| --- | --- |
| "Hub commands limited to four" — there are eleven | **fixed** |
| A changed `UIDVALIDITY` was described as resetting the cursor; it deletes the stored tasks, drafts and queued changes | **fixed**, and the test that asserted this under the name `..._so_new_mail_is_not_lost` was renamed to say what it checks |
| All 26 named regression tests were confirmed to exist, and a sample of fix rows was confirmed in the code | no drift |

## RELEASING.md

| Drift | Resolution |
| --- | --- |
| Said to set `repository` in "the two Cargo.toml files and package.json"; only `app/src-tauri/Cargo.toml` has the field | **fixed** |
| Scripts, workflows, jobs, pinned action SHAs and `UPDATE_REPO` all confirmed to exist | no drift |

## CHANGELOG.md

| Drift | Resolution |
| --- | --- |
| "Priority and category labels" are Gmail-only; other servers get the archive move and the flag | **fixed** |
| "Priority 5 for an overdue question" — the rule is due today, tomorrow or past | **fixed** |

## What this pass did not cover

The macOS and Linux builds, the installer, and any behaviour against a real mail server. A document can only be checked
against the code, and the code has still never spoken to a live mailbox.
