// Service worker for the 2.0 web edition: keeps a copy of the app so it opens and works with
// no connection, and lets the browser install it as an app.
//
// `cargo run -p xtask -- web` fills in BUILD and FILES when it copies this file into the build.
// As written here (the two markers below) it keeps nothing and the page works online only.
const BUILD = '1691e0e4a01282e8';
const FILES = ["coverage_worker.js","icon-192.png","icon-32.png","icon-512.png","icon-maskable.png","index.html","licences.html","manifest.webmanifest","stl-app.js","stl-app_bg.wasm","support-fins/VERSION.json","support-fins/engine.iife.js","worker.js"];
// Not "stl-editor-": the 1.x web edition on the same site removes every cache with that prefix
// that isn't its own.
const PREFIX = 'stle2-';
const CACHE = PREFIX + BUILD;
const FILLED = !BUILD.startsWith('__');

self.addEventListener('install', ev => {
  if (FILLED) ev.waitUntil(caches.open(CACHE).then(c => c.addAll(['./', ...FILES])));
});

self.addEventListener('activate', ev => {
  ev.waitUntil((async () => {
    for (const k of await caches.keys()) if (k.startsWith(PREFIX) && k !== CACHE) await caches.delete(k);
    await self.clients.claim();
  })());
});

// the page asks the waiting new version to take over (after the person chose to reload)
self.addEventListener('message', ev => { if (ev.data === 'skip-waiting') self.skipWaiting(); });

self.addEventListener('fetch', ev => {
  const req = ev.request;
  if (!FILLED || req.method !== 'GET' || new URL(req.url).origin !== self.location.origin) return;
  ev.respondWith((async () => {
    const cache = await caches.open(CACHE);
    // the app's own files come from the copy of this version, so they always match each other
    const hit = await cache.match(req, { ignoreSearch: true }) || (req.mode === 'navigate' ? await cache.match('./') : null);
    if (hit) return hit;
    try { return await fetch(req); } catch (e) { return Response.error(); }
  })());
});
