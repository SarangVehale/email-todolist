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
- Phase 12 review fixes (a three-seat council and two drift passes; [the record](docs/2026-11-15_PHASE12_REVIEW.md)). Signing in: the mail server is asked with exactly the new token, once, before
  anything is saved (a retry used to fall back to the old saved token, hiding the server's refusal and, when signing in again, approving the old grant instead of the
  new one); a sign-in is refused before it is used up if its mailbox cannot be saved, and a grant that was never used is cancelled at Google; signing in again cancels
  the old grant; the settings are read again after the server has been asked, so a change made meanwhile is kept; a token is never sent to a host the settings file names
  instead of the provider's (checked at every connection, not only when settings are saved); a refresh in flight cannot write a token back after its mailbox was removed;
  cancelling a Microsoft sign-in while its code is being fetched no longer leaves a code on screen; a dropped connection is an ordinary error, not a "refused sign-in"
  (it is retried soon and costs no new token); a signed-in mailbox whose sign-in has gone says "Sign in again in Settings" instead of asking for a password; a finished sign-in that is not
  used within ten minutes is forgotten. The tests that read the client IDs no longer fail once the owner fills them in. The window: a signed-in mailbox can go back to
  a code ("Use a code instead of signing in"); "Open that page" works in the Add a mailbox dialog and in Settings; Cancel is not offered once the mailbox is being saved;
  cancelling and signing in again at once is not undone by the first sign-in giving up; a new install gets the simple view as soon as the mailbox is connected (and starts with the
  computer unless unticked), not only when the last screen is closed; setup no longer resets ranking and alerts (it makes sure only that filing and the phone hub are off); a message
  put away with Later has **Bring back now**; "Last read at" follows the engine (the list is redrawn when it changes, and the engine renews it on every round of the mailbox's
  wait for new mail, about once a minute) instead of going stale on a quiet mailbox; the welcome card no longer says nothing in the mailbox changes when filing is on; the Add a mailbox dialog no
  longer calls Outlook unsupported when Microsoft sign-in is offered. Documents: 14 sentences in the guides about the old wizard or only the dense view, and about 40 in the technical documents
  (counts, the command reference, the security model, the settings reference, the design), corrected; a guide for the unsigned-installer warning (`docs/guides/install-and-the-windows-warning.md`).
  CI run 42 failed on Windows and macOS because two window tests read the page's background colour one frame after a switch; they now wait for it.
  A first round of the review loop on those fixes found: signing in again could be overwritten by a refresh of the old grant that was in flight (the refreshed token is now written
  back only if the keychain still holds the token that was used); the old Google grant was cancelled before the new one was saved (now after, and the keychain is put back if the
  settings cannot be saved); the new token was held in memory before the server had accepted it; a Cancel pressed just as the sign-in finished could still save the mailbox. CI run 43 then
  failed once on macOS in a setup test, which showed a real fault: a dialog's 30-millisecond focus timer, fired late on a slow computer, pulled the keyboard from the code box to
  the "Open page" button, so Enter opened the page instead of connecting. The timer now leaves the keyboard alone when it is already inside the dialog.
- Sign in with Google and with Microsoft (Phase 12, third part), built and off until the person who builds the app registers it (the buttons show only
  when `engine/src/oauth.rs` names a client ID: [the runbook](docs/runbooks/register-sign-in.md), [ADR-0015](docs/adr/0015-sign-in-with-google-and-microsoft-needs-the-owners-client-id.md)).
  Google uses the loopback flow with PKCE (a one-shot listener on `127.0.0.1` that keeps accepting until a request carries the right `state`, 5 seconds and 8 KB for
  each connection, a fixed page); Microsoft uses the device-code flow (the window shows the code). The mailbox signs in with XOAUTH2; a refused token is dropped and a
  new one tried once, a withdrawn sign-in is not asked about again for five minutes, and one refresh happens at a time. The lasting token is in the system keychain
  (`oauth-refresh-<n>`), the short one only in memory; tokens travel in a POST body or an IMAP command, never in an address, and no text from a provider is kept.
  A Google sign-in is only ever for `imap.gmail.com` and a Microsoft one for `outlook.office365.com`, and only the sign-in commands can change it. Nothing is saved for a token
  the mail server refuses; removing a mailbox or signing out deletes the token (and Google is told with the token in the body). New commands `oauth_start`, `oauth_poll`,
  `oauth_cancel` and `oauth_finish`, the setting `oauth`, the state fields `sign_in` and `accounts[].auth`, and `www.microsoft.com` on the pages the app may open. The
  first-run screens, the Add a mailbox dialog and Settings ("Sign in again") have the buttons. Twenty-seven engine tests against scripted token and mail servers, and
  sixteen window tests; a finished sign-in that is not saved within ten minutes is forgotten. It has not been run against Google or Microsoft: that needs a registered client ID (manual cases MT-150 to MT-156).
- The simple view (Phase 12, second part): the list for someone who does not want to learn it. Every message is a card with what it is, who it
  is from and how long ago in words, and two big buttons: **Done** (hides it) and **Later** (puts it away until nine the next morning). The
  headings are plain ("Your list", "Needs you now", "Needs you today", "Can wait", "Not important"); the tabs are Your list, Later and Done; the dot
  counters, the rail, the filter bar, the circles, stars and priority dots are hidden (the keys still work). **Undo** stays until the next thing is
  done, a card at the top says in five short sentences how it works, an opened card shows who, why and the start of the message with **Read message**
  and, for Gmail, **Open in Gmail** (also in the dense view), and the status line says when the mailbox was last read ("Watching your mailbox.
  Last read at 2:32 PM."; the engine's `state` has a new `checked_at`, for the window only). Text size (Normal, Large, Huge) and light colours are
  choices of their own in Settings, in either view, and belong to the device. Settings in the simple view has the look, the mailboxes, alerts and
  starting with the computer, with **More options** for the rest. A new install gets the simple view when setup finishes; an install already in
  use keeps the dense one. The accessibility checks run in both views, both colour schemes and every text size, the hostile-text test runs on
  the cards, and the dense view's own tests are unchanged. Guide: `docs/guides/the-simple-view.md`.
- Simple setup (Phase 12, first part). The first-run wizard is three screens and a last one, written for someone who has never heard of IMAP.
  It asks for the email address and reads the provider from it. A button opens the page where the provider makes the 16-letter code (Google,
  Apple, Yahoo, Fastmail), with three plain steps beside it. **Paste**, or one **Connect** button, tests the details, saves the mailbox and
  finishes setup with safe settings: ranked by rules, desktop alerts on, the mailbox left untouched, the phone hub off. What the server says
  becomes a sentence ("That code didn't work. Make a new one with the button above and paste it here."), with the server's own words behind
  "Details for a helper". Outlook, Hotmail and Live addresses get an honest screen instead of a form that cannot work, and an address the app
  does not know gets the plain server form with the server guessed. The questions about ranking models, phone alerts, filing mail and the
  phone hub left the first run; they are in Settings, as they were, and off. The last screen says that closing the window leaves the app
  running. The new engine command `open_page` opens only an `https` page on a short list of provider hosts (no user name, no port, nothing
  encoded) and the phone hub cannot call it; a test reads every page named in the window and checks the engine allows it. The Add a mailbox
  dialog got the same button and sentences. Four engine tests and 21 window tests; the accessibility checks (names, targets, contrast) run on every
  first-run screen. Design: `docs/designs/phase12.md`; next are the easy look of the list and Sign in with Google and with Microsoft.
- Gaps closed (Phase 11), found by writing the documentation and by the first CI run. A task number, a priority and a mail server port
  were cut to 32, 8 and 16 bits: task 2^32 + n acted on task n, priority 261 was read as 5, port 66529 as 993; they are now
  refused. A request can no longer set the phone hub's access code to a string of its choosing or the ranking mode to a word that is
  none of rules, ollama and anthropic. "Test connection", "Detect" and saving a mailbox check what they are given (a server name, an
  address, a password, an http(s) address) before they do anything, setup cannot be finished with no mailbox, and the snooze error
  says what is accepted (today, before 09:00, up to a year ahead). The diagnostics report no longer names a "Google sign-in" option
  that never existed. A write that reads first now takes the database's write lock at once: a sync committing in between made it fail
  with "database is locked", seen when adding a mailbox. A saved ranking mode that is none of the three reads as "rules", so one hand-edited value cannot block every later save. Nine new engine tests, each guard put back once to see a test notice.
- The first push to GitHub showed that CI had never been able to run: `.github/workflows/ci.yml` was not valid YAML (a step name held a colon and a space), so GitHub refused it without starting a job. The name is quoted, every pinned action was checked to exist, and `scripts/check_docs.py` now catches the mistake in a workflow or the OpenAPI file before a push. After four more fixes (the Phase 11 entry above and `docs/designs/phase11.md`) CI passes on Windows, Linux and macOS, and the window's 65 tests pass on all three.
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

- One-installer desktop app (Windows build verified by hand; the release workflow built installers for Windows, macOS (universal) and Linux on a rehearsal tag, none of them opened by hand yet on macOS or Linux).
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
- Hardening, the window: a test that puts script-carrying text in every field a message can supply and draws every place it can be seen found one place that wrote a value into the page without escaping it, the time part of a task's reminder (the engine only stores validated times, so mail cannot supply it; fixed anyway). Three new window tests. The phone hub's replies also carry `Cross-Origin-Opener-Policy` and a `Permissions-Policy` that switches off the camera, microphone, location, payment and USB, and the header test now checks every way the hub can answer (a page, a missing page, a refusal, a wrong method, a malformed request), not only the front page.
- Hardening: the engine is now held by the compiler to two rules, no `unsafe` code and no `unwrap` or `expect` outside tests (the last two were removed; a message the AI could not rank now gets a neutral verdict instead of a panic if the rules ever fail). Damaged-input tests now also cover a real backup file (bytes and JSON that still parses), a settings change, and the phone hub (damaged HTTP requests over a real socket, counting panics in its connection threads); a deep run of each found nothing. Workflows: no expression such as a tag name is pasted into a shell script (it goes through the environment; a documentation check says so), and the checkout token is not left in the checkout.
- Fixes from a review of everything since Phase 11 (three council seats, two documentation passes): the damaged-input test could not see a panic in the mail parser, which reports one as an error, and now looks for that error and checks that its samples parse undamaged; the packaged-app checks and the scale check run the app with its keychain in memory (they could have overwritten and deleted, or read, a real mailbox password), clean up in a `finally` and use an access code made for each run, and the install check refuses to run over a real install and stops only what runs from its temporary folder; a window-test browser that timed out on Windows is found and ended by its profile folder; the release workflow stops when only some of the six Apple secrets are set, checks inside the deb, the app archive and the AppImage that the three licence files are there, and keeps its two Tauri-action steps identical (a documentation check says so); CI checks the licence policy on every push and the release checks that the list is current; MPL-2.0 is allowed for five named crates only; the window says that a mailbox with no Sent folder has no follow-up reminders either; `rust-version` is 1.85, which the dependencies need; and a note on the system libraries in the Linux AppImage.
- The installer now carries the licences. `THIRD_PARTY_LICENSES.txt` names each of the 565 Rust crates in the app with its licence and holds the texts that came with them, and the installer puts it, `THIRD_PARTY_NOTICES.md` and `LICENSE` next to the program (MIT and Apache ask for that). `scripts/gen_licenses.py` writes the list; CI fails when it is out of date or when a crate has a licence nobody has reviewed (a crate that is only GPL, AGPL or LGPL stops the build; today the list is permissive licences plus MPL-2.0 for five crates used unmodified through Tauri). `tauri-action` 1.0 and `attest-build-provenance` 4.2 are taken after a rehearsal.
- A test that damages valid mail, calendar invitations, dates and rule texts in thousands of ways (bytes flipped, inserted, cut, a piece repeated hundreds of times) and fails on any panic or any input that takes over two seconds. Seven new engine tests; a run of 660,000 inputs found no panic and nothing slower than 145 ms.
- A mailbox whose Sent folder cannot be found (a translated name and no flag) can now be added: "Test connection" passes and says "no Sent folder found, so replies you send will not close tasks and there are no follow-up reminders" instead of refusing the mailbox. The sync already coped with such a mailbox; only the test (and the read-only `probe` example) refused it.
- Accessibility, checked by seven new window tests: nine fields and menus of Settings had no name a screen reader could read (their label was drawn near them, not tied to them); the setup wizard's dialog lost its name after its first step; the Add a task date box was 21 pixels tall and unstyled; the top-bar buttons, the star and the priority dots were under the 24 by 24 pixel minimum (the dots are now 24 apart; on a touch screen they are 24 wide and 34 tall, where an earlier rule never applied); the unstarred star was too faint (2.5 to 1, now 5 to 1).
- The window's tests failed three times in CI in a day (run 10, run 22 and a release rehearsal) for one reason, read from the annotation CI now writes: the browser did not open its debugging port within a fixed 15 seconds on a busy runner, and every test of that file was cancelled. The harness now waits up to 40 seconds and starts the browser once more, with three tests of that using a fake browser; the release workflow's window-test step writes the same annotation. The engine itself is no slower than at Phase 8 (the same 3,000 open and 5,000 finished tasks, timed in one process, old code against new, alternating: 143 to 163 ms against 149 to 156 ms at best).
- The release workflow was rehearsed five times on throwaway tags, and the last three ran to the end (verify, tests on three systems, an unsigned installer for each, checksums and an attestation; one draft release held the Windows installer, a universal macOS disk image and archive, and Linux deb and AppImage packages). The first rehearsal's macOS build failed because the workflow passed the Apple signing variables as empty strings when the secrets do not exist, and Tauri tried to import an empty certificate; they are now passed only when the secrets exist, and the builds run one at a time. The packaged-app checks (114 checks of the built program through the phone hub and the window's own IPC, and an install, start and uninstall cycle) are now in the repository under `scripts/packaged_check/`; they refuse to run if the app already has a data folder (the install check also if it is installed), run the app with its keychain in memory, and clean up after themselves even when they fail. One check changed with Phase 11: setting up the first mailbox now needs its password.
- Dependencies: the mail parser (`mailparse`) is on 0.18 and the random-number library (`rand`, which makes the phone hub's access code) on 0.10, both reviewed by building and running every engine test against them first. The access code still comes from a ChaCha12 generator seeded by the operating system. The HTTP client (`ureq` 3) and `base64` 0.23 were looked at and not taken: the first needs a migration of its own, the second adds unsafe SIMD code by default that nothing here uses. The two pinned GitHub actions that CI flagged as deprecated (`checkout` and `setup-node`, which ran on Node 20) are on versions that run on Node 24.
- 480 automated tests of the engine, including protocol tests against a scripted mock IMAP server and a dedicated edge-case suite, and 65 tests of the window itself (`cd app && npm test`): they run the real window in a headless browser that is already on the computer (Edge or Chrome; Node 22 or later), with real key presses and mouse clicks and nothing else to install. Nearly every defect that review found in the window in the last three runs now has a test that fails when the old behaviour is put back (`docs/2026-11-14_PHASE9_REVIEW.md` names the few that do not). Writing them showed that the restore dialog's code said it would put the keyboard on Restore but always put it on Cancel, because Cancel comes first in the dialog; the code now says Cancel outright, so that Enter pressed by reflex restores nothing. Nothing visible changed.

### Known limitations
- Microsoft (Outlook, Microsoft 365) accounts are not supported: Microsoft turned off password sign-in for IMAP.
- Not yet verified against a live mailbox.
- Release builds are unsigned until signing certificates are configured (see RELEASING.md).
- No automatic updates yet.
