//! UI test `measure_info` (milestone 6): the ribbon's Measure and Properties open their panels,
//! two picks on the basic project's block give the distance between them, and Properties shows the
//! block's volume in cm³ (in³ in inch mode).

use egui::Pos2;
use egui_kittest::kittest::Queryable;
use stl_app::viewport::pick::Hit;

use super::harness::{app_harness_ex, click_at, mesh_volume, project, settle, wait_for};

type H = egui_kittest::Harness<'static, stl_app::app::StlApp>;

fn open() -> (H, std::sync::Arc<stl_app::engine_link::native::NativeLink>) {
    let (mut h, link) = app_harness_ex(Some(project("basic.stlx").as_path()), egui::vec2(1280.0, 800.0));
    settle(&mut h, &link);
    h.state_mut().viewport.set_view("iso");
    h.state_mut().viewport.flush();
    h.run_steps(3);
    (h, link)
}

/// `holed_block`, 30 × 20 × 10 mm with its corner on the origin: the spots below are on its top face.
const BLOCK: &str = "f3";

/// The screen position of a point on the block, checked to pick that block.
fn on_block(h: &H, p: [f64; 3]) -> Pos2 {
    let at = h.state().viewport.world_to_screen(p);
    let hit = h.state().viewport.pick(at, true, false);
    assert!(
        matches!(&hit, Some(Hit::Body { id, .. }) if id == BLOCK),
        "the spot {p:?} is on the block: {hit:?}"
    );
    at
}

fn number(s: &str) -> f64 {
    s.split_whitespace().next().unwrap().parse().unwrap()
}

fn row(rows: &[(String, String)], label: &str) -> String {
    rows.iter()
        .find(|(l, _)| l == label)
        .map(|(_, v)| v.clone())
        .unwrap_or_else(|| panic!("no {label} row in {rows:?}"))
}

/// The ribbon's Measure button opens the panel and asks for two points; Done closes it.
#[test]
fn ribbon_measure_opens_its_panel_and_done_closes_it() {
    let (mut h, _link) = open();
    assert!(!h.state().measure_open());
    h.get_by_label("Measure").click();
    h.run_steps(3);
    assert!(h.state().measure_open(), "the ribbon's Measure opens Measure");
    assert!(h.query_by_label("Click two points on the model.").is_some());
    h.get_by_label("Done").click();
    h.run_steps(2);
    assert!(!h.state().measure_open(), "Done closes Measure");
    assert!(h.query_by_label("Click two points on the model.").is_none());
}

/// The ribbon's Properties button opens the panel; with no body it says what to do.
#[test]
fn ribbon_properties_opens_its_panel() {
    let (mut h, _link) = open();
    h.get_by_label("Properties").click();
    h.run_steps(3);
    assert!(h.state().info_open(), "the ribbon's Properties opens Properties");
    assert!(
        h.query_by_label("Click a body to see its size, volume and triangle count.")
            .is_some()
    );
    h.get_by_label("Close").click();
    h.run_steps(2);
    assert!(!h.state().info_open());
}

/// Two picks on the block give Distance and the three differences; a third pick starts again.
#[test]
fn two_picks_give_the_distance_and_a_third_starts_again() {
    let (mut h, _link) = open();
    h.get_by_label("Measure").click();
    h.run_steps(3);

    // 24 mm apart along X, on the top face (z = 10)
    let a = on_block(&h, [3.0, 17.0, 10.0]);
    let b = on_block(&h, [27.0, 17.0, 10.0]);
    click_at(&mut h, a);
    assert_eq!(h.state().measure_points().len(), 1);
    let rows = h.state().measure_readout();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, "Point 1");
    assert!(rows[0].1.ends_with(" mm"), "{rows:?}");

    click_at(&mut h, b);
    assert_eq!(h.state().measure_points().len(), 2);
    let rows = h.state().measure_readout();
    let near = |label: &str, want: f64| {
        let got = number(&row(&rows, label));
        assert!((got - want).abs() < 0.05, "{label}: {got} should be {want} ({rows:?})");
    };
    near("Distance", 24.0);
    near("ΔX", 24.0);
    near("ΔY", 0.0);
    near("ΔZ", 0.0);
    assert!(row(&rows, "Distance").ends_with(" mm"));
    // what is drawn is the same text
    assert!(h.query_by_label("Distance").is_some() && h.query_by_label("ΔX").is_some());

    // the line is the distance between the points really picked
    let pts = h.state().measure_points().to_vec();
    let d =
        ((pts[1][0] - pts[0][0]).powi(2) + (pts[1][1] - pts[0][1]).powi(2) + (pts[1][2] - pts[0][2]).powi(2)).sqrt();
    assert!((number(&row(&rows, "Distance")) - d).abs() < 0.001);

    // a third click starts again with Point 1
    let c = on_block(&h, [15.0, 5.0, 10.0]);
    click_at(&mut h, c);
    assert_eq!(h.state().measure_points().len(), 1, "a third pick starts again");
    assert_eq!(h.state().measure_readout()[0].0, "Point 1");

    // inch mode shows the same distance in inches
    click_at(&mut h, b);
    h.state_mut().set_units_inch(true);
    h.run_steps(2);
    let rows = h.state().measure_readout();
    assert!(row(&rows, "Distance").ends_with(" in"), "{rows:?}");
    let inches = number(&row(&rows, "Distance"));
    let mm = {
        let p = h.state().measure_points();
        ((p[1][0] - p[0][0]).powi(2) + (p[1][1] - p[0][1]).powi(2) + (p[1][2] - p[0][2]).powi(2)).sqrt()
    };
    assert!((inches * 25.4 - mm).abs() < 0.01, "{inches} in is {mm} mm");

    h.get_by_label("Done").click();
    h.run_steps(2);
    assert!(!h.state().measure_open());
    assert!(h.state().measure_points().is_empty());
}

/// Properties of the block: its volume in cm³ (in³ in inch mode), the weights, the holes.
#[test]
fn properties_shows_the_blocks_volume_in_cm3_and_in3() {
    let (mut h, link) = open();
    let vol_mm3 = mesh_volume(&h.state().bodies.iter().find(|b| b.id == BLOCK).unwrap().mesh);
    h.get_by_label("Properties").click();
    h.run_steps(3);
    // clicking a body while Properties is open shows that body
    let at = on_block(&h, [3.0, 17.0, 10.0]);
    click_at(&mut h, at);
    wait_for(&mut h, &link, "the block's properties", |h| {
        h.state().info_readout().is_some()
    });
    let (name, rows) = h.state().info_readout().unwrap();
    assert_eq!(name, "holed_block");
    assert_eq!(row(&rows, "Watertight"), "✔ Yes");
    assert_eq!(row(&rows, "Size (X×Y×Z)"), "30 × 20 × 10 mm");
    let cm3 = vol_mm3 / 1000.0;
    assert!(row(&rows, "Volume").ends_with(" cm³"), "{rows:?}");
    assert!(
        (number(&row(&rows, "Volume")) - cm3).abs() < 0.01,
        "{rows:?}: expected {cm3} cm³"
    );
    assert!((number(&row(&rows, "Weight (PLA)")) - cm3 * 1.24).abs() < 0.02);
    assert!((number(&row(&rows, "Weight (PETG)")) - cm3 * 1.27).abs() < 0.02);
    assert!(row(&rows, "Surface area").ends_with(" cm²"));
    assert!(rows.iter().any(|(l, _)| l == "Through-holes"));
    assert!(h.query_by_label("Volume").is_some(), "the panel draws the rows");
    assert!(
        h.state().shell.selection == [BLOCK.to_owned()],
        "the click selected the block"
    );

    h.state_mut().set_units_inch(true);
    h.run_steps(2);
    let (_, rows) = h.state().info_readout().unwrap();
    assert!(row(&rows, "Volume").ends_with(" in³"), "{rows:?}");
    assert!(
        (number(&row(&rows, "Volume")) - vol_mm3 / 16387.064).abs() < 0.001,
        "{rows:?}"
    );
    assert!(row(&rows, "Surface area").ends_with(" in²"));
    assert!(row(&rows, "Size (X×Y×Z)").ends_with(" in"));
}
