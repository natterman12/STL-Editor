//! egui/eframe interface and wgpu viewport; native and wasm32 targets.

pub mod app;
pub mod build_info;
#[cfg(not(target_arch = "wasm32"))]
pub mod bench;
pub mod engine_link;
pub mod fonts;
pub mod icons;
pub mod platform;
pub mod recover;
pub mod sketch;
pub mod supports;
pub mod theme;
pub mod tools;
pub mod ui;
pub mod updates;
pub mod viewport;
#[cfg(target_arch = "wasm32")]
pub mod web;
