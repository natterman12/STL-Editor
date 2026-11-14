//! Files, shortcuts and drag and drop (milestone 4a, task 9).
#![cfg(not(target_arch = "wasm32"))]

// keeps this test binary out of the real 2.0 folders (STLEDITOR2_CONFIG_DIR)
#[path = "support/test_env.rs"]
mod test_env;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui::{Key, Modifiers};
use egui_kittest::Harness;
use stl_app::engine_link::native::NativeLink;
use stl_app::engine_link::{Command, EngineLink, ExportFormat, Level, Update};
use stl_app::platform::{Pending, PickedFile, Platform};
use stl_app::ui::files::{FileAction, Files};
use stl_app::ui::keys::{self, KeyAction, KeyState};
use stl_app::ui::UiEvent;

// ------------------------------------------------------------------ fakes

/// Hands out the queued picks and records every save.
#[derive(Clone, Default)]
struct FakePlatform {
    picks: Arc<Mutex<Vec<Option<PickedFile>>>>,
    saved: Arc<Mutex<Vec<(String, Vec<u8>)>>>,
    filters: Arc<Mutex<Vec<Vec<String>>>>,
    /// Act like the desktop: picked files and dialog saves have paths, `save_to` is silent.
    with_paths: bool,
    /// Silent writes fail.
    fail_writes: bool,
    /// Save dialogs shown (`save_file_path`).
    dialogs: Arc<Mutex<usize>>,
    /// Silent writes (`save_to`): path and bytes.
    silent: Arc<Mutex<Vec<(String, Vec<u8>)>>>,
}

impl Platform for FakePlatform {
    fn open_file_path(&self, filters: &[(&str, &[&str])]) -> Pending<Option<(PickedFile, Option<String>)>> {
        let next = self.open_file(filters).take().flatten();
        let with_paths = self.with_paths;
        Pending::ready(next.map(|f| {
            let path = with_paths.then(|| format!("C:/fake/{}", f.name));
            (f, path)
        }))
    }
    fn save_file_path(&self, suggested: &str, bytes: Vec<u8>) -> Pending<Result<stl_app::platform::SaveOutcome, String>> {
        use stl_app::platform::SaveOutcome;
        *self.dialogs.lock().unwrap() += 1;
        self.saved.lock().unwrap().push((suggested.to_string(), bytes));
        Pending::ready(Ok(SaveOutcome::Done(self.with_paths.then(|| format!("C:/fake/{suggested}")))))
    }
    fn save_to(&self, path: &str, bytes: Vec<u8>) -> Pending<Result<bool, String>> {
        if self.fail_writes {
            return Pending::ready(Err(format!("Couldn't write {path}")));
        }
        self.silent.lock().unwrap().push((path.to_string(), bytes));
        Pending::ready(Ok(true))
    }
    fn open_file(&self, filters: &[(&str, &[&str])]) -> Pending<Option<PickedFile>> {
        self.filters.lock().unwrap().push(filters.iter().flat_map(|(_, e)| e.iter().map(|s| s.to_string())).collect());
        let next = self.picks.lock().unwrap().pop().unwrap_or(None);
        Pending::ready(next)
    }
    fn save_file(&self, suggested: &str, bytes: Vec<u8>) -> Pending<Result<bool, String>> {
        self.saved.lock().unwrap().push((suggested.to_string(), bytes));
        Pending::ready(Ok(true))
    }
    fn now_iso_utc(&self) -> String {
        "2026-11-12T10:00:00".to_string()
    }
    fn is_web(&self) -> bool {
        false
    }
}

/// Remembers every command; never answers.
#[derive(Default)]
struct RecLink {
    sent: Mutex<Vec<Command>>,
}

impl EngineLink for RecLink {
    fn send(&self, c: Command) {
        self.sent.lock().unwrap().push(c);
    }
    fn poll(&self) -> Vec<Update> {
        Vec::new()
    }
    fn restart(&self) {}
}

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/projects")
}

fn fixture(name: &str) -> PickedFile {
    let p = fixture_dir().join(name);
    PickedFile { name: name.to_string(), bytes: std::fs::read(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display())) }
}

/// Runs the link and the file logic until both are idle; returns the toasts.
fn settle(files: &mut Files, platform: &FakePlatform, link: &NativeLink) -> Vec<(Level, String)> {
    let end = Instant::now() + Duration::from_secs(120);
    let mut toasts = Vec::new();
    loop {
        let ups = link.poll();
        toasts.extend(files.poll(platform, link, &ups).messages);
        if !link.is_busy() && !files.busy() {
            let ups = link.poll();
            if ups.is_empty() {
                return toasts;
            }
            toasts.extend(files.poll(platform, link, &ups).messages);
        }
        assert!(Instant::now() < end, "files and engine never went idle");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn python() -> Option<Vec<String>> {
    for cmd in [vec!["py", "-3.14"], vec!["python3"], vec!["python"]] {
        let ok = std::process::Command::new(cmd[0])
            .args(&cmd[1..])
            .args(["-c", "import numpy, trimesh, shapely"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if ok {
            return Some(cmd.iter().map(|s| s.to_string()).collect());
        }
    }
    None
}

// ------------------------------------------------------------------ files

#[test]
fn open_save_round_trip() {
    let out_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/files_keys_out");
    let _ = std::fs::remove_dir_all(&out_dir);
    std::fs::create_dir_all(&out_dir).unwrap();

    let names = ["basic.stlx", "extra_keys.stlx", "unicode.stlx"];
    for name in names {
        let platform = FakePlatform::default();
        platform.picks.lock().unwrap().push(Some(fixture(name)));
        let link = NativeLink::new();
        let mut files = Files::new();

        files.start(&platform, &link, FileAction::Open, None);
        let toasts = settle(&mut files, &platform, &link);
        assert!(toasts.iter().all(|(l, _)| *l != Level::Error), "{name}: {toasts:?}");
        // the open dialog lists the project format and the mesh formats
        let listed = platform.filters.lock().unwrap()[0].clone();
        for ext in ["stlx", "stl", "obj", "3mf", "ply", "off", "glb", "gltf"] {
            assert!(listed.iter().any(|e| e == ext), "open dialog lacks .{ext}");
        }

        files.start(&platform, &link, FileAction::Save, None);
        let toasts = settle(&mut files, &platform, &link);
        let saved = platform.saved.lock().unwrap().clone();
        assert_eq!(saved.len(), 1, "{name}: one file written");
        assert_eq!(saved[0].0, name, "the suggested name is the project's");
        assert!(toasts.iter().any(|(l, t)| *l == Level::Success && t == &format!("Saved {name}")), "{toasts:?}");
        std::fs::write(out_dir.join(name), &saved[0].1).unwrap();
    }

    let Some(py) = python() else {
        eprintln!("open_save_round_trip: no Python with the 1.x packages; the 1.x reader check was skipped");
        return;
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let out = std::process::Command::new(&py[0])
        .args(&py[1..])
        .arg(root.join("dev/parity/open_check.py"))
        .arg(&out_dir)
        .current_dir(&root)
        .output()
        .expect("python runs");
    assert!(
        out.status.success(),
        "1.x could not read what was saved:\n{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

fn open_fixture(platform: &FakePlatform, files: &mut Files, link: &NativeLink, name: &str) {
    platform.picks.lock().unwrap().push(Some(fixture(name)));
    files.start(platform, link, FileAction::Open, None);
    settle(files, platform, link);
}

#[test]
fn save_reuses_path() {
    let platform = FakePlatform { with_paths: true, ..Default::default() };
    let link = NativeLink::new();
    let mut files = Files::new();
    open_fixture(&platform, &mut files, &link, "basic.stlx");

    // Save goes straight back to where the project was opened from: no dialog, twice
    for _ in 0..2 {
        files.start(&platform, &link, FileAction::Save, None);
        let toasts = settle(&mut files, &platform, &link);
        assert!(toasts.iter().any(|(l, t)| *l == Level::Success && t == "Saved basic.stlx"), "{toasts:?}");
    }
    assert_eq!(*platform.dialogs.lock().unwrap(), 0, "no dialog");
    let silent = platform.silent.lock().unwrap().clone();
    assert_eq!(silent.len(), 2);
    assert!(silent.iter().all(|(p, b)| p == "C:/fake/basic.stlx" && !b.is_empty()));

    // a new project has no path yet: the first Save asks, the next one does not
    files.new_document(&link);
    settle(&mut files, &platform, &link);
    files.start(&platform, &link, FileAction::Save, None);
    settle(&mut files, &platform, &link);
    assert_eq!(*platform.dialogs.lock().unwrap(), 1);
    assert_eq!(platform.saved.lock().unwrap().last().unwrap().0, "Untitled.stlx");
    files.start(&platform, &link, FileAction::Save, None);
    settle(&mut files, &platform, &link);
    assert_eq!(*platform.dialogs.lock().unwrap(), 1, "the second Save reuses the path");
    assert_eq!(platform.silent.lock().unwrap().last().unwrap().0, "C:/fake/Untitled.stlx");
}

#[test]
fn save_as_always_asks() {
    let platform = FakePlatform { with_paths: true, ..Default::default() };
    let link = NativeLink::new();
    let mut files = Files::new();
    open_fixture(&platform, &mut files, &link, "basic.stlx");

    for n in 1..=2 {
        files.start(&platform, &link, FileAction::SaveAs, None);
        settle(&mut files, &platform, &link);
        assert_eq!(*platform.dialogs.lock().unwrap(), n, "Save As asks every time");
    }
    assert!(platform.silent.lock().unwrap().is_empty());
    // and a plain Save afterwards goes to the Save As path
    files.start(&platform, &link, FileAction::Save, None);
    settle(&mut files, &platform, &link);
    assert_eq!(*platform.dialogs.lock().unwrap(), 2);
    assert_eq!(platform.silent.lock().unwrap().len(), 1);

    // without paths (the web) every Save is a download
    let web = FakePlatform::default();
    let mut files = Files::new();
    open_fixture(&web, &mut files, &link, "basic.stlx");
    for _ in 0..2 {
        files.start(&web, &link, FileAction::Save, None);
        settle(&mut files, &web, &link);
    }
    assert_eq!(*web.dialogs.lock().unwrap(), 2);
    assert!(web.silent.lock().unwrap().is_empty());
}

/// What the engine sends for an Open it accepted / refused (bodies first, then the error).
fn accepted() -> Vec<Update> {
    vec![Update::Busy(true), Update::Bodies(Vec::new()), Update::Busy(false)]
}
fn refused() -> Vec<Update> {
    vec![Update::Busy(true), Update::Bodies(Vec::new()), Update::Error("bad file".into()), Update::Busy(false)]
}

#[test]
fn failed_open_changes_nothing() {
    let platform = FakePlatform { with_paths: true, ..Default::default() };
    let (link, mut files) = (RecLink::default(), Files::new());
    let pick = |name: &str| platform.picks.lock().unwrap().push(Some(PickedFile { name: name.into(), bytes: vec![1] }));

    pick("good.stlx");
    files.start(&platform, &link, FileAction::Open, None);
    files.poll(&platform, &link, &[]);
    assert!(files.poll(&platform, &link, &accepted()).view_iso);

    // a refused file: no isometric view (the app marks the document clean on it), no Recent
    pick("bad.stlx");
    files.start(&platform, &link, FileAction::Open, None);
    files.poll(&platform, &link, &[]);
    // the bodies and the error may even arrive in separate polls
    let a = files.poll(&platform, &link, &refused()[..2]);
    let b = files.poll(&platform, &link, &refused()[2..]);
    assert!(!a.view_iso && !b.view_iso, "a failed open must not mark the document clean");
    assert_eq!(files.recent_names(), vec!["good.stlx".to_string()]);

    // Save goes back to the good file's path, not the bad one
    files.start(&platform, &link, FileAction::Save, None);
    files.poll(&platform, &link, &[Update::Saved { bytes: vec![2] }]);
    let silent = platform.silent.lock().unwrap().clone();
    assert_eq!(silent.len(), 1);
    assert_eq!(silent[0].0, "C:/fake/good.stlx");
    assert_eq!(files.project_name.as_deref(), Some("good"));
}

#[test]
fn earlier_command_does_not_commit_a_failing_open() {
    let platform = FakePlatform { with_paths: true, ..Default::default() };
    let (link, mut files) = (RecLink::default(), Files::new());
    let pick = |name: &str| platform.picks.lock().unwrap().push(Some(PickedFile { name: name.into(), bytes: vec![1] }));
    pick("good.stlx");
    files.start(&platform, &link, FileAction::Open, None);
    files.poll(&platform, &link, &[]);
    files.poll(&platform, &link, &accepted());

    // a slow command is running when the Open is sent
    files.poll(&platform, &link, &[Update::Busy(true)]);
    pick("bad.stlx");
    files.start(&platform, &link, FileAction::Open, None);
    files.poll(&platform, &link, &[]);
    // the slow command finishes: it must not settle the Open
    let a = files.poll(&platform, &link, &[Update::Bodies(Vec::new()), Update::Busy(false)]);
    assert!(!a.view_iso && files.project_name.as_deref() == Some("good"), "the earlier command's end is not the Open's");
    assert_eq!(files.recent_names(), vec!["good.stlx".to_string()]);
    // then the Open runs and fails
    let b = files.poll(&platform, &link, &refused());
    assert!(!b.view_iso);
    assert_eq!(files.project_name.as_deref(), Some("good"));
    assert_eq!(files.recent_names(), vec!["good.stlx".to_string()]);
}

#[test]
fn failed_write_is_not_in_recent() {
    let platform = FakePlatform { with_paths: true, fail_writes: true, ..Default::default() };
    let (link, mut files) = (RecLink::default(), Files::new());
    platform.picks.lock().unwrap().push(Some(PickedFile { name: "a.stlx".into(), bytes: vec![1] }));
    files.start(&platform, &link, FileAction::Open, None);
    files.poll(&platform, &link, &[]);
    files.poll(&platform, &link, &accepted());
    files.start(&platform, &link, FileAction::Save, None);
    files.poll(&platform, &link, &[Update::Saved { bytes: vec![9] }]);
    files.poll(&platform, &link, &[]);
    // only the opened file is there, never a copy of the unwritten save
    assert_eq!(files.recent_names(), vec!["a.stlx".to_string()]);
}

#[test]
fn failed_write_is_not_saved() {
    let saved_bytes = |p: &FakePlatform, files: &mut Files, link: &RecLink| {
        files.start(p, link, FileAction::Save, None);
        files.poll(p, link, &[Update::Saved { bytes: vec![1] }])
    };
    // a project opened with a path: Save writes silently
    let open = |p: &FakePlatform, files: &mut Files, link: &RecLink| {
        p.picks.lock().unwrap().push(Some(PickedFile { name: "a.stlx".into(), bytes: vec![1] }));
        files.start(p, link, FileAction::Open, None);
        files.poll(p, link, &[]);
        files.poll(p, link, &accepted());
    };

    let bad = FakePlatform { with_paths: true, fail_writes: true, ..Default::default() };
    let (link, mut files) = (RecLink::default(), Files::new());
    open(&bad, &mut files, &link);
    let out = saved_bytes(&bad, &mut files, &link);
    let out2 = files.poll(&bad, &link, &[]);
    assert!(!out.saved && !out2.saved, "a failed write must not mark the document clean");
    let all: Vec<_> = out.messages.iter().chain(&out2.messages).collect();
    assert!(all.iter().any(|(l, _)| *l == Level::Error), "{all:?}");

    let good = FakePlatform { with_paths: true, ..Default::default() };
    let (link, mut files) = (RecLink::default(), Files::new());
    open(&good, &mut files, &link);
    let out = saved_bytes(&good, &mut files, &link);
    let out2 = files.poll(&good, &link, &[]);
    assert!(out.saved || out2.saved);

    // an engine error clears a pending Save As
    let (link, mut files) = (RecLink::default(), Files::new());
    open(&good, &mut files, &link);
    files.start(&good, &link, FileAction::SaveAs, None);
    files.poll(&good, &link, &[Update::Error("x".into())]);
    files.poll(&good, &link, &[Update::Saved { bytes: vec![1] }]);
    assert_eq!(*good.dialogs.lock().unwrap(), 0, "plain Save after an error must not ask");
}

#[test]
fn mesh_open_sets_no_project_path() {
    let platform = FakePlatform { with_paths: true, ..Default::default() };
    let (link, mut files) = (RecLink::default(), Files::new());
    platform.picks.lock().unwrap().push(Some(PickedFile { name: "part.stl".into(), bytes: vec![1, 2, 3] }));
    files.start(&platform, &link, FileAction::Open, None);
    files.poll(&platform, &link, &[]);
    files.start(&platform, &link, FileAction::Save, None);
    files.poll(&platform, &link, &[Update::Saved { bytes: vec![1] }]);
    assert!(platform.silent.lock().unwrap().is_empty(), "a mesh path is not where the project goes");
    assert_eq!(*platform.dialogs.lock().unwrap(), 1);
}

#[test]
fn bare_save_as_name_gets_stlx() {
    use stl_app::platform::native::with_project_ext;
    assert_eq!(with_project_ext(PathBuf::from("C:/x/foo")), PathBuf::from("C:/x/foo.stlx"));
    assert_eq!(with_project_ext(PathBuf::from("C:/x/foo.v2")), PathBuf::from("C:/x/foo.v2.stlx"));
    assert_eq!(with_project_ext(PathBuf::from("C:/x/foo.STLX")), PathBuf::from("C:/x/foo.STLX"));
}

#[test]
fn import_adds_a_body_and_refuses_projects() {
    let platform = FakePlatform::default();
    let link = RecLink::default();
    let mut files = Files::new();
    platform.picks.lock().unwrap().push(Some(PickedFile { name: "part.stl".into(), bytes: vec![1, 2, 3] }));
    files.start(&platform, &link, FileAction::Import, None);
    let out = files.poll(&platform, &link, &[]);
    assert!(out.messages.is_empty());
    assert!(matches!(&link.sent.lock().unwrap()[..], [Command::Import { name, .. }] if name == "part.stl"));
    // the view is fitted once the bodies arrive
    let out = files.poll(&platform, &link, &[Update::Bodies(Vec::new())]);
    assert!(out.fit && !out.view_iso);

    // a cancelled dialog does nothing
    files.start(&platform, &link, FileAction::Import, None);
    files.poll(&platform, &link, &[]);
    assert_eq!(link.sent.lock().unwrap().len(), 1);
}

#[test]
fn export_each_format() {
    let platform = FakePlatform::default();
    platform.picks.lock().unwrap().push(Some(fixture("basic.stlx")));
    let link = NativeLink::new();
    let mut files = Files::new();
    files.start(&platform, &link, FileAction::Open, None);
    settle(&mut files, &platform, &link);

    for (format, ext) in [(ExportFormat::Stl, "stl"), (ExportFormat::ThreeMf, "3mf"), (ExportFormat::Obj, "obj"), (ExportFormat::Step, "step")] {
        let before = platform.saved.lock().unwrap().len();
        files.start(&platform, &link, FileAction::Export(format), None);
        let toasts = settle(&mut files, &platform, &link);
        let saved = platform.saved.lock().unwrap().clone();
        assert_eq!(saved.len(), before + 1, "{ext}: one file offered");
        let (name, bytes) = saved.last().unwrap();
        assert!(name.ends_with(&format!(".{ext}")), "{ext}: suggested name {name}");
        assert!(!name.starts_with('.') && name.len() > ext.len() + 1, "{ext}: the name has a base ({name})");
        assert!(!bytes.is_empty(), "{ext}: empty file");
        match ext {
            "3mf" => assert_eq!(&bytes[..2], b"PK", "a zip"),
            "obj" => assert!(String::from_utf8_lossy(bytes).contains("\nv ") || String::from_utf8_lossy(bytes).starts_with("v ")),
            "step" => assert!(String::from_utf8_lossy(bytes).contains("ISO-10303-21")),
            _ => {}
        }
        assert!(
            toasts.iter().any(|(l, t)| *l == Level::Success && t.starts_with("Exported ") && t.ends_with(name.as_str())),
            "{ext}: {toasts:?}"
        );
    }
}

#[test]
fn export_passes_ids_and_date() {
    let platform = FakePlatform::default();
    let link = RecLink::default();
    let mut files = Files::new();
    files.start(&platform, &link, FileAction::Export(ExportFormat::Obj), Some(vec!["b1".into()]));
    match &link.sent.lock().unwrap()[..] {
        [Command::Export { format: ExportFormat::Obj, ids: Some(ids), date }] => {
            assert_eq!(ids, &vec!["b1".to_string()]);
            assert_eq!(date, "2026-11-12T10:00:00");
        }
        other => panic!("{other:?}"),
    }
}

// ------------------------------------------------------------------ drag and drop

#[test]
fn drop_opens_file() {
    let platform = FakePlatform::default();
    let link = RecLink::default();
    let files = Arc::new(Mutex::new(Files::new()));
    let outcome = Arc::new(Mutex::new(Vec::new()));

    // a real file on disk, as the desktop window delivers it (a path, no bytes)
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/files_keys_drop");
    std::fs::create_dir_all(&dir).unwrap();
    let stl = dir.join("dropped.stl");
    std::fs::write(&stl, b"solid x\nendsolid x\n").unwrap();
    let stlx = dir.join("dropped.stlx");
    std::fs::write(&stlx, b"not really").unwrap();

    let (f2, o2) = (files.clone(), outcome.clone());
    let link = Arc::new(link);
    let l2 = link.clone();
    let mut harness = Harness::new_ui(move |ui| {
        let out = f2.lock().unwrap().handle_drop(ui.ctx(), &*l2);
        o2.lock().unwrap().extend(out.messages);
        ui.label("window");
    });

    // while a file is dragged over, the overlay is up
    harness.input_mut().hovered_files.push(egui::HoveredFile { path: Some(stl.clone()), mime: String::new() });
    harness.run_steps(2);
    assert!(files.lock().unwrap().dragging, "the drop overlay shows while dragging");
    harness.input_mut().hovered_files.clear();
    harness.run_steps(2);
    assert!(!files.lock().unwrap().dragging);
    let _ = &platform;

    // dropping a mesh inserts it
    harness.input_mut().dropped_files.push(Arc::new(PathFile(stl.clone())));
    harness.run();
    {
        let sent = link.sent.lock().unwrap();
        assert!(matches!(&sent[..], [Command::Import { name, bytes }] if name == "dropped.stl" && bytes.starts_with(b"solid")), "{sent:?}");
    }

    // dropping a project is refused with the 1.x hint
    harness.input_mut().dropped_files.push(Arc::new(PathFile(stlx)));
    harness.run();
    assert_eq!(link.sent.lock().unwrap().len(), 1, "no second command");
    assert!(outcome.lock().unwrap().iter().any(|(l, t)| *l == Level::Warning && t == "Use File › Open for projects."));
}

#[derive(Debug)]
struct PathFile(PathBuf);

impl egui::DroppedFile for PathFile {
    fn path(&self) -> &Path {
        &self.0
    }
    fn bytes(&self) -> Result<Vec<u8>, String> {
        std::fs::read(&self.0).map_err(|e| e.to_string())
    }
}

// ------------------------------------------------------------------ keys

fn none() -> Modifiers {
    Modifiers::NONE
}
fn ctrl() -> Modifiers {
    Modifiers::CTRL
}
fn ctrl_shift() -> Modifiers {
    Modifiers { ctrl: true, shift: true, ..Modifiers::NONE }
}
fn shift() -> Modifiers {
    Modifiers::SHIFT
}

/// Press `key` in a real egui frame and return what `keys::handle` made of it.
/// `field` puts focus in a text box first.
fn press(key: Key, mods: Modifiers, state: KeyState, field: bool) -> Vec<KeyAction> {
    let got: Arc<Mutex<Vec<KeyAction>>> = Arc::new(Mutex::new(Vec::new()));
    let g = got.clone();
    let mut text = String::new();
    let mut harness = Harness::builder().with_accessibility_check(false).build_ui(move |ui| {
        let r = ui.text_edit_singleline(&mut text);
        if field && !r.has_focus() {
            r.request_focus();
        }
        if !field {
            r.surrender_focus();
        }
        g.lock().unwrap().extend(keys::handle(ui.ctx(), &state));
    });
    harness.run();
    harness.key_press_modifiers(mods, key);
    harness.run();
    let v = got.lock().unwrap().clone();
    v
}

fn st() -> KeyState {
    KeyState::default()
}

#[test]
fn every_onkey_binding() {
    use KeyAction as A;
    let tool = KeyState { tool_open: true, ..st() };
    let sel = KeyState { selection_nonempty: true, ..st() };
    let gizmo = KeyState { tool_open: true, tool_gizmo_move: true, ..st() };
    let mode = KeyState { mode_not_model: true, ..st() };
    let sk = KeyState { sketcher_active: true, ..st() };
    let walls = KeyState { walls_active: true, walls_pending: true, ..st() };

    // (name, key, modifiers, state, expected actions)
    let table: Vec<(&str, Key, Modifiers, KeyState, Vec<KeyAction>)> = vec![
        ("ctrl+s", Key::S, ctrl(), st(), vec![A::Save]),
        ("ctrl+shift+s", Key::S, ctrl_shift(), st(), vec![A::SaveAs]),
        ("ctrl+o", Key::O, ctrl(), st(), vec![A::Open]),
        ("ctrl+i", Key::I, ctrl(), st(), vec![A::Import]),
        ("ctrl+n", Key::N, ctrl(), st(), vec![A::New]),
        ("ctrl+k", Key::K, ctrl(), st(), vec![A::FocusSearch]),
        ("slash", Key::Slash, none(), st(), vec![A::FocusSearch]),
        ("ctrl+z", Key::Z, ctrl(), st(), vec![A::Undo]),
        ("ctrl+y", Key::Y, ctrl(), st(), vec![A::Redo]),
        // as in 1.x, Ctrl+Z is tested before Shift, so Ctrl+Shift+Z undoes
        ("ctrl+shift+z", Key::Z, ctrl_shift(), st(), vec![A::Undo]),
        ("escape with a tool", Key::Escape, none(), tool, vec![A::PanelCancel]),
        ("escape in a mode", Key::Escape, none(), mode, vec![A::PanelCancel]),
        ("escape with nothing open", Key::Escape, none(), st(), vec![A::ClearSelection]),
        ("enter with a tool", Key::Enter, none(), tool, vec![A::PanelOk]),
        ("enter with no tool", Key::Enter, none(), st(), vec![]),
        ("delete with a selection", Key::Delete, none(), sel, vec![A::Tool("delete")]),
        ("delete with nothing selected", Key::Delete, none(), st(), vec![]),
        ("delete with a tool open", Key::Delete, none(), KeyState { tool_open: true, selection_nonempty: true, ..st() }, vec![]),
        ("s", Key::S, none(), st(), vec![A::BeginSketch]),
        ("shift+s", Key::S, shift(), st(), vec![A::BeginSketch]),
        ("e", Key::E, none(), st(), vec![A::Tool("extrude")]),
        ("h", Key::H, none(), st(), vec![A::Tool("hole")]),
        ("m", Key::M, none(), st(), vec![A::Tool("move")]),
        ("f", Key::F, none(), st(), vec![A::Fit]),
        ("x", Key::X, none(), st(), vec![A::ToggleCut]),
        ("w with the move gizmo", Key::W, none(), gizmo, vec![A::Gizmo { rotate: false }]),
        ("r with the move gizmo", Key::R, none(), gizmo, vec![A::Gizmo { rotate: true }]),
        ("w without it", Key::W, none(), st(), vec![]),
        ("1 front", Key::Num1, none(), st(), vec![A::View("front")]),
        ("2 back", Key::Num2, none(), st(), vec![A::View("back")]),
        ("3 left", Key::Num3, none(), st(), vec![A::View("left")]),
        ("4 right", Key::Num4, none(), st(), vec![A::View("right")]),
        ("5 top", Key::Num5, none(), st(), vec![A::View("top")]),
        ("6 bottom", Key::Num6, none(), st(), vec![A::View("bottom")]),
        ("0 iso", Key::Num0, none(), st(), vec![A::View("iso")]),
        ("shift+1 is not a view key", Key::Num1, shift(), st(), vec![]),
        ("ctrl+e is not extrude", Key::E, ctrl(), st(), vec![]),
        ("walls: enter finishes", Key::Enter, none(), walls, vec![A::Walls(keys::KeyPress { key: Key::Enter, mods: none() })]),
        ("walls: backspace undoes a point", Key::Backspace, none(), walls, vec![A::Walls(keys::KeyPress { key: Key::Backspace, mods: none() })]),
        ("sketcher gets plain keys", Key::L, none(), sk, vec![A::Sketcher(keys::KeyPress { key: Key::L, mods: none() })]),
        ("sketcher still gets file keys first", Key::S, ctrl(), sk, vec![A::Save]),
    ];
    for (name, key, mods, state, want) in table {
        let got = press(key, mods, state, false);
        // the helper's own text box gets the same key; only compare what the table expects
        assert_eq!(got, want, "{name}");
    }
}

#[test]
fn keys_while_a_text_field_has_focus() {
    use KeyAction as A;
    // letters, delete, escape, slash and undo belong to the box
    for (name, key, mods) in [
        ("e", Key::E, none()),
        ("f", Key::F, none()),
        ("1", Key::Num1, none()),
        ("slash", Key::Slash, none()),
        ("delete", Key::Delete, none()),
        ("escape", Key::Escape, none()),
        ("ctrl+z", Key::Z, ctrl()),
        ("ctrl+y", Key::Y, ctrl()),
    ] {
        let state = KeyState { selection_nonempty: true, tool_open: true, ..st() };
        assert!(press(key, mods, state, true).is_empty(), "{name} must be left to the text box");
    }
    // the file keys and the tool search still work
    assert_eq!(press(Key::S, ctrl(), st(), true), vec![A::Save]);
    assert_eq!(press(Key::O, ctrl(), st(), true), vec![A::Open]);
    assert_eq!(press(Key::K, ctrl(), st(), true), vec![A::FocusSearch]);
    // the sketcher's own boxes pass keys through, other boxes do not
    let sk = KeyState { sketcher_active: true, ..st() };
    assert!(press(Key::L, none(), sk, true).is_empty());
    let sk_dyn = KeyState { sketcher_active: true, dyn_field_focused: true, ..st() };
    assert_eq!(press(Key::Enter, none(), sk_dyn, true), vec![A::Sketcher(keys::KeyPress { key: Key::Enter, mods: none() })]);
}

#[test]
fn actions_become_commands_and_events() {
    assert!(matches!(KeyAction::Undo.command(), Some(Command::Undo)));
    assert!(matches!(KeyAction::Redo.command(), Some(Command::Redo)));
    assert_eq!(KeyAction::Tool("hole").event(), Some(UiEvent::OpenTool("hole".into())));
    assert_eq!(KeyAction::ClearSelection.event(), Some(UiEvent::Select(vec![])));
    assert_eq!(KeyAction::Save.file_action(), Some(FileAction::Save));
    assert_eq!(KeyAction::SaveAs.file_action(), Some(FileAction::SaveAs));
}
