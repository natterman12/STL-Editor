//! Desktop hosts for the Support Fins engine (`web/vendor/support-fins/engine.iife.js`).
//!
//! `FinsHost::call(name, args)` runs `SupportFinsEngine.<name>(...args)`. Every argument is
//! JSON text (a string argument is passed quoted, e.g. `serde_json::to_string(&soup)`), because
//! 1.x passes objects and numbers too (`reportLine(stats)`, `anchorFinB64(..., face, ...)`).
//! A string result comes back as-is; anything else as JSON. An engine error comes back as
//! `String(error)`, e.g. `"Error: message"`; a dead or stuck host as [`STOPPED`].

mod boa;
mod update;
#[cfg(any(windows, target_os = "linux"))]
mod webview;

pub use boa::BoaHost;
pub use update::{
    check_update, choose_engine, digest, finish_update_check, published_url, reset_fins_state, EngineInfo, FinsState,
    Settings,
};
#[cfg(any(windows, target_os = "linux"))]
pub use webview::WebviewHost;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use stl_engine::{FinsHost, HostKind, NullHost};

/// The error of a call on a host that died or ran past the time limit (spec, verbatim).
pub const STOPPED: &str = "Support Fins: the engine stopped responding.";

/// The host for this computer: the system web engine when it starts, else Boa (slow but
/// always there). A missing or unreadable engine file gives the `NullHost`.
///
/// The returned host answers `__info` (no arguments) with the active engine's version
/// fields. `has("__info")` stays false: it is not a function of `SupportFinsEngine`.
pub fn start_fins_host(engine_js: &Path) -> Arc<dyn FinsHost> {
    with_info(host_for(engine_js), engine_js)
}

/// `host` answering `__info` from the `VERSION.json` beside `engine_js`, as `start_fins_host`'s does.
pub fn with_info(host: Arc<dyn FinsHost>, engine_js: &Path) -> Arc<dyn FinsHost> {
    Arc::new(InfoHost { inner: host, version_json: engine_js.with_file_name("VERSION.json") })
}

fn host_for(engine_js: &Path) -> Arc<dyn FinsHost> {
    if !engine_js.is_file() {
        return Arc::new(NullHost);
    }
    // macOS: WKWebView must be made on the main thread, which milestone 4's app will provide
    // a hook for. Until then macOS always uses Boa (listed in EXCEPTIONS.md), and the webview
    // host isn't compiled there.
    #[cfg(any(windows, target_os = "linux"))]
    if let Ok(h) = WebviewHost::start(engine_js) {
        return Arc::new(h);
    }
    match BoaHost::start(engine_js) {
        Ok(h) => Arc::new(h),
        Err(_) => Arc::new(NullHost),
    }
}

/// Wraps whichever host started, so `WebviewHost` and `BoaHost` stay unchanged.
struct InfoHost {
    inner: Arc<dyn FinsHost>,
    version_json: PathBuf,
}

fn info_json(version_json: &Path) -> String {
    let v = std::fs::read(version_json)
        .ok()
        .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
        .unwrap_or(serde_json::Value::Null);
    let field = |k: &str| v.get(k).cloned().unwrap_or(serde_json::Value::Null);
    let pending = match update::pending() {
        Some(p) => serde_json::Value::String(p),
        None => serde_json::Value::Null,
    };
    serde_json::json!({
        "version": field("version"),
        "tag": field("tag"),
        "commit": field("commit"),
        "source": field("source"),
        "pending": pending,
    })
    .to_string()
}

impl FinsHost for InfoHost {
    fn call(&self, name: &str, args: &[&str]) -> Result<String, String> {
        if name == "__info" {
            return Ok(info_json(&self.version_json));
        }
        self.inner.call(name, args)
    }

    fn has(&self, name: &str) -> bool {
        self.inner.has(name)
    }

    fn host_kind(&self) -> HostKind {
        self.inner.host_kind()
    }
}
