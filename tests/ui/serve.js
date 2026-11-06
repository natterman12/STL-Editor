// A tiny static file server with the right content types (Python's http.server takes them from
// the Windows registry, where .js is often text/plain, and browsers then refuse to load the
// UI's ES modules).   node tests/ui/serve.js [folder] [port]   (default: web/ on 8790)
const http = require('http');
const fs = require('fs');
const path = require('path');

const TYPES = {
  '.html': 'text/html; charset=utf-8', '.js': 'text/javascript; charset=utf-8', '.mjs': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8', '.json': 'application/json', '.webmanifest': 'application/manifest+json',
  '.wasm': 'application/wasm', '.png': 'image/png', '.jpg': 'image/jpeg', '.webp': 'image/webp', '.svg': 'image/svg+xml',
  '.ico': 'image/x-icon', '.ttf': 'font/ttf', '.otf': 'font/otf', '.woff': 'font/woff', '.woff2': 'font/woff2',
  '.txt': 'text/plain; charset=utf-8', '.md': 'text/plain; charset=utf-8', '.stl': 'application/octet-stream',
};

function serve(dir, port, host = '127.0.0.1') {
  const root = path.resolve(dir);
  const server = http.createServer((req, res) => {
    let rel;
    try { rel = decodeURIComponent(new URL(req.url, 'http://x').pathname); } catch { res.writeHead(400); res.end(); return; }
    let file = path.join(root, rel);
    if (!file.startsWith(root)) { res.writeHead(403); res.end(); return; }
    fs.stat(file, (err, st) => {
      if (!err && st.isDirectory()) { if (!rel.endsWith('/')) { res.writeHead(301, { Location: rel + '/' }); res.end(); return; } file = path.join(file, 'index.html'); }
      fs.readFile(file, (e, data) => {
        if (e) { res.writeHead(404, { 'Content-Type': 'text/plain' }); res.end('not found'); return; }
        res.writeHead(200, { 'Content-Type': TYPES[path.extname(file).toLowerCase()] || 'application/octet-stream', 'Cache-Control': 'no-cache' });
        res.end(data);
      });
    });
  });
  return new Promise((ok, fail) => { server.once('error', fail); server.listen(port, host, () => ok(server)); });
}

module.exports = { serve };

if (require.main === module) {
  const dir = process.argv[2] || path.join(__dirname, '..', '..', 'web');
  const port = +(process.argv[3] || 8790);
  serve(dir, port).then(() => console.log(`Serving ${path.resolve(dir)} at http://127.0.0.1:${port}/ (Ctrl+C to stop)`));
}
