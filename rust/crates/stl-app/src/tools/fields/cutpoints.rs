//! The `cutpoints` field (2.0, Split's "Place by hand"): connector spots clicked on the cut
//! face. The cut runs through the part, so a click doesn't pick the part's surface: it lands
//! where the view ray meets the cut plane (Split's plane moved by its Distance), and while the
//! field shows the view is opened up at the cut ([`clip_plane`]) so the face can be seen.
//!
//! Each spot gets a row with its distance from the cut face's left and bottom edges (the
//! smallest `u`, `v` of the face in the plane's own axes), which can be typed over. The points
//! are stored in world millimetres as `connector_points`; the engine moves them onto the cut
//! plane and checks that each has room (`stl-engine` `connector_place.rs`).

use egui::Ui;
use serde_json::Value;

use super::bodies::{Mark, PickOut, ShowOut, handle_box, pt_from, pt4, sel_box};
use super::number::{fmt_len, input_font, parse_len, unit_name};
use super::{Bufs, text_commit};
use crate::theme::Tokens;
use crate::tools::{Ctx, Field, Values};
use crate::viewport::PlaneFrame;

/// The dots' colour: the same blue as Hole's points.
pub const MARKER: u32 = super::facepoints::MARKER;

/// The cut plane: Split's `plane` moved along its normal by `offset`.
pub fn cut_frame(values: &Values, ctx: &Ctx) -> Option<PlaneFrame> {
    let mut f = super::plane::frame_of(values.get("plane")?, ctx)?;
    let d = values.get("offset").map_or(0.0, super::number::parse_num);
    let d = if d.is_finite() { d } else { 0.0 };
    for i in 0..3 {
        f.origin[i] += f.normal[i] * d;
    }
    Some(f)
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// The cut face's extent in the plane's `(u, v)`: where the body's triangles cross the plane.
/// `None` when the plane misses the body or no body is chosen.
pub fn face_bounds(values: &Values, ctx: &Ctx, f: &PlaneFrame) -> Option<([f64; 2], [f64; 2])> {
    let id = values
        .get("body")
        .and_then(|v| v.as_str().or_else(|| v.get(0)?.as_str()))?;
    let mesh = &ctx.bodies.iter().find(|b| b.id == id)?.mesh;
    let p = |i: u32| -> [f64; 3] {
        let k = i as usize * 3;
        [
            f64::from(mesh.positions[k]),
            f64::from(mesh.positions[k + 1]),
            f64::from(mesh.positions[k + 2]),
        ]
    };
    let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    let mut add = |q: [f64; 3]| {
        let r = [q[0] - f.origin[0], q[1] - f.origin[1], q[2] - f.origin[2]];
        let uv = [dot(r, f.x), dot(r, f.y)];
        for k in 0..2 {
            lo[k] = lo[k].min(uv[k]);
            hi[k] = hi[k].max(uv[k]);
        }
    };
    let side = |q: [f64; 3]| dot([q[0] - f.origin[0], q[1] - f.origin[1], q[2] - f.origin[2]], f.normal);
    for t in mesh.triangles.chunks_exact(3) {
        let v = [p(t[0]), p(t[1]), p(t[2])];
        let s = [side(v[0]), side(v[1]), side(v[2])];
        for (a, b) in [(0, 1), (1, 2), (2, 0)] {
            if (s[a] < 0.0) != (s[b] < 0.0) {
                let k = s[a] / (s[a] - s[b]);
                add(std::array::from_fn(|i| v[a][i] + (v[b][i] - v[a][i]) * k));
            }
        }
    }
    lo[0].is_finite().then_some((lo, hi))
}

fn points(values: &Values, key: &str) -> Vec<Value> {
    values.get(key).and_then(Value::as_array).cloned().unwrap_or_default()
}

pub fn label(n: usize) -> String {
    format!("{n} spot{}", if n == 1 { "" } else { "s" })
}

/// The selection box, then a row per spot: "Spot k", its distance from the left edge and from
/// the bottom edge of the cut face, and a cross to remove it.
pub fn show(
    ui: &mut Ui,
    tk: &Tokens,
    f: &Field,
    values: &mut Values,
    active: &mut Option<String>,
    ctx: &Ctx,
) -> ShowOut {
    let mut out = ShowOut::default();
    let mut pts = points(values, f.key);
    let chips = if pts.is_empty() { vec![] } else { vec![label(pts.len())] };
    let frame = cut_frame(values, ctx);
    let bounds = frame.as_ref().and_then(|fr| face_bounds(values, ctx, fr));
    let bufs_id = egui::Id::unique(("cutpoints-bufs", f.key));
    let mut bufs: Bufs = ui.data_mut(|d| d.get_temp(bufs_id)).unwrap_or_default();
    ui.vertical(|ui| {
        let b = sel_box(
            ui,
            tk,
            egui::Id::unique(("pick", f.key)),
            active.as_deref() == Some(f.key),
            &chips,
            "Click on the cut face",
        );
        handle_box(&b, f.key, active, &mut out, |_| {
            pts.clear();
            true
        });
        let (Some(fr), Some((lo, _))) = (frame.as_ref(), bounds) else {
            return;
        };
        let unit = unit_name(ctx.inch);
        let mut remove = None;
        for (i, v) in pts.iter_mut().enumerate() {
            let Some(p) = pt_from(v) else { continue };
            let r = [p[0] - fr.origin[0], p[1] - fr.origin[1], p[2] - fr.origin[2]];
            let uv = [dot(r, fr.x) - lo[0], dot(r, fr.y) - lo[1]];
            // two lines, to fit the control column: "Spot k ... ×", then the two distances
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(format!("Spot {}", i + 1)).size(11.5).color(tk.text));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("×").on_hover_text("Remove this spot").clicked() {
                        remove = Some(i);
                    }
                });
            });
            let mut moved = None;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for (k, word) in [(0, "Left"), (1, "Bottom")] {
                    ui.label(egui::RichText::new(word).size(11.0).color(tk.muted));
                    let shown = fmt_len(uv[k], Some(2), ctx.inch);
                    let key = format!("{}:{i}:{k}", f.key);
                    let tip = Some(format!(
                        "How far the spot is from the {} edge of the cut face, in {unit}",
                        word.to_lowercase()
                    ));
                    if let Some(t) = text_commit(ui, tk, &mut bufs, &key, &shown, 46.0, input_font(), tk.text, tip)
                        && let Some(mm) = parse_len(&t, ctx.inch)
                    {
                        let mut new = uv;
                        new[k] = mm;
                        moved = Some(new);
                    }
                }
            });
            if let Some([u, w]) = moved {
                let (u, w) = (u + lo[0], w + lo[1]);
                let q: [f64; 3] = std::array::from_fn(|j| fr.origin[j] + u * fr.x[j] + w * fr.y[j]);
                *v = pt4(q);
                out.changed = true;
            }
        }
        if let Some(i) = remove {
            pts.remove(i);
            out.changed = true;
        }
    });
    ui.data_mut(|d| d.insert_temp(bufs_id, bufs));
    if out.changed {
        values.insert(f.key.to_owned(), Value::Array(pts));
    }
    out
}

/// A click while the field is active: the spot where the view ray `(origin, dir)` meets the
/// cut plane. A ray along the plane, or one meeting it behind the camera, is not the tool's.
pub fn feed_ray(f: &Field, values: &mut Values, origin: [f64; 3], dir: [f64; 3], ctx: &Ctx) -> PickOut {
    let Some(fr) = cut_frame(values, ctx) else {
        return PickOut::default();
    };
    let den = dot(dir, fr.normal);
    if den.abs() < 1e-9 {
        return PickOut::used();
    }
    let t = dot(
        [
            fr.origin[0] - origin[0],
            fr.origin[1] - origin[1],
            fr.origin[2] - origin[2],
        ],
        fr.normal,
    ) / den;
    if t <= 0.0 {
        return PickOut::used();
    }
    let p: [f64; 3] = std::array::from_fn(|i| origin[i] + dir[i] * t);
    let mut pts = points(values, f.key);
    pts.push(pt4(p));
    values.insert(f.key.to_owned(), Value::Array(pts));
    PickOut::changed()
}

/// The spots as dots, on the cut plane.
pub fn marks(f: &Field, values: &Values, ctx: &Ctx) -> Vec<Mark> {
    let fr = cut_frame(values, ctx);
    points(values, f.key)
        .iter()
        .filter_map(pt_from)
        .map(|p| {
            // shown where the engine puts it: moved onto the cut plane
            let p = match &fr {
                Some(fr) => {
                    let d = dot(
                        [p[0] - fr.origin[0], p[1] - fr.origin[1], p[2] - fr.origin[2]],
                        fr.normal,
                    );
                    std::array::from_fn(|i| p[i] - fr.normal[i] * d)
                }
                None => p,
            };
            Mark::Marker { p, color: MARKER }
        })
        .collect()
}

/// The view's cut while the field shows: the cut plane, keeping the half away from the camera
/// so its cut face (and where the connectors go on it) can be seen. In the viewport's form
/// (keep where `dot(n, p) + d >= 0`), moved a hair towards the camera so the face itself stays.
pub fn clip_plane(values: &Values, ctx: &Ctx, camera: [f64; 3]) -> Option<[f64; 4]> {
    let fr = cut_frame(values, ctx)?;
    let n = fr.normal;
    let cam_side = dot(
        [
            camera[0] - fr.origin[0],
            camera[1] - fr.origin[1],
            camera[2] - fr.origin[2],
        ],
        n,
    ) > 0.0;
    let c = dot(n, fr.origin);
    let eps = 0.02;
    Some(if cam_side {
        // keep dot(n, p) <= c + eps
        [-n[0], -n[1], -n[2], c + eps]
    } else {
        [n[0], n[1], n[2], -(c - eps)]
    })
}
