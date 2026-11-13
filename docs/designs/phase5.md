# Phase 5: reminders, search inside messages, calendar invites

* **Date:** 2026-11-13
* **Status:** design for Run 2 of the agreed roadmap (see `docs/designs/phase4.md` for Run 1).

## Problem

1. Your own tasks have a due *date* but nothing that tells you when the day comes, and nothing repeats: "take the bins out
   on Tuesdays" has to be added again every week.
2. Search covers the sender, the subject and a 400-character excerpt. The app stores up to 3,000 characters of every message,
   but you cannot find "the one that mentioned the boiler" unless the word happens to be in the excerpt.
3. A meeting invitation arrives as mail and sits in the list as text. Getting it into a calendar means retyping it.

## Constraints that do not change

No paid service. The app never sends mail and never talks to a calendar service. Data stays on the computer. The phone hub
keeps a small command set: anything that reveals message text beyond what a task already shows, or writes a file, stays
desktop-only.

## 1. Reminders and repeats (your own tasks only)

A task you added can have a **reminder** (a local date and time) and a **repeat** (daily, weekly or monthly).

* `set_reminder { uid, at, repeat }`, available from the phone as well. It only edits a task you added yourself (a mail task
  or one the app made for you, such as a follow-up, is refused; `set_due` is different and works on any task). `at` is
  `YYYY-MM-DD HH:MM` in local time, or empty. A new time must not be in the past; the minute you are in still counts. Choosing
  the time the task **already has** (to change only its repeat, say) is accepted even when it has passed, and changes nothing
  else: it is not announced again. `repeat` needs a reminder, or a due date when `at` is empty (a repeat with no reminder only
  moves the due date). Any `at` must be a real date between 2000 and 2100. An empty `at` with an empty `repeat` clears both; an
  empty `at` with a repeat is a repeat on the due date.
* **Time is the clock of the computer that holds your mail.** A phone in another time zone sees the same text, and its
  reminder box says "computer's time".
* **Firing.** The scheduler (every 20 s) finds open tasks of yours whose reminder time has come and that have not been
  announced for that time. Each is announced once through the usual channels (a desktop notification when Desktop
  notifications is on, and ntfy if you set a topic), and the time it was announced for is remembered, so a restart never
  repeats it and a different time announces its own. Reminders ignore quiet hours (you chose the time) and a snooze (it
  hides the task, not the reminder). If the app was closed, what is overdue fires when it starts; up to three get a notice
  each, more become one notice ("5 reminders are due"). Done tasks never fire.
* **Repeating.** Marking a repeating task done (a click, `x`, "Done all N", or a bulk selection) does not move it to Done: it
  stays open with the reminder, and the due date if it had one, moved to the next occurrence, and it says so in the log.
  The next occurrence is the first one after **both now and the current reminder (or, for a repeat that has only a due date,
  that date)**, counted from the *first* occurrence. So a chore finished ahead of time, such as rent paid on the 28th for
  the 1st, moves to the next period rather than staying put; one finished late skips the periods it missed. A monthly task
  set for the 31st lands on the 28th or 29th in February and on the 31st again in March (no drift). A due date that sat after
  the reminder keeps its distance. Daily and weekly add whole days to a wall-clock time, so a clock change never shifts them.
  There is no undo for this: the toast names the new time when one task is finished and says how many moved when several are;
  a second activation while the first is still being saved is ignored.
* A non-repeating task whose reminder has fired is still an ordinary open task.

## 2. Search inside message text

A desktop-only command `search { q }` returns the ids of the tasks that match, best match first, at most 500. It uses
SQLite's built-in full-text index (`FTS5`) over the subject, sender name, sender address, stored message text and your note.

* All words must match, in any order; the last word also matches as a prefix ("boil" finds "boiler"); case and accents are
  ignored. **What you type is never given to the index as query syntax**: it is split into words, each is quoted, so
  `OR`, `NEAR`, column names, quotes and `*` have no special meaning, and a query is at most 100 characters and 8 words.
* **Only the first 3,000 characters of a message are stored, so only those are searchable.**
* Desktop-only because it reveals which messages contain a word, and the phone is not allowed to read message text. The phone
  keeps the list's own search (sender, address, subject, the first 400 characters and your note, word by word).
* The index is kept up to date by database triggers, so every task that is added, changed or deleted is reflected, including
  by "Sign out and reset", pruning and a mailbox renumbering. `PRAGMA recursive_triggers` is on, because the app replaces
  rows and a replace must delete the old index entry. **The index is set to overwrite the words of a deleted entry** (FTS5
  `secure-delete`); without it a delete only hides them until a later merge, and they would stay readable in the database file.
  A database from an earlier version gets the index built the first time it is opened (after the usual one-time copy of
  the database), and `SCHEMA_VERSION` becomes 2.
* In the list, a row that the index found but the list's own word-by-word check (sender, address, subject, action line,
  excerpt, note) did not is tagged "In text".

## 3. Calendar invitations

When a message carries a calendar part (`text/calendar`, `application/ics`, or any part whose file name ends in `.ics`), the
first part that yields a readable event is used: summary, start, end, location, whether it is all day, whether it is
cancelled (`METHOD:CANCEL` or `STATUS:CANCELLED`), and whether it repeats (`RRULE` or `RDATE`). A `METHOD:REPLY` is ignored.

* Nothing is kept unless the event has a valid `DTSTART` (a date or date-time in 2000 to 2100) and ends with `END:VEVENT`.
  An end is kept only if it is the same kind as the start, in the same zone and not earlier; with no end, a `DURATION`
  (`PT2H`, `P1DT2H30M`, `P1W`, at most a year; whole days for an all-day event) gives one. An empty summary becomes
  "(no title)". **Only the first time of a repeating invitation is kept**, and the card says so.
* Time zones. UTC times stay UTC and the card shows them in this computer's time. A `TZID` that is an IANA name (`Europe/Berlin`)
  or a Windows name Outlook writes (`Pacific Standard Time`, about sixty are known) is kept as the IANA name. Any other name
  is not passed on: the time becomes a floating local time and the card says the zone is not known.
* Stored on the task as one small JSON value (readable from the phone as part of the task; only saving the file is
  desktop-only). If the task has no due date, the event's day becomes it, but only when that day is today or later and the
  event is not cancelled; a date the message text already gave is never replaced. **For a UTC time the day is the day where you
  are** (23:30Z on the 14th is the 15th in India); for other times it is the day written.
* Everything is size-limited (a calendar part over 64 KB is ignored; summary, location and zone name are cut at 200, 200 and 64
  characters). Control characters, line and paragraph separators (U+2028, U+2029), zero-width and bidirectional marks are
  removed. Only the first event is read; alarms and anything nested are skipped.
* The card shows the summary, the time (and its zone name), and the place, on separate lines. For an event that is not
  cancelled, **desktop only**, "Save .ics" writes a new file into your Downloads folder (`<home>/Downloads`; your user folder
  itself when there is none) and tells you its name. It never overwrites an existing file and never opens it.
* **The file is built by the app from the fields above, not copied from the message.** Text is escaped as RFC 5545 requires,
  lines are folded at 75 octets, and a line break in a summary or location cannot add a property. The time zone is written
  only if it is `UTC`, `GMT` or an `Area/Place` name from the IANA areas; anything else is written as a floating local time.
  UTC times stay UTC. The unique id is stable for the same message, so saving it twice does not make two events in a
  calendar. The file name is `invite-<words of the summary>-<date>.ics`: lowercase ASCII letters and digits joined by `-`, at
  most 40 characters (`event` if nothing is left), the start's `YYYYMMDD`, and a counter (`-2`, `-3`, up to 99) is added
  rather than replacing a file that exists.
* Known limits: a zoned event is written without a `VTIMEZONE` block, which Google and Apple calendars accept and some
  others may not; a repeating invitation is saved as its first occurrence only.

## Out of scope

Reminders on mail tasks; calendars, ICS subscriptions or invitations you send; reading attachments' contents beyond a
calendar part; snippets of the matched text in search results; reminders from the phone's own notifications.

## Test plan

Each item has a failing test first. Reminders: validation of times and repeats, the next occurrence across month ends, leap
days and years (repeats are wall-clock text, so a clock change cannot move them), firing once, restart, overdue summary, done
tasks, completion of a repeating task (on time, early, late, due-date only), re-saving a time that has passed, own tasks only. Search: words, prefix, accents, injection strings, the index following adds, replaces, edits, notes, deletes,
pruning, replacing and sign-out (including that the words are gone from the database file), a version-1 database upgraded in
place, desktop-only. Calendar: parsing real-world shapes (UTC,
floating, IANA and Windows zones, all-day, durations, folded lines, escapes, alarms, cancel, reply, recurrence, hostile and
oversized input), the day of a UTC time in other zones, the generated file
(escaping, folding, injection through summary and location, name, never overwriting). The UI is exercised in the browser
against the demo backend and in the packaged app.
