# Phase 2: make it a real to-do list

* **Date:** 2026-11-13
* **Status:** built. An independent review (Opus) changed several decisions before any code was written; the
  sections below describe what was built and, where they differ from the first draft, say why.

## Problem

Three independent reviewers (product, security, competitor survey) found the same gap. In the default rules mode the
list is a "people you write to come first" sort, not a to-do list:

1. A task stays open after the user has dealt with the mail somewhere else (replied from a phone, archived in Gmail).
2. A six-message thread is six tasks.
3. Corrections do almost nothing in rules mode. The log says "saved as a correction", but only the AI prompt sees the
   last ten, and only from the time the watcher started.
4. Rules mode never produces "urgent" and never produces priority 5, so the "Now" section and the urgent count stay empty.
5. Newsletters count as open tasks. The full text of a message cannot be read in the app.

## Constraints that do not change

No paid service. The app never sends mail. Data stays on the computer. Mail is read-only unless the user turned on
write-back. The default (rules only) must stay useful with no AI.

## Design (as built)

### 1. Tasks that close themselves

After each sync, and every two minutes while idle, the engine compares the open mail tasks (never the ones the user added)
with the mailbox.

* **Replied.** A message in the Sent folder whose **In-Reply-To** is the task's Message-ID closes the task with
  `closed_by = replied`. References does not count: it lists the whole conversation, and replying to the newest message does
  not answer the older ones (a second review found this). A forward (subject starting Fwd, FW, WG, TR, RV, ENC, Doorst, VS or
  VB) does not count, because forwarding is not dealing with a message. The Sent index is topped up with only the new Sent messages, and read again from scratch if the folder's
  UIDVALIDITY changes. Closing this way never queues a mailbox change. Reopening such a task sets `keep_open`, queues no
  "unarchive", and the next pass leaves it alone.
* **Left the inbox.** Tasks whose UID is no longer in INBOX are marked `gone` but stay open. They appear in their own fold,
  "Left your inbox", not in Done (a message moved by a filter or archived on purpose to snooze it is not finished). The mark
  clears if the message comes back. Review change: the first draft closed these tasks.
* **Unknown means nothing.** A server error, a FETCH reply with no UIDs, a changed inbox UIDVALIDITY, or a whole batch of at
  least five tasks vanishing while the inbox still holds other mail all change nothing. Failures here are logged and never
  drop the connection that delivers new mail.

### 2. Conversations

Each task has a `thread` key: the first id in the raw References header (read before the stored list is cut to ten), else the
id replied to, else its own Message-ID. Mail with no usable id, and tasks the user added, get `uid:<id>` and are never grouped.
Existing rows are not backfilled; they stay ungrouped. The list shows one row per conversation (the newest message) with the
highest priority and a count; it shows what is true of any message in the conversation (urgent, due, needs a reply, starred)
and is folded as automated or as left-the-inbox only when every message is. The priority dots show the newest message's own
priority (clicking them changes that message), with an "Earlier P4" tag when an older one ranks higher. Done, star and snooze
apply to the messages of that row from the same filtered list the user is looking at (search, category and the Starred tab
are respected), no newer than the row; priority applies to the row's own message only. A new message wakes a snoozed conversation, unless that message failed its sender check. Review
changes: separate stored root, message ids written without spaces are read, actions are limited to what was on screen, the
cursor moves to the nearest row (or stays on the conversation) when its row disappears.

### 3. Learning from corrections

Corrections are stored with the sender's address (not the display name) and the task, one per task: its first priority and
its latest. Nudging the same mail twice is still one correction, and walking it back to where it started (undo) removes it. Corrections of mail that failed its sender check
are not recorded. For a sender, the last three corrections are read: with at least two, all in the same direction, later
mail from that address moves by their rounded average (one or two steps, never outside 1 to 5). It applies to any message the rules ranked, including one where a
model failed and the rules took over; never to a sender with a saved preference; never to mail that failed its sender
check; and it never sets urgent.
The task shows "Learned +1" and the why line says so; setting the sender to Normal forgets everything learned.

### 4. Priority 5 in rules mode

A message gets priority 5 only when it is not bulk, did not fail its sender check, comes from someone the user writes to or
replies to the user, contains a question, and has an extracted due date of today, tomorrow or earlier. It never sets
"urgent" (the flag that bypasses quiet hours). Review change: the first draft also promoted messages containing words like
"urgent" and set the urgent flag, which would let anyone who can send mail trigger an alert through quiet hours. Words in a
message now count for nothing.

### 5. Newsletters fold away (already in place)

Bulk mail at priority 1 or 2 that the user has not touched (no star, no draft anywhere in the conversation, not
urgent, no due date) collapses behind a "Show automated" button, with "Done all automated" beside it; it is not counted
in the header.

### 6. Reading the mail

A desktop-only `read_body` returns the stored text (up to 3,000 characters). The phone gets the excerpt only. For Gmail
accounts a button copies a Gmail search link. Clicking text in an open row no longer closes it.

### 7. Smaller fixes

First run says "Reading your latest mail" while the first sync runs, and the write-back queue is labelled "to file". Undo
looks tasks up by id when it runs. Discarding an edited draft asks first. Due dates are editable on any task and titles on
tasks the user added; "snooze until" takes a date; a task due today or earlier sorts as at least priority 4, but only when someone is waiting on an
answer (it needs a reply or comes from a sender you write to, and is not bulk), so a newsletter saying "offer ends
today" cannot climb; the daily digest shows these as their own "due" figure. Dialogs get roles and labels, move focus in and back, keep Tab inside, and close on Escape (not the first-run
wizard); toasts are announced; wizard choices are real buttons and labels are bound to fields; dimmed rows no longer fall
below readable contrast; reduced motion is respected.

Not done on purpose: removing the automatic star on priority 4 and 5 (it also drives the Gmail "flagged" mark, so it is a
product decision), applying a priority change to a whole conversation, AI-mode learning (the model already sees corrections).

## Out of scope

Multiple accounts, calendar invites, AI summaries, swipe gestures, any change to how mail is sent (it is not).

## Test plan

Each item has a failing test first. Auto-close and threads run against the scripted IMAP server in
`engine/tests/imap_flows.rs`. Learning, urgency and thread keys are tested in `engine/tests/phase2.rs`.
UI behaviour is checked in the browser demo and in the packaged app.
