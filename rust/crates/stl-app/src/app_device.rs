//! The real window after a graphics driver reset (spec 2.6). A child of `app.rs`.
//!
//! eframe 0.37 can't give the window a new device: egui-wgpu's `Painter` makes its
//! `RenderState` once (`add_surface`, only while it has none) and keeps it private, and the
//! render state in `eframe::Frame` is a copy. Worse, the next paint after a loss panics inside
//! egui-wgpu (`Renderer::update_buffers`: "Failed to create staging buffer"), so the window
//! can't even show a message. So this builds the fallback:
//!
//! 1. [`StlApp::run_window`] runs the window and keeps the app when the window ends because its
//!    device was lost (the panic is caught, or the window closed after [`StlApp::device_frame`]
//!    noticed the loss);
//! 2. `main` then writes the autosave copy ([`StlApp::save_after_device_loss`]), says
//!    [`StlApp::DRIVER_RESTARTED`] in a native message box, and starts STL Editor again
//!    ([`StlApp::restart_after_device_loss`]);
//! 3. the new window opens the copy without asking ([`StlApp::after_device_reset`]).

use super::*;

/// `BodyInfo`'s `seq` for the fence that says every earlier command (the autosave) is done.
#[cfg(not(target_arch = "wasm32"))]
const FENCE_SEQ: u64 = u64::MAX - 0x5eed;

impl StlApp {
    /// What the native message box says after a driver reset, once the work is saved.
    pub const DRIVER_RESTARTED: &'static str = "The graphics driver restarted. STL Editor needs to restart to keep drawing. Your work is saved.";
    /// The same when the copy couldn't be written (pending Nathan's OK: the spec has only the
    /// first message, and "Your work is saved." would not be true).
    pub const DRIVER_RESTARTED_NOT_SAVED: &'static str = "The graphics driver restarted. STL Editor needs to restart to keep drawing.";
    /// The environment variable that tells the restarted window what to open:
    /// `recover:<session>` (the old window's autosave copy) or `open:<path>` (the saved project).
    pub const AFTER_RESET_ENV: &'static str = "STLEDITOR2_AFTER_RESET";

    /// Each frame, before the layout: notice a lost device. The first frame that sees it asks
    /// the window to close ([`StlApp::run_window`] keeps the app). True once the device is lost:
    /// nothing can be drawn any more, and a frame that draws nothing is a frame egui-wgpu
    /// doesn't panic on.
    pub(super) fn device_frame(&mut self, frame: &mut eframe::Frame) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        {
            #[cfg(debug_assertions)]
            debug_force_loss(self, frame);
            let _ = frame;
            if self.viewport.notice_device_loss() {
                eprintln!("STL Editor: the graphics device was lost; closing the window to restart");
                if let Some(ctx) = self.viewport.egui_ctx() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            self.viewport.device_lost(false)
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = frame;
            false
        }
    }

    /// Run the window with `run` (`eframe::run_native`, or a test's stand-in) and the app `make`
    /// builds. `Some(app)` when the window ended because its device was lost: the app is kept,
    /// with its project and engine. Any other panic goes on as before.
    ///
    /// The window holds a small wrapper that passes on each `eframe::App` method `StlApp`
    /// implements (`ui` and `on_exit`); a new one must be added there too.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn run_window(
        run: impl FnOnce(eframe::AppCreator<'static>) -> eframe::Result,
        make: impl FnOnce(&eframe::CreationContext<'_>) -> StlApp + 'static,
    ) -> eframe::Result<Option<StlApp>> {
        use std::cell::RefCell;
        use std::rc::Rc;

        struct Kept(Rc<RefCell<StlApp>>);
        impl eframe::App for Kept {
            fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
                eframe::App::ui(&mut *self.0.borrow_mut(), ui, frame);
            }
            fn on_exit(&mut self) {
                eframe::App::on_exit(&mut *self.0.borrow_mut());
            }
        }

        let slot: Rc<RefCell<Option<Rc<RefCell<StlApp>>>>> = Rc::new(RefCell::new(None));
        let keep = slot.clone();
        let creator: eframe::AppCreator<'static> = Box::new(move |cc| {
            let app = Rc::new(RefCell::new(make(cc)));
            *keep.borrow_mut() = Some(app.clone());
            Ok(Box::new(Kept(app)))
        });
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(creator)));
        // the window and its wrapper are gone by now (returned or unwound)
        let app = slot.borrow_mut().take().and_then(|a| Rc::try_unwrap(a).ok()).map(RefCell::into_inner);
        match (result, app) {
            (Ok(r), Some(app)) if app.viewport.device_lost(true) => {
                // the window closed after the loss; an error from tearing it down doesn't matter now
                let _ = r;
                Ok(Some(app))
            }
            (Ok(r), _) => r.map(|()| None),
            (Err(_), Some(app)) if app.viewport.device_lost(true) => {
                eprintln!("STL Editor: the graphics device was lost while drawing; restarting");
                Ok(Some(app))
            }
            (Err(panic), _) => std::panic::resume_unwind(panic),
        }
    }

    /// After a lost device: write the autosave copy now and wait (at most `wait`) until the
    /// engine has done it. True when the work is safe: nothing was unsaved, or the copy is up
    /// to date. False without autosave, when writing failed, or when the engine didn't answer.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn save_after_device_loss(&mut self, wait: std::time::Duration) -> bool {
        if !self.shell.dirty {
            return true;
        }
        if self.recover.is_none() || self.engine_lost {
            return false;
        }
        self.link.send(self.autosave_command());
        // the engine runs commands in order and always answers this one: once it is back, the
        // autosave above is done (it answers nothing when the last copy was already current)
        self.link.send(Command::BodyInfo { seq: FENCE_SEQ, id: String::new() });
        let end = web_time::Instant::now() + wait;
        let mut ok = true;
        while web_time::Instant::now() < end {
            for u in self.link.poll() {
                match u {
                    Update::Autosaved { ok: false, .. } | Update::EngineLost => ok = false,
                    Update::BodyInfo { seq: FENCE_SEQ, .. } => return ok,
                    _ => {}
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        false
    }

    /// The command that starts STL Editor again once this process has ended, telling it what
    /// to open ([`StlApp::AFTER_RESET_ENV`]). Not started here. This ends the app: its engine
    /// ends its autosave session, which unlocks this window's copy (kept while there is unsaved
    /// work, removed otherwise).
    ///
    /// The marker is `recover:<session>` only when this window's own copy is there and current
    /// (`saved`: what [`StlApp::save_after_device_loss`] answered); it is the one folder that was
    /// locked before the session ended and is recoverable after. Otherwise (the copy couldn't be
    /// written, or there was nothing to write, as for a project whose steps were all deleted)
    /// there is no marker and Recover Unsaved Work asks as usual: an older session's copy is
    /// never opened on its own. A clean project that has a file is opened from it (`open:<path>`).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn restart_after_device_loss(self, saved: bool) -> std::io::Result<std::process::Command> {
        let exe = std::env::current_exe()?;
        let mut cmd = wiring::restart_helper(&exe, std::process::id());
        let dirty = self.shell.dirty;
        let path = self.files.project_path().map(str::to_string);
        let want_copy = dirty && saved && self.recover.is_some();
        let root = stl_platform::paths::data_dir().join("autosave");
        let ids = |root: &std::path::Path| -> std::collections::HashSet<String> {
            stl_platform::autosave::recoverable(root, None).into_iter().map(|s| s.id).collect()
        };
        let before = if want_copy { ids(&root) } else { Default::default() };
        drop(self);
        if want_copy {
            let mine: Vec<String> = ids(&root).into_iter().filter(|id| !before.contains(id)).collect();
            if let [id] = mine.as_slice() {
                cmd.env(Self::AFTER_RESET_ENV, format!("recover:{id}"));
            }
        } else if let Some(path) = path.filter(|_| !dirty) {
            cmd.env(Self::AFTER_RESET_ENV, format!("open:{path}"));
        }
        Ok(cmd)
    }

    /// The window restarted after a driver reset ([`StlApp::AFTER_RESET_ENV`] is `marker`):
    /// open what the old window had, without asking. Call before [`StlApp::enable_autosave`]:
    /// its start-up listing must not run while the copy is read (two listings at once can each
    /// find the other's lock and skip the folder), and once the copy is used up it isn't offered
    /// again. Other copies an earlier session left are still offered as usual.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn after_device_reset(&mut self, marker: &str) {
        #[cfg(debug_assertions)]
        RESTARTED.store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(path) = marker.strip_prefix("open:") {
            self.open_launch_file(std::path::Path::new(path));
            return;
        }
        let Some(id) = marker.strip_prefix("recover:").filter(|id| !id.is_empty()) else { return };
        // only the old window's own copy; anything else is left for Recover Unsaved Work
        let root = stl_platform::paths::data_dir().join("autosave");
        let Some(s) = stl_platform::autosave::recoverable(&root, None).into_iter().find(|s| s.id == id) else { return };
        match recover::project_bytes(&root, &s.id) {
            Ok(bytes) => {
                stl_platform::autosave::discard(&root, &s.id);
                self.open_recovered(Recovered { name: format!("{}.stlx", s.title), bytes, path: s.path });
            }
            Err(text) => self.toasts.push(Level::Warning, text),
        }
    }
}

/// The window was started again after a reset ([`debug_force_loss`] leaves it alone).
#[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
static RESTARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Debug builds only: `STLEDITOR2_TEST_DEVICE_LOSS=<frames>` destroys the window's device after
/// that many frames, to try the whole path in the real window (not again in the window that
/// restart opens).
#[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
fn debug_force_loss(app: &mut StlApp, frame: &eframe::Frame) {
    use std::sync::atomic::{AtomicU32, Ordering};
    static FRAMES: AtomicU32 = AtomicU32::new(0);
    if RESTARTED.load(Ordering::Relaxed) {
        return;
    }
    let Some(after) = std::env::var("STLEDITOR2_TEST_DEVICE_LOSS").ok().and_then(|v| v.parse::<u32>().ok()) else { return };
    let n = FRAMES.fetch_add(1, Ordering::Relaxed);
    if let Some(ctx) = app.viewport.egui_ctx() {
        ctx.request_repaint_after(std::time::Duration::from_millis(30));
    }
    if n == after {
        if let Some(rs) = frame.wgpu_render_state() {
            eprintln!("STLEDITOR2_TEST_DEVICE_LOSS: destroying the window's device");
            crate::viewport::renderer::destroy_device(rs);
        }
    }
}
