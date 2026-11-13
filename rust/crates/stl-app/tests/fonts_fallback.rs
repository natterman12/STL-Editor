//! Missing-glyph guard: with only the fonts that ship inside the app (no system font), nothing
//! the app can print may hit a character no font has. egui draws a box for such a character, and
//! the test harness turns that into a panic (the CI crash on "Support STL Editor ♥").
#![cfg(not(target_arch = "wasm32"))]

use egui::{FontFamily, FontId, MissingGlyphPolicy};
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use stl_app::app::StlApp;
use stl_app::supports;
use stl_app::tools::{self, FieldKind};
use stl_app::ui::ThemePref;

/// Consume font-atlas uploads so Drop of [`egui::TexturesDelta`] does not panic.
fn absorb_textures(out: &mut egui::FullOutput) {
    out.textures_delta.set.clear();
    out.textures_delta.free.clear();
}

/// A context with only the bundled fonts installed, after one frame so they are loaded.
fn bundled_ctx() -> egui::Context {
    let ctx = egui::Context::default();
    ctx.set_fonts(stl_app::fonts::bundled_definitions());
    ctx.set_missing_glyph_policy(MissingGlyphPolicy::Panic);
    let mut out = ctx.run_ui(egui::RawInput::default(), |_| {});
    absorb_textures(&mut out);
    // A second frame finishes atlas packing for the newly installed fonts.
    let mut out = ctx.run_ui(egui::RawInput::default(), |_| {});
    absorb_textures(&mut out);
    ctx
}

fn has(ctx: &egui::Context, family: &FontFamily, c: char) -> bool {
    ctx.fonts_mut(|f| f.has_glyph(&FontId::new(12.0, family.clone()), c))
}

/// Every non-ASCII character that appears in code (string and char literals, `\u{..}` escapes),
/// not in comments, in the source of the app and the crates whose messages it shows.
fn source_chars() -> std::collections::BTreeMap<char, String> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut out = std::collections::BTreeMap::new();
    for krate in ["stl-app", "stl-engine", "stl-io", "stl-platform", "stl-sketch", "stl-mesh", "stl-kernel"] {
        let mut stack = vec![root.join(krate).join("src")];
        while let Some(dir) = stack.pop() {
            for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().is_some_and(|x| x == "rs") {
                    let text = std::fs::read_to_string(&p).unwrap();
                    for c in code_chars(&text) {
                        out.entry(c).or_insert_with(|| p.strip_prefix(&root).unwrap().display().to_string());
                    }
                }
            }
        }
    }
    out
}

/// Labels, help, hints and messages from the tool registry and Support Fins panel constants.
fn panel_strings() -> Vec<(&'static str, String)> {
    let mut out = Vec::new();
    let push = |out: &mut Vec<(&'static str, String)>, where_: &'static str, s: &str| {
        if !s.is_empty() {
            out.push((where_, s.to_owned()));
        }
    };
    for t in tools::tools() {
        push(&mut out, "tools.label", t.label);
        if let Some(h) = t.hint {
            push(&mut out, "tools.hint", h);
        }
        if let Some(e) = t.extra {
            push(&mut out, "tools.extra", e);
        }
        for f in &t.fields {
            push(&mut out, "tools.field.label", f.label);
            if let Some(h) = f.help {
                push(&mut out, "tools.field.help", h);
            }
            if let Some(p) = f.placeholder {
                push(&mut out, "tools.field.placeholder", p);
            }
            match &f.kind {
                FieldKind::Number { unit: Some(u) } | FieldKind::Vec3 { unit: Some(u) } => push(&mut out, "tools.unit", u),
                FieldKind::Select { options, .. } => {
                    for (val, lab, _) in options {
                        push(&mut out, "tools.select.value", val);
                        push(&mut out, "tools.select.label", lab);
                    }
                }
                FieldKind::Text { placeholder: Some(p) } => push(&mut out, "tools.text.placeholder", p.as_str()),
                _ => {}
            }
        }
    }
    for (where_, s) in [
        ("supports.MODE_HELP", supports::MODE_HELP),
        ("supports.FINS_HINT", supports::FINS_HINT),
        ("supports.ROTATE_HINT", supports::ROTATE_HINT),
        ("supports.REMOVE_HINT", supports::REMOVE_HINT),
        ("supports.CREDIT_TITLE", supports::CREDIT_TITLE),
        ("supports.NO_OVERHANG_MSG", supports::NO_OVERHANG_MSG),
        ("supports.BUSY_FINS", supports::BUSY_FINS),
        ("supports.BUSY_WALLS", supports::BUSY_WALLS),
        ("supports.LOADING", supports::LOADING),
        ("supports.LOAD_HELP", supports::LOAD_HELP),
        ("supports.POSTURE_TIP", supports::POSTURE_TIP),
        ("supports.SUGGEST_BUSY", supports::SUGGEST_BUSY),
        ("supports.PICK_PART", supports::PICK_PART),
    ] {
        push(&mut out, where_, s);
    }
    for (_, label) in supports::MODES {
        push(&mut out, "supports.MODES", label);
    }
    for (_, label, _) in supports::MATERIALS {
        push(&mut out, "supports.MATERIALS", label);
    }
    for (_, label) in supports::walls::ALL_SHAPES {
        push(&mut out, "supports.ALL_SHAPES", label);
    }
    // Close / empty markers used in the tool panel
    push(&mut out, "panel.close", "×");
    push(&mut out, "panel.emdash", "—");
    out
}

/// Non-ASCII characters outside comments. Strings are tracked so a `//` inside one (a URL) is
/// not taken for a comment.
fn code_chars(src: &str) -> Vec<char> {
    let cs: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let (mut i, n) = (0, cs.len());
    let mut in_str = false;
    while i < n {
        let c = cs[i];
        if in_str {
            if c == '\\' {
                if cs.get(i + 1) == Some(&'u') && cs.get(i + 2) == Some(&'{') {
                    let end = (i + 3..n).find(|&j| cs[j] == '}').unwrap_or(n - 1);
                    let hex: String = cs[i + 3..end].iter().filter(|c| **c != '_').collect();
                    if let Some(ch) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                        if !ch.is_ascii() {
                            out.push(ch);
                        }
                    }
                    i = end + 1;
                    continue;
                }
                i += 2;
                continue;
            }
            if c == '"' {
                in_str = false;
            } else if !c.is_ascii() {
                out.push(c);
            }
            i += 1;
            continue;
        }
        match c {
            '/' if cs.get(i + 1) == Some(&'/') => {
                while i < n && cs[i] != '\n' {
                    i += 1;
                }
            }
            '/' if cs.get(i + 1) == Some(&'*') => {
                i += 2;
                while i + 1 < n && !(cs[i] == '*' && cs[i + 1] == '/') {
                    i += 1;
                }
                i += 2;
            }
            '"' => {
                in_str = true;
                i += 1;
            }
            // a char literal such as '×' (a lifetime is never followed by a closing quote)
            '\'' if !cs.get(i + 1).is_none_or(|c| c.is_ascii()) && cs.get(i + 2) == Some(&'\'') => {
                out.push(cs[i + 1]);
                i += 3;
            }
            _ => i += 1,
        }
    }
    out
}

#[test]
fn every_character_in_the_string_table_has_a_bundled_glyph() {
    let ctx = bundled_ctx();
    let chars = source_chars();
    assert!(chars.len() > 8, "the scan found only {} characters: it is broken", chars.len());
    let mut missing = Vec::new();
    for (c, file) in &chars {
        for family in [FontFamily::Proportional, FontFamily::Name("bold".into())] {
            if !has(&ctx, &family, *c) {
                missing.push(format!("{c} (U+{:04X}) in {file}, {family:?}", *c as u32));
            }
        }
    }
    assert!(missing.is_empty(), "no glyph in the bundled fonts for:\n{}\nadd it to a bundled font or draw it as an icon", missing.join("\n"));
}

#[test]
fn every_tool_and_supports_panel_string_has_a_bundled_glyph() {
    let ctx = bundled_ctx();
    let strings = panel_strings();
    assert!(strings.len() > 40, "panel string table too small: {}", strings.len());
    let mut missing = Vec::new();
    for (where_, s) in &strings {
        for c in s.chars().filter(|c| !c.is_ascii()) {
            for family in [FontFamily::Proportional, FontFamily::Name("bold".into())] {
                if !has(&ctx, &family, c) {
                    missing.push(format!("{c} (U+{:04X}) in {where_}: {s:?}, {family:?}", c as u32));
                }
            }
        }
    }
    assert!(
        missing.is_empty(),
        "no glyph in the bundled fonts for tool/supports panel text:\n{}",
        missing.join("\n")
    );
}

/// The start page, the menus, Keyboard Shortcuts and About, in both themes, on the bundled fonts
/// alone. egui panics on a character no font has (the policy is `Panic` here), so rendering them
/// is the check.
fn render_everything(pref: ThemePref) {
    let mut init = false;
    let app = StlApp::empty(&egui::Context::default());
    let mut h = Harness::builder().with_size(egui::vec2(1500.0, 700.0)).with_accessibility_check(false).build_ui_state(
        move |ui, app: &mut StlApp| {
            if !init {
                init = true;
                // only the bundled fonts: no Segoe UI, as on the web and on a bare Linux box
                ui.ctx().set_fonts(stl_app::fonts::bundled_definitions());
                ui.ctx().set_missing_glyph_policy(MissingGlyphPolicy::Panic);
                app.set_theme(ui.ctx(), pref);
            }
            app.layout(ui);
        },
        app,
    );
    h.run_steps(4);
    // the start page: "Free and open source · Support STL Editor ♥"
    assert!(h.query_by_label("Support STL Editor").is_some(), "the start page shows its Ko-fi line");
    for menu in ["File", "Edit", "View", "Help"] {
        h.get_by_label(menu).click();
        h.run_steps(4);
        h.key_press(egui::Key::Escape);
        h.run_steps(3);
    }
    h.get_by_label("Help").click();
    h.run_steps(4);
    assert!(h.query_by_label("Support STL Editor ♥").is_some(), "the Help menu line with the heart");
    h.get_by_label("Keyboard Shortcuts").click();
    h.run_steps(4);
    h.get_by_label("OK").click();
    h.run_steps(3);
    h.get_by_label("Help").click();
    h.run_steps(4);
    h.get_by_label("About STL Editor").click();
    h.run_steps(4);
    assert!(h.query_by_label_contains("you can buy me a coffee on Ko-fi").is_some(), "the About dialog shows the heart line");

    // Open a few tool panels so × / Ø / → / ° paint on the bundled fonts.
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    for key in ["hole", "move", "scale", "thread"] {
        if h.state_mut().open_tool(key, None) {
            h.run_steps(4);
            h.state_mut().tool_cancel();
            h.run_steps(2);
        }
    }
}

#[test]
fn start_page_menus_and_about_render_on_the_bundled_fonts_dark() {
    render_everything(ThemePref::Dark);
}

#[test]
fn start_page_menus_and_about_render_on_the_bundled_fonts_light() {
    render_everything(ThemePref::Light);
}
