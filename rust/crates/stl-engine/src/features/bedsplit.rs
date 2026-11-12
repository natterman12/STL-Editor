//! 1.x `f_bedsplit` (Fit to Bed): cut a body into pieces that fit the print bed.

use serde_json::{Map, Value};

use super::common::bed::split_to_bed;
use super::common::printers::bed_from_params;
use super::common::{float, truthy};
use crate::state::Body;
use crate::{Ctx, FeatureError, FeatureResult, Registry, State};

fn f_bedsplit(
    state: &mut State,
    fid: &str,
    p: &Map<String, Value>,
    res: &mut FeatureResult,
    _ctx: &Ctx<'_>,
) -> Result<(), FeatureError> {
    let bid = p.get("body").and_then(Value::as_str).unwrap_or("").to_string();
    let b = state.body(&bid)?.clone();
    let s = b.require_solid()?.clone();
    let bed = bed_from_params(p)?;
    let margin = float(p, "margin", Some(5.0))?;
    let connector = match p.get("connector") {
        None => "pins",
        Some(Value::String(s)) => s.as_str(),
        Some(_) => "",
    };
    let optimize = p.get("optimize").map_or(true, |v| truthy(Some(v)));
    let r = split_to_bed(&s, bed, margin, connector, p.get("pin_d"), p.get("pin_len"), p.get("clearance"), optimize)?;
    // 1.x res["info"]: the split_to_bed dict (cut positions rounded to 2 places)
    let mut cuts = Map::new();
    for (ax, v) in &r.cuts {
        let vals: Vec<Value> = v
            .iter()
            .map(|x| Value::from(format!("{x:.2}").parse::<f64>().unwrap_or(*x)))
            .collect();
        cuts.insert(["x", "y", "z"][*ax].to_string(), Value::Array(vals));
    }
    let mut info = Map::new();
    info.insert("cuts".into(), Value::Object(cuts));
    info.insert("connectors".into(), Value::from(r.connectors));
    info.insert("faces_without_connectors".into(), Value::from(r.faces_without_connectors));
    info.insert("swapped".into(), Value::Bool(r.swapped));
    res.extra.insert("info".into(), Value::Object(info));
    let mut pieces = r.pieces.into_iter();
    let n = pieces.len();
    if n == 1 {
        res.warning = Some("This body already fits the bed; nothing was split.".into());
    }
    let first = pieces.next().expect("a split always keeps at least one piece");
    let name = if n > 1 { format!("{} 1", b.name) } else { b.name.clone() };
    state.bodies.insert(bid.clone(), Body { name, solid: Some(first), mesh: None });
    res.modified.push(bid);
    for (i, pc) in pieces.enumerate() {
        let i = i + 2;
        let nid = format!("{fid}.{i}");
        state.bodies.insert(nid.clone(), Body { name: format!("{} {i}", b.name), solid: Some(pc), mesh: None });
        res.created.push(nid);
    }
    for (i, ex) in r.extras.into_iter().enumerate() {
        let nid = format!("{fid}.pin{}", i + 1);
        state.bodies.insert(nid.clone(), Body { name: format!("Pin {}", i + 1), solid: Some(ex), mesh: None });
        res.created.push(nid);
    }
    if r.faces_without_connectors > 0 && connector != "none" {
        res.warning = Some(format!(
            "{} cut face(s) were too thin for connectors. Try Filament pins (1.75 mm), or glue those seams.",
            r.faces_without_connectors
        ));
    }
    Ok(())
}

pub fn register(r: Registry) -> Registry {
    r.with("bedsplit", f_bedsplit)
}
