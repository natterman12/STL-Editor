//! Instant overhangs (1.x `viewer.js` `overhangFaces` and app.js `quickPaint`): the faces
//! Support Fins will hold up, painted red over the part straight from the mesh on screen, as
//! an overlay (no mesh rebuild). The engine's exact faces replace them when its build is back.

use std::sync::{Arc, Mutex};

use egui::Color32;
use stl_kernel::MeshArrays;

use super::camera;
use super::Viewport;

/// `0xd64545`: the faces Support Fins holds up (`sf-over`).
pub const OVERHANG_RED: Color32 = Color32::from_rgb(0xd6, 0x45, 0x45);
/// `0xe0a030`: the small ones it leaves to bridge (`sf-small`).
pub const SMALL_AMBER: Color32 = Color32::from_rgb(0xe0, 0xa0, 0x30);

/// 1.x `overhangFaces` (Support Fins' overhang rule), on a body's mesh: faces pointing down
/// more steeply than `threshold_deg` from the plate, not the ones on the bed (top under
/// 0.35 mm), in regions (faces sharing a vertex) of 12 mm² or more.
pub fn overhang_faces(m: &MeshArrays, threshold_deg: f64) -> Vec<u32> {
    let mut w = OverhangWork::new(m, threshold_deg);
    loop {
        if let Some(f) = w.step(m, None) {
            return f;
        }
    }
}

/// [`overhang_faces`] as a job that can be worked on a little at a time (the web has no other
/// thread: each frame gives it a few milliseconds). Same steps, same order, same answer.
pub struct OverhangWork {
    cut: f64,
    phase: u8,
    next: usize,
    min_z: f64,
    over: Vec<bool>,
    area: Vec<f64>,
    parent: Vec<u32>,
    first_at: Vec<u32>,
    sum: Vec<f64>,
    out: Vec<u32>,
}

fn find(parent: &mut [u32], mut x: u32) -> u32 {
    while parent[x as usize] != x {
        parent[x as usize] = parent[parent[x as usize] as usize];
        x = parent[x as usize];
    }
    x
}

impl OverhangWork {
    pub fn new(m: &MeshArrays, threshold_deg: f64) -> OverhangWork {
        let nf = m.triangles.len() / 3;
        OverhangWork {
            cut: -((threshold_deg.to_radians()).cos() + 1e-4),
            phase: 0,
            next: 0,
            min_z: f64::INFINITY,
            over: vec![false; nf],
            area: vec![0.0; nf],
            parent: (0..nf as u32).collect(),
            first_at: vec![u32::MAX; m.positions.len() / 3],
            sum: vec![0.0; nf],
            out: Vec::new(),
        }
    }

    /// Work until `budget` is used up (`None`: to the end). The faces when it has finished.
    pub fn step(&mut self, m: &MeshArrays, budget: Option<std::time::Duration>) -> Option<Vec<u32>> {
        let start = web_time::Instant::now();
        let (pos, idx) = (&m.positions, &m.triangles);
        let nf = idx.len() / 3;
        let nv = pos.len() / 3;
        let p = |v: u32, k: usize| pos[v as usize * 3 + k] as f64;
        let mut n = 0usize;
        loop {
            // the clock is read every 1024 items, not for each one
            if let Some(b) = budget {
                n += 1;
                if n % 1024 == 0 && start.elapsed() >= b {
                    return None;
                }
            }
            let i = self.next;
            match self.phase {
                0 => {
                    if i >= nv {
                        self.phase = 1;
                        self.next = 0;
                        continue;
                    }
                    self.min_z = self.min_z.min(pos[i * 3 + 2] as f64);
                }
                1 => {
                    if i >= nf {
                        self.phase = 2;
                        self.next = 0;
                        continue;
                    }
                    let (a, b, c) = (idx[i * 3], idx[i * 3 + 1], idx[i * 3 + 2]);
                    let top = p(a, 2).max(p(b, 2)).max(p(c, 2)) - self.min_z;
                    if top >= 0.35 {
                        let u = [p(b, 0) - p(a, 0), p(b, 1) - p(a, 1), p(b, 2) - p(a, 2)];
                        let v = [p(c, 0) - p(a, 0), p(c, 1) - p(a, 1), p(c, 2) - p(a, 2)];
                        let nrm = camera::cross(u, v);
                        let len = camera::length(nrm);
                        if len > 0.0 && nrm[2] / len < self.cut {
                            self.over[i] = true;
                            self.area[i] = len / 2.0;
                        }
                    }
                }
                2 => {
                    if i >= nf {
                        self.phase = 3;
                        self.next = 0;
                        continue;
                    }
                    if self.over[i] {
                        for j in 0..3 {
                            let vi = idx[i * 3 + j] as usize;
                            let g = self.first_at[vi];
                            if g == u32::MAX {
                                self.first_at[vi] = i as u32;
                            } else {
                                let (ra, rb) = (find(&mut self.parent, i as u32), find(&mut self.parent, g));
                                if ra != rb {
                                    self.parent[ra as usize] = rb;
                                }
                            }
                        }
                    }
                }
                3 => {
                    // region areas, summed in face order as 1.x's Map does
                    if i >= nf {
                        self.phase = 4;
                        self.next = 0;
                        continue;
                    }
                    if self.over[i] {
                        let r = find(&mut self.parent, i as u32) as usize;
                        self.sum[r] += self.area[i];
                    }
                }
                _ => {
                    if i >= nf {
                        return Some(std::mem::take(&mut self.out));
                    }
                    if self.over[i] && self.sum[find(&mut self.parent, i as u32) as usize] >= 12.0 {
                        self.out.push(i as u32);
                    }
                }
            }
            self.next += 1;
        }
    }
}

/// The faces last worked out, kept while the same mesh and threshold are asked for again.
pub(super) struct OverhangCache {
    mesh: Arc<MeshArrays>,
    thr: f64,
    faces: Arc<Vec<u32>>,
}

/// A count of overhang faces being worked out off the UI thread.
pub(super) struct OverhangJob {
    body: String,
    mesh: Arc<MeshArrays>,
    thr: f64,
    slot: Arc<Mutex<Option<Vec<u32>>>>,
    /// Web: the work, a few milliseconds per frame (there is no other thread).
    work: Option<OverhangWork>,
}

impl Viewport {
    fn cached_overhangs(&self, mesh: &Arc<MeshArrays>, thr: f64) -> Option<Arc<Vec<u32>>> {
        self.overhang_cache.as_ref().filter(|c| Arc::ptr_eq(&c.mesh, mesh) && c.thr == thr).map(|c| c.faces.clone())
    }

    /// `show_overhangs` without making the frame wait: a repeat of the last ask paints from the
    /// cache at once (`Some(faces)`); otherwise the faces are worked out on another thread
    /// (inline on the web, which has none) and painted by [`Viewport::poll_overhangs`] when they
    /// are ready, in one overlay. The paint of an older ask is dropped.
    pub fn show_overhangs_async(&mut self, body: &str, threshold_deg: f64) -> Option<Vec<u32>> {
        self.clear_overlay("sf-over");
        self.clear_overlay("sf-small");
        self.overhang_job = None;
        let mesh = self.body_mesh(body).cloned()?;
        if let Some(f) = self.cached_overhangs(&mesh, threshold_deg) {
            self.highlight_face(body, &f, "sf-over", OVERHANG_RED);
            return Some((*f).clone());
        }
        #[cfg(target_arch = "wasm32")]
        {
            let work = Some(OverhangWork::new(&mesh, threshold_deg));
            self.overhang_job = Some(OverhangJob { body: body.to_owned(), mesh, thr: threshold_deg, slot: Arc::new(Mutex::new(None)), work });
            if let Some(c) = &self.ctx {
                c.request_repaint();
            }
            return None;
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let slot = Arc::new(Mutex::new(None));
            let (m2, s2, ctx) = (mesh.clone(), slot.clone(), self.ctx.clone());
            std::thread::spawn(move || {
                let faces = overhang_faces(&m2, threshold_deg);
                *s2.lock().unwrap_or_else(|e| e.into_inner()) = Some(faces);
                if let Some(c) = ctx {
                    c.request_repaint();
                }
            });
            self.overhang_job = Some(OverhangJob { body: body.to_owned(), mesh, thr: threshold_deg, slot, work: None });
            None
        }
    }

    /// Paint a finished [`Viewport::show_overhangs_async`]: `Some((body, faces))` the frame it lands.
    pub fn poll_overhangs(&mut self) -> Option<(String, usize)> {
        if let Some(job) = self.overhang_job.as_mut() {
            if let Some(w) = job.work.as_mut() {
                // a slice of the work each frame, so none runs past its time
                match w.step(&job.mesh, Some(std::time::Duration::from_millis(4))) {
                    Some(f) => {
                        *job.slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(f);
                        job.work = None;
                    }
                    None => {
                        if let Some(c) = &self.ctx {
                            c.request_repaint();
                        }
                        return None;
                    }
                }
            }
        }
        let job = self.overhang_job.as_ref()?;
        let faces = job.slot.lock().unwrap_or_else(|e| e.into_inner()).take()?;
        let job = self.overhang_job.take()?;
        // the body changed while it ran: the answer is for a mesh that is gone
        if !self.body_mesh(&job.body).is_some_and(|m| Arc::ptr_eq(m, &job.mesh)) {
            return None;
        }
        self.highlight_face(&job.body, &faces, "sf-over", OVERHANG_RED);
        let n = faces.len();
        self.overhang_cache = Some(OverhangCache { mesh: job.mesh, thr: job.thr, faces: Arc::new(faces) });
        Some((job.body, n))
    }

    /// Drop the quick overhang ask that is still being worked out (the exact faces or the coverage
    /// colours have taken its place).
    pub fn cancel_overhangs(&mut self) {
        self.overhang_job = None;
    }

    /// An ask is still being worked out.
    pub fn overhangs_pending(&self) -> bool {
        self.overhang_job.is_some()
    }

    /// `overhangFaces(bodyId, thresholdDeg)`: faces pointing down more steeply than the
    /// threshold, off the bed, in regions of 12 mm² or more, on the mesh on screen.
    pub fn overhang_faces(&self, body: &str, threshold_deg: f64) -> Vec<u32> {
        self.body_mesh(body).map(|m| self.cached_overhangs(m, threshold_deg).map(|f| (*f).clone()).unwrap_or_else(|| overhang_faces(m, threshold_deg))).unwrap_or_default()
    }

    /// The viewer half of `quickPaint`: clear the `sf-over` and `sf-small` overlays, then paint
    /// `body`'s overhangs in [`OVERHANG_RED`] (tag `sf-over`) at once. Returns the faces (none:
    /// the caller says "No overhangs in this position..."). The panel clears `sf-bare` and
    /// `sf-held` and stops coverage itself, as 1.x's `quickPaint` does.
    pub fn show_overhangs(&mut self, body: &str, threshold_deg: f64) -> Vec<u32> {
        self.clear_overlay("sf-over");
        self.clear_overlay("sf-small");
        let faces = self.overhang_faces(body, threshold_deg);
        self.highlight_face(body, &faces, "sf-over", OVERHANG_RED);
        faces
    }

    /// The engine's exact faces when its build is back (`r.paint`): red `over`, amber `small`.
    pub fn paint_overhangs(&mut self, body: &str, over: &[u32], small: &[u32]) {
        // a quick result still on its way would paint over these exact faces
        self.cancel_overhangs();
        self.clear_overlay("sf-over");
        self.clear_overlay("sf-small");
        self.highlight_face(body, over, "sf-over", OVERHANG_RED);
        self.highlight_face(body, small, "sf-small", SMALL_AMBER);
    }
}
