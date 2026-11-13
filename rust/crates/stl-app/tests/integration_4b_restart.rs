//! Help > About Support Fins offers "Restart now" when a newer engine has been downloaded
//! (1.x `aboutSupportFins`). Its own file: the engine's schema is installed once per process.
#![cfg(not(target_arch = "wasm32"))]

use std::path::Path;
use std::sync::{Arc, Mutex};

use egui_kittest::Harness;
use serde_json::json;
use stl_app::app::StlApp;
use stl_app::engine_link::{Command, EngineLink, Update};
use stl_app::platform::NullPlatform;
use stl_app::supports;

#[derive(Clone, Default)]
struct Rec(Arc<Mutex<Vec<Update>>>);
impl EngineLink for Rec {
    fn send(&self, _c: Command) {}
    fn poll(&self) -> Vec<Update> {
        std::mem::take(&mut *self.0.lock().unwrap())
    }
    fn restart(&self) {}
}

#[test]
fn about_support_fins_offers_restart_now_when_a_new_engine_waits() {
    let js = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../web/vendor/support-fins/engine.iife.js");
    let host = stl_platform::fins::start_fins_host(&js);
    let mut info = stl_engine::features::common::fins::info(&*host).expect("info");
    info.as_object_mut().unwrap().insert("pending".into(), json!("v9.9.9"));
    supports::install(&info).unwrap();
    let a = supports::about(false).unwrap();
    assert!(a.paragraphs.last().unwrap().contains("9.9.9"));
    let app = StlApp::new(&egui::Context::default(), Box::new(NullPlatform), Box::new(Rec::default()));
    let mut h = Harness::builder().with_size(egui::vec2(1280.0, 800.0)).with_accessibility_check(false).build_ui_state(
        {
            let mut init = false;
            move |ui, a: &mut StlApp| {
                if !init {
                    init = true;
                    stl_app::fonts::install(ui.ctx());
                }
                a.layout(ui)
            }
        },
        app,
    );
    h.run_steps(2);
    h.state_mut().show_sf_about();
    h.run_steps(3);
    use egui_kittest::kittest::Queryable;
    assert!(h.query_by_label("Restart now").is_some(), "the dialog has a Restart now link");
    assert!(h.query_by_label_contains("Restart now").is_some());
}

/// The restart helper takes the program's path from the environment: a folder with curly quotes and
/// brackets in its name changes nothing about what runs.
#[test]
fn the_restart_helper_survives_odd_folder_names() {
    let exe = std::path::Path::new("C:/Users/it\u{2019}s [mine] $x/STL Editor/stl-app.exe");
    let c = StlApp::restart_helper_command(exe, 4242);
    let args: Vec<String> = c.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
    let text = args.join(" ");
    #[cfg(windows)]
    assert!(!text.contains("STL Editor"), "the path is not in the script text: {text}");
    #[cfg(not(windows))]
    {
        // sh -c <script> stle-restart <pid> <exe>: the path is its own argument ($2), never in the script
        assert_eq!(args[0], "-c");
        assert!(!args[1].contains("STL Editor"), "the path is not in the script text: {}", args[1]);
        assert_eq!(args.last().map(String::as_str), Some(exe.to_str().unwrap()));
    }
    #[cfg(windows)]
    {
        assert!(text.contains("$env:STLE_EXE") && text.contains("Process]::Start"), "{text}");
        let program = std::path::PathBuf::from(c.get_program());
        let tail: Vec<String> = program.components().rev().take(4).map(|p| p.as_os_str().to_string_lossy().to_lowercase()).collect();
        assert_eq!(tail, ["powershell.exe", "v1.0", "windowspowershell", "system32"], "the full path: {program:?}");
        let env = c.get_envs().find(|(k, _)| *k == "STLE_EXE").and_then(|(_, v)| v.map(|v| v.to_owned()));
        assert_eq!(env.as_deref(), Some(exe.as_os_str()));
    }
}

/// And it really starts the program afterwards (a .cmd that leaves a marker file), from a folder
/// named with a curly quote and brackets.
#[cfg(windows)]
#[test]
fn the_restart_helper_starts_a_program_in_an_odd_folder() {
    let d = std::env::temp_dir().join(format!("stle it\u{2019}s [restart] {}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let cmd = d.join("go.cmd");
    std::fs::write(&cmd, "@echo started> \"%~dp0marker.txt\"\r\n").unwrap();
    // a process id that is already gone: the helper's wait ends at once
    let mut done = std::process::Command::new("cmd").args(["/c", "exit"]).spawn().unwrap();
    let pid = done.id();
    done.wait().unwrap();
    let mut helper = StlApp::restart_helper_command(&cmd, pid);
    // a .cmd needs a console to run in; the real target is a window program, which does not
    use std::os::windows::process::CommandExt;
    helper.creation_flags(0);
    helper.spawn().unwrap();
    let end = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while !d.join("marker.txt").exists() {
        assert!(std::time::Instant::now() < end, "the helper never started the program");
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    let _ = std::fs::remove_dir_all(&d);
}
