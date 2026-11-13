//! egui_kittest harness for the app shell (milestone 4a, task 0 step 2).
//!
//! `cargo test -p stl-app --test ui`
#![cfg(not(target_arch = "wasm32"))]

// `mod ui` would also see this file as `ui.rs` next to `ui/`; pin the directory module.
#[path = "ui/mod.rs"]
mod ui;
