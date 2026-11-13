# Changelog

All notable changes to Inbox Agent. Versions follow [Semantic Versioning](https://semver.org). Before 1.0, minor
versions may change behaviour.

## [0.1.0] - Unreleased

First public build.

### Added
- Reminders for tasks you add yourself: a date and time, announced once by a desktop notification (and ntfy if you use it),
  and an optional repeat (daily, weekly, monthly). Finishing a repeating task moves it to its next time instead of closing it
  (also when it is done ahead of time); there is no undo for that, and a batch's undo does not include repeating tasks.
- Search inside messages (desktop): words in the first 3,000 characters of the stored message text are found, all of them, in
  any order, accents and case ignored (up to 8 words and 100 characters). Rows that only the index found, not the list's own
  search of sender, subject, excerpt and note, are tagged "In text".
- Calendar invitations: the event in an invitation is shown on its task (UTC times in your time; Outlook's Windows zone names
  understood; an unknown zone is said to be unknown; a repeating invitation is marked, only its first time is kept), its day
  where you are becomes the due date if there is none, and "Save .ics" (desktop) writes a calendar file into Downloads, or your
  user folder if there is none, for you to open. The file is built by the app; it never replaces an existing file and nothing
  is sent to any calendar.
- Select several tasks at once (`v` or the Select button) and mark them done, star them, set a priority, snooze them or
  mute their senders in one step. One undo takes back a batch; muting a sender is reversed from one of its tasks.
- Your own rules: "if the address is, the domain is or the subject contains this, set the priority or category, or file
  the mail". The first matching rule is used; a sender you marked important or muted skips them. A rule never raises mail
  that failed its sender check, a rule on the subject raises only mail from people you write to, and a display name is never
  matched. "Apply to my list" shows a count first.
- Follow-up reminders (off by default; 2, 3, 5 or 7 days): a conversation you started that nobody answered becomes a
  "Waiting" task, and closes itself when a reply arrives. The app only searches your Inbox, and All Mail or Archive when the
  server has them, for the reply; your own follow-up messages and automatic out-of-office replies do not count as answers.
- A new app password can be saved in Settings without losing the list, notes or your own tasks.

- Update notice: once the project has a GitHub repository the app asks GitHub once a day whether a newer version exists and
  shows a link (it downloads nothing). Off until a repository is set; a Settings switch turns it off.
- "Copy diagnostics" in Settings: a report with counts, option names and recent activity, with names, subjects and
  addresses removed, for bug reports.
- Tasks close themselves when you reply to the message (forwards do not count), and mail that left the inbox is
  marked and folded away instead of cluttering the list; nothing about this ever changes your mailbox.
- Conversations are one row, with a count and the highest priority; done, star and snooze apply to the whole row.
- The list learns from corrections of a sender (by address, with a way to start over), and rules mode now produces
  priority 5 for a question from someone you write to whose date is today, tomorrow or past. Words like "urgent" in a message count for nothing.
- Read the stored message text in the app, copy a Gmail link, edit due dates, rename your own tasks, snooze until a date.
- Keyboard and screen-reader work: dialogs with roles and focus handling, Escape to close, announced toasts, readable
  contrast on every row, reduced-motion support.

- One-installer desktop app (Windows build verified; macOS and Linux builds are configured but not yet run in CI).
- First-run setup wizard: connect any IMAP mailbox with an app password, choose how mail is ranked, set alerts.
- Mail ranked by priority, category, due date and "needs a reply" from rules, a local Ollama model, or your own
  Anthropic API key. Falls back to rules whenever a model fails.
- To-do list built around one list: five-dot priority, keyboard control, filters, search, five sort orders, snooze,
  per-sender "always important" / "mute", quiet hours and a daily digest.
- Reply drafts stored inside each task: written by your chosen AI or a template, editable, copyable, and saveable to
  your mailbox's Drafts folder. The app contains no code that can send mail.
- Optional mailbox changes, off until you switch them on: archive and star everywhere, plus priority and category
  labels on Gmail (other servers get the archive move and the flag only).
- Phone and tablet access through a token-protected local hub with QR code setup.
- Desktop notifications and optional ntfy push alerts.
- Passwords and API keys in the operating system keychain.
- Due dates found without any AI: "due 20 Nov", "by Friday", "deadline 2026-11-20" (a cue word is required, so invoice dates are ignored).
- Attachments (names and count) and a copy-only unsubscribe link (https or mailto) shown on each task.
- A free-text note on every task, and tasks you add yourself (key `A`) that never touch your mailbox.
- Choose which categories raise alerts (urgent mail always does), and a `?` card listing the keyboard shortcuts.

### Security
- The phone hub no longer ships the demo backend, and a page opened without the access code is a dead end instead of a
  sign-in form. Before this, anyone on the same network could be shown a convincing password form served by the user's
  own machine.
- A mailbox with no recognisable Sent folder still loads; a refused password waits five minutes instead of retrying
  every thirty seconds; and a message the server returns empty is retried instead of being skipped for good.
- Third review (Opus, 2026-11-13): one bad unsubscribe link could blank the list; a crashing message could crash the app on every start; hostile HTML could stall the watcher; sign-out left mail text in the database files; the hub answered public networks; mail data sat in Roaming AppData. All fixed with tests; see SECURITY.md.
- A damaged database is set aside and rebuilt, newer data is refused, and a `crash.log` records internal errors (no telemetry).
- Two independent security reviews; all confirmed findings fixed with regression tests. See SECURITY.md.
- 366 automated tests, including protocol tests against a scripted mock IMAP server and a dedicated edge-case suite.

### Known limitations
- Microsoft (Outlook, Microsoft 365) accounts are not supported: Microsoft turned off password sign-in for IMAP.
- Not yet verified against a live mailbox.
- Release builds are unsigned until signing certificates are configured (see RELEASING.md).
- No automatic updates yet.
