//! Milestone 6, Task 9: the graphics device is lost in the real window (spec 2.6).
//!
//! eframe 0.37 can't give the window a new device, so the window restarts: the app is kept,
//! the work is autosaved, and the new window opens it without asking (`app_device.rs`). This
//! drives that path as the window does: `StlApp::run_window` (what `main` runs) around the
//! app's `eframe::App::ui` with an `eframe::Frame` that holds the device, and every frame
//! painted by egui-wgpu's own `Renderer` (`Headless::paint_egui`), which is where upstream
//! egui-wgpu panicked after a loss (milestone 7: our copy in `rust/third_party/egui-wgpu` skips
//! the frame instead). Only `eframe::run_native` itself is replaced, by a loop that does what
//! it does.
//!
//! Its own test binary. The GPU tests here share one settings folder and run one at a time
//! ([`setup`]).
#![cfg(not(target_arch = "wasm32"))]

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;
use stl_app::app::StlApp;
use stl_app::engine_link::native::NativeLink;
use stl_app::engine_link::{Command, EngineLink};
use stl_app::platform::Platform;
use stl_app::platform::native::NativePlatform;
use stl_app::ui::ThemePref;
use stl_app::viewport::renderer::{Headless, Stats, destroy_device};

#[path = "ui/harness.rs"]
mod harness;
// `WGPU_ADAPTER_NAME="Microsoft Basic Render Driver"` runs these on WARP, as CI does
#[path = "support/gpu.rs"]
mod gpu;
use harness::SharedLink;

const W: u32 = 1280;
const H: u32 = 800;

fn input() -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(W as f32, H as f32),
        )),
        ..Default::default()
    }
}

fn platform(cfg: &Path) -> NativePlatform {
    let p = NativePlatform::with_config_dir(cfg);
    p.set_setting_json("update_check", json!(false)); // no network from a test
    p
}

/// This binary's settings folder (never Nathan's real one), set once, and a lock so only one
/// test drives a window at a time (they share the folder, and each checks what is in it).
fn setup() -> (PathBuf, MutexGuard<'static, ()>) {
    static CFG: OnceLock<PathBuf> = OnceLock::new();
    static ONE: Mutex<()> = Mutex::new(());
    let guard = ONE.lock().unwrap_or_else(|e| e.into_inner());
    let cfg = CFG
        .get_or_init(|| {
            let cfg = std::env::temp_dir().join(format!("stle2-device-lost-window-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&cfg);
            std::fs::create_dir_all(&cfg).unwrap();
            // SAFETY: set once, under the lock, before any window or engine of these tests starts
            // (the harness's test_env set it before main; this only moves it to our own folder)
            unsafe { std::env::set_var("STLEDITOR2_CONFIG_DIR", &cfg) };
            cfg
        })
        .clone();
    assert!(autosave_root().starts_with(&cfg));
    (cfg, guard)
}

fn autosave_root() -> PathBuf {
    stl_platform::paths::data_dir().join("autosave")
}

/// Frames until `done` has held for a few frames in a row.
fn until(what: &str, mut done: impl FnMut() -> bool, mut frame: impl FnMut()) {
    let end = Instant::now() + Duration::from_secs(120);
    let mut quiet = 0;
    while quiet < 5 {
        frame();
        quiet = if done() { quiet + 1 } else { 0 };
        assert!(Instant::now() < end, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Ends {
    /// The driver resets after the project opened and a box was added (unsaved work).
    LostUnsaved,
    /// The driver resets with the project just opened from its file (nothing unsaved).
    LostClean,
    /// The driver resets after every step of the project was deleted: unsaved, but there is
    /// nothing for an autosave copy to hold.
    LostEmptied,
    /// The window is closed normally (X, File > Quit).
    Quit,
}

/// One window, run through [`StlApp::run_window`] as `main` runs it, on `gpu`. The project is
/// `basic.stlx`, opened from its path.
fn window(gpu: &Headless, cfg: &Path, ends: Ends) -> Option<StlApp> {
    let link = Arc::new(NativeLink::new());
    let stats: Arc<Mutex<Option<Arc<Stats>>>> = Arc::default();
    let make = {
        let (link, stats, cfg) = (link.clone(), stats.clone(), cfg.to_path_buf());
        move |cc: &eframe::CreationContext<'_>| {
            let mut viewport = stl_app::viewport::Viewport::new();
            viewport.init_gpu(cc.wgpu_render_state.as_ref().expect("the window's device"));
            *stats.lock().unwrap() = Some(viewport.stats_handle());
            let mut app = StlApp::new(
                &cc.egui_ctx,
                Box::new(platform(&cfg)),
                Box::new(SharedLink { inner: link.clone() }),
            );
            app.viewport = viewport;
            app.enable_autosave();
            app.open_launch_file(&harness::project("basic.stlx"));
            app
        }
    };
    let mut reached_after_paint = false;
    let mut asked_to_close = false;
    // `eframe::run_native`, minus the OS window: make the app, then ui + paint every frame
    let run = |creator: eframe::AppCreator<'static>| -> eframe::Result {
        let ctx = egui::Context::default();
        let mut cc = eframe::CreationContext::_new_kittest(ctx.clone());
        cc.wgpu_render_state = Some(gpu.render_state.clone());
        let mut app = creator(&cc).map_err(eframe::Error::AppCreation)?;
        let mut frame = eframe::Frame::_new_kittest();
        frame.wgpu_render_state = Some(gpu.render_state.clone());
        let mut step = |app: &mut Box<dyn eframe::App>| ctx.run_ui(input(), |ui| app.ui(ui, &mut frame));
        let st = stats.lock().unwrap().clone().expect("stats");
        let drawn = |st: &Stats, after: u64| {
            !link.is_busy() && st.uploads() > after && !st.upload_pending.load(Ordering::Relaxed)
        };

        until(
            "the project to open",
            || drawn(&st, 0),
            || {
                let out = step(&mut app);
                gpu.paint_egui(&ctx, out, W, H);
            },
        );
        if ends == Ends::LostUnsaved {
            let uploads = st.uploads();
            link.send(Command::AddFeature {
                kind: "primitive".into(),
                params: json!({"shape": "box", "size": [7.0, 8.0, 9.0]}),
                index: None,
                name: None,
            });
            until(
                "the change",
                || drawn(&st, uploads),
                || {
                    let out = step(&mut app);
                    gpu.paint_egui(&ctx, out, W, H);
                },
            );
        }
        if ends == Ends::LostEmptied {
            for id in ["f5", "f4", "f3"] {
                link.send(Command::DeleteFeature { id: id.into() });
            }
            until(
                "the deletes",
                || !link.is_busy(),
                || {
                    let out = step(&mut app);
                    gpu.paint_egui(&ctx, out, W, H);
                },
            );
        }
        assert_eq!(
            st.gpu_states(),
            1,
            "the scene went up through egui-wgpu's paint callback"
        );
        assert!(!st.device_lost());
        if ends == Ends::Quit {
            eframe::App::on_exit(app.as_mut());
            return Ok(()); // the window closed
        }

        // the driver resets while a frame is painted: upstream egui-wgpu panicked there; ours
        // skips the frame, and the app's next frame asks the window to close
        let out = step(&mut app);
        destroy_device(&gpu.render_state);
        gpu.paint_egui(&ctx, out, W, H);
        reached_after_paint = true;
        for _ in 0..10 {
            let out = step(&mut app);
            let close = out
                .viewport_output
                .get(&egui::ViewportId::ROOT)
                .is_some_and(|v| v.commands.contains(&egui::ViewportCommand::Close));
            gpu.paint_egui(&ctx, out, W, H);
            if close {
                asked_to_close = true;
                return Ok(()); // the window closes
            }
        }
        Ok(())
    };
    let kept = StlApp::run_window(run, make).expect("run_window");
    if ends != Ends::Quit {
        assert!(
            reached_after_paint,
            "painting on a lost device panicked (run_window caught it)"
        );
        assert!(
            asked_to_close,
            "the frames after the loss didn't ask the window to close"
        );
    }
    drop(link); // the test's handle; the app holds the other
    kept
}

/// The window `main` starts after the restart, with `marker` in [`StlApp::AFTER_RESET_ENV`].
fn new_window(cfg: &Path, marker: &str) -> (Harness<'static, StlApp>, Arc<NativeLink>) {
    let link = Arc::new(NativeLink::new());
    let mut app = StlApp::new(
        &egui::Context::default(),
        Box::new(platform(cfg)),
        Box::new(SharedLink { inner: link.clone() }),
    );
    app.after_device_reset(marker); // before enable_autosave, as `main` does
    app.enable_autosave();
    let mut frame = eframe::Frame::_new_kittest();
    let mut fonts = false;
    let mut h = Harness::builder()
        .with_size(egui::vec2(W as f32, H as f32))
        .with_accessibility_check(false)
        .build_ui_state(
            move |ui, app: &mut StlApp| {
                if !fonts {
                    fonts = true;
                    stl_app::fonts::install(ui.ctx());
                    app.set_theme(ui.ctx(), ThemePref::Dark);
                }
                eframe::App::ui(app, ui, &mut frame)
            },
            app,
        );
    harness::settle(&mut h, &link);
    h.run_steps(5);
    harness::settle(&mut h, &link);
    (h, link)
}

fn marker_of(cmd: &std::process::Command) -> Option<String> {
    cmd.get_envs()
        .find(|(k, _)| *k == StlApp::AFTER_RESET_ENV)
        .and_then(|(_, v)| v)
        .map(|v| v.to_string_lossy().into_owned())
}

#[test]
fn window_device_lost_restarts_and_recovers() {
    // never Nathan's real folders: settings and autosave go to a temp folder
    let (cfg, _one) = setup();
    let _ = std::fs::remove_dir_all(autosave_root());

    if gpu::headless().is_none() {
        // loud on purpose: the verify run looks for this line
        println!("SKIPPED: no GPU adapter (window_device_lost_restarts_and_recovers did not run)");
        eprintln!("SKIPPED: no GPU adapter (window_device_lost_restarts_and_recovers did not run)");
        return;
    }
    let path = harness::project("basic.stlx").display().to_string();

    // ---- 1. a normal quit: nothing kept, nothing left to recover
    let gpu = gpu::headless().unwrap();
    assert!(window(&gpu, &cfg, Ends::Quit).is_none(), "a normal quit kept the app");
    assert!(stl_platform::autosave::recoverable(&autosave_root(), None).is_empty());

    // ---- 2. lost with unsaved work: autosave, restart, the copy opens without asking
    let gpu = gpu::headless().unwrap();
    let mut app = window(&gpu, &cfg, Ends::LostUnsaved).expect("the app is kept after a device loss");
    assert!(app.viewport.device_lost(false));
    assert!(app.shell.dirty, "the added box is unsaved work");
    let steps = app.timeline_view.items.len();
    let bodies: Vec<String> = app.bodies.iter().map(|b| b.id.clone()).collect();
    assert!(steps >= 2 && bodies.len() >= 2, "{steps} steps, bodies {bodies:?}");
    assert!(!app.engine_lost);
    // main: autosave now, say so, restart with the marker
    let saved = app.save_after_device_loss(Duration::from_secs(60));
    assert!(saved, "the autosave copy wasn't written");
    assert_eq!(
        StlApp::DRIVER_RESTARTED,
        "The graphics driver restarted. STL Editor needs to restart to keep drawing. Your work is saved."
    );
    // the old window's engine ends: its copy stays for the new window, named in the marker
    let marker = marker_of(&app.restart_after_device_loss(saved).unwrap()).expect("a marker");
    let copies = stl_platform::autosave::recoverable(&autosave_root(), None);
    assert_eq!(copies.len(), 1, "one autosave copy: {copies:?}");
    assert_eq!(marker, format!("recover:{}", copies[0].id));
    assert_eq!(copies[0].steps, steps);

    let (h, link) = new_window(&cfg, &marker);
    let got: Vec<String> = h.state().bodies.iter().map(|b| b.id.clone()).collect();
    assert_eq!(got, bodies, "the recovered project has the old window's bodies");
    assert_eq!(h.state().timeline_view.items.len(), steps);
    assert!(h.state().shell.dirty, "recovered work is unsaved until saved");
    assert!(
        h.query_by_label("Recover unsaved work?").is_none(),
        "the copy was offered instead of opened"
    );
    // the copy was used up; the new window's own folder is locked while it runs
    assert!(stl_platform::autosave::recoverable(&autosave_root(), None).is_empty());
    drop(h);
    drop(link);
    // the recovered work is still unsaved, so that window kept a copy as it closed: put it aside
    // as an older session's leftover for part 4
    let leftover = wait_for_copy();
    let stash = cfg.join("stash");
    std::fs::rename(autosave_root(), &stash).unwrap();

    // ---- 3. lost with nothing unsaved: the new window opens the project's file
    let gpu = gpu::headless().unwrap();
    let mut app = window(&gpu, &cfg, Ends::LostClean).expect("the app is kept after a device loss");
    assert!(!app.shell.dirty);
    let bodies: Vec<String> = app.bodies.iter().map(|b| b.id.clone()).collect();
    let saved = app.save_after_device_loss(Duration::from_secs(60));
    assert!(saved, "nothing unsaved counts as saved");
    let marker = marker_of(&app.restart_after_device_loss(saved).unwrap()).expect("a marker");
    assert_eq!(marker, format!("open:{path}"));
    assert!(
        stl_platform::autosave::recoverable(&autosave_root(), None).is_empty(),
        "a clean project leaves no copy"
    );
    let (h, link) = new_window(&cfg, &marker);
    let got: Vec<String> = h.state().bodies.iter().map(|b| b.id.clone()).collect();
    assert_eq!(got, bodies, "the project's file opened again");
    assert_eq!(h.state().files.project_path(), Some(path.as_str()));
    assert!(!h.state().shell.dirty);
    assert!(h.query_by_label("Recover unsaved work?").is_none());
    drop(h);
    drop(link);

    // ---- 4. lost with every step deleted: no copy of this window, so an older session's copy
    // (left here by a window that crashed) must be offered, not opened on its own
    let _ = std::fs::remove_dir_all(autosave_root());
    std::fs::rename(&stash, autosave_root()).unwrap();
    assert_eq!(wait_for_copy(), leftover);

    let gpu = gpu::headless().unwrap();
    let app = window(&gpu, &cfg, Ends::LostEmptied).expect("the app is kept after a device loss");
    assert!(app.shell.dirty, "deleting every step is unsaved work");
    assert!(app.timeline_view.items.is_empty());
    let mut app = app;
    let saved = app.save_after_device_loss(Duration::from_secs(60));
    assert_eq!(
        marker_of(&app.restart_after_device_loss(saved).unwrap()),
        None,
        "no copy of this window, so no marker"
    );
    let copies: Vec<String> = stl_platform::autosave::recoverable(&autosave_root(), None)
        .into_iter()
        .map(|s| s.id)
        .collect();
    assert_eq!(copies, vec![leftover.clone()], "only the older session's copy is there");
    // `main` passes no marker; a stale or foreign one opens nothing either
    for marker in ["", "recover", &format!("recover:{leftover}-x")] {
        let (mut h, link) = new_window(&cfg, marker);
        assert!(h.state().bodies.is_empty(), "{marker:?} opened something on its own");
        let end = Instant::now() + Duration::from_secs(30);
        while h.query_by_label("Recover unsaved work?").is_none() {
            assert!(Instant::now() < end, "the older copy wasn't offered ({marker:?})");
            h.run_steps(1);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(h.state().bodies.is_empty());
        drop(h);
        drop(link);
        assert_eq!(
            stl_platform::autosave::recoverable(&autosave_root(), None).len(),
            1,
            "the offered copy is still there"
        );
    }
    let _ = std::fs::remove_dir_all(autosave_root());
}

/// The one recoverable copy, once its window's lock is gone.
fn wait_for_copy() -> String {
    let end = Instant::now() + Duration::from_secs(30);
    loop {
        let copies = stl_platform::autosave::recoverable(&autosave_root(), None);
        if let [c] = copies.as_slice() {
            return c.id.clone();
        }
        assert!(Instant::now() < end, "no leftover copy: {copies:?}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Milestone 7 (spec 2.1): a refused staging write, on the real paint path, is a reported loss
/// and not a panic. No `run_window` and no `catch_unwind` here: the test owns the app and paints
/// with egui-wgpu's `Renderer` (`update_buffers`, `render`), so a panic fails the test. Then the
/// app's next frame notices the loss, logs it, asks the window to close, and `main`'s restart
/// path writes the autosave copy and the restart marker.
#[test]
fn refused_staging_write_does_not_panic() {
    let (cfg, _one) = setup();
    let _ = std::fs::remove_dir_all(autosave_root());
    let Some(gpu) = gpu::headless() else {
        println!("SKIPPED: no GPU adapter (refused_staging_write_does_not_panic did not run)");
        eprintln!("SKIPPED: no GPU adapter (refused_staging_write_does_not_panic did not run)");
        return;
    };
    let link = Arc::new(NativeLink::new());
    let ctx = egui::Context::default();
    let mut cc = eframe::CreationContext::_new_kittest(ctx.clone());
    cc.wgpu_render_state = Some(gpu.render_state.clone());
    let mut viewport = stl_app::viewport::Viewport::new();
    viewport.init_gpu(&gpu.render_state);
    let st = viewport.stats_handle();
    let mut app = StlApp::new(
        &cc.egui_ctx,
        Box::new(platform(&cfg)),
        Box::new(SharedLink { inner: link.clone() }),
    );
    app.viewport = viewport;
    app.enable_autosave();
    app.open_launch_file(&harness::project("basic.stlx"));
    let mut frame = eframe::Frame::_new_kittest();
    frame.wgpu_render_state = Some(gpu.render_state.clone());
    let mut step = |app: &mut StlApp| ctx.run_ui(input(), |ui| eframe::App::ui(app, ui, &mut frame));
    let drawn =
        |st: &Stats, after: u64| !link.is_busy() && st.uploads() > after && !st.upload_pending.load(Ordering::Relaxed);

    until(
        "the project to open",
        || drawn(&st, 0),
        || {
            let out = step(&mut app);
            gpu.paint_egui(&ctx, out, W, H);
        },
    );
    let uploads = st.uploads();
    link.send(Command::AddFeature {
        kind: "primitive".into(),
        params: json!({"shape": "box", "size": [7.0, 8.0, 9.0]}),
        index: None,
        name: None,
    });
    until(
        "the change",
        || drawn(&st, uploads),
        || {
            let out = step(&mut app);
            gpu.paint_egui(&ctx, out, W, H);
        },
    );
    assert!(!app.viewport.device_lost(true));
    assert!(app.shell.dirty, "the added box is unsaved work");

    // the device goes away; this paint's staging write is refused inside `update_buffers`
    let out = step(&mut app);
    assert!(!out.shapes.is_empty(), "the frame has something to upload");
    destroy_device(&gpu.render_state);
    gpu.paint_egui(&ctx, out, W, H);
    assert!(
        gpu.render_state.renderer.read().upload_refused(),
        "the staging write was not refused (the device wasn't lost?)"
    );
    assert!(
        app.viewport.device_lost(false),
        "the refused upload counts as a lost device"
    );

    // the next frame notices it once: logged, and the window is asked to close
    let out = step(&mut app);
    let close = out
        .viewport_output
        .get(&egui::ViewportId::ROOT)
        .is_some_and(|v| v.commands.contains(&egui::ViewportCommand::Close));
    gpu.paint_egui(&ctx, out, W, H);
    assert!(close, "the first frame after the loss didn't ask the window to close");
    let log = std::fs::read_to_string(stl_platform::settings::log_file()).unwrap_or_default();
    assert!(stl_platform::settings::log_file().starts_with(&cfg));
    let line = log
        .lines()
        .rev()
        .find(|l| l.contains("The graphics device was lost"))
        .unwrap_or_else(|| panic!("no loss line in log.txt: {log:?}"));
    assert!(line.contains("upload refused: yes"), "{line}");
    println!("log.txt: {line}");

    // `main`: autosave, then the restart marker names the copy
    let saved = app.save_after_device_loss(Duration::from_secs(60));
    assert!(saved, "the autosave copy wasn't written");
    drop(link); // the test's handle: the engine (and its autosave session) ends with the app
    let marker = marker_of(&app.restart_after_device_loss(saved).unwrap()).expect("a restart marker");
    let copies = stl_platform::autosave::recoverable(&autosave_root(), None);
    assert_eq!(copies.len(), 1, "one autosave copy: {copies:?}");
    assert_eq!(marker, format!("recover:{}", copies[0].id));
    let _ = std::fs::remove_dir_all(autosave_root());
}
