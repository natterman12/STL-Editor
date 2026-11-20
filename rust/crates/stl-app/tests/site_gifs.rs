//! The website's GIFs of the 2.0 tools (`docs/img/*.gif`): short loops of the real app, drawn
//! frame by frame on the GPU with the mouse pointer showing, in the dark theme. The 2.0 app's own
//! counterpart to a screen recording, so they can be made again whenever the interface changes.
//!
//! Ignored by default: it needs a GPU (lavapipe works), `ffmpeg` on the PATH, and writes into
//! `docs/img` (the frames go to `dev/out/site-gifs/<name>/`). Run with
//! `cargo test -p stl-app --profile test-fast --test site_gifs -- --ignored`. To make only some,
//! list them in `SITE_GIFS` (`sketch_cut`, `hole_insert`, `fit_to_bed`, comma-separated).
#![cfg(not(target_arch = "wasm32"))]

use std::path::PathBuf;

use egui::{Event, PointerButton, Pos2, pos2};
use egui_kittest::kittest::Queryable;
use serde_json::json;
use stl_app::engine_link::native::NativeLink;
use stl_app::sketch::Entity;
use stl_app::viewport::renderer::Headless;

#[path = "support/wgpu_renderer.rs"]
mod wgpu_renderer;

#[path = "ui/harness.rs"]
mod harness;

#[path = "support/site.rs"]
mod site;

use harness::{fixture_stl, import_stl, repo, settle};
use site::{H, build_bracket, crop_to_window, img_dir, iso, wait_meshes};

/// The window the GIFs are drawn in; the GIF itself is scaled to [`GIF_WIDTH`].
const SIZE: egui::Vec2 = egui::vec2(1280.0, 720.0);
const GIF_WIDTH: u32 = 960;
const FPS: u32 = 12;

/// The frames of one GIF, and where the pointer is.
struct Clip {
    name: &'static str,
    frames: Vec<image::RgbaImage>,
    pointer: Pos2,
}

impl Clip {
    fn new(name: &'static str) -> Self {
        Clip {
            name,
            frames: Vec::new(),
            pointer: pos2(SIZE.x * 0.75, SIZE.y * 0.8),
        }
    }

    /// Draw the window as it is now and keep it as the next frame.
    fn frame(&mut self, h: &mut H) {
        h.state_mut().viewport.flush();
        h.run_steps(1);
        let img = h.render().unwrap_or_else(|e| panic!("render {}: {e}", self.name));
        self.frames.push(crop_to_window(&img));
    }

    /// Keep the window as it is for `seconds`.
    fn hold(&mut self, h: &mut H, seconds: f32) {
        self.frame(h);
        let last = self.frames.last().unwrap().clone();
        for _ in 1..((seconds * FPS as f32).round() as usize).max(1) {
            self.frames.push(last.clone());
        }
    }

    /// Frames while the app animates (a camera flight, a preview coming back).
    fn run(&mut self, h: &mut H, frames: usize) {
        for _ in 0..frames {
            h.run_steps(4);
            self.frame(h);
        }
    }

    /// Move the pointer to `to` over `seconds`, easing in and out, one frame per step.
    fn glide(&mut self, h: &mut H, to: Pos2, seconds: f32) {
        let from = self.pointer;
        let n = ((seconds * FPS as f32).round() as usize).max(1);
        for i in 1..=n {
            let t = i as f32 / n as f32;
            let e = t * t * (3.0 - 2.0 * t);
            let p = from + (to - from) * e;
            h.event(Event::PointerMoved(p));
            self.frame(h);
        }
        self.pointer = to;
    }

    /// Press and release the left button where the pointer is.
    fn click(&mut self, h: &mut H) {
        for pressed in [true, false] {
            h.event(Event::PointerButton {
                pos: self.pointer,
                button: PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            });
            h.step();
        }
        self.frame(h);
    }

    /// Glide to the ribbon button labelled `label` and click it.
    fn click_button(&mut self, h: &mut H, label: &str) {
        let r = h
            .query_all_by_label(label)
            .map(|n| n.rect())
            .find(|r| r.center().y < 160.0)
            .unwrap_or_else(|| panic!("no ribbon button {label}"));
        self.glide(h, r.center(), 0.6);
        self.click(h);
    }

    /// The frames to `dev/out/site-gifs/<name>/`, then `ffmpeg` makes `docs/img/<name>.gif`
    /// with one palette for the whole clip.
    fn save(self) {
        let dir = repo().join("dev/out/site-gifs").join(self.name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for (i, f) in self.frames.iter().enumerate() {
            f.save(dir.join(format!("{i:04}.png"))).unwrap();
        }
        let out: PathBuf = img_dir().join(format!("{}.gif", self.name));
        let filter = format!(
            "scale={GIF_WIDTH}:-1:flags=lanczos,split[a][b];[a]palettegen=max_colors=128:stats_mode=diff[p];\
             [b][p]paletteuse=dither=bayer:bayer_scale=4:diff_mode=rectangle"
        );
        let status = std::process::Command::new("ffmpeg")
            .args(["-y", "-loglevel", "error", "-framerate", &FPS.to_string(), "-i"])
            .arg(dir.join("%04d.png"))
            .args(["-vf", &filter, "-loop", "0"])
            .arg(&out)
            .status()
            .expect("ffmpeg is on the PATH");
        assert!(status.success(), "ffmpeg made {}", out.display());
        let kb = std::fs::metadata(&out).map(|m| m.len() / 1024).unwrap_or(0);
        eprintln!("wrote {} ({} frames, {kb} KB)", out.display(), self.frames.len());
    }
}

/// Lavapipe draws the ground grid as large grey blocks; on a real graphics card set
/// SITE_SHOTS_GRID=1 to keep it, as the app shows it.
fn prepare(h: &mut H) {
    if std::env::var("SITE_SHOTS_GRID").is_err() {
        h.state_mut().viewport.set_grid_visible(false);
    }
    h.state_mut().shell.selection.clear();
    wait_meshes(h);
}

/// Run the tool's live preview now (the app waits a moment after a change; the test sets values
/// faster than that) and wait for it to come back.
fn preview(h: &mut H, link: &NativeLink) {
    h.state_mut().run_preview_now(web_time::Instant::now());
    for _ in 0..20 {
        settle(h, link);
        h.run_steps(2);
    }
}

/// The tool panel's OK button.
fn ok_button(h: &H) -> Pos2 {
    h.query_all_by_label("OK")
        .map(|n| n.rect().center())
        .next_back()
        .expect("the OK button")
}

fn world(h: &H, p: [f64; 3]) -> Pos2 {
    h.state().viewport.world_to_screen(p)
}

fn sketch_uv(h: &H, uv: [f64; 2]) -> Pos2 {
    let s = h.state().sketch().expect("a sketch is open");
    h.state().viewport.world_to_screen(s.to_3(uv))
}

/// Sketch a rectangle on the bracket's top, give it its sizes, and cut it 5 mm into the part.
fn sketch_cut(h: &mut H, link: &NativeLink) -> Clip {
    let mut c = Clip::new("sketch-cut");
    build_bracket(h, link);
    prepare(h);
    c.hold(h, 0.8);

    // open a sketch on the top face
    c.click_button(h, "Sketch");
    let top = world(h, [22.0, -10.0, 8.0]);
    c.glide(h, top, 0.7);
    c.click(h);
    settle(h, link);
    let body = h.state().bodies[0].id.clone();
    let face = json!({"origin": [0.0, 0.0, 8.0], "normal": [0.0, 0.0, 1.0], "xdir": [1.0, 0.0, 0.0],
        "label": "Top", "body": body});
    if h.state().sketch().is_none() {
        h.state_mut().enter_sketch(Some(face), None);
    }
    settle(h, link);
    c.run(h, 10);
    assert!(h.state().sketch().is_some(), "the sketch opened");

    // a rectangle, drawn with the pointer
    h.state_mut().sketch_set_tool("rect");
    let a = sketch_uv(h, [8.0, -24.0]);
    c.glide(h, a, 0.6);
    c.click(h);
    let b = sketch_uv(h, [24.0, -14.0]);
    c.glide(h, b, 1.0);
    c.click(h);
    settle(h, link);
    c.hold(h, 0.4);

    // its sizes: 16 × 10 mm (drawn to them, so the solver leaves it where it is)
    let rect = {
        let s = h.state().sketch().expect("sketch open");
        s.entities
            .iter()
            .find(|e| matches!(e, Entity::Rect(_)))
            .and_then(|e| e.id())
            .expect("rect drawn")
            .to_owned()
    };
    for (id, kind, a, b, off, value) in [
        ("dw", "hdistance", "c0", "c1", -4.0, 16.0),
        ("dh", "vdistance", "c1", "c2", 4.0, 10.0),
    ] {
        let m = h.state_mut().sketch_mode_mut().expect("sketch open");
        m.s.constraints.push(
            serde_json::from_value(json!({"id": id, "type": kind, "a": {"e": rect, "k": a},
                "b": {"e": rect, "k": b}, "off": off, "value": value}))
            .unwrap(),
        );
        h.state_mut().sketch_set_tool("select");
        settle(h, link);
        c.hold(h, 0.6);
    }

    // Finish & Extrude, as a 5 mm cut
    c.click_button(h, "Finish & Extrude");
    if h.state().tool().is_none() {
        h.state_mut().finish_sketch(true);
    }
    settle(h, link);
    harness::wait_for(h, link, "Extrude to open", |h| {
        h.state().tool().is_some_and(|t| t.def.key == "extrude")
    });
    c.run(h, 4);
    h.state_mut().viewport.set_view("iso");
    c.run(h, 10);
    h.state_mut().tool_set_value("operation", json!("cut"));
    h.state_mut().tool_set_value("distance", json!(5.0));
    preview(h, link);
    c.hold(h, 1.2);
    let ok = ok_button(h);
    c.glide(h, ok, 0.6);
    c.click(h);
    if h.state().tool().is_some() {
        h.state_mut().tool_ok();
    }
    settle(h, link);
    harness::wait_for(h, link, "Extrude to close", |h| h.state().tool().is_none());
    h.state_mut().shell.selection.clear();
    h.state_mut().viewport.set_view("iso");
    c.run(h, 10);
    c.glide(h, pos2(SIZE.x * 0.75, SIZE.y * 0.8), 0.5);
    c.hold(h, 2.0);
    c
}

/// Click the bracket with the Hole tool, pick the M3 heat-set insert preset, and accept.
fn hole_insert(h: &mut H, link: &NativeLink) -> Clip {
    let mut c = Clip::new("hole-insert");
    build_bracket(h, link);
    prepare(h);
    c.hold(h, 0.8);
    c.click_button(h, "Hole");
    harness::select_first_body(h);
    if h.state().tool().is_none() {
        assert!(h.state_mut().open_tool("hole", None), "Hole opens");
    }
    // the preset first, so the preview of every hole is the insert's
    h.state_mut().tool_set_value("preset", json!("ins_M3"));
    c.run(h, 4);
    c.hold(h, 0.6);
    for p in [[28.0, 0.0, 8.0], [-30.0, 0.0, 8.0]] {
        let at = world(h, p);
        c.glide(h, at, 0.7);
        c.click(h);
        settle(h, link);
        c.run(h, 4);
    }
    preview(h, link);
    c.hold(h, 1.5);
    let ok = ok_button(h);
    c.glide(h, ok, 0.6);
    c.click(h);
    if h.state().tool().is_some() {
        h.state_mut().tool_ok();
    }
    settle(h, link);
    harness::wait_for(h, link, "Hole to close", |h| h.state().tool().is_none());
    h.state_mut().shell.selection.clear();
    c.run(h, 4);
    c.glide(h, pos2(SIZE.x * 0.75, SIZE.y * 0.8), 0.5);
    c.hold(h, 2.0);
    c
}

/// A part too long for the bed is cut into pieces that fit, with pins at the seam.
fn fit_to_bed(h: &mut H, link: &NativeLink) -> Clip {
    let mut c = Clip::new("fit-to-bed");
    import_stl(h, link, &fixture_stl("long.stl"));
    iso(h);
    prepare(h);
    c.hold(h, 0.8);
    c.click_button(h, "Print");
    h.state_mut().shell.tab = "print".into();
    c.run(h, 2);
    let body = h.state().bodies.last().unwrap().id.clone();
    h.state_mut().shell.selection = vec![body.clone()];
    c.click_button(h, "Fit to Bed");
    if h.state().tool().is_none() {
        let mut v = serde_json::Map::new();
        v.insert("body".into(), json!([body]));
        assert!(h.state_mut().open_tool("bedsplit", Some(v)), "Fit to Bed opens");
    }
    preview(h, link);
    c.hold(h, 1.5);
    let ok = ok_button(h);
    c.glide(h, ok, 0.6);
    c.click(h);
    if h.state().tool().is_some() {
        h.state_mut().tool_ok();
    }
    settle(h, link);
    harness::wait_for(h, link, "Fit to Bed to close", |h| h.state().tool().is_none());
    h.state_mut().shell.selection.clear();
    iso(h);
    c.run(h, 4);
    c.glide(h, pos2(SIZE.x * 0.75, SIZE.y * 0.8), 0.5);
    c.hold(h, 2.0);
    c
}

const GIFS: &[(&str, fn(&mut H, &NativeLink) -> Clip)] = &[
    ("sketch_cut", sketch_cut),
    ("hole_insert", hole_insert),
    ("fit_to_bed", fit_to_bed),
];

#[test]
#[ignore = "writes the website GIFs to docs/img; run with --ignored"]
fn site_gifs() {
    let gpu = Headless::new().expect("a GPU adapter for the site GIFs");
    let only = std::env::var("SITE_GIFS").unwrap_or_default();
    for (name, make) in GIFS {
        if !only.is_empty() && !only.split(',').any(|o| o.trim() == *name) {
            continue;
        }
        let (mut h, link) = site::fresh(&gpu, SIZE, true);
        make(&mut h, &link).save();
    }
}
