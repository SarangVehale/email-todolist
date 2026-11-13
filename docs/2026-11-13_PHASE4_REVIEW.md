# Phase 4 review record, 2026-11-13

Run 1 of the roadmap: select many rows, your own rules, follow-up reminders (design: `docs/designs/phase4.md`). Three review
gates were run on it, each by models other than the one that wrote the code, and every claim was checked against the code
before anything was changed. Fixes got a failing test first; the new guards were then checked by breaking them one at a
time (mutation checks) and confirming a test noticed.

## Gate 1: council (two seats, blind to each other)

| Seat | Verdict | Findings |
| --- | --- | --- |
| Opus | ship with caveats | own nudge and auto-replies count as replies (Gmail); archived reply on servers without All Mail; follow-up records never pruned; open follow-ups past the tenth never rechecked, every one searched on every pass; `sender` rule spoofable by a crafted address or name; race on local task ids; urgent handled differently in apply-to-list; undo of a bulk snooze wipes original snoozes; test names claiming more than they check |
| Sonnet | ship with caveats | the same `sender` rule spoofing; own nudge and auto-replies; prune promise; the check ignoring the stop flag (writes after sign-out); open follow-ups past the tenth and the cost of rechecking; non-atomic follow-up writes; rule text reaching the phone through task reasons; an untested forged branch of apply-to-list |

The seats agreed, so no loop was needed. All of it was fixed except what is recorded as a known limit.

## Gate 2: drift pass (documents and promises against code)

| Pass | Confirmed problems | Outcome |
| --- | --- | --- |
| Design documents and pseudocode (Sonnet) | 20 (13 significant: a state never written, records never pruned, the apply confirmation, bar position, selection pruned not cleared, priority and mute semantics, the done-tab bar, the search command's quotes, "your own address" being the login name, mute not blocked by a failed sender check, the retry wait, open follow-ups rechecked only ten at a time; 7 smaller) | all corrected in `phase4.md` and `PSEUDOCODE.md`; the follow-up and priority items were also behaviour changes |
| User-facing promises (Haiku) | 9 (60-day record promise; "one undo" while mute has none; rules "only lower" forged mail; open follow-ups past ten; only some folders searched; rule changes also queued while write-back is off; rules also run on a new mailbox's first sync; the 90-day clean-up running only at start-up; Enter in select mode) | README, CHANGELOG, PRIVACY, the in-app text and the manual test plan corrected; the clean-up now runs daily |

## Gate 3: sol-loop with Opus as the single reviewer (2 rounds)

| Id | Severity | Outcome |
| --- | --- | --- |
| S1 | should | **fixed**: the select keys obeyed none of the bar's per-tab limits; keys and buttons now share one check |
| S2 | nit | **fixed**: apply-to-list calmed `urgent` even when a raising rule had been refused |
| S3 | nit | declined, recorded as a known limit: a reopened follow-up does not count toward the cap of 20 |
| S4 | nit | declined, recorded: the server's `FROM` search is a substring match |
| S5 | nit | declined, recorded: a list reply does not close an open follow-up (needs a new stored field) |
| S6 | nit | **fixed**: the reason text for a category rule now matches between arrival and apply-to-list |

Round 2 verdict: **APPROVE**, no new findings. Budget used: 2 of 3 rounds. The skill names OpenAI's GPT Sol as reviewer; at
the owner's instruction Opus was used in its place.

## What these gates could not cover

The IMAP behaviour is still checked against a scripted mock server, not a real mailbox. The follow-up search in particular
(`HEADER` with `In-Reply-To` and `References`, `NOT FROM`, `NOT HEADER Auto-Submitted`) is standard IMAP but untested against
Gmail, Fastmail, iCloud and Dovecot themselves; `docs/TESTCASES.manual.md` rows MT-75 to MT-81 are the live checks to run.
