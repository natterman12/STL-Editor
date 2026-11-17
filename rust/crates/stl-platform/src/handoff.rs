//! Desktop hand-off to slicers and SolidCraft's STEP command-line tool (1.x `server.py`
//! `find_slicer`, `step_available`, `r_to_step`). Runs them as separate programs.
//!
//! The lookups take the operating system and the file-system questions as arguments
//! ([`find_slicer_with`], [`step_available_with`]) so the rules are tested without touching the
//! PC. `os` is `"windows"`, `"macos"` or anything else (Linux rules).

use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// The slicers 1.x knows: (key, name).
pub const SLICERS: [(&str, &str); 4] = [
    ("bambu", "Bambu Studio"),
    ("orca", "OrcaSlicer"),
    ("prusa", "PrusaSlicer"),
    ("solidcraft", "SolidCraft"),
];

/// 1.x `SOLIDCRAFT`.
const SOLIDCRAFT_DIR: &str = r"C:\Program Files\SolidCraft";

/// 1.x `SLICERS` on Windows: the exes tried first, in order.
fn win_exes(key: &str) -> &'static [&'static str] {
    match key {
        "bambu" => &[r"C:\Program Files\Bambu Studio\bambu-studio.exe"],
        "orca" => &[r"C:\Program Files\OrcaSlicer\orca-slicer.exe"],
        "prusa" => &[r"C:\Program Files\Prusa3D\PrusaSlicer\prusa-slicer.exe"],
        "solidcraft" => &[r"C:\Program Files\SolidCraft\solidcraft.exe"],
        _ => &[],
    }
}

/// 1.x `WIN_FOLDERS`: the install folder searched for any .exe whose name contains the key.
fn win_folder(key: &str) -> Option<&'static str> {
    match key {
        "bambu" => Some(r"C:\Program Files\Bambu Studio"),
        "orca" => Some(r"C:\Program Files\OrcaSlicer"),
        _ => None,
    }
}

/// 1.x `MAC_APPS`: the bundle name looked up in /Applications and ~/Applications.
fn mac_app(key: &str) -> Option<&'static str> {
    match key {
        "bambu" => Some("BambuStudio.app"),
        "orca" => Some("OrcaSlicer.app"),
        "prusa" => Some("PrusaSlicer.app"),
        _ => None,
    }
}

/// 1.x `LINUX_CMDS`: commands looked up on the PATH, in order.
fn linux_cmds(key: &str) -> &'static [&'static str] {
    match key {
        "bambu" => &["bambu-studio"],
        "orca" => &["orca-slicer", "orcaslicer"],
        "prusa" => &["prusa-slicer"],
        _ => &[],
    }
}

/// 1.x `LINUX_FLATPAKS`.
fn linux_flatpak(key: &str) -> Option<&'static str> {
    match key {
        "bambu" => Some("com.bambulab.BambuStudio"),
        "orca" => Some("io.github.softfever.OrcaSlicer"),
        "prusa" => Some("com.prusa3d.PrusaSlicer"),
        _ => None,
    }
}

/// 1.x `find_slicer(key)` on this computer: the command that opens a file in it.
pub fn find_slicer(key: &str) -> Option<Vec<String>> {
    let os = std::env::consts::OS;
    let exists = |p: &str| {
        if os == "windows" {
            Path::new(p).is_file()
        } else {
            Path::new(p).exists()
        }
    };
    find_slicer_with(key, os, &exists, &which, &flatpak_installed, &list_dir)
}

/// `find_slicer` with the operating system (`"windows"`, `"macos"`, `"linux"`) and the file
/// system questions passed in, for tests. The command is the program (and `open -a <app>` on
/// macOS, `flatpak run <id>` for a flatpak); the file path goes after it.
pub fn find_slicer_with(
    key: &str,
    os: &str,
    exists: &dyn Fn(&str) -> bool,
    which: &dyn Fn(&str) -> Option<String>,
    flatpak: &dyn Fn(&str) -> bool,
    listdir: &dyn Fn(&str) -> Vec<String>,
) -> Option<Vec<String>> {
    match os {
        "windows" => {
            for exe in win_exes(key) {
                if exists(exe) {
                    return Some(vec![exe.to_string()]);
                }
            }
            let folder = win_folder(key)?;
            listdir(folder)
                .into_iter()
                .find(|name| {
                    let lower = name.to_lowercase();
                    lower.ends_with(".exe") && lower.contains(key)
                })
                .map(|name| vec![format!("{folder}\\{name}")])
        }
        "macos" => {
            let name = mac_app(key)?;
            let mut apps = vec![format!("/Applications/{name}")];
            if let Some(home) = home_dir() {
                apps.push(format!("{home}/Applications/{name}"));
            }
            apps.into_iter()
                .find(|app| exists(app))
                .map(|app| vec!["open".to_string(), "-a".to_string(), app])
        }
        _ => {
            for cmd in linux_cmds(key) {
                if let Some(found) = which(cmd) {
                    return Some(vec![found]);
                }
            }
            let id = linux_flatpak(key)?;
            flatpak(id).then(|| vec!["flatpak".to_string(), "run".to_string(), id.to_string()])
        }
    }
}

/// Open `path` with a command from [`find_slicer`]. The program is started and not waited for,
/// as 1.x's `Popen` does.
pub fn open_in(cmd: &[String], path: &Path) -> Result<(), String> {
    let Some((program, args)) = cmd.split_first() else {
        return Err("there is no program to open it with".to_string());
    };
    let mut child = Command::new(program)
        .args(args)
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(plain)?;
    // Reap the program in the background, so it does not linger as a zombie on Unix.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

/// SolidCraft's command-line tool is installed (1.x `step_available`).
pub fn step_available() -> bool {
    step_available_with(std::env::consts::OS, &|p| Path::new(p).is_file())
}

/// [`step_available`] with the operating system and the file check passed in. To STEP runs
/// SolidCraft's Windows command-line tool, so it is only available on Windows.
pub fn step_available_with(os: &str, exists: &dyn Fn(&str) -> bool) -> bool {
    os == "windows" && exists(&format!("{SOLIDCRAFT_DIR}\\solidcraft-cli.exe"))
}

/// Convert `stl` to a true STEP at `step` through SolidCraft's command-line tool (1.x `r_to_step`).
/// The script it runs (`convert.json`) and its log go next to `stl`, so the caller removes that
/// folder afterwards.
pub fn to_step(stl: &Path, step: &Path) -> Result<(), String> {
    if std::env::consts::OS != "windows" {
        return Err("To STEP needs SolidCraft, which only runs on Windows.".to_string());
    }
    let cli = format!("{SOLIDCRAFT_DIR}\\solidcraft-cli.exe");
    if !Path::new(&cli).is_file() {
        return Err("SolidCraft isn't installed.".to_string());
    }
    let dir = stl
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let script = serde_json::json!({"commands": [
        {"command": "file.insert_mesh", "params": {"path": path_text(stl)}},
        {"command": "mesh.convert", "params": {"body": "body", "tolerance": 0.05}},
        {"command": "file.export", "params": {"path": path_text(step)}},
    ]});
    let script_path = dir.join("convert.json");
    std::fs::write(&script_path, script.to_string())
        .map_err(|e| format!("Couldn't write convert.json: {}", plain(e)))?;
    let log_path = dir.join("convert.log");
    let log = File::create(&log_path).map_err(plain)?;

    let mut command = Command::new(&cli);
    command
        .arg("run")
        .arg(&script_path)
        .arg("--quiet")
        .stdin(Stdio::null())
        .stdout(log.try_clone().map_err(plain)?)
        .stderr(log);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW: no console window for the converter
        command.creation_flags(0x0800_0000);
    }
    let mut child = command.spawn().map_err(plain)?;
    let deadline = Instant::now() + Duration::from_secs(600);
    let status = loop {
        if let Some(status) = child.try_wait().map_err(plain)? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err("SolidCraft took longer than 10 minutes to convert this mesh.".to_string());
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    if status.success() {
        return Ok(());
    }
    let text = std::fs::read_to_string(&log_path).unwrap_or_default();
    let last = text.trim().lines().last().unwrap_or("").trim();
    let last = if last.is_empty() { "unknown error" } else { last };
    Err(format!(
        "SolidCraft couldn't convert this mesh: {}",
        last.replace("error: ", "")
    ))
}

/// "Couldn't open <name>: <reason>": the message for a slicer that could not be started.
pub fn launch_failure_message(name: &str, reason: &str) -> String {
    format!("Couldn't open {name}: {reason}")
}

/// The folder the hand-off writes its temporary files to (1.x: `<temp>/STL Editor`).
pub fn handoff_dir() -> PathBuf {
    std::env::temp_dir().join("STL Editor")
}

/// A file name safe to write in [`handoff_dir`]: the name without its extension keeps only
/// letters, digits, spaces and `-_()` (1.x `r_send_to`); the extension stays. An empty name
/// becomes `model`.
pub fn temp_file_name(name: &str) -> String {
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], Some(&name[i + 1..])),
        _ => (name, None),
    };
    let mut safe: String = stem
        .chars()
        .filter(|c| c.is_alphanumeric() || " -_()".contains(*c))
        .collect();
    if safe.trim().is_empty() {
        safe = "model".to_string();
    }
    // The extension keeps letters and digits only, so no separator or dot can come through it.
    let ext: String = ext
        .unwrap_or("")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    if ext.is_empty() { safe } else { format!("{safe}.{ext}") }
}

fn path_text(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

/// An I/O error in plain words: the OS's "(os error 2)" suffix is dropped.
fn plain(e: std::io::Error) -> String {
    let text = e.to_string();
    match text.find(" (os error") {
        Some(i) => text[..i].to_string(),
        None => text,
    }
}

fn home_dir() -> Option<String> {
    std::env::var_os("HOME")
        .map(|h| h.to_string_lossy().into_owned())
        .filter(|h| !h.is_empty())
}

/// A program on the PATH: the first file of that name that can run.
fn which(cmd: &str) -> Option<String> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(cmd))
        .find(|p| is_runnable(p))
        .map(|p| p.to_string_lossy().into_owned())
}

fn is_runnable(p: &Path) -> bool {
    if !p.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        p.metadata()
            .map(|m| m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        true
    }
}

/// `flatpak info <id>` succeeds (false when flatpak itself is not installed).
fn flatpak_installed(app_id: &str) -> bool {
    let Ok(mut child) = Command::new("flatpak")
        .args(["info", app_id])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    else {
        return false;
    };
    // 1.x: timeout=10. A hung flatpak must not freeze the window at start-up.
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
        }
    }
}

/// The names in a folder; none when it is not a folder.
fn list_dir(folder: &str) -> Vec<String> {
    match std::fs::read_dir(folder) {
        Ok(entries) => entries
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect(),
        Err(_) => Vec::new(),
    }
}
