// par.rs — determinism-preserving parallel execution helpers
//
// Mirrors the C++ `autoPolicy` pattern (parallel.h): each site opts into
// parallelism above a size threshold, staying sequential for small inputs.
// Unlike upstream MANIFOLD_PAR (which allows nondeterministic vertex order in
// some phases), only sites whose output is provably identical to the
// sequential build are parallelized — per-index maps with indexed writes, and
// collect-then-sort pipelines whose final sort is a total order. This keeps
// the `parallel` feature bit-exact with the sequential reference.
//
// The sites, with the size at which each goes parallel (manifold-sharp's
// Par.cs mirrors this list; keep it current):
//   face_op.rs            `calculate_vert_normals`, per-vertex map    10,000 verts
//                         its first-halfedge search (atomic minimum)  100,000 halfedges
//                         `reorder_halfedges`, per triangle, then a
//                         per-halfedge map of the new pair indices    100,000 halfedges
//   face_op_triangulate.rs `face2tri` general faces, one task each    2 faces
//                         `face2tri` writes, runs of 4,096 faces into
//                         disjoint `split_at_mut` slices             2 runs
//                         output fills and cross-face pairing         100,000
//   boolean3_kernels.rs   `intersect12` queries, runs of 1,024 edges  10,000 edges; a
//                         (one edge per task against a large mesh)    mesh of 10,000 faces
//                         result stable sort and gathers              10,000
//                         `winding03` per-vert winding query map_ct  1,000 verts
//                         (its unions stay sequential)
//   boolean3.rs           `split`: the two results side by side       always
//   boolean_result.rs     `EdgeGroups::new` stable sort of the
//                         AddNewEdgeVerts edge lists                 10,000 entries
//                         `size_output` side counts and normals,
//                         `append_whole_edges` face by face (runs of
//                         4,096 faces), output fills                 100,000
//                         (WHOLE_PAR_THRESHOLD)
//   boolean_result_assemble.rs `update_reference` per triangle        100,000 triangles
//   impl_mesh.rs          `increment_mesh_ids`, `remove_unreferenced_verts` 100,000
//   sort.rs               `sort_geometry`: Morton codes, packed-key
//                         sorts, vertex/face/halfedge/tangent gathers 100,000
//                         (PAR_THRESHOLD; per-call n is verts, tris
//                         or 3 * tris)
//   collider.rs           `Collider::from_leaf_fn`: leaf boxes, radix
//                         tree nodes, box refit by subtree            10,000 leaves
//   edge_op.rs            edge-flag scans of `collapse_short_edges`,
//                         `collapse_colinear_edges`, `swap_degenerates`
//                         (filter; collapses/swaps stay sequential)  100,001 halfedges
//                                                                    (FLAG_PAR_THRESHOLD)
//   edge_op_orbits.rs     `orbit_owners` for `split_pinched_verts` and
//                         `dedupe_edges` (walks capped at 64 steps,
//                         then one sequential pass)                  100,001 halfedges
//                                                                    (ORBIT_PAR_THRESHOLD)
//                         `dedupe_edges` orbit search, runs of 1,024
//                         owners                                      12,500 owners
//   sdf.rs                level-set voxel evaluation                 10,000 voxels
//   minkowski.rs          per-face hull maps                         8
//   robust/intersection_graph.rs (via progress.rs) map_ct stages     64 / 16
//   robust/soup.rs        `compute_self_intersections` rows, any_ct 1,000 triangles
//   with_small_job_pool / with_big_job_pool  capped thread pools for
//                         revolve and boolean/split entry points
//
// The vectors the maps return come from `scratch.rs`, which recycles large
// buffers; that changes where the memory comes from and nothing else.
//
// One site is an existential "any" rather than a map: `maybe_par_any_ct`, the
// Auto engine's self-intersection pre-check
// (robust/soup.rs `compute_self_intersections`, rows of 1,000+ triangles).
// It returns no array but a boolean identical to the sequential loop's,
// because "some index hits" does not depend on which index is found first or
// how many ran (see its doc). Shared with manifold-sharp's
// `Par.MaybeParAnyCt` (its commit 1d9162c).

/// How many threads a job of moderate size (tens of thousands of triangles)
/// uses, see [`with_small_job_pool`]. Measured on one 10-core, 20-thread
/// machine.
pub const SMALL_JOB_THREADS: usize = 8;

/// How many threads a boolean on a large mesh (hundreds of thousands of
/// triangles) uses, see [`with_big_job_pool`]. Measured on one 10-core,
/// 20-thread machine.
pub const BIG_JOB_THREADS: usize = 10;

/// Run `f` so that the parallel passes inside it use at most
/// [`SMALL_JOB_THREADS`] threads.
///
/// A mesh of tens of thousands of triangles is built by a few dozen short
/// passes, each over a few hundred kilobytes. Spread over every thread of a
/// machine with hyperthreads (20 on a 10-core CPU) each pass spends more on
/// waking the threads, and on the second thread of a core fighting the first
/// for its cache, than it saves: a 51,000-triangle `revolve` took 16 ms on 20
/// threads and 11 ms on 8, and 23 ms on one. Only the number of threads
/// changes; every pass gives the result it gives on any other count.
///
/// When the current pool already has `SMALL_JOB_THREADS` threads or fewer
/// (including the small pool itself), `f` runs directly.
#[cfg(feature = "parallel")]
pub fn with_small_job_pool<R: Send>(f: impl FnOnce() -> R + Send) -> R {
    use std::sync::OnceLock;
    static POOL: OnceLock<Option<rayon::ThreadPool>> = OnceLock::new();
    with_capped_pool(&POOL, SMALL_JOB_THREADS, "manifold-small", f)
}

/// Run `f` so that the parallel passes inside it use at most
/// [`BIG_JOB_THREADS`] threads.
///
/// The same reasoning as [`with_small_job_pool`] for a boolean on a mesh of
/// half a million triangles: its passes are short and bound by memory, and on a
/// 10-core, 20-thread CPU a union with a cube took 125 ms on 20 threads, 109 ms
/// on 8, 107 ms on 10, 109 ms on 12 and 118 ms on 16. Only the number of
/// threads changes; every pass gives the result it gives on any other count.
///
/// When the current pool already has `BIG_JOB_THREADS` threads or fewer
/// (including the pool made here), `f` runs directly.
#[cfg(feature = "parallel")]
pub fn with_big_job_pool<R: Send>(f: impl FnOnce() -> R + Send) -> R {
    use std::sync::OnceLock;
    static POOL: OnceLock<Option<rayon::ThreadPool>> = OnceLock::new();
    with_capped_pool(&POOL, BIG_JOB_THREADS, "manifold-big", f)
}

#[cfg(feature = "parallel")]
fn with_capped_pool<R: Send>(
    pool: &'static std::sync::OnceLock<Option<rayon::ThreadPool>>,
    threads: usize,
    name: &'static str,
    f: impl FnOnce() -> R + Send,
) -> R {
    if rayon::current_num_threads() <= threads {
        return f();
    }
    let pool = pool.get_or_init(|| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .thread_name(move |i| format!("{name}-{i}"))
            .build()
            .ok()
    });
    match pool {
        Some(pool) => pool.install(f),
        None => f(),
    }
}

/// Sequential build: nothing to limit.
#[cfg(not(feature = "parallel"))]
pub fn with_small_job_pool<R>(f: impl FnOnce() -> R) -> R {
    f()
}

/// Sequential build: nothing to limit.
#[cfg(not(feature = "parallel"))]
pub fn with_big_job_pool<R>(f: impl FnOnce() -> R) -> R {
    f()
}

/// Map `f` over `0..n`, in parallel when the `parallel` feature is enabled and
/// `n >= threshold`. Results are returned in index order either way.
///
/// The vector comes from [`crate::scratch`], so a large one reuses a buffer
/// that an earlier array of the same type gave back.
#[cfg(feature = "parallel")]
pub fn maybe_par_map<T, F>(n: usize, threshold: usize, f: F) -> Vec<T>
where
    T: Send + 'static,
    F: Fn(usize) -> T + Sync + Send,
{
    use rayon::prelude::*;
    let mut out = crate::scratch::take(n);
    if n >= threshold {
        // An indexed iterator: rayon writes each result straight into its slot.
        out.par_extend((0..n).into_par_iter().map(f));
    } else {
        out.extend((0..n).map(f));
    }
    out
}

/// Sequential fallback: identical output to the parallel version.
#[cfg(not(feature = "parallel"))]
pub fn maybe_par_map<T, F>(n: usize, _threshold: usize, f: F) -> Vec<T>
where
    T: Send + 'static,
    F: Fn(usize) -> T,
{
    let mut out = crate::scratch::take(n);
    out.extend((0..n).map(f));
    out
}

/// The indices in `0..n` where `pred` holds, ascending, testing in parallel
/// when `n >= threshold`: the flag half of C++ `FlagStore::run_par`
/// (edge_op.cpp:54-84). rayon's `collect` keeps the order, so unlike C++ no
/// sort is needed.
#[cfg(feature = "parallel")]
pub fn maybe_par_filter<F>(n: usize, threshold: usize, pred: F) -> Vec<usize>
where
    F: Fn(usize) -> bool + Sync + Send,
{
    use rayon::prelude::*;
    if n >= threshold {
        (0..n).into_par_iter().filter(|&i| pred(i)).collect()
    } else {
        (0..n).filter(|&i| pred(i)).collect()
    }
}

/// Sequential fallback: identical output to the parallel version.
#[cfg(not(feature = "parallel"))]
pub fn maybe_par_filter<F>(n: usize, _threshold: usize, pred: F) -> Vec<usize>
where
    F: Fn(usize) -> bool,
{
    (0..n).filter(|&i| pred(i)).collect()
}

/// A stable sort by key, in parallel when `v.len() >= threshold`. A stable
/// sort is determined by the keys and input order, so rayon's gives the same
/// slice, provided `key` is a total order.
#[cfg(feature = "parallel")]
pub fn maybe_par_sort_by_key<T, K, F>(v: &mut [T], threshold: usize, key: F)
where
    T: Send,
    K: Ord,
    F: Fn(&T) -> K + Sync,
{
    use rayon::prelude::*;
    if v.len() >= threshold {
        v.par_sort_by_key(key);
    } else {
        v.sort_by_key(key);
    }
}

/// Sequential fallback: identical output to the parallel version.
#[cfg(not(feature = "parallel"))]
pub fn maybe_par_sort_by_key<T, K, F>(v: &mut [T], _threshold: usize, key: F)
where
    K: Ord,
    F: Fn(&T) -> K,
{
    v.sort_by_key(key);
}

/// Sort a slice whose elements are all different, in parallel when
/// `v.len() >= threshold`. With no two elements equal there is one sorted
/// order, so an unstable sort gives it at any thread count. Callers pack an
/// index into the low bits of each key to make that so, which also makes the
/// result the stable sort of the keys alone.
#[cfg(feature = "parallel")]
pub fn maybe_par_sort_distinct<T: Ord + Send>(v: &mut [T], threshold: usize) {
    use rayon::prelude::*;
    if v.len() >= threshold {
        v.par_sort_unstable();
    } else {
        v.sort_unstable();
    }
}

/// Sequential fallback: identical output to the parallel version.
#[cfg(not(feature = "parallel"))]
pub fn maybe_par_sort_distinct<T: Ord>(v: &mut [T], _threshold: usize) {
    v.sort_unstable();
}

/// [`maybe_par_map`] for a function that gives a pair, returning the firsts and
/// the seconds as two vectors, each in index order.
#[cfg(feature = "parallel")]
pub fn maybe_par_map_unzip<A, B, F>(n: usize, threshold: usize, f: F) -> (Vec<A>, Vec<B>)
where
    A: Send + 'static,
    B: Send + 'static,
    F: Fn(usize) -> (A, B) + Sync + Send,
{
    use rayon::prelude::*;
    let mut firsts = crate::scratch::take(n);
    let mut seconds = crate::scratch::take(n);
    if n >= threshold {
        (0..n)
            .into_par_iter()
            .map(f)
            .unzip_into_vecs(&mut firsts, &mut seconds);
    } else {
        for (a, b) in (0..n).map(f) {
            firsts.push(a);
            seconds.push(b);
        }
    }
    (firsts, seconds)
}

/// Sequential fallback: identical output to the parallel version.
#[cfg(not(feature = "parallel"))]
pub fn maybe_par_map_unzip<A, B, F>(n: usize, _threshold: usize, f: F) -> (Vec<A>, Vec<B>)
where
    A: Send + 'static,
    B: Send + 'static,
    F: Fn(usize) -> (A, B),
{
    let mut firsts = crate::scratch::take(n);
    let mut seconds = crate::scratch::take(n);
    for (a, b) in (0..n).map(f) {
        firsts.push(a);
        seconds.push(b);
    }
    (firsts, seconds)
}

/// Run `f` on every item, in parallel when `items.len() >= threshold`. Items
/// must own disjoint output, so the run order cannot change what is written.
#[cfg(feature = "parallel")]
pub fn maybe_par_for_each<T, F>(items: Vec<T>, threshold: usize, f: F)
where
    T: Send,
    F: Fn(T) + Sync + Send,
{
    use rayon::prelude::*;
    if items.len() >= threshold {
        items.into_par_iter().for_each(f);
    } else {
        items.into_iter().for_each(f);
    }
}

/// Sequential fallback: identical output to the parallel version.
#[cfg(not(feature = "parallel"))]
pub fn maybe_par_for_each<T, F>(items: Vec<T>, _threshold: usize, f: F)
where
    F: Fn(T),
{
    items.into_iter().for_each(f);
}

/// Apply `f` to every element in place, in parallel when `v.len() >=
/// threshold`. `f` sees only its own element, so the result does not depend on
/// the order.
#[cfg(feature = "parallel")]
pub fn maybe_par_for_each_mut<T, F>(v: &mut [T], threshold: usize, f: F)
where
    T: Send,
    F: Fn(&mut T) + Sync + Send,
{
    use rayon::prelude::*;
    if v.len() >= threshold {
        v.par_iter_mut().for_each(f);
    } else {
        v.iter_mut().for_each(f);
    }
}

/// Sequential fallback: identical output to the parallel version.
#[cfg(not(feature = "parallel"))]
pub fn maybe_par_for_each_mut<T, F>(v: &mut [T], _threshold: usize, f: F)
where
    F: Fn(&mut T),
{
    v.iter_mut().for_each(f);
}

/// [`maybe_par_map`] with cooperative cancellation: `None` means the token was
/// cancelled and the (necessarily incomplete) results were discarded.
///
/// Mirrors the ctx-aware `for_each` overload in C++ `parallel.h:400-437`, whose
/// contract is the same — "only safe when *skip the rest of the range* produces
/// a result the caller will discard via a post-loop `IsCancelled` check".
///
/// Two deliberate differences from the C++:
/// - A `None` token dispatches straight to [`maybe_par_map`], so the
///   uncancellable path is byte-for-byte the code that ran before cancellation
///   existed — stricter than C++, which still branches on `ctx != nullptr`
///   inside the loop.
/// - With a token we check the flag on *every* element rather than once per
///   `kSeqCancelChunk` (1024) elements. The flag is written at most once, so its
///   cache line stays shared and the relaxed load is an L1 hit; paying it per
///   element buys strictly better cancellation latency than the C++ chunking.
#[cfg(feature = "parallel")]
pub fn maybe_par_map_ct<T, F>(
    n: usize,
    threshold: usize,
    token: Option<&crate::cancel::CancelToken>,
    f: F,
) -> Option<Vec<T>>
where
    T: Send + 'static,
    F: Fn(usize) -> T + Sync + Send,
{
    use rayon::prelude::*;
    let Some(token) = token else {
        return Some(maybe_par_map(n, threshold, f));
    };
    // Collecting `Option<T>` into `Option<Vec<T>>` short-circuits on the first
    // `None` in both rayon and std, so a cancel stops the remaining work
    // instead of merely skipping each element's body.
    if n >= threshold {
        (0..n)
            .into_par_iter()
            .map(|i| {
                if token.is_cancelled() {
                    None
                } else {
                    Some(f(i))
                }
            })
            .collect()
    } else {
        (0..n)
            .map(|i| {
                if token.is_cancelled() {
                    None
                } else {
                    Some(f(i))
                }
            })
            .collect()
    }
}

/// Sequential fallback: identical output to the parallel version.
#[cfg(not(feature = "parallel"))]
pub fn maybe_par_map_ct<T, F>(
    n: usize,
    threshold: usize,
    token: Option<&crate::cancel::CancelToken>,
    f: F,
) -> Option<Vec<T>>
where
    T: Send + 'static,
    F: Fn(usize) -> T,
{
    let Some(token) = token else {
        return Some(maybe_par_map(n, threshold, f));
    };
    (0..n)
        .map(|i| {
            if token.is_cancelled() {
                None
            } else {
                Some(f(i))
            }
        })
        .collect()
}

/// Run two tasks, side by side with the `parallel` feature, and return both
/// results. Each must be independent of the other.
#[cfg(feature = "parallel")]
pub fn maybe_par_join<A, B, RA, RB>(a: A, b: B) -> (RA, RB)
where
    A: FnOnce() -> RA + Send,
    B: FnOnce() -> RB + Send,
    RA: Send,
    RB: Send,
{
    rayon::join(a, b)
}

/// Sequential fallback: identical output to the parallel version.
#[cfg(not(feature = "parallel"))]
pub fn maybe_par_join<A, B, RA, RB>(a: A, b: B) -> (RA, RB)
where
    A: FnOnce() -> RA,
    B: FnOnce() -> RB,
{
    (a(), b())
}

/// A vector of `n` copies of `value`, written in parallel when
/// `n >= threshold`.
pub fn maybe_par_filled<T>(n: usize, threshold: usize, value: T) -> Vec<T>
where
    T: Copy + Send + Sync + 'static,
{
    maybe_par_map(n, threshold, move |_| value)
}

/// Call `f` for every index in `0..n`, in parallel when `n >= threshold`. `f`
/// must give the same end state in any order (it writes through atomics, or
/// not at all).
#[cfg(feature = "parallel")]
pub fn maybe_par_for_range<F>(n: usize, threshold: usize, f: F)
where
    F: Fn(usize) + Sync + Send,
{
    use rayon::prelude::*;
    if n >= threshold {
        (0..n).into_par_iter().for_each(f);
    } else {
        (0..n).for_each(f);
    }
}

/// Sequential fallback: identical output to the parallel version.
#[cfg(not(feature = "parallel"))]
pub fn maybe_par_for_range<F>(n: usize, _threshold: usize, f: F)
where
    F: Fn(usize),
{
    (0..n).for_each(f);
}

/// Run `f` on every chunk of `size` elements (the last may be shorter), in
/// parallel when `v.len() >= threshold`. `f` sees only its own chunk.
#[cfg(feature = "parallel")]
pub fn maybe_par_for_each_chunk_mut<T, F>(v: &mut [T], size: usize, threshold: usize, f: F)
where
    T: Send,
    F: Fn(&mut [T]) + Sync + Send,
{
    use rayon::prelude::*;
    if v.len() >= threshold {
        v.par_chunks_mut(size).for_each(f);
    } else {
        v.chunks_mut(size).for_each(f);
    }
}

/// Sequential fallback: identical output to the parallel version.
#[cfg(not(feature = "parallel"))]
pub fn maybe_par_for_each_chunk_mut<T, F>(v: &mut [T], size: usize, _threshold: usize, f: F)
where
    F: Fn(&mut [T]),
{
    v.chunks_mut(size).for_each(f);
}

/// [`maybe_par_for_each_mut`] for a function that is also given the index of
/// the element.
#[cfg(feature = "parallel")]
pub fn maybe_par_for_each_indexed_mut<T, F>(v: &mut [T], threshold: usize, f: F)
where
    T: Send,
    F: Fn(usize, &mut T) + Sync + Send,
{
    use rayon::prelude::*;
    if v.len() >= threshold {
        v.par_iter_mut().enumerate().for_each(|(i, x)| f(i, x));
    } else {
        v.iter_mut().enumerate().for_each(|(i, x)| f(i, x));
    }
}

/// Sequential fallback: identical output to the parallel version.
#[cfg(not(feature = "parallel"))]
pub fn maybe_par_for_each_indexed_mut<T, F>(v: &mut [T], _threshold: usize, f: F)
where
    F: Fn(usize, &mut T),
{
    v.iter_mut().enumerate().for_each(|(i, x)| f(i, x));
}

/// [`maybe_par_map_ct`] for a short list of items of very uneven cost: in
/// parallel every index is its own task, so two slow items never wait for one
/// another on the same thread. Results are in index order either way.
#[cfg(feature = "parallel")]
pub fn maybe_par_map_each_ct<T, F>(
    n: usize,
    threshold: usize,
    token: Option<&crate::cancel::CancelToken>,
    f: F,
) -> Option<Vec<T>>
where
    T: Send + 'static,
    F: Fn(usize) -> T + Sync + Send,
{
    use rayon::prelude::*;
    if n < threshold {
        return maybe_par_map_ct(n, usize::MAX, token, f);
    }
    (0..n)
        .into_par_iter()
        .with_max_len(1)
        .map(|i| {
            if token.is_some_and(|t| t.is_cancelled()) {
                None
            } else {
                Some(f(i))
            }
        })
        .collect()
}

/// Sequential fallback: identical output to the parallel version.
#[cfg(not(feature = "parallel"))]
pub fn maybe_par_map_each_ct<T, F>(
    n: usize,
    threshold: usize,
    token: Option<&crate::cancel::CancelToken>,
    f: F,
) -> Option<Vec<T>>
where
    T: Send + 'static,
    F: Fn(usize) -> T,
{
    maybe_par_map_ct(n, threshold, token, f)
}

/// Does `predicate` hold for some index in `0..n`? Stops at the first hit, in
/// parallel when the `parallel` feature is enabled and `n >= threshold`.
/// `None` means the token was cancelled before a verdict.
///
/// The one existential site (the Auto engine's self-intersection pre-check,
/// `robust::soup::compute_self_intersections`). Its answer is a single
/// boolean rather than an array, so the index-order argument the maps above
/// rest on becomes this one: the answer is "some index's predicate is true",
/// and an existential does not depend on which index is found first or how
/// many ran. A hit, from any worker, answers `Some(true)`; a run in which
/// every index ran without a hit answers `Some(false)`. Both are exactly what
/// the sequential loop answers, so the verdict is the same in both builds;
/// only *which* hit stops the loop differs, and the caller never learns which.
///
/// `predicate` must be pure apart from its per-job scratch `L`, made by
/// `local_init` (rayon's `*_init`: one per split job in parallel, one for the
/// whole sequential loop).
///
/// Cancellation: a cancel any worker observed answers `None`, unless some
/// worker had already found a hit, which answers `Some(true)` — a real
/// verdict (the hit is a genuine witness), not a cancelled partial result.
/// The sequential loop polls the token before every index.
#[cfg(feature = "parallel")]
pub fn maybe_par_any_ct<L, I, F>(
    n: usize,
    threshold: usize,
    token: Option<&crate::cancel::CancelToken>,
    local_init: I,
    predicate: F,
) -> Option<bool>
where
    I: Fn() -> L + Sync + Send,
    F: Fn(usize, &mut L) -> bool + Sync + Send,
{
    use rayon::prelude::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    if n < threshold {
        return seq_any_ct(n, token, local_init, predicate);
    }
    // Written only ever from false to true, so two racing hits are benign.
    let found = AtomicBool::new(false);
    let stopped = (0..n)
        .into_par_iter()
        .try_for_each_init(local_init, |local, i| {
            if token.is_some_and(|t| t.is_cancelled()) {
                return Err(());
            }
            if predicate(i, local) {
                found.store(true, Ordering::Relaxed);
                return Err(());
            }
            Ok(())
        })
        .is_err();
    if found.load(Ordering::Relaxed) {
        Some(true)
    } else if stopped {
        // Stopped without a hit means some worker stopped on the cancel.
        None
    } else {
        Some(false)
    }
}

/// Sequential fallback: identical verdict to the parallel version.
#[cfg(not(feature = "parallel"))]
pub fn maybe_par_any_ct<L, I, F>(
    n: usize,
    _threshold: usize,
    token: Option<&crate::cancel::CancelToken>,
    local_init: I,
    predicate: F,
) -> Option<bool>
where
    I: Fn() -> L,
    F: Fn(usize, &mut L) -> bool,
{
    seq_any_ct(n, token, local_init, predicate)
}

/// The sequential loop both builds' [`maybe_par_any_ct`] reduce to.
fn seq_any_ct<L, I, F>(
    n: usize,
    token: Option<&crate::cancel::CancelToken>,
    local_init: I,
    predicate: F,
) -> Option<bool>
where
    I: Fn() -> L,
    F: Fn(usize, &mut L) -> bool,
{
    let mut local = local_init();
    for i in 0..n {
        if token.is_some_and(|t| t.is_cancelled()) {
            return None;
        }
        if predicate(i, &mut local) {
            return Some(true);
        }
    }
    Some(false)
}

#[cfg(test)]
#[path = "par_tests.rs"]
mod tests;
