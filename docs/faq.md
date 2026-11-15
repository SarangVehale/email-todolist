# Frequently asked questions

> **Status:** current · **Audience:** anyone using or considering Inbox Agent · **Last reviewed:** 2026-11-14

Short answers, each true of the code today, with a link to the document that owns the topic. If something is broken rather than unclear,
use the [troubleshooting page](troubleshooting.md).

## The basics

### What is Inbox Agent?

A desktop app that reads your mailbox, ranks every message by how much it needs you, and shows one list of tasks. It alerts you only
when something is important. All setup happens inside the app. See the [README](../README.md).

### Does it send mail?

No. The app has no code that sends mail (no SMTP anywhere). A reply draft stays in the app, or, if you switch on "Let Inbox Agent file
mail" and press **Save to Drafts**, goes to your mailbox's Drafts folder. You send it from your own mail program. See
[let the app file mail](guides/let-the-app-file-mail.md) and the [decision record](adr/0003-imap-only-never-send-mail.md).

### Is it free?

Yes. The app is MIT-licensed, has no account and no subscription, and needs no paid service. The one optional paid path is "Claude
with your own API key", which Anthropic bills to your own API account. See [choose how mail is ranked](guides/choose-how-mail-is-ranked.md)
and the [decision record](adr/0014-no-paid-services.md).

### Do I need an AI model?

No. The default, **Rules only**, ranks mail on your computer without one. A local model (Ollama) or your own Anthropic key is optional,
and if either fails the rules rank the mail instead. See [choose how mail is ranked](guides/choose-how-mail-is-ranked.md).

### Which operating systems does it run on?

The build is set up to make a Windows installer, a macOS app and disk image, and Linux `.deb` and AppImage packages, and the release
workflow is set up to build all three. The Windows installer has been built and installed by hand; the macOS and Linux packages were built by the release workflow on rehearsal
tags and have not been opened on a Mac or a Linux machine. No release has been published yet, and releases are unsigned, so Windows SmartScreen and macOS Gatekeeper will
warn. See
[release history](RELEASE_HISTORY.md) and [RELEASING.md](../RELEASING.md).

### How do I update it?

Download the new installer by hand. There are no automatic updates, and the daily update check is off until the project names its
repository, so the setting for it does not appear in **Settings**. See [RELEASING.md](../RELEASING.md).

## Your data and privacy

### Where is my data?

On your computer. The task list is the file `agent.db` and the options are `settings.json`, in `%LOCALAPPDATA%\app.inboxagent.desktop`
on Windows. Mail passwords and an API key are in your system keychain, never in a file. See [Privacy](../PRIVACY.md) and
[damaged database and recovery](runbooks/damaged-database-and-recovery.md).

### What leaves my computer?

Mail traffic to your own provider. Only if you choose them: details of each new message to Anthropic (Claude with your own key), the
sender and a short action text to your ntfy server, and a search for a reply to your own provider (follow-up reminders). There is no
telemetry and no account with us. See [Privacy](../PRIVACY.md).

### Is my mail encrypted on disk?

No. `agent.db` holds subjects, short excerpts and up to 3,000 characters of message text, and `settings.json` holds the phone hub's
access code. Use full-disk encryption. See [Security](../SECURITY.md#residual-risks-and-limits).

### How long does it keep things?

Finished tasks and their drafts 90 days, the log 30 days, your corrections 365 days, follow-up records 60 days, and restored entries
waiting for mail that has not been read 90 days. Open tasks are never deleted. See [Privacy](../PRIVACY.md) and the
[data model](architecture/data-model.md).

### What does "Copy diagnostics" send?

Nothing. It puts text on your clipboard, with names, subjects and addresses removed, for you to read and paste. See
[crash log and diagnostics](runbooks/crash-log-and-diagnostics.md).

### What happens if I uninstall?

The program goes. The data folder and your keychain entries stay. Use **Settings**, **Sign out and reset** before you uninstall to remove
them. See [damaged database and recovery](runbooks/damaged-database-and-recovery.md) and [Privacy](../PRIVACY.md).

### How do I delete everything?

**Settings**, **Sign out and reset**, then press it again. It removes the passwords, the list, drafts, rules, choices, the log and the
crash log. Your ntfy topic and ranking choice stay, and backup and `.ics` files in Downloads stay until you delete them.

## Mailboxes

### Why does it ask for an app password?

The app signs in over IMAP with an address and a password, and has no web sign-in. An app password is made by your provider for one
program, works for IMAP, and can be cancelled without changing your main password. See [connect a mailbox](guides/connect-a-mailbox.md).

### Why is Outlook / Microsoft 365 not supported?

Microsoft turned off password sign-in for IMAP, and the app has no "sign in with Microsoft" flow. The window lists the entry as "not
supported yet". See [connect a mailbox](guides/connect-a-mailbox.md#what-is-not-supported).

### Which providers work?

The window lists Gmail, iCloud Mail, Yahoo and Fastmail, and **Other IMAP server** takes any provider with IMAP over TLS and passwords.
The code is tested against a scripted mock server, not yet against a live mailbox of each provider. See
[Security](../SECURITY.md#residual-risks-and-limits).

### How many mailboxes can I use?

Five. They merge into one list, each task carries a small tag, and each mailbox has its own connection and password. See
[connect a mailbox](guides/connect-a-mailbox.md).

### Does it change or mark my mail?

Not by default. It reads with a read-only connection and does not mark mail read. If you switch on "Let Inbox Agent file mail" it
archives, stars and labels (labels on Gmail only) the messages you act on, and saves drafts. It never deletes a message you have not
archived (if the server cannot move one, it copies it to Archive and then removes the original from the inbox), never marks read, never
sends. See [let the app file mail](guides/let-the-app-file-mail.md).

### Does it read all my old mail?

No. A new mailbox starts with its newest 50 messages, then reads what arrives. The app reads the Inbox only: mail your provider files
elsewhere is not read. See [list is empty or stale](runbooks/list-is-empty-or-stale.md).

### How do I move to a new computer?

On the old one, **Settings**, **Save a backup file**. On the new one, finish the setup wizard, then **Restore from a backup file**, check
the preview and press **Restore**. Paste each restored mailbox's app password. See [backup and restore](guides/backup-and-restore.md).

## Using the list

### What is "left your inbox"?

A task whose message is no longer in the Inbox, because you archived or moved it somewhere. It stays open, tagged **Left inbox**, in the
fold "Show left your inbox · N", and comes back to the main list if the message returns. See
[list is empty or stale](runbooks/list-is-empty-or-stale.md).

### How do I undo?

Press `z` or the **Undo** button that appears for a few seconds. It takes back done, star, priority, snooze, due date and title changes
and whole batches. Muting a sender is the exception: open a task and choose **Normal**. Finishing a repeating task of yours cannot be
undone. See [rules and sender choices](guides/rules-and-sender-choices.md) and [keyboard shortcuts](reference/keyboard-shortcuts.md).

### Why was this ranked high or low?

Open the task: the **Signals** and **Why** rows say. Without a model the ranking comes from who you write to, replies, bulk-mail
headers and your own rules and corrections. See [choose how mail is ranked](guides/choose-how-mail-is-ranked.md).

### Does it learn from me?

Yes, on your computer. Each time you change a priority the app records a correction, and with the built-in rules two or more
corrections of a sender in one direction move that sender's later mail one or two steps. See
[rules and sender choices](guides/rules-and-sender-choices.md).

### Why did a task close by itself?

The app saw a reply from you in your Sent folder: "Closed automatically because you replied." Press `x` or the circle to reopen it. See
[the life of a message](architecture/message-lifecycle.md).

### What is a "Waiting" task?

A follow-up reminder: a conversation you started that nobody answered in the days you chose. It is off by default (**Settings**,
**Follow-ups**) and closes itself when a reply arrives. The app only asks your mail server whether a reply exists and never sends
anything. See [Privacy](../PRIVACY.md).

### Can I search inside messages?

On the computer, yes: search finds words in the stored message text (the first 3,000 characters), not only the subject. On the phone it
searches the sender, address, subject, action, excerpt and note. See [phone and tablet](guides/phone-and-tablet.md).

### Which keys are there?

Press `?` for the card. `j` and `k` move, `x` finishes, `1` to `5` set the priority, `s` stars, `r` opens a reply draft, `h` snoozes,
`v` selects several, `a` adds a task, `/` searches, `z` undoes. See [keyboard shortcuts](reference/keyboard-shortcuts.md).

## Phone, alerts and running in the background

### Is the phone hub safe?

It is plain HTTP protected by a 32-character access code, and it answers only devices on private networks. On a shared Wi-Fi the code can
be read: use your home network or a private VPN, and use **Rotate access code** if in doubt. See [phone and tablet](guides/phone-and-tablet.md)
and the [decision record](adr/0009-phone-hub-plain-http-with-token.md).

### Why did I get no notification?

Priority below 4, quiet hours, the category list, or mail from a mailbox's first read all hold an alert back. See
[alerts and reminders](guides/alerts-and-reminders.md).

### Does it keep running when I close the window?

Yes. Closing the window hides it to the tray and the engine keeps reading mail; choose **Quit** in the tray menu to stop it. See
[the runtime](architecture/runtime.md).

### How do I stop it starting with my computer?

The only switch is "Start with this computer" in the setup wizard. The window has no setting for it afterwards: use your operating
system's list of startup apps (not checked here).

### Does it work offline?

The list stays on screen because it is stored on your computer. New mail cannot arrive: the status bar shows an `error:` line and the app
tries again every 30 seconds. See [mailbox will not connect](runbooks/mailbox-wont-connect.md).

## Next steps

* [Documentation map](README.md)
* [Troubleshooting](troubleshooting.md)
* [Glossary](reference/glossary.md)
