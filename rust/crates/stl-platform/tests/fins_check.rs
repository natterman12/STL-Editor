//! `fins::check::check` (the support-section check and tilt hint the Support Fins panel shows
//! after a build) on the real bundled engine, through the webview host.
#![cfg(any(windows, target_os = "linux"))]

use serde_json::{Value, json};
use std::path::PathBuf;
use stl_engine::features::common::fins::{MeshData, check::check};
use stl_platform::fins::WebviewHost;

fn engine_js() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../web/vendor/support-fins/engine.iife.js")
}

/// Boxes as one mesh (their faces facing out).
fn boxes(list: &[[f64; 6]]) -> MeshData {
    let mut verts = Vec::new();
    let mut faces = Vec::new();
    for b in list {
        let o = verts.len();
        for i in 0..8usize {
            verts.push([
                if i & 1 == 0 { b[0] } else { b[3] },
                if i & 2 == 0 { b[1] } else { b[4] },
                if i & 4 == 0 { b[2] } else { b[5] },
            ]);
        }
        for f in [
            [0, 2, 1],
            [1, 2, 3],
            [4, 5, 6],
            [5, 7, 6],
            [0, 1, 4],
            [1, 5, 4],
            [2, 6, 3],
            [3, 6, 7],
            [0, 4, 2],
            [2, 4, 6],
            [1, 3, 5],
            [3, 7, 5],
        ] {
            faces.push([o + f[0], o + f[1], o + f[2]]);
        }
    }
    MeshData {
        verts,
        faces,
        memo: None,
    }
}

#[test]
fn check_names_a_tilt_and_checks_the_supports() {
    let host = WebviewHost::start(&engine_js()).expect("the webview host should start on this machine");
    // a 40 mm block with a 1.2 mm wall standing on it: the overhangs are too shallow for a fin
    // this way up, and laying it on its front side leaves none of them
    let ledge = boxes(&[[0.0, 0.0, 0.0, 40.0, 40.0, 20.0], [0.0, 0.0, 20.0, 40.0, 1.2, 40.0]]);
    let r = check(&host, &ledge, &json!({})).unwrap();
    println!("{r}");
    assert_eq!(r["status"], "complete");
    assert_eq!(r["unserved"], 1, "{r}");
    assert_eq!(r["findings"], json!([]));
    let tilt = &r["tilt"];
    assert_eq!(tilt["phrase"], "laying it on its front side (-Y)");
    assert_eq!(tilt["unserved"], 0);
    assert_eq!(tilt["rot"], json!([1, 0, 0, 0, 0, 1, 0, -1, 0]));

    // a T: fins are built, nothing too shallow, so no tilt is looked for; every finding has a box
    let tee = boxes(&[
        [-2.0, -2.0, 0.0, 2.0, 2.0, 30.0],
        [-30.0, -10.0, 30.0, 30.0, 10.0, 34.0],
    ]);
    let r = check(&host, &tee, &json!({})).unwrap();
    println!("{r}");
    assert_eq!(r["status"], "complete");
    assert_eq!(r["unserved"], 0);
    assert_eq!(r["floating"], 1, "the bar on top starts in mid-air");
    assert_eq!(r["tilt"], Value::Null);
    for f in r["findings"].as_array().unwrap() {
        assert_eq!(f["box"].as_array().map(Vec::len), Some(6));
    }
}
