//! Autosave folders: the lock, writing, recovering, and what is skipped.
//! Every test works in its own folder under one throwaway `STLEDITOR2_CONFIG_DIR`.

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::OnceLock;

use serde_json::{Value, json};
use stl_platform::autosave::{Autosaver, Source, discard, load, recoverable};

/// A fresh empty folder for one test, below the throwaway config folder.
fn root(test: &str) -> PathBuf {
    static BASE: OnceLock<PathBuf> = OnceLock::new();
    let base = BASE.get_or_init(|| {
        let d = std::env::temp_dir().join(format!("stle2-autosave-test-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        // SAFETY: set once, before any test reads it; the autosave calls here take explicit folders.
        unsafe { std::env::set_var("STLEDITOR2_CONFIG_DIR", &d) };
        d
    });
    let r = base.join(test);
    let _ = std::fs::remove_dir_all(&r);
    std::fs::create_dir_all(&r).unwrap();
    r
}

fn doc(title: &str, steps: usize) -> Value {
    let features: Vec<Value> = (0..steps).map(|i| json!({"id": format!("f{}", i + 1), "type": "primitive", "params": {}})).collect();
    json!({"format": 1, "app": "STL Editor", "features": features, "rollback": null, "ui": {}, "params": [],
           "sources": {}, "path": null, "title": title, "saved": 1_700_000_000.5, "count": steps})
}

fn folders(root: &PathBuf) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(root).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    v.sort();
    v
}

#[test]
fn folder_is_named_for_the_time() {
    let r = root("folder_is_named_for_the_time");
    let a = Autosaver::begin(&r).unwrap();
    let id = a.id().to_string();
    let b = id.as_bytes();
    assert_eq!(id.len(), "yyyymmdd-hhmmss-xxxxxx".len(), "{id}");
    assert!(b[..8].iter().all(u8::is_ascii_digit) && b[8] == b'-' && b[9..15].iter().all(u8::is_ascii_digit) && b[15] == b'-', "{id}");
    assert!(r.join(&id).join("lock").is_file());
    let other = Autosaver::begin(&r).unwrap();
    assert_ne!(other.id(), a.id());
}

#[test]
fn sources_kept() {
    let r = root("sources_kept");
    let a = Autosaver::begin(&r).unwrap();
    let bytes: Vec<u8> = (0..70_000u32).map(|i| (i * 31 % 251) as u8).collect();
    let mut d = doc("Bracket", 2);
    d["sources"] = json!({"s1": {"name": "part.stl", "ext": ".stl"}});
    a.write(&d, &[Source { id: "s1".into(), name: "part.stl".into(), ext: ".stl".into(), data: bytes.clone() }]).unwrap();
    // a second write that doesn't pass the source again keeps it
    a.write(&d, &[]).unwrap();
    let id = a.id().to_string();
    drop(a); // as after a crash: the folder stays, the lock is gone
    let (back, sources) = load(&r, &id).unwrap();
    assert_eq!(back, d);
    assert_eq!(sources.len(), 1);
    assert_eq!((sources[0].id.as_str(), sources[0].name.as_str(), sources[0].ext.as_str()), ("s1", "part.stl", ".stl"));
    assert_eq!(sources[0].data, bytes, "the imported mesh comes back byte for byte");
    assert!(r.join(&id).join("sources").join("s1.stl").is_file());
    assert!(!r.join(&id).join("document.json.tmp").exists(), "written to a .tmp and renamed");
}

#[test]
fn clean_exit_removes_folder() {
    let r = root("clean_exit_removes_folder");
    let a = Autosaver::begin(&r).unwrap();
    a.write(&doc("x", 1), &[]).unwrap();
    assert_eq!(folders(&r).len(), 1);
    a.end();
    assert!(folders(&r).is_empty(), "the session's folder is removed");
    assert!(recoverable(&r, None).is_empty());
}

#[test]
fn clear_removes_the_copy_but_keeps_the_session() {
    let r = root("clear_removes_the_copy_but_keeps_the_session");
    let a = Autosaver::begin(&r).unwrap();
    let mut d = doc("x", 1);
    d["sources"] = json!({"s1": {"name": "p.stl", "ext": ".stl"}});
    a.write(&d, &[Source { id: "s1".into(), name: "p.stl".into(), ext: ".stl".into(), data: vec![1, 2, 3] }]).unwrap();
    a.clear();
    let dir = r.join(a.id());
    assert!(dir.join("lock").is_file());
    assert!(!dir.join("document.json").exists() && !dir.join("sources").exists());
    a.write(&doc("again", 1), &[]).unwrap();
    assert!(dir.join("document.json").is_file());
}

#[test]
fn live_session_not_offered() {
    let r = root("live_session_not_offered");
    let live = Autosaver::begin(&r).unwrap();
    live.write(&doc("Open in a window", 3), &[]).unwrap();
    // a second window starting up sees the first one's lock and leaves it alone
    let second = Autosaver::begin(&r).unwrap();
    assert!(recoverable(&r, Some(second.id())).is_empty(), "the other window's live session is not offered");
    assert!(recoverable(&r, None).is_empty(), "nor is this window's own");
    discard(&r, live.id());
    assert!(r.join(live.id()).join("document.json").is_file(), "discard leaves a running window's folder");
    // once it ends cleanly there is nothing left either
    live.end();
    assert!(recoverable(&r, Some(second.id())).is_empty());
}

#[test]
fn crashed_session_offered() {
    // The child runs this same test: it takes a lock, writes a copy, tells us, and waits to be killed.
    if let Some(child_root) = std::env::var_os("STLEDITOR2_T3_CHILD_ROOT") {
        let a = Autosaver::begin(std::path::Path::new(&child_root)).unwrap();
        a.write(&doc("Crashed work", 4), &[]).unwrap();
        println!("READY {}", a.id());
        loop {
            std::thread::sleep(std::time::Duration::from_secs(60));
        }
    }
    let r = root("crashed_session_offered");
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["crashed_session_offered", "--exact", "--nocapture", "--test-threads=1"])
        .env("STLEDITOR2_T3_CHILD_ROOT", &r)
        .env("STLEDITOR2_CONFIG_DIR", r.join("child-config"))
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let id = loop {
        let line = lines.next().expect("the child ended before it was ready").unwrap();
        // libtest starts the line with "test crashed_session_offered ... "
        if let Some(i) = line.find("READY ") {
            break line[i + 6..].trim().to_string();
        }
    };
    // while the child runs, its session is live
    assert!(recoverable(&r, None).is_empty(), "a running window's session is not offered");
    child.kill().unwrap();
    child.wait().unwrap();
    let found = recoverable(&r, None);
    assert_eq!(found.len(), 1, "{found:?}");
    let s = &found[0];
    assert_eq!((s.id.as_str(), s.title.as_str(), s.steps, s.path.as_deref()), (id.as_str(), "Crashed work", 4, None));
    assert_eq!(s.saved, 1_700_000_000.5);
    let (back, _) = load(&r, &id).unwrap();
    assert_eq!(back["title"], "Crashed work");
    discard(&r, &id);
    assert!(recoverable(&r, None).is_empty());
    assert!(!r.join(&id).exists());
}

#[test]
fn corrupt_autosave_skipped() {
    let r = root("corrupt_autosave_skipped");
    // two good sessions from windows that crashed (no lock held: nobody has them open)
    for (name, title) in [("20260101-000001-aaaaaa", "First"), ("20260101-000003-cccccc", "Third")] {
        let d = r.join(name);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("document.json"), serde_json::to_vec(&doc(title, 1)).unwrap()).unwrap();
    }
    // bad JSON
    let bad = r.join("20260101-000002-bbbbbb");
    std::fs::create_dir_all(&bad).unwrap();
    std::fs::write(bad.join("document.json"), b"{\"features\": [ {\"id\": \"f1\"").unwrap();
    // a source the document lists that never made it to disk
    let gap = r.join("20260101-000004-dddddd");
    std::fs::create_dir_all(&gap).unwrap();
    let mut d = doc("Missing source", 1);
    d["sources"] = json!({"s1": {"name": "p.stl", "ext": ".stl"}});
    std::fs::write(gap.join("document.json"), serde_json::to_vec(&d).unwrap()).unwrap();
    // not an object
    let arr = r.join("20260101-000005-eeeeee");
    std::fs::create_dir_all(&arr).unwrap();
    std::fs::write(arr.join("document.json"), b"[1,2,3]").unwrap();

    let found: Vec<String> = recoverable(&r, None).into_iter().map(|s| s.title).collect();
    assert_eq!(found, ["First", "Third"], "the other sessions are still offered");
    assert!(load(&r, "20260101-000002-bbbbbb").is_err());
    assert!(load(&r, "20260101-000004-dddddd").is_err());
    assert!(load(&r, "no-such-folder").is_err());
    assert!(load(&r, "..").is_err(), "an id is a folder name, nothing else");
}

#[test]
fn stale_empty_folders_removed() {
    let r = root("stale_empty_folders_removed");
    let old = r.join("20200101-000000-aaaaaa");
    std::fs::create_dir_all(&old).unwrap();
    std::fs::write(old.join("lock"), b" ").unwrap();
    let no_lock = r.join("20200101-000001-bbbbbb");
    std::fs::create_dir_all(&no_lock).unwrap();
    // a folder that isn't one of ours (its name isn't a time) is left alone
    let other = r.join("scratch");
    std::fs::create_dir_all(&other).unwrap();
    // a window that has just started: no copy yet, but its lock is held, so it stays
    let live = Autosaver::begin(&r).unwrap();
    assert!(recoverable(&r, None).is_empty());
    assert!(!old.exists() && !no_lock.exists(), "empty stale folders are removed");
    assert!(r.join(live.id()).is_dir(), "a running window's folder stays");
    assert!(other.is_dir(), "a folder whose name isn't a time is not removed");
    // a brand-new empty folder (the window hasn't taken its lock yet) is left a while
    let fresh = Autosaver::begin(&r).unwrap();
    let fresh_dir = r.join(fresh.id());
    fresh.end();
    std::fs::create_dir_all(&fresh_dir).unwrap();
    assert!(recoverable(&r, None).is_empty());
    assert!(fresh_dir.is_dir(), "a folder made a moment ago is not removed");
}
