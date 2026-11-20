//! `cargo run --release -p xtask -- bench`: the same operations on `dev/bench/test_mesh.stl`
//! through the 1.x `Document` (`dev/parity/bench_1x.py`) and the Rust one; fails if 2.0 is slower.
//! `--save [file]` keeps the 2.0 times and `--compare [file]` sets a later run beside them (see `run`).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Instant;

use serde_json::{Map, Value, json};
use stl_engine::{Document, ExportFormat, FinsHost, Registry};
use stl_platform::fins::WebviewHost;

const RUNS: usize = 3;
const NAMES: [(&str, &str); 29] = [
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
    ("supportfins", "Support Fins, default options"),
    ("bedsplit", "Fit to Bed, A1 mini (part too big)"),
    ("layflat", "Lay Flat auto"),
    ("plate", "Plate Layout, 6 copies"),
    ("pattern", "Pattern circular x12"),
    ("thread", "Thread M8 x 30 external"),
    ("resizehole", "Resize Hole (hole_and_pin)"),
    ("supportwall", "Draw Walls, one line"),
    ("export_stl", "Export STL"),
    ("export_3mf", "Export 3MF"),
    ("export_obj", "Export OBJ"),
    ("export_step", "Export STEP"),
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
        .add_feature(
            "import",
            obj(json!({"source": sid, "repair": repair, "place": "keep"})),
            None,
        )
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

fn with_fins_doc(data: &[u8], host: Arc<dyn FinsHost>) -> Result<(Document, String), String> {
    let (mut d, mesh) = doc_with_mesh(data, true)?;
    d.set_fins_host(host);
    Ok((d, mesh))
}

/// Bench mesh is ~40 mm; scale ×10 so it needs cuts on the A1 mini (180 mm).
fn with_oversized_for_bed(data: &[u8]) -> Result<(Document, String), String> {
    let (mut d, mesh) = doc_with_mesh(data, true)?;
    d.add_feature(
        "scale",
        obj(json!({"bodies": [mesh], "factor": 10, "pivot": "center"})),
        None,
    )
    .map_err(|e| e.0)?;
    Ok((d, mesh))
}

fn with_six_copies(data: &[u8]) -> Result<(Document, Vec<String>), String> {
    let (mut d, mesh) = doc_with_mesh(data, true)?;
    let pat = d
        .add_feature(
            "pattern",
            obj(json!({
                "bodies": [mesh],
                "kind": "linear",
                "direction": [1, 0, 0],
                "spacing": 50,
                "count": 6,
                "join": false
            })),
            None,
        )
        .map_err(|e| e.0)?;
    let mut bodies = vec![mesh.clone()];
    for i in 2..=6 {
        bodies.push(format!("{pat}.{mesh}.{i}"));
    }
    Ok((d, bodies))
}

/// Offset from the origin so circular ×12 does real work (a centred near-sphere is almost identity).
fn with_pattern_source(data: &[u8]) -> Result<(Document, String), String> {
    let (mut d, mesh) = doc_with_mesh(data, true)?;
    d.add_feature(
        "move",
        obj(json!({"bodies": [mesh], "translate": [40, 0, 0], "rotate": [0, 0, 0], "pivot": "origin"})),
        None,
    )
    .map_err(|e| e.0)?;
    Ok((d, mesh))
}

fn time_rust(root: &Path, data: &[u8]) -> Result<Vec<f64>, String> {
    let broken = broken_copy(data);
    let hole_path = root.join("rust/tests/models/hole_and_pin.stl");
    let hole_data = std::fs::read(&hole_path).map_err(|e| {
        format!(
            "Resize Hole uses hole_and_pin.stl (bench mesh has a square cut, not a round wall): {}: {e}",
            hole_path.display()
        )
    })?;
    let engine = root.join("web/vendor/support-fins/engine.iife.js");
    let fins: Arc<dyn FinsHost> =
        Arc::new(WebviewHost::start(&engine).map_err(|e| format!("WebviewHost::start failed: {e}"))?);

    let mut out = Vec::new();
    out.push(time_op(|| {
        let mut d = Document::new(Registry::builtin());
        let sid = d.add_source("test_mesh.stl", data.to_vec());
        let t = Instant::now();
        d.add_feature(
            "import",
            obj(json!({"source": sid, "repair": true, "place": "keep"})),
            None,
        )
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
            obj(
                json!({"body": mesh, "plane": {"origin": [0, 0, 0], "normal": [0, 0, 1]},
                       "keep": "both", "connector": "none"}),
            ),
            None,
        )
        .map_err(|e| e.0)?;
        Ok(secs(t))
    })?);
    out.push(time_op(|| {
        let (mut d, mesh) = doc_with_mesh(&broken, false)?;
        let t = Instant::now();
        d.add_feature("repair", obj(json!({"body": mesh, "mode": "quick"})), None)
            .map_err(|e| e.0)?;
        Ok(secs(t))
    })?);
    let ring = json!([{"id": "r", "type": "polyline", "closed": true, "pts": ngon(30.0, 0.0, 10.0, 200)}]);
    out.push(time_op(|| {
        let (mut d, sk) = with_sketch(ring.clone(), "XY")?;
        let t = Instant::now();
        d.add_feature("extrude", obj(json!({"sketch": sk, "distance": 20})), None)
            .map_err(|e| e.0)?;
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
        let pts: Vec<Value> = (0..40)
            .map(|i| json!([5.0 * (i as f64 * 0.3).sin(), 2.0 * i as f64]))
            .collect();
        let path = d
            .add_feature(
                "sketch",
                obj(json!({"plane": "XZ", "entities": [{"id": "p", "type": "polyline", "closed": false, "pts": pts}]})),
                None,
            )
            .map_err(|e| e.0)?;
        let t = Instant::now();
        d.add_feature("sweep", obj(json!({"profile": prof, "path": path})), None)
            .map_err(|e| e.0)?;
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

    // --- batch 4/5 rows ---
    out.push(time_op(|| {
        // Same mesh+options would hit the 6-entry fins LRU across the 3 median runs.
        stl_engine::features::common::fins::clear_caches();
        let (mut d, mesh) = with_fins_doc(data, fins.clone())?;
        let t = Instant::now();
        d.add_feature(
            "supportfins",
            obj(json!({"body": mesh, "output": "join", "options": {}})),
            None,
        )
        .map_err(|e| e.0)?;
        Ok(secs(t))
    })?);
    out.push(time_op(|| {
        let (mut d, mesh) = with_oversized_for_bed(data)?;
        let t = Instant::now();
        d.add_feature(
            "bedsplit",
            obj(json!({
                "body": mesh,
                "printer": "bambu_a1mini",
                "connector": "none",
                "margin": 5,
                "optimize": true
            })),
            None,
        )
        .map_err(|e| e.0)?;
        Ok(secs(t))
    })?);
    out.push(time_op(|| {
        let (mut d, mesh) = doc_with_mesh(data, true)?;
        let t = Instant::now();
        d.add_feature("layflat", obj(json!({"bodies": [mesh], "auto": true})), None)
            .map_err(|e| e.0)?;
        Ok(secs(t))
    })?);
    out.push(time_op(|| {
        let (mut d, bodies) = with_six_copies(data)?;
        let t = Instant::now();
        d.add_feature(
            "plate",
            obj(json!({
                "bodies": bodies,
                "printer": "bambu_x1",
                "gap": 5,
                "rotate": false
            })),
            None,
        )
        .map_err(|e| e.0)?;
        Ok(secs(t))
    })?);
    out.push(time_op(|| {
        let (mut d, mesh) = with_pattern_source(data)?;
        let t = Instant::now();
        d.add_feature(
            "pattern",
            obj(json!({
                "bodies": [mesh],
                "kind": "circular",
                "center": [0, 0, 0],
                "axis": [0, 0, 1],
                "angle": 360,
                "count": 12,
                "join": true
            })),
            None,
        )
        .map_err(|e| e.0)?;
        // 1.x Manifold leaves the boolean lazy until volume/mesh; force the same work here.
        let _ = d
            .state
            .body(&mesh)
            .map_err(|e| e.0)?
            .require_solid()
            .map_err(|e| e.0)?
            .volume();
        Ok(secs(t))
    })?);
    out.push(time_op(|| {
        let mut d = Document::new(Registry::builtin());
        let t = Instant::now();
        d.add_feature(
            "thread",
            obj(json!({
                "kind": "external",
                "diameter": 8,
                "pitch": 1.25,
                "length": 30,
                "tolerance": 0.2,
                "hand": "right",
                "chamfer": true,
                "plane": "XY",
                "offset": [0, 0, 0],
                "operation": "new"
            })),
            None,
        )
        .map_err(|e| e.0)?;
        Ok(secs(t))
    })?);
    out.push(time_op(|| {
        let (mut d, mesh) = doc_with_mesh(&hole_data, true)?;
        let t = Instant::now();
        d.add_feature(
            "resizehole",
            obj(json!({"body": mesh, "point": [16, 15, 7.5], "diameter": 6})),
            None,
        )
        .map_err(|e| e.0)?;
        Ok(secs(t))
    })?);
    out.push(time_op(|| {
        stl_engine::features::common::fins::clear_caches();
        let (mut d, mesh) = with_fins_doc(data, fins.clone())?;
        let t = Instant::now();
        d.add_feature(
            "supportwall",
            obj(json!({
                "body": mesh,
                "walls": [{"shape": "line", "points": [[-5, -10, -5], [5, -10, -5]]}],
                "output": "join",
                "options": {}
            })),
            None,
        )
        .map_err(|e| e.0)?;
        Ok(secs(t))
    })?);

    // --- M3 export rows: Document::export to a temp file (same body as 1.x Document) ---
    for (fmt, ext) in [
        (ExportFormat::Stl, "stl"),
        (ExportFormat::ThreeMf, "3mf"),
        (ExportFormat::Obj, "obj"),
        (ExportFormat::Step, "step"),
    ] {
        out.push(time_op(|| {
            let (d, _mesh) = doc_with_mesh(data, true)?;
            let path = std::env::temp_dir().join(format!("stl-editor-bench-export-{ext}"));
            let t = Instant::now();
            d.export(&path, fmt, None, None).map_err(|e| e.0)?;
            let s = secs(t);
            let _ = std::fs::remove_file(&path);
            Ok(s)
        })?);
    }
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
    let line = text
        .lines()
        .rev()
        .find(|l| l.trim_start().starts_with('{'))
        .unwrap_or("");
    serde_json::from_str(line).map_err(|e| format!("bad output from bench_1x.py ({e}): {text}"))
}

/// A 2.0 row counts as slower than the saved run when it took over 20% longer and over 30 ms
/// longer. Rows move by 10 to 20% between quiet runs of the same code, and the short rows by more.
const SLOWER_RATIO: f64 = 1.2;
const SLOWER_BY_S: f64 = 0.03;

/// Where `--save` and `--compare` keep the saved run when no file is named (git-ignored: the
/// times belong to this PC).
const SAVED_RUN: &str = "dev/bench/saved_run.json";

struct Opts {
    save: Option<PathBuf>,
    compare: Option<PathBuf>,
    skip_1x: bool,
}

/// `--save [file]`, `--compare [file]`, `--skip-1x`.
fn parse_args(args: &[String], root: &Path) -> Result<Opts, String> {
    let mut o = Opts {
        save: None,
        compare: None,
        skip_1x: false,
    };
    let mut i = 0;
    while i < args.len() {
        // the file is the next argument unless that is another option
        let file = |i: &mut usize| match args.get(*i + 1) {
            Some(a) if !a.starts_with("--") => {
                *i += 1;
                PathBuf::from(a)
            }
            _ => root.join(SAVED_RUN),
        };
        match args[i].as_str() {
            "--save" => o.save = Some(file(&mut i)),
            "--compare" => o.compare = Some(file(&mut i)),
            "--skip-1x" => o.skip_1x = true,
            other => {
                return Err(format!(
                    "unknown option {other}; use --save [file], --compare [file], --skip-1x"
                ));
            }
        }
        i += 1;
    }
    Ok(o)
}

/// True when `now` is slower than `saved` by more than run-to-run noise.
fn slower_than_saved(saved: f64, now: f64) -> bool {
    now > saved * SLOWER_RATIO && now - saved > SLOWER_BY_S
}

/// True when both paths name one file, however each was written.
fn same_file(a: &Path, b: &Path) -> bool {
    a == b || matches!((a.canonicalize(), b.canonicalize()), (Ok(a), Ok(b)) if a == b)
}

/// True when `--save` must leave its file alone: the run was slower than the saved run in that
/// same file, and writing the slow times over it would make the next `--compare` pass.
fn keep_saved_run(save: &Path, compare: Option<&Path>, slower: bool) -> bool {
    slower && compare.is_some_and(|c| same_file(save, c))
}

/// The saved 2.0 seconds by row key, and the note of what was saved (commit).
fn read_saved(path: &Path) -> Result<(Map<String, Value>, String), String> {
    let text = std::fs::read_to_string(path).map_err(|e| {
        format!(
            "no saved run at {} ({e}); make one with `xtask -- bench --save`",
            path.display()
        )
    })?;
    let v: Value = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let times = v["times"]
        .as_object()
        .cloned()
        .ok_or_else(|| format!("{}: no \"times\"", path.display()))?;
    Ok((times, v["commit"].as_str().unwrap_or("unknown commit").to_string()))
}

/// The commit the code was built from, with `+changes` when files differ from it.
fn commit_note(root: &Path) -> String {
    let git = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .current_dir(root)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
    };
    let Some(hash) = git(&["rev-parse", "--short", "HEAD"]) else {
        return "unknown commit".into();
    };
    match git(&["status", "--porcelain", "--untracked-files=no"]) {
        Some(s) if !s.is_empty() => format!("{hash}+changes"),
        _ => hash,
    }
}

fn write_saved(path: &Path, root: &Path, rust: &[f64]) -> Result<(), String> {
    let times: Map<String, Value> = NAMES
        .iter()
        .zip(rust)
        .map(|((key, _), s)| (key.to_string(), json!(s)))
        .collect();
    let v = json!({"commit": commit_note(root), "runs": RUNS, "times": times});
    let text = serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?;
    std::fs::write(path, text + "\n").map_err(|e| format!("{}: {e}", path.display()))
}

/// Run the benchmark. `Ok(true)` when no 2.0 row is slower than 1.x and, with `--compare`, none
/// is slower than the saved run.
///
/// `--save [file]` keeps this run's 2.0 times; `--compare [file]` sets a later run beside them,
/// so a change that makes 2.0 slower than it was shows even where 2.0 is still far ahead of 1.x.
/// With both on one file, a run that is slower than the saved one is not saved over it.
/// `--skip-1x` leaves the 1.x timing out (it needs Python and takes the longer half of the run).
pub fn run(args: impl Iterator<Item = String>) -> Result<bool, String> {
    let root: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let args: Vec<String> = args.collect();
    let opts = parse_args(&args, &root)?;
    // read the saved run first: a missing file should stop the run before minutes of timing
    let saved = match &opts.compare {
        Some(p) => Some(read_saved(p)?),
        None => None,
    };
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
    let rust = time_rust(&root, &data)?;
    let py = if opts.skip_1x {
        None
    } else {
        println!("bench: timing 1.x (median of {RUNS})...");
        Some(time_python(&root)?)
    };

    let mut ok = true;
    if let Some(py) = &py {
        println!("\n{:<38} {:>9} {:>9} {:>8}", "operation", "1.x s", "2.0 s", "ratio");
        for ((key, label), r) in NAMES.iter().zip(&rust) {
            let p = py[key]
                .as_f64()
                .ok_or_else(|| format!("bench_1x.py gave no \"{key}\""))?;
            let ratio = r / p;
            let flag = if ratio > 1.0 { "  SLOWER" } else { "" };
            println!("{label:<38} {p:>9.3} {r:>9.3} {ratio:>8.2}{flag}");
            ok &= ratio <= 1.0;
        }
    }
    let mut slower_than_saved_run = false;
    if let Some((times, commit)) = &saved {
        println!("\n2.0 now ({}) beside the saved run ({commit}):", commit_note(&root));
        println!("{:<38} {:>9} {:>9} {:>8}", "operation", "saved s", "now s", "change");
        for ((key, label), r) in NAMES.iter().zip(&rust) {
            // a row added since the run was saved has nothing to stand beside
            let Some(s) = times.get(*key).and_then(Value::as_f64) else {
                println!("{label:<38} {:>9} {r:>9.3} {:>8}", "-", "new");
                continue;
            };
            let slower = slower_than_saved(s, *r);
            let flag = if slower { "  SLOWER" } else { "" };
            println!("{label:<38} {s:>9.3} {r:>9.3} {:>+7.0}%{flag}", (r / s - 1.0) * 100.0);
            slower_than_saved_run |= slower;
        }
        ok &= !slower_than_saved_run;
    } else if py.is_none() {
        println!("\n{:<38} {:>9}", "operation", "2.0 s");
        for ((_, label), r) in NAMES.iter().zip(&rust) {
            println!("{label:<38} {r:>9.3}");
        }
    }
    if let Some(p) = &opts.save {
        if keep_saved_run(p, opts.compare.as_deref(), slower_than_saved_run) {
            println!(
                "\nbench: not saved: this run is slower than the saved run in {}, which stays as it was",
                p.display()
            );
        } else {
            write_saved(p, &root, &rust)?;
            println!("\nbench: saved this run's 2.0 times to {}", p.display());
        }
    }
    Ok(ok)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_row_is_slower_only_beyond_the_noise() {
        // 1.000 s saved: 19% over is noise, 25% over is slower
        assert!(!slower_than_saved(1.0, 1.19));
        assert!(slower_than_saved(1.0, 1.25));
        // a 10 ms row that doubles is 10 ms slower: too short to tell
        assert!(!slower_than_saved(0.010, 0.020));
        // a 100 ms row that goes to 140 ms is over both limits
        assert!(slower_than_saved(0.100, 0.140));
        // faster and equal are never slower
        assert!(!slower_than_saved(2.0, 1.0));
        assert!(!slower_than_saved(2.0, 2.0));
    }

    #[test]
    fn options_take_a_file_or_the_usual_one() {
        let root = Path::new("root");
        let o = parse_args(&args(&["--save", "--skip-1x"]), root).unwrap();
        assert_eq!(o.save, Some(root.join(SAVED_RUN)));
        assert!(o.skip_1x && o.compare.is_none());
        let o = parse_args(&args(&["--compare", "before.json", "--save", "after.json"]), root).unwrap();
        assert_eq!(o.compare, Some(PathBuf::from("before.json")));
        assert_eq!(o.save, Some(PathBuf::from("after.json")));
        assert!(!o.skip_1x);
        assert!(parse_args(&args(&["--fast"]), root).is_err());
        let o = parse_args(&[], root).unwrap();
        assert!(o.save.is_none() && o.compare.is_none() && !o.skip_1x);
    }

    #[test]
    fn a_slower_run_does_not_replace_the_run_it_was_compared_with() {
        let saved = Path::new("root").join(SAVED_RUN);
        // slower than the saved run in the same file: keep the file
        assert!(keep_saved_run(&saved, Some(&saved), true));
        // not slower, or nothing compared: save as usual
        assert!(!keep_saved_run(&saved, Some(&saved), false));
        assert!(!keep_saved_run(&saved, None, false));
        // compared with one file and saved to another: both are kept, so save
        assert!(!keep_saved_run(
            Path::new("after.json"),
            Some(Path::new("before.json")),
            true
        ));
        // one file written two ways is still one file
        let path = std::env::temp_dir().join(format!("stl-bench-same-{}.json", std::process::id()));
        std::fs::write(&path, "{}").unwrap();
        let other = path.parent().unwrap().join(".").join(path.file_name().unwrap());
        let same = keep_saved_run(&other, Some(&path), true);
        let _ = std::fs::remove_file(&path);
        assert!(same);
    }

    #[test]
    fn a_saved_run_reads_back() {
        let path = std::env::temp_dir().join(format!("stl-bench-saved-{}.json", std::process::id()));
        let times: Vec<f64> = (0..NAMES.len()).map(|i| 0.5 + i as f64).collect();
        write_saved(&path, Path::new("."), &times).unwrap();
        let (read, commit) = read_saved(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(read.len(), NAMES.len());
        assert_eq!(read["import"].as_f64(), Some(0.5));
        assert_eq!(read["export_step"].as_f64(), Some(0.5 + (NAMES.len() - 1) as f64));
        assert!(!commit.is_empty());
        // a missing file says how to make one
        assert!(read_saved(&path).unwrap_err().contains("--save"));
    }
}
