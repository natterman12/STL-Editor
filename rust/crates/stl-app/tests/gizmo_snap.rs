//! Move snaps (milestone 7, spec 2.9): dragging Move's arrows snaps Move X Y Z to the grid step
//! the view shows at this zoom, and the part's lowest point onto the build plate within a few
//! screen points; Shift moves freely. Drop to plate puts the lowest point on the plate.
#![cfg(not(target_arch = "wasm32"))]

// keeps this test binary out of the real 2.0 folders (STLEDITOR2_CONFIG_DIR)
#[path = "support/test_env.rs"]
mod test_env;

use std::sync::{Arc, Mutex};

use egui::Pos2;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::{Map, Value, json};
use stl_app::app::StlApp;
use stl_app::engine_link::{BodyView, Command, EngineLink, Update};
use stl_app::platform::NullPlatform;
use stl_app::sketch::guides::visible_grid_step;
use stl_app::tools::gizmo::{self, Gizmo, GizmoBody, GizmoKind, PLATE_SNAP_PX};
use stl_app::ui::ThemePref;
use stl_app::viewport::Viewport;
use stl_app::viewport::camera::{self, V3};
use stl_app::viewport::gizmo_draw::{Handle, handle_unit};
use stl_kernel::{Aabb, MeshArrays};

type Values = Map<String, Value>;

/// A box `sx` x `sy` x `sz` whose bottom is at `z0`, centred on the origin in X and Y.
fn box_mesh(sx: f32, sy: f32, sz: f32, z0: f32) -> Arc<MeshArrays> {
    let (x, y) = (sx / 2.0, sy / 2.0);
    let (a, b) = (z0, z0 + sz);
    let p = [
        [-x, -y, a],
        [x, -y, a],
        [x, y, a],
        [-x, y, a],
        [-x, -y, b],
        [x, -y, b],
        [x, y, b],
        [-x, y, b],
    ];
    let t: [u32; 36] = [
        0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7, 0, 1, 5, 0, 5, 4, 1, 2, 6, 1, 6, 5, 2, 3, 7, 2, 7, 6, 3, 0, 4, 3, 4, 7,
    ];
    Arc::new(MeshArrays {
        positions: p.iter().flatten().copied().collect(),
        triangles: t.to_vec(),
    })
}

fn body(id: &str, mesh: Arc<MeshArrays>) -> BodyView {
    BodyView {
        id: id.into(),
        name: id.into(),
        color: Some("#9aa7b4".into()),
        hidden: false,
        solid: true,
        mesh,
        revision: 1,
        holds_up: None,
    }
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

fn move_values() -> Values {
    json!({ "bodies": ["b1"], "_gizmo": "move", "translate": [0, 0, 0], "rotate": [0, 0, 0], "pivot": "center", "bed": true, "copy": false })
        .as_object()
        .cloned()
        .unwrap()
}

fn vec3(v: &Value) -> [f64; 3] {
    let a = v.as_array().expect("an array");
    [0, 1, 2].map(|i| a[i].as_f64().expect("a number"))
}

fn screen(vp: &Viewport, p: V3) -> Pos2 {
    vp.world_to_screen(p)
}

/// Grab the arrow along `axis` (0 X, 1 Y, 2 Z) and drag it `d` mm along it. Returns the live
/// numbers and the grid step and points-to-mm of the drag.
fn drag_axis(vp: &mut Viewport, g: &mut Gizmo, axis: usize, d: f64, free: bool) -> (Values, f64, f64) {
    let c = vp.gizmo().expect("handles shown").position;
    let px = vp.pixel_size(c);
    let u = handle_unit(vp.camera(), c);
    let mut dir = [0.0; 3];
    dir[axis] = 1.0;
    let grab = camera::add(c, camera::scale(dir, 0.45 * u));
    let want = [Handle::X, Handle::Y, Handle::Z][axis];
    assert_eq!(vp.gizmo_handle_at(screen(vp, grab)), Some(want));
    assert!(g.press(vp, screen(vp, grab)), "the arrow takes the press");
    let to = camera::add(grab, camera::scale(dir, d));
    let live = g.drag_to(vp, screen(vp, to), free).expect("the drag plane is hit");
    (live, visible_grid_step(1.0 / px), px)
}

fn is_multiple(x: f64, step: f64) -> bool {
    let k = x / step;
    (k - k.round()).abs() < 1e-6
}

#[test]
fn translate_snaps_to_grid() {
    let mut vp = view(&[body("b1", box_mesh(20.0, 20.0, 20.0, 0.0))]);
    let mut g = Gizmo::new(GizmoKind::Move);
    let vals = move_values();
    g.sync(&mut vp, &vals);
    let c = vp.gizmo().unwrap().position;
    let step = visible_grid_step(1.0 / vp.pixel_size(c));
    assert!(step > 0.0);

    // 3.3 steps along X goes to 3 steps; the part follows the snapped number on screen
    let (live, _, _) = drag_axis(&mut vp, &mut g, 0, 3.3 * step, false);
    let t = vec3(&live["translate"]);
    assert!(
        (t[0] - 3.0 * step).abs() < 1e-6 && t[1] == 0.0 && t[2] == 0.0,
        "{t:?} (step {step})"
    );
    let m = *vp.body_transform("b1").unwrap();
    assert!((gizmo::apply(&m, [0.0; 3])[0] - 3.0 * step).abs() < 1e-6);
    // 3.6 steps goes to 4
    let at = screen(
        &vp,
        camera::add(c, [0.45 * handle_unit(vp.camera(), c) + 3.6 * step, 0.0, 0.0]),
    );
    let live = g.drag_to(&mut vp, at, false).unwrap();
    assert!(
        (vec3(&live["translate"])[0] - 4.0 * step).abs() < 1e-6,
        "{:?}",
        live["translate"]
    );
    let patch = match g.release(&mut vp) {
        gizmo::GizmoDrop::Values { patch, .. } => patch,
        other => panic!("{other:?}"),
    };
    assert!(
        is_multiple(vec3(&patch["translate"])[0], step),
        "the drop writes the snapped number: {:?}",
        patch["translate"]
    );

    // a part that was already moved off the grid: Move X lands on whole steps, not the part's centre
    let mut vals2 = move_values();
    vals2.insert("translate".into(), json!([0.37, 0, 0]));
    let mut g = Gizmo::new(GizmoKind::Move);
    g.hide(&mut vp);
    g.sync(&mut vp, &vals2);
    let s2 = visible_grid_step(1.0 / vp.pixel_size(vp.gizmo().unwrap().position));
    let (live, step, _) = drag_axis(&mut vp, &mut g, 1, 2.2 * s2, false);
    let t = vec3(&live["translate"]);
    assert!((t[0] - 0.37).abs() < 1e-9, "X is not on the Y arrow: {t:?}");
    assert!((t[1] - 2.0 * step).abs() < 1e-6, "{t:?} (step {step})");

    // zoomed in, the grid shows a finer step and a drag snaps to it
    let mut vp = view(&[body("b1", box_mesh(20.0, 20.0, 20.0, 0.0))]);
    let far = visible_grid_step(1.0 / vp.pixel_size(c));
    vp.camera_mut().zoom_at(0.1, c);
    vp.camera_mut().finish_flight();
    let mut g = Gizmo::new(GizmoKind::Move);
    g.sync(&mut vp, &move_values());
    let fine = visible_grid_step(1.0 / vp.pixel_size(vp.gizmo().unwrap().position));
    assert!(fine < far, "zoomed in: {fine} < {far}");
    let (live, _, _) = drag_axis(&mut vp, &mut g, 0, 2.4 * fine, false);
    let t = vec3(&live["translate"])[0];
    assert!((t - 2.0 * fine).abs() < 1e-6, "{t} (step {fine})");
    assert!(!is_multiple(t, far), "not the coarser step's snap: {t} (was {far})");
}

/// A part's fins (a body of their own) ride along while its arrow is dragged, as the engine
/// moves them when the Move goes in; other bodies stay put (Nathan 2026-11-16, "fins follow").
#[test]
fn the_fins_follow_the_dragged_part() {
    let mut fins = body("s", box_mesh(4.0, 4.0, 10.0, 0.0));
    fins.holds_up = Some("b1".into());
    let mut vp = view(&[
        body("b1", box_mesh(20.0, 20.0, 20.0, 10.0)),
        fins,
        body("other", box_mesh(5.0, 5.0, 5.0, 0.0)),
    ]);
    let mut g = Gizmo::new(GizmoKind::Move);
    g.sync(&mut vp, &move_values());
    let (live, _, _) = drag_axis(&mut vp, &mut g, 0, 12.0, true);
    let t = vec3(&live["translate"]);
    let part = *vp.body_transform("b1").unwrap();
    let fin = *vp.body_transform("s").expect("the fins move with the part");
    assert_eq!(
        gizmo::apply(&part, [1.0, 2.0, 3.0]),
        gizmo::apply(&fin, [1.0, 2.0, 3.0])
    );
    assert!((gizmo::apply(&fin, [0.0; 3])[0] - t[0]).abs() < 1e-6);
    assert!(vp.body_transform("other").is_none(), "a body that isn't its fins stays");
    // put back with the part
    g.reset(&mut vp);
    assert!(vp.body_transform("s").is_none());
}

#[test]
fn shift_is_free() {
    let mut vp = view(&[body("b1", box_mesh(20.0, 20.0, 20.0, 0.0))]);
    let mut g = Gizmo::new(GizmoKind::Move);
    g.sync(&mut vp, &move_values());
    let step = visible_grid_step(1.0 / vp.pixel_size(vp.gizmo().unwrap().position));
    let (live, _, _) = drag_axis(&mut vp, &mut g, 0, 3.3 * step, true);
    let t = vec3(&live["translate"]);
    assert!(
        (t[0] - 3.3 * step).abs() < 0.002,
        "Shift drags freely: {t:?} (step {step})"
    );
    assert!(!is_multiple(t[0], step));
}

#[test]
fn z_snaps_to_plate() {
    // a part floating 5.3 mm above the plate (not on any grid step)
    let lift = 5.3;
    let mut vp = view(&[body("b1", box_mesh(20.0, 20.0, 20.0, lift as f32))]);

    // within 6 points of the plate: the lowest point lands on it exactly
    let mut g = Gizmo::new(GizmoKind::Move);
    g.sync(&mut vp, &move_values());
    let px = vp.pixel_size(vp.gizmo().unwrap().position);
    let (live, _, _) = drag_axis(&mut vp, &mut g, 2, -lift + 4.0 * px, false);
    let t = vec3(&live["translate"]);
    assert!((t[2] + lift).abs() < 1e-9, "on the plate: {t:?} (px {px})");
    let m = *vp.body_transform("b1").unwrap();
    assert!(gizmo::apply(&m, [0.0, 0.0, lift])[2].abs() < 1e-9);
    g.release(&mut vp);

    // from below too
    let mut g = Gizmo::new(GizmoKind::Move);
    g.hide(&mut vp);
    g.sync(&mut vp, &move_values());
    let (live, _, px) = drag_axis(&mut vp, &mut g, 2, -lift - 5.0 * px, false);
    assert!(
        (vec3(&live["translate"])[2] + lift).abs() < 1e-9,
        "{:?}",
        live["translate"]
    );
    g.release(&mut vp);

    // farther than 6 points: the grid step wins
    let mut g = Gizmo::new(GizmoKind::Move);
    g.hide(&mut vp);
    g.sync(&mut vp, &move_values());
    let away = (PLATE_SNAP_PX + 6.0) * px;
    let (live, step, _) = drag_axis(&mut vp, &mut g, 2, -lift + away, false);
    let t = vec3(&live["translate"]);
    assert!((t[2] + lift).abs() > 1e-3, "not snapped to the plate: {t:?}");
    assert!(is_multiple(t[2], step), "{t:?} (step {step})");
    g.release(&mut vp);

    // with Shift, no plate snap
    let mut g = Gizmo::new(GizmoKind::Move);
    g.hide(&mut vp);
    g.sync(&mut vp, &move_values());
    let (live, _, px) = drag_axis(&mut vp, &mut g, 2, -lift + 3.0 * px, true);
    let t = vec3(&live["translate"]);
    assert!((t[2] + lift - 3.0 * px).abs() < 0.002, "{t:?}");
    g.release(&mut vp);

    // the X arrow never snaps Z
    let mut g = Gizmo::new(GizmoKind::Move);
    g.hide(&mut vp);
    let mut near_plate = move_values();
    near_plate.insert("translate".into(), json!([0, 0, -5.29]));
    g.sync(&mut vp, &near_plate);
    let (live, _, _) = drag_axis(&mut vp, &mut g, 0, 1.0, false);
    assert!(
        (vec3(&live["translate"])[2] + 5.29).abs() < 1e-9,
        "{:?}",
        live["translate"]
    );
}

#[test]
fn drop_to_plate_button() {
    // the numbers: a lifted part drops by its height above the plate
    let lifted = box_mesh(20.0, 20.0, 20.0, 5.3);
    let gb = GizmoBody {
        id: "b1".into(),
        bbox: Aabb {
            min: [-10.0, -10.0, 5.3],
            max: [10.0, 10.0, 25.3],
        },
        mesh: lifted.clone(),
    };
    let mut vals = move_values();
    vals.insert("translate".into(), json!([4, -2, 0]));
    let t = Gizmo::drop_to_plate(std::slice::from_ref(&gb), &vals).unwrap();
    assert!(
        (t[0] - 4.0).abs() < 1e-9 && (t[1] + 2.0).abs() < 1e-9 && (t[2] + 5.3).abs() < 1e-9,
        "{t:?}"
    );
    // turned without Land on bed: the turned part's lowest point goes onto the plate
    let tall = box_mesh(10.0, 10.0, 40.0, 0.0);
    let tb = GizmoBody {
        id: "b1".into(),
        bbox: Aabb {
            min: [-5.0, -5.0, 0.0],
            max: [5.0, 5.0, 40.0],
        },
        mesh: tall.clone(),
    };
    let mut turned = move_values();
    turned.insert("rotate".into(), json!([90, 0, 0]));
    turned.insert("bed".into(), json!(false));
    let t = Gizmo::drop_to_plate(std::slice::from_ref(&tb), &turned).unwrap();
    turned.insert("translate".into(), json!(t));
    let m = Gizmo::new(GizmoKind::Move).matrix_for(&tb, &turned);
    let lo = tall
        .positions
        .chunks(3)
        .map(|p| gizmo::apply(&m, [p[0] as f64, p[1] as f64, p[2] as f64])[2])
        .fold(f64::INFINITY, f64::min);
    assert!(lo.abs() < 1e-6, "on the plate: {lo} ({t:?})");
    // with Land on bed the turned part already rests at Move Z: Drop to plate makes it 0
    turned.insert("bed".into(), json!(true));
    turned.insert("translate".into(), json!([0, 0, 7]));
    assert_eq!(
        Gizmo::drop_to_plate(std::slice::from_ref(&tb), &turned).unwrap()[2],
        0.0
    );
    // no parts, no numbers
    assert_eq!(Gizmo::drop_to_plate(&[], &vals), None);

    // the Move panel's button in the app
    let rec = Rec::default();
    let app = StlApp::new(&egui::Context::default(), Box::new(NullPlatform), Box::new(rec.clone()));
    rec.push(Update::Bodies(vec![body("b1", lifted)]));
    let mut init = false;
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 800.0))
        .with_accessibility_check(false)
        .build_ui_state(
            move |ui, a: &mut StlApp| {
                if !init {
                    init = true;
                    stl_app::fonts::install(ui.ctx());
                    a.set_theme(ui.ctx(), ThemePref::Dark);
                }
                a.layout(ui)
            },
            app,
        );
    h.run_steps(3);
    h.state_mut().viewport.flush();
    h.run_steps(1);
    assert!(h.state_mut().open_tool("move", Some(move_values())));
    h.run_steps(3);
    h.get_by_label("Drop to plate").click();
    h.run_steps(3);
    let t = vec3(&h.state().tool().expect("Move stays open").values["translate"]);
    assert!((t[2] + 5.3).abs() < 1e-4, "{t:?}");
    let _ = rec.commands();

    // the button on a turned part (Land on bed off): its lowest point ends at Z = 0
    let rec = Rec::default();
    let app = StlApp::new(&egui::Context::default(), Box::new(NullPlatform), Box::new(rec.clone()));
    rec.push(Update::Bodies(vec![body("b1", tall.clone())]));
    let mut init = false;
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 800.0))
        .with_accessibility_check(false)
        .build_ui_state(
            move |ui, a: &mut StlApp| {
                if !init {
                    init = true;
                    stl_app::fonts::install(ui.ctx());
                    a.set_theme(ui.ctx(), ThemePref::Dark);
                }
                a.layout(ui)
            },
            app,
        );
    h.run_steps(3);
    h.state_mut().viewport.flush();
    h.run_steps(1);
    let mut start = move_values();
    start.insert("rotate".into(), json!([90, 0, 0]));
    start.insert("bed".into(), json!(false));
    assert!(h.state_mut().open_tool("move", Some(start)));
    h.run_steps(3);
    h.get_by_label("Drop to plate").click();
    h.run_steps(3);
    let vals = h.state().tool().expect("Move stays open").values.clone();
    let m = Gizmo::new(GizmoKind::Move).matrix_for(&tb, &vals);
    let lo = tall
        .positions
        .chunks(3)
        .map(|p| gizmo::apply(&m, [p[0] as f64, p[1] as f64, p[2] as f64])[2])
        .fold(f64::INFINITY, f64::min);
    assert!(
        lo.abs() < 1e-3,
        "turned part on the plate: {lo} ({:?})",
        vals["translate"]
    );
    let _ = rec.commands();
}

#[derive(Clone, Default)]
struct Rec {
    sent: Arc<Mutex<Vec<Command>>>,
    inbox: Arc<Mutex<Vec<Update>>>,
}

impl EngineLink for Rec {
    fn send(&self, c: Command) {
        self.sent.lock().unwrap().push(c);
    }
    fn poll(&self) -> Vec<Update> {
        std::mem::take(&mut *self.inbox.lock().unwrap())
    }
    fn restart(&self) {}
}

impl Rec {
    fn push(&self, u: Update) {
        self.inbox.lock().unwrap().push(u);
    }
    fn commands(&self) -> Vec<Command> {
        self.sent.lock().unwrap().clone()
    }
}
