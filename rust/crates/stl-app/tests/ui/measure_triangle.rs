//! Milestone 7 UI tests (Task 7, spec 2.5): Measure draws the straight line and the X, Y and Z
//! legs as dotted lines in their axis colours, each labelled.
#![cfg(not(target_arch = "wasm32"))]

use stl_app::viewport::Viewport;
use stl_app::viewport::overlays::{MEASURE_AXIS_COLORS, MeasureLeg};

use super::harness::{app_harness_ex, click_at, project, settle};

fn labels(vp: &Viewport) -> Vec<String> {
    vp.measure_labels().into_iter().map(|(_, t)| t).collect()
}

#[test]
fn measure_triangle_legs() {
    let mut vp = Viewport::new();
    vp.set_measure(Some(([0.0, 0.0, 0.0], [10.0, 20.0, 5.0])));
    let legs = vp.measure_legs();
    let ends: Vec<[f64; 3]> = legs.iter().map(|l| l.to).collect();
    assert_eq!(
        ends,
        [[10.0, 0.0, 0.0], [10.0, 20.0, 0.0], [10.0, 20.0, 5.0]],
        "X, then Y, then Z from the first point"
    );
    assert_eq!(legs.iter().map(|l| l.axis).collect::<Vec<_>>(), [0, 1, 2]);
    assert_eq!(legs[0].from, [0.0, 0.0, 0.0]);
    assert_eq!(legs[1].from, [10.0, 0.0, 0.0]);
    assert_eq!(legs[2].from, [10.0, 20.0, 0.0]);
    assert_eq!(
        labels(&vp),
        ["22.9 mm", "10 mm", "20 mm", "5 mm"],
        "the distance, then the legs"
    );
    // each label sits at the middle of its line
    let at: Vec<[f64; 3]> = vp.measure_labels().into_iter().map(|(p, _)| p).collect();
    assert_eq!(at[0], [5.0, 10.0, 2.5]);
    assert_eq!(at[1], [5.0, 0.0, 0.0]);
    assert_eq!(at[2], [10.0, 10.0, 0.0]);
    assert_eq!(at[3], [10.0, 20.0, 2.5]);
    // drawn: the solid line, and dotted legs in X red, Y green, Z blue
    let it = vp
        .overlay_items()
        .iter()
        .find(|o| o.tag == "measure-tri")
        .expect("the measure picture");
    assert!(it.tris.is_empty());
    let colour = |c: u32| [(c >> 16) as u8, (c >> 8) as u8, c as u8];
    for (axis, leg) in legs.iter().enumerate() {
        let want = colour(MEASURE_AXIS_COLORS[leg.axis]);
        let dashes = it.lines.iter().filter(|v| v.c0[..3] == want).count() / 2;
        assert!(dashes >= 3, "the {axis} leg is dotted: {dashes} dashes");
    }
    assert_eq!(MEASURE_AXIS_COLORS, [0xe03131, 0x2f9e44, 0x1c7ed6]);
    // gone when cleared
    vp.set_measure(None);
    assert!(vp.measure_legs().is_empty() && vp.measure_labels().is_empty());
    assert!(!vp.overlay_items().iter().any(|o| o.tag == "measure-tri"));
}

#[test]
fn measure_inches() {
    let mut vp = Viewport::new();
    vp.set_inch(true);
    vp.set_measure(Some(([0.0, 0.0, 0.0], [25.4, 0.0, 0.0])));
    assert_eq!(labels(&vp), ["1 in", "1 in"]);
}

#[test]
fn measure_one_leg() {
    let mut vp = Viewport::new();
    vp.set_measure(Some(([1.0, 2.0, 3.0], [11.0, 2.0, 3.0])));
    let legs = vp.measure_legs();
    assert_eq!(
        legs,
        [MeasureLeg {
            axis: 0,
            from: [1.0, 2.0, 3.0],
            to: [11.0, 2.0, 3.0]
        }],
        "only X differs: one leg"
    );
    assert_eq!(labels(&vp), ["10 mm", "10 mm"]);
}

/// Two clicks on a body in the app: the legs follow the picked points, and Escape takes them off.
#[test]
fn measure_in_the_app_draws_and_clears() {
    let (mut h, link) = app_harness_ex(Some(&project("basic.stlx")), egui::vec2(1280.0, 800.0));
    assert!(h.state().viewport.measure_legs().is_empty());
    h.state_mut().open_measure();
    h.run_steps(3);
    let id = h.state().bodies[0].id.clone();
    let b = h.state().viewport.body_box(&id).expect("a body box");
    let top = |fx: f64, fy: f64| {
        [
            b.min[0] + (b.max[0] - b.min[0]) * fx,
            b.min[1] + (b.max[1] - b.min[1]) * fy,
            b.max[2],
        ]
    };
    for p in [top(0.3, 0.4), top(0.7, 0.6)] {
        let at = h.state().viewport.world_to_screen(p);
        click_at(&mut h, at);
    }
    settle(&mut h, &link);
    let pts = h.state().measure_points().to_vec();
    assert_eq!(pts.len(), 2, "two picks");
    let legs = h.state().viewport.measure_legs();
    assert!(!legs.is_empty() && legs.iter().all(|l| l.axis < 3));
    let last = legs.last().unwrap();
    assert_eq!(last.to, pts[1], "the legs end at the second point");
    assert_eq!(h.state().viewport.measure_labels().len(), legs.len() + 1);
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    assert!(!h.state().measure_open());
    assert!(
        h.state().viewport.measure_legs().is_empty(),
        "closing Measure takes the legs off"
    );
}
