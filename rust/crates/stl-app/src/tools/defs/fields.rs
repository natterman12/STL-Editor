//! Shared field lists (connector fields, ops targets show, …).

use super::tables::{CONNECTOR_OPTS, OPS};
use super::util::get_str;
use super::{Field, FieldKind, Values};

pub fn ops_options() -> Vec<(String, String)> {
    OPS.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()
}

pub fn connector_options() -> Vec<(String, String)> {
    CONNECTOR_OPTS
        .iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect()
}

pub fn show_targets(v: &Values) -> bool {
    get_str(v, "operation") != Some("new")
}

pub fn connector_show_true(_: &Values) -> bool {
    true
}

pub fn connector_show_both(v: &Values) -> bool {
    get_str(v, "keep") == Some("both")
}

pub fn connector_show_not_none(v: &Values) -> bool {
    get_str(v, "connector") != Some("none")
}

pub fn connector_fields_split() -> Vec<Field> {
    let mut f = vec![Field {
        key: "connector",
        label: "Connectors",
        kind: super::select_flat(connector_options()),
        help: None,
        show: Some(connector_show_both),
        placeholder: None,
    }];
    f.extend(pin_fields(
        connector_show_both_pin,
        connector_show_both_dovetail,
        connector_show_both_any,
    ));
    f.extend([
        Field {
            key: "connector_place",
            label: "Placement",
            kind: opts(&[("auto", "Auto"), ("centre", "Centre"), ("manual", "Place by hand")]),
            help: Some(
                "Auto spreads them over the cut face. Centre puts one in the middle of each piece. Place by hand: click on the cut face where you want them.",
            ),
            show: Some(connector_show_both_any),
            placeholder: None,
        },
        how_many(connector_show_both_auto),
        Field {
            key: "connector_points",
            label: "Spots",
            kind: FieldKind::CutPoints,
            help: Some(
                "Click on the cut face to add a spot. Type a distance to move one; it is measured from the cut face's left and bottom edges.",
            ),
            show: Some(connector_show_both_manual),
            placeholder: None,
        },
    ]);
    f
}

/// The size fields: pin and pocket sizes, or the dovetail's, and the clearance for either.
fn pin_fields(pin: fn(&Values) -> bool, dovetail: fn(&Values) -> bool, any: fn(&Values) -> bool) -> Vec<Field> {
    vec![
        Field {
            key: "pin_d",
            label: "Pin / pocket Ø",
            kind: FieldKind::Number { unit: Some("mm") },
            help: None,
            show: Some(pin),
            placeholder: None,
        },
        Field {
            key: "pin_len",
            label: "Pin length",
            kind: FieldKind::Number { unit: Some("mm") },
            help: None,
            show: Some(pin),
            placeholder: None,
        },
        Field {
            key: "dovetail_width",
            label: "Dovetail width",
            kind: FieldKind::Number { unit: Some("mm") },
            help: Some(
                "How wide the dovetail is where it meets the cut face. It gets wider towards its tip, which is what holds the halves together.",
            ),
            show: Some(dovetail),
            placeholder: None,
        },
        Field {
            key: "dovetail_depth",
            label: "Dovetail depth",
            kind: FieldKind::Number { unit: Some("mm") },
            help: Some("How far the dovetail reaches into the other half."),
            show: Some(dovetail),
            placeholder: None,
        },
        Field {
            key: "dovetail_slide",
            label: "Slide",
            kind: opts(&[("long", "Along the long side"), ("short", "Along the short side")]),
            help: Some(
                "The halves slide together from the side. Along the long side grips more; along the short side is a shorter slide.",
            ),
            show: Some(dovetail),
            placeholder: None,
        },
        Field {
            key: "clearance",
            label: "Clearance",
            kind: FieldKind::Number { unit: Some("mm") },
            help: None,
            show: Some(any),
            placeholder: None,
        },
    ]
}

fn how_many(show: fn(&Values) -> bool) -> Field {
    Field {
        key: "connector_count",
        label: "How many",
        kind: FieldKind::Number { unit: None },
        help: Some("Connectors on each piece of the cut face. 0 lets the app choose from the size of the face."),
        show: Some(show),
        placeholder: None,
    }
}

fn is_dovetail(v: &Values) -> bool {
    get_str(v, "connector") == Some("dovetail")
}

fn connector_show_both_any(v: &Values) -> bool {
    connector_show_both(v) && connector_show_not_none(v)
}

fn connector_show_both_dovetail(v: &Values) -> bool {
    connector_show_both(v) && is_dovetail(v)
}

fn connector_show_both_auto(v: &Values) -> bool {
    connector_show_both_any(v) && matches!(get_str(v, "connector_place"), None | Some("auto"))
}

fn connector_show_both_manual(v: &Values) -> bool {
    connector_show_both_any(v) && get_str(v, "connector_place") == Some("manual")
}

fn connector_show_both_pin(v: &Values) -> bool {
    connector_show_both_any(v) && !is_dovetail(v)
}

pub fn connector_fields_bedsplit() -> Vec<Field> {
    let mut f = vec![Field {
        key: "connector",
        label: "Connectors",
        kind: super::select_flat(connector_options()),
        help: None,
        show: Some(connector_show_true),
        placeholder: None,
    }];
    f.extend(pin_fields(
        connector_show_bedsplit_pin,
        is_dovetail,
        connector_show_not_none,
    ));
    f.push(how_many(connector_show_not_none));
    f
}

fn connector_show_bedsplit_pin(v: &Values) -> bool {
    connector_show_not_none(v) && !is_dovetail(v)
}

/// A flat (ungrouped) select from `(id, label)` pairs.
pub fn opts(list: &[(&str, &str)]) -> FieldKind {
    super::select_flat(list.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect())
}

/// The 2.0 connector settings a new Split or Fit to Bed starts with.
pub const CONNECTOR_EXTRAS: &[(&str, fn() -> serde_json::Value)] = &[
    ("connector_place", || serde_json::json!("auto")),
    ("connector_count", || serde_json::json!(0)),
    ("connector_points", || serde_json::json!([])),
    ("dovetail_width", || serde_json::json!(10)),
    ("dovetail_depth", || serde_json::json!(5)),
    ("dovetail_slide", || serde_json::json!("long")),
];

/// Fill in the 2.0 connector settings a step saved without them (one from 1.x, or one that
/// left them at their defaults).
pub fn connector_extras_fill(o: &mut serde_json::Map<String, serde_json::Value>) {
    for (k, v) in CONNECTOR_EXTRAS {
        o.entry(*k).or_insert_with(v);
    }
}

/// Leave out the 2.0 connector settings that change nothing, so a step that uses none of them
/// is saved just as 1.x saves it: the placement on Auto, How many at 0, spots unless placed by
/// hand, the dovetail's sizes unless it is a dovetail, and `separate` when off.
pub fn connector_extras_strip(m: &mut serde_json::Map<String, serde_json::Value>) {
    use serde_json::Value;
    let s = |m: &serde_json::Map<String, Value>, k: &str| m.get(k).and_then(Value::as_str).map(str::to_owned);
    let place = s(m, "connector_place");
    if matches!(place.as_deref(), None | Some("auto")) {
        m.remove("connector_place");
    }
    if place.as_deref() != Some("manual") {
        m.remove("connector_points");
    }
    if place.as_deref().is_some_and(|p| p != "auto")
        || matches!(m.get("connector_count"), Some(Value::Number(n)) if n.as_f64() == Some(0.0))
        || matches!(m.get("connector_count"), Some(Value::Null))
    {
        m.remove("connector_count");
    }
    if s(m, "connector").as_deref() != Some("dovetail") {
        for k in ["dovetail_width", "dovetail_depth", "dovetail_slide"] {
            m.remove(k);
        }
    } else if s(m, "dovetail_slide").as_deref() == Some("long") {
        m.remove("dovetail_slide");
    }
    if matches!(m.get("separate"), Some(Value::Bool(false)) | Some(Value::Null)) {
        m.remove("separate");
    }
}
