# Changelog

All notable changes to Inbox Agent. Versions follow [Semantic Versioning](https://semver.org). Before 1.0, minor
versions may change behaviour.

This is the user-facing record, kept up to date with every piece of work. Two companion documents answer other questions:
[`docs/VERSION_HISTORY.md`](docs/VERSION_HISTORY.md) says which development build had what (commit, date, tests, review), and
[`docs/RELEASE_HISTORY.md`](docs/RELEASE_HISTORY.md) says what was actually published. Until the first release is published, all
work goes under `[0.1.0] - Unreleased`; after it, new work goes under `## [Unreleased]` at the top, and the next release dates it.

## [0.1.0] - Unreleased

First public build.

### Added
- Gaps closed (Phase 11), found by writing the documentation and by the first CI run. A task number, a priority and a mail server port
  were cut to 32, 8 and 16 bits: task 2^32 + n acted on task n, priority 261 was read as 5, port 66529 as 993; they are now
  refused. A request can no longer set the phone hub's access code to a string of its choosing or the ranking mode to a word that is
  none of rules, ollama and anthropic. "Test connection", "Detect" and saving a mailbox check what they are given (a server name, an
  address, a password, an http(s) address) before they do anything, setup cannot be finished with no mailbox, and the snooze error
  says what is accepted (today, before 09:00, up to a year ahead). The diagnostics report no longer names a "Google sign-in" option
  that never existed. A write that reads first now takes the database's write lock at once: a sync committing in between made it fail
  with "database is locked", seen when adding a mailbox. A saved ranking mode that is none of the three reads as "rules", so one hand-edited value cannot block every later save. Nine new engine tests, each guard put back once to see a test notice.
- The first push to GitHub showed that CI had never been able to run: `.github/workflows/ci.yml` was not valid YAML (a step name held a colon and a space), so GitHub refused it without starting a job. The name is quoted, every pinned action was checked to exist, and `scripts/check_docs.py` now catches the mistake in a workflow or the OpenAPI file before a push. After four more fixes (the Phase 11 entry above and `docs/designs/phase11.md`) CI passes on Windows, Linux and macOS, and the window's 49 tests pass on all three.
- Documentation as code. A [documentation map](docs/README.md) and about 80 documents in the repository, with 54 diagrams (Mermaid, so they are
  reviewed like code): the architecture (the system, the life of a message, the data model, the threads and the poll loop, the trust
  boundaries), fourteen architecture decision records, a reference for every one of the engine's 36 commands, the phone hub's HTTP
  interface (with an OpenAPI file), the answer to `state` and every setting, guides for each service the app connects to, runbooks,
  a troubleshooting page and a FAQ, product requirements and 40 user stories that name the tests checking them, coding standards,
  a documentation style guide (the Google developer documentation style guide plus this project's rules), a testing strategy and a
  release test plan, and templates for every kind of document. `python scripts/check_docs.py` (39 tests of its own; also run by CI and
  by the release workflow) checks every link and anchor, every code block and diagram, the header and review date of each document
  (180 days), that the map reaches every document, that each engine command has an entry in the reference and none is invented, the
  numbering of the decision records, and that every public item of the engine has a doc comment (100 were added); the Rust
  documentation now builds with warnings as errors. The release workflow now also runs the documentation checks and the window's
  tests. Reading the code for the documents corrected a few things: the wizard's sentence about filing mail (stars become flags;
  priorities become labels on Gmail only), the demo backend's two mailbox error messages (now the engine's words), the count of
  commands a phone may use (twelve, not eleven), the statement that Ollama stays on the computer (true while its address is this
  computer's), and several code comments. It also found one defect, now fixed with a test: the Settings "Test" button for ranking
  said it worked when no model was chosen or no API key was saved (the engine quietly ranks with rules, which never fail); it now
  says "No model is chosen" or "No API key is saved". New: `CONTRIBUTING.md`, a pull-request checklist that asks for the documents
  to change in the same pull request, and a runbook for a leaked password, key or access code.
- A big mailbox stays fast. Measured with the packaged app and 3,000 open plus 5,000 finished tasks: the window was re-reading 6.4 MB
  every 3 seconds, drawing the list took 2 seconds and each key press about a second. The window is now sent every open task but only
  the 200 most recently finished ones and how many there are in all (the Done tab says "The 200 most recently finished of N" and has
  "Show 500 older"; finished tasks are kept 90 days as before), draws 150 rows at a time with "Show 150 more" and "Show all N more"
  (the cursor keys draw more as they reach it), draws a row's detail only for the open row, moves the cursor without redrawing the
  page, and stops asking for updates while the window is hidden. Counts, "Select all" and "Done all" cover the whole list the window
  holds, and undo of a bulk finish reopens every task it finished. Measured afterwards on the same mailbox: loading 0.25 s, a key press 2 ms, the Done tab 13 ms, an answer to a poll about 0.13 s and 2.6 MB. There is no search of finished tasks (there never was one).
- Backup and restore (Settings, "Backup"): "Save a backup file" writes your rules, sender choices, corrections, tasks (with notes, reminders
  and repeats), what you did with mail (finished, starred, snoozed, your note and a reply draft you edited, by message ID and never its
  text), your mailboxes and a set of options into one new file in Downloads (not encrypted; no passwords, no phone code, no message
  bodies; a correction keeps the sender and subject of the mail you corrected). A file holds up to 5,000 sender choices, 2,000
  corrections, 2,000 tasks of yours and 5,000 entries about mail, and says how many it left out. "Restore from a backup file" shows a
  preview and then adds what is missing and never removes anything: restoring the same file twice changes nothing, though a later
  snooze, and the options if you tick a box, replace yours. What you did with mail not yet read waits (up to 5,000 entries, 90 days)
  and is applied when that message arrives. Options and mailboxes come back only if you tick a box; a restored mailbox has no password
  until you paste one, and the preview names its server; a file can never turn on the phone hub, write-back or the paid cloud ranking,
  or change where notifications go. Desktop only.
- Documents of record: this changelog, `docs/VERSION_HISTORY.md` (each development build) and `docs/RELEASE_HISTORY.md` (what was
  published), and a release check that refuses a tag without a dated changelog entry and a release-history row.
- Several mailboxes: add up to five (Settings, "Add another mailbox") and read them as one list. Each task is tagged with its
  mailbox and there is a filter chip per mailbox; every mailbox has its own connection, password in the keychain, sent-mail
  learning, follow-ups and write-back (a change goes only to the mailbox the message is in). "Remove" on a mailbox deletes its
  tasks, drafts, waiting changes, follow-up records, the corrections made on its mail and its password, and leaves everything else. A new app password is saved per mailbox, and saving one
  now reconnects every mailbox at once. The phone sees one list and no mailbox names. Existing lists need no migration.
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
- Dependencies: the mail parser (`mailparse`) is on 0.18 and the random-number library (`rand`, which makes the phone hub's access code) on 0.10, both reviewed by building and running every engine test against them first. The access code still comes from a ChaCha12 generator seeded by the operating system. The HTTP client (`ureq` 3) and `base64` 0.23 were looked at and not taken: the first needs a migration of its own, the second adds unsafe SIMD code by default that nothing here uses.
- 470 automated tests of the engine, including protocol tests against a scripted mock IMAP server and a dedicated edge-case suite, and 49 tests of the window itself (`cd app && npm test`): they run the real window in a headless browser that is already on the computer (Edge or Chrome; Node 22 or later), with real key presses and mouse clicks and nothing else to install. Nearly every defect that review found in the window in the last three runs now has a test that fails when the old behaviour is put back (`docs/2026-11-14_PHASE9_REVIEW.md` names the few that do not). Writing them showed that the restore dialog's code said it would put the keyboard on Restore but always put it on Cancel, because Cancel comes first in the dialog; the code now says Cancel outright, so that Enter pressed by reflex restores nothing. Nothing visible changed.

### Known limitations
- Microsoft (Outlook, Microsoft 365) accounts are not supported: Microsoft turned off password sign-in for IMAP.
- Not yet verified against a live mailbox.
- Release builds are unsigned until signing certificates are configured (see RELEASING.md).
- No automatic updates yet.
