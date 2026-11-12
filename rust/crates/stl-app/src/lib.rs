//! egui/eframe interface and wgpu viewport; native and wasm32 targets.

#[cfg(target_arch = "wasm32")]
pub mod web;
