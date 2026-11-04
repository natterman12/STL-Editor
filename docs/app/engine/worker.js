// Web edition engine: runs in a Web Worker, answers the same API routes as the desktop app.
import { init, sweep } from './wasm.js';
import { GeometryError } from './geom.js';
import { setFontLoader } from './meshtools.js';
import opentype from '../vendor/opentype.module.js';
import * as SFX from './supportfins.js';

let ROUTES = null, app = null;
const supplied = new Map();   // fonts the page passed in (fonts installed on this computer)

export const BUNDLED_FONTS = {
  'DejaVu Sans': { '': 'DejaVuSans.ttf', b: 'DejaVuSans-Bold.ttf', i: 'DejaVuSans-Oblique.ttf', bi: 'DejaVuSans-BoldOblique.ttf' },
  'DejaVu Serif': { '': 'DejaVuSerif.ttf', b: 'DejaVuSerif-Bold.ttf' },
  'DejaVu Sans Mono': { '': 'DejaVuSansMono.ttf', b: 'DejaVuSansMono-Bold.ttf' },
};

async function loadFont(name, bold, italic) {
  const style = (bold ? 'b' : '') + (italic ? 'i' : '');
  const key = `${name}|${style}`;
  if (supplied.has(key)) return supplied.get(key);
  if (supplied.has(`${name}|`)) return supplied.get(`${name}|`);
  const fam = BUNDLED_FONTS[name] || BUNDLED_FONTS['DejaVu Sans'];
  const file = fam[style] || fam[bold ? 'b' : ''] || fam[''];
  const r = await fetch(new URL(`../fonts/${file}`, import.meta.url));
  if (!r.ok) throw new GeometryError('The font could not be loaded.');
  return r.arrayBuffer();
}

const ready = (async () => {
  await init();
  setFontLoader(loadFont);
  await SFX.load();
  ({ ROUTES, app } = await import('./routes.js'));
})();

// one request at a time: freeing unused geometry between requests must never touch a
// request that is still running
let queue = Promise.resolve();
self.onmessage = ev => { queue = queue.then(() => handle(ev.data)); };

async function handle({ id, route, data }) {
  try {
    await ready;
    if (data?.fontData) { supplied.set(`${data.font}|${(data.bold ? 'b' : '') + (data.italic ? 'i' : '')}`, data.fontData); delete data.fontData; }
    if (route === 'font_info') {
      // name and style of a font file the person added
      let font;
      try { font = opentype.parse(data.data); } catch { throw new GeometryError('That file isn\u2019t a font STL Editor can read (use .ttf, .otf or .woff).'); }
      const nm = font.names || {}, pick = n => n && (n.en || Object.values(n)[0]);
      const family = pick(nm.preferredFamily) || pick(nm.fontFamily) || 'Font';
      const sub = pick(nm.preferredSubfamily) || pick(nm.fontSubfamily) || 'Regular';
      self.postMessage({ id, ok: true, result: { ok: true, family, style: (/bold|black|heavy/i.test(sub) ? 'b' : '') + (/italic|oblique/i.test(sub) ? 'i' : '') } });
      return;
    }
    if (route === 'fonts') { self.postMessage({ id, ok: true, result: { ok: true, fonts: Object.keys(BUNDLED_FONTS), default: 'DejaVu Sans' } }); return; }
    const fn = ROUTES[route];
    if (!fn) throw new GeometryError(`unknown route ${route}`);
    let out = await fn(data || {});
    let transfer = [];
    if (out && out.result !== undefined && out.transfer) { transfer = out.transfer; out = out.result; }
    self.postMessage({ id, ok: true, result: out }, transfer);
  } catch (e) {
    if (!(e instanceof GeometryError)) console.error(e);
    self.postMessage({ id, ok: false, error: e instanceof GeometryError ? e.message : `${e?.name || 'Error'}: ${e?.message || e}` });
  } finally {
    try { if (app) sweep(app.doc.liveObjects()); } catch (e) { console.error('sweep', e); }
  }
}
