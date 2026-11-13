//! Support Fins panel: schema, saved values, modes, Held card, sections, edit-not-stack,
//! instant overhangs, strength and suggested orientations.

use std::path::Path;
use std::sync::{Arc, OnceLock};

use serde_json::{json, Value};
use stl_app::engine_link::{BodyView, TimelineItem};
use stl_app::supports::walls::hint_for;
use stl_app::supports::{self, Cover, OverhangOverlay, Plastic, SectionMemory, SpecKind, Spot, SuggestUi};
use stl_app::tools::Ctx;
use stl_engine::features::common::fins;
use stl_engine::{FinsHost, HostKind};
use stl_kernel::MeshArrays;

fn ensure() {
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(|| {
        let host = SchemaHost { schema: schema_json(), info: version_json() };
        let info = fins::info(&host).expect("fins::info");
        supports::install(&info).expect("install schema");
    });
}

struct SchemaHost {
    schema: String,
    info: String,
}

impl FinsHost for SchemaHost {
    fn call(&self, name: &str, _args: &[&str]) -> Result<String, String> {
        match name {
            "optionsSchemaJson" => Ok(self.schema.clone()),
            "__info" => Ok(self.info.clone()),
            other => Err(format!("test host does not answer {other}")),
        }
    }
    fn has(&self, name: &str) -> bool {
        matches!(name, "drawWallThroughB64" | "gussetWallB64" | "anchorFinB64" | "suggestPosesJson")
    }
    fn host_kind(&self) -> HostKind {
        HostKind::None
    }
}

fn schema_json() -> String {
    let engine = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../web/vendor/support-fins/engine.esm.js");
    assert!(engine.exists(), "missing {}", engine.display());
    let url = file_url(&engine);
    let script = format!("import * as SF from {}; process.stdout.write(JSON.stringify(SF.OPTIONS_SCHEMA));", serde_json::to_string(&url).unwrap());
    let out = std::process::Command::new("node").arg("--input-type=module").arg("-e").arg(script).output().expect("node");
    assert!(out.status.success(), "node schema: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout).expect("schema utf-8")
}

fn version_json() -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../web/vendor/support-fins/version.js");
    let t = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{e}"));
    let start = t.find('{').expect("version object");
    let end = t.rfind('}').expect("version end");
    t[start..=end].to_string()
}

fn file_url(p: &Path) -> String {
    let abs = std::fs::canonicalize(p).unwrap();
    let s = abs.to_string_lossy();
    let s = s.trim_start_matches(r"\\?\").replace('\\', "/").replace(' ', "%20");
    format!("file:///{s}")
}

fn mesh() -> Arc<MeshArrays> {
    Arc::new(MeshArrays { positions: vec![], triangles: vec![] })
}

fn body(id: &str, hidden: bool) -> BodyView {
    BodyView { id: id.into(), name: id.into(), color: None, hidden, solid: true, mesh: mesh(), revision: 1 }
}

fn project<'a>(bodies: &'a [BodyView], features: &'a [TimelineItem], selection: &'a [String]) -> Ctx<'a> {
    Ctx { bodies, planes: &[], sketches: &[], features, selection, last_sketch: Some("sk1"), sketch_on_body: true }
}

fn fixture() -> (Vec<BodyView>, Vec<String>) {
    (vec![body("body_a", false), body("body_b", false)], vec!["body_a".into()])
}

fn step(id: &str, kind: &str, label: &str, params: Value, active: bool, suppressed: bool) -> TimelineItem {
    TimelineItem { id: id.into(), kind: kind.into(), label: label.into(), error: None, suppressed, warning: None, active, params }
}

struct Paint {
    clears: u32,
    asks: Vec<(String, f64)>,
    faces: Vec<u32>,
    highlighted: Vec<(String, Vec<u32>, [u8; 3])>,
}

impl OverhangOverlay for Paint {
    fn clear_support_paint(&mut self) {
        self.clears += 1;
    }
    fn overhang_faces(&mut self, body: &str, threshold_deg: f64) -> Vec<u32> {
        self.asks.push((body.to_string(), threshold_deg));
        self.faces.clone()
    }
    fn highlight_overhangs(&mut self, body: &str, faces: &[u32], rgb: [u8; 3]) {
        self.highlighted.push((body.to_string(), faces.to_vec(), rgb));
    }
}

#[test]
fn schema_builds_panel() {
    ensure();
    assert_eq!(supports::engine_version(), "1.0.5");
    assert!(supports::orient_available());
    assert!(supports::engine_features().curves);

    let (bodies, sel) = fixture();
    let ctx = project(&bodies, &[], &sel);
    let defs = supports::defaults("supportfins", &ctx);
    assert_eq!(defs.get("body"), Some(&json!(["body_a"])));
    assert_eq!(defs.get("_mode"), Some(&json!("rotate")));
    assert_eq!(defs.get("output"), Some(&json!("join")));
    assert_eq!(defs.get("sf:threshold"), Some(&json!(45)));
    assert_eq!(defs.get("sf:tineDensity"), Some(&json!(0)));
    assert_eq!(defs.get("sf:coverage"), Some(&json!(50)));
    assert_eq!(defs.get("sf:sway.reach"), Some(&json!(15)));
    assert_eq!(defs.get("sf:nozzle"), Some(&json!("0.4")));
    assert_eq!(defs.get("sf:tines"), Some(&json!(true)));

    let specs = supports::fields("supportfins");
    let threshold = specs.iter().find(|f| f.key == "sf:threshold").unwrap();
    assert_eq!(threshold.label, "Overhang");
    assert_eq!(threshold.kind, SpecKind::Number);
    assert_eq!(threshold.unit, Some("°"));
    assert!(threshold.help.is_some());
    let density = specs.iter().find(|f| f.key == "sf:tineDensity").unwrap();
    assert_eq!(density.unit, Some("%"));
    assert!(supports::field_visible(density, &defs));
    let mut hidden = defs.clone();
    hidden.insert("sf:tines".into(), json!(false));
    assert!(!supports::field_visible(density, &hidden));
    let pin = specs.iter().find(|f| f.key == "sf:pinDepth").unwrap();
    assert!(!supports::field_visible(pin, &defs));
    let coverage = specs.iter().find(|f| f.key == "sf:coverage").unwrap();
    assert!(supports::field_visible(coverage, &defs));
    hidden.insert("sf:mode".into(), json!("full"));
    assert!(!supports::field_visible(coverage, &hidden));
    let separate = specs.iter().find(|f| f.key == "sf:separateWalls").unwrap();
    assert!(supports::field_visible(separate, &hidden));

    assert!(supports::section_default_open("supportfins", "_hdr_placement"));
    assert!(supports::section_default_open("supportfins", "_hdr_setup"));
    assert!(!supports::section_default_open("supportfins", "_hdr_tines"));
    assert!(supports::section_default_open("supportfins", "_hdr_check"));
    assert!(supports::section_default_open("supportwall", "_hdr_setup"));
    assert!(!supports::section_default_open("supportwall", "_hdr_tines"));

    let wall = supports::fields("supportwall");
    assert!(wall.iter().any(|f| f.key == "sf:threshold"));
    assert!(wall.iter().all(|f| f.key != "sf:mode" && f.key != "_mode" && f.key != "_pose" && f.key != "_reach"));
    assert_eq!(wall.iter().find(|f| f.key == "body").unwrap().placeholder, Some("The part you draw on"));

    let tool = supports::support_tool("supportfins").unwrap();
    assert_eq!(tool.kind, "supportfins");
    assert!(tool.flags.paint && tool.flags.gizmo && !tool.flags.wall_tool);
    assert_eq!(tool.hint, Some(supports::FINS_HINT));
    assert_eq!(supports::gizmo_mode("supportfins"), Some("pose"));
    assert!(tool.fields.iter().any(|f| f.key == "sf:threshold" && f.show.is_some()));
    let wall_tool = supports::support_tool("supportwall").unwrap();
    assert!(wall_tool.flags.wall_tool && wall_tool.flags.paint && !wall_tool.flags.gizmo);

    let params = supports::to_params("supportfins", &defs, &ctx);
    assert_eq!(params.get("options"), Some(&json!({})));
    assert_eq!(params.get("body"), Some(&json!("body_a")));
    assert!(params.get("walls").is_none());

    assert_eq!(supports::credit_line("1.0.5"), "Support Fins 1.0.5 · gittrahan (MIT)");
    assert_eq!(supports::credit_line(""), "Support Fins  · gittrahan (MIT)");
    assert!(supports::CREDIT_TITLE.contains("Help"));
    let about = supports::about(false).unwrap();
    assert!(about.paragraphs[0].contains("unchanged"));
    assert!(about.paragraphs[2].contains("Update Settings"));
    assert_eq!(supports::preview_ask("supportfins", &defs, &ctx).unwrap().fins_only, true);
    assert_eq!(supports::preview_ask("supportwall", &defs, &ctx).unwrap().busy, supports::BUSY_WALLS);

    let lone = vec![body("only", false)];
    let none: Vec<String> = vec![];
    assert_eq!(supports::defaults("supportfins", &project(&lone, &[], &none)).get("body"), Some(&json!(["only"])));
    let hidden_only = vec![body("only", true)];
    assert_eq!(supports::defaults("supportfins", &project(&hidden_only, &[], &none)).get("body"), Some(&json!([])));
    let pick_b = vec!["body_b".into()];
    assert_eq!(supports::defaults("supportfins", &project(&bodies, &[], &pick_b)).get("body"), Some(&json!(["body_b"])));

    check_golden("supportfins", &ctx);
    check_golden("supportwall", &ctx);
}

fn check_golden(tool: &str, ctx: &Ctx) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../tests/parity/tools/{tool}.json"));
    let raw = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("golden {}: {e}", path.display()));
    let g: Value = serde_json::from_str(&raw.replace("\r\n", "\n")).unwrap();
    let def = supports::support_tool(tool).unwrap();
    assert_eq!(g["tool"].as_str(), Some(tool));
    assert_eq!(g["label"].as_str(), Some(def.label));
    assert_eq!(g["type"].as_str(), Some(def.kind));
    assert_eq!(g["icon"].as_str(), Some(def.icon));
    assert_eq!(g["hint"].as_str(), def.hint, "{tool} hint");
    assert!(g["extra"].is_null());
    let flags = &g["flags"];
    assert_eq!(flags["placeOnFace"].as_bool(), Some(false));
    assert_eq!(flags["noLivePreview"].as_bool(), Some(false));
    assert_eq!(flags["planeTool"].as_bool(), Some(false));
    assert_eq!(flags["sculpt"].as_bool(), Some(false));
    assert_eq!(flags["paint"].as_bool(), Some(def.flags.paint));
    assert_eq!(flags["wallTool"].as_bool(), Some(def.flags.wall_tool));
    match &flags["gizmo"] {
        Value::String(s) => {
            assert_eq!(s, "pose");
            assert!(def.flags.gizmo);
            assert_eq!(supports::gizmo_mode(tool), Some("pose"));
        }
        Value::Bool(b) => {
            assert_eq!(*b, def.flags.gizmo);
            assert!(supports::gizmo_mode(tool).is_none());
        }
        other => panic!("{tool} gizmo flag {other}"),
    }

    let defs = supports::defaults(tool, ctx);
    assert_eq!(Value::Object(defs.clone()), g["defaults"], "{tool} defaults");
    assert_eq!(supports::to_params(tool, &defs, ctx), g["toParams_defaults"], "{tool} toParams");
    let shown = supports::shown_keys(tool, &defs);
    let expect: Vec<_> = g["shown_defaults"].as_array().unwrap().iter().filter_map(|v| v.as_str()).collect();
    assert_eq!(shown, expect, "{tool} shown_defaults");

    let gfields = g["fields"].as_array().unwrap();
    let specs = supports::fields(tool);
    assert_eq!(specs.len(), gfields.len(), "{tool} field count");
    for (spec, gf) in specs.iter().zip(gfields) {
        assert_field(tool, spec, gf);
    }
    for case in g["cases"].as_array().unwrap() {
        let values = case["values"].as_object().unwrap().clone();
        let name = case["name"].as_str().unwrap();
        assert_eq!(supports::to_params(tool, &values, ctx), case["toParams"], "{tool} {name}");
        let shown = supports::shown_keys(tool, &values);
        let expect: Vec<_> = case["shown"].as_array().unwrap().iter().filter_map(|v| v.as_str()).collect();
        assert_eq!(shown, expect, "{tool} {name} shown");
    }
    for trip in g["roundTrips"].as_array().unwrap() {
        let name = trip["name"].as_str().unwrap();
        let from = supports::from_params(tool, trip["params"].clone());
        assert_eq!(Value::Object(from.clone()), trip["fromParams"], "{tool} {name} fromParams");
        assert_eq!(supports::to_params(tool, &from, ctx), trip["toParams"], "{tool} {name} toParams");
    }
}

fn assert_field(tool: &str, spec: &stl_app::supports::FieldSpec, gf: &Value) {
    let where_ = format!("{tool} {}", spec.key);
    assert_eq!(spec.key, gf["key"].as_str().unwrap(), "{where_}");
    assert_eq!(spec.kind_name(), gf["kind"].as_str().unwrap(), "{where_}");
    assert_eq!(spec.label, gf["label"].as_str().unwrap_or(""), "{where_}");
    if let Some(u) = gf.get("unit").and_then(Value::as_str) {
        assert_eq!(spec.unit, Some(u), "{where_} unit");
    }
    if spec.key == "_reach" {
        assert!(gf.get("unit").is_none(), "{where_} golden has no unit");
        assert!(spec.unit.is_none(), "{where_} has no unit in 1.x");
    }
    if let Some(h) = gf.get("help").and_then(Value::as_str) {
        assert_eq!(spec.help, Some(h), "{where_} help");
    }
    if let Some(p) = gf.get("placeholder").and_then(Value::as_str) {
        assert_eq!(spec.placeholder, Some(p), "{where_} placeholder");
    }
    if let Some(m) = gf.get("multi").and_then(Value::as_bool) {
        assert_eq!(spec.multi, m, "{where_} multi");
    }
    if let Some(opts) = gf.get("options").and_then(Value::as_array) {
        assert_eq!(spec.options.len(), opts.len(), "{where_} options");
        for ((id, label), g) in spec.options.iter().zip(opts) {
            let g = g.as_array().expect("option pair");
            assert_eq!(*id, golden_id(&g[0]), "{where_} option");
            assert_eq!(*label, g.get(1).and_then(Value::as_str).unwrap_or(""), "{where_} option label");
        }
    }
    let has_show = gf.get("hasShow").and_then(Value::as_bool).unwrap_or(false);
    assert_eq!(spec.show.is_some(), has_show, "{where_} show");
}

fn golden_id(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        other => other.to_string(),
    }
}

#[test]
fn only_changed_values_saved() {
    ensure();
    let (bodies, sel) = fixture();
    let ctx = project(&bodies, &[], &sel);
    let mut v = supports::defaults("supportfins", &ctx);
    let opts = |v: &stl_app::tools::Values| supports::to_params("supportfins", v, &ctx).get("options").cloned().unwrap();
    assert_eq!(opts(&v), json!({}));

    v.insert("sf:threshold".into(), json!(50));
    assert_eq!(opts(&v), json!({"threshold": 50}));
    v.insert("sf:threshold".into(), json!(45));
    assert_eq!(opts(&v), json!({}));
    v.insert("sf:threshold".into(), json!("40+5"));
    assert_eq!(opts(&v), json!({}), "40+5 is the default overhang");
    v.insert("sf:threshold".into(), json!("=width/2"));
    assert_eq!(opts(&v), json!({"threshold": 0}), "1.x parseNum turns an expression into 0");
    v.insert("sf:threshold".into(), json!(""));
    assert_eq!(opts(&v), json!({}));

    v.insert("sf:tineDensity".into(), json!(20));
    assert_eq!(opts(&v).get("tineDensity"), Some(&json!(0.2)));
    v.insert("sf:coverage".into(), json!(50));
    assert!(opts(&v).get("coverage").is_none());
    v.insert("sf:tines".into(), json!(false));
    assert_eq!(opts(&v).get("tines"), Some(&json!(false)));
    v.insert("sf:mode".into(), json!("full"));
    assert_eq!(opts(&v).get("mode"), Some(&json!("full")));

    v.insert("walls".into(), json!([{"shape": "line", "points": [[0, 0, 0], [1, 0, 0]]}]));
    v.insert("removed".into(), json!([{"kind": "wall", "c": [0, 0, 0], "size": [1, 1, 1]}]));
    let p = supports::to_params("supportfins", &v, &ctx);
    assert!(p.get("walls").unwrap().as_array().unwrap().len() == 1);
    assert!(p.get("removed").unwrap().as_array().unwrap().len() == 1);

    let back = supports::from_params("supportfins", json!({"body": "body_a", "options": {"threshold": 50, "sway.reach": 0.2}}));
    assert_eq!(back.get("sf:threshold"), Some(&json!(50)));
    assert_eq!(back.get("sf:sway.reach"), Some(&json!(20)));
    assert_eq!(back.get("sf:gap"), supports::defaults("supportfins", &ctx).get("sf:gap"));
    let again = supports::to_params("supportfins", &back, &ctx);
    assert_eq!(again["options"]["threshold"], json!(50));
    assert_eq!(again["options"]["sway.reach"], json!(0.2));

    let mut wall = supports::defaults("supportwall", &ctx);
    wall.insert("sf:mode".into(), json!("full"));
    wall.insert("sf:gap".into(), json!(0.4));
    let wp = supports::to_params("supportwall", &wall, &ctx);
    assert!(wp["options"].get("mode").is_none(), "mode is not a Draw Walls setting");
    assert_eq!(wp["options"]["gap"], json!(0.4));
    assert_eq!(wp["walls"], json!([]));

    let legacy = supports::from_params("supportwall", json!({"body": "body_a", "points": [[0, 0, 0], [2, 0, 0]]}));
    assert_eq!(legacy["walls"][0]["shape"], json!("line"));
    assert_eq!(legacy["walls"][0]["points"], json!([[0, 0, 0], [2, 0, 0]]));
}

#[test]
fn modes_switch() {
    ensure();
    let (bodies, sel) = fixture();
    let ctx = project(&bodies, &[], &sel);
    let mut v = supports::defaults("supportfins", &ctx);
    let wall_hint = hint_for("line");
    assert_eq!(supports::MODES, &[("rotate", "Rotate"), ("add", "Add fins"), ("remove", "Remove fins")]);

    let add = supports::set_mode(&mut v, "add", wall_hint);
    assert!(add.reset_walls && add.clear_preview_hover && add.rerender);
    assert!(!add.request_strength);
    let add_hint = format!("Draw walls by hand where the auto fins left a gap. {wall_hint}");
    assert_eq!(add.hint.as_deref(), Some(add_hint.as_str()));
    assert!(supports::shown_keys("supportfins", &v).contains(&"walls"));
    assert!(!supports::shown_keys("supportfins", &v).contains(&"removed"));

    let rot = supports::set_mode(&mut v, "rotate", wall_hint);
    assert!(rot.request_strength);
    assert_eq!(rot.hint.as_deref(), Some(supports::ROTATE_HINT));
    assert!(!supports::shown_keys("supportfins", &v).contains(&"walls"));

    let rem = supports::set_mode(&mut v, "remove", wall_hint);
    assert!(!rem.request_strength);
    assert_eq!(rem.hint.as_deref(), Some(supports::REMOVE_HINT));
    assert!(supports::shown_keys("supportfins", &v).contains(&"removed"));

    supports::set_mode(&mut v, "rotate", wall_hint);
    v.insert("removed".into(), json!([{"kind": "wall"}]));
    assert!(supports::shown_keys("supportfins", &v).contains(&"removed"), "a removed fin stays listed");

    let warn = "\"Block\" has no overhangs in this position. Turn it the way you want to print it (drag the rings), then add fins.";
    assert_eq!(supports::warning_line("rotate", warn), "");
    assert_eq!(supports::warning_line("add", warn), warn);
    assert_eq!(supports::warning_line("rotate", "Keep this. \"Block\" has no overhangs in this position. Next sentence. Still here."), "Keep this. Still here.");

    v.insert("_load".into(), json!("up"));
    let load = supports::on_form_change("supportfins", "_load", &v, wall_hint);
    assert!(load.request_strength && load.rerender && !load.quick_paint);
    let thr = supports::on_form_change("supportfins", "sf:threshold", &v, wall_hint);
    assert!(thr.quick_paint && !thr.request_strength);
    let body = supports::on_form_change("supportfins", "body", &v, wall_hint);
    assert!(body.quick_paint);
    let idle = supports::on_form_change("supportfins", "sf:gap", &v, wall_hint);
    assert_eq!(idle, supports::FormEffects::default());
    assert_eq!(supports::mode_hint("supportwall", &v, wall_hint), wall_hint);
}

#[test]
fn held_card() {
    ensure();
    let (bodies, sel) = fixture();
    let ctx = project(&bodies, &[], &sel);
    let mut v = supports::defaults("supportfins", &ctx);
    let dash = supports::status_of(&v, None, None, "pla", 5.0, true);
    assert!(dash.held.is_some());
    // A chosen body with no coverage answer yet is "Checking…", as sfStatusHTML does.
    assert_eq!(dash.held.as_ref().unwrap().value, "Checking…");
    assert_eq!(dash.plastic.value, "—");

    v.insert("body".into(), json!([]));
    let empty = supports::status_of(&v, None, None, "pla", 5.0, true);
    assert_eq!(empty.held.unwrap().value, "—");
    v.insert("body".into(), json!(["body_a"]));

    let checking = supports::status_of(&v, None, None, "pla", 5.0, true);
    assert_eq!(checking.held.as_ref().unwrap().value, "Checking…");
    assert_eq!(checking.held.as_ref().map(|c| c.tone), Some(supports::Tone::Busy));

    let none = Cover { none: true, busy: false, error: None, total: 0.0, held: 0.0, spots: vec![] };
    assert_eq!(supports::status_of(&v, Some(&none), None, "pla", 5.0, true).held.unwrap().value, "—");

    let err = Cover { none: false, busy: false, error: Some("too big".into()), total: 1.0, held: 0.0, spots: vec![] };
    let card = supports::status_of(&v, Some(&err), None, "pla", 5.0, true).held.unwrap();
    assert_eq!(card.value, "too big");
    assert_eq!(card.tone, supports::Tone::Bad);

    let flat = Cover { none: false, busy: false, error: None, total: 0.0, held: 0.0, spots: vec![] };
    let flat_card = supports::status_of(&v, Some(&flat), None, "pla", 5.0, true);
    assert_eq!(flat_card.held.as_ref().unwrap().value, "Nothing to hold up");
    assert_eq!(flat_card.held.as_ref().unwrap().sub, "Turn it to the way you’ll print it");
    v.insert("_mode".into(), json!("add"));
    assert_eq!(supports::status_of(&v, Some(&flat), None, "pla", 5.0, true).held.unwrap().sub, "");
    v.insert("_mode".into(), json!("rotate"));

    let all = Cover { none: false, busy: false, error: None, total: 4.0, held: 4.0, spots: vec![] };
    let all_card = supports::status_of(&v, Some(&all), None, "pla", 5.0, true).held.unwrap();
    assert_eq!(all_card.value, "All held");
    assert_eq!(all_card.tone, supports::Tone::Ok);

    let bare = Cover {
        none: false,
        busy: false,
        error: None,
        total: 10.0,
        held: 5.0,
        spots: vec![Spot { far: Some(3.14), ends: true }, Spot { far: Some(12.4), ends: false }],
    };
    let bare_card = supports::status_of(&v, Some(&bare), None, "pla", 5.0, true);
    let held = bare_card.held.unwrap();
    assert_eq!(held.value, "50% held");
    assert_eq!(held.sub, "2 bare spots");
    assert!(bare_card.note.starts_with("The worst bare spot (bright red) is 3.1 mm from a support."));
    assert!(bare_card.note.contains("Try Placement › Full coverage"));
    assert!(!bare_card.fill_gaps);
    v.insert("_mode".into(), json!("add"));
    let fill = supports::status_of(&v, Some(&bare), None, "pla", 5.0, true);
    assert!(fill.fill_gaps);
    assert!(!fill.note.contains("Fill gaps"), "the button is drawn inline, not spelled in the text");
    assert_eq!(fill.note_tail, "Or click a red spot to drop a fin there.");
    assert!(fill.note.ends_with("Add fins there."));
    v.insert("sf:mode".into(), json!("full"));
    let full = supports::status_of(&v, Some(&bare), None, "pla", 5.0, true);
    assert!(full.note.contains("Try more Wide-face coverage, or Add fins there."));
    let unheld = Cover { spots: vec![Spot { far: None, ends: false }], ..bare.clone() };
    assert!(supports::status_of(&v, Some(&unheld), None, "pla", 5.0, true).note.starts_with("Nothing holds the worst bare spot (bright red) up."));
    let capped = Cover { total: 10.0, held: 10.0, ..bare.clone() };
    assert_eq!(supports::status_of(&v, Some(&capped), None, "pla", 5.0, true).held.unwrap().value, "99% held");

    let plastic = Plastic { volume: 1000.0, pieces: 1 };
    let p = supports::status_of(&v, Some(&all), Some(&plastic), "pla", 5.0, true).plastic;
    assert_eq!(p.value, "1.2 g");
    assert_eq!(p.sub, "PLA · 1.0 cm³ · 1 piece");
    let heavy = Plastic { volume: 20000.0, pieces: 2 };
    let h = supports::status_of(&v, Some(&all), Some(&heavy), "petg", 5.0, true).plastic;
    assert_eq!(h.value, "25 g");
    assert!(h.sub.starts_with("PETG · 20.0 cm³ · 2 pieces"));
    let zero = Plastic { volume: 10.0, pieces: 0 };
    assert_eq!(supports::status_of(&v, Some(&all), Some(&zero), "pla", 5.0, true).plastic.value, "None needed");

    let wall_only = supports::status_of(&v, Some(&bare), Some(&plastic), "pla", 5.0, false);
    assert!(wall_only.held.is_none());
    assert_eq!(wall_only.plastic.value, "1.2 g");
    assert!(held.tip.contains("5 mm"));
}

#[test]
fn sections_remembered() {
    ensure();
    let (bodies, sel) = fixture();
    let ctx = project(&bodies, &[], &sel);
    let defs = supports::defaults("supportfins", &ctx);
    let mut mem = SectionMemory::default();
    assert!(mem.is_open("supportfins", "_hdr_placement", true));
    assert!(!mem.is_open("supportfins", "_hdr_tines", false));
    mem.toggle("supportfins", "_hdr_placement", true);
    assert!(!mem.is_open("supportfins", "_hdr_placement", true));
    assert!(SectionMemory::default().is_open("supportfins", "_hdr_placement", true), "a new session starts from the schema again");
    assert!(mem.is_open("supportwall", "_hdr_setup", true), "Draw Walls keeps its own folds");

    let mut v = defs.clone();
    v.insert("sf:threshold".into(), json!(60));
    mem.toggle("supportfins", "_hdr_setup", true);
    assert_eq!(supports::section_badge("supportfins", "_hdr_setup", &v, &defs, &mem, "pla", 5.0).as_deref(), Some("1 changed"));
    v.insert("sf:nozzle".into(), json!("0.6"));
    assert_eq!(supports::section_badge("supportfins", "_hdr_setup", &v, &defs, &mem, "pla", 5.0).as_deref(), Some("2 changed"));
    mem.toggle("supportfins", "_hdr_setup", true);
    assert!(supports::section_badge("supportfins", "_hdr_setup", &v, &defs, &mem, "pla", 5.0).is_none(), "an open section shows no count");

    v.insert("sf:tines".into(), json!(false));
    v.insert("sf:tineDensity".into(), json!(40));
    assert_eq!(
        supports::section_badge("supportfins", "_hdr_tines", &v, &defs, &mem, "pla", 5.0).as_deref(),
        Some("1 changed"),
        "a hidden tine-grip field does not count"
    );

    let mut closed = mem.clone();
    closed.toggle("supportfins", "_hdr_check", true);
    assert_eq!(supports::section_badge("supportfins", "_hdr_check", &v, &defs, &closed, "petg", 8.0).as_deref(), Some("2 changed"));
    assert_eq!(supports::section_badge("supportfins", "_hdr_check", &v, &defs, &closed, "pla", 5.0), None);
}

#[test]
fn edits_not_stacks() {
    ensure();
    let fins = step("f1", "supportfins", "Support Fins 1", json!({"body": "b1"}), true, false);
    let later = step("f2", "supportwall", "Draw Walls 1", json!({"body": "b1"}), true, false);
    let other = step("f3", "supportfins", "Support Fins 2", json!({"body": "b2"}), true, false);
    let off = step("f4", "supportfins", "Support Fins 3", json!({"body": "b1"}), false, false);
    let gone = step("f5", "supportfins", "Support Fins 4", json!({"body": "b1"}), true, true);
    let line = vec![fins.clone(), later.clone(), other, off, gone];
    assert_eq!(supports::reopen_supports(&line, "b1").as_deref(), Some("f2"));
    assert_eq!(supports::reopen_supports(&line, "b2").as_deref(), Some("f3"));
    assert!(supports::reopen_supports(&line, "missing").is_none());

    let plan = supports::reopen_plan(&line, "supportfins", "b1", "Bracket").unwrap();
    assert_eq!(plan.feature_id, "f2");
    assert_eq!(plan.tool_key, "supportwall");
    assert_eq!(plan.mode, None);
    assert_eq!(plan.toast, "Bracket already has Draw Walls 1: opened it to change it");

    let only_fins = vec![fins.clone()];
    let add = supports::reopen_plan(&only_fins, "supportwall", "b1", "Bracket").unwrap();
    assert_eq!(add.tool_key, "supportfins");
    assert_eq!(add.mode, Some("add"));
    let rot = supports::reopen_plan(&only_fins, "supportfins", "b1", "").unwrap();
    assert_eq!(rot.mode, Some("rotate"));
    assert!(rot.toast.starts_with("b1 already has Support Fins 1:"));

    let via_list = step("f6", "supportfins", "Support Fins 6", json!({"bodies": ["b9", "b1"]}), true, false);
    assert_eq!(supports::reopen_supports(&[via_list], "b1").as_deref(), Some("f6"));

    let suppressed_last = vec![fins, step("f7", "supportfins", "Hidden", json!({"body": "b1"}), true, true)];
    assert_eq!(supports::reopen_supports(&suppressed_last, "b1").as_deref(), Some("f1"));
}

#[test]
fn instant_overhangs() {
    ensure();
    let (bodies, sel) = fixture();
    let ctx = project(&bodies, &[], &sel);
    let mut v = supports::defaults("supportfins", &ctx);
    v.insert("body".into(), json!([]));
    let mut paint = Paint { clears: 0, asks: vec![], faces: vec![1, 2], highlighted: vec![] };
    let empty = supports::quick_paint(&v, false, &mut paint);
    assert_eq!(paint.clears, 1);
    assert!(paint.asks.is_empty() && paint.highlighted.is_empty());
    assert!(empty.message.is_none() && empty.clear_cover && empty.body.is_none());

    v.insert("body".into(), json!(["body_a"]));
    v.insert("sf:threshold".into(), json!("30"));
    let hit = supports::quick_paint(&v, false, &mut paint);
    assert_eq!(paint.asks, vec![("body_a".into(), 30.0)]);
    assert_eq!(paint.highlighted[0].2, supports::OVERHANG_RED);
    assert_eq!(hit.faces, 2);
    assert!(hit.message.is_none());

    paint.faces.clear();
    paint.highlighted.clear();
    let bare = supports::quick_paint(&v, false, &mut paint);
    assert_eq!(bare.message, Some(supports::NO_OVERHANG_MSG));
    let wall = supports::quick_paint(&v, true, &mut paint);
    assert!(wall.message.is_none(), "Draw Walls does not say there is nothing to support");

    v.insert("sf:threshold".into(), json!(0));
    paint.asks.clear();
    supports::quick_paint(&v, false, &mut paint);
    assert_eq!(paint.asks.last().unwrap().1, 45.0);
    v.remove("sf:threshold");
    supports::quick_paint(&v, false, &mut paint);
    assert_eq!(paint.asks.last().unwrap().1, 45.0);
}

#[test]
fn strength_and_suggest() {
    ensure();
    assert_eq!(supports::load_vector(&json!("up")), Some([0.0, 0.0, 1.0]));
    assert_eq!(supports::load_vector(&json!("none")), None);
    assert_eq!(supports::load_vector(&json!([1.0, 0.0, 0.0])), Some([1.0, 0.0, 0.0]));
    let id = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    assert_eq!(supports::turn_load([0.0, 0.0, 1.0], id), json!("up"));
    let to_right = [[0.0, 0.0, 1.0], [0.0, 1.0, 0.0], [-1.0, 0.0, 0.0]];
    assert_eq!(supports::turn_load([0.0, 0.0, 1.0], to_right), json!("right"));
    assert!(supports::turn_load([1.0, 1.0, 0.0], id).as_array().is_some());
    assert_eq!(supports::match_load([0.0, 0.0, 1.0]), json!("up"));

    assert_eq!(
        supports::posture_text("strong"),
        Some("Lying down: its longest side runs along the layers, the strong way to print it.")
    );
    assert_eq!(supports::confidence_note("high"), Some("A clear winner."));
    assert_eq!(supports::confidence_note("none"), Some("No pose sits flat on the plate: every way it balances on a point."));

    let cand = json!({"overArea": 12.2, "height": 30.4, "walls": 1, "volume": 1000.0, "posture": "strong"});
    assert_eq!(supports::format_candidate(&cand, "pla"), "12 mm² to support · 30 mm tall · 1 fin · 1.2 g · lying down");
    let two = json!({"overArea": 1, "height": 2, "walls": 2, "volume": 0, "posture": "weak"});
    assert_eq!(supports::format_candidate(&two, "pla"), "1 mm² to support · 2 mm tall · 2 fins · standing up");

    let (bodies, sel) = fixture();
    let ctx = project(&bodies, &[], &sel);
    let mut v = supports::defaults("supportfins", &ctx);
    v.insert("_load".into(), json!("up"));
    let ask = supports::strength_ask("supportfins", &v).unwrap();
    assert_eq!(ask.body, "body_a");
    assert_eq!(ask.options, json!({}));
    assert_eq!(ask.load, json!([0.0, 0.0, 1.0]));
    v.insert("sf:threshold".into(), json!(30));
    assert_eq!(supports::strength_ask("supportfins", &v).unwrap().options, json!({"threshold": 30}));

    let sug = supports::suggest_ask("supportfins", &v).unwrap();
    assert_eq!(sug.top, 3);
    assert_eq!(sug.body, "body_a");
    v.insert("body".into(), json!([]));
    assert_eq!(supports::suggest_ask("supportfins", &v).unwrap_err(), supports::PICK_PART);
    assert!(supports::strength_ask("supportfins", &v).is_none());

    v.insert("body".into(), json!(["body_a"]));
    let strength = json!({"posture": "strong", "load": {"quality": "good", "text": "This pose takes an upward pull well."}, "strongest": {"rot": [1, 0, 0]}});
    let busy = supports::pose_view(true, &v, &SuggestUi::Busy, Some(&strength), "pla");
    assert_eq!(busy.busy, Some(supports::SUGGEST_BUSY));
    assert!(busy.show_strongest);
    assert!(busy.posture.unwrap().starts_with("Lying down"));
    assert_eq!(busy.verdict.as_deref(), Some("This pose takes an upward pull well."));
    assert_eq!(busy.verdict_quality.as_deref(), Some("good"));
    assert_eq!(busy.load_selected, "up");
    assert!(busy.load_options.iter().any(|(k, l)| *k == "up" && *l == "Pulled up"));

    let list = SuggestUi::Ready { confidence: "medium".into(), candidates: vec![cand, two] };
    let ready = supports::pose_view(true, &v, &list, None, "pla");
    assert_eq!(ready.confidence, supports::confidence_note("medium"));
    assert_eq!(ready.rows[0].title, "Best");
    assert_eq!(ready.rows[1].title, "2");
    assert!(!ready.show_strongest);

    let off = supports::pose_view(false, &v, &SuggestUi::Idle, None, "pla");
    assert!(!off.available && off.rows.is_empty());

    let fx = supports::on_form_change("supportfins", "_mode", &{
        let mut r = supports::defaults("supportfins", &ctx);
        r.insert("_mode".into(), json!("rotate"));
        r
    }, "");
    assert!(fx.request_strength);
}
