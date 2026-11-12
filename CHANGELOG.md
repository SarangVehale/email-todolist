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

### Security
- Two independent security reviews; all confirmed findings fixed with regression tests. See SECURITY.md.
- 109 automated tests, including protocol tests against a scripted mock IMAP server and a dedicated edge-case suite.

### Known limitations
- Microsoft (Outlook, Microsoft 365) accounts are not supported: Microsoft turned off password sign-in for IMAP.
- Not yet verified against a live mailbox.
- Release builds are unsigned until signing certificates are configured (see RELEASING.md).
- No automatic updates yet.
