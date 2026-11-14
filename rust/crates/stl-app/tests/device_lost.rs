//! Milestone 5, Task 6: the graphics device is lost (a driver reset, a GPU unplugged). The
//! viewport makes a new device itself, uploads the scene again, and the project is untouched.
//!
//! Its own test binary with one GPU test, so it owns a [`Headless`] directly (no turn-taking).
#![cfg(not(target_arch = "wasm32"))]

use stl_app::app::StlApp;
use stl_app::viewport::renderer::Headless;

#[path = "ui/harness.rs"]
mod harness;

/// What the project is, as the app holds it: bodies (id, revision, mesh size) and the timeline.
fn project_state(app: &StlApp) -> (Vec<(String, u64, usize, usize)>, String, String) {
    let bodies = app
        .bodies
        .iter()
        .map(|b| (b.id.clone(), b.revision, b.mesh.positions.len(), b.mesh.triangles.len()))
        .collect();
    (bodies, format!("{:?}", app.timeline_view), format!("{:?}", app.param_views))
}

#[test]
fn device_lost_recovers() {
    // never Nathan's real folders: settings and autosave go to a temp folder
    let cfg = std::env::temp_dir().join(format!("stle2-device-lost-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&cfg);
    std::fs::create_dir_all(&cfg).unwrap();
    // SAFETY: the only test in this binary, set before any thread starts
    unsafe { std::env::set_var("STLEDITOR2_CONFIG_DIR", &cfg) };

    // the viewport first: the project is open and every body is on screen
    let (mut h, link) = harness::app_harness_ex(Some(&harness::project("basic.stlx")), egui::vec2(1280.0, 800.0));
    harness::settle(&mut h, &link);
    assert!(!h.state().bodies.is_empty(), "basic.stlx opened no body");
    let before_project = project_state(h.state());

    let Some(mut gpu) = Headless::new() else {
        // loud on purpose: the verify run looks for this line
        println!("SKIPPED: no GPU adapter (device_lost_recovers did not run)");
        eprintln!("SKIPPED: no GPU adapter (device_lost_recovers did not run)");
        return;
    };
    let (w, hgt) = (160u32, 120u32);
    let picture = {
        let vp = &mut h.state_mut().viewport;
        vp.init_gpu(&gpu.render_state);
        vp.set_view_size(w as f32, hgt as f32);
        vp.fit_all();
        vp.camera_mut().finish_flight();
        let mut pic = None;
        // a large body may take a few frames to copy
        for _ in 0..50 {
            pic = gpu.render(vp, w, hgt);
            if !vp.stats().upload_pending.load(std::sync::atomic::Ordering::Relaxed) {
                break;
            }
        }
        pic.expect("a picture before the loss")
    };
    let vp = &mut h.state_mut().viewport;
    let uploads = vp.stats().uploads();
    assert!(uploads >= 1);
    assert_eq!(vp.stats().gpu_states(), 1);

    // the device goes away: nothing is drawn on it any more, and nothing panics
    gpu.force_device_loss();
    for _ in 0..3 {
        assert!(gpu.render(vp, w, hgt).is_none(), "drew on a lost device");
    }
    assert!(vp.stats().device_lost());
    assert!(gpu.is_lost());

    // the viewport makes a new device; the next frames draw the scene again from the kept meshes
    assert!(gpu.recover(vp), "no new device after the loss");
    assert!(!gpu.is_lost());
    let mut after = None;
    for _ in 0..50 {
        after = gpu.render(vp, w, hgt);
        if !vp.stats().upload_pending.load(std::sync::atomic::Ordering::Relaxed) {
            break;
        }
    }
    let after = after.expect("a picture on the new device");
    assert!(!vp.stats().device_lost());
    assert_eq!(vp.stats().gpu_states(), 2);
    assert_eq!(vp.stats().uploads(), uploads * 2, "every body uploaded again, once");
    assert_eq!(gpu.bodies_on_gpu() as u64, uploads);
    assert_eq!(picture, after, "the same scene on the new device");
    // a second recover with a healthy device does nothing
    assert!(!gpu.recover(vp));

    // the project is unchanged, and the app keeps running on it
    h.run_steps(3);
    harness::settle(&mut h, &link);
    assert_eq!(project_state(h.state()), before_project);
    assert!(!h.state().engine_lost);
    drop(h);
    let _ = std::fs::remove_dir_all(&cfg);
}
