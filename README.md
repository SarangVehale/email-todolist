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
- **Keyboard first.** `j` `k` move, `x` done, `1`-`5` priority, `s` star, `r` reply draft, `h` snooze, `a` add task,
  `/` search, `z` undo, `?` for the full list.

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

Preview the UI with demo data and no engine: serve `app/ui` over HTTP and open `/?demo`.
The demo backend is used whenever no engine is present; without `?demo` it opens the setup wizard.

Read-only check against a real mailbox, without the app:

```bash
IMAP_HOST=imap.example.com IMAP_USER=you@example.com IMAP_PASS=... cargo run --example probe --release
```

## Status

Built and tested: engine (235 tests, including protocol tests against a scripted mock IMAP server and a dedicated edge-case suite), UI in a browser, phone hub, installer build.
The IMAP sync and write-back are tested against a scripted mock server, which proves what the client sends and how it parses
standard responses, but a real Gmail or other server may differ. Not yet exercised against a live mailbox, and releases are unsigned.
See [SECURITY.md](SECURITY.md) for the full list of limits.
