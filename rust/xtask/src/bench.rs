//! `cargo run --release -p xtask -- bench`: the same operations on `dev/bench/test_mesh.stl`
//! through the 1.x `Document` (`dev/parity/bench_1x.py`) and the Rust one; fails if 2.0 is slower.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use serde_json::{json, Map, Value};
use stl_engine::{Document, Registry};

const RUNS: usize = 3;
const NAMES: [(&str, &str); 17] = [
    ("import", "Import (repair on)"),
    ("combine_join", "Combine join, 30 mm cube"),
    ("combine_cut", "Combine cut, 30 mm cube"),
    ("split", "Split at z = 0"),
    ("repair_quick", "Repair quick, 1% triangles missing"),
    ("extrude", "Extrude 200-segment sketch, 20 mm"),
    ("revolve", "Revolve 200-segment sketch, 360 deg"),
    ("sweep", "Sweep 10 mm circle, 40-point path"),
    ("simplify", "Simplify ratio 0.1"),
    ("remesh", "Remesh edge 1.0"),
    ("hollow", "Hollow 2 mm normal"),
    ("shell", "Shell 2 mm, no faces"),
    ("lattice", "Lattice cell 6, strut 1, wall 2"),
    ("offset", "Offset grow 0.5 mm"),
    ("thincheck", "Thin-wall Check 0.8 mm"),
    ("repair_rebuild", "Repair rebuild normal, broken"),
    ("sculpt", "Sculpt Smooth, 20 pts"),
];

fn obj(v: Value) -> Map<String, Value> {
    match v {
        Value::Object(m) => m,
        _ => Map::new(),
    }
}

/// Binary STL with every 100th triangle (index 99, 199, ...) removed. Same rule as bench_1x.py.
fn broken_copy(data: &[u8]) -> Vec<u8> {
    let n = u32::from_le_bytes([data[80], data[81], data[82], data[83]]) as usize;
    let kept: Vec<usize> = (0..n).filter(|i| i % 100 != 99).collect();
    let mut out = data[..80].to_vec();
    out.extend_from_slice(&(kept.len() as u32).to_le_bytes());
    for i in kept {
        out.extend_from_slice(&data[84 + 50 * i..134 + 50 * i]);
    }
    out
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.total_cmp(b));
    v[v.len() / 2]
}

fn doc_with_mesh(data: &[u8], repair: bool) -> Result<(Document, String), String> {
    let mut d = Document::new(Registry::builtin());
    let sid = d.add_source("test_mesh.stl", data.to_vec());
    let id = d
        .add_feature("import", obj(json!({"source": sid, "repair": repair, "place": "keep"})), None)
        .map_err(|e| e.0)?;
    Ok((d, id))
}

fn with_cube(data: &[u8]) -> Result<(Document, String, String), String> {
    let (mut d, mesh) = doc_with_mesh(data, true)?;
    let cube = d
        .add_feature(
            "primitive",
            obj(json!({"shape": "box", "length": 30, "width": 30, "height": 30,
                       "anchor": "center", "plane": "XY", "operation": "new"})),
            None,
        )
        .map_err(|e| e.0)?;
    Ok((d, mesh, cube))
}

fn time_op(mut run: impl FnMut() -> Result<f64, String>) -> Result<f64, String> {
    let mut v = Vec::new();
    for _ in 0..RUNS {
        v.push(run()?);
    }
    Ok(median(v))
}

fn secs(t: Instant) -> f64 {
    t.elapsed().as_secs_f64()
}

/// Regular n-gon points (same rule as bench_1x.py).
fn ngon(cx: f64, cy: f64, r: f64, n: usize) -> Value {
    let pts: Vec<Value> = (0..n)
        .map(|i| {
            let a = 2.0 * std::f64::consts::PI * i as f64 / n as f64;
            json!([cx + r * a.cos(), cy + r * a.sin()])
        })
        .collect();
    Value::Array(pts)
}

fn with_sketch(entities: Value, plane: &str) -> Result<(Document, String), String> {
    let mut d = Document::new(Registry::builtin());
    let sk = d
        .add_feature("sketch", obj(json!({"plane": plane, "entities": entities})), None)
        .map_err(|e| e.0)?;
    Ok((d, sk))
}

fn time_rust(data: &[u8]) -> Result<Vec<f64>, String> {
    let broken = broken_copy(data);
    let mut out = Vec::new();
    out.push(time_op(|| {
        let mut d = Document::new(Registry::builtin());
        let sid = d.add_source("test_mesh.stl", data.to_vec());
        let t = Instant::now();
        d.add_feature("import", obj(json!({"source": sid, "repair": true, "place": "keep"})), None)
            .map_err(|e| e.0)?;
        Ok(secs(t))
    })?);
    for op in ["join", "cut"] {
        out.push(time_op(|| {
            let (mut d, mesh, cube) = with_cube(data)?;
            let t = Instant::now();
            d.add_feature(
                "combine",
                obj(json!({"target": mesh, "tools": [cube], "operation": op, "keep_tools": false})),
                None,
            )
            .map_err(|e| e.0)?;
            Ok(secs(t))
        })?);
    }
    out.push(time_op(|| {
        let (mut d, mesh) = doc_with_mesh(data, true)?;
        let t = Instant::now();
        d.add_feature(
            "split",
            obj(json!({"body": mesh, "plane": {"origin": [0, 0, 0], "normal": [0, 0, 1]},
                       "keep": "both", "connector": "none"})),
            None,
        )
        .map_err(|e| e.0)?;
        Ok(secs(t))
    })?);
    out.push(time_op(|| {
        let (mut d, mesh) = doc_with_mesh(&broken, false)?;
        let t = Instant::now();
        d.add_feature("repair", obj(json!({"body": mesh, "mode": "quick"})), None).map_err(|e| e.0)?;
        Ok(secs(t))
    })?);
    let ring = json!([{"id": "r", "type": "polyline", "closed": true, "pts": ngon(30.0, 0.0, 10.0, 200)}]);
    out.push(time_op(|| {
        let (mut d, sk) = with_sketch(ring.clone(), "XY")?;
        let t = Instant::now();
        d.add_feature("extrude", obj(json!({"sketch": sk, "distance": 20})), None).map_err(|e| e.0)?;
        Ok(secs(t))
    })?);
    out.push(time_op(|| {
        let (mut d, sk) = with_sketch(ring.clone(), "XY")?;
        let t = Instant::now();
        d.add_feature("revolve", obj(json!({"sketch": sk, "axis": "y", "angle": 360})), None)
            .map_err(|e| e.0)?;
        Ok(secs(t))
    })?);
    out.push(time_op(|| {
        let (mut d, prof) = with_sketch(json!([{"id": "c", "type": "circle", "cx": 0, "cy": 0, "r": 5}]), "XY")?;
        let pts: Vec<Value> = (0..40).map(|i| json!([5.0 * (i as f64 * 0.3).sin(), 2.0 * i as f64])).collect();
        let path = d
            .add_feature(
                "sketch",
                obj(json!({"plane": "XZ", "entities": [{"id": "p", "type": "polyline", "closed": false, "pts": pts}]})),
                None,
            )
            .map_err(|e| e.0)?;
        let t = Instant::now();
        d.add_feature("sweep", obj(json!({"profile": prof, "path": path})), None).map_err(|e| e.0)?;
        Ok(secs(t))
    })?);
    out.push(time_op(|| {
        let (mut d, mesh) = doc_with_mesh(data, true)?;
        let t = Instant::now();
        d.add_feature("simplify", obj(json!({"body": mesh, "ratio": 0.1})), None)
            .map_err(|e| e.0)?;
        Ok(secs(t))
    })?);
    out.push(time_op(|| {
        let (mut d, mesh) = doc_with_mesh(data, true)?;
        let t = Instant::now();
        d.add_feature("remesh", obj(json!({"body": mesh, "edge": 1.0})), None)
            .map_err(|e| e.0)?;
        Ok(secs(t))
    })?);
    out.push(time_op(|| {
        let (mut d, mesh) = doc_with_mesh(data, true)?;
        let t = Instant::now();
        d.add_feature(
            "hollow",
            obj(json!({"body": mesh, "thickness": 2, "resolution": "normal"})),
            None,
        )
        .map_err(|e| e.0)?;
        Ok(secs(t))
    })?);
    out.push(time_op(|| {
        let (mut d, mesh) = doc_with_mesh(data, true)?;
        let t = Instant::now();
        d.add_feature("shell", obj(json!({"body": mesh, "thickness": 2, "faces": []})), None)
            .map_err(|e| e.0)?;
        Ok(secs(t))
    })?);
    out.push(time_op(|| {
        let (mut d, mesh) = doc_with_mesh(data, true)?;
        let t = Instant::now();
        d.add_feature(
            "lattice",
            obj(json!({"body": mesh, "cell": 6, "strut": 1, "thickness": 2})),
            None,
        )
        .map_err(|e| e.0)?;
        Ok(secs(t))
    })?);
    out.push(time_op(|| {
        let (mut d, mesh) = doc_with_mesh(data, true)?;
        let t = Instant::now();
        d.add_feature(
            "offset",
            obj(json!({"bodies": [mesh], "distance": 0.5, "direction": "grow"})),
            None,
        )
        .map_err(|e| e.0)?;
        Ok(secs(t))
    })?);
    out.push(time_op(|| {
        let (mut d, mesh) = doc_with_mesh(data, true)?;
        let t = Instant::now();
        d.add_feature("thincheck", obj(json!({"body": mesh, "min": 0.8})), None)
            .map_err(|e| e.0)?;
        Ok(secs(t))
    })?);
    out.push(time_op(|| {
        let (mut d, mesh) = doc_with_mesh(&broken, false)?;
        let t = Instant::now();
        d.add_feature(
            "repair",
            obj(json!({"body": mesh, "mode": "rebuild", "resolution": "normal"})),
            None,
        )
        .map_err(|e| e.0)?;
        Ok(secs(t))
    })?);
    out.push(time_op(|| {
        let (mut d, mesh) = doc_with_mesh(data, true)?;
        let bb = d
            .state
            .body(&mesh)
            .map_err(|e| e.0)?
            .require_solid()
            .map_err(|e| e.0)?
            .bounding_box();
        let (lo, hi) = (bb.min, bb.max);
        let y = 0.5 * (lo[1] + hi[1]);
        let z = hi[2];
        let pts: Vec<Value> = (0..20)
            .map(|i| {
                let x = lo[0] + i as f64 * (hi[0] - lo[0]) / 19.0;
                json!([x, y, z])
            })
            .collect();
        let t = Instant::now();
        d.add_feature(
            "sculpt",
            obj(json!({
                "body": mesh,
                "strokes": [{"points": pts, "radius": 5, "strength": 0.5, "mode": "smooth"}]
            })),
            None,
        )
        .map_err(|e| e.0)?;
        Ok(secs(t))
    })?);
    Ok(out)
}

fn time_python(root: &Path) -> Result<Value, String> {
    let python = std::env::var("PYTHON").unwrap_or_else(|_| r"C:\Program Files\Python314\python.exe".into());
    let out = Command::new(&python)
        .arg(root.join("dev/parity/bench_1x.py"))
        .current_dir(root)
        .output()
        .map_err(|e| format!("could not run {python}: {e}"))?;
    if !out.status.success() {
        return Err(format!("bench_1x.py failed: {}", String::from_utf8_lossy(&out.stderr)));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.lines().rev().find(|l| l.trim_start().starts_with('{')).unwrap_or("");
    serde_json::from_str(line).map_err(|e| format!("bad output from bench_1x.py ({e}): {text}"))
}

/// Run the benchmark. `Ok(true)` when every ratio is at most 1.0.
pub fn run() -> Result<bool, String> {
    let root: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().map_err(|e| e.to_string())?;
    let mesh = root.join("dev/bench/test_mesh.stl");
    if !mesh.exists() {
        println!("bench: making {} with node", mesh.display());
        let st = Command::new("node")
            .arg(root.join("dev/bench/make_test_mesh.mjs"))
            .current_dir(&root)
            .status()
            .map_err(|e| format!("could not run node: {e}"))?;
        if !st.success() || !mesh.exists() {
            return Err("make_test_mesh.mjs failed".into());
        }
    }
    let data = std::fs::read(&mesh).map_err(|e| e.to_string())?;
    println!("bench: timing 2.0 (median of {RUNS})...");
    let rust = time_rust(&data)?;
    println!("bench: timing 1.x (median of {RUNS})...");
    let py = time_python(&root)?;

    println!("\n{:<38} {:>9} {:>9} {:>8}", "operation", "1.x s", "2.0 s", "ratio");
    let mut ok = true;
    for ((key, label), r) in NAMES.iter().zip(rust) {
        let p = py[key].as_f64().ok_or_else(|| format!("bench_1x.py gave no \"{key}\""))?;
        let ratio = r / p;
        let flag = if ratio > 1.0 { "  SLOWER" } else { "" };
        println!("{label:<38} {p:>9.3} {r:>9.3} {ratio:>8.2}{flag}");
        ok &= ratio <= 1.0;
    }
    Ok(ok)
}
