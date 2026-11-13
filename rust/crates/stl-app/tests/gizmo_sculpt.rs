//! Task 4 (M4b): the gizmo (1.x `web/js/gizmo.js` on three's TransformControls), the Sculpt
//! brush (app.js' sculpt pointer handlers, viewer.js `setBrush` / `drawStrokes`) and the
//! instant overhang overlay (viewer.js `overhangFaces`, app.js `quickPaint`).
//!
//! The rotation table is made by `node dev/parity/js/gizmo_quat.mjs --write` from 1.x's own
//! `quatOf` / `rotOf` in Node (`--check` in CI).
#![cfg(not(target_arch = "wasm32"))]

use std::sync::Arc;

use egui::Pos2;
use serde_json::{json, Map, Value};
use stl_app::engine_link::BodyView;
use stl_app::tools::gizmo::{self, Gizmo, GizmoDrop, GizmoKind};
use stl_app::tools::sculpt::{Sculpt, SculptEvent, OTHER_BODY};
use stl_app::viewport::camera::{self, M4, V3};
use stl_app::viewport::gizmo_draw::{handle_unit, Handle};
use stl_app::viewport::scene::rgba;
use stl_app::viewport::{overhang_faces, PreviewMesh, PreviewPack, Viewport};
use stl_kernel::{Aabb, MeshArrays};

type Values = Map<String, Value>;

/// A box `sx` x `sy` x `sz` standing on the bed, centred on the origin in X and Y.
fn box_mesh(sx: f32, sy: f32, sz: f32) -> Arc<MeshArrays> {
    let (x, y) = (sx / 2.0, sy / 2.0);
    let p = [[-x, -y, 0.0], [x, -y, 0.0], [x, y, 0.0], [-x, y, 0.0], [-x, -y, sz], [x, -y, sz], [x, y, sz], [-x, y, sz]];
    let t: [u32; 36] = [0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7, 0, 1, 5, 0, 5, 4, 1, 2, 6, 1, 6, 5, 2, 3, 7, 2, 7, 6, 3, 0, 4, 3, 4, 7];
    Arc::new(MeshArrays { positions: p.iter().flatten().copied().collect(), triangles: t.to_vec() })
}

fn body(id: &str, mesh: Arc<MeshArrays>) -> BodyView {
    BodyView { id: id.into(), name: id.into(), color: Some("#9aa7b4".into()), hidden: false, solid: true, mesh, revision: 1 }
}

fn view(bodies: &[BodyView]) -> Viewport {
    let mut vp = Viewport::new();
    vp.set_pick_trees(false);
    vp.set_view_size(800.0, 600.0);
    vp.sync_bodies(bodies);
    vp.flush();
    vp.fit_all();
    vp.camera_mut().finish_flight();
    vp
}

fn obj(v: Value) -> Values {
    v.as_object().cloned().unwrap()
}

fn move_values(gizmo_mode: &str) -> Values {
    obj(json!({ "bodies": ["b1"], "_gizmo": gizmo_mode, "translate": [0, 0, 0], "rotate": [0, 0, 0], "pivot": "center", "bed": true, "copy": false }))
}

fn vec3(v: &Value) -> [f64; 3] {
    let a = v.as_array().expect("an array");
    [0, 1, 2].map(|i| a[i].as_f64().expect("a number"))
}

fn screen(vp: &Viewport, p: V3) -> Pos2 {
    vp.world_to_screen(p)
}

fn close(a: [f64; 3], b: [f64; 3], tol: f64) -> bool {
    (0..3).all(|k| (a[k] - b[k]).abs() <= tol)
}

fn apply(m: &M4, p: V3) -> V3 {
    gizmo::apply(m, p)
}

fn patch_of(d: GizmoDrop) -> Values {
    match d {
        GizmoDrop::Values { patch, auto_accept } => {
            assert!(auto_accept, "1.x accepts on drop");
            patch
        }
        other => panic!("expected values, got {other:?}"),
    }
}

/// Grab the X arrow and drag it `dx` mm along X.
fn drag_x(vp: &mut Viewport, g: &mut Gizmo, dx: f64) -> Values {
    let c = vp.gizmo().expect("handles shown").position;
    let u = handle_unit(vp.camera(), c);
    let grab = camera::add(c, [0.45 * u, 0.0, 0.0]);
    assert_eq!(vp.gizmo_handle_at(screen(vp, grab)), Some(Handle::X));
    assert!(g.press(vp, screen(vp, grab)), "the X arrow takes the press");
    let to = camera::add(grab, [dx, 0.0, 0.0]);
    g.drag_to(vp, screen(vp, to), false).expect("the drag plane is hit")
}

#[test]
fn gizmo_move_writes_fields() {
    let mut vp = view(&[body("b1", box_mesh(20.0, 20.0, 20.0))]);
    let mut g = Gizmo::new(GizmoKind::Move);
    let vals = move_values("move");
    g.sync(&mut vp, &vals);
    let shape = vp.gizmo().expect("handles on the part").clone();
    assert!(close(shape.position, [0.0, 0.0, 10.0], 1e-9), "{:?}", shape.position);
    let cam = vp.camera().clone();
    let rev = vp.shown_revision("b1");

    let live = drag_x(&mut vp, &mut g, 15.0);
    assert!(g.drag_active());
    assert!(vp.camera_locked(), "the camera holds still while a handle is dragged");
    let t = vec3(&live["translate"]);
    assert!(close(t, [15.0, 0.0, 0.0], 0.002), "live numbers {t:?}");
    // the part follows on screen, through its transform (no new mesh)
    let m = *vp.body_transform("b1").expect("moved on screen");
    assert!(close(apply(&m, [0.0, 0.0, 0.0]), [t[0], 0.0, 0.0], 1e-9));
    assert_eq!(vp.shown_revision("b1"), rev);

    let patch = patch_of(g.release(&mut vp));
    assert!(!g.drag_active());
    assert!(!vp.camera_locked());
    assert!(g.consume_click(), "the click that grabbed a handle isn't a click on the part");
    assert!(!g.consume_click());
    let mut keys: Vec<_> = patch.keys().cloned().collect();
    keys.sort();
    assert_eq!(keys, ["rotate", "translate"], "Move's drop writes Move X Y Z and Rotate X Y Z");
    assert_eq!(vec3(&patch["translate"]), t);
    assert_eq!(vec3(&patch["rotate"]), [0.0; 3]);
    // whole millimetres go in as JS writes them (15, not 15.0)
    let mut whole = move_values("move");
    whole.insert("translate".into(), json!([15, 0, 0]));
    if t == [15.0, 0.0, 0.0] {
        assert_eq!(patch["translate"], whole["translate"]);
    }
    assert_eq!(vp.camera().position, cam.position);
    assert_eq!(vp.camera().target, cam.target);

    // the panel puts the numbers in; the handles follow them
    let mut after = vals.clone();
    for (k, v) in patch {
        after.insert(k, v);
    }
    g.sync(&mut vp, &after);
    assert!(close(vp.gizmo().unwrap().position, [t[0], 0.0, 10.0], 1e-9));

    // Scale: the middle box scales evenly and writes Factor
    let mut s = Gizmo::new(GizmoKind::Scale);
    let sv = obj(json!({ "bodies": ["b1"], "mode": "factor", "factor": 1, "factor3": [1, 1, 1], "size": [20, 20, 20], "uniform": true, "units": "in_to_mm", "pivot": "base" }));
    g.hide(&mut vp);
    s.sync(&mut vp, &sv);
    let c = vp.gizmo().unwrap().position;
    assert!(close(c, [0.0, 0.0, 0.0], 1e-9), "Scale's handles sit on the base: {c:?}");
    let u = handle_unit(vp.camera(), c);
    let grab = camera::add(c, [0.54 * u, 0.0, 0.0]);
    assert_eq!(vp.gizmo_handle_at(screen(&vp, grab)), Some(Handle::X));
    let at = screen(&vp, grab);
    assert!(s.press(&mut vp, at));
    let at = screen(&vp, camera::scale(grab, 1.5));
    s.drag_to(&mut vp, at, false).unwrap();
    let patch = patch_of(s.release(&mut vp));
    assert_eq!(patch["mode"], json!("factor"));
    assert_eq!(patch["uniform"], json!(true), "Uniform on: one factor for all three");
    assert!((patch["factor"].as_f64().unwrap() - 1.5).abs() < 0.002, "{patch:?}");
}

/// Put the Z ring under the pointer and turn it.
fn ring_drag(free: bool) -> (f64, f64) {
    let mut vp = view(&[body("b1", box_mesh(20.0, 20.0, 20.0))]);
    let mut g = Gizmo::new(GizmoKind::Move);
    g.sync(&mut vp, &move_values("rotate"));
    let c = vp.gizmo().unwrap().position;
    let u = handle_unit(vp.camera(), c);
    let e = camera::sub(vp.camera().position, c);
    let d = camera::normalize([e[0], e[1], 0.0]);
    let grab = camera::add(c, camera::scale(d, 0.5 * u));
    assert_eq!(vp.gizmo_handle_at(screen(&vp, grab)), Some(Handle::Z), "the Z ring");
    let at = screen(&vp, grab);
    assert!(g.press(&mut vp, at));
    // along the ring, sideways on screen
    let side = camera::normalize(camera::cross([0.0, 0.0, 1.0], d));
    let to = camera::add(grab, camera::scale(side, 0.23 * u));
    let at = screen(&vp, to);
    let live = g.drag_to(&mut vp, at, free).unwrap();
    let r = vec3(&live["rotate"]);
    assert_eq!((r[0], r[1]), (0.0, 0.0), "only Z turns: {r:?}");
    assert!(close(vec3(&live["translate"]), [0.0; 3], 0.0), "turning doesn't move it");
    let patch = patch_of(g.release(&mut vp));
    assert_eq!(vec3(&patch["rotate"]), r);
    // the speed is three's: 20 / distance per millimetre of pointer travel on the drag plane
    let speed = 20.0 / camera::length(camera::sub(c, vp.camera().position));
    (r[2], speed)
}

#[test]
fn rotate_snaps_5_deg() {
    let (z, _) = ring_drag(false);
    assert!(z != 0.0, "it turned");
    assert_eq!(z % 5.0, 0.0, "5° steps: {z}");
    let (free, _) = ring_drag(true);
    assert_eq!(z, ((free / 5.0) + 0.5).floor() * 5.0, "the step is the free turn rounded: {free} -> {z}");
}

#[test]
fn shift_rotates_free() {
    let (free, _) = ring_drag(true);
    assert!(free.abs() > 0.5, "{free}");
    assert!(free % 5.0 != 0.0, "Shift turns freely: {free}");
    // to 0.001°, as rotOf writes it
    assert_eq!((free * 1000.0).round() / 1000.0, free);
}

#[test]
fn quat_rot_round_trip_zyx() {
    #[rustfmt::skip]
    let cases: &[([f64; 3], [f64; 4], [f64; 3], [f64; 3], [f64; 4])] = &[
    // BEGIN gizmo_quat.mjs
    ([0.0, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0], [0.0, 0.0, 0.0], [0.0, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0]),
    ([30.0, 0.0, 0.0], [0.25881904510252074, 0.0, 0.0, 0.9659258262890683], [30.0, 0.0, 0.0], [30.0, 0.0, 0.0], [0.2588190451025207, 0.0, 0.0, 0.9659258262890684]),
    ([0.0, 45.0, 0.0], [0.0, 0.3826834323650898, 0.0, 0.9238795325112867], [0.0, 45.0, 0.0], [0.0, 45.0, 0.0], [0.0, 0.38268343236508984, 0.0, 0.9238795325112867]),
    ([0.0, 0.0, 60.0], [0.0, 0.0, 0.49999999999999994, 0.8660254037844387], [0.0, 0.0, 60.0], [0.0, 0.0, 60.0], [0.0, 0.0, 0.5, 0.8660254037844385]),
    ([10.0, 20.0, 30.0], [0.03813457647485015, 0.18930785741199999, 0.2392983377447303, 0.9515485246437885], [10.0, 20.0, 30.0], [10.0, 20.0, 30.0], [0.03813457647485014, 0.18930785741199999, 0.23929833774473028, 0.9515485246437886]),
    ([-35.0, 75.0, 120.0], [-0.6220852815619539, 0.08368887330856589, 0.7467937692294795, 0.21978430605562893], [-35.0, 75.0, 120.0], [-35.0, 75.0, 120.0], [-0.6220852815619539, 0.08368887330856589, 0.7467937692294795, 0.2197843060556289]),
    ([90.0, 0.0, 0.0], [0.7071067811865475, 0.0, 0.0, 0.7071067811865476], [90.0, 0.0, 0.0], [90.0, 0.0, 0.0], [0.7071067811865475, 0.0, 0.0, 0.7071067811865476]),
    ([0.0, 90.0, 0.0], [0.0, 0.7071067811865475, 0.0, 0.7071067811865476], [0.0, 90.0, 0.0], [0.0, 90.0, 0.0], [0.0, 0.7071067811865475, 0.0, 0.7071067811865476]),
    ([0.0, -90.0, 0.0], [0.0, -0.7071067811865475, 0.0, 0.7071067811865476], [0.0, -90.0, 0.0], [0.0, -90.0, 0.0], [0.0, -0.7071067811865475, 0.0, 0.7071067811865476]),
    ([45.0, 90.0, 30.0], [0.09229595564125731, 0.7010573846499778, -0.09229595564125723, 0.7010573846499779], [0.0, 90.0, -15.0], [0.0, 90.0, -15.0], [0.0922959556412573, 0.7010573846499778, -0.09229595564125721, 0.7010573846499779]),
    ([12.0, -90.0, 77.0], [0.4956176937828986, -0.5043442292812725, 0.49561769378289866, 0.5043442292812726], [0.0, -90.0, 89.0], [0.0, -90.0, 89.0], [0.49561769378289844, -0.5043442292812724, 0.49561769378289855, 0.5043442292812728]),
    ([0.0, 89.9995, 0.0], [0.0, 0.7071036958444425, 0.0, 0.7071098665151901], [0.0, 89.999, 0.0], [0.0, 89.999, 0.0], [0.0, 0.7071036958444424, 0.0, 0.7071098665151903]),
    ([5.0, 89.999, 5.0], [5.378093922356331e-7, 0.7071006339701408, 5.378093922391025e-7, 0.7071129283491051], [0.0, 89.999, 0.0], [0.0, 89.999, 0.0], [5.378093922356332e-7, 0.7071006339701409, 5.378093922391025e-7, 0.707112928349105]),
    ([0.0, 90.0004, 0.0], [0.0, 0.7071092494505385, 0.0, 0.7071043129139406], [0.0, 90.0, 0.0], [0.0, 90.0, 0.0], [0.0, 0.7071092494505387, 0.0, 0.7071043129139406]),
    ([200.0, 0.0, 0.0], [0.984807753012208, 0.0, -0.0, -0.1736481776669303], [-160.0, 0.0, 0.0], [-160.0, 0.0, 0.0], [0.9848077530122079, 0.0, 0.0, -0.17364817766693033]),
    ([0.0, 0.0, 270.0], [-0.0, 0.0, 0.7071067811865476, -0.7071067811865475], [0.0, 0.0, -90.0], [0.0, 0.0, -90.0], [0.0, -0.0, -0.7071067811865476, 0.7071067811865475]),
    ([-190.0, 10.0, 359.0], [0.9924323766487018, -0.0010644134002652924, -0.08757845633152547, 0.08606310934488713], [170.0, 10.0, -1.0], [170.0, 10.0, -1.0], [0.9924323766487019, -0.0010644134002652926, -0.08757845633152547, 0.08606310934488712]),
    ([181.0, 181.0, 181.0], [0.008802020473715338, -0.008649721429408331, 0.008802020473715338, 0.9998851089952057], [1.0, -1.0, 1.0], [1.0, -1.0, 1.0], [0.008802020473715338, -0.008649721429408331, 0.008802020473715338, 0.9998851089952057]),
    ([720.0, -45.0, 15.0], [0.049950211252314594, -0.3794095225512604, 0.1205904774487395, 0.9159756150367534], [0.0, -45.0, 15.0], [0.0, -45.0, 15.0], [0.04995021125231459, -0.37940952255126037, 0.1205904774487395, 0.9159756150367535]),
    ([0.0005, 0.0015, -0.0025], [0.000004363608707390779, 0.000013089874193400194, -0.000021816672761887868, 0.9999999996668234], [0.0, 0.001, -0.002], [0.0, 0.001, -0.002], [0.000004363608707390779, 0.000013089874193400194, -0.000021816672761887868, 0.9999999996668234]),
    ([123.4565, -0.0005, 45.0005], [0.8136701856705555, 0.3370351486630007, 0.18126500857786507, 0.4375969997404225], [123.457, 0.0, 45.001], [123.457, 0.0, 45.001], [0.8136701856705555, 0.3370351486630007, 0.18126500857786507, 0.4375969997404225]),
    ([1e-10, -1e-10, 0.0], [8.726646259971648e-13, -8.726646259971648e-13, 7.615435494667715e-25, 1.0], [0.0, 0.0, 0.0], [0.0, 0.0, 0.0], [8.726646259971648e-13, -8.726646259971648e-13, 7.615435494667715e-25, 1.0]),
    ([5.0, 10.0, 15.0], [0.03171637284819485, 0.0919996771632968, 0.12613658517555598, 0.9872282881762724], [5.0, 10.0, 15.0], [5.0, 10.0, 15.0], [0.03171637284819485, 0.0919996771632968, 0.12613658517555598, 0.9872282881762724]),
    ([15.0, 0.0, -30.0], [0.12607862007251908, -0.03378266443126182, -0.25660481229257065, 0.9576621969425486], [15.0, 0.0, -30.0], [15.0, 0.0, -30.0], [0.12607862007251905, -0.03378266443126182, -0.25660481229257065, 0.9576621969425487]),
    ([0.0, 5.0, f64::NAN], [0.0, 0.043619387365336, 0.0, 0.9990482215818578], [0.0, 5.0, 0.0], [0.0, 5.0, 0.0], [0.0, 0.043619387365336, 0.0, 0.9990482215818578]),
    // END gizmo_quat.mjs
    ];
    assert!(cases.len() >= 20, "run `node dev/parity/js/gizmo_quat.mjs --write`");
    for (rot, q, back, neg, q2) in cases {
        let ours = gizmo::quat_of(*rot);
        for k in 0..4 {
            assert!((ours[k] - q[k]).abs() <= 1e-12, "quatOf({rot:?}) = {ours:?}, 1.x {q:?}");
        }
        assert_eq!(gizmo::rot_of(*q), *back, "rotOf({q:?})");
        assert_eq!(gizmo::rot_of(q.map(|c| -c)), *neg, "rotOf(-q) for {rot:?}");
        // our own quatOf can differ from V8's in the last bit, which can tip a value sitting on a
        // 0.0005° tie (89.9995) to its neighbour: within one step
        let chain = gizmo::rot_of(ours);
        assert!(close(chain, *back, 0.0010000001), "rotOf(quatOf({rot:?})) = {chain:?}, 1.x {back:?}");
        let m = gizmo::mat_of_quat(*q);
        let qm = gizmo::quat_of_mat(&m);
        for k in 0..4 {
            assert!((qm[k] - q2[k]).abs() <= 1e-12, "setFromRotationMatrix for {rot:?}: {qm:?}, 1.x {q2:?}");
        }
    }
    // ZYX: R = Rz·Ry·Rx, so X turns first
    let m = gizmo::mat_of_quat(gizmo::quat_of([90.0, 0.0, 90.0]));
    assert!(close(apply(&m, [0.0, 1.0, 0.0]), [0.0, 0.0, 1.0], 1e-12));
    assert!(close(apply(&m, [1.0, 0.0, 0.0]), [0.0, 1.0, 0.0], 1e-12));
    assert_eq!(gizmo::js_round(-2.5), -2.0, "Math.round");
    assert_eq!(gizmo::js_round(2.5), 3.0);
}

#[test]
fn land_on_bed() {
    let tall = box_mesh(10.0, 10.0, 40.0);
    let mut vp = view(&[body("b1", tall.clone())]);
    let mut g = Gizmo::new(GizmoKind::Move);
    let mut vals = move_values("move");
    vals.insert("rotate".into(), json!([90, 0, 0]));
    g.sync(&mut vp, &vals);
    // turned about its centre (z 20) it spans z 15..25; landing puts its lowest point at Move Z
    let c = vp.gizmo().unwrap().position;
    assert!(close(c, [0.0, 0.0, 5.0], 1e-9), "{c:?}");
    let gb = gizmo::GizmoBody { id: "b1".into(), bbox: Aabb { min: [-5.0, -5.0, 0.0], max: [5.0, 5.0, 40.0] }, mesh: tall.clone() };
    let m = g.matrix_for(&gb, &vals);
    let lo = tall.positions.chunks(3).map(|p| apply(&m, [p[0] as f64, p[1] as f64, p[2] as f64])[2]).fold(f64::INFINITY, f64::min);
    assert!(lo.abs() < 1e-9, "on the bed: {lo}");

    // dragging up: Move Z is the height above the bed
    let u = handle_unit(vp.camera(), c);
    let grab = camera::add(c, [0.0, 0.0, 0.45 * u]);
    assert_eq!(vp.gizmo_handle_at(screen(&vp, grab)), Some(Handle::Z));
    let at = screen(&vp, grab);
    assert!(g.press(&mut vp, at));
    let at = screen(&vp, camera::add(grab, [0.0, 0.0, 3.0]));
    let live = g.drag_to(&mut vp, at, false).unwrap();
    let t = vec3(&live["translate"]);
    assert!(close(t, [0.0, 0.0, 3.0], 0.002), "{t:?}");
    let m = *vp.body_transform("b1").unwrap();
    let lo = tall.positions.chunks(3).map(|p| apply(&m, [p[0] as f64, p[1] as f64, p[2] as f64])[2]).fold(f64::INFINITY, f64::min);
    assert!((lo - t[2]).abs() < 1e-9, "lowest point at Move Z: {lo}");
    g.release(&mut vp);

    // Land on bed off: it turns in place
    let mut g = Gizmo::new(GizmoKind::Move);
    vals.insert("bed".into(), json!(false));
    g.hide(&mut vp);
    g.sync(&mut vp, &vals);
    assert!(close(vp.gizmo().unwrap().position, [0.0, 0.0, 20.0], 1e-9));
}

#[test]
fn drag_during_preview() {
    let cube = box_mesh(20.0, 20.0, 20.0);
    let mut vp = view(&[body("b1", cube.clone())]);
    let pack = PreviewPack { meshes: vec![PreviewMesh { id: "b1".into(), mesh: cube.clone() }], ..Default::default() };
    let mut g = Gizmo::new(GizmoKind::Move);
    g.sync(&mut vp, &move_values("move"));
    // a preview on screen goes when a handle is grabbed (1.x `start()`)
    vp.show_preview(&pack);
    vp.flush(); // prepared on a worker thread
    assert_eq!(vp.preview_meshes().count(), 1);
    let live = drag_x(&mut vp, &mut g, 12.0);
    assert_eq!(vp.preview_meshes().count(), 0, "grabbing clears the preview");
    let held = *vp.body_transform("b1").unwrap();

    // a late preview lands mid-drag: it is dropped, the part stays under the pointer
    if g.accept_preview(&mut vp) {
        vp.show_preview(&pack);
    }
    assert_eq!(vp.preview_meshes().count(), 0, "no preview while dragging");
    assert!(vp.body_visible("b1"), "the dragged part is still drawn");
    assert_eq!(vp.body_transform("b1"), Some(&held), "not snapped back");
    // the drag goes on from where it was
    let c = vp.gizmo().unwrap().position;
    assert!((c[0] - vec3(&live["translate"])[0]).abs() < 0.0006, "the handles stay where the drag put them");

    g.release(&mut vp);
    assert_eq!(vp.body_transform("b1"), Some(&held), "after letting go the part stays until the preview is back");
    assert!(g.accept_preview(&mut vp));
    vp.show_preview(&pack);
    vp.flush();
    assert_eq!(vp.preview_meshes().count(), 1);
    assert!(vp.body_transform("b1").is_none(), "the preview shows the moved part; the body is put back");
}

#[test]
fn sculpt_brush_follows_surface() {
    let mut vp = view(&[body("b1", box_mesh(20.0, 20.0, 20.0)), body("b2", {
        // a second box off to the side
        let m = box_mesh(10.0, 10.0, 10.0);
        let mut p = m.positions.clone();
        for v in p.chunks_mut(3) {
            v[0] += 40.0;
        }
        Arc::new(MeshArrays { positions: p, triangles: m.triangles.clone() })
    })]);
    let mut s = Sculpt::new();
    let vals = obj(json!({ "body": null, "mode": "inflate", "radius": 5, "strength": 0.5, "strokes": [] }));
    Sculpt::open(&mut vp, &vals);
    assert_eq!(vp.mouse_mode, stl_app::viewport::MouseMode::Sculpt);

    // the ring sits on the surface under the pointer, facing out of it
    let top = [2.0, -3.0, 20.0];
    let at = screen(&vp, top);
    s.hover(&mut vp, Some(at), &vals);
    let (p, n, r) = vp.brush().expect("the brush ring on the part");
    assert!(close(p, top, 1e-4), "{p:?}");
    assert!(close(n, [0.0, 0.0, 1.0], 1e-9), "{n:?}");
    assert_eq!(r, 5.0);
    let side = [10.0, 0.0, 12.0];
    let at = screen(&vp, side);
    s.hover(&mut vp, Some(at), &vals);
    let (p, n, _) = vp.brush().unwrap();
    assert!(close(p, side, 1e-4) && close(n, [1.0, 0.0, 0.0], 1e-9), "{p:?} {n:?}");
    // off the part: no ring
    s.hover(&mut vp, Some(Pos2::new(2.0, 2.0)), &vals);
    assert!(vp.brush().is_none());

    // a stroke: a point each time the pointer is more than a fifth of the brush away
    let at = screen(&vp, [-6.0, 0.0, 20.0]);
    assert_eq!(s.press(&mut vp, at, &vals), SculptEvent::Drawing);
    let at = screen(&vp, [-5.5, 0.0, 20.0]);
    assert_eq!(s.hover(&mut vp, Some(at), &vals), SculptEvent::None, "0.5 mm: too close");
    let at = screen(&vp, [-4.0, 0.0, 20.0]);
    assert_eq!(s.hover(&mut vp, Some(at), &vals), SculptEvent::Drawing);
    let at = screen(&vp, [1.0 / 3.0, 0.0, 20.0]);
    assert_eq!(s.hover(&mut vp, Some(at), &vals), SculptEvent::Drawing);
    assert!(vp.overlay_items().iter().any(|o| o.tag == "strokes" && !o.lines.is_empty()), "the stroke is drawn as it grows");
    let patch = s.release(&vals).expect("a stroke");
    assert_eq!(patch["body"], json!("b1"));
    let st = &patch["strokes"][0];
    assert_eq!(st.as_object().unwrap().keys().collect::<Vec<_>>().len(), 4, "no body in the stroke: {st}");
    assert_eq!(st["mode"], json!("inflate"));
    assert_eq!(st["radius"], json!(5));
    assert_eq!(st["strength"], json!(0.5));
    let pts = st["points"].as_array().unwrap();
    assert_eq!(pts.len(), 3);
    assert!(close(vec3(&pts[2]), [0.333, 0.0, 20.0], 0.0015), "{:?}", pts[2]);
    for q in pts {
        for c in q.as_array().unwrap() {
            let x = c.as_f64().unwrap();
            assert_eq!((x * 1000.0).round() / 1000.0, x, "to 0.001 mm");
        }
    }

    // strokes on one body: a stroke on the other is refused, word for word
    let mut more = vals.clone();
    for (k, v) in patch {
        more.insert(k, v);
    }
    let at = screen(&vp, [40.0, 0.0, 10.0]);
    assert_eq!(s.press(&mut vp, at, &more), SculptEvent::Warn(OTHER_BODY));
    assert_eq!(OTHER_BODY, "Finish (OK) the strokes on the other body first.");
    // the brush size and strength rules
    let odd = obj(json!({ "radius": "0", "strength": "7" }));
    assert_eq!(stl_app::tools::sculpt::brush_radius(&odd), 5.0, "parseFloat || 5");
    assert_eq!(stl_app::tools::sculpt::brush_strength(&odd), 1.0);
    let tiny = obj(json!({ "radius": 0.01, "strength": 0.001 }));
    assert_eq!(stl_app::tools::sculpt::brush_radius(&tiny), 0.05);
    assert_eq!(stl_app::tools::sculpt::brush_strength(&tiny), 0.01);
    assert_eq!(stl_app::tools::sculpt::fixed3(0.0625), 0.063, "toFixed: a tie goes up");
    assert_eq!(stl_app::tools::sculpt::fixed3(-0.0625), -0.063);
    assert_eq!(stl_app::tools::sculpt::fixed3(1.23449), 1.234);

    s.close(&mut vp);
    assert!(vp.brush().is_none());
    assert!(!vp.overlay_items().iter().any(|o| o.tag == "strokes"));
    assert_eq!(vp.mouse_mode, stl_app::viewport::MouseMode::Model);
}

#[test]
fn overhang_overlay_draws() {
    // a foot on the bed, a 4 x 4 ledge facing down at z 5 (16 mm²), and a 2 x 2 one (4 mm²,
    // too small) at z 8
    #[rustfmt::skip]
    let positions: Vec<f32> = vec![
        0.0, 0.0, 0.0,  10.0, 0.0, 0.0,  0.0, 10.0, 0.0,
        0.0, 0.0, 5.0,  4.0, 0.0, 5.0,  4.0, 4.0, 5.0,  0.0, 4.0, 5.0,
        6.0, 6.0, 8.0,  8.0, 6.0, 8.0,  8.0, 8.0, 8.0,  6.0, 8.0, 8.0,
        // a face tipped 60° from straight down, facing down (normal z -0.5): not an overhang at
        // 45°, but one at 70°
        20.0, 0.0, 5.0,  30.0, 0.0, 5.0,  25.0, 5.0, 5.0 + 8.66,
    ];
    let triangles = vec![
        0, 1, 2, // on the bed
        3, 5, 4, 3, 6, 5, // the ledge, facing down
        7, 9, 8, 7, 10, 9, // the small ledge
        11, 13, 12,
    ];
    let mesh = Arc::new(MeshArrays { positions, triangles });
    assert_eq!(overhang_faces(&mesh, 45.0), vec![1, 2]);
    // at 70° the tipped face (normal z -0.5, facing down) counts too
    assert_eq!(overhang_faces(&mesh, 70.0), vec![1, 2, 5]);

    let mut vp = view(&[body("b1", mesh)]);
    vp.highlight_face("b1", &[0], "sf-over", egui::Color32::BLUE);
    let faces = vp.show_overhangs("b1", 45.0);
    assert_eq!(faces, vec![1, 2]);
    let items: Vec<_> = vp.overlay_items().iter().filter(|o| o.tag == "sf-over").collect();
    assert_eq!(items.len(), 1, "the old paint is replaced");
    let it = items[0];
    assert_eq!(it.tris.len(), 6, "two triangles");
    let red = rgba(0xd64545, 0.45);
    assert!(it.tris.iter().all(|v| v.c0 == red), "1.x's red at 0.45");
    assert!(it.tris.iter().all(|v| (v.pos[2] - 5.0).abs() < 1e-6), "on the ledge");
    // nothing to hold up: nothing painted
    let mut vp2 = view(&[body("b1", box_mesh(10.0, 10.0, 10.0))]);
    assert!(vp2.show_overhangs("b1", 45.0).is_empty());
}

#[path = "support/gpu.rs"]
mod gpu;

/// The moved body and the handles reach the picture: drawn by the GPU, with no GPU error.
#[test]
fn gizmo_transform_and_handles_render() {
    let Some(gpu) = gpu::headless() else { return eprintln!("no GPU adapter: skipped") };
    let (w, h) = (800u32, 600u32);
    let mut vp = view(&[body("b1", box_mesh(20.0, 20.0, 20.0))]);
    vp.fit(Aabb { min: [-30.0, -30.0, 0.0], max: [70.0, 30.0, 20.0] });
    vp.camera_mut().finish_flight();
    let px = |img: &[u8], p: Pos2| {
        let (x, y) = (p.x.round() as usize, p.y.round() as usize);
        let i = (y * w as usize + x) * 4;
        [img[i], img[i + 1], img[i + 2]]
    };
    let before = gpu.render(&mut vp, w, h).expect("a picture");
    // off the axis lines: a point on the front of the box, and the same 40 mm along X
    let (old, new) = (screen(&vp, [5.0, -10.0, 14.0]), screen(&vp, [45.0, -10.0, 14.0]));

    let m: M4 = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [40.0, 0.0, 0.0, 1.0]];
    vp.set_body_transform("b1", Some(m));
    let moved = gpu.render(&mut vp, w, h).expect("a picture");
    assert_ne!(px(&moved, old), px(&before, old), "nothing left where it was");
    assert_ne!(px(&moved, new), px(&before, new), "drawn 40 mm along X");
    assert_eq!(px(&moved, new), px(&before, old), "the same face, lit the same");
    assert_eq!(vp.stats().uploads(), 1, "moved without a new upload");

    // the Move handles on the moved part: the X cone is pure red
    let mut g = Gizmo::new(GizmoKind::Move);
    vp.set_body_transform("b1", None);
    g.sync(&mut vp, &move_values("move"));
    let c = vp.gizmo().unwrap().position;
    let u = handle_unit(vp.camera(), c);
    let img = gpu.render(&mut vp, w, h).expect("a picture");
    assert_eq!(px(&img, screen(&vp, camera::add(c, [0.52 * u, 0.0, 0.0]))), [255, 0, 0], "the X arrow's cone");
    assert_eq!(px(&img, screen(&vp, camera::add(c, [0.0, 0.0, 0.52 * u]))), [0, 0, 255], "the Z arrow's cone");
    // hovered: yellow
    let at = screen(&vp, camera::add(c, [0.45 * u, 0.0, 0.0]));
    g.hover_at(&mut vp, Some(at));
    let img = gpu.render(&mut vp, w, h).expect("a picture");
    // just off the axis: the white helper line along it is drawn over the middle, as in 1.x
    assert_eq!(px(&img, screen(&vp, camera::add(c, [0.52 * u, 0.0, 0.02 * u]))), [255, 255, 0], "the hovered arrow turns yellow");
    assert_eq!(vp.stats().gpu_errors(), 0);
}

/// `Gizmo::update` driven by egui's pointer, after `Viewport::show`, as the app runs it: the
/// camera holds still through a handle drag, and the events come in order.
#[test]
fn update_drives_a_drag_without_turning_the_view() {
    use egui_kittest::Harness;
    use stl_app::tools::gizmo::GizmoEvent;
    struct St {
        vp: Viewport,
        g: Gizmo,
        vals: Values,
        events: Vec<GizmoEvent>,
    }
    let st = St { vp: view(&[body("b1", box_mesh(20.0, 20.0, 20.0))]), g: Gizmo::new(GizmoKind::Move), vals: move_values("move"), events: Vec::new() };
    let mut h = Harness::builder().with_size(egui::vec2(800.0, 600.0)).with_accessibility_check(false).build_ui_state(
        |ui, st: &mut St| {
            let r = st.vp.show(ui);
            let e = st.g.update(&mut st.vp, ui, &r.response, &st.vals);
            if e != GizmoEvent::None {
                st.events.push(e);
            }
        },
        st,
    );
    h.run_steps(3);
    let (grab, to) = {
        let vp = &h.state().vp;
        let c = vp.gizmo().expect("handles shown").position;
        let u = handle_unit(vp.camera(), c);
        let g = camera::add(c, [0.45 * u, 0.0, 0.0]);
        (screen(vp, g), screen(vp, camera::add(g, [8.0, 0.0, 0.0])))
    };
    let cam = h.state().vp.camera().clone();
    let button = |pos: Pos2, pressed: bool| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() };
    h.event(egui::Event::PointerMoved(grab));
    h.run_steps(2);
    h.event(button(grab, true));
    h.run_steps(2);
    for k in 1..=10 {
        h.event(egui::Event::PointerMoved(grab + (to - grab) * (k as f32 / 10.0)));
        h.run_steps(1);
        assert!(h.state().vp.camera_locked(), "held still while the handle is dragged");
    }
    h.event(button(to, false));
    h.run_steps(2);
    let st = h.state();
    assert_eq!(st.vp.camera().position, cam.position, "the view didn't turn");
    assert_eq!(st.vp.camera().target, cam.target);
    assert!(!st.vp.camera_locked());
    assert_eq!(st.events.first(), Some(&GizmoEvent::Started), "{:?}", st.events);
    assert!(st.events.iter().any(|e| matches!(e, GizmoEvent::Moved(_))));
    let Some(GizmoEvent::Dropped(GizmoDrop::Values { patch, auto_accept: true })) = st.events.last() else {
        panic!("ends with the new values: {:?}", st.events.last())
    };
    let t = vec3(&patch["translate"]);
    assert!((t[0] - 8.0).abs() < 0.05 && t[1] == 0.0 && t[2] == 0.0, "{t:?}");
}
