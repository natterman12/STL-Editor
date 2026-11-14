#![allow(unexpected_cfgs)]
//! Preparing a tool preview for the screen.
//!
//! On the desktop this stays on a worker thread (`Viewport::show_preview`). On wasm32 the page
//! has no other thread, so a large preview is built a few milliseconds at a time, the same way
//! the red overhangs are. The mesh that comes out matches [`scene::prepare_mesh`], and the
//! picking tree is the same [`Bvh::build`](stl_mesh::bvh::Bvh::build) call. A small preview,
//! one whose mesh and tree fit in one slice, finishes in the frame it arrives in. A large one
//! shares this slice when the mesh left most of the budget, and takes the next frame when the
//! slice is already busy.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use stl_kernel::MeshArrays;
use stl_mesh::bvh::Bvh;

use super::scene::{self, CreaseBuild, LitVertex, PreparedMesh};

/// How long one frame spends on a preview on wasm, matching the overhang slices.
pub const SLICE: Duration = Duration::from_millis(4);

/// A mesh of at most this many triangles, together with its picking tree, fits in one [`SLICE`].
///
/// The bound is the triangle count. A drag replaces the preview every frame, so a small mesh
/// whose own step already used a quarter of the slice must still build its tree before yielding.
/// A larger mesh stays sliced, and cannot freeze the page.
pub const ONE_SLICE_TRIS: usize = 4_096;

/// One preview's meshes, prepared a slice at a time.
pub struct Batch {
    meshes: Vec<Arc<MeshArrays>>,
    trees: Vec<Arc<OnceLock<Bvh>>>,
    builds: Arc<AtomicUsize>,
    index: usize,
    phase: u8,
    crease: Option<CreaseBuild>,
    vertices: Vec<LitVertex>,
    out: Vec<PreparedMesh>,
    started: bool,
}

impl Batch {
    pub fn new(meshes: Vec<Arc<MeshArrays>>, trees: Vec<Arc<OnceLock<Bvh>>>, builds: Arc<AtomicUsize>) -> Self {
        Batch { meshes, trees, builds, index: 0, phase: 0, crease: None, vertices: Vec::new(), out: Vec::new(), started: false }
    }

    /// `Some` when every mesh is ready. `budget` `None` runs to the end.
    pub fn poll(&mut self, budget: Option<Duration>) -> Option<Vec<PreparedMesh>> {
        let t0 = web_time::Instant::now();
        let mut worked = false;
        loop {
            if self.index >= self.meshes.len() {
                return Some(std::mem::take(&mut self.out));
            }
            let mesh = Arc::clone(&self.meshes[self.index]);
            let ntri = mesh.triangles.len() / 3;
            let fits = ntri <= ONE_SLICE_TRIS;
            // A large preview yields once this slice is spent. A small one runs to the tree
            // in the frame it arrives in, even when the mesh step itself was slow.
            if !fits && budget.is_some_and(|b| t0.elapsed() >= b) && self.started {
                return None;
            }
            match self.phase {
                0 => {
                    if !self.started {
                        self.builds.fetch_add(1, Ordering::Relaxed);
                        self.started = true;
                    }
                    worked = true;
                    if ntri > scene::FLAT_ABOVE {
                        self.vertices = mesh
                            .positions
                            .chunks_exact(3)
                            .map(|p| LitVertex { pos: [p[0], p[1], p[2]], normal: [0.0; 3] })
                            .collect();
                        self.phase = 1;
                    } else {
                        let left = if fits { None } else { budget.map(|b| b.saturating_sub(t0.elapsed())) };
                        let done = {
                            let job = self.crease.get_or_insert_with(|| CreaseBuild::new(&mesh, scene::CREASE_DEG));
                            job.step(&mesh, left)
                        };
                        if let Some(v) = done {
                            self.vertices = v;
                            self.crease = None;
                            self.phase = 1;
                        } else {
                            return None;
                        }
                    }
                }
                1 => {
                    worked = true;
                    let edges = if ntri < scene::EDGES_BELOW { Some(scene::feature_edges(&mesh, scene::EDGE_DEG)) } else { None };
                    let flat = ntri > scene::FLAT_ABOVE;
                    let indices = if flat { Some(mesh.triangles[..ntri * 3].to_vec()) } else { None };
                    let prepared = PreparedMesh {
                        vertices: std::mem::take(&mut self.vertices),
                        indices,
                        flat,
                        edges,
                        bbox: scene::mesh_box(&mesh),
                        triangles: ntri,
                    };
                    self.out.push(prepared);
                    self.phase = 2;
                    if !fits && budget.is_some_and(|b| t0.elapsed() >= b) {
                        return None;
                    }
                }
                _ => {
                    // The tree is one call (`Bvh::build` stays whole). A small mesh always
                    // builds it here. A large one leaves it for the next frame when this
                    // slice has already used a quarter of its budget.
                    if !fits {
                        if let Some(b) = budget {
                            if worked && t0.elapsed() >= b / 4 {
                                return None;
                            }
                        }
                    }
                    let _ = self.trees[self.index].set(Bvh::build(&mesh));
                    self.index += 1;
                    self.phase = 0;
                    self.started = false;
                }
            }
        }
    }
}

/// A triangle strip of `tris` triangles, for the large-preview measurement.
///
/// Compiled on wasm only with `--cfg stl_preview_measure` (`xtask web --measure-preview`).
/// Native builds keep it for `tests/flyin.rs`.
#[cfg(any(stl_preview_measure, not(target_arch = "wasm32")))]
pub fn bench_mesh(tris: usize) -> MeshArrays {
    let tris = tris.max(1);
    let nvert = tris + 2;
    let mut positions = Vec::with_capacity(nvert * 3);
    for i in 0..nvert {
        let x = i as f32;
        let y = if i % 2 == 0 { 0.0 } else { 1.0 };
        positions.extend_from_slice(&[x, y, 0.0]);
    }
    let mut triangles = Vec::with_capacity(tris * 3);
    for t in 0..tris {
        let i = t as u32;
        if t % 2 == 0 {
            triangles.extend_from_slice(&[i, i + 1, i + 2]);
        } else {
            triangles.extend_from_slice(&[i, i + 2, i + 1]);
        }
    }
    MeshArrays { positions, triangles }
}

/// Measurement exports for `xtask web --measure-preview`. Not in a release wasm: the build
/// passes `--cfg stl_preview_measure` only for that run.
#[cfg(all(target_arch = "wasm32", stl_preview_measure))]
mod measure {
    use super::*;
    use std::cell::RefCell;

    use crate::viewport::renderer::{self, FrameData};

    struct Prep {
        mesh: Arc<MeshArrays>,
        batch: Option<Batch>,
    }

    struct AppRun {
        vp: super::super::Viewport,
        chunks: u32,
    }

    thread_local! {
        static PREP: RefCell<Option<Prep>> = const { RefCell::new(None) };
        static GPU: RefCell<Option<renderer::MeasureGpu>> = const { RefCell::new(None) };
        static APP: RefCell<Option<AppRun>> = const { RefCell::new(None) };
    }

    fn mesh_from_bytes(positions: &[u8], triangles: &[u8]) -> Result<MeshArrays, String> {
        if positions.len() % 4 != 0 || triangles.len() % 4 != 0 {
            return Err("error: bad payload".into());
        }
        let positions: Vec<f32> = positions.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
        let triangles: Vec<u32> = triangles.chunks_exact(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
        if triangles.len() % 3 != 0 || positions.len() % 3 != 0 {
            return Err("error: bad mesh".into());
        }
        Ok(MeshArrays { positions, triangles })
    }

    fn pack_of(mesh: Arc<MeshArrays>, id: &str) -> super::super::PreviewPack {
        super::super::PreviewPack {
            meshes: vec![super::super::PreviewMesh { id: id.into(), mesh }],
            ..Default::default()
        }
    }

    #[wasm_bindgen::prelude::wasm_bindgen]
    pub fn stl_preview_measure_alloc() -> String {
        let mesh = Arc::new(bench_mesh(500_000));
        PREP.with(|m| *m.borrow_mut() = Some(Prep { mesh, batch: None }));
        "ok".into()
    }

    /// The old one-shot preparation: creased normals and the picking tree, no receive and no upload.
    #[wasm_bindgen::prelude::wasm_bindgen]
    pub fn stl_preview_measure_blocking() -> String {
        PREP.with(|slot| {
            let slot = slot.borrow();
            let Some(m) = slot.as_ref() else { return "error: call alloc first".into() };
            let _ = scene::prepare_mesh(&m.mesh);
            let _ = Bvh::build(&m.mesh);
            "ok".into()
        })
    }

    #[wasm_bindgen::prelude::wasm_bindgen]
    pub fn stl_preview_measure_begin() -> String {
        PREP.with(|slot| {
            let mut slot = slot.borrow_mut();
            let Some(m) = slot.as_mut() else { return "error: call alloc first".into() };
            let tree = Arc::new(OnceLock::new());
            m.batch = Some(Batch::new(vec![m.mesh.clone()], vec![tree], Arc::new(AtomicUsize::new(0))));
            "ok".into()
        })
    }

    /// One 4 ms slice of the old prep-only path. `"pending"` until it is prepared, then `"done"`.
    #[wasm_bindgen::prelude::wasm_bindgen]
    pub fn stl_preview_measure_slice() -> String {
        PREP.with(|slot| {
            let mut slot = slot.borrow_mut();
            let Some(m) = slot.as_mut() else { return "error: call alloc first".into() };
            let Some(batch) = m.batch.as_mut() else { return "error: call begin first".into() };
            match batch.poll(Some(SLICE)) {
                Some(_) => "done".into(),
                None => "pending".into(),
            }
        })
    }

    /// Shader compile lives here, outside the timed preview calls.
    #[wasm_bindgen::prelude::wasm_bindgen]
    pub async fn stl_preview_measure_gpu() -> String {
        match renderer::measure_open_gpu().await {
            Ok(gpu) => {
                GPU.with(|g| *g.borrow_mut() = Some(gpu));
                "ok".into()
            }
            Err(e) => format!("error: {e}"),
        }
    }

    fn with_gpu<T>(f: impl FnOnce(&mut renderer::MeasureGpu) -> T) -> Result<T, String> {
        GPU.with(|slot| {
            let mut slot = slot.borrow_mut();
            let Some(gpu) = slot.as_mut() else { return Err("error: call gpu first".into()) };
            Ok(f(gpu))
        })
    }

    fn upload_all(frame: &FrameData) -> Result<u32, String> {
        let mut chunks = 0u32;
        loop {
            chunks += 1;
            if chunks > 10_000 {
                return Err("error: upload did not finish".into());
            }
            let more = with_gpu(|gpu| renderer::measure_upload_frame(gpu, frame))??;
            if !more {
                return Ok(chunks);
            }
        }
    }

    /// Receive the engine's bytes, prepare the preview and upload it, all on this call.
    #[wasm_bindgen::prelude::wasm_bindgen]
    pub fn stl_preview_measure_app_blocking(positions: &[u8], triangles: &[u8]) -> String {
        let mesh = match mesh_from_bytes(positions, triangles) {
            Ok(m) => Arc::new(m),
            Err(e) => return e,
        };
        let mut vp = super::super::Viewport::new();
        vp.show_preview(&pack_of(mesh, "block"));
        if !vp.collect_preview(true) {
            return "error: preview did not install".into();
        }
        let frame = vp.frame_for_size(800.0, 600.0, 1.0);
        match upload_all(&frame) {
            Ok(chunks) => format!("ok chunks={chunks}"),
            Err(e) => e,
        }
    }

    fn app_turn(run: &mut AppRun) -> Result<String, String> {
        let frame = run.vp.frame_for_size(800.0, 600.0, 1.0);
        if run.vp.preview_meshes().next().is_none() {
            return Ok("pending".into());
        }
        let more = with_gpu(|gpu| renderer::measure_upload_frame(gpu, &frame))??;
        run.chunks += 1;
        if more {
            Ok("upload".into())
        } else {
            Ok(format!("done chunks={}", run.chunks))
        }
    }

    /// Copy the payload in, stage the preview, and run the first frame (one slice, plus the
    /// first upload chunk when that slice finishes the preview).
    #[wasm_bindgen::prelude::wasm_bindgen]
    pub fn stl_preview_measure_app_begin(positions: &[u8], triangles: &[u8]) -> String {
        let mesh = match mesh_from_bytes(positions, triangles) {
            Ok(m) => Arc::new(m),
            Err(e) => return e,
        };
        let mut vp = super::super::Viewport::new();
        vp.show_preview(&pack_of(mesh, "slice"));
        let mut run = AppRun { vp, chunks: 0 };
        let status = match app_turn(&mut run) {
            Ok(s) => s,
            Err(e) => return e,
        };
        APP.with(|slot| *slot.borrow_mut() = Some(run));
        status
    }

    /// One more app frame: a prep slice, or one 2 MiB upload chunk once the preview is on screen.
    #[wasm_bindgen::prelude::wasm_bindgen]
    pub fn stl_preview_measure_app_frame() -> String {
        APP.with(|slot| {
            let mut slot = slot.borrow_mut();
            let Some(run) = slot.as_mut() else { return "error: call app_begin first".into() };
            match app_turn(run) {
                Ok(s) => s,
                Err(e) => e,
            }
        })
    }

    #[used]
    static KEEP_ALLOC: fn() -> String = stl_preview_measure_alloc;
    #[used]
    static KEEP_BLOCK: fn() -> String = stl_preview_measure_blocking;
    #[used]
    static KEEP_BEGIN: fn() -> String = stl_preview_measure_begin;
    #[used]
    static KEEP_SLICE: fn() -> String = stl_preview_measure_slice;
    #[used]
    static KEEP_APP_BLOCK: fn(&[u8], &[u8]) -> String = stl_preview_measure_app_blocking;
    #[used]
    static KEEP_APP_BEGIN: fn(&[u8], &[u8]) -> String = stl_preview_measure_app_begin;
    #[used]
    static KEEP_APP_FRAME: fn() -> String = stl_preview_measure_app_frame;
}
