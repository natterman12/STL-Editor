//! Send to Slicer and To STEP discovery (1.x `tests/test_platform.py`), through the pure
//! `find_slicer_with` / `step_available_with`: no real folders, no real programs.

use std::path::Path;

use stl_platform::handoff::{find_slicer_with, launch_failure_message, open_in, step_available_with, temp_file_name};

/// Runs `find_slicer_with` for `key` on `os`, with these installed paths, commands, flatpaks and
/// folder listing (the 1.x tests' `fake`).
fn find(key: &str, os: &str, paths: &[&str], cmds: &[&str], flatpaks: &[&str], listing: &[&str]) -> Option<Vec<String>> {
    let exists = |p: &str| paths.contains(&p);
    let which = |c: &str| if cmds.contains(&c) { Some(format!("/usr/bin/{c}")) } else { None };
    let flatpak = |a: &str| flatpaks.contains(&a);
    let listdir = |_: &str| listing.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    find_slicer_with(key, os, &exists, &which, &flatpak, &listdir)
}

#[test]
fn windows_finds_exe() {
    let exe = r"C:\Program Files\Bambu Studio\bambu-studio.exe";
    assert_eq!(find("bambu", "windows", &[exe], &[], &[], &[]), Some(vec![exe.to_string()]));
    assert_eq!(find("orca", "windows", &[exe], &[], &[], &[]), None);
}

#[test]
fn windows_searches_the_install_folder() {
    // 1.x: any .exe in the folder whose name contains the key
    let listing = ["readme.txt", "OrcaSlicer-uninstall.exe", "orca-slicer.exe"];
    assert_eq!(
        find("orca", "windows", &[], &[], &[], &listing),
        Some(vec![r"C:\Program Files\OrcaSlicer\OrcaSlicer-uninstall.exe".to_string()])
    );
    assert_eq!(find("bambu", "windows", &[], &[], &[], &["bambu-helper.dll"]), None);
    // no folder search for PrusaSlicer, as 1.x
    assert_eq!(find("prusa", "windows", &[], &[], &[], &["prusa-slicer.exe"]), None);
}

#[test]
fn windows_ignores_mac_and_linux_installs() {
    let paths = ["/Applications/BambuStudio.app"];
    assert_eq!(find("bambu", "windows", &paths, &["bambu-studio"], &["com.bambulab.BambuStudio"], &[]), None);
}

#[test]
fn mac_applications_folder() {
    let paths = ["/Applications/BambuStudio.app"];
    assert_eq!(
        find("bambu", "macos", &paths, &[], &[], &[]),
        Some(vec!["open".into(), "-a".into(), "/Applications/BambuStudio.app".into()])
    );
    assert_eq!(find("orca", "macos", &paths, &[], &[], &[]), None);
}

#[test]
fn mac_user_applications_folder() {
    // 1.x also looks in ~/Applications; the home folder comes from the environment
    let Some(home) = std::env::var_os("HOME").map(|h| h.to_string_lossy().into_owned()) else {
        assert_eq!(find("prusa", "macos", &[], &[], &[], &[]), None);
        return;
    };
    let app = format!("{home}/Applications/PrusaSlicer.app");
    assert_eq!(find("prusa", "macos", &[app.as_str()], &[], &[], &[]), Some(vec!["open".into(), "-a".into(), app.clone()]));
    let orca = "/Applications/OrcaSlicer.app";
    assert_eq!(find("orca", "macos", &[orca], &[], &[], &[]), Some(vec!["open".into(), "-a".into(), orca.into()]));
}

#[test]
fn linux_path_commands() {
    assert_eq!(find("bambu", "linux", &[], &["bambu-studio"], &[], &[]), Some(vec!["/usr/bin/bambu-studio".into()]));
    assert_eq!(find("orca", "linux", &[], &["orcaslicer"], &[], &[]), Some(vec!["/usr/bin/orcaslicer".into()]));
    assert_eq!(find("orca", "linux", &[], &["orca-slicer"], &[], &[]), Some(vec!["/usr/bin/orca-slicer".into()]));
    assert_eq!(find("prusa", "linux", &[], &["prusa-slicer"], &[], &[]), Some(vec!["/usr/bin/prusa-slicer".into()]));
    assert_eq!(find("bambu", "linux", &[], &[], &[], &[]), None);
}

#[test]
fn linux_flatpaks() {
    let f = ["com.bambulab.BambuStudio", "io.github.softfever.OrcaSlicer", "com.prusa3d.PrusaSlicer"];
    assert_eq!(find("bambu", "linux", &[], &[], &f, &[]), Some(vec!["flatpak".into(), "run".into(), "com.bambulab.BambuStudio".into()]));
    assert_eq!(find("orca", "linux", &[], &[], &f, &[]), Some(vec!["flatpak".into(), "run".into(), "io.github.softfever.OrcaSlicer".into()]));
    assert_eq!(find("prusa", "linux", &[], &[], &f, &[]), Some(vec!["flatpak".into(), "run".into(), "com.prusa3d.PrusaSlicer".into()]));
}

#[test]
fn linux_prefers_path_over_flatpak() {
    assert_eq!(
        find("prusa", "linux", &[], &["prusa-slicer"], &["com.prusa3d.PrusaSlicer"], &[]),
        Some(vec!["/usr/bin/prusa-slicer".into()])
    );
}

#[test]
fn unknown_key_is_none() {
    assert_eq!(find("nope", "linux", &[], &["nope"], &[], &[]), None);
    assert_eq!(find("nope", "windows", &[], &[], &[], &[]), None);
    assert_eq!(find("nope", "macos", &[], &[], &[], &[]), None);
}

#[test]
fn solidcraft_is_found_by_its_exe_on_windows_only() {
    let exe = r"C:\Program Files\SolidCraft\solidcraft.exe";
    assert_eq!(find("solidcraft", "windows", &[exe], &[], &[], &[]), Some(vec![exe.to_string()]));
    assert_eq!(find("solidcraft", "macos", &[exe], &[], &[], &[]), None);
    assert_eq!(find("solidcraft", "linux", &[exe], &["solidcraft"], &[], &[]), None);
}

#[test]
fn to_step_only_on_windows_with_the_cli() {
    let cli = r"C:\Program Files\SolidCraft\solidcraft-cli.exe";
    assert!(step_available_with("windows", &|p| p == cli));
    assert!(!step_available_with("windows", &|_| false));
    assert!(!step_available_with("macos", &|_| true));
    assert!(!step_available_with("linux", &|_| true));
}

#[test]
fn launch_failure_says_what_failed() {
    assert_eq!(launch_failure_message("Bambu Studio", "the file is missing"), "Couldn't open Bambu Studio: the file is missing");
}

#[test]
fn open_in_reports_a_missing_program() {
    // a program name nobody has: spawning fails, nothing real is started
    let cmd = vec!["stl-editor-no-such-program-m6-t6".to_string()];
    let err = open_in(&cmd, Path::new("model.3mf")).unwrap_err();
    assert!(!err.is_empty());
    assert!(!err.contains("(os error"), "plain words, not the OS suffix: {err}");
}

#[test]
fn temp_file_names_keep_the_extension_and_drop_odd_characters() {
    assert_eq!(temp_file_name("Bracket (v2).3mf"), "Bracket (v2).3mf");
    assert_eq!(temp_file_name("Body 1/2.stl"), "Body 12.stl");
    assert_eq!(temp_file_name("///.3mf"), "model.3mf");
}

#[test]
fn temp_file_names_cannot_escape_the_folder() {
    // separators and dots in the extension are dropped, so the name stays in the hand-off folder
    assert_eq!(temp_file_name("a.x/.."), "ax");
    assert_eq!(temp_file_name("model.3m/f"), "model.3mf");
    assert_eq!(temp_file_name("../../x.3mf"), "x.3mf");
    for name in ["a.x/..", "../../etc.passwd", "x.\\..\\y", "..", "a..b"] {
        let out = temp_file_name(name);
        assert!(!out.contains('/') && !out.contains('\\') && !out.contains(".."), "{name} -> {out}");
    }
}
