# ADR-0001: A Tauri shell around a Rust engine library

> **Status:** accepted · **Date:** 2026-11-12 · **Deciders:** the owner and Claude · **Last reviewed:** 2026-11-14

## Context

The owner asked for one installer with all setup inside the app, a window that stays in the tray and keeps watching mail,
desktop notifications, start at login, passwords in the OS keychain, and the same list on a phone. The engine parses mail that
attackers control ([SECURITY.md](../../SECURITY.md), trust boundaries). The first commit (2026-11-12) records the choice, not a
comparison; the alternatives below are the obvious ones, weighed against those constraints.

## Decision

We build the app as a Tauri 2 shell (`app/src-tauri`) that links one Rust library, `engine/`, which holds all the logic. The
window reaches it through a single Tauri command, `rpc`, that calls `Core::rpc`.

## Consequences

* One process and one executable: the shell depends on the engine by path and the release profile strips and optimises for
  size (`app/src-tauri/Cargo.toml`).
* The engine is tested on its own with `cargo test`, with no window, including a scripted IMAP server.
* The same `Core::rpc` serves the phone hub (`Core::rpc_public`), so desktop and phone share one command set.
* Mail parsing runs in a memory-safe language; a panic is contained per message ([runtime](../architecture/runtime.md)).
* The window uses each system's webview, so it can behave differently per OS; Linux builds need GTK and WebKitGTK libraries,
  and each OS is built separately in CI (`.github/workflows/release.yml`).
* Revisit if the system webview cannot do something the window needs, or if Tauri 2 stops being maintained.

## Alternatives considered

| Option | Why it was not chosen |
| --- | --- |
| Electron | Ships its own browser and a Node runtime: a larger installer, and a second language for the background work. |
| A native window per OS | Three interfaces to build and test, and none of them could be served to a phone. |
| A background service and a browser page | A second program to install and start, against "one installer". |

## References

* `app/src-tauri/src/lib.rs`, `engine/src/core.rs`
* [Architecture overview](../architecture/overview.md)
