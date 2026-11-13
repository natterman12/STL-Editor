//! Tables ported from `web/js/tools.js`.

use serde_json::{json, Value};

use super::util::r2;

pub const OPS: &[(&str, &str)] = &[
    ("new", "New body"),
    ("join", "Join"),
    ("cut", "Cut"),
    ("intersect", "Intersect"),
];

pub const AXES: &[(&str, &str)] = &[
    ("x", "X"),
    ("y", "Y"),
    ("z", "Z"),
];

pub fn axv(name: &str) -> [f64; 3] {
    match name {
        "x" => [1.0, 0.0, 0.0],
        "y" => [0.0, 1.0, 0.0],
        _ => [0.0, 0.0, 1.0],
    }
}

pub const REPEATABLE: &[&str] = &["extrude", "revolve", "hole", "primitive"];

pub const RESIZE_SIZES: &[(&str, &str, Option<f64>)] = &[
    ("custom", "Custom", None),
    ("m16", "M1.6 screw clearance (1.8)", Some(1.8)),
    ("m2", "M2 screw clearance (2.4)", Some(2.4)),
    ("m25", "M2.5 screw clearance (2.9)", Some(2.9)),
    ("m3", "M3 screw clearance (3.4)", Some(3.4)),
    ("m4", "M4 screw clearance (4.5)", Some(4.5)),
    ("m5", "M5 screw clearance (5.5)", Some(5.5)),
    ("m6", "M6 screw clearance (6.6)", Some(6.6)),
    ("m2tap", "M2 self-tapping into plastic (1.7)", Some(1.7)),
    ("m25tap", "M2.5 self-tapping into plastic (2.1)", Some(2.1)),
    ("m3tap", "M3 self-tapping into plastic (2.6)", Some(2.6)),
    ("m2ins", "M2 heat-set insert (3.2)", Some(3.2)),
    ("m3ins", "M3 heat-set insert (4.0)", Some(4_f64)),
    ("m4ins", "M4 heat-set insert (5.6)", Some(5.6)),
    ("i4", "#4 screw clearance (3.26)", Some(3.26)),
    ("i6", "#6 screw clearance (3.8)", Some(3.8)),
    ("i8", "#8 screw clearance (4.5)", Some(4.5)),
    ("i10", "#10 screw clearance (5.11)", Some(5.11)),
    ("i14", "1/4\" screw clearance (6.76)", Some(6.76)),
    ("i4ins", "#4-40 heat-set insert (4.09)", Some(4.09)),
    ("i6ins", "#6-32 heat-set insert (5.11)", Some(5.11)),
    ("mag6", "6 mm magnet (6.2)", Some(6.2)),
    ("mag8", "8 mm magnet (8.2)", Some(8.2)),
    ("mag10", "10 mm magnet (10.2)", Some(10.2)),
    ("fil", "1.75 mm filament pin (1.9)", Some(1.9)),
    ("b608", "608 bearing seat (22.1)", Some(22.1)),
];

pub const PRINTER_OPTS: &[(&str, &str)] = &[
    ("bambu_x1", "Bambu X1C / P1S / P1P / A1 (256)"),
    ("bambu_a1mini", "Bambu A1 mini (180)"),
    ("bambu_h2d", "Bambu H2D (325×320)"),
    ("prusa_mk4", "Prusa MK4 (250×210)"),
    ("prusa_xl", "Prusa XL (360)"),
    ("voron_350", "Voron 2.4 350"),
    ("voron_300", "Voron 2.4 300"),
    ("ender3", "Ender 3 (220)"),
    ("custom", "Custom size"),
];

pub const CONNECTOR_OPTS: &[(&str, &str)] = &[
    ("pins", "Printed pins"),
    ("dowels", "Holes + loose pins"),
    ("filament", "Filament pins (1.75)"),
    ("magnets", "Magnets 6×3"),
    ("none", "None"),
];

pub const CONNECTOR_DEF: &[(&str, f64, f64)] = &[
    ("pins", 5_f64, 10_f64),
    ("dowels", 5_f64, 12_f64),
    ("filament", 1.75, 10_f64),
    ("magnets", 6_f64, 3_f64),
    ("none", 5_f64, 10_f64),
];

pub const THREAD_SIZES: &[(&str, f64, f64, &str, &str)] = &[
    ("M1.6", 1.6, 0.35, "M1.6 × 0.35", "Metric (ISO coarse)"),
    ("M2", 2_f64, 0.4, "M2 × 0.4", "Metric (ISO coarse)"),
    ("M2.5", 2.5, 0.45, "M2.5 × 0.45", "Metric (ISO coarse)"),
    ("M3", 3_f64, 0.5, "M3 × 0.5", "Metric (ISO coarse)"),
    ("M4", 4_f64, 0.7, "M4 × 0.7", "Metric (ISO coarse)"),
    ("M5", 5_f64, 0.8, "M5 × 0.8", "Metric (ISO coarse)"),
    ("M6", 6_f64, 1_f64, "M6 × 1", "Metric (ISO coarse)"),
    ("M7", 7_f64, 1_f64, "M7 × 1", "Metric (ISO coarse)"),
    ("M8", 8_f64, 1.25, "M8 × 1.25", "Metric (ISO coarse)"),
    ("M10", 10_f64, 1.5, "M10 × 1.5", "Metric (ISO coarse)"),
    ("M12", 12_f64, 1.75, "M12 × 1.75", "Metric (ISO coarse)"),
    ("M14", 14_f64, 2_f64, "M14 × 2", "Metric (ISO coarse)"),
    ("M16", 16_f64, 2_f64, "M16 × 2", "Metric (ISO coarse)"),
    ("M20", 20_f64, 2.5, "M20 × 2.5", "Metric (ISO coarse)"),
    ("M24", 24_f64, 3_f64, "M24 × 3", "Metric (ISO coarse)"),
    ("M30", 30_f64, 3.5, "M30 × 3.5", "Metric (ISO coarse)"),
    ("#0-80", 1.524, 0.3175, "#0-80 UNF", "Inch, fine (UNF)"),
    ("#1-64", 1.8542, 0.3969, "#1-64 UNC", "Inch, coarse (UNC)"),
    ("#2-56", 2.1844, 0.4536, "#2-56 UNC", "Inch, coarse (UNC)"),
    ("#4-40", 2.8448, 0.635, "#4-40 UNC", "Inch, coarse (UNC)"),
    ("#6-32", 3.5052, 0.7938, "#6-32 UNC", "Inch, coarse (UNC)"),
    ("#8-32", 4.1656, 0.7938, "#8-32 UNC", "Inch, coarse (UNC)"),
    ("#10-24", 4.826, 1.0583, "#10-24 UNC", "Inch, coarse (UNC)"),
    (r#"1/4"-20"#, 6.35, 1.27, r#"1/4"-20 UNC"#, "Inch, coarse (UNC)"),
    (r#"5/16"-18"#, 7.9375, 1.4111, r#"5/16"-18 UNC"#, "Inch, coarse (UNC)"),
    (r#"3/8"-16"#, 9.525, 1.5875, r#"3/8"-16 UNC"#, "Inch, coarse (UNC)"),
    (r#"7/16"-14"#, 11.1125, 1.8143, r#"7/16"-14 UNC"#, "Inch, coarse (UNC)"),
    (r#"1/2"-13"#, 12.7, 1.9538, r#"1/2"-13 UNC"#, "Inch, coarse (UNC)"),
    (r#"5/8"-11"#, 15.875, 2.3091, r#"5/8"-11 UNC"#, "Inch, coarse (UNC)"),
    (r#"3/4"-10"#, 19.05, 2.54, r#"3/4"-10 UNC"#, "Inch, coarse (UNC)"),
    ("#10-32", 4.826, 0.7938, "#10-32 UNF", "Inch, fine (UNF)"),
    (r#"1/4"-28"#, 6.35, 0.9071, r#"1/4"-28 UNF"#, "Inch, fine (UNF)"),
    (r#"5/16"-24"#, 7.9375, 1.0583, r#"5/16"-24 UNF"#, "Inch, fine (UNF)"),
    (r#"3/8"-24"#, 9.525, 1.0583, r#"3/8"-24 UNF"#, "Inch, fine (UNF)"),
    (r#"1/2"-20"#, 12.7, 1.27, r#"1/2"-20 UNF"#, "Inch, fine (UNF)"),
    ("custom", 0_f64, 0_f64, "Custom", ""),
];

pub fn hole_preset_options() -> Vec<(String, String, String)> {
    vec![
        ("custom".into(), "Custom".into(), "".into()),
        ("ins_M1.6".into(), "Heat-set insert M1.6".into(), "Heat-set inserts (metric)".into()),
        ("ins_M2".into(), "Heat-set insert M2".into(), "Heat-set inserts (metric)".into()),
        ("ins_M2.5".into(), "Heat-set insert M2.5".into(), "Heat-set inserts (metric)".into()),
        ("ins_M3".into(), "Heat-set insert M3".into(), "Heat-set inserts (metric)".into()),
        ("ins_M4".into(), "Heat-set insert M4".into(), "Heat-set inserts (metric)".into()),
        ("ins_M5".into(), "Heat-set insert M5".into(), "Heat-set inserts (metric)".into()),
        ("clr_M1.6".into(), "Screw clearance M1.6".into(), "Screws (metric)".into()),
        ("clr_M2".into(), "Screw clearance M2".into(), "Screws (metric)".into()),
        ("clr_M2.5".into(), "Screw clearance M2.5".into(), "Screws (metric)".into()),
        ("clr_M3".into(), "Screw clearance M3".into(), "Screws (metric)".into()),
        ("clr_M4".into(), "Screw clearance M4".into(), "Screws (metric)".into()),
        ("clr_M5".into(), "Screw clearance M5".into(), "Screws (metric)".into()),
        ("clr_M6".into(), "Screw clearance M6".into(), "Screws (metric)".into()),
        ("clr_M8".into(), "Screw clearance M8".into(), "Screws (metric)".into()),
        ("cb_M2".into(), "Socket head M2 (counterbore)".into(), "Screws (metric)".into()),
        ("cb_M2.5".into(), "Socket head M2.5 (counterbore)".into(), "Screws (metric)".into()),
        ("cb_M3".into(), "Socket head M3 (counterbore)".into(), "Screws (metric)".into()),
        ("cb_M4".into(), "Socket head M4 (counterbore)".into(), "Screws (metric)".into()),
        ("cb_M5".into(), "Socket head M5 (counterbore)".into(), "Screws (metric)".into()),
        ("cb_M6".into(), "Socket head M6 (counterbore)".into(), "Screws (metric)".into()),
        ("nut_M2".into(), "Hex nut trap M2".into(), "Screws (metric)".into()),
        ("nut_M2.5".into(), "Hex nut trap M2.5".into(), "Screws (metric)".into()),
        ("nut_M3".into(), "Hex nut trap M3".into(), "Screws (metric)".into()),
        ("nut_M4".into(), "Hex nut trap M4".into(), "Screws (metric)".into()),
        ("nut_M5".into(), "Hex nut trap M5".into(), "Screws (metric)".into()),
        ("nut_M6".into(), "Hex nut trap M6".into(), "Screws (metric)".into()),
        ("tap_M1.6".into(), "Self-tapping M1.6 (into plastic)".into(), "Self-tapping into plastic".into()),
        ("tap_M2".into(), "Self-tapping M2 (into plastic)".into(), "Self-tapping into plastic".into()),
        ("tap_M2.5".into(), "Self-tapping M2.5 (into plastic)".into(), "Self-tapping into plastic".into()),
        ("tap_M3".into(), "Self-tapping M3 (into plastic)".into(), "Self-tapping into plastic".into()),
        ("tap_M4".into(), "Self-tapping M4 (into plastic)".into(), "Self-tapping into plastic".into()),
        ("tap_M5".into(), "Self-tapping M5 (into plastic)".into(), "Self-tapping into plastic".into()),
        ("ins_#2-56".into(), "Heat-set insert #2-56".into(), "Heat-set inserts (inch)".into()),
        ("ins_#4-40".into(), "Heat-set insert #4-40".into(), "Heat-set inserts (inch)".into()),
        ("ins_#6-32".into(), "Heat-set insert #6-32".into(), "Heat-set inserts (inch)".into()),
        ("ins_#8-32".into(), "Heat-set insert #8-32".into(), "Heat-set inserts (inch)".into()),
        ("ins_#10-32".into(), "Heat-set insert #10-32".into(), "Heat-set inserts (inch)".into()),
        ("ins_1/4\"-20".into(), "Heat-set insert 1/4\"-20".into(), "Heat-set inserts (inch)".into()),
        ("clr_#2".into(), "Screw clearance #2".into(), "Screws (inch)".into()),
        ("clr_#4".into(), "Screw clearance #4".into(), "Screws (inch)".into()),
        ("clr_#6".into(), "Screw clearance #6".into(), "Screws (inch)".into()),
        ("clr_#8".into(), "Screw clearance #8".into(), "Screws (inch)".into()),
        ("clr_#10".into(), "Screw clearance #10".into(), "Screws (inch)".into()),
        ("clr_1/4\"".into(), "Screw clearance 1/4\"".into(), "Screws (inch)".into()),
        ("clr_5/16\"".into(), "Screw clearance 5/16\"".into(), "Screws (inch)".into()),
        ("clr_3/8\"".into(), "Screw clearance 3/8\"".into(), "Screws (inch)".into()),
        ("cb_#4".into(), "Socket head #4 (counterbore)".into(), "Screws (inch)".into()),
        ("cb_#6".into(), "Socket head #6 (counterbore)".into(), "Screws (inch)".into()),
        ("cb_#8".into(), "Socket head #8 (counterbore)".into(), "Screws (inch)".into()),
        ("cb_#10".into(), "Socket head #10 (counterbore)".into(), "Screws (inch)".into()),
        ("cb_1/4\"".into(), "Socket head 1/4\" (counterbore)".into(), "Screws (inch)".into()),
        ("nut_#4".into(), "Hex nut trap #4".into(), "Screws (inch)".into()),
        ("nut_#6".into(), "Hex nut trap #6".into(), "Screws (inch)".into()),
        ("nut_#8".into(), "Hex nut trap #8".into(), "Screws (inch)".into()),
        ("nut_#10".into(), "Hex nut trap #10".into(), "Screws (inch)".into()),
        ("nut_1/4\"".into(), "Hex nut trap 1/4\"".into(), "Screws (inch)".into()),
        ("mag_5_2_mm".into(), "Magnet 5 × 2 mm".into(), "Magnet pockets".into()),
        ("mag_6_2_mm".into(), "Magnet 6 × 2 mm".into(), "Magnet pockets".into()),
        ("mag_6_3_mm".into(), "Magnet 6 × 3 mm".into(), "Magnet pockets".into()),
        ("mag_8_2_mm".into(), "Magnet 8 × 2 mm".into(), "Magnet pockets".into()),
        ("mag_8_3_mm".into(), "Magnet 8 × 3 mm".into(), "Magnet pockets".into()),
        ("mag_10_2_mm".into(), "Magnet 10 × 2 mm".into(), "Magnet pockets".into()),
        ("mag_10_3_mm".into(), "Magnet 10 × 3 mm".into(), "Magnet pockets".into()),
        ("mag_12_3_mm".into(), "Magnet 12 × 3 mm".into(), "Magnet pockets".into()),
        ("mag_15_3_mm".into(), "Magnet 15 × 3 mm".into(), "Magnet pockets".into()),
        ("mag_1_8_1_16_".into(), "Magnet 1/8\" × 1/16\"".into(), "Magnet pockets".into()),
        ("mag_1_4_1_8_".into(), "Magnet 1/4\" × 1/8\"".into(), "Magnet pockets".into()),
        ("mag_3_8_1_8_".into(), "Magnet 3/8\" × 1/8\"".into(), "Magnet pockets".into()),
        ("mag_1_2_1_8_".into(), "Magnet 1/2\" × 1/8\"".into(), "Magnet pockets".into()),
    ]
}

/// How a hole preset sizes the hole: the rules of `insert` / `clear` / `tap` / `cbore` /
/// `nut` / `magnet` in tools.js, as functions of the print tolerance.
enum HoleApply {
    Insert { d: f64, depth: f64 },
    Clear { d: f64 },
    Tap { d: f64 },
    Cbore { d: f64, head_d: f64, head_h: f64 },
    Nut { d: f64, af: f64, h: f64 },
    Magnet { dia: f64, h: f64 },
}

const IN: f64 = 25.4;

type Rules = Vec<(String, HoleApply)>;

fn hole_preset_rules() -> Rules {
    let mut v: Rules = Vec::new();
    let ins = |v: &mut Rules, m: &str, d: f64, len: f64| {
        v.push((format!("ins_{m}"), HoleApply::Insert { d, depth: r2(len + 1.0) }));
    };
    let clr = |v: &mut Rules, m: &str, d: f64| v.push((format!("clr_{m}"), HoleApply::Clear { d }));
    let tap = |v: &mut Rules, m: &str, d: f64| v.push((format!("tap_{m}"), HoleApply::Tap { d }));
    let cb = |v: &mut Rules, m: &str, d: f64, head_d: f64, head_h: f64| {
        v.push((format!("cb_{m}"), HoleApply::Cbore { d, head_d, head_h }));
    };
    let nut = |v: &mut Rules, m: &str, d: f64, af: f64, h: f64| {
        v.push((format!("nut_{m}"), HoleApply::Nut { d, af, h }));
    };
    let mag = |v: &mut Rules, id: &str, dia: f64, h: f64| {
        v.push((id.to_string(), HoleApply::Magnet { dia, h }));
    };
    ins(&mut v, "M1.6", 2.4, 2.5);
    ins(&mut v, "M2", 3.2, 3.0);
    ins(&mut v, "M2.5", 3.6, 4.0);
    ins(&mut v, "M3", 4.0, 5.7);
    ins(&mut v, "M4", 5.6, 8.1);
    ins(&mut v, "M5", 6.4, 9.5);
    for (m, d) in [("M1.6", 1.8), ("M2", 2.4), ("M2.5", 2.9), ("M3", 3.4), ("M4", 4.5), ("M5", 5.5), ("M6", 6.6), ("M8", 9.0)] {
        clr(&mut v, m, d);
    }
    cb(&mut v, "M2", 2.4, 3.8, 2.0);
    cb(&mut v, "M2.5", 2.9, 4.5, 2.5);
    cb(&mut v, "M3", 3.4, 5.5, 3.0);
    cb(&mut v, "M4", 4.5, 7.0, 4.0);
    cb(&mut v, "M5", 5.5, 8.5, 5.0);
    cb(&mut v, "M6", 6.6, 10.0, 6.0);
    nut(&mut v, "M2", 2.4, 4.0, 1.6);
    nut(&mut v, "M2.5", 2.9, 5.0, 2.0);
    nut(&mut v, "M3", 3.4, 5.5, 2.4);
    nut(&mut v, "M4", 4.5, 7.0, 3.2);
    nut(&mut v, "M5", 5.5, 8.0, 4.0);
    nut(&mut v, "M6", 6.6, 10.0, 5.0);
    for (m, d) in [("M1.6", 1.3), ("M2", 1.7), ("M2.5", 2.1), ("M3", 2.6), ("M4", 3.5), ("M5", 4.3)] {
        tap(&mut v, m, d);
    }
    ins(&mut v, "#2-56", r2(0.125 * IN), r2(0.115 * IN));
    ins(&mut v, "#4-40", r2(0.161 * IN), r2(0.135 * IN));
    ins(&mut v, "#6-32", r2(0.201 * IN), r2(0.150 * IN));
    ins(&mut v, "#8-32", r2(0.228 * IN), r2(0.188 * IN));
    ins(&mut v, "#10-32", r2(0.250 * IN), r2(0.250 * IN));
    ins(&mut v, "1/4\"-20", r2(0.348 * IN), r2(0.300 * IN));
    for (m, d) in [
        ("#2", 0.102),
        ("#4", 0.1285),
        ("#6", 0.1495),
        ("#8", 0.177),
        ("#10", 0.201),
        ("1/4\"", 0.266),
        ("5/16\"", 0.332),
        ("3/8\"", 0.397),
    ] {
        clr(&mut v, m, r2(d * IN));
    }
    cb(&mut v, "#4", r2(0.1285 * IN), r2(0.183 * IN), r2(0.112 * IN));
    cb(&mut v, "#6", r2(0.1495 * IN), r2(0.226 * IN), r2(0.138 * IN));
    cb(&mut v, "#8", r2(0.177 * IN), r2(0.270 * IN), r2(0.164 * IN));
    cb(&mut v, "#10", r2(0.201 * IN), r2(0.312 * IN), r2(0.190 * IN));
    cb(&mut v, "1/4\"", r2(0.266 * IN), r2(0.375 * IN), r2(0.250 * IN));
    nut(&mut v, "#4", r2(0.1285 * IN), r2(0.250 * IN), r2(0.094 * IN));
    nut(&mut v, "#6", r2(0.1495 * IN), r2(0.3125 * IN), r2(0.109 * IN));
    nut(&mut v, "#8", r2(0.177 * IN), r2(0.344 * IN), r2(0.125 * IN));
    nut(&mut v, "#10", r2(0.201 * IN), r2(0.375 * IN), r2(0.125 * IN));
    nut(&mut v, "1/4\"", r2(0.266 * IN), r2(0.4375 * IN), r2(0.219 * IN));
    for (id, dia, h) in [
        ("mag_5_2_mm", 5.0, 2.0),
        ("mag_6_2_mm", 6.0, 2.0),
        ("mag_6_3_mm", 6.0, 3.0),
        ("mag_8_2_mm", 8.0, 2.0),
        ("mag_8_3_mm", 8.0, 3.0),
        ("mag_10_2_mm", 10.0, 2.0),
        ("mag_10_3_mm", 10.0, 3.0),
        ("mag_12_3_mm", 12.0, 3.0),
        ("mag_15_3_mm", 15.0, 3.0),
    ] {
        mag(&mut v, id, dia, h);
    }
    mag(&mut v, "mag_1_8_1_16_", r2(0.125 * IN), r2(0.0625 * IN));
    mag(&mut v, "mag_1_4_1_8_", r2(0.25 * IN), r2(0.125 * IN));
    mag(&mut v, "mag_3_8_1_8_", r2(0.375 * IN), r2(0.125 * IN));
    mag(&mut v, "mag_1_2_1_8_", r2(0.5 * IN), r2(0.125 * IN));
    v
}

/// Patch from hole preset `onChange` (tools.js `pr.apply(+v.tol || 0)`); `tol` is the
/// coerced tolerance in mm (NaN counts as 0).
pub fn hole_preset_patch(preset: &str, tol: f64) -> Option<serde_json::Map<String, Value>> {
    use super::util::js_num;
    let t = if tol.is_nan() { 0.0 } else { tol };
    let (_, rule) = hole_preset_rules().into_iter().find(|(id, _)| id == preset)?;
    let mut m = serde_json::Map::new();
    let mut put = |k: &str, v: Value| {
        m.insert(k.to_string(), v);
    };
    match rule {
        HoleApply::Insert { d, depth } => {
            put("kind", json!("simple"));
            put("diameter", js_num(d));
            put("through", json!(false));
            put("depth", js_num(depth));
            put("tip", json!(false));
        }
        HoleApply::Clear { d } => {
            put("kind", json!("simple"));
            put("diameter", js_num(r2(d + t)));
            put("through", json!(true));
        }
        HoleApply::Tap { d } => {
            put("kind", json!("simple"));
            put("diameter", js_num(r2(d)));
            put("through", json!(false));
            put("depth", json!(10));
            put("tip", json!(false));
        }
        HoleApply::Cbore { d, head_d, head_h } => {
            put("kind", json!("counterbore"));
            put("diameter", js_num(r2(d + t)));
            put("through", json!(true));
            put("cb_diameter", js_num(r2(head_d + 0.5 + t)));
            put("cb_depth", js_num(r2(head_h + 0.3)));
        }
        HoleApply::Nut { d, af, h } => {
            put("kind", json!("hex"));
            put("diameter", js_num(r2(d + t)));
            put("through", json!(true));
            put("hex_af", js_num(r2(af + t)));
            put("hex_depth", js_num(r2(h + 0.2)));
        }
        HoleApply::Magnet { dia, h } => {
            put("kind", json!("simple"));
            put("diameter", js_num(r2(dia + t)));
            put("through", json!(false));
            put("depth", js_num(r2(h + t)));
            put("tip", json!(false));
        }
    }
    Some(m)
}

pub fn resize_size_diameter(size: &str) -> Option<f64> {
    RESIZE_SIZES.iter().find(|(k, _, _)| *k == size).and_then(|(_, _, d)| *d)
}

pub fn connector_patch(connector: &str) -> Option<serde_json::Map<String, Value>> {
    use super::util::js_num;
    let row = CONNECTOR_DEF.iter().find(|(k, _, _)| *k == connector)?;
    Some(serde_json::Map::from_iter([
        ("pin_d".into(), js_num(row.1)),
        ("pin_len".into(), js_num(row.2)),
    ]))
}

pub fn thread_size_patch(size: &str) -> Option<serde_json::Map<String, Value>> {
    if size == "custom" { return None; }
    let t = THREAD_SIZES.iter().find(|(id, _, _, _, _)| *id == size)?;
    Some(
        serde_json::json!({
            "diameter": serde_json::Number::from_f64(t.1).unwrap_or_else(|| serde_json::Number::from(0)),
            "pitch": serde_json::Number::from_f64(t.2).unwrap_or_else(|| serde_json::Number::from(0)),
        })
        .as_object()
        .unwrap()
        .clone(),
    )
}
