// Browser storage for the web edition (IndexedDB): autosave copies for crash recovery, the
// recent-files list (file handles where the browser has them, small copies otherwise) and
// fonts added from font files. Everything stays in this browser on this computer.
const DB_NAME = 'stl-editor', DB_VERSION = 1;
// small records are kept apart from file contents ('blobs') and file handles ('handles'), so
// listing them stays quick and never has to read a handle back
const STORES = ['autosave', 'recent', 'fonts', 'blobs', 'handles'];
let dbp = null;

function db() {
  if (!dbp) {
    dbp = new Promise((resolve, reject) => {
      let req;
      try { req = indexedDB.open(DB_NAME, DB_VERSION); } catch (e) { reject(e); return; }
      req.onupgradeneeded = () => { for (const s of STORES) if (!req.result.objectStoreNames.contains(s)) req.result.createObjectStore(s, { keyPath: 'id' }); };
      req.onsuccess = () => resolve(req.result);
      req.onerror = () => reject(req.error);
      req.onblocked = () => reject(new Error('storage blocked'));
    });
    dbp.catch(() => { dbp = null; });
  }
  return dbp;
}

function tx(store, mode, fn) {
  return db().then(d => new Promise((resolve, reject) => {
    const t = d.transaction(store, mode), s = t.objectStore(store);
    let out;
    const r = fn(s);
    if (r) r.onsuccess = () => { out = r.result; };
    t.oncomplete = () => resolve(out);
    t.onerror = () => reject(t.error);
    t.onabort = () => reject(t.error);
  }));
}

export const put = (store, value) => tx(store, 'readwrite', s => s.put(value));
export const get = (store, id) => tx(store, 'readonly', s => s.get(id));
export const del = (store, id) => tx(store, 'readwrite', s => s.delete(id));
export const all = store => tx(store, 'readonly', s => s.getAll()).then(x => x || []);

// ---------------------------------------------------------------- this tab's session
// Each tab holds a Web Lock named after its session while it is open, so another tab can tell
// a crashed session (lock free) from one that is still running.
export const SESSION = `${new Date().toISOString().replace(/\D/g, '').slice(0, 14)}-${Math.random().toString(36).slice(2, 8)}`;
const LOCK = id => `stle-session-${id}`;
let lockHeld = false;
export function holdSessionLock() {
  if (lockHeld || !navigator.locks) return;
  lockHeld = true;
  navigator.locks.request(LOCK(SESSION), () => new Promise(() => {})).catch(() => { lockHeld = false; });
}
export async function liveSessions() {
  if (!navigator.locks?.query) return null;
  const q = await navigator.locks.query();
  return new Set((q.held || []).map(l => l.name).filter(n => n.startsWith('stle-session-')).map(n => n.slice(13)));
}

export const newId = () => Date.now().toString(36) + Math.random().toString(36).slice(2, 8);
