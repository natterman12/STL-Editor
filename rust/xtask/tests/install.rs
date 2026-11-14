//! `xtask install` / `uninstall` (milestone 5, Task 5). Windows only, always into temp folders:
//! nothing here touches the real Programs folder, Start Menu or settings.
#![cfg(windows)]

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output};

struct Env {
    root: PathBuf,
    install: PathBuf,
    config: PathBuf,
    start: PathBuf,
}

impl Env {
    fn new(tag: &str) -> Env {
        let root = std::env::temp_dir().join(format!("stl2-install-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        Env { install: root.join("Programs").join("STL Editor 2"), config: root.join("cfg"), start: root.join("start"), root }
    }
    fn xtask(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_xtask"))
            .args(args)
            .env("STLEDITOR2_INSTALL_DIR", &self.install)
            .env("STLEDITOR2_CONFIG_DIR", &self.config)
            .env("STLEDITOR2_STARTMENU_DIR", &self.start)
            .output()
            .unwrap()
    }
    fn fake_exe(&self, body: &str) -> PathBuf {
        let p = self.root.join("fake-stl-app.exe");
        std::fs::write(&p, body).unwrap();
        p
    }
    fn installed(&self) -> PathBuf {
        self.install.join("stl-editor-2.exe")
    }
    fn lnk(&self) -> PathBuf {
        self.start.join("STL Editor 2.lnk")
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn text(o: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

fn utf16(s: &str) -> Vec<u8> {
    s.encode_utf16().flat_map(|u| u.to_le_bytes()).collect()
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

/// The shortcut is a Shell Link: right header and class id, our exe as its target, the install
/// folder as its working directory.
fn check_lnk(lnk: &Path, exe: &Path, dir: &Path) {
    let b = std::fs::read(lnk).unwrap();
    assert_eq!(&b[0..4], &[0x4C, 0, 0, 0], "header size");
    assert_eq!(
        &b[4..20],
        &[0x01, 0x14, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46],
        "link class id"
    );
    // target ID list, link info, name, working dir, icon location, Unicode strings
    let flags = u32::from_le_bytes(b[20..24].try_into().unwrap());
    assert_eq!(flags, 1 | (1 << 1) | (1 << 2) | (1 << 4) | (1 << 6) | (1 << 7), "link flags {flags:#x}");
    assert!(contains(&b, exe.to_str().unwrap().as_bytes()), "target path (ANSI) in link info");
    assert!(contains(&b, &utf16(exe.to_str().unwrap())), "target path (UTF-16) in link info");
    assert!(contains(&b, &utf16(dir.to_str().unwrap())), "working dir");
}

#[test]
fn install_then_uninstall() {
    let e = Env::new("basic");
    let exe = e.fake_exe("version one");
    let out = e.xtask(&["install", "--exe", exe.to_str().unwrap()]);
    assert!(out.status.success(), "{}", text(&out));
    assert_eq!(std::fs::read_to_string(e.installed()).unwrap(), "version one");
    check_lnk(&e.lnk(), &e.installed(), &e.install);
    // settings and autosave belong to the app, not the installer
    assert!(!e.config.exists());

    let out = e.xtask(&["uninstall"]);
    assert!(out.status.success(), "{}", text(&out));
    assert!(!e.installed().exists());
    assert!(!e.install.exists(), "the folder goes too");
    assert!(!e.lnk().exists());
    // a second uninstall has nothing to do and says so without failing
    let out = e.xtask(&["uninstall"]);
    assert!(out.status.success(), "{}", text(&out));
}

#[test]
fn install_replaces_previous() {
    let e = Env::new("replace");
    let one = e.fake_exe("version one");
    assert!(e.xtask(&["install", "--exe", one.to_str().unwrap()]).status.success());
    let two = e.fake_exe("version two, longer");
    let out = e.xtask(&["install", "--exe", two.to_str().unwrap()]);
    assert!(out.status.success(), "{}", text(&out));
    assert_eq!(std::fs::read_to_string(e.installed()).unwrap(), "version two, longer");
    check_lnk(&e.lnk(), &e.installed(), &e.install);
}

/// Not a test: when `STL2_INSTALL_TEST_SLEEP` is set this stands in for a running stl-editor-2.exe.
#[test]
fn sleep_helper() {
    if std::env::var_os("STL2_INSTALL_TEST_SLEEP").is_some() {
        std::thread::sleep(std::time::Duration::from_secs(120));
    }
}

struct Killer(Child);
impl Drop for Killer {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn refuses_while_running() {
    let e = Env::new("running");
    std::fs::create_dir_all(&e.install).unwrap();
    // a copy of this test program, installed as the running app
    std::fs::copy(std::env::current_exe().unwrap(), e.installed()).unwrap();
    let child = Command::new(e.installed())
        .args(["--exact", "sleep_helper", "--nocapture"])
        .env("STL2_INSTALL_TEST_SLEEP", "1")
        .spawn()
        .unwrap();
    let _kill = Killer(child);
    std::thread::sleep(std::time::Duration::from_millis(500));
    let before = std::fs::metadata(e.installed()).unwrap().len();

    let new = e.fake_exe("new build");
    let out = e.xtask(&["install", "--exe", new.to_str().unwrap()]);
    assert!(!out.status.success(), "{}", text(&out));
    let msg = text(&out).to_lowercase();
    assert!(msg.contains("close") && msg.contains("stl editor 2"), "{msg}");
    assert_eq!(std::fs::metadata(e.installed()).unwrap().len(), before, "the running exe is untouched");
    assert!(!e.lnk().exists());

    let out = e.xtask(&["uninstall"]);
    assert!(!out.status.success(), "{}", text(&out));
    assert!(e.installed().exists());
}

#[test]
fn purge_removes_settings_and_autosave() {
    let e = Env::new("purge");
    let exe = e.fake_exe("v");
    assert!(e.xtask(&["install", "--exe", exe.to_str().unwrap()]).status.success());
    std::fs::create_dir_all(e.config.join("data").join("autosave")).unwrap();
    std::fs::write(e.config.join("settings.json"), "{}").unwrap();
    std::fs::write(e.config.join("data").join("autosave").join("a.json"), "{}").unwrap();

    // without --purge they stay
    let out = e.xtask(&["uninstall"]);
    assert!(out.status.success(), "{}", text(&out));
    assert!(e.config.join("settings.json").exists());
    assert!(e.config.join("data").join("autosave").join("a.json").exists());

    assert!(e.xtask(&["install", "--exe", exe.to_str().unwrap()]).status.success());
    let out = e.xtask(&["uninstall", "--purge"]);
    assert!(out.status.success(), "{}", text(&out));
    assert!(!e.config.exists());
    assert!(!e.install.exists());
    assert!(!e.lnk().exists());
}

#[test]
fn leaves_1x_folders_alone() {
    // a folder called "STL Editor" is 1.x's: never removed, even if pointed at
    let e = Env::new("one-x");
    let one_x = e.root.join("STL Editor");
    std::fs::create_dir_all(&one_x).unwrap();
    std::fs::write(one_x.join("settings.json"), "{}").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(["uninstall", "--purge"])
        .env("STLEDITOR2_INSTALL_DIR", &one_x)
        .env("STLEDITOR2_CONFIG_DIR", &e.config)
        .env("STLEDITOR2_STARTMENU_DIR", &e.start)
        .output()
        .unwrap();
    assert!(!out.status.success(), "{}", text(&out));
    assert!(one_x.join("settings.json").exists());
}

#[test]
fn purge_keeps_unrelated_files() {
    // STLEDITOR2_CONFIG_DIR can point anywhere: only 2.0's own files go
    let e = Env::new("purge-keep");
    let exe = e.fake_exe("v");
    assert!(e.xtask(&["install", "--exe", exe.to_str().unwrap()]).status.success());
    std::fs::create_dir_all(e.config.join("data").join("autosave").join("s1")).unwrap();
    std::fs::create_dir_all(e.config.join("data").join("windows")).unwrap();
    std::fs::create_dir_all(e.config.join("support-fins")).unwrap();
    for f in ["settings.json", "settings.json.bad", "settings.lock", ".1.2.tmp", "data/log.txt", "data/autosave/s1/document.json", "support-fins/engine.iife.js"] {
        std::fs::write(e.config.join(f), "x").unwrap();
    }
    std::fs::write(e.config.join("holiday-photos.txt"), "mine").unwrap();
    std::fs::write(e.config.join("data").join("notes.txt"), "mine too").unwrap();

    let out = e.xtask(&["uninstall", "--purge"]);
    assert!(out.status.success(), "{}", text(&out));
    assert_eq!(std::fs::read_to_string(e.config.join("holiday-photos.txt")).unwrap(), "mine");
    assert_eq!(std::fs::read_to_string(e.config.join("data").join("notes.txt")).unwrap(), "mine too");
    for gone in ["settings.json", "settings.json.bad", "settings.lock", ".1.2.tmp", "support-fins", "data/log.txt", "data/autosave", "data/windows"] {
        assert!(!e.config.join(gone).exists(), "{gone} should be gone");
    }
    assert!(!e.installed().exists());
}

#[test]
fn purge_refuses_unsafe_folders() {
    let e = Env::new("purge-unsafe");
    let exe = e.fake_exe("v");
    assert!(e.xtask(&["install", "--exe", exe.to_str().unwrap()]).status.success());
    // a relative override, with a settings.json under the working directory
    let cwd = e.root.join("cwd");
    std::fs::create_dir_all(cwd.join("rel")).unwrap();
    std::fs::write(cwd.join("rel").join("settings.json"), "{}").unwrap();
    let run = |config: &str, dir: Option<&Path>| {
        let mut c = Command::new(env!("CARGO_BIN_EXE_xtask"));
        c.args(["uninstall", "--purge"])
            .env("STLEDITOR2_INSTALL_DIR", &e.install)
            .env("STLEDITOR2_CONFIG_DIR", config)
            .env("STLEDITOR2_STARTMENU_DIR", &e.start);
        if let Some(d) = dir {
            c.current_dir(d);
        }
        c.output().unwrap()
    };
    let out = run("rel", Some(&cwd));
    assert!(!out.status.success(), "{}", text(&out));
    assert!(cwd.join("rel").join("settings.json").exists());
    // a whole drive
    let drive = format!("{}\\", e.root.to_str().unwrap().split(':').next().unwrap().to_owned() + ":");
    let out = run(&drive, None);
    assert!(!out.status.success(), "{}", text(&out));
    // the user's profile
    let profile = std::env::var("USERPROFILE").unwrap();
    let out = run(&profile, None);
    assert!(!out.status.success(), "{}", text(&out));
    // nothing was removed: the exe and the shortcut are still there
    assert!(e.installed().exists());
    assert!(e.lnk().exists());
}
