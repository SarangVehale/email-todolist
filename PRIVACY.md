# Privacy

Inbox Agent runs on your computer. There is no Inbox Agent server and no account with us.

**What it reads.** Your mailbox over IMAP (read-only unless you switch on "Let Inbox Agent file mail"),
and the recipients, Message-IDs, subjects and dates of your Sent folder to learn who you write to and, if you turn on
follow-up reminders, which conversations you started.

**What it stores, on your computer only.**
- A task list: sender, subject, a 400-character excerpt, up to 3,000 characters of the message text (used to write replies, and shown when you press "Read message"), the ranking, a due date if the message or an invitation's event states one, up to five attachment names (never the files), the message's unsubscribe link, your own notes, tasks you add yourself, and a short reason.
  Finished tasks (and their drafts) are deleted after 90 days and the activity log after 30; the clean-up runs when the app starts and once a day while it is open.
- Your corrections (which sender, which subject, and the priority before and after), kept for 365 days and used to rank later mail from that sender; your "always important" and "mute" choices, kept until you change them; and any mailbox changes waiting to be applied.
- A full-text index of the stored message text (subject, sender, address, the first 3,000 characters of the text, and your note), in the same database, so search can find words inside messages. It follows the task list: when a task is pruned, replaced or removed, or you sign out, its words are overwritten in the index rather than just hidden.
- The first calendar event of an invitation (title, time, place, whether it is cancelled), reminder times you set, and your repeats.
- Your rules: the text you typed, the field it looks at and what it does. They stay when you switch to another mailbox and are removed by "Sign out and reset".
- If you turn on follow-up reminders: a record of each sent message that was checked (its Message-ID and a date), removed by the app's daily clean-up after 60 days, and a task naming the recipient and the subject, which follows the usual 90 days once finished.
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
- **Reminders** you set are announced by a desktop notification (when Desktop notifications is on) and, if you set an ntfy topic, by ntfy: the task's title goes to the ntfy server you configured. They ignore quiet hours, because you chose the time.
- The **phone hub** serves the list only to devices that hold its access code, on your own private networks (your home Wi-Fi, a VPN such as Tailscale). It may set reminders on your own tasks (their times are the computer's clock), and it shows an invitation's event. It can search the sender, subject, the first 400 characters and your note, but not the stored message text, and it cannot save calendar files.
- **Follow-up reminders** (off by default) ask your own mail provider whether a reply to a message you sent exists. The
  request is a search, in your Inbox and in All Mail or Archive when your provider has them, carrying that message's
  Message-ID and your own address; it goes only to your mail provider and no message is downloaded for it. A reply the app
  has already read is recognised without asking at all.
- **Update check.** Once the project has a public repository, the app asks api.github.com once a day for the latest release
  (the request carries only "InboxAgent/<version>" and your network address). It never downloads anything. You can turn it
  off in Settings. Until a repository is set, nothing is sent.
- **Copy diagnostics** only puts text on your clipboard. Nothing is sent unless you paste it somewhere. It holds counts, option
  names and recent activity with names, subjects and addresses removed; read it first.

**What the app writes outside its own folder.** Only one thing, and only when you click **Save .ics** on an invitation:
a new `.ics` file in your Downloads folder (your user folder if there is none), never replacing a file that exists, never opened by the app. It holds the event's
title, time and place, built by the app, not copied from the message.

**Turning on "Let Inbox Agent file mail".** While it is off, nothing in your mailbox is touched, but the changes you
make (done, star, priority) are remembered, and so are the changes your rules make (a rule that files mail marks it done, a
rule that sets a priority queues its label). Rules also run on the mail read when you connect a mailbox. Switching it on applies the ones that are still waiting. Sign out first if you
do not want that.

**Deleting everything.** Settings, "Sign out and reset" removes the saved password, the task list, your drafts, the
activity log, your corrections, sender choices and rules, follow-up records, the database backup and the crash log. Your ntfy topic and AI choice
stay in settings.json.
Calendar files you saved stay in Downloads: they are yours. "Sign out and reset" also turns the phone hub off, forgets its access code, and clears the server and address.
Uninstalling removes the program; the data folder is `app.inboxagent.desktop` under your user
profile's local application-data location (on Windows, `%LOCALAPPDATA%`). Use "Sign out and reset" before uninstalling so
the saved password is removed from the keychain too.
