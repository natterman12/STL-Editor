//! The real window after a graphics driver reset (spec 2.6). A child of `app.rs`.
//!
//! eframe 0.37 can't give the window a new device: egui-wgpu's `Painter` makes its
//! `RenderState` once (`add_surface`, only while it has none) and keeps it private, and the
//! render state in `eframe::Frame` is a copy. Upstream egui-wgpu also panics on the next paint
//! after a loss (`Renderer::update_buffers`: "Failed to create staging buffer"), inside winit's
//! window procedure, which Windows can't unwind through: the process ended (0xc000041d). Our
//! copy (`rust/third_party/egui-wgpu`, see its `VENDORED.md`) skips that frame instead and says
//! so (`Renderer::upload_refused`, read by `Viewport::device_lost`). So this builds the fallback:
//!
//! 1. [`StlApp::run_window`] runs the window and keeps the app when the window ends because its
//!    device was lost (the window closed after [`StlApp::device_frame`] noticed the loss and
//!    wrote it to `log.txt`; a panic while drawing is still caught as a last resort);
//! 2. `main` then writes the autosave copy ([`StlApp::save_after_device_loss`]), says
//!    [`StlApp::DRIVER_RESTARTED`] in a native message box, and starts STL Editor again
//!    ([`StlApp::restart_after_device_loss`]);
//! 3. the new window opens the copy without asking ([`StlApp::after_device_reset`]).

use super::*;

/// `BodyInfo`'s `seq` for the fence that says every earlier command (the autosave) is done.
#[cfg(not(target_arch = "wasm32"))]
const FENCE_SEQ: u64 = u64::MAX - 0x5eed;

/// After a panic while drawing: whether the device turns out to be lost, polling it for up to a
/// second (a driver may report the loss a little after the draw that failed, and WARP never
/// reports it: there the device just refuses uploads, as it did for the draw that panicked).
#[cfg(not(target_arch = "wasm32"))]
fn lost_soon(app: &StlApp) -> bool {
    let end = web_time::Instant::now() + std::time::Duration::from_secs(1);
    loop {
        if app.viewport.device_lost(true) || app.viewport.device_refuses_uploads() {
            return true;
        }
        if web_time::Instant::now() >= end {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// One line in `log.txt` when the window notices its device is gone: wgpu's reason and message
/// (when the driver reported the loss), whether the last paint was refused its upload, and the
/// adapter. The window has no console, so this is what a crash report can be read from.
#[cfg(not(target_arch = "wasm32"))]
fn log_device_loss(viewport: &crate::viewport::Viewport, frame: &eframe::Frame) {
    use std::io::Write;
    let reason = viewport.stats().lost_reason().unwrap_or_else(|| "no reason reported".into());
    let (refused, adapter) = match frame.wgpu_render_state() {
        Some(rs) => {
            let i = rs.adapter.get_info();
            (rs.renderer.read().upload_refused(), format!("{} ({:?}, driver {} {})", i.name, i.backend, i.driver, i.driver_info))
        }
        None => (false, "unknown adapter".into()),
    };
    let when = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let line = format!(
        "[{when}] The graphics device was lost: {reason}; upload refused: {}; adapter: {adapter}",
        if refused { "yes" } else { "no" }
    );
    eprintln!("{line}");
    let path = stl_platform::settings::log_file();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "{line}");
    }
}

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
            #[cfg(debug_assertions)]
            debug_auto_fins(self);
            if self.viewport.notice_device_loss() {
                log_device_loss(&self.viewport, frame);
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
            // some drivers (WARP on CI) report the loss a moment after the draw that failed:
            // poll the device for up to a second before treating the panic as a real one
            (Err(_), Some(app)) if lost_soon(&app) => {
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

/// Debug builds only: `STLEDITOR2_TEST_FINS=1` runs Nathan's two fins steps in the real window
/// (spec 2.1, milestone 7): once the first part is shown, Support Fins on it (opened, previewed,
/// OK), then Draw Walls with one wall across its lowest points (OK), then the window closes.
/// Each step is written to stderr, so a device loss can be placed.
#[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
fn debug_auto_fins(app: &mut StlApp) {
    use std::sync::Mutex;
    static STATE: Mutex<Option<(u32, web_time::Instant)>> = Mutex::new(None);
    if std::env::var("STLEDITOR2_TEST_FINS").as_deref() != Ok("1") || RESTARTED.load(std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    if let Some(ctx) = app.viewport.egui_ctx() {
        ctx.request_repaint_after(std::time::Duration::from_millis(30));
    }
    let mut st = STATE.lock().unwrap_or_else(|e| e.into_inner());
    let (stage, since) = *st.get_or_insert((0, web_time::Instant::now()));
    let waited = since.elapsed().as_secs_f32();
    let next = |st: &mut Option<(u32, web_time::Instant)>, what: &str| {
        eprintln!("STLEDITOR2_TEST_FINS: {what} ({waited:.1} s after the last step)");
        *st = Some((stage + 1, web_time::Instant::now()));
    };
    let tool = app.tool.as_ref().map(|t| t.def.key);
    match stage {
        0 if !app.bodies.is_empty() && waited > 3.0 => {
            app.shell.selection = vec![app.bodies[0].id.clone()];
            app.open_tool("supportfins", None);
            next(&mut st, "opening Support Fins");
        }
        // reopened until the engine's settings are in (the first open only asks for them)
        1 if tool != Some("supportfins") && waited > 2.0 => {
            app.open_tool("supportfins", None);
            *st = Some((1, web_time::Instant::now()));
        }
        1 if tool == Some("supportfins") && waited > 15.0 => {
            app.tool_ok();
            next(&mut st, "Support Fins OK");
        }
        2 if tool.is_none() && waited > 8.0 => {
            app.shell.selection = vec![app.bodies[0].id.clone()];
            app.open_tool("supportwall", None);
            next(&mut st, "opening Draw Walls");
        }
        3 if tool.is_some() && waited > 5.0 => {
            // one wall between the two lowest vertices that are at least 5 mm apart
            let m = &app.bodies[0].mesh;
            let mut pts: Vec<[f64; 3]> = m.positions.chunks_exact(3).map(|p| [p[0] as f64, p[1] as f64, p[2] as f64]).collect();
            pts.sort_by(|a, b| a[2].total_cmp(&b[2]));
            let a = pts[pts.len() / 10];
            let b = pts.iter().skip(pts.len() / 10).find(|p| ((p[0] - a[0]).powi(2) + (p[1] - a[1]).powi(2)).sqrt() > 5.0).copied().unwrap_or(a);
            app.tool_set_value("walls", serde_json::json!([{ "shape": "line", "points": [a, b] }]));
            next(&mut st, &format!("drawing a wall from {a:?} to {b:?} with {tool:?}"));
        }
        4 if waited > 10.0 => {
            app.tool_ok();
            next(&mut st, "Draw Walls OK");
        }
        5 if waited > 10.0 => {
            next(&mut st, "done; closing the window");
            if let Some(ctx) = app.viewport.egui_ctx() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        _ => {}
    }
}
