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
  "0.1.0 in development". The milestone names (Phase 1 to Phase 7) are the order the work was done in, not release numbers.
* After `0.1.0` is published, new work goes under `## [Unreleased]` in the changelog, and the release that carries it chooses the
  next number: a minor for new features, a patch for fixes only.
* A milestone is one commit. Each was reviewed before it was committed (a two-model council, a documentation drift pass and a
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
| Phase 7 | 2026-11-14 | _(set when it is committed)_ | 453 | Backup and restore, and these history documents | `SECURITY.md` Review 8, `docs/2026-11-14_PHASE7_REVIEW.md` |

## What has not happened yet

Applying the roadmap is finished with Phase 7. What stands between this and a first release is not code: a GitHub repository,
code-signing, a security contact, and running the live-mailbox and real-calendar manual tests
(`docs/TESTCASES.manual.md`) on clean machines. `RELEASING.md` lists them in order. When the first release is published, its row
goes in [`RELEASE_HISTORY.md`](RELEASE_HISTORY.md) and `[0.1.0]` in the changelog gets its date. Both are made when the release is
prepared, before the tag: the release check refuses a tag without a dated changelog entry and a release-history row.
