# ADR-0005: One SQLite file holds the app's data

> **Status:** accepted · **Date:** 2026-11-12 · **Deciders:** the owner and Claude · **Last reviewed:** 2026-11-14

## Context

The app keeps tasks, drafts, corrections, sender choices, the outbox of mailbox changes and an activity log, on your computer
only. Several threads write at once (one sync and one write-back per mailbox, the scheduler, the window's commands), the app can
be killed at any moment, and a new version must read an old version's data. Nothing may need a second program to install.

## Decision

We keep all of it in one SQLite database, `agent.db`, in the app's local data folder, with SQLite bundled into the binary
(`rusqlite` with `bundled`). Each thread opens its own connection in WAL mode with a 5-second busy timeout. `Store::open`
creates and upgrades the schema in place, and `user_version` records its format (`SCHEMA_VERSION`).

## Consequences

* One file to back up, wipe or set aside; no server. Transactions where they matter (a restore is all or nothing).
* Search inside message text is an FTS5 index in the same file ([Phase 5](../designs/phase5.md)); `secure_delete` overwrites removed rows.
* Upgrades only add tables and columns; a copy (`agent.db.bak`) is taken first. A database from a newer version is refused, never
  repaired. A damaged one is set aside and rebuilt ([data model](../architecture/data-model.md#schema-versions-and-a-damaged-database)).
* Connections opened at the same moment raced while setting the database up; opening now takes turns ([Phase 6](../designs/phase6.md)).
* The file is not encrypted at rest; full-disk encryption is the advice ([SECURITY.md](../../SECURITY.md)).
* Revisit if encryption at rest becomes a requirement, or if data must sync between computers (out of scope in [Phase 7](../designs/phase7.md)).

## Alternatives considered

| Option | Why it was not chosen |
| --- | --- |
| JSON files | No transactions, no search, and every change rewrites a whole file. |
| A database server | A second program to install and run. |

## References

* `engine/src/store.rs` (`SCHEMA`, `Store::open`, `Store::open_recovering`)
* [Data model](../architecture/data-model.md)
