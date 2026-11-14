# Phase 9 review record, 2026-11-14

Run 6, which is not on the original roadmap: tests for the window (`app/ui`), which had none (design: `docs/designs/phase9.md`). Nothing
the user sees changed, so these gates looked at the tests more than at the product: whether they can fail, whether they can fail when
nothing is wrong, whether they clean up after themselves, and whether the documents claim only what is true. Three gates, each by models
other than the one that wrote the code. Every claim was checked against the code before anything changed.

## What was built

* `app/test/harness.mjs`: no dependency. It serves `app/ui`, starts a Chromium-based browser that is already on the machine, drives it
  over the DevTools protocol with Node's own `WebSocket` and `fetch`, and ends it and removes its profile afterwards. (A first attempt
  with `jsdom` was refused by the permission system, so none was installed; running the real files in a real browser is also a better
  test of focus, scrolling and key events than a simulated page.)
* `app/test/{list,cursor,mailboxes,restore}.test.mjs`: 49 tests, run against the window's own demo backend (`mock.js`), which a test can
  wrap to make an answer wait until the test lets it go (`hold` and `release`), fail, or say something else.
* `cd app && npm test`; a Linux step in CI (`INBOX_AGENT_UI_TESTS=require`, so a missing browser fails the build); the test catalogue
  (`docs/TESTCASES.md`) now lists them too.
* One product line, with no change in what it does: in the restore dialog, `[data-q=go]:not([disabled]), [data-q=cancel]` always found
  Cancel first (it comes first in the dialog), so the code now says Cancel, and a test pins that Enter pressed by reflex restores nothing.

## Gate 1: council (two seats, blind to each other)

| Seat | Verdict | Findings (all fixed unless noted) |
| --- | --- | --- |
| Opus | do not ship until two things were fixed | the test catalogue was stale, so CI's "catalogue is up to date" step would fail on all three systems, and the documents counted 41 or 42 tests where there were 49 and cited a review record that did not exist yet; no time limit on the process search that ends a browser; a Ctrl-C left profile folders behind; a malformed `%` in a request address could crash the test process; the CI step sat before the engine's tests and could hide them; four test files at once on four cores; a scroll test that never checked the row was out of sight before the key; 450 ms pauses against a 280 ms animation; a wrong comment; "Show all" asserted only `>= 600` |
| Sonnet | ship with caveats | **a test passed for the wrong reason**: "Cancel while the connection is being tested" released a call that then waited the demo's 900 ms, and looked at the page before the answer came, so removing the guard it is named for did not fail it (with a longer wait it did); counts stale; `scripts/__pycache__` untracked and not ignored; temporary profile folders left behind under load (62 counted: the folder was dropped from the exit list even when it could not be deleted, and the delete gave up after 3 s); labels that promised more than they asserted; no Node 22 guard; the comma sent with the wrong key code |

## Gate 2: drift passes

| Pass | Confirmed | Outcome |
| --- | --- | --- |
| Design and pseudocode (Sonnet) | 10: the design said every defect of Runs 3 to 5 was in the window (the engine's were not, and had engine tests); the browser list; "a file's tests" where it is a group of tests; links to this record; the "harness" test plan read as if it had tests of its own; a test title that claimed `j` at the last row; the pseudocode's list of what drives the window; a present-tense sentence in the Phase 8 record; a stale catalogue | wording corrected; the catalogue is regenerated last |
| Promises (Haiku) | 6: the test count (42, now 49); "Chromium" is not looked for on Windows; "nothing to install" without saying Node 22; the restore-focus sentence said a behaviour was fixed when only the code was; "open the real window" (it is headless); "every defect has a test" (see below) | wording corrected |

The second pass also produced the list below: each window defect that earlier reviews found, against the test that fails when it comes
back. Twelve of them had no test; tests were written for nine, and the other three are marked none.

| Defect found in review | Test that fails if it comes back |
| --- | --- |
| Run 3: Cancel during a connection test threw | Cancel while the connection is being tested goes back to Settings… |
| Run 3: Cancel during an add threw | Cancel while the mailbox is being added goes back to Settings… (written after the drift pass) |
| Run 3: a redraw dropped the keyboard | a redraw keeps the keyboard in the box it was in |
| Run 3: what was read or could be undone followed a reused mailbox number | a failed add keeps the dialog…; Remove asks twice…; signing out forgets what was read… (the last written after the drift pass) |
| Run 3: the cursor jumped to the other mailbox's copy of a conversation | when a newer message arrives in one mailbox's copy… (written after the drift pass) |
| Run 3: one message in two mailboxes was one conversation | the same message that reached two mailboxes is two rows (written after the drift pass) |
| Run 3: the demo backend did not reuse mailbox number 0 | **none**: it is the demo backend only |
| Run 4: Escape while the preview loads threw | Escape while the file is still being read leaves nothing to draw… |
| Run 4: the editor kept stale rules and the next Save dropped the restored ones | while Restore is working… the rule editor shows the restored rule |
| Run 4: Cancel and Escape closed the dialog while applying; the box was not off | while Restore is working Cancel and Escape do nothing… |
| Run 4: the dialog never closed after a success | the same test |
| Run 4: a preview of the wrong tick of the box (and its error) | a preview that arrives late…; a preview that fails late… |
| Run 4: a restore with unsaved rules | rule changes that are not saved stop a restore from starting |
| Run 4: a backup dropped entries without saying so | when the file holds only so many of something… |
| Run 4: the preview did not list mailboxes, warn, or say a paid ranking choice is not restored | with the box ticked the preview lists each mailbox…, warns…, and says… (written after the drift pass) |
| Run 5: undo of a bulk finish reopened only the tasks the window still held | undo of "Done all automated"…; undo of "Done all" for mail that left your inbox…; undo of "Done all N" for a filtered list…; select mode… even after a poll (the last two written after the drift pass) |
| Run 5: a focused row took the keys; `x` finished the wrong task | a row that has the keyboard focus does not take the keys back… |
| Run 5: a click in a fold drew the whole main list | opening a row inside a fold… |
| Run 5: `k` on the first row did not scroll | k on the first row and j on the last row bring the cursor back into view |
| Run 5: the Done count was stale | the Done count moves when a task is finished or reopened here… |
| Run 5: one row count for the list and both folds | Show 150 more under an open fold…; a cursor far inside a fold… |
| Run 5: the Done tab kept asking for more, showed the wrong 200, or offered "Show older" after 20,000 | the Done tab test (it also checks that the 200 are the newest); Show older goes away when the window holds every finished task… (written after the drift pass) |
| Run 5: shift-click selected rows that were not drawn | shift-click selects the drawn rows… |
| Run 5: a search of finished tasks that does not exist; a reset to 150 that was not true with a deep cursor | **none**: these were wrong sentences in documents, now corrected, not behaviour |

## Gate 3: sol-loop with Opus as the single reviewer

Round 1 (a ledger of the earlier findings was given up front). The reviewer ran the whole suite once; it hung for 9 minutes.

| Id | Severity | Outcome |
| --- | --- | --- |
| R1 | must | **fixed**: a start that failed part-way (the browser's port file was locked while it was written) threw after the server and the browser were running; nothing ended them, so the test run never ended. The port is now read in a polling loop until it is a number, everything after the temporary folder is made is inside a try that ends what was begun, the server and the started process are unref'd, and a spawn error is noticed |
| R2 | should | **fixed**: profile folders were left behind (Edge's helper processes hold files for 20 seconds or more after the browser is gone), and the record said none were. Found while fixing it: `taskkill` could not be found from a Node started in Git Bash, so the first harness never ended anything; the tools are called by full path, the browser is asked for its own process number and that tree is ended, one browser is started for each test file (not each group), and a folder still locked at the end of the test process is handed to a small detached process that keeps trying for two minutes; the record now says what is true |
| R3 | should | **fixed**: a row of the mutation table named a test that never presses Escape; the table is rebuilt from a final pass and lists every test that failed for each mutant |
| R4 | should | **fixed**: a test title promised "the next try can succeed"; after a failed restore the dialog offers only Cancel (the error stays until the file is chosen again), so the title says what is checked and the test pins that |
| R5, R6 | nit | **fixed**: two different counts of the untested items; "3 seconds" in the design |

Round 2: **APPROVE**, with one should and one nit. R2-1: the browser's process was ended by a number it had reported a moment before, after the browser had stopped answering, when another process might already have been given that number: now it is ended on Windows at the moment the browser reports it, and elsewhere only the process Node itself started is ended (whose handle Node holds). R2-2: the comment on the list of running profiles claimed that a crashed run's browser is ended; reworded. Both fixed afterwards and checked by the suite and a new full mutation pass; a third round was not run. Budget used: 2 of 3 rounds. The skill names OpenAI's GPT Sol; at the owner's instruction Opus was used in its place.

Two more defects were found by the author while running the checks and are fixed: the detached cleaner inherited the test folder as its working directory and kept it locked for two minutes (it now starts in the system's temporary folder), and clicks could land on the wrong thing when the app moved its own focus (and scrolled the dialog) 30 ms after a dialog opened (a click now waits until its target stays where it is and is what is under the pointer; the helpers wait for the focus).

## Mutation checks

The old behaviour of each guard was put back in a copy of `app/ui` (never in the real files, so none can be left changed), one at a time,
and the tests that cover it were run: **49 mutants on the final tree (the fourth full pass), all caught, none survived.** Earlier passes found two that did
survive, and the tests were fixed: a failure of the preview for a box that is no longer ticked (a test was missing), and the cursor
jumping to the other mailbox's copy (the test's data sorted the other mailbox's copy last, so it could not tell).
An earlier pass also showed an unrelated test failing under two mutants (a Backup test that has nothing to do with Escape), which was
the clicks racing the app's own 30 ms focus move; after that fix the last pass shows only the tests that belong to each mutant (three
mutants make two or more fail: the two fold tests, the two Cancel tests, and the focus on Cancel, which every restore test waits for).

| File | The old behaviour put back | Tests that failed (titles cut) |
| --- | --- | --- |
| `main.js` | focused row keeps the keys | a row that has the keyboard focus does not take the keys back: j keeps moving and x finish |
| `main.js` | a cursor key redraws the list | a cursor key moves the cursor by changing classes, without drawing the page again |
| `main.js` | one row limit for every part | opening a row inside a fold does not draw the whole list above it or throw the page away f; a cursor far inside a fold raises only the fold's count |
| `main.js` | counts not reset by tab or filter | another tab, another sort order or a search starts the list again at 150 rows |
| `main.js` | Show all only shows 150 more | Show 150 more adds 150, Show all draws everything |
| `main.js` | Done tab keeps its size after leaving | the Done tab shows the newest finished tasks, says how many there are, and can show older |
| `main.js` | Show older adds 1000 | the Done tab shows the newest finished tasks, says how many there are, and can show older |
| `main.js` | Done count does not move | the Done count moves when a task is finished or reopened here, without waiting for the nex |
| `main.js` | every row draws its detail | only the open row has its detail drawn |
| `main.js` | rail marker stays | the rail marks the row the cursor is on |
| `main.js` | edge key does not scroll back | k on the first row and j on the last row bring the cursor back into view |
| `main.js` | Show older stays past 20,000 | Show older goes away when the window holds every finished task the engine can send |
| `main.js` | the filter stays on a removed mailbox | removing the mailbox the list is filtered to clears the filter instead of showing an empty |
| `main.js` | the cursor jumps to the other mailbox's copy | when a newer message arrives in one mailbox's copy of a conversation the cursor stays on t |
| `main.js` | a conversation is keyed without its mailbox | the same message that reached two mailboxes is two rows, one in each |
| `drafts.js` | shift-select takes undrawn rows | shift-click selects the drawn rows between two clicks and none that are not drawn |
| `drafts.js` | undo of Done all automated reopens only loaded | undo of 'Done all automated' reopens every task, even those the window no longer holds aft |
| `drafts.js` | undo of Done all left-inbox reopens only loaded | undo of 'Done all' for mail that left your inbox reopens every task too |
| `drafts.js` | select All takes only drawn rows | select mode: All selects the whole list and Done finishes all of it, and one Undo takes th |
| `drafts.js` | undo of Done all N reopens only loaded | undo of 'Done all N' for a filtered list reopens every task, even those the window no long |
| `drafts.js` | undo of select-mode Done reopens only loaded | select mode: All selects the whole list and Done finishes all of it, and one Undo takes th |
| `drafts.js` | a tag with one mailbox | with two mailboxes every task of a mailbox has its tag and a chip filters to it; with one |
| `drafts.js` | chips with one mailbox | with two mailboxes every task of a mailbox has its tag and a chip filters to it; with one |
| `drafts.js` | Escape does nothing in the mailbox dialog | Escape goes back to Settings, and Escape again closes it |
| `drafts.js` | Escape does nothing in the restore dialog | Escape while the file is still being read leaves nothing to draw when the answer comes |
| `setup.js` | Cancel during a request still draws | Cancel while the connection is being tested goes back to Settings and nothing is drawn whe; Cancel while the mailbox is being added goes back to Settings, and the refusal that comes |
| `setup.js` | a failed add leaves the dialog busy | a failed add keeps the dialog and says why; after a fix the same dialog adds the mailbox |
| `setup.js` | a redraw drops the focus | a redraw keeps the keyboard in the box it was in |
| `setup.js` | adding a mailbox keeps old reads | a failed add keeps the dialog and says why; after a fix the same dialog adds the mailbox |
| `setup.js` | removing a mailbox keeps old reads | Remove asks twice, and then the mailbox and its tasks are gone and Settings lists one mail |
| `setup.js` | Cancel while a mailbox is being added still draws | Cancel while the mailbox is being added goes back to Settings, and the refusal that comes |
| `setup.js` | signing out keeps what was read | signing out forgets what was read and what could be undone |
| `setup.js` | a stale preview is shown | a preview that arrives late for an unticked box is not shown for the box as it is now |
| `setup.js` | a stale preview error is shown | a preview that fails late for an unticked box does not put its error on the dialog as it i |
| `setup.js` | the dialog is drawn after Escape while reading | Escape while the file is still being read leaves nothing to draw when the answer comes |
| `setup.js` | Cancel works while restoring | while Restore is working Cancel and Escape do nothing; when it is done the dialog closes, |
| `setup.js` | the rule editor keeps stale rules | while Restore is working Cancel and Escape do nothing; when it is done the dialog closes, |
| `setup.js` | the dialog stays busy after a success | while Restore is working Cancel and Escape do nothing; when it is done the dialog closes, |
| `setup.js` | the dialog stays busy after a failure | a restore that fails shows why, is not stuck busy, and leaves Cancel working |
| `setup.js` | unsaved rules do not stop a restore | rule changes that are not saved stop a restore from starting |
| `setup.js` | a huge file is read | a file over 8 MB is refused before it is read |
| `setup.js` | the keyboard goes to Restore | names what Restore will do before it does anything, and puts the keyboard on Cancel so Ent; a preview that arrives late for an unticked box is not shown for the box as it is now; a preview that fails late for an unticked box does not put its error on the dialog as it i; ticking the box adds the mailboxes and options to the preview, and the box keeps the keybo; while Restore is working Cancel and Escape do nothing; when it is done the dialog closes,; a restore that fails shows why, is not stuck busy, and leaves Cancel working |
| `setup.js` | the options box can be ticked while restoring | while Restore is working Cancel and Escape do nothing; when it is done the dialog closes, |
| `setup.js` | Cancel is on while restoring | while Restore is working Cancel and Escape do nothing; when it is done the dialog closes, |
| `setup.js` | Restore is on with nothing new | a backup with nothing new in it says so and offers no Restore |
| `setup.js` | the backup message hides what was left out | when the file holds only so many of something, the message says what was left out |
| `setup.js` | the backup error is not shown | a backup that fails to save says why and does not claim it saved |
| `setup.js` | no note that a paid ranking choice is not restored | with the box ticked the preview lists each mailbox with its server, warns about it, and sa |
| `setup.js` | no warning to restore only your own file | with the box ticked the preview lists each mailbox with its server, warns about it, and sa |

## Things the work itself taught

* Ending a browser is the hard part of such a harness. On Windows Edge hands over to another process, so the process that was started
  is not the browser; the first version also could not find `taskkill` (a Node started from Git Bash does not search the system
  folder), so it left 43 browsers and their profile folders after one full run. The harness now starts one browser for each test file,
  asks it for its own process number and to quit, ends that process and its helpers by number with the tool's full path, and deletes
  the profile folder. A helper can still hold a file for 20 seconds or more, so a folder that cannot be deleted at once is passed, when
  the test process ends, to a small detached process that keeps trying for two minutes (nothing waits for it). Only if the browser
  does not stop answering are processes searched for by the profile folder (that search alone took 10 seconds on a busy machine). A run
  that was killed outright has its stale folders (older than an hour) cleared by the next run. After a full run and a minute, no
  browser and no folder is left on this machine.
* A start that failed part-way (the browser's port file was locked while Edge wrote it) used to leave the server listening, so the whole
  test run never ended; found by the Opus reviewer when its own run hung for 9 minutes. A failed start now ends what it began.
* A process search with no time limit stalled whole runs for 10 minutes under load; every wait now has a limit and a hung test fails
  (per test 120 s, protocol answers 60 s, `waitFor` 15 s) instead of stopping the run.
* A pause of "long enough" is wrong in both directions. Tests that need a late answer now hold it and release it, so nothing depends on
  how fast the machine is.
* Mutation checks run in parallel on copies of `app/ui`; four at once on this machine was too many (runs timed out; two or three
  finished everything).

## What these gates could not cover

Nothing was run on Linux or macOS, and nothing on Node 22 (Node 24 here); the CI step has never run. The tests prove what the window does
with the demo backend's answers, not that the engine's answers are right. Key events were checked only for the keys the tests use
(`,` `j` `k` `x` `z` Enter Escape); Shift-range selection by key and the Tab trap in dialogs have no test. Three items have no test (the
demo backend's mailbox numbering, and two sentences in documents), listed above.
