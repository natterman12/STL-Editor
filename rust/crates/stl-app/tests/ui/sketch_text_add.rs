//! UI test `sketch_text_add`: the sketch's Text tool puts the letters in the sketch. Pick Text,
//! click the sketch, type in the dialog and press Add text: the engine's outlines of the letters
//! come back and must land in the sketch (they used to be dropped, because the dialog had already
//! switched to the Select tool, which ignored the answer).
#![cfg(not(target_arch = "wasm32"))]

use egui_kittest::kittest::Queryable;
use serde_json::json;
use stl_sketch::Entity;

use super::harness::{click_at, empty_harness, enter_sketch_plane, settle, sketch_click_uv};

#[test]
fn add_text_puts_the_letters_in_the_sketch() {
    let (mut h, link) = empty_harness();
    enter_sketch_plane(
        &mut h,
        &link,
        json!({"origin": [0.0, 0.0, 0.0], "normal": [0.0, 0.0, 1.0], "xdir": [1.0, 0.0, 0.0], "label": "XY"}),
    );
    h.state_mut().sketch_set_tool("text");
    h.run_steps(2);
    sketch_click_uv(&mut h, [0.0, 0.0]);
    h.run_steps(4);
    // the text box starts as "Text"; the dialog's title says "Text" too, so take the biggest
    let field = h
        .get_all_by_value("Text")
        .max_by(|a, b| a.rect().area().total_cmp(&b.rect().area()))
        .expect("the text box, holding \"Text\"")
        .rect()
        .center();
    click_at(&mut h, field);
    h.run_steps(2);
    h.event(egui::Event::Key {
        key: egui::Key::A,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::COMMAND,
    });
    h.event(egui::Event::Text("HI".into()));
    h.run_steps(3);
    assert!(h.query_all_by_value("HI").next().is_some(), "the text box took the typing");
    h.get_by_label("Add text").click();
    h.run_steps(2);
    settle(&mut h, &link);
    h.run_steps(2);
    let s = h.state().sketch().expect("the sketch is still open");
    let shapes = s.entities.iter().filter(|e| !matches!(e, Entity::Point(_))).count();
    assert!(shapes > 0, "Add text left the sketch empty: {:?}", s.entities.len());
    assert!(h.query_by_label("Add text").is_none(), "the dialog closed");
}
