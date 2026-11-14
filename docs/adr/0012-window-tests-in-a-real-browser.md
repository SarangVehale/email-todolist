# ADR-0012: The window is tested in a real browser with no new dependency

> **Status:** accepted · **Date:** 2026-11-14 · **Deciders:** the owner and Claude · **Last reviewed:** 2026-11-14

## Context

The engine had 459 tests; the window (about 1,250 lines of JavaScript that also run on the phone) had none. Most of the defects
found in the reviews of Runs 3 to 5 that no test would catch again were in it: a Cancel that threw while a request was out, a
dialog that never closed, a key press stolen by a focused row ([Phase 9](../designs/phase9.md)). The project's
`app/package.json` has no dependency beyond the Tauri command line, and a new one was not accepted for this.

## Decision

We test the window by running the real `app/ui` files in a Chromium-based browser that is already on the machine, driven over
the browser's debugging protocol by a harness built only from Node's own modules (`app/test/harness.mjs`: `node:http`, the
built-in `WebSocket` and `fetch`), with tests written in `node:test`. Each test opens `demo.html?demo=1`, so the window runs
against its own demo backend (`mock.js`).

## Consequences

* `cd app && npm test` needs nothing installed beyond Node 22 and a browser. Real key and mouse events give real focus and layout.
* Without a browser the tests are skipped, unless `INBOX_AGENT_UI_TESTS=require`, which CI sets on all three systems.
* The tests prove what the window does with the answers it gets, not that the engine's answers are right; the engine's tests and
  the packaged-app run cover that.
* The harness has to clean up browsers and profile folders itself, which was hard on Windows (Phase 9 records 43 browsers left
  behind by the first version).
* CI runs them on Linux, Windows and macOS (the release workflow, on Linux only). They ran on Linux only until Phase 11. One Linux
  run failed and the next, on the same code, passed; the cause was not found.

## Alternatives considered

| Option | Why it was not chosen |
| --- | --- |
| A browser-automation package (Playwright, Puppeteer) | A new dependency, which was not accepted. |
| A simulated DOM in Node | No real focus or layout, which is where several of the defects were. |
| Testing against the engine | The window's tests then need a running engine and mailbox; the demo backend lets a test make a request slow or fail. |

## References

* `app/test/harness.mjs`, `app/test/*.test.mjs`, `.github/workflows/ci.yml`
* [Phase 9 review](../2026-11-14_PHASE9_REVIEW.md)
