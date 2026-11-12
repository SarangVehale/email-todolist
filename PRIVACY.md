# Privacy

Inbox Agent runs on your computer. There is no Inbox Agent server and no account with us.

**What it reads.** Your mailbox over IMAP (read-only unless you switch on "Let Inbox Agent file mail"),
and the recipients and Message-IDs of your Sent folder to learn who you write to.

**What it stores, on your computer only.**
- A task list: sender, subject, a 400-character excerpt, up to 3,000 characters of the message text (used only to write replies), the ranking, a due date if the message states one, and a short reason.
  Finished tasks (and their drafts) are deleted after 90 days and the activity log after 30.
- Reply drafts you or the AI wrote. They are never sent; saving one to your mailbox's Drafts folder is a separate click and needs "Let Inbox Agent file mail" switched on.
- Your settings (server name, address, alert options, phone-hub access code).
- Your mail password and any API key, in the operating system keychain, never in a file.

**What leaves your computer.**
- Mail traffic to your own mail provider.
- If you choose **Claude with your own API key**: the sender, subject and a short excerpt of each new
  message go to Anthropic to be ranked, and up to 3,000 characters of a message go when a reply draft is written for it. Rules-only and local Ollama send nothing out.
- If you set an **ntfy topic**: the sender name and a short action text of high-priority mail go to
  the ntfy server you configured (ntfy.sh by default). Treat the topic like a password.
- The **phone hub** serves the list only to devices that hold its access code, on your network.

**Deleting everything.** Settings, "Sign out and reset" removes the saved password and the task list.
Uninstalling removes the program; the data folder is `app.inboxagent.desktop` under your user
profile's application-data location.
