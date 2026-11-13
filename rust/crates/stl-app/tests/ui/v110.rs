//! SUPPORTS tab, Export and Send to Slicer menus, Support Fins edited not stacked.
//! Counterpart of `tests/ui/v110.js`.
#![cfg(not(target_arch = "wasm32"))]

use std::time::Instant;

use super::harness::{wait_for, empty_harness, fixture_stl, import_stl, select_first_body, settle, wait_support_open};
use egui_kittest::kittest::Queryable;
use serde_json::json;
use stl_app::ui::ribbon;

#[test]
fn v110_supports_tab_lists_tools() {
    let (mut h, link) = empty_harness();
    import_stl(&mut h, &link, &fixture_stl("tipped.stl"));
    h.get_by_label("Supports").click();
    h.run_steps(3);
    let def = ribbon::def("supports");
    let tools: Vec<_> = def.groups.iter().flat_map(|g| g.tools().into_iter().map(|t| t.key)).collect();
    assert!(tools.iter().any(|k| *k == "supportfins"), "Support Fins on SUPPORTS: {tools:?}");
    assert!(tools.iter().any(|k| *k == "supportwall"), "Draw Walls on SUPPORTS: {tools:?}");
    assert!(h.query_by_label("Support Fins").is_some() || h.query_by_label("More tools").is_some());
}

#[test]
fn v110_support_fins_edit_not_stack() {
    let (mut h, link) = empty_harness();
    import_stl(&mut h, &link, &fixture_stl("tipped.stl"));
    select_first_body(&mut h);
    wait_support_open(&mut h, &link, "supportfins");

    if h.state().tool().unwrap().values.contains_key("sf:tineDensity") {
        h.state_mut().tool_set_value("sf:tineDensity", json!(100.0));
        h.run_steps(2);
    }
    h.state_mut().tool_ok();
    settle(&mut h, &link);
    let sf_steps: Vec<_> = h.state().timeline_view.items.iter().filter(|i| i.kind == "supportfins").cloned().collect();
    assert_eq!(sf_steps.len(), 1, "one Support Fins step after OK: {:?}", h.state().timeline_view.items.iter().map(|i| &i.kind).collect::<Vec<_>>());

    // Open again — edits the same step
    wait_support_open(&mut h, &link, "supportfins");
    let t = h.state().tool().expect("reopen");
    assert_eq!(t.edit_id.as_deref(), Some(sf_steps[0].id.as_str()), "edits existing step, not a second set");
    assert!(t.title().contains("Edit") || t.edit_id.is_some(), "title={}", t.title());

    if t.values.contains_key("sf:tineDensity") {
        h.state_mut().tool_set_value("sf:tineDensity", json!(50.0));
        h.run_steps(2);
    }
    h.state_mut().tool_ok();
    settle(&mut h, &link);
    let sf_after: Vec<_> = h.state().timeline_view.items.iter().filter(|i| i.kind == "supportfins").collect();
    assert_eq!(sf_after.len(), 1, "still one Support Fins step after edit");
}

#[test]
fn v110_draw_walls_on_same_body_opens_add_mode() {
    let (mut h, link) = empty_harness();
    import_stl(&mut h, &link, &fixture_stl("tipped.stl"));
    select_first_body(&mut h);
    wait_support_open(&mut h, &link, "supportfins");
    h.state_mut().tool_ok();
    settle(&mut h, &link);

    // After fins exist, openTool(supportwall) may reopen Support Fins in Add fins (1.x reopenSupports).
    let _ = h.state_mut().open_tool("supportwall", None);
    let end = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while Instant::now() < end {
        settle(&mut h, &link);
        h.run_steps(2);
        if h.state().tool().is_some_and(|t| matches!(t.def.key, "supportwall" | "supportfins")) {
            break;
        }
        let _ = h.state_mut().open_tool("supportwall", None);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let t = h.state().tool().expect("Draw Walls / Support Fins reopen");
    let mode = t.values.get("_mode").and_then(|v| v.as_str()).unwrap_or("");
    assert!(
        t.def.key == "supportfins" || t.def.key == "supportwall",
        "opened supports tool: {}",
        t.def.key
    );
    if t.def.key == "supportfins" {
        assert_eq!(mode, "add", "Add fins mode when reopening via Draw Walls");
    }
    h.state_mut().tool_cancel();
    settle(&mut h, &link);
}

#[test]
fn v110_export_menu_still_present() {
    // 4a covers Export / Send to Slicer menus; keep a smoke check with a project open.
    let (mut h, link) = empty_harness();
    import_stl(&mut h, &link, &fixture_stl("blk.stl"));
    h.get_by_label("Print").click();
    h.run_steps(3);
    let export = h.query_all_by_label_contains("Export").count();
    let send = h.query_all_by_label_contains("Send to Slicer").count();
    let more = h.query_all_by_label_contains("More tools").count();
    assert!(export + send + more > 0, "Print tab shows Export / Send / More");
    settle(&mut h, &link);
}

/// Turning the part after it has fins puts the turn before the fins, so they are rebuilt for the
/// new position instead of tipping with the part.
#[test]
fn v110_rotate_after_fins_rebuilds() {
    use egui::{Event, PointerButton, Pos2};
    use stl_app::viewport::camera;
    use stl_app::viewport::gizmo_draw::{handle_unit, Handle};

    let (mut h, link) = empty_harness();
    import_stl(&mut h, &link, &fixture_stl("tipped.stl"));
    select_first_body(&mut h);
    wait_support_open(&mut h, &link, "supportfins");
    wait_for(&mut h, &link, "the fins preview", |h| h.state().viewport.preview_meshes().next().is_some());
    h.state_mut().tool_ok();
    wait_for(&mut h, &link, "the fins step", |h| h.state().tool().is_none() && h.state().timeline_view.items.iter().any(|i| i.kind == "supportfins"));
    let steps_before = h.state().timeline_view.items.len();

    select_first_body(&mut h);
    h.state_mut().viewport.set_view("front");
    h.state_mut().on_ribbon("rotate", Pos2::ZERO);
    h.run_steps(4);
    let vp = &h.state().viewport;
    let c = vp.gizmo().expect("rings").position;
    let u = handle_unit(vp.camera(), c);
    let grab = vp.world_to_screen(camera::add(c, [u * 0.3536, 0.0, u * 0.3536]));
    let ptr = |h: &mut egui_kittest::Harness<'_, stl_app::app::StlApp>, at: Pos2, pressed: Option<bool>| {
        h.event(Event::PointerMoved(at));
        if let Some(p) = pressed {
            h.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: p, modifiers: Default::default() });
        }
        h.step();
    };
    ptr(&mut h, grab, None);
    assert_eq!(h.state().viewport.gizmo().and_then(|g| g.axis), Some(Handle::Y), "on the Y ring");
    ptr(&mut h, grab, Some(true));
    for i in 1..=10 {
        ptr(&mut h, Pos2::new(grab.x + i as f32 * 6.0, grab.y + i as f32 * 9.0), None);
    }
    ptr(&mut h, Pos2::new(grab.x + 60.0, grab.y + 90.0), Some(false));
    h.run_steps(3);
    h.state_mut().tool_ok();
    wait_for(&mut h, &link, "the turn", |h| h.state().tool().is_none() && h.state().timeline_view.items.len() > steps_before);

    let items = &h.state().timeline_view.items;
    let turn = items.iter().position(|i| i.kind == "move").expect("the turn is a Move step");
    let fins = items.iter().position(|i| i.kind == "supportfins").expect("the fins step is still there");
    assert!(turn < fins, "the turn goes before the fins so they are rebuilt: {:?}", items.iter().map(|i| &i.kind).collect::<Vec<_>>());
    assert_eq!(items[fins].error, None, "the fins were rebuilt for the new position");
    let min_z = h.state().bodies[0].mesh.positions.iter().skip(2).step_by(3).fold(f32::INFINITY, |a, &z| a.min(z));
    assert!(min_z.abs() < 0.1, "the turned part still sits on the bed: {min_z}");
}
