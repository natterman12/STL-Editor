//! The `bodies` pick field (1.x `fieldHTML` case `'bodies'`, `feedPick` for bodies), and the
//! pieces every pick field shares: the dashed selection box with its chips, what a viewport
//! click returns ([`PickOut`]), what a field wants drawn in the viewport ([`Mark`]) and the
//! "next field" rule (`advanceField`).
//!
//! The panel (Task 2) draws each field's label column; the `show` functions draw only the control.

use egui::{Align2, Color32, CornerRadius, FontFamily, FontId, Id, Rect, Sense, Stroke, Ui, Vec2};
use serde_json::Value;

use crate::engine_link::{Level, SketchOutline};
use crate::theme::Tokens;
use crate::tools::{Ctx, Field, FieldKind, ToolDef, Values};
use crate::viewport::pick::Hit;
use crate::viewport::{scene, PlaneFrame, Viewport};

// ------------------------------------------------------------------ shared pieces

/// What a pick field's `show` changed.
#[derive(Default, Debug, Clone, PartialEq)]
pub struct ShowOut {
    /// A value changed (a chip was removed, a row ticked): re-render and schedule a preview.
    pub changed: bool,
    /// The `"face"` highlight should go (a plane or face was removed).
    pub clear_face: bool,
}

/// What a viewport click did to the tool (`feedPick`'s side effects, as data).
#[derive(Default, Debug, Clone, PartialEq)]
pub struct PickOut {
    /// The click belonged to the tool: it must not also change the selection.
    pub used: bool,
    /// A value changed: re-render the form and schedule a preview (60 ms in 1.x).
    pub changed: bool,
    /// The active field moves on to the next empty pick field (`advanceField`).
    pub advance: bool,
    /// Replace the `"face"` overlay with these triangles of a body (`clearOverlay` + `highlightFace`).
    pub highlight: Option<(String, Vec<u32>)>,
    /// Remove the `"face"` overlay.
    pub clear_face: bool,
    /// A line for the tool panel's message area (`toolMsg`).
    pub message: Option<(Level, String)>,
    /// Support Fins: a body was clicked with no step being edited, so reopen on it (`reopenSupports`).
    pub reopen_supports: Option<String>,
    /// A paint tool took a body: run `quickPaint`.
    pub quick_paint: bool,
}

impl PickOut {
    pub fn used() -> Self {
        PickOut { used: true, ..Default::default() }
    }
    pub fn changed() -> Self {
        PickOut { used: true, changed: true, ..Default::default() }
    }
    pub fn said(level: Level, text: impl Into<String>) -> Self {
        PickOut { used: true, message: Some((level, text.into())), ..Default::default() }
    }
}

/// What a field wants in the viewport (`renderOverlayForTool`, `drawRegionOverlay`).
#[derive(Debug, Clone, PartialEq)]
pub enum Mark {
    Marker { p: [f64; 3], color: u32 },
    PlaneGhost(PlaneFrame),
    Regions { frame: PlaneFrame, faces: Vec<Vec<Vec<[f64; 2]>>> },
    SketchLines { frame: PlaneFrame, outlines: std::sync::Arc<[SketchOutline]> },
}

/// Draw the marks. 1.x clears `marker`, `plane-ghost`, `regions` and `sk` first; the `face`
/// highlight is kept (it is set by a pick, not by the form).
pub fn apply_marks(vp: &mut Viewport, marks: &[Mark]) {
    for tag in ["marker", "plane-ghost", "regions", "sk"] {
        vp.clear_overlay(tag);
    }
    for m in marks {
        match m {
            Mark::Marker { p, color } => vp.add_marker(*p, hex_color(*color), "marker", None),
            Mark::PlaneGhost(f) => vp.add_plane_ghost(f, "plane-ghost", hex_color(scene::PLANE_COL)),
            Mark::Regions { frame, faces } => vp.add_regions(frame, faces, "regions"),
            Mark::SketchLines { frame, outlines } => vp.add_sketch_lines(frame, outlines, hex_color(0xd9480f), 0.9, true, "sk"),
        }
    }
}

/// Show a pick's face highlight (`highlightFace` after `clearOverlay('face')`).
pub fn apply_face(vp: &mut Viewport, out: &PickOut) {
    if out.clear_face || out.highlight.is_some() {
        vp.clear_overlay("face");
    }
    if let Some((body, tris)) = &out.highlight {
        vp.highlight_face(body, tris, "face", hex_color(scene::FACE_HL));
    }
}

pub fn hex_color(c: u32) -> Color32 {
    Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8)
}

/// `isSelKind`.
pub fn is_sel_kind(k: &FieldKind) -> bool {
    matches!(
        k,
        FieldKind::Bodies { .. }
            | FieldKind::Plane { .. }
            | FieldKind::FacePoints
            | FieldKind::Points3
            | FieldKind::PFace { .. }
            | FieldKind::CylFace
            | FieldKind::Regions
    )
}

/// `isEmptySel`.
pub fn is_empty_sel(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => true,
        Some(Value::String(s)) => s.is_empty(),
        Some(Value::Array(a)) => a.is_empty(),
        _ => false,
    }
}

/// `advanceField(after)`: the next visible pick field that is still empty. `None` closes the
/// active field.
pub fn advance_field(tool: &ToolDef, values: &Values, after: &str) -> Option<String> {
    let fields: Vec<&Field> = tool.fields.iter().filter(|f| is_sel_kind(&f.kind) && f.show.is_none_or(|s| s(values))).collect();
    let i = fields.iter().position(|f| f.key == after)?;
    fields[i + 1..].iter().find(|f| is_empty_sel(values.get(f.key)) && !matches!(f.kind, FieldKind::Regions) && f.placeholder.is_none()).map(|f| f.key.to_owned())
}

/// The field to open first (`openTool`'s `activeField`).
pub fn first_active(tool: &ToolDef, values: &Values) -> Option<String> {
    tool.fields.iter().find(|f| is_sel_kind(&f.kind) && !matches!(f.kind, FieldKind::Regions) && f.placeholder.is_none() && f.show.is_none_or(|s| s(values)) && is_empty_sel(values.get(f.key))).map(|f| f.key.to_owned())
}

/// `bodyName(id)`.
pub fn body_name(ctx: &Ctx, id: &str) -> String {
    ctx.bodies.iter().find(|b| b.id == id).map(|b| b.name.clone()).unwrap_or_else(|| id.to_owned())
}

/// Round to 4 places, as `+x.toFixed(4)`.
pub fn r4(x: f64) -> f64 {
    (x * 1e4).round() / 1e4
}

pub fn pt4(p: [f64; 3]) -> Value {
    Value::from(vec![r4(p[0]), r4(p[1]), r4(p[2])])
}

pub fn pt_from(v: &Value) -> Option<[f64; 3]> {
    let a = v.as_array()?;
    Some([a.first()?.as_f64()?, a.get(1)?.as_f64()?, a.get(2)?.as_f64()?])
}

fn font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}

/// The selection box (`.sel-box`): chips (each with a remove cross) or a grey placeholder.
/// The empty box has a dashed edge (painted after the frame); the active one a solid accent edge.
pub struct BoxOut {
    /// The box was clicked (not a cross): toggle the field as the active one.
    pub clicked: bool,
    /// The cross of this chip was clicked.
    pub removed: Option<usize>,
}

pub fn sel_box(ui: &mut Ui, tk: &Tokens, id: Id, active: bool, chips: &[String], placeholder: &str) -> BoxOut {
    let mut out = BoxOut { clicked: false, removed: None };
    // `.sel-box { border: 1px dashed }`, `.active { border: 1.5px solid accent }`: the dashed edge is
    // painted after the frame, since a frame stroke can't be dashed
    let (fill, stroke) = if active { (tk.accent_soft, Stroke::new(1.5, tk.accent)) } else { (tk.input, Stroke::NONE) };
    let mut x_rects: Vec<Rect> = Vec::new();
    let resp = egui::Frame::new().fill(fill).stroke(stroke).corner_radius(CornerRadius::same(tk.radius_sm as u8)).inner_margin(egui::Margin::symmetric(6, 3)).show(ui, |ui| {
        ui.set_min_height(20.0);
        ui.set_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(3.0, 3.0);
            if chips.is_empty() {
                ui.label(egui::RichText::new(placeholder).size(11.5).color(tk.muted));
            }
            for (i, c) in chips.iter().enumerate() {
                let g = ui.painter().layout_no_wrap(c.clone(), font(11.5), tk.text);
                let w = g.size().x + 8.0 + 4.0 + 10.0 + 8.0;
                let (r, _) = ui.allocate_exact_size(Vec2::new(w, 18.0), Sense::hover());
                ui.painter().rect_filled(r, CornerRadius::same(9), tk.chip);
                ui.painter().galley(egui::pos2(r.min.x + 8.0, r.center().y - g.size().y / 2.0), g, tk.text);
                let xr = Rect::from_center_size(egui::pos2(r.max.x - 12.0, r.center().y), Vec2::splat(14.0));
                let xresp = ui.interact(xr, id.with(("x", i)), Sense::click());
                xresp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, format!("Remove {c}")));
                let col = if xresp.hovered() { tk.text } else { tk.muted };
                ui.painter().text(xr.center(), Align2::CENTER_CENTER, "×", font(12.0), col);
                x_rects.push(xr);
                if xresp.clicked() {
                    out.removed = Some(i);
                }
            }
        });
    });
    let rect = resp.response.rect;
    if !active {
        let r = rect.shrink(0.5);
        let pts = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
        ui.painter().extend(egui::Shape::dashed_line(&pts, Stroke::new(1.0, tk.input_line), 3.0, 3.0));
    }
    let bresp = ui.interact(rect, id, Sense::hover());
    bresp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, if chips.is_empty() { placeholder.to_owned() } else { chips.join(", ") }));
    if out.removed.is_none() && bresp.hovered() {
        ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::PointingHand);
        let over_x = ui.input(|i| i.pointer.interact_pos()).is_some_and(|p| x_rects.iter().any(|r| r.contains(p)));
        if !over_x && ui.input(|i| i.pointer.primary_clicked()) {
            out.clicked = true;
        }
    }
    out
}

/// What the box's clicks do to the active field and values; shared by the simple kinds.
/// `unpick` is run when a chip's cross was clicked.
pub(crate) fn handle_box(b: &BoxOut, key: &str, active: &mut Option<String>, out: &mut ShowOut, unpick: impl FnOnce(usize) -> bool) {
    if let Some(i) = b.removed {
        if unpick(i) {
            out.changed = true;
            *active = Some(key.to_owned());
        }
    } else if b.clicked {
        *active = if active.as_deref() == Some(key) { None } else { Some(key.to_owned()) };
    }
}

// ------------------------------------------------------------------ bodies

fn ids_of(v: Option<&Value>) -> Vec<String> {
    match v {
        Some(Value::Array(a)) => a.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect(),
        Some(Value::String(s)) if !s.is_empty() => vec![s.clone()],
        _ => Vec::new(),
    }
}

/// `fieldHTML` `'bodies'`: the chips are the body names; a cross removes that body.
pub fn show(ui: &mut Ui, tk: &Tokens, f: &Field, values: &mut Values, active: &mut Option<String>, ctx: &Ctx) -> ShowOut {
    let multi = matches!(f.kind, FieldKind::Bodies { multi: true });
    let ids = ids_of(values.get(f.key));
    let chips: Vec<String> = ids.iter().map(|id| body_name(ctx, id)).collect();
    let ph = f.placeholder.unwrap_or(if multi { "Click bodies" } else { "Click a body" });
    let b = sel_box(ui, tk, Id::unique(("pick", f.key)), active.as_deref() == Some(f.key), &chips, ph);
    let mut out = ShowOut::default();
    handle_box(&b, f.key, active, &mut out, |i| {
        let rest: Vec<Value> = ids.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, s)| Value::from(s.clone())).collect();
        values.insert(f.key.to_owned(), Value::Array(rest));
        true
    });
    out
}

/// `feedPick` for `bodies`. Not a body: the click is used and does nothing. A single-body field
/// takes the body and moves on; a multi field toggles it.
pub fn feed_pick(tool: &ToolDef, f: &Field, values: &mut Values, hit: &Hit, _ctx: &Ctx) -> PickOut {
    let Hit::Body { id, .. } = hit else { return PickOut::used() };
    let multi = matches!(f.kind, FieldKind::Bodies { multi: true });
    let mut cur = ids_of(values.get(f.key));
    if multi {
        if let Some(p) = cur.iter().position(|x| x == id) {
            cur.remove(p);
        } else {
            cur.push(id.clone());
        }
    } else {
        cur = vec![id.clone()];
    }
    values.insert(f.key.to_owned(), Value::Array(cur.into_iter().map(Value::from).collect()));
    let mut out = PickOut::changed();
    out.advance = !multi;
    if matches!(tool.key, "supportfins" | "supportwall") {
        out.reopen_supports = Some(id.clone()); // the caller drops this when a step is being edited
    }
    out.quick_paint = tool.flags.paint;
    out
}

/// The bodies field has nothing to draw.
pub fn marks(_f: &Field, _values: &Values, _ctx: &Ctx) -> Vec<Mark> {
    Vec::new()
}

// ------------------------------------------------------------------ dispatch (the hook Task 2 calls)

/// Draw any pick-kind field's control. `None` when the field is not one of the ten pick kinds.
/// `edit_id` is the timeline step being edited (the `features` list leaves it out).
pub fn show_pick(ui: &mut Ui, tk: &Tokens, f: &Field, values: &mut Values, active: &mut Option<String>, ctx: &Ctx, edit_id: Option<&str>) -> Option<ShowOut> {
    use super::{axisline, cylface, facepoints, features, pface, plane, points3, regions, sketch};
    Some(match f.kind {
        FieldKind::Bodies { .. } => show(ui, tk, f, values, active, ctx),
        FieldKind::Plane { .. } => plane::show(ui, tk, f, values, active, ctx),
        FieldKind::Sketch => sketch::show(ui, tk, f, values, active, ctx),
        FieldKind::Regions => regions::show(ui, tk, f, values, active, ctx),
        FieldKind::Features => features::show(ui, tk, f, values, ctx, edit_id),
        FieldKind::FacePoints => facepoints::show(ui, tk, f, values, active, ctx),
        FieldKind::PFace { .. } => pface::show(ui, tk, f, values, active, ctx),
        FieldKind::CylFace => cylface::show(ui, tk, f, values, active, ctx),
        FieldKind::Points3 => points3::show(ui, tk, f, values, active, ctx),
        FieldKind::AxisLine => axisline::show(ui, tk, f, values, active, ctx),
        _ => return None,
    })
}

/// `feedPick(hit)`: a viewport click while a tool is open. `active` is the active pick field.
/// `used == false` means the click is not the tool's (it falls through to plain selection).
pub fn feed(tool: &ToolDef, active: Option<&str>, values: &mut Values, hit: &Hit, ctx: &Ctx) -> PickOut {
    use super::{cylface, facepoints, pface, plane, points3};
    let Some(f) = plane::target_field(tool, active, hit) else { return PickOut::default() };
    match f.kind {
        FieldKind::Bodies { .. } => feed_pick(tool, f, values, hit, ctx),
        FieldKind::Plane { .. } => plane::feed_pick(tool, f, values, hit, ctx),
        FieldKind::CylFace => cylface::feed_pick(f, values, hit, ctx),
        FieldKind::PFace { .. } => pface::feed_pick(tool, f, values, hit, ctx),
        FieldKind::FacePoints => facepoints::feed_pick(f, values, hit, ctx),
        FieldKind::Points3 => points3::feed_pick(tool, f, values, hit, ctx),
        // sketch, regions (clicked on the sketch plane), features (ticked in the timeline), the
        // rest: the click is not a body or plane pick
        _ => PickOut::used(),
    }
}

/// Everything the open tool's fields want drawn (`renderOverlayForTool`).
pub fn tool_marks(tool: &ToolDef, values: &Values, ctx: &Ctx) -> Vec<Mark> {
    use super::{facepoints, plane, points3, regions};
    let mut out = Vec::new();
    for f in &tool.fields {
        if f.show.is_some_and(|s| !s(values)) {
            continue;
        }
        match f.kind {
            FieldKind::Plane { .. } => out.extend(plane::marks(f, values, ctx, tool.flags.plane_tool)),
            FieldKind::FacePoints => out.extend(facepoints::marks(f, values)),
            FieldKind::Points3 => out.extend(points3::marks(f, values)),
            FieldKind::Regions => out.extend(regions::marks(f, values, ctx)),
            _ => {}
        }
    }
    out
}
