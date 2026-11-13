//! UI smoothness bench (`stl-app --bench <out.json> [--quick]`).
//!
//! Drives the real window: orbit / pan / zoom / idle / one rebuild, writes frame stats as JSON.
//! There is no mesh Fillet in `stl-engine`. The rebuild is Hollow at fine quality (about 4 s on
//! the bench mesh). Offset at 0.2 mm takes minutes and is not used.

#![cfg(not(target_arch = "wasm32"))]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use serde_json::{json, Value};

use crate::app::StlApp;
use crate::engine_link::native::NativeLink;
use crate::engine_link::{Command, EngineLink, Update};
use crate::platform::native::NativePlatform;
use crate::viewport::Viewport;

const ORBIT_S: f64 = 10.0;
const PAN_S: f64 = 5.0;
const ZOOM_S: f64 = 5.0;
const IDLE_S: f64 = 5.0;
const QUICK_S: f64 = 1.0;
/// Stop the rebuild row if the new mesh has not been drawn by then.
const REBUILD_GIVE_UP_S: f64 = 90.0;

const REBUILD_KIND: &str = "hollow";
const REBUILD_NOTE: &str = "\
stl-engine has no Fillet. Release engine times on this 504k-triangle mesh: \
simplify 50% 376 ms; hollow draft 636 ms; hollow normal 1.43 s; hollow fine 4.16 s; \
remesh edge 2 mm 3.09 s (down to 5k triangles); remesh edge 1 mm 2.35 s. \
Offset 0.2 mm took about 200–360 s in earlier full runs, and offset 5 mm was still running at 45 s: \
a 0.2 mm grow only simplifies within 0.08 mm, so the Minkowski sum still sees most of the triangles. \
This row is Hollow, wall 2 mm, fine quality. Frame times are the frames that kept orbiting until the new mesh was uploaded and drawn. \
Rayon's pool keeps two physical cores free when the chip has hyperthreads, one physical core free without them, and two processors free where the layout can't be read. \
On Windows the window thread runs above normal priority and is not pinned. The engine thread and its Rayon workers run below the window. Other threads stay at normal priority.";
const IDLE_NOTE: &str = "\
Idle frames are paints something other than this bench asked for. Not counted: the frame already queued by the last zoom, \
egui's extra settle frame after an immediate repaint, and the wake this bench schedules so it can leave the idle window. \
Toasts are dismissed and any camera flight is finished as idle starts.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Loading,
    Orbit,
    Pan,
    Zoom,
    Idle,
    Rebuild,
    Done,
}

struct ScenarioStat {
    times_ms: Vec<f64>,
    idle_frames: u64,
    uploads_end: u64,
}

impl ScenarioStat {
    fn new() -> Self {
        Self { times_ms: Vec::new(), idle_frames: 0, uploads_end: 0 }
    }

    fn record(&mut self, dt_ms: f64) {
        self.times_ms.push(dt_ms);
    }

    fn to_json(&self) -> Value {
        let (median, p99, longest) = summarise(&self.times_ms);
        json!({
            "frames": self.times_ms.len(),
            "median_ms": median,
            "p99_ms": p99,
            "longest_ms": longest,
            "idle_frames": self.idle_frames,
            "uploads": self.uploads_end,
        })
    }
}

fn summarise(times: &[f64]) -> (f64, f64, f64) {
    if times.is_empty() {
        return (0.0, 0.0, 0.0);
    }
    let mut v = times.to_vec();
    v.sort_by(|a, b| a.total_cmp(b));
    let median = v[v.len() / 2];
    let idx = ((v.len() as f64 - 1.0) * 0.99).round() as usize;
    let p99 = v[idx.min(v.len() - 1)];
    let longest = v[v.len() - 1];
    (median, p99, longest)
}

/// `EngineLink` over a shared [`NativeLink`] so the driver can call `is_busy`.
struct SharedLink {
    inner: Arc<NativeLink>,
}

impl EngineLink for SharedLink {
    fn send(&self, c: Command) {
        self.inner.send(c);
    }
    fn poll(&self) -> Vec<Update> {
        self.inner.poll()
    }
    fn restart(&self) {
        self.inner.restart();
    }
}

/// Wrapper that drives the bench phases around [`StlApp`].
pub struct BenchApp {
    app: StlApp,
    link: Arc<NativeLink>,
    out: PathBuf,
    quick: bool,
    launched: Instant,
    first_frame_ms: Option<f64>,
    phase: Phase,
    phase_started: Instant,
    last_frame: Instant,
    orbit: ScenarioStat,
    pan: ScenarioStat,
    zoom: ScenarioStat,
    idle: ScenarioStat,
    rebuild: ScenarioStat,
    rebuild_sent: bool,
    rebuild_uploads_before: u64,
    rebuild_started: Option<Instant>,
    rebuild_ms: f64,
    rebuild_gave_up: bool,
    rebuild_slow: u32,
    idle_prepared: bool,
    mesh_path: PathBuf,
    open_sent: bool,
}

impl BenchApp {
    pub fn new(cc: &eframe::CreationContext<'_>, out: PathBuf, quick: bool, launched: Instant) -> Self {
        // GPU first, then the engine, the same order as a normal launch. The smaller Rayon
        // pool and the lower-priority engine threads are set up in `main` and `NativeLink`.
        let mut viewport = Viewport::new();
        if let Some(rs) = cc.wgpu_render_state.as_ref() {
            viewport.init_gpu(rs);
        }
        let link = Arc::new(NativeLink::new());
        let wake_ctx = cc.egui_ctx.clone();
        link.set_waker(move || wake_ctx.request_repaint());
        let wake_ctx = cc.egui_ctx.clone();
        let mut app = StlApp::new(
            &cc.egui_ctx,
            Box::new(NativePlatform::with_waker(move || wake_ctx.request_repaint())),
            Box::new(SharedLink { inner: link.clone() }),
        );
        app.viewport = viewport;
        let mesh_path = find_bench_mesh().unwrap_or_else(|| PathBuf::from("dev/bench/test_mesh.stl"));
        Self {
            app,
            link,
            out,
            quick,
            launched,
            first_frame_ms: None,
            phase: Phase::Loading,
            phase_started: Instant::now(),
            last_frame: Instant::now(),
            orbit: ScenarioStat::new(),
            pan: ScenarioStat::new(),
            zoom: ScenarioStat::new(),
            idle: ScenarioStat::new(),
            rebuild: ScenarioStat::new(),
            rebuild_sent: false,
            rebuild_uploads_before: 0,
            rebuild_started: None,
            rebuild_ms: 0.0,
            rebuild_gave_up: false,
            rebuild_slow: 0,
            idle_prepared: false,
            mesh_path,
            open_sent: false,
        }
    }

    fn phase_secs(&self, full: f64) -> f64 {
        if self.quick { QUICK_S } else { full }
    }

    fn finish_and_close(&mut self, ctx: &egui::Context) {
        let report = json!({
            "date": chrono_date(),
            "quick": self.quick,
            "launch_to_first_frame_ms": self.first_frame_ms.unwrap_or(0.0),
            "rebuild_kind": REBUILD_KIND,
            "rebuild_ms": self.rebuild_ms,
            "rebuild_gave_up": self.rebuild_gave_up,
            "rebuild_note": if self.rebuild_gave_up {
                format!("{REBUILD_NOTE} This run stopped after {REBUILD_GIVE_UP_S:.0} s before the new mesh was drawn.")
            } else {
                REBUILD_NOTE.to_string()
            },
            "idle_note": IDLE_NOTE,
            "mesh": self.mesh_path.display().to_string(),
            "scenarios": {
                "orbit": self.orbit.to_json(),
                "pan": self.pan.to_json(),
                "zoom": self.zoom.to_json(),
                "idle": self.idle.to_json(),
                "fillet_rebuild": self.rebuild.to_json(),
            }
        });
        if let Err(e) = std::fs::write(&self.out, serde_json::to_string_pretty(&report).unwrap_or_else(|_| "{}".into())) {
            eprintln!("uibench: could not write {}: {e}", self.out.display());
        } else {
            eprintln!("uibench: wrote {}", self.out.display());
        }
        self.phase = Phase::Done;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
}

impl eframe::App for BenchApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let now = Instant::now();
        if self.first_frame_ms.is_none() {
            self.first_frame_ms = Some(self.launched.elapsed().as_secs_f64() * 1000.0);
        }
        let dt_ms = now.duration_since(self.last_frame).as_secs_f64() * 1000.0;
        self.last_frame = now;

        if !self.open_sent {
            match std::fs::read(&self.mesh_path) {
                Ok(bytes) => {
                    let name = self.mesh_path.file_name().and_then(|s| s.to_str()).unwrap_or("test_mesh.stl").to_owned();
                    self.link.send(Command::Import { name, bytes });
                    self.open_sent = true;
                    self.phase_started = Instant::now();
                }
                Err(e) => {
                    eprintln!("uibench: {}: {e}", self.mesh_path.display());
                    self.finish_and_close(ui.ctx());
                    return;
                }
            }
        }

        // Drive camera / rebuild before layout so this frame shows the change.
        match self.phase {
            Phase::Orbit => {
                self.orbit.record(dt_ms);
                self.app.viewport.camera_mut().rotate(6.0, 2.0);
                ui.ctx().request_repaint();
            }
            Phase::Pan => {
                self.pan.record(dt_ms);
                self.app.viewport.camera_mut().pan(4.0, -3.0);
                ui.ctx().request_repaint();
            }
            Phase::Zoom => {
                self.zoom.record(dt_ms);
                let cam = self.app.viewport.camera();
                let cursor = [cam.width * 0.5, cam.height * 0.5];
                self.app.viewport.camera_mut().wheel(40.0, cursor);
                ui.ctx().request_repaint();
            }
            Phase::Idle => {
                if !self.idle_prepared {
                    // A toast expiry or a camera flight would wake the window for a real change.
                    // Idle starts with neither pending.
                    self.app.toasts.clear();
                    self.app.viewport.camera_mut().finish_flight();
                    self.idle_prepared = true;
                }
                let causes = ui.ctx().repaint_causes();
                let only_bench = !causes.is_empty()
                    && causes.iter().all(|c| c.file.rsplit(['/', '\\']).next() == Some("bench.rs"));
                if only_bench {
                    eprintln!(
                        "uibench: idle wake not counted ({dt_ms:.2} ms) {}",
                        causes.iter().map(|c| c.to_string()).collect::<Vec<_>>().join(" | ")
                    );
                } else {
                    self.idle.record(dt_ms);
                    self.idle.idle_frames += 1;
                    eprintln!(
                        "uibench: idle frame counted ({dt_ms:.2} ms) {}",
                        if causes.is_empty() {
                            "(no repaint cause)".to_string()
                        } else {
                            causes.iter().map(|c| c.to_string()).collect::<Vec<_>>().join(" | ")
                        }
                    );
                }
                let left = self.phase_secs(IDLE_S) - self.phase_started.elapsed().as_secs_f64();
                if left > 0.0 {
                    ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(left));
                }
            }
            Phase::Rebuild => {
                // Keep the camera moving so every one of these frames is a real paint, not a gap
                // while the window had nothing to draw.
                self.app.viewport.camera_mut().rotate(6.0, 2.0);
                ui.ctx().request_repaint();
                if !self.rebuild_sent {
                    if let Some(id) = self.app.bodies.first().map(|b| b.id.clone()) {
                        self.rebuild_uploads_before = self.app.viewport.stats().uploads();
                        let params = json!({
                            "body": id,
                            "thickness": 2.0,
                            "resolution": "fine",
                        });
                        self.link.send(Command::AddFeature {
                            kind: REBUILD_KIND.into(),
                            params,
                            index: None,
                        });
                        self.rebuild_sent = true;
                        self.rebuild_started = Some(Instant::now());
                    } else {
                        eprintln!("uibench: no body to rebuild");
                        self.finish_and_close(ui.ctx());
                        return;
                    }
                }
                if dt_ms > 16.7 {
                    if self.rebuild_slow < 24 {
                        let at = self.rebuild_started.map(|t| t.elapsed().as_secs_f64() * 1000.0).unwrap_or(0.0);
                        eprintln!("uibench: rebuild frame {dt_ms:.2} ms at {at:.0} ms into the rebuild");
                    }
                    self.rebuild_slow += 1;
                }
                self.rebuild.record(dt_ms);
            }
            Phase::Loading | Phase::Done => {}
        }

        self.app.ui(ui, frame);

        // Advance phases after poll/layout so body state is current.
        match self.phase {
            Phase::Loading => {
                if !self.link.is_busy() && !self.app.bodies.is_empty() {
                    self.app.viewport.fit_all();
                    self.phase = Phase::Orbit;
                    self.phase_started = Instant::now();
                    self.last_frame = Instant::now();
                }
                ui.ctx().request_repaint();
            }
            Phase::Orbit if self.phase_started.elapsed().as_secs_f64() >= self.phase_secs(ORBIT_S) => {
                self.orbit.uploads_end = self.app.viewport.stats().uploads();
                self.phase = Phase::Pan;
                self.phase_started = Instant::now();
            }
            Phase::Pan if self.phase_started.elapsed().as_secs_f64() >= self.phase_secs(PAN_S) => {
                self.pan.uploads_end = self.app.viewport.stats().uploads();
                self.phase = Phase::Zoom;
                self.phase_started = Instant::now();
            }
            Phase::Zoom if self.phase_started.elapsed().as_secs_f64() >= self.phase_secs(ZOOM_S) => {
                self.zoom.uploads_end = self.app.viewport.stats().uploads();
                self.phase = Phase::Idle;
                self.phase_started = Instant::now();
            }
            Phase::Idle if self.phase_started.elapsed().as_secs_f64() >= self.phase_secs(IDLE_S) => {
                self.idle.uploads_end = self.app.viewport.stats().uploads();
                self.phase = Phase::Rebuild;
                self.phase_started = Instant::now();
                self.rebuild_sent = false;
                // The next frame is the first orbiting rebuild frame. Drop the idle gap.
                self.last_frame = Instant::now();
                ui.ctx().request_repaint();
            }
            Phase::Rebuild if self.rebuild_sent => {
                let uploads = self.app.viewport.stats().uploads();
                let pending = self.app.viewport.stats().upload_pending.load(std::sync::atomic::Ordering::Relaxed);
                // `uploads` ticks when the new revision's last chunk has been copied, and that
                // same paint draws it. Seen here on the following frame, once the engine is idle.
                let shown = uploads > self.rebuild_uploads_before && !pending && !self.link.is_busy();
                let gave_up = self.phase_started.elapsed().as_secs_f64() > REBUILD_GIVE_UP_S;
                if shown || gave_up {
                    self.rebuild.uploads_end = uploads;
                    self.rebuild_ms = self.rebuild_started.map(|t| t.elapsed().as_secs_f64() * 1000.0).unwrap_or(0.0);
                    self.rebuild_gave_up = gave_up && !shown;
                    if self.rebuild_gave_up {
                        eprintln!(
                            "uibench: rebuild gave up after {:.0} ms, uploads {uploads} (before {})",
                            self.rebuild_ms, self.rebuild_uploads_before
                        );
                    }
                    eprintln!("uibench: rebuild frames over 16.7 ms: {}", self.rebuild_slow);
                    self.finish_and_close(ui.ctx());
                }
            }
            _ => {}
        }
    }
}

fn chrono_date() -> String {
    // Local YYYY-MM-DD without pulling in chrono.
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let days = (secs / 86400) as i64;
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}")
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

fn find_bench_mesh() -> Option<PathBuf> {
    let candidates = [
        PathBuf::from("dev/bench/test_mesh.stl"),
        PathBuf::from("../dev/bench/test_mesh.stl"),
        PathBuf::from("../../dev/bench/test_mesh.stl"),
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../dev/bench/test_mesh.stl"),
    ];
    candidates.into_iter().find(|p| p.is_file())
}

/// Parse `--bench <out.json>` and optional `--quick` from process args.
/// Returns `None` when this process is not in bench mode.
pub fn parse_args(args: impl IntoIterator<Item = String>) -> Option<(PathBuf, bool)> {
    let mut args = args.into_iter().peekable();
    let mut out = None;
    let mut quick = false;
    while let Some(a) = args.next() {
        if a == "--bench" {
            out = args.next().map(PathBuf::from);
        } else if let Some(path) = a.strip_prefix("--bench=") {
            out = Some(PathBuf::from(path));
        } else if a == "--quick" {
            quick = true;
        }
    }
    out.map(|p| (p, quick))
}
