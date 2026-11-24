// Reads SVG and DXF drawings into sketch shapes (mm, sketch coordinates, y up).
// DXF: LINE, CIRCLE, ARC and LWPOLYLINE (bulges become arcs). SVG: every path and basic shape
// becomes a polyline, with transforms applied and units taken from width / viewBox.

const SHAPES = 'path,line,polyline,polygon,rect,circle,ellipse';

/** Points of a DXF bulge arc from a to b (bulge = tan(sweep / 4)), without a. */
function bulgePts(a, b, bulge, out) {
  if (!bulge) { out.push(b); return; }
  const d = Math.hypot(b[0] - a[0], b[1] - a[1]) / 2, sweep = 4 * Math.atan(bulge);
  const off = (1 - bulge * bulge) / (2 * bulge) * d, R = Math.hypot(d, off);
  const nx = -(b[1] - a[1]), ny = b[0] - a[0], nl = Math.hypot(nx, ny) || 1;
  const cx = (a[0] + b[0]) / 2 + nx / nl * off, cy = (a[1] + b[1]) / 2 + ny / nl * off;
  const a0 = Math.atan2(a[1] - cy, a[0] - cx), n = Math.min(2000, Math.max(4, Math.ceil(Math.abs(sweep) * R / 0.5)));
  for (let k = 1; k < n; k++) { const t = a0 + sweep * k / n; out.push([cx + R * Math.cos(t), cy + R * Math.sin(t)]); }
  out.push(b);
}

/** DXF text → sketch shapes. */
export function parseDXF(text) {
  const L = text.split(/\r?\n/), tags = [];
  for (let i = 0; i + 1 < L.length; i += 2) tags.push([+L[i].trim(), L[i + 1].trim()]);
  const start = tags.findIndex(([c, v]) => c === 2 && v === 'ENTITIES');
  if (start < 0) return [];
  const ents = [];
  for (const [c, v] of tags.slice(start + 1)) {
    if (c === 0) { if (v === 'ENDSEC') break; ents.push({ type: v, g: [] }); } else ents[ents.length - 1]?.g.push([c, v]);
  }
  const num = (g, c, d = 0) => { const t = g.find(x => x[0] === c); return t ? +t[1] : d; };
  const out = [];
  for (const { type, g } of ents) {
    if (type === 'LINE') out.push({ type: 'line', p1: [num(g, 10), num(g, 20)], p2: [num(g, 11), num(g, 21)] });
    else if (type === 'CIRCLE') out.push({ type: 'circle', cx: num(g, 10), cy: num(g, 20), r: num(g, 40) });
    else if (type === 'ARC') out.push({ type: 'arc', cx: num(g, 10), cy: num(g, 20), r: num(g, 40), a0: num(g, 50), a1: num(g, 51) });
    else if (type === 'LWPOLYLINE') {
      const v = [];
      for (const [c, s] of g) {
        if (c === 10) v.push({ x: +s, y: 0, b: 0 });
        else if (c === 20 && v.length) v[v.length - 1].y = +s;
        else if (c === 42 && v.length) v[v.length - 1].b = +s;
      }
      if (v.length < 2) continue;
      const closed = (num(g, 70) & 1) === 1, pts = [[v[0].x, v[0].y]];
      for (let i = 1; i < v.length; i++) bulgePts(pts[pts.length - 1], [v[i].x, v[i].y], v[i - 1].b, pts);
      if (closed) bulgePts(pts[pts.length - 1], pts[0], v[v.length - 1].b, pts);
      out.push({ type: 'polyline', pts, closed });
    }
  }
  return out;
}

/** SVG text → sketch shapes (uses the browser's own path maths; needs a DOM). */
export function parseSVG(text) {
  const doc = new DOMParser().parseFromString(text, 'image/svg+xml'), svg = doc.documentElement;
  if (!svg || svg.nodeName.toLowerCase() !== 'svg' || doc.querySelector('parsererror')) throw new Error('This SVG file can’t be read.');
  // a copy in the page so the shapes can be measured; scripts and event handlers are dropped
  const holder = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
  holder.style.cssText = 'position:absolute;width:0;height:0;visibility:hidden';
  const copy = document.importNode(svg, true);
  copy.querySelectorAll('script,foreignObject').forEach(n => n.remove());
  for (const n of [copy, ...copy.querySelectorAll('*')]) for (const a of [...n.attributes]) if (a.name.startsWith('on')) n.removeAttribute(a.name);
  holder.append(copy); document.body.append(holder);
  try {
    const vb = copy.viewBox?.baseVal, wAttr = copy.getAttribute('width') || '';
    const wv = parseFloat(wAttr) || 0, unit = (wAttr.match(/[a-z%]+$/i) || [''])[0].toLowerCase();
    const per = { mm: 1, cm: 10, in: 25.4, pt: 25.4 / 72, pc: 25.4 / 6 }[unit] ?? 25.4 / 96;   // px and unitless are 96 per inch
    const mmPer = vb?.width ? (wv ? wv * per : vb.width * 25.4 / 96) / vb.width : 25.4 / 96;
    const out = [];
    for (const el of copy.querySelectorAll(SHAPES)) {
      if (el.closest('defs,clipPath,mask,symbol,pattern,marker')) continue;
      const total = el.getTotalLength?.() || 0;
      if (!(total > 0)) continue;
      const m = copy.getCTM().inverse().multiply(el.getCTM()), closed = /^(rect|circle|ellipse|polygon)$/.test(el.localName) || /z\s*$/i.test(el.getAttribute('d') || '');
      const n = Math.min(4000, Math.max(8, Math.ceil(total / 0.5)));
      const pts = [];
      for (let k = 0; k < (closed ? n : n + 1); k++) {
        const p = el.getPointAtLength(total * k / n).matrixTransform(m);
        pts.push([p.x * mmPer, -p.y * mmPer]);
      }
      if (pts.length > 1) out.push({ type: 'polyline', pts, closed });
    }
    return out;
  } finally { holder.remove(); }
}
