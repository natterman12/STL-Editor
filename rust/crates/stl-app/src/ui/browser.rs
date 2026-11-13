//! Browser tree: body count, rename, colour, hide, and selection (1.x `#tree` / `#browser-count`).

use egui::{Color32, Context, RichText, Ui};

use crate::engine_link::{BodyView, Command, PlaneView, SketchView};
use crate::icons;
use crate::theme::Tokens;
use crate::ui::UiEvent;

/// Default swatch when a body has no colour (`app.js` pickColor fallback, not a CSS token).
const DEFAULT_BODY_RGB: [u8; 3] = [154, 167, 180]; // #9aa7b4

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
    pub param_count: usize,
}

fn parse_hex(s: &str) -> Option<[u8; 3]> {
    let h = s.strip_prefix('#')?;
    if h.len() != 6 {
        return None;
    }
    let n = u32::from_str_radix(h, 16).ok()?;
    Some([(n >> 16) as u8, (n >> 8) as u8, n as u8])
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
    selection: &[String],
    commands: &mut Vec<Command>,
    events: &mut Vec<UiEvent>,
) {
    let indent = 8.0 + 14.0 * 2.0;
    let solid = b.solid;

    ui.horizontal(|ui| {
        ui.add_space(indent);

        let (show_tip, hide_tip) = (format!("Show {}", b.name), format!("Hide {}", b.name));
        if eye_btn(ui, ctx, tokens, &format!("browser_eye_{}", b.id), b.hidden, &show_tip, &hide_tip) {
            commands.push(Command::BodyProps { id: b.id.clone(), name: None, color: None, hidden: Some(!b.hidden) });
        }

        let rgb = b.color.as_deref().and_then(parse_hex).unwrap_or(DEFAULT_BODY_RGB);
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
                let r = ui.add(
                    egui::Button::new(RichText::new(&b.name).color(if selected { tokens.sel_text } else { tokens.text }))
                        .fill(if selected { tokens.sel } else { Color32::TRANSPARENT })
                        .stroke(egui::Stroke::NONE),
                );
                ui.label(RichText::new(tag).color(tag_colour).size(10.5));
                r
            })
            .inner
            .on_hover_text(&b.name);
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
    let param_count = view.param_count;
    ui.horizontal(|ui| {
        ui.label(RichText::new("Browser").strong().color(tokens.strong).size(12.0));
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
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            ui.label(RichText::new(view.doc_name).strong().color(tokens.label).size(12.0));
        });
        ui.horizontal(|ui| {
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
            ui.label(RichText::new("Origin").color(tokens.label).strong());
        });
        ui.horizontal(|ui| {
            ui.add_space(8.0 + 14.0);
            ui.label(RichText::new("Bodies").strong().color(tokens.label));
            ui.label(RichText::new(view.bodies.len().to_string()).color(tokens.muted).size(10.5));
        });
        for b in view.bodies {
            let selected = view.selection.contains(&b.id);
            body_row(ui, ctx, tokens, state, b, selected, view.selection, commands, events);
        }

        if !planes.is_empty() {
            ui.horizontal(|ui| {
                ui.add_space(8.0 + 14.0);
                ui.label(RichText::new("Planes").strong().color(tokens.label));
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

        if param_count > 0 {
            ui.horizontal(|ui| {
                ui.add_space(8.0 + 14.0);
                let r = ui.add(
                    egui::Label::new(RichText::new("Parameters").strong().color(tokens.label)).sense(egui::Sense::click()),
                );
                ui.label(RichText::new(param_count.to_string()).color(tokens.muted).size(10.5));
                if r.clicked() {
                    state.open_params = true;
                }
            });
        }

        if !sketches.is_empty() {
            ui.horizontal(|ui| {
                ui.add_space(8.0 + 14.0);
                ui.label(RichText::new("Sketches").strong().color(tokens.label));
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
