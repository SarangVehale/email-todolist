# Phase 11 review record, 2026-11-14

Run 8, which is not on the original roadmap: close the gaps that writing the documentation (Phase 10) and the first push to GitHub
found (design: `docs/designs/phase11.md`). No feature. Three gates, each by models other than the one that wrote the work. Every
finding was checked against the code before anything changed.

## What was built

* A task number, a priority and a mail server port are no longer cut to 32, 8 and 16 bits (`try_from`): task 2^32 + n no longer acts
  on task n, priority 261 is not read as 5, port 66529 is not read as 993. A port must be a whole number from 1 to 65535 (none, or
  `null`, is 993; a number in quotes is refused now).
* `save_settings` refuses a ranking mode that is not rules, ollama or anthropic, and refuses to change the phone hub's access code
  (only the app makes it). A settings file already saved with another mode reads as "rules" instead of blocking every later save.
* `test_account`, `detect_ollama`, `save_account`, `add_account` and `finish_setup` check what they are given before they do
  anything: server name, address (no control character, at most 320 bytes), password, an http(s) address; setup cannot be finished
  with no mailbox, no address or no password in the keychain.
* The snooze error says what is accepted; the diagnostics report no longer names a Google sign-in that was never built.
* Found by the first CI run and committed earlier in this phase: write transactions that may read first take the lock at once
  (`BEGIN IMMEDIATE`); the hub tests wait until the hub answers; a status test accepts the real order; `ci.yml` is valid YAML and a
  failing job puts the failing tests (and the last lines of the log) into an annotation, because a job log needs a sign-in to read;
  the window's tests run on Windows and macOS in CI too.

Nine new engine tests (`engine/tests/phase11.rs`), 470 in all, red before the fix. Every guard was put back to its old behaviour
once: 23 mutants (14 before the council, 9 after; two of the first were replaced by new forms of the same guard), each caught by a test.

## Gate 1: council (three seats, blind to each other)

| Seat | Verdict | Findings |
| --- | --- | --- |
| Opus | ship | `finish_setup` should use `secrets::lookup` so an unreadable keychain is not reported as a missing mailbox; its password clause untested; the hub-code test only covered an empty code; a saved bad mode would block every save; the demo backend does not refuse everything the engine does |
| Sonnet | ship with fixes | `rpc.md` said an empty server is not refused when saving the first mailbox (it is); that refusal was not in the design table; `add_account`'s port and the default and quoted ports untested; `finish_setup`'s password clause untested; an array-shaped `hub` patch untested; `test_account` did not check the address as saving does |
| Fable | ship with fixes | the same two untested clauses of `finish_setup`; a non-empty hub code never exercised (a mutant refusing every save once a code exists would pass); `test_account` and the address; the default port untested; one remaining narrowing cast in `add_task` |

All of it was true and is fixed, except the demo backend (see below). No seat found a bug a user could meet, a bypass, or a
regression in the window's own calls (all of which were read: the wizard, Settings, the add-mailbox dialog).

## Gate 2: drift passes

| Pass | Claims checked | Confirmed drifts |
| --- | --- | --- |
| Technical documents (Sonnet) | about 95 | 12: stale "CI has never run" and "window tests on Linux only" in the testing strategy, the coding standards, a decision record and the test plan; a test count (7, now 9); 320 and 200 are bytes, not characters; the command reference said a wrong argument is never an error except the port (also `save_settings`' patch and `rules_save`'s rules); the settings reference counted two refusals from `save_settings` (three); the CI runbook said every step after a red one is skipped (the annotation steps and the window's tests run after a failure); the `npm test` comment lacked `--test-reporter=spec`; a changelog sentence pointed at the wrong place |
| Promises in the guides, FAQ and troubleshooting (Haiku) | about 115 | 3 reported, 2 real: the empty-password message is not what **Add mailbox** shows (the button stays off until a test passes; Settings says "Paste the new password first"), and **Finish** saves the mailbox first so the window shows that call's message, not "Add your first mailbox before finishing setup". The third ("Closed automatically because you replied" is not in the code) was wrong: it is in `app/ui/main.js` line 150 |

Also found and corrected outside either pass's list: a user story and `CONTRIBUTING.md` still said no repository exists and nothing had
run on Linux or macOS.

## Gate 3: sol-loop with Opus as the single reviewer

Round 1 (a ledger of the council's and the drift passes' findings was given up front): **APPROVE**, no blocker or major.

| Id | Severity | Outcome |
| --- | --- | --- |
| R1-1 | minor | **fixed**: an assertion ("an empty password saved nothing") could not fail, because an empty password reads as no password; it now checks that no server and no password were saved |
| R1-2 | nit | **not changed**: restoring a backup whose saved ranking mode is none of the three now fails the restore with the mode message. Only a request could ever have saved such a mode (the window sends one of three), so no real backup holds one |

Budget used: 1 of 3 rounds. The skill names OpenAI's GPT Sol; at the owner's instruction Opus was used in its place.

## What CI showed

| Run | Commit | Result |
| --- | --- | --- |
| 9 | `f01f503` (after four fixes) | green on Windows, Linux and macOS (engine tests; the window's tests on Linux) |
| 10 | `98e2456` (window tests on all three) | **failed on Ubuntu**, in the window's tests; the annotation said only "exit code 1" (a test that times out matches none of the names the annotation step looked for); Windows and macOS passed |
| 11 | `35f11e0` (annotations carry the last lines of the log) | green on all three, window tests included |
| 12 | `05c6613` (this phase) | green on all three, window tests included, and the `audit` job |

**The cause of run 10's failure was not found.** The same code passed in run 11, so it is either a flake or a race, probably in the
window's tests on a small runner (two browsers at once). It is recorded in the testing strategy, the test plan and the decision record
instead of being called fixed. If it comes back, the annotation now names the last lines of the log. Two green runs in a row (11 and
12) are not proof that it is gone.

*Added 2026-11-15:* it came back twice (CI run 22 on a documentation-only commit, and a release rehearsal), and the annotation this phase added
showed the cause: `the browser did not start`, thrown by the harness after a fixed wait of 15 seconds for the browser to open its debugging
port, on a busy runner with two browsers starting at once. The harness now waits up to 40 seconds and starts the browser once more
(Phase 12). The 54 repeated runs on Ubuntu did not reproduce it; all but the first run of each job started a browser that the system had already cached, which
CI and the rehearsal do not (a likely reason, not a proven one).

## What these gates could not cover

* The demo backend (`app/ui/mock.js`) does not refuse everything the engine now refuses (for example a port in quotes), so a window
  test cannot show the new messages; the window shows an engine message as it is.
* `test_account` still refuses a mailbox with no Sent folder, though the sync copes with none.
* Nothing here was tried against a live mailbox, on a clean machine, or by hand on Linux or macOS; no installer has been built for
  them; `release.yml` has never run.
* The packaged app was not rebuilt or run for this phase; the window was exercised only through the demo backend, and the engine
  only through its own tests.
