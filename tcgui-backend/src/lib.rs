// nlink 0.26 boxed `Connection::send_dump` to close the *layout*-depth
// recursion class (nlink #315). That trades depth in one solver for depth in
// another: proving `Send` for a future that awaits down the netlink request
// chain now recurses past rustc's default limit of 128, and `tokio::spawn`
// needs exactly that proof. The limit belongs to this crate, not to nlink —
// which is the point nlink's own changelog makes about who can fix it.
// Compile-time only; no runtime cost.
#![recursion_limit = "256"]

//! Library crate exposing modules for testing
//!
//! This exposes internal modules for integration tests

pub mod bandwidth;
pub mod commands;
pub mod config;
pub mod container;
pub mod diagnostics;
pub mod interfaces;
pub mod namespace_watcher;
pub mod netns;
pub mod network;
pub mod preset_loader;
pub mod scenario;
pub mod tc_commands;
pub mod utils;
