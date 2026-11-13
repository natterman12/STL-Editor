//! Shared field lists (connector fields, ops targets show, …).

use super::{Field, FieldKind, Values};
use super::tables::{CONNECTOR_OPTS, OPS};
use super::util::get_str;

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
    vec![
        Field {
            key: "connector",
            label: "Connectors",
            kind: super::select_flat(connector_options()),
            help: None,
            show: Some(connector_show_both),
            placeholder: None,
        },
        Field {
            key: "pin_d",
            label: "Pin / pocket Ø",
            kind: FieldKind::Number { unit: Some("mm") },
            help: None,
            show: Some(connector_show_both_pin),
            placeholder: None,
        },
        Field {
            key: "pin_len",
            label: "Pin length",
            kind: FieldKind::Number { unit: Some("mm") },
            help: None,
            show: Some(connector_show_both_pin),
            placeholder: None,
        },
        Field {
            key: "clearance",
            label: "Clearance",
            kind: FieldKind::Number { unit: Some("mm") },
            help: None,
            show: Some(connector_show_both_pin),
            placeholder: None,
        },
    ]
}

fn connector_show_both_pin(v: &Values) -> bool {
    connector_show_both(v) && connector_show_not_none(v)
}

pub fn connector_fields_bedsplit() -> Vec<Field> {
    vec![
        Field {
            key: "connector",
            label: "Connectors",
            kind: super::select_flat(connector_options()),
            help: None,
            show: Some(connector_show_true),
            placeholder: None,
        },
        Field {
            key: "pin_d",
            label: "Pin / pocket Ø",
            kind: FieldKind::Number { unit: Some("mm") },
            help: None,
            show: Some(connector_show_bedsplit_pin),
            placeholder: None,
        },
        Field {
            key: "pin_len",
            label: "Pin length",
            kind: FieldKind::Number { unit: Some("mm") },
            help: None,
            show: Some(connector_show_bedsplit_pin),
            placeholder: None,
        },
        Field {
            key: "clearance",
            label: "Clearance",
            kind: FieldKind::Number { unit: Some("mm") },
            help: None,
            show: Some(connector_show_bedsplit_pin),
            placeholder: None,
        },
    ]
}

fn connector_show_bedsplit_pin(v: &Values) -> bool {
    connector_show_not_none(v)
}

/// A flat (ungrouped) select from `(id, label)` pairs.
pub fn opts(list: &[(&str, &str)]) -> FieldKind {
    super::select_flat(list.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect())
}
