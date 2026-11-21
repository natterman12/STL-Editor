//! UI test `split_connectors` (2.0): the new Split connector controls, clicked through as a
//! person would. Connectors → Dovetail from the list, typed sizes, Placement → Place by hand,
//! spots clicked on the cut face (the view opens at the cut), a spot moved by typing its
//! distance from the edge, OK, and the step reopened from the timeline with its spots. Also How
//! many, and "Each piece its own body" ticked and unticked.
#![cfg(not(target_arch = "wasm32"))]

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::{Value, json};
use stl_app::app::StlApp;
use stl_app::engine_link::Command;
use stl_app::engine_link::native::NativeLink;

use super::harness::{app_harness_ex, click_at, mesh_volume, open_tool, select_first_body, settle};

type H = Harness<'static, StlApp>;

const TAN15: f64 = 0.267_949_192_431_122_7;

fn add_box(h: &mut H, link: &NativeLink, size: [f64; 3], at: [f64; 3], op: &str) {
    h.state_mut().link.send(Command::AddFeature {
        kind: "primitive".into(),
        params: json!({"shape": "box", "length": size[0], "width": size[1], "height": size[2], "plane": "XY",
                       "offset": at, "rotate": [0, 0, 0], "anchor": "corner", "operation": op, "targets": ["f1"]}),
        index: None,
        name: Some("Block".into()),
    });
    settle(h, link);
}

/// Open Split on the first body, cutting across z = 10, seen from the top.
fn split_across_z10(h: &mut H, link: &NativeLink) {
    h.state_mut().viewport.set_view("top");
    h.state_mut().viewport.flush();
    h.run_steps(3);
    select_first_body(h);
    assert!(open_tool(h, "split"));
    h.state_mut().tool_set_value(
        "plane",
        json!({"origin": [0, 0, 10], "normal": [0, 0, 1], "xdir": [1, 0, 0], "label": "Plane"}),
    );
    h.run_steps(3);
    settle(h, link);
}

fn values(h: &H) -> serde_json::Map<String, Value> {
    h.state().tool().expect("Split is open").values.clone()
}

/// The control of `role` on the same row as the field label `label`, right of it.
fn control_by(h: &H, label: &str, role: Role) -> egui::Rect {
    let l = h.get_by_label(label).rect();
    let mut best: Option<egui::Rect> = None;
    for n in h.query_all_by_role(role) {
        let r = n.rect();
        if r.min.x > l.min.x && (r.center().y - l.center().y).abs() < 14.0 {
            best = Some(best.map_or(r, |b| if r.min.x < b.min.x { r } else { b }));
        }
    }
    best.unwrap_or_else(|| panic!("no {role:?} next to {label:?}"))
}

fn click_rect(h: &mut H, r: egui::Rect) {
    click_at(h, r.center());
    h.run_steps(3);
}

/// Pick `option` from the drop-down list next to `label`.
fn choose(h: &mut H, label: &str, option: &str) {
    let r = control_by(h, label, Role::ComboBox);
    click_rect(h, r);
    h.get_by_label(option).click();
    h.run_steps(3);
}

/// Click into the text box at `r`, select all, type `text` and press Enter.
fn type_in(h: &mut H, r: egui::Rect, text: &str) {
    h.query_all_by_role(Role::TextInput)
        .find(|n| n.rect() == r)
        .expect("the box")
        .focus();
    h.run_steps(2);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.run_steps(1);
    h.event(egui::Event::Text(text.to_owned()));
    h.run_steps(1);
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
}

fn type_field(h: &mut H, label: &str, text: &str) {
    let r = control_by(h, label, Role::TextInput);
    type_in(h, r, text);
}

fn press_ok(h: &mut H, link: &NativeLink) {
    h.get_by_label("OK").click();
    h.run_steps(2);
    settle(h, link);
}

fn body_volumes(h: &H) -> Vec<f64> {
    let mut v: Vec<f64> = h.state().bodies.iter().map(|b| mesh_volume(&b.mesh)).collect();
    v.sort_by(f64::total_cmp);
    v
}

fn split_error(h: &H) -> Option<String> {
    h.state()
        .timeline_view
        .items
        .iter()
        .rev()
        .find(|i| i.kind == "split")
        .expect("the Split step")
        .error
        .clone()
}

#[test]
fn split_dovetail_placed_by_hand() {
    let (mut h, link) = app_harness_ex(None, egui::vec2(1280.0, 800.0));
    add_box(&mut h, &link, [40.0, 30.0, 20.0], [0.0, 0.0, 0.0], "new");
    split_across_z10(&mut h, &link);

    // Connectors: Dovetail. Its own sizes show; the pin ones go.
    choose(&mut h, "Connectors", "Dovetail (slide together)");
    assert_eq!(values(&h)["connector"], json!("dovetail"));
    assert!(h.query_by_label("Dovetail width").is_some());
    assert!(h.query_by_label("Pin length").is_none());
    type_field(&mut h, "Dovetail width", "6");
    type_field(&mut h, "Dovetail depth", "3");
    assert_eq!(values(&h)["dovetail_width"].as_f64(), Some(6.0));
    assert_eq!(values(&h)["dovetail_depth"].as_f64(), Some(3.0));

    // Placement: Place by hand. The spots box shows and the view opens at the cut.
    assert!(h.query_by_label("How many").is_some(), "Auto has How many");
    h.get_by_label("Place by hand").click();
    h.run_steps(3);
    assert_eq!(values(&h)["connector_place"], json!("manual"));
    assert!(h.query_by_label("How many").is_none());
    assert!(h.state().viewport.active_section().is_some(), "the view is cut open");
    if h.state().tool().unwrap().active_field.as_deref() != Some("connector_points") {
        let r = h
            .query_all_by_label("Click on the cut face")
            .find(|n| n.accesskit_node().role() == Role::Button)
            .expect("the spots box")
            .rect();
        click_at(&mut h, r.center());
        h.run_steps(2);
    }
    assert_eq!(
        h.state().tool().unwrap().active_field.as_deref(),
        Some("connector_points")
    );

    // two clicks on the cut face, across the long side
    for p in [[20.0, 8.0, 10.0], [20.0, 22.0, 10.0]] {
        let at = h.state().viewport.world_to_screen(p);
        click_at(&mut h, at);
        h.run_steps(3);
    }
    let pts = values(&h)["connector_points"].clone();
    assert_eq!(pts.as_array().map(Vec::len), Some(2), "{pts}");
    for (i, want) in [[20.0, 8.0], [20.0, 22.0]].iter().enumerate() {
        let got = [
            pts[i][0].as_f64().unwrap(),
            pts[i][1].as_f64().unwrap(),
            pts[i][2].as_f64().unwrap(),
        ];
        assert!(
            (got[0] - want[0]).abs() < 0.3 && (got[1] - want[1]).abs() < 0.3 && (got[2] - 10.0).abs() < 1e-6,
            "spot {i} on the cut face: {got:?}"
        );
    }
    assert!(h.query_by_label("2 spots").is_some());

    // move each spot by typing its distance from the left and bottom edges of the cut face
    // (the two boxes on the line under "Spot k")
    let rows = |h: &H, spot: &str| -> Vec<egui::Rect> {
        let l = h.get_by_label(spot).rect();
        let mut v: Vec<egui::Rect> = h
            .query_all_by_role(Role::TextInput)
            .map(|n| n.rect())
            .filter(|r| r.min.y > l.max.y - 2.0 && r.min.y < l.max.y + 24.0 && r.max.x > l.min.x)
            .collect();
        v.sort_by(|a, b| a.min.x.total_cmp(&b.min.x));
        v
    };
    for (spot, left, bottom) in [("Spot 1", "16", "8"), ("Spot 2", "24", "22")] {
        let r = rows(&h, spot);
        assert_eq!(r.len(), 2, "{spot}: Left and Bottom");
        type_in(&mut h, r[0], left);
        let r = rows(&h, spot);
        type_in(&mut h, r[1], bottom);
    }
    let pts = values(&h)["connector_points"].clone();
    assert_eq!(pts, json!([[16.0, 8.0, 10.0], [24.0, 22.0, 10.0]]), "typed positions");

    press_ok(&mut h, &link);
    assert_eq!(split_error(&h), None);
    assert!(
        h.state().viewport.active_section().is_none(),
        "the cut view goes with the tool"
    );
    // two dovetails, 6 wide and 3 deep, each running the 40 mm of the long side
    let tongue = (6.0 * 3.0 + TAN15 * 9.0) * 40.0;
    let groove = (6.3 * 3.15 + TAN15 * 3.15 * 3.15) * 40.0;
    let v = body_volumes(&h);
    assert_eq!(v.len(), 2, "{v:?}");
    let (below, above) = (v[0], v[1]);
    assert!((above - (12000.0 + 2.0 * tongue)).abs() < 0.05, "above {above}");
    assert!((below - (12000.0 - 2.0 * groove)).abs() < 0.05, "below {below}");

    // reopened from the timeline: the same choices and spots
    let sid = h
        .state()
        .timeline_view
        .items
        .iter()
        .find(|i| i.kind == "split")
        .unwrap()
        .id
        .clone();
    assert!(h.state_mut().edit_feature(&sid));
    settle(&mut h, &link);
    h.run_steps(3);
    let v = values(&h);
    assert_eq!(v["connector"], json!("dovetail"));
    assert_eq!(v["connector_place"], json!("manual"));
    assert_eq!(v["connector_points"].as_array().map(Vec::len), Some(2));
    assert!(h.query_by_label("Spot 2").is_some());
}

#[test]
fn split_pins_how_many_and_centre() {
    let (mut h, link) = app_harness_ex(None, egui::vec2(1280.0, 800.0));
    add_box(&mut h, &link, [40.0, 30.0, 20.0], [0.0, 0.0, 0.0], "new");
    split_across_z10(&mut h, &link);
    choose(&mut h, "Connectors", "Printed pins");
    type_field(&mut h, "How many", "3");
    // a plain number box keeps the typed text; the engine gets the number
    assert_eq!(values(&h)["connector_count"], json!("3"));
    press_ok(&mut h, &link);
    assert_eq!(split_error(&h), None);
    // three 5 mm pin holes in the half below: 48-sided, radius 2.65, 5.39 deep
    let hole = 24.0 * 2.65_f64.powi(2) * (std::f64::consts::PI / 24.0).sin() * 5.39;
    let v = body_volumes(&h);
    assert!((v[0] - (12000.0 - 3.0 * hole)).abs() < 0.05, "{v:?}");

    // Centre: one magnet pocket in the middle of the face
    h.state_mut().link.send(Command::Undo);
    settle(&mut h, &link);
    split_across_z10(&mut h, &link);
    choose(&mut h, "Connectors", "Magnets 6×3");
    h.get_by_label("Centre").click();
    h.run_steps(3);
    assert_eq!(values(&h)["connector_place"], json!("centre"));
    press_ok(&mut h, &link);
    assert_eq!(split_error(&h), None);
    // a 48-sided pocket of radius 3.15, 3.25 deep, in each half (3.24 of it below the cut)
    let pocket = 24.0 * 3.15_f64.powi(2) * (std::f64::consts::PI / 24.0).sin();
    let v = body_volumes(&h);
    assert!((v[0] - (12000.0 - pocket * 3.24)).abs() < 0.05, "{v:?}");
}

#[test]
fn split_each_piece_its_own_body() {
    // a U: a 30 × 10 × 5 base and two 10 × 10 × 20 legs; the cut goes through both legs
    let (mut h, link) = app_harness_ex(None, egui::vec2(1280.0, 800.0));
    add_box(&mut h, &link, [30.0, 10.0, 5.0], [0.0, 0.0, 0.0], "new");
    add_box(&mut h, &link, [10.0, 10.0, 20.0], [0.0, 0.0, 0.0], "join");
    add_box(&mut h, &link, [10.0, 10.0, 20.0], [20.0, 0.0, 0.0], "join");
    assert_eq!(h.state().bodies.len(), 1);

    split_across_z10(&mut h, &link);
    assert_eq!(values(&h)["separate"], json!(true), "on for a new Split");
    press_ok(&mut h, &link);
    let v = body_volumes(&h);
    assert_eq!(v.len(), 3, "two legs and the base: {v:?}");
    assert!((v[0] - 1000.0).abs() < 0.01 && (v[1] - 1000.0).abs() < 0.01 && (v[2] - 2500.0).abs() < 0.01);

    // untick it: one body each side of the cut
    h.state_mut().link.send(Command::Undo);
    settle(&mut h, &link);
    split_across_z10(&mut h, &link);
    let r = control_by(&h, "Each piece its own body", Role::CheckBox);
    click_rect(&mut h, r);
    assert_eq!(values(&h)["separate"], json!(false));
    press_ok(&mut h, &link);
    assert_eq!(body_volumes(&h).len(), 2);
}

#[test]
fn fit_to_bed_with_dovetails() {
    // a 300 mm bar on the default printer's 256 mm bed: one seam, with a dovetail across it
    let (mut h, link) = app_harness_ex(None, egui::vec2(1280.0, 800.0));
    add_box(&mut h, &link, [300.0, 30.0, 20.0], [0.0, 0.0, 0.0], "new");
    select_first_body(&mut h);
    assert!(open_tool(&mut h, "bedsplit"));
    choose(&mut h, "Connectors", "Dovetail (slide together)");
    assert!(h.query_by_label("Dovetail width").is_some() && h.query_by_label("Pin length").is_none());
    assert!(
        h.query_by_label("Placement").is_none(),
        "Fit to Bed makes several cuts: no spots by hand"
    );
    press_ok(&mut h, &link);
    let item = h
        .state()
        .timeline_view
        .items
        .iter()
        .find(|i| i.kind == "bedsplit")
        .unwrap();
    assert_eq!(item.error, None);
    // 10 wide, 5 deep (the defaults), 0.15 clearance, along the cut face's 30 mm side
    let tongue = (10.0 * 5.0 + TAN15 * 25.0) * 30.0;
    let groove = (10.3 * 5.15 + TAN15 * 5.15 * 5.15) * 30.0;
    let v = body_volumes(&h);
    assert_eq!(v.len(), 2, "{v:?}");
    let total: f64 = v.iter().sum();
    assert!((total - (180_000.0 + tongue - groove)).abs() < 0.5, "{total}");
}
