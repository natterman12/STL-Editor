//! The GPU the tests render on, and the gate for picture snapshots.
//!
//! A picture snapshot is only a fair test on the adapter that recorded it: other GPUs and
//! software rasterizers (WARP on the Windows CI runner, llvmpipe / lavapipe on Linux) shade,
//! blend and antialias edges differently, and a 3D view differs in thousands of pixels from the
//! recorded one without anything being wrong. So [`snapshot`] compares the picture with
//! `tests/snapshots/<name>.png` only when the adapter is the one named in
//! `tests/snapshots/adapter.txt` (name, backend and device type, from wgpu's adapter info). On
//! any other adapter it skips the comparison (and writes nothing in `tests/snapshots`) and the
//! test's own structural checks (cap pixels, grid lines, axis colours, body colour...) are what
//! is tested. Every test that calls [`snapshot`] must have such checks. Pictures of egui alone
//! (no 3D view) differ much less between adapters and are compared everywhere, with a small
//! tolerance off the recorded adapter ([`snapshot_2d`]).
//!
//! To record the snapshots on a new machine: put its adapter (printed by a skipped
//! comparison) in `adapter.txt`, then run the tests with `UPDATE_SNAPSHOTS=1`.
//!
//! `WGPU_ADAPTER_NAME=<part of a name>` picks the adapter, for trying the other path locally,
//! for example `WGPU_ADAPTER_NAME="Microsoft Basic Render Driver"` (WARP, what the Windows CI
//! runner has). CI sets nothing: its adapter simply isn't the recorded one.
//!
//! Use from a test file with `#[path = "support/gpu.rs"] mod gpu;`.
#![allow(dead_code)]

// Keeps every test process out of the real 2.0 settings folder (runs before main).
#[path = "test_env.rs"]
mod test_env;

use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::ThreadId;

use egui_kittest::Harness;
use stl_app::viewport::renderer::{block_on, Headless};

/// The test GPU: a [`Headless`], and on a software adapter the right to use one (see [`headless`]).
pub struct TestGpu {
    gpu: Headless,
    _turn: Option<Turn>,
}

impl std::ops::Deref for TestGpu {
    type Target = Headless;
    fn deref(&self) -> &Headless {
        &self.gpu
    }
}

/// [`Headless::new`], or the adapter named by `WGPU_ADAPTER_NAME` when that is set (a panic when
/// no adapter matches, so a mistyped name never turns into skipped tests).
///
/// On a software adapter the tests take turns: WARP crashes now and then (an access violation
/// that ends the test process) when several tests drive their own devices at once, so the test
/// (thread) that made one keeps the turn until its last `TestGpu` is dropped. Real GPUs run the
/// tests in parallel as before.
pub fn headless() -> Option<TestGpu> {
    let turn = Turn::take();
    let gpu = make_headless()?;
    let software = is_software(&gpu);
    Some(TestGpu { gpu, _turn: software.then_some(turn) })
}

fn make_headless() -> Option<Headless> {
    let Ok(want) = std::env::var("WGPU_ADAPTER_NAME") else {
        return Headless::new();
    };
    let want = want.to_lowercase();
    let mut setup = egui_wgpu::WgpuSetup::without_display_handle();
    if let egui_wgpu::WgpuSetup::CreateNew(create) = &mut setup {
        create.native_adapter_selector = Some(Arc::new(move |adapters: &[wgpu::Adapter], _| {
            adapters.iter().find(|a| a.get_info().name.to_lowercase().contains(&want)).cloned().ok_or_else(|| {
                let names: Vec<String> = adapters.iter().map(|a| adapter_key_of(&a.get_info())).collect();
                format!("no adapter named like {want:?} in {names:?}")
            })
        }));
    }
    let instance = block_on(setup.new_instance());
    let config = egui_wgpu::WgpuConfiguration { wgpu_setup: setup, ..Default::default() };
    let render_state = block_on(egui_wgpu::RenderState::create(&config, &instance, None, egui_wgpu::RendererOptions::default()))
        .unwrap_or_else(|e| panic!("WGPU_ADAPTER_NAME: {e}"));
    Some(Headless { render_state })
}

/// One test thread's turn on the GPU, re-entrant (a test may make a second device).
struct Turn;

static OWNER: Mutex<Option<(ThreadId, usize)>> = Mutex::new(None);
static FREED: Condvar = Condvar::new();

impl Turn {
    fn take() -> Turn {
        let me = std::thread::current().id();
        let mut owner = OWNER.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            match &mut *owner {
                None => {
                    *owner = Some((me, 1));
                    return Turn;
                }
                Some((id, n)) if *id == me => {
                    *n += 1;
                    return Turn;
                }
                Some(_) => owner = FREED.wait(owner).unwrap_or_else(|e| e.into_inner()),
            }
        }
    }
}

impl Drop for Turn {
    fn drop(&mut self) {
        let mut owner = OWNER.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((_, n)) = &mut *owner {
            *n -= 1;
            if *n == 0 {
                *owner = None;
                FREED.notify_all();
            }
        }
    }
}

fn adapter_key_of(info: &wgpu::AdapterInfo) -> String {
    format!("{} | {:?} | {:?}", info.name.trim(), info.backend, info.device_type)
}

/// `name | backend | device type` of the adapter `gpu` renders on.
pub fn adapter_key(gpu: &Headless) -> String {
    adapter_key_of(&gpu.render_state.adapter.get_info())
}

/// A software rasterizer (WARP, llvmpipe / lavapipe): no frame-time budgets there.
pub fn is_software(gpu: &Headless) -> bool {
    gpu.render_state.adapter.get_info().device_type == wgpu::DeviceType::Cpu
}

fn adapter_file() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/snapshots/adapter.txt")
}

/// The adapter the snapshots were recorded on: the first line of `adapter.txt` that is not
/// blank or a `#` comment.
pub fn recorded_adapter() -> Option<String> {
    let text = std::fs::read_to_string(adapter_file()).ok()?;
    text.lines().map(str::trim).find(|l| !l.is_empty() && !l.starts_with('#')).map(str::to_owned)
}

/// True when pictures from `gpu` can be compared with the recorded snapshots.
pub fn same_adapter(gpu: &Headless) -> bool {
    recorded_adapter().is_some_and(|r| r == adapter_key(gpu))
}

/// For a 3D view (the viewport's paint callback): compare the harness's picture with
/// `tests/snapshots/<name>.png` on the recorded adapter; elsewhere only say why it was skipped.
pub fn snapshot<State>(harness: &mut Harness<'_, State>, gpu: &Headless, name: &str) {
    if same_adapter(gpu) {
        harness.snapshot(name);
    } else {
        eprintln!(
            "{name}: picture not compared: snapshots were made on {:?}, this is {:?} (structural checks only)",
            recorded_adapter().unwrap_or_else(|| "(no tests/snapshots/adapter.txt)".into()),
            adapter_key(gpu)
        );
    }
}

/// True when `UPDATE_SNAPSHOTS` asks kittest to rewrite snapshots, by kittest's own rule:
/// `true`, `1`, `yes`, `on` or `force`; unset, `false`, `0`, `no` and `off` compare. (Any other
/// value is left to kittest, which rejects it.)
pub fn updating_snapshots() -> bool {
    std::env::var("UPDATE_SNAPSHOTS").is_ok_and(|v| matches!(v.as_str(), "true" | "1" | "yes" | "on" | "force"))
}

/// The top `48` pixel rows of a picture that is all viewport: where the view toolbar is.
pub fn top_band(img: &image::RgbaImage) -> [u32; 4] {
    [0, 0, img.width(), 48.min(img.height())]
}

/// The viewport's view toolbar (Top, Front, ... Fit | Section, egui-drawn at its top right) is
/// in `region` (`[x0, y0, x1, y1]`), on any adapter. Dark theme: its light labels (nothing in
/// the 3D view is that bright: 0 such pixels below the toolbar in every snapshot; about 340 to
/// 450 in it). Light theme: its white panel (the 3D view has no pure white; about 4,300 to 6,000
/// pixels) with dark labels on it (about 410 to 530).
pub fn assert_view_toolbar(img: &image::RgbaImage, dark: bool, region: [u32; 4], name: &str) {
    let [x0, y0, x1, y1] = region;
    let lum = |x: u32, y: u32| {
        let [r, g, b, _] = img.get_pixel(x, y).0;
        (r as u32 + g as u32 + b as u32) / 3
    };
    let cells = || (y0..y1).flat_map(move |y| (x0..x1).map(move |x| (x, y)));
    if dark {
        let labels = cells().filter(|&(x, y)| lum(x, y) >= 185).count();
        assert!(labels >= 150, "{name}: no view toolbar: {labels} label pixels in {region:?}");
        return;
    }
    let white: Vec<(u32, u32)> = cells().filter(|&(x, y)| img.get_pixel(x, y).0[..3].iter().all(|&c| c >= 250)).collect();
    assert!(white.len() >= 2000, "{name}: no view toolbar: {} panel pixels in {region:?}", white.len());
    let (bx0, bx1) = (white.iter().map(|p| p.0).min().unwrap_or(0), white.iter().map(|p| p.0).max().unwrap_or(0));
    let (by0, by1) = (white.iter().map(|p| p.1).min().unwrap_or(0), white.iter().map(|p| p.1).max().unwrap_or(0));
    let labels = (by0..=by1).flat_map(|y| (bx0..=bx1).map(move |x| (x, y))).filter(|&(x, y)| lum(x, y) < 150).count();
    assert!(labels >= 150, "{name}: the view toolbar has no labels: {labels} label pixels");
}

/// For egui alone (panels, menus, dialogs, no 3D view): egui's own shapes come out nearly the
/// same on every adapter, so they are compared everywhere: as recorded (kittest's defaults) on
/// the recorded adapter, and elsewhere with a small colour tolerance for antialiased text and
/// shadows (WARP: at most 85 pixels past the default 0.6, none past 5.0; a missing or moved
/// widget changes hundreds of pixels by far more). Elsewhere the comparison never rewrites the
/// snapshot, even with `UPDATE_SNAPSHOTS` set. Never use it for a picture with the 3D view in it
/// (one made with `init_gpu`): that is [`snapshot`]'s job.
pub fn snapshot_2d<State>(harness: &mut Harness<'_, State>, gpu: &Headless, name: &str) {
    if same_adapter(gpu) {
        harness.snapshot(name);
    } else if updating_snapshots() {
        eprintln!("{name}: not updated: snapshots are recorded on {:?}, this is {:?}", recorded_adapter(), adapter_key(gpu));
    } else {
        let mut options = egui_kittest::SnapshotOptions::default();
        options.threshold = 8.0;
        options.max_failed_pixels = 25;
        harness.snapshot_options(name, &options);
    }
}
