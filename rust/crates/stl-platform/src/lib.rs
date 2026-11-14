//! Desktop only: updater, settings, autosave, slicer and SolidCraft hand-off. Compiled out of the web target.

#[cfg(not(target_arch = "wasm32"))]
pub mod fins;

#[cfg(not(target_arch = "wasm32"))]
pub mod net;

#[cfg(not(target_arch = "wasm32"))]
pub mod paths;

#[cfg(not(target_arch = "wasm32"))]
pub mod settings;

#[cfg(not(target_arch = "wasm32"))]
pub mod autosave;

#[cfg(not(target_arch = "wasm32"))]
pub mod update;

#[cfg(not(target_arch = "wasm32"))]
pub mod fonts;
