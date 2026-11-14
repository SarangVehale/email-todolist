# Phase 6: several mailboxes in one list

* **Date:** 2026-11-13
* **Status:** design for Run 3 of the agreed roadmap (see `docs/designs/phase5.md` for Run 2). Written before the code; revised
  once, the same day, when the first draft's 64-bit ids turned out to need a change in every place the UI compares an id.

## Problem

Most people have more than one mailbox: a personal one, a work one, an old one that still receives bills. Until now the app read
exactly one. Everything assumed it: the settings held one account, the keychain one password, the database numbered tasks by
the IMAP UID (which is only unique inside one mailbox), and one watcher thread fed one list.

## Constraints that do not change

No paid service, nothing sent, data local, app passwords only. Rules, sender preferences, your own tasks, reminders, the search
index and every other setting stay one set for the whole app: they are about you, not about a mailbox. The phone hub keeps its
small command set and never sees mailbox names or addresses.

## Design

### 1. A task's id says which mailbox it came from

A task is identified everywhere (the list, the commands, the phone) by one `u32`. The id space is cut into blocks:

```text
MAILBOX_SPAN = 400,000,000
task id of IMAP message `uid` in mailbox n  =  n * MAILBOX_SPAN + uid          (n = 0 to 4; mailbox 0 is the first)
tasks you add yourself and follow-ups       =  LOCAL_UID_BASE (2,000,000,000) and up
```

Five blocks end exactly at `LOCAL_UID_BASE`, so a mail task never collides with one of yours. Mailbox 0 keeps the raw IMAP UID,
so **an existing database needs no migration and every id the UI or a phone already holds stays valid**. A UID of 400,000,000 or
more (IMAP UIDs are 32-bit; real mailboxes are nowhere near this) is skipped with a line in the log, never mapped into a
neighbour's block; the cursor moves past it so it is not asked for again.

The layout lives in `store.rs` (`global_uid`, `mailbox_of`, `imap_uid`, the block bounds, and the check that turns an IMAP number into
a task id) and in the sync's size check. A task also carries `mailbox`
(the mailbox of a mail task, the mailbox that sent a followed-up message, or none for a task you added), so the list never does
arithmetic on ids.

Write-back finds a message by its Message-ID, never by UID, so it needed no change except which queue it drains.

### 2. A store that knows its mailbox

`Store::open` and `Store::memory` return an **unscoped** store: it writes and resets as mailbox 0 (every existing caller and test
is unchanged) and reads the whole list. `store.scoped(n)` returns a store for one mailbox's sync and write-back: the IMAP numbers
the sync speaks are turned into task ids on the way in and back on the way out, and these are per mailbox:

* the cursor and its companions (`last_uid`, UIDVALIDITY, in-flight and no-content counters), kept under meta keys with an `@n`
  suffix; mailbox 0 keeps the plain keys;
* which tasks the "left the inbox" and "you replied" checks look at (`open_mail_tasks`, `set_gone`, `auto_close`);
* which outbox items a write-back loop drains (`pending`), and which follow-ups are checked (`open_followups`).

A renumbered mailbox (`reconcile_uid_validity`) deletes **only that mailbox's** tasks, drafts, outbox items and counters; the
corrections made on its messages are kept (what was learned about senders still holds) but unlinked, parked on the first number of
that mailbox's block, so they belong to it and to no other. A removed mailbox (`remove_mailbox`), and a mailbox whose server or
address changed, lose all of that and also its corrections, its follow-up tasks and its follow-up records (each record carries
the mailbox it was made in, and is kept while its task exists, so even a follow-up left open for months is found when its mailbox goes; a follow-up from before this change whose record the old clean-up had already deleted is the first mailbox's, the only one that could make them then). Never the other mailboxes', your own tasks, your rules, your sender choices or the log. (The old reset also deleted every `meta` row,
which erased the update-check date; it now deletes only the per-mailbox keys.)

The first mailbox's data is reset when its server or address changes, exactly as before, now through the same code.

### 3. Settings, the keychain and the limit

`settings.json` keeps `account` as mailbox 0 and gains `more_accounts` (up to four, ids 1 to 4). A file from the previous version
has no such list and loads unchanged. Validation refuses more than four extras, ids repeated or out of range, and the same
server and address twice, the first mailbox included, however the server is written (letter case, a trailing dot, or
`googlemail.com` for `gmail.com`).

The password of mailbox *n* is the keychain entry `imap-password-<n>`; mailbox 0 keeps `imap-password`. The engine's own tests always run on an in-memory keychain (`secrets::in_memory`: cargo's variables or the test binary's folder
recognise them, wherever cargo was started; `INBOX_AGENT_KEYCHAIN=memory` asks for it anywhere), so a test can never read, overwrite
or delete the passwords of the person running it.

### 4. One watcher and one write-back loop per mailbox

`Core::start` starts, for every mailbox that has its password, a sync thread and a write-back thread, each with its own scoped
store, its own status and the shared stop flag; a mailbox without a password (or whose keychain entry cannot be read) gets only
its status ("no password", or the keychain's own error) and the others carry on. The Sent index, the server's search for replies,
the cap of 20 open follow-ups, UIDVALIDITY, the cursor and the Gmail-label rule are per mailbox (a mailbox is Gmail or it is
not); the check of mail already read, which closes a follow-up whose reply is stored, looks at every mailbox, since a reply can
reach another of your addresses. A sync that has been stopped (a restart, or its mailbox removed) reports no more status and
does not move its cursor, so a message that was being ranked when the stop came is read next time instead of being skipped. The AI choice and key, alerts, quiet hours, the digest, rules, sender choices, the write-back
switch and the phone hub are global.

The status shown is the worst of the mailboxes: an error, else "no password", else "connecting", else "syncing", else anything
else, else "idle". Before any mailbox has reported it is "starting"; with setup unfinished it is "setup"; finished with no mailbox
left, "no password". The dot's tooltip lists every mailbox's own status.

### 5. Commands (desktop only; the phone is refused)

* `add_account { host, port, user, password }`: needs setup done and a first mailbox; the password must not be empty; takes the
  lowest free number from 0 to 4 (0 is the first mailbox's place, free again after it was removed, and then holds the new mailbox in `account`; "You can read at most five mailboxes at once" when none is free); validated like every
  settings change; clears whatever an earlier mailbox of that number left behind; stores the password; restarts the threads of
  every mailbox (`start()`); returns its number. The window tests the connection
  (`test_account`) first and only then enables **Add mailbox**.
* `remove_account { id }`: stops the sync, deletes that mailbox's block as above, deletes its keychain entry and its settings, and
  starts the others again. Nothing in the mailbox itself is touched. When the last mailbox goes, setup is asked for again; notes
  you wrote on a mail task go with that task; your rules, sender choices and own tasks (and their notes) stay. The window asks for a second click and offers it only when there is more than one
  mailbox (with one, "Sign out and reset" is the way).
* `save_account { id?, host, port, user, password }`: a new password, or a new server or address, for mailbox `id` (the first when
  absent). Other mailboxes keep their data. When the server or address changes, every sync is stopped first, so none can write the old
  mailbox's mail back after its data is reset, and they are started again whatever happens next. It now restarts the syncs (every mailbox's) when setup is done, so a pasted new password takes
  effect at once (before, it did so only after a restart).
* `save_settings` refuses to change `more_accounts`, like `account` before it: only `save_account`, `add_account` and
  `remove_account` change a mailbox, because they also take care of its data and password.
* "Sign out and reset" wipes every mailbox, every password, and returns to setup.
* `state` carries `accounts: [{ id, user, host, gmail, status }]` and the settings' `more_accounts` for the desktop. The phone's
  state has neither, and its task list shows no tags. The sync log it receives has the login removed from its
  "connected as <login>" lines (the log is the one place an address was written).
* `draft_to_mailbox` saves a draft under the login of the mailbox the message is in. Diagnostics name the first mailbox's server only.

### 6. The list

With one mailbox nothing changes on screen. With more:

* each mail task shows a small tag with the mailbox's name (the part of the address before `@`; `name@` and the first part of the domain when two
  mailboxes would read the same; at most 16 characters; the full address on hover);
* the filter bar gets one chip per mailbox, after Reply and Drafts, with a count; choosing one shows only that mailbox's tasks
  (your own tasks belong to no mailbox and show under "All"); "All" clears it; a mailbox that is removed while chosen clears itself;
* **conversations stay inside a mailbox**: a message that reached two of your addresses is two tasks, each acted on in its own
  mailbox, never merged into one row that would hide one of them, and news in one never wakes the other's snooze;
* "Copy Gmail link" appears for tasks of a Gmail mailbox only, and with more than one mailbox opens the one for that address
  (`/mail/u/<address>/`) instead of the browser's first signed-in account.

Settings lists the mailboxes (address, server, how each is doing) each with its own "New app password" and, with more than one,
**Remove**, and an **Add another mailbox** button that opens a form with the same provider help as the first-run wizard.
The first-run wizard is unchanged.

## Out of scope

Rules, write-back or AI settings per mailbox; Microsoft or Google sign-in; reading folders other than the inbox; moving mail between
mailboxes; recognising the same person across mailboxes; more than five mailboxes.

## Test plan

Each item has a failing test first (`engine/tests/phase6.rs`, `engine/tests/imap_flows.rs`).

* **Ids:** the layout helpers; the same IMAP number in two mailboxes is two tasks; an existing database is unchanged; a UID past the
  layout is refused by the store and skipped, with a log line, by the sync; every task, follow-up and task of your own reports
  the right `mailbox`.
* **Scoping:** cursor, UIDVALIDITY and counters per mailbox; a renumbered mailbox deletes only its own data (your own tasks, your
  rules, the other mailbox and the update-check date intact); outbox routing; follow-ups per mailbox; the whole-list readers still
  see every mailbox; a store opened by many threads at once on a new database (a migration race this work exposed).
* **Settings:** a file from before loads as one mailbox; up to four extras; too many, duplicate or badly numbered lists refused;
  passwords never in state or in `settings.json`; one keychain entry per mailbox.
* **Commands:** add, list, remove (everything of that mailbox gone, nothing else); the numbers taken and freed; refusals (nothing set
  up, empty password, a sixth mailbox, a duplicate); removing the first with others left, and the last; changing the first mailbox
  leaves the others; one mailbox's password changed without touching another's; sign-out removes all; the phone refused.
* **Sync:** two scripted IMAP servers feeding one store; write-back reaching only its own server; replies and departures judged in
  their own mailbox; the worst-of status.
* **Found by the final Opus review:** a follow-up open for more than 60 days kept no mailbox and survived its mailbox's removal; the
  digest and wake notices counted a conversation once across mailboxes; the first place needed no address; `remove_account` and
  `sign_out` left every thread stopped when they failed halfway (restart whatever happens, by construction like `save_account`).
* **Found by the reviews (each failed first, except the stop-before-reset in `save_account`, which cannot be raced reliably in a test
  and mirrors `remove_account`):** a snooze is woken only by news in its own mailbox; the same server written another way is a duplicate;
  the engine's tests use the in-memory keychain even when cargo is started from the top folder; adding a mailbox does not log a removal; a follow-up record of an already-answered message belongs to its own mailbox; an
  unlinked correction is not deleted with another mailbox; the first mailbox's place is taken again by the next one added, so five
  can always be read; the phone's log carries no login; a stop while the last message of a batch is ranked leaves the cursor
  before it (this also fixes a message skipped for good after any restart, which predates this run); a new mailbox starts clean even
  if a stopped sync left a cursor or a task in its number; a reply sent from one mailbox closes only that mailbox's task.
* **Mutation checks:** every guard above broken one at a time and caught by a test.
* **The window:** exercised in the browser against the demo backend with two mailboxes (tags, chips, add, change password, remove,
  filter cleared when its mailbox goes) and in the packaged app against the real engine.
