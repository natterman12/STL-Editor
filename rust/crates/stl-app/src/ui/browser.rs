//! Browser tree: body count, rename, colour, hide, and selection (1.x `#tree` / `#browser-count`).

use egui::{Color32, Context, RichText, Ui};

use crate::engine_link::{BodyView, Command, ParamView, PlaneView, SketchView};
use crate::icons;
use crate::theme::Tokens;
use crate::ui::UiEvent;

/// The swatch colour of the body at `index`: exactly what the viewport draws for it. That is the
/// user's colour, else the feature's (`BodyView.color` carries whichever the engine resolved), else
/// the default body colour for that position; a body that isn't watertight is drawn `NOT_SOLID` pink.
pub fn swatch_rgb(b: &BodyView, index: usize) -> [u8; 3] {
    crate::viewport::body_color(b, index)
}

/// 1.x's `num()` (app.js:33): rounded to three decimals, no trailing zeros.
pub fn fmt_num(v: f64) -> String {
    format!("{}", (v * 1000.0).round() / 1000.0)
}

/// The text of a Parameters row in the tree: `dx = 5`, or `dx = ?` when the value has an error.
pub fn param_row_text(q: &ParamView) -> String {
    match (&q.error, q.value) {
        (Some(_), _) | (None, None) => format!("{} = ?", q.name),
        (None, Some(v)) => format!("{} = {}", q.name, fmt_num(v)),
    }
}

/// Local UI state for rename and colour pickers.
#[derive(Clone, Debug, Default)]
pub struct Browser {
    rename_id: Option<String>,
    rename_text: String,
    colour_id: Option<String>,
    colour: [u8; 3],
    /// Toggled by the Origin eye; app turns base planes on/off in the viewport (`app.js:2435`).
    pub origin_toggle: Option<bool>,
    /// Clicking the Parameters folder (app opens the params panel).
    pub open_params: bool,
    /// Narrow windows only: the browser drawer is open (1.x `html.browser-open`). Starts closed;
    /// it keeps its value if the window is widened and narrowed again, as in 1.x.
    pub drawer_open: bool,
}

impl Browser {
    pub fn with_rename(id: impl Into<String>, text: impl Into<String>) -> Self {
        Self { rename_id: Some(id.into()), rename_text: text.into(), ..Default::default() }
    }

    pub fn take_origin_toggle(&mut self) -> Option<bool> {
        self.origin_toggle.take()
    }

    pub fn take_open_params(&mut self) -> bool {
        std::mem::take(&mut self.open_params)
    }
}

/// What to draw this frame.
pub struct BrowserView<'a> {
    pub doc_name: &'a str,
    pub bodies: &'a [BodyView],
    pub selection: &'a [String],
    pub origin_visible: bool,
    pub planes: &'a [PlaneView],
    pub sketches: &'a [SketchView],
    /// Listed under the Parameters folder, always expanded (1.x `renderTree`).
    pub params: &'a [ParamView],
}

fn fmt_triangles(n: usize) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{}k", (n as f64 / 1_000.0).round() as u32)
    } else {
        n.to_string()
    }
}

/// `.panel-title` and `.tnode.folder` are `font-weight: 600`. egui's `strong()` only
/// changes the colour, so the weight is the bold family (Selawik Bold stands in for Semibold).
fn bold(text: impl Into<String>, color: Color32, size: f32) -> RichText {
    RichText::new(text).color(color).size(size).font(egui::FontId::new(size, egui::FontFamily::Name("bold".into())))
}

/// `.tnode .nm { flex: 1 }` pushes `.tag` to the right, 8 px in from the row's edge.
fn row_tag(ui: &mut Ui, text: &str, color: Color32, bad: bool) {
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.add_space(8.0);
        let label = if bad { bold(text, color, 10.5) } else { RichText::new(text).color(color).size(10.5) };
        ui.label(label);
    });
}

fn eye_btn(ui: &mut Ui, ctx: &Context, tokens: &Tokens, salt: &str, hidden: bool, tip_show: &str, tip_hide: &str) -> bool {
    let eye_name = if hidden { "eye_off" } else { "eye" };
    let tip = if hidden { tip_show } else { tip_hide };
    // `push_id(..).response` is the scope's own hover-only response; the button is `.inner`.
    let response = ui
        .push_id(salt, |ui| {
            ui.add(egui::Button::image(icons::icon(ctx, eye_name, 15.0, tokens.muted)).frame(false))
        })
        .inner;
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, tip));
    response.on_hover_text(tip).clicked()
}

fn body_row(
    ui: &mut Ui,
    ctx: &Context,
    tokens: &Tokens,
    state: &mut Browser,
    b: &BodyView,
    selected: bool,
    index: usize,
    selection: &[String],
    commands: &mut Vec<Command>,
    events: &mut Vec<UiEvent>,
) {
    let indent = 8.0 + 14.0 * 2.0;
    let solid = b.solid;
    // `.tnode.selected` paints the whole row, not a chip behind the name.
    let width = ui.available_width();
    egui::Frame::new().fill(if selected { tokens.sel } else { Color32::TRANSPARENT }).show(ui, |ui| {
        ui.set_width(width);
        ui.spacing_mut().button_padding = egui::vec2(0.0, 0.0);
        ui.horizontal(|ui| {
            ui.set_min_width(width);
            // padding-left is the indent; the 6 px is `.tnode { gap: 6px }` before the eye
            ui.spacing_mut().item_spacing.x = 6.0;
            ui.add_space(if indent > 6.0 { indent - 6.0 } else { 0.0 });

        let (show_tip, hide_tip) = (format!("Show {}", b.name), format!("Hide {}", b.name));
        if eye_btn(ui, ctx, tokens, &format!("browser_eye_{}", b.id), b.hidden, &show_tip, &hide_tip) {
            commands.push(Command::BodyProps { id: b.id.clone(), name: None, color: None, hidden: Some(!b.hidden) });
        }

        let rgb = swatch_rgb(b, index);
        let swatch = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
        let sw_resp = ui
            .push_id(format!("browser_swatch_{}", b.id), |ui| {
                // a flat 11 px square (app.css:236); the 3 px corner is the radius 1.x uses there
                let (rect, resp) = ui.allocate_exact_size(egui::vec2(11.0, 11.0), egui::Sense::click());
                ui.painter().rect_filled(rect, 3.0, swatch);
                ui.painter().rect_stroke(
                    rect,
                    3.0,
                    egui::Stroke::new(1.0, Color32::from_black_alpha(64)), // rgba(0,0,0,.25), app.css:236
                    egui::StrokeKind::Inside,
                );
                resp
            })
            .inner
            .on_hover_text("Colour");
        sw_resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, format!("Colour of {}", b.name)));
        if sw_resp.clicked() {
            state.colour_id = Some(b.id.clone());
            state.colour = rgb;
        }

        let tag = if solid {
            fmt_triangles(b.mesh.triangles.len() / 3)
        } else {
            "needs repair".to_string()
        };
        let tag_colour = if solid { tokens.muted } else { tokens.warn };
        let response = ui
            .push_id(format!("browser_body_{}", b.id), |ui| {
                ui.add(
                    egui::Button::new(RichText::new(&b.name).color(if selected { tokens.sel_text } else { tokens.text }))
                        .fill(Color32::TRANSPARENT)
                        .stroke(egui::Stroke::NONE)
                        .frame(false),
                )
            })
            .inner
            .on_hover_text(&b.name);
        // 1.x `.tnode .tag` keeps `--muted`, and `.tag.bad` keeps `--warn`, on a selected row.
        row_tag(ui, &tag, tag_colour, !solid);
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &b.name));

        if response.clicked() {
            let multi = ui.input(|i| i.modifiers.shift || i.modifiers.command || i.modifiers.ctrl);
            let next = if multi {
                if selection.contains(&b.id) {
                    selection.iter().filter(|x| *x != &b.id).cloned().collect()
                } else {
                    let mut v = selection.to_vec();
                    v.push(b.id.clone());
                    v
                }
            } else {
                vec![b.id.clone()]
            };
            events.push(UiEvent::Select(next));
        }
        if response.double_clicked() {
            state.rename_id = Some(b.id.clone());
            state.rename_text = b.name.clone();
        }
        response.context_menu(|ui| {
            if ui.button("Rename…").clicked() {
                state.rename_id = Some(b.id.clone());
                state.rename_text = b.name.clone();
                ui.close();
            }
            let hide_label = if b.hidden { "Show" } else { "Hide" };
            if ui.button(hide_label).clicked() {
                commands.push(Command::BodyProps {
                    id: b.id.clone(),
                    name: None,
                    color: None,
                    hidden: Some(!b.hidden),
                });
                ui.close();
            }
            if ui.button("Colour…").clicked() {
                state.colour_id = Some(b.id.clone());
                state.colour = rgb;
                ui.close();
            }
        });
        });
    });
}

/// Draw the browser panel; append commands and `UiEvent`s to the vectors.
pub fn show(
    ui: &mut Ui,
    ctx: &Context,
    tokens: &Tokens,
    state: &mut Browser,
    view: BrowserView<'_>,
    commands: &mut Vec<Command>,
    events: &mut Vec<UiEvent>,
) {
    let planes = view.planes;
    let sketches = view.sketches;
    let params = view.params;
    ui.horizontal(|ui| {
        ui.label(bold("Browser", tokens.strong, 12.0));
        let n = view.bodies.len();
        let count = if n == 0 {
            String::new()
        } else {
            format!("{n} bod{}", if n > 1 { "ies" } else { "y" })
        };
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.push_id("browser_count", |ui| ui.label(RichText::new(count).color(tokens.muted).size(11.0)));
        });
    });
    ui.add_space(4.0);

    egui::ScrollArea::vertical().show(ui, |ui| {
        let row_w = ui.available_width();
        ui.horizontal(|ui| {
            ui.set_min_width(row_w);
            ui.add_space(8.0);
            ui.label(bold(view.doc_name, tokens.label, 12.0));
        });
        ui.horizontal(|ui| {
            ui.set_min_width(row_w);
            ui.add_space(8.0);
            if eye_btn(
                ui,
                ctx,
                tokens,
                "browser_eye_origin",
                !view.origin_visible,
                "Show origin planes",
                "Hide origin planes",
            ) {
                state.origin_toggle = Some(!view.origin_visible);
            }
            ui.label(bold("Origin", tokens.label, 12.0));
        });
        ui.horizontal(|ui| {
            ui.set_min_width(row_w);
            ui.add_space(8.0 + 14.0);
            ui.label(bold("Bodies", tokens.label, 12.0));
            row_tag(ui, &view.bodies.len().to_string(), tokens.muted, false);
        });
        for (index, b) in view.bodies.iter().enumerate() {
            let selected = view.selection.contains(&b.id);
            body_row(ui, ctx, tokens, state, b, selected, index, view.selection, commands, events);
        }

        if !planes.is_empty() {
            ui.horizontal(|ui| {
                ui.set_min_width(row_w);
                ui.add_space(8.0 + 14.0);
                ui.label(bold("Planes", tokens.label, 12.0));
            });
            for p in planes {
                ui.horizontal(|ui| {
                    ui.add_space(8.0 + 14.0 * 2.0);
                    let hidden = !p.visible;
                    if eye_btn(ui, ctx, tokens, &format!("browser_eye_plane_{}", p.id), hidden, &format!("Show {}", p.name), &format!("Hide {}", p.name)) {
                        commands.push(Command::BodyProps {
                            id: p.id.clone(),
                            name: None,
                            color: None,
                            hidden: Some(!hidden),
                        });
                    }
                    ui.label(RichText::new(&p.name).color(tokens.text));
                });
            }
        }

        if !params.is_empty() {
            ui.horizontal(|ui| {
                ui.set_min_width(row_w);
                ui.add_space(8.0 + 14.0);
                let r = ui.add(egui::Label::new(bold("Parameters", tokens.label, 12.0)).sense(egui::Sense::click()));
                row_tag(ui, &params.len().to_string(), tokens.muted, false);
                if r.clicked() {
                    state.open_params = true;
                }
            });
            // always expanded, `name = value` (1.x lists them under the folder); a click opens the table
            for q in params {
                ui.horizontal(|ui| {
                    ui.add_space(8.0 + 14.0 * 2.0);
                    let colour = if q.error.is_some() { tokens.danger } else { tokens.text };
                    let r = ui.push_id(format!("browser_param_{}", q.name), |ui| {
                        ui.add(egui::Label::new(RichText::new(param_row_text(q)).color(colour)).sense(egui::Sense::click()))
                    });
                    let tip = q.error.as_deref().unwrap_or(q.note.as_str());
                    let r = r.inner;
                    if !tip.is_empty() {
                        r.clone().on_hover_text(tip);
                    }
                    if r.clicked() {
                        state.open_params = true;
                    }
                });
            }
        }

        if !sketches.is_empty() {
            ui.horizontal(|ui| {
                ui.set_min_width(row_w);
                ui.add_space(8.0 + 14.0);
                ui.label(bold("Sketches", tokens.label, 12.0));
            });
            for s in sketches {
                ui.horizontal(|ui| {
                    ui.add_space(8.0 + 14.0 * 2.0);
                    if eye_btn(ui, ctx, tokens, &format!("browser_eye_sketch_{}", s.id), s.hidden, &format!("Show {}", s.name), &format!("Hide {}", s.name)) {
                        commands.push(Command::BodyProps {
                            id: s.id.clone(),
                            name: None,
                            color: None,
                            hidden: Some(!s.hidden),
                        });
                    }
                    ui.label(RichText::new(&s.name).color(tokens.text));
                    // `<span class="tag">${k.entities.length}</span>`: how many shapes it holds
                    row_tag(ui, &s.outlines.len().to_string(), tokens.muted, false);
                });
            }
        }
    });

    if let Some(id) = state.rename_id.clone() {
        let mut open = true;
        egui::Window::new("Rename body")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                let te = ui.add(egui::TextEdit::singleline(&mut state.rename_text).desired_width(200.0));
                te.widget_info(|| egui::WidgetInfo::labeled(egui::Role::TextInput, true, "Body name"));
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        state.rename_id = None;
                    }
                    if ui.button("OK").clicked() {
                        let name = state.rename_text.trim().to_string();
                        if !name.is_empty() {
                            commands.push(Command::BodyProps {
                                id: id.clone(),
                                name: Some(name),
                                color: None,
                                hidden: None,
                            });
                        }
                        state.rename_id = None;
                    }
                });
            });
        if !open {
            state.rename_id = None;
        }
    }

    if let Some(id) = state.colour_id.clone() {
        let mut srgb = state.colour;
        let mut keep = true;
        egui::Window::new("Colour")
            .collapsible(false)
            .resizable(false)
            .open(&mut keep)
            .show(ctx, |ui| {
                if ui.color_edit_button_srgb(&mut srgb).changed() {
                    state.colour = srgb;
                }
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        state.colour_id = None;
                    }
                    if ui.button("OK").clicked() {
                        let hex = format!("#{:02x}{:02x}{:02x}", srgb[0], srgb[1], srgb[2]);
                        commands.push(Command::BodyProps {
                            id: id.clone(),
                            name: None,
                            color: Some(hex),
                            hidden: None,
                        });
                        state.colour_id = None;
                    }
                });
            });
        if !keep {
            state.colour_id = None;
        }
    }
}

/// `@media (max-width: 760px), (max-height: 520px)` in 1.x's `app.css`: below these the browser
/// is a drawer. Phones in portrait and landscape; tablets keep the floating panel.
pub const NARROW_MAX_WIDTH: f32 = 760.0;
pub const NARROW_MAX_HEIGHT: f32 = 520.0;
/// `#browser { width: min(280px, 85vw) }` on a narrow window.
const DRAWER_MAX_WIDTH: f32 = 280.0;
/// `transition: transform .18s ease`.
const DRAWER_SECS: f32 = 0.18;
/// `.panel` padding (9 px each side) and border (1 px each side) around the content.
const PANEL_CHROME_X: f32 = 20.0;

/// CSS `ease`, `cubic-bezier(.25, .1, .25, 1)`: the progress (0..=1) for a linear time `t` (0..=1).
pub fn css_ease(t: f32) -> f32 {
    const BEZIER: (f32, f32, f32, f32) = (0.25, 0.1, 0.25, 1.0);
    let (x1, y1, x2, y2) = BEZIER;
    let coord = |u: f32, a: f32, b: f32| 3.0 * (1.0 - u).powi(2) * u * a + 3.0 * (1.0 - u) * u * u * b + u * u * u;
    let t = t.clamp(0.0, 1.0);
    // solve x(u) = t by bisection; x is monotonic for these control points
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    for _ in 0..24 {
        let mid = 0.5 * (lo + hi);
        if coord(mid, x1, x2) < t {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    coord(0.5 * (lo + hi), y1, y2)
}

/// Is a window of this size (points) narrow enough for the drawer?
pub fn is_narrow(window: egui::Vec2) -> bool {
    window.x <= NARROW_MAX_WIDTH || window.y <= NARROW_MAX_HEIGHT
}

/// The drawer's outer width: `min(280px, 85vw)`.
pub fn drawer_outer_width(window_width: f32) -> f32 {
    DRAWER_MAX_WIDTH.min(window_width * 0.85)
}

/// The browser over the model (1.x `#browser`, and `#browser-toggle` on narrow windows).
///
/// Wide windows: the floating panel, 240 wide, 8 px from the top left of the viewport `ws`.
/// Narrow windows: a drawer that slides in from the left over everything else, opened by the
/// button over the model (labelled "Bodies", "Close" while open) as 1.x's `setupDrawer`. The button
/// is the only way to close it, as in 1.x.
pub fn show_panel(
    ctx: &Context,
    tokens: &Tokens,
    ws: egui::Rect,
    state: &mut Browser,
    view: BrowserView<'_>,
    commands: &mut Vec<Command>,
    events: &mut Vec<UiEvent>,
) {
    use egui::{Area, Frame, Id, Margin, Order, Pos2, Stroke, Vec2};
    let frame = || {
        Frame::new()
            .fill(tokens.panel)
            .stroke(Stroke::new(1.0, tokens.line))
            .corner_radius(egui::CornerRadius::same(tokens.radius as u8))
            .shadow(tokens.shadow)
    };
    let max_h = (ws.height() - 60.0).max(40.0);
    let window = ctx.content_rect().size();

    if !is_narrow(window) {
        // 240 wide in all, like `#browser` (220 + the 9 px margins + the 1 px borders). Not inside a
        // `ui.horizontal`: that gives its content one row of height and clips the tree.
        Area::new(Id::unique("browser-panel")).order(Order::Middle).fixed_pos(ws.min + Vec2::new(8.0, 8.0)).show(ctx, |ui| {
            frame().inner_margin(Margin::symmetric(9, 6)).show(ui, |ui| {
                ui.set_width(220.0);
                ui.set_max_height(max_h);
                show(ui, ctx, tokens, state, view, commands, events);
            });
        });
        return;
    }

    let outer = drawer_outer_width(window.x);
    // the button sits at the top left; when open it moves to the drawer's right edge
    let toggle_x = if state.drawer_open { outer + 16.0 } else { 8.0 };
    let t = ctx.animate_bool_with_time(Id::unique("browser-drawer-slide"), state.drawer_open, DRAWER_SECS);
    let eased = css_ease(t);
    if t > 0.0 {
        // closed it sits `-100% - 16px` to the left, off the window; the slide is CSS `ease`
        let x = ws.min.x + 8.0 - (1.0 - eased) * (outer + 16.0);
        Area::new(Id::unique("browser-panel")).order(Order::Foreground).fixed_pos(Pos2::new(x, ws.min.y + 8.0)).show(ctx, |ui| {
            frame().inner_margin(Margin::symmetric(9, 6)).show(ui, |ui| {
                ui.set_width(outer - PANEL_CHROME_X);
                ui.set_max_height(max_h);
                show(ui, ctx, tokens, state, view, commands, events);
            });
        });
    }
    Area::new(Id::unique("browser-toggle")).order(Order::Foreground).fixed_pos(Pos2::new(ws.min.x + toggle_x, ws.min.y + 8.0)).show(ctx, |ui| {
        // `#browser-toggle`: 12 px text, 5 px 10 px padding, panel fill, line border, shadow. Foreground, not
        // 1.x's z-index 6: 2.0's tool panel is not a bottom sheet on phones yet and would cover it.
        frame().show(ui, |ui| {
            ui.spacing_mut().button_padding = egui::vec2(10.0, 5.0);
            let label = if state.drawer_open { "Close" } else { "Bodies" };
            let text = RichText::new(label).size(12.0).color(tokens.text);
            let button = egui::Button::new(text)
                .fill(tokens.panel)
                .stroke(Stroke::NONE)
                .corner_radius(egui::CornerRadius::same(tokens.radius as u8));
            if ui.add(button).clicked() {
                state.drawer_open = !state.drawer_open;
            }
        });
    });
}
