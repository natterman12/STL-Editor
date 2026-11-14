//! `cargo run -p xtask -- web [--serve] [--check [--webgl2]] [--measure-preview]`
//!
//! Builds the wasm app into `<CARGO_TARGET_DIR>/web/` with wasm-bindgen 0.2.130 (the same CLI
//! the headless Chrome tests use) and, with `--serve`, serves it on localhost. `--check` opens
//! the page in headless Chrome, loads a project fixture into the running app, and checks the
//! body count the egui browser tree publishes, picks across the view, turns the section view on
//! and checks which graphics backend ran (WebGPU, or with `--webgl2` WebGPU off and WebGL2).

use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const BINDGEN_VERSION: &str = "0.2.130";

pub fn run(args: impl Iterator<Item = String>) -> Result<(), String> {
    let args: Vec<String> = args.collect();
    let serve = args.iter().any(|a| a == "--serve");
    let check = args.iter().any(|a| a == "--check");
    let webgl2 = args.iter().any(|a| a == "--webgl2");
    let measure = args.iter().any(|a| a == "--measure-preview");
    if webgl2 && !check {
        return Err("--webgl2 goes with --check".into());
    }
    if measure && (check || serve) {
        return Err("--measure-preview is on its own".into());
    }
    let out = build(measure)?;
    println!("web: built {}", out.display());
    if measure {
        return measure_preview(&out);
    }
    if check {
        return headless_check(&out, webgl2);
    }
    if serve {
        serve_forever(&out)?;
    }
    Ok(())
}

fn rust_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// `cargo` puts the wasm here. `CARGO_TARGET_DIR` may be relative to the rust workspace.
fn target_dir() -> PathBuf {
    match std::env::var_os("CARGO_TARGET_DIR") {
        Some(dir) => {
            let path = PathBuf::from(dir);
            if path.is_absolute() { path } else { rust_dir().join(path) }
        }
        None => rust_dir().join("target"),
    }
}

fn build(measure: bool) -> Result<PathBuf, String> {
    let rust = rust_dir();
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let mut flags = std::env::var("RUSTFLAGS").unwrap_or_default();
    // One codegen unit so the app binary keeps the worker exports from the library.
    if !flags.contains("codegen-units") {
        if !flags.is_empty() {
            flags.push(' ');
        }
        flags.push_str("-C codegen-units=1");
    }
    // The preview-measurement exports stay out of a release wasm. This cfg is the switch.
    if measure && !flags.contains("stl_preview_measure") {
        if !flags.is_empty() {
            flags.push(' ');
        }
        flags.push_str("--cfg stl_preview_measure");
    }
    let status = Command::new(&cargo)
        .args(["build", "-p", "stl-app", "--target", "wasm32-unknown-unknown", "--release"])
        .current_dir(&rust)
        .env("RUSTFLAGS", &flags)
        .status()
        .map_err(|e| format!("could not run cargo: {e}"))?;
    if !status.success() {
        return Err("cargo build of the wasm app failed".into());
    }
    let wasm = target_dir().join("wasm32-unknown-unknown/release/stl-app.wasm");
    if !wasm.is_file() {
        return Err(format!("missing {}", wasm.display()));
    }
    let out = target_dir().join("web");
    fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    let bindgen = wasm_bindgen_bin()?;
    let status = Command::new(&bindgen)
        .args(["--target", "web", "--out-dir"])
        .arg(&out)
        .arg("--out-name")
        .arg("stl-app")
        .arg(&wasm)
        .status()
        .map_err(|e| format!("could not run wasm-bindgen: {e}"))?;
    if !status.success() {
        return Err("wasm-bindgen failed".into());
    }
    let js_path = out.join("stl-app.js");
    let js = fs::read_to_string(&js_path).map_err(|e| format!("no stl-app.js: {e}"))?;
    if !js.contains("engine_worker") {
        return Err(
            "stl-app.js does not export engine_worker. The app binary dropped the web worker entry."
                .into(),
        );
    }
    let patched = patch_start(&js)?;
    fs::write(&js_path, patched).map_err(|e| e.to_string())?;
    let web = rust.join("crates/stl-app/web");
    fs::copy(web.join("index.html"), out.join("index.html")).map_err(|e| e.to_string())?;
    fs::copy(web.join("worker.js"), out.join("worker.js")).map_err(|e| e.to_string())?;
    fs::copy(web.join("coverage_worker.js"), out.join("coverage_worker.js")).map_err(|e| e.to_string())?;
    // Support Fins' own engine, for the worker to load (rust/crates/stl-app/web/worker.js)
    let fins = out.join("support-fins");
    fs::create_dir_all(&fins).map_err(|e| e.to_string())?;
    let src = rust.join("../web/vendor/support-fins");
    for f in ["engine.iife.js", "VERSION.json"] {
        fs::copy(src.join(f), fins.join(f)).map_err(|e| format!("no support-fins/{f}: {e}"))?;
    }
    Ok(out)
}

fn patch_start(js: &str) -> Result<String, String> {
    const NEEDLE: &str = "wasm.__wbindgen_start();";
    if !js.contains(NEEDLE) {
        return Err("wasm-bindgen output has no wasm.__wbindgen_start(); the worker cannot skip the page entry".into());
    }
    Ok(js.replacen(
        NEEDLE,
        "if (!globalThis.__STL_ENGINE_WORKER__ && !globalThis.__STL_COVERAGE_WORKER__) { wasm.__wbindgen_start(); }",
        1,
    ))
}

fn wasm_bindgen_bin() -> Result<PathBuf, String> {
    if let Some(path) = on_path("wasm-bindgen") {
        if version_ok(&path)? {
            return Ok(path);
        }
    }
    let home = std::env::var("CARGO_HOME").or_else(|_| {
        std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .map(|h| format!("{h}/.cargo"))
    });
    if let Ok(home) = home {
        let name = if cfg!(windows) { "wasm-bindgen.exe" } else { "wasm-bindgen" };
        let path = PathBuf::from(home).join("bin").join(name);
        if path.is_file() && version_ok(&path)? {
            return Ok(path);
        }
    }
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    println!("web: installing wasm-bindgen-cli {BINDGEN_VERSION}");
    let status = Command::new(cargo)
        .args(["install", "wasm-bindgen-cli", "--version", BINDGEN_VERSION, "--locked", "--force"])
        .status()
        .map_err(|e| format!("could not install wasm-bindgen-cli: {e}"))?;
    if !status.success() {
        return Err(format!("could not install wasm-bindgen-cli {BINDGEN_VERSION}"));
    }
    on_path("wasm-bindgen").ok_or_else(|| "wasm-bindgen was installed but is not on PATH".into())
}

fn version_ok(path: &Path) -> Result<bool, String> {
    let out = Command::new(path)
        .arg("-V")
        .output()
        .map_err(|e| format!("could not run {}: {e}", path.display()))?;
    let text = String::from_utf8_lossy(&out.stdout);
    Ok(text.contains(BINDGEN_VERSION))
}

fn on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
        let exe = dir.join(format!("{name}.exe"));
        if exe.is_file() {
            return Some(exe);
        }
    }
    None
}

fn serve_forever(root: &Path) -> Result<(), String> {
    let fixtures = fixture_dir()?;
    let report = Arc::new(Mutex::new(None));
    let running = Arc::new(AtomicBool::new(true));
    let addr = serve_on(root, &fixtures, &report, &running, "127.0.0.1:8734")?;
    println!("web: http://{addr}");
    loop {
        thread::sleep(Duration::from_secs(3600));
    }
}

fn headless_check(root: &Path, webgl2: bool) -> Result<(), String> {
    let fixture = fixture_file()?;
    let fixture_bodies = fixture_bodies(&fixture)?;
    let expect = fixture_bodies.len();
    // The block is the body named like a block (the holed block in basic.stlx), else the first.
    let block = fixture_bodies.iter().find(|(_, n)| n.to_lowercase().contains("block")).unwrap_or(&fixture_bodies[0]).0.clone();
    let ids: Vec<&str> = fixture_bodies.iter().map(|(id, _)| id.as_str()).collect();
    let fixtures = fixture.parent().unwrap().to_path_buf();
    let name = fixture.file_name().and_then(|s| s.to_str()).unwrap_or("basic.stlx");
    let report = Arc::new(Mutex::new(None));
    let running = Arc::new(AtomicBool::new(true));
    let addr = serve_on(root, &fixtures, &report, &running, "127.0.0.1:0")?;
    let url = format!("http://{addr}/?project=/fixtures/{name}&report=1&full=1&bodies={}&block={block}", ids.join(","));
    println!("web: checking {url} (engine bodies: {expect}: {}; block: {block})", describe(&fixture_bodies));
    let chrome = find_chrome()?;
    let profile = std::env::temp_dir().join(if webgl2 { "stl-editor-web-check-webgl2" } else { "stl-editor-web-check" });
    let _ = fs::create_dir_all(&profile);
    // WebGL2 mode: `--disable-features=WebGPU` alone leaves `navigator.gpu` in Chrome 157. With the GPU off
    // (software WebGL via SwiftShader) `requestAdapter()` finds nothing and wgpu falls back to GL.
    let gpu_flags: &[&str] = if webgl2 {
        &["--disable-features=WebGPU", "--disable-gpu", "--enable-unsafe-swiftshader"]
    } else {
        &["--enable-unsafe-webgpu", "--enable-features=WebGPU"]
    };
    let mut child = Command::new(&chrome)
        .args(gpu_flags)
        .args([
            "--headless=new",
            "--window-size=1280,800",
            "--no-first-run",
            "--disable-extensions",
            "--no-sandbox",
            &format!("--user-data-dir={}", profile.display()),
            &url,
        ])
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("could not start {}: {e}", chrome.display()))?;
    let deadline = Instant::now() + Duration::from_secs(90);
    let mut got = None;
    while Instant::now() < deadline {
        if let Some(text) = report.lock().map_err(|e| e.to_string())?.clone() {
            got = Some(text);
            break;
        }
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            running.store(false, Ordering::SeqCst);
            return Err(format!("chrome exited ({status}) before the page reported a body count"));
        }
        thread::sleep(Duration::from_millis(200));
    }
    let _ = child.kill();
    let _ = child.wait();
    running.store(false, Ordering::SeqCst);
    let Some(text) = got else {
        return Err("headless Chrome did not report a body count within 90s".into());
    };
    if let Some(err) = text.strip_prefix("error:") {
        return Err(format!("the page reported: {err}"));
    }
    let (count, status) = text.split_once('|').unwrap_or((text.as_str(), ""));
    let n: usize = count.trim().parse().map_err(|_| format!("the browser tree showed {count:?}, not a count"))?;
    if n != expect {
        return Err(format!("browser tree shows {n} bodies, the fixture has {expect}"));
    }
    println!("web: headless Chrome saw {n} bodies in the browser tree ({name})");
    if let Some(err) = status.strip_prefix("error:") {
        return Err(format!("the pick and section check failed: {err}"));
    }
    // "ok:<backend>:<details>"
    let mut parts = status.strip_prefix("ok:").unwrap_or("").splitn(2, ':');
    let backend = parts.next().unwrap_or("");
    let details = parts.next().unwrap_or("");
    let want = if webgl2 { "Gl" } else { "BrowserWebGpu" };
    if backend != want {
        return Err(format!("the viewport ran on {backend:?}, expected {want:?}{}", if webgl2 { "" } else { " (does this Chrome have WebGPU?)" }));
    }
    if !details.contains(&format!("block={block}@centre")) {
        return Err(format!("the page did not report a pick at the centre of the block {block} that hit it: {details:?}"));
    }
    if !details.contains("section differs in") {
        return Err(format!("the page did not report the section view readback: {details:?}"));
    }
    println!("web: backend {backend}; {details}");
    Ok(())
}

fn describe(bodies: &[(String, String)]) -> String {
    bodies.iter().map(|(id, name)| format!("{id}={name}")).collect::<Vec<_>>().join(", ")
}

/// The fixture's body ids and names, in order.
fn fixture_bodies(path: &Path) -> Result<Vec<(String, String)>, String> {
    let bytes = fs::read(path).map_err(|e| format!("could not read {}: {e}", path.display()))?;
    let doc = stl_engine::stlx::from_bytes(&bytes, stl_engine::Registry::builtin()).map_err(|e| e.0)?;
    let bodies: Vec<(String, String)> = doc.state.bodies.iter().map(|(id, b)| (id.clone(), b.name.clone())).collect();
    if bodies.is_empty() {
        return Err(format!("{} has no bodies", path.display()));
    }
    Ok(bodies)
}

fn fixture_file() -> Result<PathBuf, String> {
    let rust = rust_dir();
    let preferred = rust.join("tests/models/projects");
    if let Some(path) = first_stlx(&preferred) {
        return Ok(path);
    }
    let basic = rust.join("tests/projects/basic.stlx");
    if basic.is_file() {
        return Ok(basic);
    }
    Err("no project fixture under tests/models/projects or tests/projects".into())
}

fn fixture_dir() -> Result<PathBuf, String> {
    Ok(fixture_file()?.parent().unwrap().to_path_buf())
}

fn first_stlx(dir: &Path) -> Option<PathBuf> {
    let rd = fs::read_dir(dir).ok()?;
    let mut names: Vec<PathBuf> = rd.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "stlx")).collect();
    names.sort();
    names.into_iter().next()
}

fn serve_on(
    root: &Path,
    fixtures: &Path,
    report: &Arc<Mutex<Option<String>>>,
    running: &Arc<AtomicBool>,
    addr: &str,
) -> Result<std::net::SocketAddr, String> {
    let listener = TcpListener::bind(addr).map_err(|e| format!("could not listen on {addr}: {e}"))?;
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let bound = listener.local_addr().map_err(|e| e.to_string())?;
    let root = root.to_path_buf();
    let fixtures = fixtures.to_path_buf();
    let report = report.clone();
    let running = running.clone();
    thread::spawn(move || {
        while running.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((sock, _)) => {
                    let root = root.clone();
                    let fixtures = fixtures.clone();
                    let report = report.clone();
                    thread::spawn(move || {
                        let _ = handle(sock, &root, &fixtures, &report);
                    });
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => thread::sleep(Duration::from_millis(30)),
                Err(_) => break,
            }
        }
    });
    Ok(bound)
}

fn handle(mut sock: TcpStream, root: &Path, fixtures: &Path, report: &Mutex<Option<String>>) -> Result<(), String> {
    let _ = sock.set_read_timeout(Some(Duration::from_secs(5)));
    let mut buf = Vec::new();
    let mut tmp = [0u8; 4096];
    let header_end;
    loop {
        let n = sock.read(&mut tmp).map_err(|e| e.to_string())?;
        if n == 0 {
            return Ok(());
        }
        buf.extend_from_slice(&tmp[..n]);
        if let Some(i) = find_header_end(&buf) {
            header_end = i;
            break;
        }
        if buf.len() > 1024 * 1024 {
            return Ok(());
        }
    }
    let head = String::from_utf8_lossy(&buf[..header_end]).to_string();
    let mut lines = head.split("\r\n");
    let req = lines.next().unwrap_or("");
    let mut parts = req.split_whitespace();
    let method = parts.next().unwrap_or("");
    let target = parts.next().unwrap_or("/");
    let path = target.split('?').next().unwrap_or("/");
    let mut len = 0usize;
    for line in lines {
        let lower = line.to_ascii_lowercase();
        if let Some(v) = lower.strip_prefix("content-length:") {
            len = v.trim().parse().unwrap_or(0);
        }
    }
    let mut body = buf[header_end + 4..].to_vec();
    while body.len() < len {
        let n = sock.read(&mut tmp).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&tmp[..n]);
    }
    body.truncate(len);
    if method == "POST" && path == "/__report" {
        let text = String::from_utf8_lossy(&body).trim().to_string();
        *report.lock().map_err(|e| e.to_string())? = Some(text);
        respond(&mut sock, "204 No Content", "text/plain", b"")?;
        return Ok(());
    }
    if method != "GET" && method != "HEAD" {
        respond(&mut sock, "405 Method Not Allowed", "text/plain", b"")?;
        return Ok(());
    }
    let file = if let Some(name) = path.strip_prefix("/fixtures/") {
        if name.contains('/') || name.contains('\\') || name.contains("..") {
            None
        } else {
            Some(fixtures.join(name))
        }
    } else {
        let rel = if path == "/" { "index.html" } else { path.trim_start_matches('/') };
        if rel.contains("..") { None } else { Some(root.join(rel)) }
    };
    let Some(file) = file.filter(|p| p.is_file()) else {
        respond(&mut sock, "404 Not Found", "text/plain", b"not found")?;
        return Ok(());
    };
    let bytes = fs::read(&file).map_err(|e| e.to_string())?;
    let ctype = content_type(&file);
    if method == "HEAD" {
        respond(&mut sock, "200 OK", ctype, b"")?;
    } else {
        respond(&mut sock, "200 OK", ctype, &bytes)?;
    }
    Ok(())
}

fn find_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n")
}

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "wasm" => "application/wasm",
        "json" => "application/json",
        _ => "application/octet-stream",
    }
}

fn respond(sock: &mut TcpStream, status: &str, content_type: &str, body: &[u8]) -> Result<(), String> {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    );
    sock.write_all(head.as_bytes()).map_err(|e| e.to_string())?;
    sock.write_all(body).map_err(|e| e.to_string())?;
    Ok(())
}

const MEASURE_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head><meta charset="utf-8"><title>preview measure</title></head>
<body>
<script type="module">
const pass = function (fn) { return fn; };
if (typeof WebAssembly.Suspending !== "function") WebAssembly.Suspending = pass;
if (typeof WebAssembly.promising !== "function") WebAssembly.promising = pass;
const init = async () => {
  const mod = await import("./stl-app.js");
  await mod.default();
  return mod;
};
const report = async (text) => {
  await fetch("/__report", { method: "POST", body: text });
};
const watch = () => {
  const tasks = [];
  let obs = null;
  try {
    obs = new PerformanceObserver((list) => {
      for (const e of list.getEntries()) tasks.push(e.duration);
    });
    obs.observe({ type: "longtask", buffered: false });
  } catch (e) {}
  return {
    max: () => tasks.reduce((m, d) => Math.max(m, d), 0),
    stop: () => { if (obs) obs.disconnect(); },
  };
};
// The same strip `bench_mesh` builds, as the bytes an engine result would hand the page.
function strip(tris) {
  const nvert = tris + 2;
  const pos = new Float32Array(nvert * 3);
  for (let i = 0; i < nvert; i++) {
    pos[i * 3] = i;
    pos[i * 3 + 1] = (i % 2) ? 1 : 0;
    pos[i * 3 + 2] = 0;
  }
  const tri = new Uint32Array(tris * 3);
  for (let t = 0; t < tris; t++) {
    const i = t;
    if (t % 2 === 0) {
      tri[t * 3] = i; tri[t * 3 + 1] = i + 1; tri[t * 3 + 2] = i + 2;
    } else {
      tri[t * 3] = i; tri[t * 3 + 1] = i + 2; tri[t * 3 + 2] = i + 1;
    }
  }
  return { pos: new Uint8Array(pos.buffer), tri: new Uint8Array(tri.buffer) };
}
try {
  const mod = await init();
  await new Promise((r) => setTimeout(r, 300));
  const alloc = mod.stl_preview_measure_alloc();
  if (alloc !== "ok") throw new Error(alloc);
  await new Promise((r) => setTimeout(r, 200));
  const before = watch();
  const t0 = performance.now();
  const blocking = mod.stl_preview_measure_blocking();
  const beforeCall = performance.now() - t0;
  await new Promise((r) => setTimeout(r, 50));
  before.stop();
  if (blocking !== "ok") throw new Error(blocking);
  const begun = mod.stl_preview_measure_begin();
  if (begun !== "ok") throw new Error(begun);
  const after = watch();
  let status = "pending";
  let slices = 0;
  let maxSlice = 0;
  let deadline = performance.now() + 120000;
  while (status === "pending") {
    if (performance.now() > deadline) throw new Error("slices did not finish in 120s");
    const s0 = performance.now();
    status = mod.stl_preview_measure_slice();
    maxSlice = Math.max(maxSlice, performance.now() - s0);
    slices++;
    await new Promise((r) => requestAnimationFrame(r));
  }
  await new Promise((r) => setTimeout(r, 50));
  after.stop();
  if (status !== "done") throw new Error(status);
  // Shader compile is not part of a preview pause.
  const gpu = await mod.stl_preview_measure_gpu();
  if (gpu !== "ok") throw new Error(gpu);
  const bytes = strip(500000);
  const appBlockWatch = watch();
  const b0 = performance.now();
  const appBlocking = mod.stl_preview_measure_app_blocking(bytes.pos, bytes.tri);
  const appBlockingMs = performance.now() - b0;
  await new Promise((r) => setTimeout(r, 50));
  appBlockWatch.stop();
  if (String(appBlocking).startsWith("error:")) throw new Error(appBlocking);
  const appWatch = watch();
  status = "pending";
  let frames = 0;
  let maxFrame = 0;
  let appDetail = "";
  deadline = performance.now() + 180000;
  while (status === "pending" || status === "upload") {
    if (performance.now() > deadline) throw new Error("app frames did not finish in 180s");
    const s0 = performance.now();
    status = frames === 0
      ? mod.stl_preview_measure_app_begin(bytes.pos, bytes.tri)
      : mod.stl_preview_measure_app_frame();
    maxFrame = Math.max(maxFrame, performance.now() - s0);
    frames++;
    appDetail = status;
    if (String(status).startsWith("error:")) throw new Error(status);
    await new Promise((r) => requestAnimationFrame(r));
  }
  await new Promise((r) => setTimeout(r, 50));
  appWatch.stop();
  await report([
    "before_call_ms=" + beforeCall.toFixed(1),
    "before_longtask_ms=" + before.max().toFixed(1),
    "after_max_slice_ms=" + maxSlice.toFixed(1),
    "after_longtask_ms=" + after.max().toFixed(1),
    "slices=" + slices,
    "app_blocking_ms=" + appBlockingMs.toFixed(1),
    "app_blocking_longtask_ms=" + appBlockWatch.max().toFixed(1),
    "app_blocking_detail=" + appBlocking,
    "app_max_frame_ms=" + maxFrame.toFixed(1),
    "app_longtask_ms=" + appWatch.max().toFixed(1),
    "app_frames=" + frames,
    "app_detail=" + appDetail,
  ].join(" "));
} catch (e) {
  await report("error:" + (e && e.message ? e.message : e));
}
</script>
</body>
</html>
"#;

/// A 500,000-triangle preview in headless Chrome. Prints the old prep-only pauses and the
/// main-thread pauses of receiving that preview, preparing it and uploading it.
fn measure_preview(root: &Path) -> Result<(), String> {
    fs::write(root.join("measure.html"), MEASURE_HTML).map_err(|e| e.to_string())?;
    let fixtures = fixture_dir()?;
    let report = Arc::new(Mutex::new(None));
    let running = Arc::new(AtomicBool::new(true));
    let addr = serve_on(root, &fixtures, &report, &running, "127.0.0.1:0")?;
    let url = format!("http://{addr}/measure.html");
    println!("web: measuring a 500,000-triangle preview at {url}");
    let chrome = find_chrome()?;
    let profile = std::env::temp_dir().join("stl-editor-web-measure");
    let _ = fs::create_dir_all(&profile);
    let mut child = Command::new(&chrome)
        .args(["--enable-unsafe-webgpu", "--enable-features=WebGPU"])
        .args([
            "--headless=new",
            "--window-size=1280,800",
            "--no-first-run",
            "--disable-extensions",
            "--no-sandbox",
            &format!("--user-data-dir={}", profile.display()),
            &url,
        ])
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("could not start {}: {e}", chrome.display()))?;
    let deadline = Instant::now() + Duration::from_secs(300);
    let mut got = None;
    while Instant::now() < deadline {
        if let Some(text) = report.lock().map_err(|e| e.to_string())?.clone() {
            got = Some(text);
            break;
        }
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            running.store(false, Ordering::SeqCst);
            return Err(format!("chrome exited ({status}) before the preview measurement"));
        }
        thread::sleep(Duration::from_millis(200));
    }
    let _ = child.kill();
    let _ = child.wait();
    running.store(false, Ordering::SeqCst);
    let Some(text) = got else {
        return Err("headless Chrome did not report a preview measurement within 300s".into());
    };
    println!("web: measure-preview {text}");
    if let Some(err) = text.strip_prefix("error:") {
        return Err(format!("the page reported: {err}"));
    }
    Ok(())
}

fn find_chrome() -> Result<PathBuf, String> {
    if let Some(p) = std::env::var_os("CHROME_PATH") {
        let path = PathBuf::from(p);
        if path.is_file() {
            return Ok(path);
        }
    }
    for name in ["chrome", "chrome.exe", "google-chrome", "google-chrome-stable", "msedge", "msedge.exe"] {
        if let Some(path) = on_path(name) {
            return Ok(path);
        }
    }
    let candidates = [
        r"C:\Program Files\Google\Chrome\Application\chrome.exe",
        r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
        r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
        r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
    ];
    for c in candidates {
        let path = PathBuf::from(c);
        if path.is_file() {
            return Ok(path);
        }
    }
    Err("could not find Chrome or Edge for the headless check".into())
}
