//! Mail engine: IMAP watcher, signals, pluggable triage, SQLite store, write-back, phone hub.
pub mod ai;
pub mod commands;
pub mod core;
pub mod dates;
pub mod diag;
pub mod drafts;
pub mod hub;
pub mod jobs;
pub mod mail;
pub mod secrets;
pub mod settings;
pub mod signals;
pub mod store;
pub mod triage;
pub mod watcher;
pub mod writeback;
