# Review record: everything after Phase 11, 2026-11-15

The work between `bec607b` (the Phase 11 review record) and `54514d9`, about twenty commits that were not one feature: dependency updates,
an accessibility pass, a demo backend that refuses what the engine refuses, a mailbox with no Sent folder, a fix to the window tests' browser
start-up, damaged-input tests, the licences in the installer, the packaged-app checks moved into the repository, and five rehearsals of the
release workflow. The standing gates were run once over all of it, because none of the pieces was a feature run of its own.

## Gate 1: council (three seats, blind to each other)

| Seat | Verdict | Findings |
| --- | --- | --- |
| Opus | ship with fixes | the real-app check could overwrite and delete a real keychain password (suggested running the app with `INBOX_AGENT_KEYCHAIN=memory`, which is what was done); its cleanup sat outside `finally`, so an early exit left a seeded folder and a public access code on the network; the install check had no guard against a real install and killed any process called `app`; a Windows browser whose start timed out was left running; a 2-second wall-clock limit in the fuzz test; the signed release step was chosen on one Apple secret; the no-Sent-folder text omitted the lost follow-up reminders |
| Sonnet | ship with fixes | the damaged-input test could not see a panic in the mail parser (`mail::parse` turns a panic into an error and the test threw the answer away), and its samples were never asserted to parse; the final release workflow had not been rehearsed after the licence files were added; the Apple secrets, with the two `with:` blocks kept equal by hand; the same keychain, cleanup, install-check and browser-leak findings; the licence list would fail every Dependabot update; the Linux AppImage's system libraries were not mentioned; small leftovers (a dead CSS rule, `rust-version`) |
| Fable | ship with fixes | the same keychain, cleanup, install-check and browser-leak findings; the Apple secrets; the 2-second limit could trip on a stalled shared runner; MPL-2.0 allowed for any crate, not five named ones; the harness test failing instead of skipping on old Node |

All of it was true, and all of it was fixed. No seat found a blocker for the product itself (engine, window, installer, workflows); the
three majors were in developer scripts and a test.

## Gate 2: drift passes

| Pass | Claims checked | Confirmed drifts |
| --- | --- | --- |
| Technical documents (Sonnet) | about 85 | 9: "in parallel" for builds that run one after another; rehearsal counts; touch targets "30" (it is 34 tall on a touch screen); which scripts refuse to run; MIT-0 missing from a list; licence steps missing from the lists of what CI and the release run; an installer table without the licence files; the accessibility test's reach |
| Promises to users (Haiku) | about 75 | 1: a flowchart label that was not a substring of the real message |

## Gate 3: sol-loop with Opus as the single reviewer

| Round | Verdict | Findings |
| --- | --- | --- |
| 1 | changes requested | **major:** the in-memory keychain reached two of the three scripts that start the app; `scale.py` seeds a first mailbox and would read a real password (fixed); **minor:** an AppImage that cannot be extracted ended the step without a message (fixed) |
| 2 | approve | two documentation points: name the tree rehearsal 5 ran on, and add a history row (both done) |

Budget used: 2 of 3 rounds. The skill names OpenAI's GPT Sol; at the owner's instruction Opus was used in its place.

## What the gates could not cover

* The signed release paths (Apple notarization, the Windows certificate step, which rewrites `tauri.conf.json`, now with `resources`)
  have never run. A half-set group of Apple secrets now stops the build with a message; nothing else about that path is known.
* The macOS and Linux packages were built and their contents listed by a rehearsal, never installed or opened on those systems.
* The Windows installer's contents are checked by `scripts/packaged_check/install_check.ps1` on a developer's machine, not in CI.
* The licence list covers Rust crates; the system libraries in the Linux AppImage are named in `THIRD_PARTY_NOTICES.md`, not listed.
* A review by models reads code and documents; it did not use the app.
