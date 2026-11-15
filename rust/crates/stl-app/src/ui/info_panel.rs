//! The Properties panel (1.x `startInfo`, `showInfo`): one body's size, area, volume and weight.

use egui::Rect;
use serde_json::Value;

use super::measure_panel::{self, PanelAct, Row};
use crate::theme::Tokens;
use crate::tools::fields::number::{fmt_len, num3, unit_name};

/// PLA and PETG, g/cm³ (1.x `showInfo`).
const PLA: f64 = 1.24;
const PETG: f64 = 1.27;

/// The panel's text: the body's name, and its rows. A row is marked when it should be in the
/// warning colour ("No — run Repair"). `None` when `info` is no body's record.
pub fn info_rows(info: &Value, inch: bool) -> Option<(String, Vec<(Row, bool)>)> {
    let f = |k: &str| info.get(k).and_then(Value::as_f64);
    let size = info.get("size")?.as_array()?.iter().map(|v| v.as_f64().unwrap_or(0.0)).collect::<Vec<_>>();
    let name = info.get("name").and_then(Value::as_str).unwrap_or("").to_owned();
    let solid = info.get("solid").and_then(Value::as_bool).unwrap_or(false);
    let area = f("area").unwrap_or(0.0);
    let unit = unit_name(inch);
    let mut rows = vec![
        (Row::new("Watertight", if solid { "✔ Yes" } else { "No — run Repair" }), !solid),
        (Row::new("Size (X×Y×Z)", format!("{} {unit}", size.iter().map(|v| fmt_len(*v, Some(3), inch)).collect::<Vec<_>>().join(" × "))), false),
        (Row::new("Triangles", with_commas(info.get("triangles").and_then(Value::as_u64).unwrap_or(0))), false),
        (Row::new("Surface area", if inch { format!("{} in²", num3(area / 645.16)) } else { format!("{} cm²", num3(area / 100.0)) }), false),
    ];
    if let Some(volume) = f("volume") {
        let cm3 = volume / 1000.0;
        rows.push((Row::new("Volume", if inch { format!("{} in³", num3(volume / 16387.064)) } else { format!("{} cm³", num3(cm3)) }), false));
        rows.push((Row::new("Weight (PLA)", format!("{} g", num3(cm3 * PLA))), false));
        rows.push((Row::new("Weight (PETG)", format!("{} g", num3(cm3 * PETG))), false));
        let genus = info.get("genus").and_then(Value::as_i64).unwrap_or(-1);
        if genus >= 0 {
            rows.push((Row::new("Through-holes", genus.to_string()), false));
        }
    }
    if let Some(b) = info.get("bbox").and_then(Value::as_array).filter(|b| b.len() == 2) {
        let corner = |c: &Value| c.as_array().map(|a| a.iter().map(|v| fmt_len(v.as_f64().unwrap_or(0.0), None, inch)).collect::<Vec<_>>().join(", ")).unwrap_or_default();
        rows.push((Row::new("Bounds min", corner(&b[0])), false));
        rows.push((Row::new("Bounds max", corner(&b[1])), false));
    }
    Some((name, rows))
}

/// `Number.toLocaleString()` for a count: 12,345.
fn with_commas(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// The Properties panel; `info` is the engine's answer for the body shown, if there is one.
pub fn show(ctx: &egui::Context, tk: &Tokens, ws: Rect, info: Option<&Value>, inch: bool) -> PanelAct {
    let shown = info.and_then(|i| info_rows(i, inch));
    measure_panel::frame(ctx, tk, ws, "info-panel", "Properties", "info", "Close", |ui| match &shown {
        None => measure_panel::note(ui, tk, "Click a body to see its size, volume and triangle count."),
        Some((name, rows)) => {
            measure_panel::section_label(ui, tk, name);
            for (r, warn) in rows {
                measure_panel::value_row(ui, tk, &r.label, &r.value, r.bold, if *warn { tk.warn_text } else { tk.text });
                ui.add_space(6.0);
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn cube() -> Value {
        json!({"name": "Box", "solid": true, "size": [20.0, 20.0, 20.0], "bbox": [[0, 0, 0], [20, 20, 20]], "triangles": 12, "area": 2400.0, "volume": 8000.0, "genus": 0})
    }

    fn get(rows: &[(Row, bool)], label: &str) -> String {
        rows.iter().find(|(r, _)| r.label == label).map(|(r, _)| r.value.clone()).unwrap_or_else(|| format!("(no {label} row)"))
    }

    #[test]
    fn solid_rows_in_mm() {
        let (name, rows) = info_rows(&cube(), false).unwrap();
        assert_eq!(name, "Box");
        assert_eq!(get(&rows, "Watertight"), "✔ Yes");
        assert_eq!(get(&rows, "Size (X×Y×Z)"), "20 × 20 × 20 mm");
        assert_eq!(get(&rows, "Surface area"), "24 cm²");
        assert_eq!(get(&rows, "Volume"), "8 cm³");
        assert_eq!(get(&rows, "Weight (PLA)"), "9.92 g");
        assert_eq!(get(&rows, "Weight (PETG)"), "10.16 g");
        assert_eq!(get(&rows, "Through-holes"), "0");
    }

    #[test]
    fn inch_mode_units() {
        let (_, rows) = info_rows(&cube(), true).unwrap();
        assert_eq!(get(&rows, "Size (X×Y×Z)"), "0.787 × 0.787 × 0.787 in");
        assert_eq!(get(&rows, "Surface area"), "3.72 in²");
        assert_eq!(get(&rows, "Volume"), "0.488 in³");
    }

    #[test]
    fn open_mesh_has_no_volume_or_weights() {
        let open = json!({"name": "Open", "solid": false, "size": [1.0, 2.0, 3.0], "triangles": 1234567, "area": 100.0, "volume": null, "genus": -1});
        let (_, rows) = info_rows(&open, false).unwrap();
        assert_eq!(get(&rows, "Watertight"), "No — run Repair");
        assert!(rows[0].1, "the warning colour");
        assert_eq!(get(&rows, "Triangles"), "1,234,567");
        for l in ["Volume", "Weight (PLA)", "Weight (PETG)", "Through-holes"] {
            assert!(get(&rows, l).starts_with("(no "), "{l} must be left out");
        }
    }
}
