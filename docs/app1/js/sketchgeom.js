// 2D outlines of sketch entities (mirrors stleditor/sketch.py for display).
export function entityOutline(e) {
  const t = e.type;
  if (t === 'rect') {
    let { x, y, w, h } = e; const r0 = +e.fillet || 0;
    const x0 = Math.min(x, x + w), x1 = Math.max(x, x + w), y0 = Math.min(y, y + h), y1 = Math.max(y, y + h);
    const r = Math.min(r0, (x1 - x0) / 2 - 1e-6, (y1 - y0) / 2 - 1e-6);
    let pts;
    if (r > 1e-6) {
      pts = [];
      const n = 10;
      for (const [cx, cy, a0] of [[x1 - r, y0 + r, -90], [x1 - r, y1 - r, 0], [x0 + r, y1 - r, 90], [x0 + r, y0 + r, 180]])
        for (let i = 0; i <= n; i++) { const a = (a0 + 90 * i / n) * Math.PI / 180; pts.push([cx + r * Math.cos(a), cy + r * Math.sin(a)]); }
    } else pts = [[x0, y0], [x1, y0], [x1, y1], [x0, y1]];
    if (e.angle) {
      const cx = x + w / 2, cy = y + h / 2, a = e.angle * Math.PI / 180, c = Math.cos(a), s = Math.sin(a);
      pts = pts.map(([px, py]) => [cx + (px - cx) * c - (py - cy) * s, cy + (px - cx) * s + (py - cy) * c]);
    }
    return { pts, closed: true };
  }
  if (t === 'circle') {
    const n = 96, pts = [];
    for (let i = 0; i < n; i++) { const a = 2 * Math.PI * i / n; pts.push([e.cx + e.r * Math.cos(a), e.cy + e.r * Math.sin(a)]); }
    return { pts, closed: true };
  }
  if (t === 'polygon') {
    const n = Math.max(3, e.n | 0), a0 = (e.angle || 0) * Math.PI / 180, pts = [];
    for (let i = 0; i < n; i++) { const a = a0 + 2 * Math.PI * i / n; pts.push([e.cx + e.r * Math.cos(a), e.cy + e.r * Math.sin(a)]); }
    return { pts, closed: true };
  }
  if (t === 'slot') {
    const { x1, y1, x2, y2, r } = e, ang = Math.atan2(y2 - y1, x2 - x1), n = 24, pts = [];
    for (let i = 0; i <= n; i++) { const a = ang - Math.PI / 2 + Math.PI * i / n; pts.push([x2 + r * Math.cos(a), y2 + r * Math.sin(a)]); }
    for (let i = 0; i <= n; i++) { const a = ang + Math.PI / 2 + Math.PI * i / n; pts.push([x1 + r * Math.cos(a), y1 + r * Math.sin(a)]); }
    return { pts, closed: true };
  }
  if (t === 'polyline') return { pts: e.pts || [], closed: !!e.closed };
  if (t === 'line') return { pts: [e.p1, e.p2], closed: false };
  if (t === 'arc') return { pts: arcPts(e), closed: false };
  return { pts: [], closed: false };
}

export function arcSweep(e) { let s = ((e.a1 - e.a0) % 360 + 360) % 360; return s < 1e-9 ? 360 : s; }
export function arcPts(e, n) {
  const sw = arcSweep(e); n = n || Math.max(8, Math.round(sw / 4));
  const out = [];
  for (let i = 0; i <= n; i++) { const a = (e.a0 + sw * i / n) * Math.PI / 180; out.push([e.cx + e.r * Math.cos(a), e.cy + e.r * Math.sin(a)]); }
  return out;
}
// circle through three points -> arc entity from p1 to p3 passing p2 (null if collinear)
export function arcFrom3(p1, p2, p3) {
  const [ax, ay] = p1, [bx, by] = p2, [cx, cy] = p3;
  const d = 2 * (ax * (by - cy) + bx * (cy - ay) + cx * (ay - by));
  if (Math.abs(d) < 1e-12) return null;
  const ux = ((ax * ax + ay * ay) * (by - cy) + (bx * bx + by * by) * (cy - ay) + (cx * cx + cy * cy) * (ay - by)) / d;
  const uy = ((ax * ax + ay * ay) * (cx - bx) + (bx * bx + by * by) * (ax - cx) + (cx * cx + cy * cy) * (bx - ax)) / d;
  const r = Math.hypot(ax - ux, ay - uy);
  const ang = p => Math.atan2(p[1] - uy, p[0] - ux) * 180 / Math.PI;
  let a0 = ang(p1), a1 = ang(p3), am = ang(p2);
  const ccw = (x, from, to) => ((x - from) % 360 + 360) % 360 <= ((to - from) % 360 + 360) % 360;
  if (!ccw(am, a0, a1)) [a0, a1] = [a1, a0];
  return { type: 'arc', cx: ux, cy: uy, r, a0, a1 };
}

export function keyPoints(e) {
  switch (e.type) {
    case 'rect': return [[e.x, e.y], [e.x + e.w, e.y], [e.x + e.w, e.y + e.h], [e.x, e.y + e.h], [e.x + e.w / 2, e.y + e.h / 2],
      [e.x + e.w / 2, e.y], [e.x + e.w, e.y + e.h / 2], [e.x + e.w / 2, e.y + e.h], [e.x, e.y + e.h / 2]];
    case 'circle': return [[e.cx, e.cy], [e.cx + e.r, e.cy], [e.cx - e.r, e.cy], [e.cx, e.cy + e.r], [e.cx, e.cy - e.r]];
    case 'polygon': return [[e.cx, e.cy], ...entityOutline(e).pts];
    case 'slot': return [[e.x1, e.y1], [e.x2, e.y2]];
    case 'line': return [e.p1, e.p2, [(e.p1[0] + e.p2[0]) / 2, (e.p1[1] + e.p2[1]) / 2]];
    case 'arc': { const p = arcPts(e, 2); return [p[0], p[2], p[1], [e.cx, e.cy]]; }
    case 'polyline': {
      const p = e.pts || [], out = [...p];
      for (let i = 0; i + 1 < p.length; i++) out.push([(p[i][0] + p[i + 1][0]) / 2, (p[i][1] + p[i + 1][1]) / 2]);
      return out;
    }
  }
  return [];
}

// distance from point to an entity's outline (for picking in the sketcher)
export function distToEntity(e, p) {
  const { pts, closed } = entityOutline(e);
  let best = Infinity;
  const n = pts.length;
  for (let i = 0; i < n - (closed ? 0 : 1); i++) {
    const a = pts[i], b = pts[(i + 1) % n];
    best = Math.min(best, segDist(p, a, b));
  }
  if (n === 1) best = Math.hypot(p[0] - pts[0][0], p[1] - pts[0][1]);
  return best;
}

export function segDist(p, a, b) {
  const dx = b[0] - a[0], dy = b[1] - a[1], L = dx * dx + dy * dy;
  let t = L ? ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / L : 0;
  t = Math.max(0, Math.min(1, t));
  return Math.hypot(p[0] - (a[0] + t * dx), p[1] - (a[1] + t * dy));
}

export const ENTITY_LABEL = { rect: 'Rectangle', circle: 'Circle', polygon: 'Polygon', slot: 'Slot', polyline: 'Polyline', line: 'Line', arc: 'Arc' };

export function describe(e) {
  const f = v => (+v).toFixed(2).replace(/\.?0+$/, '');
  switch (e.type) {
    case 'rect': return `${f(Math.abs(e.w))} × ${f(Math.abs(e.h))}`;
    case 'circle': return `Ø${f(e.r * 2)}`;
    case 'polygon': return `${e.n} sides, R${f(e.r)}`;
    case 'slot': return `L${f(Math.hypot(e.x2 - e.x1, e.y2 - e.y1))} W${f(e.r * 2)}`;
    case 'polyline': return `${(e.pts || []).length} pts${e.closed ? ', closed' : ''}`;
    case 'line': return `L${f(Math.hypot(e.p2[0] - e.p1[0], e.p2[1] - e.p1[1]))}`;
    case 'arc': return `R${f(e.r)}, ${f(arcSweep(e))}°`;
  }
  return '';
}

// editable dimensions shown in the sketch: [{label, at:[u,v], key, value}]
export function dimensions(e) {
  const f = v => (Math.round(v * 100) / 100).toString();
  switch (e.type) {
    case 'rect': return [
      { key: 'w', value: Math.abs(e.w), label: f(Math.abs(e.w)), at: [e.x + e.w / 2, Math.min(e.y, e.y + e.h)], side: 'below' },
      { key: 'h', value: Math.abs(e.h), label: f(Math.abs(e.h)), at: [Math.max(e.x, e.x + e.w), e.y + e.h / 2], side: 'right' }];
    case 'circle': return [{ key: 'd', value: e.r * 2, label: '⌀' + f(e.r * 2), at: [e.cx + e.r * 0.7071, e.cy + e.r * 0.7071] }];
    case 'arc': { const m = arcPts(e, 2)[1]; return [{ key: 'r', value: e.r, label: 'R' + f(e.r), at: m }]; }
    case 'polygon': return [{ key: 'r', value: e.r, label: 'R' + f(e.r), at: [e.cx + e.r, e.cy] }];
    case 'slot': return [{ key: 'len', value: Math.hypot(e.x2 - e.x1, e.y2 - e.y1), label: f(Math.hypot(e.x2 - e.x1, e.y2 - e.y1)), at: [(e.x1 + e.x2) / 2, (e.y1 + e.y2) / 2] },
      { key: 'w', value: e.r * 2, label: '⌀' + f(e.r * 2), at: [e.x2 + e.r, e.y2] }];
    case 'line': return [{ key: 'len', value: Math.hypot(e.p2[0] - e.p1[0], e.p2[1] - e.p1[1]), label: f(Math.hypot(e.p2[0] - e.p1[0], e.p2[1] - e.p1[1])), at: [(e.p1[0] + e.p2[0]) / 2, (e.p1[1] + e.p2[1]) / 2] }];
  }
  return [];
}

// apply an edited dimension value, keeping the shape's anchor point fixed
export function applyDimension(e, key, v) {
  if (!(v > 0)) return null;
  switch (e.type) {
    case 'rect': return key === 'w' ? { w: Math.sign(e.w || 1) * v } : { h: Math.sign(e.h || 1) * v };
    case 'circle': return { r: v / 2 };
    case 'arc': return { r: v };
    case 'polygon': return { r: v };
    case 'slot': if (key === 'w') return { r: v / 2 }; {
      const L = Math.hypot(e.x2 - e.x1, e.y2 - e.y1) || 1, k = v / L;
      return { x2: e.x1 + (e.x2 - e.x1) * k, y2: e.y1 + (e.y2 - e.y1) * k };
    }
    case 'line': {
      const L = Math.hypot(e.p2[0] - e.p1[0], e.p2[1] - e.p1[1]) || 1, k = v / L;
      return { p2: [e.p1[0] + (e.p2[0] - e.p1[0]) * k, e.p1[1] + (e.p2[1] - e.p1[1]) * k] };
    }
  }
  return null;
}
