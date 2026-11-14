//! Tablet and phone layout, browser drawer, one-finger orbit; open a tool for panel layout.
//! Counterpart of `tests/ui/touch.js`.
#![cfg(not(target_arch = "wasm32"))]

use super::harness::{app_harness_ex, fixture_stl, import_stl, select_first_body, settle};
use egui::{Event, PointerButton, Pos2};
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::json;

fn sizes() -> [(&'static str, egui::Vec2); 3] {
    [
        ("ipad", egui::vec2(834.0, 1194.0)),
        ("ipad-land", egui::vec2(1194.0, 834.0)),
        ("phone", egui::vec2(390.0, 844.0)),
    ]
}

#[test]
fn touch_layouts_open_hollow_and_orbit() {
    for (name, size) in sizes() {
        let (mut h, link) = app_harness_ex(None, size);
        import_stl(&mut h, &link, &fixture_stl("blk.stl"));
        select_first_body(&mut h);

        let body = h.state().bodies[0].id.clone();
        let mut vals = serde_json::Map::new();
        vals.insert("body".into(), json!([body]));
        vals.insert("thickness".into(), json!(2.0));
        assert!(h.state_mut().open_tool("hollow", Some(vals)));
        h.run_steps(4);
        assert_eq!(h.state().tool().map(|t| t.def.key), Some("hollow"));
        assert!(
            h.query_by_label("OK").is_some() || h.query_by_label("Cancel").is_some(),
            "{name}: hollow panel buttons at {size:?}"
        );

        // Close tool so orbit is not stolen by the panel
        h.state_mut().tool_cancel();
        h.run_steps(2);

        let cam0 = h.state().viewport.camera().position;
        // One-finger (primary) drag on the viewport — Model mode orbits (viewport::drag_action).
        let mid = Pos2::new(size.x * 0.5, size.y * 0.55);
        h.event(Event::PointerMoved(mid));
        h.event(Event::PointerButton { pos: mid, button: PointerButton::Primary, pressed: true, modifiers: Default::default() });
        h.step();
        for i in 1..=8 {
            let p = Pos2::new(mid.x + i as f32 * 12.0, mid.y);
            h.event(Event::PointerMoved(p));
            h.step();
        }
        h.event(Event::PointerButton {
            pos: Pos2::new(mid.x + 96.0, mid.y),
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        });
        h.run_steps(3);
        let cam1 = h.state().viewport.camera().position;
        let moved = (0..3).any(|i| (cam0[i] - cam1[i]).abs() > 1e-3);
        eprintln!("touch {name}: one-finger orbit moved={moved}");
        // Phone chrome can steal the drag; tablet widths must orbit.
        if name != "phone" {
            assert!(moved, "{name}: one-finger drag should orbit the camera");
        }
        settle(&mut h, &link);
    }
}

#[test]
fn touch_narrow_ribbon_still_lists_tools() {
    let (mut h, _link) = app_harness_ex(Some(super::harness::project("basic.stlx").as_path()), egui::vec2(390.0, 844.0));
    // Model tab tools or More fold — either is fine on a phone width.
    h.run_steps(3);
    let has_extrude = h.query_by_label("Extrude").is_some();
    let has_more = h.query_by_label("More tools").is_some();
    assert!(has_extrude || has_more, "narrow window shows Extrude or More tools");
}

#[test]
fn touch_browser_drawer_toggle() {
    // both phone orientations get the drawer; the tablets keep the floating browser
    let all = sizes().into_iter().chain([("phone-land", egui::vec2(844.0, 390.0))]);
    for (name, size) in all {
        let (mut h, link) = app_harness_ex(None, size);
        import_stl(&mut h, &link, &fixture_stl("blk.stl"));
        select_first_body(&mut h);
        h.run_steps(3);
        if !stl_app::ui::browser::is_narrow(size) {
            assert!(button(&h, "Bodies").is_none(), "{name}: tablets have no drawer button");
            assert!(browser_shown(&h), "{name}: the browser floats over the model");
            continue;
        }
        // closed to start: the model is clear and only the button shows
        assert!(!h.state().browser.drawer_open);
        assert!(button(&h, "Bodies").is_some(), "{name}: the Bodies button is over the model");
        assert!(!browser_shown(&h), "{name}: the drawer starts closed");

        // open a tool as touch.js does, then tap the button
        let body = h.state().bodies[0].id.clone();
        let mut vals = serde_json::Map::new();
        vals.insert("body".into(), json!([body]));
        vals.insert("thickness".into(), json!(2.0));
        assert!(h.state_mut().open_tool("hollow", Some(vals)));
        h.run_steps(4);
        button(&h, "Bodies").expect("Bodies button").click();
        steps_until(&mut h, "the drawer never opened", browser_shown);
        assert!(h.state().browser.drawer_open);
        assert!(button(&h, "Close").is_some(), "the button now says Close");
        assert!(button(&h, "Bodies").is_none());

        button(&h, "Close").expect("Close button").click();
        steps_until(&mut h, "the drawer never closed", |h| !browser_shown(h));
        assert!(!h.state().browser.drawer_open);
        assert!(button(&h, "Bodies").is_some(), "the button says Bodies again");
        settle(&mut h, &link);
    }
}

/// 1.x's breakpoint is `(max-width: 760px), (max-height: 520px)`: a short landscape phone is narrow too.
#[test]
fn touch_drawer_breakpoint() {
    use stl_app::ui::browser::{drawer_outer_width, is_narrow};
    assert!(is_narrow(egui::vec2(390.0, 844.0)));
    assert!(is_narrow(egui::vec2(760.0, 900.0)), "760 itself is narrow");
    assert!(!is_narrow(egui::vec2(761.0, 900.0)));
    assert!(is_narrow(egui::vec2(844.0, 390.0)), "a landscape phone is short");
    assert!(is_narrow(egui::vec2(1200.0, 520.0)));
    assert!(!is_narrow(egui::vec2(834.0, 1194.0)), "an iPad portrait");
    assert!(!is_narrow(egui::vec2(1194.0, 834.0)), "an iPad landscape");
    assert!(!is_narrow(egui::vec2(1280.0, 800.0)));
    // the drawer is `min(280px, 85vw)` wide
    assert_eq!(drawer_outer_width(390.0), 280.0);
    assert!((drawer_outer_width(300.0) - 255.0).abs() < 1e-3);
    // CSS `ease` runs from 0 to 1, fast in the middle and slow at the end
    let ease = stl_app::ui::browser::css_ease;
    assert!(ease(0.0).abs() < 1e-4 && (ease(1.0) - 1.0).abs() < 1e-4);
    assert!((ease(0.5) - 0.8024).abs() < 2e-3, "ease(0.5) = {}", ease(0.5));
    assert!(ease(0.9) > 0.95 && ease(0.2) < ease(0.4) && ease(0.4) < ease(0.6), "it rises and settles");

    // a short, wide window gets the drawer in the app too
    let (mut h, _link) = app_harness_ex(None, egui::vec2(900.0, 500.0));
    h.run_steps(3);
    assert!(button(&h, "Bodies").is_some(), "900x500 is short: the button shows");
}

type Harness = egui_kittest::Harness<'static, stl_app::app::StlApp>;

/// The drawer button by its label. Other buttons can say "Close" too (the tool panel's), so match
/// the role and the upper part of the window (the tool panel is a sheet at the bottom).
fn button<'a>(h: &'a Harness, label: &'a str) -> Option<egui_kittest::Node<'a>> {
    h.query_all_by_label(label)
        .find(|n| n.accesskit_node().role() == egui::accesskit::Role::Button && n.rect().min.y < 300.0)
}

/// The browser panel is on screen: its "Browser" title is painted.
fn browser_shown(h: &Harness) -> bool {
    h.query_all_by_label("Browser").next().is_some()
}

/// Step until `done` holds (the drawer slides for 0.18 s).
fn steps_until(h: &mut Harness, what: &str, done: impl Fn(&Harness) -> bool) {
    for _ in 0..40 {
        h.run_steps(1);
        if done(h) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("{what}");
}
