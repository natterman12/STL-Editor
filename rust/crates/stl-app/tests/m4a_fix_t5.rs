//! M4a fixes, Task 5: the grid and axes on the start page, the section cap, and the grid seen
//! through a body that crosses the bed.
//!
//! Start page: 1.x `app.js` calls `viewer.setView('iso')` once at startup. With an empty scene
//! that puts the camera 60 / sin(halfFov) * 1.05 from the origin along (1, -1, 0.85), so the
//! 200 mm grid runs off both sides of the view and the X, Y and Z axes show around the origin.
//!
//! Section cap: 1.x's cut face (`viewer.js` `o.back`) is the body's back faces in a flat
//! `#d06060`, with no pattern of its own. The fine "hatch" seen on it in 1.x is the floor grid:
//! three draws the (transparent) grid after the opaque bodies with a normal depth test, so it
//! shows wherever the back faces under the cut lie on the grid's plane (a part standing on the
//! bed) or below it (a part crossing the bed). A part floating above the bed shows a plain cut
//! face; with the grid off every cut face is plain.
//!
//! Grid through a body: where a part dips below the bed, the grid lies in front of it and shows
//! over it, blended at the grid's own opacity (at most 0.9 dark / 0.6 light, the major lines'),
//! never more; a part standing on the bed shows no grid.
//!
//! Native only: the GPU these tests use does not exist on wasm32.
#![cfg(not(target_arch = "wasm32"))]

use std::sync::Arc;

use egui_kittest::Harness;
use stl_app::engine_link::BodyView;
use stl_app::viewport::renderer::Headless;
use stl_app::viewport::section::SectionAxis;
use stl_app::viewport::Viewport;
use stl_kernel::MeshArrays;

#[path = "support/wgpu_renderer.rs"]
mod wgpu_renderer;
#[path = "support/gpu.rs"]
mod gpu;

/// A 20 mm wide box from `z0` to `z1`, centred on the origin in X and Y.
fn block(id: &str, z0: f32, z1: f32) -> BodyView {
    let p = [
        [-10.0, -10.0, z0],
        [10.0, -10.0, z0],
        [10.0, 10.0, z0],
        [-10.0, 10.0, z0],
        [-10.0, -10.0, z1],
        [10.0, -10.0, z1],
        [10.0, 10.0, z1],
        [-10.0, 10.0, z1],
    ];
    let t: [u32; 36] = [0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7, 0, 1, 5, 0, 5, 4, 1, 2, 6, 1, 6, 5, 2, 3, 7, 2, 7, 6, 3, 0, 4, 3, 4, 7];
    let mesh = Arc::new(MeshArrays { positions: p.iter().flatten().copied().collect(), triangles: t.to_vec() });
    BodyView { id: id.into(), name: "Block".into(), color: Some("#9aa7b4".into()), hidden: false, solid: true, mesh, revision: 1 }
}

/// Render `vp` in a `w` x `h` window through egui and the paint callback; check the snapshot
/// `name` when given.
fn picture(gpu: &Headless, mut vp: Viewport, dark: bool, w: f32, h: f32, name: Option<&str>) -> image::RgbaImage {
    let renderer = wgpu_renderer::WgpuRenderer::new(gpu.render_state.clone());
    let mut harness = Harness::builder().with_size(egui::vec2(w, h)).with_render_cursor(false).renderer(renderer).build_ui(move |ui| {
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| {
            vp.show(ui);
        });
    });
    stl_app::theme::apply(&harness.ctx, &stl_app::theme::tokens(dark));
    harness.run();
    if let Some(name) = name {
        gpu::snapshot(&mut harness, gpu, name);
    }
    let img = harness.render().expect("picture");
    let errors: Vec<String> = harness.take_snapshot_results().into_inner().iter().map(|e| e.to_string()).collect();
    assert!(errors.is_empty(), "{}", errors.join("\n"));
    gpu::assert_view_toolbar(&img, dark, gpu::top_band(&img), name.unwrap_or("view"));
    img
}

// ---------------------------------------------------------------------- start page

/// 1.x's start view: `setView('iso')` on an empty scene.
fn start_view(gpu: &Headless, dark: bool, w: f32, h: f32) -> Viewport {
    let mut vp = Viewport::new();
    vp.init_gpu(&gpu.render_state);
    vp.set_theme(dark);
    vp.set_view_size(w, h);
    vp.set_view("iso");
    vp.camera_mut().finish_flight();
    vp
}

fn is_axis(p: &image::Rgba<u8>, c: u32) -> bool {
    let [r, g, b, _] = p.0;
    let (cr, cg, cb) = ((c >> 16) as i32 & 255, (c >> 8) as i32 & 255, c as i32 & 255);
    (r as i32 - cr).abs() < 40 && (g as i32 - cg).abs() < 40 && (b as i32 - cb).abs() < 40
}

fn start_grid_and_axes(name: &str, dark: bool) {
    let Some(gpu) = gpu::headless() else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    // 1.x's viewport under the ribbon (1280 x 645) at half size, in a window with kittest's
    // 8-point margin
    let (w, h) = (656.0, 338.0);
    let vp = start_view(&gpu, dark, w - 16.0, h - 16.0);
    let (target, pos, _) = vp.camera().goal();
    let d = 60.0 / vp.camera().half_fov().sin() * 1.05;
    let n = (1.0f64 + 1.0 + 0.85 * 0.85).sqrt();
    let want = [d / n, -d / n, 0.85 * d / n];
    assert!((0..3).all(|k| (pos[k] - want[k]).abs() < 1e-6 && target[k].abs() < 1e-9), "start camera {pos:?}, want {want:?}");
    let img = picture(&gpu, vp, dark, w, h, Some(name));
    let (iw, ih) = img.dimensions();
    // the grid runs off both sides: the view's outermost columns cross grid lines
    for x in [10, iw - 11] {
        let mut colours: Vec<[u8; 4]> = (ih / 2..ih).map(|y| img.get_pixel(x, y).0).collect();
        colours.sort();
        colours.dedup();
        assert!(colours.len() > 3, "{name}: column {x} shows no grid ({} colours)", colours.len());
    }
    // the three axis lines meet at the origin, in the middle of the view; Z points up
    let count = |c: u32, above: bool| {
        img.enumerate_pixels().filter(|(_, y, p)| (!above || *y < ih / 2) && is_axis(p, c)).count()
    };
    let (x, y, z) = (count(0xe25555, false), count(0x22a35a, false), count(0x3b82f6, true));
    assert!(x >= 10 && y >= 10 && z >= 10, "{name}: axis pixels x {x}, y {y}, z above the centre {z}");
}

#[test]
fn start_grid_and_axes_dark() {
    start_grid_and_axes("start_grid_and_axes_dark", true);
}

#[test]
fn start_grid_and_axes_light() {
    start_grid_and_axes("start_grid_and_axes_light", false);
}

/// The app's start page must use that view: `app.js` calls `setView('iso')` once at startup.
/// With a GPU, the start page (1280 x 800, as the 1.x screenshots; dark and light) is also checked against
/// `start_grid_and_axes_app_<theme>.png`.
fn start_page(dark: bool) {
    let gpu = gpu::headless();
    let mut app = stl_app::app::StlApp::empty(&egui::Context::default());
    if let Some(gpu) = &gpu {
        app.viewport.init_gpu(&gpu.render_state);
    }
    let pref = if dark { stl_app::ui::ThemePref::Dark } else { stl_app::ui::ThemePref::Light };
    let mut init = false;
    let mut builder = Harness::builder().with_size(egui::vec2(1280.0, 800.0)).with_accessibility_check(false).with_render_cursor(false);
    if let Some(gpu) = &gpu {
        builder = builder.renderer(wgpu_renderer::WgpuRenderer::new(gpu.render_state.clone()));
    }
    let mut h = builder.build_ui_state(
        move |ui, app: &mut stl_app::app::StlApp| {
            if !init {
                init = true;
                stl_app::fonts::install(ui.ctx());
                app.set_theme(ui.ctx(), pref);
            }
            app.layout(ui)
        },
        app,
    );
    h.run_steps(4);
    let cam = h.state().viewport.camera();
    let (target, pos, _) = cam.goal();
    let d = 60.0 / cam.half_fov().sin() * 1.05;
    let n = (1.0f64 + 1.0 + 0.85 * 0.85).sqrt();
    let want = [d / n, -d / n, 0.85 * d / n];
    assert!((0..3).all(|k| (pos[k] - want[k]).abs() < 1e-6 && target[k].abs() < 1e-9), "start camera {pos:?}, want {want:?}");
    if let Some(gpu) = &gpu {
        let name = if dark { "start_grid_and_axes_app_dark" } else { "start_grid_and_axes_app_light" };
        gpu::snapshot(&mut h, gpu, name);
        // on any adapter: the Z axis rises from the origin in the middle of the view (the
        // start card covers the X and Y axes), and the grid fills the view below the card
        let img = h.render().expect("start page");
        let (iw, ih) = img.dimensions();
        // the view toolbar at the viewport's top right, under the ribbon
        gpu::assert_view_toolbar(&img, dark, [iw / 2, 160, iw, 210], name);
        let z = (ih * 3 / 8..ih * 9 / 20)
            .flat_map(|y| (iw / 2 - 20..iw / 2 + 20).map(move |x| (x, y)))
            .filter(|&(x, y)| {
                // the 1-pixel axis blends with the grey around it: clearly blue, not grey
                let [r, g, b, _] = img.get_pixel(x, y).0.map(i32::from);
                b > r + 60 && b > g + 30
            })
            .count();
        assert!(z >= 10, "{name}: {z} Z axis pixels above the start card");
        for x in [iw / 6, iw * 5 / 6] {
            let mut colours: Vec<[u8; 4]> = (ih * 11 / 16..ih * 29 / 32).map(|y| img.get_pixel(x, y).0).collect();
            colours.sort();
            colours.dedup();
            assert!(colours.len() > 3, "{name}: column {x} shows no grid ({} colours)", colours.len());
        }
    } else {
        eprintln!("no GPU adapter: start page snapshot skipped");
    }
}

#[test]
fn start_page_uses_iso_view() {
    start_page(true);
}

#[test]
fn start_page_uses_iso_view_light() {
    start_page(false);
}

// ---------------------------------------------------------------------- section cap

/// The grid's lines in `_buildOrigin`, sRGB colour and opacity: minor, then the major lines
/// (centre and the rest), drawn in that order.
fn grid_minor(dark: bool) -> (u32, f64) {
    if dark {
        (0x3a3f49, 0.6)
    } else {
        (0xd2d7de, 0.5)
    }
}

fn grid_major(dark: bool) -> [(u32, f64); 2] {
    if dark {
        [(0x515866, 0.9), (0x454b56, 0.9)]
    } else {
        [(0x9aa5b1, 0.6), (0xbcc3cc, 0.6)]
    }
}

/// A body, cut on Z at `cut` when given, the camera above (iso, fitted), with the grid on or off.
fn cut_view(gpu: &Headless, dark: bool, body: &BodyView, cut: Option<f64>, grid: bool) -> Viewport {
    let mut vp = Viewport::new();
    vp.init_gpu(&gpu.render_state);
    vp.set_theme(dark);
    vp.sync_bodies(std::slice::from_ref(body));
    vp.flush();
    vp.set_view_size(320.0, 240.0);
    vp.set_view("iso");
    vp.camera_mut().finish_flight();
    if let Some(cut) = cut {
        vp.set_section_cut(Some((SectionAxis::Z, cut, false)));
        vp.set_section(true);
    }
    vp.set_grid_visible(grid);
    vp
}

/// Pixels well inside the area where `inside` holds (they and their four neighbours do).
fn mask(img: &image::RgbaImage, inside: impl Fn(u32, u32) -> bool) -> Vec<(u32, u32)> {
    let (w, h) = img.dimensions();
    let mut out = Vec::new();
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            if inside(x, y) && inside(x - 1, y) && inside(x + 1, y) && inside(x, y - 1) && inside(x, y + 1) {
                out.push((x, y));
            }
        }
    }
    out
}

/// Pixels well inside the cut face (`#d06060`).
fn cap_mask(img: &image::RgbaImage) -> Vec<(u32, u32)> {
    mask(img, |x, y| {
        let [r, g, b, _] = img.get_pixel(x, y).0;
        (r as i32 - 0xd0).abs() <= 3 && (g as i32 - 0x60).abs() <= 3 && (b as i32 - 0x60).abs() <= 3
    })
}

/// What the grid does to the pixels of `mask`, from `off` (grid off) to `on` (grid on).
struct GridChange {
    /// pixels that change
    changed: usize,
    /// every change is a blend toward the grid's colours, never past them
    toward_grid: bool,
    /// the median blend toward the minor lines' colour (0 none, 1 the line's colour)
    median: f64,
}

fn grid_changes(dark: bool, mask: &[(u32, u32)], off: &image::RgbaImage, on: &image::RgbaImage) -> GridChange {
    let rgb = |c: u32| [((c >> 16) & 255) as f64, ((c >> 8) & 255) as f64, (c & 255) as f64];
    let (minor, _) = grid_minor(dark);
    let colours: Vec<[f64; 3]> = std::iter::once(minor).chain(grid_major(dark).map(|m| m.0)).map(rgb).collect();
    let mut out = GridChange { changed: 0, toward_grid: true, median: 0.0 };
    let mut ts = Vec::new();
    for &(x, y) in mask {
        let (a, b) = (on.get_pixel(x, y).0, off.get_pixel(x, y).0);
        if !(0..3).any(|k| a[k].abs_diff(b[k]) > 2) {
            continue;
        }
        out.changed += 1;
        let (a, b) = ([0, 1, 2].map(|k| a[k] as f64), [0, 1, 2].map(|k| b[k] as f64));
        // lines blend over each other where they crowd together, so any mix of the pixel and
        // the grid's colours is fine; going past them is not
        out.toward_grid &= (0..3).all(|k| {
            let lo = colours.iter().map(|c| c[k]).fold(b[k], f64::min) - 3.0;
            let hi = colours.iter().map(|c| c[k]).fold(b[k], f64::max) + 3.0;
            (lo..=hi).contains(&a[k])
        });
        let g = colours[0];
        let k = (0..3).max_by(|&i, &j| (g[i] - b[i]).abs().total_cmp(&(g[j] - b[j]).abs())).unwrap_or(0);
        if (g[k] - b[k]).abs() > 20.0 {
            ts.push((a[k] - b[k]) / (g[k] - b[k]));
        }
    }
    ts.sort_by(f64::total_cmp);
    out.median = ts.get(ts.len() / 2).copied().unwrap_or(0.0);
    out
}

/// The grid's strongest opacity (its major lines').
fn grid_opacity(dark: bool) -> f64 {
    grid_major(dark)[0].1
}

/// Cap pixels (found with the grid off) and what the grid does to them.
fn grid_over_cap(gpu: &Headless, dark: bool, body: &BodyView, cut: f64, name: Option<&str>) -> (usize, GridChange) {
    let off = picture(gpu, cut_view(gpu, dark, body, Some(cut), false), dark, 320.0, 240.0, None);
    let on = picture(gpu, cut_view(gpu, dark, body, Some(cut), true), dark, 320.0, 240.0, name);
    let mask = cap_mask(&off);
    (mask.len(), grid_changes(dark, &mask, &off, &on))
}

/// The grid shows on `n` pixels, as a blend toward its colours no stronger than its opacity.
fn assert_grid_like_1x(name: &str, what: &str, dark: bool, n: usize, c: &GridChange) {
    assert!(c.changed >= 20, "{name}: the grid shows on only {} of {n} pixels ({what})", c.changed);
    assert!(c.toward_grid, "{name}: the grid changes pixels past its own colours ({what})");
    let most = grid_opacity(dark) + 0.05;
    assert!(c.median > 0.05 && c.median <= most, "{name}: the grid's median blend is {:.2}, want 0.05..{most:.2} ({what})", c.median);
}

fn section_cap_shows_grid(name: &str, dark: bool) {
    let Some(gpu) = gpu::headless() else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    // standing on the bed: the cut face's floor (the bottom's back side) is on the grid's
    // plane, and the grid shows on it as 1.x's "hatch"
    let (cap, c) = grid_over_cap(&gpu, dark, &block("b1", 0.0, 20.0), 10.0, Some(name));
    assert!(cap >= 200, "{name}: only {cap} cap pixels on the block on the bed");
    assert_grid_like_1x(name, "block on the bed", dark, cap, &c);
    // crossing the bed: the back faces under the cut face are below the grid
    let (cap, c) = grid_over_cap(&gpu, dark, &block("b1", -10.0, 10.0), 5.0, None);
    assert!(cap >= 200, "{name}: only {cap} cap pixels on the block crossing the bed");
    assert_grid_like_1x(name, "block crossing the bed", dark, cap, &c);
    // cut below the bed: the grid is between the camera and the cut face
    let (cap, c) = grid_over_cap(&gpu, dark, &block("b1", -20.0, 10.0), -5.0, None);
    assert!(cap >= 200, "{name}: only {cap} cap pixels below the bed");
    assert_grid_like_1x(name, "cut below the bed", dark, cap, &c);
    // floating above the bed: the grid is behind the part, and the cut face is plain
    let (cap, c) = grid_over_cap(&gpu, dark, &block("b1", 5.0, 25.0), 15.0, None);
    assert!(cap >= 200, "{name}: only {cap} cap pixels above the bed");
    assert_eq!(c.changed, 0, "{name}: the grid behind a part above the bed shows on {} of {cap} cap pixels", c.changed);
}

#[test]
fn section_cap_shows_grid_dark() {
    section_cap_shows_grid("section_cap_shows_grid_dark", true);
}

#[test]
fn section_cap_shows_grid_light() {
    section_cap_shows_grid("section_cap_shows_grid_light", false);
}

// ---------------------------------------------------------------------- grid through a body

/// A tan body (two_blobs' colour family) from `z0` to `z1`, without a section: its pixels
/// (tan with the grid off: red over green over blue, unlike the grey background and the axes;
/// not on its edge lines, which blend over the grid) and what the grid does to them.
fn grid_through_body(gpu: &Headless, dark: bool, z0: f32, z1: f32, name: Option<&str>) -> (usize, GridChange) {
    let body = BodyView { color: Some("#e0b070".into()), ..block("b1", z0, z1) };
    let off = picture(gpu, cut_view(gpu, dark, &body, None, false), dark, 320.0, 240.0, None);
    let on = picture(gpu, cut_view(gpu, dark, &body, None, true), dark, 320.0, 240.0, name);
    let (w, h) = off.dimensions();
    let px = |x: u32, y: u32| off.get_pixel(x.min(w - 1), y.min(h - 1)).0.map(i32::from);
    let mask = mask(&off, |x, y| {
        let [r, g, b, _] = px(x, y);
        let smooth = |q: [i32; 4]| (0..3).all(|k| (q[k] - px(x, y)[k]).abs() <= 12);
        r > b + 40 && g > b + 20 && smooth(px(x + 1, y)) && smooth(px(x, y + 1))
    });
    (mask.len(), grid_changes(dark, &mask, &off, &on))
}

fn grid_through_body_case(name: &str, dark: bool) {
    let Some(gpu) = gpu::headless() else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    // standing on the bed: the grid is under the part and does not show on it (a pixel or two
    // may blend along the bottom edge, where the part meets the bed)
    let (body, c) = grid_through_body(&gpu, dark, 0.0, 20.0, None);
    assert!(body >= 500, "{name}: only {body} body pixels on the bed");
    assert!(c.changed * 1000 < body, "{name}: the grid shows through {} of {body} pixels of a part on the bed", c.changed);
    // crossing the bed: the grid shows over the part below the bed, at its own opacity only
    let (body, c) = grid_through_body(&gpu, dark, -10.0, 10.0, Some(name));
    assert!(body >= 500, "{name}: only {body} body pixels crossing the bed");
    assert!(c.changed * 2 < body, "{name}: the grid shows over {} of {body} pixels, more than the part below the bed", c.changed);
    assert_grid_like_1x(name, "part crossing the bed", dark, body, &c);
}

#[test]
fn grid_through_body_dark() {
    grid_through_body_case("grid_through_body_dark", true);
}

#[test]
fn grid_through_body_light() {
    grid_through_body_case("grid_through_body_light", false);
}
