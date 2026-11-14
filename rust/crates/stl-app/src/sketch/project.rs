//! Project and Use Edge (`app.js` `sketchProject`, `sketcher.js` `pickEdge` / `useEdge`).

use serde_json::{json, Map, Value};
use stl_sketch::entities::{Circle, Entity, Line};
use stl_sketch::solver::{refs_exist, Constraint, Ref, World};

use super::geom::dist_to_entity;
use super::{SketchDialog, SketchEffect, SketchInput, SketchSession, SketchTool, ToolCx};
use crate::engine_link::Level;
use crate::theme::Tokens;
use crate::tools::fields::{self, Bufs};
use crate::tools::fields::number::{parse_len, unit_name};
use crate::tools::panel::ctl_button;

/// What [`use_edge`] did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UseEdge {
    /// A line or circle was added, tied to the part edge.
    Added,
    /// Nothing orange under the pointer.
    Miss,
    /// The edge is not a line or a circle; the sketch is unchanged.
    Ignored,
}

/// The part edge under `uv`, within 8 screen pixels (`pickEdge`). `px` is millimetres per pixel.
pub fn pick_edge(s: &SketchSession, uv: [f64; 2], px: f64) -> Option<String> {
    let mut best: Option<(f64, String)> = None;
    let limit = 8.0 * px;
    for r in &s.refs {
        if r.missing {
            continue;
        }
        match edge_flag(&r.ent) {
            Some("edge") | None => {}
            Some(_) => continue,
        }
        let Some(id) = r.ent.id() else { continue };
        let d = dist_to_entity(&r.ent, uv);
        let closer = best.as_ref().map(|(bd, _)| d < *bd).unwrap_or(true);
        if d < limit && closer {
            best = Some((d, id.to_string()));
        }
    }
    best.map(|(_, id)| id)
}

/// Copy the part edge under `uv` into the sketch and tie it there (`useEdge`).
pub fn use_edge(s: &mut SketchSession, uv: [f64; 2], px: f64) -> UseEdge {
    let Some(pick) = pick_edge(s, uv, px) else {
        return UseEdge::Miss;
    };
    let Some(edge) = s.refs.iter().find(|r| r.ent.id() == Some(pick.as_str())).map(|r| r.ent.clone()) else {
        return UseEdge::Miss;
    };
    s.push();
    let id = s.new_id();
    match &edge {
        Entity::Line(l) => {
            s.entities.push(Entity::Line(Line {
                id: Some(id.clone()),
                p1: l.p1,
                p2: l.p2,
                construction: None,
                extra: Map::new(),
            }));
            let (c1, c2) = (s.new_id(), s.new_id());
            s.constraints.push(tied(&c1, "coincident", Ref::pt(&id, "p1"), Ref::pt(&pick, "p1")));
            s.constraints.push(tied(&c2, "coincident", Ref::pt(&id, "p2"), Ref::pt(&pick, "p2")));
        }
        Entity::Circle(c) => {
            s.entities.push(Entity::Circle(Circle {
                id: Some(id.clone()),
                cx: c.cx,
                cy: c.cy,
                r: c.r,
                construction: None,
                extra: Map::new(),
            }));
            let (c1, c2) = (s.new_id(), s.new_id());
            s.constraints.push(tied(&c1, "concentric", Ref::ent(&id), Ref::ent(&pick)));
            s.constraints.push(tied(&c2, "equal", Ref::ent(&id), Ref::ent(&pick)));
        }
        _ => {
            s.undo_local();
            return UseEdge::Ignored;
        }
    }
    s.sel.clear();
    s.sel.insert(id);
    UseEdge::Added
}

fn tied(id: &str, kind: &str, a: Ref, b: Ref) -> Constraint {
    let mut c = Constraint::new(kind);
    c.id = Some(Value::String(id.to_string()));
    c.a = Some(a);
    c.b = Some(b);
    c.extra.insert("auto".into(), Value::Bool(true));
    c
}

fn edge_flag(e: &Entity) -> Option<&str> {
    let extra = match e {
        Entity::Line(x) => &x.extra,
        Entity::Arc(x) => &x.extra,
        Entity::Circle(x) => &x.extra,
        Entity::Rect(x) => &x.extra,
        Entity::Polygon(x) => &x.extra,
        Entity::Slot(x) => &x.extra,
        Entity::Polyline(x) => &x.extra,
        Entity::Point(x) => &x.extra,
    };
    extra.get("ref").and_then(Value::as_str)
}

/// The `sketch_op` `project` payload 1.x sends (`sketchProject` plus `sketchServerOp`).
pub fn project_request(s: &SketchSession, mode: &str, body: Option<&str>, offset: f64, construction: bool) -> SketchEffect {
    let f = &s.frame;
    let mut data = Map::new();
    data.insert(
        "frame".into(),
        json!([
            [f.origin[0], f.origin[1], f.origin[2]],
            [f.x[0], f.x[1], f.x[2]],
            [f.y[0], f.y[1], f.y[2]],
            [f.normal[0], f.normal[1], f.normal[2]],
        ]),
    );
    let mode = if mode.is_empty() { "section" } else { mode };
    data.insert("mode".into(), Value::String(mode.into()));
    data.insert("offset".into(), json!(offset));
    data.insert("construction".into(), Value::Bool(construction));
    data.insert("entities".into(), serde_json::to_value(&s.entities).unwrap_or(json!([])));
    data.insert("seed".into(), Value::String(s.seed.clone()));
    if let Some(b) = body.filter(|b| !b.is_empty()) {
        data.insert("bodies".into(), json!([b]));
    }
    SketchEffect::SketchOp { op: "project".into(), data: Value::Object(data) }
}

struct ProjectTool;

struct UseEdgeTool;

impl SketchTool for ProjectTool {
    fn key(&self) -> &'static str {
        "sk_project"
    }
    fn hint(&self, _s: &SketchSession) -> String {
        String::new()
    }
    fn input(&mut self, cx: &mut ToolCx, ev: &SketchInput) -> bool {
        // 1.x opens a dialog instead of a click tool (the ribbon opens it directly).
        if matches!(ev, SketchInput::Down { .. }) {
            cx.out.push(SketchEffect::Dialog(SketchDialog::Project));
            return true;
        }
        false
    }
    fn cancel(&mut self, _cx: &mut ToolCx) {}
    fn picks_geometry(&self) -> bool {
        false
    }
    fn preview(&self, _s: &SketchSession) -> Vec<crate::viewport::overlays::SketchOutline> {
        Vec::new()
    }
    fn sketch_op_result(&mut self, cx: &mut ToolCx, op: &str, result: &Value) {
        if op != "project" {
            return;
        }
        let fx = apply_result(cx.s, result);
        cx.out.extend(fx);
    }
}

/// The answer of `project` (`sketchProject` after `sketchServerOp`): the shapes are replaced,
/// the new outlines selected, and the toast says how many.
pub fn apply_result(s: &mut SketchSession, result: &Value) -> Vec<SketchEffect> {
    let Some(list) = result.get("entities").and_then(Value::as_array) else { return Vec::new() };
    let Ok(ents) = stl_sketch::entities::parse_entities(list) else { return Vec::new() };
    s.push();
    s.entities = ents;
    let mut all = s.entities.clone();
    all.extend(s.refs.iter().map(|r| r.ent.clone()));
    let world = World::new(&all);
    s.constraints.retain(|c| refs_exist(c, &world));
    let added: Vec<String> = result
        .get("added")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    s.sel.clear();
    for id in &added {
        s.sel.insert(id.clone());
    }
    let n = added.len();
    let word = if n == 1 { "outline" } else { "outlines" };
    vec![SketchEffect::Toast(Level::Success, format!("Added {n} {word}.")), SketchEffect::Changed]
}

// ------------------------------------------------------------------ the Project outline dialog

/// What the Project outline dialog (`sketchProject`) holds while it is open.
pub struct ProjectForm {
    /// The visible solid bodies: (id, name).
    pub bodies: Vec<(String, String)>,
    /// `"section"` or `"silhouette"`.
    pub mode: String,
    /// The body picked; empty = all visible bodies.
    pub body: String,
    pub construction: bool,
    pub bufs: Bufs,
    pub inch: bool,
}

impl ProjectForm {
    /// The dialog's defaults: section, all bodies, offset 0, not construction.
    pub fn new(bodies: Vec<(String, String)>, inch: bool) -> Self {
        let mut bufs = Bufs::new();
        bufs.insert("offset".into(), "0".into());
        ProjectForm { bodies, mode: "section".into(), body: String::new(), construction: false, bufs, inch }
    }

    /// The offset typed, in mm (`U.parse(...) || 0`).
    pub fn offset_mm(&self) -> f64 {
        self.bufs.get("offset").and_then(|t| parse_len(t.trim(), self.inch)).filter(|v| v.is_finite()).unwrap_or(0.0)
    }
}

/// The dialog's answer.
pub enum ProjectAnswer {
    Cancel,
    Project,
}

/// The card's contents; the app puts it over a scrim. Enter projects, Escape cancels.
pub fn dialog(ui: &mut egui::Ui, tk: &Tokens, f: &mut ProjectForm) -> Option<ProjectAnswer> {
    use egui::{CornerRadius, Frame, Margin, RichText};
    let mut ans = None;
    ui.label(RichText::new("Project outline").font(egui::FontId::new(15.0, egui::FontFamily::Name("bold".into()))).color(tk.strong));
    ui.add_space(8.0);
    ui.add(egui::Label::new(RichText::new("Adds the outline of the model to this sketch as shapes you can offset, trim or extrude.").size(12.0).color(tk.muted)).wrap());
    ui.add_space(6.0);
    fields::row_raw(ui, tk, "Outline", None, false, |ui, w| {
        let opts = [("section", "Where the sketch plane cuts the model"), ("silhouette", "Shadow of the whole model on the plane")];
        let shown = opts.iter().find(|(v, _)| *v == f.mode).map(|(_, l)| *l).unwrap_or(opts[0].1);
        let mut chosen = None;
        fields::framed_combo(ui, tk, ("sk-project", "mode"), w, shown, |ui| {
            for (v, l) in opts {
                if ui.selectable_label(f.mode == v, l).clicked() {
                    chosen = Some(v);
                }
            }
        });
        if let Some(v) = chosen {
            f.mode = v.into();
        }
        None::<()>
    });
    fields::row_raw(ui, tk, "Bodies", None, false, |ui, w| {
        let shown = f.bodies.iter().find(|(id, _)| *id == f.body).map(|(_, n)| n.clone()).unwrap_or_else(|| "All visible bodies".into());
        let mut chosen = None;
        fields::framed_combo(ui, tk, ("sk-project", "bodies"), w, &shown, |ui| {
            if ui.selectable_label(f.body.is_empty(), "All visible bodies").clicked() {
                chosen = Some(String::new());
            }
            for (id, name) in &f.bodies {
                if ui.selectable_label(f.body == *id, name.as_str()).clicked() {
                    chosen = Some(id.clone());
                }
            }
        });
        if let Some(v) = chosen {
            f.body = v;
        }
        None::<()>
    });
    fields::row_raw(ui, tk, "Offset", None, false, |ui, w| {
        let shown = f.bufs.get("offset").cloned().unwrap_or_else(|| "0".into());
        fields::text_commit(ui, tk, &mut f.bufs, "offset", &shown, w - 22.0, fields::number::input_font(), tk.text, None);
        fields::unit_label(ui, tk, unit_name(f.inch));
        None::<()>
    });
    fields::row_raw(ui, tk, "Construction", None, false, |ui, _w| {
        fields::check_box(ui, tk, &mut f.construction);
        ui.label(egui::RichText::new("Reference only (not part of the profile)").size(12.0).color(tk.text));
        None::<()>
    });
    ui.add_space(4.0);
    Frame::new().fill(tk.info_bg).corner_radius(CornerRadius::same(tk.radius_sm as u8)).inner_margin(Margin::symmetric(8, 6)).show(ui, |ui| {
        ui.add(
            egui::Label::new(
                RichText::new("Offset grows (+) or shrinks (−) the outline, e.g. +0.3 for a lid that fits over the part. Round outlines come in as circles.")
                    .size(12.0)
                    .color(tk.info_text),
            )
            .wrap(),
        );
    });
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            if ctl_button(ui, tk, "Project", true).clicked() {
                ans = Some(ProjectAnswer::Project);
            }
            if ctl_button(ui, tk, "Cancel", false).clicked() {
                ans = Some(ProjectAnswer::Cancel);
            }
        });
    });
    // Enter / Escape, unless a field is being typed in (it takes Enter itself)
    if ans.is_none() && !ui.ctx().egui_wants_keyboard_input() {
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)) {
            ans = Some(ProjectAnswer::Project);
        } else if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            ans = Some(ProjectAnswer::Cancel);
        }
    }
    ans
}

impl SketchTool for UseEdgeTool {
    fn key(&self) -> &'static str {
        "sk_useedge"
    }
    fn hint(&self, _s: &SketchSession) -> String {
        "Click an orange part edge to turn it into a sketch line or circle (it stays tied to the edge)".into()
    }
    fn input(&mut self, cx: &mut ToolCx, ev: &SketchInput) -> bool {
        let SketchInput::Down { uv, .. } = ev else { return false };
        let px = cx.vp.pixel_size(cx.s.to_3(*uv));
        match use_edge(cx.s, *uv, px) {
            UseEdge::Added => cx.out.push(SketchEffect::Changed),
            UseEdge::Miss => {
                cx.out.push(SketchEffect::Toast(Level::Warning, "Click one of the orange part edges.".into()))
            }
            UseEdge::Ignored => {}
        }
        true
    }
    fn cancel(&mut self, _cx: &mut ToolCx) {}
    fn picks_geometry(&self) -> bool {
        true
    }
    fn preview(&self, _s: &SketchSession) -> Vec<crate::viewport::overlays::SketchOutline> {
        Vec::new()
    }
}

/// `sk_project` and `sk_useedge`.
pub fn tool_for(key: &str) -> Option<Box<dyn SketchTool>> {
    match key {
        "sk_project" => Some(Box::new(ProjectTool)),
        "sk_useedge" => Some(Box::new(UseEdgeTool)),
        _ => None,
    }
}
