//! The website and README screenshots of the 2.0 app (`docs/img/*.png`, dark theme, 1600×900
//! points at `SITE_SCALE` pixels per point, 2 unless set),
//! the 2.0 counterpart of 1.x's `dev/capture_site_screenshots.mjs`: the same six pictures of the
//! same mounting bracket and test meshes, drawn by the real app on the GPU.
//!
//! Ignored by default: it needs a GPU (lavapipe works) and writes into `docs/img`. Run with
//! `cargo test -p stl-app --profile test-fast --test site_shots -- --ignored`. To take only some,
//! list them in `SITE_SHOTS` (`hero`, `tool_search`, `holes_threads`, `sketch`, `fit_to_bed`,
//! `support_fins`, comma-separated).
#![cfg(not(target_arch = "wasm32"))]

use serde_json::json;
use stl_app::engine_link::native::NativeLink;
use stl_app::sketch::Entity;
use stl_app::viewport::renderer::Headless;

#[path = "support/wgpu_renderer.rs"]
mod wgpu_renderer;

#[path = "ui/harness.rs"]
mod harness;

#[path = "support/site.rs"]
mod site;

use harness::{click_at, fixture_stl, import_stl, settle, sketch_click_uv};
use site::{H, build_bracket, crop_to_window, img_dir, iso, wait_meshes};

const SIZE: egui::Vec2 = egui::vec2(1600.0, 900.0);

fn shot(h: &mut H, name: &str) {
    // Lavapipe draws the ground grid as large grey blocks; on a real graphics card set
    // SITE_SHOTS_GRID=1 to keep it, as the app shows it.
    if std::env::var("SITE_SHOTS_GRID").is_err() {
        h.state_mut().viewport.set_grid_visible(false);
    }
    wait_meshes(h);
    // the pointer off to the right, where it hovers nothing
    h.event(egui::Event::PointerMoved(egui::pos2(SIZE.x - 10.0, SIZE.y / 2.0)));
    h.run();
    let img = crop_to_window(&h.render().unwrap_or_else(|e| panic!("render {name}: {e}")));
    let out = img_dir().join(format!("{name}.png"));
    img.save(&out).unwrap_or_else(|e| panic!("save {}: {e}", out.display()));
    eprintln!("wrote {} ({}x{})", out.display(), img.width(), img.height());
}

fn hero(h: &mut H, link: &NativeLink) {
    build_bracket(h, link);
    h.state_mut().shell.selection.clear();
    shot(h, "hero");
}

fn tool_search(h: &mut H, link: &NativeLink) {
    build_bracket(h, link);
    h.state_mut().shell.focus_search = true;
    h.run_steps(2);
    h.event(egui::Event::Text("ho".to_owned()));
    h.run_steps(3);
    shot(h, "tool-search");
}

fn holes_threads(h: &mut H, link: &NativeLink) {
    build_bracket(h, link);
    harness::select_first_body(h);
    assert!(h.state_mut().open_tool("hole", None), "Hole opens");
    h.run_steps(4);
    h.state_mut().viewport.flush();
    h.run_steps(1);
    let at = h.state().viewport.world_to_screen([28.0, 0.0, 8.0]);
    click_at(h, at);
    h.state_mut().tool_set_value("preset", json!("ins_M3"));
    h.run_steps(4);
    // the live preview of the hole comes back from the engine
    for _ in 0..40 {
        settle(h, link);
        h.run_steps(2);
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    shot(h, "holes-threads");
}

fn sketch(h: &mut H, link: &NativeLink) {
    build_bracket(h, link);
    let body = h.state().bodies[0].id.clone();
    let face = json!({"origin": [0.0, 0.0, 8.0], "normal": [0.0, 0.0, 1.0], "xdir": [1.0, 0.0, 0.0],
        "label": "Top", "body": body});
    h.state_mut().enter_sketch(Some(face), None);
    settle(h, link);
    iso(h);
    assert!(h.state().sketch().is_some(), "the sketch opened");

    h.state_mut().sketch_set_tool("rect");
    h.run_steps(2);
    sketch_click_uv(h, [12.0, 2.0]);
    sketch_click_uv(h, [24.0, 10.0]);
    h.state_mut().sketch_set_tool("circle");
    h.run_steps(2);
    sketch_click_uv(h, [24.0, -14.0]);
    sketch_click_uv(h, [29.0, -14.0]);
    settle(h, link);
    let (rect, circle) = {
        let s = h.state().sketch().expect("sketch open");
        let id = |want: fn(&Entity) -> bool| {
            s.entities
                .iter()
                .find(|e| want(e))
                .and_then(|e| e.id())
                .map(str::to_owned)
        };
        (
            id(|e| matches!(e, Entity::Rect(_))).expect("rect drawn"),
            id(|e| matches!(e, Entity::Circle(_))).expect("circle drawn"),
        )
    };
    let m = h.state_mut().sketch_mode_mut().expect("sketch open");
    for c in [
        json!({"id": "dw", "type": "hdistance", "a": {"e": rect, "k": "c0"}, "b": {"e": rect, "k": "c1"},
            "off": -4, "value": 12}),
        json!({"id": "dh", "type": "vdistance", "a": {"e": rect, "k": "c1"}, "b": {"e": rect, "k": "c2"},
            "off": 4, "value": 8}),
        json!({"id": "dd", "type": "diameter", "a": {"e": circle}, "value": 10}),
    ] {
        m.s.constraints.push(serde_json::from_value(c).unwrap());
    }
    h.state_mut().sketch_set_tool("select");
    h.run_steps(4);
    settle(h, link);
    h.run_steps(4);
    shot(h, "sketch");
}

fn fit_to_bed(h: &mut H, link: &NativeLink) {
    import_stl(h, link, &fixture_stl("long.stl"));
    h.state_mut().shell.tab = "print".into();
    let body = h.state().bodies.last().unwrap().id.clone();
    h.state_mut().shell.selection = vec![body.clone()];
    let mut v = serde_json::Map::new();
    v.insert("body".into(), json!([body]));
    assert!(h.state_mut().open_tool("bedsplit", Some(v)), "Fit to Bed opens");
    h.run_steps(6);
    settle(h, link);
    h.state_mut().tool_ok();
    settle(h, link);
    harness::wait_for(h, link, "Fit to Bed to close", |h| h.state().tool().is_none());
    h.state_mut().shell.selection.clear();
    iso(h);
    shot(h, "fit-to-bed");
}

fn support_fins(h: &mut H, link: &NativeLink) {
    import_stl(h, link, &fixture_stl("mushroom.stl"));
    harness::select_first_body(h);
    iso(h);
    h.state_mut().shell.tab = "supports".into();
    harness::wait_support_open(h, link, "supportfins");
    // the Rotate mode's rings cover the fins in a still picture: show the Add fins mode
    h.state_mut().tool_set_value("_mode", json!("add"));
    h.run_steps(4);
    let end = std::time::Instant::now() + std::time::Duration::from_secs(90);
    while std::time::Instant::now() < end {
        settle(h, link);
        h.run_steps(2);
        let done = h
            .state()
            .tool()
            .and_then(|t| t.sf.as_ref())
            .is_some_and(|s| s.cover.as_ref().is_some_and(|c| !c.busy) && s.plastic.is_some());
        if done {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
    h.run_steps(4);
    shot(h, "support-fins");
}

const SHOTS: &[(&str, fn(&mut H, &NativeLink))] = &[
    ("hero", hero),
    ("tool_search", tool_search),
    ("holes_threads", holes_threads),
    ("sketch", sketch),
    ("fit_to_bed", fit_to_bed),
    ("support_fins", support_fins),
];

#[test]
#[ignore = "writes the website screenshots to docs/img; run with --ignored"]
fn site_shots() {
    let gpu = Headless::new().expect("a GPU adapter for the site screenshots");
    let only = std::env::var("SITE_SHOTS").unwrap_or_default();
    for (name, take) in SHOTS {
        if !only.is_empty() && !only.split(',').any(|o| o.trim() == *name) {
            continue;
        }
        let (mut h, link) = site::fresh(&gpu, SIZE, false, None);
        take(&mut h, &link);
    }
}
