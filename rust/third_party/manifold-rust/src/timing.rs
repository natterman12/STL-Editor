// Lightweight, env-gated phase timing for the boolean pipeline, mirroring the
// C++ MANIFOLD_TIMING instrumentation (Timer::Print in boolean3.cpp /
// boolean_result.cpp). Enabled by setting the MANIFOLD_TIMING environment
// variable to any non-empty value; otherwise every call is a no-op so release
// performance is unaffected. Used to compare per-stage wall-clock against the
// C++ reference when hunting performance gaps (see CLAUDE.md "Instrumentation
// Strategy").

use std::sync::OnceLock;
use std::time::Instant;

fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("MANIFOLD_TIMING").is_ok_and(|v| !v.is_empty()))
}

/// Start a stage timer. Returns None (and times nothing) unless the
/// MANIFOLD_TIMING environment variable is set.
pub(crate) fn start() -> Option<Instant> {
    if enabled() {
        Some(Instant::now())
    } else {
        None
    }
}

/// Platform-safe stopwatch for ALWAYS-ON aggregate instrumentation (hot-path
/// counters that accumulate into atomics). `std::time::Instant::now()`
/// PANICS on wasm32-unknown-unknown ("time not implemented on this
/// platform"), so any unconditional timing in library code must go through
/// this type — on wasm it measures nothing and reports zero.
#[derive(Clone, Copy)]
pub(crate) struct Stopwatch {
    #[cfg(not(target_arch = "wasm32"))]
    t0: Instant,
}

impl Stopwatch {
    #[inline]
    pub fn start() -> Self {
        Stopwatch {
            #[cfg(not(target_arch = "wasm32"))]
            t0: Instant::now(),
        }
    }

    #[inline]
    pub fn elapsed_ns(self) -> u64 {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.t0.elapsed().as_nanos() as u64
        }
        #[cfg(target_arch = "wasm32")]
        {
            0
        }
    }

    #[inline]
    pub fn elapsed_secs(self) -> f64 {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.t0.elapsed().as_secs_f64()
        }
        #[cfg(target_arch = "wasm32")]
        {
            0.0
        }
    }
}

/// Optional memory reporter, registered by profiling harnesses (e.g. the
/// mem_profile example's counting allocator). Returns (current bytes, peak
/// bytes since the previous call) — the implementation resets its peak
/// watermark on read so each stage line reports that stage's own peak.
pub type MemHook = fn() -> (usize, usize);

static MEM_HOOK: OnceLock<MemHook> = OnceLock::new();

pub fn set_mem_hook(hook: MemHook) {
    let _ = MEM_HOOK.set(hook);
}

/// Print a counter/diagnostic line, gated on the same MANIFOLD_TIMING
/// switch as the stage timers.
pub(crate) fn print_count(label: &str) {
    if enabled() {
        eprintln!("{label}");
    }
}

/// A second switch, `MANIFOLD_PROFILE`, for summaries that must stay quiet
/// when `MANIFOLD_TIMING` would print a line per boolean. Off unless the
/// variable is set and non-empty, so a release build that leaves it unset
/// does not read the clock.
fn profile_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("MANIFOLD_PROFILE").is_ok_and(|v| !v.is_empty()))
}

/// Halfedges (or 3× triangles) above which a per-call profile line is worth
/// printing. Smaller meshes, such as one Minkowski hull, stay in the
/// accumulators instead of flooding stderr.
pub(crate) const PROFILE_BIG: usize = 300_000;

#[inline]
pub(crate) fn profile_start() -> Option<Instant> {
    if profile_enabled() {
        Some(Instant::now())
    } else {
        None
    }
}

/// Print one `profile <label>: N sec` line. No-op when `t0` is `None`.
pub(crate) fn profile_print(label: &str, t0: Option<Instant>) {
    if let Some(t0) = t0 {
        eprintln!("profile {}: {:.6} sec", label, t0.elapsed().as_secs_f64());
    }
}

/// Prints on drop when `show` is set and profiling is on. `show` is the
/// caller's size gate so tiny meshes do not each emit a line.
pub(crate) struct ProfileSpan {
    label: &'static str,
    t0: Option<Instant>,
}

impl ProfileSpan {
    #[inline]
    pub fn big(label: &'static str, n: usize) -> Self {
        Self {
            label,
            t0: if n >= PROFILE_BIG {
                profile_start()
            } else {
                None
            },
        }
    }
}

impl Drop for ProfileSpan {
    fn drop(&mut self) {
        profile_print(self.label, self.t0.take());
    }
}

/// Nanoseconds accumulated across parallel hulls. Only touched when profiling
/// is on. [`profile_take_secs`] reads and clears.
pub(crate) static PROFILE_HULL_BUILD: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);
pub(crate) static PROFILE_HULL_FINISH: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

#[inline]
pub(crate) fn profile_reset_hulls() {
    if profile_enabled() {
        use std::sync::atomic::Ordering::Relaxed;
        PROFILE_HULL_BUILD.store(0, Relaxed);
        PROFILE_HULL_FINISH.store(0, Relaxed);
    }
}

#[inline]
pub(crate) fn profile_add(slot: &std::sync::atomic::AtomicU64, t0: Option<Instant>) {
    if let Some(t0) = t0 {
        slot.fetch_add(t0.elapsed().as_nanos() as u64, std::sync::atomic::Ordering::Relaxed);
    }
}

pub(crate) fn profile_take_secs(slot: &std::sync::atomic::AtomicU64) -> f64 {
    slot.swap(0, std::sync::atomic::Ordering::Relaxed) as f64 / 1e9
}

/// Boolean wall time by the sum of the two meshes' triangle counts, plus the
/// five heavy stages. Parallel booleans add into the same atomics.
struct ProfileBool {
    ns: [std::sync::atomic::AtomicU64; 4],
    n: [std::sync::atomic::AtomicU64; 4],
}

const fn profile_bool_new() -> ProfileBool {
    const Z: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    ProfileBool {
        ns: [Z, Z, Z, Z],
        n: [Z, Z, Z, Z],
    }
}

static PROFILE_BOOL: ProfileBool = profile_bool_new();
pub(crate) static PROFILE_INTERSECT: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);
pub(crate) static PROFILE_ASSEMBLY: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);
pub(crate) static PROFILE_TRIANGULATE: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);
pub(crate) static PROFILE_SIMPLIFY: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);
pub(crate) static PROFILE_SORTING: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

fn bool_bucket(ntri: usize) -> usize {
    match ntri {
        0..200 => 0,
        200..2_000 => 1,
        2_000..20_000 => 2,
        _ => 3,
    }
}

const BOOL_LABELS: [&str; 4] = ["<200", "<2k", "<20k", ">=20k"];

/// Times one boolean call, including the fast paths. No clock unless profiling.
pub(crate) struct BoolSpan {
    t0: Option<Instant>,
    ntri: usize,
}

impl BoolSpan {
    #[inline]
    pub fn new(ntri: usize) -> Self {
        Self {
            t0: profile_start(),
            ntri,
        }
    }
}

impl Drop for BoolSpan {
    fn drop(&mut self) {
        let Some(t0) = self.t0.take() else {
            return;
        };
        let b = bool_bucket(self.ntri);
        use std::sync::atomic::Ordering::Relaxed;
        PROFILE_BOOL.ns[b].fetch_add(t0.elapsed().as_nanos() as u64, Relaxed);
        PROFILE_BOOL.n[b].fetch_add(1, Relaxed);
    }
}

pub(crate) fn profile_reset_bools() {
    if !profile_enabled() {
        return;
    }
    use std::sync::atomic::Ordering::Relaxed;
    for b in 0..4 {
        PROFILE_BOOL.ns[b].store(0, Relaxed);
        PROFILE_BOOL.n[b].store(0, Relaxed);
    }
    PROFILE_INTERSECT.store(0, Relaxed);
    PROFILE_ASSEMBLY.store(0, Relaxed);
    PROFILE_TRIANGULATE.store(0, Relaxed);
    PROFILE_SIMPLIFY.store(0, Relaxed);
    PROFILE_SORTING.store(0, Relaxed);
}

pub(crate) fn profile_dump_bools() {
    if !profile_enabled() {
        return;
    }
    use std::sync::atomic::Ordering::Relaxed;
    for b in 0..4 {
        let n = PROFILE_BOOL.n[b].swap(0, Relaxed);
        let ns = PROFILE_BOOL.ns[b].swap(0, Relaxed);
        if n > 0 {
            eprintln!(
                "profile bool {} x{n}: {:.3} sec",
                BOOL_LABELS[b],
                ns as f64 / 1e9
            );
        }
    }
    for (label, slot) in [
        ("intersect", &PROFILE_INTERSECT),
        ("assembly", &PROFILE_ASSEMBLY),
        ("triangulate", &PROFILE_TRIANGULATE),
        ("simplify", &PROFILE_SIMPLIFY),
        ("sorting", &PROFILE_SORTING),
    ] {
        let ns = slot.swap(0, Relaxed);
        if ns > 0 {
            eprintln!("profile stage {label}: {:.3} sec", ns as f64 / 1e9);
        }
    }
}

/// Print the elapsed time for a stage started with `start`, matching the C++
/// Timer::Print format ("label: N sec") on stderr. If a memory hook is
/// registered, appends current/stage-peak heap use.
pub(crate) fn print(label: &str, t0: Option<Instant>) {
    if let Some(t0) = t0 {
        match MEM_HOOK.get() {
            Some(hook) => {
                let (current, peak) = hook();
                eprintln!(
                    "{}: {} sec, current = {:.1} MB, stage peak = {:.1} MB",
                    label,
                    t0.elapsed().as_secs_f64(),
                    current as f64 / 1048576.0,
                    peak as f64 / 1048576.0
                );
            }
            None => eprintln!("{}: {} sec", label, t0.elapsed().as_secs_f64()),
        }
    }
}
