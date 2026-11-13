//! App-level egui_kittest tests. The harness lives in [`harness`].
//!
//! M4b UI parity (Task 8): `desktop_mirror`, `desktop_place`, `desktop_tools`, `gizmo`,
//! `sf11`, `sf9`, `sfcov`, `sfpanel`, `touch`, `v110`, `web7`, `web8`.

mod harness;

mod desktop_mirror;
mod desktop_place;
mod desktop_tools;
mod gizmo;
mod sf11;
mod sf9;
mod sfcov;
mod sfpanel;
mod touch;
mod v110;
mod web7;
mod web8;

use std::path::Path;

use harness::app_harness;

fn fixture(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/projects").join(name)
}

/// Opens `basic.stlx` through the harness and checks the engine filled the body list.
#[test]
fn opens_basic_stlx_body_count() {
    let path = fixture("basic.stlx");
    let h = app_harness(Some(path.as_path()));
    let n = h.state().bodies.len();
    // basic.stlx: two Imports active (rollback before the Move) → two bodies.
    assert_eq!(n, 2, "basic.stlx should yield two bodies, got {n}");
}
