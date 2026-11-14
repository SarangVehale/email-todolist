# Phase 7: backup, export and restore

* **Date:** 2026-11-14
* **Status:** design for Run 4 of the agreed roadmap (see `docs/designs/phase6.md` for Run 3). Written before the code, and
  corrected after its reviews (`docs/2026-11-14_PHASE7_REVIEW.md`): the first draft promised more than the code could keep in a few
  places, and those promises are now exact.

## Problem

Everything you have told the app lives in one database and one settings file on one computer: your rules, the senders you
marked important or muted, the corrections that taught the ranker, the tasks you added, their reminders and repeats, your notes
on mail, reply drafts you edited, which mail you finished, starred or snoozed, and your options. A new computer, a reinstall, or
"Sign out and reset" loses all of it. The mail itself is safe in the mailbox; the work you did on top of it is not.

## Constraints that do not change

No paid service, nothing sent, data local, no network use. The phone hub keeps its small command set: both new commands are
handled in `core.rs` and are not in `commands::PUBLIC`, so the hub refuses them. Passwords and keys never leave the keychain, so a
backup cannot contain one.

## What a backup holds, and what it never holds

One JSON file, `inbox-agent-backup-<date>-<time>.json`, written to Downloads (your user folder if there is none), never over an
existing file, never opened by the app, readable by you only where the system has such a thing. It is **not encrypted**, like the
database it comes from: keep it as you would keep a private document.

| Holds | Notes |
| --- | --- |
| Your rules | |
| Sender choices ("always important", "mute") | up to 5,000 |
| Corrections of the ranking: sender name, subject, address, old and new priority, date | the one place a mail's sender and subject appear; the newest 2,000 |
| The tasks you added: title, priority, due, reminder, repeat and its anchor, done or open, star, note | open ones first, up to 2,000 |
| For mail, only what *you* did, keyed by Message-ID and the mailbox: done, starred, snoozed until, your note, **a reply draft you edited** | no subject, sender, excerpt or text of any mail; the newest 5,000 |
| The mailboxes (server, port, address) | to set them up again |
| A short list of options (below) | |

**Never in the file:** a password or API key; the phone hub's access code, port or on/off state; the ntfy topic and server; the
Ollama address; "Let Inbox Agent file mail"; whether setup is done; the activity log; the text of any message; reply drafts written
by the AI or from a template (they can paraphrase the message, and are quick to make again); follow-up tasks (they are made again
from your Sent folder); the outbox of changes waiting for the mailbox.

Options in the file (an allow-list, so a new setting is never exported by accident): ranking mode and the Ollama model name,
desktop notifications, alert categories, quiet hours, digest time, reply-draft options (automatic, tone, signature, minimum
priority, daily limit), the follow-up wait, the update-check switch. A test pins the exact list.

**Limits.** A file holds at most 50 rules, 5,000 sender choices, 2,000 corrections, 2,000 tasks of yours, 5,000 entries about mail and
5 mailboxes, and at most 8 MB. When you have more than a file can hold, the backup says how many it left out. Restoring refuses a
file over any limit.

## Restore

`backup_import { json, apply, settings }`, desktop only, and only once setup is finished (the whole restore is refused before then).
The window reads the chosen file (up to 8 MB), asks for a preview (`apply: false`: nothing is written, the answer says what would
happen, including each new rule in words and each mailbox it would add with its server), shows it, and applies on a second click.

**Restore adds what is missing and never removes anything.** It does not overwrite what you have, with these named exceptions:
ticked options take the file's values; a snooze in the file that is later than yours replaces yours; and choosing a sender
"important" or "mute" does what choosing it yourself does to that sender's open mail (priority at least 4, or priority 1). The same
file restored twice changes nothing the second time.

* Rules: added after yours unless one with the same field, text and outcome is there; new ids; the 50-rule limit is respected (the
  rest are counted as skipped). A rule can file mail away as it arrives, so the preview spells every new rule out.
* Sender choices: added where the sender has none. Corrections: added unless the same one is there (compared in the form a backup
  writes it); they are filed under no mailbox, so removing a mailbox does not take what you taught the ranker.
* Your tasks: added unless a task with the same title and due date is there (a repeating one: the same title, repeat and anchor),
  unless it has another reminder still to come. A reminder that has already passed is not restored (it would announce itself at
  once); a repeating task moves to its next time and its due date moves with it; a finished task keeps neither reminder nor repeat.
* Mail: for each entry the file names a mailbox by server and address. If that mailbox is set up here the state is applied to the
  task with that Message-ID when it exists, and **parked** when it does not yet, to be applied the moment that message is read.
  Applying is monotone: finished stays finished (and is not also snoozed), a star stays, a snooze keeps the later time, your existing
  note and draft win. Nothing is queued for the mailbox (restoring "done" does not archive anything). Entries for a mailbox that is
  not set up here are counted and skipped; add the mailbox and restore again. Parked state expires after 90 days, is limited to 5,000
  entries, and goes with its mailbox and with sign-out.
* Options and mailboxes (only when "Also restore options and mailboxes" is ticked, off by default): the allow-listed options are
  applied through the normal settings check, **except that the paid cloud ranking ("Claude with your own key") is never restored**:
  choosing it is for you, in Settings, and the preview says so. Mailboxes that are not set up are added **without a password**, up to
  the limit of five, and the preview lists each with its server, because a restored mailbox's server comes from the file and you will
  be asked for its app password: restore only a file you made yourself. A new mailbox starts from nothing, like one added by hand: its number is cleared of anything an earlier mailbox left, and any password still in the keychain under that number is deleted, so it can never sign in to the file's server with another server's password. The
  first mailbox is never changed (an emptied first place can be taken by a restored mailbox).

**The file is untrusted input.** It is size-limited before parsing, refused when it is not this app's format or from a newer
version, refused when any list exceeds its limit, and every value goes through the checks that the same thing would have to pass
if typed in (rule validation, title and note lengths and control characters, dates, reminders, priorities, draft length, a snooze
no further ahead than one can be typed, settings validation, a correction no later than now). One bad value refuses the whole
restore: the database part runs in one transaction, and a late failure rolls it back. A hostile file cannot turn on the phone hub,
write-back or the paid ranking, change where notifications go, or reach another mailbox's data, because those fields are not read
(or, for the ranking, not accepted). The one network name a file can supply is a mailbox's server, which is why the preview shows it.

## Design

* New module `backup.rs`: the file model, `build` (export), `parse` (validate), `import` (plan and apply), `export_in(core, dir)`.
* New `store/transfer.rs` (a child module of `store`, so it can use the connection): reading what goes in the file, and one
  transaction that applies a parsed file. A dry run is the same code with the transaction rolled back, so the preview cannot
  disagree with the restore.
* New table `restored_state(mailbox, message_id, state, at)`, at most 5,000 rows, read by `Store::add` (which applies the state to
  a newly read message and deletes the row), pruned at 90 days, cleared with its mailbox and by sign-out. New indexes on
  `tasks(message_id)` and `tasks(addr)`, so a restore of the biggest file takes well under a second instead of a minute.
* The restore takes the database's write lock at the start (and waits for it), so a sync that commits in the middle cannot make it fail
  with "database is locked". Corrections are ranked by their date, not by the order they were stored in, so an old backup never
  outweighs what you have taught the ranker since.
* Times in a backup read back exactly (`serde_json` with `float_roundtrip`), and comparisons that matter use a tolerance, so a
  restore on the computer a backup came from finds everything already there.
* Commands `backup_export` (returns the path, what was saved and what was left out) and `backup_import`.
* Window: Settings, "Backup": **Save a backup file** and **Restore from a backup file**, with a preview dialog. A restore is not
  started while rule edits are unsaved, and the rule editor reads the rules again afterwards. A restore in progress cannot be cancelled.
* A message that arrives already finished (restored as done) gets no automatic reply draft.

## Version history and release history

Run 4 also sets up the project's history documents: `CHANGELOG.md` (what changed for users, per version), `docs/VERSION_HISTORY.md`
(each development milestone: commit, date, tests, review record, and the rule for what bumps the version) and
`docs/RELEASE_HISTORY.md` (what was actually published: tag, date, platforms, signed or not, checksums, manual-test result; empty
until the first release). `scripts/bump_version.py --check <version>` (run by the release workflow) now also refuses a tag whose
changelog heading is not dated or that has no row in the release history. `scripts/test_bump_version.py` tests those rules.

## Out of scope

Encrypting the file; scheduled or automatic backups; restoring message text, follow-up tasks or the activity log; moving the
database file itself; syncing between computers.

## Test plan

Each item has a failing test first (`engine/tests/phase7.rs`).

* **What is in the file:** every kind of data above round-trips; none of the "never" list appears, checked by planting a recognisable
  value in each (a follow-up, an AI draft, a template draft included) and searching the file; no mail's subject, sender, excerpt or
  text appears except a correction's; the exact allow-list of options is pinned; the file names a version and format.
* **Restore is additive and idempotent:** nothing existing is deleted or replaced (apart from the named exceptions); restoring twice
  adds nothing, including on the computer the file came from, for a finished task that kept a reminder, for corrections with very
  long subjects, and for a repeating task that moved on; rules respect the limit; duplicates are skipped; a past reminder is dropped
  and a repeat moves on with its due date.
* **Preview equals result:** a dry run writes nothing and reports the counts the real run then produces.
* **Mail state:** applied to an existing task; parked for a message not yet read and applied when it arrives, once; monotone merge;
  finished mail is not snoozed; nothing queued for the mailbox; a mailbox that is not set up is counted and skipped; parked rows
  expire, go with their mailbox and with sign-out; the 5,000-row bound.
* **Hostile files:** not JSON, another format, a newer version, too large (also as valid JSON), too many items, a bad rule, bad dates,
  control characters, oversized strings everywhere, a snooze years away, a correction from the future, ids that collide, a fake `hub`,
  `writeback`, `ntfy_server`, `ollama_url`, `onboarded`, `account` and `more_accounts` in the options, the paid ranking mode: each
  refused or ignored, whether or not options were asked for, and a bad item in the middle changes nothing (atomic, with a real
  database failure forced by a trigger).
* **Options and mailboxes:** the allow-list only; mailboxes added without a password (a password left in the keychain under their number is deleted, never used), listed in the
  preview with their servers; the limit and duplicates; a new mailbox starts clean, with no password, even if the keychain still holds one under its number; refused before setup.
* **Limits at their edge:** a file with exactly 50 rules, 5,000 sender choices, 2,000 corrections, 2,000 tasks, 5,000 entries about
  mail and 5 mailboxes is accepted; one more of any is refused.
* **Speed and contention:** a preview and a restore of the biggest file over a database of 30,000 tasks; a restore started while another
  connection holds the write lock waits and then succeeds.
* **The file:** created new, never over another, the name; what had to be left out is reported; the phone refuses both commands.
* **History:** `scripts/test_bump_version.py`: a release without a row, with an undated heading, with a dated heading and a row.
* **The window:** exercised in the browser against the demo backend (preview, refusals, Escape, rule editor, unsaved rules) and in the
  packaged app (the file chooser is driven with the browser protocol).
