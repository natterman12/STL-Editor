//! M4b Task 3: the pick fields, placing on a face, editing a step from the timeline, and
//! opening every tool from the ribbon and the tool search.
#![cfg(not(target_arch = "wasm32"))]

// keeps this test binary out of the real 2.0 folders (STLEDITOR2_CONFIG_DIR)
#[path = "support/test_env.rs"]
mod test_env;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use serde_json::{json, Value};
use stl_app::engine_link::{BodyView, Command, Level, PlaneView, SketchView, TimelineItem, TimelineView};
use stl_app::theme;
use stl_app::tools::fields::bodies::{self, Mark};
use stl_app::tools::fields::{axisline, cylface, facepoints, features, pface, plane, points3, regions, sketch};
use stl_app::tools::{Ctx, Field, FieldKind, ToolDef, ToolFlags, Values};
use stl_app::ui::ribbon;
use stl_app::ui::timeline::{self, Timeline};
use stl_app::ui::UiEvent;
use stl_app::viewport::pick::Hit;
use stl_kernel::MeshArrays;

// ------------------------------------------------------------------ fixtures

/// A 10 mm cube: triangles 0-1 bottom, 2-3 top, 4-5 front (y 0), 6-7 back, 8-9 left, 10-11 right.
fn cube() -> MeshArrays {
    let p: [[f32; 3]; 8] = [[0., 0., 0.], [10., 0., 0.], [10., 10., 0.], [0., 10., 0.], [0., 0., 10.], [10., 0., 10.], [10., 10., 10.], [0., 10., 10.]];
    let t: [[u32; 3]; 12] = [[0, 2, 1], [0, 3, 2], [4, 5, 6], [4, 6, 7], [0, 1, 5], [0, 5, 4], [2, 3, 7], [2, 7, 6], [3, 0, 4], [3, 4, 7], [1, 2, 6], [1, 6, 5]];
    MeshArrays { positions: p.iter().flatten().copied().collect(), triangles: t.iter().flatten().copied().collect() }
}

/// A round pin: 48 sides, radius 5, 20 tall, with caps.
fn pin() -> MeshArrays {
    let n = 48u32;
    let mut pos = Vec::new();
    for z in [0.0f32, 20.0] {
        for i in 0..n {
            let a = i as f32 / n as f32 * std::f32::consts::TAU;
            pos.extend([5.0 * a.cos(), 5.0 * a.sin(), z]);
        }
    }
    pos.extend([0.0, 0.0, 0.0, 0.0, 0.0, 20.0]);
    let (c0, c1) = (2 * n, 2 * n + 1);
    let mut tri = Vec::new();
    for i in 0..n {
        let j = (i + 1) % n;
        tri.extend([i, j, n + j, i, n + j, n + i]);
    }
    for i in 0..n {
        let j = (i + 1) % n;
        tri.extend([c0, j, i, c1, n + i, n + j]);
    }
    MeshArrays { positions: pos, triangles: tri }
}

fn body(id: &str, name: &str, mesh: MeshArrays) -> BodyView {
    BodyView { id: id.into(), name: name.into(), color: None, hidden: false, solid: true, mesh: Arc::new(mesh), revision: 1 }
}

fn item(id: &str, kind: &str, label: &str, params: Value) -> TimelineItem {
    TimelineItem { id: id.into(), kind: kind.into(), label: label.into(), active: true, params, ..Default::default() }
}

struct Scene {
    bodies: Vec<BodyView>,
    planes: Vec<PlaneView>,
    sketches: Vec<SketchView>,
    features: Vec<TimelineItem>,
}

impl Scene {
    fn new() -> Self {
        Scene { bodies: vec![body("b1", "Box", cube())], planes: Vec::new(), sketches: Vec::new(), features: Vec::new() }
    }
    fn ctx(&self) -> Ctx<'_> {
        Ctx { bodies: &self.bodies, planes: &self.planes, sketches: &self.sketches, features: &self.features, selection: &[], last_sketch: None, sketch_on_body: false, inch: false }
    }
}

fn field(key: &'static str, kind: FieldKind) -> Field {
    Field { key, label: "Field", kind, help: None, show: None, placeholder: None }
}

fn tool(key: &'static str, fields: Vec<Field>, flags: ToolFlags) -> ToolDef {
    ToolDef {
        key,
        label: "Tool",
        icon: "box",
        kind: "box",
        fields,
        hint: None,
        extra: None,
        flags,
        defaults: |_| Values::new(),
        to_params: |v, _| Value::Object(v.clone()),
        from_params: |p| p.as_object().cloned().unwrap_or_default(),
        on_change: None,
    }
}

fn hit_body(face: u32, point: [f64; 3], normal: [f64; 3]) -> Hit {
    Hit::Body { id: "b1".into(), face, point, normal }
}

fn num(v: &Value, i: usize) -> f64 {
    v[i].as_f64().unwrap()
}

// ------------------------------------------------------------------ bodies

struct Ui1 {
    f: Field,
    values: Values,
    active: Option<String>,
    scene: Scene,
    out: bodies::ShowOut,
}

#[test]
fn pick_bodies() {
    let sc = Scene::new();
    let single = tool("extrude", vec![field("body", FieldKind::Bodies { multi: false }), field("other", FieldKind::Bodies { multi: false })], ToolFlags::default());
    let mut v = Values::new();
    // a click takes the body and moves on to the next empty pick field
    let out = bodies::feed_pick(&single, &single.fields[0], &mut v, &hit_body(2, [5., 5., 10.], [0., 0., 1.]), &sc.ctx());
    assert!(out.used && out.changed && out.advance);
    assert_eq!(v["body"], json!(["b1"]));
    assert_eq!(bodies::advance_field(&single, &v, "body").as_deref(), Some("other"));
    // a second click replaces it (a single-body field)
    let mut v2 = Values::new();
    v2.insert("body".into(), json!(["zz"]));
    bodies::feed_pick(&single, &single.fields[0], &mut v2, &hit_body(2, [5., 5., 10.], [0., 0., 1.]), &sc.ctx());
    assert_eq!(v2["body"], json!(["b1"]));
    // a multi field toggles
    let multi = tool("combine", vec![field("bodies", FieldKind::Bodies { multi: true })], ToolFlags::default());
    let mut m = Values::new();
    let h = hit_body(2, [5., 5., 10.], [0., 0., 1.]);
    bodies::feed_pick(&multi, &multi.fields[0], &mut m, &h, &sc.ctx());
    assert_eq!(m["bodies"], json!(["b1"]));
    let out = bodies::feed_pick(&multi, &multi.fields[0], &mut m, &h, &sc.ctx());
    assert_eq!(m["bodies"], json!([]));
    assert!(!out.advance);
    // a plane is used but changes nothing
    let out = bodies::feed_pick(&multi, &multi.fields[0], &mut m, &Hit::Plane { id: "XY".into(), point: [0.; 3] }, &sc.ctx());
    assert!(out.used && !out.changed);
    // Support Fins: a body click asks to reopen the Support Fins panel on it; Paint quick-paints
    let sf = tool("supportfins", vec![field("body", FieldKind::Bodies { multi: false })], ToolFlags { paint: true, ..Default::default() });
    let out = bodies::feed_pick(&sf, &sf.fields[0], &mut Values::new(), &h, &sc.ctx());
    assert_eq!(out.reopen_supports.as_deref(), Some("b1"));
    assert!(out.quick_paint);
    // the first pick field to open
    assert_eq!(bodies::first_active(&single, &Values::new()).as_deref(), Some("body"));
}

#[test]
fn pick_bodies_chip_and_cross() {
    let mut values = Values::new();
    values.insert("bodies".into(), json!(["b1"]));
    let st = Ui1 { f: field("bodies", FieldKind::Bodies { multi: true }), values, active: None, scene: Scene::new(), out: Default::default() };
    let mut h = Harness::builder().with_size(egui::vec2(300., 80.)).with_accessibility_check(false).build_ui_state(
        |ui, st: &mut Ui1| {
            let tk = theme::tokens(true);
            let ctx = st.scene.ctx();
            let o = bodies::show(ui, &tk, &st.f, &mut st.values, &mut st.active, &ctx);
            st.out.changed |= o.changed;
        },
        st,
    );
    h.run();
    // the chip shows the body's name; its cross removes it and makes the field active
    h.get_by_label("Remove Box").click();
    h.run();
    assert_eq!(h.state().values["bodies"], json!([]));
    assert!(h.state().out.changed);
    assert_eq!(h.state().active.as_deref(), Some("bodies"));
}

// ------------------------------------------------------------------ plane and place on face

#[test]
fn pick_plane() {
    let mut sc = Scene::new();
    sc.planes.push(PlaneView { id: "p1".into(), name: "Plane 1".into(), origin: [0., 0., 5.], x: [1., 0., 0.], y: [0., 1., 0.], normal: [0., 0., 1.], visible: true });
    let t = tool("extrude", vec![field("plane", FieldKind::Plane { any_face: false })], ToolFlags::default());
    let f = &t.fields[0];
    let mut v = Values::new();
    // a base plane hit stores its id
    let out = plane::feed_pick(&t, f, &mut v, &Hit::Plane { id: "XZ".into(), point: [0.; 3] }, &sc.ctx());
    assert!(out.changed && out.advance);
    assert_eq!(v["plane"], json!("XZ"));
    assert_eq!(plane::plane_label(v.get("plane"), &sc.ctx()), "XZ plane");
    assert_eq!(plane::plane_label(Some(&json!("p1")), &sc.ctx()), "Plane 1");
    // a face hit stores the face and highlights it
    let out = plane::feed_pick(&t, f, &mut v, &hit_body(2, [3., 4., 10.], [0., 0., 1.]), &sc.ctx());
    assert_eq!(v["plane"]["label"], json!("Face of Box"));
    assert_eq!(v["plane"]["body"], json!("b1"));
    assert_eq!(num(&v["plane"]["normal"], 2), 1.0);
    assert!((num(&v["plane"]["origin"], 2) - 10.0).abs() < 1e-9);
    let (b, tris) = out.highlight.unwrap();
    assert_eq!((b.as_str(), tris), ("b1", vec![2, 3]));
    assert!(out.clear_face);
    // the plane field wants planes shown while it is active, and draws a ghost of its plane
    assert!(plane::wants_planes(&t, Some("plane")));
    assert!(!plane::wants_planes(&t, None));
    assert!(matches!(plane::marks(f, &v, &sc.ctx(), false).as_slice(), [Mark::PlaneGhost(_)]));
    assert!(plane::marks(f, &v, &sc.ctx(), true).is_empty());
    let fr = plane::frame_of(&json!("p1"), &sc.ctx()).unwrap();
    assert_eq!(fr.origin, [0., 0., 5.]);
    let xz = plane::frame_of(&json!("XZ"), &sc.ctx()).unwrap();
    assert_eq!(xz.normal, [0., -1., 0.]);
}

#[test]
fn place_on_face_fills_plane() {
    let sc = Scene::new();
    let t = tool("primitive_box", vec![field("plane", FieldKind::Plane { any_face: false })], ToolFlags { place_on_face: true, ..Default::default() });
    let mut v = Values::new();
    // no pick field is active: a face click still goes to the plane field
    let out = bodies::feed(&t, None, &mut v, &hit_body(2, [3., 4., 10.], [0., 0., 1.]), &sc.ctx());
    assert!(out.used && out.changed);
    assert_eq!(v["plane"]["label"], json!("Face of Box"));
    assert_eq!(v["offset"], json!([3.0, 4.0, 0.0]), "the shape goes where the face was clicked");
    // a plane click too
    let mut v = Values::new();
    let out = bodies::feed(&t, None, &mut v, &Hit::Plane { id: "XY".into(), point: [1., 2., 0.] }, &sc.ctx());
    assert!(out.used);
    assert_eq!(v["plane"], json!("XY"));
    // a tool without placeOnFace and with nothing active leaves the click to the selection
    let other = tool("move", vec![], ToolFlags::default());
    let out = bodies::feed(&other, None, &mut Values::new(), &hit_body(2, [3., 4., 10.], [0., 0., 1.]), &sc.ctx());
    assert!(!out.used);
    assert!(plane::wants_planes(&t, None));
}

// ------------------------------------------------------------------ sketch, regions, axis line

fn rect_sketch_scene() -> Scene {
    let mut sc = Scene::new();
    let ents = json!([
        { "id": "r1", "type": "rect", "x": 0.0, "y": 0.0, "w": 10.0, "h": 10.0 },
        { "id": "r2", "type": "rect", "x": 20.0, "y": 0.0, "w": 10.0, "h": 10.0 },
        { "id": "l1", "type": "line", "p1": [0.0, 0.0], "p2": [5.0, 5.0], "construction": true },
        { "id": "pl", "type": "polyline", "pts": [[0.0, 0.0], [3.0, 0.0], [3.0, 3.0]], "closed": false },
    ]);
    sc.features.push(item("s1", "sketch", "Sketch 1", json!({ "entities": ents })));
    sc.sketches.push(SketchView {
        id: "s1".into(),
        name: "Sketch 1".into(),
        plane: "XY".into(),
        hidden: false,
        frame: [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.], [0., 0., 1.]],
        outlines: Arc::from(Vec::new()),
    });
    sc
}

#[test]
fn pick_sketch() {
    let sc = rect_sketch_scene();
    let mut v = Values::new();
    assert_eq!(sketch::selected_label(&v, "sketch", &sc.ctx()), sketch::NONE_LABEL);
    sketch::choose(&mut v, "sketch", "s1");
    assert_eq!(v["sketch"], json!("s1"));
    assert_eq!(sketch::selected_label(&v, "sketch", &sc.ctx()), "Sketch 1");
}

#[test]
fn pick_axisline() {
    let sc = rect_sketch_scene();
    let mut v = Values::new();
    v.insert("sketch".into(), json!("s1"));
    let rows = axisline::options(&v, &sc.ctx());
    assert_eq!(
        rows,
        vec![
            ("x".to_owned(), "Sketch X axis".to_owned()),
            ("y".to_owned(), "Sketch Y axis".to_owned()),
            ("l1".to_owned(), "Construction line 1".to_owned()),
            ("pl".to_owned(), "Line 2".to_owned()),
        ]
    );
    v.insert("axis".into(), json!("pl"));
    assert_eq!(axisline::selected_label(&v, "axis", &sc.ctx()), "Line 2");
    // no sketch: the two axes only
    assert_eq!(axisline::options(&Values::new(), &sc.ctx()).len(), 2);
}

#[test]
fn pick_regions() {
    let sc = rect_sketch_scene();
    let f = field("regions", FieldKind::Regions);
    let mut v = Values::new();
    v.insert("sketch".into(), json!("s1"));
    // outside every region: a warning, nothing stored
    let out = regions::feed_click(&f, &mut v, [15.0, 5.0], &sc.ctx());
    assert_eq!(out.message, Some((Level::Warning, "Click inside a closed region of the sketch.".to_owned())));
    assert!(v.get("regions").is_none());
    // inside the first square: picked; the overlay shows the sketch and the region
    let out = regions::feed_click(&f, &mut v, [5.0, 5.0], &sc.ctx());
    assert!(out.changed);
    assert_eq!(v["regions"], json!([[5.0, 5.0]]));
    let marks = regions::marks(&f, &v, &sc.ctx());
    assert!(matches!(marks[0], Mark::SketchLines { .. }));
    assert!(matches!(&marks[1], Mark::Regions { faces, .. } if faces.len() == 1));
    // the second square too
    regions::feed_click(&f, &mut v, [25.0, 5.0], &sc.ctx());
    assert_eq!(v["regions"].as_array().unwrap().len(), 2);
    // a click in a picked region again un-picks it (not at the same spot)
    regions::feed_click(&f, &mut v, [2.0, 2.0], &sc.ctx());
    assert_eq!(v["regions"], json!([[25.0, 5.0]]));
    assert_eq!(regions::label(2), "2 regions");
    assert_eq!(regions::label(1), "1 region");
}

// ------------------------------------------------------------------ features

#[test]
fn pick_features() {
    let mut sc = Scene::new();
    sc.features = vec![
        item("f1", "extrude", "Extrude 1", json!({ "operation": "add" })),
        item("f2", "hole", "Hole 1", json!({})),
        item("f3", "move", "Move 1", json!({})),
        TimelineItem { error: Some("bad".into()), ..item("f4", "extrude", "Extrude 2", json!({})) },
        TimelineItem { suppressed: true, ..item("f5", "extrude", "Extrude 3", json!({})) },
        TimelineItem { active: false, ..item("f6", "revolve", "Revolve 1", json!({})) },
    ];
    let cx = sc.ctx();
    let ids: Vec<&str> = features::repeatable(&cx).iter().map(|f| f.id.as_str()).collect();
    assert_eq!(ids, ["f1", "f2"], "only active, working, unsuppressed cuts, holes, extrudes, revolves and primitives");
    assert_eq!(features::operation(&sc.features[0]), "add");
    assert_eq!(features::operation(&sc.features[1]), "cut", "a hole cuts");
    let mut v = Values::new();
    features::toggle(&mut v, "features", "f1");
    features::toggle(&mut v, "features", "f2");
    features::toggle(&mut v, "features", "f1");
    assert_eq!(v["features"], json!(["f2"]));
}

// ------------------------------------------------------------------ face points, face, cylinder, 3 points

#[test]
fn pick_facepoints() {
    let sc = Scene::new();
    let f = field("points", FieldKind::FacePoints);
    let mut v = Values::new();
    let out = facepoints::feed_pick(&f, &mut v, &hit_body(2, [2.0, 3.0, 10.0], [0., 0., 1.]), &sc.ctx());
    assert!(out.changed);
    assert_eq!(v["points"], json!([[2.0, 3.0, 10.0]]));
    assert_eq!(v["normal"], json!([0.0, 0.0, 1.0]));
    assert_eq!(v["targets"], json!([]));
    facepoints::feed_pick(&f, &mut v, &hit_body(2, [4.0, 6.0, 10.0], [0., 0., 1.]), &sc.ctx());
    assert_eq!(v["points"].as_array().unwrap().len(), 2);
    assert_eq!(facepoints::label(2), "2 points");
    let m = facepoints::marks(&f, &v);
    assert_eq!(m.len(), 2);
    assert_eq!(m[0], Mark::Marker { p: [2.0, 3.0, 10.0], color: 0x1f6fd1 });
    // the cross clears the points and the normal
    facepoints::clear(&mut v, "points");
    assert_eq!(v["points"], json!([]));
    assert_eq!(v["normal"], Value::Null);
}

#[test]
fn pick_pface() {
    let sc = Scene::new();
    let t = tool("resizehole", vec![field("face", FieldKind::PFace { any_face: false })], ToolFlags::default());
    let mut v = Values::new();
    let out = pface::feed_pick(&t, &t.fields[0], &mut v, &hit_body(2, [3., 3., 10.], [0., 0., 1.]), &sc.ctx());
    assert!(out.changed && out.advance);
    assert_eq!(v["face"]["label"], json!("Face of Box"));
    assert_eq!(v["body"], json!("b1"));
    assert_eq!(v["face"]["contours"].as_array().unwrap().len(), 1, "one outline ring");
    assert_eq!(v["face"]["contours"][0].as_array().unwrap().len(), 4);
    assert_eq!(out.highlight.as_ref().unwrap().1, vec![2, 3]);
    // a face that isn't flat (one lone triangle) is refused with 1.x's words
    let mut sc2 = Scene::new();
    sc2.bodies = vec![body("b1", "Box", MeshArrays { positions: vec![0., 0., 0., 1., 0., 0., 0., 1., 0.], triangles: vec![0, 1, 2] })];
    let mut v = Values::new();
    let out = pface::feed_pick(&t, &t.fields[0], &mut v, &hit_body(0, [0.2, 0.2, 0.], [0., 0., 1.]), &sc2.ctx());
    assert_eq!(out.message, Some((Level::Error, "That face isn't flat. Pick a flat face.".to_owned())));
    assert!(v.is_empty());
    // Lay Flat's face field takes a curved face as a point
    let lay = tool("layflat", vec![field("face", FieldKind::PFace { any_face: true })], ToolFlags::default());
    let out = pface::feed_pick(&lay, &lay.fields[0], &mut v, &hit_body(0, [0.2, 0.2, 0.], [0., 0., 1.]), &sc2.ctx());
    assert!(out.changed);
    assert_eq!(v["face"]["label"], json!("Point on Box"));
}

#[test]
fn pick_cylface() {
    let mut sc = Scene::new();
    sc.bodies = vec![body("b1", "Pin", pin())];
    let f = field("face", FieldKind::CylFace);
    let mut v = Values::new();
    // triangle 0 is on the side wall
    let out = cylface::feed_pick(&f, &mut v, &hit_body(0, [5., 0., 10.], [1., 0., 0.]), &sc.ctx());
    assert!(out.changed, "{:?}", out.message);
    assert_eq!(v["kind"], json!("pin"));
    assert_eq!(v["measured"], json!(10.0));
    assert_eq!(v["diameter"], json!(10.0));
    assert_eq!(v["size"], json!("custom"));
    assert_eq!(v["body"], json!("b1"));
    assert_eq!(v["face"]["label"], json!("Pin Ø10 mm × 20 mm deep"));
    assert_eq!(out.message, Some((Level::Info, "Measured pin Ø10 mm. Type the new diameter.".to_owned())));
    assert!(!out.highlight.unwrap().1.is_empty());
    // a diameter the user already typed stays
    let mut v = Values::new();
    v.insert("_sized".into(), json!(true));
    v.insert("diameter".into(), json!(6.0));
    cylface::feed_pick(&f, &mut v, &hit_body(0, [5., 0., 10.], [1., 0., 0.]), &sc.ctx());
    assert_eq!(v["diameter"], json!(6.0));
    // inch mode: the label and message use inches (U.str); the values stay in millimetres
    let mut v = Values::new();
    let mut c = sc.ctx();
    c.inch = true;
    let out = cylface::feed_pick(&f, &mut v, &hit_body(0, [5., 0., 10.], [1., 0., 0.]), &c);
    assert_eq!(v["face"]["label"], json!("Pin Ø0.3937 in × 0.7874 in deep"));
    assert_eq!(out.message, Some((Level::Info, "Measured pin Ø0.3937 in. Type the new diameter.".to_owned())));
    assert_eq!(v["measured"], json!(10.0));
    assert_eq!(v["diameter"], json!(10.0));
    // a flat wall is not round
    let sc = Scene::new();
    let out = cylface::feed_pick(&f, &mut Values::new(), &hit_body(2, [3., 3., 10.], [0., 0., 1.]), &sc.ctx());
    assert_eq!(out.message.as_ref().map(|m| m.0), Some(Level::Error));
    assert!(!out.changed);
}

#[test]
fn pick_points3() {
    let sc = Scene::new();
    let t = tool("plane_3pt", vec![field("body", FieldKind::Bodies { multi: true }), field("points", FieldKind::Points3)], ToolFlags::default());
    let f = &t.fields[1];
    let mut v = Values::new();
    for (i, p) in [[0., 0., 10.], [10., 0., 10.], [10., 10., 10.]].iter().enumerate() {
        let out = points3::feed_pick(&t, f, &mut v, &hit_body(2, *p, [0., 0., 1.]), &sc.ctx());
        assert!(out.changed);
        assert_eq!(v["points"].as_array().unwrap().len(), i + 1);
    }
    assert_eq!(v["body"], json!(["b1"]), "the tool's empty body field takes the body the points are on");
    // a fourth click starts over; a plane hit counts too and leaves the body alone
    v.insert("body".into(), json!(["other"]));
    points3::feed_pick(&t, f, &mut v, &Hit::Plane { id: "XY".into(), point: [1., 2., 0.] }, &sc.ctx());
    assert_eq!(v["points"], json!([[1.0, 2.0, 0.0]]));
    assert_eq!(v["body"], json!(["other"]));
    assert_eq!(points3::marks(f, &v), vec![Mark::Marker { p: [1., 2., 0.], color: 0xd9480f }]);
}

// ------------------------------------------------------------------ editing a step from the timeline

struct Tl {
    tl: Timeline,
    view: TimelineView,
    events: Rc<RefCell<Vec<UiEvent>>>,
    tool_open: bool,
}

fn tl_harness(st: Tl) -> Harness<'static, Tl> {
    Harness::builder().with_size(egui::vec2(720., 80.)).with_step_dt(0.05).with_accessibility_check(false).build_ui_state(
        |ui, st: &mut Tl| {
            stl_app::fonts::install(ui.ctx());
            let tk = theme::tokens(true);
            theme::apply(ui.ctx(), &tk);
            let ctx = ui.ctx().clone();
            let mut commands: Vec<Command> = Vec::new();
            let mut events: Vec<UiEvent> = Vec::new();
            timeline::show(ui, &ctx, &tk, &mut st.tl, &st.view, st.tool_open, false, &mut commands, &mut events);
            st.events.borrow_mut().extend(events);
        },
        st,
    )
}

fn tl_view() -> TimelineView {
    TimelineView { items: vec![item("f1", "extrude", "Extrude 1", json!({})), item("f2", "hole", "Hole 1", json!({}))], marker: None, can_undo: false, can_redo: false }
}

/// The timeline's button for a step (the status line says the same words).
fn step_button<'a, 'h, S>(h: &'a Harness<'h, S>, label: &'a str) -> egui_kittest::Node<'a> {
    h.get_by(|n| n.role() == egui::accesskit::Role::Button && n.label().as_deref() == Some(label))
}

fn double_click<S>(h: &mut Harness<'_, S>, label: &str) {
    let at = step_button(h, label).rect().center();
    // the pointer arrives first, so the layer under it is known when the buttons go down
    h.event(egui::Event::PointerMoved(at));
    h.step();
    for _ in 0..2 {
        h.event(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() });
        h.step();
        h.event(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() });
        h.step();
    }
    h.run();
}

/// Editing a step from the timeline, end to end in the real app: a double-click (or the menu's
/// Edit) reopens that step's tool and rolls the timeline back to it; Cancel and OK put the old
/// marker back. While a tool with a features list is open, a click on a repeatable step ticks it
/// instead.
#[test]
fn edit_rolls_back_and_restores() {
    use std::sync::Mutex;
    use stl_app::app::StlApp;
    use stl_app::engine_link::{EngineLink, Update};

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
    let rec = Rec::default();
    let app = StlApp::new(&egui::Context::default(), Box::new(stl_app::platform::NullPlatform), Box::new(rec.clone()));
    let view = TimelineView { items: vec![item("f1", "extrude", "Extrude 1", json!({})), item("f2", "primitive", "Box 1", json!({"shape": "box", "length": 10, "width": 10, "height": 10, "plane": "XY"}))], marker: None, can_undo: false, can_redo: false };
    rec.inbox.lock().unwrap().push(Update::Timeline(view));
    let mut init = false;
    // two clicks count as a double-click when they are under 0.3 s apart; the default test frame is 0.25 s
    let mut h = Harness::builder().with_size(egui::vec2(1280.0, 800.0)).with_step_dt(0.05).with_accessibility_check(false).build_ui_state(
        move |ui, a: &mut StlApp| {
            if !init {
                init = true;
                stl_app::fonts::install(ui.ctx());
                a.set_theme(ui.ctx(), stl_app::ui::ThemePref::Dark);
            }
            a.layout(ui)
        },
        app,
    );
    h.run();
    let rolls = |rec: &Rec| -> Vec<Option<usize>> {
        rec.sent.lock().unwrap().iter().filter_map(|c| if let Command::Rollback(i) = c { Some(*i) } else { None }).collect()
    };
    // a double-click opens the step's tool, rolled back to just before it
    double_click(&mut h, "Box 1");
    let t = h.state().tool().expect("the tool of the step opened");
    assert_eq!((t.def.key, t.edit_id.as_deref()), ("primitive_box", Some("f2")));
    assert_eq!(rolls(&rec), vec![Some(1)]);
    // Cancel puts the marker back where it was (the end)
    h.state_mut().tool_cancel();
    h.run();
    assert_eq!(rolls(&rec), vec![Some(1), None]);
    assert!(h.state().tool().is_none());
    // OK edits the step and puts the marker back too
    double_click(&mut h, "Box 1");
    h.state_mut().tool_set_value("length", json!(12.0));
    h.state_mut().tool_ok();
    rec.inbox.lock().unwrap().push(Update::Timeline(TimelineView { items: vec![item("f1", "extrude", "Extrude 1", json!({})), item("f2", "primitive", "Box 1", json!({}))], marker: None, can_undo: false, can_redo: false }));
    h.run();
    let cmds = rec.sent.lock().unwrap().clone();
    let at = cmds.iter().position(|c| matches!(c, Command::EditFeature { id, .. } if id == "f2")).expect("the step is edited");
    assert!(matches!(&cmds[at], Command::EditFeature { rollback_after: Some(None), .. }), "the edit carries the marker: {:?}", &cmds[at..]);
    // the menu's Edit does the same for another step
    step_button(&h, "Extrude 1").click_secondary();
    h.run();
    h.get_by_label("Edit…").click();
    h.run();
    assert_eq!(h.state().tool().and_then(|t| t.edit_id.clone()).as_deref(), Some("f1"));
    h.state_mut().tool_cancel();
    h.run();
    assert_eq!(rolls(&rec).last(), Some(&None));
}

#[test]
fn timeline_ticks_steps_for_a_features_list() {
    let events = Rc::new(RefCell::new(Vec::new()));
    let mut h = tl_harness(Tl { tl: Timeline::default(), view: tl_view(), events: events.clone(), tool_open: false });
    h.run();
    // double-click and the menu's Edit ask the app to reopen the step
    double_click(&mut h, "Hole 1");
    assert!(events.borrow().contains(&UiEvent::EditFeature("f2".into())), "{:?}", events.borrow());
    assert_eq!(h.state_mut().tl.take_edit_feature().as_deref(), Some("f2"));
    // a features list is open: a click ticks the step and shows no status
    events.borrow_mut().clear();
    h.state_mut().tl.take_status();
    h.state_mut().tl.features_pick = true;
    h.get_by_label("Extrude 1").click();
    h.run();
    assert_eq!(h.state_mut().tl.take_ticked().as_deref(), Some("f1"));
    assert!(h.state_mut().tl.take_status().is_none());
    assert!(events.borrow().is_empty());
}

// ------------------------------------------------------------------ ribbon and search

/// The 41 keys of `TOOLS` in web/js/tools.js.
const TOOLS_1X: [&str; 41] = [
    "extrude", "revolve", "loft", "sweep", "primitive_box", "primitive_cylinder", "primitive_sphere", "primitive_cone", "primitive_torus", "snapfit", "hinge", "hole",
    "combine", "split", "bedsplit", "plate", "layflat", "thread", "offset", "thincheck", "fittest", "resizehole", "pushpull", "mirror", "move", "scale", "pattern",
    "hollow", "lattice", "shell", "plane_offset", "plane_angle", "plane_3pt", "plane_mid", "import", "sculpt", "remesh", "repair", "simplify", "separate", "delete",
];

#[test]
fn tools_list_matches_tools_js() {
    let src = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../web/js/tools.js")).unwrap().replace("\r\n", "\n");
    let at = src.find("export const TOOLS = {").expect("TOOLS");
    let mut keys = Vec::new();
    for l in src[at..].lines().skip(1) {
        if l.starts_with("};") {
            break;
        }
        if let Some(rest) = l.strip_prefix("  ") {
            if !rest.starts_with(' ') {
                if let Some((k, _)) = rest.split_once(": {") {
                    keys.push(k.to_owned());
                }
            }
        }
    }
    // the five primitives are written as `primitive_box: primitive(...)`, not `key: {`, so the scan skips them
    assert!(src.contains("primitive_"), "tools.js still defines the primitive tools");
    let want: Vec<&str> = TOOLS_1X.iter().copied().filter(|k| !k.starts_with("primitive_")).collect();
    assert_eq!(keys, want, "the hard-coded list is the tools.js list");
}

fn ribbon_keys() -> Vec<&'static str> {
    let mut out = Vec::new();
    for (tab, _) in ribbon::TABS {
        let d = ribbon::def(tab);
        for g in d.groups.iter().chain(std::iter::once(&d.pinned)) {
            out.extend(g.tools().iter().map(|t| t.key));
        }
    }
    out
}

#[test]
fn ribbon_opens_every_tool() {
    let on_ribbon = ribbon_keys();
    let commands = ribbon::all_commands();
    let mut keys: Vec<&str> = TOOLS_1X.to_vec();
    // tools() is the registry (Task 1): every key in it is reachable too
    keys.extend(stl_app::tools::tools().iter().map(|t| t.key));
    for k in keys {
        assert!(on_ribbon.contains(&k) || commands.iter().any(|c| c.k == k), "tool {k} has no ribbon button, menu entry or search entry");
        // choosing it opens it
        assert_eq!(ribbon::tool_event(k), UiEvent::OpenTool(k.to_owned()));
    }
    // every ribbon button's key either is a tool, or one of the actions the app handles itself
    let actions = ["sketch", "params", "measure", "section", "info", "export", "send", "to_step", "supportfins", "supportwall", "sf_about", "rotate"];
    for k in &on_ribbon {
        assert!(TOOLS_1X.contains(k) || actions.contains(k), "ribbon key {k} is neither a tool nor a known action");
    }
    // 1.x's onRibbon sends these (search and More menu rows included): the SUPPORTS tab opens its two panels
    let sup: Vec<&str> = ribbon::def("supports").groups.iter().flat_map(|g| g.tools()).map(|t| t.key).collect();
    assert!(sup.contains(&"supportfins") && sup.contains(&"supportwall"));
    assert_eq!(ribbon::tool_event("supportwall"), UiEvent::OpenTool("supportwall".into()));
    // Rotate is not a tool of its own: it opens Move with the rings (app.js:346)
    let Value::Object(rot) = json!({"_gizmo": "rotate"}) else { unreachable!() };
    assert_eq!(ribbon::tool_event("rotate"), UiEvent::OpenToolWith("move".into(), rot));
    assert!(on_ribbon.contains(&"rotate") || commands.iter().any(|c| c.k == "rotate"), "Rotate has a button or a search entry");
}

#[test]
fn search_finds_every_tool() {
    let commands = ribbon::all_commands();
    for k in TOOLS_1X.iter().chain(["supportfins", "supportwall", "rotate"].iter()) {
        let c = commands.iter().find(|c| c.k == *k).unwrap_or_else(|| panic!("{k} is not in the tool search"));
        let hits = ribbon::search(&c.l);
        assert!(hits.iter().any(|h| h.k == *k), "searching for \"{}\" does not find {k}: {:?}", c.l, hits.iter().map(|h| &h.k).collect::<Vec<_>>());
        // by key words too ("plane 3pt", "primitive box")
        let words = k.replace('_', " ");
        assert!(ribbon::search(&words).iter().any(|h| h.k == *k), "searching for \"{words}\" does not find {k}");
    }
    assert!(ribbon::search("").is_empty());
}

#[test]
fn rotate_opens_move_gizmo() {
    match ribbon::tool_event("rotate") {
        UiEvent::OpenToolWith(k, v) => {
            assert_eq!(k, "move");
            assert_eq!(v.get("_gizmo"), Some(&json!("rotate")));
        }
        e => panic!("Rotate must open Move with the rotate gizmo, got {e:?}"),
    }
    // Move itself opens plain, and Rotate is the Print tab's second Position button
    assert_eq!(ribbon::tool_event("move"), UiEvent::OpenTool("move".into()));
    assert!(ribbon::def("print").groups[0].tools().iter().any(|t| t.key == "rotate"));
    // the search finds it too
    assert!(ribbon::search("rotate").iter().any(|c| c.k == "rotate"));
}

/// 1.x `validTool` and `advanceField` skip a `bodies` field that has any placeholder
/// ("Auto (bodies it touches)", "The part you draw on"), not only the exact word "Auto".
#[test]
fn any_placeholder_makes_a_bodies_field_optional() {
    for ph in ["Auto", "Auto (bodies it touches)", "The part you draw on"] {
        let mut opt = field("targets", FieldKind::Bodies { multi: true });
        opt.placeholder = Some(ph);
        let first = field("body", FieldKind::Bodies { multi: false });
        let t: &'static ToolDef = Box::leak(Box::new(tool("t", vec![first, opt], ToolFlags::default())));
        let mut v = Values::new();
        v.insert("body".into(), json!(["b1"]));
        let open = stl_app::tools::panel::OpenTool::new(t, v.clone(), None, None);
        assert!(open.valid(), "{ph}: an empty optional field doesn't block OK");
        // advancing from `body` doesn't land on the optional field, and it isn't the first active one
        let mut e = Values::new();
        e.insert("body".into(), json!([]));
        assert_eq!(bodies::advance_field(t, &e, "body"), None, "{ph}");
        assert_eq!(bodies::first_active(t, &e).as_deref(), Some("body"), "{ph}");
    }
    // without a placeholder the field is required and is advanced to
    let t: &'static ToolDef = Box::leak(Box::new(tool("t", vec![field("a", FieldKind::Bodies { multi: false }), field("b", FieldKind::Bodies { multi: false })], ToolFlags::default())));
    assert_eq!(bodies::advance_field(t, &Values::new(), "a").as_deref(), Some("b"));
}

#[test]
fn text_field_placeholder_is_always_optional() {
    use stl_app::tools::fields::text::placeholder_text;
    assert_eq!(placeholder_text(None), "optional");
    assert_eq!(placeholder_text(Some("anything the tool says")), "optional");
}
