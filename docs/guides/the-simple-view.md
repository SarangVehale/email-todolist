# The simple view

> **Status:** current · **Audience:** anyone who finds the dense list too much, and whoever sets the app up for them · **Last reviewed:** 2026-11-15

By the end of this guide you will know what the simple view shows, how to turn it on or off, how to make the text bigger, and where
everything that it hides has gone. It takes about two minutes.

## What you need

* Inbox Agent with a mailbox connected. Nothing else.

## What changes

A new install starts in the simple view: setup turns it on when you press **Open my list**. An install that was already in use keeps the
dense view it had until its owner turns the simple one on. The choice belongs to the device (the window's own storage keeps it), so a laptop
and a phone can differ. The phone hub always shows the dense view for now.

| Dense view | Simple view |
| --- | --- |
| Tabs To-do, Starred, Snoozed, Done, Log | **Your list**, **Later**, **Done** |
| A heading "To-do" and "Inbox · Sat 15 Nov" | "Your list" and the date in full |
| Section names "01 // Now", "02 // Today", "03 // This week", "04 // Later" | "Needs you now", "Needs you today", "Can wait", "Not important" |
| Dot-matrix counters, a rail of dots, a filter bar, search, sort, select mode, a row of key caps | None of them; a sentence says how many messages need you |
| A circle, a star, five priority dots and small print on each row | A card: what it is, who it is from and how long ago, and two big buttons **Done** and **Later** |
| An opened row with signals, priority, snooze, sender choices and reply drafts | Who it is from, why it is in the list, the start of the message, and buttons to read all of it and (Gmail) to open it in Gmail |
| Undo shows for five seconds | Undo stays until you do the next thing |
| `error: …` in the status line and in a mailbox row of Settings | A sentence: what to do ("Your mail server did not accept the saved code. Open Settings and paste a new one.") or that Inbox Agent will try again by itself ("Couldn't reach Gmail. Check that this computer is online, then try again. Inbox Agent will try again by itself.") |
| "Watching your inbox" | "Watching your mailbox. Last read at 2:32 PM." (the time is when the mailbox last answered: it is renewed about once a minute while the connection is up) |
| White text on black, 15-pixel text | Dark text on a light page, large text |

Nothing is removed from the app. The keyboard shortcuts still work in the simple view (they are listed in
[keyboard shortcuts](../reference/keyboard-shortcuts.md)), except `/`, whose search box is hidden there, and everything the view hides is back when you turn it off.

## Steps

### What the two buttons do

1. **Done** hides the message from your list. With "Let Inbox Agent file mail" off (the default) your mailbox is not touched; see
   [let the app file mail](let-the-app-file-mail.md). In the **Done** tab the same button says **Put back**.
2. **Later** puts the message away until nine o'clock tomorrow morning (the same as the snooze preset "Tomorrow"). It is in the **Later** tab and comes back by itself; there its card has **Bring back now**.
3. If you press the wrong one, **Undo** is at the bottom of the window. It stays there until you do the next thing.

The first time, a card at the top of the list says this in five short sentences. **Got it** closes it for good on this device.

### Make the text bigger

1. Press **Settings** (or `,`).
2. Under **How it looks**, press **Normal**, **Large** or **Huge**. The simple view starts at Large (text 20 percent larger); Huge is 45 percent larger;
   Normal is the size of the dense view. The change is at once.

### Turn the simple view off or on

1. Press **Settings**. Under **How it looks**, tick or untick **Simple view**. The list changes at once.
2. **Light colours** is a switch of its own, in either view: dark text on a light page, or light text on black. The simple view starts with it on.

### Settings in the simple view

Settings shows **How it looks**, **Your mailboxes** (with **Add another mailbox**), **Alerts** ("Tell me when something important arrives") and, when
the app is the installed one, "Open Inbox Agent when I turn on this computer". **More options** shows the whole page: ranking, alert times, reply
drafts, follow-ups, your rules, filing mail, backup, the phone hub and **Sign out and reset**. **Back to simple settings** returns.

A mailbox whose code stopped working says "Needs a new code. Paste it below." (the dense view says "Needs a new app password").

## Check that it worked

The list heading reads **Your list**, each message has **Done** and **Later**, and the page is light. Turning **Simple view** off brings back the
circles, stars and dots.

## What the view stores

Four values in the window's own storage (`localStorage`), none of them sent anywhere: `ia.easy` (`1` or `0`), `ia.size` (`normal`, `large` or `huge`),
`ia.theme` (`light` or `dark`) and `ia.welcomed` (`1` once the card is dismissed). Clearing the window's stored data brings the defaults back.
**Sign out and reset** does not touch them: whoever signs out and sets up again keeps this device's look.

## If something goes wrong

| What you see | Why | What to do |
| --- | --- | --- |
| The list is dense on a computer where you expected the simple view | The install was in use before the simple view existed, or you turned it off | **Settings**, **How it looks**, tick **Simple view** |
| There is no Starred or Log tab | The simple view has neither | Turn **Simple view** off. Starring still works with `S` |
| "Open in Gmail" is not shown | Only Gmail mailboxes have it, and only mail that has a Message-ID | Use **Read message** |
| The page looks too big for a small window | Large and Huge text scale everything | **Settings**, **Normal** |

## Next steps

* [Connect a mailbox](connect-a-mailbox.md)
* [Keyboard shortcuts](../reference/keyboard-shortcuts.md)
* [Design: Phase 12](../designs/phase12.md)
