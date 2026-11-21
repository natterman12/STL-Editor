//! The support check under the Support Fins result: the tilt hint and its "Turn it that way"
//! button, and the flagged spots outlined in the view (`supports/check.rs`, `app_sfcheck.rs`).
#![cfg(not(target_arch = "wasm32"))]

use super::harness::{app_harness_ex, import_stl, select_first_body, settle, wait_for, wait_support_open};
use egui_kittest::kittest::Queryable;
use stl_app::supports::check::{CheckReady, CheckUi, Finding};

/// A binary STL of boxes, written to a temporary file.
fn boxes_stl(name: &str, list: &[[f32; 6]]) -> std::path::PathBuf {
    let mut tris: Vec<[[f32; 3]; 3]> = Vec::new();
    for b in list {
        let p = |i: usize| {
            [
                if i & 1 == 0 { b[0] } else { b[3] },
                if i & 2 == 0 { b[1] } else { b[4] },
                if i & 4 == 0 { b[2] } else { b[5] },
            ]
        };
        for f in [
            [0, 2, 1],
            [1, 2, 3],
            [4, 5, 6],
            [5, 7, 6],
            [0, 1, 4],
            [1, 5, 4],
            [2, 6, 3],
            [3, 6, 7],
            [0, 4, 2],
            [2, 4, 6],
            [1, 3, 5],
            [3, 7, 5],
        ] {
            tris.push([p(f[0]), p(f[1]), p(f[2])]);
        }
    }
    let mut out = vec![0u8; 80];
    out.extend((tris.len() as u32).to_le_bytes());
    for t in &tris {
        out.extend([0u8; 12]);
        for v in t {
            for c in v {
                out.extend(c.to_le_bytes());
            }
        }
        out.extend([0u8; 2]);
    }
    let dir = std::env::temp_dir().join(format!("stle-sfcheck-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, out).unwrap();
    path
}

fn check_of(h: &egui_kittest::Harness<'static, stl_app::app::StlApp>) -> CheckUi {
    h.state()
        .tool()
        .and_then(|t| t.sf.as_ref())
        .map(|s| s.check.clone())
        .unwrap_or_default()
}

#[test]
fn sfcheck_tilt_hint_turns_the_part() {
    let (mut h, link) = app_harness_ex(None, egui::vec2(1280.0, 1500.0));
    // a block with a thin wall on it: its overhangs are too shallow for a fin this way up
    let stl = boxes_stl(
        "ledge.stl",
        &[[0.0, 0.0, 0.0, 40.0, 40.0, 20.0], [0.0, 0.0, 20.0, 40.0, 1.2, 40.0]],
    );
    import_stl(&mut h, &link, &stl);
    select_first_body(&mut h);
    wait_support_open(&mut h, &link, "supportfins");
    wait_for(&mut h, &link, "the support check", |h| {
        matches!(check_of(h), CheckUi::Ready(_))
    });
    let CheckUi::Ready(r) = check_of(&h) else {
        unreachable!()
    };
    assert_eq!(r.unserved, 1);
    assert_eq!(
        r.tilt.as_ref().map(|t| (t.phrase.as_str(), t.left)),
        Some(("laying it on its front side (-Y)", 0))
    );
    h.run_steps(3);
    assert!(
        h.query_by_label_contains("Laying it on its front side (-Y) leaves none of them.")
            .is_some(),
        "the tilt hint is shown"
    );
    let before = h.state().timeline_view.items.len();
    h.get_by_label("Turn it that way").click();
    wait_for(&mut h, &link, "the turn", |h| {
        h.state().timeline_view.items.len() > before
    });
    let turn = h.state().timeline_view.items.iter().find(|i| i.kind == "move").cloned();
    assert_eq!(
        turn.map(|t| t.label),
        Some("Rotate".to_owned()),
        "the button adds a Rotate step"
    );
    // laid on its front, the thin wall is flat on the bed: nothing too shallow, so no hint
    wait_for(
        &mut h,
        &link,
        "the check of the new pose",
        |h| matches!(check_of(h), CheckUi::Ready(r) if r.unserved == 0),
    );
    h.run_steps(3);
    assert!(h.query_by_label("Turn it that way").is_none());
    h.state_mut().tool_cancel();
    settle(&mut h, &link);
}

#[test]
fn sfcheck_findings_are_outlined_one_at_a_time() {
    let (mut h, link) = app_harness_ex(None, egui::vec2(1280.0, 1500.0));
    let stl = boxes_stl(
        "tee.stl",
        &[
            [-2.0, -2.0, 0.0, 2.0, 2.0, 30.0],
            [-30.0, -10.0, 30.0, 30.0, 10.0, 34.0],
        ],
    );
    import_stl(&mut h, &link, &stl);
    select_first_body(&mut h);
    wait_support_open(&mut h, &link, "supportfins");
    wait_for(&mut h, &link, "the support check", |h| {
        matches!(check_of(h), CheckUi::Ready(_))
    });
    // none of our test parts gets a finding from this engine, so two are put in by hand
    let spots = vec![
        Finding {
            kind: "narrow".into(),
            z: 0.4,
            bx: [-4.0, -0.25, 0.2, 4.0, 0.25, 0.4],
        },
        Finding {
            kind: "start".into(),
            z: 12.2,
            bx: [0.0, 0.0, 12.0, 1.0, 1.0, 12.2],
        },
    ];
    let s = h.state_mut().tool_mut().unwrap().sf.as_mut().unwrap();
    s.check = CheckUi::Ready(CheckReady {
        status: "complete".into(),
        findings: spots,
        unserved: 0,
        tilt: None,
    });
    h.run_steps(3);
    assert!(
        h.query_by_label_contains("2 support spots may not print well")
            .is_some()
    );
    let outline = |h: &egui_kittest::Harness<'static, stl_app::app::StlApp>| {
        h.state()
            .viewport
            .overlay_items()
            .iter()
            .filter(|o| o.tag == "sf-finding")
            .map(|o| o.lines.len())
            .sum::<usize>()
    };
    assert_eq!(outline(&h), 0);
    h.get_by_label("Show").click();
    h.run_steps(3);
    assert_eq!(outline(&h), 24, "one box: twelve edges");
    assert!(h.query_by_label("Spot 1 of 2: too thin, 0.4 mm up.").is_some());
    h.get_by_label("Next").click();
    h.run_steps(3);
    assert_eq!(outline(&h), 24, "still one box");
    assert!(
        h.query_by_label("Spot 2 of 2: starts in mid-air, 12.2 mm up.")
            .is_some()
    );
    h.get_by_label("Hide").click();
    h.run_steps(3);
    assert_eq!(outline(&h), 0);
    h.state_mut().tool_cancel();
    settle(&mut h, &link);
}
