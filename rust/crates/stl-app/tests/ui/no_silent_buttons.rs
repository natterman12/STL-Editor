//! UI test `no_silent_buttons` (milestone 6, spec 2.5): with the basic project open and a body
//! selected, every ribbon item on every tab, every tool-search command and every item of the body
//! right-click menu does something: a tool panel, a menu, a dialog, a mode, a toast, a changed
//! model or a recorded file dialog / program launch. Each is closed again before the next.
//!
//! Runs on a [`RecordPlatform`], which records file dialogs and launches instead of opening them
//! (a real dialog would hang the test), once as the desktop app and once as the web edition.

use egui::accesskit::Role;
use egui_kittest::kittest::{NodeT, Queryable};
use stl_app::engine_link::Command;
use stl_app::ui::menu::MenuItem;
use stl_app::ui::ribbon::{self, DeskTools, Item, Tool};

use super::harness::{RecordPlatform, app_harness_on, project, settle};

type H = egui_kittest::Harness<'static, stl_app::app::StlApp>;
type Link = std::sync::Arc<stl_app::engine_link::native::NativeLink>;

/// Everything the user could see change, as one string. Two equal strings mean nothing happened.
fn fingerprint(h: &H) -> String {
    let a = h.state();
    let menu = a.shell.menu.as_ref().map(|m| (m.tag.clone(), m.items.clone()));
    let bodies: Vec<_> = a
        .bodies
        .iter()
        .map(|b| {
            (
                b.id.clone(),
                b.name.clone(),
                b.color.clone(),
                b.hidden,
                b.solid,
                b.revision,
            )
        })
        .collect();
    format!(
        "{:?}",
        (
            menu,
            a.tool().map(|t| t.def.key),
            a.shell.active_tool.clone(),
            a.shell.dialogs.is_open(),
            a.sketch().is_some(),
            a.params_open,
            a.toasts.len(),
            a.platform.launch_log(),
            (
                a.section_open(),
                a.measure_open(),
                a.info_open(),
                a.browser.is_editing()
            ),
            a.shell.hint.clone(),
            bodies,
            (a.timeline_view.items.len(), a.timeline_view.marker),
        )
    )
}

struct Walk {
    h: H,
    link: Link,
    body: String,
    /// What was tried, and what did nothing.
    walked: Vec<String>,
    silent: Vec<String>,
}

fn press(h: &mut H, label: &str) -> bool {
    let nodes: Vec<_> = h
        .query_all_by_label(label)
        .filter(|n| n.accesskit_node().role() == Role::Button)
        .collect();
    match nodes.first() {
        Some(n) => {
            n.click();
            true
        }
        None => false,
    }
}

/// Press a row of the open menu. The ribbon is drawn before the popup menus (which are on a
/// layer above it), so a label that is also on the ribbon (Properties, Export, Send to Slicer)
/// must be taken from the last match, which is the menu's row.
fn press_menu(h: &mut H, label: &str) -> bool {
    if h.state().shell.menu.is_none() {
        return false;
    }
    let nodes: Vec<_> = h
        .query_all_by_label(label)
        .filter(|n| n.accesskit_node().role() == Role::Button)
        .collect();
    match nodes.last() {
        Some(n) => {
            n.click();
            true
        }
        None => false,
    }
}

fn right_click(h: &mut H, label: &str) -> bool {
    let nodes: Vec<_> = h
        .query_all_by_label(label)
        .filter(|n| n.accesskit_node().role() == Role::Button)
        .collect();
    match nodes.first() {
        Some(n) => {
            n.click_secondary();
            true
        }
        None => false,
    }
}

impl Walk {
    fn new(platform: RecordPlatform, width: f32) -> Walk {
        let (mut h, link) = app_harness_on(
            Some(project("basic.stlx").as_path()),
            egui::vec2(width, 900.0),
            platform,
        );
        let body = h
            .state()
            .bodies
            .first()
            .expect("the basic project has a body")
            .id
            .clone();
        h.state_mut().shell.selection = vec![body.clone()];
        settle(&mut h, &link);
        Walk {
            h,
            link,
            body,
            walked: Vec::new(),
            silent: Vec::new(),
        }
    }

    fn frames(&mut self) {
        self.h.run_steps(3);
        settle(&mut self.h, &self.link);
        self.h.run_steps(2);
    }

    /// Close whatever is open and select the body again.
    fn reset(&mut self) {
        for _ in 0..3 {
            self.h.state_mut().shell.menu = None;
            self.h.state_mut().shell.query.clear();
            self.h.key_press(egui::Key::Escape);
            self.h.run_steps(2);
        }
        if self.h.state().sketch().is_some() {
            self.h.state_mut().on_ribbon("sk_cancel", egui::Pos2::ZERO);
        }
        if self.h.state().browser.is_editing() {
            press(&mut self.h, "Cancel");
            self.h.run_steps(2);
        }
        let hidden: Vec<String> = self
            .h
            .state()
            .bodies
            .iter()
            .filter(|b| b.hidden)
            .map(|b| b.id.clone())
            .collect();
        for id in hidden {
            self.h.state_mut().link.send(Command::BodyProps {
                id,
                name: None,
                color: None,
                hidden: Some(false),
            });
        }
        settle(&mut self.h, &self.link);
        self.h.state_mut().toasts.clear();
        self.h.state_mut().shell.menu = None;
        self.h.state_mut().shell.tab = "solid".to_owned();
        self.h.state_mut().shell.selection = vec![self.body.clone()];
        self.h.run_steps(2);
        let a = self.h.state();
        let open = a.tool().is_some()
            || a.shell.active_tool.is_some()
            || a.section_open()
            || a.measure_open()
            || a.info_open()
            || a.shell.dialogs.is_open()
            || a.sketch().is_some();
        assert!(
            !open,
            "could not close what the last item opened: {}",
            fingerprint(&self.h)
        );
    }

    /// Run `act` on a clean screen; it passes when something on screen or in the model changed.
    fn check(&mut self, what: &str, act: impl FnOnce(&mut H) -> bool) {
        self.reset();
        let before = fingerprint(&self.h);
        let found = act(&mut self.h);
        self.frames();
        let after = fingerprint(&self.h);
        self.walked.push(what.to_owned());
        if !found {
            self.silent
                .push(format!("{what}: there is no such button or row to press"));
        } else if before == after {
            self.silent.push(format!("{what}: pressed, and nothing happened"));
        }
    }
}

fn tool_buttons(item: &Item) -> Vec<Tool> {
    match item {
        Item::Big(t) => vec![*t],
        Item::Stack(v) => v.clone(),
        _ => Vec::new(),
    }
}

/// Every ribbon item on every tab, as the ribbon shows them on this platform.
fn walk_ribbon(w: &mut Walk, desk: &DeskTools) {
    for (tab, _) in ribbon::TABS {
        let d = ribbon::def_shown(tab, desk);
        let groups: Vec<_> = d.groups.iter().chain(std::iter::once(&d.pinned)).collect();
        for g in groups {
            for item in &g.items {
                for t in tool_buttons(item) {
                    if ribbon::tool_disabled(t.key, desk) {
                        // greyed out on purpose (To STEP without SolidCraft's tool): a click does nothing
                        continue;
                    }
                    w.check(&format!("ribbon {tab} › {} › {}", g.label, t.label), |h| {
                        h.state_mut().shell.tab = tab.to_owned();
                        h.run_steps(2);
                        press(h, t.label)
                    });
                }
                if let Item::Drop { id, label, items } = item {
                    for t in items {
                        w.check(
                            &format!("ribbon {tab} › {} › {label} › {}", g.label, t.label),
                            |h| {
                                h.state_mut().shell.tab = tab.to_owned();
                                h.run_steps(2);
                                press(h, label) && {
                                    h.run_steps(3);
                                    press_menu(h, t.label)
                                }
                            },
                        );
                    }
                    w.check(&format!("ribbon {tab} › {} › {label} dropdown", g.label), |h| {
                        h.state_mut().shell.tab = tab.to_owned();
                        h.run_steps(2);
                        press(h, label)
                    });
                    let _ = id;
                }
            }
            if g.has_drop() {
                w.check(&format!("ribbon {tab} › {} group menu", g.label), |h| {
                    h.state_mut().shell.tab = tab.to_owned();
                    h.run_steps(2);
                    press(h, &format!("{} tools", g.label))
                });
            }
        }
    }
    w.h.state_mut().shell.tab = "solid".to_owned();
}

/// The menus the ribbon's own buttons open: Export and Send to Slicer.
fn walk_button_menus(w: &mut Walk, desk: &DeskTools, web: bool) {
    for label in ["STL (.stl)", "3MF (.3mf)", "OBJ (.obj)", "STEP (.step), flat faces"] {
        w.check(&format!("Export menu › {label}"), |h| {
            h.state_mut().shell.tab = "print".to_owned();
            h.run_steps(2);
            press(h, "Export") && {
                h.run_steps(3);
                press_menu(h, label)
            }
        });
    }
    if desk.send && !web {
        for (key, name) in w.h.state().platform.slicers() {
            w.check(&format!("Send to Slicer menu › {name} ({key})"), |h| {
                h.state_mut().shell.tab = "print".to_owned();
                h.run_steps(2);
                press(h, "Send to Slicer") && {
                    h.run_steps(3);
                    press_menu(h, &name)
                }
            });
        }
    }
    w.h.state_mut().shell.tab = "solid".to_owned();
}

/// The More menu a narrow window folds groups into: every row in it.
fn walk_more(web: bool) -> usize {
    let mut walked = 0;
    for width in [1000.0, 900.0, 800.0] {
        let platform = if web {
            RecordPlatform::web()
        } else {
            RecordPlatform::new(&[("bambu", "Bambu Studio")], true)
        };
        let mut w = Walk::new(platform, width);
        for (tab, _) in ribbon::TABS {
            w.h.state_mut().shell.tab = tab.to_owned();
            w.h.run_steps(3);
            if w.h.query_all_by_label("More tools").next().is_none() {
                continue;
            }
            w.check(&format!("More menu on {tab} at {width}"), |h| press(h, "More tools"));
            let rows: Vec<String> = match &w.h.state().shell.menu {
                Some(m) => m
                    .items
                    .iter()
                    .filter_map(|i| {
                        if let MenuItem::Item { label, dis: false, .. } = i {
                            Some(label.clone())
                        } else {
                            None
                        }
                    })
                    .collect(),
                None => Vec::new(),
            };
            for label in rows {
                w.check(&format!("More menu on {tab} at {width} › {label}"), |h| {
                    h.state_mut().shell.tab = tab.to_owned();
                    h.run_steps(3);
                    press(h, "More tools") && {
                        h.run_steps(3);
                        press_menu(h, &label)
                    }
                });
            }
        }
        walked += w.walked.len();
        assert!(
            w.silent.is_empty(),
            "silent items in the More menu:\n{}",
            w.silent.join("\n")
        );
        if walked > 0 {
            break;
        }
    }
    assert!(
        walked > 0,
        "no tab folded anything into the More menu at 1000, 900 or 800 wide"
    );
    walked
}

/// Every command the tool search lists (Ctrl+K): type its name, pick the row.
fn walk_search(w: &mut Walk, desk: &DeskTools) {
    // what the search lists on this platform (it leaves out the desktop tools the ribbon hides)
    for c in ribbon::all_commands_shown(false, desk) {
        let (k, l) = (c.k.clone(), c.l.clone());
        w.check(&format!("search › {l} ({k})"), |h| {
            let nodes: Vec<_> = h.query_all_by_role_and_label(Role::TextInput, "Search tools").collect();
            let Some(input) = nodes.first() else { return false };
            input.click();
            input.type_text(&l);
            h.run_steps(3);
            press_menu(h, &l)
        });
    }
}

/// Import a mesh that can't be made watertight (one triangle: import repairs what it can, so a
/// box with a hole would come back solid) and return its body's id: its menu has the Repair… row.
fn add_open_body(w: &mut Walk) -> String {
    let before: Vec<String> = w.h.state().bodies.iter().map(|b| b.id.clone()).collect();
    let stl = "solid tri
facet normal 0 0 1
outer loop
vertex 0 0 0
vertex 10 0 0
vertex 0 10 0
endloop
endfacet
endsolid tri
";
    w.h.state_mut().link.send(Command::Import {
        name: "one_triangle.stl".to_owned(),
        bytes: stl.as_bytes().to_vec(),
    });
    w.frames();
    let open =
        w.h.state()
            .bodies
            .iter()
            .find(|b| !before.contains(&b.id))
            .expect("the import added a body");
    assert!(
        !open.solid,
        "a single triangle is not watertight, so the body needs repair"
    );
    open.id.clone()
}

/// Every row of the body's right-click menu in the browser tree.
fn walk_body_menu(w: &mut Walk) {
    let body = w.h.state().bodies.iter().find(|b| b.id == w.body).cloned().unwrap();
    let name = body.name.clone();
    // what the menu lists
    w.reset();
    assert!(
        right_click(&mut w.h, &name),
        "no row for the body {name} in the browser"
    );
    w.frames();
    let rows: Vec<(String, bool)> = match &w.h.state().shell.menu {
        Some(m) => m
            .items
            .iter()
            .filter_map(|i| {
                if let MenuItem::Item { label, dis, .. } = i {
                    Some((label.clone(), *dis))
                } else {
                    None
                }
            })
            .collect(),
        None => Vec::new(),
    };
    if rows.is_empty() {
        w.walked.push("body menu".to_owned());
        w.silent
            .push("body menu: right-clicking a body in the browser opens no menu".to_owned());
        return;
    }
    assert_eq!(
        rows.iter().any(|(l, _)| l == "Repair…"),
        !body.solid,
        "Repair… is in the menu of a body that isn't watertight, and only there: {rows:?}"
    );
    for (label, dis) in rows {
        if dis {
            continue;
        }
        let n = name.clone();
        w.check(&format!("body menu › {label}"), |h| {
            right_click(h, &n) && {
                h.run_steps(3);
                press_menu(h, &label)
            }
        });
        // a row that opens a second list (Send to Slicer): every row of it too
        if w.h
            .state()
            .shell
            .menu
            .as_ref()
            .is_some_and(|m| m.tag.starts_with("send"))
        {
            let subs: Vec<String> =
                w.h.state()
                    .shell
                    .menu
                    .as_ref()
                    .map(|m| {
                        m.items
                            .iter()
                            .filter_map(|i| {
                                if let MenuItem::Item { label, dis: false, .. } = i {
                                    Some(label.clone())
                                } else {
                                    None
                                }
                            })
                            .collect()
                    })
                    .unwrap_or_default();
            for s in subs {
                let (n, l) = (name.clone(), label.clone());
                w.check(&format!("body menu › {label} › {s}"), |h| {
                    right_click(h, &n) && {
                        h.run_steps(3);
                        press_menu(h, &l) && {
                            h.run_steps(3);
                            press_menu(h, &s)
                        }
                    }
                });
            }
        }
    }
}

fn report(what: &str, w: &Walk, extra: usize) {
    println!(
        "no_silent_buttons ({what}): walked {} items ({} of them in the More menu)",
        w.walked.len() + extra,
        extra
    );
    for line in w.walked.iter().filter(|l| l.starts_with("body menu")) {
        println!("  {line}");
    }
    assert!(
        w.silent.is_empty(),
        "{} item(s) did nothing ({what}):\n{}",
        w.silent.len(),
        w.silent.join("\n")
    );
}

/// The desktop app: SolidCraft's command-line tool and one slicer are "installed".
#[test]
fn no_silent_buttons_on_desktop() {
    let mut w = Walk::new(RecordPlatform::new(&[("bambu", "Bambu Studio")], true), 1900.0);
    let desk = DeskTools::of(&*w.h.state().platform);
    walk_ribbon(&mut w, &desk);
    walk_button_menus(&mut w, &desk, false);
    walk_search(&mut w, &desk);
    walk_body_menu(&mut w);
    let open = add_open_body(&mut w);
    let first = std::mem::replace(&mut w.body, open);
    walk_body_menu(&mut w);
    w.body = first;
    let more = walk_more(false);
    report("desktop", &w, more);
}

/// The web edition: Send to Slicer and To STEP say they are desktop-only (a dialog).
#[test]
fn no_silent_buttons_on_web() {
    let mut w = Walk::new(RecordPlatform::web(), 1900.0);
    let desk = DeskTools::of(&*w.h.state().platform);
    walk_ribbon(&mut w, &desk);
    walk_button_menus(&mut w, &desk, true);
    walk_search(&mut w, &desk);
    walk_body_menu(&mut w);
    let open = add_open_body(&mut w);
    let first = std::mem::replace(&mut w.body, open);
    walk_body_menu(&mut w);
    w.body = first;
    let more = walk_more(true);
    report("web", &w, more);
}

/// The walk itself notices a button that does nothing, and one that isn't there.
#[test]
fn the_walk_catches_a_silent_button() {
    let mut w = Walk::new(RecordPlatform::new(&[], false), 1900.0);
    w.check("a button that does nothing", |_| true);
    w.check("a button that isn't there", |h| press(h, "No Such Button"));
    assert_eq!(w.silent.len(), 2, "{:?}", w.silent);
}

/// The tool search doesn't list Send to Slicer or To STEP where the ribbon hides them.
#[test]
fn search_leaves_out_tools_this_platform_hides() {
    let keys = |d: &DeskTools, q: &str| {
        ribbon::search_shown(q, false, d)
            .into_iter()
            .map(|c| c.k)
            .collect::<Vec<_>>()
    };
    let hidden = DeskTools {
        send: false,
        to_step: false,
        step_ready: false,
    };
    let shown = DeskTools {
        send: true,
        to_step: true,
        step_ready: true,
    };
    assert!(keys(&hidden, "slicer").is_empty(), "{:?}", keys(&hidden, "slicer"));
    assert!(keys(&hidden, "step").is_empty(), "{:?}", keys(&hidden, "step"));
    assert!(keys(&shown, "slicer").contains(&"send".to_owned()));
    assert!(keys(&shown, "step").contains(&"to_step".to_owned()));
    assert!(
        !ribbon::all_commands_shown(false, &hidden)
            .iter()
            .any(|c| c.k == "send" || c.k == "to_step")
    );
}

fn menu_rows(h: &H) -> Vec<(String, bool)> {
    match &h.state().shell.menu {
        Some(m) => m
            .items
            .iter()
            .filter_map(|i| {
                if let MenuItem::Item { label, dis, .. } = i {
                    Some((label.clone(), *dis))
                } else {
                    None
                }
            })
            .collect(),
        None => Vec::new(),
    }
}

/// The body's right-click menu lists 1.x's rows, and they act on that body.
#[test]
fn body_menu_has_the_1x_rows_and_they_act() {
    let mut w = Walk::new(RecordPlatform::new(&[("bambu", "Bambu Studio")], false), 1900.0);
    let name =
        w.h.state()
            .bodies
            .iter()
            .find(|b| b.id == w.body)
            .map(|b| b.name.clone())
            .unwrap();
    assert!(right_click(&mut w.h, &name));
    w.frames();
    let rows = menu_rows(&w.h);
    let mut want: Vec<(&str, bool)> = vec![
        ("Rename…", false),
        ("Hide", false),
        ("Colour…", false),
        ("Properties", false),
        ("Move / Rotate…", false),
        ("Scale…", false),
        ("Simplify…", false),
        ("Export STL…", false),
        ("Export 3MF…", false),
        ("Send to Slicer", false),
    ];
    if cfg!(windows) {
        // SolidCraft's tool isn't installed here: the row shows, greyed out
        want.push(("Convert to STEP (SolidCraft)…", true));
    }
    want.push(("Remove", false));
    let got: Vec<(&str, bool)> = rows.iter().map(|(l, d)| (l.as_str(), *d)).collect();
    assert_eq!(got, want);

    // Properties shows the clicked body, also when another one is selected first and both are
    let other =
        w.h.state()
            .bodies
            .iter()
            .find(|b| b.id != w.body)
            .map(|b| b.id.clone())
            .expect("the basic project has two bodies");
    let other_name =
        w.h.state()
            .bodies
            .iter()
            .find(|b| b.id == other)
            .map(|b| b.name.clone())
            .unwrap();
    assert_ne!(name, other_name);
    w.reset();
    w.h.state_mut().shell.selection = vec![w.body.clone(), other.clone()];
    w.h.run_steps(2);
    assert!(right_click(&mut w.h, &other_name));
    w.h.run_steps(3);
    assert!(press_menu(&mut w.h, "Properties"));
    w.frames();
    assert!(w.h.state().info_open());
    assert_eq!(
        w.h.state().info_readout().map(|(n, _)| n),
        Some(other_name.clone()),
        "Properties shows the clicked body"
    );
    w.reset();
    // a different body selected on its own: right-click selects the clicked one and shows it
    assert!(right_click(&mut w.h, &name));
    w.h.run_steps(3);
    assert!(press_menu(&mut w.h, "Properties"));
    w.frames();
    assert_eq!(w.h.state().info_readout().map(|(n, _)| n), Some(name.clone()));
    w.reset();

    // Repair… is in the menu of a body that isn't watertight, and opens Repair for it
    let open = add_open_body(&mut w);
    let open_name =
        w.h.state()
            .bodies
            .iter()
            .find(|b| b.id == open)
            .map(|b| b.name.clone())
            .unwrap();
    w.reset();
    assert!(right_click(&mut w.h, &open_name));
    w.h.run_steps(3);
    assert!(menu_rows(&w.h).iter().any(|(l, _)| l == "Repair…"));
    assert!(press_menu(&mut w.h, "Repair…"));
    w.frames();
    let tool = w.h.state().tool().expect("Repair opens");
    assert_eq!(tool.def.key, "repair");
    assert_eq!(tool.values.get("body"), Some(&serde_json::json!([open.clone()])));
    w.reset();

    // Export STL asks where to save, for this body
    assert!(right_click(&mut w.h, &name));
    w.h.run_steps(3);
    assert!(press_menu(&mut w.h, "Export STL…"));
    w.frames();
    assert!(
        w.h.state()
            .platform
            .launch_log()
            .iter()
            .any(|l| l.starts_with("save ") && l.ends_with(".stl")),
        "{:?}",
        w.h.state().platform.launch_log()
    );
    w.reset();

    // Send to Slicer lists the slicers and sends one 3MF
    assert!(right_click(&mut w.h, &name));
    w.h.run_steps(3);
    assert!(press_menu(&mut w.h, "Send to Slicer"));
    w.h.run_steps(3);
    assert!(w.h.query_by_label("Bambu Studio").is_some());
    assert!(press_menu(&mut w.h, "Bambu Studio"));
    w.frames();
    let launches: Vec<String> =
        w.h.state()
            .platform
            .launch_log()
            .into_iter()
            .filter(|l| l.starts_with("launch "))
            .collect();
    assert_eq!(launches.len(), 1, "{launches:?}");
    assert!(
        launches[0].starts_with("launch bambu ") && launches[0].ends_with(".3mf"),
        "{launches:?}"
    );
    w.reset();

    // Hide hides it, and the row then says Show
    assert!(right_click(&mut w.h, &name));
    w.h.run_steps(3);
    assert!(press_menu(&mut w.h, "Hide"));
    w.frames();
    assert!(w.h.state().bodies.iter().find(|b| b.id == w.body).unwrap().hidden);
    assert!(right_click(&mut w.h, &name));
    w.h.run_steps(3);
    assert!(menu_rows(&w.h).iter().any(|(l, _)| l == "Show"));
}

/// On the web there is no Send to Slicer or Convert to STEP in the body menu (1.x leaves them out).
#[test]
fn body_menu_on_the_web_has_no_desktop_rows() {
    let mut w = Walk::new(RecordPlatform::web(), 1900.0);
    let name =
        w.h.state()
            .bodies
            .iter()
            .find(|b| b.id == w.body)
            .map(|b| b.name.clone())
            .unwrap();
    assert!(right_click(&mut w.h, &name));
    w.frames();
    let rows = menu_rows(&w.h);
    assert!(rows.iter().any(|(l, _)| l == "Export 3MF…"));
    assert!(
        !rows
            .iter()
            .any(|(l, _)| l == "Send to Slicer" || l.starts_with("Convert to STEP")),
        "{rows:?}"
    );
}

fn right_press(h: &mut H, at: egui::Pos2, release_at: egui::Pos2) {
    use egui::{Event, PointerButton};
    h.event(Event::PointerMoved(at));
    h.step();
    h.event(Event::PointerButton {
        pos: at,
        button: PointerButton::Secondary,
        pressed: true,
        modifiers: Default::default(),
    });
    h.step();
    if release_at != at {
        h.event(Event::PointerMoved(release_at));
        h.step();
    }
    h.event(Event::PointerButton {
        pos: release_at,
        button: PointerButton::Secondary,
        pressed: false,
        modifiers: Default::default(),
    });
    h.run_steps(3);
}

/// Right-click in the 3D view: on a body its menu (the body is selected first), on empty space the
/// view menu; a right-drag (pan) opens nothing.
#[test]
fn right_click_in_the_3d_view_opens_the_body_or_view_menu() {
    let mut w = Walk::new(RecordPlatform::new(&[("bambu", "Bambu Studio")], false), 1280.0);
    w.h.state_mut().viewport.set_view("iso");
    w.h.state_mut().viewport.flush();
    w.frames();
    // a spot on the first body, found by looking for a pick
    let on_body = {
        let b = w.h.state().viewport.body_box(&w.body).unwrap();
        let c = [(b.min[0] + b.max[0]) / 2.0, (b.min[1] + b.max[1]) / 2.0, b.max[2]];
        w.h.state().viewport.world_to_screen(c)
    };
    // nothing selected: the right-click selects the body it is on
    w.h.state_mut().shell.selection.clear();
    right_press(&mut w.h, on_body, on_body);
    assert_eq!(
        w.h.state().shell.menu.as_ref().map(|m| m.tag.as_str()),
        Some("body"),
        "a body menu opens on a body"
    );
    assert_eq!(
        w.h.state().shell.selection,
        vec![w.body.clone()],
        "the body is selected"
    );
    assert!(menu_rows(&w.h).iter().any(|(l, _)| l == "Properties"));
    w.reset();

    // a right-drag orbits and opens nothing
    let moved = on_body + egui::vec2(60.0, 40.0);
    let cam = w.h.state().viewport.camera().position;
    right_press(&mut w.h, on_body, moved);
    assert!(w.h.state().shell.menu.is_none(), "a right-drag opens no menu");
    assert_ne!(w.h.state().viewport.camera().position, cam, "the right-drag orbited");
    w.reset();

    // empty space: the view menu, every row of which acts
    let empty = egui::pos2(1240.0, 700.0);
    assert!(
        w.h.state().viewport.pick(empty, true, false).is_none(),
        "the spot is empty"
    );
    right_press(&mut w.h, empty, empty);
    assert_eq!(w.h.state().shell.menu.as_ref().map(|m| m.tag.as_str()), Some("view"));
    let rows: Vec<String> = menu_rows(&w.h).into_iter().map(|(l, _)| l).collect();
    assert_eq!(rows, ["Fit All", "Isometric", "Top", "Insert Mesh…", "Create Sketch"]);
    w.reset();

    right_press(&mut w.h, empty, empty);
    let pos = w.h.state().viewport.camera().position;
    assert!(press_menu(&mut w.h, "Top"));
    w.h.run_steps(3);
    for _ in 0..90 {
        w.h.run_steps(1);
    }
    assert_ne!(w.h.state().viewport.camera().position, pos, "Top turns the view");
    w.reset();

    right_press(&mut w.h, empty, empty);
    assert!(press_menu(&mut w.h, "Insert Mesh…"));
    w.frames();
    assert!(
        w.h.state().platform.launch_log().iter().any(|l| l == "open file"),
        "{:?}",
        w.h.state().platform.launch_log()
    );
    w.reset();

    right_press(&mut w.h, empty, empty);
    assert!(press_menu(&mut w.h, "Create Sketch"));
    w.frames();
    assert!(
        w.h.state().tool().is_some() || w.h.state().sketch().is_some(),
        "Create Sketch starts a sketch"
    );
    w.reset();

    // Fit All and Isometric only move the camera: the menu closes
    for label in ["Fit All", "Isometric"] {
        right_press(&mut w.h, empty, empty);
        assert!(press_menu(&mut w.h, label));
        w.h.run_steps(3);
        assert!(w.h.state().shell.menu.is_none(), "{label} closes the menu");
    }
}

/// Colour… in the body menu opens the colour window; a picked colour is sent for that body.
#[test]
fn body_menu_colour_sends_the_picked_colour() {
    let mut w = Walk::new(RecordPlatform::new(&[], false), 1900.0);
    // egui's colour picker draws a clipboard emoji that no installed font has (tofu in the real
    // window); the test harness panics on a missing glyph unless told to draw tofu
    w.h.ctx.set_missing_glyph_policy(egui::MissingGlyphPolicy::Tofu);
    let name =
        w.h.state()
            .bodies
            .iter()
            .find(|b| b.id == w.body)
            .map(|b| b.name.clone())
            .unwrap();
    assert!(right_click(&mut w.h, &name));
    w.h.run_steps(3);
    assert!(press_menu(&mut w.h, "Colour…"));
    w.frames();
    assert!(w.h.state().browser.is_editing(), "Colour… opens the colour window");
    // open the picker and set red to 200 (green and blue stay as they are)
    let well =
        w.h.query_all_by_role(Role::ColorWell)
            .next()
            .expect("the colour button")
            .rect();
    super::harness::click_at(&mut w.h, well.center());
    w.h.run_steps(3);
    let red =
        w.h.query_all_by_role(Role::SpinButton)
            .find(|n| n.accesskit_node().label().as_deref() == Some("R"))
            .expect("the red box")
            .rect();
    super::harness::click_at(&mut w.h, red.center());
    w.h.run_steps(2);
    w.h.event(egui::Event::Text("200".to_owned()));
    w.h.run_steps(2);
    w.h.key_press(egui::Key::Enter);
    w.h.run_steps(3);
    let rgb = |h: &H| -> Vec<String> {
        h.query_all_by_role(Role::SpinButton)
            .filter_map(|n| n.accesskit_node().value().map(|v| v.to_string()))
            .collect()
    };
    assert_eq!(rgb(&w.h)[0], "R 200", "{:?}", rgb(&w.h));
    // OK sends the colour for this body (the first click closes the picker, the second is OK)
    let ok = w.h.get_by_label("OK").rect().center();
    super::harness::click_at(&mut w.h, ok);
    w.h.run_steps(2);
    if w.h.state().browser.is_editing() {
        let ok = w.h.get_by_label("OK").rect().center();
        super::harness::click_at(&mut w.h, ok);
    }
    w.frames();
    assert!(!w.h.state().browser.is_editing(), "OK closes the colour window");
    let colour =
        w.h.state()
            .bodies
            .iter()
            .find(|b| b.id == w.body)
            .and_then(|b| b.color.clone());
    assert_eq!(
        colour.as_deref(),
        Some("#c8a7b4"),
        "the picked colour is the body's colour"
    );
}
