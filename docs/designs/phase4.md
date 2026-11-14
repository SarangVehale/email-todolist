# Phase 4: select many, your own rules, follow-ups

* **Date:** 2026-11-13
* **Status:** design for Run 1 of the agreed roadmap (see `docs/designs/phase3.md` for what came before).

## Problem

1. A long list is slow to clear: every action is one row at a time. "Done all" exists only for the filtered view.
2. Ranking is a guess. People know things the app cannot: "everything from my accountant matters", "that shop is noise".
   Sender preferences cover one address; nothing covers a domain or a subject pattern.
3. Mail you *send* and never hear back about is invisible. Tasks close when you reply, but nothing notices the opposite.

## Constraints that do not change

No paid service. The app never sends mail. Data stays on the computer. The phone hub keeps its small command set. Anything
that would change a mailbox stays behind the existing write-back switch.

## 1. Select many

Client-only; no new engine command. A **Select** button in the bar (key `v`, which also selects the highlighted row)
switches the list into selection mode: each row shows a check box, a click on the row toggles it (Shift-click selects the
range from the last click), and a bar at the **top** of the list (it stays in view while scrolling) offers **Done, Star,
Priority 1-5, Tomorrow, Next week, Mute senders**, and **All / None / Exit**. In the Done tab the bar offers **Reopen**
only; in the Snoozed tab it offers **Bring back** in place of the two snooze buttons (a snooze cannot be put back exactly
as it was, so re-snoozing snoozed rows is not offered).

Keys while selecting: Space or Enter toggles the highlighted row, Shift with `j`/`k` or the arrows extends the
selection, `x`/`e`, `1`-`5`, `s` and `h` act on the selection, `Esc` leaves.

* Done, Star and the snooze buttons act on the same messages a single row would (`members()`): only messages shown in this
  tab and not newer than the row, so mail that arrived after the screen was drawn is never closed unseen.
* **Priority changes the newest message of each selected conversation**, as pressing 1-5 on a row does. Every priority
  change is a correction the ranker learns from, so one click on a long thread must not count as one correction per message.
* **Mute senders is sender-wide:** it mutes every selected address, which lowers every open task from that sender in every
  tab and all future mail. It asks twice and has no undo (it says how to reverse it from a task: "Normal" also forgets what
  was learned about the sender).
* Changing tab, filter or search drops the rows that are no longer visible from the selection (rows still shown stay
  selected), and the actions re-read the visible list when they run, so nothing outside what the user sees is acted on.
* One undo step restores a whole batch of Done, Star, Priority or Snooze.
* Works on the phone too: it only uses commands the hub already allows.

## 2. Your own rules

A rule is `{ id, field, text, priority?, category?, file }`.

* `field` is `sender` (the address is **exactly** this address), `domain` (the address is that domain or a sub-domain; a
  leading `@` is ignored) or `subject` (contains the text). Case-insensitive plain text: **no regular expressions**, so no
  rule can be made to run for a long time. A display name is never matched, and neither is a longer or look-alike address:
  anyone can write any name, and a look-alike domain is free to register. Text is 1-100 characters with no control characters.
* The outcome is a priority 1-5, a category from the existing list, or **file** (mark the task done on arrival; with
  write-back on that archives the message, exactly as pressing X would). `file` cannot be combined with a priority. At
  least one outcome is required. At most 50 rules.
* **The first rule that matches is used.** Order is the order in Settings, with up and down buttons.
* Stored in a `rules` table in the database, not in `settings.json`, edited by the desktop-only commands `rules_save` and
  `rules_apply`; the phone hub can neither read nor change them. The reason line a rule leaves on a task ("A rule of yours set
  the priority to 5") does not repeat the rule's text, because the phone sees task reasons.
* Rules belong to the user, not to a mailbox: they stay when the account changes and are removed by "Sign out and reset".
* They run on **every** message read, including the first sync of a newly connected mailbox. With write-back on, a rule that
  files mail archives it there, and a priority change queues its Gmail label. With write-back off those changes wait in the
  outbox and are applied when it is switched on, like the changes you make by hand.

### Where rules sit in ranking

1. The model or rules ranker proposes a priority.
2. A sender preference ("always important", "mute") wins over everything, including rules: it is the most specific thing
   the user said, so a task from a sender with a preference **skips rules entirely**.
3. Otherwise the first matching rule is applied. A rule that sets a priority also removes the `urgent` flag when it sets
   priority 3 or less (urgent mail bypasses quiet hours, so a rule that says "this is unimportant" must not leave it on).
4. The learned shift from earlier corrections is **not** applied when a rule set the priority.
5. **A rule may lower a priority, set a category or file mail without any condition. It may raise a priority only if the
   mail passed its sender check, and a `subject` rule only if the sender is someone you write to** (the message is a reply
   to your mail, or the address is one you have written to). Anyone can put any words in a subject, so a stranger must not be
   able to climb by choosing them; and a forged From address must not be able to use an "this address matters" rule.
6. `file` forces priority 2 or lower, clears `urgent` and `needs_reply`, and the task is closed after it is stored, so a
   filed message can never raise an alert or trigger an automatic draft.

### Apply to what is already in the list

`rules_apply` runs the rules over open mail tasks (`dry` defaults to true, and only counts). It never touches a task that has
a sender preference, a task whose priority the user already corrected by hand (it has a correction on record), or one you
added yourself, and it does **not** record corrections, so rules never teach the ranker. It uses the same guards as new
mail, resets the "learned" tag when it changes a priority, and takes `urgent` off a task a low-priority rule calms even when
the number already matches. The Settings button refuses while there are unsaved edits ("Save the rules first"), shows the count
first and asks twice whenever anything would change.

## 3. Follow-ups: "waiting on a reply"

Off by default. Settings has a choice of 2, 3, 5 or 7 days (0 = off). When on, the sync connection looks at what you
sent and makes a task for conversations **you started** that nobody answered.

A candidate is a message in Sent that:

* has a usable Message-ID, no `In-Reply-To` (you started the thread; a reply you sent is usually the answer to someone) and
  a subject that is not a forward;
* has at least one recipient that is not you. "You" is every address your sent mail was written from, plus the login name
  when it is an address, so a login such as `jsmith` still recognises a note to yourself;
* was sent between 14 days ago and *N* days ago (a `Date` in the future is too recent to count);
* has not been looked at before. Every message is looked at **once**: the outcome (`waiting` or `answered`) is kept in a
  `followups` table keyed by Message-ID, so answered mail is never searched again. A task you close yourself keeps its
  `waiting` record, which is what stops it being made again.

Only the newest 200 started conversations of the last 500 sent messages are kept in memory; the subject is cut at 150
characters and only the first ten To and Cc addresses are read.

For each candidate (newest first, at most 10 per pass) the client first looks in the mail it has **already read**: a stored
message that replies to it counts, even if you have since filed it away. Otherwise it asks the server, read-only, in INBOX
and in the `\All` and `\Archive` folders when the server has them:

```text
UID SEARCH NOT FROM "<your address>" ... NOT HEADER Auto-Submitted "auto-" OR HEADER In-Reply-To "<id>" HEADER References "<id>"
```

The id is the cleaned Message-ID (visible ASCII, no quotes or backslashes), and your addresses come from mail you sent, so
neither can break out of the command. The two exclusions matter: Gmail's All Mail holds your own mail too (a nudge you send
in the same thread is not an answer), and out-of-office notices carry `Auto-Submitted`.

* A reply found: remember `answered`, make nothing.
* No reply found: remember `waiting` and create a task titled "Follow up: *subject*", priority 3, with the recipient and the
  date in its text. It is one of your own tasks (no mailbox message to change), tagged "Waiting".
* **Any error or an unsupported search is "unknown":** nothing is created and nothing is remembered for that message, so the
  next pass tries again (what an earlier candidate of the same pass decided stays decided).
* **At most 20 follow-ups are open at once.** More than that is a list nobody reads; the rest wait until some are closed.
* **Open follow-ups close from the mail the app has read, with no search:** a reply that arrives in your inbox is stored like
  any other mail, and the next pass closes the task ("They replied"). A task the user closed and then reopened is never
  closed again. A reply that went straight into another folder and was never read is not noticed.
* The check stops at once when the app is signing out, and writes nothing after that.
* The feature never alerts and never writes to the mailbox. With the feature off, no command is sent at all, and open
  follow-ups stay open until you close them.
* Follow-up records older than 60 days are removed by the routine clean-up (at start and once a day), unless their task still exists: since several mailboxes the record also says which mailbox the task belongs to.

**Known limits.** A reply that a mail rule moved to some other folder before the app read it is not found, so that
conversation shows as waiting; one click closes it, and it is not made again. An autoresponder that does not mark itself with
`Auto-Submitted` counts as a reply. A server with no Sent folder it can find produces no follow-ups.

Three smaller ones, accepted: the server's `FROM` search is a substring match, so a real reply from an address that merely
contains yours (`someme@x.com` when you are `me@x.com`) is not counted and a task is made, which one click closes; a follow-up
you reopened no longer counts toward the cap of 20; and a reply that comes through a mailing list does not close an open
follow-up (list mail is treated as automatic when the app reads it), although it does count when the server is asked about a
new candidate.

## Out of scope

Reading the *body* of sent mail to guess whether it asked a question; regular expressions in rules; rules that send, forward
or delete anything; follow-ups for replies you sent.

## Test plan

Each item has a failing test first. Rules: matching per field including look-alike addresses and domains, first match wins, every ranking interaction above,
the guards on raising (forged sender, a stranger's subject) on arrival and in apply-to-existing, `file` side effects, the
50-rule and 100-character limits, apply-to-existing (dry run, skipped tasks, no corrections recorded), and that a task's reason
never repeats a rule's text. Follow-ups: the candidate filter as a pure function (an id that could break a command is dropped
before any command is built), the search command as the mock server receives it (your own addresses and automatic replies left
out), unknown on error, once-only, closing from stored mail, the cap of 20, stop, switching off, replies in All Mail and
Archive, and the routine clean-up. Select many has no Rust surface; it is exercised in the browser against the demo backend
and in the packaged app.
