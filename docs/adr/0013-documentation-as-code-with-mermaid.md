# ADR-0013: Documentation lives in the repository, with Mermaid diagrams

> **Status:** accepted · **Date:** 2026-11-14 · **Deciders:** the owner and Claude · **Last reviewed:** 2026-11-14

## Context

On 2026-11-14 the owner asked for a full set of documents (architecture with diagrams, decision records, API specifications,
guides, runbooks, product requirements, standards, test plans, troubleshooting, templates) "stored with the source and updated in
the same change", concise and current, automated where possible, with as many visuals as possible
([Phase 10](../designs/phase10.md)). The owner has no budget for paid services, so a hosted wiki is out
([ADR-0014](0014-no-paid-services.md)).

## Decision

We keep every document as Markdown in the repository, next to the code it describes, and draw every diagram in Mermaid inside the
Markdown. A document changes in the same commit as the behaviour it describes. `scripts/check_docs.py` checks links and anchors,
Mermaid blocks, the header lines and review dates, the documentation map, the command reference against the code, ADR numbering
and the engine's doc comments, and runs in CI. The rules are in the [style guide](../standards/docs-style-guide.md).

## Consequences

* A diagram is text, so it is reviewed in a diff like code; GitHub draws it with no build step.
* Documents that fall behind the code are caught by the checker where a script can see it (a missing command, a broken link, a
  review date older than 180 days) and by a drift pass at the end of each run where it cannot.
* Mermaid limits what a diagram can show (about 15 nodes per diagram by our own rule), so big pictures are split.
* Every change now has a documentation cost. Revisit if the checker's rules start to block useful changes more than they catch drift.

## Alternatives considered

| Option | Why it was not chosen |
| --- | --- |
| A documentation website from a static site generator | A dependency; GitHub already renders the Markdown and Mermaid. |
| A hosted wiki | A paid service, and it drifts from the code because it is not changed in the same commit. |
| Exported diagram images | Cannot be reviewed in a diff; the style guide forbids committing them. |

## References

* [Phase 10 design](../designs/phase10.md), [documentation style guide](../standards/docs-style-guide.md)
* `scripts/check_docs.py`, `scripts/test_check_docs.py`
