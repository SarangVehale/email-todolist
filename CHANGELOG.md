# Changelog

All notable changes to Inbox Agent. Versions follow [Semantic Versioning](https://semver.org). Before 1.0, minor
versions may change behaviour.

## [0.1.0] - Unreleased

First public build.

### Added
- One-installer desktop app (Windows build verified; macOS and Linux builds are configured but not yet run in CI).
- First-run setup wizard: connect any IMAP mailbox with an app password, choose how mail is ranked, set alerts.
- Mail ranked by priority, category, due date and "needs a reply" from rules, a local Ollama model, or your own
  Anthropic API key. Falls back to rules whenever a model fails.
- To-do list built around one list: five-dot priority, keyboard control, filters, search, five sort orders, snooze,
  per-sender "always important" / "mute", quiet hours and a daily digest.
- Reply drafts stored inside each task: written by your chosen AI or a template, editable, copyable, and saveable to
  your mailbox's Drafts folder. The app contains no code that can send mail.
- Optional mailbox changes (archive, star, priority and category labels), off until you switch them on.
- Phone and tablet access through a token-protected local hub with QR code setup.
- Desktop notifications and optional ntfy push alerts.
- Passwords and API keys in the operating system keychain.
- Due dates found without any AI: "due 20 Nov", "by Friday", "deadline 2026-11-20" (a cue word is required, so invoice dates are ignored).
- Attachments (names and count) and a copy-only unsubscribe link (https or mailto) shown on each task.
- A free-text note on every task, and tasks you add yourself (key `A`) that never touch your mailbox.
- Choose which categories raise alerts (urgent mail always does), and a `?` card listing the keyboard shortcuts.

### Security
- Third review (Opus, 2026-11-13): one bad unsubscribe link could blank the list; a crashing message could crash the app on every start; hostile HTML could stall the watcher; sign-out left mail text in the database files; the hub answered public networks; mail data sat in Roaming AppData. All fixed with tests; see SECURITY.md.
- A damaged database is set aside and rebuilt, newer data is refused, and a `crash.log` records internal errors (no telemetry).
- Two independent security reviews; all confirmed findings fixed with regression tests. See SECURITY.md.
- 157 automated tests, including protocol tests against a scripted mock IMAP server and a dedicated edge-case suite.

### Known limitations
- Microsoft (Outlook, Microsoft 365) accounts are not supported: Microsoft turned off password sign-in for IMAP.
- Not yet verified against a live mailbox.
- Release builds are unsigned until signing certificates are configured (see RELEASING.md).
- No automatic updates yet.
