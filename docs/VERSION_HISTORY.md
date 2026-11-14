# Version history

How the project got to the version it is at, one milestone at a time. Three documents record change, and they answer three
different questions:

| Document | Question | Updated |
| --- | --- | --- |
| [`CHANGELOG.md`](../CHANGELOG.md) | What changed for a person using the app, per version? | every run |
| `docs/VERSION_HISTORY.md` (this file) | Which build had what: commit, date, tests, review? | every run, once the commit exists |
| [`docs/RELEASE_HISTORY.md`](RELEASE_HISTORY.md) | What was actually published, signed or not, with which checksums? | only when something is released |

## How versions work

* One version number for the whole project (`app/package.json`, `tauri.conf.json`, both `Cargo.toml` and both `Cargo.lock`),
  kept in step by `scripts/bump_version.py`, and a release is the git tag `v<version>`. See [`RELEASING.md`](../RELEASING.md).
* [Semantic Versioning](https://semver.org). Before 1.0, a minor version (`0.2.0`) may change behaviour and a patch (`0.1.1`)
  only fixes.
* **Nothing has been published yet.** Everything below is development towards `0.1.0`, the first public build, so every row is
  "0.1.0 in development". The milestone names (Phase 1 to Phase 11) are the order the work was done in, not release numbers.
* After `0.1.0` is published, new work goes under `## [Unreleased]` in the changelog, and the release that carries it chooses the
  next number: a minor for new features, a patch for fixes only.
* A milestone is one commit. Each was reviewed before it was committed (a two-model council, three-seat with Fable from Phase 10 on, a documentation drift pass and a
  single-reviewer loop, from Phase 4 on), and the review is written down; the row links it.

## Milestones (all within 0.1.0, in development)

| Milestone | Date | Commit | Tests | What it added | Review |
| --- | --- | --- | --- | --- | --- |
| First build | 2026-11-12 | `0aa0488` | 109 | IMAP watcher, rules or AI ranking, reply drafts (never sent), phone hub, installer, CI and release workflow | `SECURITY.md` |
| Phase 1 | 2026-11-13 | `389e628` | 157 | Notes, tasks you add yourself, due dates, attachments, a copy-only unsubscribe link; hardening (crash containment, sign-out that really wipes, keychain errors) | `SECURITY.md` Review 3 |
| Phase 2 | 2026-11-13 | `dd452dc` | 208 | A list that tidies itself: tasks close when you reply, conversations are one row, mail that left the inbox folds away, the ranking learns from corrections | `SECURITY.md`, `docs/designs/phase2.md` |
| Phase 3 | 2026-11-13 | `583abbe` | 235 | Update notice (off until a repository is named) and "Copy diagnostics"; Google sign-in deferred | `docs/designs/phase3.md` |
| Fixes after the first council | 2026-11-13 | `358f380` | 241 | The phone hub no longer serves a fake sign-in form; six other fixes | `SECURITY.md` Review 4, `docs/2026-11-13_DRIFT_REPORT.md` |
| Phase 4 | 2026-11-13 | `2f895a3` | 303 | Select many, your own rules, follow-up reminders | `SECURITY.md` Review 5, `docs/2026-11-13_PHASE4_REVIEW.md` |
| Phase 5 | 2026-11-13 | `c451c59` | 366 | Reminders and repeats, search inside messages, calendar invitations to `.ics` | `SECURITY.md` Review 6, `docs/2026-11-13_PHASE5_REVIEW.md` |
| Phase 6 | 2026-11-13 | `4a525eb` | 412 | Several mailboxes in one list (up to five) | `SECURITY.md` Review 7, `docs/2026-11-13_PHASE6_REVIEW.md` |
| Phase 7 | 2026-11-14 | `e0fb142` | 453 | Backup and restore, and these history documents | `SECURITY.md` Review 8, `docs/2026-11-14_PHASE7_REVIEW.md` |
| Phase 8 | 2026-11-14 | `d262695` | 459 | A big mailbox stays fast (the window is sent the newest finished tasks, draws 150 rows at a time and moves the cursor without redrawing) | `docs/designs/phase8.md`, `docs/2026-11-14_PHASE8_REVIEW.md` |
| Phase 9 | 2026-11-14 | `e6be8ee` | 459 + 49 window | Tests for the window: the real files in a real browser, with nothing to install (`cd app && npm test`) | `docs/designs/phase9.md`, `docs/2026-11-14_PHASE9_REVIEW.md` |
| Phase 10 | 2026-11-14 | `e9a88f8` | 461 + 49 window + 38 documentation-check | Documentation as code: a documentation map, architecture with diagrams, 14 decision records, API and reference documents, guides, runbooks, product requirements, standards, a testing strategy and templates, with a checker run by CI; a fix for the ranking Test button | `docs/designs/phase10.md`, `docs/2026-11-14_PHASE10_REVIEW.md` |
| Phase 11 | 2026-11-14 | `5c60cfd` | 470 + 49 window + 39 documentation-check | Gaps closed that writing the documentation and the first CI run found: task number, priority and port no longer wrapped to 32, 8 and 16 bits; the phone hub's access code and the ranking mode cannot be set to anything; "Test connection", "Detect" and saving a mailbox check what they are given; write transactions take the lock at once (a "database is locked" seen on Linux); the window's tests run on all three systems in CI | `SECURITY.md` Review 12, `docs/designs/phase11.md`, `docs/2026-11-14_PHASE11_REVIEW.md` |
| After Phase 11 | 2026-11-14 | `c582f4b` (with `aa391ad`, `92207f1`, `ac803d0`) | 470 + 51 window + 39 documentation-check | Upkeep: `rand` 0.10 and `mailparse` 0.18 (tried on a copy first, all tests passing), the CI actions that ran on Node 20 moved to Node 24, notes on what to watch when the release workflow first runs, and the demo backend refuses what the engine refuses (two new window tests) | tests and mutation checks only (no council: no feature) |
| Accessibility | 2026-11-15 | `644d6d2` | 470 + 58 window + 39 documentation-check | Names for nine Settings fields and menus and for each dialog after a redraw, 24-pixel click targets, a readable unstarred star, an Add a task date box like the others; seven window tests that keep it so (`app/test/a11y.test.mjs`) | tests and mutation checks only |
| Release rehearsal | 2026-11-15 | `ef35fe6` | 470 + 58 window + 39 documentation-check + 114 packaged-app | The release workflow rehearsed on throwaway tags until it ran to the end: the macOS build failed (empty Apple signing variables) and is fixed, builds run one at a time, the checksums job reports its files; the packaged-app and install checks moved into `scripts/packaged_check/` with a guard | a rehearsal on GitHub, the packaged-app checks and the install cycle |
| Window-test start-up | 2026-11-15 | `06fa885` | 470 + 61 window + 39 documentation-check + 114 packaged-app | The browser of the window's tests is waited for up to 40 seconds and started once more (three CI failures had one cause: a fixed 15-second wait); three tests of that with a fake browser; the release workflow reports a failed window test as an annotation | the failing runs' own annotation, the new tests and a mutation check |
| No Sent folder is not a refusal | 2026-11-15 | `979bd1c` | 470 + 62 window + 39 documentation-check + 114 packaged-app | "Test connection" passes for a server whose Sent folder cannot be found and says what is lost, instead of refusing the mailbox | new assertions on the scripted server, a window test, and a mutation check of each |
| Damaged-input tests | 2026-11-15 | _pending_ | 477 + 62 window + 39 documentation-check + 114 packaged-app | A test that feeds the mail, calendar, date and rule parsers thousands of damaged inputs and fails on a panic or a slow input (660,000 inputs run once: none) | the test's own checks that it notices a panic and a slow input |

## What has not happened yet

The roadmap was finished with Phase 7; Phase 8 came from measuring a heavy mailbox, Phase 9 from the window having no tests, Phase 10 from the owner's list of the documents a project should have, and Phase 11 from what writing them and the first CI run found. What stands between this and a first release is not code: `UPDATE_REPO` (the code is on GitHub and CI passes on Windows, Linux and macOS),
code-signing, a security contact, and running the live-mailbox and real-calendar manual tests
(`docs/TESTCASES.manual.md`) on clean machines. `RELEASING.md` lists them in order. When the first release is published, its row
goes in [`RELEASE_HISTORY.md`](RELEASE_HISTORY.md) and `[0.1.0]` in the changelog gets its date. Both are made when the release is
prepared, before the tag: the release check refuses a tag without a dated changelog entry and a release-history row.
