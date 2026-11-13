//! The theme and icons stay in step with 1.x's web/css and web/js/icons.js.

use egui::Color32;
use std::collections::BTreeMap;
use stl_app::{icons, theme};

fn repo() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(repo().join(rel)).unwrap().replace("\r\n", "\n")
}

/// `--name: value;` declarations of the block that starts at `open` (up to its closing "\n}").
fn block(css: &str, open: &str) -> BTreeMap<String, String> {
    let start = css.find(open).unwrap_or_else(|| panic!("no {open} block")) + open.len();
    let body = &css[start..start + css[start..].find("\n}").unwrap()];
    let mut clean = String::new();
    let mut rest = body;
    while let Some(i) = rest.find("/*") {
        clean += &rest[..i];
        rest = &rest[i + 2 + rest[i..].find("*/").unwrap()..];
    }
    clean += rest;
    let mut m = BTreeMap::new();
    for d in clean.split(';') {
        if let Some(i) = d.find("--") {
            if let Some((k, v)) = d[i + 2..].split_once(':') {
                m.insert(k.trim().to_string(), v.trim().to_string());
            }
        }
    }
    m
}

fn colour(v: &str) -> Color32 {
    if let Some(h) = v.strip_prefix('#') {
        let n = u32::from_str_radix(h, 16).unwrap();
        assert_eq!(h.len(), 6, "{v}");
        return Color32::from_rgb((n >> 16) as u8, (n >> 8) as u8, n as u8);
    }
    let inner = v.strip_prefix("rgba(").and_then(|s| s.strip_suffix(')')).unwrap_or_else(|| panic!("not a colour: {v}"));
    let p: Vec<&str> = inner.split(',').map(str::trim).collect();
    let a: f32 = p[3].parse().unwrap();
    Color32::from_rgba_unmultiplied(p[0].parse().unwrap(), p[1].parse().unwrap(), p[2].parse().unwrap(), (a * 255.0).round() as u8)
}

fn check(dark: bool, css: &BTreeMap<String, String>) {
    let t = theme::tokens(dark);
    let mut seen = 0;
    for (k, v) in css {
        match k.as_str() {
            "font" => assert!(v.starts_with("\"Segoe UI\""), "font stack changed: {v}"),
            "radius" => assert_eq!(t.radius, v.trim_end_matches("px").parse::<f32>().unwrap()),
            "radius-sm" => assert_eq!(t.radius_sm, v.trim_end_matches("px").parse::<f32>().unwrap()),
            "shadow" | "shadow-lg" => {
                // "0 6px 20px rgba(...)"
                let (head, rgba) = v.split_at(v.find("rgba").unwrap());
                let n: Vec<f32> = head.split_whitespace().map(|x| x.trim_end_matches("px").parse().unwrap()).collect();
                let s = if k == "shadow" { t.shadow } else { t.shadow_lg };
                assert_eq!((s.offset[0] as f32, s.offset[1] as f32, s.blur as f32, s.spread as f32), (n[0], n[1], n[2], 0.0), "--{k}");
                assert_eq!(s.color, colour(rgba), "--{k}");
            }
            "color-scheme" => {}
            _ => {
                let got = t.colour(k).unwrap_or_else(|| panic!("--{k} has no token"));
                assert_eq!(got, colour(v), "--{k} ({})", if dark { "dark" } else { "light" });
                seen += 1;
            }
        }
    }
    assert!(seen > 40);
}

#[test]
fn tokens_match_css() {
    let css = read("web/css/app.css");
    let dark = block(&css, ":root {");
    let mut light = dark.clone();
    light.extend(block(&css, "html[data-theme=light] {"));
    // Every colour the code knows is in the CSS, and every CSS colour is in the code.
    for n in theme::COLOUR_NAMES {
        assert!(dark.contains_key(*n), "token {n} is not in the CSS");
    }
    check(true, &dark);
    check(false, &light);
    // The light block must not leave out a colour that only the dark block sets without a fallback.
    assert_eq!(theme::tokens(true).accent, colour("#4c8df6"));
}

#[test]
fn icons_cover_icons_js() {
    let js = read("web/js/icons.js");
    // (A key defined twice in icons.js keeps its last definition, so count each name once.)
    let keys: Vec<&str> = js
        .lines()
        .filter_map(|l| l.strip_prefix("  ")?.split_once(": S(").map(|(k, _)| k.trim()))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    assert!(keys.len() > 60, "found only {} icon keys", keys.len());
    for k in &keys {
        assert!(icons::names().contains(k), "icon {k} missing from assets/icons.json (run assets/gen_icons.mjs)");
        let px = icons::render_rgba(k, 48, Color32::from_rgb(0x4c, 0x8d, 0xf6)).unwrap_or_else(|e| panic!("{k}: {e}"));
        assert_eq!(px.len(), 48 * 48 * 4);
        assert!(px.chunks(4).any(|p| p[3] > 0), "icon {k} draws nothing");
    }
    assert_eq!(keys.len(), icons::names().len(), "icons.json has extra or missing icons");
    // Unknown names fall back to `primitive`, like icon() in icons.js.
    assert!(icons::render_rgba("no-such-icon", 24, Color32::WHITE).is_ok());
}

#[test]
fn fonts_load_and_style_applies() {
    let ctx = egui::Context::default();
    stl_app::fonts::install(&ctx);
    for dark in [true, false] {
        theme::apply(&ctx, &theme::tokens(dark));
    }
    let out = ctx.run_ui(egui::RawInput::default(), |ui| {
        ui.label("Segoe UI 12px");
        let src = icons::icon(ui.ctx(), "undo", 18.0, Color32::WHITE);
        ui.add(egui::Image::new(src));
    });
    let mut out = out;
    out.textures_delta.clear();
}
