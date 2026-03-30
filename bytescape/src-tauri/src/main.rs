//! # main.rs — Binary Entry Point
//!
//! This is the Rust executable entry point for Bytescape. It does exactly one
//! thing: call [`bytescape_lib::run()`], which initializes the Tauri application,
//! sets up the encrypted keychain, connects to the database, and opens the window.
//!
//! All actual logic lives in `lib.rs` (the library crate). Separating the binary
//! from the library allows the same code to be used by integration tests and the
//! `test-peer` helper without starting a full Tauri window.
//!
//! ## Related Files
//! - `lib.rs` — The library crate where `run()` is defined (app setup, command
//!   registration, state initialization)

// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    bytescape_lib::run()
}
