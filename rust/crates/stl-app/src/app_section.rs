//! Section View (1.x `startSection`, `renderSection`, `applyCut`, `toggleCut`, `clearCut`), with
//! the drag-the-plane control. A child of `app.rs`; its panel is `ui/section_panel.rs`.
//!
//! The cut is view-only and not saved in the project. Its plane is X, Y or Z (1.x's Axis: the
//! distance is the coordinate along it) or a picked plane or flat face (the distance is along
//! its normal from where it was picked). Which half stays follows `viewer.js` `setSection`: the
//! half away from the camera at the moment the cut is set, swapped by Flip.

use super::*;
use crate::tools::fields::number;
use crate::ui::section_panel::{self, SectionAct, SectionView};
use crate::viewport::camera::{self as vc, V3};
use crate::viewport::plane_drag::{self, DragPlane};

/// `startSection`'s stand-in box for an empty scene.
const EMPTY_BOX: stl_kernel::Aabb = stl_kernel::Aabb {
    min: [-50.0, -50.0, -50.0],
    max: [50.0, 50.0, 50.0],
};
const AXES: [V3; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

/// Where the cut's plane comes from.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum CutPlane {
    /// X, Y or Z through the world origin: the distance is the coordinate.
    Axis(usize),
    /// A construction plane or a flat face, as picked.
    Picked { origin: V3, normal: V3, label: String },
}

/// `sectionState`: the plane, its distance (mm, or an `=` expression) and Flip.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Cut {
    plane: CutPlane,
    distance: serde_json::Value,
    flip: bool,
}

/// The Section View panel and the cut it edits.
pub(super) struct SectionState {
    cut: Option<Cut>,
    /// The cut shows (`sectionOn`). When the viewport's plane goes while there is a cut, that
    /// was a new or opened project (`set_section_enabled(false)`): the cut is removed (`clearCut`).
    on: bool,
    open: bool,
    hatch: bool,
    /// What Cancel puts back (`_sectionUndo`): the cut, on, and the viewport's plane as it was.
    undo: Option<(Option<Cut>, bool, Option<[f64; 4]>)>,
    msg: Option<String>,
    bufs: crate::tools::fields::Bufs,
}

impl Default for SectionState {
    fn default() -> Self {
        SectionState {
            cut: None,
            on: false,
            open: false,
            hatch: true,
            undo: None,
            msg: None,
            bufs: Default::default(),
        }
    }
}

impl StlApp {
    /// The ribbon's Section button and the tool search (`startSection`).
    pub fn open_section(&mut self) {
        self.close_tool(false);
        self.close_measure();
        if self.section.open {
            return;
        }
        let plane = self.viewport.section_plane();
        self.section.undo = Some((self.section.cut.clone(), self.section.on, plane));
        if self.section.cut.is_none() {
            // a new cut starts halfway up the parts on Z
            let b = self.section_box();
            self.section.cut = Some(Cut {
                plane: CutPlane::Axis(2),
                distance: number::num_value((b.min[2] + b.max[2]) / 2.0),
                flip: false,
            });
        }
        self.section.open = true;
        self.section.msg = None;
        self.shell.active_tool = Some("section".to_owned());
        self.shell.hint = section_panel::HINT.to_owned();
        self.viewport.set_viewcube_inset(crate::tools::panel::VIEWCUBE_INSET);
        self.apply_cut();
    }

    /// X, View › Section View and the view cube's Section button (`toggleCut`): no cut yet opens
    /// Section; otherwise the cut turns off and on.
    pub fn toggle_cut(&mut self) {
        if self.section.cut.is_none() {
            return self.open_section();
        }
        self.section.on = !self.section.on;
        self.viewport.set_section(self.section.on);
        self.shell.hint = if self.section.on {
            "Section view on (X turns it off)"
        } else {
            "Section view off (X turns it back on)"
        }
        .to_owned();
    }

    /// Remove the cut altogether (`clearCut`: Remove cut, or a new or opened project).
    pub fn clear_cut(&mut self) {
        self.section.cut = None;
        self.section.on = false;
        self.viewport.set_section_plane(None);
        self.viewport.set_section(false);
        self.viewport.set_section_hatch(false);
        // the next cut starts with Hatch on again
        self.section.hatch = true;
        if self.section.open {
            self.section.undo = None;
            self.close_section();
        }
    }

    /// The plane the drag control shows while the panel is open.
    pub(super) fn section_drag_plane(&self) -> Option<DragPlane> {
        if !self.section.open {
            return None;
        }
        self.cut_drag_plane()
    }

    /// The arrow was dragged to `d`; `done` when the drag ended. The cut follows at once.
    pub(super) fn section_dragged(&mut self, d: f64, _done: bool) {
        let Some(cut) = self.section.cut.as_mut() else { return };
        cut.distance = number::num_value(d);
        self.section.msg = None;
        self.apply_cut();
    }

    /// A click in the viewport while the panel is open: a plane or a flat face becomes the cut's
    /// plane, halfway through what it was picked on when it only touches it. True when used.
    pub(super) fn section_click(&mut self, pos: Pos2) -> bool {
        if !self.section.open {
            return false;
        }
        if self.viewport.drag_plane_hot() {
            return true; // the arrow's own click
        }
        let planes = self.viewport.base_planes_visible() || !self.planes.is_empty();
        let Some(hit) = self.viewport.pick(pos, true, planes) else {
            return true;
        };
        let picked = match &hit {
            crate::viewport::pick::Hit::Plane { id, .. } => {
                let ctx = make_ctx(
                    &self.bodies,
                    &self.planes,
                    &self.sketches,
                    &self.timeline_view.items,
                    &self.shell.selection,
                    self.shell.inch,
                );
                let v = serde_json::Value::from(id.clone());
                let frame = crate::tools::fields::plane::frame_of(&v, &ctx);
                let label = crate::tools::fields::plane::plane_label(Some(&v), &ctx);
                frame.map(|f| (f.origin, f.normal, label, self.section_box()))
            }
            crate::viewport::pick::Hit::Body { id, face, .. } => {
                let Some(b) = self.bodies.iter().find(|b| &b.id == id) else {
                    return true;
                };
                let Some(fi) = crate::tools::fields::plane::face_info(&b.mesh, *face as usize, false) else {
                    return true;
                };
                if !fi.planar {
                    self.section.msg = Some(section_panel::NOT_FLAT.to_owned());
                    return true;
                }
                let span = self.viewport.body_box(id).unwrap_or_else(|| self.section_box());
                Some((fi.origin, fi.normal, format!("Face of {}", b.name), span))
            }
        };
        let Some((origin, normal, label, span)) = picked else {
            return true;
        };
        let d = plane_drag::start_distance(origin, normal, &span);
        let flip = self.section.cut.as_ref().is_some_and(|c| c.flip);
        self.section.cut = Some(Cut {
            plane: CutPlane::Picked { origin, normal, label },
            distance: number::num_value(d),
            flip,
        });
        self.section.msg = None;
        self.apply_cut();
        true
    }

    /// Draw the panel (each frame).
    pub(super) fn section_frame(&mut self, ctx: &egui::Context, tk: &Tokens, ws: Rect) {
        // the drag control's distance box (Section's and Split's) in the current units
        self.viewport.set_inch(self.shell.inch);
        if self.section.cut.is_some() && self.viewport.section_plane().is_none() {
            // the viewport's cut was removed behind Section's back: New or Open (`clearCut`)
            self.clear_cut();
        }
        if !self.section.open {
            return;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) && ctx.memory(|m| m.focused().is_none()) {
            return self.section_cancel();
        }
        let Some(view) = self.section_view() else { return };
        match section_panel::show(ctx, tk, ws, &view, &mut self.section.bufs) {
            SectionAct::None => {}
            SectionAct::Axis(i) => self.section_axis(i),
            SectionAct::Distance(text) => {
                let v = number::commit_number(&text, true, self.shell.inch);
                if let Some(c) = self.section.cut.as_mut() {
                    c.distance = v;
                }
                // an expression that doesn't work says why, as the tool panel does
                self.section.msg = self
                    .section
                    .cut
                    .as_ref()
                    .and_then(|c| stl_engine::params::resolve_deep(&c.distance, &self.param_map()).err())
                    .map(|e| e.0);
                self.apply_cut();
            }
            SectionAct::Flip(f) => {
                if let Some(c) = self.section.cut.as_mut() {
                    c.flip = f;
                }
                self.apply_cut();
            }
            SectionAct::Hatch(h) => {
                self.section.hatch = h;
                self.apply_cut();
            }
            SectionAct::Remove => self.clear_cut(),
            SectionAct::Done => {
                self.section.undo = None;
                self.close_section();
            }
            SectionAct::Cancel => self.section_cancel(),
        }
    }

    /// Close the panel (another tool or mode opened, or Done); the cut stays.
    pub(super) fn close_section(&mut self) {
        if !self.section.open {
            return;
        }
        self.section.open = false;
        self.section.undo = None;
        self.section.msg = None;
        if self.shell.active_tool.as_deref() == Some("section") {
            self.shell.active_tool = None;
        }
        self.shell.hint.clear();
        self.viewport.set_viewcube_inset(0.0);
    }

    // ------------------------------------------------------------------ for the tests

    /// The Section View panel is open.
    pub fn section_open(&self) -> bool {
        self.section.open
    }

    /// There is a cut (on or off).
    pub fn section_has_cut(&self) -> bool {
        self.section.cut.is_some()
    }

    /// The cut's distance in mm.
    pub fn section_distance(&self) -> Option<f64> {
        self.cut_distance()
    }

    // ------------------------------------------------------------------ inside

    /// Cancel (`panelCancel`): the cut as it was when the panel opened.
    fn section_cancel(&mut self) {
        if let Some((cut, on, plane)) = self.section.undo.take() {
            self.section.cut = cut;
            self.section.on = on && self.section.cut.is_some();
            self.viewport.set_section_plane(plane);
            self.viewport.set_section(self.section.on);
            if self.section.cut.is_none() {
                self.viewport.set_section_plane(None);
                self.viewport.set_section_hatch(false);
            }
        }
        self.close_section();
    }

    /// An axis button: 1.x keeps the slider's place in the parts' box (`t`) on the new axis.
    fn section_axis(&mut self, i: usize) {
        let b = self.section_box();
        let t = match (self.section.cut.as_ref().map(|c| &c.plane), self.cut_distance()) {
            (Some(CutPlane::Axis(j)), Some(d)) if b.max[*j] > b.min[*j] => {
                ((d - b.min[*j]) / (b.max[*j] - b.min[*j])).clamp(0.0, 1.0)
            }
            _ => 0.5,
        };
        let flip = self.section.cut.as_ref().is_some_and(|c| c.flip);
        let d = b.min[i] + (b.max[i] - b.min[i]) * t;
        self.section.cut = Some(Cut {
            plane: CutPlane::Axis(i),
            distance: number::num_value(d),
            flip,
        });
        self.section.msg = None;
        self.apply_cut();
    }

    /// The scene's box (`sceneBox`), or the stand-in when it is empty.
    fn section_box(&self) -> stl_kernel::Aabb {
        self.viewport.scene_box().unwrap_or(EMPTY_BOX)
    }

    /// The cut's distance in mm (an expression worked out with the parameters).
    fn cut_distance(&self) -> Option<f64> {
        match &self.section.cut.as_ref()?.distance {
            serde_json::Value::Number(n) => n.as_f64(),
            serde_json::Value::String(e) => {
                stl_engine::params::evaluate(e.trim().trim_start_matches('='), &self.param_map()).ok()
            }
            _ => Some(0.0),
        }
    }

    fn cut_drag_plane(&self) -> Option<DragPlane> {
        let cut = self.section.cut.as_ref()?;
        let distance = self.cut_distance()?;
        let span = self.section_box();
        let (origin, normal) = match &cut.plane {
            CutPlane::Axis(i) => ([0.0; 3], AXES[*i]),
            CutPlane::Picked { origin, normal, .. } => (*origin, *normal),
        };
        Some(DragPlane {
            origin,
            normal,
            distance,
            span,
        })
    }

    /// `applyCut`: show the cut (a change turns it on), keeping the half away from the camera.
    fn apply_cut(&mut self) {
        let Some(p) = self.cut_drag_plane() else { return };
        let flip = self.section.cut.as_ref().is_some_and(|c| c.flip);
        let cam = self.viewport.camera();
        let n = vc::normalize(p.normal);
        let cam_side = vc::dot(vc::sub(cam.position, cam.target), n) > 0.0;
        let keep_front = if cam_side { flip } else { !flip };
        self.viewport
            .set_section_plane(Some(plane_drag::plane_eq(&p, keep_front)));
        self.viewport.set_section_hatch(self.section.hatch);
        self.section.on = true;
        self.viewport.set_section(true);
    }

    fn section_view(&self) -> Option<SectionView> {
        let cut = self.section.cut.as_ref()?;
        let inch = self.shell.inch;
        let expr = number::is_expr(&cut.distance);
        let tip = expr.then(|| {
            let s = cut.distance.as_str().unwrap_or("");
            number::expr_title(s, true, &self.param_map())
        });
        let (axis, picked) = match &cut.plane {
            CutPlane::Axis(i) => (Some(*i), None),
            CutPlane::Picked { label, .. } => (None, Some(label.clone())),
        };
        Some(SectionView {
            axis,
            picked,
            distance: number::shown_number(&cut.distance, true, inch),
            expr,
            tip,
            flip: cut.flip,
            hatch: self.section.hatch,
            inch,
            msg: self.section.msg.clone(),
        })
    }
}
