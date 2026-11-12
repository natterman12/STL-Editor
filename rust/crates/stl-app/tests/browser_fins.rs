//! Headless Chrome: bundled Support Fins engine via [`stl_app::web::fins::BrowserHost`].
#![cfg(target_arch = "wasm32")]

use serde_json::Value;
use sha2::{Digest, Sha256};
use stl_app::web::fins::BrowserHost;
use stl_engine::{FinsHost, HostKind};
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

const ENGINE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../web/vendor/support-fins/engine.iife.js"
));
const GOLDEN: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/parity/fins_golden.json"));
const T_BRACKET: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/models/t_bracket.stl"));

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

fn load_model(bytes: &[u8]) -> (Vec<f64>, Vec<u32>) {
    let n = u32::from_le_bytes(bytes[80..84].try_into().unwrap()) as usize;
    assert_eq!(bytes.len(), 84 + 50 * n, "not a binary STL");
    let mut pos = Vec::with_capacity(n * 9);
    for t in 0..n {
        let rec = &bytes[84 + 50 * t..];
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

fn options(host: &BrowserHost, values: &Value) -> String {
    host.call("optionsFromDialogJson", &[&js(&values.to_string())]).expect("optionsFromDialogJson")
}

struct Outcome {
    stats: Value,
    pieces: usize,
    report: String,
    over: usize,
    small: usize,
}

fn compare(want: &Value, got: &Outcome) -> Vec<String> {
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
            bad.push(format!("stats.{k}: want {a}, got {b}"));
        }
    }
    if want["pieces"].as_u64().unwrap() as usize != got.pieces {
        bad.push(format!("pieces: want {}, got {}", want["pieces"], got.pieces));
    }
    if want["report"].as_str().unwrap() != got.report {
        bad.push(format!("report: want {:?}, got {:?}", want["report"], got.report));
    }
    if want["over"].as_u64().unwrap() as usize != got.over || want["small"].as_u64().unwrap() as usize != got.small {
        bad.push(format!(
            "over/small: want {}/{}, got {}/{}",
            want["over"], want["small"], got.over, got.small
        ));
    }
    bad
}

/// Like a `<script>` tag: `var SupportFinsEngine` must land on `globalThis`, not a function scope.
fn load_engine() -> BrowserHost {
    let src = serde_json::to_string(ENGINE).expect("escape engine source");
    let boot = format!("(0,eval)({src})");
    js_sys::eval(&boot).expect("eval Support Fins engine");
    let host = BrowserHost;
    assert!(host.has("computeFinsB64"), "SupportFinsEngine did not load");
    host
}

#[wasm_bindgen_test]
fn browser_call_reports_thrown_error() {
    js_sys::eval("(0,eval)('globalThis.SupportFinsEngine = { boom: function () { throw new Error(\"boom\"); } };')")
        .expect("stub engine");
    let err = BrowserHost.call("boom", &[]).unwrap_err();
    assert_eq!(err, "Error: boom");
}

#[wasm_bindgen_test]
fn browser_options_schema_json() {
    let host = load_engine();
    assert_eq!(host.host_kind(), HostKind::Browser);
    let s = host.call("optionsSchemaJson", &[]).expect("optionsSchemaJson");
    assert!(serde_json::from_str::<Value>(&s).is_ok());
}

#[wasm_bindgen_test]
fn browser_t_bracket_matches_fins_golden() {
    let host = load_engine();
    let golden: Value = serde_json::from_str(GOLDEN).unwrap();
    let case = &golden["t_bracket_default"];
    let (v, f) = load_model(T_BRACKET);
    let opts = options(&host, &case["options"]);
    let raw: Value = serde_json::from_str(
        &host
            .call("computeFinsB64", &[&js(&soup(&v, &f)), &js(&opts)])
            .expect("computeFinsB64"),
    )
    .unwrap();
    let report = host
        .call("reportLine", &[&raw["stats"].to_string()])
        .expect("reportLine");
    let tri = b64_decode(raw["triangles"].as_str().unwrap());
    let sha = format!("{:x}", Sha256::digest(&tri));
    let got = Outcome {
        stats: raw["stats"].clone(),
        pieces: raw["pieces"].as_array().unwrap().len(),
        report,
        over: raw["overFaces"].as_array().unwrap().len(),
        small: raw["smallFaces"].as_array().unwrap().len(),
    };
    assert_eq!(sha, case["triangles_sha256"].as_str().unwrap());
    let bad = compare(case, &got);
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}
