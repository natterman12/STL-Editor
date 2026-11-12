//! Support Fins engine hosts against the 1.x engine golden (rust/tests/parity/fins_golden.json,
//! made by dev/parity/make_fins_golden.py with the bundled v1.0.5 engine).
//!
//! Matching rules (spec section 5): integer stats exact, float stats within 1e-6 relative,
//! piece count and report text exact. The triangle hash is reported, not required.
//!
//! Argument convention of `FinsHost::call`: every argument is JSON text.

use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use stl_engine::{FinsHost, HostKind};
use stl_platform::fins::BoaHost;
#[cfg(any(windows, target_os = "linux"))]
use stl_platform::fins::WebviewHost;

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn engine_js() -> PathBuf {
    repo().join("web/vendor/support-fins/engine.iife.js")
}

fn golden() -> serde_json::Map<String, Value> {
    let p = repo().join("rust/tests/parity/fins_golden.json");
    match serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap() {
        Value::Object(m) => m,
        _ => panic!("fins_golden.json is not an object"),
    }
}

// ---- small test helpers standing in for 1.x SF._soup / SF._options (Task 4 ports the real ones)

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

fn b64_decode(s: &str) -> Vec<u8> {
    let mut val = [255u8; 256];
    for (i, &c) in B64.iter().enumerate() {
        val[c as usize] = i as u8;
    }
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    let (mut acc, mut bits) = (0u32, 0);
    for &c in s.as_bytes() {
        let v = val[c as usize];
        if v == 255 {
            continue;
        }
        acc = acc << 6 | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    out
}

/// 1.x `G.read_stl_bytes` (binary) then `G.weld`.
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

/// 1.x `SF._soup`: base64 of the `<f8` triangle soup `v[f]`.
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

/// 1.x `SF._options(values)`.
fn options(host: &dyn FinsHost, values: &Value) -> String {
    host.call("optionsFromDialogJson", &[&js(&values.to_string())]).expect("optionsFromDialogJson")
}

struct Outcome {
    stats: Value,
    pieces: usize,
    report: String,
    sha: String,
    over: usize,
    small: usize,
}

fn run_case(host: &dyn FinsHost, case: &Value) -> Outcome {
    let (v, f) = load_model(case["model"].as_str().unwrap());
    let opts = options(host, &case["options"]);
    let raw: Value =
        serde_json::from_str(&host.call("computeFinsB64", &[&js(&soup(&v, &f)), &js(&opts)]).expect("computeFinsB64"))
            .unwrap();
    let report = host.call("reportLine", &[&raw["stats"].to_string()]).expect("reportLine");
    let tri = b64_decode(raw["triangles"].as_str().unwrap());
    Outcome {
        stats: raw["stats"].clone(),
        pieces: raw["pieces"].as_array().unwrap().len(),
        report,
        sha: format!("{:x}", Sha256::digest(&tri)),
        over: raw["overFaces"].as_array().unwrap().len(),
        small: raw["smallFaces"].as_array().unwrap().len(),
    }
}

/// Spec section 5 rules. Returns the problems found (empty = match).
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
        bad.push(format!("{name}: over/small faces: want {}/{}, got {}/{}", want["over"], want["small"], got.over, got.small));
    }
    bad
}

#[cfg(any(windows, target_os = "linux"))]
fn webview() -> WebviewHost {
    WebviewHost::start(&engine_js()).expect("the webview host should start on this machine")
}

#[cfg(any(windows, target_os = "linux"))]
#[test]
fn webview_matches_fins_golden() {
    let host = webview();
    assert_eq!(host.host_kind(), HostKind::Webview);
    let mut bad = Vec::new();
    for (name, case) in golden() {
        let t = Instant::now();
        let got = run_case(&host, &case);
        let same_tris = got.sha == case["triangles_sha256"].as_str().unwrap();
        println!("webview {name}: {:.3} s, triangles {}", t.elapsed().as_secs_f64(), if same_tris { "identical" } else { "differ" });
        bad.extend(compare(&name, &case, &got));
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

#[test]
fn boa_matches_fins_golden_t_bracket() {
    let t = Instant::now();
    let host = BoaHost::start(&engine_js()).expect("Boa host");
    println!("boa: engine loaded in {:.3} s", t.elapsed().as_secs_f64());
    assert_eq!(host.host_kind(), HostKind::Boa);
    let g = golden();
    let mut bad = Vec::new();
    let mut n = 0;
    // One case: Boa needs over a minute per T-bracket build (spec 6: "one fallback (Boa) case on the T-bracket").
    for (name, case) in g.iter().filter(|(k, _)| *k == "t_bracket_default") {
        let t = Instant::now();
        let got = run_case(&host, case);
        println!("boa {name}: {:.3} s, triangles {}", t.elapsed().as_secs_f64(),
            if got.sha == case["triangles_sha256"].as_str().unwrap() { "identical" } else { "differ" });
        bad.extend(compare(name, case, &got));
        n += 1;
    }
    assert!(n > 0, "no t_bracket cases in the golden");
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

#[cfg(any(windows, target_os = "linux"))]
#[test]
fn engine_errors_are_reported() {
    let host = BoaHost::start(&engine_js()).unwrap();
    assert!(host.call("noSuchFunction", &[]).is_err());
    let w = webview();
    assert!(w.call("noSuchFunction", &[]).is_err());
    // Bad JSON in an argument is an error, not a panic.
    assert!(w.call("reportLine", &["{not json"]).is_err());
    // The host still works afterwards.
    assert_eq!(w.call("reportLine", &[r#"{"braces":2,"tines":1}"#]).unwrap(), "2 walls, 1 tine");
}

#[cfg(windows)]
#[test]
fn webview_crash_recovers() {
    let host = webview();
    let g = golden();
    let case = &g["t_bracket_default"];
    assert!(compare("before", case, &run_case(&host, case)).is_empty());
    let pid = host.browser_pid().expect("WebView2 browser process id");
    let st = std::process::Command::new("taskkill").args(["/F", "/T", "/PID", &pid.to_string()]).output().unwrap();
    assert!(st.status.success(), "taskkill failed: {}", String::from_utf8_lossy(&st.stderr));
    let t = Instant::now();
    let err = host.call("reportLine", &[r#"{"braces":1}"#]);
    println!("crash detected in {:.3} s: {err:?}", t.elapsed().as_secs_f64());
    assert_eq!(err, Err("Support Fins: the engine stopped responding.".to_string()));
    assert!(t.elapsed().as_secs() < 30, "the dead host should be noticed quickly");
    // The next call starts a fresh host and is correct.
    assert!(compare("after", case, &run_case(&host, case)).is_empty());
    assert_ne!(host.browser_pid(), Some(pid));
}

/// The UI thread ending (as a crash would) must not leave the host dead for good: the call in
/// flight gets the error, the next starts a new UI thread and webview. Runs in a child process
/// of this test binary, so stopping the shared UI thread can't disturb the other tests.
#[cfg(any(windows, target_os = "linux"))]
#[test]
fn ui_thread_restarts() {
    if std::env::var_os("SF_UI_RESTART_CHILD").is_none() {
        let out = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["ui_thread_restarts", "--exact", "--nocapture", "--test-threads=1"])
            .env("SF_UI_RESTART_CHILD", "1")
            .output()
            .unwrap();
        let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
        println!("{text}");
        assert!(out.status.success() && text.contains("1 passed"), "child run failed:\n{text}");
        return;
    }
    const LINE: &str = r#"{"braces":2,"tines":1}"#;
    let host = webview();
    assert_eq!(host.call("reportLine", &[LINE]).as_deref(), Ok("2 walls, 1 tine"));
    WebviewHost::stop_ui_thread_for_tests();
    let t = Instant::now();
    assert_eq!(host.call("reportLine", &[LINE]), Err(stl_platform::fins::STOPPED.to_string()));
    assert!(t.elapsed().as_secs() < 10, "a dead UI thread should be noticed at once");
    // Windows only: on Linux GTK can't move to a second thread, so the host stays failed there.
    #[cfg(windows)]
    {
        let t = Instant::now();
        assert_eq!(host.call("reportLine", &[LINE]).as_deref(), Ok("2 walls, 1 tine"));
        println!("fresh UI thread and webview in {:.3} s", t.elapsed().as_secs_f64());
        // A host started afterwards shares the new UI thread.
        assert_eq!(webview().call("reportLine", &[LINE]).as_deref(), Ok("2 walls, 1 tine"));
    }
}

#[cfg(any(windows, target_os = "linux"))]
#[test]
fn concurrent_calls_serialize() {
    let host: Arc<dyn FinsHost> = Arc::new(webview());
    let g = golden();
    let cases = ["t_bracket_default", "lipped_shelf_full"];
    let handles: Vec<_> = cases
        .iter()
        .map(|&name| {
            let host = host.clone();
            let case = g[name].clone();
            std::thread::spawn(move || {
                let mut bad = Vec::new();
                for _ in 0..3 {
                    bad.extend(compare(name, &case, &run_case(&*host, &case)));
                }
                bad
            })
        })
        .collect();
    let bad: Vec<String> = handles.into_iter().flat_map(|h| h.join().unwrap()).collect();
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

#[cfg(any(windows, target_os = "linux"))]
#[test]
fn has_reports_extras() {
    let info: Value = serde_json::from_str(
        &std::fs::read_to_string(repo().join("web/vendor/support-fins/VERSION.json")).unwrap(),
    )
    .unwrap();
    let extras: Vec<&str> = info["extras"].as_array().unwrap().iter().map(|e| e.as_str().unwrap()).collect();
    assert!(!extras.is_empty());
    let w = webview();
    let b = BoaHost::start(&engine_js()).unwrap();
    for host in [&w as &dyn FinsHost, &b] {
        for e in &extras {
            assert!(host.has(e), "{:?}: has({e}) is false", host.host_kind());
        }
        assert!(host.has("computeFinsB64"));
        assert!(!host.has("noSuchFunction"));
    }
}

#[test]
fn start_fins_host_picks_webview() {
    let host = stl_platform::fins::start_fins_host(&engine_js());
    let want = if cfg!(target_os = "macos") { HostKind::Boa } else { HostKind::Webview };
    assert_eq!(host.host_kind(), want);
    assert!(host.has("computeFinsB64"));
}

#[test]
fn missing_engine_file_falls_back_to_null() {
    let host = stl_platform::fins::start_fins_host(std::path::Path::new("no/such/engine.iife.js"));
    assert_eq!(host.host_kind(), HostKind::None);
    assert!(host.call("reportLine", &["{}"]).is_err());
}
