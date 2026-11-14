//! The desktop engine link: commands in, updates out, with cancel, progress and panic recovery.
#![cfg(not(target_arch = "wasm32"))]

// keeps this test binary out of the real 2.0 folders (STLEDITOR2_CONFIG_DIR)
#[path = "support/test_env.rs"]
mod test_env;

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::json;
use stl_app::engine_link::native::NativeLink;
use stl_app::engine_link::worker::{Worker, TEST_SLOW_KIND};
use stl_app::engine_link::{BodyView, Command, EngineLink, Level, TimelineView, Update};

fn fixture(name: &str) -> Vec<u8> {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/projects").join(name);
    std::fs::read(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

/// Collects updates until the engine has nothing left to do.
fn drain(link: &NativeLink) -> Vec<Update> {
    let end = Instant::now() + Duration::from_secs(120);
    let mut all = Vec::new();
    loop {
        all.extend(link.poll());
        if !link.is_busy() {
            all.extend(link.poll());
            return all;
        }
        assert!(Instant::now() < end, "the engine never went idle");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn last_bodies(ups: &[Update]) -> Vec<BodyView> {
    ups.iter().rev().find_map(|u| if let Update::Bodies(b) = u { Some(b.clone()) } else { None }).expect("bodies")
}

fn last_timeline(ups: &[Update]) -> TimelineView {
    ups.iter().rev().find_map(|u| if let Update::Timeline(t) = u { Some(t.clone()) } else { None }).expect("timeline")
}

type Log = Arc<Mutex<Vec<Update>>>;

/// A worker whose updates land in a list, to run commands by hand without a thread.
fn manual() -> (Worker, Log, Arc<AtomicBool>) {
    let log: Log = Arc::new(Mutex::new(Vec::new()));
    let sink = log.clone();
    let cancel = Arc::new(AtomicBool::new(false));
    let w = Worker::new(Arc::new(move |u| sink.lock().unwrap().push(u)), cancel.clone());
    (w, log, cancel)
}

fn take(log: &Log) -> Vec<Update> {
    std::mem::take(&mut *log.lock().unwrap())
}

fn box_params(w: f64) -> serde_json::Value {
    json!({"shape": "box", "size": [w, 10.0, 10.0]})
}

fn open_basic() -> Command {
    Command::Open { name: "basic.stlx".into(), bytes: fixture("basic.stlx") }
}

#[test]
fn open_project_sends_bodies() {
    let link = NativeLink::new();
    link.send(open_basic());
    let ups = drain(&link);
    let bodies = last_bodies(&ups);
    assert!(!bodies.is_empty(), "the fixture has bodies");
    assert!(bodies.iter().all(|b| !b.mesh.triangles.is_empty() && !b.name.is_empty()));
    let t = last_timeline(&ups);
    assert_eq!(t.items.len(), 3);
    assert_eq!(t.items[0].kind, "import");
    assert!(ups.iter().any(|u| matches!(u, Update::Params(p) if p.iter().any(|q| q.name == "dx" && q.value == Some(5.0)))));
    assert!(ups.iter().any(|u| matches!(u, Update::Planes(_))));
    assert!(ups.iter().any(|u| matches!(u, Update::Busy(true))));
    assert!(matches!(ups.last(), Some(Update::Busy(false))));
}

fn slow() -> Command {
    Command::AddFeature { kind: TEST_SLOW_KIND.into(), params: json!({}), index: None, name: None }
}

fn add_box(w: f64) -> Command {
    Command::AddFeature { kind: "primitive".into(), params: box_params(w), index: None, name: None }
}

/// Polls until the slow command reports that it is under way (it has already changed the document).
fn wait_slow_started(link: &NativeLink) -> Vec<Update> {
    let end = Instant::now() + Duration::from_secs(30);
    let mut ups = Vec::new();
    while !ups.iter().any(|u| matches!(u, Update::Progress { label, .. } if label == "slow")) {
        ups.extend(link.poll());
        assert!(Instant::now() < end, "the slow command never started");
        std::thread::sleep(Duration::from_millis(2));
    }
    ups
}

fn same_bodies(a: &[BodyView], b: &[BodyView]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.id == y.id && x.mesh.positions == y.mesh.positions)
}

#[test]
fn command_during_rebuild() {
    // a command sent while a slow one runs returns at once and runs after it, in order
    let link = NativeLink::new();
    link.send(open_basic());
    drain(&link);
    link.send(slow());
    wait_slow_started(&link);
    let t0 = Instant::now();
    link.send(add_box(6.0));
    assert!(t0.elapsed() < Duration::from_millis(50), "send() blocked for {:?}", t0.elapsed());
    assert!(link.is_busy());
    let ups = drain(&link);
    let t = last_timeline(&ups);

    let (mut w, log, _) = manual();
    w.handle(open_basic());
    w.handle(slow());
    w.handle(add_box(6.0));
    let expect = take(&log);
    assert_eq!(t.items.len(), 3 + 2 + 1);
    assert_eq!(t.items.len(), last_timeline(&expect).items.len());
    assert!(same_bodies(&last_bodies(&ups), &last_bodies(&expect)));
}

#[test]
fn cancel_during_rebuild_then_next_command() {
    // Cancel stops the running command; the one queued behind it still runs, on the old state
    let link = NativeLink::new();
    link.send(open_basic());
    drain(&link);
    link.send(slow());
    wait_slow_started(&link);
    link.send(add_box(6.0));
    let t0 = Instant::now();
    link.send(Command::Cancel);
    assert!(t0.elapsed() < Duration::from_millis(50), "Cancel blocked");
    let ups = drain(&link);
    assert!(ups.iter().any(|u| matches!(u, Update::Message { text, .. } if text == "Cancelled.")));
    assert_eq!(last_timeline(&ups).items.len(), 3 + 1, "the slow command left nothing; the box was added");

    let (mut w, log, _) = manual();
    w.handle(open_basic());
    w.handle(add_box(6.0));
    assert!(same_bodies(&last_bodies(&ups), &last_bodies(&take(&log))));
}

#[test]
fn cancel_restores_document() {
    let link = NativeLink::new();
    link.send(open_basic());
    let opened = last_bodies(&drain(&link));
    link.send(add_box(4.0));
    link.send(add_box(6.0));
    link.send(Command::Undo); // undo depth 2, redo depth 1
    let before = drain(&link);
    let (tl, bodies) = (last_timeline(&before), last_bodies(&before));
    assert_eq!(tl.items.len(), 4);
    assert!(tl.can_undo && tl.can_redo);

    link.send(slow());
    wait_slow_started(&link);
    link.send(Command::Cancel);
    let ups = drain(&link);
    assert!(ups.iter().any(|u| matches!(u, Update::Message { level: Level::Info, text } if text == "Cancelled.")));
    assert!(!ups.iter().any(|u| matches!(u, Update::Error(_))), "a cancel is a status message, not an error");
    let tl2 = last_timeline(&ups);
    assert_eq!(tl2.items.len(), 4);
    assert!(tl2.can_undo && tl2.can_redo, "undo and redo history are as they were");
    let b2 = last_bodies(&ups);
    assert!(same_bodies(&b2, &bodies));
    assert!(b2.iter().zip(&bodies).all(|(x, y)| x.revision == y.revision));

    // history is intact: Redo gives the box again, then two Undos reach the opened project
    link.send(Command::Redo);
    assert_eq!(last_timeline(&drain(&link)).items.len(), 5);
    link.send(Command::Undo);
    link.send(Command::Undo);
    let ups = drain(&link);
    assert_eq!(last_timeline(&ups).items.len(), 4 - 1);
    link.send(Command::Undo);
    let ups = drain(&link);
    assert_eq!(last_timeline(&ups).items.len(), 3);
    assert!(same_bodies(&last_bodies(&ups), &opened));
}

#[test]
fn progress_reports_each_feature() {
    let (mut w, log, _) = manual();
    w.handle(open_basic());
    let ups = take(&log);
    let steps: Vec<(usize, usize)> = ups
        .iter()
        .filter_map(|u| if let Update::Progress { done, total, .. } = u { Some((*done, *total)) } else { None })
        .collect();
    // basic.stlx has 3 features with the rollback bar before the last: 2 are active
    assert_eq!(steps, vec![(1, 2), (2, 2)]);
    assert!(ups.iter().any(|u| matches!(u, Update::Progress { label, .. } if label == "Import1")));
}

#[test]
fn unchanged_body_keeps_revision() {
    let (mut w, log, _) = manual();
    w.handle(open_basic());
    w.handle(Command::Rollback(None)); // the Move that uses `dx` is past the bar in the fixture
    let first = last_bodies(&take(&log));
    w.handle(Command::BodyProps { id: first[0].id.clone(), name: Some("Renamed".into()), color: None, hidden: None });
    let second = last_bodies(&take(&log));
    assert_eq!(second[0].name, "Renamed");
    assert_eq!(second[0].revision, first[0].revision, "same mesh, same revision");
    assert!(Arc::ptr_eq(&second[0].mesh, &first[0].mesh));
    // a feature that changes the mesh changes the revision of that body only
    w.handle(Command::SetParams(vec![stl_engine::params::Param {
        name: "dx".into(),
        expr: "9".into(),
        note: String::new(),
    }]));
    let third = last_bodies(&take(&log));
    assert_eq!(third.len(), second.len());
    let changed = third.iter().zip(&second).filter(|(a, b)| a.revision != b.revision).count();
    assert_eq!(changed, 1, "only the moved body is new");
}

#[test]
fn engine_panic_shows_restart() {
    let link = NativeLink::new();
    link.send(open_basic());
    drain(&link);
    link.send(Command::Save);
    let saved = drain(&link).into_iter().find_map(|u| if let Update::Saved { bytes } = u { Some(bytes) } else { None });
    assert!(saved.is_some());
    link.send(Command::AddFeature {
        kind: stl_app::engine_link::worker::TEST_PANIC_KIND.into(),
        params: json!({}),
        index: None,
        name: None,
    });
    let end = Instant::now() + Duration::from_secs(30);
    let mut ups = Vec::new();
    while !ups.iter().any(|u| matches!(u, Update::EngineLost)) {
        ups.extend(link.poll());
        assert!(Instant::now() < end, "no EngineLost");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(!link.is_busy(), "a lost engine is not busy");
    // Restart reopens the last saved project
    link.restart();
    let ups = drain(&link);
    assert_eq!(last_timeline(&ups).items.len(), 3);
    assert!(!last_bodies(&ups).is_empty());
}

fn last_sketches(ups: &[Update]) -> Vec<stl_app::engine_link::SketchView> {
    ups.iter().rev().find_map(|u| if let Update::Sketches(s) = u { Some(s.clone()) } else { None }).expect("sketches")
}

#[test]
fn set_suppressed_round_trip() {
    let (mut w, log, _) = manual();
    w.handle(open_basic());
    let ups = take(&log);
    let before = last_bodies(&ups);
    let tl = last_timeline(&ups);
    // the second import is inside the rollback marker, so switching it off must change the bodies
    let id = tl.items[1].id.clone();
    assert!(!tl.items[1].suppressed);

    w.handle(Command::SetSuppressed { id: id.clone(), suppressed: true });
    let ups = take(&log);
    assert!(last_timeline(&ups).items[1].suppressed);
    let off = last_bodies(&ups);
    assert!(!same_bodies(&before, &off), "suppressing a step changes the bodies");

    w.handle(Command::Undo);
    assert!(!last_timeline(&take(&log)).items[1].suppressed, "undo brings it back");
    w.handle(Command::Redo);
    assert!(last_timeline(&take(&log)).items[1].suppressed);

    w.handle(Command::SetSuppressed { id, suppressed: false });
    let ups = take(&log);
    assert!(!last_timeline(&ups).items[1].suppressed);
    assert!(same_bodies(&before, &last_bodies(&ups)));
}

#[test]
fn sketch_view_has_geometry() {
    let (mut w, log, _) = manual();
    let params = json!({"plane": "XY", "name": "S1", "entities": [
        {"id": "r1", "type": "rect", "x": 0, "y": 0, "w": 20, "h": 10},
        {"id": "l1", "type": "line", "p1": [0, 0], "p2": [5, 5], "construction": true}
    ]});
    w.handle(Command::AddFeature { kind: "sketch".into(), params, index: None, name: None });
    let first = last_sketches(&take(&log));
    assert_eq!(first.len(), 1);
    let s = &first[0];
    assert_eq!(s.outlines.len(), 2);
    assert!(s.outlines[0].closed && !s.outlines[0].construction && s.outlines[0].pts.len() >= 4);
    assert!(s.outlines[1].construction && !s.outlines[1].closed);
    assert_eq!(s.frame[3], [0.0, 0.0, 1.0], "XY plane normal");

    // an unrelated change keeps the very same curves
    w.handle(Command::AddFeature { kind: "primitive".into(), params: box_params(4.0), index: None, name: None });
    let second = last_sketches(&take(&log));
    assert!(Arc::ptr_eq(&first[0].outlines, &second[0].outlines));
}

fn last_planes(ups: &[Update]) -> Vec<stl_app::engine_link::PlaneView> {
    ups.iter().rev().find_map(|u| if let Update::Planes(p) = u { Some(p.clone()) } else { None }).expect("planes")
}

#[test]
fn planes_skip_base_and_follow_hidden() {
    let (mut w, log, _) = manual();
    w.handle(Command::AddFeature {
        kind: "plane".into(),
        params: json!({"mode": "offset", "base": "XY", "distance": 10, "name": "P1"}),
        index: None,
        name: None,
    });
    let planes = last_planes(&take(&log));
    assert_eq!(planes.len(), 1, "base planes are drawn by the viewport, not sent");
    assert!(!["XY", "XZ", "YZ"].contains(&planes[0].id.as_str()));
    assert!(planes[0].visible);
    let id = planes[0].id.clone();

    w.handle(Command::BodyProps { id: id.clone(), name: None, color: None, hidden: Some(true) });
    let planes = last_planes(&take(&log));
    assert_eq!(planes.len(), 1);
    assert!(!planes[0].visible, "a hidden plane is not drawn");

    w.handle(Command::BodyProps { id, name: None, color: None, hidden: Some(false) });
    assert!(last_planes(&take(&log))[0].visible);
}
