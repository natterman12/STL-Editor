//! The ribbon (1.x `#ribbon`): data from `ribbonDef()` in web/js/app.js, drawn to the sizes of
//! web/css/app.css. Sketch mode's ribbon is batch 4c.

use crate::icons;
use crate::theme::Tokens;
use egui::{Align2, Color32, CornerRadius, FontFamily, FontId, Id, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tool {
    pub key: &'static str,
    pub label: &'static str,
    pub icon: &'static str,
}

const fn t(key: &'static str, label: &'static str, icon: &'static str) -> Tool {
    Tool { key, label, icon }
}

#[derive(Clone, Debug)]
pub enum Item {
    /// A big button.
    Big(Tool),
    /// Up to four small buttons stacked.
    Stack(Vec<Tool>),
    /// A dropdown (`drop` id, label, tools).
    Drop { id: &'static str, label: &'static str, items: Vec<Tool> },
}

#[derive(Clone, Debug)]
pub struct Group {
    pub label: &'static str,
    pub items: Vec<Item>,
}

impl Group {
    pub fn tools(&self) -> Vec<Tool> {
        flat_items(&self.items)
    }
    pub fn has_drop(&self) -> bool {
        self.items.iter().any(|i| matches!(i, Item::Drop { .. }))
    }
}

pub struct Def {
    pub tabs: Vec<(&'static str, &'static str)>,
    pub groups: Vec<Group>,
    pub pinned: Group,
}

/// Every tool inside a list of items (`App.flatItems`).
pub fn flat_items(items: &[Item]) -> Vec<Tool> {
    items
        .iter()
        .flat_map(|i| match i {
            Item::Big(t) => vec![*t],
            Item::Stack(v) => v.clone(),
            Item::Drop { items, .. } => items.clone(),
        })
        .collect()
}

pub const TABS: [(&str, &str); 4] = [("solid", "Model"), ("print", "Print"), ("supports", "Supports"), ("mesh", "Mesh")];

pub fn tab_icon(tab: &str) -> &'static str {
    match tab {
        "print" => "bedsplit",
        "supports" => "fins",
        "mesh" => "remesh",
        "sketch" => "sketch",
        _ => "box",
    }
}

/// `ribbonDef(tab)` for the model (non-sketch) ribbon.
pub fn def(tab: &str) -> Def {
    use Item::*;
    let planes = vec![t("plane_offset", "Offset Plane", "plane"), t("plane_angle", "Plane at Angle", "plane_angle"), t("plane_3pt", "3-Point Plane", "plane_3pt"), t("plane_mid", "Midplane", "plane_mid")];
    let pinned = Group { label: "Inspect", items: vec![Stack(vec![t("measure", "Measure", "measure"), t("section", "Section", "section"), t("info", "Properties", "info")])] };
    let g = |label, items| Group { label, items };
    let groups = match tab {
        "print" => vec![
            g("Position", vec![Big(t("layflat", "Lay Flat", "layflat")), Stack(vec![t("move", "Move", "move"), t("rotate", "Rotate", "rotate"), t("scale", "Scale", "scale")])]),
            g("Fit", vec![Big(t("bedsplit", "Fit to Bed", "bedsplit")), Big(t("plate", "Plate Layout", "bedsplit")), Big(t("fittest", "Fit Test", "hole"))]),
            g("Check", vec![Big(t("thincheck", "Thin-wall Check", "measure"))]),
            g("Output", vec![Big(t("export", "Export", "export")), Big(t("send", "Send to Slicer", "slicer"))]),
        ],
        "supports" => vec![
            g("Supports", vec![Big(t("supportfins", "Support Fins", "fins")), Big(t("supportwall", "Draw Walls", "finwall"))]),
            g("Support Fins", vec![Stack(vec![t("sf_about", "About Support Fins", "info")])]),
        ],
        "mesh" => vec![
            g("Mesh", vec![Big(t("repair", "Repair", "repair")), Big(t("simplify", "Simplify", "simplify")), Big(t("remesh", "Remesh", "remesh")), Stack(vec![t("separate", "Separate", "separate"), t("delete", "Remove Bodies", "delete")])]),
            g("Sculpt", vec![Big(t("sculpt", "Brushes", "sculpt"))]),
            g("Convert", vec![Big(t("to_step", "To STEP", "step"))]),
        ],
        _ => vec![
            g(
                "Create",
                vec![
                    Big(t("sketch", "Sketch", "sketch")),
                    Big(t("extrude", "Extrude", "extrude")),
                    Big(t("revolve", "Revolve", "revolve")),
                    Big(t("loft", "Loft", "extrude")),
                    Big(t("sweep", "Sweep", "revolve")),
                    Big(t("snapfit", "Snap-fit", "extrude")),
                    Drop {
                        id: "primitive",
                        label: "Primitive",
                        items: vec![t("primitive_box", "Box", "box"), t("primitive_cylinder", "Cylinder", "cylinder"), t("primitive_sphere", "Sphere", "sphere"), t("primitive_cone", "Cone", "cone"), t("primitive_torus", "Torus", "torus")],
                    },
                ],
            ),
            g("Modify", vec![Big(t("pushpull", "Push/Pull", "pushpull")), Big(t("combine", "Combine", "combine")), Big(t("split", "Split", "split")), Big(t("mirror", "Mirror", "mirror")), Big(t("pattern", "Pattern", "pattern"))]),
            g("Holes", vec![Big(t("hole", "Hole", "hole")), Big(t("thread", "Thread", "thread")), Stack(vec![t("resizehole", "Resize Hole", "resizehole")])]),
            g("Shape", vec![Stack(vec![t("hollow", "Hollow", "hollow"), t("shell", "Shell", "hollow"), t("lattice", "Lattice", "hollow"), t("offset", "Offset Body", "offset")]), Big(t("hinge", "Living hinge", "split"))]),
            g("Reference", vec![Drop { id: "plane", label: "Plane", items: planes }, Big(t("params", "Parameters", "params"))]),
        ],
    };
    Def { tabs: TABS.to_vec(), groups, pinned }
}

/// The remaining entries of `TOOLS` in web/js/tools.js that have a label (key, label), for the
/// tool search. Generated by importing tools.js; the ribbon ones are skipped when listed.
const TOOL_LABELS: &[(&str, &str)] = &[
    ("extrude", "Extrude"), ("revolve", "Revolve"), ("loft", "Loft"), ("sweep", "Sweep"), ("primitive_box", "Box"), ("primitive_cylinder", "Cylinder"),
    ("primitive_sphere", "Sphere"), ("primitive_cone", "Cone"), ("primitive_torus", "Torus"), ("snapfit", "Snap-fit"), ("hinge", "Living hinge"), ("hole", "Hole"),
    ("combine", "Combine"), ("split", "Split"), ("bedsplit", "Fit to Bed"), ("plate", "Plate Layout"), ("layflat", "Lay Flat"), ("thread", "Thread"),
    ("offset", "Offset Body"), ("thincheck", "Thin-wall Check"), ("fittest", "Fit Test"), ("resizehole", "Resize Hole"), ("pushpull", "Push/Pull"), ("mirror", "Mirror"),
    ("move", "Move / Rotate"), ("scale", "Scale"), ("pattern", "Pattern"), ("hollow", "Hollow"), ("lattice", "Lattice"), ("shell", "Shell"),
    ("plane_offset", "Offset Plane"), ("plane_angle", "Plane at Angle"), ("plane_3pt", "3-Point Plane"), ("plane_mid", "Midplane"), ("import", "Imported Mesh"),
    ("sculpt", "Sculpt"), ("remesh", "Remesh"), ("repair", "Repair"), ("simplify", "Simplify"), ("separate", "Separate"), ("delete", "Remove Bodies"),
];

/// One entry of the tool search (`allCommands()`).
#[derive(Clone, Debug, PartialEq)]
pub struct Command {
    pub k: String,
    pub l: String,
    pub ic: String,
    /// The tab it lives on (`None` for File commands and tools only the search knows).
    pub tab: Option<&'static str>,
    pub group: String,
}

pub fn all_commands() -> Vec<Command> {
    let mut out: Vec<Command> = Vec::new();
    let mut add = |k: &str, l: &str, ic: &str, tab: Option<&'static str>, group: &str| {
        if !out.iter().any(|c| c.k == k) {
            out.push(Command { k: k.into(), l: l.into(), ic: ic.into(), tab, group: group.into() });
        }
    };
    for (tab, _) in TABS {
        for g in def(tab).groups {
            for x in g.tools() {
                add(x.key, x.label, x.icon, Some(tab), g.label);
            }
        }
    }
    let d = def("solid");
    for x in d.pinned.tools() {
        add(x.key, x.label, x.icon, None, d.pinned.label);
    }
    add("import", "Insert Mesh", "import", None, "File");
    add("export", "Export", "export", None, "File");
    add("send", "Send to Slicer", "slicer", None, "File");
    for (k, l) in TOOL_LABELS {
        if *k != "delete" {
            add(k, l, k, None, "");
        }
    }
    out
}

/// The search: every word must be in "label group key"; best first, at most 12.
pub fn search(q: &str) -> Vec<Command> {
    let q = q.trim().to_lowercase();
    if q.is_empty() {
        return Vec::new();
    }
    let words: Vec<&str> = q.split_whitespace().collect();
    let mut hits: Vec<(u8, Command)> = all_commands()
        .into_iter()
        .filter_map(|c| {
            let hay = format!("{} {} {}", c.l, c.group, c.k.replace('_', " ")).to_lowercase();
            if !words.iter().all(|w| hay.contains(w)) {
                return None;
            }
            let l = c.l.to_lowercase();
            let score = if l.starts_with(&q) {
                0
            } else if l.split(|ch: char| ch.is_whitespace() || ch == '/').any(|w| w.starts_with(words[0])) {
                1
            } else {
                2
            };
            Some((score, c))
        })
        .collect();
    hits.sort_by_key(|h| h.0);
    hits.into_iter().take(12).map(|h| h.1).collect()
}

// ------------------------------------------------------------------ drawing

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Normal,
    Compact,
    Tight,
}

/// The height of the buttons row: 54, or taller when a group stacks more than three small buttons
/// (1.x's `.rgroup-items` grows with its tallest stack: the Model tab's Shape group is 4 x 18).
pub fn items_h(tab: &str) -> f32 {
    let d = def(tab);
    let stack = d.groups.iter().chain(std::iter::once(&d.pinned)).flat_map(|g| g.items.iter()).map(|i| if let Item::Stack(v) = i { v.len() as f32 * 18.0 } else { 0.0 }).fold(0.0, f32::max);
    stack.max(54.0)
}

/// Total height of the ribbon for a tab (tab row + body), points.
pub fn height(tab: &str) -> f32 {
    25.0 + 3.0 + items_h(tab) + 17.0 + 3.0 + 1.0
}

/// What a click on the ribbon asks for.
#[derive(Clone, Debug, PartialEq)]
pub enum Pick {
    Tab(&'static str),
    Tool(&'static str),
    /// A dropdown button: id and the rect to open the menu under.
    Drop(&'static str, Rect),
    /// The "More" button.
    More(Rect),
    /// A group name with a dropdown: group index and rect.
    GroupMenu(usize, Rect),
}

pub struct RibbonState<'a> {
    pub tab: &'a str,
    /// A tool that is open, drawn as pressed.
    pub active: Option<&'a str>,
    /// The tool last chosen from each dropdown (its icon shows on the button).
    pub drop_last: &'a std::collections::HashMap<String, String>,
    pub web: bool,
}

/// Which groups are folded into "More" at this width; also the size mode.
pub struct Fit {
    pub mode: Mode,
    pub folded: Vec<bool>,
    pub more: bool,
}

fn font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}

fn text_w(ui: &Ui, s: &str, size: f32) -> f32 {
    ui.painter().layout_no_wrap(s.to_owned(), font(size), Color32::WHITE).size().x
}

const CARET_W: f32 = 8.0;

fn tool_w(ui: &Ui, mode: Mode, big: bool, label: &str, caret: bool) -> f32 {
    let tw = text_w(ui, label, if mode == Mode::Compact && big { 10.0 } else { 11.0 }) + if caret { CARET_W } else { 0.0 };
    match (big, mode) {
        (true, Mode::Normal) => (tw + 10.0 + 2.0).max(46.0),
        (true, Mode::Compact) => (tw + 4.0 + 2.0).max(40.0),
        (true, Mode::Tight) => 34.0,
        (false, Mode::Tight) => 15.0 + 6.0 + 2.0,
        (false, _) => 15.0 + 5.0 + tw + 9.0 + 2.0,
    }
}

fn item_w(ui: &Ui, mode: Mode, it: &Item) -> f32 {
    match it {
        Item::Big(x) => tool_w(ui, mode, true, x.label, false),
        Item::Stack(v) => v.iter().map(|x| tool_w(ui, mode, false, x.label, false)).fold(0.0, f32::max),
        Item::Drop { label, .. } => tool_w(ui, mode, true, label, true),
    }
}

fn group_w(ui: &Ui, mode: Mode, g: &Group) -> f32 {
    let pad = if mode == Mode::Normal { 12.0 } else { 6.0 };
    let items: f32 = g.items.iter().map(|i| item_w(ui, mode, i)).sum::<f32>() + 2.0 * g.items.len().saturating_sub(1) as f32;
    let label = text_w(ui, g.label, 11.0) + if g.has_drop() { 12.0 } else { 0.0 };
    items.max(label) + pad + 1.0
}

/// Pick the size mode and fold groups into "More" like `fitRibbon()`.
pub fn fit(ui: &Ui, d: &Def, avail: f32) -> Fit {
    let total = |m: Mode| d.groups.iter().map(|g| group_w(ui, m, g)).sum::<f32>() + group_w(ui, m, &d.pinned);
    for m in [Mode::Normal, Mode::Compact, Mode::Tight] {
        if total(m) <= avail {
            return Fit { mode: m, folded: vec![false; d.groups.len()], more: false };
        }
    }
    let mut folded = vec![false; d.groups.len()];
    let more_w = 34.0 + 6.0 + 1.0;
    let mut sum = total(Mode::Tight) + more_w;
    let mut i = d.groups.len();
    while i > 1 && sum > avail {
        i -= 1;
        folded[i] = true;
        sum -= group_w(ui, Mode::Tight, &d.groups[i]);
    }
    Fit { mode: Mode::Tight, folded, more: true }
}

fn paint_tool(ui: &mut Ui, tk: &Tokens, rect: Rect, id: Id, tool: &Tool, icon: &str, label: &str, big: bool, caret: bool, mode: Mode, on: bool, web: bool) -> egui::Response {
    let desk_only = web && matches!(tool.key, "to_step" | "send");
    let resp = ui.interact(rect, id, Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
    let p = ui.painter();
    let finish = tool.key.starts_with("finish");
    let (mut fg, ic) = if on { (tk.sel_text, tk.sel_text) } else if finish { (tk.ok, tk.ok) } else { (tk.text, tk.accent) };
    if resp.hovered() && !on {
        p.rect(rect, CornerRadius::same(tk.radius_sm as u8), tk.hover, Stroke::new(1.0, tk.line), StrokeKind::Inside);
    } else if on {
        p.rect(rect, CornerRadius::same(tk.radius_sm as u8), tk.sel, Stroke::new(1.0, tk.sel_line), StrokeKind::Inside);
    }
    if desk_only {
        fg = fg.gamma_multiply(0.7);
    }
    let cx = rect.center().x;
    if big {
        let (isz, fsz) = if mode == Mode::Compact { (20.0, 10.0) } else { (22.0, 11.0) };
        let top = rect.min.y + if mode == Mode::Tight { 7.0 } else { 5.0 };
        let img = icons::icon(ui.ctx(), icon, isz, ic);
        egui::Image::new(img).paint_at(ui, Rect::from_center_size(Pos2::new(cx, top + isz / 2.0), Vec2::splat(isz)));
        if mode != Mode::Tight {
            let g = ui.painter().layout_no_wrap(label.to_owned(), font(fsz), fg);
            let w = g.size().x + if caret { CARET_W } else { 0.0 };
            let pos = Pos2::new(cx - w / 2.0, top + isz + 4.0);
            ui.painter().galley(pos, g, fg);
            if caret {
                let c = Pos2::new(pos.x + w - CARET_W + 4.0, pos.y + 6.5);
                ui.painter().add(egui::Shape::convex_polygon(vec![c + Vec2::new(-2.5, -1.5), c + Vec2::new(2.5, -1.5), c + Vec2::new(0.0, 1.5)], tk.muted, Stroke::NONE));
            }
        } else if caret {
            let c = Pos2::new(cx + 12.0, rect.max.y - 12.0);
            ui.painter().add(egui::Shape::convex_polygon(vec![c + Vec2::new(-2.5, -1.5), c + Vec2::new(2.5, -1.5), c + Vec2::new(0.0, 1.5)], tk.muted, Stroke::NONE));
        }
    } else {
        let x0 = rect.min.x + 4.0;
        let img = icons::icon(ui.ctx(), icon, 15.0, ic);
        egui::Image::new(img).paint_at(ui, Rect::from_center_size(Pos2::new(x0 + 7.5, rect.center().y), Vec2::splat(15.0)));
        if mode != Mode::Tight {
            let g = ui.painter().layout_no_wrap(label.to_owned(), font(11.0), fg);
            let y = rect.center().y - g.size().y / 2.0;
            ui.painter().galley(Pos2::new(x0 + 20.0, y), g, fg);
        }
    }
    let tip = if desk_only { format!("{label} · desktop app") } else { label.to_owned() };
    resp.on_hover_text(tip)
}

/// Draw the ribbon into `ui` (the whole panel). Returns what was clicked.
pub fn show(ui: &mut Ui, tk: &Tokens, st: &RibbonState) -> (Option<Pick>, Fit) {
    let d = def(st.tab);
    let full = ui.available_rect_before_wrap();
    let items_h = items_h(st.tab);
    let rect = Rect::from_min_size(full.min, Vec2::new(full.width(), height(st.tab)));
    ui.allocate_rect(rect, Sense::hover());
    let mut pick = None;
    ui.painter().rect_filled(rect, 0.0, tk.ribbon);
    ui.painter().hline(rect.x_range(), rect.max.y - 0.5, Stroke::new(1.0, tk.ribbon_line));

    // badge
    let bx = rect.min.x + 8.0;
    let badge = Rect::from_min_size(Pos2::new(bx, rect.min.y + 6.0), Vec2::new(60.0, (height(st.tab) - 13.0).min(70.0)));
    let br = CornerRadius::same(tk.radius as u8);
    ui.painter().rect_filled(badge, br, tk.accent);
    ui.painter().rect_filled(Rect::from_min_max(badge.min + Vec2::new(0.0, 2.0), badge.max), CornerRadius { nw: 0, ne: 0, sw: br.sw, se: br.se }, tk.panel_2);
    let tab_name = TABS.iter().find(|x| x.0 == st.tab).map(|x| x.1).unwrap_or("Model");
    let bc = badge.center();
    let img = icons::icon(ui.ctx(), tab_icon(st.tab), 22.0, tk.accent);
    egui::Image::new(img).paint_at(ui, Rect::from_center_size(Pos2::new(bc.x, bc.y - 9.0), Vec2::splat(22.0)));
    ui.painter().text(Pos2::new(bc.x, bc.y + 13.0), Align2::CENTER_CENTER, tab_name, FontId::new(11.0, FontFamily::Name("bold".into())), tk.strong);

    // tabs
    let mut x = badge.max.x + 4.0 + 8.0;
    let ty = rect.min.y + 5.0;
    for (k, l) in TABS {
        let tf = FontId::new(11.5, FontFamily::Name("bold".into()));
        let tw = ui.painter().layout_no_wrap(l.to_owned(), tf.clone(), tk.text).size().x;
        let r = Rect::from_min_size(Pos2::new(x, ty), Vec2::new(tw + 24.0, 22.0));
        let resp = ui.interact(r, ui.scope_id().with(("rtab", k)), Sense::click());
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, l));
        let active = k == st.tab;
        let col = if active {
            ui.painter().rect_filled(r, CornerRadius::same(11), tk.sel);
            tk.sel_text
        } else if resp.hovered() {
            ui.painter().rect_filled(r, CornerRadius::same(11), tk.hover);
            tk.text
        } else {
            tk.muted
        };
        ui.painter().text(r.center(), Align2::CENTER_CENTER, l, tf, col);
        if resp.clicked() {
            pick = Some(Pick::Tab(k));
        }
        x += r.width() + 3.0;
    }

    // body
    let body = Rect::from_min_max(Pos2::new(badge.max.x + 4.0, rect.min.y + 25.0), Pos2::new(rect.max.x, rect.max.y));
    let avail = body.width() - 8.0;
    let fit = fit(ui, &d, avail);
    let mode = fit.mode;
    let top = body.min.y + 3.0;
    let mut gx = body.min.x + 4.0;
    let draw_group = |ui: &mut Ui, g: &Group, gi: Option<usize>, x0: f32, pick: &mut Option<Pick>, border_left: bool| -> f32 {
        let w = group_w(ui, mode, g);
        let pad = if mode == Mode::Normal { 6.0 } else { 3.0 };
        let gr = Rect::from_min_size(Pos2::new(x0, top), Vec2::new(w, items_h + 17.0));
        if border_left {
            ui.painter().vline(gr.min.x + 0.5, gr.y_range(), Stroke::new(1.0, tk.divider));
        } else {
            ui.painter().vline(gr.max.x - 0.5, gr.y_range(), Stroke::new(1.0, tk.divider));
        }
        let mut ix = x0 + pad + if border_left { 1.0 } else { 0.0 };
        // centre the items when the group label is wider
        let items_w: f32 = g.items.iter().map(|i| item_w(ui, mode, i)).sum::<f32>() + 2.0 * g.items.len().saturating_sub(1) as f32;
        ix += ((w - 1.0 - 2.0 * pad - items_w) / 2.0).max(0.0);
        for it in &g.items {
            let iw = item_w(ui, mode, it);
            match it {
                Item::Big(tool) => {
                    let r = Rect::from_min_size(Pos2::new(ix, top), Vec2::new(iw, items_h));
                    let on = st.active == Some(tool.key);
                    if paint_tool(ui, tk, r, ui.scope_id().with(("tool", tool.key)), tool, tool.icon, tool.label, true, false, mode, on, st.web).clicked() {
                        *pick = Some(Pick::Tool(tool.key));
                    }
                }
                Item::Stack(v) => {
                    for (n, tool) in v.iter().enumerate() {
                        let r = Rect::from_min_size(Pos2::new(ix, top + 18.0 * n as f32), Vec2::new(iw, 18.0));
                        let on = st.active == Some(tool.key);
                        if paint_tool(ui, tk, r, ui.scope_id().with(("tool", tool.key)), tool, tool.icon, tool.label, false, false, mode, on, st.web).clicked() {
                            *pick = Some(Pick::Tool(tool.key));
                        }
                    }
                }
                Item::Drop { id, label, items } => {
                    let r = Rect::from_min_size(Pos2::new(ix, top), Vec2::new(iw, items_h));
                    let last = st.drop_last.get(*id).and_then(|k| items.iter().find(|x| x.key == k)).unwrap_or(&items[0]);
                    let on = st.active.is_some_and(|a| items.iter().any(|x| x.key == a));
                    let tool = Tool { key: id, label, icon: last.icon };
                    if paint_tool(ui, tk, r, ui.scope_id().with(("drop", *id)), &tool, last.icon, label, true, true, mode, on, st.web).clicked() {
                        *pick = Some(Pick::Drop(id, r));
                    }
                }
            }
            ix += iw + 2.0;
        }
        // the group name
        let ly = top + items_h + 2.0;
        let lr = Rect::from_min_size(Pos2::new(x0 + 1.0, ly), Vec2::new(w - 2.0, 14.0));
        if let (true, Some(gi)) = (g.has_drop(), gi) {
            let resp = ui.interact(lr, ui.scope_id().with(("glabel", gi)), Sense::click());
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, format!("{} tools", g.label)));
            if resp.hovered() {
                ui.painter().rect_filled(lr, CornerRadius::same(4), tk.hover);
            }
            let col = if resp.hovered() { tk.text } else { tk.muted };
            let gl = ui.painter().layout_no_wrap(g.label.to_owned(), font(11.0), col);
            let tw = gl.size().x + 10.0;
            let px = lr.center().x - tw / 2.0;
            ui.painter().galley(Pos2::new(px, lr.min.y + 1.0), gl, col);
            let c = Pos2::new(px + tw - 3.0, lr.min.y + 6.5);
            ui.painter().line(vec![c + Vec2::new(-2.5, -1.5), c + Vec2::new(0.0, 1.5), c + Vec2::new(2.5, -1.5)], Stroke::new(1.0, col));
            if resp.clicked() {
                *pick = Some(Pick::GroupMenu(gi, lr));
            }
        } else {
            ui.painter().text(Pos2::new(lr.center().x, lr.min.y + 1.0), Align2::CENTER_TOP, g.label, font(11.0), tk.muted);
        }
        w
    };
    for (gi, g) in d.groups.iter().enumerate() {
        if fit.folded[gi] {
            continue;
        }
        gx += draw_group(ui, g, Some(gi), gx, &mut pick, false);
    }
    // the pinned group sits at the right edge
    let pw = group_w(ui, mode, &d.pinned);
    let px = body.max.x - 4.0 - pw;
    if fit.more {
        let mw = 34.0 + 6.0 + 1.0;
        let mx = px - mw;
        let mr = Rect::from_min_size(Pos2::new(mx, top), Vec2::new(mw, items_h + 17.0));
        ui.painter().vline(mr.min.x + 0.5, mr.y_range(), Stroke::new(1.0, tk.divider));
        let br = Rect::from_min_size(Pos2::new(mx + 4.0, top), Vec2::new(34.0, items_h));
        let mt = Tool { key: "more", label: "More", icon: "more" };
        let resp = paint_tool(ui, tk, br, ui.scope_id().with("more"), &mt, "more", "More tools", true, false, Mode::Tight, false, st.web);
        if resp.clicked() {
            pick = Some(Pick::More(br));
        }
    }
    draw_group(ui, &d.pinned, None, px, &mut pick, true);
    (pick, fit)
}

/// The tools of the groups folded into "More", as menu rows (headers and tools).
pub fn folded_groups(st_tab: &str, fit: &Fit) -> Vec<(&'static str, Vec<Tool>)> {
    let d = def(st_tab);
    d.groups.iter().enumerate().filter(|(i, _)| fit.folded.get(*i).copied().unwrap_or(false)).map(|(_, g)| (g.label, g.tools())).collect()
}
