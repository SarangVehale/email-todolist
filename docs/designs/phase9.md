# Phase 9: tests for the window

* **Date:** 2026-11-14
* **Status:** design for Run 6. Not a feature: the window (`app/ui`, about 1,250 lines of JavaScript that also run on the phone) had no
  automated test, and most of the defects found in review of Runs 3, 4 and 5 that had no test to catch them were in it.

## Problem

The engine has 459 tests. The window has none. The engine's defects from the last three runs each got a failing test first; the window's
were found by reading or by driving it by hand, and had nothing to catch them again: a Cancel that threw while a request was out, a restore dialog that never closed after a
success, a preview that could belong to a different tick of a box, an undo that reopened only some tasks, a key press stolen by a
focused row, a click in a fold that jumped the page. A defect fixed once can come back with the next change, and nothing would say so.

## Design

Tests that run the real files in a real browser, with no new dependency (the project's `app/package.json` has none beyond the Tauri
command line, and a new dependency was not accepted for this):

* `app/test/harness.mjs`: serves `app/ui` from a small server of Node's own (`node:http`) on a free port with caching off; finds a
  Chromium-based browser already on the machine (`INBOX_AGENT_BROWSER`, which is used as given with no fallback if the file is not
  there, else Edge or Chrome on Windows; Chrome, Edge or Chromium on macOS and Linux); starts it headless with an empty profile in a temporary folder; and drives it over the browser's debugging
  protocol with Node's built-in `WebSocket` and `fetch`. Each test gets a fresh page (`demo.html?demo=1`), which runs `mock.js`: the
  window's own demo backend, so a test can add thousands of tasks, make a request slow or make it fail by wrapping `window.mock`.
* `app/test/*.test.mjs`: written with Node's built-in `node:test`. A test reads like the manual case it replaces: it sets the state up
  in the page, presses keys and clicks as a person would (real key and mouse events, so real focus and layout), and checks what is on
  the page and what the demo backend was asked.
* One browser is started for each test file and ended after the file's last test: it is asked for its own process number
  (`SystemInfo.getProcessInfo`) and to quit (`Browser.close`), that process and its helpers are ended by number, and the temporary
  profile folder is deleted. Ending the process that was started is not enough: on Windows Edge hands over to another process and the
  one that was started exits. The first version of this harness did that, and also could not find `taskkill` (a Node started from Git
  Bash does not search the system folder, so the Windows tools are now called by full path), and left 43 browsers and their profile
  folders behind. A helper process can still hold a file of the profile for 20 seconds or more, so a folder that cannot be deleted at
  once is handed, when the test process ends, to a small detached process that goes on trying for two minutes and nothing waits for
  it. Only if the browser does not stop answering are processes searched for by the profile folder (slow on a busy machine, so only
  then, and with a time limit). Folders left by a run that was killed outright are cleared by the next run once they are an hour old.
* `cd app && npm test` runs them. Without a browser they are skipped with a message, unless `INBOX_AGENT_UI_TESTS=require`, which
  makes a missing browser a failure (CI sets it, on Linux).

What they cover is what went wrong, then the main paths: the long list (150 rows, per-part counts, the cursor and the fast path, a focused
row, folds, "Show all"); the Done tab; undo of a bulk finish across a poll (for the automated fold and for mail that left the inbox);
select mode and shift-select; the mailbox dialog (Cancel during a request, a failed add, focus kept, Escape) and Settings' mailbox list
(remove, filter cleared, what was read forgotten); the restore dialog (preview, options race for both answers and errors, busy, success,
failure, unsaved rules, Escape while loading, a file that is not a backup, too large, from a newer version, with nothing new); the Backup
section; tags and chips with several mailboxes.

A test is only worth having if it fails when the code is wrong, so each guard is broken once (the old code put back) and a test must
notice, as for the engine. The result is in `docs/2026-11-14_PHASE9_REVIEW.md`.

## What the tests are not

They run the window against the demo backend (`mock.js`), not the engine, so they prove what the window does with the answers it is
given and what it asks for, not that the engine's answers are right (the engine has its own 459 tests, and the packaged-app run
covers the two together). A test waits for a condition where it can; it uses a fixed pause only to let the demo backend's slow answers
(which a test makes slow on purpose) or the window's own short timers come back, and to show that something did not happen.

## Out of scope

Visual appearance; the phone's layout; the packaged window's file chooser (the packaged-app run covers it; the tests hand the window a
`File` the way the chooser would); a coverage figure; running them on macOS and Windows in CI (the harness finds Edge, Chrome or
Chromium on all three, but CI runs them on Linux only, where the runner's browser and the way it is started are known).

## Test plan

* **The harness**: has no test of its own; it was checked by hand and by every test using it: it starts, serves the page with the right
  types, gives each test an empty page, cleans up after itself (best effort, as above; checked by counting browser processes and profile
  folders after a full run, and a minute later), and fails clearly when there is no browser and one is required.
* **The window**: one test for each defect named above, plus the paths listed; each run against the demo backend with a lot of tasks
  where size matters.
* **Mutation checks**: the old behaviour of each fix put back in `app/ui`, one at a time, run on copies of `app/ui` so the real files
  are never changed.
