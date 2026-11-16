//! Feature panels laid out like SolidCraft's (Nathan, 2026-11-16): the main rows first, the rarely
//! changed ones folded under "More options".
#![cfg(not(target_arch = "wasm32"))]

use egui_kittest::kittest::Queryable;
use stl_app::tools::panel::laid_out;

use super::harness::{click_at, empty_harness, open_tool, settle};

#[test]
fn extrude_main_rows_come_first() {
    let def = stl_app::tools::defs::tool("extrude").expect("Extrude is a tool");
    let (main, more) = laid_out(def);
    let keys: Vec<&str> = main.iter().map(|f| f.key).collect();
    assert_eq!(&keys[..3], &["regions", "sketch", "direction"], "{keys:?}");
    assert!(keys.iter().position(|k| *k == "distance") < keys.iter().position(|k| *k == "operation"));
    assert_eq!(more.iter().map(|f| f.key).collect::<Vec<_>>(), ["start_offset", "taper"]);
    assert_eq!(main.len() + more.len(), def.fields.len(), "no field is lost or doubled");
}

#[test]
fn more_options_fold_opens_on_click() {
    let (mut h, link) = empty_harness();
    assert!(open_tool(&mut h, "extrude"));
    settle(&mut h, &link);
    h.run_steps(3);
    assert!(h.query_by_label("Taper angle").is_none(), "Taper starts folded away");
    let at = h.get_by_label("More options").rect().center();
    click_at(&mut h, at);
    h.run_steps(3);
    assert!(h.query_by_label("Taper angle").is_some(), "More options shows Taper");
    assert!(h.query_by_label("Start offset").is_some(), "and Start offset");
}
