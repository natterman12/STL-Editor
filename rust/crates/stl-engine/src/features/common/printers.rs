//! 1.x `printing.PRINTERS`: the print beds Fit to Bed and Plate Layout know by name.

use serde_json::{Map, Value};

use super::truthy;
use crate::FeatureError;

/// `(key, label, (x, y, z) bed size in mm)`, in 1.x's order.
pub const PRINTERS: [(&str, &str, [f64; 3]); 8] = [
    ("bambu_x1", "Bambu X1C / X1E / P1S / P1P / A1", [256.0, 256.0, 256.0]),
    ("bambu_a1mini", "Bambu A1 mini", [180.0, 180.0, 180.0]),
    ("bambu_h2d", "Bambu H2D", [325.0, 320.0, 325.0]),
    ("prusa_mk4", "Prusa MK4 / MK3S", [250.0, 210.0, 220.0]),
    ("prusa_xl", "Prusa XL", [360.0, 360.0, 360.0]),
    ("voron_350", "Voron 2.4 (350)", [350.0, 350.0, 340.0]),
    ("voron_300", "Voron 2.4 (300)", [300.0, 300.0, 280.0]),
    ("ender3", "Ender 3 / V2", [220.0, 220.0, 250.0]),
];

/// The bed size of a printer key; an unknown key gives the Bambu X1 bed, as in 1.x.
pub fn printer_bed(key: Option<&str>) -> [f64; 3] {
    let k = key.unwrap_or("bambu_x1");
    PRINTERS.iter().find(|p| p.0 == k).map_or(PRINTERS[0].2, |p| p.2)
}

/// 1.x `p.get("bed") or PRINTERS.get(p.get("printer", "bambu_x1"), ...)[1]`.
pub fn bed_from_params(p: &Map<String, Value>) -> Result<[f64; 3], FeatureError> {
    if truthy(p.get("bed")) {
        if let Some(Value::Array(a)) = p.get("bed") {
            let bad = || FeatureError("bed must be three numbers.".into());
            if a.len() != 3 {
                return Err(bad());
            }
            let mut out = [0.0; 3];
            for (i, v) in a.iter().enumerate() {
                out[i] = v.as_f64().ok_or_else(bad)?;
            }
            return Ok(out);
        }
        return Err(FeatureError("bed must be three numbers.".into()));
    }
    Ok(printer_bed(p.get("printer").and_then(Value::as_str).or(if p.get("printer").is_none() { None } else { Some("") })))
}
