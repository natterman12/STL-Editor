//! Milestone 7 UI tests (Task 7, spec 2.8): a face highlight a tool made is kept as
//! (body, triangles), drawn from the body's mesh as it is now, and cleared when the tool closes.
#![cfg(not(target_arch = "wasm32"))]

use std::sync::Arc;

use egui_kittest::Harness;
use stl_app::app::StlApp;
use stl_app::engine_link::BodyView;
use stl_app::engine_link::native::NativeLink;
use stl_app::viewport::{Viewport, overlays::Depth};
use stl_kernel::MeshArrays;

use super::harness::{app_harness_ex, click_at, open_tool, project, select_first_body, settle, wait_for};

/// Split open on the first body with its top face picked: the "face" highlight is showing.
fn picked() -> (Harness<'static, StlApp>, Arc<NativeLink>) {
    let (mut h, link) = app_harness_ex(Some(&project("basic.stlx")), egui::vec2(1280.0, 800.0));
    select_first_body(&mut h);
    assert!(open_tool(&mut h, "split"));
    // the top face's middle, a little in from the edge
    let m = &h.state().bodies[0].mesh;
    let (mut lo, mut hi) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
    for p in m.positions.chunks_exact(3) {
        for i in 0..3 {
            lo[i] = lo[i].min(p[i]);
            hi[i] = hi[i].max(p[i]);
        }
    }
    let top = [
        f64::from(lo[0] + hi[0]) / 2.0,
        f64::from(lo[1] + hi[1]) / 2.0,
        f64::from(hi[2]),
    ];
    let at = h.state().viewport.world_to_screen(top);
    click_at(&mut h, at);
    settle(&mut h, &link);
    assert_eq!(
        h.state().viewport.tool_highlight_tags(),
        ["face"],
        "a click on the top face highlights it"
    );
    assert!(!h.state().viewport.tool_highlight_corners("face").is_empty());
    (h, link)
}

#[test]
fn ok_clears_highlights() {
    let (mut h, link) = picked();
    h.state_mut().tool_ok();
    settle(&mut h, &link);
    assert!(h.state().tool().is_none(), "OK closed the tool");
    assert!(h.state().viewport.tool_highlight_tags().is_empty());
}

#[test]
fn cancel_clears_highlights() {
    let (mut h, _link) = picked();
    h.state_mut().tool_cancel();
    h.run_steps(2);
    assert!(h.state().viewport.tool_highlight_tags().is_empty());
}

#[test]
fn escape_clears_highlights() {
    let (mut h, _link) = picked();
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    assert!(h.state().tool().is_none(), "Escape closed the tool");
    assert!(
        h.state().viewport.tool_highlight_tags().is_empty(),
        "no highlight is left"
    );
}

#[test]
fn another_tool_clears_highlights() {
    let (mut h, _link) = picked();
    assert!(h.state_mut().open_tool("hole", None));
    h.run_steps(3);
    assert!(h.state().viewport.tool_highlight_tags().is_empty());
}

fn cube() -> BodyView {
    let p = [
        [-10.0, -10.0, 0.0],
        [10.0, -10.0, 0.0],
        [10.0, 10.0, 0.0],
        [-10.0, 10.0, 0.0],
        [-10.0, -10.0, 20.0],
        [10.0, -10.0, 20.0],
        [10.0, 10.0, 20.0],
        [-10.0, 10.0, 20.0],
    ];
    let t: [u32; 36] = [
        0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7, 0, 1, 5, 0, 5, 4, 1, 2, 6, 1, 6, 5, 2, 3, 7, 2, 7, 6, 3, 0, 4, 3, 4, 7,
    ];
    let mesh = Arc::new(MeshArrays {
        positions: p.iter().flatten().map(|&v| v as f32).collect(),
        triangles: t.to_vec(),
    });
    BodyView {
        id: "b1".into(),
        name: "Cube".into(),
        color: None,
        hidden: false,
        solid: true,
        mesh,
        revision: 1,
        holds_up: None,
    }
}

/// The highlight's triangles as drawn (the overlay batch of draw order 3).
fn drawn(vp: &mut Viewport) -> Vec<[f32; 3]> {
    let f = vp.frame_for_size(64.0, 48.0, 1.0);
    f.batches
        .iter()
        .filter(|b| b.order == 3 && !b.lines && b.depth == Depth::TestOffset)
        .flat_map(|b| {
            f.flat[b.range.start as usize..b.range.end as usize]
                .iter()
                .map(|v| v.pos)
        })
        .collect()
}

#[test]
fn highlight_follows_moved_body() {
    let mut vp = Viewport::new();
    vp.set_grid_visible(false);
    vp.sync_bodies(&[cube()]);
    vp.flush();
    // the top face is triangles 2 and 3 (z = 20)
    vp.set_face_highlight("face", "b1", vec![2, 3], 0x2f8cff);
    let before = drawn(&mut vp);
    assert_eq!(before.len(), 6);
    assert!(before.iter().all(|p| p[2] == 20.0));
    // the gizmo moves the body 5 mm along X and up 7: the highlight goes with it, no new pick
    let mut m = [[0.0; 4]; 4];
    for (i, r) in m.iter_mut().enumerate() {
        r[i] = 1.0;
    }
    m[3][0] = 5.0;
    m[3][2] = 7.0;
    vp.set_body_transform("b1", Some(m));
    let after = drawn(&mut vp);
    assert_eq!(after.len(), 6, "still one face");
    for (a, b) in before.iter().zip(&after) {
        assert_eq!(
            [b[0] - a[0], b[1] - a[1], b[2] - a[2]],
            [5.0, 0.0, 7.0],
            "moved with the body"
        );
    }
    assert_eq!(vp.tool_highlight_corners("face").len(), 6);
    // put back: where it was
    vp.set_body_transform("b1", None);
    assert_eq!(drawn(&mut vp), before);
    // a new revision of the body (the model rebuilt): drawn from the new mesh
    let mut moved = cube();
    moved.revision = 2;
    moved.mesh = Arc::new(MeshArrays {
        positions: cube()
            .mesh
            .positions
            .chunks_exact(3)
            .flat_map(|p| [p[0], p[1], p[2] + 3.0])
            .collect(),
        triangles: cube().mesh.triangles.clone(),
    });
    vp.sync_bodies(&[moved]);
    vp.flush();
    let rebuilt = drawn(&mut vp);
    assert_eq!(rebuilt.len(), 6);
    assert!(rebuilt.iter().all(|p| p[2] == 23.0), "{rebuilt:?}");
    // cleared: gone from the picture
    vp.clear_tool_highlights();
    assert!(drawn(&mut vp).is_empty());
}

/// Living Hinge open on a body: its preview carries the slot, tinted while the setting is on.
fn hinge_preview(tint: bool) -> Harness<'static, StlApp> {
    let (mut h, link) = app_harness_ex(Some(&project("basic.stlx")), egui::vec2(1280.0, 800.0));
    h.state_mut().cut_tint_on = tint;
    select_first_body(&mut h);
    assert!(open_tool(&mut h, "hinge"));
    h.state_mut().tool_set_value("slot", serde_json::json!(1));
    // the preview comes back from the engine: its pocketed body is on screen
    wait_for(&mut h, &link, "the hinge preview", |h| {
        h.state().viewport.preview_meshes().next().is_some()
    });
    h
}

#[test]
fn cut_tint_shows_with_the_setting_on() {
    let mut h = hinge_preview(true);
    assert_eq!(
        h.state().viewport.cut_tint_ids(),
        ["tool"],
        "the slot is tinted in the preview"
    );
    h.state_mut().tool_cancel();
    h.run_steps(2);
    assert!(
        h.state().viewport.cut_tint_ids().is_empty(),
        "closing the tool takes the tint off"
    );
}

#[test]
fn cut_tint_is_off_with_the_setting_off() {
    let h = hinge_preview(false);
    assert!(h.state().viewport.cut_tint_ids().is_empty());
}
