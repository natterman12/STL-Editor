//! The browser's File System Access API (Chrome and Edge): open a file and keep its handle, so
//! Save writes straight back to it and File › Recent can open it again, as the 1.x web edition
//! did. Browsers without it (Firefox, Safari) keep the file input and the download link.
//!
//! A kept handle is named by a token, `file-handle/<id>/<file name>`, which the app treats as
//! the file's path (its last part is the name). Handles live in this page's memory and in
//! IndexedDB `stle2-files`, so a token from an earlier visit still finds its file; the browser
//! then asks for permission again.

use js_sys::{Array, Function, Promise, Reflect, Uint8Array};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

use super::PickedFile;

/// What every token starts with.
pub const TOKEN_PREFIX: &str = "file-handle/";

/// The page side. Evaluated once; the object it returns is kept as `globalThis.__stlFsa`.
/// `globalThis.__stlLaunch` is filled by `index.html` with files the installed app was opened
/// with (`{name, bytes, handle}`).
const PAGE: &str = r#"
const mem = new Map();
const open = () => new Promise((res, rej) => {
  const r = indexedDB.open('stle2-files', 1);
  r.onupgradeneeded = () => r.result.createObjectStore('handles');
  r.onsuccess = () => res(r.result);
  r.onerror = () => rej(r.error);
});
const store = async (mode, fn) => {
  const d = await open();
  try {
    return await new Promise((res, rej) => {
      const tx = d.transaction('handles', mode);
      const rq = fn(tx.objectStore('handles'));
      tx.oncomplete = () => res(rq ? rq.result : undefined);
      tx.onerror = () => rej(tx.error);
      tx.onabort = () => rej(tx.error);
    });
  } finally { d.close(); }
};
const id = () => (globalThis.crypto && crypto.randomUUID)
  ? crypto.randomUUID() : Date.now().toString(36) + Math.random().toString(36).slice(2);
const keep = (h) => {
  const t = 'file-handle/' + id() + '/' + h.name;
  mem.set(t, h);
  store('readwrite', s => s.put(h, t)).catch(() => {});
  return t;
};
const token = async (h) => {
  for (const [t, o] of mem) { try { if (await o.isSameEntry(h)) return t; } catch (e) {} }
  return keep(h);
};
const find = async (t) => {
  if (mem.has(t)) return mem.get(t);
  let h;
  try { h = await store('readonly', s => s.get(t)); } catch (e) {}
  if (h) mem.set(t, h);
  return h || null;
};
const allowed = async (h, mode) => {
  try {
    if ((await h.queryPermission({ mode })) === 'granted') return true;
    return (await h.requestPermission({ mode })) === 'granted';
  } catch (e) { return false; }
};
const write = async (h, bytes) => { const w = await h.createWritable(); await w.write(bytes); await w.close(); };
const types = (name) => {
  const m = /\.[A-Za-z0-9]+$/.exec(name);
  if (!m) return undefined;
  const ext = m[0].toLowerCase();
  return [{ description: ext.slice(1).toUpperCase() + ' file', accept: { 'application/octet-stream': [ext] } }];
};
const cancelled = (e) => e && e.name === 'AbortError';
return {
  can: () => typeof globalThis.showOpenFilePicker === 'function'
    && typeof globalThis.showSaveFilePicker === 'function',
  open: async (exts) => {
    let h;
    try {
      [h] = await showOpenFilePicker({
        types: exts.length ? [{ description: 'Models and projects', accept: { 'application/octet-stream': exts } }] : undefined,
        multiple: false,
      });
    } catch (e) { if (cancelled(e)) return null; throw e; }
    const f = await h.getFile();
    return { name: f.name, bytes: new Uint8Array(await f.arrayBuffer()), token: await token(h) };
  },
  saveAs: async (name, bytes, remember) => {
    let h;
    try { h = await showSaveFilePicker({ suggestedName: name, types: types(name) }); }
    catch (e) { if (cancelled(e)) return null; throw e; }
    await write(h, bytes);
    return remember ? await token(h) : '';
  },
  saveTo: async (t, bytes) => {
    const h = await find(t);
    if (!h) return 'gone';
    if (!(await allowed(h, 'readwrite'))) return 'denied';
    await write(h, bytes);
    return 'ok';
  },
  read: async (t) => {
    const h = await find(t);
    if (!h) return null;
    if (!(await allowed(h, 'read'))) return 'denied';
    let f;
    try { f = await h.getFile(); } catch (e) { return null; }
    return { name: f.name, bytes: new Uint8Array(await f.arrayBuffer()) };
  },
  launched: () => {
    const q = globalThis.__stlLaunch;
    if (!q || !q.length) return null;
    const it = q.shift();
    return { name: it.name, bytes: it.bytes, token: it.handle ? keep(it.handle) : '' };
  },
};
"#;

fn page() -> Option<JsValue> {
    let global = js_sys::global();
    let key = JsValue::from_str("__stlFsa");
    if let Ok(v) = Reflect::get(&global, &key)
        && v.is_object()
    {
        return Some(v);
    }
    let made = Function::new_no_args(PAGE).call0(&JsValue::UNDEFINED).ok()?;
    let _ = Reflect::set(&global, &key, &made);
    Some(made)
}

fn call(name: &str, args: &[JsValue]) -> Result<JsValue, JsValue> {
    let page = page().ok_or(JsValue::NULL)?;
    let f: Function = Reflect::get(&page, &JsValue::from_str(name))?.dyn_into()?;
    let argv = Array::new();
    for a in args {
        argv.push(a);
    }
    f.apply(&page, &argv)
}

async fn wait(name: &str, args: &[JsValue]) -> Result<JsValue, JsValue> {
    let promise: Promise = call(name, args)?.dyn_into()?;
    JsFuture::from(promise).await
}

fn field(obj: &JsValue, name: &str) -> JsValue {
    Reflect::get(obj, &JsValue::from_str(name)).unwrap_or(JsValue::UNDEFINED)
}

fn file_of(obj: &JsValue) -> Option<(PickedFile, Option<String>)> {
    let name = field(obj, "name").as_string()?;
    let bytes = field(obj, "bytes").dyn_into::<Uint8Array>().ok()?.to_vec();
    let token = field(obj, "token").as_string().filter(|t| !t.is_empty());
    Some((PickedFile { name, bytes }, token))
}

/// This browser can open and save files in place.
pub fn can() -> bool {
    call("can", &[]).is_ok_and(|v| v.as_bool() == Some(true))
}

/// The open dialog. `exts` are `.stl`-style extensions. `Ok(None)`: closed without a file.
pub async fn open(exts: Vec<String>) -> Result<Option<(PickedFile, Option<String>)>, JsValue> {
    let list = Array::new();
    for e in &exts {
        list.push(&JsValue::from_str(e));
    }
    let got = wait("open", &[list.into()]).await?;
    Ok(file_of(&got))
}

/// The save dialog, then the write. `Ok(None)`: closed without saving. `Ok(Some(token))`: saved;
/// the token is empty unless `remember`.
pub async fn save_as(name: &str, bytes: &[u8], remember: bool) -> Result<Option<String>, JsValue> {
    let got = wait(
        "saveAs",
        &[
            JsValue::from_str(name),
            Uint8Array::from(bytes).into(),
            JsValue::from_bool(remember),
        ],
    )
    .await?;
    Ok(got.as_string())
}

/// How a write to a kept file went.
pub enum Wrote {
    Ok,
    /// The handle isn't known any more.
    Gone,
    /// The browser (or the person) refused permission.
    Denied,
}

pub async fn save_to(token: &str, bytes: &[u8]) -> Result<Wrote, JsValue> {
    let got = wait("saveTo", &[JsValue::from_str(token), Uint8Array::from(bytes).into()]).await?;
    Ok(match got.as_string().as_deref() {
        Some("ok") => Wrote::Ok,
        Some("denied") => Wrote::Denied,
        _ => Wrote::Gone,
    })
}

/// A kept file's contents. `Ok(None)`: it isn't there any more. `Err`: permission refused.
pub async fn read(token: &str) -> Result<Option<PickedFile>, String> {
    let name = name_of(token);
    let got = wait("read", &[JsValue::from_str(token)])
        .await
        .map_err(|_| format!("Couldn\u{2019}t open {name}."))?;
    if got.as_string().as_deref() == Some("denied") {
        return Err(format!("The browser wasn\u{2019}t allowed to open {name}."));
    }
    Ok(file_of(&got).map(|(f, _)| f))
}

/// The next file the installed app was opened with, if one is waiting.
pub fn launched() -> Option<(PickedFile, Option<String>)> {
    file_of(&call("launched", &[]).ok()?)
}

/// The file name a token ends with.
pub fn name_of(token: &str) -> &str {
    token.rsplit('/').next().unwrap_or(token)
}
