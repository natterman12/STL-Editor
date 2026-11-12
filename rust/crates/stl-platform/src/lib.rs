//! Desktop only: updater, settings, autosave, slicer and SolidCraft hand-off. Compiled out of the web target.

#[cfg(not(target_arch = "wasm32"))]
pub mod fins;

#[cfg(not(target_arch = "wasm32"))]
pub mod net;
