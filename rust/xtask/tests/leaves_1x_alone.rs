//! M5 review focus 6, for `xtask install` / `uninstall --purge`: 1.x's folders are never touched.
//! (The platform half, settings, recent files, autosave, Recover and the update check, is in
//! `crates/stl-app/tests/leaves_1x_alone.rs`.)
//!
//! A throwaway profile holds 1.x's `STL Editor` folders (sample files and a 1.x `settings.json`)
//! and 2.0's own. The child processes see that profile through `APPDATA`, `LOCALAPPDATA` and
//! `USERPROFILE`, with no `STLEDITOR2_*` override, so install and uninstall work out 2.0's real
//! default folders, the Start Menu one included, below it. Nothing here reaches the real
//! Programs folder, Start Menu or settings.
#![cfg(windows)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, SystemTime};

type Snapshot = BTreeMap<PathBuf, (Option<Vec<u8>>, Option<SystemTime>)>;

fn snapshot(root: &Path) -> Snapshot {
    fn walk(dir: &Path, root: &Path, out: &mut Snapshot) {
        for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let p = e.path();
            let rel = p.strip_prefix(root).unwrap().to_path_buf();
            if p.is_dir() {
                out.insert(rel, (None, None));
                walk(&p, root, out);
            } else {
                out.insert(rel, (Some(std::fs::read(&p).unwrap()), e.metadata().ok().and_then(|m| m.modified().ok())));
            }
        }
    }
    let mut out = Snapshot::new();
    walk(root, root, &mut out);
    out
}

struct Profile {
    root: PathBuf,
    roaming: PathBuf,
    local: PathBuf,
}

impl Profile {
    fn new(tag: &str) -> Profile {
        let root = std::env::temp_dir().join(format!("stle2-leaves-1x-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let roaming = root.join("AppData").join("Roaming");
        let local = root.join("AppData").join("Local");
        std::fs::create_dir_all(&roaming).unwrap();
        std::fs::create_dir_all(&local).unwrap();
        Profile { root, roaming, local }
    }
    fn one_x_config(&self) -> PathBuf {
        self.roaming.join("STL Editor")
    }
    fn one_x_data(&self) -> PathBuf {
        self.local.join("STL Editor")
    }
    fn write(&self, p: PathBuf, body: &[u8]) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    }
    /// 1.x's folders as a 1.x install leaves them.
    fn make_1x(&self) {
        let (c, d) = (self.one_x_config(), self.one_x_data());
        self.write(
            c.join("settings.json"),
            br#"{"recent": ["C:\\parts\\old.stlx"], "last_dir": "C:\\parts", "update_repo": "natterman12/STL-Editor", "update_skip": "1.12.0"}"#,
        );
        self.write(c.join("support-fins").join("engine.js"), b"// 1.x's downloaded Support Fins engine\n");
        self.write(d.join("log.txt"), b"1.x started\n1.x closed normally\n");
        self.write(d.join("autosave").join("19700101-aaaa").join("document.json"), br#"{"title":"1.x work","count":3}"#);
        self.write(d.join("autosave").join("19700101-aaaa").join("sources").join("s1.stl"), b"solid 1.x\nendsolid\n");
        self.write(d.join("webview").join("Default").join("Local State"), &[0, 1, 2, 3, 255, 254]);
    }
    /// 2.0's own settings and autosave, which `--purge` removes.
    fn make_2x(&self) {
        self.write(self.roaming.join("STL Editor 2").join("settings.json"), br#"{"recent": []}"#);
        self.write(self.local.join("STL Editor 2").join("autosave").join("s1").join("document.json"), b"{}");
        self.write(self.local.join("STL Editor 2").join("windows").join("1-0.lock"), b"");
    }
    fn xtask(&self, args: &[&str]) -> Output {
        self.xtask_with(args, &[])
    }
    fn xtask_with(&self, args: &[&str], extra: &[(&str, &Path)]) -> Output {
        let mut c = Command::new(env!("CARGO_BIN_EXE_xtask"));
        c.args(args)
            .env("USERPROFILE", &self.root)
            .env("HOME", &self.root)
            .env("APPDATA", &self.roaming)
            .env("LOCALAPPDATA", &self.local)
            .env_remove("STLEDITOR2_CONFIG_DIR")
            .env_remove("STLEDITOR2_INSTALL_DIR")
            .env_remove("STLEDITOR2_STARTMENU_DIR");
        for (k, v) in extra {
            c.env(k, v);
        }
        c.output().unwrap()
    }
}

impl Drop for Profile {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn text(o: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

fn assert_unchanged(p: &Profile, before: &(Snapshot, Snapshot), when: &str) {
    let after = (snapshot(&p.one_x_config()), snapshot(&p.one_x_data()));
    assert_eq!(before.0.keys().collect::<Vec<_>>(), after.0.keys().collect::<Vec<_>>(), "{when}: no file or folder appeared or went in 1.x's settings folder");
    assert_eq!(before.1.keys().collect::<Vec<_>>(), after.1.keys().collect::<Vec<_>>(), "{when}: no file or folder appeared or went in 1.x's data folder");
    assert_eq!(before.0, after.0, "{when}: 1.x's settings folder is byte for byte as it was");
    assert_eq!(before.1, after.1, "{when}: 1.x's data folder is byte for byte as it was");
}

#[test]
fn leaves_1x_alone() {
    let p = Profile::new("install");
    p.make_1x();
    p.make_2x();
    let before = (snapshot(&p.one_x_config()), snapshot(&p.one_x_data()));
    assert!(before.0.len() >= 3 && before.1.len() >= 6, "the fixtures are there");
    std::thread::sleep(Duration::from_millis(30));

    let exe = p.root.join("fake-stl-app.exe");
    std::fs::write(&exe, b"the 2.0 exe").unwrap();
    let installed = p.local.join("Programs").join("STL Editor 2").join("stl-editor-2.exe");
    let lnk = p.roaming.join("Microsoft").join("Windows").join("Start Menu").join("Programs").join("STL Editor 2.lnk");

    // install, into the defaults below the temp profile
    let o = p.xtask(&["install", "--exe", exe.to_str().unwrap()]);
    assert!(o.status.success(), "{}", text(&o));
    assert_eq!(std::fs::read(&installed).unwrap(), b"the 2.0 exe", "{}", text(&o));
    assert!(lnk.is_file(), "the Start Menu shortcut is below the temp profile: {}", text(&o));
    assert_unchanged(&p, &before, "after install");

    // install again over it, then uninstall: the exe and the shortcut go, 2.0's settings stay
    assert!(p.xtask(&["install", "--exe", exe.to_str().unwrap()]).status.success());
    let o = p.xtask(&["uninstall"]);
    assert!(o.status.success(), "{}", text(&o));
    assert!(!installed.exists() && !lnk.exists(), "{}", text(&o));
    assert!(p.roaming.join("STL Editor 2").join("settings.json").is_file(), "uninstall without --purge keeps 2.0's settings");
    assert_unchanged(&p, &before, "after uninstall");

    // uninstall --purge: 2.0's settings, autosave and folders go, and nothing of 1.x
    assert!(p.xtask(&["install", "--exe", exe.to_str().unwrap()]).status.success());
    let o = p.xtask(&["uninstall", "--purge"]);
    assert!(o.status.success(), "{}", text(&o));
    assert!(!p.roaming.join("STL Editor 2").exists(), "2.0's settings folder is gone: {}", text(&o));
    assert!(!p.local.join("STL Editor 2").exists(), "2.0's data folder is gone: {}", text(&o));
    assert!(!p.local.join("Programs").join("STL Editor 2").exists(), "2.0's install folder is gone: {}", text(&o));
    assert_unchanged(&p, &before, "after uninstall --purge");
}

#[test]
fn refuses_to_touch_1x_s_folders() {
    let p = Profile::new("refuse");
    p.make_1x();
    p.make_2x();
    let before = (snapshot(&p.one_x_config()), snapshot(&p.one_x_data()));
    let exe = p.root.join("fake-stl-app.exe");
    std::fs::write(&exe, b"the 2.0 exe").unwrap();

    // an override that points into 1.x's folder is refused by install, and by uninstall --purge
    let (c, d) = (p.one_x_config(), p.one_x_data());
    let o = p.xtask_with(&["install", "--exe", exe.to_str().unwrap()], &[("STLEDITOR2_INSTALL_DIR", &d)]);
    assert!(!o.status.success(), "{}", text(&o));
    assert!(text(&o).contains("STL Editor 1.x's folder"), "{}", text(&o));
    assert!(!d.join("stl-editor-2.exe").exists());
    let o = p.xtask_with(&["uninstall", "--purge"], &[("STLEDITOR2_CONFIG_DIR", &c)]);
    assert!(!o.status.success(), "{}", text(&o));
    assert!(text(&o).contains("STL Editor 1.x's folder"), "{}", text(&o));
    let o = p.xtask_with(&["uninstall", "--purge"], &[("STLEDITOR2_INSTALL_DIR", &d)]);
    assert!(!o.status.success(), "{}", text(&o));
    assert_unchanged(&p, &before, "after the refused runs");
}
