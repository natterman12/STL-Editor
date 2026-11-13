//! Timeline bar: features, rollback marker and navigation (1.x `#timeline-bar`).

use egui::{Context, Rect, RichText, Ui};

use crate::engine_link::{Command, TimelineItem, TimelineView};
use crate::icons;
use crate::theme::Tokens;
use crate::ui::UiEvent;

#[derive(Clone, Debug, Default)]
pub struct Timeline {
    drag_marker: bool,
    item_rects: Vec<Rect>,
    /// Set on item click; app copies into the status line (`app.js:2124`).
    pub status_message: Option<String>,
    /// Set on double-click; same as `UiEvent::OpenTool` with the feature id for edit in 4b.
    pub edit_feature: Option<String>,
}

impl Timeline {
    pub fn take_status(&mut self) -> Option<String> {
        self.status_message.take()
    }

    pub fn take_edit_feature(&mut self) -> Option<String> {
        self.edit_feature.take()
    }
}

fn feature_icon(kind: &str) -> &'static str {
    match kind {
        "shell" | "lattice" => "hollow",
        "import" => "import",
        "primitive" | "box" => "primitive",
        "plane" => "plane",
        "sketch" => "sketch",
        "extrude" | "loft" | "snapfit" => "extrude",
        "revolve" | "sweep" => "revolve",
        "hole" | "fittest" => "hole",
        "combine" => "combine",
        "split" | "hinge" => "split",
        "mirror" => "mirror",
        "move" => "move",
        "scale" => "scale",
        "hollow" => "hollow",
        "repair" => "repair",
        "simplify" => "simplify",
        "pattern" => "pattern",
        "delete" => "delete",
        "separate" => "separate",
        "bedsplit" | "plate" => "bedsplit",
        "pushpull" => "pushpull",
        "sculpt" => "sculpt",
        "remesh" => "remesh",
        "layflat" => "layflat",
        "thread" => "thread",
        "offset" => "offset",
        "resizehole" => "resizehole",
        "supportfins" => "fins",
        "supportwall" => "finwall",
        "thincheck" => "measure",
        _ => "primitive",
    }
}

fn marker_at_end(marker: Option<usize>, len: usize) -> bool {
    marker.map_or(true, |m| m >= len)
}

fn icon_nav(ui: &mut Ui, ctx: &Context, tokens: &Tokens, salt: &str, tip: &str, ic: &str, enabled: bool) -> bool {
    // `push_id(..).response` is the scope's own (hover-only) response; the button is `.inner`.
    let response = ui
        .push_id(salt, |ui| {
            ui.add_enabled(
                enabled,
                egui::Button::image(icons::icon(ctx, ic, 15.0, tokens.text)).min_size(egui::vec2(24.0, 24.0)),
            )
        })
        .inner;
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, tip));
    enabled && response.on_hover_text(tip).clicked()
}

fn index_at_x(rects: &[Rect], x: f32, len: usize) -> usize {
    for (i, r) in rects.iter().enumerate() {
        if x < r.center().x {
            return i;
        }
    }
    len
}

/// Draw the timeline strip (not the status line).
pub fn show(
    ui: &mut Ui,
    ctx: &Context,
    tokens: &Tokens,
    state: &mut Timeline,
    view: &TimelineView,
    tool_open: bool,
    sketch_active: bool,
    commands: &mut Vec<Command>,
    events: &mut Vec<UiEvent>,
) {
    state.item_rects.clear();
    let nav_locked = tool_open || sketch_active;

    ui.horizontal(|ui| {
        ui.label(RichText::new("History").strong().color(tokens.strong).size(12.0));
        ui.add_space(6.0);
        ui.separator();
        ui.add_space(4.0);

        let len = view.items.len();
        let r = view.marker.unwrap_or(len);

        if icon_nav(ui, ctx, tokens, "tl_start", "Roll back to start", "tl_start", !nav_locked) {
            commands.push(Command::Rollback(Some(0)));
        }
        if icon_nav(ui, ctx, tokens, "tl_prev", "Step back", "tl_prev", !nav_locked) {
            commands.push(Command::Rollback(Some(r.saturating_sub(1).min(len))));
        }
        if icon_nav(ui, ctx, tokens, "tl_next", "Step forward", "tl_next", !nav_locked && view.marker.is_some()) {
            if let Some(m) = view.marker {
                let next = if m + 1 >= len { None } else { Some(m + 1) };
                commands.push(Command::Rollback(next));
            }
        }
        if icon_nav(ui, ctx, tokens, "tl_end", "Roll forward to end", "tl_end", !nav_locked && view.marker.is_some()) {
            commands.push(Command::Rollback(None));
        }

        ui.add_space(4.0);

        egui::ScrollArea::horizontal().show(ui, |ui| {
            ui.horizontal(|ui| {
                for (i, item) in view.items.iter().enumerate() {
                    if view.marker == Some(i) {
                        draw_marker(ui, tokens, state, len, commands, false);
                    }
                    let rect = draw_item(
                        ui,
                        ctx,
                        tokens,
                        item,
                        i,
                        len,
                        view.marker,
                        item.active,
                        nav_locked,
                        commands,
                        events,
                        state,
                    );
                    state.item_rects.push(rect);
                }
                if marker_at_end(view.marker, len) {
                    draw_marker(ui, tokens, state, len, commands, true);
                }
            });
        });
    });
}

fn draw_marker(ui: &mut Ui, tokens: &Tokens, state: &mut Timeline, len: usize, commands: &mut Vec<Command>, at_end: bool) {
    // Same tooltips as 1.x (app.js:569 and :575). The accessible label stays "Rollback marker".
    let (rect, response) = ui.allocate_exact_size(egui::vec2(10.0, 24.0), egui::Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    let x = rect.center().x;
    painter.line_segment(
        [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
        egui::Stroke::new(2.0, tokens.accent),
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "Rollback marker"));
    if response.drag_started() {
        state.drag_marker = true;
    }
    if response.drag_stopped() && state.drag_marker {
        if let Some(pos) = ui.input(|i| i.pointer.interact_pos()) {
            let ix = index_at_x(&state.item_rects, pos.x, len);
            commands.push(Command::Rollback(if ix >= len { None } else { Some(ix) }));
        }
        state.drag_marker = false;
    }
    response.on_hover_text(if at_end { "End of history" } else { "Rollback marker" });
}

fn draw_item(
    ui: &mut Ui,
    ctx: &Context,
    tokens: &Tokens,
    item: &TimelineItem,
    index: usize,
    len: usize,
    marker: Option<usize>,
    active: bool,
    nav_locked: bool,
    commands: &mut Vec<Command>,
    events: &mut Vec<UiEvent>,
    state: &mut Timeline,
) -> Rect {
    // `.tl-item` (app.css:346-357): err/warn badge, suppressed .4, inactive .32.
    let err = item.error.is_some();
    let dim = (if item.suppressed { 0.4 } else { 1.0 }) * (if active { 1.0 } else { 0.32 });
    let bg = if err { tokens.err_bg } else { tokens.panel_2 }.gamma_multiply(dim);
    let ic = feature_icon(&item.kind);
    // The same text as 1.x's tooltip (app.js:568-571), minus the timing line.
    let mut tip = item.label.clone();
    if let Some(e) = &item.error {
        tip.push_str(&format!("\n⚠ {e}"));
    }
    if let Some(w) = &item.warning {
        tip.push_str(&format!("\n{w}"));
    }
    tip.push_str("\nDouble-click to edit · right-click for more");

    let response = ui
        .push_id(format!("tl_item_{}", item.id), |ui| {
            ui.add(
                egui::Button::image(icons::icon(ctx, ic, 16.0, tokens.text.gamma_multiply(dim)))
                    .fill(bg)
                    .min_size(egui::vec2(24.0, 24.0)),
            )
        })
        .inner
        .on_hover_text(tip);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &item.label));

    if item.suppressed {
        let r = response.rect.shrink(4.0);
        ui.painter().line_segment([r.left_center(), r.right_center()], egui::Stroke::new(1.0, tokens.text.gamma_multiply(dim)));
    }
    if err || item.warning.is_some() {
        let c = if err { tokens.danger } else { tokens.warn };
        ui.painter().text(
            response.rect.right_top() + egui::vec2(-1.0, -3.0),
            egui::Align2::RIGHT_TOP,
            "!",
            egui::FontId::proportional(10.0),
            c,
        );
    }

    if response.clicked() {
        let msg = if let Some(e) = &item.error {
            format!("{} — {}", item.label, e)
        } else {
            item.label.clone()
        };
        state.status_message = Some(msg);
    }
    if response.double_clicked() {
        state.edit_feature = Some(item.id.clone());
        events.push(UiEvent::OpenTool(item.kind.clone()));
    }

    response.context_menu(|ui| {
        if ui
            .add_enabled(!nav_locked, egui::Button::new(if item.kind == "import" { "Position / Repair…" } else { "Edit…" }))
            .clicked()
        {
            events.push(UiEvent::OpenTool(item.kind.clone()));
            ui.close();
        }
        if ui
            .add_enabled(!nav_locked, egui::Button::new("Delete"))
            .clicked()
        {
            commands.push(Command::DeleteFeature { id: item.id.clone() });
            ui.close();
        }
        // Same label and action as 1.x (app.js:2598): flips the feature's suppressed flag.
        let supp_label = if item.suppressed { "Unsuppress" } else { "Suppress" };
        if ui
            .add_enabled(!nav_locked, egui::Button::new(supp_label))
            .clicked()
        {
            commands.push(Command::SetSuppressed { id: item.id.clone(), suppressed: !item.suppressed });
            ui.close();
        }
        ui.separator();
        if ui
            .add_enabled(!nav_locked && index > 0, egui::Button::new("Move left"))
            .clicked()
        {
            commands.push(Command::MoveFeature { id: item.id.clone(), index: index - 1 });
            ui.close();
        }
        if ui
            .add_enabled(!nav_locked && index + 1 < len, egui::Button::new("Move right"))
            .clicked()
        {
            commands.push(Command::MoveFeature { id: item.id.clone(), index: index + 1 });
            ui.close();
        }
        ui.separator();
        if ui
            .add_enabled(!nav_locked, egui::Button::new("Roll Back to Here"))
            .clicked()
        {
            commands.push(Command::Rollback(Some(index)));
            ui.close();
        }
        if ui
            .add_enabled(!nav_locked && marker.is_some(), egui::Button::new("Roll Forward to End"))
            .clicked()
        {
            commands.push(Command::Rollback(None));
            ui.close();
        }
    });

    response.rect
}
