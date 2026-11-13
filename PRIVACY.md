# Privacy

Inbox Agent runs on your computer. There is no Inbox Agent server and no account with us.

**What it reads.** Your mailbox over IMAP (read-only unless you switch on "Let Inbox Agent file mail"),
and the recipients and Message-IDs of your Sent folder to learn who you write to.

**What it stores, on your computer only.**
- A task list: sender, subject, a 400-character excerpt, up to 3,000 characters of the message text (used to write replies, and shown when you press "Read message"), the ranking, a due date if the message states one, up to five attachment names (never the files), the message's unsubscribe link, your own notes, tasks you add yourself, and a short reason.
  Finished tasks (and their drafts) are deleted after 90 days and the activity log after 30.
- Your corrections (which sender, which subject, and the priority before and after), kept for 365 days and used to rank later mail from that sender; your "always important" and "mute" choices, kept until you change them; and any mailbox changes waiting to be applied.
- Reply drafts you or the AI wrote. They are never sent; saving one to your mailbox's Drafts folder is a separate click and needs "Let Inbox Agent file mail" switched on.
- Your settings (server name, address, alert options, phone-hub access code).
- Your mail password and any API key, in the operating system keychain, never in a file.
- A small `crash.log` (the text of an internal error and the code location, never sent anywhere, capped at 100 KB). There is no telemetry.

**What leaves your computer.**
- Mail traffic to your own mail provider.
- If you choose **Claude with your own API key**: the sender, subject and a short excerpt of each new
  message go to Anthropic to be ranked, and up to 3,000 characters of a message go when a reply draft is written for it.
  Each ranking request also carries your ten most recent corrections, which include the sender name and subject of those
  other messages. Rules-only and local Ollama send nothing out.
- If you set an **ntfy topic**: the sender name and a short action text of high-priority mail go to
  the ntfy server you configured (ntfy.sh by default). The daily digest (counts only) and "back from snooze" notices
  (which carry a task's title) go there too, whatever their priority. Treat the topic like a password.
- The **phone hub** serves the list only to devices that hold its access code, on your network.
- **Update check.** Once the project has a public repository, the app asks api.github.com once a day for the latest release
  (the request carries only "InboxAgent/<version>" and your network address). It never downloads anything. You can turn it
  off in Settings. Until a repository is set, nothing is sent.
- **Copy diagnostics** only puts text on your clipboard. Nothing is sent unless you paste it somewhere. It holds counts, option
  names and recent activity with names, subjects and addresses removed; read it first.

**Turning on "Let Inbox Agent file mail".** While it is off, nothing in your mailbox is touched, but the changes you
make (done, star, priority) are remembered. Switching it on applies the ones that are still waiting. Sign out first if you
do not want that.

**Deleting everything.** Settings, "Sign out and reset" removes the saved password, the task list, your drafts, the
activity log, your corrections and sender choices, the database backup and the crash log. Your ntfy topic and AI choice
stay in settings.json.
Uninstalling removes the program; the data folder is `app.inboxagent.desktop` under your user
profile's local application-data location (on Windows, `%LOCALAPPDATA%`). Use "Sign out and reset" before uninstalling so
the saved password is removed from the keychain too.
