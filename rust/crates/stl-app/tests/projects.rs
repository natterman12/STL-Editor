//! Open each 1.x project fixture in the 2.0 app and check views against 1.x Document.
//!
//! Fixtures: `rust/tests/projects/{basic,unicode,extra_keys}.stlx` from
//! `dev/parity/make_project_fixtures.py`. Expected JSON comes from
//! `dev/m4a_shots/dump_1x.py` (PYTHON, else `py -3.14` on Windows, else python3).
#![cfg(not(target_arch = "wasm32"))]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use serde_json::Value;
use stl_app::engine_link::native::NativeLink;
use stl_app::engine_link::{Command as EngCmd, EngineLink, Update};

#[path = "ui/harness.rs"]
mod harness;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/projects")
}

/// Interpreter with the 1.x packages: `PYTHON` env, else `py -3.14` on Windows, else `python3`.
fn python() -> Option<Vec<String>> {
    let mut candidates: Vec<Vec<String>> = Vec::new();
    if let Ok(p) = std::env::var("PYTHON") {
        let p = p.trim();
        if !p.is_empty() {
            candidates.push(vec![p.to_string()]);
        }
    }
    if cfg!(windows) {
        candidates.push(vec!["py".into(), "-3.14".into()]);
    }
    candidates.push(vec!["python3".into()]);

    for cmd in candidates {
        let ok = Command::new(&cmd[0])
            .args(&cmd[1..])
            .args(["-c", "import numpy, trimesh, shapely"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if ok {
            return Some(cmd);
        }
    }
    None
}

fn dump_1x(path: &Path) -> Value {
    let py = python().expect("Python with the 1.x packages is required for projects.rs");
    let script = repo().join("dev/m4a_shots/dump_1x.py");
    let out = Command::new(&py[0])
        .args(&py[1..])
        .arg(&script)
        .arg(path)
        .current_dir(repo())
        .output()
        .expect("dump_1x.py runs");
    assert!(
        out.status.success(),
        "dump_1x failed for {}:\n{}{}",
        path.display(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("dump_1x JSON")
}

fn settle_link(link: &NativeLink) {
    let end = Instant::now() + Duration::from_secs(120);
    loop {
        let _ = link.poll();
        if !link.is_busy() {
            let more = link.poll();
            if more.is_empty() && !link.is_busy() {
                return;
            }
        }
        assert!(Instant::now() < end, "engine never went idle");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn compare_project(name: &str, want: &Value, app: &stl_app::app::StlApp) -> Vec<String> {
    let mut bad = Vec::new();

    let want_bodies = want["bodies"].as_array().unwrap();
    if app.bodies.len() != want_bodies.len() {
        bad.push(format!(
            "{name}: body count want {}, got {}",
            want_bodies.len(),
            app.bodies.len()
        ));
    }
    for wb in want_bodies {
        let id = wb["id"].as_str().unwrap();
        let Some(got) = app.bodies.iter().find(|b| b.id == id) else {
            bad.push(format!("{name}: missing body {id}"));
            continue;
        };
        if got.name != wb["name"].as_str().unwrap() {
            bad.push(format!("{name}: body {id} name want {:?}, got {:?}", wb["name"], got.name));
        }
        let want_color = wb.get("color").and_then(Value::as_str);
        let got_color = got.color.as_deref();
        if want_color != got_color {
            bad.push(format!("{name}: body {id} color want {want_color:?}, got {got_color:?}"));
        }
        if got.hidden != wb["hidden"].as_bool().unwrap_or(false) {
            bad.push(format!("{name}: body {id} hidden want {}, got {}", wb["hidden"], got.hidden));
        }
    }

    // 1.x Document.state.planes is user planes only; 2.0 may also list origin XY/XZ/YZ.
    let want_planes = want["planes"].as_array().unwrap();
    let base = ["XY", "XZ", "YZ"];
    let got_user: Vec<_> = app.planes.iter().filter(|p| !base.contains(&p.id.as_str())).collect();
    if got_user.len() != want_planes.len() {
        bad.push(format!(
            "{name}: user plane count want {}, got {} (app.planes={})",
            want_planes.len(),
            got_user.len(),
            app.planes.len()
        ));
    }
    for wp in want_planes {
        let id = wp["id"].as_str().unwrap();
        match app.planes.iter().find(|p| p.id == id) {
            Some(got) if got.name != wp["name"].as_str().unwrap() => {
                bad.push(format!("{name}: plane {id} name want {:?}, got {:?}", wp["name"], got.name));
            }
            None => bad.push(format!("{name}: missing user plane {id}")),
            _ => {}
        }
    }

    let want_sk = want["sketches"].as_array().unwrap();
    if app.sketches.len() != want_sk.len() {
        bad.push(format!("{name}: sketch count want {}, got {}", want_sk.len(), app.sketches.len()));
    }
    for ws in want_sk {
        let id = ws["id"].as_str().unwrap();
        let Some(got) = app.sketches.iter().find(|s| s.id == id) else {
            bad.push(format!("{name}: missing sketch {id}"));
            continue;
        };
        if got.name != ws["name"].as_str().unwrap() {
            bad.push(format!("{name}: sketch {id} name want {:?}, got {:?}", ws["name"], got.name));
        }
        if got.plane != ws["plane"].as_str().unwrap_or("") {
            bad.push(format!("{name}: sketch {id} plane want {:?}, got {:?}", ws["plane"], got.plane));
        }
        if got.hidden != ws["hidden"].as_bool().unwrap_or(false) {
            bad.push(format!("{name}: sketch {id} hidden mismatch"));
        }
    }

    let wt = &want["timeline"];
    let want_marker = wt.get("marker").and_then(Value::as_u64).map(|n| n as usize);
    if app.timeline_view.marker != want_marker {
        bad.push(format!(
            "{name}: timeline marker want {want_marker:?}, got {:?}",
            app.timeline_view.marker
        ));
    }
    let want_items = wt["items"].as_array().unwrap();
    if app.timeline_view.items.len() != want_items.len() {
        bad.push(format!(
            "{name}: timeline len want {}, got {}",
            want_items.len(),
            app.timeline_view.items.len()
        ));
    }
    for (i, wi) in want_items.iter().enumerate() {
        let Some(gi) = app.timeline_view.items.get(i) else { break };
        if gi.id != wi["id"].as_str().unwrap() {
            bad.push(format!("{name}: timeline[{i}] id want {:?}, got {:?}", wi["id"], gi.id));
        }
        if gi.kind != wi["kind"].as_str().unwrap() {
            bad.push(format!("{name}: timeline[{i}] kind want {:?}, got {:?}", wi["kind"], gi.kind));
        }
        if gi.label != wi["label"].as_str().unwrap() {
            bad.push(format!("{name}: timeline[{i}] label want {:?}, got {:?}", wi["label"], gi.label));
        }
        if gi.suppressed != wi["suppressed"].as_bool().unwrap_or(false) {
            bad.push(format!("{name}: timeline[{i}] suppressed mismatch"));
        }
        if gi.active != wi["active"].as_bool().unwrap_or(true) {
            bad.push(format!("{name}: timeline[{i}] active mismatch"));
        }
        let want_err = wi.get("error").and_then(Value::as_str);
        let got_err = gi.error.as_deref();
        if want_err != got_err {
            bad.push(format!("{name}: timeline[{i}] error want {want_err:?}, got {got_err:?}"));
        }
    }

    let want_params = want["params"].as_array().unwrap();
    if app.param_views.len() != want_params.len() {
        bad.push(format!(
            "{name}: param count want {}, got {}",
            want_params.len(),
            app.param_views.len()
        ));
    }
    for wp in want_params {
        let pname = wp["name"].as_str().unwrap();
        let Some(got) = app.param_views.iter().find(|p| p.name == pname) else {
            bad.push(format!("{name}: missing param {pname}"));
            continue;
        };
        if got.expr != wp["expr"].as_str().unwrap() {
            bad.push(format!("{name}: param {pname} expr want {:?}, got {:?}", wp["expr"], got.expr));
        }
        if got.note != wp["note"].as_str().unwrap_or("") {
            bad.push(format!("{name}: param {pname} note want {:?}, got {:?}", wp["note"], got.note));
        }
    }

    bad
}

fn save_and_check_1x(name: &str, path: &Path, out_dir: &Path) {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let link = NativeLink::new();
    link.send(EngCmd::Open { name: name.to_string(), bytes });
    settle_link(&link);

    link.send(EngCmd::Save);
    let end = Instant::now() + Duration::from_secs(60);
    let mut saved = None;
    while Instant::now() < end {
        for u in link.poll() {
            if let Update::Saved { bytes } = u {
                saved = Some(bytes);
            }
        }
        if saved.is_some() && !link.is_busy() {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let saved = saved.unwrap_or_else(|| panic!("{name}: no Update::Saved"));
    let out = out_dir.join(name);
    std::fs::write(&out, &saved).unwrap();

    let py = python().expect("python for 1.x open check");
    let check = Command::new(&py[0])
        .args(&py[1..])
        .args([
            "-c",
            &format!(
                "import sys; sys.path.insert(0, r'{}'); from stleditor.document import Document; Document.open(r'{}'); print('ok')",
                repo().display(),
                out.display()
            ),
        ])
        .output()
        .expect("1.x Document.open");
    assert!(
        check.status.success(),
        "{name}: 1.x could not open the 2.0 save:\n{}{}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );
}

#[test]
fn projects_match_1x() {
    let Some(_py) = python() else {
        eprintln!(
            "projects_match_1x: no Python with the 1.x packages \
             (set PYTHON, or use py -3.14 / python3 with numpy+trimesh+shapely); skipped"
        );
        return;
    };

    let dir = fixtures_dir();
    let names = ["basic.stlx", "unicode.stlx", "extra_keys.stlx"];
    let out_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../rust/target/t10/projects_out");
    let _ = std::fs::remove_dir_all(&out_dir);
    std::fs::create_dir_all(&out_dir).unwrap();

    let mut all_bad = Vec::new();
    for name in names {
        let path = dir.join(name);
        assert!(path.is_file(), "missing fixture {}", path.display());
        let want = dump_1x(&path);
        let h = harness::app_harness(Some(path.as_path()));
        all_bad.extend(compare_project(name, &want, h.state()));
        save_and_check_1x(name, &path, &out_dir);
    }
    assert!(all_bad.is_empty(), "{}", all_bad.join("\n"));
}
