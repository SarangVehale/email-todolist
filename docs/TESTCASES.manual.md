# Manual test plan (live mailbox)

The automated suite never touches a real mailbox. These cases need your own account. Use a throwaway message wherever
a case changes mail. Record the result and, for any failure, the **Activity log** tab text (never the password).

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
