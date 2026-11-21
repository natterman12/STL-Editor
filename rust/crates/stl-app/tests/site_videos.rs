//! The website's clips of the 2.0 tools (`docs/img/<name>.mp4`, with a still of the last frame as
//! `<name>.jpg` for the poster): short loops of the real app, drawn frame by frame on the GPU with
//! the mouse pointer showing, in the dark theme. The 2.0 app's own counterpart to a screen
//! recording, so they can be made again whenever the interface changes.
//!
//! Ignored by default: it needs a GPU, `ffmpeg` (with libx264) on the PATH, and writes into
//! `docs/img`. Run with `cargo test -p stl-app --profile test-fast --test site_videos -- --ignored`.
//! To make only some, list them in `SITE_VIDEOS` (`sketch_cut`, `hole_insert`, `fit_to_bed`,
//! `support_fins`, `section_view`, `timeline_edit`, comma-separated). The window is drawn at
//! `SITE_SCALE` pixels per point (2 unless set) and the clip scaled to [`VIDEO_WIDTH`]. Lavapipe
//! works but draws the ground grid badly, so the grid is hidden unless `SITE_SHOTS_GRID=1`
//! (set it on a real graphics card).
#![cfg(not(target_arch = "wasm32"))]

use std::io::Write;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

use egui::{Event, PointerButton, Pos2, pos2};
use egui_kittest::kittest::Queryable;
use serde_json::json;
use stl_app::engine_link::native::NativeLink;
use stl_app::sketch::Entity;
use stl_app::viewport::plane_drag;
use stl_app::viewport::renderer::Headless;

#[path = "support/wgpu_renderer.rs"]
mod wgpu_renderer;

#[path = "ui/harness.rs"]
mod harness;

#[path = "support/site.rs"]
mod site;

use harness::{fixture_stl, import_stl, repo, settle};
use site::{H, build_bracket, crop_to_window, img_dir, iso, wait_meshes};

/// The window the clips are drawn in (points); the video is scaled to [`VIDEO_WIDTH`] pixels.
const SIZE: egui::Vec2 = egui::vec2(1280.0, 720.0);
const VIDEO_WIDTH: u32 = 1920;
const FPS: u32 = 30;

/// One clip being recorded: frames go straight to `ffmpeg` as they are drawn.
struct Clip {
    name: &'static str,
    ffmpeg: Option<Child>,
    size: (u32, u32),
    last: Option<image::RgbaImage>,
    frames: usize,
    pointer: Pos2,
}

impl Clip {
    fn new(name: &'static str) -> Self {
        Clip {
            name,
            ffmpeg: None,
            size: (0, 0),
            last: None,
            frames: 0,
            pointer: pos2(SIZE.x * 0.75, SIZE.y * 0.8),
        }
    }

    fn out(&self, ext: &str) -> PathBuf {
        img_dir().join(format!("{}.{ext}", self.name))
    }

    /// Send `img` to the encoder, started on the first frame (when the size is known).
    fn push(&mut self, img: &image::RgbaImage) {
        if self.ffmpeg.is_none() {
            self.size = img.dimensions();
            let child = Command::new("ffmpeg")
                .args(["-y", "-loglevel", "error", "-f", "rawvideo", "-pix_fmt", "rgba"])
                .args(["-s", &format!("{}x{}", self.size.0, self.size.1)])
                .args(["-framerate", &FPS.to_string(), "-i", "-"])
                .args(["-vf", &format!("scale={VIDEO_WIDTH}:-2:flags=lanczos")])
                .args([
                    "-c:v", "libx264", "-preset", "slow", "-crf", "20", "-pix_fmt", "yuv420p",
                ])
                .args(["-movflags", "+faststart", "-an"])
                .arg(self.out("mp4"))
                .stdin(Stdio::piped())
                .spawn()
                .expect("ffmpeg is on the PATH");
            self.ffmpeg = Some(child);
        }
        assert_eq!(
            img.dimensions(),
            self.size,
            "{}: every frame is the same size",
            self.name
        );
        let stdin = self.ffmpeg.as_mut().unwrap().stdin.as_mut().unwrap();
        stdin.write_all(img.as_raw()).expect("ffmpeg takes the frame");
        self.frames += 1;
    }

    /// Draw the window as it is now and keep it as the next frame.
    fn frame(&mut self, h: &mut H) {
        h.state_mut().viewport.flush();
        h.step();
        let img = crop_to_window(&h.render().unwrap_or_else(|e| panic!("render {}: {e}", self.name)));
        self.push(&img);
        self.last = Some(img);
    }

    /// Keep the window as it is for `seconds`.
    fn hold(&mut self, h: &mut H, seconds: f32) {
        self.frame(h);
        let last = self.last.take().unwrap();
        for _ in 1..((seconds * FPS as f32).round() as usize).max(1) {
            self.push(&last);
        }
        self.last = Some(last);
    }

    /// `frames` frames while the app animates (a camera flight, a preview coming back).
    fn run(&mut self, h: &mut H, frames: usize) {
        for _ in 0..frames {
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
            h.event(Event::PointerMoved(from + (to - from) * e));
            self.frame(h);
        }
        self.pointer = to;
    }

    fn button(&mut self, h: &mut H, pressed: bool) {
        h.event(Event::PointerButton {
            pos: self.pointer,
            button: PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        });
        h.step();
    }

    /// Press and release the left button where the pointer is.
    fn click(&mut self, h: &mut H) {
        self.button(h, true);
        self.button(h, false);
        self.frame(h);
    }

    fn double_click(&mut self, h: &mut H) {
        for _ in 0..2 {
            self.button(h, true);
            self.button(h, false);
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

    /// Back to the resting spot, and the result held a moment.
    fn rest(&mut self, h: &mut H) {
        self.glide(h, pos2(SIZE.x * 0.75, SIZE.y * 0.8), 0.5);
        self.hold(h, 2.0);
    }

    /// Finish the video, and save the last frame as its poster (`<name>.jpg`).
    fn save(mut self) {
        let mut child = self.ffmpeg.take().expect("the clip has frames");
        drop(child.stdin.take());
        assert!(
            child.wait().unwrap().success(),
            "ffmpeg made {}",
            self.out("mp4").display()
        );
        let tmp = repo().join("dev/out/site-videos");
        std::fs::create_dir_all(&tmp).unwrap();
        let png = tmp.join(format!("{}-poster.png", self.name));
        self.last.as_ref().unwrap().save(&png).unwrap();
        let status = Command::new("ffmpeg")
            .args(["-y", "-loglevel", "error", "-i"])
            .arg(&png)
            .args(["-vf", &format!("scale={VIDEO_WIDTH}:-2:flags=lanczos"), "-q:v", "3"])
            .arg(self.out("jpg"))
            .status()
            .expect("ffmpeg is on the PATH");
        assert!(status.success(), "ffmpeg made the poster of {}", self.name);
        let kb = |e| std::fs::metadata(self.out(e)).map(|m| m.len() / 1024).unwrap_or(0);
        eprintln!(
            "wrote {} ({} frames, {:.1} s, {} KB; poster {} KB)",
            self.out("mp4").display(),
            self.frames,
            self.frames as f32 / FPS as f32,
            kb("mp4"),
            kb("jpg")
        );
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
    c.run(h, 24);
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
    c.run(h, 8);
    h.state_mut().viewport.set_view("iso");
    c.run(h, 24);
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
    c.run(h, 24);
    c.rest(h);
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
    c.run(h, 8);
    c.hold(h, 0.6);
    for p in [[28.0, 0.0, 8.0], [-30.0, 0.0, 8.0]] {
        let at = world(h, p);
        c.glide(h, at, 0.7);
        c.click(h);
        settle(h, link);
        c.run(h, 8);
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
    c.run(h, 8);
    c.rest(h);
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
    c.run(h, 6);
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
    c.run(h, 8);
    c.rest(h);
    c
}

/// Support Fins on the mushroom: the fins grow under the cap while the tool works them out.
fn support_fins(h: &mut H, link: &NativeLink) -> Clip {
    let mut c = Clip::new("support-fins");
    import_stl(h, link, &fixture_stl("mushroom.stl"));
    iso(h);
    prepare(h);
    c.hold(h, 0.8);
    c.click_button(h, "Supports");
    h.state_mut().shell.tab = "supports".into();
    c.run(h, 6);
    harness::select_first_body(h);
    c.click_button(h, "Support Fins");
    harness::wait_support_open(h, link, "supportfins");
    // the Rotate mode's rings would cover the fins: show the Add fins mode
    h.state_mut().tool_set_value("_mode", json!("add"));
    let end = std::time::Instant::now() + std::time::Duration::from_secs(120);
    while std::time::Instant::now() < end {
        settle(h, link);
        c.frame(h);
        let done = h
            .state()
            .tool()
            .and_then(|t| t.sf.as_ref())
            .is_some_and(|s| s.cover.as_ref().is_some_and(|c| !c.busy) && s.plastic.is_some());
        if done {
            break;
        }
    }
    c.hold(h, 1.5);
    let ok = ok_button(h);
    c.glide(h, ok, 0.6);
    c.click(h);
    if h.state().tool().is_some() {
        h.state_mut().tool_ok();
    }
    settle(h, link);
    harness::wait_for(h, link, "Support Fins to close", |h| h.state().tool().is_none());
    h.state_mut().shell.selection.clear();
    c.run(h, 8);
    c.rest(h);
    c
}

/// Section View on the bracket: the cut starts halfway up, then its arrow is dragged down
/// through the counterbored holes.
fn section_view(h: &mut H, link: &NativeLink) -> Clip {
    let mut c = Clip::new("section-view");
    build_bracket(h, link);
    prepare(h);
    c.hold(h, 0.8);
    c.click_button(h, "Section");
    if !h.state().section_open() {
        h.state_mut().open_section();
    }
    c.run(h, 8);
    c.hold(h, 0.8);
    let p = *h.state().viewport.drag_plane().expect("the drag control");
    let (cam, rect) = (h.state().viewport.camera().clone(), h.state().viewport.rect());
    let base = cam.world_to_view(plane_drag::center(&p));
    let tip = plane_drag::handle_at(&cam, &p);
    let at = |t: f64| {
        pos2(
            rect.min.x + (base[0] + (tip[0] - base[0]) * t) as f32,
            rect.min.y + (base[1] + (tip[1] - base[1]) * t) as f32,
        )
    };
    // grab the arrow and drag it down (along -Z) to just above the plate, then back up a little
    c.glide(h, at(0.6), 0.8);
    c.button(h, true);
    let len = ((tip[0] - base[0]).powi(2) + (tip[1] - base[1]).powi(2)).sqrt();
    let mm = len / plane_drag::arrow_len(&cam, &p);
    let to = |dz: f64| at(0.6 + dz * mm / len);
    c.glide(h, to(-9.0), 1.6);
    c.glide(h, to(-4.0), 0.8);
    c.button(h, false);
    c.hold(h, 0.6);
    c.rest(h);
    c
}

/// Change a step in the timeline: the round boss's step is opened, made wider and lower, and
/// every later step (its hole, the counterbores) is worked out again.
fn timeline_edit(h: &mut H, link: &NativeLink) -> Clip {
    let mut c = Clip::new("timeline-edit");
    build_bracket(h, link);
    prepare(h);
    c.hold(h, 0.8);
    let step = h.state().timeline_view.items[1].clone();
    let chip = h
        .query_all_by_label(&step.label)
        .map(|n| n.rect().center())
        .find(|p| p.y > SIZE.y - 120.0)
        .unwrap_or_else(|| panic!("no timeline step {}", step.label));
    c.glide(h, chip, 0.9);
    c.double_click(h);
    if h.state().tool().is_none() {
        assert!(h.state_mut().edit_feature(&step.id), "the step opens");
    }
    c.run(h, 8);
    c.hold(h, 0.8);
    for (key, v) in [("radius", 18.0), ("height", 15.0)] {
        h.state_mut().tool_set_value(key, json!(v));
        preview(h, link);
        c.hold(h, 0.9);
    }
    let ok = ok_button(h);
    c.glide(h, ok, 0.6);
    c.click(h);
    if h.state().tool().is_some() {
        h.state_mut().tool_ok();
    }
    settle(h, link);
    harness::wait_for(h, link, "the step to close", |h| h.state().tool().is_none());
    h.state_mut().shell.selection.clear();
    c.run(h, 8);
    c.rest(h);
    c
}

const CLIPS: &[(&str, fn(&mut H, &NativeLink) -> Clip)] = &[
    ("sketch_cut", sketch_cut),
    ("hole_insert", hole_insert),
    ("fit_to_bed", fit_to_bed),
    ("support_fins", support_fins),
    ("section_view", section_view),
    ("timeline_edit", timeline_edit),
];

#[test]
#[ignore = "writes the website clips to docs/img; run with --ignored"]
fn site_videos() {
    let gpu = Headless::new().expect("a GPU adapter for the site clips");
    let only = std::env::var("SITE_VIDEOS").unwrap_or_default();
    for (name, make) in CLIPS {
        if !only.is_empty() && !only.split(',').any(|o| o.trim() == *name) {
            continue;
        }
        let (mut h, link) = site::fresh(&gpu, SIZE, true, Some(1.0 / FPS as f32));
        make(&mut h, &link).save();
    }
}
