// Shared settings for the UI tests (Playwright). See README.md.
const path = require('path');
const fs = require('fs');

const ROOT = path.resolve(__dirname, '..', '..');
const FIX = path.join(__dirname, 'fixtures');          // test parts
const OUT = path.join(__dirname, 'out');               // screenshots and saved files (git-ignored)
fs.mkdirSync(OUT, { recursive: true });

// The web edition (web/ served as static files) and the desktop engine (main.py --server).
// run.js starts both; override with WEB_URL / DESKTOP_URL to test elsewhere, e.g. the live site.
const WEB = process.env.WEB_URL || 'http://127.0.0.1:8790/index.html?web';
const DESKTOP = (process.env.DESKTOP_URL || 'http://127.0.0.1:8765/').replace(/\/?$/, '/');

module.exports = { ROOT, FIX, OUT, WEB, DESKTOP };
