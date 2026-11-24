//! `check`: what the Support Fins panel shows after a build, asked of the engine separately so
//! the build itself stays as fast as before and its result (and the 1.x golden files) is unchanged.
//!
//! * The support-section check (Support Fins 1.0.8): the engine slices the fins it built layer by
//!   layer and flags sections narrower than one line of the nozzle, and pieces that start in mid-air
//!   with nothing under them. The engine runs it only when asked (`diagnostics: true`), so this
//!   builds the fins once more with that set. The findings it counts are the website's
//!   (`web/fins/diagnostics.js` `supportFindings`): unresolved starts and narrow sections.
//! * The tilt hint (Support Fins 1.0.9, `tiltHintB64`): when overhangs were too shallow for a fin,
//!   a pose that leaves fewer of them, with its rotation (the same column-major 3x3 as Suggest's
//!   candidates) so the panel can turn the part.
//!
//! An engine older than either piece answers without it: no `diagnostics`, or no `tiltHintB64`.

use serde_json::{Value, json};

use super::{MeshData, call, has, options, parse_json, soup};
use crate::{FeatureError, FinsHost};

/// The check for `mesh` as it sits, with the panel's `values`:
/// `{"status", "findings": [{"kind", "piece", "z", "box"}], "unserved", "floating", "tilt"}`.
/// `status` is the engine's (`complete`, `incomplete`, `unavailable`) or null when this engine
/// has no check. Boxes are `[x0, y0, z0, x1, y1, z1]` in the body's own frame, one layer tall.
pub fn check(host: &dyn FinsHost, mesh: &MeshData, values: &Value) -> Result<Value, FeatureError> {
    let opts = options(host, values)?;
    let mut o = parse_json(&opts)?;
    if let Value::Object(m) = &mut o {
        m.insert("diagnostics".into(), Value::Bool(true));
    }
    let o = serde_json::to_string(&o).unwrap_or_else(|_| opts.clone());
    let s = soup(mesh);
    let r = parse_json(&call(host, "computeFinsB64", &[&s, &o])?)?;
    let stats = r.get("stats").cloned().unwrap_or(Value::Null);
    let count = |k: &str| stats.get(k).and_then(Value::as_u64).unwrap_or(0);
    let (unserved, floating) = (count("unserved"), count("floating"));
    let support = r.get("diagnostics").and_then(|d| d.get("support"));
    let status = support.and_then(|s| s.get("status")).cloned().unwrap_or(Value::Null);
    let findings = support.map(|s| findings(s, r.get("offset"))).unwrap_or_default();
    let tilt = if unserved > 0 && has(host, "tiltHintB64") {
        parse_json(&call(
            host,
            "tiltHintB64",
            &[&s, &opts, &unserved.to_string(), &floating.to_string()],
        )?)?
    } else {
        Value::Null
    };
    Ok(json!({
        "status": status,
        "findings": findings,
        "unserved": unserved,
        "floating": floating,
        "tilt": tilt,
    }))
}

/// `supportFindings`: unresolved starts and narrow sections, lowest layer first, each as a box in
/// the body's frame. The engine reports them in its seated frame (seated = input + offset).
fn findings(support: &Value, offset: Option<&Value>) -> Vec<Value> {
    let off = |k: &str| offset.and_then(|o| o.get(k)).and_then(Value::as_f64).unwrap_or(0.0);
    let (ox, oy, oz) = (off("x"), off("y"), off("z"));
    let ctx = support.get("context");
    let layer_h = |layer: u64| {
        let key = if layer == 0 { "firstLayerHeight" } else { "layerHeight" };
        ctx.and_then(|c| c.get(key)).and_then(Value::as_f64).unwrap_or(0.2)
    };
    let list = |key: &str| support.get(key).and_then(Value::as_array).cloned().unwrap_or_default();
    let mut out: Vec<(u64, String, &'static str, Value)> = Vec::new();
    let mut add = |f: &Value, kind: &'static str| {
        let b: Vec<f64> = f
            .get("bounds")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_f64).collect())
            .unwrap_or_default();
        let (Some(z), true) = (f.get("z").and_then(Value::as_f64), b.len() == 4) else {
            return;
        };
        let layer = f.get("layer").and_then(Value::as_u64).unwrap_or(0);
        let piece = match f.get("piece") {
            Some(Value::String(s)) => s.clone(),
            Some(v) => v.to_string(),
            None => String::new(),
        };
        let z1 = z - oz;
        let z0 = z1 - layer_h(layer);
        let item = json!({
            "kind": kind,
            "piece": piece.clone(),
            "z": f.get("printZ").and_then(Value::as_f64).unwrap_or(z),
            "box": [b[0] - ox, b[1] - oy, z0, b[2] - ox, b[3] - oy, z1],
        });
        out.push((layer, piece, kind, item));
    };
    for s in list("starts") {
        if s.get("root").and_then(Value::as_str) == Some("unresolved") {
            add(&s, "start");
        }
    }
    for s in list("narrow") {
        add(&s, "narrow");
    }
    // the website's order: layer, then piece, then kind
    out.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)).then_with(|| a.2.cmp(b.2)));
    out.into_iter().map(|t| t.3).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn findings_are_counted_like_the_website_and_moved_to_the_body_frame() {
        let support = json!({
            "context": {"layerHeight": 0.2, "firstLayerHeight": 0.3},
            "starts": [
                {"piece": "1", "layer": 4, "z": 1.1, "printZ": 1.1, "bounds": [0, 0, 1, 1], "root": "unresolved"},
                {"piece": "0", "layer": 0, "z": 0.3, "printZ": 0.3, "bounds": [0, 0, 1, 1], "root": "plate"},
                {"piece": "2", "layer": 1, "z": 0.5, "printZ": 0.5, "bounds": [0, 0, 1, 1], "root": "narrow-below"}
            ],
            "narrow": [{"piece": "0", "layer": 0, "z": 0.3, "printZ": 0.3, "bounds": [-4, -0.25, 4, 0.25]}],
            "growth": [{"piece": "3", "layer": 2, "z": 0.7, "bounds": [0, 0, 1, 1]}]
        });
        let f = findings(&support, Some(&json!({"x": 10.0, "y": -5.0, "z": 2.0})));
        assert_eq!(
            f.len(),
            2,
            "plate starts, narrow-below starts and growth are not counted"
        );
        assert_eq!(f[0]["kind"], "narrow");
        assert_eq!(f[0]["piece"], "0");
        assert_eq!(f[0]["z"], 0.3);
        // seated - offset; one first layer (0.3 mm) tall, ending at the layer's top
        let b: Vec<f64> = f[0]["box"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap())
            .collect();
        let want = [-14.0, 4.75, -2.0, -6.0, 5.25, -1.7];
        for (g, w) in b.iter().zip(want) {
            assert!((g - w).abs() < 1e-9, "{b:?}");
        }
        assert_eq!(f[1]["kind"], "start");
        assert_eq!(f[1]["piece"], "1");
    }

    #[test]
    fn no_report_no_findings() {
        assert!(findings(&json!({}), None).is_empty());
    }
}
