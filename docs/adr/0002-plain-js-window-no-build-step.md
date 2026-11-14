# ADR-0002: A plain JavaScript window with no build step

> **Status:** accepted · **Date:** 2026-11-12 · **Deciders:** the owner and Claude · **Last reviewed:** 2026-11-14

## Context

The window runs in three places: the Tauri webview, a phone browser served by the hub, and `demo.html` with the demo backend
(`mock.js`) for development. The hub serves the window's files from a list compiled into the engine (`hub.rs`, `FILES`), and
both the app and the hub apply a strict content security policy (`script-src 'self'`). The project avoids dependencies: the
window's tests were written without one because "a new dependency was not accepted" ([Phase 9](../designs/phase9.md)).

## Decision

We write the window as plain HTML, CSS and JavaScript in `app/ui`, loaded exactly as they are in the repository: no framework,
bundler, transpiler or type checker. The scripts load in a fixed order (`main.js`, `drafts.js`, `setup.js`, `boot.js`) and
share their top-level names.

## Consequences

* What is in the repository is what runs, on the desktop and on the phone; there is no build output to keep in step.
* `app/package.json` has one development dependency, the Tauri command line. CI checks each script with `node --check` and runs
  the window's tests in a browser ([ADR-0012](0012-window-tests-in-a-real-browser.md)).
* Performance is done by hand: drawing 150 rows at a time and a fast path for cursor keys ([Phase 8](../designs/phase8.md)).
* There are no types and the scripts share globals, so a rename must be searched for across the four files.
* Revisit if the window grows to where a type checker or components would prevent more defects than a build step costs.

## Alternatives considered

| Option | Why it was not chosen |
| --- | --- |
| A framework with a bundler (React, Vue, Svelte) | A build step and a dependency tree to audit, and the hub would serve build output instead of the files in the repository. |
| TypeScript | Needs a compiler, which is a build step. |

## References

* `app/ui/index.html`, `app/ui/main.js`, `engine/src/hub.rs`
* [README.md](../../README.md), Repository table
