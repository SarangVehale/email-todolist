# Phase 7 review record, 2026-11-14

Run 4 of the roadmap: backup, export and restore, plus the project's version and release history (design:
`docs/designs/phase7.md`). Three gates, each by models other than the one that wrote the code. Every claim was checked against the
code before anything changed; fixes got a failing test first; the guards were then broken one at a time to confirm a test noticed
(mutation checks).

## Before the gates: mutation checks on the first version

Every guard in the first version was broken one at a time (33 in `backup.rs`, 20 in `store/transfer.rs`, 4 in `store.rs`, 5 in
`scripts/bump_version.py`). Ten survived. Two are equivalent (`rules::validate` also enforces the 50-rule limit, and a random rule id
cannot be forced to collide); the other eight were gaps in the tests (an 8 MB file that is valid JSON, a mailbox or an option that is
bad when options are not asked for, a sender choice that is not an address, a finished task's reminder, a snooze on finished mail,
empty entries being kept, a correction dated in the future), and each got a test that failed before the fix and a mutation that
confirmed it.

## Gate 1: council (two seats, blind to each other)

| Seat | Verdict | Findings |
| --- | --- | --- |
| Opus | ship with caveats | **a file could switch ranking to the paid cloud service** (with a saved key, mail details go to Anthropic and cost money); a restored mailbox kept leftovers from an earlier mailbox with the same number; a snooze from a file was unlimited; reply drafts written by the AI or from a template were exported (they paraphrase mail); a restored repeating task kept an overdue date; restored corrections were filed under mailbox 0 and deleted with it; export dropped entries over its limits without saying; Escape while the preview loads threw |
| Sonnet | ship with caveats | restored rules vanished from the rule editor and were deleted by the next "Save rules"; "restoring twice changes nothing" was false for a finished task that kept a reminder, for a correction dated in the future and for one with a very long subject; **a restore of the biggest file took about 45 seconds and locked the database** (no index on the columns it looks up); the cloud-ranking switch; the missing clean-up of a restored mailbox; an unlimited snooze; the preview showed no mailbox server and only counted rules that file mail away; one log line per restored sender; finished mail could still get an automatic reply draft |

The seats agreed on the facts. Everything was verified and fixed except what is accepted below.

## Gate 2: drift passes

| Pass | Confirmed | Outcome |
| --- | --- | --- |
| Design documents, pseudocode and history documents (Sonnet) | 14: the commands are not in `DESKTOP_ONLY` (the phone refuses them because they are not in `PUBLIC`); sender and subject are in a backup (in corrections); a restore needs setup finished, not only the mailboxes; a sender choice also changes that sender's open mail; the duplicate rule for tasks; the one network name a file can supply (a mailbox's server); an emptied first place can be refilled; restored corrections were filed under mailbox 0 (a code bug); the clean-up lists omitted the restored state; "every limit at its edge" overclaimed; the release check said "dated" but only rejected the word Unreleased; RELEASING.md step 3; when the history rows are made | code and documents corrected; the release check now requires a date and a row, with tests (`scripts/test_bump_version.py`, run by CI) |
| User-facing promises (Haiku) | 6: "only adds" was not true of options and a later snooze; "never a network address" was not true of mailbox servers (a crafted file with the options box ticked could add a mailbox pointing at someone else's server, and the user would then paste an app password into it); "restoring twice changes nothing" (the finished task); silent truncation; "no message text" (corrections hold sender and subject); the option list | wording corrected everywhere it appears; **the preview now lists each mailbox it would add with its server, and says the paid ranking is not restored**; the finished-task bug fixed |

## Gate 3: sol-loop with Opus as the single reviewer (3 rounds)

Round 1 (a ledger of the earlier findings was given up front, so the reviewer looked for new things):

| Id | Severity | Outcome |
| --- | --- | --- |
| R1 | blocker | **fixed**: a restored mailbox takes the first free number, and a password left in the keychain under that number (by an earlier install, or a mailbox removed in a hurry) would have been sent to the server the file names. Restoring now deletes it |
| R2 | should | **fixed**: an old backup's corrections were stored later than newer ones and so counted as the newest; corrections now go by their date |
| R3 | should | **fixed**: a restore that began while a sync was saving mail failed with "database is locked" (a deferred transaction that reads first cannot upgrade after another connection commits); it now takes the write lock first and waits for it |
| R4 | nit | **fixed**: Cancel and Escape closed the dialog while a restore was applying, hiding a failure; the "already here" count left out two kinds |

Round 2: R1 to R4 verified. One new finding, **fixed**: the fix for R4 introduced a regression (the dialog never closed after a
successful restore, because the busy flag was not reset). The code review found it by reading; the window had not been re-run after
the change. It now has been, on all three paths (success, applying, refused).

Round 3: **APPROVE**. One finding, **fixed**: two previews in flight (the options box ticked, unticked and ticked again quickly) could
leave the dialog showing the wrong one, which is the preview that lists the servers; an answer is now kept only if it is for what the
box says now, and the box is disabled while a restore is applying. A sentence in the design that contradicted itself was corrected.
Budget used: 3 of 3 rounds. The skill names OpenAI's GPT Sol; at the owner's instruction Opus was used in its place.

## Accepted, documented

The backup file is not encrypted (it holds notes and drafts you edited; PRIVACY.md says so and what to do). A restored mailbox's server
comes from the file; the preview lists it and says to restore only a file you made yourself. The activity log is not part of a backup.

## What these gates could not cover

Nothing was run against a live mailbox, so restoring onto a real second computer with a real inbox, and the order in which a first
sync and a restore interleave there, are manual tests (`docs/TESTCASES.manual.md` MT-112 to MT-127). `backup_export`'s Downloads
lookup is exercised by the packaged-app run, not by an engine test. A real sync racing a real restore was reasoned about and the lock
behaviour reproduced with two connections, but not raced end to end. macOS and Linux file choosers were not tried.

## A lesson

One defect found in review was introduced by a fix (a busy flag never reset, so the dialog never closed after a success). It was in
code that was changed after the last browser run. The rule for the next run: any change to `app/ui` is followed by a browser run of every
path of that dialog before it goes to review.
