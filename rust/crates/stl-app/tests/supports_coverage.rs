//! Support Fins coverage and Draw Walls (M4b Task 6).

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use serde_json::Value;
use stl_app::supports::coverage::{
    bare_points, compute, compute_cancellable, ComputeCancel, CoverageInput, CoverageJob, CoverageJobResult,
    DEFAULT_REACH,
};
use stl_app::supports::walls::{
    arc_path, wall_line, walls_to_params, Wall, WallClickHit, WallClickMsg, WallDrawer, SHAPE_STORAGE_KEY,
};
use stl_app::viewport::overhang_faces;
use stl_kernel::MeshArrays;

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn workspace_root() -> PathBuf {
    manifest_dir().join("../../..")
}

fn rust_root() -> PathBuf {
    manifest_dir().join("../..")
}

fn read_binary_stl(path: &PathBuf) -> MeshArrays {
    let buf = fs::read(path).expect("stl");
    let n = u32::from_le_bytes(buf[80..84].try_into().unwrap()) as usize;
    let mut positions = Vec::with_capacity(n * 9);
    let mut triangles = Vec::with_capacity(n * 3);
    let mut off = 84usize;
    for _ in 0..n {
        let i0 = (positions.len() / 3) as u32;
        for v in 0..3 {
            let base = off + 12 + v * 12;
            positions.push(f32::from_le_bytes(buf[base..base + 4].try_into().unwrap()));
            positions.push(f32::from_le_bytes(buf[base + 4..base + 8].try_into().unwrap()));
            positions.push(f32::from_le_bytes(buf[base + 8..base + 12].try_into().unwrap()));
        }
        triangles.extend_from_slice(&[i0, i0 + 1, i0 + 2]);
        off += 50;
    }
    MeshArrays { positions, triangles }
}

fn shelf_mesh() -> CoverageInput {
    CoverageInput {
        positions: vec![
            0.0, 0.0, 10.0, 30.0, 0.0, 10.0, 30.0, 20.0, 10.0, 0.0, 20.0, 10.0, 0.0, 0.0, 0.0, 0.0, 20.0, 0.0,
        ],
        triangles: vec![0, 2, 1, 0, 3, 2, 0, 4, 5, 0, 5, 3],
        over_faces: vec![0, 1],
        fins: Vec::new(),
        gap: 0.2,
        reach: 5.0,
        max_sub: 250_000,
    }
}

fn shelf_fin() -> Vec<f32> {
    let p = [
        [19.6, 0.0, 0.0],
        [19.6, 0.0, 9.8],
        [19.6, 20.0, 0.0],
        [19.6, 20.0, 9.8],
        [20.4, 0.0, 0.0],
        [20.4, 0.0, 9.8],
        [20.4, 20.0, 0.0],
        [20.4, 20.0, 9.8],
    ];
    let mut out = Vec::new();
    for i in 0..6 {
        out.extend_from_slice(&[
            p[i][0], p[i][1], p[i][2], p[i + 1][0], p[i + 1][1], p[i + 1][2], p[i + 2][0], p[i + 2][1], p[i + 2][2],
        ]);
    }
    out
}

/// Same corner order and triangles as `finSoup` in `dev/parity/js/sfcoverage_golden.mjs`.
fn fin_soup(lo: [f64; 3], hi: [f64; 3]) -> Vec<f32> {
    let mut p = Vec::with_capacity(8);
    for x in [lo[0], hi[0]] {
        for y in [lo[1], hi[1]] {
            for z in [lo[2], hi[2]] {
                p.push([x as f32, y as f32, z as f32]);
            }
        }
    }
    let mut out = Vec::new();
    for i in 0..6 {
        for k in 0..3 {
            out.extend_from_slice(&p[i + k]);
        }
    }
    out
}

/// Seat the part on z = 0 after `orient`. Must match `orientPositions` in the golden script.
fn orient_positions(positions: &[f32], orient: &str) -> Vec<f32> {
    let n = positions.len() / 3;
    let mut pts = Vec::with_capacity(n);
    for i in 0..n {
        let x = positions[i * 3] as f64;
        let y = positions[i * 3 + 1] as f64;
        let z = positions[i * 3 + 2] as f64;
        let (x, y, z) = match orient {
            "ry90" => (z, y, -x),
            "lift_y" => (x, y, z + 0.5 * y),
            other => panic!("unknown orient {other}"),
        };
        pts.push([x, y, z]);
    }
    let min_z = pts.iter().map(|p| p[2]).fold(f64::INFINITY, f64::min);
    let mut out = Vec::with_capacity(positions.len());
    for p in pts {
        out.push(p[0] as f32);
        out.push(p[1] as f32);
        out.push((p[2] - min_z) as f32);
    }
    out
}

fn vec3(v: &Value) -> [f64; 3] {
    let a = v.as_array().expect("point");
    [a[0].as_f64().unwrap(), a[1].as_f64().unwrap(), a[2].as_f64().unwrap()]
}

fn input_from_case(case: &Value) -> CoverageInput {
    if case.get("synthetic").and_then(Value::as_str) == Some("shelf") {
        let mut input = shelf_mesh();
        if case.get("fins").and_then(Value::as_bool).unwrap_or(false) {
            input.fins = vec![shelf_fin()];
        }
        return input;
    }
    let name = case["stl"].as_str().expect("stl");
    let mut mesh = read_binary_stl(&workspace_root().join("tests/ui/fixtures").join(name));
    if let Some(orient) = case.get("orient").and_then(Value::as_str) {
        mesh.positions = orient_positions(&mesh.positions, orient);
    }
    let thr = case["threshold_deg"].as_f64().unwrap_or(45.0);
    let mut input = CoverageInput {
        positions: mesh.positions.clone(),
        triangles: mesh.triangles.clone(),
        over_faces: overhang_faces(&mesh, thr),
        fins: Vec::new(),
        gap: case["gap"].as_f64().unwrap_or(0.2),
        reach: case["reach"].as_f64().unwrap_or(DEFAULT_REACH),
        max_sub: 250_000,
    };
    if let Some(box_) = case.get("fin_box").and_then(Value::as_array) {
        input.fins = vec![fin_soup(vec3(&box_[0]), vec3(&box_[1]))];
    }
    input
}

fn round4(x: f64) -> f64 {
    (x * 1e4).round() / 1e4
}

fn round_pts(v: &[[f64; 3]]) -> Vec<[f64; 3]> {
    v.iter().map(|p| [round4(p[0]), round4(p[1]), round4(p[2])]).collect()
}

fn bare_want(case: &Value) -> Vec<[f64; 3]> {
    case["bare_at"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            let a = p.as_array().unwrap();
            [a[0].as_f64().unwrap(), a[1].as_f64().unwrap(), a[2].as_f64().unwrap()]
        })
        .collect()
}

#[test]
fn coverage_matches_1x() {
    let golden_dir = rust_root().join("tests/parity/sfcoverage");
    for ent in fs::read_dir(&golden_dir).unwrap() {
        let path = ent.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let case: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        let name = path.file_name().unwrap().to_string_lossy();
        let out = compute(&input_from_case(&case));
        assert_eq!(round_pts(&bare_points(&out)), bare_want(&case), "{name}");
        if let (Some(h), Some(t)) = (case.get("held"), case.get("total")) {
            assert!((out.held - h.as_f64().unwrap()).abs() < 0.05, "{name} held");
            assert!((out.total - t.as_f64().unwrap()).abs() < 0.05, "{name} total");
        }
        if !bare_want(&case).is_empty() {
            assert!(!out.spots.is_empty(), "{name} expected bare spots");
        }
    }
}

#[test]
fn coverage_with_fin_changes_held() {
    let alone = compute(&shelf_mesh());
    let mut with_fin = shelf_mesh();
    with_fin.fins = vec![shelf_fin()];
    let fin = compute(&with_fin);
    assert!(fin.held > alone.held * 2.0);
    assert_eq!(fin.spots.len(), 2);
}

#[test]
fn coverage_off_ui_thread() {
    let mesh = read_binary_stl(&workspace_root().join("tests/ui/fixtures/mushroom.stl"));
    let input = input_from_case(&serde_json::json!({
        "stl": "mushroom.stl", "threshold_deg": 45, "gap": 0.2, "reach": 5
    }));
    let _ = mesh; // input_from_case reloads
    let job = CoverageJob::new();
    let seq = job.bump_generation();
    let main_id = thread::current().id();
    let t0 = Instant::now();
    job.start(seq, input);
    assert!(t0.elapsed() < Duration::from_millis(20), "start returns immediately");
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut worker = None;
    let mut result = None;
    while Instant::now() < deadline {
        if let Some(wid) = job.worker_thread_id() {
            worker = Some(wid);
            assert_ne!(wid, main_id);
        }
        if let Some(r) = job.poll() {
            result = Some(r);
            break;
        }
        thread::sleep(Duration::from_millis(2));
    }
    assert!(worker.is_some(), "coverage ran on a worker thread");
    assert!(matches!(result, Some(CoverageJobResult::Done(_))));
}

#[test]
fn compute_cancellable_none_when_cancelled() {
    let generation = Arc::new(AtomicU64::new(2));
    let cancel = ComputeCancel { generation, seq: 1 };
    assert!(compute_cancellable(&shelf_mesh(), Some(&cancel)).is_none());
}

#[test]
fn newer_change_cancels() {
    let alone = shelf_mesh();
    let mut with_fin = shelf_mesh();
    with_fin.fins = vec![shelf_fin()];
    let job = CoverageJob::new();
    job.hold_worker_at_enter();
    let seq1 = job.bump_generation();
    job.start(seq1, alone);
    let deadline = Instant::now() + Duration::from_secs(5);
    while !job.worker_entered() {
        assert!(Instant::now() < deadline, "worker did not start");
        thread::sleep(Duration::from_millis(1));
    }
    assert!(job.worker_thread_id().is_some(), "coverage is running");
    let seq2 = job.bump_generation();
    assert!(seq2 > seq1);
    job.release_enter_hold();
    let deadline = Instant::now() + Duration::from_secs(5);
    while job.worker_thread_id().is_some() {
        assert!(Instant::now() < deadline, "cancelled worker did not finish");
        thread::sleep(Duration::from_millis(1));
    }
    assert!(job.poll().is_none(), "cancelled run published a result");

    job.start(seq2, with_fin.clone());
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut got = None;
    while Instant::now() < deadline {
        if let Some(r) = job.poll() {
            got = Some(r);
            break;
        }
        thread::sleep(Duration::from_millis(2));
    }
    match got {
        Some(CoverageJobResult::Done(out)) => {
            let expect = compute(&with_fin);
            assert_eq!(out.spots.len(), expect.spots.len());
            assert!((out.held - expect.held).abs() < 1e-6);
        }
        other => panic!("expected Done for the newer run, got {other:?}"),
    }
}

fn hit(body: &str, p: [f64; 3], n: [f64; 3]) -> WallClickHit {
    WallClickHit {
        body_id: body.into(),
        point: p,
        normal: n,
    }
}

fn name(id: &str) -> String {
    if id == "b1" {
        "Body 1".into()
    } else {
        id.into()
    }
}

#[test]
fn wall_shape_line() {
    let mut d = WallDrawer::default();
    d.set_shape("line", |_, _| {});
    let (_, _) = d.click(Some(&hit("b1", [1.0, 2.0, 3.0], [0.0, 0.0, 1.0])), &[], None, name, false);
    let (w, _) = d.click(
        Some(&hit("b1", [4.0, 5.0, 6.0], [0.0, 0.0, 1.0])),
        &[],
        Some("b1"),
        name,
        false,
    );
    let list = w.expect("line wall");
    let json = walls_to_params(&list);
    assert_eq!(list[0].shape, "line");
    assert_eq!(list[0].points.len(), 2);
    assert_eq!(json[0]["shape"], "line");
}

#[test]
fn wall_shape_path() {
    let mut d = WallDrawer::default();
    d.set_shape("path", |_, _| {});
    d.click(Some(&hit("b", [0.0, 0.0, 0.0], [0.0, 1.0, 0.0])), &[], Some("b"), name, false);
    d.click(Some(&hit("b", [1.0, 0.0, 0.0], [0.0, 1.0, 0.0])), &[], Some("b"), name, false);
    d.click(Some(&hit("b", [2.0, 0.0, 0.0], [0.0, 1.0, 0.0])), &[], Some("b"), name, false);
    let mut walls = Vec::new();
    assert!(d.finish_path(&mut walls));
    assert_eq!(walls[0].shape, "path");
}

#[test]
fn wall_shape_curve() {
    let mut d = WallDrawer::default();
    d.set_shape("curve", |_, _| {});
    let clicks = [[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [5.0, 5.0, 2.0]];
    let mut committed = None;
    for (i, p) in clicks.into_iter().enumerate() {
        let (w, m) = d.click(Some(&hit("b", p, [0.0, 0.0, 1.0])), &[], Some("b"), name, false);
        assert_eq!(m, WallClickMsg::None);
        if i < 2 {
            assert!(w.is_none(), "a curve commits on the third click");
        }
        committed = w.or(committed);
    }
    let list = committed.expect("curve wall from the three clicks");
    assert_eq!(list[0].shape, "curve");
    assert_eq!(list[0].points, vec![[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [5.0, 5.0, 2.0]]);
    assert!(wall_line(&list[0]).len() >= 8);
    assert!(arc_path(list[0].points[0], list[0].points[1], list[0].points[2], 1.5).is_some());
}

#[test]
fn wall_shape_gusset() {
    let mut d = WallDrawer::default();
    d.set_shape("gusset", |_, _| {});
    let (_, m) = d.click(Some(&hit("b", [0.0, 0.0, 5.0], [1.0, 0.0, 0.0])), &[], Some("b"), name, false);
    assert_eq!(m, WallClickMsg::None);
    let (_, m) = d.click(Some(&hit("b", [0.0, 0.0, 10.0], [0.0, 0.0, -1.0])), &[], Some("b"), name, false);
    assert_eq!(m, WallClickMsg::None);
    let (_, m) = d.click(Some(&hit("b", [0.0, 0.0, 5.0], [0.0, 0.0, 1.0])), &[], Some("b"), name, false);
    assert_eq!(
        m,
        WallClickMsg::Warn("Start a gusset on a side of the part: an upright face below the overhang.".into())
    );
}

#[test]
fn wall_shape_anchor() {
    let mut d = WallDrawer::default();
    d.set_shape("anchor", |_, _| {});
    let (_, m) = d.click(Some(&hit("b", [0.0, 0.0, 0.0], [0.0, 0.0, 1.0])), &[], Some("b"), name, false);
    assert!(matches!(m, WallClickMsg::Warn(_)));
    let (w, _) = d.click(Some(&hit("b", [1.0, 2.0, 3.0], [1.0, 0.0, 0.0])), &[], Some("b"), name, false);
    assert_eq!(w.unwrap()[0].shape, "anchor");
}

#[test]
fn wall_shape_drop_fin_hint() {
    let d = WallDrawer::new_with_remembered(|k| {
        assert_eq!(k, SHAPE_STORAGE_KEY);
        Some("drop".into())
    });
    assert_eq!(d.shape, "drop");
    assert!(d.hint().contains("bare"));
}

#[test]
fn wall_shape_remembered() {
    use std::cell::RefCell;
    let stored = RefCell::new(None::<String>);
    let mut d = WallDrawer::new_with_remembered(|k| {
        assert_eq!(k, SHAPE_STORAGE_KEY);
        stored.borrow().clone()
    });
    d.set_shape("curve", |k, v| {
        assert_eq!(k, SHAPE_STORAGE_KEY);
        *stored.borrow_mut() = Some(v.to_string());
    });
    let d2 = WallDrawer::new_with_remembered(|_| stored.borrow().clone());
    assert_eq!(d2.shape, "curve");
}

#[test]
fn wall_body_mismatch_message() {
    let mut d = WallDrawer::default();
    let wall = Wall {
        shape: "line".into(),
        points: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]],
        normal: None,
    };
    let (_, m) = d.click(Some(&hit("other", [0.0, 0.0, 0.0], [0.0, 0.0, 1.0])), &[wall], Some("b1"), name, false);
    assert_eq!(
        m,
        WallClickMsg::Warn(
            "These walls are on Body 1. Finish (OK) them first, then draw on another body.".into()
        )
    );
}

#[test]
fn wall_path_double_click_finishes() {
    let mut d = WallDrawer::default();
    d.set_shape("path", |_, _| {});
    d.click(Some(&hit("b", [0.0, 0.0, 0.0], [0.0, 1.0, 0.0])), &[], Some("b"), name, false);
    d.click(Some(&hit("b", [5.0, 0.0, 0.0], [0.0, 1.0, 0.0])), &[], Some("b"), name, false);
    let (w, _) = d.click(
        Some(&hit("b", [5.0, 0.0, 0.0], [0.0, 1.0, 0.0])),
        &[],
        Some("b"),
        name,
        true,
    );
    assert!(w.is_some());
    assert!(d.pending.is_empty());
}

#[test]
fn wall_band_empty_pending() {
    let d = WallDrawer::default();
    assert!(d.band_polyline().is_none());
    let mut d2 = d;
    d2.pending.push([1.0, 2.0, 3.0]);
    assert!(d2.band_polyline().is_some());
}

#[test]
fn wall_key_enter_and_backspace() {
    let mut d = WallDrawer::default();
    d.set_shape("path", |_, _| {});
    d.pending = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]];
    let mut walls = Vec::new();
    assert!(d.handle_key("Enter", false, &mut walls));
    assert_eq!(walls.len(), 1);
    d.pending.push([2.0, 0.0, 0.0]);
    assert!(d.handle_key("Backspace", false, &mut walls));
    assert!(d.pending.is_empty());
}

#[test]
fn wall_json_format() {
    let w = Wall {
        shape: "gusset".into(),
        points: vec![[1.23456, 2.0, 3.0], [4.0, 5.0, 6.0]],
        normal: Some([1.0, 0.0, 0.0]),
    };
    let v = w.to_value();
    assert_eq!(v["shape"], "gusset");
    assert_eq!(v["normal"], serde_json::json!([1.0, 0.0, 0.0]));
    assert_eq!(v["points"].as_array().unwrap().len(), 2);
}
