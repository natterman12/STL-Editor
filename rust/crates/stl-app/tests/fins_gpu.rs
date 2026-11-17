//! Milestone 7 (spec 2.1): Support Fins and Draw Walls on a large part, on this machine's real
//! GPU, with every frame painted by egui-wgpu's `Renderer` as the window paints it. Nathan's
//! M6 build lost its device here and then crashed. Neither flow may lose the device or panic.
//!
//! Runs only with `STLEDITOR2_GPU_TESTS=1` (it needs a real GPU and the system web engine, and
//! takes a minute); otherwise it prints "fins_gpu: not run" and passes. CI has only WARP, so this is the
//! lead's local verify step: `STLEDITOR2_GPU_TESTS=1 cargo test -p stl-app --test fins_gpu`.
#![cfg(not(target_arch = "wasm32"))]
#![cfg(any(windows, target_os = "linux"))]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use stl_app::app::StlApp;
use stl_app::engine_link::Command;
use stl_app::engine_link::native::NativeLink;
use stl_app::viewport::pick::Hit;
use stl_app::viewport::renderer::{Headless, Stats};

#[path = "ui/harness.rs"]
mod harness;
use harness::{FakePlatform, SharedLink};

const W: u32 = 1600;
const H: u32 = 1000;

/// A UV sphere of radius `r` resting on the bed, `rings` x `segs` (2 * segs * (rings - 1)
/// triangles), written as a binary STL. Its lower half overhangs, so Support Fins has work.
fn sphere_stl(path: &Path, r: f32, rings: usize, segs: usize) -> usize {
    let pt = |i: usize, j: usize| -> [f32; 3] {
        let th = std::f32::consts::PI * i as f32 / rings as f32;
        let ph = std::f32::consts::TAU * (j % segs) as f32 / segs as f32;
        [r * th.sin() * ph.cos(), r * th.sin() * ph.sin(), r + r * th.cos()]
    };
    let mut tris: Vec<[[f32; 3]; 3]> = Vec::new();
    for i in 0..rings {
        for j in 0..segs {
            let (a, b, c, d) = (pt(i, j), pt(i, j + 1), pt(i + 1, j), pt(i + 1, j + 1));
            if i != 0 {
                tris.push([a, c, b]);
            }
            if i != rings - 1 {
                tris.push([b, c, d]);
            }
        }
    }
    let mut out = vec![0u8; 80];
    out.extend_from_slice(&(tris.len() as u32).to_le_bytes());
    for t in &tris {
        out.extend_from_slice(&[0u8; 12]);
        for v in t {
            for k in v {
                out.extend_from_slice(&k.to_le_bytes());
            }
        }
        out.extend_from_slice(&[0u8; 2]);
    }
    std::fs::write(path, out).unwrap();
    tris.len()
}

fn input(events: Vec<egui::Event>) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(W as f32, H as f32),
        )),
        events,
        ..Default::default()
    }
}

struct Run {
    gpu: Headless,
    ctx: egui::Context,
    frame: eframe::Frame,
    app: StlApp,
    link: Arc<NativeLink>,
    st: Arc<Stats>,
}

impl Run {
    /// One frame: the app's `ui`, then a paint through egui-wgpu. Fails on a lost device.
    fn frame(&mut self, events: Vec<egui::Event>) {
        let app = &mut self.app;
        let out = self
            .ctx
            .run_ui(input(events), |ui| eframe::App::ui(app, ui, &mut self.frame));
        self.gpu.paint_egui(&self.ctx, out, W, H);
        let refused = self.gpu.render_state.renderer.read().upload_refused();
        assert!(
            !refused && !self.app.viewport.device_lost(true),
            "the GPU device was lost (upload refused: {refused}; reason: {:?})",
            self.st.lost_reason()
        );
        assert_eq!(self.st.gpu_errors(), 0, "the viewport reported a GPU error");
    }

    /// Frames until the engine is idle and every upload is on the GPU, a few frames in a row.
    fn settle(&mut self, what: &str) {
        let end = Instant::now() + Duration::from_secs(300);
        let mut quiet = 0;
        while quiet < 8 {
            self.frame(Vec::new());
            let idle = !self.link.is_busy() && !self.st.upload_pending.load(Ordering::Relaxed);
            quiet = if idle { quiet + 1 } else { 0 };
            assert!(Instant::now() < end, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn until(&mut self, what: &str, done: impl Fn(&StlApp) -> bool) {
        let end = Instant::now() + Duration::from_secs(300);
        while !done(&self.app) {
            self.frame(Vec::new());
            assert!(Instant::now() < end, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn click(&mut self, at: egui::Pos2) {
        use egui::{Event, Modifiers, PointerButton};
        self.frame(vec![Event::PointerMoved(at)]);
        self.frame(vec![Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        }]);
        self.frame(vec![Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }]);
        self.frame(Vec::new());
    }
}

fn tmp_dir() -> PathBuf {
    let d = std::env::temp_dir().join(format!("stle2-fins-gpu-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn fins_and_walls_on_a_large_part_keep_the_device() {
    if std::env::var("STLEDITOR2_GPU_TESTS").as_deref() != Ok("1") {
        println!(
            "fins_gpu: not run (set STLEDITOR2_GPU_TESTS=1 to run fins_and_walls_on_a_large_part_keep_the_device on the real GPU)"
        );
        return;
    }
    let Some(gpu) = Headless::new() else {
        panic!("STLEDITOR2_GPU_TESTS=1 but there is no GPU adapter");
    };
    let info = gpu.render_state.adapter.get_info();
    println!(
        "adapter: {} ({:?}, {} {})",
        info.name, info.backend, info.driver, info.driver_info
    );

    let dir = tmp_dir();
    let part = dir.join("sphere_200k.stl");
    let tris = sphere_stl(&part, 30.0, 250, 400);
    assert!(tris >= 199_000, "{tris} triangles");

    let link = Arc::new(NativeLink::new());
    let ctx = egui::Context::default();
    let mut cc = eframe::CreationContext::_new_kittest(ctx.clone());
    cc.wgpu_render_state = Some(gpu.render_state.clone());
    let mut viewport = stl_app::viewport::Viewport::new();
    viewport.init_gpu(&gpu.render_state);
    let st = viewport.stats_handle();
    let mut app = StlApp::new(
        &cc.egui_ctx,
        Box::new(FakePlatform::default()),
        Box::new(SharedLink { inner: link.clone() }),
    );
    app.viewport = viewport;
    let mut frame = eframe::Frame::_new_kittest();
    frame.wgpu_render_state = Some(gpu.render_state.clone());
    let mut run = Run {
        gpu,
        ctx,
        frame,
        app,
        link,
        st,
    };
    stl_app::fonts::install(&run.ctx);

    // the part, as File > Import does
    let bytes = std::fs::read(&part).unwrap();
    run.app.link.send(Command::Import {
        name: "sphere_200k.stl".into(),
        bytes,
    });
    run.until("the import", |a| !a.bodies.is_empty());
    run.settle("the part to draw");
    let body = run.app.bodies[0].id.clone();
    run.app.shell.selection = vec![body.clone()];

    // ---- Support Fins: open (the preview generates the fins), then OK
    let t = Instant::now();
    let _ = run.app.open_tool("supportfins", None);
    let end = Instant::now() + Duration::from_secs(120);
    while !(stl_app::supports::is_installed() && run.app.tool().is_some_and(|t| t.def.key == "supportfins")) {
        run.settle("the Support Fins engine");
        if run.app.tool().is_none() {
            let _ = run.app.open_tool("supportfins", None);
        }
        assert!(Instant::now() < end, "Support Fins never opened");
    }
    run.settle("the fins preview");
    for _ in 0..30 {
        run.frame(Vec::new());
    }
    run.settle("the fins preview");
    println!("Support Fins preview: {:.1} s", t.elapsed().as_secs_f64());
    let steps = run.app.timeline_view.items.len();
    run.app.tool_ok();
    run.until("the Support Fins step", |a| a.timeline_view.items.len() > steps);
    run.settle("the fins");
    assert!(
        run.app.timeline_view.items.iter().any(|i| i.kind == "supportfins"),
        "a Support Fins step was added"
    );
    println!("Support Fins done: {:.1} s", t.elapsed().as_secs_f64());

    // ---- Draw Walls ("Draw Fins"): open, draw one wall with two clicks on the part. With fins
    // on the part, 1.x (and 2.0) reopen Support Fins in its Add fins mode, which draws them.
    let t = Instant::now();
    run.app.shell.selection = vec![body.clone()];
    let end = Instant::now() + Duration::from_secs(60);
    loop {
        let _ = run.app.open_tool("supportwall", None);
        run.settle("Draw Walls");
        if let Some(t) = run
            .app
            .tool()
            .filter(|t| matches!(t.def.key, "supportwall" | "supportfins"))
        {
            println!("Draw Walls opened {} (mode {:?})", t.def.key, t.values.get("_mode"));
            break;
        }
        assert!(Instant::now() < end, "Draw Walls never opened");
    }
    // two points on the part's lower half, 60 px apart, found by picking
    let rect = run.app.viewport.rect();
    let mut hits = Vec::new();
    'scan: for iy in (0..60).rev() {
        for ix in 20..40 {
            let at = egui::pos2(
                rect.min.x + rect.width() * ix as f32 / 60.0,
                rect.min.y + rect.height() * iy as f32 / 60.0,
            );
            if let Some(Hit::Body { id, .. }) = run.app.viewport.pick(at, true, false)
                && id == body
                && hits
                    .iter()
                    .all(|h: &egui::Pos2| (h.y - at.y).abs() < 1.0 && (h.x - at.x).abs() >= 60.0)
            {
                hits.push(at);
                if hits.len() == 2 {
                    break 'scan;
                }
            }
        }
        hits.clear();
    }
    assert_eq!(hits.len(), 2, "no two points on the part to draw a wall between");
    let walls_before = run
        .app
        .tool()
        .and_then(|t| t.values.get("walls").and_then(|w| w.as_array().map(Vec::len)))
        .unwrap_or(0);
    run.click(hits[0]);
    run.click(hits[1]);
    run.settle("the wall preview");
    let walls = run
        .app
        .tool()
        .and_then(|t| t.values.get("walls").and_then(|w| w.as_array().map(Vec::len)))
        .unwrap_or(0);
    assert_eq!(walls, walls_before + 1, "two clicks drew one wall");
    run.app.tool_ok();
    run.until("the tool to close", |a| a.tool().is_none());
    run.settle("the wall");
    let walled = run.app.timeline_view.items.iter().any(|i| {
        matches!(i.kind.as_str(), "supportfins" | "supportwall")
            && i.params
                .get("walls")
                .and_then(|w| w.as_array())
                .is_some_and(|w| !w.is_empty())
    });
    assert!(walled, "the drawn wall is in a support step");
    println!("Draw Walls done: {:.1} s", t.elapsed().as_secs_f64());

    for _ in 0..30 {
        run.frame(Vec::new());
    }
    assert!(
        run.st.lost_reason().is_none(),
        "the device-lost callback ran: {:?}",
        run.st.lost_reason()
    );
    drop(run);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The same two steps in the real app window (winit, the swapchain, and the fins host's own
/// hidden tao window and WebView2 in one process): the app binary, driven by its debug-only
/// `STLEDITOR2_TEST_FINS=1` hook (`app_device.rs`), must get through Support Fins and Draw Walls
/// and close normally. Nathan's M6 build ended with 0xc000041d the moment Support Fins started
/// its host, because the host's window and the app's window shared a Win32 window class.
#[test]
fn fins_and_walls_in_the_real_window() {
    if std::env::var("STLEDITOR2_GPU_TESTS").as_deref() != Ok("1") {
        println!(
            "fins_gpu: not run (set STLEDITOR2_GPU_TESTS=1 to run fins_and_walls_in_the_real_window; it opens a window)"
        );
        return;
    }
    if !cfg!(debug_assertions) {
        println!("fins_gpu: not run (fins_and_walls_in_the_real_window needs a debug build; its hook is debug-only)");
        return;
    }
    let dir = std::env::temp_dir().join(format!("stle2-fins-window-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("cfg")).unwrap();
    let part = dir.join("sphere_200k.stl");
    sphere_stl(&part, 30.0, 250, 400);
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_stl-app"))
        .arg(&part)
        .current_dir(&dir)
        .env("STLEDITOR2_CONFIG_DIR", dir.join("cfg")) // never Nathan's real folders
        .env("STLEDITOR2_TEST_FINS", "1")
        .env_remove("STLEDITOR2_TEST_DEVICE_LOSS")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("start the app");
    let mut err = child.stderr.take().unwrap();
    let reader = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = std::io::Read::read_to_string(&mut err, &mut s);
        s
    });
    let end = Instant::now() + Duration::from_secs(300);
    let status = loop {
        if let Some(s) = child.try_wait().unwrap() {
            break s;
        }
        if Instant::now() > end {
            let _ = child.kill();
            panic!("the app window didn't finish the fins steps in 5 minutes");
        }
        std::thread::sleep(Duration::from_millis(200));
    };
    let stderr = reader.join().unwrap();
    for line in stderr
        .lines()
        .filter(|l| l.starts_with("STLEDITOR2_TEST_FINS") || l.contains("device was lost"))
    {
        println!("{line}");
    }
    assert!(
        status.success(),
        "the app ended with {status:?} (0xc000041d is the M6 crash)\n{stderr}"
    );
    assert!(
        stderr.contains("STLEDITOR2_TEST_FINS: done"),
        "the app closed before finishing:\n{stderr}"
    );
    assert!(
        !stderr.contains("device was lost"),
        "the GPU device was lost:\n{stderr}"
    );
    let log = std::fs::read_to_string(dir.join("cfg").join("data").join("log.txt")).unwrap_or_default();
    assert!(log.is_empty(), "log.txt: {log}");
    let _ = std::fs::remove_dir_all(&dir);
}
