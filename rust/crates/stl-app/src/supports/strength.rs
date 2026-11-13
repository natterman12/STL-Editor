//! Strength, suggested orientations, and the Held / plastic readouts.
//!
//! Port of `sfStrength`, `sfPoseHTML` and `sfStatusHTML` in `web/js/app.js`. The engine
//! still works out the numbers (`strengthOfJson`, `suggestPosesJson`); this module only
//! asks for them and turns the answer into the words the panel shows.

use serde_json::{json, Map, Value};

use crate::tools::Values;

/// Filament densities (g/cm³). PLA and PETG are Support Fins' own; the rest match 1.x.
pub const MATERIALS: &[(&str, &str, f64)] = &[
    ("pla", "PLA", 1.24),
    ("petg", "PETG", 1.27),
    ("abs", "ABS", 1.04),
    ("asa", "ASA", 1.07),
    ("tpu", "TPU", 1.21),
    ("pa", "Nylon", 1.14),
];

pub const LOAD_HELP: &str = "Which way the part is pulled or pushed when it's in use. The strength line then says how well this pose takes it.";
pub const POSTURE_TIP: &str = "How the print's layers sit for this pose. Prints are weakest across their layers.";
pub const SUGGEST_BUSY: &str = "Trying the part every way down\u{2026}";
pub const PICK_PART: &str = "Pick the part first.";

const LOADS: &[(&str, [f64; 3])] = &[
    ("up", [0.0, 0.0, 1.0]),
    ("down", [0.0, 0.0, -1.0]),
    ("left", [-1.0, 0.0, 0.0]),
    ("right", [1.0, 0.0, 0.0]),
    ("front", [0.0, -1.0, 0.0]),
    ("back", [0.0, 1.0, 0.0]),
];

/// A known filament, or PLA when the stored name is missing.
pub fn material_key(key: &str) -> &'static str {
    MATERIALS.iter().find(|(k, _, _)| *k == key).map(|(k, _, _)| *k).unwrap_or("pla")
}

pub fn material_name(key: &str) -> &'static str {
    let key = material_key(key);
    MATERIALS.iter().find(|(k, _, _)| *k == key).map(|(_, n, _)| *n).unwrap_or("PLA")
}

pub fn material_density(key: &str) -> f64 {
    let key = material_key(key);
    MATERIALS.iter().find(|(k, _, _)| *k == key).map(|(_, _, d)| *d).unwrap_or(1.24)
}

/// The load direction as a vector. A stored array is a direction that turned with the part.
/// `none` and anything unknown are no load.
pub fn load_vector(load: &Value) -> Option<[f64; 3]> {
    if let Some(a) = load.as_array() {
        if a.len() >= 3 {
            return Some([num(&a[0]), num(&a[1]), num(&a[2])]);
        }
        return None;
    }
    let key = load.as_str().unwrap_or("none");
    LOADS.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
}

/// Snap a direction back to a named load, or keep the numbers (1.x `poseDrop`).
pub fn match_load(v: [f64; 3]) -> Value {
    let w = v.map(round_dir);
    for (name, dir) in LOADS {
        if dir.iter().zip(w).all(|(a, b)| (a - b).abs() < 1e-3) {
            return Value::String((*name).into());
        }
    }
    json!(w.to_vec())
}

/// Turn a load with the part. `rot` is row-major and applied as v' = R v, then normalised,
/// which is what three.js `transformDirection` does with that matrix.
pub fn turn_load(load: [f64; 3], rot: [[f64; 3]; 3]) -> Value {
    let mut v = [
        rot[0][0] * load[0] + rot[0][1] * load[1] + rot[0][2] * load[2],
        rot[1][0] * load[0] + rot[1][1] * load[1] + rot[1][2] * load[2],
        rot[2][0] * load[0] + rot[2][1] * load[1] + rot[2][2] * load[2],
    ];
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len > 0.0 {
        v = [v[0] / len, v[1] / len, v[2] / len];
    }
    match_load(v)
}

fn round_dir(x: f64) -> f64 {
    if x.abs() < 1e-6 {
        0.0
    } else {
        (x * 1e4).round() / 1e4
    }
}

pub fn posture_text(posture: &str) -> Option<&'static str> {
    Some(match posture {
        "strong" => "Lying down: its longest side runs along the layers, the strong way to print it.",
        "mixed" => "At an angle: its longest side crosses the layers on a slant.",
        "weak" => "Standing up: its longest side runs up through the layers, where prints split first.",
        _ => return None,
    })
}

fn posture_short(posture: &str) -> Option<&'static str> {
    Some(match posture {
        "strong" => "lying down",
        "mixed" => "on a slant",
        "weak" => "standing up",
        _ => return None,
    })
}

pub fn confidence_note(confidence: &str) -> Option<&'static str> {
    Some(match confidence {
        "high" => "A clear winner.",
        "medium" => "A good pick, though the next one is close.",
        "low" => "Several poses are about as good: choose by how the part is loaded.",
        "none" => "No pose sits flat on the plate: every way it balances on a point.",
        _ => return None,
    })
}

/// One suggested pose, as the list shows it.
pub fn format_candidate(c: &Value, material: &str) -> String {
    let over = c.get("overArea").map(num).unwrap_or(0.0).round();
    let height = c.get("height").map(num).unwrap_or(0.0).round();
    let mut bits = vec![format!("{over:.0} mm\u{00b2} to support"), format!("{height:.0} mm tall")];
    if let Some(walls) = c.get("walls").filter(|v| !v.is_null()) {
        let walls = walls.as_i64().unwrap_or_else(|| num(walls) as i64);
        bits.push(format!("{walls} fin{}", if walls == 1 { "" } else { "s" }));
    }
    let volume = c.get("volume").map(num).unwrap_or(0.0);
    if volume != 0.0 {
        let g = volume * material_density(material) / 1000.0;
        bits.push(format!("{} g", fixed1(g)));
    }
    let mut line = bits.join(" \u{00b7} ");
    if let Some(word) = c.get("posture").and_then(Value::as_str).and_then(posture_short) {
        line.push_str(" \u{00b7} ");
        line.push_str(word);
    }
    line
}

fn fixed1(x: f64) -> String {
    format!("{:.1}", (x * 10.0).round() / 10.0)
}

fn num(v: &Value) -> f64 {
    match v {
        Value::Number(n) => n.as_f64().unwrap_or(0.0),
        Value::String(s) => s.trim().parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

/// What the panel asks the engine (`supportfins/strength`). `load` is null when none is set.
#[derive(Clone, Debug, PartialEq)]
pub struct StrengthRequest {
    pub body: String,
    pub options: Value,
    pub load: Value,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SuggestRequest {
    pub body: String,
    pub options: Value,
    pub top: i64,
}

pub fn strength_request(body: &str, options: Value, load: &Value) -> StrengthRequest {
    let load = match load_vector(load) {
        Some(v) => json!(v.to_vec()),
        None => Value::Null,
    };
    StrengthRequest { body: body.to_string(), options, load }
}

pub fn suggest_request(body: &str, options: Value) -> SuggestRequest {
    SuggestRequest { body: body.to_string(), options, top: 3 }
}

#[derive(Clone, Debug, PartialEq)]
pub enum SuggestUi {
    Idle,
    Busy,
    Error(String),
    Ready { confidence: String, candidates: Vec<Value> },
}

impl Default for SuggestUi {
    fn default() -> Self {
        SuggestUi::Idle
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CandRow {
    pub title: String,
    pub detail: String,
    pub index: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PoseView {
    pub available: bool,
    pub busy: Option<&'static str>,
    pub error: Option<String>,
    pub confidence: Option<&'static str>,
    pub rows: Vec<CandRow>,
    pub show_strongest: bool,
    pub posture: Option<&'static str>,
    pub posture_tip: &'static str,
    pub load_help: &'static str,
    pub load_selected: String,
    pub load_options: Vec<(&'static str, &'static str)>,
    pub verdict: Option<String>,
    pub verdict_quality: Option<String>,
}

pub fn load_selected(load: Option<&Value>) -> String {
    match load {
        Some(Value::Array(_)) => "turned".into(),
        Some(Value::String(s)) if !s.is_empty() => s.clone(),
        _ => "none".into(),
    }
}

pub fn load_options(selected: &str) -> Vec<(&'static str, &'static str)> {
    let mut v = vec![
        ("none", "Not set"),
        ("up", "Pulled up"),
        ("down", "Pushed down"),
        ("left", "Pulled left"),
        ("right", "Pulled right"),
        ("front", "Pulled to the front"),
        ("back", "Pulled to the back"),
    ];
    if selected == "turned" {
        v.push(("turned", "As set (turned with the part)"));
    }
    v
}

/// The Orientation block. Empty when this engine has no strength extra.
pub fn pose_view(orient: bool, values: &Values, suggest: &SuggestUi, strength: Option<&Value>, material: &str) -> PoseView {
    let selected = load_selected(values.get("_load"));
    let mut view = PoseView {
        available: orient,
        busy: None,
        error: None,
        confidence: None,
        rows: Vec::new(),
        show_strongest: false,
        posture: None,
        posture_tip: POSTURE_TIP,
        load_help: LOAD_HELP,
        load_selected: selected.clone(),
        load_options: load_options(&selected),
        verdict: None,
        verdict_quality: None,
    };
    if !orient {
        return view;
    }
    match suggest {
        SuggestUi::Busy => view.busy = Some(SUGGEST_BUSY),
        SuggestUi::Error(e) => view.error = Some(e.clone()),
        SuggestUi::Ready { confidence, candidates } => {
            view.confidence = confidence_note(confidence);
            view.rows = candidates
                .iter()
                .enumerate()
                .map(|(i, c)| CandRow {
                    title: if i == 0 { "Best".into() } else { format!("{}", i + 1) },
                    detail: format_candidate(c, material),
                    index: i,
                })
                .collect();
        }
        SuggestUi::Idle => {}
    }
    if let Some(st) = strength {
        if let Some(p) = st.get("posture").and_then(Value::as_str) {
            view.posture = posture_text(p);
        }
        if js_present(st.get("strongest")) {
            view.show_strongest = true;
        }
        if let Some(load) = st.get("load").filter(|v| !v.is_null()) {
            view.verdict = load.get("text").and_then(Value::as_str).map(str::to_string);
            view.verdict_quality = load.get("quality").and_then(Value::as_str).map(str::to_string);
        }
    }
    view
}

fn js_present(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) | Some(Value::Bool(false)) => false,
        Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0) != 0.0,
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(_)) | Some(Value::Object(_)) | Some(Value::Bool(true)) => true,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Plain,
    Muted,
    Ok,
    Bad,
    Busy,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Card {
    pub key: &'static str,
    pub caption: &'static str,
    pub value: String,
    pub tone: Tone,
    pub sub: String,
    pub tip: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Spot {
    pub far: Option<f64>,
    pub ends: bool,
}

/// The coverage answer the Held card reads. `None` on the session means the check has not landed yet.
#[derive(Clone, Debug, PartialEq)]
pub struct Cover {
    pub none: bool,
    pub busy: bool,
    pub error: Option<String>,
    pub total: f64,
    pub held: f64,
    pub spots: Vec<Spot>,
}

/// Exact support volume (mm³) and piece count from the engine preview.
#[derive(Clone, Debug, PartialEq)]
pub struct Plastic {
    pub volume: f64,
    pub pieces: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Status {
    pub held: Option<Card>,
    pub plastic: Card,
    pub note: String,
    /// What follows the inline Fill gaps button (1.x puts the button between the two parts).
    pub note_tail: String,
    pub fill_gaps: bool,
}

const PLASTIC_TIP: &str = "The plastic the supports add, from their exact volume. Set your filament under Print check \u{203a} Material.";

/// The two readouts. `with_cover` is false for Draw Walls (plastic only).
pub fn status_cards(values: &Values, cover: Option<&Cover>, plastic: Option<&Plastic>, material: &str, reach: f64, with_cover: bool) -> Status {
    let has_body = body_chosen(values);
    let plastic_card = plastic_card(has_body, plastic, material);
    if !with_cover {
        return Status { held: None, plastic: plastic_card, note: String::new(), note_tail: String::new(), fill_gaps: false };
    }
    let (value, tone, sub, note, fill) = held_value(values, cover, has_body);
    let held = Card {
        key: "_cover",
        caption: "Held",
        value,
        tone,
        sub,
        tip: format!(
            "An overhang within {} mm of a fin, or of the part itself, is held. Further than that it would sag or fail: it shows bright red. Change the distance under Print check \u{203a} Bare beyond.",
            js_num(reach)
        ),
    };
    let note_tail = if fill { "Or click a red spot to drop a fin there.".to_owned() } else { String::new() };
    Status { held: Some(held), plastic: plastic_card, note, note_tail, fill_gaps: fill }
}

fn body_chosen(values: &Values) -> bool {
    match values.get("body") {
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::String(s)) => !s.is_empty(),
        _ => false,
    }
}

fn plastic_card(has_body: bool, plastic: Option<&Plastic>, material: &str) -> Card {
    let mut value = "\u{2014}".to_string();
    let mut tone = Tone::Muted;
    let mut sub = String::new();
    if let Some(sp) = plastic {
        if has_body && sp.pieces == 0 {
            value = "None needed".into();
        } else if has_body {
            let g = sp.volume * material_density(material) / 1000.0;
            value = format!("{} g", if g < 9.95 { fixed1(g) } else { format!("{:.0}", g.round()) });
            tone = Tone::Plain;
            let pieces = if sp.pieces == 1 { "piece" } else { "pieces" };
            sub = format!("{} \u{00b7} {} cm\u{00b3} \u{00b7} {} {pieces}", material_name(material), fixed1(sp.volume / 1000.0), sp.pieces);
        }
    }
    Card { key: "_plastic", caption: "Support plastic", value, tone, sub, tip: PLASTIC_TIP.into() }
}

fn held_value(values: &Values, cover: Option<&Cover>, has_body: bool) -> (String, Tone, String, String, bool) {
    let mode = values.get("_mode").and_then(Value::as_str).unwrap_or("rotate");
    if !has_body || cover.is_some_and(|c| c.none) {
        return ("\u{2014}".into(), Tone::Muted, String::new(), String::new(), false);
    }
    let Some(c) = cover else {
        return ("Checking\u{2026}".into(), Tone::Busy, String::new(), String::new(), false);
    };
    if c.busy {
        return ("Checking\u{2026}".into(), Tone::Busy, String::new(), String::new(), false);
    }
    if let Some(err) = &c.error {
        return (err.clone(), Tone::Bad, String::new(), String::new(), false);
    }
    if !(c.total > 0.0) {
        let sub = if mode == "rotate" {
            "Turn it to the way you\u{2019}ll print it".into()
        } else {
            String::new()
        };
        return ("Nothing to hold up".into(), Tone::Muted, sub, String::new(), false);
    }
    if c.spots.is_empty() {
        return ("All held".into(), Tone::Ok, String::new(), String::new(), false);
    }
    let n = c.spots.len();
    let pct = (100.0 * c.held / c.total).floor().min(99.0) as i64;
    let value = format!("{pct}% held");
    let sub = format!("{n} bare spot{}", if n > 1 { "s" } else { "" });
    let w = &c.spots[0];
    let worst = match w.far {
        None => "Nothing holds the worst bare spot (bright red) up.".to_string(),
        Some(far) => {
            let dist = if far < 9.95 { fixed1(far) } else { format!("{:.0}", far.round()) };
            format!("The worst bare spot (bright red) is {dist} mm from a support.")
        }
    };
    let full = values.get("sf:mode").and_then(Value::as_str) == Some("full");
    let fix = if full {
        "Try more Wide-face coverage, or Add fins there."
    } else {
        "Try Placement \u{203a} Full coverage, more Wide-face coverage, or Add fins there."
    };
    let fill = mode == "add" && c.spots.iter().any(|s| s.ends);
    let note = format!("{worst} {fix}");
    (value, Tone::Bad, sub, note, fill)
}

/// `Math.round(v * 1000) / 1000` then JavaScript's number-to-string.
pub fn js_num(v: f64) -> String {
    let r = (v * 1000.0).round() / 1000.0;
    if !r.is_finite() {
        return "0".into();
    }
    if (r - r.round()).abs() < 1e-9 && r.abs() < 1e15 {
        return format!("{}", r.round() as i64);
    }
    let s = format!("{r:.3}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Options object inside a feature's params (only the keys the panel saved).
pub fn options_object(params: &Value) -> Value {
    params.get("options").cloned().unwrap_or_else(|| Value::Object(Map::new()))
}
