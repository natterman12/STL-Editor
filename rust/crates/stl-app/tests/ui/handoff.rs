//! UI test `handoff` (milestone 6): Send to Slicer and To STEP, on a platform that records program
//! launches instead of starting them (no real slicer runs here).

use egui::Pos2;
use egui_kittest::kittest::{NodeT, Queryable};

use super::harness::{RecordPlatform, app_harness_on, project, settle, wait_for};

fn launches(h: &egui_kittest::Harness<'static, stl_app::app::StlApp>) -> Vec<String> {
    h.state()
        .platform
        .launch_log()
        .into_iter()
        .filter(|l| l.starts_with("launch "))
        .collect()
}

/// Send to Slicer › Bambu Studio exports the model and opens one 3MF in Bambu Studio.
#[test]
fn send_to_bambu_studio_records_one_3mf_launch() {
    let platform = RecordPlatform::new(&[("bambu", "Bambu Studio")], false);
    let (mut h, link) = app_harness_on(
        Some(project("basic.stlx").as_path()),
        egui::vec2(1280.0, 800.0),
        platform,
    );

    h.state_mut().on_ribbon("send", Pos2::new(300.0, 120.0));
    h.run_steps(3);
    assert!(
        h.query_by_label("Bambu Studio").is_some(),
        "the Send to Slicer menu lists Bambu Studio"
    );
    h.get_by_label("Bambu Studio").click();
    h.run_steps(2);
    wait_for(&mut h, &link, "the launch", |h| !launches(h).is_empty());
    settle(&mut h, &link);

    let found = launches(&h);
    assert_eq!(found.len(), 1, "exactly one launch: {found:?}");
    assert!(found[0].starts_with("launch bambu "), "{found:?}");
    assert!(
        found[0].ends_with(".3mf"),
        "the model goes to the slicer as a 3MF: {found:?}"
    );
    assert!(
        !h.state().platform.launch_log().iter().any(|l| l.starts_with("save ")),
        "no save dialog for a send"
    );
}

/// The Send to Slicer menu says so when no slicer is installed (nothing to pick).
#[test]
fn send_menu_says_when_no_slicer_is_found() {
    let platform = RecordPlatform::new(&[], false);
    let (mut h, _link) = app_harness_on(None, egui::vec2(1280.0, 800.0), platform);

    h.state_mut().on_ribbon("send", Pos2::new(300.0, 120.0));
    h.run_steps(3);
    assert!(
        h.query_by_label("No slicer found: install Bambu Studio or OrcaSlicer")
            .is_some()
    );
    assert!(launches(&h).is_empty());
}

/// To STEP without SolidCraft's command-line tool: on Windows the button shows and cannot be
/// pressed; elsewhere it is left out of the ribbon (1.x `deskToolShown`).
#[test]
fn to_step_is_disabled_without_solidcraft_cli() {
    let platform = RecordPlatform::new(&[], false);
    let (mut h, link) = app_harness_on(
        Some(project("basic.stlx").as_path()),
        egui::vec2(1280.0, 800.0),
        platform,
    );
    // To STEP sits on the Mesh tab
    h.state_mut().shell.tab = "mesh".to_owned();
    settle(&mut h, &link);
    h.run_steps(3);

    if cfg!(windows) {
        let disabled = h.get_by_label("To STEP").accesskit_node().is_disabled();
        assert!(disabled, "To STEP is greyed out without SolidCraft");
        h.get_by_label("To STEP").click();
        h.run_steps(2);
        assert!(
            h.state().platform.launch_log().is_empty(),
            "a disabled To STEP starts nothing"
        );
    } else {
        assert!(h.query_by_label("To STEP").is_none(), "To STEP is hidden off Windows");
    }
}
