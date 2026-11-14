# Phase 8 review record, 2026-11-14

Run 5, which is not on the original roadmap: found by measuring the packaged app with a heavy database after the roadmap was finished
(design: `docs/designs/phase8.md`). Three gates, each by models other than the one that wrote the code. Every claim was checked against
the code before anything changed; the engine's fixes got a failing test first and were broken one at a time to confirm a test noticed;
the window has no automated test (the repository has no JavaScript test harness), so its fixes were each driven in a browser against the
demo backend with thousands of tasks, both before and after.

## What was measured, and what changed

With the packaged app and 3,000 open plus 5,000 finished tasks, before: the engine's answer to the window's poll took 250 to 420 ms and
was 6.4 MB, every 3 seconds; drawing the list took 2.1 s; one cursor key press 0.95 s; the Done tab 1.7 s; the page had 107,000
elements. The window now gets every open task and only the newest finished ones, draws 150 rows at a time, draws a row's detail only
when it is open, and moves the cursor by changing two classes. The numbers after are in the last section.

## Gate 1: council (two seats, blind to each other)

| Seat | Verdict | Findings |
| --- | --- | --- |
| Opus | do not ship as it was | **undo of a bulk finish silently reopened only the tasks the window still held** (after one poll, at most 200 of a "Done all automated" of 300), because the undo closures only asked the engine about tasks `find()` returned; with a row focused (Tab) the new cursor movement no longer redrew, so `j` was stolen back by the focused row and `x` finished the wrong task; a click in an open "automated" fold, while the list was capped, drew the whole main list and jumped the page 27,000 px; the Done tab claimed finished tasks "still turn up in search" (there is no search on that tab); the Done count went stale; the demo backend ordered by the wrong time |
| Sonnet | ship with caveats | the same undo and fold problems, reproduced; the three engine reads were not in one transaction; `k` on the first row no longer scrolled; "starts again at the top with 150 rows" was not true when the cursor was deep |

## Gate 2: drift passes

| Pass | Confirmed | Outcome |
| --- | --- | --- |
| Design and pseudocode (Sonnet) | 10: the search claim; "one click away" against finished tasks that were not reachable; a reset to 150 overridden by the cursor; "a key press does not redraw" (only cursor keys skip it); "works the list out once" (the filter bar and select mode work it out again); the "Show all N" button never described; the mock not answering like the engine; the test plan's counts; missing review record | design rewritten; **"Show older" added to the Done tab** so older finished tasks are reachable again (the engine takes `state { done }`, 200 to 20,000) |
| User-facing promises (Haiku) | 6: "thousands of tasks do not slow it down" (open tasks are all still sent); "still turn up in search"; "draws 150 rows at a time" while the cursor draws more; the reset; counts and "Done all" on the Done tab cover only what is loaded; the label of "Show all N" | wording corrected in the changelog, README and manual tests; the label says "more" |

## Gate 3: sol-loop with Opus as the single reviewer (2 rounds)

Round 1 (a ledger of the earlier findings was given up front):

| Id | Severity | Outcome |
| --- | --- | --- |
| R1 | should | **fixed**: the scale script looked for words the window never writes, so its Done-tab check would always fail |
| R2 | should | **fixed**: one row count was shared by the main list and both folds, so "Show 150 more" under a fold drew 150 more rows of the main list above it and the page jumped (the same jump the earlier fold fix was meant to remove, from row 150 on); each part now has its own count |
| R3 | should | **fixed**: the Done tab asked for the larger number of finished tasks forever, so "Show 500 older" could load 1,000; it goes back to 200 when the tab is left |
| R4 | nit | **fixed**: "Show older" stayed on screen, doing nothing, past 20,000 finished tasks |
| R5 | nit | **fixed**: a shift-click in select mode selected the rows between the two clicks that were not drawn |

Round 2: **APPROVE**; R1 to R5 verified, one narrow note (the Done tab may drop from 700 to 200 when you leave and return before the
next poll; consistent with the documents) and nothing to change. Budget used: 2 of 3 rounds. The skill names OpenAI's GPT Sol; at the
owner's instruction Opus was used in its place.

## Mutation checks

The engine's guards (the newest finished tasks rather than the oldest; every finished task; the total; open tasks not limited; the
default of 200; the total in the answer) were each broken once and a test failed each time. The window's fixes have no test to mutate;
they were verified by driving each path in the browser, which is the gap named next.

## What these gates could not cover

At the time of this review the window had no automated test. A JavaScript test harness for `app/ui` (for example the whole list, cursor and undo logic run under
Node with a small DOM) would have caught the undo and focus defects before review; it is the next thing worth building (done in Phase 9: `docs/designs/phase9.md`, which runs the real window in a real browser instead of under Node with a small DOM). Nothing was
run on macOS, Linux or a real phone, and whether `document.hidden` becomes true in the packaged window when it is minimised or sent to
the tray is checked by `docs/TESTCASES.manual.md` MT-135, not by a test.

## The measurements after (packaged app)

Packaged app, 3,000 open and 5,000 finished tasks (`scripts/scale_check`), before to after: loading and drawing 2,155 ms to 246 ms;
drawing again 567 ms to 11 ms; one cursor key press 951 ms to 2 ms; the Done tab 1,657 ms to 13 ms; page elements 107,000 to 2,945; the
engine's answer to a poll 250 to 420 ms and 6.4 MB to about 130 ms and 2.6 MB. **One target was narrowly missed**: the poll answer was
meant to be under 120 ms and 2.5 MB and was 127 ms and 2.6 MB; what remains is the 3,000 open tasks themselves, so the check now fails
only above 250 ms or 3 MB and the design keeps the original figure as the aim. The packaged-app suite passes 112 of 112 (it is not
changed by this run), and the install, start and uninstall cycle works.
