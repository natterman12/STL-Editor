//! Properties' numbers (1.x `r_info`), worked out by hand.

use std::sync::Arc;

use stl_engine::State;
use stl_engine::info::body_info;
use stl_engine::state::Body;
use stl_kernel::Solid;

fn state_with(name: &str, solid: Option<Solid>, mesh: Option<stl_kernel::MeshArrays>) -> State {
    let mut st = State::default();
    st.bodies.insert(
        "b1".into(),
        Body {
            name: name.into(),
            solid,
            mesh: mesh.map(Arc::new),
        },
    );
    st
}

fn near(v: &serde_json::Value, want: f64) -> bool {
    (v.as_f64().unwrap() - want).abs() < 1e-6 * want.abs().max(1.0)
}

#[test]
fn info_cube() {
    let st = state_with("Box", Some(Solid::cube([20.0, 20.0, 20.0], false)), None);
    let i = body_info(&st, "b1").unwrap();
    assert_eq!(i["name"], "Box");
    assert_eq!(i["solid"], true);
    for k in 0..3 {
        assert!(near(&i["size"][k], 20.0), "size {k}: {}", i["size"][k]);
    }
    assert_eq!(i["triangles"], 12);
    assert!(near(&i["area"], 2400.0), "area {}", i["area"]);
    assert!(near(&i["volume"], 8000.0), "volume {}", i["volume"]);
    assert_eq!(i["genus"], 0);
}

#[test]
fn info_ring() {
    // a 20 mm cube with a round hole through it: one handle
    let hole = Solid::cylinder(40.0, 4.0, 4.0, 32, true).translate([10.0, 10.0, 10.0]);
    let ring = Solid::cube([20.0, 20.0, 20.0], false).difference(&hole);
    let st = state_with("Ring", Some(ring), None);
    let i = body_info(&st, "b1").unwrap();
    assert_eq!(i["solid"], true);
    assert_eq!(i["genus"], 1);
    let v = i["volume"].as_f64().unwrap();
    assert!(
        v < 8000.0 && v > 8000.0 - 20.0 * std::f64::consts::PI * 16.0 - 1.0,
        "volume {v}"
    );
}

#[test]
fn info_open_mesh() {
    // a cube with its top two triangles left out: not watertight
    let m = Solid::cube([20.0, 20.0, 20.0], false).to_mesh();
    let top: Vec<usize> = (0..m.triangles.len() / 3)
        .filter(|&t| (0..3).all(|k| m.positions[m.triangles[t * 3 + k] as usize * 3 + 2] > 19.0))
        .collect();
    assert_eq!(top.len(), 2, "the top face is two triangles");
    let triangles: Vec<u32> = (0..m.triangles.len() / 3)
        .filter(|t| !top.contains(t))
        .flat_map(|t| m.triangles[t * 3..t * 3 + 3].to_vec())
        .collect();
    let open = stl_kernel::MeshArrays {
        positions: m.positions.clone(),
        triangles,
    };
    let st = state_with("Open", None, Some(open));
    let i = body_info(&st, "b1").unwrap();
    assert_eq!(i["solid"], false);
    assert!(i["volume"].is_null());
    assert_eq!(i["genus"], -1);
    assert_eq!(i["triangles"], 10);
    assert!(near(&i["area"], 2000.0), "area {}", i["area"]);
    assert!(near(&i["size"][2], 20.0));
}

#[test]
fn info_missing_body() {
    let e = body_info(&State::default(), "nope").unwrap_err();
    assert_eq!(e.0, "Body 'nope' no longer exists.");
}
