//! Autosave through the engine worker, and the Recover Unsaved Work dialog.
//! Every test points `STLEDITOR2_CONFIG_DIR` at a throwaway folder first.
#![cfg(not(target_arch = "wasm32"))]

// keeps this test binary out of the real 2.0 folders (STLEDITOR2_CONFIG_DIR)
#[path = "support/test_env.rs"]
mod test_env;

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use serde_json::{json, Value};
use stl_app::engine_link::native::NativeLink;
use stl_app::engine_link::worker::Worker;
use stl_app::engine_link::{Command, EngineLink, TimelineView, Update};
use stl_app::platform::Pending;
use stl_app::recover::{self, Recover, RecoverAction, RecoverEntry, RecoverStore};
use stl_app::theme;

/// A fresh empty folder for one test below the throwaway config folder (which is also where
/// `data_dir()` goes, so nothing here can reach the real ones).
fn sandbox(test: &str) -> PathBuf {
    static BASE: OnceLock<PathBuf> = OnceLock::new();
    let base = BASE.get_or_init(|| {
        let d = std::env::temp_dir().join(format!("stle2-recover-test-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        // SAFETY: set once before any test reads it; the tests only ever set it to this folder.
        unsafe { std::env::set_var("STLEDITOR2_CONFIG_DIR", &d) };
        d
    });
    assert_eq!(stl_platform::paths::config_dir(), *base, "the tests must not touch the real folders");
    let r = base.join(test);
    let _ = std::fs::remove_dir_all(&r);
    std::fs::create_dir_all(&r).unwrap();
    r
}

fn fixture(name: &str) -> Vec<u8> {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/projects").join(name);
    std::fs::read(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

type Log = Arc<Mutex<Vec<Update>>>;

/// A worker whose updates land in a list, writing its autosave copies under `root`.
fn worker(root: &Path) -> (Worker, Log) {
    let log: Log = Arc::new(Mutex::new(Vec::new()));
    let sink = log.clone();
    let mut w = Worker::new(Arc::new(move |u| sink.lock().unwrap().push(u)), Arc::new(AtomicBool::new(false)));
    w.set_autosave_root(root.to_path_buf());
    (w, log)
}

fn take(log: &Log) -> Vec<Update> {
    std::mem::take(&mut *log.lock().unwrap())
}

fn timeline(ups: &[Update]) -> TimelineView {
    ups.iter().rev().find_map(|u| if let Update::Timeline(t) = u { Some(t.clone()) } else { None }).expect("a timeline update")
}

fn autosaved(ups: &[Update]) -> Vec<(bool, Option<String>)> {
    ups.iter().filter_map(|u| if let Update::Autosaved { ok, error } = u { Some((*ok, error.clone())) } else { None }).collect()
}

fn autosave() -> Command {
    Command::Autosave { path: None, title: None }
}

fn box_feature() -> Command {
    Command::AddFeature { kind: "primitive".into(), params: json!({"shape": "box", "size": [4.0, 4.0, 4.0]}), index: None, name: None }
}

fn copy_of(root: &Path, id: &str) -> Value {
    serde_json::from_slice(&std::fs::read(root.join(id).join("document.json")).unwrap()).unwrap()
}

#[test]
fn writes_only_when_changed() {
    let root = sandbox("writes_only_when_changed");
    let (mut w, log) = worker(&root);
    // a project that was just opened has nothing unsaved
    w.handle(Command::Open { name: "basic.stlx".into(), bytes: fixture("basic.stlx") });
    take(&log);
    w.handle(autosave());
    assert!(autosaved(&take(&log)).is_empty(), "nothing changed: nothing written, nothing said");
    assert!(w.autosave_id().is_none() && std::fs::read_dir(&root).unwrap().next().is_none(), "no folder until there is something to keep");

    w.handle(box_feature());
    take(&log);
    w.handle(autosave());
    let ups = take(&log);
    assert_eq!(autosaved(&ups), [(true, None)]);
    assert!(!ups.iter().any(|u| matches!(u, Update::Busy(_) | Update::Bodies(_))), "an autosave is quiet: no busy overlay, no redraw");
    let id = w.autosave_id().expect("a session folder now");
    let first = copy_of(&root, &id);
    assert_eq!(first["count"], 4);
    assert_eq!(first["title"], "basic");
    assert_eq!(first["path"], Value::Null);
    let stamp = std::fs::metadata(root.join(&id).join("document.json")).unwrap().modified().unwrap();

    // asked again with no change: not written again
    std::thread::sleep(Duration::from_millis(30));
    w.handle(autosave());
    assert!(autosaved(&take(&log)).is_empty());
    assert_eq!(std::fs::metadata(root.join(&id).join("document.json")).unwrap().modified().unwrap(), stamp);

    // a change writes again, into the same folder
    w.handle(box_feature());
    w.handle(autosave());
    take(&log);
    assert_eq!(w.autosave_id().as_deref(), Some(id.as_str()));
    assert_eq!(copy_of(&root, &id)["count"], 5);
}

#[test]
fn the_copy_records_the_path_and_title() {
    let root = sandbox("the_copy_records_the_path_and_title");
    let (mut w, log) = worker(&root);
    w.handle(Command::Open { name: "basic.stlx".into(), bytes: fixture("basic.stlx") });
    w.handle(box_feature());
    take(&log);
    w.handle(Command::Autosave { path: Some(r"C:\parts\bracket.stlx".into()), title: Some("bracket".into()) });
    let id = w.autosave_id().unwrap();
    let copy = copy_of(&root, &id);
    assert_eq!((copy["path"].as_str(), copy["title"].as_str()), (Some(r"C:\parts\bracket.stlx"), Some("bracket")));
    // the Recover list shows them
    let listed = stl_platform::autosave::load(&root, &id).map(|(d, _)| d).unwrap();
    assert_eq!(listed["path"], r"C:\parts\bracket.stlx");
}

#[test]
fn saving_or_new_clears_the_copy() {
    let root = sandbox("saving_or_new_clears_the_copy");
    let (mut w, log) = worker(&root);
    w.handle(Command::Open { name: "basic.stlx".into(), bytes: fixture("basic.stlx") });
    w.handle(box_feature());
    w.handle(autosave());
    let id = w.autosave_id().unwrap();
    assert!(root.join(&id).join("document.json").is_file());

    w.handle(Command::Save);
    assert!(!root.join(&id).join("document.json").exists(), "saved: nothing left to recover");
    assert!(!root.join(&id).join("sources").exists());
    assert!(root.join(&id).join("lock").is_file(), "the window's session goes on");

    // changed again, then New (the app opens an empty project)
    w.handle(box_feature());
    w.handle(autosave());
    assert!(root.join(&id).join("document.json").is_file());
    let empty = stl_engine::stlx::to_bytes(&stl_engine::Document::new(stl_engine::Registry::builtin())).unwrap();
    w.handle(Command::Open { name: "Untitled.stlx".into(), bytes: empty });
    assert!(!root.join(&id).join("document.json").exists(), "New: the old project's copy is gone");
    take(&log);
    w.handle(autosave());
    assert!(autosaved(&take(&log)).is_empty(), "an empty project is not worth keeping");
}

#[test]
fn recover_rebuilds_timeline() {
    let root = sandbox("recover_rebuilds_timeline");
    let (mut w, log) = worker(&root);
    w.handle(Command::Open { name: "basic.stlx".into(), bytes: fixture("basic.stlx") });
    w.handle(box_feature());
    w.handle(Command::Rollback(Some(3))); // the rollback marker is somewhere in the middle
    let before = timeline(&take(&log));
    assert_eq!(before.marker, Some(3));
    w.handle(autosave());
    take(&log);
    let id = w.autosave_id().unwrap();

    // the window crashed; the next start rebuilds the project from the folder (the folder is read
    // through the same function the Recover dialog uses)
    let bytes = recover::project_bytes(&root, &id).unwrap();
    let (mut w2, log2) = worker(&sandbox("recover_rebuilds_timeline_2"));
    w2.handle(Command::Open { name: "basic.stlx".into(), bytes });
    let ups = take(&log2);
    let after = timeline(&ups);
    assert_eq!(after.marker, before.marker, "the rollback marker is where it was");
    assert_eq!(after.items.len(), before.items.len());
    for (a, b) in after.items.iter().zip(&before.items) {
        assert_eq!((&a.id, &a.kind, &a.params, a.active), (&b.id, &b.kind, &b.params, b.active), "step {} comes back as it was", b.id);
    }
    assert!(ups.iter().any(|u| matches!(u, Update::Params(p) if p.iter().any(|q| q.name == "dx" && q.value == Some(5.0)))), "the parameters come back");
    // it is unsaved work: a new copy was written straight away (as 1.x does), and a later save
    // writes a clean project file
    assert_eq!(autosaved(&ups), [(true, None)], "recovered work is kept again at once");
    w2.handle(Command::Save);
    let saved = take(&log2).into_iter().find_map(|u| if let Update::Saved { bytes } = u { Some(bytes) } else { None }).unwrap();
    let doc = stl_engine::stlx::from_bytes(&saved, stl_engine::Registry::builtin()).unwrap();
    for stale in ["recovered", "title", "path", "saved", "count"] {
        assert!(!doc.extra.contains_key(stale), "{stale} must not end up in the saved project");
    }
}

#[test]
fn sources_kept() {
    let root = sandbox("sources_kept");
    let (mut w, log) = worker(&root);
    let bytes: Vec<u8> = fixture("basic.stlx");
    w.handle(Command::Open { name: "basic.stlx".into(), bytes });
    w.handle(box_feature());
    take(&log);
    w.handle(autosave());
    let id = w.autosave_id().unwrap();
    // both imported meshes are in the folder, as the project holds them
    let z = stl_engine::stlx::from_bytes(&fixture("basic.stlx"), stl_engine::Registry::builtin()).unwrap();
    assert_eq!(z.sources.len(), 2);
    for (sid, s) in &z.sources {
        let file = root.join(&id).join("sources").join(format!("{sid}{}", s.ext));
        assert_eq!(std::fs::read(&file).unwrap(), s.data, "{sid} comes back byte for byte");
    }
}

#[test]
fn clean_exit_removes_folder() {
    let root = sandbox("clean_exit_removes_folder_native");
    let link = NativeLink::new();
    link.set_autosave_root(root.clone());
    link.send(Command::Open { name: "basic.stlx".into(), bytes: fixture("basic.stlx") });
    link.send(box_feature());
    link.send(autosave());
    wait_idle(&link);
    assert!(link.poll().iter().any(|u| matches!(u, Update::Autosaved { ok: true, .. })));
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
    // unsaved work: ending the session keeps the copy
    link.end_session(Duration::from_secs(10));
    assert!(root.read_dir().unwrap().flatten().all(|e| e.path().join("document.json").is_file()), "the copy stays");
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
    // saved work: nothing to keep
    let root2 = sandbox("clean_exit_removes_folder_saved");
    let link = NativeLink::new();
    link.set_autosave_root(root2.clone());
    link.send(Command::Open { name: "basic.stlx".into(), bytes: fixture("basic.stlx") });
    link.send(box_feature());
    link.send(autosave());
    link.send(Command::Save);
    wait_idle(&link);
    assert_eq!(std::fs::read_dir(&root2).unwrap().count(), 1);
    link.end_session(Duration::from_secs(10));
    assert_eq!(std::fs::read_dir(&root2).unwrap().count(), 0, "a clean exit removes this window's folder");
}

#[test]
fn closing_the_app_ends_the_session() {
    let root = sandbox("closing_the_app_ends_the_session");
    let link = NativeLink::new();
    link.set_autosave_root(root.clone());
    link.send(Command::Open { name: "basic.stlx".into(), bytes: fixture("basic.stlx") });
    link.send(box_feature());
    link.send(autosave());
    wait_idle(&link);
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
    // saved: nothing is unsaved, so the window's folder goes with it
    link.send(Command::Save);
    link.send(autosave());
    wait_idle(&link);
    drop(link); // what happens when the window closes
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0, "a saved project leaves nothing behind");
}

#[test]
fn closing_with_unsaved_work_keeps_it_for_recover() {
    let root = sandbox("closing_with_unsaved_work_keeps_it_for_recover");
    let link = NativeLink::new();
    link.set_autosave_root(root.clone());
    link.send(Command::Open { name: "basic.stlx".into(), bytes: fixture("basic.stlx") });
    link.send(box_feature());
    link.send(Command::Autosave { path: Some(r"C:\parts\basic.stlx".into()), title: Some("basic".into()) });
    wait_idle(&link);
    // a change after the last autosave is written too when the window closes
    link.send(box_feature());
    wait_idle(&link);
    drop(link);
    let found = stl_platform::autosave::recoverable(&root, None);
    assert_eq!(found.len(), 1, "unsaved work stays for the next start: {found:?}");
    assert_eq!((found[0].title.as_str(), found[0].steps, found[0].path.as_deref()), ("basic", 5, Some(r"C:\parts\basic.stlx")));
}

fn wait_idle(link: &NativeLink) {
    let end = Instant::now() + Duration::from_secs(120);
    while link.is_busy() {
        assert!(Instant::now() < end, "the engine never went idle");
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// A binary STL of a UV sphere with about `n * n * 2` triangles.
fn sphere_stl(n: usize) -> Vec<u8> {
    let p = |i: usize, j: usize| {
        let (a, b) = (std::f32::consts::PI * i as f32 / n as f32, 2.0 * std::f32::consts::PI * j as f32 / n as f32);
        [20.0 * a.sin() * b.cos(), 20.0 * a.sin() * b.sin(), 20.0 * a.cos()]
    };
    let mut tris: Vec<[[f32; 3]; 3]> = Vec::new();
    for i in 0..n {
        for j in 0..n {
            let (a, b, c, d) = (p(i, j), p(i + 1, j), p(i + 1, j + 1), p(i, j + 1));
            if i > 0 {
                tris.push([a, b, d]);
            }
            if i + 1 < n {
                tris.push([b, c, d]);
            }
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
    out
}

#[test]
fn autosave_off_ui_thread() {
    let root = sandbox("autosave_off_ui_thread");
    let link = NativeLink::new();
    link.set_autosave_root(root.clone());
    link.send(Command::Import { name: "ball.stl".into(), bytes: sphere_stl(400) });
    wait_idle(&link);
    link.poll();
    link.send(box_feature());
    wait_idle(&link);
    link.poll();

    // The UI's side of an autosave is a queue push: time it, and the polls around it, while the
    // engine thread writes the copy (a big imported mesh and the project).
    let mut worst = Duration::ZERO;
    let timed = |worst: &mut Duration, f: &mut dyn FnMut()| {
        let t = Instant::now();
        f();
        *worst = (*worst).max(t.elapsed());
    };
    let started = Instant::now();
    timed(&mut worst, &mut || link.send(autosave()));
    assert!(link.is_busy(), "send returned while the engine thread was still writing the copy");
    let mut done = false;
    let mut polls = 0;
    while !done {
        timed(&mut worst, &mut || {
            done = link.poll().iter().any(|u| matches!(u, Update::Autosaved { ok: true, .. }));
        });
        polls += 1;
        std::thread::sleep(Duration::from_millis(1));
        assert!(started.elapsed() < Duration::from_secs(60), "the autosave never finished");
    }
    // changed again: a second write while the UI keeps polling
    link.send(box_feature());
    timed(&mut worst, &mut || link.send(autosave()));
    wait_idle(&link);
    timed(&mut worst, &mut || {
        link.poll();
    });
    println!("autosave took {:?}; slowest UI-side call {:?} over {polls} polls", started.elapsed(), worst);
    assert!(worst < Duration::from_micros(16_700), "a UI-side call took {worst:?}, over one frame (16.7 ms)");
    let sessions: Vec<_> = std::fs::read_dir(&root).unwrap().flatten().collect();
    assert_eq!(sessions.len(), 1);
    assert!(sessions[0].path().join("document.json").is_file());
    link.end_session(Duration::from_secs(10));
}

// ------------------------------------------------------------------------ the dialog

/// A store with a fixed list.
struct FakeStore {
    list: Vec<RecoverEntry>,
    log: Arc<Mutex<Vec<String>>>,
}

impl RecoverStore for FakeStore {
    fn list(&self) -> Pending<Vec<RecoverEntry>> {
        Pending::ready(self.list.clone())
    }
    fn load(&self, id: &str) -> Pending<Result<Vec<u8>, String>> {
        self.log.lock().unwrap().push(format!("load {id}"));
        Pending::ready(Ok(vec![1, 2, 3]))
    }
    fn discard(&self, id: &str) {
        self.log.lock().unwrap().push(format!("discard {id}"));
    }
}

fn entry(id: &str, title: &str, steps: usize) -> RecoverEntry {
    RecoverEntry { id: id.into(), title: title.into(), path: Some(format!("C:\\parts\\{title}.stlx")), saved: 1_700_000_000.0, steps }
}

struct Ui {
    recover: Recover,
    dirty: bool,
    actions: Vec<String>,
}

fn dialog(list: Vec<RecoverEntry>, web: bool, manual: bool, dirty: bool) -> (Harness<'static, Ui>, Arc<Mutex<Vec<String>>>) {
    let log = Arc::new(Mutex::new(Vec::new()));
    let mut recover = Recover::new(Box::new(FakeStore { list, log: log.clone() }), web);
    if manual {
        recover.open_manual();
    } else {
        recover.start();
    }
    let mut fonts = false;
    let h = Harness::builder().with_size(egui::vec2(800.0, 600.0)).with_accessibility_check(false).build_ui_state(
        move |ui, st: &mut Ui| {
            if !fonts {
                fonts = true;
                stl_app::fonts::install(ui.ctx());
            }
            let tk = theme::tokens(false);
            theme::apply(ui.ctx(), &tk);
            match st.recover.show(ui.ctx(), &tk, st.dirty) {
                Some(RecoverAction::Open(r)) => st.actions.push(format!("open {} {:?} {}", r.name, r.path, r.bytes.len())),
                Some(RecoverAction::Message { text, warn }) => st.actions.push(format!("message {text} {warn}")),
                None => {}
            }
        },
        Ui { recover, dirty, actions: Vec::new() },
    );
    (h, log)
}

#[test]
fn dialog_texts_as_1x() {
    let (mut h, _) = dialog(vec![entry("a", "Bracket", 3), entry("b", "Lid", 1)], false, false, false);
    h.run();
    h.get_by_label("Recover unsaved work?");
    h.get_by_label("STL Editor didn’t close normally last time. This work was saved automatically:");
    h.get_by_label("Bracket");
    h.get_by_label_contains("3 steps · autosaved 20");
    h.get_by_label_contains("1 step · autosaved 20");
    h.get_by_label("Later");
    let (mut w, _) = dialog(vec![entry("a", "Bracket", 3)], true, false, false);
    w.run();
    w.get_by_label("This work wasn’t saved before STL Editor was last closed. A copy was kept in this browser:");
}

#[test]
fn dialog_discard_and_recover() {
    let (mut h, log) = dialog(vec![entry("a", "Bracket", 3), entry("b", "Lid", 1)], false, false, false);
    h.run();
    // Discard the first copy: it is removed and the other stays
    h.get_all_by_label("Discard").next().unwrap().click();
    h.run();
    assert_eq!(*log.lock().unwrap(), ["discard a"]);
    assert!(h.query_by_label("Bracket").is_none());
    h.get_by_label("Lid");
    // Recover the other: the project comes back for the app to open
    h.get_by_label("Recover").click();
    h.run();
    h.run();
    assert_eq!(*log.lock().unwrap(), ["discard a", "load b"]);
    assert_eq!(h.state().actions, ["open Lid.stlx Some(\"C:\\\\parts\\\\Lid.stlx\") 3"]);
    assert!(!h.state().recover.is_open());
}

#[test]
fn dialog_asks_before_replacing_unsaved_work() {
    let (mut h, log) = dialog(vec![entry("a", "Bracket", 3)], false, false, true);
    h.run();
    h.get_by_label("Recover").click();
    h.run();
    h.get_by_label("Replace the current unsaved work with the recovered project?");
    assert!(log.lock().unwrap().is_empty());
    h.get_by_label("Cancel").click();
    h.run();
    h.get_by_label("Recover unsaved work?");
    h.get_by_label("Recover").click();
    h.run();
    h.get_by_label("OK").click();
    h.run();
    h.run();
    assert_eq!(*log.lock().unwrap(), ["load a"]);
    assert_eq!(h.state().actions.len(), 1);
}

#[test]
fn nothing_to_recover() {
    // start-up: no dialog and no message
    let (mut h, _) = dialog(Vec::new(), false, false, false);
    h.run();
    h.run();
    assert!(h.state().actions.is_empty() && !h.state().recover.is_open());
    // Help > Recover Unsaved Work: 1.x's toast
    let (mut h, _) = dialog(Vec::new(), false, true, false);
    h.run();
    h.run();
    assert_eq!(h.state().actions, ["message There is no unsaved work to recover. false"]);
}

#[test]
fn desktop_store_lists_and_loads_a_crashed_session() {
    let root = sandbox("desktop_store");
    // a session whose window is gone: only the files are left
    let (mut w, _log) = worker(&root);
    w.handle(Command::Open { name: "basic.stlx".into(), bytes: fixture("basic.stlx") });
    w.handle(box_feature());
    w.handle(autosave());
    let id = w.autosave_id().unwrap();
    // (this process still holds its lock, so look at it as an earlier session through a copy)
    let copy = sandbox("desktop_store_copy");
    copy_dir(&root.join(&id), &copy.join(&id));
    let _ = std::fs::remove_file(copy.join(&id).join("lock"));
    let store = recover::DesktopStore { root: copy.clone() };
    let list = poll(&store.list());
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].path, None);
    assert_eq!((list[0].id.as_str(), list[0].title.as_str(), list[0].steps), (id.as_str(), "basic", 4));
    let bytes = poll(&store.load(&id)).unwrap();
    assert!(stl_engine::stlx::from_bytes(&bytes, stl_engine::Registry::builtin()).is_ok());
    assert!(!copy.join(&id).exists(), "loaded copies are discarded: opening them writes a new one");
}

fn poll<T>(p: &Pending<T>) -> T {
    let end = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(v) = p.take() {
            return v;
        }
        assert!(Instant::now() < end, "no answer");
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let (src, dst) = (e.path(), to.join(e.file_name()));
        if src.is_dir() {
            copy_dir(&src, &dst);
        } else {
            std::fs::copy(&src, &dst).unwrap();
        }
    }
}

#[test]
fn when_is_local_and_plain() {
    // the exact hour depends on the time zone; the shape and date do not
    let s = recover::when(1_700_000_000.0);
    assert!(s == "2023-11-14 22:13" || s.starts_with("2023-11-1"), "{s}");
    assert_eq!(s.len(), "2023-11-14 22:13".len());
    assert_eq!(recover::when(0.0), "");
}
