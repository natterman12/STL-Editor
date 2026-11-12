//! Support Fins update check (1.x `supportfins.py`): a newer valid engine is kept for the
//! next `choose_engine`, and a bad download never replaces the one already in use.
//!
//! The local publish folder is served by `dev/parity/fins_update_server.py`.

use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{BufRead, BufReader};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;
use stl_engine::HostKind;
use stl_platform::fins::{
    check_update, choose_engine, digest, finish_update_check, published_url, reset_fins_state, start_fins_host, EngineInfo,
    FinsState, Settings,
};
use stl_platform::net::https_get;

static GATE: Mutex<()> = Mutex::new(());

fn gate() -> MutexGuard<'static, ()> {
    GATE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Drop order: this runs while the test still holds [`GATE`], so a background check cannot
/// leak into the next test.
struct Clean {
    dir: PathBuf,
}

impl Drop for Clean {
    fn drop(&mut self) {
        let _ = finish_update_check();
        reset_fins_state();
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn scratch(name: &str) -> (PathBuf, Clean) {
    let dir = std::env::temp_dir().join(format!("stl-fins-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    reset_fins_state();
    (dir.clone(), Clean { dir })
}

fn python() -> PathBuf {
    let known = PathBuf::from(r"C:\Program Files\Python314\python.exe");
    if known.is_file() { known } else { PathBuf::from("python") }
}

struct Server {
    child: Child,
    port: u16,
}

impl Server {
    fn start(dir: &Path) -> Self {
        let mut child = Command::new(python())
            .arg(repo().join("dev/parity/fins_update_server.py"))
            .arg(dir)
            .arg("0")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("start fins_update_server.py");
        let stdout = child.stdout.take().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut line = String::new();
            let _ = BufReader::new(stdout).read_line(&mut line);
            let _ = tx.send(line);
        });
        let line = rx.recv_timeout(Duration::from_secs(15)).expect("update server printed no port");
        let port: u16 = line.trim().parse().unwrap_or_else(|_| {
            let mut err = String::new();
            if let Some(mut e) = child.stderr.take() {
                use std::io::Read;
                let _ = e.read_to_string(&mut err);
            }
            panic!("update server port {line:?}; stderr: {err}");
        });
        Server { child, port }
    }

    fn base(&self) -> String {
        format!("http://127.0.0.1:{}/", self.port)
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn write_engine(dir: &Path, tag: &str, body: &[u8]) {
    fs::create_dir_all(dir).unwrap();
    fs::write(dir.join("engine.iife.js"), body).unwrap();
    let version = tag.trim_start_matches('v');
    let info = serde_json::json!({
        "tag": tag,
        "version": version,
        "commit": format!("commit-{version}"),
        "source": "test",
        "api": 1,
        "sha256": { "engine.iife.js": digest(body) },
    });
    fs::write(dir.join("VERSION.json"), serde_json::to_vec(&info).unwrap()).unwrap();
}

/// Publish `body` as `tag`. `sha` overrides the checksum (a bad one is refused).
fn publish(dir: &Path, tag: &str, api: i64, body: &[u8], sha: Option<&str>) {
    fs::create_dir_all(dir).unwrap();
    fs::write(dir.join("engine.iife.js"), body).unwrap();
    let version = tag.trim_start_matches('v');
    let info = serde_json::json!({
        "tag": tag,
        "version": version,
        "commit": format!("commit-{version}"),
        "source": "published",
        "api": api,
        "sha256": { "engine.iife.js": sha.unwrap_or(&digest(body)) },
    });
    fs::write(dir.join("VERSION.json"), serde_json::to_vec(&info).unwrap()).unwrap();
}

fn settings(bundled: &Path, config: &Path, base: &str) -> Settings {
    let mut s = Settings::new(bundled, config);
    s.repo = Some(base.trim_end_matches('/').to_string());
    s
}

fn run(settings: &Settings, force: bool) -> FinsState {
    check_update(settings, force);
    finish_update_check()
}

fn engine_bytes(user: &Path) -> Option<Vec<u8>> {
    fs::read(user.join("engine.iife.js")).ok()
}

#[test]
fn digest_ignores_crlf() {
    let _g = gate();
    let lf = b"line\nline\n";
    let crlf = b"line\r\nline\r\n";
    assert_eq!(digest(lf), digest(crlf), "CRLF and LF are the same bundle");
    assert_eq!(digest(lf), format!("{:x}", Sha256::digest(lf)));
    assert_ne!(digest(crlf), format!("{:x}", Sha256::digest(crlf)));
    assert_eq!(digest(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");

    let (root, _clean) = scratch("crlf");
    let dir = root.join("eng");
    write_engine(&dir, "v1.2.3", crlf);
    let info = choose_engine(&dir, &root.join("none")).expect("a CRLF checkout is valid");
    assert_eq!(info.tag, "v1.2.3");
    // A tampered file, even with a CRLF comment, is not used.
    fs::write(dir.join("engine.iife.js"), b"line\r\nline\r\n// tampered\r\n").unwrap();
    assert!(choose_engine(&dir, &root.join("none")).is_err());
}

#[test]
fn newer_engine_used_next_start() {
    let _g = gate();
    let (root, _clean) = scratch("newer");
    let bundled = root.join("bundled");
    let config = root.join("config");
    let user = config.join("support-fins");
    write_engine(&bundled, "v1.0.5", b"bundled-engine-v1.0.5");
    let first: EngineInfo = choose_engine(&bundled, &user).expect("bundled engine");
    assert_eq!(first.tag, "v1.0.5");
    assert_eq!(first.path, bundled.join("engine.iife.js"));

    let pubdir = root.join("pub");
    publish(&pubdir, "v9.9.9", 1, b"published-engine-v9.9.9", None);
    let server = Server::start(&pubdir);
    assert_eq!(published_url(Some(&format!("http://127.0.0.1:{}", server.port))), server.base());
    let got = https_get(&format!("{}VERSION.json", server.base()), Duration::from_secs(5)).expect("http get");
    assert!(got.windows(6).any(|w| w == b"v9.9.9"));

    let st = run(&settings(&bundled, &config, &server.base()), true);
    assert_eq!(st.pending.as_deref(), Some("v9.9.9"), "pending is the downloaded tag; error={:?}", st.error);
    assert!(st.error.is_none(), "{:?}", st.error);
    assert!(st.checked.is_some());
    // The download is on disk, and the next choose (the next start) uses it.
    let next = choose_engine(&bundled, &user).unwrap();
    assert_eq!(next.tag, "v9.9.9");
    assert_eq!(next.folder, user);
    assert_eq!(fs::read(next.path).unwrap(), b"published-engine-v9.9.9");
    // Nothing newer the second time.
    let again = run(&settings(&bundled, &config, &server.base()), true);
    assert!(again.pending.as_deref() == Some("v9.9.9") || again.pending.is_none());
    assert_eq!(choose_engine(&bundled, &user).unwrap().tag, "v9.9.9");
    assert!(again.error.is_none(), "{:?}", again.error);
}

#[test]
fn bad_update_refused() {
    let _g = gate();
    let (root, _clean) = scratch("bad");
    let bundled = root.join("bundled");
    let config = root.join("config");
    let user = config.join("support-fins");
    let previous = b"previous-valid-engine";
    write_engine(&bundled, "v1.0.5", b"bundled-engine");
    write_engine(&user, "v1.0.0", previous);
    let previous_js = fs::read(user.join("engine.iife.js")).unwrap();
    let previous_ver = fs::read(user.join("VERSION.json")).unwrap();

    let pubdir = root.join("pub");
    fs::create_dir_all(&pubdir).unwrap();
    let server = Server::start(&pubdir);
    let settings = settings(&bundled, &config, &server.base());
    let full = b"brand-new-engine-bytes";

    // Bad checksum: the file is intact, the published hash is not.
    publish(&pubdir, "v9.9.9", 1, full, Some("0000000000000000000000000000000000000000000000000000000000000000"));
    let st = run(&settings, true);
    assert!(st.error.as_deref().unwrap_or("").contains("checksum mismatch"), "bad checksum: {st:?}");
    assert!(st.pending.is_none(), "{st:?}");
    assert_eq!(engine_bytes(&user).unwrap(), previous_js);
    assert_eq!(choose_engine(&bundled, &user).unwrap().tag, "v1.0.5");

    // Cut-off file: VERSION.json names the full engine, the served file is only a prefix.
    fs::write(pubdir.join("engine.iife.js"), &full[..8]).unwrap();
    let hash = digest(full);
    publish(&pubdir, "v9.9.9", 1, &full[..8], Some(hash.as_str()));
    let st = run(&settings, true);
    assert!(st.error.as_deref().unwrap_or("").contains("checksum mismatch"), "cut-off file: {st:?}");
    assert_eq!(engine_bytes(&user).unwrap(), previous_js);
    assert_eq!(fs::read(user.join("VERSION.json")).unwrap(), previous_ver);
    assert_eq!(choose_engine(&bundled, &user).unwrap().tag, "v1.0.5");

    // A different integration api is refused; the previous engine stays.
    publish(&pubdir, "v9.9.9", 2, full, None);
    let st = run(&settings, true);
    assert!(st.error.as_deref().unwrap_or("").to_lowercase().contains("api"), "different api: {st:?}");
    assert!(st.pending.is_none(), "{st:?}");
    assert_eq!(engine_bytes(&user).unwrap(), previous_js);
    assert_eq!(fs::read(user.join("VERSION.json")).unwrap(), previous_ver);
    assert_eq!(choose_engine(&bundled, &user).unwrap().tag, "v1.0.5");
}

#[test]
fn offline_check_is_quiet() {
    let _g = gate();
    let (root, _clean) = scratch("offline");
    let bundled = root.join("bundled");
    let config = root.join("config");
    write_engine(&bundled, "v1.0.5", b"bundled-engine");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let settings = settings(&bundled, &config, &format!("http://127.0.0.1:{port}/"));
    let st = run(&settings, true);
    assert!(st.error.as_ref().is_some_and(|e| !e.is_empty()), "a failed check is recorded, not thrown: {st:?}");
    assert!(st.pending.is_none(), "{st:?}");
    assert!(engine_bytes(&config.join("support-fins")).is_none());
    assert_eq!(choose_engine(&bundled, &config.join("support-fins")).unwrap().tag, "v1.0.5");
    // Still quiet on the follow-up: no panic, the bundled engine stays.
    let st2 = run(&settings, false);
    assert!(st2.pending.is_none());
    assert_eq!(choose_engine(&bundled, &config.join("support-fins")).unwrap().tag, "v1.0.5");
}

#[test]
fn once_a_day() {
    let _g = gate();
    assert_eq!(
        published_url(None),
        "https://natterman12.github.io/STL-Editor/app/vendor/support-fins/"
    );
    assert_eq!(published_url(Some("noslash")), "");

    let (root, _clean) = scratch("daily");
    let bundled = root.join("bundled");
    let config = root.join("config");
    let user = config.join("support-fins");
    write_engine(&bundled, "v1.0.0", b"bundled-1.0.0");
    let pubdir = root.join("pub");
    publish(&pubdir, "v1.0.1", 1, b"engine-1.0.1", None);
    let server = Server::start(&pubdir);
    let mut settings = settings(&bundled, &config, &server.base());

    // Update checks switched off: nothing is fetched.
    settings.update_check = false;
    let st = run(&settings, false);
    assert!(st.pending.is_none(), "{st:?}");
    assert!(engine_bytes(&user).is_none());
    settings.update_check = true;

    let st = run(&settings, false);
    assert_eq!(st.pending.as_deref(), Some("v1.0.1"), "{st:?}");
    assert_eq!(engine_bytes(&user).unwrap(), b"engine-1.0.1");
    let saved: Value = serde_json::from_slice(&fs::read(config.join("settings.json")).unwrap()).unwrap();
    let last = saved["supportfins_last"].as_i64().unwrap();
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;
    assert!((now - last).abs() < 30, "supportfins_last {last} vs {now}");

    // A newer publish the same day is left for tomorrow, unless the check is forced.
    publish(&pubdir, "v1.0.2", 1, b"engine-1.0.2", None);
    let st = run(&settings, false);
    assert_eq!(engine_bytes(&user).unwrap(), b"engine-1.0.1", "the same-day check must not download; {st:?}");
    assert_eq!(choose_engine(&bundled, &user).unwrap().tag, "v1.0.1");
    let st = run(&settings, true);
    assert_eq!(st.pending.as_deref(), Some("v1.0.2"), "{st:?}");
    assert_eq!(engine_bytes(&user).unwrap(), b"engine-1.0.2");
    assert_eq!(choose_engine(&bundled, &user).unwrap().tag, "v1.0.2");
}

#[test]
fn info_call_reports_engine_and_pending() {
    let _g = gate();
    let (root, _clean) = scratch("info");
    let missing = start_fins_host(Path::new("no/such/engine.iife.js"));
    let blank: Value = serde_json::from_str(&missing.call("__info", &[]).expect("__info on a missing engine")).unwrap();
    assert!(blank["version"].is_null() && blank["tag"].is_null() && blank["pending"].is_null());
    assert!(!missing.has("__info"));
    assert_eq!(missing.host_kind(), HostKind::None);
    assert!(missing.call("ping", &[]).is_err());

    let bundled = root.join("bundled");
    let js = b"var SupportFinsEngine = { ping: function () { return \"pong\"; } };\n";
    write_engine(&bundled, "v1.0.5", js);
    let host = start_fins_host(&bundled.join("engine.iife.js"));
    assert!(!host.has("__info"), "has(\"__info\") stays false");
    assert!(host.has("ping"));
    let info: Value = serde_json::from_str(&host.call("__info", &[]).unwrap()).unwrap();
    assert_eq!(info["version"], "1.0.5");
    assert_eq!(info["tag"], "v1.0.5");
    assert_eq!(info["commit"], "commit-1.0.5");
    assert_eq!(info["source"], "test");
    assert!(info["pending"].is_null());
    assert_eq!(host.call("ping", &[]).unwrap(), "pong");

    let config = root.join("config");
    let pubdir = root.join("pub");
    publish(&pubdir, "v1.0.6", 1, b"next-engine", None);
    let server = Server::start(&pubdir);
    let st = run(&settings(&bundled, &config, &server.base()), true);
    assert_eq!(st.pending.as_deref(), Some("v1.0.6"), "{st:?}");
    let info: Value = serde_json::from_str(&host.call("__info", &[]).unwrap()).unwrap();
    assert_eq!(info["version"], "1.0.5", "the running host keeps its engine");
    assert_eq!(info["tag"], "v1.0.5");
    assert_eq!(info["pending"], "v1.0.6");
    assert_eq!(choose_engine(&bundled, &config.join("support-fins")).unwrap().tag, "v1.0.6");
}
