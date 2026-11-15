//! Mail engine: IMAP watcher, signals, pluggable triage, SQLite store, write-back, phone hub.
//!
//! The engine reads text nobody here wrote, so it is held to two rules by the compiler: no `unsafe` code at all, and no `unwrap` or `expect`
//! outside tests (a panic in the sync thread would be a crash on every start; see `mail::parse`, which also contains the parser's panics).
#![forbid(unsafe_code)]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]
pub mod ai;
pub mod backup;
pub mod calendar;
pub mod commands;
pub mod core;
pub mod dates;
pub mod diag;
pub mod drafts;
pub mod followup;
pub mod hub;
pub mod jobs;
pub mod mail;
pub mod oauth;
pub mod reminders;
pub mod rules;
pub mod secrets;
pub mod settings;
pub mod signals;
pub mod store;
pub mod tray;
pub mod triage;
pub mod updates;
pub mod watcher;
pub mod webpage;
pub mod writeback;
