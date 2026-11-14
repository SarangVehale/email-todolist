# ADR-0011: The window gets a bounded list on a big mailbox

> **Status:** accepted · **Date:** 2026-11-14 · **Deciders:** the owner and Claude · **Last reviewed:** 2026-11-14

## Context

Measured with the packaged app and 3,000 open plus 5,000 finished tasks, the answer to the window's 3-second poll took 250 to 420
ms and was 6.4 MB, drawing the list took 2.1 s, and one cursor key press took 0.95 s. The causes: every finished task was sent on
every poll, every row drew its hidden detail panel, and a cursor key drew the whole page again ([Phase 8](../designs/phase8.md)).
The phone asks for the same answer over Wi-Fi. What is stored and for how long was not to change.

## Decision

We send every open task but only the 200 most recently finished ones, with `done_total` (`Store::tasks_for_window`,
`DONE_SHOWN`); the window may ask for more with `state { done: N }`, which the engine clamps between 200 and 20,000
(`done_asked`, `DONE_MAX`). The window draws 150 rows of each part of the list at a time, draws a row's details only when it is
open, moves the cursor by changing two classes, and does not poll while hidden.

## Consequences

* Measured after the change: loading and drawing 246 ms, a key press 2 ms, the Done tab 13 ms, 2,945 page elements; the poll
  answer 127 ms and 2.6 MB, just over its 120 ms and 2.5 MB target ([Phase 8](../designs/phase8.md)).
* A finished task older than the newest 200 is reached through "Show older" on the Done tab; the engine's search still covers it.
* Undo of a bulk finish reopens every task, including ones the next poll no longer holds.
* A finished conversation that straddles the limit shows only the messages the window holds.
* What is left of the answer is the open tasks themselves. Revisit with a "revision" stamp, so an unchanged answer is not sent at
  all, or with paging of the open list, if open lists grow far beyond 3,000.

## Alternatives considered

| Option | Why it was not chosen |
| --- | --- |
| Page the open list from the engine | Out of scope in Phase 8: open tasks are what you act on, so all of them are sent. |
| A revision stamp so an unchanged answer is not sent | Out of scope for this run; listed as the next step. |
| Virtual scrolling | Out of scope; drawing 150 rows at a time met the targets. |

## References

* `engine/src/core.rs` (`state`, `done_asked`), `engine/src/store.rs` (`tasks_for_window`), `app/ui/main.js` (`ROW_PAGE`, `load`)
* [Phase 8 review](../2026-11-14_PHASE8_REVIEW.md), `scripts/scale_check`
