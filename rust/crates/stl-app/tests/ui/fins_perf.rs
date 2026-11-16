//! Where the time goes when Support Fins adds many fins (Nathan, 2026-11-16: "really slow").
//! Run by hand: `cargo test --release -p stl-app --test ui fins_perf -- --ignored --nocapture`.
#![cfg(not(target_arch = "wasm32"))]

use std::time::Instant;

use serde_json::json;
use stl_app::engine_link::Command;

use super::harness::{empty_harness, import_stl, select_first_body, settle, wait_support_open};

#[test]
#[ignore]
fn fins_perf_many_fins() {
    let (mut h, link) = empty_harness();
    // FINS_PERF_STL=<path>: time a real part instead of the built one
    if let Ok(path) = std::env::var("FINS_PERF_STL") {
        let t = Instant::now();
        import_stl(&mut h, &link, std::path::Path::new(&path));
        eprintln!("import: {:?}, triangles {}", t.elapsed(), h.state().bodies.first().map(|b| b.mesh.triangles.len() / 3).unwrap_or(0));
        return time_fins(h, link);
    }
    // a mushroom: a post holding up a wide plate, so the plate's whole underside wants fins
    h.state_mut().link.send(Command::AddFeature {
        kind: "primitive".into(),
        params: json!({
            "shape": "box", "width": 10, "depth": 10, "height": 80,
            "plane": "XY", "offset": [0,0,0], "rotate": [0,0,0],
            "anchor": "base", "operation": "new", "targets": []
        }),
        index: None,
        name: Some("Post".into()),
    });
    settle(&mut h, &link);
    let post = h.state().bodies[0].id.clone();
    h.state_mut().link.send(Command::AddFeature {
        kind: "primitive".into(),
        params: json!({
            "shape": "box", "width": 120, "depth": 120, "height": 4,
            "plane": "XY", "offset": [0,0,80], "rotate": [0,0,0],
            "anchor": "base", "operation": "join", "targets": [post]
        }),
        index: None,
        name: Some("Plate".into()),
    });
    settle(&mut h, &link);
    // and a big ball beside it: a curved underside wants many fins
    h.state_mut().link.send(Command::AddFeature {
        kind: "primitive".into(),
        params: json!({
            "shape": "sphere", "radius": 40,
            "plane": "XY", "offset": [0,0,40], "rotate": [0,0,0],
            "anchor": "base", "operation": "join", "targets": [post]
        }),
        index: None,
        name: Some("Ball".into()),
    });
    settle(&mut h, &link);
    eprintln!("bodies before: {}", h.state().bodies.len());
    time_fins(h, link);
}

fn time_fins(mut h: egui_kittest::Harness<'static, stl_app::app::StlApp>, link: std::sync::Arc<stl_app::engine_link::native::NativeLink>) {
    select_first_body(&mut h);
    let t = Instant::now();
    wait_support_open(&mut h, &link, "supportfins");
    eprintln!("open Support Fins: {:?}", t.elapsed());
    for round in 0..2 {
        if let Some(tl) = h.state_mut().tool_mut() {
            tl.last_result = serde_json::Value::Null;
        }
        stl_engine::features::common::fins::clear_caches();
        let t = Instant::now();
        h.state_mut().run_preview_now(web_time::Instant::now());
        h.run_steps(1);
        let mut engine = None;
        while t.elapsed().as_secs() < 300 {
            h.run_steps(1);
            if engine.is_none() && !link.is_busy() {
                engine = Some(t.elapsed());
            }
            let done = h.state().tool().is_some_and(|tl| !tl.last_result.is_null());
            if engine.is_some() && done {
                break;
            }
        }
        let all = t.elapsed();
        let res = h.state().tool().map(|tl| tl.last_result.clone()).unwrap_or_default();
        let info: String = res.to_string().chars().take(600).collect();
        eprintln!("preview {round}: engine idle {engine:?}, shown {all:?}, preview bodies {}, result {info}", h.state().viewport.preview_meshes().count());
    }
    let t = Instant::now();
    h.run_steps(20);
    eprintln!("20 frames after: {:?}", t.elapsed());
    let t = Instant::now();
    h.state_mut().tool_ok();
    settle(&mut h, &link);
    eprintln!("OK (merge into the part): {:?}, bodies {}", t.elapsed(), h.state().bodies.len());
}
