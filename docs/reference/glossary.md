# Glossary

> **Status:** current · **Audience:** everyone who writes code, documents or tests for this project · **Last reviewed:** 2026-11-14

The words this project uses, alphabetically, with what the code and the window mean by them. Use these words and no others
for the same thing (see the [style guide](../standards/docs-style-guide.md)).

**Access code.** The 32-character secret a phone needs to use the hub's commands, sent as a bearer token. Shown inside the
QR code in Settings; "Rotate access code" replaces it. In the code: `hub.token`. Not a password for the mailbox.

**Alert.** A desktop notification, and an ntfy push when a topic is set, for new mail of priority 4 or 5 or urgent mail, in
the chosen categories and outside quiet hours (urgent mail ignores both). Not sent for the first read of a new mailbox.

**App password.** A password the mail provider makes for one app, used instead of your account password. The app signs in
to IMAP with it and keeps it in the system keychain, never in a file.

**Automated mail fold.** The "Show automated · N" row in the plain To-do view: open mail that is bulk, priority 2 or lower,
not starred, not urgent, with no due date and no draft, folded into one row. "Done all automated" finishes them together.

**Backup.** One JSON file (`inbox-agent-backup-<date>-<time>.json`, in Downloads) with your rules, sender choices,
corrections, tasks you added, notes and what you did with mail, plus the allowed options and mailbox list. No password, no
access code, no message text; not encrypted.

**Bounded window.** The rule that keeps a big mailbox fast: the engine sends every unfinished task but only the newest
finished ones (200 unless the Done tab asks for more, at most 20,000), and the window draws 150 rows at a time.

**Category.** One of `work`, `personal`, `finance`, `travel`, `notification`, `promo`, `reading`, `other`, given by triage
or a rule. Filters the list and the alerts.

**Conversation.** Messages of one mailbox that share a thread key: the first id in `References` (else `In-Reply-To`, else
the message's own `Message-ID`).
The window shows a conversation as one row: its newest message, with the highest priority and a message count. Done, star
and snooze on the row apply to every message of it shown in that tab.

**Correction.** A priority you changed by hand on a mail task, remembered by sender address. The last three corrections of
a sender can shift that sender's later mail by one or two steps ("Learned +1"). Choosing Normal for the sender forgets them.

**Digest.** A once-a-day notification at or after `alerts.digest_time` ("Inbox digest") counting open conversations, high,
due, urgent and drafts ready. Not sent when nothing is open.

**Done.** A task's `status` is `done`: finished by you, filed by a rule, or closed by the app because you replied. Finished
tasks are kept 90 days. A repeating task you added is never done: finishing it moves it to its next time.

**Draft.** A reply the app wrote (by a model or from a template) or you edited, kept with the task. Never sent. You copy it,
or save it to the mailbox's Drafts folder when write-back is on.

**Due date.** A `YYYY-MM-DD` date on a task, found in the message or set by you. A task due today or earlier that is
waiting on you sorts as at least priority 4.

**Engine.** The Rust library in `engine/`: the mail sync, triage, the database, write-back, the scheduler and the hub. It
answers the window's commands.

**Follow-up.** A task the app makes ("Waiting") when nobody has answered a conversation you started within the chosen days
(2, 3, 5 or 7). It closes itself when a reply arrives. Its id is in the range of tasks you added, but it belongs to a mailbox.

**Hub.** The phone hub: a small HTTP server in the engine that serves the window to a phone or tablet on your own network
and accepts a safe subset of commands with the access code. Plain HTTP; off by default.

**Keychain.** The operating system's password store (Windows Credential Manager, macOS Keychain, Secret Service), where app
passwords and the Anthropic key live.

**Left the inbox.** A task whose message is no longer in the inbox because it was archived, moved or deleted elsewhere
(`gone`). Such tasks fold into "Show left your inbox · N"; they are not closed for you.

**Log.** The activity record on the Log tab: what the app did, newest first; the window shows the newest 100, kept 30 days.

**Mailbox.** One IMAP account the app reads (its INBOX, Sent and, for follow-ups, Archive or All Mail). Up to five,
numbered 0 to 4; the number is part of every task id it makes. Not "account" in documents.

**Pending.** Mailbox changes waiting in the queue (`state.pending`, "to file"). They are sent only while write-back is on.

**Priority.** 1 (lowest) to 5 (highest). The list groups it as Now (5), Today (4), This week (3) and Later (1 and 2).

**Quiet hours.** A daily window (`quiet_start` to `quiet_end`) in which alerts for anything but urgent mail are held back.

**Reminder.** A local date and time on a task you added at which the app notifies you once, whatever the quiet hours.
Messages use snooze instead.

**Repeat.** Daily, weekly or monthly on a task you added: finishing it moves its reminder (or its due date) to the next
time, counted from the first one, and it stays open. There is no undo for that.

**Restore preview.** The first step of a restore: the engine reads the file and reports what restoring it would add,
change or leave out, writing nothing. Restore runs the same steps for real.

**Rule.** An "if the address, domain or subject matches, then set the priority or category, or file the mail" line you
write in Settings. The first match wins; a sender choice skips the rules. At most 50.

**Select mode.** The window mode for acting on many rows at once (`V`, or the Select button): pick rows, then Done, Star,
a priority, snooze or Mute senders, with one undo for the batch.

**Sender check.** DKIM, SPF and DMARC results the mail server added. Mail that fails is not "someone you write to", is
not raised by rules, and teaches nothing when corrected; its reply draft warns before saving.

**Sender choice.** "Always important", "Mute" or "Normal" for a sender address (`pref`). Important lifts their open and
future mail to at least 4; mute drops it to 1 and silences it; normal forgets the choice and the corrections.

**Signal.** A fact the engine reads from your own mail before any model: `known` (you have written to this address),
`reply` (a reply to your message), `bulk` (list or no-reply headers). Shown as "Signals" on an open task.

**Snooze.** Hiding a task until a time (3 hours, tomorrow 09:00, next Monday 09:00, or a day you pick, 09:00). It comes
back with a "Back from snooze" notice. Nothing in the mailbox changes.

**Status.** The sync state shown by the dot and the footer: `idle`, `syncing`, `connecting`, `no password`, `setup`,
`starting`, or `error: ...`. With several mailboxes the worst one is shown.

**Task.** One row of the list: a message that may need you, a follow-up, or something you added yourself (id 2,000,000,000
or above, tagged "Mine"). Not "item" or "card".

**Triage.** Ranking a new message: priority, category, action, urgent, needs-reply and due date, by the rules-only ranker
or a model (Ollama or Claude), after your sender choices and rules.

**Undo.** `Z` or the Undo button: reverses the last change made in this window (done, star, priority, snooze, due date,
title). Kept in the window only, not across restarts.

**Urgent.** A mark from triage for mail that cannot wait; it alerts even in quiet hours. A rule that sets priority 3 or
less removes it.

**Window.** The app's user interface in `app/ui`: plain HTML, CSS and JavaScript, shown by the Tauri shell on the desktop
and by the hub on a phone. Not "front end" or "client" in documents.

**Write-back.** "Let Inbox Agent file mail": when on, done archives the message, a star flags it, priorities and categories
become labels on Gmail, and a draft can be saved to Drafts. Off by default; changes made while it is off are queued and
applied when it is turned on.
