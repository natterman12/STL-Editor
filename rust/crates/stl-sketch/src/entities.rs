//! Sketch entities in the exact 1.x JSON shape (`web/js/constraints.js`, `stleditor/sketch.py`).
//! Unknown keys are kept in `extra`, so a round trip is lossless.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub type P2 = [f64; 2];

const D2R: f64 = std::f64::consts::PI / 180.0;

fn default_sides() -> f64 {
    6.0
}

/// The most sides a polygon is drawn with; a bigger `n` is treated as this, so a bad value in a
/// file can't make the engine run out of memory.
pub const MAX_POLYGON_SIDES: i64 = 10_000;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Line {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub p1: P2,
    pub p2: P2,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub construction: Option<bool>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Arc {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub cx: f64,
    pub cy: f64,
    pub r: f64,
    /// Start and end angle in degrees.
    pub a0: f64,
    pub a1: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub construction: Option<bool>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Circle {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub cx: f64,
    pub cy: f64,
    pub r: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub construction: Option<bool>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub angle: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fillet: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub construction: Option<bool>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Polygon {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub cx: f64,
    pub cy: f64,
    pub r: f64,
    /// Number of sides; a polygon without it has 6, as in 1.x.
    #[serde(default = "default_sides")]
    pub n: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub angle: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub construction: Option<bool>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Polyline {
    /// Whether the polyline is closed (a missing flag is `false`, as in 1.x).
    pub fn is_closed(&self) -> bool {
        self.closed.unwrap_or(false)
    }
}

impl Polygon {
    /// The side count as a whole number, kept between 0 and [`MAX_POLYGON_SIDES`] (a missing,
    /// negative or not-a-number `n` gives 0, which draws nothing).
    pub fn sides(&self) -> i64 {
        (self.n as i64).clamp(0, MAX_POLYGON_SIDES)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Slot {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
    pub r: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub construction: Option<bool>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Polyline {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub pts: Vec<P2>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closed: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub construction: Option<bool>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Point {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub x: f64,
    pub y: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub construction: Option<bool>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// One sketch entity; `"type"` selects the variant, as in 1.x.
///
/// The JSON round trip is lossless: `construction` and a polyline's `closed` are `Option`s, so a
/// missing key stays missing and an explicit `"construction": false` stays.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Entity {
    Line(Line),
    Arc(Arc),
    Circle(Circle),
    Rect(Rect),
    Polygon(Polygon),
    Slot(Slot),
    Polyline(Polyline),
    Point(Point),
}

/// The 1.x `"type"` strings the port knows.
pub const ENTITY_KINDS: [&str; 8] = ["line", "arc", "circle", "rect", "polygon", "slot", "polyline", "point"];

/// Read a JSON list of entities the way the JS does: an entry with a type this port doesn't know
/// is ignored instead of failing the whole list. A known type with bad fields is still an error.
pub fn parse_entities(list: &[Value]) -> Result<Vec<Entity>, String> {
    let mut out = Vec::new();
    for v in list {
        let known = v.get("type").and_then(Value::as_str).is_some_and(|t| ENTITY_KINDS.contains(&t));
        if known {
            out.push(serde_json::from_value::<Entity>(v.clone()).map_err(|e| e.to_string())?);
        }
    }
    Ok(out)
}

/// `#[serde(deserialize_with = "stl_sketch::entities::lenient_entities")]` for a `Vec<Entity>`
/// field: skips unknown entity types (see [`parse_entities`]).
pub fn lenient_entities<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<Entity>, D::Error> {
    let list = Vec::<Value>::deserialize(d)?;
    parse_entities(&list).map_err(serde::de::Error::custom)
}

impl Entity {
    pub fn id(&self) -> Option<&str> {
        match self {
            Entity::Line(e) => e.id.as_deref(),
            Entity::Arc(e) => e.id.as_deref(),
            Entity::Circle(e) => e.id.as_deref(),
            Entity::Rect(e) => e.id.as_deref(),
            Entity::Polygon(e) => e.id.as_deref(),
            Entity::Slot(e) => e.id.as_deref(),
            Entity::Polyline(e) => e.id.as_deref(),
            Entity::Point(e) => e.id.as_deref(),
        }
    }

    /// Whether the entity is construction geometry (a missing flag is `false`, as in 1.x).
    pub fn construction(&self) -> bool {
        match self {
            Entity::Line(e) => e.construction,
            Entity::Arc(e) => e.construction,
            Entity::Circle(e) => e.construction,
            Entity::Rect(e) => e.construction,
            Entity::Polygon(e) => e.construction,
            Entity::Slot(e) => e.construction,
            Entity::Polyline(e) => e.construction,
            Entity::Point(e) => e.construction,
        }
        .unwrap_or(false)
    }

    /// The 1.x `"type"` string.
    pub fn kind(&self) -> &'static str {
        match self {
            Entity::Line(_) => "line",
            Entity::Arc(_) => "arc",
            Entity::Circle(_) => "circle",
            Entity::Rect(_) => "rect",
            Entity::Polygon(_) => "polygon",
            Entity::Slot(_) => "slot",
            Entity::Polyline(_) => "polyline",
            Entity::Point(_) => "point",
        }
    }
}

/// Port of `pointKeys` (constraints.js): the named points an entity offers.
pub fn point_keys(e: &Entity) -> Vec<String> {
    let s = |v: &[&str]| v.iter().map(|k| k.to_string()).collect();
    match e {
        Entity::Line(_) => s(&["p1", "p2", "m"]),
        Entity::Arc(_) => s(&["s", "e", "c"]),
        Entity::Circle(_) => s(&["c"]),
        Entity::Rect(_) => s(&["c0", "c1", "c2", "c3", "c"]),
        Entity::Polygon(_) => s(&["c"]),
        Entity::Slot(_) => s(&["s", "e"]),
        Entity::Polyline(p) => (0..p.pts.len()).map(|i| format!("v{i}")).collect(),
        Entity::Point(_) => s(&["p"]),
    }
}

/// Port of `point` (constraints.js): where a named point of an entity is.
/// Mirrors the JS fallbacks: a line's unknown key is its midpoint, a slot's unknown key its end;
/// an unknown rect or polyline key gives `None` (JS returns `undefined`).
pub fn point(e: &Entity, k: &str) -> Option<P2> {
    match e {
        Entity::Line(l) => Some(match k {
            "p1" => l.p1,
            "p2" => l.p2,
            _ => [(l.p1[0] + l.p2[0]) / 2.0, (l.p1[1] + l.p2[1]) / 2.0],
        }),
        Entity::Arc(a) => {
            if k == "c" {
                return Some([a.cx, a.cy]);
            }
            let ang = (if k == "s" { a.a0 } else { a.a1 }) * D2R;
            Some([a.cx + a.r * ang.cos(), a.cy + a.r * ang.sin()])
        }
        Entity::Circle(c) => Some([c.cx, c.cy]),
        Entity::Polygon(p) => Some([p.cx, p.cy]),
        Entity::Rect(r) => match k {
            "c0" => Some([r.x, r.y]),
            "c1" => Some([r.x + r.w, r.y]),
            "c2" => Some([r.x + r.w, r.y + r.h]),
            "c3" => Some([r.x, r.y + r.h]),
            "c" => Some([r.x + r.w / 2.0, r.y + r.h / 2.0]),
            _ => None,
        },
        Entity::Slot(s) => Some(if k == "s" { [s.x1, s.y1] } else { [s.x2, s.y2] }),
        Entity::Polyline(p) => k.get(1..).and_then(|n| n.parse::<usize>().ok()).and_then(|i| p.pts.get(i).copied()),
        Entity::Point(p) => Some([p.x, p.y]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    fn parity_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/parity")
    }

    /// 0 and 0.0 are the same JSON number to JS; make them compare equal.
    fn norm(v: &Value) -> Value {
        match v {
            Value::Number(n) => serde_json::json!(n.as_f64().unwrap()),
            Value::Array(a) => Value::Array(a.iter().map(norm).collect()),
            Value::Object(o) => Value::Object(o.iter().map(|(k, v)| (k.clone(), norm(v))).collect()),
            other => other.clone(),
        }
    }

    const KINDS: [&str; 8] = ["line", "arc", "circle", "rect", "polygon", "slot", "polyline", "point"];

    fn collect(v: &Value, out: &mut Vec<Value>) {
        match v {
            Value::Object(o) => {
                if o.get("type").and_then(Value::as_str).is_some_and(|t| KINDS.contains(&t)) {
                    out.push(v.clone());
                }
                o.values().for_each(|c| collect(c, out));
            }
            Value::Array(a) => a.iter().for_each(|c| collect(c, out)),
            _ => {}
        }
    }

    fn json_files(dir: &Path, out: &mut Vec<PathBuf>) {
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                json_files(&p, out);
            } else if p.extension().is_some_and(|x| x == "json") {
                out.push(p);
            }
        }
    }

    #[test]
    fn every_case_entity_round_trips() {
        let mut files = Vec::new();
        for d in ["sketch", "solver", "import2d"] {
            json_files(&parity_dir().join(d), &mut files);
        }
        let single = parity_dir().join("features/sketch.json");
        files.push(single);
        let mut count = 0;
        for f in files {
            let text = std::fs::read_to_string(&f).unwrap();
            let v: Value = serde_json::from_str(&text).unwrap();
            let mut ents = Vec::new();
            collect(&v, &mut ents);
            for e in ents {
                let parsed: Entity =
                    serde_json::from_value(e.clone()).unwrap_or_else(|err| panic!("{}: {err}: {e}", f.display()));
                let back = serde_json::to_value(&parsed).unwrap();
                assert_eq!(norm(&back), norm(&e), "{}", f.display());
                count += 1;
            }
        }
        assert!(count > 20, "only {count} entities found");
    }

    #[test]
    fn unknown_keys_survive() {
        let j = serde_json::json!({"id":"a","type":"line","p1":[0,0],"p2":[1,1],"ref":"axis","construction":true});
        let e: Entity = serde_json::from_value(j.clone()).unwrap();
        assert_eq!(norm(&serde_json::to_value(&e).unwrap()), norm(&j));
        assert!(e.construction());
    }

    #[test]
    fn round_trip_is_lossless_for_edge_cases() {
        // a polyline without `closed` stays without it; with it, it stays
        for j in [
            serde_json::json!({"type":"polyline","id":"p","pts":[[0,0],[1,0]]}),
            serde_json::json!({"type":"polyline","id":"p","pts":[[0,0],[1,0]],"closed":false}),
            serde_json::json!({"type":"polyline","id":"p","pts":[[0,0],[1,0]],"closed":true}),
            // an explicit construction:false stays
            serde_json::json!({"type":"line","id":"l","p1":[0,0],"p2":[1,1],"construction":false}),
            serde_json::json!({"type":"circle","id":"c","cx":0,"cy":0,"r":1,"construction":false}),
        ] {
            let e: Entity = serde_json::from_value(j.clone()).unwrap();
            assert_eq!(norm(&serde_json::to_value(&e).unwrap()), norm(&j), "{j}");
        }
        let e: Entity = serde_json::from_value(serde_json::json!({"type":"polyline","pts":[[0,0]]})).unwrap();
        if let Entity::Polyline(p) = &e {
            assert!(!p.is_closed());
        }
        // an unknown entity type is ignored, not an error for the whole list
        let list = vec![
            serde_json::json!({"type":"spline","id":"s","pts":[]}),
            serde_json::json!({"type":"point","id":"a","x":1,"y":2}),
        ];
        let got = parse_entities(&list).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].id(), Some("a"));
        #[derive(Deserialize)]
        struct Doc {
            #[serde(deserialize_with = "lenient_entities")]
            entities: Vec<Entity>,
        }
        let d: Doc = serde_json::from_value(serde_json::json!({"entities": list})).unwrap();
        assert_eq!(d.entities.len(), 1);
        // a known type with bad fields is still an error
        assert!(serde_json::from_value::<Entity>(serde_json::json!({"type":"line","p1":[0,0]})).is_err());
    }

    #[test]
    fn points() {
        let e: Entity = serde_json::from_value(serde_json::json!({"type":"rect","x":1,"y":2,"w":4,"h":6})).unwrap();
        assert_eq!(point(&e, "c"), Some([3.0, 5.0]));
        assert_eq!(point_keys(&e).len(), 5);
        let a: Entity =
            serde_json::from_value(serde_json::json!({"type":"arc","cx":0,"cy":0,"r":2,"a0":90,"a1":0})).unwrap();
        let s = point(&a, "s").unwrap();
        assert!(s[0].abs() < 1e-12 && (s[1] - 2.0).abs() < 1e-12);
    }
}
