// Service worker for the web edition: keeps a copy of the app so it opens and works offline,
// and lets the browser install it as an app. packaging/build_web.py fills in BUILD and FILES.
const BUILD = 'bb14e19199a2';
const FILES = ["icon-192.png","icon-512.png","icon-maskable.png","icon.png","index.html","manifest.webmanifest","css/app.css","engine/decimate.js","engine/document.js","engine/features.js","engine/geom.js","engine/io.js","engine/meshedit.js","engine/meshtools.js","engine/printing.js","engine/routes.js","engine/sketch2d.js","engine/supportfins.js","engine/voxel.js","engine/wasm.js","engine/worker.js","fonts/DEJAVU-LICENSE.txt","fonts/DejaVuSans-Bold.ttf","fonts/DejaVuSans-BoldOblique.ttf","fonts/DejaVuSans-Oblique.ttf","fonts/DejaVuSans.ttf","fonts/DejaVuSansMono-Bold.ttf","fonts/DejaVuSansMono.ttf","fonts/DejaVuSerif-Bold.ttf","fonts/DejaVuSerif.ttf","js/api.js","js/app.js","js/constraints.js","js/dims.js","js/drawwalls.js","js/expr.js","js/gizmo.js","js/icons.js","js/sfcoverage.js","js/sfcoverage.worker.js","js/sketcher.js","js/sketchgeom.js","js/tools.js","js/units.js","js/viewer.js","js/webstore.js","vendor/BufferGeometryUtils.js","vendor/FFLATE-LICENSE.txt","vendor/OPENTYPE-LICENSE.txt","vendor/OrbitControls.js","vendor/THREE-LICENSE.txt","vendor/THREE-MESH-BVH-LICENSE.txt","vendor/TransformControls.js","vendor/fflate.module.js","vendor/opentype.module.js","vendor/three-mesh-bvh.module.js","vendor/three.module.min.js","vendor/manifold/LICENSE.txt","vendor/manifold/manifold.js","vendor/manifold/manifold.wasm","vendor/support-fins/LICENSE.txt","vendor/support-fins/VERSION.json","vendor/support-fins/engine.esm.js","vendor/support-fins/engine.iife.js","vendor/support-fins/version.js"];
const CACHE = `stl-editor-${BUILD}`;

self.addEventListener('install', ev => {
  ev.waitUntil(caches.open(CACHE).then(c => c.addAll(['./', ...FILES])));
});

self.addEventListener('activate', ev => {
  ev.waitUntil((async () => {
    for (const k of await caches.keys()) if (k.startsWith('stl-editor-') && k !== CACHE) await caches.delete(k);
    await self.clients.claim();
  })());
});

// the page asks the waiting new version to take over (after the person chose to reload)
self.addEventListener('message', ev => { if (ev.data === 'skip-waiting') self.skipWaiting(); });

self.addEventListener('fetch', ev => {
  const req = ev.request;
  if (req.method !== 'GET' || new URL(req.url).origin !== self.location.origin) return;
  ev.respondWith((async () => {
    const cache = await caches.open(CACHE);
    // the app's own files come from the copy of this version, so they always match each other
    const hit = await cache.match(req, { ignoreSearch: true }) || (req.mode === 'navigate' ? await cache.match('./') : null);
    if (hit) return hit;
    try { return await fetch(req); } catch (e) { return hit || Response.error(); }
  })());
});
