//! Missing-glyph guard: with only the fonts that ship inside the app (no system font), nothing
//! the app can print may hit a character no font has. egui draws a box for such a character, and
//! the test harness turns that into a panic (the CI crash on "Support STL Editor ♥").
#![cfg(not(target_arch = "wasm32"))]

// keeps this test binary out of the real 2.0 folders (STLEDITOR2_CONFIG_DIR)
#[path = "support/test_env.rs"]
mod test_env;

#[path = "../../stl-platform/tests/support/fake_releases.rs"]
mod fake_releases;

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

/// Sketch labels, hints, toasts and panel text collected from `sketch/*.rs` constants and
/// the relation / dimension glyphs 1.x shows (⌀ ƒ • ⊙ ∥ ⊥ ◎ ⌯ 🔒 and °).
fn sketch_strings() -> Vec<(&'static str, String)> {
    let mut out = Vec::new();
    let push = |out: &mut Vec<(&'static str, String)>, where_: &'static str, s: &str| {
        if !s.is_empty() {
            out.push((where_, s.to_owned()));
        }
    };
    for (k, label, hint) in stl_app::sketch::relations::CTOOLS {
        push(&mut out, "sketch.CTOOLS.label", label);
        push(&mut out, "sketch.CTOOLS.hint", hint);
        let _ = k;
    }
    for key in [
        "select", "line", "arc3", "trim", "sk_mirror", "sk_corner", "sk_useedge", "rect", "rect_center", "circle", "polygon", "slot", "text",
    ] {
        let h = stl_app::sketch::draw::hint_for(key);
        if !h.is_empty() {
            push(&mut out, "sketch.draw.hint_for", h);
        }
        let h2 = stl_app::sketch::session::tool_hint(key);
        if !h2.is_empty() {
            push(&mut out, "sketch.session.tool_hint", &h2);
        }
    }
    for (where_, s) in [
        ("sketch.panel.FINISH_LABEL", stl_app::sketch::panel::FINISH_LABEL),
        ("sketch.session.UNKEPT", stl_app::sketch::session::UNKEPT),
        // U+232F ⌯ (conical taper) is the symmetric mark in relations/dims; not U+233F.
        ("sketch.glyphs", "⌀ƒ•⊙∥⊥◎\u{232F}🔒°×—✓−✕✗"),
        ("sketch.drawable.sample", "⌀ 12 · ƒ w · • coincident · ⊙ on · ∥ // · ⊥ _| · ◎ o · \u{232F} S · 🔒 F"),
    ] {
        push(&mut out, where_, s);
    }
    // Every non-ASCII character literally present in sketch source (string/char literals).
    let sketch_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/sketch");
    let mut stack = vec![sketch_dir];
    while let Some(dir) = stack.pop() {
        for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "rs") {
                let text = std::fs::read_to_string(&p).unwrap_or_default();
                let sample: String = code_chars(&text).into_iter().collect();
                if !sample.is_empty() {
                    push(&mut out, "sketch.src", &sample);
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

#[test]
fn every_sketch_label_hint_toast_and_panel_string_has_a_bundled_glyph() {
    let ctx = bundled_ctx();
    let strings = sketch_strings();
    assert!(strings.len() > 20, "sketch string table too small: {}", strings.len());
    // The 1.x sketch marks must be present (Selawik has ƒ •; symbols.ttf has the rest).
    for c in "⌀ƒ•⊙∥⊥◎\u{232F}🔒°−✕✗".chars() {
        assert!(has(&ctx, &FontFamily::Proportional, c), "bundled fonts lack sketch glyph {c} (U+{:04X})", c as u32);
        assert!(has(&ctx, &FontFamily::Name("bold".into()), c), "bold family lacks sketch glyph {c}");
    }
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
        "no glyph in the bundled fonts for sketch UI text:\n{}",
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

// ---------------------------------------------------------------- milestone 5: the new dialogs

/// What 2.0's Recover dialog and update banner print, written out: every character must be in the
/// bundled fonts (1.x's curly apostrophes and the "·" and "…" among them).
fn m5_strings() -> Vec<(&'static str, String)> {
    use stl_app::{recover, updates};
    let mut out: Vec<(&'static str, String)> = vec![
        ("recover.RECOVERED_TOAST", recover::RECOVERED_TOAST.into()),
        ("recover.NOTHING_TO_RECOVER", recover::NOTHING_TO_RECOVER.into()),
        ("recover.REPLACE_CONFIRM", recover::REPLACE_CONFIRM.into()),
        ("recover.RECOVERING_MSG", recover::RECOVERING_MSG.into()),
        ("recover.desktop", "STL Editor didn’t close normally last time. This work was saved automatically:".into()),
        ("recover.web", "This work wasn’t saved before STL Editor was last closed. A copy was kept in this browser:".into()),
        ("recover.row", "3 steps · autosaved 14 Nov 2026, 12:00".into()),
        ("updates.CHECKING", updates::CHECKING.into()),
        ("updates.CHECKS_OFF", updates::CHECKS_OFF.into()),
        ("updates.DOWNLOADING", updates::DOWNLOADING.into()),
        ("updates.RESTARTING", updates::RESTARTING.into()),
        ("updates.INSTALL_LINE", updates::INSTALL_LINE.into()),
        ("updates.DOWNLOAD_LINE", updates::DOWNLOAD_LINE.into()),
        ("updates.OTHER_WINDOWS", updates::OTHER_WINDOWS.into()),
        ("updates.SETTINGS_TITLE", updates::SETTINGS_TITLE.into()),
        ("updates.SETTINGS_CHECK", updates::SETTINGS_CHECK.into()),
        ("updates.SETTINGS_NOTE", updates::SETTINGS_NOTE.into()),
        ("updates.available_title", updates::available_title("2.0.1")),
        ("updates.latest_text", updates::latest_text("2.0.0")),
        ("drawer.open", "Bodies".into()),
        ("drawer.close", "Close".into()),
        ("help.recover", "Recover Unsaved Work…".into()),
    ];
    for item in updates::help_menu_items(false) {
        out.push(("updates.help_menu_items", item.1.into()));
    }
    for b in [
        updates::BannerButton::Skip,
        updates::BannerButton::Later,
        updates::BannerButton::WhatsNew,
        updates::BannerButton::UpdateNow,
        updates::BannerButton::Download,
        updates::BannerButton::Cancel,
        updates::BannerButton::Close,
        updates::BannerButton::DownloadInstead,
    ] {
        out.push(("updates.BannerButton", b.label().into()));
    }
    out
}

#[test]
fn every_recover_update_and_drawer_string_has_a_bundled_glyph() {
    let ctx = bundled_ctx();
    let strings = m5_strings();
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
    assert!(missing.is_empty(), "no glyph in the bundled fonts for the Recover / update / drawer text:\n{}", missing.join("\n"));
}

/// A Recover store with fixed copies (one with a title that needs the bundled fonts' non-ASCII).
struct Copies;

impl stl_app::recover::RecoverStore for Copies {
    fn list(&self) -> stl_app::platform::Pending<Vec<stl_app::recover::RecoverEntry>> {
        let e = |id: &str, title: &str, steps| stl_app::recover::RecoverEntry {
            id: id.into(),
            title: title.into(),
            path: Some(format!("C:\\parts\\{title}.stlx")),
            saved: 1_700_000_000.0,
            steps,
        };
        stl_app::platform::Pending::ready(vec![e("a", "Bracket – Ø8 × 20", 3), e("b", "Lid", 1)])
    }
    fn load(&self, _id: &str) -> stl_app::platform::Pending<Result<Vec<u8>, String>> {
        stl_app::platform::Pending::ready(Ok(vec![1, 2, 3]))
    }
    fn discard(&self, _id: &str) {}
}

struct Dialogs {
    recover: stl_app::recover::Recover,
    updates: stl_app::updates::Updates,
    platform: stl_app::platform::native::NativePlatform,
}

/// The Recover dialog (desktop and web wording, and its replace question), the update banner in
/// both of its forms, the Update Settings dialog and the phone drawer's button, drawn on the
/// bundled fonts alone. egui panics on a character no font has, so drawing them is the check.
#[test]
fn recover_update_and_drawer_dialogs_render_on_the_bundled_fonts() {
    use fake_releases::{FakeReleases, Faults};
    use stl_app::platform::native::NativePlatform;
    use stl_app::recover::Recover;
    use stl_app::updates::{BannerButton, Updates};

    let cfg = std::env::temp_dir().join(format!("stle2-fonts-m5-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&cfg);
    let release = serde_json::json!([{ "tag_name": "v2.0.1", "draft": false, "body": "What changed", "assets": [{
        "name": stl_platform::update::ASSET_NAME, "size": 10,
        "browser_download_url": format!("https://github.com/natterman12/STL-Editor/releases/download/v2.0.1/{}", stl_platform::update::ASSET_NAME) }] }]);
    let srv = FakeReleases::start(release, vec![], Faults::default());
    let api = format!("{}/releases", srv.base_url());
    let exe = cfg.join("stl-editor-2.exe");
    std::fs::create_dir_all(&cfg).unwrap();
    std::fs::write(&exe, b"x").unwrap();
    let mut recover = Recover::new(Box::new(Copies), false);
    recover.start();
    let mut st = Dialogs {
        recover,
        updates: Updates::custom(&api, "2.0.0", Some(exe), cfg.join("windows")),
        platform: NativePlatform::with_config_dir(&cfg),
    };
    st.updates.check_now(&st.platform, 1_800_000_000.0);

    let mut init = false;
    let mut h = Harness::builder().with_size(egui::vec2(1000.0, 700.0)).with_accessibility_check(false).build_ui_state(
        move |ui, st: &mut Dialogs| {
            if !init {
                init = true;
                ui.ctx().set_fonts(stl_app::fonts::bundled_definitions());
                ui.ctx().set_missing_glyph_policy(MissingGlyphPolicy::Panic);
            }
            let tk = stl_app::theme::tokens(false);
            stl_app::theme::apply(ui.ctx(), &tk);
            st.recover.show(ui.ctx(), &tk, true);
            st.updates.show(ui.ctx(), &tk, &st.platform, 1_800_000_000.0);
        },
        st,
    );
    let end = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while h.query_by_label("STL Editor 2.0.1 is available").is_none() || h.query_by_label("Recover unsaved work?").is_none() {
        assert!(std::time::Instant::now() < end, "the dialogs never showed");
        h.run_steps(2);
        std::thread::sleep(std::time::Duration::from_millis(15));
    }
    h.get_by_label_contains("Bracket – Ø8 × 20");
    // the replace question (the open project has unsaved work), then the banner's other forms
    h.get_all_by_label("Recover").next().unwrap().click();
    h.run_steps(4);
    h.get_by_label("Replace the current unsaved work with the recovered project?");
    h.get_by_label("Cancel").click();
    h.run_steps(3);
    // "Later" is on both the dialog and the banner: either is fine
    h.get_all_by_label("Later").next().unwrap().click();
    h.run_steps(3);
    let ui = h.state_mut();
    ui.updates.open_settings(&ui.platform);
    h.run_steps(4);
    h.get_by_label("Check for new versions once a day");
    let ui = h.state_mut();
    let _ = ui.updates.press(BannerButton::Skip, &ui.platform);
    h.run_steps(3);

    // the web wording of the dialog
    let mut recover = Recover::new(Box::new(Copies), true);
    recover.start();
    h.state_mut().recover = recover;
    h.run_steps(4);
    h.get_by_label("This work wasn’t saved before STL Editor was last closed. A copy was kept in this browser:");

    // the phone drawer: the button over the model, "Bodies" and, open, "Close"
    let mut init = false;
    let mut app = StlApp::empty(&egui::Context::default());
    app.browser.drawer_open = false;
    let mut p = Harness::builder().with_size(egui::vec2(390.0, 844.0)).with_accessibility_check(false).build_ui_state(
        move |ui, app: &mut StlApp| {
            if !init {
                init = true;
                ui.ctx().set_fonts(stl_app::fonts::bundled_definitions());
                ui.ctx().set_missing_glyph_policy(MissingGlyphPolicy::Panic);
                app.set_theme(ui.ctx(), ThemePref::Light);
            }
            app.layout(ui);
        },
        app,
    );
    p.run_steps(4);
    assert!(p.query_all_by_label("Bodies").count() > 0, "the drawer button over the model says Bodies");
    p.state_mut().browser.drawer_open = true;
    p.run_steps(12);
    assert!(p.query_all_by_label("Close").count() > 0, "and Close while the drawer is open");
    p.state_mut().browser.drawer_open = false;
    p.run_steps(12);
}
