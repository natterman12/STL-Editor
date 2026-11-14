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
pub mod gizmo_draw;
pub mod overhang;
pub mod overlays;
pub mod pick;
pub mod preview_prep;
pub mod renderer;
pub mod scene;
pub mod section;
pub mod viewcube;

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};

use egui::{Color32, Pos2, Rect};
use stl_kernel::{Aabb, MeshArrays};
use stl_mesh::bvh::Bvh;

use crate::engine_link::{BodyView, PlaneView, SketchView};
use camera::{Camera, DragAction, V3};
use overlays::{Depth, OverlayItem, SketchOutline, Stroke};
use renderer::{BodyDraw, BodyUniform, FlatBatch, FrameData, Globals, GpuState, Stats};
pub use gizmo_draw::{GizmoMode, GizmoShape, Handle};
pub use overhang::{overhang_faces, OVERHANG_RED, SMALL_AMBER};
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
    /// The display mesh: built on a worker thread (native; inline on wasm), so `None` for a
    /// frame or two on a big mesh. Nothing is drawn or boxed until it is there.
    prepared: Option<Arc<PreparedMesh>>,
    rx: Option<std::sync::mpsc::Receiver<PreparedMesh>>,
    /// The picking tree: filled by the same worker right after `prepared`; picks test every
    /// triangle until then.
    bvh: Arc<OnceLock<Bvh>>,
    revision: u64,
    color: [f32; 3],
    emissive: f32,
    ghost: bool,
}

/// A preview whose meshes are still being prepared. It is swapped in, and the bodies it
/// replaces hidden, in one step when every mesh is ready; until then it is neither drawn nor
/// picked, and the preview on screen (if any) stays up. `cancel` stops its worker early.
struct StagedPreview {
    entries: Vec<PreviewEntry>,
    hide: HashSet<String>,
    cancel: Arc<AtomicBool>,
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

/// Where the viewport's time went, summed since the last [`Viewport::take_trace`] (the UI
/// bench reads it every frame to explain a slow one).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FrameTrace {
    /// `sync_bodies`: the engine's new body list.
    pub sync_ms: f64,
    /// Picking up meshes the workers prepared (dropping their finished threads included).
    pub collect_ms: f64,
    /// Moving uploaded revisions on screen; frees the revision they replace.
    pub promote_ms: f64,
    /// Revisions moved on screen.
    pub promoted: u32,
    /// `frame_for_size` in all (collect and promote included).
    pub frame_ms: f64,
    /// The paint callback's `prepare`: the upload slice, then encoding the render.
    pub paint_ms: f64,
    /// Paint callbacks that ran.
    pub paints: u32,
    /// A paint callback left bytes to upload.
    pub upload_pending: bool,
    /// Body uploads that finished (counted by the paint callbacks).
    pub uploads_done: u64,
    /// At the last frame: meshes still being prepared for the GPU, and picking trees still
    /// being built, off the UI thread.
    pub prep_jobs: u32,
    pub trees_building: u32,
    /// Times the overlays' vertex list was rebuilt (a gizmo drag must not cause any).
    pub flat_rebuilds: u32,
    /// Times the gizmo's own vertex list was rebuilt.
    pub gizmo_rebuilds: u32,
}

#[derive(Default)]
struct TraceCell(std::sync::Mutex<FrameTrace>);

impl TraceCell {
    fn add(&self, f: impl FnOnce(&mut FrameTrace)) {
        f(&mut self.0.lock().unwrap_or_else(|e| e.into_inner()));
    }
}

/// Free a replaced mesh revision (its display mesh, picking tree and maybe the mesh) off the
/// window thread: giving back a big body's memory took about 2 ms of a frame. Inline on wasm.
fn free_later<T: Send + 'static>(old: T) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let spawned = std::thread::Builder::new().name("stl-viewport-free".into()).spawn(move || drop(old));
        // No thread to be had: free it here.
        drop(spawned);
    }
    #[cfg(target_arch = "wasm32")]
    drop(old);
}

fn ms_since(t: web_time::Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

/// [`renderer::ViewportCallback`], timed for [`FrameTrace`].
struct TimedCallback {
    inner: renderer::ViewportCallback,
    trace: Arc<TraceCell>,
}

impl egui_wgpu::CallbackTrait for TimedCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        screen: &egui_wgpu::ScreenDescriptor,
        encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let t = web_time::Instant::now();
        let uploads = self.inner.stats.uploads();
        let out = egui_wgpu::CallbackTrait::prepare(&self.inner, device, queue, screen, encoder, resources);
        let pending = self.inner.stats.upload_pending.load(std::sync::atomic::Ordering::Relaxed);
        let done = self.inner.stats.uploads().saturating_sub(uploads);
        self.trace.add(|tr| {
            tr.paint_ms += ms_since(t);
            tr.paints += 1;
            tr.upload_pending |= pending;
            tr.uploads_done += done;
        });
        out
    }

    fn finish_prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        egui_wgpu::CallbackTrait::finish_prepare(&self.inner, device, queue, encoder, resources)
    }

    fn paint(&self, info: egui::PaintCallbackInfo, pass: &mut wgpu::RenderPass<'static>, resources: &egui_wgpu::CallbackResources) {
        egui_wgpu::CallbackTrait::paint(&self.inner, info, pass, resources);
    }
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
    brush_at: Option<([f64; 3], [f64; 3], f64)>,
    preview: Vec<PreviewEntry>,
    preview_hidden: HashSet<String>,
    /// The cancel flag of the worker that built `preview` (set when it is replaced or cleared).
    preview_cancel: Arc<AtomicBool>,
    preview_staged: Option<StagedPreview>,
    /// One preview build at a time: a build that waited and was replaced meanwhile never starts.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))] // the web build prepares previews inline
    preview_gate: Arc<std::sync::Mutex<()>>,
    /// One preview picking-tree build at a time. It is a separate gate from `preview_gate`, so the
    /// next preview's display meshes don't wait for the last one's trees.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    preview_tree_gate: Arc<std::sync::Mutex<()>>,
    /// How many preview display meshes were started (tests).
    preview_builds: Arc<AtomicUsize>,
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
    trace: Arc<TraceCell>,
    /// egui's target format, once [`Viewport::init_gpu`] ran.
    gpu_format: Option<wgpu::TextureFormat>,
    ctx: Option<egui::Context>,
    /// Per-body on-screen transforms (a gizmo drag): column-major, millimetres.
    body_transforms: HashMap<String, camera::M4>,
    /// The transform handles, and their geometry for the camera they were built for.
    gizmo: Option<GizmoShape>,
    gizmo_key: Option<(GizmoShape, V3, V3, V3, [u32; 2])>,
    gizmo_version: u64,
    gizmo_flat: Arc<Vec<FlatVertex>>,
    gizmo_batches: Vec<FlatBatch>,
    /// A handle drag holds the camera still (1.x: `controls.enabled = !dragging`).
    camera_locked: bool,
    overhang_cache: Option<overhang::OverhangCache>,
    overhang_job: Option<overhang::OverhangJob>,
    /// `_buildPlaneGrid`: sheet, grid and axes, built when the sketch opens.
    sketch_sheet: Vec<FlatVertex>,
    sketch_grid_lines: Vec<FlatVertex>,
    sketch_axes: Vec<FlatVertex>,
    /// wasm: the preview still being prepared a slice at a time.
    #[cfg(target_arch = "wasm32")]
    preview_batch: Option<preview_prep::Batch>,
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
            brush_at: None,
            preview: Vec::new(),
            preview_hidden: HashSet::new(),
            preview_cancel: Arc::new(AtomicBool::new(false)),
            preview_staged: None,
            preview_gate: Arc::new(std::sync::Mutex::new(())),
            preview_tree_gate: Arc::new(std::sync::Mutex::new(())),
            preview_builds: Arc::new(AtomicUsize::new(0)),
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
            trace: Arc::default(),
            gpu_format: None,
            ctx: None,
            body_transforms: HashMap::new(),
            gizmo: None,
            gizmo_key: None,
            gizmo_version: 0,
            gizmo_flat: Arc::new(Vec::new()),
            gizmo_batches: Vec::new(),
            camera_locked: false,
            overhang_cache: None,
            overhang_job: None,
            sketch_sheet: Vec::new(),
            sketch_grid_lines: Vec::new(),
            sketch_axes: Vec::new(),
            #[cfg(target_arch = "wasm32")]
            preview_batch: None,
        }
    }
}

pub fn color_of(css: Option<&str>, index: usize) -> [u8; 3] {
    css.and_then(scene::parse_css_hex).unwrap_or_else(|| scene::rgb(scene::PALETTE[index % scene::PALETTE.len()]))
}

/// `_makeBody`: `info.solid ? info.color : '#d9a0a0'`.
pub fn body_color(view: &BodyView, index: usize) -> [u8; 3] {
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

    /// What the viewport spent since the last call, and start again (the UI bench).
    pub fn take_trace(&self) -> FrameTrace {
        std::mem::take(&mut *self.trace.0.lock().unwrap_or_else(|e| e.into_inner()))
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
        let t = web_time::Instant::now();
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
                    if let Some(old) = entry.ready.replace(r) {
                        free_later(old);
                    }
                }
            }
            self.bodies.push(entry);
        }
        // Bodies that are gone.
        if !old.is_empty() {
            free_later(old);
        }
        self.promote();
        self.update_extent();
        self.overlays_changed();
        self.trace.add(|tr| tr.sync_ms += ms_since(t));
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
        let t = web_time::Instant::now();
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
                    let r = Revision { revision: job.revision, mesh: job.mesh, prepared: Arc::new(p), bvh: job.bvh };
                    if let Some(old) = e.ready.replace(r) {
                        free_later(old);
                    }
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
        changed |= self.collect_preview(wait);
        self.trace.add(|tr| tr.collect_ms += ms_since(t));
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
        let t = web_time::Instant::now();
        let immediate = !self.has_gpu() || self.stats.device_lost();
        let on_gpu = if immediate { HashMap::new() } else { self.stats.on_gpu_map().clone() };
        let mut moved = false;
        let mut promoted = 0;
        let mut replaced = Vec::new();
        for e in &mut self.bodies {
            let Some(r) = &e.ready else { continue };
            if immediate || on_gpu.get(&e.view.id) == Some(&r.revision) {
                replaced.extend(std::mem::replace(&mut e.shown, e.ready.take()));
                moved = true;
                promoted += 1;
            }
        }
        drop(on_gpu);
        if !replaced.is_empty() {
            free_later(replaced);
        }
        self.trace.add(|tr| {
            tr.promote_ms += ms_since(t);
            tr.promoted += promoted;
        });
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

    /// The start-up flight: `setView('iso')` over 320 ms, as `app.js` does after the first
    /// refresh (`loadSettings` then `refresh`, then `this.viewer.setView('iso')`).
    ///
    /// Call it once from `StlApp::update` in `app.rs`, on the first frame after `viewport.show`
    /// has run (that sets the camera clock). An empty window flies too; the launch-file
    /// `set_view("iso")` is a later flight, not this one. A pointer press, drag or wheel stops
    /// it where it is ([`Camera::cancel_flight`](camera::Camera::cancel_flight)).
    pub fn start_fly_in(&mut self) {
        self.set_view("iso");
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

    /// `lookAtPlane(frame, radius)`. Entering a sketch calls this (`sketcher.start`).
    pub fn look_at_plane(&mut self, frame: &PlaneFrame, radius: Option<f64>) {
        if self.boxes_pending() {
            self.pending_views.push(ViewRequest::LookAt(*frame, radius));
            return;
        }
        let scene = self.scene_box();
        self.camera.look_at_plane(frame.origin, frame.y, frame.normal, radius, scene);
        self.changed();
    }

    /// `_buildPlaneGrid`: the sketch plane's grid, axes and sheet. `None` removes it.
    /// The size follows the scene box at the moment it is set, as `sketcher.js` builds it once
    /// in `start`.
    pub fn set_sketch_plane_grid(&mut self, frame: Option<PlaneFrame>) {
        match frame {
            Some(f) => {
                let diag = self.scene_box().map(|b| camera::box_diagonal(&b));
                let (sheet, lines, axes) = scene::sketch_plane_grid(&f.as_frame(), diag);
                self.sketch_sheet = sheet;
                self.sketch_grid_lines = lines;
                self.sketch_axes = axes;
            }
            None => {
                self.sketch_sheet.clear();
                self.sketch_grid_lines.clear();
                self.sketch_axes.clear();
            }
        }
        self.overlays_changed();
    }

    /// The sketch plane grid is showing.
    pub fn sketch_plane_grid_visible(&self) -> bool {
        !self.sketch_sheet.is_empty()
    }

    /// `sceneBox`: shown bodies (the revision on screen) and preview meshes, `None` when there
    /// are none.
    pub fn scene_box(&self) -> Option<Aabb> {
        let mut b = camera::empty_box();
        for r in self.bodies.iter().filter(|e| !e.view.hidden).filter_map(|e| e.current()) {
            b = camera::box_union(&b, &r.prepared.bbox);
        }
        for p in self.preview.iter().filter_map(|p| p.prepared.as_ref()) {
            b = camera::box_union(&b, &p.bbox);
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
        if self.brush_at == brush {
            return;
        }
        self.brush_at = brush;
        self.brush = brush.map(|(p, n, r)| overlays::brush(p, n, r));
        self.overlays_changed();
    }

    /// The brush ring shown: (point, normal, radius).
    pub fn brush(&self) -> Option<([f64; 3], [f64; 3], f64)> {
        self.brush_at
    }

    /// The tagged overlays, in the order they were added.
    pub fn overlay_items(&self) -> &[OverlayItem] {
        &self.overlays
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

    /// [`Self::add_regions`] in `color` at `alpha` (the colour's own alpha is ignored), drawn
    /// under sketch strokes. 1.x sketch fill is `0xf6c7a8` at 0.38 and does not follow the theme.
    pub fn add_regions_tinted(&mut self, frame: &PlaneFrame, faces: &[Vec<Vec<[f64; 2]>>], tag: &str, color: Color32, alpha: f32) {
        self.overlays.extend(overlays::regions_tinted(&frame.as_frame(), faces, tag, hex(color), alpha));
        self.overlays_changed();
    }

    /// `sketchLines(frame, entities, color, opacity, construction)` as a tagged overlay.
    pub fn add_sketch_lines(&mut self, frame: &PlaneFrame, entities: &[SketchOutline], color: Color32, opacity: f32, construction: bool, tag: &str) {
        self.push(overlays::sketch_lines(&frame.as_frame(), entities, hex(color), opacity, construction, tag));
    }

    // ------------------------------------------------------------------ preview

    /// `showPreview(pack, { hide })`: the meshes of a tool's result, replacing the bodies they
    /// stand for. Their display meshes and picking trees are built off the UI thread (native),
    /// by one worker per preview; the preview appears, and the bodies it replaces go, in one
    /// step when all its meshes are ready. The preview already on screen stays up meanwhile, and
    /// a preview replaced before it is ready stops its worker.
    pub fn show_preview(&mut self, pack: &PreviewPack) {
        let hide: HashSet<String> = pack.meshes.iter().map(|m| m.id.clone()).chain(pack.removed.iter().cloned()).chain(pack.hide.iter().cloned()).collect();
        const PAL: [u32; 8] = [0x7fb0e8, 0xe8a87f, 0x8fd18a, 0xd88fd1, 0xe8d27f, 0x7fe0d6, 0xb39ae8, 0xe87f8f];
        if let Some(old) = self.preview_staged.take() {
            old.cancel.store(true, Ordering::Relaxed);
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let meshes: Vec<Arc<MeshArrays>> = pack.meshes.iter().map(|m| m.mesh.clone()).collect();
        let prep = self.prepare_previews(&meshes, &cancel);
        let mut pi: i64 = -1;
        let mut entries = Vec::with_capacity(pack.meshes.len());
        for (m, (prepared, rx, bvh)) in pack.meshes.iter().zip(prep) {
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
            entries.push(PreviewEntry {
                id: m.id.clone(),
                mesh: m.mesh.clone(),
                prepared,
                rx,
                bvh,
                revision: self.preview_counter,
                color: scene::linear(color),
                emissive: if existing.is_some() || tint.is_some() { 0.08 } else { 0.2 },
                ghost,
            });
        }
        let staged = StagedPreview { entries, hide, cancel };
        if staged.entries.iter().all(|e| e.prepared.is_some()) {
            self.install_preview(staged);
        } else {
            self.preview_staged = Some(staged);
        }
        self.changed();
    }

    /// Swap a prepared preview in (and the one it replaces out, stopping that one's worker).
    fn install_preview(&mut self, mut st: StagedPreview) {
        // a mesh whose worker stopped without it (it panicked) is left out, and the body it
        // stands for stays on screen instead of vanishing
        let lost: Vec<String> = st.entries.iter().filter(|e| e.prepared.is_none()).map(|e| e.id.clone()).collect();
        if !lost.is_empty() {
            st.entries.retain(|e| e.prepared.is_some());
            for id in &lost {
                st.hide.remove(id);
            }
        }
        self.preview_cancel.store(true, Ordering::Relaxed);
        self.preview_cancel = st.cancel;
        self.preview = st.entries;
        self.preview_hidden = st.hide;
        self.preview_hover = None;
        self.changed();
    }

    /// Display meshes and picking trees for a preview's meshes: on one worker thread, in order
    /// (native; the display meshes first, then the trees), or inline (wasm has no threads). The
    /// worker checks `cancel` between stages and asks for a repaint only while it is wanted.
    #[allow(clippy::type_complexity)]
    fn prepare_previews(&mut self, meshes: &[Arc<MeshArrays>], cancel: &Arc<AtomicBool>) -> Vec<(Option<Arc<PreparedMesh>>, Option<std::sync::mpsc::Receiver<PreparedMesh>>, Arc<OnceLock<Bvh>>)> {
        let trees: Vec<Arc<OnceLock<Bvh>>> = meshes.iter().map(|_| Arc::new(OnceLock::new())).collect();
        #[cfg(not(target_arch = "wasm32"))]
        {
            let (txs, rxs): (Vec<_>, Vec<_>) = meshes.iter().map(|_| std::sync::mpsc::channel::<PreparedMesh>()).unzip();
            let (meshes_w, ctx, trees_w, cancel_w) = (meshes.to_vec(), self.ctx.clone(), trees.clone(), cancel.clone());
            let (gate, tree_gate, builds) = (self.preview_gate.clone(), self.preview_tree_gate.clone(), self.preview_builds.clone());
            let thread = std::thread::spawn(move || {
                let guard = gate.lock().unwrap_or_else(|e| e.into_inner());
                for (mesh, tx) in meshes_w.iter().zip(txs) {
                    if cancel_w.load(Ordering::Relaxed) {
                        return;
                    }
                    builds.fetch_add(1, Ordering::Relaxed);
                    let sent = tx.send(scene::prepare_mesh(mesh)).is_ok();
                    drop(tx);
                    if sent && !cancel_w.load(Ordering::Relaxed) {
                        if let Some(ctx) = &ctx {
                            ctx.request_repaint();
                        }
                    }
                }
                drop(guard);
                // the trees one preview at a time too; a preview replaced while it waited skips them
                let _trees = tree_gate.lock().unwrap_or_else(|e| e.into_inner());
                for (mesh, tree) in meshes_w.iter().zip(&trees_w) {
                    if cancel_w.load(Ordering::Relaxed) {
                        return;
                    }
                    let _ = tree.set(Bvh::build(mesh));
                }
            });
            self.builders.push(thread);
            rxs.into_iter().zip(trees).map(|(rx, t)| (None, Some(rx), t)).collect()
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = cancel;
            // Not on this turn: a 500,000-triangle preview would freeze the page. Slices run
            // from [`collect_preview`](Self::collect_preview), 4 ms a frame, like the overhangs.
            self.preview_batch = Some(preview_prep::Batch::new(meshes.to_vec(), trees.clone(), self.preview_builds.clone()));
            if let Some(c) = &self.ctx {
                c.request_repaint();
            }
            trees.into_iter().map(|t| (None, None, t)).collect()
        }
    }

    /// Pick up the staged preview's meshes prepared in the background (with `wait`, block until
    /// they are all), and swap it in when it is complete. True when it was swapped in.
    fn collect_preview(&mut self, wait: bool) -> bool {
        use std::sync::mpsc::TryRecvError;
        #[cfg(target_arch = "wasm32")]
        if !self.poll_preview_batch(wait) {
            return false;
        }
        let Some(st) = self.preview_staged.as_mut() else { return false };
        for e in st.entries.iter_mut().filter(|e| e.prepared.is_none()) {
            let Some(rx) = &e.rx else { continue };
            let got = if wait { rx.recv().map_err(|_| TryRecvError::Disconnected) } else { rx.try_recv() };
            match got {
                Ok(p) => {
                    e.prepared = Some(Arc::new(p));
                    e.rx = None;
                }
                Err(TryRecvError::Disconnected) => e.rx = None,
                Err(TryRecvError::Empty) => {}
            }
        }
        // a mesh whose worker is gone never arrives; the rest is shown without it
        if !st.entries.iter().all(|e| e.prepared.is_some() || e.rx.is_none()) {
            return false;
        }
        match self.preview_staged.take() {
            Some(st) => {
                self.install_preview(st);
                true
            }
            None => false,
        }
    }

    /// How many preview display meshes were started so far (tests: a replaced preview's stops early).
    #[doc(hidden)]
    pub fn preview_builds(&self) -> usize {
        self.preview_builds.load(Ordering::Relaxed)
    }

    /// One slice of a wasm preview (or the rest of it, when `wait`). False while it still runs.
    #[cfg(target_arch = "wasm32")]
    fn poll_preview_batch(&mut self, wait: bool) -> bool {
        if self.preview_batch.is_none() {
            return true;
        }
        let budget = if wait { None } else { Some(preview_prep::SLICE) };
        let ready = self.preview_batch.as_mut().and_then(|b| b.poll(budget));
        let Some(ready) = ready else {
            if let Some(c) = &self.ctx {
                c.request_repaint();
            }
            return false;
        };
        self.preview_batch = None;
        if let Some(st) = self.preview_staged.as_mut() {
            for (e, p) in st.entries.iter_mut().zip(ready) {
                e.prepared = Some(Arc::new(p));
            }
        }
        true
    }

    /// `clearPreview`.
    pub fn clear_preview(&mut self) {
        #[cfg(target_arch = "wasm32")]
        {
            self.preview_batch = None;
        }
        self.preview_cancel.store(true, Ordering::Relaxed);
        if let Some(st) = self.preview_staged.take() {
            st.cancel.store(true, Ordering::Relaxed);
        }
        self.preview.clear();
        self.preview_hidden.clear();
        self.preview_hover = None;
        self.changed();
    }

    /// The preview meshes on screen (for `pickPreview`); one still being prepared is not there yet.
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
        if !self.sketch_sheet.is_empty() {
            items.push((1, Depth::Test, self.sketch_sheet.clone(), Vec::new()));
        }
        if !self.sketch_grid_lines.is_empty() {
            items.push((1, Depth::Test, Vec::new(), self.sketch_grid_lines.clone()));
        }
        if !self.sketch_axes.is_empty() {
            items.push((9, Depth::Off, Vec::new(), self.sketch_axes.clone()));
        }
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
        self.trace.add(|tr| tr.flat_rebuilds += 1);
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
            let (model, normal) = match self.body_transforms.get(id) {
                Some(m) => (camera::to_f32(m), camera::to_f32(&normal_matrix(m))),
                None => (renderer::IDENTITY, renderer::IDENTITY),
            };
            out.push(BodyDraw {
                key: id.clone(),
                revision: *rev,
                mesh: mesh.clone(),
                uniform: BodyUniform {
                    color: [c[0], c[1], c[2], if dim { scene::DIM_OPACITY } else { 1.0 }],
                    emissive: [emissive[0], emissive[1], emissive[2], scene::ROUGHNESS],
                    edge: edge(if dim { scene::EDGE_OPACITY_DIM } else { scene::EDGE_OPACITY }),
                    params: [scene::METALNESS, if mesh.flat { 1.0 } else { 0.0 }, 0.0, 0.0],
                    model,
                    normal,
                },
                visible: !e.view.hidden && !self.preview_hidden.contains(id),
                transparent: dim,
                edges: self.show_edges,
                cap: true,
                depth: dist(&mesh.bbox),
            });
        }
        for p in &self.preview {
            let Some(prepared) = &p.prepared else { continue };
            let hovered = self.preview_hover.as_ref() == Some(&p.id);
            let em = if hovered { lin_scaled(0xd64545, 0.6) } else { lin_scaled(0x2f6fd1, p.emissive) };
            let e = scene::rgb(0x1f4f8f);
            out.push(BodyDraw {
                key: format!("\u{0}preview:{}", p.id),
                revision: p.revision,
                mesh: prepared.clone(),
                uniform: BodyUniform {
                    color: [p.color[0], p.color[1], p.color[2], if p.ghost { 0.28 } else { 1.0 }],
                    emissive: [em[0], em[1], em[2], 0.6],
                    edge: [e[0] as f32 / 255.0, e[1] as f32 / 255.0, e[2] as f32 / 255.0, 0.6],
                    params: [scene::METALNESS, if prepared.flat { 1.0 } else { 0.0 }, 0.0, 0.0],
                    model: renderer::IDENTITY,
                    normal: renderer::IDENTITY,
                },
                visible: true,
                transparent: p.ghost,
                edges: !p.ghost,
                cap: false,
                depth: dist(&prepared.bbox),
            });
        }
        out
    }

    /// The frame for a view of `w` x `h` points at `ppp` pixels per point (1.x caps the pixel
    /// ratio at 2).
    pub fn frame_for_size(&mut self, w: f32, h: f32, ppp: f32) -> Arc<FrameData> {
        let t = web_time::Instant::now();
        self.collect_jobs(false);
        #[cfg(not(target_arch = "wasm32"))]
        {
            let (jobs, trees) = (self.jobs.len() as u32, self.builders.len() as u32);
            self.trace.add(|tr| {
                tr.prep_jobs = jobs;
                tr.trees_building = trees;
            });
        }
        if self.flat_dirty {
            self.rebuild_flat();
            self.dirty = true;
        }
        self.camera.set_size(w as f64, h as f64);
        let s = ppp.min(2.0);
        self.rebuild_gizmo([(w * s).round().max(0.0) as u32, (h * s).round().max(0.0) as u32]);
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
        let frame = Arc::new(FrameData {
            id: self.frame_id,
            size,
            clear: [bg.r() as f64 / 255.0, bg.g() as f64 / 255.0, bg.b() as f64 / 255.0, 1.0],
            globals: self.globals(),
            bodies: self.body_draws(),
            flat_version: self.flat_version,
            flat: self.flat.clone(),
            batches: self.batches.clone(),
            gizmo_version: self.gizmo_version,
            gizmo: self.gizmo_flat.clone(),
            gizmo_batches: self.gizmo_batches.clone(),
        });
        self.trace.add(|tr| tr.frame_ms += ms_since(t));
        frame
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
        #[cfg(target_arch = "wasm32")]
        if self.preview_batch.is_some() {
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
            let inner = renderer::ViewportCallback { frame, ctx: ui.ctx().clone(), format, stats: self.stats.clone() };
            ui.painter().add(egui_wgpu::Callback::new_paint_callback(rect, TimedCallback { inner, trace: self.trace.clone() }));
        }
        let cube = viewcube::show(ui, rect, self.viewcube_inset, self.section_on, self.dark);
        if let Some(v) = cube.view {
            self.set_view(v);
        }
        ViewportResponse { response, view: cube.view, section_clicked: cube.section_clicked }
    }

    fn handle_input(&mut self, ui: &egui::Ui, response: &egui::Response) {
        if self.camera_locked {
            return;
        }
        // A click stops the start-up flight here, before this frame's step. A drag and the
        // wheel do it in `Camera::rotate` / `pan` / `wheel`, which run next, still before `animate`.
        if (response.hovered() || response.dragged()) && ui.input(|i| i.pointer.any_pressed()) {
            self.camera.cancel_flight();
        }
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

/// The normal matrix of a transform: the inverse transpose of its 3x3 part (column-major).
pub fn normal_matrix(m: &camera::M4) -> camera::M4 {
    let a = |r: usize, c: usize| m[c][r];
    let cof = |r: usize, c: usize| {
        let (r0, r1) = ((r + 1) % 3, (r + 2) % 3);
        let (c0, c1) = ((c + 1) % 3, (c + 2) % 3);
        a(r0, c0) * a(r1, c1) - a(r0, c1) * a(r1, c0)
    };
    let det = a(0, 0) * cof(0, 0) + a(0, 1) * cof(0, 1) + a(0, 2) * cof(0, 2);
    let k = if det.abs() > 1e-300 { 1.0 / det } else { 0.0 };
    let mut n = [[0.0; 4]; 4];
    for r in 0..3 {
        for c in 0..3 {
            // (A^-1)^T = cofactor / det
            n[c][r] = cof(r, c) * k;
        }
    }
    n[3][3] = 1.0;
    n
}

impl Viewport {
    // ------------------------------------------------------------------ gizmo

    /// Draw a body moved by `m` (column-major, millimetres) without touching its mesh: the
    /// gizmo's instant on-screen motion. `None` puts it back where its mesh has it.
    pub fn set_body_transform(&mut self, id: &str, m: Option<camera::M4>) {
        match m {
            Some(m) => {
                self.body_transforms.insert(id.to_owned(), m);
            }
            None => {
                if self.body_transforms.remove(id).is_none() {
                    return;
                }
            }
        }
        self.changed();
    }

    /// Put every moved body back (gizmo `reset`).
    pub fn clear_body_transforms(&mut self) {
        if !self.body_transforms.is_empty() {
            self.body_transforms.clear();
            self.changed();
        }
    }

    pub fn body_transform(&self, id: &str) -> Option<&camera::M4> {
        self.body_transforms.get(id)
    }

    /// A body's box as it is on screen (its mesh, before any gizmo transform).
    pub fn body_box(&self, id: &str) -> Option<Aabb> {
        self.entry(id).and_then(|e| e.current()).map(|r| r.prepared.bbox)
    }

    /// Show the transform handles, or `None` to hide them (`gizmo.hide()` when a tool closes
    /// and when a sketch opens). Cheap to call every frame: the handles are rebuilt only when
    /// they or the camera change, in their own small buffer.
    pub fn set_gizmo(&mut self, shape: Option<GizmoShape>) {
        if self.gizmo == shape {
            return;
        }
        self.gizmo = shape;
        self.changed();
    }

    pub fn gizmo(&self) -> Option<&GizmoShape> {
        self.gizmo.as_ref()
    }

    /// The handle under a screen position, if the gizmo shows.
    pub fn gizmo_handle_at(&self, pos: Pos2) -> Option<Handle> {
        let g = self.gizmo.as_ref()?;
        let (o, d) = self.ray(pos);
        gizmo_draw::pick(g, &self.camera, o, d)
    }

    /// Hold the camera still (a handle is being dragged): mouse drags and the wheel do nothing.
    pub fn set_camera_locked(&mut self, on: bool) {
        self.camera_locked = on;
    }

    pub fn camera_locked(&self) -> bool {
        self.camera_locked
    }

    fn rebuild_gizmo(&mut self, size: [u32; 2]) {
        let Some(g) = &self.gizmo else {
            if !self.gizmo_flat.is_empty() {
                self.gizmo_flat = Arc::new(Vec::new());
                self.gizmo_batches.clear();
                self.gizmo_key = None;
                self.gizmo_version += 1;
                self.dirty = true;
            }
            return;
        };
        let key = (g.clone(), self.camera.position, self.camera.target, self.camera.up, size);
        if self.gizmo_key.as_ref() == Some(&key) {
            return;
        }
        let (v, b) = gizmo_draw::build(g, &self.camera);
        self.gizmo_flat = Arc::new(v);
        self.gizmo_batches = b;
        self.gizmo_key = Some(key);
        self.gizmo_version += 1;
        self.dirty = true;
        self.trace.add(|tr| tr.gizmo_rebuilds += 1);
    }
}
