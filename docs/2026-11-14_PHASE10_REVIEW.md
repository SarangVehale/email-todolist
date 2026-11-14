# Phase 10 review record, 2026-11-14

Run 7, which is not on the original roadmap: documentation as code (design: `docs/designs/phase10.md`), asked for by the owner on
2026-11-14 with a list of document kinds. The work is mostly documents (about 80, with 54 diagrams), a checker for them
(`scripts/check_docs.py`), 100 doc comments in the engine, and one defect found while writing. Three gates, each by models other than
the one that wrote the work; for the first time the council has three seats, with Fable added at the owner's request. Every finding was
checked against the code before anything changed.

## How the documents were written

Four drafters (two Opus, two Sonnet) wrote disjoint sets of files in parallel from the same brief (style guide, templates, the list of
planned file names), each told to verify every statement against the code and to report where an existing document disagreed with it. A
fifth added the missing doc comments. They reported 17 disagreements between existing text and the code before any reviewer started;
all were corrected (for example SECURITY.md said eleven commands may be called from a phone, the code has twelve; PRIVACY.md and
SECURITY.md said Ollama stays on the computer, true only while its address is this computer's; a module comment described a file
that does not exist; the wizard's sentence about filing mail was loose).

## Gate 1: council (three seats, blind to each other)

| Seat | Verdict | Claims checked | Findings |
| --- | --- | --- | --- |
| Opus | ship with caveats | about 75 | the runbook for a release said the release workflow runs neither `npm test` nor the documentation checks (false: this same change added both); no Phase 10 row in the version history or review record yet; a hub timeout described as a limit on the whole head when it is on each read; "within 15 seconds" for a step whose name lookup has no limit; the README's Develop lines could not be pasted as a script, and a bash-only form on a Windows-first project; a doc comment above a multi-line attribute would be reported missing |
| Sonnet | ship with caveats | about 85 | the generated test catalogue was stale (it would fail CI's "catalogue is up to date" on day one); the same release-workflow statements; a guide quoting wizard text that this change had already corrected; "Activity log" for the tab that is called "Log"; the checker crashed printing an arrow to a Windows pipe; an unquoted label with parentheses passed the diagram check; no way to run the app from source and no Python or Linux-library prerequisites in the README; no pull-request template or contributing page, so "update the documents in the same pull request" was only a statement |
| Fable | ship with caveats | about 130 | the same release-workflow statements; an FAQ answer that said the build makes macOS and Linux packages (configured, never built); the diagrams in the design used unquoted labels; the checker passed reference-style links nobody uses, could crash on an impossible date, took any `"word" =>` arm in `core.rs` for a command, and would turn CI red on every document at once on 2027-05-13; 586 relative links and 138 test names in the user stories were checked mechanically and are right |

## Gate 2: drift passes

| Pass | Claims checked | Confirmed drifts |
| --- | --- | --- |
| Technical documents (Sonnet): architecture, ADRs, API, reference, standards, testing, product | about 700 | 8: the `diagnostics` report prints a server name only for eleven well-known providers; a mistyped host gives the system's lookup error, not "could not find the server"; a mailbox's data is deleted in two transactions, not one; the update check retries hourly while the server gives no answer; `rules_apply` logs only when it changed something; `save_settings` refuses a patch that would change, not "touch", the account; a hub code is cleared only by `save_settings`; every mailbox's threads stop when one account is saved |
| Promises in guides, FAQ, troubleshooting, README, PRIVACY (Haiku) | about 310 | 8: the Test button for ranking said "Works" when no model or key was chosen (**a defect in the engine, fixed below**); the FAQ's macOS and Linux claim; "never deletes" a message (a failed move on a non-Gmail server copies to Archive and removes the original); one connection for each change (it is one for each batch of up to 50); which rules may raise a priority ("people you write to, or who reply to you"); PRIVACY.md left out two of the fields phone search covers; a 64-character limit read as 200 |
| Procedures: runbooks, test plan, strategy, standards checklists, templates (Haiku) | about 320 | 11: the release-workflow statements again; a flowchart that put "Reading your latest mail" in the bottom bar; "from any folder" for a command with a relative path; bash-only syntax with no PowerShell form; `cargo clippy` without `--locked` in RELEASING.md; a step name and a parameter name that differ from the code |

## What was found in the code, and fixed

* **The Settings "Test" button for ranking said it worked when nothing was configured.** With the mode "Local AI (Ollama)" and no model
  chosen, or "Claude" and no key saved, the engine quietly ranks with rules, and rules never fail, so Test showed a green "Works".
  `test_ai` now answers "No model is chosen" or "No API key is saved". Test first (`engine/tests/phase10.rs`, red before the fix), and each of
  the two guards was put back to its old behaviour once: the test failed each time.
* Doc comments: 100 public items had none (the checker now requires one for every public function, struct, enum, trait, constant and
  type); one comment broke `cargo doc` and was fixed; four comments described the code wrongly and were corrected.
* The demo backend's two mailbox error messages are now the engine's words, and the window test that reads one was updated.
* A wizard sentence now says what the code does (stars become flags; priorities become labels on Gmail only).

## What the checker now guards, and what it still cannot

`scripts/check_docs.py` has 38 tests of its own, each rule failing when broken: links and anchors (a link to a source line must use
`#L120`, which GitHub resolves), a language on every code block, a valid Mermaid first line, balanced brackets, subgraphs and blocks,
quoted labels with parentheses, the header line and review date of the documents that need one (a warning 30 days before the 180-day
limit), the map reaching every document, a section in the command reference for each engine command and none invented (read only from
the dispatcher functions), the window calling only real commands, ADR numbering, filler words, a doc comment on every public item,
the hub's limits and routes appearing in the hub document, and the README's test counts matching the generated catalogue.

It cannot tell whether a sentence is true. It does not check the arguments or answers in the command reference, `openapi.yaml`
(only that it parses, by a reader), quoted window texts, the test names in the user stories, or that a diagram is valid beyond its
structure; the drift passes and the council are how those were checked, and they will have to be done again as the code changes.
The 180-day rule turns CI red on every unchanged document together on 2027-05-13; the warning appears 30 days before, and bumping a
date without reading the document defeats the rule, so the maintenance runbook says what a review is.

## Gate 3: sol-loop with Opus as the single reviewer

Round 1 (a ledger of the council's and the drift passes' findings was given up front; the reviewer read about 60 claims in the
documents the drift passes covered least, the engine change and all of the checker):

| Id | Severity | Outcome |
| --- | --- | --- |
| F1 | should | **fixed**: the leaked-secret runbook said the old phone QR code "asks for an access code"; it opens the page with `error: unauthorized` in the bottom bar |
| F2 | should | **fixed**: the checker failed a correct file saved with a byte-order mark (Notepad and Windows PowerShell 5.1 write one): files are read as `utf-8-sig` |
| F3 | nit | **fixed**: a heading that starts with an emoji got the wrong anchor (GitHub keeps the hyphen it leaves) |
| F4 | nit | **fixed**: link forms the checker skipped or misread: titles in single quotes or parentheses, targets in angle brackets, reference-style definitions, root-relative paths |
| F5 | nit | **fixed**: the doc-comment check accepted `////` and an empty `///` and missed `pub async unsafe fn` and `pub unsafe extern "C" fn` |
| F6 | nit | **fixed**: the demo backend's Test for ranking still said "Works" for no model or no key; it now refuses like the engine |
| F7 | nit | **fixed**: a story promised that switching the hub off and on gives a new access code with no test; there is one, and each of its two guards was put back to its old behaviour once |

The reviewer judged the engine change (`test_ai`) correct: it does not block the wizard's path (which saves the key and the mode
before it tests), a model name with spaces, or a key held only in the keychain, and it refuses only where ranking would have quietly used rules.

Round 2: **APPROVE**. F1 to F7 verified by trying to defeat each with a small tree; two nits: a footnote definition (`[^1]: text`)
was read as a link definition, and an image inside a link (`[![x](a.png)](b.md)`) had only its inner target checked. Both fixed, with a
test each. Budget used: 2 of 3 rounds. The skill names OpenAI's GPT Sol; at the owner's instruction Opus was used in its place.

## What these gates could not cover

Nothing was run on Linux or macOS, the CI and release workflow changes have never run (no push has been made), and Mermaid diagrams
were checked for structure but not drawn by a renderer. The documents state what is not verified, in the same words as the review
records before this one.
