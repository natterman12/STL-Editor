//! Support Fins panel: schema-built tools, modes, folding sections, Held card.
//!
//! Settings come from the engine's options schema (`fins::info`), never from a hard-coded list.
//! Only values that differ from that schema's defaults are written into a feature.
//! Strings the [`crate::tools::ToolDef`] function pointers need are leaked once, for the process.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde_json::{json, Map, Value};

use super::strength::{self, Cover, Plastic, Status, SuggestUi};
use crate::engine_link::TimelineItem;
use crate::theme::Tokens;
use crate::tools::fields;
use crate::tools::{Ctx, Field, FieldKind, ToolDef, ToolFlags, Values};

pub const MODES: &[(&str, &str)] = &[("rotate", "Rotate"), ("add", "Add fins"), ("remove", "Remove fins")];

pub const MODE_HELP: &str = "Rotate: drag the rings to turn the part the way it will print (it stays on the bed, and the fins follow). Add fins: draw walls by hand where Auto left a gap. Remove fins: click a fin to take it away, click it again to bring it back.";
pub const OUTPUT_HELP: &str = "Part of the body: the fins are joined to it, so it exports and slices as one object (what Support Fins itself does). Separate body: a body of its own you can hide or remove; export both together.";
pub const FINS_HINT: &str = "Red faces need support. Turn the part with the rings (it stays on the bed); Support Fins puts breakaway fins and a bed pad under the red.";
pub const ROTATE_HINT: &str = "Drag the rings to turn the part the way you want to print it (5° steps, Shift for any angle). It stays on the bed, and the fins are rebuilt.";
pub const REMOVE_HINT: &str = "Click a fin to remove it; click a red one to bring it back. Walls you drew by hand are removed for good (Ctrl+Z in Add fins brings them back).";
pub const MAT_HELP: &str = "The filament you print the supports in, for the weight under Support plastic.";
pub const REACH_HELP: &str = "Overhangs further than this from a fin, or from the part itself, are shown bright red: they'd sag or fail. Bridging over a few mm works; over 5 mm usually droops.";
pub const CREDIT_TITLE: &str = "Support Fins by gittrahan, MIT licence (Help \u{203a} About Support Fins)";
pub const NO_OVERHANG_MSG: &str = "No overhangs in this position: nothing needs support. Rotate the part the way you want to print it.";
pub const OVERHANG_RED: [u8; 3] = [0xd6, 0x45, 0x45];
pub const BUSY_FINS: &str = "Building supports\u{2026}";
pub const BUSY_WALLS: &str = "Building the walls\u{2026}";
pub const LOADING: &str = "Loading Support Fins\u{2026}";

const OUTPUTS: &[(&str, &str)] = &[("join", "Part of the body"), ("separate", "Separate body")];
const WALL_KEYS: &[&str] = &["threshold", "nozzle", "gap", "tines", "tineDensity", "layerHeight", "plateOnly"];
const MAX_OPTS: usize = 64;
const MAX_HDRS: usize = 24;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpecKind {
    Bodies,
    Select,
    Bool,
    Number,
    Hdr,
    Walls,
    Removed,
    Status,
    Pose,
    Material,
    Reach,
}

#[derive(Clone, Copy)]
pub struct FieldSpec {
    pub key: &'static str,
    pub label: &'static str,
    pub kind: SpecKind,
    pub unit: Option<&'static str>,
    pub help: Option<&'static str>,
    pub placeholder: Option<&'static str>,
    pub nolabel: bool,
    pub multi: bool,
    pub cover: bool,
    pub default_open: bool,
    pub options: &'static [(&'static str, &'static str)],
    pub show: Option<fn(&Values) -> bool>,
}

impl FieldSpec {
    pub fn kind_name(self) -> &'static str {
        match self.kind {
            SpecKind::Bodies => "bodies",
            SpecKind::Select => "select",
            SpecKind::Bool => "bool",
            SpecKind::Number => "number",
            SpecKind::Hdr => "hdr",
            SpecKind::Walls => "walls",
            SpecKind::Removed => "removed",
            SpecKind::Status => "sfstatus",
            SpecKind::Pose => "sfpose",
            SpecKind::Material => "sfmat",
            SpecKind::Reach => "sfreach",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum OptKind {
    Bool,
    Choice,
    Number,
}

struct Opt {
    key: &'static str,
    field_key: &'static str,
    label: &'static str,
    help: Option<&'static str>,
    section: &'static str,
    kind: OptKind,
    default_raw: Value,
    shown: Value,
    percent: bool,
    show_if: Vec<Value>,
    unit: Option<&'static str>,
    choices: &'static [(&'static str, &'static str)],
}

struct Hdr {
    slots: Vec<usize>,
}

struct Installed {
    options: Vec<Opt>,
    headers: Vec<Hdr>,
    tools: Vec<ToolDef>,
    specs: Vec<Vec<FieldSpec>>,
    version: &'static str,
    commit: &'static str,
    source: &'static str,
    pending: Option<&'static str>,
    orient: bool,
    curves: bool,
    gussets: bool,
    anchors: bool,
    host: &'static str,
    notice: Option<&'static str>,
}

static INSTALLED: OnceLock<Installed> = OnceLock::new();

fn show_n<const N: usize>(v: &Values) -> bool {
    opt_visible_slot(N, v)
}
fn hdr_n<const N: usize>(v: &Values) -> bool {
    header_visible(N, v)
}

const OPT_SHOW: [fn(&Values) -> bool; MAX_OPTS] = [
    show_n::<0>, show_n::<1>, show_n::<2>, show_n::<3>, show_n::<4>, show_n::<5>, show_n::<6>, show_n::<7>,
    show_n::<8>, show_n::<9>, show_n::<10>, show_n::<11>, show_n::<12>, show_n::<13>, show_n::<14>, show_n::<15>,
    show_n::<16>, show_n::<17>, show_n::<18>, show_n::<19>, show_n::<20>, show_n::<21>, show_n::<22>, show_n::<23>,
    show_n::<24>, show_n::<25>, show_n::<26>, show_n::<27>, show_n::<28>, show_n::<29>, show_n::<30>, show_n::<31>,
    show_n::<32>, show_n::<33>, show_n::<34>, show_n::<35>, show_n::<36>, show_n::<37>, show_n::<38>, show_n::<39>,
    show_n::<40>, show_n::<41>, show_n::<42>, show_n::<43>, show_n::<44>, show_n::<45>, show_n::<46>, show_n::<47>,
    show_n::<48>, show_n::<49>, show_n::<50>, show_n::<51>, show_n::<52>, show_n::<53>, show_n::<54>, show_n::<55>,
    show_n::<56>, show_n::<57>, show_n::<58>, show_n::<59>, show_n::<60>, show_n::<61>, show_n::<62>, show_n::<63>,
];

const HDR_SHOW: [fn(&Values) -> bool; MAX_HDRS] = [
    hdr_n::<0>, hdr_n::<1>, hdr_n::<2>, hdr_n::<3>, hdr_n::<4>, hdr_n::<5>, hdr_n::<6>, hdr_n::<7>,
    hdr_n::<8>, hdr_n::<9>, hdr_n::<10>, hdr_n::<11>, hdr_n::<12>, hdr_n::<13>, hdr_n::<14>, hdr_n::<15>,
    hdr_n::<16>, hdr_n::<17>, hdr_n::<18>, hdr_n::<19>, hdr_n::<20>, hdr_n::<21>, hdr_n::<22>, hdr_n::<23>,
];

fn leak(s: impl Into<String>) -> &'static str {
    Box::leak(s.into().into_boxed_str())
}

fn installed() -> Option<&'static Installed> {
    INSTALLED.get()
}

/// Load the object `fins::info` returns. Call once. A second call keeps the first schema.
pub fn install(info: &Value) -> Result<(), String> {
    if INSTALLED.get().is_some() {
        return Ok(());
    }
    let built = build(info)?;
    let _ = INSTALLED.set(built);
    Ok(())
}

pub fn is_installed() -> bool {
    INSTALLED.get().is_some()
}

pub fn support_tools() -> &'static [ToolDef] {
    installed().map(|g| g.tools.as_slice()).unwrap_or(&[])
}

pub fn support_tool(key: &str) -> Option<&'static ToolDef> {
    let g = installed()?;
    g.tools.iter().find(|t| t.key == key)
}

pub fn fields(tool: &str) -> &'static [FieldSpec] {
    let Some(g) = installed() else { return &[] };
    match tool {
        "supportfins" => g.specs.first().map(Vec::as_slice).unwrap_or(&[]),
        "supportwall" => g.specs.get(1).map(Vec::as_slice).unwrap_or(&[]),
        _ => &[],
    }
}

pub fn field_visible(spec: &FieldSpec, values: &Values) -> bool {
    spec.show.map(|f| f(values)).unwrap_or(true)
}

pub fn shown_keys(tool: &str, values: &Values) -> Vec<&'static str> {
    fields(tool).iter().filter(|f| f.kind != SpecKind::Hdr && field_visible(f, values)).map(|f| f.key).collect()
}

/// `gizmo: 'pose'` in 1.x. [`ToolFlags::gizmo`] is only a bool, so the pose kind lives here.
pub fn gizmo_mode(tool: &str) -> Option<&'static str> {
    if tool == "supportfins" { Some("pose") } else { None }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EngineFeatures {
    pub orient: bool,
    pub curves: bool,
    pub gussets: bool,
    pub anchors: bool,
}

pub fn engine_features() -> EngineFeatures {
    installed()
        .map(|g| EngineFeatures { orient: g.orient, curves: g.curves, gussets: g.gussets, anchors: g.anchors })
        .unwrap_or(EngineFeatures { orient: false, curves: false, gussets: false, anchors: false })
}

pub fn orient_available() -> bool {
    engine_features().orient
}

pub fn engine_version() -> &'static str {
    installed().map(|g| g.version).unwrap_or("")
}

fn opt_visible_slot(slot: usize, values: &Values) -> bool {
    let Some(g) = installed() else { return true };
    let Some(o) = g.options.get(slot) else { return true };
    o.show_if.iter().all(|c| holds(g, values, c))
}

fn header_visible(index: usize, values: &Values) -> bool {
    let Some(g) = installed() else { return true };
    let Some(h) = g.headers.get(index) else { return true };
    h.slots.iter().any(|&s| opt_visible_slot(s, values))
}

fn holds(g: &Installed, values: &Values, c: &Value) -> bool {
    if let Some(s) = c.as_str() {
        return get_opt(g, values, s).as_ref() == &Value::Bool(true);
    }
    if let Some(any) = c.get("any").and_then(Value::as_array) {
        return any.iter().any(|x| holds(g, values, x));
    }
    let Some(key) = c.get("key").and_then(Value::as_str) else { return false };
    let Some(list) = c.get("in").and_then(Value::as_array) else { return false };
    let got = get_opt(g, values, key);
    list.iter().any(|item| item == got.as_ref())
}

fn get_opt<'a>(g: &'a Installed, values: &'a Values, key: &str) -> Cow<'a, Value> {
    if let Some(o) = g.options.iter().find(|o| o.key == key) {
        if let Some(x) = values.get(o.field_key) {
            if !x.is_null() && x.as_str() != Some("") {
                return Cow::Borrowed(x);
            }
        }
        return Cow::Borrowed(&o.default_raw);
    }
    Cow::Owned(Value::Null)
}

fn show_walls(v: &Values) -> bool {
    v.get("_mode").and_then(Value::as_str) == Some("add")
}
fn show_removed(v: &Values) -> bool {
    v.get("_mode").and_then(Value::as_str) == Some("remove") || v.get("removed").and_then(Value::as_array).is_some_and(|a| !a.is_empty())
}
fn show_pose(v: &Values) -> bool {
    v.get("_mode").and_then(Value::as_str).unwrap_or("rotate") == "rotate"
}

fn build(info: &Value) -> Result<Installed, String> {
    let schema = info.get("schema").ok_or("Support Fins: the engine info has no options schema.")?;
    let options_v = schema.get("options").and_then(Value::as_array).ok_or("Support Fins: the options schema has no settings list.")?;
    let sections_v = schema.get("sections").and_then(Value::as_array).ok_or("Support Fins: the options schema has no sections.")?;
    if options_v.len() > MAX_OPTS {
        return Err(format!("Support Fins: this engine has {} settings and the panel can show {MAX_OPTS}.", options_v.len()));
    }
    let mut options = Vec::with_capacity(options_v.len());
    for (slot, o) in options_v.iter().enumerate() {
        options.push(parse_opt(slot, o)?);
    }
    let mut sections = Vec::new();
    for s in sections_v {
        let id = s.get("id").and_then(Value::as_str).unwrap_or("").to_string();
        let label = s.get("label").and_then(Value::as_str).unwrap_or("").to_string();
        if id.is_empty() {
            continue;
        }
        sections.push((leak(id), leak(label)));
    }
    let mut headers = Vec::new();
    let fins_specs = assemble("supportfins", &options, &sections, None, 2, true, &mut headers)?;
    let wall_specs = assemble("supportwall", &options, &sections, Some(WALL_KEYS), 1, false, &mut headers)?;
    let tools = vec![tool_def("supportfins", &fins_specs), tool_def("supportwall", &wall_specs)];
    let feat = info.get("features");
    let flag = |k: &str| feat.and_then(|f| f.get(k)).and_then(Value::as_bool).unwrap_or(false);
    Ok(Installed {
        options,
        headers,
        tools,
        specs: vec![fins_specs, wall_specs],
        version: leak(json_str(info, "version")),
        commit: leak(json_str(info, "commit")),
        source: leak(json_str(info, "source")),
        pending: info.get("pending").and_then(Value::as_str).filter(|s| !s.is_empty()).map(|s| leak(s.to_string())),
        orient: flag("orient"),
        curves: flag("curves"),
        gussets: flag("gussets"),
        anchors: flag("anchors"),
        host: leak(json_str(info, "host")),
        notice: info.get("notice").and_then(Value::as_str).filter(|s| !s.is_empty()).map(|s| leak(s.to_string())),
    })
}

fn json_str(v: &Value, key: &str) -> String {
    match v.get(key) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Null) | None => String::new(),
        Some(other) => other.to_string(),
    }
}

fn parse_opt(_slot: usize, o: &Value) -> Result<Opt, String> {
    let key = o.get("key").and_then(Value::as_str).unwrap_or("").to_string();
    if key.is_empty() {
        return Err("Support Fins: a setting in the schema has no name.".into());
    }
    let label = o.get("label").and_then(Value::as_str).unwrap_or("").to_string();
    let help = o.get("tooltip").and_then(Value::as_str).filter(|s| !s.is_empty()).map(|s| leak(s.to_string()));
    let section = leak(o.get("section").and_then(Value::as_str).unwrap_or("").to_string());
    let percent = o.get("percent").and_then(Value::as_bool).unwrap_or(false);
    let hint = o.get("hint").and_then(Value::as_str).unwrap_or("");
    let unit = unit_of(percent, hint).map(leak);
    let default_raw = o.get("default").cloned().unwrap_or(Value::Null);
    let kind = match o.get("type").and_then(Value::as_str).unwrap_or("number") {
        "bool" => OptKind::Bool,
        "choice" => OptKind::Choice,
        _ => OptKind::Number,
    };
    let choices: &'static [(&'static str, &'static str)] = if kind == OptKind::Choice {
        let pairs = o.get("choices").and_then(Value::as_array).into_iter().flatten().filter_map(|c| {
            let value = c.get("value").and_then(value_to_string)?;
            let label = c.get("label").and_then(Value::as_str).unwrap_or("").to_string();
            Some((value, label))
        }).collect::<Vec<_>>();
        leak_pairs(pairs)
    } else {
        &[]
    };
    let show_if = o.get("showIf").and_then(Value::as_array).cloned().unwrap_or_default();
    let shown = shown_default(percent, &default_raw);
    Ok(Opt {
        field_key: leak(format!("sf:{key}")),
        key: leak(key),
        label: leak(label),
        help,
        section,
        kind,
        default_raw,
        shown,
        percent,
        show_if,
        unit,
        choices,
    })
}

fn value_to_string(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn leak_pairs(v: Vec<(String, String)>) -> &'static [(&'static str, &'static str)] {
    let leaked: Vec<(&'static str, &'static str)> = v.into_iter().map(|(a, b)| (leak(a), leak(b))).collect();
    Box::leak(leaked.into_boxed_slice())
}

fn unit_of(percent: bool, hint: &str) -> Option<String> {
    if percent {
        return Some("%".into());
    }
    let b = hint.as_bytes();
    if b.len() >= 2 && &b[..2] == b"mm" && (b.len() == 2 || !is_word(b[2])) {
        return Some("mm".into());
    }
    if hint.starts_with('°') || hint.starts_with('%') {
        return Some(hint.to_string());
    }
    None
}

fn is_word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn shown_default(percent: bool, default_raw: &Value) -> Value {
    if percent {
        if let Some(n) = as_f64(default_raw) {
            return num_value((n * 100.0 * 1000.0).round() / 1000.0);
        }
    }
    default_raw.clone()
}

fn as_f64(v: &Value) -> Option<f64> {
    v.as_f64().or_else(|| v.as_i64().map(|n| n as f64)).or_else(|| v.as_u64().map(|n| n as f64))
}

fn num_value(x: f64) -> Value {
    if !x.is_finite() {
        return Value::from(0);
    }
    let r = x.round();
    if (x - r).abs() < 1e-9 && r.abs() < 1e15 {
        Value::Number((r as i64).into())
    } else {
        serde_json::Number::from_f64(x).map(Value::Number).unwrap_or_else(|| Value::from(0))
    }
}

fn assemble(
    tool: &str,
    options: &[Opt],
    sections: &[(&'static str, &'static str)],
    keys: Option<&[&str]>,
    n_open: usize,
    cover: bool,
    headers: &mut Vec<Hdr>,
) -> Result<Vec<FieldSpec>, String> {
    let mut specs = Vec::new();
    specs.push(spec_body(tool));
    specs.push(spec_output());
    if tool == "supportfins" {
        specs.push(FieldSpec {
            key: "_mode",
            label: "On the part",
            kind: SpecKind::Select,
            unit: None,
            help: Some(MODE_HELP),
            placeholder: None,
            nolabel: true,
            multi: false,
            cover: false,
            default_open: false,
            options: MODES,
            show: None,
        });
        specs.push(FieldSpec {
            key: "walls",
            label: "Hand walls",
            kind: SpecKind::Walls,
            show: Some(show_walls),
            ..blank()
        });
        specs.push(FieldSpec {
            key: "removed",
            label: "Removed",
            kind: SpecKind::Removed,
            show: Some(show_removed),
            ..blank()
        });
    } else {
        specs.push(FieldSpec {
            key: "walls",
            label: "Walls",
            kind: SpecKind::Walls,
            show: None,
            ..blank()
        });
    }
    let mut opened = 0usize;
    for (id, label) in sections {
        let slots: Vec<usize> = options.iter().enumerate().filter(|(_, o)| o.section == *id && keys.is_none_or(|k| k.contains(&o.key))).map(|(i, _)| i).collect();
        if slots.is_empty() {
            continue;
        }
        let idx = headers.len();
        if idx >= MAX_HDRS {
            return Err("Support Fins: too many folding sections for the panel.".into());
        }
        headers.push(Hdr { slots: slots.clone() });
        specs.push(FieldSpec {
            key: leak(format!("_hdr_{id}")),
            label,
            kind: SpecKind::Hdr,
            default_open: opened < n_open,
            show: Some(HDR_SHOW[idx]),
            ..blank()
        });
        opened += 1;
        for s in slots {
            specs.push(option_spec_at(&options[s], s));
        }
    }
    specs.push(FieldSpec { key: "_hdr_check", label: "Print check", kind: SpecKind::Hdr, default_open: true, show: None, ..blank() });
    specs.push(FieldSpec { key: "_status", label: "", kind: SpecKind::Status, cover, show: None, ..blank() });
    if cover {
        specs.push(FieldSpec { key: "_pose", label: "Orientation", kind: SpecKind::Pose, show: Some(show_pose), ..blank() });
    }
    specs.push(FieldSpec { key: "_mat", label: "Material", kind: SpecKind::Material, help: Some(MAT_HELP), show: None, ..blank() });
    if cover {
        specs.push(FieldSpec { key: "_reach", label: "Bare beyond", kind: SpecKind::Reach, help: Some(REACH_HELP), show: None, ..blank() });
    }
    Ok(specs)
}

fn blank() -> FieldSpec {
    FieldSpec {
        key: "",
        label: "",
        kind: SpecKind::Bool,
        unit: None,
        help: None,
        placeholder: None,
        nolabel: false,
        multi: false,
        cover: false,
        default_open: false,
        options: &[],
        show: None,
    }
}

fn spec_body(tool: &str) -> FieldSpec {
    FieldSpec {
        key: "body",
        label: "Body",
        kind: SpecKind::Bodies,
        placeholder: if tool == "supportwall" { Some("The part you draw on") } else { None },
        multi: false,
        ..blank()
    }
}

fn spec_output() -> FieldSpec {
    FieldSpec {
        key: "output",
        label: "Add as",
        kind: SpecKind::Select,
        help: Some(OUTPUT_HELP),
        options: OUTPUTS,
        ..blank()
    }
}

fn option_spec(o: &Opt) -> FieldSpec {
    FieldSpec {
        key: o.field_key,
        label: o.label,
        kind: match o.kind {
            OptKind::Bool => SpecKind::Bool,
            OptKind::Choice => SpecKind::Select,
            OptKind::Number => SpecKind::Number,
        },
        unit: o.unit,
        help: o.help,
        options: o.choices,
        show: Some(OPT_SHOW[0]),
        ..blank()
    }
}

fn tool_def(key: &'static str, specs: &[FieldSpec]) -> ToolDef {
    let fins = key == "supportfins";
    ToolDef {
        key,
        label: if fins { "Support Fins" } else { "Draw Walls" },
        icon: if fins { "fins" } else { "finwall" },
        kind: key,
        fields: specs.iter().filter_map(to_field).collect(),
        // 1.x sets Draw Walls' hint to `walls.hint()` when the tool is built. The default shape is line.
        hint: if fins { Some(FINS_HINT) } else { Some(super::walls::hint_for("line")) },
        extra: None,
        flags: ToolFlags { paint: true, wall_tool: !fins, gizmo: fins, ..ToolFlags::default() },
        defaults: if fins { defaults_fins } else { defaults_wall },
        to_params: if fins { to_params_fins } else { to_params_wall },
        from_params: if fins { from_params_fins } else { from_params_wall },
        on_change: None,
    }
}

fn to_field(spec: &FieldSpec) -> Option<Field> {
    let kind = match spec.kind {
        SpecKind::Bodies => FieldKind::Bodies { multi: spec.multi },
        SpecKind::Select => FieldKind::Select {
            options: spec
                .options
                .iter()
                .map(|(a, b)| ((*a).to_string(), (*b).to_string(), None))
                .collect(),
            grouped: false,
        },
        SpecKind::Bool => FieldKind::Bool,
        SpecKind::Number => FieldKind::Number { unit: spec.unit },
        _ => return None,
    };
    Some(Field {
        key: spec.key,
        label: spec.label,
        kind,
        help: spec.help,
        show: spec.show,
        placeholder: spec.placeholder,
    })
}

/// Golden-shaped field record (no functions).
pub fn field_meta(spec: &FieldSpec) -> Value {
    let mut m = Map::new();
    m.insert("key".into(), json!(spec.key));
    m.insert("label".into(), json!(spec.label));
    m.insert("kind".into(), json!(spec.kind_name()));
    if let Some(u) = spec.unit {
        if !u.is_empty() {
            m.insert("unit".into(), json!(u));
        }
    }
    if spec.kind == SpecKind::Bodies {
        m.insert("multi".into(), Value::Bool(spec.multi));
    }
    if let Some(p) = spec.placeholder {
        m.insert("placeholder".into(), json!(p));
    }
    if let Some(h) = spec.help {
        m.insert("help".into(), json!(h));
    }
    if !spec.options.is_empty() {
        m.insert("options".into(), Value::Array(spec.options.iter().map(|(a, b)| json!([*a, *b])).collect()));
    }
    if spec.show.is_some() {
        m.insert("hasShow".into(), Value::Bool(true));
    }
    Value::Object(m)
}

// The slot on an option spec must be the schema index, not a guess from the key length.
// `option_spec` above stamps a placeholder; `assemble` overwrites it. Done here so the
// placeholder cannot ship: we rebuild the show pointer from the option's real slot.
fn option_spec_at(o: &Opt, slot: usize) -> FieldSpec {
    let mut spec = option_spec(o);
    spec.show = Some(OPT_SHOW[slot]);
    spec
}

fn defaults_fins(ctx: &Ctx) -> Values {
    defaults("supportfins", ctx)
}
fn defaults_wall(ctx: &Ctx) -> Values {
    defaults("supportwall", ctx)
}
fn to_params_fins(v: &Values, ctx: &Ctx) -> Value {
    to_params("supportfins", v, ctx)
}
fn to_params_wall(v: &Values, ctx: &Ctx) -> Value {
    to_params("supportwall", v, ctx)
}
fn from_params_fins(p: Value) -> Values {
    from_params("supportfins", p)
}
fn from_params_wall(p: Value) -> Values {
    from_params("supportwall", p)
}

pub fn defaults(tool: &str, ctx: &Ctx) -> Values {
    let mut v = Values::new();
    v.insert("body".into(), one_body(ctx));
    if tool == "supportwall" {
        v.insert("walls".into(), json!([]));
    } else {
        v.insert("_mode".into(), json!("rotate"));
        v.insert("walls".into(), json!([]));
        v.insert("removed".into(), json!([]));
    }
    v.insert("output".into(), json!("join"));
    insert_option_defaults(&mut v, if tool == "supportwall" { Some(WALL_KEYS) } else { None });
    v
}

fn one_body(ctx: &Ctx) -> Value {
    for id in ctx.selection {
        if ctx.bodies.iter().any(|b| b.id == *id) {
            return json!([id]);
        }
    }
    let shown: Vec<_> = ctx.bodies.iter().filter(|b| !b.hidden).collect();
    if shown.len() == 1 { json!([shown[0].id]) } else { json!([]) }
}

pub fn to_params(tool: &str, values: &Values, _ctx: &Ctx) -> Value {
    let mut p = Map::new();
    if let Some(body) = params_body(values) {
        p.insert("body".into(), body);
    }
    if tool == "supportwall" {
        p.insert("walls".into(), values.get("walls").cloned().filter(js_truthy).unwrap_or_else(|| json!([])));
    }
    if let Some(output) = values.get("output") {
        p.insert("output".into(), output.clone());
    }
    let keys = if tool == "supportwall" { Some(WALL_KEYS) } else { None };
    p.insert("options".into(), Value::Object(to_options(values, keys)));
    if tool != "supportwall" {
        if let Some(Value::Array(w)) = values.get("walls") {
            if !w.is_empty() {
                p.insert("walls".into(), Value::Array(w.clone()));
            }
        }
        if let Some(Value::Array(r)) = values.get("removed") {
            if !r.is_empty() {
                p.insert("removed".into(), Value::Array(r.clone()));
            }
        }
    }
    Value::Object(p)
}

fn params_body(values: &Values) -> Option<Value> {
    values.get("body")?.as_array()?.first().filter(|v| !v.is_null()).cloned()
}

fn to_options(values: &Values, keys: Option<&[&str]>) -> Map<String, Value> {
    let Some(g) = installed() else { return Map::new() };
    let mut out = Map::new();
    for o in &g.options {
        if keys.is_some_and(|k| !k.contains(&o.key)) {
            continue;
        }
        let Some(raw) = values.get(o.field_key) else { continue };
        if raw.is_null() || raw.as_str() == Some("") {
            continue;
        }
        if o.kind == OptKind::Number {
            let mut x = parse_num(raw);
            if o.percent {
                x = (x * 1e6 / 100.0).round() / 1e6;
            }
            let def = as_f64(&o.default_raw).unwrap_or(f64::NAN);
            if (x - def).abs() < 1e-9 {
                continue;
            }
            out.insert(o.key.to_string(), num_value(x));
        } else if raw != &o.default_raw {
            out.insert(o.key.to_string(), raw.clone());
        }
    }
    out
}

pub fn from_params(tool: &str, params: Value) -> Values {
    let obj = match params {
        Value::Object(m) => m,
        _ => Map::new(),
    };
    let mut v = Values::new();
    v.insert("body".into(), body_from(&obj));
    if tool == "supportwall" {
        v.insert("walls".into(), wall_tool_walls(&obj));
    } else {
        v.insert("_mode".into(), json!("rotate"));
        v.insert("walls".into(), truthy_or_empty(obj.get("walls")));
        v.insert("removed".into(), truthy_or_empty(obj.get("removed")));
    }
    v.insert("output".into(), output_of(&obj));
    insert_option_defaults(&mut v, if tool == "supportwall" { Some(WALL_KEYS) } else { None });
    if let Some(options) = obj.get("options") {
        from_options(options, &mut v);
    }
    v
}

fn insert_option_defaults(into: &mut Values, keys: Option<&[&str]>) {
    let Some(g) = installed() else { return };
    for o in &g.options {
        if keys.is_some_and(|k| !k.contains(&o.key)) {
            continue;
        }
        into.insert(o.field_key.to_string(), o.shown.clone());
    }
}

fn from_options(options: &Value, into: &mut Values) {
    let Some(obj) = options.as_object() else { return };
    let Some(g) = installed() else { return };
    for (k, x) in obj {
        let Some(o) = g.options.iter().find(|o| o.key == k) else {
            into.insert(format!("sf:{k}"), x.clone());
            continue;
        };
        let shown = if o.percent {
            as_f64(x).map(|n| num_value((n * 100.0 * 1000.0).round() / 1000.0)).unwrap_or_else(|| x.clone())
        } else {
            x.clone()
        };
        into.insert(o.field_key.to_string(), shown);
    }
}

fn body_from(p: &Map<String, Value>) -> Value {
    match p.get("body") {
        Some(v) if js_truthy(v) => Value::Array(vec![v.clone()]),
        _ => json!([]),
    }
}

fn output_of(p: &Map<String, Value>) -> Value {
    match p.get("output") {
        Some(v) if js_truthy(v) => v.clone(),
        _ => json!("join"),
    }
}

fn truthy_or_empty(v: Option<&Value>) -> Value {
    match v {
        Some(v) if js_truthy(v) => v.clone(),
        _ => json!([]),
    }
}

fn wall_tool_walls(p: &Map<String, Value>) -> Value {
    if let Some(Value::Array(a)) = p.get("walls") {
        return Value::Array(a.clone());
    }
    if let Some(Value::Array(pts)) = p.get("points") {
        if pts.len() == 2 {
            return json!([{ "shape": "line", "points": pts }]);
        }
    }
    json!([])
}

fn js_truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|x| x != 0.0) || n.as_i64().is_some_and(|x| x != 0),
        Value::String(s) => !s.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

/// `parseNum` in app.js. An `=expression` becomes 0, the same as 1.x does inside `toOptions`.
pub fn parse_num(v: &Value) -> f64 {
    match v {
        Value::Number(n) => n.as_f64().unwrap_or(0.0),
        Value::String(s) => parse_num_str(s),
        Value::Bool(b) => parse_num_str(if *b { "true" } else { "false" }),
        Value::Null => 0.0,
        other => parse_num_str(&other.to_string()),
    }
}

fn parse_num_str(raw: &str) -> f64 {
    let s = raw.trim().replace(',', ".");
    if s.is_empty() {
        return 0.0;
    }
    if s.bytes().all(|c| matches!(c, b'0'..=b'9' | b'+' | b'-' | b'*' | b'/' | b'(' | b')' | b'.' | b' ')) {
        if let Some(v) = eval_arith(&s) {
            if v.is_finite() {
                return v;
            }
        }
    }
    parse_float_js(&s).unwrap_or(0.0)
}

fn eval_arith(s: &str) -> Option<f64> {
    let mut p = Arith { s: s.as_bytes(), i: 0 };
    let v = p.expr()?;
    p.skip();
    if p.i != p.s.len() { None } else { Some(v) }
}

struct Arith<'a> {
    s: &'a [u8],
    i: usize,
}
impl Arith<'_> {
    fn skip(&mut self) {
        while self.i < self.s.len() && self.s[self.i] == b' ' {
            self.i += 1;
        }
    }
    fn expr(&mut self) -> Option<f64> {
        let mut v = self.term()?;
        loop {
            self.skip();
            if self.i >= self.s.len() || (self.s[self.i] != b'+' && self.s[self.i] != b'-') {
                break;
            }
            let op = self.s[self.i];
            self.i += 1;
            let r = self.term()?;
            if op == b'+' { v += r } else { v -= r }
        }
        Some(v)
    }
    fn term(&mut self) -> Option<f64> {
        let mut v = self.unary()?;
        loop {
            self.skip();
            if self.i >= self.s.len() || (self.s[self.i] != b'*' && self.s[self.i] != b'/') {
                break;
            }
            let op = self.s[self.i];
            self.i += 1;
            let r = self.unary()?;
            if op == b'*' { v *= r } else { v /= r }
        }
        Some(v)
    }
    fn unary(&mut self) -> Option<f64> {
        self.skip();
        if self.i >= self.s.len() {
            return None;
        }
        if self.s[self.i] == b'+' {
            self.i += 1;
            return self.unary();
        }
        if self.s[self.i] == b'-' {
            self.i += 1;
            return Some(-self.unary()?);
        }
        self.primary()
    }
    fn primary(&mut self) -> Option<f64> {
        self.skip();
        if self.i < self.s.len() && self.s[self.i] == b'(' {
            self.i += 1;
            let v = self.expr()?;
            self.skip();
            if self.i >= self.s.len() || self.s[self.i] != b')' {
                return None;
            }
            self.i += 1;
            return Some(v);
        }
        let start = self.i;
        while self.i < self.s.len() && (self.s[self.i].is_ascii_digit() || self.s[self.i] == b'.') {
            self.i += 1;
        }
        if start == self.i {
            return None;
        }
        std::str::from_utf8(&self.s[start..self.i]).ok()?.parse().ok()
    }
}

/// JavaScript `parseFloat`: the number a string starts with.
pub fn parse_float_js(s: &str) -> Option<f64> {
    let s = s.trim();
    let b = s.as_bytes();
    if b.is_empty() {
        return None;
    }
    let mut end = 0usize;
    if b[0] == b'+' || b[0] == b'-' {
        end = 1;
    }
    let mut saw = false;
    while end < b.len() && b[end].is_ascii_digit() {
        saw = true;
        end += 1;
    }
    if end < b.len() && b[end] == b'.' {
        end += 1;
        while end < b.len() && b[end].is_ascii_digit() {
            saw = true;
            end += 1;
        }
    }
    if end < b.len() && (b[end] == b'e' || b[end] == b'E') {
        let e = end;
        end += 1;
        if end < b.len() && (b[end] == b'+' || b[end] == b'-') {
            end += 1;
        }
        let ed = end;
        while end < b.len() && b[end].is_ascii_digit() {
            end += 1;
        }
        if end == ed {
            end = e;
        }
    }
    if !saw {
        return None;
    }
    s[..end].parse().ok()
}

pub fn mode_of(values: &Values) -> &str {
    values.get("_mode").and_then(Value::as_str).unwrap_or("rotate")
}

pub fn mode_hint(tool: &str, values: &Values, wall_hint: &str) -> String {
    if tool == "supportwall" {
        return wall_hint.to_string();
    }
    match mode_of(values) {
        "rotate" => ROTATE_HINT.into(),
        "remove" => REMOVE_HINT.into(),
        _ => format!("Draw walls by hand where the auto fins left a gap. {wall_hint}"),
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FormEffects {
    pub quick_paint: bool,
    pub reset_walls: bool,
    pub clear_preview_hover: bool,
    pub request_strength: bool,
    pub rerender: bool,
    pub hint: Option<String>,
}

/// Side effects of `setValue` for Support Fins (`app.js` 825–827). No value patch: 1.x has no `onChange` on these tools.
pub fn on_form_change(tool: &str, key: &str, values: &Values, wall_hint: &str) -> FormEffects {
    let mut e = FormEffects::default();
    let paint = matches!(tool, "supportfins" | "supportwall");
    if paint && (key == "body" || key == "sf:threshold") {
        e.quick_paint = true;
    }
    if key == "_mode" {
        e.reset_walls = true;
        e.clear_preview_hover = true;
        e.hint = Some(mode_hint(tool, values, wall_hint));
        e.rerender = true;
        if mode_of(values) == "rotate" && orient_available() {
            e.request_strength = true;
        }
    }
    if key == "_load" && orient_available() {
        e.request_strength = true;
        e.rerender = true;
    }
    e
}

pub fn set_mode(values: &mut Values, mode: &str, wall_hint: &str) -> FormEffects {
    values.insert("_mode".into(), Value::String(mode.into()));
    on_form_change("supportfins", "_mode", values, wall_hint)
}

/// Drop the "no overhangs in this position" sentence in Rotate. The Held card says it.
pub fn warning_line(mode: &str, warn: &str) -> String {
    if mode != "rotate" {
        return warn.to_string();
    }
    strip_rotate_warning(warn).trim().to_string()
}

fn strip_rotate_warning(warn: &str) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < warn.len() {
        if warn.as_bytes()[i] == b'"' {
            if let Some(end) = rotate_span(warn, i) {
                i = end;
                continue;
            }
        }
        let ch = warn[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

fn rotate_span(s: &str, i: usize) -> Option<usize> {
    let rest = &s[i..];
    if !rest.starts_with('"') {
        return None;
    }
    let close = rest[1..].find('"')?;
    let after = &rest[1 + close + 1..];
    let prefix = " has no overhangs in this position. ";
    if !after.starts_with(prefix) {
        return None;
    }
    let tail = &after[prefix.len()..];
    let dot = tail.find('.')?;
    let mut end = i + 1 + close + 1 + prefix.len() + dot + 1;
    let bytes = s.as_bytes();
    while end < bytes.len() && bytes[end].is_ascii_whitespace() {
        end += 1;
    }
    Some(end)
}

#[derive(Clone, Debug, PartialEq)]
pub struct Reopen {
    pub feature_id: String,
    /// `supportfins` or `supportwall`, from the feature that was found.
    pub tool_key: &'static str,
    /// `_mode` 1.x forces when it opens that step.
    pub mode: Option<&'static str>,
    pub toast: String,
}

/// The last Support Fins or Draw Walls step on this body, to edit instead of stacking another.
pub fn reopen_supports(timeline: &[TimelineItem], body: &str) -> Option<String> {
    find_support(timeline, body).map(|f| f.id.clone())
}

/// Same search, plus which tool to open and the mode 1.x sets from the button that was clicked.
pub fn reopen_plan(timeline: &[TimelineItem], opening: &str, body: &str, body_name: &str) -> Option<Reopen> {
    let f = find_support(timeline, body)?;
    let tool_key: &'static str = if f.kind == "supportwall" { "supportwall" } else { "supportfins" };
    let mode = if opening == "supportwall" && tool_key == "supportfins" {
        Some("add")
    } else if opening == "supportfins" && tool_key == "supportfins" {
        Some("rotate")
    } else {
        None
    };
    let name = if body_name.is_empty() { body } else { body_name };
    let toast = format!("{name} already has {}: opened it to change it", f.label);
    Some(Reopen { feature_id: f.id.clone(), tool_key, mode, toast })
}

fn find_support<'a>(timeline: &'a [TimelineItem], body: &str) -> Option<&'a TimelineItem> {
    timeline.iter().rev().find(|f| (f.kind == "supportfins" || f.kind == "supportwall") && f.active && !f.suppressed && on_body(f, body))
}

fn on_body(f: &TimelineItem, body: &str) -> bool {
    let Value::Object(p) = &f.params else { return false };
    if p.get("body").and_then(Value::as_str) == Some(body) {
        return true;
    }
    p.get("bodies").and_then(Value::as_array).is_some_and(|a| a.iter().any(|v| v.as_str() == Some(body)))
}

/// Instant red overhangs. Task 4 draws them; this only asks.
pub trait OverhangOverlay {
    fn clear_support_paint(&mut self);
    fn overhang_faces(&mut self, body: &str, threshold_deg: f64) -> Vec<u32>;
    fn highlight_overhangs(&mut self, body: &str, faces: &[u32], rgb: [u8; 3]);
}

#[derive(Clone, Debug, PartialEq)]
pub struct QuickPaint {
    pub clear_cover: bool,
    pub body: Option<String>,
    pub threshold_deg: f64,
    pub faces: usize,
    pub message: Option<&'static str>,
}

pub fn quick_paint(values: &Values, wall_tool: bool, overlay: &mut dyn OverhangOverlay) -> QuickPaint {
    overlay.clear_support_paint();
    let body = body_id(values);
    let Some(body) = body else {
        return QuickPaint { clear_cover: true, body: None, threshold_deg: 45.0, faces: 0, message: None };
    };
    let thr = values.get("sf:threshold").map(parse_num).unwrap_or(0.0);
    let thr = if thr.is_finite() && thr > 0.0 { thr } else { 45.0 };
    let faces = overlay.overhang_faces(&body, thr);
    let message = if faces.is_empty() && !wall_tool { Some(NO_OVERHANG_MSG) } else { None };
    overlay.highlight_overhangs(&body, &faces, OVERHANG_RED);
    QuickPaint { clear_cover: true, body: Some(body), threshold_deg: thr, faces: faces.len(), message }
}

pub fn body_id(values: &Values) -> Option<String> {
    match values.get("body") {
        Some(Value::Array(a)) => a.first().and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string),
        Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
        _ => None,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PreviewAsk {
    pub kind: &'static str,
    pub params: Value,
    /// The engine returns fins beside the part (`preview_only`), not a second copy of the body.
    pub fins_only: bool,
    pub busy: &'static str,
}

pub fn preview_ask(tool: &str, values: &Values, ctx: &Ctx) -> Option<PreviewAsk> {
    let (kind, busy) = match tool {
        "supportfins" => ("supportfins", BUSY_FINS),
        "supportwall" => ("supportwall", BUSY_WALLS),
        _ => return None,
    };
    Some(PreviewAsk { kind, params: to_params(tool, values, ctx), fins_only: true, busy })
}

pub fn strength_ask(tool: &str, values: &Values) -> Option<strength::StrengthRequest> {
    let body = body_id(values)?;
    if !orient_available() {
        return None;
    }
    let params = to_params(tool, values, &empty_ctx());
    let load = values.get("_load").cloned().unwrap_or(Value::Null);
    Some(strength::strength_request(&body, strength::options_object(&params), &load))
}

pub fn suggest_ask(tool: &str, values: &Values) -> Result<strength::SuggestRequest, &'static str> {
    let body = body_id(values).ok_or(strength::PICK_PART)?;
    let params = to_params(tool, values, &empty_ctx());
    Ok(strength::suggest_request(&body, strength::options_object(&params)))
}

fn empty_ctx() -> Ctx<'static> {
    Ctx { bodies: &[], planes: &[], sketches: &[], features: &[], selection: &[], last_sketch: None, sketch_on_body: false, inch: false }
}

pub fn status_of(values: &Values, cover: Option<&Cover>, plastic: Option<&Plastic>, material: &str, reach: f64, with_cover: bool) -> Status {
    strength::status_cards(values, cover, plastic, material, reach, with_cover)
}

pub fn credit_line(version: &str) -> String {
    format!("Support Fins {version} \u{00b7} gittrahan (MIT)")
}

#[derive(Clone, Debug, PartialEq)]
pub struct About {
    pub title: &'static str,
    pub paragraphs: Vec<String>,
    pub links: Vec<(String, String)>,
}

/// Help › About Support Fins, once [`install`] has run.
pub fn about(web: bool) -> Option<About> {
    let g = installed()?;
    let short: String = g.commit.chars().take(7).collect();
    let mut paragraphs = vec![
        format!("Support Fins {} ({short}) adds the breakaway support fins that let a part print tipped over, in its strong orientation. The fins come from Support Fins' own engine, unchanged.", g.version),
        "By gittrahan, MIT licence: github.com/gittrahan/support-fins \u{00b7} printfins.com. The technique is Slant3D's designed-in supports.".into(),
    ];
    if web {
        paragraphs.push("New Support Fins releases reach the web edition automatically.".into());
    } else {
        paragraphs.push("New Support Fins releases are downloaded automatically when update checks are on (Help \u{203a} Update Settings).".into());
    }
    if let Some(pending) = g.pending {
        let ver = pending.strip_prefix('v').unwrap_or(pending);
        paragraphs.push(format!("Support Fins {ver} has been downloaded and is used from the next time you start STL Editor. Restart now"));
    }
    let source = if g.source.is_empty() { "https://github.com/gittrahan/support-fins" } else { g.source };
    Some(About {
        title: "Support Fins",
        paragraphs,
        links: vec![("github.com/gittrahan/support-fins".into(), source.into()), ("printfins.com".into(), "https://printfins.com".into())],
    })
}

pub fn notice() -> Option<&'static str> {
    installed().and_then(|g| g.notice)
}

/// The platform setting that keeps the folding sections between runs (1.x `localStorage`).
pub const SECTIONS_KEY: &str = "stle_sections";

/// Folding sections, remembered across runs through the platform's setting store (1.x
/// `stle_sections`: one JSON object, `"tool:header": open`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SectionMemory {
    map: BTreeMap<String, bool>,
}

impl SectionMemory {
    /// What 1.x kept in `localStorage`; anything that isn't an object of booleans is ignored.
    pub fn from_json(text: &str) -> SectionMemory {
        let map = match serde_json::from_str::<Value>(text) {
            Ok(Value::Object(m)) => m.into_iter().filter_map(|(k, v)| v.as_bool().map(|b| (k, b))).collect(),
            _ => BTreeMap::new(),
        };
        SectionMemory { map }
    }
    pub fn to_json(&self) -> String {
        Value::Object(self.map.iter().map(|(k, v)| (k.clone(), Value::Bool(*v))).collect()).to_string()
    }
    pub fn is_open(&self, tool: &str, header: &str, default_open: bool) -> bool {
        self.map.get(&format!("{tool}:{header}")).copied().unwrap_or(default_open)
    }
    pub fn toggle(&mut self, tool: &str, header: &str, default_open: bool) {
        let key = format!("{tool}:{header}");
        let cur = self.map.get(&key).copied().unwrap_or(default_open);
        self.map.insert(key, !cur);
    }
}

pub fn section_default_open(tool: &str, header: &str) -> bool {
    fields(tool).iter().find(|f| f.key == header).map(|f| f.default_open).unwrap_or(true)
}

/// `Some("2 changed")` when the section is closed and something inside it is not the default.
pub fn section_badge(tool: &str, header: &str, values: &Values, defs: &Values, mem: &SectionMemory, material: &str, reach: f64) -> Option<String> {
    let specs = fields(tool);
    let start = specs.iter().position(|f| f.kind == SpecKind::Hdr && f.key == header)?;
    if !field_visible(&specs[start], values) {
        return None;
    }
    if mem.is_open(tool, header, specs[start].default_open) {
        return None;
    }
    let mut end = start + 1;
    while end < specs.len() && specs[end].kind != SpecKind::Hdr {
        end += 1;
    }
    let n = specs[start + 1..end].iter().filter(|f| field_visible(f, values) && field_changed(f, values, defs, material, reach)).count();
    if n == 0 { None } else { Some(format!("{n} changed")) }
}

fn field_changed(spec: &FieldSpec, values: &Values, defs: &Values, material: &str, reach: f64) -> bool {
    match spec.kind {
        SpecKind::Material => strength::material_key(material) != "pla",
        SpecKind::Reach => (reach - 5.0).abs() > 1e-9,
        _ => {
            if !defs.contains_key(spec.key) {
                return false;
            }
            value_changed(values.get(spec.key), defs.get(spec.key))
        }
    }
}

fn value_changed(a: Option<&Value>, b: Option<&Value>) -> bool {
    let Some(b) = b else { return false };
    let a_owned;
    let a = match a {
        Some(v) => v,
        None => {
            a_owned = Value::Null;
            &a_owned
        }
    };
    if let (Some(x), Some(y)) = (coerce_num(a), coerce_num(b)) {
        return (x - y).abs() > 1e-9;
    }
    a != b
}

fn coerce_num(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => {
            let t = s.trim();
            if t.is_empty() || t.starts_with('=') {
                return None;
            }
            t.parse::<f64>().ok().filter(|n| n.is_finite())
        }
        _ => None,
    }
}

pub struct Session {
    pub tool_key: String,
    pub edit_id: Option<String>,
    pub values: Values,
    pub defs: Values,
    pub sections: SectionMemory,
    pub material: String,
    pub reach: f64,
    pub cover: Option<Cover>,
    pub plastic: Option<Plastic>,
    pub strength: Option<Value>,
    pub suggest: SuggestUi,
    pub version: String,
    /// Draw Walls: the shape picked, the points of the wall in progress, how each wall came out.
    pub wall_shape: String,
    pub wall_pending: usize,
    pub wall_ok: Vec<bool>,
    buffers: crate::tools::fields::Bufs,
    /// The names of the bodies, for the Body chip (the panel fills it each frame).
    pub body_names: BTreeMap<String, String>,
    /// The Body box is the pick field that viewport clicks fill (`activeField`).
    pub body_active: bool,
}

impl Session {
    /// The tool's values as they are now (picks, the gizmo and walls change them outside the form).
    pub fn set_values(&mut self, v: Values) {
        if self.values != v {
            self.values = v;
        }
    }

    pub fn open(tool: &str, ctx: &Ctx) -> Self {
        let values = defaults(tool, ctx);
        Self::new(tool, None, values.clone(), values)
    }
    pub fn open_edit(tool: &str, params: Value, mode: Option<&str>, ctx: &Ctx, edit_id: &str) -> Self {
        let defs = defaults(tool, ctx);
        let mut values = from_params(tool, params);
        if let Some(m) = mode {
            values.insert("_mode".into(), Value::String(m.into()));
        }
        Self::new(tool, Some(edit_id.to_string()), values, defs)
    }
    fn new(tool: &str, edit_id: Option<String>, values: Values, defs: Values) -> Self {
        Session {
            tool_key: tool.to_string(),
            edit_id,
            values,
            defs,
            sections: SectionMemory::default(),
            material: "pla".into(),
            reach: 5.0,
            cover: None,
            plastic: None,
            strength: None,
            suggest: SuggestUi::Idle,
            version: engine_version().to_string(),
            wall_shape: "line".into(),
            wall_pending: 0,
            wall_ok: Vec::new(),
            buffers: Default::default(),
            body_names: BTreeMap::new(),
            body_active: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum PanelEvent {
    Edited(String),
    /// A section was folded or opened: the remembered state to keep (`stle_sections`).
    Sections(String),
    QuickPaint,
    Strength,
    ResetWalls,
    ClearHover,
    Hint(String),
    Suggest,
    UseCandidate(usize),
    UseStrongest,
    FillGaps,
    RestoreRemoved,
    Material(String),
    Reach(f64),
    /// Draw Walls: a shape button, Finish wall, Undo, Clear all.
    WallShape(String),
    WallFinish,
    WallUndo,
    WallClear,
}

/// Draw the Support Fins form into `ui`. The caller owns the window. The look is 1.x's
/// `#tool-form` CSS: the same label column, framed boxes, `.seg` strips and `.field-hdr` sections
/// as the other tools (the widgets come from `tools::fields`).
pub fn show(ui: &mut egui::Ui, tk: &Tokens, session: &mut Session, wall_hint: &str) -> Vec<PanelEvent> {
    let mut out = Vec::new();
    if !is_installed() {
        ui.label(egui::RichText::new(LOADING).size(12.0).color(tk.muted));
        return out;
    }
    let tool = session.tool_key.clone();
    let specs = fields(&tool);
    let mut i = 0;
    while i < specs.len() {
        let spec = specs[i];
        if spec.kind == SpecKind::Hdr {
            if !field_visible(&spec, &session.values) {
                i += 1;
                continue;
            }
            let open = session.sections.is_open(&tool, spec.key, spec.default_open);
            let badge = section_badge(&tool, spec.key, &session.values, &session.defs, &session.sections, &session.material, session.reach);
            ui.add_space(2.0); // the 6 px after the row above plus 2 = the header's 8 px margin
            if fold_header(ui, tk, spec.label, open, badge.as_deref()) {
                session.sections.toggle(&tool, spec.key, spec.default_open);
                out.push(PanelEvent::Sections(session.sections.to_json()));
            }
            ui.add_space(6.0);
            if !open {
                let mut end = i + 1;
                while end < specs.len() && specs[end].kind != SpecKind::Hdr {
                    end += 1;
                }
                i = end;
                continue;
            }
            i += 1;
            continue;
        }
        if !field_visible(&spec, &session.values) {
            i += 1;
            continue;
        }
        draw_field(ui, tk, session, &spec, wall_hint, &mut out);
        ui.add_space(6.0);
        i += 1;
    }
    // the credit sits in the tool panel's footer (`OpenTool::credit`), as 1.x's `#tool-credit`
    let _ = session.edit_id.as_ref();
    out
}

/// `.field-hdr`: a full-width row with a divider above, a chevron, the title in small capitals and
/// "N changed" at the right when the section is closed. Returns true when it was clicked.
fn fold_header(ui: &mut egui::Ui, tk: &Tokens, label: &str, open: bool, badge: Option<&str>) -> bool {
    use egui::{Align2, FontFamily, FontId, Rect, Sense, Stroke};
    let (r, _) = ui.allocate_exact_size(egui::vec2(fields::FORM_W, 28.0), Sense::hover());
    // `width: calc(100% + 20px); margin: 8px -10px 2px`: it runs to both panel edges
    let full = Rect::from_min_max(egui::pos2(r.min.x - 10.0, r.min.y), egui::pos2(r.max.x + 10.0, r.max.y));
    let resp = ui.interact(full, ui.id().with(("fold", label)), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
    let hot = resp.hovered();
    if hot {
        ui.painter().rect_filled(full, 0.0, tk.hover);
    }
    ui.painter().hline(full.x_range(), full.min.y, Stroke::new(1.0, tk.divider));
    let ink = if hot { tk.text } else { tk.muted };
    // the chevron points right when the section is closed and down when open
    let c = egui::pos2(full.min.x + 10.0 + 5.0, full.center().y + 0.5);
    let st = Stroke::new(1.4, ink);
    let pts = if open { [c + egui::vec2(-3.5, -1.5), c + egui::vec2(0.0, 2.5), c + egui::vec2(3.5, -1.5)] } else { [c + egui::vec2(-1.5, -3.5), c + egui::vec2(2.5, 0.0), c + egui::vec2(-1.5, 3.5)] };
    ui.painter().line_segment([pts[0], pts[1]], st);
    ui.painter().line_segment([pts[1], pts[2]], st);
    let mut right = full.max.x - 10.0;
    if let Some(b) = badge {
        let g = ui.painter().layout_no_wrap(b.to_owned(), FontId::proportional(10.5), tk.muted);
        right -= g.size().x;
        ui.painter().galley(egui::pos2(right, full.center().y - g.size().y / 2.0), g, tk.muted);
        right -= 6.0;
    }
    let text = label.to_uppercase();
    let font = FontId::new(11.0, FontFamily::Name("bold".into()));
    // letter-spacing .5px: drawn one letter at a time
    let mut x = full.min.x + 10.0 + 10.0 + 6.0;
    for ch in text.chars() {
        let g = ui.painter().layout_no_wrap(ch.to_string(), font.clone(), ink);
        if x + g.size().x > right {
            break;
        }
        let w = g.size().x;
        ui.painter().galley(egui::pos2(x, full.center().y - g.size().y / 2.0), g, ink);
        x += w + 0.5;
    }
    let _ = Align2::LEFT_CENTER;
    resp.clicked()
}

fn draw_field(ui: &mut egui::Ui, tk: &Tokens, session: &mut Session, spec: &FieldSpec, wall_hint: &str, out: &mut Vec<PanelEvent>) {
    match spec.kind {
        SpecKind::Select if spec.key == "_mode" => draw_modes(ui, tk, session, wall_hint, out),
        SpecKind::Bool => {
            let mut on = session.values.get(spec.key).and_then(Value::as_bool).unwrap_or(false);
            let changed = fields::row_raw(ui, tk, spec.label, spec.help, false, |ui, _| fields::check_box(ui, tk, &mut on).changed());
            if changed {
                apply_edit(session, spec.key, Value::Bool(on), wall_hint, out);
            }
        }
        SpecKind::Select => draw_select(ui, tk, session, spec, wall_hint, out),
        SpecKind::Number | SpecKind::Reach => draw_number(ui, tk, session, spec, wall_hint, out),
        SpecKind::Material => draw_material(ui, tk, session, out),
        SpecKind::Status => draw_status(ui, tk, session, spec.cover, out),
        SpecKind::Pose => draw_pose(ui, tk, session, out),
        SpecKind::Walls => draw_walls(ui, tk, session, spec, out),
        SpecKind::Removed => {
            let n = session.values.get("removed").and_then(Value::as_array).map(|a| a.len()).unwrap_or(0);
            let mut restore = false;
            fields::row_raw(ui, tk, spec.label, None, false, |ui, _| {
                if n == 0 {
                    ui.label(egui::RichText::new("None: click a fin to remove it").size(12.0).color(tk.muted));
                } else {
                    ui.label(egui::RichText::new(format!("{n} fin{}", if n > 1 { "s" } else { "" })).size(12.0).color(tk.text));
                    restore = crate::tools::panel::ctl_button(ui, tk, "Bring all back", true).clicked();
                }
            });
            if restore {
                apply_edit(session, "removed", json!([]), wall_hint, out);
                out.push(PanelEvent::RestoreRemoved);
            }
        }
        SpecKind::Bodies => {
            // the body chip with its name and a ×, as the generic `bodies` field
            let ids: Vec<String> = match session.values.get(spec.key) {
                Some(Value::Array(a)) => a.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect(),
                Some(Value::String(s)) if !s.is_empty() => vec![s.clone()],
                _ => Vec::new(),
            };
            let chips: Vec<String> = ids.iter().map(|id| session.body_names.get(id).cloned().unwrap_or_else(|| id.clone())).collect();
            let ph = spec.placeholder.unwrap_or("Click a body");
            let b = fields::row_raw(ui, tk, spec.label, spec.help, false, |ui, _| fields::bodies::sel_box(ui, tk, egui::Id::new(("sf-pick", spec.key)), session.body_active, &chips, ph));
            if let Some(i) = b.removed {
                let rest: Vec<Value> = ids.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, s)| Value::from(s.clone())).collect();
                session.body_active = true;
                apply_edit(session, spec.key, Value::Array(rest), "", out);
            } else if b.clicked {
                session.body_active = !session.body_active;
            }
        }
        SpecKind::Hdr => {}
    }
}

/// Draw Walls' controls (1.x `WallDrawer.fieldHTML`): the shape buttons, the count, Undo and Clear all.
fn draw_walls(ui: &mut egui::Ui, tk: &Tokens, session: &mut Session, spec: &FieldSpec, out: &mut Vec<PanelEvent>) {
    let shapes = super::walls::visible_shapes(engine_features().gussets, engine_features().anchors, session.tool_key == "supportfins");
    if !shapes.contains(&session.wall_shape.as_str()) {
        session.wall_shape = "line".into();
    }
    let pairs: Vec<(String, String)> = shapes.iter().map(|k| ((*k).to_string(), super::walls::ALL_SHAPES.iter().find(|(x, _)| x == k).map(|(_, l)| (*l).to_string()).unwrap_or_else(|| (*k).to_string()))).collect();
    let cur = session.wall_shape.clone();
    let hint = super::walls::hint_for(&cur);
    let fits = fields::select::seg_fits(ui, &pairs, false);
    let pick = fields::row_raw(ui, tk, "Draw", Some(hint), false, |ui, w| {
        let mut pick = None;
        if fits {
            pick = fields::seg_strip(ui, tk, ui.id().with("wshape"), w, &pairs, &cur);
        } else {
            let text = pairs.iter().find(|(k, _)| *k == cur).map(|(_, l)| l.clone()).unwrap_or_default();
            fields::framed_combo(ui, tk, "wshape-sel", w, &text, |ui| {
                for (k, l) in &pairs {
                    if ui.selectable_label(*k == cur, l).clicked() {
                        pick = Some(k.clone());
                    }
                }
            });
        }
        pick
    });
    if let Some(k) = pick {
        if k != cur {
            out.push(PanelEvent::WallShape(k));
        }
    }
    ui.add_space(6.0);
    let n = session.values.get("walls").and_then(Value::as_array).map(|a| a.len()).unwrap_or(0);
    let bad = session.wall_ok.iter().filter(|ok| !**ok).count();
    fields::row_raw(ui, tk, spec.label, None, false, |ui, _| {
        if n == 0 {
            ui.label(egui::RichText::new("None yet: click on the part").size(12.0).color(tk.muted));
        } else {
            let mut t = format!("{n} wall{}", if n > 1 { "s" } else { "" });
            if bad > 0 {
                t.push_str(&format!(" ({bad} couldn’t be placed)"));
            }
            ui.label(egui::RichText::new(t).size(12.0).color(tk.text));
        }
        if session.wall_pending > 0 {
            ui.label(egui::RichText::new(format!("· drawing ({} point{})", session.wall_pending, if session.wall_pending > 1 { "s" } else { "" })).size(12.0).color(tk.text));
        }
    });
    ui.add_space(6.0);
    let pending = session.wall_pending;
    let path_ready = session.wall_shape == "path" && pending >= 2;
    fields::row_raw(ui, tk, "", None, false, |ui, _| {
        if path_ready && crate::tools::panel::ctl_button(ui, tk, "Finish wall", true).clicked() {
            out.push(PanelEvent::WallFinish);
        }
        if crate::tools::panel::ctl_button(ui, tk, "Undo", n > 0 || pending > 0).on_hover_text("Ctrl+Z").clicked() {
            out.push(PanelEvent::WallUndo);
        }
        if crate::tools::panel::ctl_button(ui, tk, "Clear all", n > 0).clicked() {
            out.push(PanelEvent::WallClear);
        }
    });
}

fn draw_modes(ui: &mut egui::Ui, tk: &Tokens, session: &mut Session, wall_hint: &str, out: &mut Vec<PanelEvent>) {
    let cur = mode_of(&session.values).to_string();
    let pairs: Vec<(String, String)> = MODES.iter().map(|(k, l)| ((*k).to_string(), (*l).to_string())).collect();
    // `.field.wide.nolabel`: the strip takes the whole row, the help is the hover text
    let pick = fields::row_raw(ui, tk, "", Some(MODE_HELP), true, |ui, w| fields::seg_strip(ui, tk, ui.id().with("mode"), w, &pairs, &cur));
    if let Some(k) = pick {
        if k != cur {
            let fx = set_mode(&mut session.values, &k, wall_hint);
            push_effects(fx, out);
            out.push(PanelEvent::Edited("_mode".into()));
        }
    }
}

fn draw_select(ui: &mut egui::Ui, tk: &Tokens, session: &mut Session, spec: &FieldSpec, wall_hint: &str, out: &mut Vec<PanelEvent>) {
    let cur = session.values.get(spec.key).and_then(Value::as_str).unwrap_or("").to_string();
    let text = spec.options.iter().find(|(k, _)| *k == cur).map(|(_, l)| *l).unwrap_or("");
    let pairs: Vec<(String, String)> = spec.options.iter().map(|(k, l)| ((*k).to_string(), (*l).to_string())).collect();
    let seg = spec.options.len() <= 4 && fields::select::seg_fits(ui, &pairs, spec.nolabel);
    let picked = fields::row_raw(ui, tk, spec.label, spec.help, spec.nolabel, |ui, w| {
        let mut picked = None;
        if seg {
            picked = fields::seg_strip(ui, tk, ui.id().with(("sfseg", spec.key)), w, &pairs, &cur);
        } else {
            fields::framed_combo(ui, tk, ("sf", spec.key), w, text, |ui| {
                for (k, l) in spec.options {
                    if ui.selectable_label(*k == cur, *l).clicked() {
                        picked = Some((*k).to_string());
                    }
                }
            });
        }
        picked
    });
    if let Some(k) = picked {
        if k != cur {
            apply_edit(session, spec.key, Value::String(k), wall_hint, out);
        }
    }
}

fn draw_number(ui: &mut egui::Ui, tk: &Tokens, session: &mut Session, spec: &FieldSpec, wall_hint: &str, out: &mut Vec<PanelEvent>) {
    let shown = if spec.kind == SpecKind::Reach { strength::js_num(session.reach) } else { display_value(session.values.get(spec.key)) };
    let expr = shown.trim_start().starts_with('=');
    let (font, colour) = if expr { (fields::number::expr_font(), tk.expr) } else { (fields::number::input_font(), tk.text) };
    let uw = if spec.unit.is_some() { 22.0 + 4.0 } else { 0.0 };
    let buffers = &mut session.buffers;
    let edited = fields::row_raw(ui, tk, spec.label, spec.help, false, |ui, w| {
        let typed = fields::text_commit(ui, tk, buffers, spec.key, &shown, w - uw, font, colour, None);
        if let Some(unit) = spec.unit {
            fields::unit_label(ui, tk, unit);
        }
        typed
    });
    if let Some(text) = edited {
        if spec.kind == SpecKind::Reach {
            if let Some(v) = parse_float_js(&text) {
                if v > 0.0 {
                    session.reach = v;
                    out.push(PanelEvent::Reach(v));
                }
            }
        } else if text.trim().starts_with('=') {
            apply_edit(session, spec.key, Value::String(text), wall_hint, out);
        } else if let Some(n) = parse_float_js(&text) {
            apply_edit(session, spec.key, num_value(n), wall_hint, out);
        }
    }
}

fn display_value(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => strength::js_num(n.as_f64().unwrap_or(0.0)),
        Some(Value::Bool(b)) => b.to_string(),
        _ => String::new(),
    }
}

fn draw_material(ui: &mut egui::Ui, tk: &Tokens, session: &mut Session, out: &mut Vec<PanelEvent>) {
    let cur = session.material.clone();
    let name = strength::material_name(&cur);
    let picked = fields::row_raw(ui, tk, "Material", Some(MAT_HELP), false, |ui, w| {
        let mut picked = None;
        fields::framed_combo(ui, tk, "sf-mat", w, name, |ui| {
            for (k, name, _) in strength::MATERIALS {
                if ui.selectable_label(*k == cur, *name).clicked() {
                    picked = Some((*k).to_string());
                }
            }
        });
        picked
    });
    if let Some(k) = picked {
        if k != cur {
            session.material = k.clone();
            out.push(PanelEvent::Material(k));
        }
    }
}

fn draw_status(ui: &mut egui::Ui, tk: &Tokens, session: &mut Session, cover: bool, out: &mut Vec<PanelEvent>) {
    let status = status_of(&session.values, session.cover.as_ref(), session.plastic.as_ref(), &session.material, session.reach, cover);
    ui.add_space(2.0); // .sf-status { margin: 8px 0 }
    let mut cards: Vec<&strength::Card> = Vec::new();
    if let Some(held) = &status.held {
        cards.push(held);
    }
    cards.push(&status.plastic);
    cards_row(ui, tk, &cards);
    if !status.note.trim().is_empty() {
        ui.add_space(6.0);
        // `.sf-note`: the text, the Fill gaps button inline, then the rest of the sentence
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
            ui.label(egui::RichText::new(status.note.trim()).size(12.0).color(tk.muted));
            if status.fill_gaps {
                if crate::tools::panel::ctl_button(ui, tk, "Fill gaps", true).clicked() {
                    out.push(PanelEvent::FillGaps);
                }
                ui.label(egui::RichText::new(&status.note_tail).size(12.0).color(tk.muted));
            }
        });
    }
    ui.add_space(2.0);
}

/// `.sf-cards`: equal columns, 6 px apart, all as tall as the tallest.
fn cards_row(ui: &mut egui::Ui, tk: &Tokens, cards: &[&strength::Card]) {
    use egui::{Rect, Sense, Shape, Stroke};
    let n = cards.len().max(1) as f32;
    let cw = (fields::FORM_W - 6.0 * (n - 1.0)) / n;
    let top = ui.cursor().min;
    let bgs: Vec<_> = cards.iter().map(|_| ui.painter().add(Shape::Noop)).collect();
    let mut h: f32 = 0.0;
    for (i, c) in cards.iter().enumerate() {
        let x = top.x + i as f32 * (cw + 6.0);
        let r = Rect::from_min_size(egui::pos2(x + 8.0, top.y + 6.0), egui::vec2(cw - 16.0, 0.0));
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_max(r.min, egui::pos2(r.max.x, top.y + 400.0))).layout(egui::Layout::top_down(egui::Align::LEFT)));
        child.spacing_mut().item_spacing.y = 0.0;
        card(&mut child, tk, c);
        h = h.max(child.min_rect().height() + 12.0);
    }
    for (i, bg) in bgs.into_iter().enumerate() {
        let x = top.x + i as f32 * (cw + 6.0);
        let r = Rect::from_min_size(egui::pos2(x, top.y), egui::vec2(cw, h));
        ui.painter().set(bg, Shape::Rect(egui::epaint::RectShape::new(r, 6.0, tk.input, Stroke::new(1.0, tk.input_line), egui::StrokeKind::Inside)));
        let resp = ui.interact(r, ui.id().with(("sfcard", i)), Sense::hover());
        if !cards[i].tip.is_empty() {
            resp.on_hover_text(cards[i].tip.clone());
        }
    }
    ui.allocate_space(egui::vec2(fields::FORM_W, h));
}

fn card(ui: &mut egui::Ui, tk: &Tokens, card: &strength::Card) {
    let color = match card.tone {
        strength::Tone::Ok => tk.ok,
        strength::Tone::Bad => tk.danger,
        strength::Tone::Muted | strength::Tone::Busy => tk.muted,
        strength::Tone::Plain => tk.strong,
    };
    ui.label(egui::RichText::new(card.caption).size(11.0).color(tk.muted));
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        if card.tone == strength::Tone::Busy {
            ui.add(egui::Spinner::new().size(10.0).color(tk.accent));
        }
        ui.add(egui::Label::new(egui::RichText::new(&card.value).size(if card.tone == strength::Tone::Bad && card.value.len() > 14 { 12.0 } else { 14.0 }).color(color)).wrap());
    });
    if !card.sub.is_empty() {
        ui.add(egui::Label::new(egui::RichText::new(&card.sub).size(11.0).color(tk.muted)).wrap());
    }
}

fn draw_pose(ui: &mut egui::Ui, tk: &Tokens, session: &mut Session, out: &mut Vec<PanelEvent>) {
    if !orient_available() {
        return;
    }
    use crate::tools::panel::ctl_button;
    let view = strength::pose_view(true, &session.values, &session.suggest, session.strength.as_ref(), &session.material);
    // `.sf-pose`: buttons, then notes and candidates, 6 px apart
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        if ctl_button(ui, tk, "Suggest orientations", view.busy.is_none()).clicked() {
            out.push(PanelEvent::Suggest);
        }
        if view.show_strongest && ctl_button(ui, tk, "Turn to strongest pose", true).clicked() {
            out.push(PanelEvent::UseStrongest);
        }
    });
    let note = |ui: &mut egui::Ui, text: &str, col: egui::Color32| {
        ui.add_space(6.0);
        ui.add(egui::Label::new(egui::RichText::new(text).size(12.0).color(col)).wrap());
    };
    if let Some(b) = view.busy {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.add(egui::Spinner::new().size(11.0).color(tk.accent));
            ui.add_space(8.0);
            ui.add(egui::Label::new(egui::RichText::new(b).size(12.0).color(tk.muted)).wrap());
        });
    }
    if let Some(e) = &view.error {
        note(ui, e, tk.danger);
    }
    if let Some(c) = view.confidence {
        note(ui, c, tk.muted);
    }
    for row in &view.rows {
        ui.add_space(6.0);
        egui::Frame::new().stroke(egui::Stroke::new(1.0, tk.line)).corner_radius(egui::CornerRadius::same(tk.radius_sm as u8)).inner_margin(egui::Margin { left: 7, right: 4, top: 3, bottom: 3 }).show(ui, |ui| {
            ui.set_width(fields::FORM_W - 13.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                let used = ui.available_width();
                ui.allocate_ui_with_layout(egui::vec2(used - 40.0, 0.0), egui::Layout::top_down(egui::Align::LEFT), |ui| {
                    ui.add(egui::Label::new(egui::RichText::new(format!("{} \u{00b7} {}", row.title, row.detail)).size(12.0).color(tk.text)).wrap());
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ctl_button(ui, tk, "Use", true).clicked() {
                        out.push(PanelEvent::UseCandidate(row.index));
                    }
                });
            });
        });
    }
    if let Some(p) = view.posture {
        ui.add_space(6.0);
        ui.add(egui::Label::new(egui::RichText::new(p).size(12.0).color(tk.muted))).on_hover_text(view.posture_tip);
    }
    ui.add_space(6.0);
    let mut picked = None;
    let sel = view.load_selected.clone();
    let label = view.load_options.iter().find(|(k, _)| *k == sel).map(|(_, l)| *l).unwrap_or("Not set");
    fields::row_raw(ui, tk, "Load in use", Some(view.load_help), false, |ui, w| {
        fields::framed_combo(ui, tk, "sf-load", w, label, |ui| {
            for (k, l) in &view.load_options {
                if ui.selectable_label(*k == sel, *l).clicked() {
                    picked = Some((*k).to_string());
                }
            }
        });
    });
    if let Some(k) = picked {
        if k != view.load_selected && k != "turned" {
            apply_edit(session, "_load", Value::String(k), "", out);
        }
    }
    if let Some(text) = &view.verdict {
        let (bg, fg) = match view.verdict_quality.as_deref() {
            Some("good") => (tk.good_bg, tk.good_text),
            Some("mixed") => (tk.warn_bg, tk.warn_text),
            Some("poor") => (tk.err_bg, tk.err_text),
            _ => (tk.info_bg, tk.info_text),
        };
        ui.add_space(6.0);
        egui::Frame::new().fill(bg).corner_radius(egui::CornerRadius::same(4)).inner_margin(egui::Margin::symmetric(7, 5)).show(ui, |ui| {
            ui.set_width(fields::FORM_W - 14.0);
            ui.add(egui::Label::new(egui::RichText::new(text).size(12.0).color(fg)).wrap());
        });
    }
}

fn apply_edit(session: &mut Session, key: &str, value: Value, wall_hint: &str, out: &mut Vec<PanelEvent>) {
    session.values.insert(key.to_string(), value);
    let fx = on_form_change(&session.tool_key, key, &session.values, wall_hint);
    push_effects(fx, out);
    out.push(PanelEvent::Edited(key.to_string()));
}

fn push_effects(fx: FormEffects, out: &mut Vec<PanelEvent>) {
    if fx.quick_paint {
        out.push(PanelEvent::QuickPaint);
    }
    if fx.reset_walls {
        out.push(PanelEvent::ResetWalls);
    }
    if fx.clear_preview_hover {
        out.push(PanelEvent::ClearHover);
    }
    if fx.request_strength {
        out.push(PanelEvent::Strength);
    }
    if let Some(h) = fx.hint {
        out.push(PanelEvent::Hint(h));
    }
}

pub fn engine_host() -> &'static str {
    installed().map(|g| g.host).unwrap_or("")
}
