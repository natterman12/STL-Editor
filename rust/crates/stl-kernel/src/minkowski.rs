//! The per-triangle branch of a Minkowski sum or difference with a convex tool.
//!
//! `minkowski.rs` in manifold-rust unions those triangle hulls in batches of 1000,
//! one batch after another, then unions the batches with the original solid.
//! On a part of several thousand triangles almost all of the time is that chain
//! of booleans, and each batch is independent of the others. This runs the same
//! batches at once.
//!
//! The hulls inside a batch stay in triangle order, every batch is still 1000
//! triangles, and the closing union still sees `[solid, batch0, batch1, ...]`
//! with the same operation. A boolean does not change with the thread count, so
//! the mesh stays the one the sequential chain builds. The web build has no
//! thread pool and runs the batches one by one.

use std::cell::Cell;
use std::sync::atomic::{AtomicU64, Ordering};

use manifold_rust::csg_tree::CsgNode;
use manifold_rust::impl_mesh::ManifoldImpl;
use manifold_rust::quickhull::convex_hull;
use manifold_rust::types::{Error, OpType};

thread_local! {
    static FORCE_STOCK: Cell<bool> = const { Cell::new(false) };
    /// `0` means the normal pool. A positive count is a pool built for this call.
    static BATCH_THREADS_OVERRIDE: Cell<usize> = const { Cell::new(0) };
}

static BATCH_PATH_HITS: AtomicU64 = AtomicU64::new(0);

struct RestoreStock(bool);
impl Drop for RestoreStock {
    fn drop(&mut self) {
        FORCE_STOCK.with(|c| c.set(self.0));
    }
}

struct RestoreThreads(usize);
impl Drop for RestoreThreads {
    fn drop(&mut self) {
        BATCH_THREADS_OVERRIDE.with(|c| c.set(self.0));
    }
}

/// How many times the batch path has run in this process. Tests use the
/// difference to check a call did not fall through to the stock path.
pub fn minkowski_batch_path_hits() -> u64 {
    BATCH_PATH_HITS.load(Ordering::Relaxed)
}

/// Run `f` on the stock Minkowski. Tests record that mesh, then compare the
/// batch path with it. The flag is this thread only.
pub fn with_stock_minkowski<R>(f: impl FnOnce() -> R) -> R {
    let prev = FORCE_STOCK.with(|c| c.replace(true));
    let _restore = RestoreStock(prev);
    f()
}

/// Run `f` with the batch pool at `threads` threads (at least 1).
/// `RAYON_NUM_THREADS` does not change this pool. The web build has no pool
/// and ignores the count.
pub fn with_minkowski_batch_threads<R>(threads: usize, f: impl FnOnce() -> R) -> R {
    let prev = BATCH_THREADS_OVERRIDE.with(|c| c.replace(threads.max(1)));
    let _restore = RestoreThreads(prev);
    f()
}

/// Matches `BATCH_SIZE` in manifold-rust's `minkowski.rs`. A different size
/// would pair different hulls and change the mesh.
const BATCH_SIZE: usize = 1000;

/// At most this many threads, and fewer than manifold's big-job pool, so a
/// boolean started here stays on this pool instead of queueing behind every
/// other boolean on the process-wide one.
#[cfg(not(target_arch = "wasm32"))]
const BATCH_THREADS: usize = 8;

/// `Some` when `a` and `b` take the per-triangle branch (a convex tool, and
/// either an inset or a non-convex solid). `None` leaves convex-convex,
/// non-convex-non-convex, empty and non-manifold inputs on the stock path.
pub(crate) fn try_tri_batches(
    a: &ManifoldImpl,
    b: &ManifoldImpl,
    inset: bool,
) -> Option<ManifoldImpl> {
    if FORCE_STOCK.with(|c| c.get()) {
        return None;
    }
    if a.is_soup || b.is_soup || a.status != Error::NoError || b.status != Error::NoError {
        return None;
    }
    if a.is_empty() || b.is_empty() {
        return None;
    }
    let mut a_convex = a.is_convex();
    let mut b_convex = b.is_convex();
    let (a, b) = if a_convex && !b_convex {
        std::mem::swap(&mut a_convex, &mut b_convex);
        (b, a)
    } else {
        (a, b)
    };
    if !((inset || !a_convex) && b_convex) {
        return None;
    }
    // A handful of batches is quicker on the stock path, which maps the hulls
    // of one batch across the threads. Several batches at once is quicker once
    // the part is a few thousand triangles.
    if a.num_tri() < 4 * BATCH_SIZE {
        return None;
    }
    Some(tri_batches(a, b, inset))
}

fn tri_batches(a: &ManifoldImpl, b: &ManifoldImpl, inset: bool) -> ManifoldImpl {
    let num_tri = a.num_tri();
    let n_batches = num_tri.div_ceil(BATCH_SIZE);
    BATCH_PATH_HITS.fetch_add(1, Ordering::Relaxed);
    if std::env::var_os("MANIFOLD_PROFILE").is_some_and(|v| !v.is_empty()) {
        eprintln!("profile minkowski batch-path batches={n_batches} tris={num_tri}");
    }
    let batches = map_batches(n_batches, |batch| {
        let start = batch * BATCH_SIZE;
        let end = (start + BATCH_SIZE).min(num_tri);
        let mut hulls = Vec::with_capacity(end - start);
        for tri in start..end {
            hulls.push(tri_hull(a, b, tri));
        }
        union_all(hulls, OpType::Add)
    });
    let mut parts = Vec::with_capacity(1 + batches.len());
    parts.push(a.clone());
    parts.extend(batches);
    let op = if inset { OpType::Subtract } else { OpType::Add };
    let mut out = union_all(parts, op);
    out.initialize_original();
    out.set_normals_and_coplanar();
    out
}

/// Convex hull of one triangle of `a` plus every vertex of `b`, in the same
/// point order manifold-rust uses (each triangle vertex, then `b`'s vertices).
fn tri_hull(a: &ManifoldImpl, b: &ManifoldImpl, tri: usize) -> ManifoldImpl {
    let mut pts = Vec::with_capacity(3 * b.vert_pos.len());
    for i in 0..3 {
        let a_vert = a.vert_pos[a.halfedge[tri * 3 + i].start_vert as usize];
        for &b_vert in &b.vert_pos {
            pts.push(a_vert + b_vert);
        }
    }
    convex_hull(&pts)
}

fn union_all(meshes: Vec<ManifoldImpl>, op: OpType) -> ManifoldImpl {
    if meshes.is_empty() {
        return ManifoldImpl::new();
    }
    if meshes.len() == 1 {
        return meshes.into_iter().next().unwrap();
    }
    let children: Vec<CsgNode> = meshes.into_iter().map(CsgNode::leaf).collect();
    CsgNode::op_n(op, children).evaluate()
}

fn map_batches<T, F>(n: usize, f: F) -> Vec<T>
where
    T: Send + 'static,
    F: Fn(usize) -> T + Sync + Send,
{
    #[cfg(not(target_arch = "wasm32"))]
    if n >= 2 {
        let override_n = BATCH_THREADS_OVERRIDE.with(|c| c.get());
        if override_n > 0 {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(override_n)
                .thread_name(|i| format!("minkowski-batch-{i}"))
                .build()
                .expect("minkowski batch pool");
            return pool.install(|| manifold_rust::par::maybe_par_map(n, 2, f));
        }
        return batch_pool().install(|| manifold_rust::par::maybe_par_map(n, 2, f));
    }
    manifold_rust::par::maybe_par_map(n, 2, f)
}

#[cfg(not(target_arch = "wasm32"))]
fn batch_pool() -> &'static rayon::ThreadPool {
    use std::sync::OnceLock;
    static POOL: OnceLock<rayon::ThreadPool> = OnceLock::new();
    POOL.get_or_init(|| {
        let n = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
        rayon::ThreadPoolBuilder::new()
            .num_threads(n.clamp(1, BATCH_THREADS))
            .thread_name(|i| format!("minkowski-batch-{i}"))
            .build()
            .expect("minkowski batch pool")
    })
}
