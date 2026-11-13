//! Support Fins through the desktop webview host while the real app (kittest harness) is open.
//!
//! Matches `rust/tests/parity/fins_golden.json` the same way milestone 2's
//! `stl-platform` / `browser_fins` tests do.
#![cfg(not(target_arch = "wasm32"))]
#![cfg(any(windows, target_os = "linux"))]

use std::path::{Path, PathBuf};
use std::time::Instant;

use serde_json::Value;
use stl_engine::{FinsHost, HostKind};
use stl_platform::fins::WebviewHost;

#[path = "ui/harness.rs"]
mod harness;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn engine_js() -> PathBuf {
    repo().join("web/vendor/support-fins/engine.iife.js")
}

fn golden() -> serde_json::Map<String, Value> {
    let p = repo().join("rust/tests/parity/fins_golden.json");
    match serde_json::from_str(&std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))).unwrap() {
        Value::Object(m) => m,
        _ => panic!("fins_golden.json is not an object"),
    }
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn b64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        out.push(B64[(n >> 18) as usize & 63] as char);
        out.push(B64[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 { B64[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if c.len() > 2 { B64[n as usize & 63] as char } else { '=' });
    }
    out
}

fn load_model(name: &str) -> (Vec<f64>, Vec<u32>) {
    let b = std::fs::read(repo().join("rust/tests/models").join(name)).unwrap();
    let n = u32::from_le_bytes(b[80..84].try_into().unwrap()) as usize;
    assert_eq!(b.len(), 84 + 50 * n, "{name} is not a binary STL");
    let mut pos = Vec::with_capacity(n * 9);
    for t in 0..n {
        let rec = &b[84 + 50 * t..];
        for k in 0..9 {
            let o = 12 + 4 * k;
            pos.push(f32::from_le_bytes(rec[o..o + 4].try_into().unwrap()) as f64);
        }
    }
    let tris: Vec<u32> = (0..(n * 3) as u32).collect();
    stl_mesh::weld(&pos, &tris, 1e-5)
}

fn soup(v: &[f64], f: &[u32]) -> String {
    let mut bytes = Vec::with_capacity(f.len() * 24);
    for &i in f {
        for k in 0..3 {
            bytes.extend_from_slice(&v[3 * i as usize + k].to_le_bytes());
        }
    }
    b64_encode(&bytes)
}

fn js(s: &str) -> String {
    serde_json::to_string(s).unwrap()
}

fn options(host: &dyn FinsHost, values: &Value) -> String {
    host.call("optionsFromDialogJson", &[&js(&values.to_string())]).expect("optionsFromDialogJson")
}

struct Outcome {
    stats: Value,
    pieces: usize,
    report: String,
    over: usize,
    small: usize,
}

fn run_case(host: &dyn FinsHost, case: &Value) -> Outcome {
    let (v, f) = load_model(case["model"].as_str().unwrap());
    let opts = options(host, &case["options"]);
    let raw: Value =
        serde_json::from_str(&host.call("computeFinsB64", &[&js(&soup(&v, &f)), &js(&opts)]).expect("computeFinsB64")).unwrap();
    let report = host.call("reportLine", &[&raw["stats"].to_string()]).expect("reportLine");
    Outcome {
        stats: raw["stats"].clone(),
        pieces: raw["pieces"].as_array().unwrap().len(),
        report,
        over: raw["overFaces"].as_array().unwrap().len(),
        small: raw["smallFaces"].as_array().unwrap().len(),
    }
}

fn compare(name: &str, want: &Value, got: &Outcome) -> Vec<String> {
    let mut bad = Vec::new();
    let ws = want["stats"].as_object().unwrap();
    let gs = got.stats.as_object().unwrap();
    let mut keys: Vec<&String> = ws.keys().chain(gs.keys()).collect();
    keys.sort();
    keys.dedup();
    for k in keys {
        let (a, b) = (ws.get(k).unwrap_or(&Value::Null), gs.get(k).unwrap_or(&Value::Null));
        let ok = match (a, b) {
            (Value::Number(x), Value::Number(y)) if x.is_f64() || y.is_f64() => {
                let (x, y) = (x.as_f64().unwrap(), y.as_f64().unwrap());
                (x - y).abs() <= 1e-6 * x.abs().max(y.abs())
            }
            _ => a == b,
        };
        if !ok {
            bad.push(format!("{name}: stats.{k}: want {a}, got {b}"));
        }
    }
    if want["pieces"].as_u64().unwrap() as usize != got.pieces {
        bad.push(format!("{name}: pieces: want {}, got {}", want["pieces"], got.pieces));
    }
    if want["report"].as_str().unwrap() != got.report {
        bad.push(format!("{name}: report: want {:?}, got {:?}", want["report"], got.report));
    }
    if want["over"].as_u64().unwrap() as usize != got.over || want["small"].as_u64().unwrap() as usize != got.small {
        bad.push(format!(
            "{name}: over/small: want {}/{}, got {}/{}",
            want["over"], want["small"], got.over, got.small
        ));
    }
    bad
}

/// The app window is open (kittest harness with a real NativeLink) and a Support Fins call
/// through stl-platform's webview host matches `fins_golden.json`.
#[test]
fn fins_in_app() {
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/projects/basic.stlx");
    let mut app = harness::app_harness(Some(project.as_path()));
    assert!(
        !app.state().bodies.is_empty(),
        "harness should have opened a project before the fins call"
    );

    let host = WebviewHost::start(&engine_js()).expect("the webview host should start on this machine");
    assert_eq!(host.host_kind(), HostKind::Webview);

    // Keep the app alive and pumping while the host runs (honest "window is open").
    app.run_steps(1);

    let g = golden();
    // One representative golden case (same pick as browser_fins); full suite is in stl-platform.
    let (name, case) = g
        .get_key_value("t_bracket_default")
        .unwrap_or_else(|| panic!("fins_golden.json missing t_bracket_default"));
    let t = Instant::now();
    let got = run_case(&host, case);
    eprintln!("fins_in_app {name}: {:.3} s", t.elapsed().as_secs_f64());
    app.run_steps(1);
    assert!(!app.state().bodies.is_empty(), "app still open after fins call");
    let bad = compare(name, case, &got);
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}
