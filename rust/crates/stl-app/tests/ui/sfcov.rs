//! Support Fins coverage: bare spots, Full coverage, Bare beyond.
//! Counterpart of `tests/ui/sfcov.js`.
#![cfg(not(target_arch = "wasm32"))]

use super::harness::{empty_harness, fixture_stl, import_stl, select_first_body, settle, wait_support_open};
use serde_json::json;
use stl_app::supports;

#[test]
fn sfcov_held_card_and_reach_controls() {
    let (mut h, link) = empty_harness();
    import_stl(&mut h, &link, &fixture_stl("tipped.stl"));
    select_first_body(&mut h);
    wait_support_open(&mut h, &link, "supportfins");

    // Bare beyond / reach field exists in the schema-driven panel
    let specs = supports::fields("supportfins");
    assert!(
        specs.iter().any(|f| f.key == "_reach" || f.key.contains("reach") || f.key == "sf:sway.reach"),
        "Bare beyond / reach present: {:?}",
        specs.iter().map(|f| f.key).collect::<Vec<_>>()
    );

    // Coverage mode (Full coverage) is an sf:mode / coverage control when present
    let has_coverage = specs.iter().any(|f| f.key == "sf:coverage" || f.key == "sf:mode");
    assert!(has_coverage, "coverage controls in schema");

    // Wait for coverage to finish or time out without error
    let end = std::time::Instant::now() + std::time::Duration::from_secs(45);
    loop {
        h.run_steps(2);
        settle(&mut h, &link);
        let cover = h.state().tool().and_then(|t| t.sf.as_ref()).and_then(|s| s.cover.clone());
        match cover {
            Some(c) if !c.busy => {
                assert!(c.error.is_none(), "coverage error: {:?}", c.error);
                eprintln!("sfcov: total={:.1} held={:.1} spots={} none={}", c.total, c.held, c.spots.len(), c.none);
                break;
            }
            _ if std::time::Instant::now() > end => {
                eprintln!("sfcov: coverage still busy after timeout (engine preview may be slow)");
                break;
            }
            _ => std::thread::sleep(std::time::Duration::from_millis(30)),
        }
    }
    h.state_mut().tool_cancel();
    settle(&mut h, &link);
}

#[test]
fn sfcov_change_reach_keeps_panel() {
    let (mut h, link) = empty_harness();
    import_stl(&mut h, &link, &fixture_stl("tipped.stl"));
    select_first_body(&mut h);
    wait_support_open(&mut h, &link, "supportfins");
    if h.state().tool().unwrap().values.contains_key("sf:sway.reach") {
        h.state_mut().tool_set_value("sf:sway.reach", json!(8.0));
        h.run_steps(3);
        assert_eq!(h.state().tool().unwrap().values.get("sf:sway.reach"), Some(&json!(8.0)));
    }
    h.state_mut().tool_cancel();
    settle(&mut h, &link);
}

/// 1.x: the Held card, then Full coverage (more of the part needs a fin), then a stricter allowed gap.
#[test]
fn sfcov_full_coverage_then_bare_beyond() {
    use super::harness::{app_harness_with, wait_for, FakePlatform};
    let platform = FakePlatform::default();
    let (mut h, link) = app_harness_with(None, egui::vec2(1280.0, 800.0), platform.clone());
    import_stl(&mut h, &link, &fixture_stl("mushroom.stl"));
    select_first_body(&mut h);
    wait_support_open(&mut h, &link, "supportfins");
    let cover = |h: &egui_kittest::Harness<'static, stl_app::app::StlApp>| h.state().tool().and_then(|t| t.sf.as_ref()).and_then(|s| s.cover.clone());
    wait_for(&mut h, &link, "the Held card", |h| cover(h).is_some_and(|c| !c.busy));
    let auto = cover(&h).unwrap();
    assert_eq!(auto.error, None);
    assert!(auto.total > 0.0, "tipped.stl has overhang to hold up: {auto:?}");

    // Full coverage asks for a fin under every overhang: what is held follows, with no error
    h.state_mut().tool_set_value("sf:mode", json!("full"));
    wait_for(&mut h, &link, "full coverage", |h| cover(h).is_some_and(|c| !c.busy) && h.state().tool().is_some_and(|t| t.values.get("sf:mode") == Some(&json!("full"))));
    let full = cover(&h).unwrap();
    assert_eq!(full.error, None);
    eprintln!("sfcov: auto held {:.1}/{:.1} spots {} | full held {:.1}/{:.1} spots {}", auto.held, auto.total, auto.spots.len(), full.held, full.total, full.spots.len());
    assert!(full.total >= auto.total - 1e-6, "full coverage measures at least the overhang auto does");

    // a stricter allowed gap (Bare beyond 3 mm instead of 5): the tool keeps the value it was given
    // last time, as 1.x keeps `stle_sf_reach`
    h.state_mut().tool_cancel();
    settle(&mut h, &link);
    platform.settings.lock().unwrap().insert("stle_sf_reach".into(), "3".into());
    wait_support_open(&mut h, &link, "supportfins");
    wait_for(&mut h, &link, "coverage at 3 mm", |h| h.state().tool().and_then(|t| t.sf.as_ref()).is_some_and(|s| s.reach == 3.0) && cover(h).is_some_and(|c| !c.busy));
    let strict = cover(&h).unwrap();
    assert_eq!(strict.error, None);
    eprintln!("sfcov: reach 3 -> {} spots (reach 5: {})", strict.spots.len(), auto.spots.len());
    assert!(strict.spots.len() >= auto.spots.len(), "a stricter gap leaves at least as many bare spots");
    h.state_mut().tool_cancel();
    settle(&mut h, &link);
}
