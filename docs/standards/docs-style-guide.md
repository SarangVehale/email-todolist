# Documentation style guide

> **Status:** current · **Audience:** anyone who writes or reviews a document in this repository · **Last reviewed:** 2026-11-14

This guide says how documents in this repository are written and kept. It follows the
[Google developer documentation style guide](https://developers.google.com/style) (free, widely used) and adds the rules that
only this project needs. When the two disagree, this page wins.

## Principles

1. **Docs are code.** Documents live in the repository next to the source and change in the same commit as the code they describe.
   A change that makes a document untrue is not finished until the document is fixed.
2. **Short and current beats long and complete.** Say what a reader needs and link to the source of truth for the rest. A sentence
   that can only be kept true by hand should be generated or checked by a script instead.
3. **A picture where a picture is clearer.** Every architecture, flow and state document has at least one diagram.
4. **One home for each fact.** Describe a thing once, then link to it. The documents that already own a topic are listed in the
   [documentation map](../README.md).

## Kinds of document, and where they live

| Kind | Answers | Home | Template |
| --- | --- | --- | --- |
| README | What is this, how do I start? | `README.md` | none |
| Tutorial / how-to guide | How do I do one task? | `docs/guides/` | [how-to](../templates/how-to-guide.md) |
| Reference | What exactly does it accept and return? | `docs/api/`, `docs/reference/` | none (tables) |
| Architecture | How is it built, and why? | `docs/architecture/` | none (diagrams) |
| Architecture decision record (ADR) | Why did we choose this? | `docs/adr/` | [adr](../templates/adr.md) |
| Design document | What will this change do? | `docs/designs/` | [design](../templates/design-doc.md) |
| Product requirements, user stories | What should it do, for whom? | `docs/product/` | [user story](../templates/user-story.md) |
| Runbook / playbook | Something is wrong or must be done: what are the steps? | `docs/runbooks/` | [runbook](../templates/runbook.md) |
| Test plan, strategy, catalogue | What do we test and how? | `docs/testing/`, `docs/TESTCASES*.md` | [test plan](../templates/test-plan.md) |
| Release notes, changelog, history | What changed, what was published? | `CHANGELOG.md`, `docs/VERSION_HISTORY.md`, `docs/RELEASE_HISTORY.md` | [release notes](../templates/release-notes.md) |
| Review record | What did the reviewers find? | `docs/<date>_*_REVIEW.md` | none |
| Postmortem | What went wrong, and what changes? | `docs/postmortems/` (the folder is made with the first one) | [postmortem](../templates/postmortem.md) |
| Troubleshooting, FAQ | It does not work, why? | `docs/troubleshooting.md`, `docs/faq.md` | none |
| Code comments and docstrings | What does this code promise? | in the source | see [coding standards](coding-standards.md) |

## Voice and wording

* Write to the reader as "you". Use the present tense and the active voice: "The engine reads new mail", not "New mail will be read".
* Use sentence case for headings ("Restore from a backup"), not title case.
* Keep sentences short. Prefer one idea per sentence and one topic per paragraph.
* Do not use "simply", "just", "easy", "obviously" or "please note".
* Use **British spelling**, as the existing documents do (behaviour, colour). Do not mix spellings in one document.
* Use the terms in the [glossary](../reference/glossary.md) and no others for the same thing: *task*, *mailbox*, *conversation*,
  *window*, *engine*, *hub*, *app password*.
* Say what the app does, never what it "tries" to do. If something is not verified, say so and where it is tracked.

## Structure of a document

Every document starts the same way:

```markdown
# Title in sentence case

> **Status:** current | draft | superseded · **Audience:** who it is for · **Last reviewed:** YYYY-MM-DD

One or two sentences: what this document is for and what you can do with it.
```

* Put the answer first, the background after.
* Use numbered lists for steps in order, bullet lists for sets, tables for things with several attributes.
* A document longer than about 300 lines should be split, or should start with a table of contents.
* **Update `Last reviewed` whenever you check or change the document.** A document not reviewed for 180 days fails the documentation check.

## Diagrams

* Use [Mermaid](https://mermaid.js.org) in a fenced block tagged `mermaid`. GitHub and most editors draw it, and a text diagram can
  be reviewed in a diff like code. Do not commit exported images of diagrams.
* Put one sentence before each diagram saying what it shows, and keep it to about 15 nodes. Split a bigger picture into two.
* Use the diagram type that fits: `flowchart` for flow and structure, `sequenceDiagram` for who talks to whom in what order,
  `stateDiagram-v2` for a thing with states, `erDiagram` for stored data, `C4Context`-style boxes drawn as a `flowchart` for the big picture.
* Name things as the code names them, so a reader can search for them.

## Code, commands and examples

* Tag every fenced block with its language (`bash`, `powershell`, `json`, `rust`, `javascript`, `yaml`, `text`).
* A command a reader may run goes in its own `bash` block with no `$` prompt and no output mixed in. Show output in a separate `text` block.
* Examples use invented data (`you@example.com`, `imap.example.com`), never a real address, host or secret.

## Links

* Link with relative paths (`[the design](../designs/phase9.md)`), never absolute URLs to this repository.
* Link to the source of truth instead of copying it. When you move or rename a file, fix the links in the same commit.
* `python scripts/check_docs.py` checks every relative link and anchor.

## Review

* A document that changes behaviour claims is reviewed with the code in the same commit.
* Every run of work ends with a drift pass: someone other than the author compares the documents with the code and fixes what has
  drifted. See [the testing strategy](../testing/strategy.md).
* Documents are checked in CI by `scripts/check_docs.py` (links, diagrams, the map, the RPC reference, ADR numbering, review dates).
