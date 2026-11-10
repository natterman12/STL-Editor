use serde_json::{json, Map, Value};
use stl_engine::features::common::*;
use stl_engine::{FeatureResult, State};
use stl_kernel::Solid;

fn close(a: [f64; 3], b: [f64; 3]) -> bool {
    (0..3).all(|i| (a[i] - b[i]).abs() < 1e-12)
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[test]
fn base_planes_match_1x() {
    let p = base_planes();
    assert_eq!(p[0], ("XY", [0.0; 3], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]));
    assert_eq!(p[1], ("XZ", [0.0; 3], [0.0, -1.0, 0.0], [1.0, 0.0, 0.0]));
    assert_eq!(p[2], ("YZ", [0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]));
}

#[test]
fn plane_frame_orthonormal() {
    // given x direction, not perpendicular to n: it is made perpendicular
    let f = plane_frame([1.0, 2.0, 3.0], [0.0, 0.0, 2.0], Some([1.0, 0.0, 1.0])).unwrap();
    assert_eq!(f.o, [1.0, 2.0, 3.0]);
    assert!(close(f.n, [0.0, 0.0, 1.0]) && close(f.x, [1.0, 0.0, 0.0]) && close(f.y, [0.0, 1.0, 0.0]));
    // default rule, floor-like normal: x = world X
    let f = plane_frame([0.0; 3], [0.0, 0.0, -1.0], None).unwrap();
    assert!(close(f.x, [1.0, 0.0, 0.0]) && close(f.y, [0.0, -1.0, 0.0]));
    // default rule, wall-like normal: y keeps world Z "up"
    let f = plane_frame([0.0; 3], [1.0, 0.0, 0.0], None).unwrap();
    assert!(close(f.y, [0.0, 0.0, 1.0]) && close(f.x, [0.0, 1.0, 0.0]));
    // an x direction along the normal falls back to the default rule
    let f = plane_frame([0.0; 3], [1.0, 0.0, 0.0], Some([1.0, 0.0, 0.0])).unwrap();
    assert!(close(f.y, [0.0, 0.0, 1.0]));
    // all orthonormal and right handed
    for n in [[0.3, -0.5, 0.8], [1.0, 1.0, 0.0], [0.0, 0.0, 1.0]] {
        let f = plane_frame([0.0; 3], n, None).unwrap();
        assert!((dot(f.x, f.x) - 1.0).abs() < 1e-12 && (dot(f.y, f.y) - 1.0).abs() < 1e-12);
        assert!(dot(f.x, f.y).abs() < 1e-12 && dot(f.x, f.n).abs() < 1e-12 && dot(f.y, f.n).abs() < 1e-12);
        let c = [
            f.x[1] * f.y[2] - f.x[2] * f.y[1],
            f.x[2] * f.y[0] - f.x[0] * f.y[2],
            f.x[0] * f.y[1] - f.x[1] * f.y[0],
        ];
        assert!(close(c, f.n));
    }
    assert_eq!(plane_frame([0.0; 3], [0.0; 3], None).unwrap_err().0, "Zero-length direction.");
}

#[test]
fn frame_resolves_all_ref_kinds() {
    let mut st = State::default();
    let xz = st.frame(Some(&json!("XZ"))).unwrap();
    assert!(close(xz.n, [0.0, -1.0, 0.0]) && close(xz.y, [0.0, 0.0, 1.0]));
    assert!(close(st.frame(None).unwrap().n, [0.0, 0.0, 1.0]));
    assert!(close(st.frame(Some(&Value::Null)).unwrap().n, [0.0, 0.0, 1.0]));
    let custom = plane_frame([0.0, 0.0, 5.0], [0.0, 0.0, 1.0], None).unwrap();
    st.planes.insert("p1".into(), PlaneDef { frame: custom, extra: Map::new() });
    assert_eq!(st.frame(Some(&json!("p1"))).unwrap().o, [0.0, 0.0, 5.0]);
    assert_eq!(st.frame(Some(&json!({"plane": "p1"}))).unwrap().o, [0.0, 0.0, 5.0]);
    let f = st
        .frame(Some(&json!({"origin": [1, 2, 3], "normal": [0, 0, 1], "xdir": [0, 1, 0]})))
        .unwrap();
    assert!(close(f.o, [1.0, 2.0, 3.0]) && close(f.x, [0.0, 1.0, 0.0]) && close(f.y, [-1.0, 0.0, 0.0]));
    let g = st
        .frame(Some(&json!({"origin": [1, 2, 3], "normal": [0, 0, 1], "xdir": [0, 1, 0], "flip": true})))
        .unwrap();
    assert!(close(g.x, f.x) && close(g.y, [1.0, 0.0, 0.0]) && close(g.n, [0.0, 0.0, -1.0]));
    assert_eq!(st.frame(Some(&json!(5))).unwrap_err().0, "Invalid plane reference.");
    assert_eq!(st.frame(Some(&json!({"foo": 1}))).unwrap_err().0, "Invalid plane reference.");
}

#[test]
fn missing_plane_message() {
    let st = State::default();
    assert_eq!(st.frame(Some(&json!("p9"))).unwrap_err().0, "Plane 'p9' no longer exists.");
}

#[test]
fn rotation_matches_1x() {
    // printed by 1.x _rotation(30, 45, 60)
    let want = [
        [0.35355339059327384, -0.5732233047033631, 0.7391989197401166],
        [0.6123724356957946, 0.7391989197401168, 0.2803300858899107],
        [-0.7071067811865476, 0.35355339059327373, 0.6123724356957946],
    ];
    let got = rotation(30.0, 45.0, 60.0);
    for i in 0..3 {
        for j in 0..3 {
            assert!((got[i][j] - want[i][j]).abs() < 1e-15, "{i}{j}: {} vs {}", got[i][j], want[i][j]);
        }
    }
}

#[test]
fn float_message() {
    let p = json!({"a": "2.5", "b": "x", "c": "", "d": 3}).as_object().unwrap().clone();
    assert_eq!(float(&p, "a", None).unwrap(), 2.5);
    assert_eq!(float(&p, "d", None).unwrap(), 3.0);
    assert_eq!(float(&p, "b", None).unwrap_err().0, "b must be a number.");
    assert_eq!(float(&p, "c", None).unwrap_err().0, "Missing value: c.");
    assert_eq!(float(&p, "zz", None).unwrap_err().0, "Missing value: zz.");
    assert_eq!(float(&p, "zz", Some(7.0)).unwrap(), 7.0);
    assert_eq!(float(&p, "c", Some(7.0)).unwrap(), 7.0);
}

fn res() -> FeatureResult {
    FeatureResult::default()
}

#[test]
fn apply_tool_new_join_cut_intersect() {
    let cube = || Solid::cube([10.0, 10.0, 10.0], false);
    let tool = || cube().translate([5.0, 0.0, 0.0]);
    let vol = |st: &State, id: &str| st.bodies[id].solid.as_ref().unwrap().volume();
    let fresh = || {
        let mut st = State::default();
        let mut r = res();
        apply_tool(&mut st, "f1", cube(), "new", None, "Box", &mut r).unwrap();
        assert_eq!(r.created, vec!["f1".to_string()]);
        assert_eq!(st.bodies["f1"].name, "Box");
        assert_eq!(st.tools["f1"].operation, "new");
        st
    };
    let mut st = fresh();
    assert!((vol(&st, "f1") - 1000.0).abs() < 1e-9);

    let mut r = res();
    apply_tool(&mut st, "f2", tool(), "join", None, "J", &mut r).unwrap();
    assert_eq!(r.modified, vec!["f1".to_string()]);
    assert!((vol(&st, "f1") - 1500.0).abs() < 1e-6);

    let mut st = fresh();
    let mut r = res();
    apply_tool(&mut st, "f2", tool(), "cut", None, "C", &mut r).unwrap();
    assert!((vol(&st, "f1") - 500.0).abs() < 1e-6);
    assert_eq!(st.tools["f2"].operation, "cut");

    let mut st = fresh();
    let mut r = res();
    apply_tool(&mut st, "f2", tool(), "intersect", Some(&json!(["f1"])), "I", &mut r).unwrap();
    assert!((vol(&st, "f1") - 500.0).abs() < 1e-6);

    // nothing to join to: a new body with a warning; cut with no overlap: an error
    let mut st = State::default();
    let mut r = res();
    apply_tool(&mut st, "f1", cube(), "join", None, "J", &mut r).unwrap();
    assert_eq!(r.warning.as_deref(), Some("Nothing to join to: made a new body."));
    let mut r = res();
    let far = cube().translate([100.0, 0.0, 0.0]);
    let e = apply_tool(&mut st, "f2", far, "cut", None, "C", &mut r).unwrap_err();
    assert_eq!(e.0, "Nothing to cut: the tool doesn't touch any body.");
    let e = apply_tool(&mut st, "f3", cube(), "cut", Some(&json!(["zz"])), "C", &mut res()).unwrap_err();
    assert_eq!(e.0, "The target body no longer exists.");
    let e = apply_tool(&mut st, "f3", cube(), "bogus", None, "C", &mut res()).unwrap_err();
    assert_eq!(e.0, "Unknown operation bogus.");
}
