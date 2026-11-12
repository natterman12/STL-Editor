//! Support Fins in the browser: `globalThis.SupportFinsEngine` (same convention as 1.x web).

use js_sys::{Function, Reflect, JSON};
use stl_engine::{FinsHost, HostKind};
use wasm_bindgen::{JsCast, JsValue};

/// Calls the loaded `SupportFinsEngine` IIFE. The web app must load `engine.iife.js` first.
pub struct BrowserHost;

fn engine() -> Option<js_sys::Object> {
    let g = js_sys::global();
    let e = Reflect::get(&g, &JsValue::from_str("SupportFinsEngine")).ok()?;
    if e.is_undefined() {
        return None;
    }
    e.dyn_into().ok()
}

fn stringify_result(v: JsValue) -> String {
    if let Some(s) = v.as_string() {
        return s;
    }
    if v.is_undefined() {
        return "null".into();
    }
    JSON::stringify(&v)
        .ok()
        .and_then(|j| j.as_string())
        .unwrap_or_else(|| "null".into())
}

/// Same as JavaScript `String(v)` (how the webview host reports throws).
fn err_string(v: JsValue) -> String {
    let f = Function::new_with_args("v", "return String(v)");
    f.call1(&JsValue::UNDEFINED, &v)
        .ok()
        .and_then(|j| j.as_string())
        .unwrap_or_else(|| "Error".into())
}

impl FinsHost for BrowserHost {
    fn call(&self, name: &str, args: &[&str]) -> Result<String, String> {
        let e = engine().ok_or_else(|| format!("TypeError: SupportFinsEngine.{name} is not a function"))?;
        let raw = Reflect::get(&e, &JsValue::from_str(name))
            .map_err(|_| format!("TypeError: SupportFinsEngine.{name} is not a function"))?;
        let f = raw
            .dyn_ref::<Function>()
            .ok_or_else(|| format!("TypeError: SupportFinsEngine.{name} is not a function"))?;
        let parsed: Vec<JsValue> = args
            .iter()
            .map(|a| JSON::parse(a).map_err(err_string))
            .collect::<Result<_, _>>()?;
        let argv = js_sys::Array::new();
        for v in parsed {
            argv.push(&v);
        }
        match f.apply(&e, &argv) {
            Ok(v) => Ok(stringify_result(v)),
            Err(e) => Err(err_string(e)),
        }
    }

    fn has(&self, name: &str) -> bool {
        let Some(e) = engine() else {
            return false;
        };
        Reflect::get(&e, &JsValue::from_str(name))
            .ok()
            .and_then(|v| v.dyn_ref::<Function>().map(|_| true))
            .unwrap_or(false)
    }

    fn host_kind(&self) -> HostKind {
        HostKind::Browser
    }
}
