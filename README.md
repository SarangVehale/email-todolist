# Inbox Agent

An AI-triaged to-do list for your email. One installer, and all setup happens inside the app.
It watches your mailbox, ranks every message by how much it needs you, shows one focused list,
and alerts you only when something is important.

- **Private by design.** Mail is read on your computer; your password lives in the OS keychain.
  See [PRIVACY.md](PRIVACY.md).
- **Your choice of brain.** Rules only (default, nothing leaves your computer), a local model
  through Ollama, or Claude with your own API key.
- **Works on your phone and iPad.** The desktop app can serve the same list to your devices
  (QR code in Settings).
- **Drafts reply for you, never sends.** Mail that needs an answer gets a draft inside its task: written by your
  chosen AI (or a quick-reply template), editable, copyable, and saveable to your mailbox's Drafts folder.
- **Sorts everything.** Priority, category, due date and "needs a reply" for every message; filter chips,
  search, five sort orders, snooze, "always important" or "mute" per sender, quiet hours and a daily digest.
- **A list that tidies itself.** Tasks close when you reply, conversations are one row, and the order learns from how you
  correct it. Newsletters and mail that left your inbox fold away.
- **Your own tasks and notes.** Add tasks that never came by email (`a`), and keep a note on any task. Attachments and a
  copy-only unsubscribe link show on each message; due dates are found even without AI.
- **Your own rules and follow-ups.** "If the address, domain or subject matches, set the priority or category, or file
  it", edited in Settings and applied to new mail and, on request, to the list. Optional follow-up reminders turn a
  conversation you started that nobody answered into a task that closes itself when a reply arrives.
- **Select many.** `v` (or the Select button) lets you pick several rows, then done, star, priority, snooze or mute
  for all of them at once; one undo takes back a batch (muting a sender is the exception: you reverse it from a task).
- **Reminders, search inside messages, calendar invitations.** Give a task of your own a time to be reminded and let it
  repeat daily, weekly or monthly. Search finds words inside the stored message text, not only in the subject. An
  invitation shows its event, and "Save .ics" writes a calendar file you open yourself.
- **Several mailboxes, one list.** Add up to five mailboxes (any IMAP provider with an app password) in Settings. Their mail
  merges into one list with a small tag on each task and a filter chip per mailbox; each is read, filed and followed up in
  its own account, and removing one deletes its tasks, drafts, follow-up records and saved password and leaves the mailbox itself untouched.
- **Backup and restore.** One file with your rules, sender choices, corrections, tasks, notes and what you did with mail, for a new
  computer or a fresh start. No password goes in it, and no message bodies. Restoring adds what is missing, never removes anything, and shows a preview first.
- **Fast with a big mailbox.** The window is sent every open task but only the newest finished ones (the Done tab can show older ones), and
  the list draws 150 rows at a time, so thousands of tasks do not slow it down.
- **Keyboard first.** `j` `k` or the arrow keys move, `Enter` opens a task, `x` (or `e`) done, `1`-`5` priority,
  `s` star, `r` reply draft (mail only), `h` snooze, `v` select several, `a` add task, `/` search, `z` undo, `,` settings, `?` for the full
  list. Capitals work too.

## Repository

| Path | What it is |
| --- | --- |
| `app/` | Tauri 2 desktop app. UI in `app/ui` (plain HTML, CSS and JS, no build step); shell in `app/src-tauri` |
| `engine/` | Rust engine: IMAP watcher, signals, triage, SQLite, mailbox write-back, phone hub |
| `SECURITY.md` | Security review, fixes, residual risks |
| `RELEASING.md` | How to cut a release: signing secrets, tagging, CI, verification, rollback |
| `CHANGELOG.md` | What changed in each version |
| `docs/PSEUDOCODE.md` | How the engine works, step by step, with the edge cases each step handles |
| `docs/TESTCASES.md` | Catalogue of every automated test (generated) and the manual live-mailbox test plan |
| `.github/workflows/` | CI (format, lint, tests, audit) and a three-OS release build |

## Develop

```bash
cd engine && cargo test                       # unit, security and hub tests
cd engine && cargo clippy --all-targets -- -D warnings
cd app && npm ci && npx tauri build            # Windows installer; needs Rust, Node, MSVC Build Tools
```

Preview the UI with demo data and no engine: serve `app/ui` over HTTP and open `/demo.html?demo=1`.
`demo.html` loads the invented-mail backend and accepts any password, so the phone hub refuses to serve it; it is for
development only.

Read-only check against a real mailbox, without the app:

```bash
IMAP_HOST=imap.example.com IMAP_USER=you@example.com IMAP_PASS=... cargo run --example probe --release
```

## Status

Built and tested: engine (459 tests, including protocol tests against a scripted mock IMAP server and a dedicated edge-case suite), UI in a browser, phone hub, installer build.
The IMAP sync and write-back are tested against a scripted mock server, which proves what the client sends and how it parses
standard responses, but a real Gmail or other server may differ. Not yet exercised against a live mailbox, and releases are unsigned.
See [SECURITY.md](SECURITY.md) for the full list of limits.
