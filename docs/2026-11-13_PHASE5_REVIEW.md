# Phase 5 review record, 2026-11-13

Run 2 of the roadmap: reminders and repeats, search inside message text, calendar invitations (design:
`docs/designs/phase5.md`). Three gates, each by models other than the one that wrote the code. Every claim was checked against
the code before anything changed; fixes got a failing test first; the new guards were then broken one at a time to confirm a
test noticed (mutation checks).

## Gate 1: council (two seats, blind to each other)

| Seat | Verdict | Findings |
| --- | --- | --- |
| Opus | **do not ship** | a repeat with only a due date cannot be finished early (blocker); words of deleted messages stay in the index on disk; Windows zone names dropped, so the hour is wrong with no hint; UTC invitations take the UTC day; a reminder cannot be re-saved after it fired; a race when a reminder is edited as it fires; follow-ups accept reminders; the download folder fallback and a wrong comment |
| Sonnet | ship with caveats | the same index remanence (reproduced, with the fix verified), due-only repeat and UTC-day findings; plus DURATION, an end time in another zone and RRULE silently dropped; U+2028, U+2029 and bidi marks passing the cleaning; no guard against finishing a repeating task twice; the phone's time zone |

The seats disagreed on the verdict but not on the facts; every finding was verified before it was acted on.

## Gate 2: drift passes

| Pass | Confirmed | Outcome |
| --- | --- | --- |
| Design documents and pseudocode (Sonnet) | 17 (the due-only repeat, an empty time not always clearing the repeat, "Done all N" and the toast, the calendar part types and "first part that yields an event", the Downloads fallback, a daylight-saving test that did not exist, "like `set_due`", search limits, re-arming, the phone seeing the event, and more) | corrected in `phase5.md` and `PSEUDOCODE.md`; the first and the "Done all" items were behaviour fixes |
| User-facing promises (Haiku) | 13 ("the phone cannot search message text", one undo for everything, the "In text" tag comparing the wrong things, search limits, Downloads, reminders and quiet hours or the Desktop notifications switch, the repeat toast, the time of invitations in other zones, the due date's sources, the sign-out list, the hub's networks, a stale test count) | PRIVACY, README, CHANGELOG, the manual test plan and the in-app text corrected; the tag now compares words including the note |

## Gate 3: sol-loop with Opus as the single reviewer (2 rounds)

| Id | Severity | Outcome |
| --- | --- | --- |
| S1 | should | **fixed**: "Done all N" could show an Undo bar with nothing to undo, which then reversed an unrelated earlier action |
| S2 | nit | **fixed**: the index's secure-delete setting was written on every open; now only when missing |
| S3 | nit | **fixed**: a comment said an oversized calendar part was not decoded |
| S4 | nit | **fixed**: the log line for a repeat with only a due date named a time |

Round 2 verdict: **APPROVE**, no new findings. Budget used: 2 of 3 rounds. The skill names OpenAI's GPT Sol; at the owner's
instruction Opus was used in its place.

## What these gates could not cover

Calendar files were checked by parsing them back and by reading them, not by importing them into Google, Apple or Outlook
calendars. Reminders were exercised with an injected clock; a real notification at a real minute is `docs/TESTCASES.manual.md`
MT-82 and MT-83. The packaged app's own suite checks the refusals of `save_event` but not a real file written to Downloads,
because the app exits at start-up when its home folder is redirected; that path is covered by the Rust tests that give it a
folder.
