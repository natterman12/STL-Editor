//! The 3D view, matching 1.x `web/js/viewer.js`. The only module that uses wgpu.
//!
//! The UI thread keeps the scene (bodies, highlights, overlays, camera); each frame [`Viewport::show`]
//! hands a [`renderer::FrameData`] to an egui-wgpu paint callback. Body meshes are prepared for
//! the GPU (creased normals, feature edges, box) off the UI thread and uploaded once per
//! revision; the same worker then builds the revision's picking tree.
//!
//! A body has the revision on screen (on the GPU) and maybe a newer one still uploading.
//! Picks, face highlights and the scene box use the one on screen, so a highlight always
//! matches the triangles drawn. A body with nothing on screen yet uses its newest prepared
//! revision (without a GPU, a prepared revision counts as shown at once).

pub mod camera;
pub mod overlays;
pub mod pick;
pub mod renderer;
pub mod scene;
pub mod section;
pub mod viewcube;

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, OnceLock};

use egui::{Color32, Pos2, Rect};
use stl_kernel::{Aabb, MeshArrays};
use stl_mesh::bvh::Bvh;

use crate::engine_link::{BodyView, PlaneView, SketchView};
use camera::{Camera, DragAction, V3};
use overlays::{Depth, OverlayItem, SketchOutline, Stroke};
use renderer::{BodyDraw, BodyUniform, FlatBatch, FrameData, Globals, GpuState, Stats};
use scene::{FlatVertex, Frame, PreparedMesh};

/// Which mouse buttons do what.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum MouseMode {
    #[default]
    Model,
    Sketch,
    Sculpt,
}

/// A plane's origin and axes, in millimetres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlaneFrame {
    pub origin: [f64; 3],
    pub x: [f64; 3],
    pub y: [f64; 3],
    pub normal: [f64; 3],
}

impl PlaneFrame {
    pub fn as_frame(&self) -> Frame {
        [self.origin, self.x, self.y, self.normal]
    }
}

pub struct ViewportResponse {
    pub response: egui::Response,
    /// A view-cube button clicked this frame (`"top"`, ..., `"fit"`); the view is already set.
    pub view: Option<&'static str>,
    /// The Section button was clicked (the app decides what that does, as 1.x's app.js).
    pub section_clicked: bool,
}

/// A mesh of a preview (`showPreview`'s `pack.meshes[i]`).
#[derive(Clone, Debug)]
pub struct PreviewMesh {
    pub id: String,
    pub mesh: Arc<MeshArrays>,
}

/// `showPreview(pack, { hide })`.
#[derive(Clone, Debug, Default)]
pub struct PreviewPack {
    pub meshes: Vec<PreviewMesh>,
    /// Bodies the result removes.
    pub removed: Vec<String>,
    /// Bodies to hide while the preview shows.
    pub hide: Vec<String>,
    /// `pack.result.colors`: a tint per mesh id.
    pub colors: HashMap<String, String>,
    /// `pack.result.ghosts`: see-through meshes (a fin you removed).
    pub ghosts: Vec<String>,
}

struct PreviewEntry {
    id: String,
    mesh: Arc<MeshArrays>,
    prepared: Arc<PreparedMesh>,
    /// Built with the preview (on the UI thread, like its display mesh).
    bvh: Arc<Bvh>,
    revision: u64,
    color: [f32; 3],
    emissive: f32,
    ghost: bool,
}

/// One revision of a body's mesh, prepared off the UI thread.
#[derive(Clone)]
struct Revision {
    revision: u64,
    mesh: Arc<MeshArrays>,
    prepared: Arc<PreparedMesh>,
    /// The picking tree: filled by the worker right after `prepared` (inline on wasm). Until
    /// then a pick tests every triangle instead of waiting.
    bvh: Arc<OnceLock<Bvh>>,
}

struct BodyEntry {
    view: BodyView,
    /// The revision on the GPU, on screen.
    shown: Option<Revision>,
    /// A newer prepared revision whose upload has not finished.
    ready: Option<Revision>,
}

impl BodyEntry {
    /// What picks, highlights and the box use: the revision on screen, or the newest prepared
    /// one while nothing of this body is on screen yet.
    fn current(&self) -> Option<&Revision> {
        self.shown.as_ref().or(self.ready.as_ref())
    }

    /// What the GPU should have.
    fn newest(&self) -> Option<&Revision> {
        self.ready.as_ref().or(self.shown.as_ref())
    }
}

#[cfg(not(target_arch = "wasm32"))]
struct Job {
    revision: u64,
    mesh: Arc<MeshArrays>,
    rx: std::sync::mpsc::Receiver<PreparedMesh>,
    bvh: Arc<OnceLock<Bvh>>,
    thread: std::thread::JoinHandle<()>,
}

/// A camera move asked for while a shown body's box was still being worked out.
enum ViewRequest {
    Fit(Aabb),
    View(String),
    LookAt(PlaneFrame, Option<f64>),
}

pub struct Viewport {
    pub mouse_mode: MouseMode,
    camera: Camera,
    dark: bool,
    bodies: Vec<BodyEntry>,
    #[cfg(not(target_arch = "wasm32"))]
    jobs: HashMap<String, Job>,
    /// Workers still building a picking tree after their mesh was handed over.
    #[cfg(not(target_arch = "wasm32"))]
    builders: Vec<std::thread::JoinHandle<()>>,
    pick_trees: bool,
    pending_views: Vec<ViewRequest>,
    selected: HashSet<String>,
    hovered: Option<String>,
    dimmed: HashSet<String>,
    show_edges: bool,
    grid_visible: bool,
    base_visible: bool,
    extent: f64,
    origin_s: f64,
    built_once: bool,
    planes: Vec<PlaneView>,
    sketches: Vec<SketchView>,
    active_sketch: Option<String>,
    overlays: Vec<OverlayItem>,
    band: Vec<OverlayItem>,
    brush: Option<OverlayItem>,
    preview: Vec<PreviewEntry>,
    preview_hidden: HashSet<String>,
    preview_hover: Option<String>,
    preview_counter: u64,
    section_plane: Option<[f64; 4]>,
    section_on: bool,
    viewcube_inset: f32,
    rect: Rect,
    frame_id: u64,
    dirty: bool,
    flat_dirty: bool,
    flat_version: u64,
    flat: Arc<Vec<FlatVertex>>,
    batches: Vec<FlatBatch>,
    last_size: [u32; 2],
    stats: Arc<Stats>,
    /// egui's target format, once [`Viewport::init_gpu`] ran.
    gpu_format: Option<wgpu::TextureFormat>,
    ctx: Option<egui::Context>,
}

impl Default for Viewport {
    fn default() -> Self {
        Viewport {
            mouse_mode: MouseMode::Model,
            camera: Camera::new(),
            dark: true,
            bodies: Vec::new(),
            #[cfg(not(target_arch = "wasm32"))]
            jobs: HashMap::new(),
            #[cfg(not(target_arch = "wasm32"))]
            builders: Vec::new(),
            pick_trees: true,
            pending_views: Vec::new(),
            selected: HashSet::new(),
            hovered: None,
            dimmed: HashSet::new(),
            show_edges: true,
            grid_visible: true,
            base_visible: false,
            extent: 100.0,
            origin_s: scene::origin_size(100.0),
            built_once: false,
            planes: Vec::new(),
            sketches: Vec::new(),
            active_sketch: None,
            overlays: Vec::new(),
            band: Vec::new(),
            brush: None,
            preview: Vec::new(),
            preview_hidden: HashSet::new(),
            preview_hover: None,
            preview_counter: 0,
            section_plane: None,
            section_on: false,
            viewcube_inset: 0.0,
            rect: Rect::from_min_size(Pos2::ZERO, egui::vec2(1.0, 1.0)),
            frame_id: 1,
            dirty: true,
            flat_dirty: true,
            flat_version: 0,
            flat: Arc::new(Vec::new()),
            batches: Vec::new(),
            last_size: [0, 0],
            stats: Arc::new(Stats::default()),
            gpu_format: None,
            ctx: None,
        }
    }
}

fn color_of(css: Option<&str>, index: usize) -> [u8; 3] {
    css.and_then(scene::parse_css_hex).unwrap_or_else(|| scene::rgb(scene::PALETTE[index % scene::PALETTE.len()]))
}

/// `_makeBody`: `info.solid ? info.color : '#d9a0a0'`.
fn body_color(view: &BodyView, index: usize) -> [u8; 3] {
    if view.solid { color_of(view.color.as_deref(), index) } else { scene::rgb(scene::NOT_SOLID) }
}

fn lin_scaled(hex: u32, s: f32) -> [f32; 3] {
    scene::linear(scene::rgb(hex)).map(|c| c * s)
}

impl Viewport {
    pub fn new() -> Self {
        Self::default()
    }

    // ------------------------------------------------------------------ GPU

    /// Set up the GPU side once, with eframe's (or a test's) render state. A device that
    /// replaces a lost one needs nothing here: the paint callback makes its state.
    pub fn init_gpu(&mut self, rs: &egui_wgpu::RenderState) {
        let gpu = GpuState::new(&rs.device, rs.target_format, self.stats.clone());
        rs.renderer.write().callback_resources.insert(gpu);
        self.gpu_format = Some(rs.target_format);
        self.dirty = true;
    }

    pub fn has_gpu(&self) -> bool {
        self.gpu_format.is_some()
    }

    /// Upload and render counters.
    pub fn stats(&self) -> &Stats {
        &self.stats
    }

    /// The counters, shared (a test keeps them while the viewport lives in a UI closure).
    pub fn stats_handle(&self) -> Arc<Stats> {
        self.stats.clone()
    }

    fn changed(&mut self) {
        self.dirty = true;
        if let Some(ctx) = &self.ctx {
            ctx.request_repaint();
        }
    }

    fn overlays_changed(&mut self) {
        self.flat_dirty = true;
        self.changed();
    }

    // ------------------------------------------------------------------ bodies

    /// `syncBodies`: keep these bodies. A body whose `revision` is new is prepared off the UI
    /// thread (display mesh and box, then its picking tree) and uploaded once; colour, name and
    /// hidden changes apply at once. Nothing here walks a mesh.
    pub fn sync_bodies(&mut self, bodies: &[BodyView]) {
        let mut old: HashMap<String, BodyEntry> = self.bodies.drain(..).map(|b| (b.view.id.clone(), b)).collect();
        #[cfg(not(target_arch = "wasm32"))]
        self.jobs.retain(|id, _| bodies.iter().any(|b| &b.id == id));
        for b in bodies {
            let mut entry = match old.remove(&b.id) {
                Some(mut e) => {
                    e.view = b.clone();
                    e
                }
                None => BodyEntry { view: b.clone(), shown: None, ready: None },
            };
            if entry.newest().is_none_or(|r| r.revision != b.revision) {
                if let Some(r) = self.schedule(b) {
                    entry.ready = Some(r);
                }
            }
            self.bodies.push(entry);
        }
        self.promote();
        self.update_extent();
        self.overlays_changed();
    }

    /// Prepare a body's mesh: on a worker thread (native; `None`, picked up by a later frame),
    /// which then builds the picking tree; or inline, tree included (wasm has no threads).
    fn schedule(&mut self, b: &BodyView) -> Option<Revision> {
        let bvh = Arc::new(OnceLock::new());
        #[cfg(not(target_arch = "wasm32"))]
        {
            if self.jobs.get(&b.id).is_some_and(|j| j.revision == b.revision) {
                return None;
            }
            let (tx, rx) = std::sync::mpsc::channel();
            let mesh = b.mesh.clone();
            let ctx = self.ctx.clone();
            let tree = bvh.clone();
            let trees = self.pick_trees;
            let thread = std::thread::spawn(move || {
                let _ = tx.send(scene::prepare_mesh(&mesh));
                drop(tx);
                if let Some(ctx) = ctx {
                    ctx.request_repaint();
                }
                if trees {
                    let _ = tree.set(Bvh::build(&mesh));
                }
            });
            self.jobs.insert(b.id.clone(), Job { revision: b.revision, mesh: b.mesh.clone(), rx, bvh, thread });
            None
        }
        #[cfg(target_arch = "wasm32")]
        {
            if self.pick_trees {
                let _ = bvh.set(Bvh::build(&b.mesh));
            }
            Some(Revision { revision: b.revision, mesh: b.mesh.clone(), prepared: Arc::new(scene::prepare_mesh(&b.mesh)), bvh })
        }
    }

    /// Tests: build no picking trees, so every pick takes the direct triangle test.
    #[doc(hidden)]
    pub fn set_pick_trees(&mut self, on: bool) {
        self.pick_trees = on;
    }

    /// Whether `id`'s picking tree (of the revision picks use) is built.
    pub fn pick_tree_ready(&self, id: &str) -> bool {
        self.entry(id).and_then(|e| e.current()).is_some_and(|r| r.bvh.get().is_some())
    }

    fn entry(&self, id: &str) -> Option<&BodyEntry> {
        self.bodies.iter().find(|b| b.view.id == id)
    }

    /// Pick up meshes prepared in the background (with `wait`, block until every job is done,
    /// picking trees included), then show revisions whose upload has finished.
    fn collect_jobs(&mut self, wait: bool) {
        let mut changed = false;
        #[cfg(not(target_arch = "wasm32"))]
        {
            use std::sync::mpsc::TryRecvError;
            let mut done = Vec::new();
            for (id, job) in &self.jobs {
                let got = if wait { job.rx.recv().map_err(|_| TryRecvError::Disconnected) } else { job.rx.try_recv() };
                match got {
                    Ok(p) => done.push((id.clone(), Some(p))),
                    Err(TryRecvError::Disconnected) => done.push((id.clone(), None)),
                    Err(TryRecvError::Empty) => {}
                }
            }
            for (id, p) in done {
                let Some(job) = self.jobs.remove(&id) else { continue };
                self.builders.push(job.thread);
                let Some(p) = p else { continue };
                if let Some(e) = self.bodies.iter_mut().find(|e| e.view.id == id && e.view.revision == job.revision) {
                    e.ready = Some(Revision { revision: job.revision, mesh: job.mesh, prepared: Arc::new(p), bvh: job.bvh });
                    changed = true;
                }
            }
            if wait {
                for t in self.builders.drain(..) {
                    let _ = t.join();
                }
            } else {
                self.builders.retain(|t| !t.is_finished());
            }
        }
        #[cfg(target_arch = "wasm32")]
        let _ = wait;
        changed |= self.promote();
        if changed {
            // A new box can move the grid, the origin and the plane sizes. A revision that only
            // moved to "shown" changes nothing drawn: its upload already cleared the last render.
            let before = (self.extent, self.origin_s);
            self.update_extent();
            if before != (self.extent, self.origin_s) {
                self.overlays_changed();
            }
        }
        self.run_pending_views();
    }

    /// Move each ready revision to "shown" once the GPU has it (at once without a GPU, or when
    /// the device is lost and nothing can be uploaded). True when anything moved.
    fn promote(&mut self) -> bool {
        if self.bodies.iter().all(|e| e.ready.is_none()) {
            return false;
        }
        let immediate = !self.has_gpu() || self.stats.device_lost();
        let on_gpu = if immediate { HashMap::new() } else { self.stats.on_gpu_map().clone() };
        let mut moved = false;
        for e in &mut self.bodies {
            let Some(r) = &e.ready else { continue };
            if immediate || on_gpu.get(&e.view.id) == Some(&r.revision) {
                e.shown = e.ready.take();
                moved = true;
            }
        }
        moved
    }

    /// A shown body is still waiting for its first box.
    fn boxes_pending(&self) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.bodies.iter().any(|e| !e.view.hidden && e.current().is_none() && self.jobs.contains_key(&e.view.id))
        }
        #[cfg(target_arch = "wasm32")]
        {
            false
        }
    }

    fn run_pending_views(&mut self) {
        if self.pending_views.is_empty() || self.boxes_pending() {
            return;
        }
        for r in std::mem::take(&mut self.pending_views) {
            match r {
                ViewRequest::Fit(b) => self.fit(b),
                ViewRequest::View(v) => self.set_view(&v),
                ViewRequest::LookAt(f, radius) => self.look_at_plane(&f, radius),
            }
        }
    }

    /// Wait until every body is prepared and its picking tree built (tests, screenshots).
    pub fn flush(&mut self) {
        self.collect_jobs(true);
    }

    /// The bodies as last synced (hidden ones included), in order.
    pub fn bodies(&self) -> impl Iterator<Item = &BodyView> {
        self.bodies.iter().map(|b| &b.view)
    }

    /// The mesh of a body as it is on screen, for picking and face highlights (pick's face
    /// numbers refer to it). `None` until the body's first revision is prepared.
    pub fn body_mesh(&self, id: &str) -> Option<&Arc<MeshArrays>> {
        self.entry(id).and_then(|e| e.current()).map(|r| &r.mesh)
    }

    /// The revision of a body that picks and highlights use (the one on screen).
    pub fn shown_revision(&self, id: &str) -> Option<u64> {
        self.entry(id).and_then(|e| e.current()).map(|r| r.revision)
    }

    /// A body is drawn (not hidden, not replaced by a preview).
    pub fn body_visible(&self, id: &str) -> bool {
        self.bodies.iter().any(|b| b.view.id == id && !b.view.hidden && !self.preview_hidden.contains(id))
    }

    fn update_extent(&mut self) {
        let mut b = camera::empty_box();
        for e in self.bodies.iter().filter(|e| !e.view.hidden) {
            if let Some(r) = e.current() {
                b = camera::box_union(&b, &r.prepared.bbox);
            }
        }
        let size = if camera::box_is_empty(&b) { 100.0 } else { camera::box_diagonal(&b) };
        let ext = size.max(20.0);
        // Don't size the grid from the empty-scene stand-in while a box is still on its way.
        let placeholder = camera::box_is_empty(&b) && self.boxes_pending();
        if !placeholder && ((ext.log10() - self.extent.log10()).abs() > 0.5 || !self.built_once) {
            self.origin_s = scene::origin_size(ext);
            self.built_once = true;
        }
        self.extent = ext;
    }

    /// `this.extent`: the visible bodies' diagonal, at least 20 (100 when empty).
    pub fn extent(&self) -> f64 {
        self.extent
    }

    pub fn set_selection(&mut self, ids: &[String]) {
        self.selected = ids.iter().cloned().collect();
        self.changed();
    }

    pub fn set_hover(&mut self, id: Option<&str>) {
        if self.hovered.as_deref() == id {
            return;
        }
        self.hovered = id.map(str::to_owned);
        self.changed();
    }

    /// `setBodiesDim(ids, dim)`: with `dim`, exactly these bodies are see-through; without, none.
    pub fn set_dimmed(&mut self, ids: &[String], dim: bool) {
        self.dimmed = if dim { ids.iter().cloned().collect() } else { HashSet::new() };
        self.changed();
    }

    pub fn set_edges_visible(&mut self, v: bool) {
        self.show_edges = v;
        self.changed();
    }

    pub fn set_grid_visible(&mut self, v: bool) {
        self.grid_visible = v;
        self.overlays_changed();
    }

    pub fn set_base_planes_visible(&mut self, v: bool) {
        self.base_visible = v;
        self.overlays_changed();
    }

    pub fn base_planes_visible(&self) -> bool {
        self.base_visible
    }

    pub fn set_mouse_mode(&mut self, m: MouseMode) {
        self.mouse_mode = m;
    }

    pub fn set_theme(&mut self, dark: bool) {
        self.dark = dark;
        self.overlays_changed();
    }

    pub fn dark(&self) -> bool {
        self.dark
    }

    /// Move the view cube left of an open tool panel (1.x: `right: 306px`).
    pub fn set_viewcube_inset(&mut self, right: f32) {
        self.viewcube_inset = right;
    }

    // ------------------------------------------------------------------ planes & sketches

    /// `syncPlanes`: construction planes, `max(extent * 0.35, 20)` across.
    pub fn sync_planes(&mut self, planes: &[PlaneView]) {
        self.planes = planes.to_vec();
        self.overlays_changed();
    }

    /// The planes that can be picked: (id, frame, size), the base planes first when shown.
    pub fn shown_planes(&self) -> Vec<(String, Frame, f64)> {
        let mut out = Vec::new();
        if self.base_visible {
            for (name, f, _) in scene::BASE_FRAMES {
                out.push((name.to_owned(), f, self.origin_s * 0.3));
            }
        }
        for p in self.planes.iter().filter(|p| p.visible) {
            out.push((p.id.clone(), [p.origin, p.x, p.y, p.normal], (self.extent * 0.35).max(20.0)));
        }
        out
    }

    /// `syncSketches`: every shown sketch except the one being edited, as `sketchLines(frame,
    /// entities, SKETCH_COL, 0.9)` (construction lines dashed).
    pub fn sync_sketches(&mut self, s: &[SketchView], active: Option<&str>) {
        self.sketches = s.to_vec();
        self.active_sketch = active.map(str::to_owned);
        self.overlays_changed();
    }

    // ------------------------------------------------------------------ camera

    pub fn camera(&self) -> &Camera {
        &self.camera
    }

    pub fn camera_mut(&mut self) -> &mut Camera {
        self.dirty = true;
        &mut self.camera
    }

    /// The rectangle the viewport was last shown in.
    pub fn rect(&self) -> Rect {
        self.rect
    }

    /// Set the view's size without a UI (tests, headless renders).
    pub fn set_view_size(&mut self, w: f32, h: f32) {
        self.rect = Rect::from_min_size(self.rect.min, egui::vec2(w, h));
        self.camera.set_size(w as f64, h as f64);
        self.dirty = true;
    }

    /// `fit(box)`: an empty box (min above max) fits the whole scene. While a shown body's box
    /// is still being worked out (a body just synced), the fit waits for it; so do
    /// [`set_view`](Self::set_view) and [`look_at_plane`](Self::look_at_plane).
    pub fn fit(&mut self, b: Aabb) {
        if self.boxes_pending() {
            self.pending_views.push(ViewRequest::Fit(b));
            return;
        }
        let scene = self.scene_box();
        self.camera.fit(b, scene);
        self.changed();
    }

    /// Fit everything shown (the view cube's Fit, the F key).
    pub fn fit_all(&mut self) {
        self.fit(camera::empty_box());
    }

    /// `setView(name)`: top, bottom, front, back, right, left, iso or fit.
    pub fn set_view(&mut self, name: &str) {
        if name == "fit" {
            return self.fit_all();
        }
        if self.boxes_pending() {
            self.pending_views.push(ViewRequest::View(name.to_owned()));
            return;
        }
        let scene = self.scene_box();
        if self.camera.set_view(name, scene) {
            self.changed();
        }
    }

    /// `lookAtPlane(frame, radius)`.
    pub fn look_at_plane(&mut self, frame: &PlaneFrame, radius: Option<f64>) {
        if self.boxes_pending() {
            self.pending_views.push(ViewRequest::LookAt(*frame, radius));
            return;
        }
        let scene = self.scene_box();
        self.camera.look_at_plane(frame.origin, frame.y, frame.normal, radius, scene);
        self.changed();
    }

    /// `sceneBox`: shown bodies (the revision on screen) and preview meshes, `None` when there
    /// are none.
    pub fn scene_box(&self) -> Option<Aabb> {
        let mut b = camera::empty_box();
        for r in self.bodies.iter().filter(|e| !e.view.hidden).filter_map(|e| e.current()) {
            b = camera::box_union(&b, &r.prepared.bbox);
        }
        for p in &self.preview {
            b = camera::box_union(&b, &p.prepared.bbox);
        }
        (!camera::box_is_empty(&b)).then_some(b)
    }

    /// `worldToScreen`: where a point shows, in egui points.
    pub fn world_to_screen(&self, p: [f64; 3]) -> Pos2 {
        let v = self.camera.world_to_view(p);
        self.rect.min + egui::vec2(v[0] as f32, v[1] as f32)
    }

    /// `pixelSize`: millimetres per screen point near a point.
    pub fn pixel_size(&self, at: [f64; 3]) -> f64 {
        self.camera.pixel_size(at)
    }

    /// The pick ray under a screen position: (origin, unit direction).
    pub fn ray(&self, pos: Pos2) -> (V3, V3) {
        let v = pos - self.rect.min;
        self.camera.ray([v.x as f64, v.y as f64])
    }

    // ------------------------------------------------------------------ section hook

    /// The section cut, for the section view (task 6): keep where `dot(n, p) + d >= 0`.
    pub fn set_section_plane(&mut self, plane: Option<[f64; 4]>) {
        self.section_plane = plane;
        self.changed();
    }

    pub fn section_plane(&self) -> Option<[f64; 4]> {
        self.section_plane
    }

    /// `setSectionEnabled(on)`: the same as [`set_section`](Self::set_section), the one place
    /// that turns the view on or off.
    pub fn set_section_enabled(&mut self, on: bool) {
        self.set_section(on);
    }

    pub fn section_enabled(&self) -> bool {
        self.section_on
    }

    /// The cut in force: enabled and set.
    pub fn active_section(&self) -> Option<[f64; 4]> {
        self.section_plane.filter(|_| self.section_on)
    }

    // ------------------------------------------------------------------ overlays

    fn push(&mut self, it: OverlayItem) {
        self.overlays.push(it);
        self.overlays_changed();
    }

    /// `clearOverlay(tag)`; an empty tag clears every overlay.
    pub fn clear_overlay(&mut self, tag: &str) {
        self.overlays.retain(|o| !(tag.is_empty() || o.tag == tag));
        self.overlays_changed();
    }

    /// `highlightFace(bodyId, tris, tag, color)` (1.x defaults: tag `"face"`, colour
    /// [`scene::FACE_HL`]).
    pub fn highlight_face(&mut self, body: &str, tris: &[u32], tag: &str, color: Color32) {
        let Some(mesh) = self.body_mesh(body).cloned() else { return };
        self.push(overlays::highlight_face(&mesh, tris, tag, hex(color)));
    }

    /// `addDistTris(arr, dist, limit, tag, [c0, a0], [c1, a1])`.
    pub fn add_dist_tris(&mut self, arr: &[f32], dist: &[f64], limit: f64, tag: &str, near: (Color32, f32), far: (Color32, f32)) {
        if arr.is_empty() {
            return;
        }
        self.push(overlays::dist_tris(arr, dist, limit, tag, (hex(near.0), near.1), (hex(far.0), far.1)));
    }

    /// `addMarker(p, color, tag, size)` (defaults 0xd9480f, `"marker"`, `max(extent * 0.006, 0.3)`).
    pub fn add_marker(&mut self, p: [f64; 3], color: Color32, tag: &str, size: Option<f64>) {
        let it = overlays::marker(p, hex(color), tag, size, self.extent);
        self.push(it);
    }

    /// `addArrow(p, dir, len, color, tag)` (defaults 0x1f6fd1, `"arrow"`).
    pub fn add_arrow(&mut self, p: [f64; 3], dir: [f64; 3], len: f64, color: Color32, tag: &str) {
        self.push(overlays::arrow(p, dir, len, hex(color), tag));
    }

    /// `addLine(a, b, color, tag)` (defaults 0x1f6fd1, `"line"`).
    pub fn add_line(&mut self, a: [f64; 3], b: [f64; 3], color: Color32, tag: &str) {
        self.push(overlays::line(a, b, hex(color), tag));
    }

    /// `addPolyline(points, color, tag)` (defaults 0x1f6fd1, `"line"`).
    pub fn add_polyline(&mut self, points: &[[f64; 3]], color: Color32, tag: &str) {
        if let Some(it) = overlays::polyline(points, hex(color), tag) {
            self.push(it);
        }
    }

    /// `addPlaneGhost(frame, tag, color)` (defaults `"plane-ghost"`, [`scene::PLANE_COL`]).
    pub fn add_plane_ghost(&mut self, frame: &PlaneFrame, tag: &str, color: Color32) {
        let it = overlays::plane_ghost(&frame.as_frame(), tag, hex(color), self.extent);
        self.push(it);
    }

    /// `setBand(points, cursor, color)` (default 0x59d98e): the rubber band of a wall being
    /// drawn and a dot where the pointer meets the part. Empty points and no cursor hide it.
    pub fn set_band(&mut self, points: &[[f64; 3]], cursor: Option<[f64; 3]>, color: Color32) {
        let c = cursor.map(|p| (p, self.pixel_size(p)));
        self.band = overlays::band(points, c, hex(color));
        self.overlays_changed();
    }

    /// `setBrush(b)`: the sculpt brush ring, or `None` to hide it.
    pub fn set_brush(&mut self, brush: Option<([f64; 3], [f64; 3], f64)>) {
        self.brush = brush.map(|(p, n, r)| overlays::brush(p, n, r));
        self.overlays_changed();
    }

    /// `drawStrokes(strokes)`: replaces the `"strokes"` overlay.
    pub fn draw_strokes(&mut self, strokes: &[Stroke]) {
        self.overlays.retain(|o| o.tag != "strokes");
        self.overlays.extend(overlays::strokes(strokes));
        self.overlays_changed();
    }

    /// `addRegions(frame, faces, tag)`: each face is its outer ring then its holes.
    pub fn add_regions(&mut self, frame: &PlaneFrame, faces: &[Vec<Vec<[f64; 2]>>], tag: &str) {
        self.overlays.extend(overlays::regions(&frame.as_frame(), faces, tag));
        self.overlays_changed();
    }

    /// `sketchLines(frame, entities, color, opacity, construction)` as a tagged overlay.
    pub fn add_sketch_lines(&mut self, frame: &PlaneFrame, entities: &[SketchOutline], color: Color32, opacity: f32, construction: bool, tag: &str) {
        self.push(overlays::sketch_lines(&frame.as_frame(), entities, hex(color), opacity, construction, tag));
    }

    /// `overhangFaces(bodyId, thresholdDeg)`: faces pointing down more steeply than the
    /// threshold, off the bed, in regions of 12 mm² or more.
    pub fn overhang_faces(&self, body: &str, threshold_deg: f64) -> Vec<u32> {
        self.body_mesh(body).map(|m| overhang_faces(m, threshold_deg)).unwrap_or_default()
    }

    // ------------------------------------------------------------------ preview

    /// `showPreview(pack, { hide })`: the meshes of a tool's result, replacing the bodies they
    /// stand for.
    pub fn show_preview(&mut self, pack: &PreviewPack) {
        self.clear_preview();
        let mut pi: i64 = -1;
        self.preview_hidden = pack.meshes.iter().map(|m| m.id.clone()).chain(pack.removed.iter().cloned()).chain(pack.hide.iter().cloned()).collect();
        const PAL: [u32; 8] = [0x7fb0e8, 0xe8a87f, 0x8fd18a, 0xd88fd1, 0xe8d27f, 0x7fe0d6, 0xb39ae8, 0xe87f8f];
        for m in &pack.meshes {
            let tint = pack.colors.get(&m.id).and_then(|c| scene::parse_css_hex(c));
            let existing = self.bodies.iter().position(|b| b.view.id == m.id);
            let color = if let Some(t) = tint {
                t
            } else if let Some(i) = existing {
                body_color(&self.bodies[i].view, i)
            } else {
                pi += 1;
                scene::rgb(PAL[(pi as usize) % PAL.len()])
            };
            let ghost = pack.ghosts.contains(&m.id);
            self.preview_counter += 1;
            self.preview.push(PreviewEntry {
                id: m.id.clone(),
                mesh: m.mesh.clone(),
                prepared: Arc::new(scene::prepare_mesh(&m.mesh)),
                bvh: Arc::new(Bvh::build(&m.mesh)),
                revision: self.preview_counter,
                color: scene::linear(color),
                emissive: if existing.is_some() || tint.is_some() { 0.08 } else { 0.2 },
                ghost,
            });
        }
        self.changed();
    }

    /// `clearPreview`.
    pub fn clear_preview(&mut self) {
        self.preview.clear();
        self.preview_hidden.clear();
        self.preview_hover = None;
        self.changed();
    }

    /// The preview meshes (for `pickPreview`).
    pub fn preview_meshes(&self) -> impl Iterator<Item = (&str, &Arc<MeshArrays>)> {
        self.preview.iter().map(|p| (p.id.as_str(), &p.mesh))
    }

    /// `setPreviewHover(id)`: show which fin a click would remove (or bring back).
    pub fn set_preview_hover(&mut self, id: Option<&str>) {
        if self.preview_hover.as_deref() == id {
            return;
        }
        self.preview_hover = id.map(str::to_owned);
        self.changed();
    }

    // ------------------------------------------------------------------ dev

    /// Load a mesh file straight into the view (development: `stl-app <file>`).
    pub fn load_dev_mesh(&mut self, name: &str, bytes: &[u8]) -> Result<(), String> {
        let parts = stl_io::load_mesh_file(bytes, name).map_err(|e| e.0)?;
        let views: Vec<BodyView> = parts
            .into_iter()
            .enumerate()
            .map(|(i, p)| BodyView {
                id: format!("dev{i}"),
                name: p.name,
                color: None,
                hidden: false,
                solid: true,
                mesh: Arc::new(MeshArrays { positions: p.positions.iter().map(|&v| v as f32).collect(), triangles: p.triangles }),
                revision: 1,
            })
            .collect();
        self.sync_bodies(&views);
        self.fit_all();
        Ok(())
    }

    // ------------------------------------------------------------------ frame

    fn rebuild_flat(&mut self) {
        let mut items: Vec<(i32, Depth, Vec<FlatVertex>, Vec<FlatVertex>)> = Vec::new();
        let s = self.origin_s;
        if self.grid_visible {
            items.push((-1, Depth::Test, Vec::new(), scene::grid_lines(s, self.dark)));
        }
        if self.base_visible {
            for (_, f, c) in scene::BASE_FRAMES {
                let (t, l) = scene::plane_mesh(&f, s * 0.3, c, 0.13);
                items.push((2, Depth::Test, t, l));
            }
        }
        let size = (self.extent * 0.35).max(20.0);
        for p in self.planes.iter().filter(|p| p.visible) {
            let (t, l) = scene::plane_mesh(&[p.origin, p.x, p.y, p.normal], size, scene::PLANE_COL, 0.13);
            items.push((2, Depth::Test, t, l));
        }
        items.push((5, Depth::Off, Vec::new(), scene::axis_lines(s)));
        for sk in self.sketches.iter().filter(|s| !s.hidden && Some(&s.id) != self.active_sketch.as_ref()) {
            let it = overlays::sketch_lines(&sk.frame, &sk.outlines, scene::SKETCH_COL, 0.9, true, "");
            items.push((it.order, it.depth, it.tris, it.lines));
        }
        for it in self.overlays.iter().chain(self.band.iter()).chain(self.brush.iter()) {
            items.push((it.order, it.depth, it.tris.clone(), it.lines.clone()));
        }
        items.sort_by_key(|i| i.0);
        let mut flat = Vec::new();
        let mut batches = Vec::new();
        for (order, depth, tris, lines) in items {
            for (is_lines, v) in [(false, tris), (true, lines)] {
                if v.is_empty() {
                    continue;
                }
                let start = flat.len() as u32;
                flat.extend(v);
                batches.push(FlatBatch { order, lines: is_lines, depth, range: start..flat.len() as u32 });
            }
        }
        self.flat = Arc::new(flat);
        self.batches = batches;
        self.flat_version += 1;
        self.flat_dirty = false;
    }

    fn globals(&self) -> Globals {
        let cam = &self.camera;
        let (x, y, z) = cam.basis();
        let view = cam.view_matrix();
        let to_view = |d: V3| {
            let v = [camera::dot(x, d), camera::dot(y, d), camera::dot(z, d)];
            let n = camera::normalize(v);
            [n[0] as f32, n[1] as f32, n[2] as f32]
        };
        // a DirectionalLight in the camera shines from its world position towards its target,
        // which stays at the world origin
        let light = |off: V3, intensity: f32| {
            let p = camera::add(cam.position, camera::add(camera::add(camera::scale(x, off[0]), camera::scale(y, off[1])), camera::scale(z, off[2])));
            let d = to_view(p);
            [d[0], d[1], d[2], intensity]
        };
        let hemi = to_view([0.0, 1.0, 0.0]);
        let sky = scene::linear(scene::rgb(scene::HEMI_SKY));
        let ground = scene::linear(scene::rgb(scene::HEMI_GROUND));
        let (clip, on) = match self.active_section() {
            Some(p) => ([p[0] as f32, p[1] as f32, p[2] as f32, p[3] as f32], 1.0),
            None => ([0.0; 4], 0.0),
        };
        Globals {
            view_proj: camera::to_f32(&cam.view_projection()),
            view: camera::to_f32(&view),
            key_light: light(scene::KEY_OFFSET, scene::KEY_INTENSITY),
            fill_light: light(scene::FILL_OFFSET, scene::FILL_INTENSITY),
            hemi_dir: [hemi[0], hemi[1], hemi[2], scene::HEMI_INTENSITY],
            sky: [sky[0], sky[1], sky[2], 1.0],
            ground: [ground[0], ground[1], ground[2], 1.0],
            clip,
            flags: [on, 0.0, 0.0, 0.0],
        }
    }

    fn body_draws(&self) -> Vec<BodyDraw> {
        let edge_rgb = scene::rgb(if self.dark { scene::EDGE_DARK } else { scene::EDGE_LIGHT });
        let edge = |a: f32| [edge_rgb[0] as f32 / 255.0, edge_rgb[1] as f32 / 255.0, edge_rgb[2] as f32 / 255.0, a];
        let dist = |b: &Aabb| camera::length(camera::sub(camera::box_center(b), self.camera.position));
        let mut out = Vec::new();
        for (i, e) in self.bodies.iter().enumerate() {
            let Some(r) = e.newest() else { continue };
            let (rev, mesh) = (&r.revision, &r.prepared);
            let id = &e.view.id;
            let dim = self.dimmed.contains(id);
            let emissive = if self.selected.contains(id) {
                lin_scaled(scene::SEL, scene::SEL_STRENGTH)
            } else if self.hovered.as_ref() == Some(id) {
                lin_scaled(scene::HOV, scene::HOV_STRENGTH)
            } else {
                [0.0; 3]
            };
            let c = scene::linear(body_color(&e.view, i));
            out.push(BodyDraw {
                key: id.clone(),
                revision: *rev,
                mesh: mesh.clone(),
                uniform: BodyUniform {
                    color: [c[0], c[1], c[2], if dim { scene::DIM_OPACITY } else { 1.0 }],
                    emissive: [emissive[0], emissive[1], emissive[2], scene::ROUGHNESS],
                    edge: edge(if dim { scene::EDGE_OPACITY_DIM } else { scene::EDGE_OPACITY }),
                    params: [scene::METALNESS, if mesh.flat { 1.0 } else { 0.0 }, 0.0, 0.0],
                },
                visible: !e.view.hidden && !self.preview_hidden.contains(id),
                transparent: dim,
                edges: self.show_edges,
                cap: true,
                depth: dist(&mesh.bbox),
            });
        }
        for p in &self.preview {
            let hovered = self.preview_hover.as_ref() == Some(&p.id);
            let em = if hovered { lin_scaled(0xd64545, 0.6) } else { lin_scaled(0x2f6fd1, p.emissive) };
            let e = scene::rgb(0x1f4f8f);
            out.push(BodyDraw {
                key: format!("\u{0}preview:{}", p.id),
                revision: p.revision,
                mesh: p.prepared.clone(),
                uniform: BodyUniform {
                    color: [p.color[0], p.color[1], p.color[2], if p.ghost { 0.28 } else { 1.0 }],
                    emissive: [em[0], em[1], em[2], 0.6],
                    edge: [e[0] as f32 / 255.0, e[1] as f32 / 255.0, e[2] as f32 / 255.0, 0.6],
                    params: [scene::METALNESS, if p.prepared.flat { 1.0 } else { 0.0 }, 0.0, 0.0],
                },
                visible: true,
                transparent: p.ghost,
                edges: !p.ghost,
                cap: false,
                depth: dist(&p.prepared.bbox),
            });
        }
        out
    }

    /// The frame for a view of `w` x `h` points at `ppp` pixels per point (1.x caps the pixel
    /// ratio at 2).
    pub fn frame_for_size(&mut self, w: f32, h: f32, ppp: f32) -> Arc<FrameData> {
        self.collect_jobs(false);
        if self.flat_dirty {
            self.rebuild_flat();
            self.dirty = true;
        }
        self.camera.set_size(w as f64, h as f64);
        let s = ppp.min(2.0);
        let size = [(w * s).round().max(0.0) as u32, (h * s).round().max(0.0) as u32];
        if size != self.last_size {
            self.last_size = size;
            self.dirty = true;
        }
        if self.dirty {
            self.frame_id += 1;
            self.dirty = false;
        }
        let bg = crate::theme::tokens(self.dark).bg;
        Arc::new(FrameData {
            id: self.frame_id,
            size,
            clear: [bg.r() as f64 / 255.0, bg.g() as f64 / 255.0, bg.b() as f64 / 255.0, 1.0],
            globals: self.globals(),
            bodies: self.body_draws(),
            flat_version: self.flat_version,
            flat: self.flat.clone(),
            batches: self.batches.clone(),
        })
    }

    /// Handle the mouse and draw the view, filling the available space.
    pub fn show(&mut self, ui: &mut egui::Ui) -> ViewportResponse {
        if self.ctx.is_none() {
            self.ctx = Some(ui.ctx().clone());
        }
        let (rect, response) = ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
        if rect != self.rect {
            self.dirty = true;
        }
        self.rect = rect;
        self.camera.set_size(rect.width() as f64, rect.height() as f64);
        let before = (self.camera.position, self.camera.target, self.camera.up);
        self.handle_input(ui, &response);
        let now = ui.input(|i| i.time);
        if self.camera.is_flying() && self.camera.animate(now) {
            ui.ctx().request_repaint();
        }
        if self.stats.upload_pending.load(std::sync::atomic::Ordering::Relaxed) {
            ui.ctx().request_repaint();
        }
        self.camera.set_time(now);
        if before != (self.camera.position, self.camera.target, self.camera.up) {
            self.dirty = true;
        }

        let ppp = ui.ctx().pixels_per_point();
        let frame = self.frame_for_size(rect.width(), rect.height(), ppp);
        let bg = crate::theme::tokens(self.dark).bg;
        ui.painter().rect_filled(rect, 0.0, bg);
        if let Some(format) = self.gpu_format.filter(|_| rect.width() >= 1.0 && rect.height() >= 1.0) {
            ui.painter().add(egui_wgpu::Callback::new_paint_callback(
                rect,
                renderer::ViewportCallback { frame, ctx: ui.ctx().clone(), format, stats: self.stats.clone() },
            ));
        }
        let cube = viewcube::show(ui, rect, self.viewcube_inset, self.section_on, self.dark);
        if let Some(v) = cube.view {
            self.set_view(v);
        }
        ViewportResponse { response, view: cube.view, section_clicked: cube.section_clicked }
    }

    fn handle_input(&mut self, ui: &egui::Ui, response: &egui::Response) {
        let delta = response.drag_delta();
        if delta != egui::Vec2::ZERO {
            for b in [egui::PointerButton::Primary, egui::PointerButton::Middle, egui::PointerButton::Secondary] {
                if response.dragged_by(b) {
                    match camera::drag_action(self.mouse_mode, b) {
                        Some(DragAction::Rotate) => self.camera.rotate(delta.x as f64, delta.y as f64),
                        Some(DragAction::Pan) => self.camera.pan(delta.x as f64, delta.y as f64),
                        None => {}
                    }
                    break;
                }
            }
        }
        if !response.contains_pointer() {
            return;
        }
        let rect = self.rect;
        let (wheel, cursor, touch) = ui.input(|i| {
            let mut dy = 0.0f64;
            for e in &i.raw.events {
                if let egui::Event::MouseWheel { unit, delta, modifiers, .. } = e {
                    // WheelEvent.deltaY points the other way; a notch is 100 in Chromium
                    let mut d = -delta.y as f64
                        * match unit {
                            egui::MouseWheelUnit::Point => 1.0,
                            egui::MouseWheelUnit::Line => 100.0,
                            egui::MouseWheelUnit::Page => 100.0,
                        };
                    if modifiers.ctrl {
                        d *= 10.0;
                    }
                    dy += d;
                }
            }
            (dy, i.pointer.hover_pos(), i.multi_touch())
        });
        if wheel != 0.0 {
            let c = cursor.unwrap_or(rect.center()) - rect.min;
            self.camera.wheel(wheel, [c.x as f64, c.y as f64]);
        }
        if let Some(t) = touch.filter(|t| t.num_touches >= 2) {
            let c = t.center_pos - rect.min;
            if t.zoom_delta > 0.0 && t.zoom_delta != 1.0 {
                self.camera.zoom_to_cursor(1.0 / t.zoom_delta as f64, [c.x as f64, c.y as f64]);
            }
            let d = t.translation_delta;
            if self.mouse_mode == MouseMode::Sculpt {
                self.camera.rotate(d.x as f64, d.y as f64);
            } else {
                self.camera.pan(d.x as f64, d.y as f64);
            }
        }
    }
}

fn hex(c: Color32) -> u32 {
    ((c.r() as u32) << 16) | ((c.g() as u32) << 8) | c.b() as u32
}

/// 1.x `overhangFaces` (Support Fins' overhang rule), on a body's mesh.
pub fn overhang_faces(m: &MeshArrays, threshold_deg: f64) -> Vec<u32> {
    let pos = &m.positions;
    let idx = &m.triangles;
    let nf = idx.len() / 3;
    let cut = -((threshold_deg.to_radians()).cos() + 1e-4);
    let min_z = pos.iter().skip(2).step_by(3).fold(f64::INFINITY, |a, &z| a.min(z as f64));
    let mut over = vec![false; nf];
    let mut area = vec![0.0f64; nf];
    let p = |v: u32, k: usize| pos[v as usize * 3 + k] as f64;
    for f in 0..nf {
        let (a, b, c) = (idx[f * 3], idx[f * 3 + 1], idx[f * 3 + 2]);
        let top = p(a, 2).max(p(b, 2)).max(p(c, 2)) - min_z;
        if top < 0.35 {
            continue;
        }
        let u = [p(b, 0) - p(a, 0), p(b, 1) - p(a, 1), p(b, 2) - p(a, 2)];
        let v = [p(c, 0) - p(a, 0), p(c, 1) - p(a, 1), p(c, 2) - p(a, 2)];
        let n = camera::cross(u, v);
        let len = camera::length(n);
        if !(len > 0.0) {
            continue;
        }
        if n[2] / len < cut {
            over[f] = true;
            area[f] = len / 2.0;
        }
    }
    let mut parent: Vec<usize> = (0..nf).collect();
    fn find(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    }
    let mut first_at = vec![usize::MAX; pos.len() / 3];
    for f in 0..nf {
        if !over[f] {
            continue;
        }
        for j in 0..3 {
            let vi = idx[f * 3 + j] as usize;
            let g = first_at[vi];
            if g == usize::MAX {
                first_at[vi] = f;
            } else {
                let (ra, rb) = (find(&mut parent, f), find(&mut parent, g));
                if ra != rb {
                    parent[ra] = rb;
                }
            }
        }
    }
    let mut sum: HashMap<usize, f64> = HashMap::new();
    for f in 0..nf {
        if over[f] {
            let r = find(&mut parent, f);
            *sum.entry(r).or_default() += area[f];
        }
    }
    (0..nf)
        .filter(|&f| over[f] && {
            let r = find(&mut parent, f);
            sum[&r] >= 12.0
        })
        .map(|f| f as u32)
        .collect()
}
