//! The web edition's autosave copies in IndexedDB, database `stle2` (never the 1.x web edition's
//! `stl-editor` database). Ported from 1.x `web/js/webstore.js` and the autosave part of `api.js`,
//! with the same rule for which tab owns a session: each tab holds a Web Lock named after its
//! session id while it is open, and a copy whose lock is free belongs to a tab that was closed.
//! (Browsers without Web Locks: a copy not written for three minutes counts as closed.)
//!
//! The page thread owns this (the engine's Web Worker only makes the project bytes and posts
//! them), so the lock lives exactly as long as the tab.

use js_sys::{Function, Reflect, Uint8Array};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

use crate::platform::Pending;
use crate::recover::{RecoverEntry, RecoverStore};

/// The helper object, built once on `globalThis.__stle2Store`.
const STORE_JS: &str = r#"
if (globalThis.__stle2Store) return globalThis.__stle2Store;
const DB = 'stle2', STORES = ['autosave', 'blobs'];
let dbp = null;
const db = () => dbp || (dbp = new Promise((resolve, reject) => {
  let req;
  try { req = indexedDB.open(DB, 1); } catch (e) { reject(e); return; }
  req.onupgradeneeded = () => { for (const s of STORES) if (!req.result.objectStoreNames.contains(s)) req.result.createObjectStore(s, { keyPath: 'id' }); };
  req.onsuccess = () => resolve(req.result);
  req.onerror = () => reject(req.error);
  req.onblocked = () => reject(new Error('storage blocked'));
}).catch(e => { dbp = null; throw e; }));
// one transaction over several stores: either every write lands or none does
const txm = (stores, fn) => db().then(d => new Promise((resolve, reject) => {
  const t = d.transaction(stores, 'readwrite');
  fn(t);
  t.oncomplete = () => resolve();
  t.onerror = () => reject(t.error);
  t.onabort = () => reject(t.error);
}));
const tx = (store, mode, fn) => db().then(d => new Promise((resolve, reject) => {
  const t = d.transaction(store, mode), s = t.objectStore(store);
  let out;
  const r = fn(s);
  if (r) r.onsuccess = () => { out = r.result; };
  t.oncomplete = () => resolve(out);
  t.onerror = () => reject(t.error);
  t.onabort = () => reject(t.error);
}));
const SESSION = `${new Date().toISOString().replace(/\D/g, '').slice(0, 14)}-${Math.random().toString(36).slice(2, 8)}`;
const LOCK = id => `stle2-session-${id}`;
let held = false;
const dropCopy = id => txm(['autosave', 'blobs'], t => { t.objectStore('autosave').delete(id); t.objectStore('blobs').delete('autosave:' + id); });
return globalThis.__stle2Store = {
  session: () => SESSION,
  hold() {
    if (held || !navigator.locks) return;
    held = true;
    navigator.locks.request(LOCK(SESSION), () => new Promise(() => {})).catch(() => { held = false; });
  },
  put: (title, steps, saved, bytes) =>
    txm(['autosave', 'blobs'], t => {
      t.objectStore('blobs').put({ id: 'autosave:' + SESSION, bytes });
      t.objectStore('autosave').put({ id: SESSION, title, steps, saved });
    }),
  clear: () => dropCopy(SESSION),
  async list() {
    const items = (await tx('autosave', 'readonly', s => s.getAll())) || [];
    let live = null;
    try {
      if (navigator.locks && navigator.locks.query) {
        const q = await navigator.locks.query();
        live = new Set((q.held || []).map(l => l.name).filter(n => n.startsWith('stle2-session-')).map(n => n.slice(14)));
      }
    } catch (e) { live = null; }
    return items
      .filter(it => it.id !== SESSION && (live ? !live.has(it.id) : Date.now() / 1000 - it.saved > 180))
      .map(it => ({ id: it.id, title: it.title, steps: it.steps, saved: it.saved }));
  },
  async get(id) {
    const b = await tx('blobs', 'readonly', s => s.get('autosave:' + id));
    return b ? b.bytes : undefined;
  },
  discard: id => dropCopy(id),
};
"#;

fn api() -> Result<JsValue, String> {
    let g = js_sys::global();
    let cached = Reflect::get(&g, &JsValue::from_str("__stle2Store")).unwrap_or(JsValue::UNDEFINED);
    if !cached.is_undefined() {
        return Ok(cached);
    }
    Function::new_no_args(STORE_JS).call0(&JsValue::UNDEFINED).map_err(|e| format!("{e:?}"))
}

/// `api[name](...args)`, awaited.
async fn call(name: &str, args: &[JsValue]) -> Result<JsValue, String> {
    let api = api()?;
    let f: Function = Reflect::get(&api, &JsValue::from_str(name)).map_err(|e| format!("{e:?}"))?.dyn_into().map_err(|_| "storage is not available".to_string())?;
    let list = js_sys::Array::new();
    for a in args {
        list.push(a);
    }
    let out = f.apply(&api, &list).map_err(|e| format!("{e:?}"))?;
    JsFuture::from(js_sys::Promise::resolve(&out)).await.map_err(|e| e.as_string().unwrap_or_else(|| "the browser's storage refused".to_string()))
}

/// This tab takes its session lock (call once, when the page starts).
pub fn hold_session_lock() {
    if let Ok(api) = api() {
        if let Ok(f) = Reflect::get(&api, &JsValue::from_str("hold")).and_then(|f| f.dyn_into::<Function>()) {
            let _ = f.call0(&api);
        }
    }
}

/// Keep this tab's copy (the project bytes), replacing the last one.
pub async fn put_copy(title: &str, steps: usize, saved: f64, bytes: &[u8]) -> Result<(), String> {
    let u = Uint8Array::new_with_length(bytes.len() as u32);
    u.copy_from(bytes);
    call("put", &[JsValue::from_str(title), JsValue::from_f64(steps as f64), JsValue::from_f64(saved), u.into()]).await.map(|_| ())
}

/// This tab saved or opened another project: its copy is not needed.
pub async fn clear_copy() -> Result<(), String> {
    call("clear", &[]).await.map(|_| ())
}

/// The browser's copy-keeping for Recover.
pub struct WebStore;

impl RecoverStore for WebStore {
    fn list(&self) -> Pending<Vec<RecoverEntry>> {
        let out = Pending::new();
        let answer = out.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let mut list = Vec::new();
            if let Ok(v) = call("list", &[]).await {
                for it in js_sys::Array::from(&v).iter() {
                    let field = |k: &str| Reflect::get(&it, &JsValue::from_str(k)).unwrap_or(JsValue::UNDEFINED);
                    list.push(RecoverEntry {
                        id: field("id").as_string().unwrap_or_default(),
                        title: field("title").as_string().filter(|t| !t.is_empty()).unwrap_or_else(|| "Untitled".into()),
                        path: None,
                        saved: field("saved").as_f64().unwrap_or(0.0),
                        steps: field("steps").as_f64().unwrap_or(0.0) as usize,
                    });
                }
                list.sort_by(|a, b| a.saved.total_cmp(&b.saved));
            }
            answer.set(list);
        });
        out
    }

    fn load(&self, id: &str) -> Pending<Result<Vec<u8>, String>> {
        let (id, out) = (id.to_string(), Pending::new());
        let answer = out.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let r = match call("get", &[JsValue::from_str(&id)]).await {
                Ok(v) if !v.is_undefined() && !v.is_null() => Ok(Uint8Array::new(&v).to_vec()),
                Ok(_) => Err("That autosave copy is gone.".to_string()),
                Err(e) => Err(format!("Couldn't recover that copy: {e}")),
            };
            if r.is_ok() {
                let _ = call("discard", &[JsValue::from_str(&id)]).await;
            }
            answer.set(r);
        });
        out
    }

    fn discard(&self, id: &str) {
        let id = id.to_string();
        wasm_bindgen_futures::spawn_local(async move {
            let _ = call("discard", &[JsValue::from_str(&id)]).await;
        });
    }
}
