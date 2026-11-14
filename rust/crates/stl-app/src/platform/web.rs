//! The browser: a file input to open, a download link to save, and `Date` for the clock.

use js_sys::{Date, Function, Uint8Array};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{Blob, Document, File, HtmlAnchorElement, HtmlElement, HtmlInputElement, Url};

use super::{Pending, PickedFile, Platform};

pub struct WebPlatform;

impl WebPlatform {
    pub fn new() -> Self {
        WebPlatform
    }
}

impl Default for WebPlatform {
    fn default() -> Self {
        Self::new()
    }
}

fn page() -> Result<Document, String> {
    web_sys::window()
        .and_then(|w| w.document())
        .ok_or_else(|| "This page has no document.".to_string())
}

fn call(body: &str, args: &[&JsValue]) -> Result<JsValue, JsValue> {
    let names: Vec<_> = (0..args.len()).map(|i| format!("a{i}")).collect();
    let f = Function::new_with_args(&names.join(","), body);
    let argv = js_sys::Array::new();
    for a in args {
        argv.push(a);
    }
    f.apply(&JsValue::UNDEFINED, &argv)
}

fn element(doc: &Document, tag: &str) -> Result<JsValue, String> {
    call("return a0.createElement(a1)", &[doc.unchecked_ref(), &JsValue::from_str(tag)])
        .map_err(|e| js_string(&e))
}

fn append(parent: &JsValue, child: &JsValue) {
    let _ = call("a0.appendChild(a1)", &[parent, child]);
}

fn listen(target: &JsValue, name: &str, cb: &Function) {
    let _ = call("a0.addEventListener(a1, a2)", &[target, &JsValue::from_str(name), cb]);
}

fn js_string(v: &JsValue) -> String {
    v.as_string().unwrap_or_else(|| {
        Function::new_with_args("v", "return String(v)")
            .call1(&JsValue::UNDEFINED, v)
            .ok()
            .and_then(|s| s.as_string())
            .unwrap_or_else(|| "something went wrong".into())
    })
}

fn accept(filters: &[(&str, &[&str])]) -> String {
    let mut parts = Vec::new();
    for (_, exts) in filters {
        for ext in *exts {
            let ext = ext.trim_start_matches('.');
            if !ext.is_empty() {
                parts.push(format!(".{ext}"));
            }
        }
    }
    parts.join(",")
}

impl Platform for WebPlatform {
    fn open_file(&self, filters: &[(&str, &[&str])]) -> Pending<Option<PickedFile>> {
        let pending = Pending::new();
        if let Err(e) = arm_open(filters, pending.clone()) {
            // A page with no document cannot ask. Leave the pending empty so the UI does not
            // treat a setup failure as "the user cancelled".
            let _ = e;
        }
        pending
    }

    fn save_file(&self, suggested: &str, bytes: Vec<u8>) -> Pending<Result<bool, String>> {
        let pending = Pending::new();
        pending.set(save_download(suggested, &bytes));
        pending
    }

    fn now_iso_utc(&self) -> String {
        let d = Date::new_0();
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
            d.get_utc_full_year(),
            d.get_utc_month() + 1,
            d.get_utc_date(),
            d.get_utc_hours(),
            d.get_utc_minutes(),
            d.get_utc_seconds()
        )
    }

    fn is_web(&self) -> bool {
        true
    }

    fn setting(&self, key: &str) -> Option<String> {
        self.setting_json(key).and_then(|v| v.as_str().map(str::to_owned))
    }

    fn set_setting(&self, key: &str, value: &str) {
        self.set_setting_json(key, serde_json::Value::String(value.to_owned()));
    }

    /// Stored as JSON text in `localStorage` under a `stle2_` key, so the 1.x web edition's
    /// `stle_*` / `stle.*` keys on the same site are never read or written.
    fn setting_json(&self, key: &str) -> Option<serde_json::Value> {
        let (store, f) = local_storage_fn("getItem")?;
        let text = f.call1(&store, &JsValue::from_str(&storage_key(key))).ok()?.as_string()?;
        serde_json::from_str(&text).ok()
    }

    fn set_setting_json(&self, key: &str, value: serde_json::Value) {
        if let Some((store, f)) = local_storage_fn("setItem") {
            let _ = f.call2(&store, &JsValue::from_str(&storage_key(key)), &JsValue::from_str(&value.to_string()));
        }
    }

    /// In IndexedDB `stle2` (store `blobs`), not `localStorage`.
    fn put_blob(&self, key: &str, bytes: &[u8]) -> bool {
        crate::web::store::put_blob(key, bytes);
        true
    }

    fn get_blob(&self, key: &str) -> Option<crate::platform::Pending<Option<Vec<u8>>>> {
        Some(crate::web::store::get_blob(key))
    }
}

/// The `localStorage` key for a setting: `stle2_` + the name (a leading `stle_`, 1.x's prefix,
/// is dropped so `stle_sf_material` is stored as `stle2_sf_material`).
pub fn storage_key(key: &str) -> String {
    format!("stle2_{}", key.strip_prefix("stle_").unwrap_or(key))
}

/// `localStorage.<name>` without needing web-sys's Storage feature.
fn local_storage_fn(name: &str) -> Option<(JsValue, js_sys::Function)> {
    let store = js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str("localStorage")).ok()?;
    if store.is_undefined() || store.is_null() {
        return None;
    }
    let f = js_sys::Reflect::get(&store, &JsValue::from_str(name)).ok()?.dyn_into::<js_sys::Function>().ok()?;
    Some((store, f))
}

fn arm_open(filters: &[(&str, &[&str])], pending: Pending<Option<PickedFile>>) -> Result<(), String> {
    let doc = page()?;
    let el = element(&doc, "input")?;
    let input: HtmlInputElement = el.clone().dyn_into().map_err(|_| "could not make a file input".to_string())?;
    input.set_type("file");
    let accept = accept(filters);
    if !accept.is_empty() {
        input.set_accept(&accept);
    }
    let _ = call(
        "a0.style.position='fixed'; a0.style.left='-10000px'; a0.style.top='0'",
        &[&el],
    );
    let body = doc.body().ok_or_else(|| "This page has no document.".to_string())?;
    append(body.unchecked_ref(), &el);

    let input_change = input.clone();
    let pending_change = pending.clone();
    let el_change = el.clone();
    let on_change = wasm_bindgen::closure::Closure::wrap(Box::new(move |_ev: JsValue| {
        let Some(files) = input_change.files() else {
            pending_change.set(None);
            let _ = call("a0.remove()", &[&el_change]);
            return;
        };
        let Some(file) = files.item(0) else {
            pending_change.set(None);
            let _ = call("a0.remove()", &[&el_change]);
            return;
        };
        let pending = pending_change.clone();
        let el = el_change.clone();
        wasm_bindgen_futures::spawn_local(async move {
            pending.set(read_file(file).await);
            let _ = call("a0.remove()", &[&el]);
        });
    }) as Box<dyn FnMut(JsValue)>);
    listen(&el, "change", on_change.as_ref().unchecked_ref());
    on_change.forget();

    let pending_cancel = pending.clone();
    let el_cancel = el.clone();
    let on_cancel = wasm_bindgen::closure::Closure::wrap(Box::new(move |_ev: JsValue| {
        pending_cancel.set(None);
        let _ = call("a0.remove()", &[&el_cancel]);
    }) as Box<dyn FnMut(JsValue)>);
    listen(&el, "cancel", on_cancel.as_ref().unchecked_ref());
    on_cancel.forget();

    input.unchecked_ref::<HtmlElement>().click();
    Ok(())
}

async fn read_file(file: File) -> Option<PickedFile> {
    let name = file.name();
    let buf = JsFuture::from(file.unchecked_ref::<Blob>().array_buffer()).await.ok()?;
    let bytes_js = Uint8Array::new(&buf);
    let mut bytes = vec![0u8; bytes_js.length() as usize];
    bytes_js.copy_to(&mut bytes);
    Some(PickedFile { name, bytes })
}

fn save_download(suggested: &str, bytes: &[u8]) -> Result<bool, String> {
    let doc = page()?;
    let parts = js_sys::Array::of1(&Uint8Array::from(bytes));
    let blob = Blob::new_with_u8_array_sequence(&parts).map_err(|e| js_string(&e))?;
    let url = Url::create_object_url_with_blob(&blob).map_err(|e| js_string(&e))?;
    let el = element(&doc, "a")?;
    let a: HtmlAnchorElement = el.clone().dyn_into().map_err(|_| "could not make a download link".to_string())?;
    a.set_href(&url);
    a.set_download(suggested);
    let body = doc.body().ok_or_else(|| "This page has no document.".to_string())?;
    append(body.unchecked_ref(), &el);
    a.unchecked_ref::<HtmlElement>().click();
    let _ = call("a0.remove()", &[&el]);
    let url2 = url.clone();
    let revoke = wasm_bindgen::closure::Closure::once(move || {
        let _ = Url::revoke_object_url(&url2);
    });
    if let Some(win) = web_sys::window() {
        let _ = win.set_timeout_with_callback_and_timeout_and_arguments_0(
            revoke.as_ref().unchecked_ref(),
            60_000,
        );
        revoke.forget();
    }
    // The browser does not say whether the user kept the file. The download was offered.
    Ok(true)
}
