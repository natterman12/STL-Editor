//! Headless Chrome: the engine Web Worker opens a project, transfers mesh buffers, and
//! reports EngineLost when the worker hits an error.
#![cfg(target_arch = "wasm32")]

use stl_app::engine_link::web::WebLink;
use stl_app::engine_link::worker::TEST_PANIC_KIND;
use stl_app::engine_link::{Command, EngineLink, Level, Update};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

const BASIC: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/projects/basic.stlx"));

fn kinds(updates: &[Update]) -> String {
    updates
        .iter()
        .map(|u| match u {
            Update::Bodies(b) => format!("bodies({})", b.len()),
            Update::Timeline(t) => format!("timeline({})", t.items.len()),
            Update::Params(p) => format!("params({})", p.len()),
            Update::Planes(p) => format!("planes({})", p.len()),
            Update::Sketches(s) => format!("sketches({})", s.len()),
            Update::Message { level, text } => format!("message({level:?}, {text})"),
            Update::Progress { done, total, label } => format!("progress({done}/{total} {label})"),
            Update::Busy(b) => format!("busy({b})"),
            Update::Saved { bytes } => format!("saved({})", bytes.len()),
            Update::Exported { name, count, .. } => format!("exported({name}, {count})"),
            Update::Error(t) => format!("error({t})"),
            Update::EngineLost => "engineLost".into(),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

async fn sleep(ms: i32) {
    let p = js_sys::Promise::new(&mut |resolve, _reject| {
        let win = web_sys::window().expect("window");
        let resolve = resolve.clone();
        let c = wasm_bindgen::closure::Closure::once(move || {
            let _ = resolve.call0(&wasm_bindgen::JsValue::UNDEFINED);
        });
        let _ = win.set_timeout_with_callback_and_timeout_and_arguments_0(c.as_ref().unchecked_ref(), ms);
        c.forget();
    });
    let _ = JsFuture::from(p).await;
}

async fn until(link: &WebLink, ms: f64, pred: impl Fn(&WebLink, &[Update]) -> bool) -> Vec<Update> {
    let start = js_sys::Date::now();
    let mut all = Vec::new();
    loop {
        all.extend(link.poll());
        if pred(link, &all) {
            return all;
        }
        if js_sys::Date::now() - start > ms {
            panic!("timed out after {ms}ms; saw [{}]; fault: {}", kinds(&all), link.fault());
        }
        sleep(20).await;
    }
}

fn open_basic(link: &WebLink) {
    link.send(Command::Open { name: "basic.stlx".into(), bytes: BASIC.to_vec() });
}

fn bodies(updates: &[Update]) -> Option<&[stl_app::engine_link::BodyView]> {
    updates.iter().rev().find_map(|u| match u {
        Update::Bodies(b) => Some(b.as_slice()),
        _ => None,
    })
}

fn timeline_len(updates: &[Update]) -> Option<usize> {
    updates.iter().rev().find_map(|u| match u {
        Update::Timeline(t) => Some(t.items.len()),
        _ => None,
    })
}

#[wasm_bindgen_test]
async fn worker_opens_project() {
    let link = WebLink::new().expect("worker");
    open_basic(&link);
    let updates = until(&link, 20_000.0, |_l, u| bodies(u).is_some_and(|b| !b.is_empty())).await;
    let bodies = bodies(&updates).expect("bodies");
    assert!(bodies.iter().all(|b| !b.name.is_empty()), "every body has a name");
    assert!(
        bodies.iter().any(|b| !b.mesh.positions.is_empty() && !b.mesh.triangles.is_empty()),
        "a body came back with a mesh"
    );
}

#[wasm_bindgen_test]
async fn worker_mesh_is_transferred() {
    let link = WebLink::new().expect("worker");
    open_basic(&link);
    let updates = until(&link, 20_000.0, |l, u| {
        bodies(u).is_some_and(|b| b.iter().any(|body| !body.mesh.positions.is_empty())) && l.mesh_was_transferred() == Some(true)
    })
    .await;
    let bodies = bodies(&updates).expect("bodies");
    let mesh = bodies.iter().find(|b| !b.mesh.positions.is_empty()).expect("mesh");
    assert_eq!(mesh.mesh.positions.len() % 3, 0);
    assert!(!mesh.mesh.triangles.is_empty());
    assert_eq!(link.mesh_was_transferred(), Some(true), "the worker buffer was not detached");
}

#[wasm_bindgen_test]
async fn worker_error_sends_engine_lost() {
    let link = WebLink::new().expect("worker");
    open_basic(&link);
    let opened = until(&link, 20_000.0, |_l, u| bodies(u).is_some_and(|b| !b.is_empty()) && timeline_len(u).is_some()).await;
    let n_bodies = bodies(&opened).unwrap().len();
    let n_items = timeline_len(&opened).unwrap();
    link.send(Command::AddFeature { kind: TEST_PANIC_KIND.into(), params: serde_json::json!({}), index: None });
    let _lost = until(&link, 20_000.0, |_l, u| u.iter().any(|x| matches!(x, Update::EngineLost))).await;
    link.restart();
    let restored = until(&link, 20_000.0, |_l, u| {
        bodies(u).is_some_and(|b| b.len() == n_bodies) && timeline_len(u) == Some(n_items)
    })
    .await;
    assert_eq!(bodies(&restored).unwrap().len(), n_bodies, "restart reopened the last saved project");
    assert_eq!(timeline_len(&restored), Some(n_items), "restart restored the timeline");
}

#[wasm_bindgen_test]
async fn worker_cancel_stops_between_features() {
    let link = WebLink::new().expect("worker");
    open_basic(&link);
    let opened = until(&link, 20_000.0, |_l, u| bodies(u).is_some_and(|b| !b.is_empty()) && timeline_len(u).is_some()).await;
    let n_bodies = bodies(&opened).unwrap().len();
    let n_items = timeline_len(&opened).unwrap();
    // Queued behind the add, so the yield before the first feature of that rebuild sees it.
    link.send(Command::AddFeature {
        kind: "primitive".into(),
        params: serde_json::json!({"shape": "box", "size": [4.0, 10.0, 10.0]}),
        index: None,
    });
    link.send(Command::Cancel);
    let updates = until(&link, 20_000.0, |_l, u| {
        u.iter().any(|x| matches!(x, Update::Message { text, .. } if text == "Cancelled.")) && timeline_len(u) == Some(n_items)
    })
    .await;
    assert!(updates.iter().any(|u| matches!(u, Update::Message { level: Level::Info, text } if text == "Cancelled.")));
    assert!(!updates.iter().any(|u| matches!(u, Update::Error(_))), "a cancel is a status message, not an error");
    assert_eq!(timeline_len(&updates), Some(n_items), "the cancelled feature was not kept");
    assert_eq!(bodies(&updates).map(|b| b.len()), Some(n_bodies));
}

fn set_no_jspi(on: bool) {
    let _ = js_sys::Reflect::set(&js_sys::global(), &"__STL_NO_JSPI__".into(), &wasm_bindgen::JsValue::from_bool(on));
}

/// Forces the synchronous worker path. The module must still start, and a
/// cancel posted with an add must not roll that add back.
#[wasm_bindgen_test]
async fn worker_without_jspi_keeps_the_feature() {
    set_no_jspi(true);
    let link = WebLink::new().expect("worker");
    set_no_jspi(false);
    open_basic(&link);
    let opened = until(&link, 20_000.0, |_l, u| bodies(u).is_some_and(|b| !b.is_empty()) && timeline_len(u).is_some()).await;
    let n_items = timeline_len(&opened).unwrap();
    link.send(Command::AddFeature {
        kind: "primitive".into(),
        params: serde_json::json!({"shape": "box", "size": [4.0, 10.0, 10.0]}),
        index: None,
    });
    link.send(Command::Cancel);
    let updates = until(&link, 20_000.0, |_l, u| timeline_len(u) == Some(n_items + 1)).await;
    assert!(!updates.iter().any(|u| matches!(u, Update::Message { text, .. } if text == "Cancelled.")));
    assert!(!updates.iter().any(|u| matches!(u, Update::EngineLost | Update::Error(_))), "the sync path should finish the add");
    assert_eq!(timeline_len(&updates), Some(n_items + 1), "without JSPI the in-flight add is not stopped");
}

/// A project that fails to open must not become the file Restart reloads.
#[wasm_bindgen_test]
async fn worker_failed_open_keeps_last_project() {
    let link = WebLink::new().expect("worker");
    open_basic(&link);
    let opened = until(&link, 20_000.0, |_l, u| bodies(u).is_some_and(|b| !b.is_empty()) && timeline_len(u).is_some()).await;
    let n_bodies = bodies(&opened).unwrap().len();
    let n_items = timeline_len(&opened).unwrap();
    link.send(Command::Open { name: "bad.stlx".into(), bytes: b"not a project".to_vec() });
    let _bad = until(&link, 20_000.0, |_l, u| {
        u.iter().any(|x| matches!(x, Update::Error(text) if text.contains("isn't an STL Editor project")))
    })
    .await;
    link.send(Command::AddFeature { kind: TEST_PANIC_KIND.into(), params: serde_json::json!({}), index: None });
    let _lost = until(&link, 20_000.0, |_l, u| u.iter().any(|x| matches!(x, Update::EngineLost))).await;
    link.restart();
    let restored = until(&link, 20_000.0, |_l, u| {
        bodies(u).is_some_and(|b| b.len() == n_bodies) && timeline_len(u) == Some(n_items)
    })
    .await;
    assert_eq!(bodies(&restored).unwrap().len(), n_bodies, "restart reopened the project that finished opening");
    assert_eq!(timeline_len(&restored), Some(n_items));
}
