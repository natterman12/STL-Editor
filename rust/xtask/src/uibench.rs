//! `cargo run --release -p xtask -- uibench [--quick] [--results <file.md>]`
//!
//! Builds the release `stl-app` binary, runs it with `--bench <tmp.json>`, reads the JSON, and
//! appends a dated markdown table to `rust/tests/UI_BENCH.md` (or the `--results` file). It also
//! prints the rebuild's frame trace: where the slowest frames' time went.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

pub fn run(args: impl Iterator<Item = String>) -> Result<(), String> {
    let args: Vec<String> = args.collect();
    if args.iter().any(|a| a == "--time-features") {
        return time_features();
    }
    let quick = args.iter().any(|a| a == "--quick");
    let results = args.iter().position(|a| a == "--results").and_then(|i| args.get(i + 1)).map(PathBuf::from);

    let root = workspace_root()?;
    let md_path = results.unwrap_or_else(|| root.join("tests/UI_BENCH.md"));
    let out_json = std::env::temp_dir().join(format!("stl-uibench-{}.json", std::process::id()));

    build_app(&root)?;
    run_app(&root, &out_json, quick)?;

    let text = std::fs::read_to_string(&out_json).map_err(|e| format!("read {}: {e}", out_json.display()))?;
    let report: Value = serde_json::from_str(&text).map_err(|e| format!("parse bench JSON: {e}"))?;
    print_trace(&report);
    let table = format_table(&report)?;
    append_results(&md_path, &table)?;
    let _ = std::fs::remove_file(&out_json);
    println!("uibench: appended results to {}", md_path.display());
    Ok(())
}

/// Time a few real features on the bench mesh. Not part of the UI table.
/// `offset` 0.2 mm is already known to take minutes (it barely simplifies a 504k mesh
/// before the Minkowski sum); this tries coarser work that can finish in well under a minute.
fn time_features() -> Result<(), String> {
    let root = workspace_root()?;
    let repo = root.parent().unwrap_or(&root);
    let mesh = repo.join("dev/bench/test_mesh.stl");
    let bytes = std::fs::read(&mesh).map_err(|e| format!("read {}: {e}", mesh.display()))?;
    println!("uibench: timing features on {} ({} bytes)", mesh.display(), bytes.len());

    // Each try imports again so one feature does not feed the next.
    let tries: &[(&str, serde_json::Value)] = &[
        ("hollow", serde_json::json!({"thickness": 2.0, "resolution": "normal"})),
        ("hollow", serde_json::json!({"thickness": 2.0, "resolution": "fine"})),
        ("remesh", serde_json::json!({"edge": 2.0})),
        ("remesh", serde_json::json!({"edge": 1.0})),
    ];
    for (kind, extra) in tries {
        let label = format!("{kind} {extra}");
        match time_one(&bytes, kind, extra) {
            Ok(line) => println!("uibench: {label}: {line}"),
            Err(e) => {
                println!("uibench: {label}: {e}");
                // A timed-out feature is still running on this process's thread. Stop so it
                // cannot sit under the next try or the UI bench.
                if e.starts_with("timeout") {
                    return Err(e);
                }
            }
        }
    }
    Ok(())
}

fn time_one(bytes: &[u8], kind: &str, extra: &serde_json::Value) -> Result<String, String> {
    let bytes = bytes.to_vec();
    let kind_owned = kind.to_string();
    let extra = extra.clone();
    let kind = kind_owned.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name(format!("time-{kind}"))
        .spawn(move || {
            let started = std::time::Instant::now();
            let result = (|| {
                let mut doc = stl_engine::Document::new(stl_engine::Registry::builtin());
                let sid = doc.add_source("test_mesh.stl", bytes);
                let import = serde_json::json!({"source": sid, "repair": true, "place": "center"});
                let import = match import {
                    serde_json::Value::Object(m) => m,
                    _ => Default::default(),
                };
                let t_import = std::time::Instant::now();
                doc.add_feature("import", import, None).map_err(|e| e.0)?;
                let import_ms = t_import.elapsed().as_secs_f64() * 1000.0;
                let (id, tris, bbox) = doc
                    .state
                    .bodies
                    .iter()
                    .next()
                    .map(|(id, b)| {
                        let n = b.solid.as_ref().map(|s| s.num_tri()).or_else(|| b.mesh.as_ref().map(|m| m.triangles.len() / 3)).unwrap_or(0);
                        let bbox = b.solid.as_ref().map(|s| {
                            let a = s.bounding_box();
                            format!("[{:.1},{:.1},{:.1}]..[{:.1},{:.1},{:.1}]", a.min[0], a.min[1], a.min[2], a.max[0], a.max[1], a.max[2])
                        }).unwrap_or_else(|| "no-solid".into());
                        (id.clone(), n, bbox)
                    })
                    .ok_or_else(|| "import produced no body".to_string())?;
                let mut params = match extra {
                    serde_json::Value::Object(m) => m,
                    _ => Default::default(),
                };
                if kind == "offset" {
                    params.insert("bodies".into(), serde_json::json!([id]));
                } else {
                    params.insert("body".into(), serde_json::Value::String(id));
                }
                let t_feat = std::time::Instant::now();
                doc.add_feature(&kind, params, None).map_err(|e| e.0)?;
                let feat_ms = t_feat.elapsed().as_secs_f64() * 1000.0;
                let out_tris = doc
                    .state
                    .bodies
                    .values()
                    .next()
                    .map(|b| b.solid.as_ref().map(|s| s.num_tri()).or_else(|| b.mesh.as_ref().map(|m| m.triangles.len() / 3)).unwrap_or(0))
                    .unwrap_or(0);
                Ok::<_, String>(format!(
                    "import {import_ms:.0} ms, feature {feat_ms:.0} ms, tris {tris} -> {out_tris}, box {bbox}, load+feature {:.0} ms",
                    started.elapsed().as_secs_f64() * 1000.0
                ))
            })();
            let _ = tx.send(result);
        })
        .map_err(|e| e.to_string())?;
    match rx.recv_timeout(std::time::Duration::from_secs(45)) {
        Ok(r) => r,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Err(format!("timeout after 45 s ({kind_owned} still running)")),
        Err(e) => Err(e.to_string()),
    }
}

fn workspace_root() -> Result<PathBuf, String> {
    // xtask's manifest is rust/xtask; the rust workspace root is one up.
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    Ok(manifest.join("..").canonicalize().map_err(|e| e.to_string())?)
}

fn cargo() -> String {
    std::env::var("CARGO").unwrap_or_else(|_| "cargo".into())
}

fn build_app(root: &Path) -> Result<(), String> {
    let status = Command::new(cargo())
        .current_dir(root)
        .args(["build", "--release", "-p", "stl-app"])
        .status()
        .map_err(|e| format!("could not run cargo: {e}"))?;
    if !status.success() {
        return Err("cargo build -p stl-app --release failed".into());
    }
    Ok(())
}

fn app_bin(root: &Path) -> PathBuf {
    let target = std::env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| "target".into());
    let target = PathBuf::from(target);
    let base = if target.is_absolute() { target } else { root.join(target) };
    let name = if cfg!(windows) { "stl-app.exe" } else { "stl-app" };
    base.join("release").join(name)
}

fn run_app(root: &Path, out_json: &Path, quick: bool) -> Result<(), String> {
    let bin = app_bin(root);
    if !bin.is_file() {
        return Err(format!("missing binary {}", bin.display()));
    }
    let mut cmd = Command::new(&bin);
    // Repo root (parent of rust/) so `dev/bench/test_mesh.stl` resolves.
    let repo = root.parent().unwrap_or(root);
    cmd.current_dir(repo);
    cmd.arg("--bench").arg(out_json);
    if quick {
        cmd.arg("--quick");
    }
    let status = cmd.status().map_err(|e| format!("could not run {}: {e}", bin.display()))?;
    if !status.success() {
        return Err(format!("{} exited with {status}", bin.display()));
    }
    if !out_json.is_file() {
        return Err(format!("bench did not write {}", out_json.display()));
    }
    Ok(())
}

/// The rebuild's frame trace: the slowest frame and every one over 16.7 ms, split up, and
/// the most each part took in any rebuild frame.
fn print_trace(report: &Value) {
    let Some(t) = report.get("rebuild_trace") else { return };
    if let Some(r) = t.get("slowest").filter(|r| !r.is_null()) {
        println!("uibench: slowest rebuild frame {}", trace_line(r));
    }
    for r in t.get("over_16_7").and_then(|v| v.as_array()).into_iter().flatten() {
        println!("uibench: over 16.7 ms {}", trace_line(r));
    }
    for r in report.get("after_rebuild").and_then(|v| v.as_array()).into_iter().flatten() {
        println!("uibench: after the rebuild (not counted) {}", trace_line(r));
    }
    if let Some(m) = t.get("max") {
        println!(
            "uibench: most per rebuild frame: poll {:.2} (sync {:.2}), ui {:.2} (viewport frame {:.2}: collect {:.2}, promote {:.2}), paint {:.2}, other {:.2} (egui {:.2}, wait {:.2}) ms",
            num(m, "poll_ms"),
            num(m, "sync_ms"),
            num(m, "ui_ms"),
            num(m, "frame_ms"),
            num(m, "collect_ms"),
            num(m, "promote_ms"),
            num(m, "paint_ms"),
            num(m, "other_ms"),
            num(m, "egui_ms"),
            num(m, "wait_ms"),
        );
    }
}

fn trace_line(r: &Value) -> String {
    format!(
        "{:.2} ms at {:.0} ms: poll {:.2} (sync {:.2}), ui {:.2} (viewport frame {:.2}: collect {:.2}, promote {:.2} x{}), paint {:.2}{}{}, other {:.2} (egui {:.2}, wait {:.2}); {}",
        num(r, "dt_ms"),
        num(r, "at_ms"),
        num(r, "poll_ms"),
        num(r, "sync_ms"),
        num(r, "ui_ms"),
        num(r, "frame_ms"),
        num(r, "collect_ms"),
        num(r, "promote_ms"),
        uint(r, "promoted"),
        num(r, "paint_ms"),
        if r.get("upload_pending").and_then(|v| v.as_bool()).unwrap_or(false) { ", uploading" } else { "" },
        if uint(r, "uploads_done") > 0 { ", upload finished" } else { "" },
        num(r, "other_ms"),
        num(r, "egui_ms"),
        num(r, "wait_ms"),
        r.get("stage").and_then(|v| v.as_str()).unwrap_or("?"),
    )
}

/// One sentence for the results file: where the slowest rebuild frame's time went.
fn slowest_note(report: &Value) -> Option<String> {
    let r = report.pointer("/rebuild_trace/slowest").filter(|r| !r.is_null())?;
    Some(format!(
        "Slowest rebuild frame: {:.2} ms. The frame before it spent {:.2} ms on engine updates, {:.2} ms on the rest of the layout, {:.2} ms in the viewport's paint callback (upload and render), {:.2} ms on egui's own paint, and {:.2} ms waiting (the swapchain, the submit, and until the window thread ran again).",
        num(r, "dt_ms"),
        num(r, "poll_ms"),
        num(r, "ui_ms"),
        num(r, "paint_ms"),
        num(r, "egui_ms"),
        num(r, "wait_ms"),
    ))
}

fn scenario<'a>(report: &'a Value, key: &str) -> Result<&'a Value, String> {
    let path = format!("/scenarios/{key}");
    report.pointer(&path).ok_or_else(|| format!("bench JSON missing scenarios.{key}"))
}

fn num(v: &Value, key: &str) -> f64 {
    v.get(key).and_then(|x| x.as_f64()).unwrap_or(0.0)
}

fn uint(v: &Value, key: &str) -> u64 {
    v.get(key).and_then(|x| x.as_u64()).unwrap_or(0)
}

fn row(label: &str, s: &Value) -> String {
    format!(
        "| {label} | {} | {:.2} | {:.2} | {:.2} | {} | {} |",
        uint(s, "frames"),
        num(s, "median_ms"),
        num(s, "p99_ms"),
        num(s, "longest_ms"),
        uint(s, "idle_frames"),
        uint(s, "uploads"),
    )
}

pub fn format_table(report: &Value) -> Result<String, String> {
    let date = report.get("date").and_then(|d| d.as_str()).unwrap_or("unknown");
    let quick = report.get("quick").and_then(|q| q.as_bool()).unwrap_or(false);
    let launch = num(report, "launch_to_first_frame_ms");
    let note = if quick { " (quick)" } else { "" };
    let rebuild_note = report.get("rebuild_note").and_then(|s| s.as_str()).unwrap_or("");
    let idle_note = report.get("idle_note").and_then(|s| s.as_str()).unwrap_or("");
    let rebuild_kind = report.get("rebuild_kind").and_then(|s| s.as_str()).unwrap_or("rebuild");
    let rebuild_ms = num(report, "rebuild_ms");

    let orbit = scenario(report, "orbit")?;
    let pan = scenario(report, "pan")?;
    let zoom = scenario(report, "zoom")?;
    let idle = scenario(report, "idle")?;
    let fillet = scenario(report, "fillet_rebuild")?;

    let mut out = String::new();
    out.push_str(&format!("\n### {date}{note}\n\n"));
    if !rebuild_note.is_empty() {
        out.push_str(&format!("_{rebuild_note}_\n\n"));
    }
    if !idle_note.is_empty() {
        out.push_str(&format!("_{idle_note}_\n\n"));
    }
    if rebuild_ms > 0.0 {
        out.push_str(&format!(
            "_This run: {rebuild_kind} took {rebuild_ms:.0} ms from the command until the new mesh was uploaded and drawn. Frame times on that row are the orbiting frames in between._\n\n"
        ));
    }
    if let Some(n) = slowest_note(report) {
        out.push_str(&format!("_{n}_\n\n"));
    }
    out.push_str("| Scenario | frames | median ms | p99 ms | longest ms | idle frames | uploads |\n");
    out.push_str("| --- | ---: | ---: | ---: | ---: | ---: | ---: |\n");
    out.push_str(&row("orbit 10 s", orbit));
    out.push('\n');
    out.push_str(&row("pan 5 s", pan));
    out.push('\n');
    out.push_str(&row("zoom 5 s", zoom));
    out.push('\n');
    out.push_str(&row("idle 5 s", idle));
    out.push('\n');
    out.push_str(&row(&format!("{rebuild_kind} rebuild"), fillet));
    out.push('\n');
    out.push_str(&format!(
        "| launch to first frame | — | {launch:.2} | — | — | — | — |\n"
    ));
    Ok(out)
}

fn append_results(md_path: &Path, table: &str) -> Result<(), String> {
    let mut md = if md_path.is_file() {
        std::fs::read_to_string(md_path).map_err(|e| e.to_string())?
    } else {
        String::from("# UI bench (`xtask uibench`)\n\n## Results\n")
    };
    // Replace the placeholder under ## Results, or append at the end of the file.
    const PLACEHOLDER: &str = "_(none yet)_";
    if let Some(idx) = md.find(PLACEHOLDER) {
        md.replace_range(idx..idx + PLACEHOLDER.len(), table.trim_start());
    } else {
        if !md.contains("## Results") {
            md.push_str("\n## Results\n");
        }
        if !md.ends_with('\n') {
            md.push('\n');
        }
        md.push_str(table);
    }
    std::fs::write(md_path, md).map_err(|e| format!("write {}: {e}", md_path.display()))
}

#[allow(dead_code)]
fn today() -> String {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let days = (secs / 86400) as i64;
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}")
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn format_table_reads_scenarios() {
        let report = json!({
            "date": "2026-11-12",
            "quick": true,
            "launch_to_first_frame_ms": 42.5,
            "rebuild_kind": "hollow",
            "rebuild_ms": 4164.0,
            "rebuild_note": "note",
            "idle_note": "idle note",
            "scenarios": {
                "orbit": {"frames": 10, "median_ms": 1.0, "p99_ms": 2.0, "longest_ms": 3.0, "idle_frames": 0, "uploads": 1},
                "pan": {"frames": 5, "median_ms": 1.1, "p99_ms": 2.1, "longest_ms": 3.1, "idle_frames": 0, "uploads": 1},
                "zoom": {"frames": 5, "median_ms": 1.2, "p99_ms": 2.2, "longest_ms": 3.2, "idle_frames": 0, "uploads": 1},
                "idle": {"frames": 1, "median_ms": 0.0, "p99_ms": 0.0, "longest_ms": 0.0, "idle_frames": 1, "uploads": 1},
                "fillet_rebuild": {"frames": 4, "median_ms": 5.0, "p99_ms": 6.0, "longest_ms": 7.0, "idle_frames": 0, "uploads": 2},
            }
        });
        let t = format_table(&report).unwrap();
        assert!(t.contains("### 2026-11-12 (quick)"));
        assert!(t.contains("| orbit 10 s | 10 |"));
        assert!(t.contains("| hollow rebuild | 4 |"));
        assert!(t.contains("42.50"));
        assert!(t.contains("_note_"));
        assert!(t.contains("_idle note_"));
        assert!(t.contains("4164 ms"));
        assert!(!t.contains("Slowest rebuild frame"));
    }

    #[test]
    fn format_table_notes_slowest_rebuild_frame() {
        let s = json!({"frames": 1, "median_ms": 1.0, "p99_ms": 1.0, "longest_ms": 1.0, "idle_frames": 0, "uploads": 1});
        let report = json!({
            "date": "2026-11-12",
            "scenarios": {"orbit": s, "pan": s, "zoom": s, "idle": s, "fillet_rebuild": s},
            "rebuild_trace": {
                "slowest": {"dt_ms": 9.5, "at_ms": 10.0, "poll_ms": 1.25, "ui_ms": 2.0, "paint_ms": 3.0, "other_ms": 3.25, "egui_ms": 1.0, "wait_ms": 2.25},
                "over_16_7": [],
                "max": {}
            }
        });
        let t = format_table(&report).unwrap();
        assert!(t.contains("Slowest rebuild frame: 9.50 ms"), "{t}");
        assert!(t.contains("1.25 ms on engine updates"), "{t}");
        assert!(t.contains("3.00 ms in the viewport's paint callback"), "{t}");
        assert!(t.contains("2.25 ms waiting"), "{t}");
    }

    #[test]
    fn append_replaces_placeholder() {
        let dir = std::env::temp_dir().join(format!("uibench-md-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("UI_BENCH.md");
        std::fs::write(&path, "# UI bench\n\n## Results\n\n_(none yet)_\n").unwrap();
        append_results(&path, "\n### 2026-11-12\n\n| ok |\n").unwrap();
        let got = std::fs::read_to_string(&path).unwrap();
        assert!(got.contains("### 2026-11-12"));
        assert!(!got.contains("_(none yet)_"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
