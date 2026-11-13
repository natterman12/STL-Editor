//! The desktop link: the engine on its own thread, fed through a queue.
//!
//! The UI thread never waits: `send` queues a command, `poll` takes what has come back.
//! `Cancel` doesn't wait in the queue; it sets the flag the running command checks.
//!
//! The window is not pinned. On Windows it runs above normal priority, on whichever
//! processor is free. [`reserve_core_for_the_window`] lets the engine build Rayon's pool smaller
//! than the chip: on Windows two physical cores fewer when the chip has hyperthreads, one fewer
//! without them, and two processors fewer where the layout can't be read. So the window and the
//! GPU are not sharing every core. The engine thread and
//! those Rayon workers run below the window. File dialogs, mesh preparation and driver
//! threads stay at normal priority.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex, OnceLock};

use super::worker::Worker;
use super::{Command, EngineLink, Update};

/// What the worker thread is asked to do.
enum Job {
    Run(Command),
    Restart,
}

pub struct NativeLink {
    jobs: Sender<Job>,
    updates: Arc<Mutex<Vec<Update>>>,
    cancel: Arc<AtomicBool>,
    /// Jobs queued or running.
    pending: Arc<AtomicUsize>,
    /// Called after the worker has put updates in the list (wakes the UI).
    wake: Arc<Mutex<Option<Box<dyn Fn() + Send>>>>,
}

impl NativeLink {
    pub fn new() -> NativeLink {
        let (jobs, rx) = channel::<Job>();
        let updates = Arc::new(Mutex::new(Vec::new()));
        let cancel = Arc::new(AtomicBool::new(false));
        let pending = Arc::new(AtomicUsize::new(0));
        let wake: Arc<Mutex<Option<Box<dyn Fn() + Send>>>> = Arc::new(Mutex::new(None));
        let (u, c, p, w) = (updates.clone(), cancel.clone(), pending.clone(), wake.clone());
        std::thread::Builder::new()
            .name("stl-engine".into())
            .spawn(move || {
                // Pool first: Rayon's workers are spawned from this thread, so on Linux they
                // would inherit its nice value if it were lowered before the pool exists.
                if window_core_is_reserved() {
                    install_engine_pool();
                }
                put_this_thread_below_the_window();
                let sink_updates = u.clone();
                let sink_wake = w.clone();
                let sink = Arc::new(move |up: Update| {
                    sink_updates.lock().unwrap_or_else(|e| e.into_inner()).push(up);
                    if let Some(f) = sink_wake.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
                        f();
                    }
                });
                let mut worker = Worker::new(sink, c);
                while let Ok(job) = rx.recv() {
                    match job {
                        Job::Run(cmd) => worker.handle(cmd),
                        Job::Restart => worker.restart(),
                    }
                    p.fetch_sub(1, Ordering::SeqCst);
                }
            })
            .expect("the engine thread starts");
        NativeLink { jobs, updates, cancel, pending, wake }
    }

    /// `f` runs on the engine thread whenever new updates are waiting (for `Context::request_repaint`).
    pub fn set_waker(&self, f: impl Fn() + Send + 'static) {
        *self.wake.lock().unwrap_or_else(|e| e.into_inner()) = Some(Box::new(f));
    }

    /// True while a command is queued or running.
    pub fn is_busy(&self) -> bool {
        self.pending.load(Ordering::SeqCst) > 0
    }

    fn queue(&self, job: Job) {
        self.pending.fetch_add(1, Ordering::SeqCst);
        if self.jobs.send(job).is_err() {
            self.pending.fetch_sub(1, Ordering::SeqCst);
        }
    }
}

impl Default for NativeLink {
    fn default() -> Self {
        NativeLink::new()
    }
}

impl EngineLink for NativeLink {
    fn send(&self, c: Command) {
        match c {
            // only something running can be cancelled; one that arrives while idle is dropped
            Command::Cancel => {
                if self.is_busy() {
                    self.cancel.store(true, Ordering::SeqCst);
                }
            }
            c => self.queue(Job::Run(c)),
        }
    }

    fn poll(&self) -> Vec<Update> {
        std::mem::take(&mut *self.updates.lock().unwrap_or_else(|e| e.into_inner()))
    }

    fn restart(&self) {
        self.queue(Job::Restart);
    }
}

// ---------------------------------------------------------------- window core
//
// Rayon's workers start at normal priority. A pool the size of every logical processor
// fills the machine, and the window misses frames even when it outranks that work. One
// worker per physical core still left a single Hollow frame past 16.7 ms, because a free
// hyperthread shares the core's execution and memory. Two physical cores stay unused by
// the pool, and each worker is lowered as it starts.

/// Set from [`reserve_core_for_the_window`], which runs on the window thread before the
/// engine exists. The engine thread builds Rayon's pool only then, so tests that build a
/// [`NativeLink`] keep the default pool.
static WINDOW_CORE_RESERVED: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy)]
struct Split {
    /// How many Rayon workers to start. The window thread is not one of them.
    background_threads: usize,
}

fn logical_split() -> Split {
    static SPLIT: OnceLock<Split> = OnceLock::new();
    *SPLIT.get_or_init(compute_split)
}

fn compute_split() -> Split {
    #[cfg(windows)]
    if let Some(split) = windows_core_split() {
        return split;
    }
    fallback_split()
}

/// Two fewer Rayon workers than physical cores when the chip has hyperthreads, so two whole
/// cores stay free for the window and the GPU. Without hyperthreads, keep one processor back.
/// `None` when the layout can't be read or the machine has a single core.
#[cfg(windows)]
fn windows_core_split() -> Option<Split> {
    unsafe {
        let mut len = 0u32;
        // The first call only writes the byte count.
        GetLogicalProcessorInformation(std::ptr::null_mut(), &mut len);
        let stride = std::mem::size_of::<LogicalProcessor>();
        if len == 0 || len as usize % stride != 0 {
            return None;
        }
        let n = len as usize / stride;
        let mut buf = vec![LogicalProcessor::zero(); n];
        if GetLogicalProcessorInformation(buf.as_mut_ptr(), &mut len) == 0 {
            return None;
        }
        let cores: Vec<usize> = buf.iter().filter(|e| e.relationship == 0 && e.mask != 0).map(|e| e.mask).collect();
        if cores.len() < 2 {
            return None;
        }
        let physical = cores.len();
        let logical: usize = cores.iter().map(|mask| mask.count_ones() as usize).sum();
        let threads = if logical > physical {
            physical.saturating_sub(2)
        } else {
            physical.saturating_sub(1)
        };
        Some(Split { background_threads: threads.max(1) })
    }
}

fn fallback_split() -> Split {
    let n = std::thread::available_parallelism().map(|p| p.get()).unwrap_or(1);
    // No topology (macOS, Linux, or a Windows lookup that failed): keep two processors
    // back for the window.
    Split { background_threads: n.saturating_sub(2).max(1) }
}

fn window_core_is_reserved() -> bool {
    WINDOW_CORE_RESERVED.load(Ordering::SeqCst)
}

/// Call on the window thread at process start, before the engine thread. On Windows, raises
/// this thread above normal priority. Does not pin it. The engine thread then builds Rayon's
/// global pool (see [`install_engine_pool`]).
pub fn reserve_core_for_the_window() {
    WINDOW_CORE_RESERVED.store(true, Ordering::SeqCst);
    put_window_thread_ahead();
}

/// Once, on the engine thread: Rayon's global pool, sized for this machine, with every
/// worker lowered below the window. A pool that was already started is left as it is.
fn install_engine_pool() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let n = logical_split().background_threads.max(1);
        let built = rayon::ThreadPoolBuilder::new()
            .num_threads(n)
            .thread_name(|i| format!("stl-rayon-{i}"))
            .start_handler(|_| put_this_thread_below_the_window())
            .build_global();
        if let Err(e) = built {
            eprintln!("stl-engine: Rayon pool was already started ({e})");
        }
    });
}

/// Window thread: ahead of the engine. Not pinned. Other platforms leave the default.
#[cfg(windows)]
fn put_window_thread_ahead() {
    unsafe {
        SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_ABOVE_NORMAL);
    }
}

#[cfg(not(windows))]
fn put_window_thread_ahead() {}

/// Engine thread: below the window. Not pinned, so the scheduler can place it anywhere.
#[cfg(windows)]
fn put_this_thread_below_the_window() {
    unsafe {
        SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL);
    }
}

/// Engine thread: below the window. Linux `nice` applies to this thread. macOS `nice` would
/// lower the whole process, so it is left unchanged.
#[cfg(target_os = "linux")]
fn put_this_thread_below_the_window() {
    unsafe extern "C" {
        fn nice(inc: i32) -> i32;
    }
    // Five steps is "below normal".
    unsafe {
        let _ = nice(5);
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
fn put_this_thread_below_the_window() {}

#[cfg(windows)]
const THREAD_PRIORITY_ABOVE_NORMAL: i32 = 1;
#[cfg(windows)]
const THREAD_PRIORITY_BELOW_NORMAL: i32 = -1;

#[cfg(windows)]
#[derive(Clone, Copy)]
#[repr(C)]
struct LogicalProcessor {
    mask: usize,
    relationship: i32,
    // `repr(C)` inserts the padding that aligns the union. On 64-bit that is 4 bytes,
    // which is the layout of SYSTEM_LOGICAL_PROCESSOR_INFORMATION.
    _union: [u64; 2],
}

#[cfg(windows)]
impl LogicalProcessor {
    fn zero() -> Self {
        Self { mask: 0, relationship: 0, _union: [0; 2] }
    }
}

#[cfg(all(windows, target_pointer_width = "64"))]
const _: () = assert!(std::mem::size_of::<LogicalProcessor>() == 32);

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetCurrentThread() -> *mut core::ffi::c_void;
    fn SetThreadPriority(thread: *mut core::ffi::c_void, priority: i32) -> i32;
    fn GetLogicalProcessorInformation(buf: *mut LogicalProcessor, len: *mut u32) -> i32;
}
