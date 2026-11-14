//! `xtask install` and `xtask uninstall [--purge]` (milestone 5, Task 5): the local test build.
//!
//! `install` builds `stl-app` in release (or takes `--exe <path>`, which tests use), copies it to
//! `install_dir()\stl-editor-2.exe` and, on Windows, writes the Start Menu shortcut
//! "STL Editor 2.lnk". No file associations, so 1.x keeps opening `.stl` and `.stlx` files.
//! `uninstall` removes the exe, the folder and the shortcut; `--purge` also removes 2.0's settings
//! and autosave. Nothing here ever touches 1.x's `STL Editor` folders.
//!
//! `STLEDITOR2_STARTMENU_DIR` replaces the Start Menu Programs folder (tests).

use std::fs::{self, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use stl_platform::paths;

const EXE_NAME: &str = if cfg!(windows) { "stl-editor-2.exe" } else { "stl-editor-2" };
/// What the app's updater leaves behind after a swap.
const OLD_EXE_NAME: &str = stl_platform::update::OLD_NAME;
const SHORTCUT_NAME: &str = "STL Editor 2.lnk";
const SHORTCUT_TITLE: &str = "STL Editor 2";

pub fn run(mut args: impl Iterator<Item = String>) -> Result<(), String> {
    let cmd = args.next().ok_or("install or uninstall?")?;
    let rest: Vec<String> = args.collect();
    match cmd.as_str() {
        "install" => {
            let mut exe = None;
            let mut it = rest.into_iter();
            while let Some(a) = it.next() {
                match a.as_str() {
                    "--exe" => exe = Some(PathBuf::from(it.next().ok_or("--exe needs a path")?)),
                    other => return Err(format!("unknown option {other}")),
                }
            }
            install(exe)
        }
        "uninstall" => {
            let mut purge = false;
            for a in rest {
                match a.as_str() {
                    "--purge" => purge = true,
                    other => return Err(format!("unknown option {other}")),
                }
            }
            uninstall(purge)
        }
        other => Err(format!("unknown command {other}")),
    }
}

fn start_menu_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("STLEDITOR2_STARTMENU_DIR").filter(|v| !v.is_empty()) {
        return PathBuf::from(d);
    }
    let appdata = std::env::var_os("APPDATA")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let home = std::env::var_os("USERPROFILE").map(PathBuf::from).unwrap_or_default();
            home.join("AppData").join("Roaming")
        });
    appdata.join("Microsoft").join("Windows").join("Start Menu").join("Programs")
}

/// 1.x's folders are called "STL Editor". Refuse to touch a path with a folder of that name in it.
fn guard_not_1x(what: &str, dir: &Path) -> Result<(), String> {
    let is_1x = dir.components().any(|c| c.as_os_str().to_string_lossy().eq_ignore_ascii_case("STL Editor"));
    if is_1x {
        return Err(format!("{what} is {}, which is STL Editor 1.x's folder. Nothing was changed.", dir.display()));
    }
    Ok(())
}

/// A path is only fair game for `--purge` when it is absolute, not a drive root, outside 1.x's
/// folders, and not the user's profile (or a folder holding it): a mistyped override must not
/// reach anything but 2.0's own files.
fn guard_purge(what: &str, dir: &Path) -> Result<(), String> {
    guard_not_1x(what, dir)?;
    let refuse = |why: &str| Err(format!("{what} is {}, {why}. Nothing was changed.", dir.display()));
    if !dir.is_absolute() {
        return refuse("which is not a full path");
    }
    if dir.parent().is_none() {
        return refuse("which is a whole drive");
    }
    let norm = |p: &Path| p.to_string_lossy().replace('/', "\\").trim_end_matches('\\').to_lowercase();
    let d = norm(dir);
    for var in ["USERPROFILE", "APPDATA", "LOCALAPPDATA", "HOME"] {
        let Some(v) = std::env::var_os(var).filter(|v| !v.is_empty()) else { continue };
        let root = norm(Path::new(&v));
        if root == d || root.starts_with(&format!("{d}\\")) {
            return refuse("which holds your user folders");
        }
    }
    Ok(())
}

/// Windows refuses to open a running exe for writing (and so to replace or delete it).
fn ensure_not_running(exe: &Path) -> Result<(), String> {
    match OpenOptions::new().write(true).open(exe) {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        // ERROR_SHARING_VIOLATION, ERROR_LOCK_VIOLATION
        Err(e) if matches!(e.raw_os_error(), Some(32 | 33)) => Err(format!(
            "STL Editor 2 is open, so {} can't be changed. Close every STL Editor 2 window, then run this again.",
            exe.display()
        )),
        Err(e) => Err(format!("could not check {}: {e}", exe.display())),
    }
}

fn rust_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn target_dir() -> PathBuf {
    match std::env::var_os("CARGO_TARGET_DIR") {
        Some(dir) => {
            let path = PathBuf::from(dir);
            if path.is_absolute() { path } else { rust_dir().join(path) }
        }
        None => rust_dir().join("target"),
    }
}

fn build_release() -> Result<PathBuf, String> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    println!("install: building stl-app (release)...");
    let status = Command::new(cargo)
        .current_dir(rust_dir())
        .args(["build", "--release", "-p", "stl-app"])
        .status()
        .map_err(|e| format!("could not run cargo: {e}"))?;
    if !status.success() {
        return Err("the release build of stl-app failed".into());
    }
    let name = if cfg!(windows) { "stl-app.exe" } else { "stl-app" };
    Ok(target_dir().join("release").join(name))
}

fn install(exe: Option<PathBuf>) -> Result<(), String> {
    let dir = paths::install_dir();
    guard_not_1x("The install folder", &dir)?;
    let dest = dir.join(EXE_NAME);
    // Before a long build: no point building if the installed copy can't be replaced.
    ensure_not_running(&dest)?;
    let src = match exe {
        Some(p) => p,
        None => build_release()?,
    };
    if !src.is_file() {
        return Err(format!("there is no program to install at {}", src.display()));
    }
    // The build can take minutes; check again just before the swap.
    ensure_not_running(&dest)?;
    fs::create_dir_all(&dir).map_err(|e| format!("could not create {}: {e}", dir.display()))?;
    let tmp = dir.join(format!("{EXE_NAME}.new"));
    fs::copy(&src, &tmp).map_err(|e| format!("could not copy {} to {}: {e}", src.display(), tmp.display()))?;
    fs::rename(&tmp, &dest).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        format!("could not replace {}: {e}", dest.display())
    })?;
    println!("install: {}", dest.display());
    if cfg!(windows) {
        let menu = start_menu_dir();
        fs::create_dir_all(&menu).map_err(|e| format!("could not create {}: {e}", menu.display()))?;
        let lnk = menu.join(SHORTCUT_NAME);
        let size = fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);
        fs::write(&lnk, shell_link(&dest, &dir, size)).map_err(|e| format!("could not write {}: {e}", lnk.display()))?;
        println!("install: shortcut {}", lnk.display());
    } else {
        println!("install: no Start Menu shortcut on this system");
    }
    println!("install: done. 1.x still opens .stl and .stlx files; nothing was associated.");
    Ok(())
}

fn remove_file_if_exists(file: &Path) -> Result<bool, String> {
    match fs::remove_file(file) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(format!("could not remove {}: {e}", file.display())),
    }
}

/// Remove only what 2.0 itself keeps in `dir`: the named files and folders, and any `*.tmp`. The
/// folder goes too, but only when that leaves it empty, so an override that points at a folder
/// with other things in it loses nothing else.
fn purge_dir(dir: &Path, files: &[&str], folders: &[&str]) -> Result<bool, String> {
    let mut removed = false;
    for f in files {
        removed |= remove_file_if_exists(&dir.join(f))?;
    }
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_lowercase();
            if name.ends_with(".tmp") && entry.file_type().is_ok_and(|t| t.is_file()) {
                removed |= remove_file_if_exists(&entry.path())?;
            }
        }
    }
    for d in folders {
        match fs::remove_dir_all(dir.join(d)) {
            Ok(()) => removed = true,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("could not remove {}: {e}", dir.join(d).display())),
        }
    }
    remove_dir_if_empty(dir)?;
    Ok(removed)
}

/// `remove_dir`, where a missing or non-empty folder is fine. True when the folder was removed.
fn remove_dir_if_empty(dir: &Path) -> Result<bool, String> {
    match fs::remove_dir(dir) {
        Ok(()) => Ok(true),
        Err(e) if matches!(e.kind(), io::ErrorKind::NotFound | io::ErrorKind::DirectoryNotEmpty) => Ok(false),
        Err(e) => Err(format!("could not remove {}: {e}", dir.display())),
    }
}

fn uninstall(purge: bool) -> Result<(), String> {
    let dir = paths::install_dir();
    guard_not_1x("The install folder", &dir)?;
    if purge {
        guard_purge("The settings folder", &paths::config_dir())?;
        guard_purge("The data folder", &paths::data_dir())?;
    }
    let exe = dir.join(EXE_NAME);
    ensure_not_running(&exe)?;
    let mut removed = false;
    for name in [EXE_NAME, OLD_EXE_NAME] {
        removed |= remove_file_if_exists(&dir.join(name))?;
    }
    // Only our own files, and the folder when nothing else is left in it.
    if dir.exists() {
        if remove_dir_if_empty(&dir)? {
            removed = true;
        } else {
            println!("uninstall: {} has other files in it, so the folder was left", dir.display());
        }
    }
    removed |= remove_file_if_exists(&start_menu_dir().join(SHORTCUT_NAME))?;
    if purge {
        // The data folder first: with STLEDITOR2_CONFIG_DIR it sits inside the settings folder.
        removed |= purge_dir(&paths::data_dir(), &["log.txt"], &["autosave", "windows"])?;
        removed |= purge_dir(&paths::config_dir(), &["settings.json", "settings.json.bad", "settings.lock"], &["support-fins"])?;
        println!("uninstall: settings and autosave removed");
    } else {
        println!("uninstall: settings and autosave kept ({}); add --purge to remove them", paths::config_dir().display());
    }
    println!("{}", if removed { "uninstall: done" } else { "uninstall: nothing was installed" });
    Ok(())
}

// ---- Windows shell link (.lnk) writer: MS-SHLLINK, pure Rust ------------------------------

fn u16le(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn u32le(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn win_path(p: &Path) -> String {
    p.to_string_lossy().replace('/', "\\")
}

fn ansi(s: &str) -> Vec<u8> {
    let mut v: Vec<u8> = s.chars().map(|c| if c.is_ascii() { c as u8 } else { b'?' }).collect();
    v.push(0);
    v
}

fn utf16z(s: &str) -> Vec<u8> {
    let mut v: Vec<u8> = s.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
    v.extend_from_slice(&[0, 0]);
    v
}

fn string_data(out: &mut Vec<u8>, s: &str) {
    let units: Vec<u16> = s.encode_utf16().collect();
    u16le(out, units.len() as u16);
    for u in units {
        u16le(out, u);
    }
}

/// The shell's own view of a drive path (My Computer, the drive, then one item per folder and the
/// file). `None` for a path without a drive letter (the link info still holds the path).
fn target_id_list(target: &str) -> Option<Vec<u8>> {
    let b = target.as_bytes();
    if b.len() < 3 || !b[0].is_ascii_alphabetic() || b[1] != b':' || b[2] != b'\\' {
        return None;
    }
    let mut v = Vec::new();
    // My Computer
    u16le(&mut v, 0x14);
    v.extend_from_slice(&[0x1F, 0x50]);
    v.extend_from_slice(&[0xE0, 0x4F, 0xD0, 0x20, 0xEA, 0x3A, 0x69, 0x10, 0xA2, 0xD8, 0x08, 0x00, 0x2B, 0x30, 0x30, 0x9D]);
    // the drive: "C:\" padded to 22 bytes
    u16le(&mut v, 0x19);
    v.push(0x2F);
    let mut drive = target[..3].to_ascii_uppercase().into_bytes();
    drive.resize(22, 0);
    v.extend_from_slice(&drive);
    let parts: Vec<&str> = target[3..].split('\\').filter(|p| !p.is_empty()).collect();
    for (i, part) in parts.iter().enumerate() {
        let is_file = i + 1 == parts.len();
        let mut item = vec![if is_file { 0x32 } else { 0x31 }, 0];
        item.extend_from_slice(&[0u8; 4]); // size
        item.extend_from_slice(&[0u8; 4]); // modified date and time
        item.extend_from_slice(&(if is_file { 0x20u16 } else { 0x10u16 }).to_le_bytes());
        item.extend_from_slice(&ansi(part));
        if (item.len() + 2) % 2 == 1 {
            item.push(0);
        }
        u16le(&mut v, (item.len() + 2) as u16);
        v.extend_from_slice(&item);
    }
    u16le(&mut v, 0); // end of list
    Some(v)
}

/// A shortcut to `target` that starts in `work_dir` and uses the exe's own icon. Written without a
/// target ID list: Windows resolves the target from the link info's local path.
pub(crate) fn shell_link(target: &Path, work_dir: &Path, target_size: u64) -> Vec<u8> {
    let target = win_path(target);
    let work_dir = win_path(work_dir);
    let mut out = Vec::new();
    // ShellLinkHeader
    u32le(&mut out, 0x4C);
    out.extend_from_slice(&[0x01, 0x14, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46]);
    // HasLinkTargetIDList (when the path has a drive letter) | HasLinkInfo | HasName |
    // HasWorkingDir | HasIconLocation | IsUnicode
    let id_list = target_id_list(&target);
    let id_flag = if id_list.is_some() { 1 } else { 0 };
    u32le(&mut out, id_flag | (1 << 1) | (1 << 2) | (1 << 4) | (1 << 6) | (1 << 7));
    u32le(&mut out, 0x20); // FILE_ATTRIBUTE_ARCHIVE
    out.extend_from_slice(&[0u8; 24]); // creation, access, write times
    u32le(&mut out, target_size.min(u32::MAX as u64) as u32);
    u32le(&mut out, 0); // icon index
    u32le(&mut out, 1); // SW_SHOWNORMAL
    out.extend_from_slice(&[0u8; 12]); // hot key, then reserved fields
    debug_assert_eq!(out.len(), 76);
    if let Some(list) = id_list {
        u16le(&mut out, list.len() as u16);
        out.extend_from_slice(&list);
    }

    // LinkInfo: local path only, with the Unicode copy for non-ASCII folders
    let header = 0x24u32;
    let volume = {
        let mut v = Vec::new();
        u32le(&mut v, 0x11); // size
        u32le(&mut v, 3); // fixed drive
        u32le(&mut v, 0); // serial number
        u32le(&mut v, 0x10); // label offset
        v.push(0); // empty label
        v
    };
    let base_ansi = ansi(&target);
    let suffix_ansi = [0u8];
    let base_uni = utf16z(&target);
    let suffix_uni = [0u8, 0];
    let volume_off = header;
    let base_off = volume_off + volume.len() as u32;
    let suffix_off = base_off + base_ansi.len() as u32;
    let base_uni_off = suffix_off + suffix_ansi.len() as u32;
    let suffix_uni_off = base_uni_off + base_uni.len() as u32;
    let total = suffix_uni_off + suffix_uni.len() as u32;
    u32le(&mut out, total);
    u32le(&mut out, header);
    u32le(&mut out, 1); // VolumeIDAndLocalBasePath
    u32le(&mut out, volume_off);
    u32le(&mut out, base_off);
    u32le(&mut out, 0); // no network location
    u32le(&mut out, suffix_off);
    u32le(&mut out, base_uni_off);
    u32le(&mut out, suffix_uni_off);
    out.extend_from_slice(&volume);
    out.extend_from_slice(&base_ansi);
    out.extend_from_slice(&suffix_ansi);
    out.extend_from_slice(&base_uni);
    out.extend_from_slice(&suffix_uni);

    // StringData: description, working directory, icon
    string_data(&mut out, SHORTCUT_TITLE);
    string_data(&mut out, &work_dir);
    string_data(&mut out, &target);
    u32le(&mut out, 0); // terminal block
    out
}
