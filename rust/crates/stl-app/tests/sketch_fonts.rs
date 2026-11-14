//! Milestone 5, Task 6: the sketch text font list. "More: fonts on this computer…" (desktop) and
//! "Add a font file…" (desktop and web), as 1.x's text dialog (`app.js` `sketchText`, `api.js`
//! `addFontFiles` / the `fonts` route). An added font is remembered in settings (`text_fonts`).
//!
//! The fixture font is the bundled Selawik (`assets/fonts/selawk.ttf`), which is not one of the
//! text fonts, so seeing it in the list proves it came from the added file.
#![cfg(not(target_arch = "wasm32"))]

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde_json::Value;
use stl_app::platform::{Pending, PickedFile, Platform};
use stl_app::sketch::text::{self, FontList, FontSource};

/// A platform whose settings live in one JSON file: a new one over the same file is a restart.
struct FilePlatform {
    file: PathBuf,
    web: bool,
    writes: Mutex<usize>,
}

impl FilePlatform {
    fn new(file: &Path, web: bool) -> Self {
        FilePlatform { file: file.to_path_buf(), web, writes: Mutex::new(0) }
    }
    fn all(&self) -> serde_json::Map<String, Value> {
        std::fs::read(&self.file)
            .ok()
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default()
    }
}

impl Platform for FilePlatform {
    fn open_file(&self, _filters: &[(&str, &[&str])]) -> Pending<Option<PickedFile>> {
        Pending::ready(None)
    }
    fn save_file(&self, _suggested: &str, _bytes: Vec<u8>) -> Pending<Result<bool, String>> {
        Pending::ready(Ok(false))
    }
    fn now_iso_utc(&self) -> String {
        String::new()
    }
    fn is_web(&self) -> bool {
        self.web
    }
    fn setting_json(&self, key: &str) -> Option<Value> {
        self.all().get(key).cloned()
    }
    fn set_setting_json(&self, key: &str, value: Value) {
        let mut m = self.all();
        m.insert(key.into(), value);
        std::fs::write(&self.file, serde_json::to_vec(&Value::Object(m)).unwrap()).unwrap();
        *self.writes.lock().unwrap() += 1;
    }
}

fn temp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("stle2-fonts-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn selawik_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/fonts/selawk.ttf")
}

fn selawik() -> Vec<u8> {
    std::fs::read(selawik_path()).unwrap()
}

fn labels(fl: &FontList) -> Vec<String> {
    fl.entries().into_iter().map(|(_, label)| label).collect()
}

#[test]
fn system_fonts_listed() {
    // the folders are searched for .ttf / .otf files and each is listed with its family name
    let dir = temp_dir("system");
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    std::fs::copy(selawik_path(), dir.join("sub").join("selawk.ttf")).unwrap();
    std::fs::write(dir.join("notes.txt"), b"not a font").unwrap();
    let found = text::system_fonts_in(&[dir.clone()]);
    assert_eq!(found, vec![("Selawik".to_string(), dir.join("sub").join("selawk.ttf"))]);

    // desktop: the list starts with the bundled fonts, then offers the computer's fonts
    let p = FilePlatform::new(&dir.join("settings.json"), false);
    let mut fl = FontList::load(&p);
    assert_eq!(fl.families(), vec!["DejaVu Sans", "DejaVu Serif", "DejaVu Sans Mono"]);
    assert_eq!(labels(&fl), vec!["DejaVu Sans", "DejaVu Serif", "DejaVu Sans Mono", "More: fonts on this computer…", "Add a font file…"]);
    assert!(fl.can_ask_local());

    // chosen: they join the list (sorted, after the bundled ones) and the entry goes away
    let pick = fl.set_system(found.clone());
    assert_eq!(pick, None, "no Arial among these: the dialog keeps the default");
    assert_eq!(fl.families(), vec!["DejaVu Sans", "DejaVu Serif", "DejaVu Sans Mono", "Selawik"]);
    assert_eq!(labels(&fl).last().map(String::as_str), Some("Add a font file…"));
    assert!(!fl.can_ask_local());
    assert!(!labels(&fl).iter().any(|l| l == "More: fonts on this computer…"));
    match fl.font_source("Selawik", false, false) {
        Some(FontSource::Path(p)) => assert_eq!(PathBuf::from(p), dir.join("sub").join("selawk.ttf")),
        other => panic!("Selawik's file: {other:?}"),
    }

    // the web edition can't list them: only "Add a font file…"
    let web = FilePlatform::new(&dir.join("web.json"), true);
    let fl = FontList::load(&web);
    assert!(!fl.can_ask_local());
    assert_eq!(labels(&fl), vec!["DejaVu Sans", "DejaVu Serif", "DejaVu Sans Mono", "Add a font file…"]);

    // this computer's own fonts (Windows: %WINDIR%\Fonts has Arial and Segoe UI)
    let real = text::system_fonts();
    #[cfg(windows)]
    assert!(real.iter().any(|(f, _)| f == "Arial" || f == "Segoe UI"), "{} system fonts, no Arial or Segoe UI", real.len());
    assert!(real.iter().all(|(f, p)| !f.is_empty() && p.is_file()));
    let mut fl = FontList::load(&p);
    let pick = fl.set_system(real.clone());
    if real.iter().any(|(f, _)| f == "Arial") {
        assert_eq!(pick.as_deref(), Some("Arial"), "1.x picks Arial when the computer has it");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn font_file_added_and_remembered() {
    let dir = temp_dir("added");
    let settings = dir.join("settings.json");
    let font_file = dir.join("selawk.ttf");
    std::fs::copy(selawik_path(), &font_file).unwrap();

    // the family and style are read from the file
    assert_eq!(text::font_info(&selawik()).unwrap(), ("Selawik".to_string(), String::new()));
    let bold = std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/fonts/selawkb.ttf")).unwrap();
    assert_eq!(text::font_info(&bold).unwrap(), ("Selawik".to_string(), "b".to_string()));

    // desktop: added from a file on disk
    {
        let p = FilePlatform::new(&settings, false);
        let mut fl = FontList::load(&p);
        assert!(!fl.families().iter().any(|f| f == "Selawik"));
        let added = fl.add_file(&p, "selawk.ttf", &selawik(), Some(font_file.to_str().unwrap())).unwrap();
        assert_eq!(added, vec!["Selawik"]);
        assert_eq!(text::added_message(&added), "Added Selawik");
        assert!(fl.families().iter().any(|f| f == "Selawik"));
        // the same file again: still one entry
        fl.add_file(&p, "selawk.ttf", &selawik(), Some(font_file.to_str().unwrap())).unwrap();
        assert_eq!(fl.families().iter().filter(|f| *f == "Selawik").count(), 1);
    }
    // a restart: the font is still in the list, through settings
    {
        let p = FilePlatform::new(&settings, false);
        let saved = p.setting_json("text_fonts").expect("text_fonts setting");
        assert_eq!(saved.as_array().map(Vec::len), Some(1), "{saved}");
        let fl = FontList::load(&p);
        assert_eq!(fl.families(), vec!["DejaVu Sans", "DejaVu Serif", "DejaVu Sans Mono", "Selawik"]);
        assert_eq!(fl.font_source("Selawik", false, false), Some(FontSource::Path(font_file.to_string_lossy().into_owned())));
        assert_eq!(fl.font_bytes("Selawik", true, false).as_deref(), Some(&selawik()[..]), "Bold falls back to Regular");
        assert_eq!(fl.font_bytes("DejaVu Sans", false, false), None, "bundled fonts are the engine's own");
    }

    // web: no path, so the file's bytes are kept
    let web_settings = dir.join("web.json");
    {
        let p = FilePlatform::new(&web_settings, true);
        let mut fl = FontList::load(&p);
        let added = fl.add_file(&p, "selawk.ttf", &selawik(), None).unwrap();
        assert_eq!(added, vec!["Selawik"]);
        fl.add_file(&p, "selawkb.ttf", &bold, None).unwrap();
    }
    {
        let p = FilePlatform::new(&web_settings, true);
        let fl = FontList::load(&p);
        assert!(fl.families().iter().any(|f| f == "Selawik"));
        assert_eq!(fl.font_bytes("Selawik", false, false).as_deref(), Some(&selawik()[..]));
        assert_eq!(fl.font_bytes("Selawik", true, false).as_deref(), Some(&bold[..]));
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn bad_font_file_says_why() {
    let dir = temp_dir("bad");
    let settings = dir.join("settings.json");
    let p = FilePlatform::new(&settings, false);
    let mut fl = FontList::load(&p);
    let why = "That file isn\u{2019}t a font STL Editor can read (use .ttf or .otf).";
    let truncated = selawik()[..200].to_vec();
    for (name, bytes) in [
        ("notes.ttf", b"hello, not a font".to_vec()),
        ("empty.otf", Vec::new()),
        ("cut.ttf", truncated),
        ("model.stl", b"solid x\nendsolid x\n".to_vec()),
    ] {
        assert_eq!(fl.add_file(&p, name, &bytes, None), Err(why.to_string()), "{name}");
        assert_eq!(text::font_info(&bytes).unwrap_err(), why, "{name}");
    }
    // nothing was added or written
    assert_eq!(fl.families(), vec!["DejaVu Sans", "DejaVu Serif", "DejaVu Sans Mono"]);
    assert_eq!(*p.writes.lock().unwrap(), 0);
    assert!(p.setting_json("text_fonts").is_none());

    // a remembered font whose file has gone: left out of the list, the rest still load
    p.set_setting_json(
        "text_fonts",
        serde_json::json!([
            {"family": "Gone", "style": "", "path": dir.join("gone.ttf").to_string_lossy()},
            {"family": 7},
            "junk",
            {"family": "Selawik", "style": "", "path": selawik_path().to_string_lossy()},
        ]),
    );
    let fl = FontList::load(&p);
    assert_eq!(fl.families(), vec!["DejaVu Sans", "DejaVu Serif", "DejaVu Sans Mono", "Selawik"]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn dialog_font_box_adds_a_file() {
    use stl_app::engine_link::Level;
    use stl_app::sketch::text::{TextDialogState, BAD_FONT, FILE};
    use stl_app::sketch::SketchEffect;

    /// [`FilePlatform`] whose file picker hands out one file.
    struct Picker(FilePlatform, Mutex<Option<PickedFile>>);
    impl Platform for Picker {
        fn open_file(&self, filters: &[(&str, &[&str])]) -> Pending<Option<PickedFile>> {
            assert_eq!(filters, &[("Fonts", &["ttf", "otf"][..])]);
            Pending::ready(self.1.lock().unwrap().take())
        }
        fn save_file(&self, s: &str, b: Vec<u8>) -> Pending<Result<bool, String>> {
            self.0.save_file(s, b)
        }
        fn now_iso_utc(&self) -> String {
            String::new()
        }
        fn is_web(&self) -> bool {
            false
        }
        fn setting_json(&self, key: &str) -> Option<Value> {
            self.0.setting_json(key)
        }
        fn set_setting_json(&self, key: &str, value: Value) {
            self.0.set_setting_json(key, value)
        }
    }

    let dir = temp_dir("dialog");
    for (bytes, want_font, want_toast) in [
        (selawik(), "Selawik", (Level::Info, "Added Selawik".to_string())),
        (b"junk".to_vec(), "default", (Level::Warning, BAD_FONT.to_string())),
    ] {
        let want_font = if want_font == "default" { text::default_font_name() } else { want_font.to_string() };
        let p = Picker(FilePlatform::new(&dir.join("s.json"), false), Mutex::new(Some(PickedFile { name: "f.ttf".into(), bytes })));
        let mut state = TextDialogState::default();
        state.form.font = "DejaVu Serif".into();
        assert!(text::choose_font(&mut state, FILE, Some(&p)).is_none());
        let answer = state.font_pick.take().expect("a file picker opened").take().expect("answered");
        let (file, path) = answer.expect("a file");
        match text::font_file_picked(&mut state, &file, path.as_deref(), &p) {
            Some(SketchEffect::Toast(level, msg)) => assert_eq!((level, msg), want_toast),
            _ => panic!("no toast"),
        }
        assert_eq!(state.form.font, want_font);
    }

    // "More: fonts on this computer…" picks Arial when the computer has it
    #[cfg(windows)]
    {
        let p = FilePlatform::new(&dir.join("s.json"), false);
        let mut state = TextDialogState::default();
        text::choose_font(&mut state, text::LOCAL, Some(&p));
        if text::system_fonts().iter().any(|(f, _)| f == "Arial") {
            assert_eq!(state.form.font, "Arial");
        }
        assert!(!state.fonts.as_ref().unwrap().can_ask_local());
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn default_font_is_the_desktop_one() {
    // 1.x `MT.default_font()`: Arial, Segoe UI, DejaVu Sans, Liberation Sans, as the computer has
    let have = |f: &str| text::system_fonts().iter().any(|(x, _)| x == f);
    let want = ["Arial", "Segoe UI", "DejaVu Sans", "Liberation Sans"].into_iter().find(|f| have(f)).unwrap_or("DejaVu Sans");
    assert_eq!(text::default_font_name(), want);
    #[cfg(windows)]
    if have("Arial") {
        assert_eq!(text::default_font_name(), "Arial");
    }
    assert_eq!(text::TextForm::default().font, want);
}

#[test]
fn crafted_font_headers_never_overflow() {
    let why = "That file isn\u{2019}t a font STL Editor can read (use .ttf or .otf).";
    let be32 = |v: u32| v.to_be_bytes().to_vec();
    let mut cases: Vec<(&str, Vec<u8>)> = Vec::new();
    // a collection whose first font starts at the very end of the address space
    cases.push(("ttc base", [b"ttcf".to_vec(), be32(0x0001_0000), be32(1), be32(u32::MAX)].concat()));
    // one table at offset and length u32::MAX
    let mut t = vec![0, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0];
    t.extend(b"name");
    t.extend(be32(0));
    t.extend(be32(u32::MAX));
    t.extend(be32(u32::MAX));
    cases.push(("table offset", t));
    // a name table whose record count and string offset run far past the end
    let mut f = vec![0, 1, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0];
    f.extend(b"glyf");
    f.extend(be32(0));
    f.extend(be32(0));
    f.extend(be32(0));
    f.extend(b"name");
    f.extend(be32(0));
    f.extend(be32(44));
    f.extend(be32(6));
    f.extend([0, 0, 0xff, 0xff, 0xff, 0xff]);
    cases.push(("name records", f));
    // the real font, cut inside its table directory
    cases.push(("cut directory", selawik()[..40].to_vec()));
    for (what, bytes) in cases {
        assert_eq!(text::font_info(&bytes).unwrap_err(), why, "{what}");
    }
}
