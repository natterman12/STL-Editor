//! The kernel's buffer pool (kept inside manifold-rust) and when it is emptied.
//!
//! Inside and between operations the pool lets big buffers be reused, which saves a lot of
//! time. So that the app does not sit on up to 512 MB while idle, the pool is emptied:
//!
//! * after about 5 seconds with no kernel operation (`set_idle_timeout` changes this),
//! * by `release_scratch()`, which the engine calls at the end of a rebuild and when a project
//!   is closed.
//!
//! An operation (one feature evaluation) wraps itself in a [`ScratchGuard`]. The pool is never
//! emptied while a guard is alive: the idle check and `release_scratch` both take the same lock
//! that `ScratchGuard::begin` takes. Results never depend on the pool, only speed does.
//!
//! **Native**: one background thread (std only, started the first time an operation ends)
//! sleeps until the idle time is up and then empties the pool.
//! **wasm32**: there are no threads and no `std::time::Instant`, so nothing runs by itself.
//! The host gives a clock with [`set_clock`] (for example `performance.now()` in milliseconds);
//! at the start of the next operation the pool is emptied if more than the idle time passed
//! since the last one ended. With no clock set, the pool is never emptied on idle (it still is
//! by `release_scratch()`, and it is capped at 64 MB on wasm32).

use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

/// How long the kernel may sit idle before the pool is emptied.
pub const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(5);

struct State {
    /// Operations running right now.
    active: usize,
    /// When the last operation ended (time since the clock's start), if the pool may hold something.
    last_end: Option<Duration>,
    timeout: Duration,
}

static STATE: Mutex<State> = Mutex::new(State { active: 0, last_end: None, timeout: DEFAULT_IDLE_TIMEOUT });

fn lock() -> MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

fn clear_pool() {
    // a panic while clearing must not kill the idle timer thread for good
    let _ = std::panic::catch_unwind(manifold_rust::scratch::clear);
}

// ---- the clock ----

#[cfg(not(target_arch = "wasm32"))]
fn now() -> Option<Duration> {
    use std::sync::OnceLock;
    use std::time::Instant;
    static BASE: OnceLock<Instant> = OnceLock::new();
    Some(BASE.get_or_init(Instant::now).elapsed())
}

#[cfg(target_arch = "wasm32")]
static CLOCK: Mutex<Option<fn() -> f64>> = Mutex::new(None);

#[cfg(target_arch = "wasm32")]
fn now() -> Option<Duration> {
    let f = (*CLOCK.lock().unwrap_or_else(|e| e.into_inner()))?;
    let ms = f();
    (ms.is_finite() && ms >= 0.0).then(|| Duration::from_secs_f64(ms / 1000.0))
}

/// wasm32 only: the clock the host provides, returning milliseconds (any fixed start, never going back).
#[cfg(target_arch = "wasm32")]
pub fn set_clock(clock: fn() -> f64) {
    *CLOCK.lock().unwrap_or_else(|e| e.into_inner()) = Some(clock);
}

// ---- the native timer ----

#[cfg(not(target_arch = "wasm32"))]
static WAKE: std::sync::Condvar = std::sync::Condvar::new();

#[cfg(not(target_arch = "wasm32"))]
fn start_timer() {
    use std::sync::Once;
    static STARTED: Once = Once::new();
    STARTED.call_once(|| {
        // if the thread cannot start, the pool is still emptied by release_scratch()
        let _ = std::thread::Builder::new().name("scratch-idle".into()).spawn(timer_loop);
    });
}

#[cfg(not(target_arch = "wasm32"))]
fn timer_loop() {
    let mut st = lock();
    loop {
        let mut wait = None;
        if let (0, Some(end), Some(n)) = (st.active, st.last_end, now()) {
            let due = end + st.timeout;
            if n >= due {
                clear_pool();
                st.last_end = None;
            } else {
                wait = Some(due - n);
            }
        }
        st = match wait {
            Some(d) => WAKE.wait_timeout(st, d).unwrap_or_else(|e| e.into_inner()).0,
            None => WAKE.wait(st).unwrap_or_else(|e| e.into_inner()),
        };
    }
}

// ---- public interface ----

/// Change how long the kernel may sit idle before the pool is emptied (default 5 seconds). For tests.
pub fn set_idle_timeout(timeout: Duration) {
    lock().timeout = timeout;
    #[cfg(not(target_arch = "wasm32"))]
    WAKE.notify_all();
}

/// Give back every buffer the pool is holding, now. Does nothing while an operation is running
/// (the idle release or the next call will do it). Results do not depend on the pool.
pub fn release_scratch() {
    let mut st = lock();
    if st.active == 0 {
        clear_pool();
        st.last_end = None;
    }
}

/// Start the idle time now, if no operation is running. For code that drops kernel solids outside
/// any operation (their buffers go back to the pool with no timer armed).
pub fn touch() {
    let mut st = lock();
    if st.active == 0 {
        st.last_end = now();
        #[cfg(not(target_arch = "wasm32"))]
        {
            drop(st);
            start_timer();
            WAKE.notify_all();
        }
    }
}

/// Bytes the pool holds right now. For tests and diagnostics.
pub fn scratch_held_bytes() -> usize {
    manifold_rust::scratch::held_bytes()
}

/// Marks one kernel operation. While it is alive the pool is never emptied; when it is dropped
/// (success, error or panic) the idle time starts counting.
pub struct ScratchGuard(());

impl ScratchGuard {
    /// Start an operation. On wasm32 this is where an idle pool is released.
    pub fn begin() -> ScratchGuard {
        let mut st = lock();
        #[cfg(target_arch = "wasm32")]
        if st.active == 0 {
            if let (Some(end), Some(n)) = (st.last_end, now()) {
                if n.saturating_sub(end) > st.timeout {
                    clear_pool();
                    st.last_end = None;
                }
            }
        }
        st.active += 1;
        ScratchGuard(())
    }
}

impl Drop for ScratchGuard {
    fn drop(&mut self) {
        let mut st = lock();
        st.active = st.active.saturating_sub(1);
        if st.active == 0 {
            st.last_end = now();
            #[cfg(not(target_arch = "wasm32"))]
            {
                drop(st);
                start_timer();
                WAKE.notify_all();
            }
        }
    }
}
