//! DXF import: LINE, CIRCLE, ARC, LWPOLYLINE (with bulges), matching `web/js/import2d.js`.

use serde_json::Map;

use crate::entities::{Arc, Circle, Entity, Line, Polyline, P2};
use crate::SketchError;

/// Points of a DXF bulge arc from `a` to `b` (bulge = tan(sweep / 4)), without `a`.
fn bulge_pts(a: P2, b: P2, bulge: f64, out: &mut Vec<P2>) {
    if bulge == 0.0 {
        out.push(b);
        return;
    }
    let d = ((b[0] - a[0]).hypot(b[1] - a[1])) / 2.0;
    let sweep = 4.0 * bulge.atan();
    let off = (1.0 - bulge * bulge) / (2.0 * bulge) * d;
    let r = d.hypot(off);
    let mut nx = -(b[1] - a[1]);
    let mut ny = b[0] - a[0];
    let nl = nx.hypot(ny);
    let nl = if nl == 0.0 { 1.0 } else { nl };
    nx /= nl;
    ny /= nl;
    let cx = (a[0] + b[0]) / 2.0 + nx * off;
    let cy = (a[1] + b[1]) / 2.0 + ny * off;
    let a0 = (a[1] - cy).atan2(a[0] - cx);
    let n = ((sweep.abs() * r / 0.5).ceil() as usize).clamp(4, 2000);
    for k in 1..n {
        let t = a0 + sweep * (k as f64) / (n as f64);
        out.push([cx + r * t.cos(), cy + r * t.sin()]);
    }
    out.push(b);
}

fn num(g: &[(f64, String)], code: f64, default: f64) -> f64 {
    g.iter()
        .find(|(c, _)| *c == code)
        .map(|(_, v)| js_plus(v))
        .map(|v| if v.is_finite() { v } else { default })
        .unwrap_or(default)
}

/// 1.x `+s.trim()`: `''`→0, `'10.0'`→10, non-numeric→NaN.
fn js_plus(s: &str) -> f64 {
    let t = s.trim();
    if t.is_empty() {
        return 0.0;
    }
    t.parse::<f64>().unwrap_or(f64::NAN)
}

/// DXF text → sketch entities. Broken / empty ENTITIES → empty list (no error), as in 1.x.
pub fn import_dxf(text: &str) -> Result<Vec<Entity>, SketchError> {
    let lines: Vec<&str> = text.split('\n').map(|l| l.trim_end_matches('\r')).collect();
    // 1.x: `tags.push([+L[i].trim(), L[i+1].trim()])` — blank code is 0, `'10.0'` is 10,
    // non-numeric is NaN and matches no entity rule (pair kept but inert).
    let mut tags: Vec<(f64, String)> = Vec::new();
    let mut i = 0;
    while i + 1 < lines.len() {
        let code = js_plus(lines[i]);
        let val = lines[i + 1].trim().to_string();
        i += 2;
        tags.push((code, val));
    }
    let start = tags.iter().position(|(c, v)| *c == 2.0 && v == "ENTITIES");
    let Some(start) = start else {
        return Ok(Vec::new());
    };
    let mut ents: Vec<(String, Vec<(f64, String)>)> = Vec::new();
    for (c, v) in tags.into_iter().skip(start + 1) {
        if c == 0.0 {
            if v == "ENDSEC" {
                break;
            }
            ents.push((v, Vec::new()));
        } else if c.is_finite() {
            if let Some((_, g)) = ents.last_mut() {
                g.push((c, v));
            }
        }
        // NaN codes: 1.x still pushes them into g, but every `c === N` test fails — skip.
    }
    let mut out = Vec::new();
    for (ty, g) in ents {
        match ty.as_str() {
            "LINE" => out.push(Entity::Line(Line {
                id: None,
                p1: [num(&g, 10.0, 0.0), num(&g, 20.0, 0.0)],
                p2: [num(&g, 11.0, 0.0), num(&g, 21.0, 0.0)],
                construction: None,
                extra: Map::new(),
            })),
            "CIRCLE" => out.push(Entity::Circle(Circle {
                id: None,
                cx: num(&g, 10.0, 0.0),
                cy: num(&g, 20.0, 0.0),
                r: num(&g, 40.0, 0.0),
                construction: None,
                extra: Map::new(),
            })),
            "ARC" => out.push(Entity::Arc(Arc {
                id: None,
                cx: num(&g, 10.0, 0.0),
                cy: num(&g, 20.0, 0.0),
                r: num(&g, 40.0, 0.0),
                a0: num(&g, 50.0, 0.0),
                a1: num(&g, 51.0, 0.0),
                construction: None,
                extra: Map::new(),
            })),
            "LWPOLYLINE" => {
                let mut v: Vec<(f64, f64, f64)> = Vec::new();
                for (c, s) in &g {
                    if *c == 10.0 {
                        let x = js_plus(s);
                        v.push((if x.is_finite() { x } else { 0.0 }, 0.0, 0.0));
                    } else if *c == 20.0 {
                        if let Some(last) = v.last_mut() {
                            let y = js_plus(s);
                            last.1 = if y.is_finite() { y } else { 0.0 };
                        }
                    } else if *c == 42.0 {
                        if let Some(last) = v.last_mut() {
                            let b = js_plus(s);
                            last.2 = if b.is_finite() { b } else { 0.0 };
                        }
                    }
                }
                if v.len() < 2 {
                    continue;
                }
                let closed = (num(&g, 70.0, 0.0) as i64 & 1) == 1;
                let mut pts = vec![[v[0].0, v[0].1]];
                for i in 1..v.len() {
                    let a = *pts.last().unwrap();
                    bulge_pts(a, [v[i].0, v[i].1], v[i - 1].2, &mut pts);
                }
                if closed {
                    let a = *pts.last().unwrap();
                    let b = pts[0];
                    bulge_pts(a, b, v[v.len() - 1].2, &mut pts);
                }
                out.push(Entity::Polyline(Polyline {
                    id: None,
                    pts,
                    closed: Some(closed),
                    construction: None,
                    extra: Map::new(),
                }));
            }
            _ => {}
        }
    }
    Ok(out)
}
