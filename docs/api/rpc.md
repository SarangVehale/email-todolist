# Engine command reference

> **Status:** current · **Audience:** developers who change the window, the engine or the phone hub · **Last reviewed:** 2026-11-14

Every command the engine answers, with its arguments, its answer, its errors and what it changes. The window calls them
through `rpc(cmd, args)` (see [the overview](README.md)); the code is `Core::rpc` in
[`engine/src/core.rs`](../../engine/src/core.rs) and `commands::run` in [`engine/src/commands.rs`](../../engine/src/commands.rs).

## Contents

* [Conventions](#conventions)
* [All commands](#all-commands)
* [Reading the list](#reading-the-list): `state`
* [Changing a task](#changing-a-task): `set_done`, `set_star`, `set_priority`, `set_snooze`, `set_due`, `set_title`, `set_note`, `set_reminder`, `add_task`, `set_pref`
* [Reply drafts](#reply-drafts): `draft_generate`, `draft_update`, `draft_discard`, `draft_to_mailbox`
* [Message text, search and files](#message-text-search-and-files): `read_body`, `search`, `save_event`, `export_markdown`
* [Your rules](#your-rules): `rules_save`, `rules_apply`
* [Mailboxes and setup](#mailboxes-and-setup): `test_account`, `save_account`, `add_account`, `remove_account`, `finish_setup`, `sign_out`
* [Settings, keys and ranking](#settings-keys-and-ranking): `save_settings`, `set_secret`, `detect_ollama`, `test_ai`
* [Phone hub](#phone-hub): `hub_info`, `rotate_hub_token`
* [Backup and support](#backup-and-support): `backup_export`, `backup_import`, `diagnostics`

## Conventions

* **Arguments** are one JSON object. A missing or wrongly typed argument is read as its default (an empty string, `0`,
  `false`, or the default named below); it is never an error by itself. Strings for accounts (`host`, `user`, `password`,
  `url`, `name`, `value`) are trimmed; a password also loses its spaces (app passwords are often shown in groups of four).
* **`uid`** is a task id (an unsigned integer). A mail task's id is `mailbox × 400,000,000 + IMAP UID`; a task you added, or a
  follow-up, is 2,000,000,000 or above (`store::LOCAL_UID_BASE`). A missing `uid` is 0, which is no task.
* **Answers.** Many task commands answer `true`, or `false` when the task does not exist or the value was refused (no error
  text). The window treats `false` as success, so it relies on the next `state` poll to show the real value.
* **Errors** are plain strings. Over Tauri the promise rejects with the text; over the hub the answer is
  `{"ok": false, "error": "..."}` with status 400. Any command can also fail with a database error (for example when the
  database is locked or damaged); those texts come from SQLite and are not listed.
* **Write-back.** Some commands queue a change for the mailbox (archive, flag, label, save a draft). The queue is filled
  whether or not "Let Inbox Agent file mail" (`writeback`) is on; it is sent to the mail server only while it is on, by
  the write-back thread of the task's mailbox. Tasks you added never queue anything. `state.pending` counts the queue.
* **Hub**: whether the phone hub allows the command (`commands::PUBLIC`, plus `state`). Every other command answers
  `not available from the hub` there.

## All commands

| Command | Area | Hub | Answer |
| --- | --- | --- | --- |
| [`state`](#state) | Reading | yes (reduced) | object |
| [`set_done`](#set_done) | Task | yes | boolean |
| [`set_star`](#set_star) | Task | yes | boolean |
| [`set_priority`](#set_priority) | Task | yes | boolean |
| [`set_snooze`](#set_snooze) | Task | yes | boolean |
| [`set_due`](#set_due) | Task | yes | boolean |
| [`set_title`](#set_title) | Task | yes | boolean |
| [`set_note`](#set_note) | Task | yes | boolean |
| [`set_reminder`](#set_reminder) | Task | yes | `true` |
| [`add_task`](#add_task) | Task | yes | task id |
| [`set_pref`](#set_pref) | Task | yes | boolean |
| [`draft_generate`](#draft_generate) | Drafts | no | object |
| [`draft_update`](#draft_update) | Drafts | yes | `true` |
| [`draft_discard`](#draft_discard) | Drafts | yes | boolean |
| [`draft_to_mailbox`](#draft_to_mailbox) | Drafts | no | `true` |
| [`read_body`](#read_body) | Text and files | no | string |
| [`search`](#search) | Text and files | no | array of ids |
| [`save_event`](#save_event) | Text and files | no | file path |
| [`export_markdown`](#export_markdown) | Text and files | no | string |
| [`rules_save`](#rules_save) | Rules | no | `true` |
| [`rules_apply`](#rules_apply) | Rules | no | object |
| [`test_account`](#test_account) | Mailboxes | no | object |
| [`save_account`](#save_account) | Mailboxes | no | `true` |
| [`add_account`](#add_account) | Mailboxes | no | mailbox number |
| [`remove_account`](#remove_account) | Mailboxes | no | `true` |
| [`finish_setup`](#finish_setup) | Mailboxes | no | `true` |
| [`sign_out`](#sign_out) | Mailboxes | no | `true` |
| [`save_settings`](#save_settings) | Settings | no | settings object |
| [`set_secret`](#set_secret) | Settings | no | `true` |
| [`detect_ollama`](#detect_ollama) | Settings | no | array of names |
| [`test_ai`](#test_ai) | Settings | no | object |
| [`hub_info`](#hub_info) | Hub | no | object |
| [`rotate_hub_token`](#rotate_hub_token) | Hub | no | `true` |
| [`backup_export`](#backup_export) | Backup | no | object |
| [`backup_import`](#backup_import) | Backup | no | object |
| [`diagnostics`](#diagnostics) | Support | no | string |

Every command is called by the window (`app/ui/main.js`, `drafts.js`, `setup.js`) and answered by the demo backend
(`app/ui/mock.js`). There is no command the window calls that the engine lacks, and none the engine has that the window
never calls.

## Reading the list

### `state`

Everything the window draws: settings, tasks, log, status. Polled every 3 seconds while the window is visible.

* **Arguments:** `done` (integer, optional): how many finished tasks to include, newest first. Clamped to 200…20,000; a
  value that is not a non-negative integer means 200.
* **Answer:** an object; every field is described in [the state shape](../reference/state-shape.md).
* **Errors:** database errors only.
* **Side effects:** none. Reads the settings file, the database, and the keychain (whether an Anthropic key exists).
* **Hub:** yes, with a reduced answer: `settings` holds only `onboarded` and `writeback`; there is no `rules` or
  `accounts`; `has_anthropic_key` is `false`; `update` is `null`; a log line `connected as <login>; ...` loses the login.

## Changing a task

### `set_done`

Marks a task done, or reopens it.

* **Arguments:** `uid` (integer, required); `done` (boolean, optional, default `true`).
* **Answer:** `true`; `false` when there is no such task (or a repeating task has no date to move from).
* **Side effects:** sets the status and the finish time, clears any snooze. A repeating task you added is not finished:
  it moves to its next reminder or due date and stays open. Reopening a task the app closed itself (you had replied)
  marks it to stay open and queues nothing; reopening a follow-up also keeps it open. Otherwise queues `archive` or
  `unarchive` for the mailbox. Writes a log line.
* **Hub:** yes.

### `set_star`

* **Arguments:** `uid` (integer, required); `star` (boolean, optional, default `true`).
* **Answer:** `true`; `false` when there is no such task.
* **Side effects:** sets the star, queues `star` or `unstar` (a flag on the message), writes a log line.
* **Hub:** yes.

### `set_priority`

Changes a task's priority; this is a correction the ranker learns from.

* **Arguments:** `uid` (integer, required); `priority` (integer 1 to 5, required).
* **Answer:** `true` (also when the priority is already that value, which changes nothing); `false` when there is no such
  task or the priority is outside 1 to 5. The value is cut to a byte before the check, so 260 is read as 4.
* **Side effects:** sets the priority. Records one correction per task, keyed by the sender's address: moving it again
  updates that correction, moving it back to where it started deletes it, and mail that failed its sender check or has no
  address records none. Queues a label `AI/P<n>`, writes a log line, and stars the task at 4 or 5 if it was not starred
  (which queues `star`).
* **Hub:** yes.

### `set_snooze`

Hides a task until a time, or brings it back.

* **Arguments:** `uid` (integer, required), and one of:
  * `date` (string `YYYY-MM-DD`): wake at 09:00 local time that day. Checked first when present.
  * `preset` (string): `3h` (three hours from now), `tomorrow` (09:00 tomorrow), `nextweek` (09:00 next Monday),
    `clear` (not snoozed).
* **Answer:** `true`; `false` when there is no such task.
* **Errors:** `Pick a day from tomorrow to a year ahead` (not a real date between 2000 and 2100, more than 366 days ahead,
  or 09:00 that day has passed; a date of today before 09:00 is accepted); `unknown snooze option`.
* **Side effects:** sets `snooze_until`, writes a log line. Nothing reaches the mailbox. The scheduler wakes the task and
  sends a "Back from snooze" notice.
* **Hub:** yes.

### `set_due`

* **Arguments:** `uid` (integer, required); `due` (string, required): `YYYY-MM-DD` between 2000 and 2100, or `""` to clear.
  A missing `due` is refused.
* **Answer:** `true`; `false` when there is no such task or the date is not valid.
* **Side effects:** sets the due date of any task. For a repeating task you added with no reminder time, the repeat follows
  the new date, and clearing the date ends the repeat. No log line, nothing reaches the mailbox.
* **Hub:** yes.

### `set_title`

Renames a task you added. A message's subject cannot be changed.

* **Arguments:** `uid` (integer, required, 2,000,000,000 or above); `title` (string, 1 to 200 characters after trimming, no
  control characters).
* **Answer:** `true`; `false` for a mail task, a missing task or a title that is refused.
* **Side effects:** sets both the subject and the action text.
* **Hub:** yes.

### `set_note`

* **Arguments:** `uid` (integer, required); `note` (string, at most 2,000 characters; `""` clears).
* **Answer:** `true`; `false` when there is no such task or the note is too long.
* **Side effects:** saves the note with control characters other than line breaks and tabs removed. The note is searchable.
* **Hub:** yes.

### `set_reminder`

Sets or clears the reminder time and the repeat of a task you added. Times are the computer's local wall clock, also when
the phone sets them.

* **Arguments:** `uid` (integer, required); `at` (string `YYYY-MM-DD HH:MM`, exactly that shape, or `""`);
  `repeat` (string: `""`, `daily`, `weekly` or `monthly`).
* **Answer:** `true`.
* **Errors:** `no such task`;
  `Reminders are for tasks you added yourself. To see a message later, snooze it`;
  `Reminders are for tasks you added yourself, not the ones the app made for you` (a follow-up);
  `A task can repeat daily, weekly or monthly`; `A repeat needs a reminder time or a due date`;
  `Pick a date and a time`; `Pick a date between 2000 and 2100`; `Pick a time that has not passed` (the current minute
  still counts, and the time the task already has is accepted even if it has passed).
* **Side effects:** `at` and `repeat` both empty clear the reminder and the repeat. `at` empty with a repeat makes the
  due date repeat. A new time is announced once by the scheduler, whatever the quiet hours.
* **Hub:** yes.

### `add_task`

Adds a task that did not come from an email.

* **Arguments:** `title` (string, 1 to 200 characters after trimming, no control characters); `priority` (integer 1 to 5,
  default 3); `due` (string `YYYY-MM-DD` or `""`, default `""`).
* **Answer:** the new task's id (an integer, 2,000,000,000 or above).
* **Errors:** `Give the task a title (up to 200 characters), a priority from 1 to 5, and a date like 2026-11-20 if you set one`.
* **Side effects:** a new open task with sender "You", category `personal`, why "Added by you"; a log line. Nothing reaches
  the mailbox.
* **Hub:** yes.

### `set_pref`

The sender choice: always important, mute, or normal. It applies to the sender's address, not only this task.

* **Arguments:** `uid` (integer, required: any task from that sender); `level` (string: `important`, `mute` or `normal`).
* **Answer:** `true`; `false` when the task has no sender address (a task you added) or the level is not one of the three.
* **Errors:** `no such task`.
* **Side effects:** `important` raises every open task from the address to at least priority 4; `mute` sets them to
  priority 1 and not urgent; both are kept for future mail and skip your rules. `normal` forgets the choice and every
  correction recorded for that address. A log line. Nothing is queued for the mailbox.
* **Hub:** yes.

## Reply drafts

Nothing in this group sends mail. The app has no way to send.

### `draft_generate`

Writes, or rewrites, the reply draft of a mail task.

* **Arguments:** `uid` (integer, required); `instruction` (string, optional: control characters removed, first 200
  characters kept, for example `decline politely`); `force` (boolean, optional, default `false`): replace a draft you edited.
* **Answer:**

  ```json
  {"body": "Hi Alex,\n\nThanks, that works for me. Consider it confirmed.\n\nBest,\nSam", "source": "template"}
  ```

  `source` is `ai` when the configured model wrote it, `template` when the quick-reply template was used (rules-only
  ranking, or the model did not answer).
* **Errors:** `This draft has your edits. Confirm to replace it.`; `no such task`;
  `This task did not come from an email, so there is nothing to reply to`; `could not save the draft`.
* **Side effects:** with Ollama, the message goes to the local model; with Claude, the sender, subject and message text go
  to Anthropic and are billed to the user's key. Stores the draft (replacing any), writes a log line. Does not use the
  daily budget for automatic drafts.
* **Hub:** no (spends AI quota).

### `draft_update`

Saves your edit of a draft.

* **Arguments:** `uid` (integer, required); `body` (string, at most 8,000 characters).
* **Answer:** `true`.
* **Errors:** `that draft is too long or the task no longer exists`.
* **Side effects:** stores the text with source `user` and clears the "saved to your Drafts folder" mark; creates a draft if
  the task had none. A log line.
* **Hub:** yes.

### `draft_discard`

* **Arguments:** `uid` (integer, required).
* **Answer:** `true` when a draft was deleted, `false` when there was none.
* **Side effects:** deletes the draft from the app. A copy already saved to the mailbox's Drafts folder stays there.
* **Hub:** yes.

### `draft_to_mailbox`

Asks for the draft to be saved into the Drafts folder of the mailbox the message came to.

* **Arguments:** `uid` (integer, required).
* **Answer:** `true` (queued; the save happens on the write-back thread within seconds).
* **Errors:** `Turn on “Let Inbox Agent file mail” in Settings to save drafts to your mailbox`;
  `that message's mailbox is not set up any more`;
  `Your login name isn't an email address, so drafts can't be saved to your mailbox`; `no such task`;
  `there is no draft or no valid reply address`.
* **Side effects:** queues one `draft` change (not a second while one is waiting). The write-back appends a message to the
  Drafts folder as that mailbox's user and marks the draft saved.
* **Hub:** no (writes to the mailbox).

## Message text, search and files

### `read_body`

* **Arguments:** `uid` (integer, required).
* **Answer:** the stored message text as a string (at most 3,000 characters; empty when none was kept, and for tasks you added).
* **Errors:** `no such task`.
* **Side effects:** none.
* **Hub:** no (the phone never reads message text).

### `search`

Full-text search over the subject, sender name, address, stored message text and note of every task, finished ones included.

* **Arguments:** `q` (string). Only letters and digits count; the first 100 characters and 8 words are used; every word must
  match, and the last also matches as a prefix. Accents are folded.
* **Answer:** an array of task ids, best match first, at most 500; `[]` when nothing is left of `q`.
* **Side effects:** none.
* **Hub:** no. The phone filters on what the list itself holds.

### `save_event`

Writes the calendar event an invitation carried as a new `.ics` file.

* **Arguments:** `uid` (integer, required).
* **Answer:** the full path of the file written, as a string.
* **Errors:** `The app could not find your Downloads folder`; `no such task`; `This message has no event to save`;
  `This event was cancelled, so there is nothing to add to a calendar`; `Could not save the file: <reason>`.
* **Side effects:** writes a new file in the Downloads folder (or the user folder when there is no Downloads folder); an
  existing file is never replaced (the name gets `-2` up to `-99`). The file is not opened or sent.
* **Hub:** no.

### `export_markdown`

* **Arguments:** none.
* **Answer:** a Markdown string: `# To-do`, then one `- [ ] **P<n>** <action or subject> — <sender> (<category>)` line per
  open task that is not snoozed, with `, due <date>` and `, draft ready` when they apply. Markup characters in mail text are
  escaped.
* **Side effects:** none (the window copies it to the clipboard).
* **Hub:** no.

## Your rules

### `rules_save`

Replaces the whole rule list, in order. New mail follows it from then on.

* **Arguments:** `rules` (array of rule objects):

  ```json
  {"rules": [{"id": "r1", "field": "domain", "text": "news.example.com", "priority": 1, "category": "reading", "file": false}]}
  ```

  `id`: 1 to 32 letters, digits, `-` or `_`, unique. `field`: `sender` (the address is exactly this), `domain` (that domain
  or a sub-domain) or `subject` (contains). `text`: 1 to 100 characters, no control characters. `priority`: 1 to 5 or
  `null`. `category`: one of `work`, `personal`, `finance`, `travel`, `notification`, `promo`, `reading`, `other`, or `null`.
  `file`: mark done on arrival. A missing field takes its default (`field` `sender`, the rest empty).
* **Answer:** `true`.
* **Errors:** `Those rules couldn't be read` (not an array of objects); `At most 50 rules are allowed`;
  `Every rule needs its own short id`; `A rule looks at the sender, the domain or the subject`;
  `A rule needs text of 1 to 100 characters`; `A rule's priority is from 1 to 5`; `Unknown category in a rule`;
  `A rule has to do something: set a priority, set a category, or file the mail`;
  `Filing mail and setting its priority don't go together`. A refused list changes nothing.
* **Hub:** no.

### `rules_apply`

Runs the saved rules over the open mail already in the list.

* **Arguments:** `dry` (boolean, optional, default `true`): only count.
* **Answer:** `{"matched": 4, "changed": 3, "filed": 1}`: open mail tasks a rule applies to, those that would change, and
  those that would be marked done.
* **Side effects (when `dry` is `false`):** skips tasks with a sender choice, tasks whose priority you corrected and tasks you
  added. A subject rule may raise only mail from someone you write to or who replies to you, and no rule raises mail that failed
  its sender check. Sets priority, category and the "why" text; a rule priority of 3 or less takes the urgent mark off; filing
  clears urgent and needs-reply, caps the priority at 2 and marks the task done (which queues `archive`). Queues a label for every
  priority change. Writes one log line, but only when at least one task changed. Records no corrections.
* **Hub:** no.

## Mailboxes and setup

### `test_account`

Signs in to an IMAP server to check the details. Stores nothing.

* **Arguments:** `host` (string); `port` (integer, default 993); `user` (string); `password` (string).
* **Answer:** `{"ok": true, "sent": "[Gmail]/Sent Mail", "gmail": true}`: the Sent folder found and whether the host is Gmail.
* **Errors:** the operating system's own lookup error for a name that does not resolve (a mistyped host);
  `could not find the server <host>` (only when the lookup succeeds but returns no address); `secure connection failed: <reason>`;
  `sign-in refused: <server text>`; `no Sent folder found among <n> folders`; network errors (15-second connect timeout; the
  name lookup before it has no limit of its own).
* **Side effects:** one TLS connection to `host:port`, sign-in, folder list, sign-out. The host is not checked against the
  settings rules before connecting.
* **Hub:** no.

### `save_account`

Saves a new app password, server or address for a mailbox that exists (or for the first mailbox during setup).

* **Arguments:** `id` (integer, optional; missing or `null` is 0, the first mailbox); `host`; `port` (default 993); `user`;
  `password`.
* **Answer:** `true`.
* **Errors:** `There is no such mailbox` (an `id` from 1 to 4 that is not set up); any [settings validation](../reference/settings.md#validation)
  message, such as `That mail server name isn't valid` or `That mailbox is already in the app`.
* **Side effects:** when the server or address of an existing mailbox changes, the background threads stop (the stop flag is
  shared, so every mailbox's threads stop and reconnect, not only this one's) and that mailbox's tasks,
  drafts, queued changes, corrections and follow-ups are deleted first. Stores the password in the system keychain (an empty
  one reads as "no password"), saves the settings, and restarts the background threads if setup is finished. It does not
  test the password: the window calls `test_account` first.
* **Hub:** no.

### `add_account`

Adds another mailbox (up to five in all).

* **Arguments:** `host`; `port` (default 993); `user`; `password` (required).
* **Answer:** the new mailbox's number, 0 to 4 (the lowest free one).
* **Errors:** `Set up your first mailbox before adding another`; `Enter the app password for this mailbox`;
  `You can read at most five mailboxes at once`; `Enter the mail server and the email address of this mailbox`; settings
  validation messages, such as `That mailbox is already in the app`.
* **Side effects:** clears anything left under that number, stores the password in the keychain, saves the settings and
  restarts the background threads, so the new mailbox starts reading. No connection test (the window tests first).
* **Hub:** no.

### `remove_account`

* **Arguments:** `id` (integer, required).
* **Answer:** `true`.
* **Errors:** `There is no such mailbox`.
* **Side effects:** stops the syncs, deletes the mailbox's tasks, drafts, queued changes, corrections, follow-ups and counters,
  deletes its password from the keychain, removes it from the settings (the last one also sets `onboarded` to `false`), and
  restarts. Nothing in the mailbox itself is touched.
* **Hub:** no.

### `finish_setup`

* **Arguments:** none.
* **Answer:** `true`.
* **Side effects:** sets `onboarded` to `true`, saves, restarts the background threads. It checks nothing: the wizard calls
  it after `save_account`.
* **Hub:** no.

### `sign_out`

"Sign out and reset": forgets everything the app stored.

* **Arguments:** none.
* **Answer:** `true`.
* **Side effects:** stops the background threads; deletes every task, draft, queued change, log line, correction, sender
  choice, rule, follow-up record and counter, then rewrites the database file; deletes `crash.log` and set-aside database
  copies; deletes every mailbox password and the Anthropic key from the keychain; removes the mailboxes from the settings,
  sets `onboarded` to `false` and turns the hub off with its access code cleared. Other options (ranking, alerts, drafts)
  stay. The mailboxes themselves are not touched. Restarts, so the window shows setup again.
* **Hub:** no.

## Settings, keys and ranking

### `save_settings`

Changes options by merging a patch into the current settings.

* **Arguments:** `patch` (object, required): any part of the settings object, nested as it is stored. Unknown keys are
  dropped. For example `{"patch": {"alerts": {"quiet_start": "22:00", "quiet_end": "07:00"}}}`.
* **Answer:** the whole settings object after the change (the same shape as `state.settings`).
* **Errors:** `A settings change must be an object`;
  `The account is changed with save_account, not with settings` (the patch would change `account`, `more_accounts` or
  `onboarded`; a patch that leaves their values as saved is accepted); a type error from reading the result (for example a string where a number belongs); any
  [validation message](../reference/settings.md#validation).
* **Side effects:** saves `settings.json`. A save with the hub off clears its access code; a save with the hub on and no code
  makes a new 32-character one. Restarts the background threads (sync, write-back, scheduler, hub), which also reconnects every
  mailbox.
* **Hub:** no.

### `set_secret`

* **Arguments:** `name` (string, must be `anthropic-key`); `value` (string; `""` deletes the key).
* **Answer:** `true`.
* **Errors:** `unknown secret`; a keychain error.
* **Side effects:** writes or deletes the key in the system keychain. `draft_generate` and `test_ai` read it at once; the
  ranking of new mail picks it up when the background threads next restart (any `save_settings` restarts them).
* **Hub:** no.

### `detect_ollama`

* **Arguments:** `url` (string, optional; default the saved `ai.ollama_url`).
* **Answer:** an array of model names from `<url>/api/tags`, or `[]` when nothing answers within 2 seconds.
* **Side effects:** one HTTP request to that address. The `url` argument is not validated.
* **Hub:** no.

### `test_ai`

Ranks one built-in sample message with the saved ranking settings.

* **Arguments:** none.
* **Answer:** `{"priority": 5, "category": "work", "action": "Sign the contract by 5pm", "why": "...", "needs_reply": true, "due": ""}`.
* **Errors:** `No model is chosen` (mode `ollama` with an empty model name); `No API key is saved` (mode `anthropic` with no key
  in the keychain); otherwise the model's own error (for example when Ollama is not running or the key is refused). With
  rules-only ranking it always answers, with the sample ranked by the rules.
* **Side effects:** with Claude, one paid request to Anthropic with the invented sample.
* **Hub:** no.

## Phone hub

### `hub_info`

* **Arguments:** none.
* **Answer:** `{"url": "http://192.168.1.20:8787/?token=<code>", "svg": "<svg ...>", "enabled": true}`: the address a phone
  opens (this computer's local network address, or `127.0.0.1` when none is found), a QR code of it as SVG, and whether the
  hub is on. Answers also when the hub is off.
* **Side effects:** none.
* **Hub:** no.

### `rotate_hub_token`

* **Arguments:** none.
* **Answer:** `true`.
* **Side effects:** saves a new 32-character access code (without checking whether the hub is on) and restarts the background
  threads; phones holding the old code get `401` from then on.
* **Hub:** no.

## Backup and support

### `backup_export`

* **Arguments:** none.
* **Answer:**

  ```json
  {"path": "C:/Users/you/Downloads/inbox-agent-backup-20261114-091500.json",
   "counts": {"rules": 2, "sender_choices": 14, "corrections": 30, "own_tasks": 5, "mail_state": 120, "mailboxes": 1, "left_out": {}}}
  ```

  `left_out` names each kind that hit its limit (5,000 sender choices, 2,000 corrections, 2,000 tasks of yours, 5,000 entries
  about mail) and how many did not fit.
* **Errors:** `The app could not find your Downloads folder`; `Your data is too large for one backup file (over 8 MB)`;
  `Could not save the backup file: <reason>`.
* **Side effects:** writes a new file in Downloads (never over another one) and a log line. The file holds no password, no
  access code and no message text, and it is not encrypted.
* **Hub:** no.

### `backup_import`

Checks a backup file and previews or performs a restore. A restore only adds; it never removes anything.

* **Arguments:** `json` (string: the whole file); `apply` (boolean, default `false`: preview only, nothing written);
  `settings` (boolean, default `false`: also restore the allowed options and add the file's mailboxes, without passwords).
* **Answer:** a report (`backup::Report` in [`engine/src/backup.rs`](../../engine/src/backup.rs)):

  ```json
  {"applied": false, "rules_added": 1, "rules_skipped": 0, "sender_choices_added": 3, "sender_choices_skipped": 0,
   "corrections_added": 0, "corrections_skipped": 0, "own_tasks_added": 2, "own_tasks_skipped": 0, "reminders_dropped": 0,
   "mail_applied": 4, "mail_unchanged": 1, "mail_parked": 7, "mail_skipped_no_mailbox": 0, "mail_skipped_full": 0,
   "options_changed": [], "options_skipped": [], "mailboxes_added": 0, "mailboxes_skipped": 1, "mailboxes_new": [],
   "rules_new": ["If the domain is news.example.com: priority 1"]}
  ```

* **Errors:** `Finish setting up your first mailbox before restoring a backup`;
  `This file is too large to be a backup made by Inbox Agent (the limit is 8 MB)`;
  `This is not a backup made by Inbox Agent: it could not be read`; `This is not a backup made by Inbox Agent`;
  `This is not a backup made by Inbox Agent: it has no version`;
  `This backup was made by a newer version of Inbox Agent. Update the app, then restore it`; messages naming an entry that
  is not valid or a kind that exceeds its limit; settings validation messages;
  `Your data was restored, but the options and mailboxes could not be saved: <reason>`.
* **Side effects (when `apply` is `true`):** adds what is missing in one transaction; parks entries about mail that has not
  been read yet so they apply when it arrives. With `settings`, saves the options and new mailboxes and restarts. The paid
  cloud ranking choice is never restored (it is listed in `options_skipped`). The steps are in
  [docs/PSEUDOCODE.md](../PSEUDOCODE.md).
* **Hub:** no.

### `diagnostics`

* **Arguments:** none.
* **Answer:** a plain-text support report: version, system, status, task counts, option names, the first mailbox's server
  (named only for one of 11 well-known providers; any other reads `custom server`, and an empty one `not set`), and the last 30
  log lines and last 20 crash-log lines with names, subjects and addresses removed
  (`diag::report` in [`engine/src/diag.rs`](../../engine/src/diag.rs)).
* **Side effects:** none.
* **Hub:** no.
