# Manual test plan (live mailbox)

The automated suite never touches a real mailbox. These cases need your own account. Use a throwaway message wherever
a case changes mail. Record the result and, for any failure, the **Log** tab text (never the password).

**Setup:** install the build, run the wizard, use "Rules only", keep "Let Inbox Agent file mail" off until MT-20.

## A. Connection and sign-in

| ID | Case | Steps | Expected |
| --- | --- | --- | --- |
| MT-01 | Valid app password | Wizard → Gmail → address + app password → Test connection | "Connected · Sent folder: …" |
| MT-02 | Wrong password | Same with a wrong password | Clear red error; Continue stays disabled |
| MT-03 | IMAP disabled | Turn IMAP off in Gmail, test | A readable error, not a hang (15 s connect / 60 s read limit) |
| MT-04 | Spaces in the password | Paste the 16 characters with spaces | Accepted (spaces are removed) |
| MT-05 | No network | Disconnect Wi-Fi, start the app | Status shows an error; reconnects within about 30 s of the network returning |
| MT-06 | Microsoft account | Choose Outlook | Entry says it is not supported yet; connection is refused with a clear message |
| MT-07 | Non-Gmail provider | iCloud / Yahoo / Fastmail with an app password | Connects; Sent folder is found |
| MT-08 | Non-English mailbox | Account whose folders are localised | Sent / Archive / Drafts are still found (special-use flags) |

## B. Reading and ranking (write-back OFF)

| ID | Case | Expected |
| --- | --- | --- |
| MT-10 | First sync | Latest 50 messages become cards; **none** become unread→read in Gmail |
| MT-11 | New mail while the app is open | A card appears within seconds; priority 4+ raises a notification |
| MT-12 | New mail while the app is closed | Appears on next start; alerts for it still fire (not a first-run backfill) |
| MT-13 | Reply to something you sent | Ranked high ("Replying to you") |
| MT-14 | Newsletter / noreply | Ranked low, category notification or promo |
| MT-15 | HTML-only email | Card has readable excerpt text, no tags or entities |
| MT-16 | Very large attachment mail | Skipped (log says "larger than 5 MB"), sync continues |
| MT-17 | Message with no Message-ID, empty body, emoji subject | Appears; nothing crashes |
| MT-18 | Spoofed sender (if you can send one) | "Known sender" trust is withheld when the provider reports a failed check |
| MT-19 | Restart the app | Cards, drafts and snoozes are all still there |

## C. Write-back (turn it on in Settings; use ONE throwaway message)

| ID | Case | Expected in Gmail |
| --- | --- | --- |
| MT-20 | Mark done | Leaves the Inbox, gains label `AI/Done`, still in All Mail |
| MT-21 | Undo | Returns to the Inbox |
| MT-22 | Set priority 5 | Label `AI/P5` (and no other `AI/P#`); starred |
| MT-23 | Category label | `AI/Work` (or the matching category) appears on new mail |
| MT-24 | Star / unstar | Star appears / disappears |
| MT-25 | Non-Gmail provider: done | Message moves to the Archive folder |
| MT-26 | Switch write-back off, act, switch on | Queued changes apply once, in order |
| MT-27 | Two actions quickly | Both land; nothing is duplicated |
| MT-28 | Network drops mid-change | The change retries; after 5 failures it is dropped and logged |
| MT-29 | An identical-looking Message-ID decoy | Only the exact message changes |

## D. Drafts (never sent)

| ID | Case | Expected |
| --- | --- | --- |
| MT-30 | `r` on a task that needs a reply, rules mode | Template draft with the sender's first name and your signature |
| MT-31 | Same with Ollama / your own key | AI draft, labelled "Written by AI"; if the model is unreachable, a template with a notice |
| MT-32 | Edit the draft, then Rewrite | Asks "Replace your edits?" first; your text survives a cancel |
| MT-33 | Save to Drafts | Appears in the Drafts folder, threaded under the original, **not sent**, flagged as a draft |
| MT-34 | Save twice quickly | Only one copy appears |
| MT-35 | Message with a different Reply-To | Red warning shown; saving needs a second click |
| MT-36 | Draft containing a link | Header says "contains a link" |
| MT-37 | Login name that is not an email address | Clear message; nothing is saved |
| MT-38 | Auto-draft on, daily limit 2 | At most 2 AI drafts per day, one at a time |
| MT-39 | Sign out | Drafts, log and sender preferences are all gone |

## E. List, sort, snooze, preferences

| ID | Case | Expected |
| --- | --- | --- |
| MT-40 | Category chips, Reply and Drafts chips | Filter correctly; counts match |
| MT-41 | Search, then clear with Esc | Filters live; focus stays in the box |
| MT-42 | Sort by due date | Overdue items first; section headers hidden |
| MT-43 | Snooze 3 hours / tomorrow / next week | Leaves the list, appears in Snoozed, returns with a notification |
| MT-44 | Snooze across midnight, month end, DST change | Wakes at 09:00 local the right day |
| MT-45 | "Always important" / "Mute" a sender | Existing and future mail from them re-ranks |
| MT-46 | Done all (filtered view) | Needs a second click; one Undo restores everything |
| MT-47 | Keyboard | `j k x 1-5 s r h / z` all work; nothing fires while typing in a box |
| MT-48 | Quiet hours 22:00–07:00 | No alert for normal mail, urgent mail still alerts |
| MT-49 | Daily digest | One notification per day after the chosen time, even if the app was closed at that time |

## F. Phone and tablet (phone hub)

| ID | Case | Expected |
| --- | --- | --- |
| MT-50 | Scan the QR on the same Wi-Fi | List opens; the access code disappears from the address bar |
| MT-51 | Mark done / priority / snooze from the phone | Shows on the desktop within 4 s |
| MT-52 | Edit a draft on the phone | Saved; no "write" or "save to Drafts" buttons are offered |
| MT-53 | Rotate the access code | Old phone link stops working immediately |
| MT-54 | 10 wrong access codes | Further attempts from that address are refused for a minute |
| MT-55 | Turn the hub off and on | A new access code is issued |

## G. Install and lifecycle

| ID | Case | Expected |
| --- | --- | --- |
| MT-60 | Install | Per-user, no admin prompt, Start Menu entry |
| MT-61 | Close the window | App keeps running in the tray; alerts continue |
| MT-62 | Tray → Quit | Process ends |
| MT-63 | Start with the computer on | Starts hidden in the tray after sign-in |
| MT-64 | Second launch | Focuses the running window instead of starting a second copy |
| MT-65 | Uninstall | Program, shortcut and registry entry removed; data folder remains (documented) |

## H. Select many, your rules, follow-ups

| ID | Case | Expected |
| --- | --- | --- |
| MT-70 | Press `v`, move with Shift+`j`, press `x`, then `z` | The rows between are selected, all go to Done, one undo brings them all back |
| MT-71 | Select rows from three senders, press "Mute senders" twice | Asks twice, all three senders show "Muted", their open mail drops to priority 1 |
| MT-72 | Add a rule "Domain is <a real sender's domain> then Priority 5", save, send yourself a mail from it | New mail arrives at priority 5 with "Your rule: ..." in its reason |
| MT-73 | "Apply to my list" with that rule | Shows how many would change, needs a second click, changes only tasks you have not corrected |
| MT-74 | A rule "File away" with write-back on | New matching mail appears in Done and is archived in the real mailbox, with no alert |
| MT-75 | Turn follow-ups on (2 days), write a new mail to a second address, wait | After two days a "Waiting" task appears; replying from the other address closes it ("They replied") |
| MT-76 | Gmail: the reply was archived straight away | The conversation is still found as answered (the All Mail folder is searched) |
| MT-77 | A server that refuses header searches | No follow-up tasks appear, and the Log says once that replies could not be checked |
| MT-78 | Turn follow-ups off while one is open | No search is sent any more; the task stays until you close it |
| MT-79 | On Gmail, with a Waiting task open, send yourself a nudge inside the same thread | The task stays open: your own mail is not an answer |
| MT-80 | The person you wrote to has an out-of-office reply | It does not close the task or stop one being made |
| MT-81 | A rule "Address is" with a look-alike address in the From line | The look-alike mail is not raised by the rule |

## I. Reminders, search inside messages, calendar invitations

| ID | Case | Expected |
| --- | --- | --- |
| MT-82 | Add a task, open it, set a reminder two minutes ahead | A desktop notification appears once at that minute, and not again after a restart |
| MT-83 | Close the app, let a reminder pass, start the app | The overdue reminder is announced at start; five overdue at once give one notice |
| MT-84 | A weekly reminder: press `x` on it | It stays in the list with next week's date and time; there is no undo |
| MT-85 | A monthly reminder on the 31st, finish it in January | Next one is 28 or 29 February, then 31 March |
| MT-86 | Set a reminder on a mail task | There is no reminder control; snooze is what mail has |
| MT-87 | Search for a word that is in the body of a message but not in its first 400 characters, subject or sender | The message is found and tagged "In text"; typing on quickly never shows an older answer |
| MT-88 | Search with quotes, `OR`, `*`, `body:x` | Nothing breaks; they behave as plain words |
| MT-89 | Sign out, then look at the data folder | No message text remains in the database files |
| MT-90 | Open the app after upgrading from the earlier build | Old mail is searchable straight away; a copy `agent.db.bak` was made first |
| MT-91 | A Google invitation (UTC time) and an Outlook one (a named zone) | The Google one shows your local time; the Outlook one shows the hour as written with its zone's name; "Save .ics" writes a file that opens in your calendar at the right time |
| MT-92 | Save the same invitation twice | A second file with `-2` in its name; the first is untouched; adding both does not make two events |
| MT-93 | A cancelled invitation | It says cancelled and offers no save |
| MT-94 | An invitation with a Windows time zone name | The time is saved as a floating local time, not in a wrong zone |
| MT-95 | An Outlook invitation sent from another time zone | The saved file opens in your calendar at the hour the sender meant (the zone was understood) |
| MT-96 | A recurring invitation | The card says only the first time is kept |
| MT-97 | A due-date-only monthly task (rent): finish it a few days early | It moves to next month's date and stays in the list |
| MT-98 | Change only the repeat of a reminder that already fired | The change is accepted and nothing fires again |

## J. Several mailboxes

| ID | Case | Expected |
| --- | --- | --- |
| MT-99 | Settings, "Add another mailbox", a second real mailbox with its app password, Test, Add | Its mail appears within a minute with a small tag with its name (hover shows the address); the first mailbox's tasks gain a tag too |
| MT-100 | Add a second mailbox at a provider other than the first (for example Gmail and iCloud) | Each is read with its own login; only the Gmail one offers "Copy Gmail link" |
| MT-101 | Add two Google accounts; on a task of the second, press "Copy Gmail link" and open it | It opens that message in the second account (the link names the address), not the browser's first Google account |
| MT-102 | Filter chips: choose a mailbox, then All | Only that mailbox's tasks, then everything; your own tasks show under All only |
| MT-103 | Turn on "Let Inbox Agent file mail", mark a task of the second mailbox done | It is archived in the second mailbox only; a task of the first mailbox is untouched |
| MT-104 | Reply to a message in the second mailbox from the second mailbox | Its task closes ("you replied"); a task of the same subject in the first mailbox stays |
| MT-105 | Give the second mailbox a wrong app password (or revoke it at the provider) | The first mailbox keeps working; Settings says what is wrong with the second; pasting a new password there resumes it without a restart |
| MT-106 | Remove the second mailbox | Its tasks vanish from every tab; the mailbox itself is untouched; its keychain entry (`imap-password-1`, service InboxAgent) is gone; the first mailbox and your own tasks stay |
| MT-107 | Add the same address and server twice, or a sixth mailbox | Each is refused with a plain message |
| MT-108 | Remove the first mailbox while a second exists | The second keeps working; "Sign out and reset" afterwards removes every password |
| MT-109 | Open the phone hub with two mailboxes | The same list, no tags, no mailbox names anywhere in the page or its data |
| MT-110 | Upgrade from the earlier build with an existing list | Nothing moves, no task is lost, the old mailbox is mailbox 0 and shows no tag until a second is added |
| MT-111 | A second mailbox with a large inbox, first connect | The first load takes only the newest messages (as for the first mailbox) and the app stays responsive while both read |

## K. Backup and restore

| ID | Case | Expected |
| --- | --- | --- |
| MT-112 | Settings, "Save a backup file" | A new `inbox-agent-backup-<date>-<time>.json` appears in Downloads and the path is shown; saving again makes a second file and leaves the first |
| MT-113 | Open the file in a text editor | Readable JSON; it holds your rules, tasks and notes; it has no password, no API key, no phone code, no notification topic and no message body (only a correction you made holds the sender and subject of its mail); a reply draft the AI wrote is not in it |
| MT-114 | Set up the app on a second computer (or after "Sign out and reset"), then "Restore from a backup file" and choose the file | A preview lists what will be added; "Restore" adds it; your rules, own tasks (with reminders and repeats), notes and sender choices are back |
| MT-115 | Restore before the first sync has finished, then wait for your mail to load | Mail you had finished stays finished, starred mail is starred, your notes and drafts are on the right messages |
| MT-116 | Restore the same file a second time | The preview says nothing is new; nothing changes |
| MT-117 | Restore on a computer that already has rules, notes and finished mail | What you have is kept; only what is missing is added (a later snooze in the file replaces an earlier one; a sender you chose "important" or "mute" changes that sender's open mail) |
| MT-118 | Tick "Also restore options and mailboxes" | Alert times and reply-draft options come back; each extra mailbox is listed in Settings as needing its app password; no mailbox starts reading until you paste one |
| MT-119 | Choose a file that is not a backup (a photo, a text file, an empty file) | A plain message says it is not a backup; nothing changes |
| MT-120 | Edit a copy of the file so that one rule has no text, then restore it | The whole restore is refused with a message; nothing was added |
| MT-121 | A backup from a newer version of the app | The message says to update the app |
| MT-122 | Restore with write-back switched on | Nothing is archived or flagged in the mailbox because of the restore |
| MT-123 | A reminder that was due last week and a weekly repeat, in the file | The past reminder is left out; the repeat is on its next time |
| MT-125 | Tick "Also restore options and mailboxes" with a file that asks for ranking with Claude | The preview says that is not restored; your ranking mode is unchanged |
| MT-126 | Restore a file whose mailbox names a server you do not know | The preview lists that mailbox and its server before you confirm |
| MT-127 | Save a backup on a computer with a very large history | The message says how many entries did not fit |
| MT-124 | A backup made on a phone-hub computer, restored on another | The second computer's phone hub stays off and its write-back setting is unchanged |

## L. A big mailbox

| ID | Case | Expected |
| --- | --- | --- |
| MT-128 | With thousands of open tasks (a long-used mailbox, or a copy of one), open the app | The list appears at once, with 150 rows and "Show 150 more" and "Show all N more" under them (N is how many are still hidden) |
| MT-129 | Hold `j` down past the 150th row | More rows are drawn as the cursor reaches them (a short pause each time 150 more are drawn); the cursor never leaves the screen |
| MT-130 | Click "Show all N more" on a long list | Every row is drawn (a moment's pause is fine); the keyboard still works |
| MT-131 | Change the tab, a category chip, the search text or the sort | The list starts again at 150 rows (more if the cursor's task is still shown further down) |
| MT-132 | Open the Done tab with more than 200 finished tasks | It says "The 200 most recently finished of N finished tasks" and offers "Show 500 older"; the tab's number is N; "Show 500 older" adds older ones; leaving the tab and coming back shows 200 again |
| MT-133 | Use "Done all automated" (or select mode, "All", "Done") on more than 200 tasks, wait a few seconds, then press Undo | Every task comes back, not only the ones the window still holds |
| MT-134 | Mark a task done and press Undo | It comes back into the list (the cursor stays where it is) |
| MT-135 | Minimise the window for a minute, then bring it back | The list is up to date at once; the app used almost no processor while hidden |
| MT-136 | Press Tab until a row is focused, then `j` several times and `x` | The highlighted row moves down each time, and `x` finishes the highlighted row |
| MT-137 | Open "Show automated" on a long list, click a row in it, then "Show 150 more" under the fold | The page does not jump; the row opens where it is; only the fold gets more rows, not the list above it |
| MT-138 | In select mode click a row, then shift-click a row further down | The rows between them are selected, and no row that is not drawn |
| MT-139 | On a clean Windows computer with no earlier settings, start the installed app, press Get started, type a Gmail address, press Continue, then press the button "Open Google's page" | The default browser opens Google's app-passwords page; the window says that page should be open |
| MT-140 | Make a code there, copy it, come back and press Paste (Windows may ask to allow reading the clipboard) | The window connects by itself and moves to "You're connected."; the list starts to fill. If Windows refuses the clipboard, the window says to press Ctrl and V and the pasted code connects with Connect |
| MT-141 | Do the same with a wrong code (change one letter) | "That code didn't work. Make a new one with the button above and paste it here." and nothing is saved; Details for a helper shows the server's own words |
| MT-142 | Repeat MT-139 to MT-140 with Yahoo, iCloud and Fastmail addresses | The button opens that provider's page; the steps written beside it match what the page shows (note any that have drifted) |
| MT-143 | In a build with no Microsoft client ID, type an Outlook or Hotmail address | The screen says it cannot be connected yet, and offers a way back |
| MT-144 | After setup close the window with its X | The app keeps running (its icon is near the clock); clicking the icon brings the list back. The last setup screen said so |
| MT-145 | In the simple view press Done on a message, wait ten seconds, then press Undo | Undo is still there and brings the message back |
| MT-146 | In the simple view press Later on a message | It leaves the list, is in the Later tab, and returns at nine the next morning |
| MT-147 | On a Gmail message in the simple view open it and press Open in Gmail | The browser opens that message in the right Google account |
| MT-148 | Set the text to Huge in Settings on a 1366 by 768 laptop screen | Nothing important is cut off; the Done and Later buttons can be pressed; the list scrolls |
| MT-149 | Ask a person who has never used the app, aged 70 or more, to set it up from the downloaded installer to a filled list, with nobody helping | Note where they stop, what they say, and how long it took. This is the test the design cannot replace |
| MT-150 | With a Google client ID in `oauth.rs`, type a Gmail address, press Continue and press Sign in with Google | The browser opens Google's page with the address as the hint; after you allow it the tab says it can be closed, and the window moves to "You're connected." Mail is read |
| MT-151 | Do it with an account that is not yours to untick the mail permission on Google's page | "Mail access was not allowed…" and nothing is saved |
| MT-152 | After MT-150 remove the app at myaccount.google.com/permissions, then wait for the next sync | The status bar says "your account's sign-in was refused. Sign in again in Settings" and the mailbox row says it needs you to sign in again; Settings, Sign in again, works; the list is untouched |
| MT-153 | Remove the mailbox in Settings | The app disappears from myaccount.google.com/permissions (it was revoked) and no entry `oauth-refresh-0` is left in the system keychain |
| MT-154 | With a Microsoft client ID, type an Outlook or Hotmail address and press Sign in with Microsoft | A code is shown, Microsoft's page opens, the code is accepted, and the window connects; mail is read |
| MT-155 | Leave a Google sign-in waiting for six minutes (a Microsoft one until its code runs out, up to 15 minutes), then try again | "The sign-in took too long. Try again."; a new sign-in works |
| MT-156 | Leave the app running for more than an hour with a signed-in mailbox, then receive a message | It arrives without any prompt (the token was refreshed by itself) |
