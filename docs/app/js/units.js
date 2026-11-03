// Display units. The engine always works in millimetres; only what the user sees and types
// changes in inch mode.
export const U = {
  mode: 'mm',
  get factor() { return this.mode === 'in' ? 25.4 : 1; },
  get label() { return this.mode === 'in' ? 'in' : 'mm'; },
  // mm -> display number
  toDisp(mm) { const v = (+mm || 0) / this.factor; return Math.round(v * (this.mode === 'in' ? 1e5 : 1e4)) / (this.mode === 'in' ? 1e5 : 1e4); },
  // mm -> short display string
  fmt(mm, digits) {
    const v = (+mm || 0) / this.factor;
    const d = digits ?? (this.mode === 'in' ? 4 : 3);
    return (Math.round(v * 10 ** d) / 10 ** d).toString();
  },
  // with unit suffix
  str(mm, digits) { return `${this.fmt(mm, digits)} ${this.label}`; },
  // typed text (display units; fractions like 1/4 or 1 3/8 allowed) -> mm
  parse(text) {
    const v = parseExpr(text);
    return Number.isFinite(v) ? v * this.factor : NaN;
  },
  snapSteps() {
    return this.mode === 'in'
      ? [['1/64"', 25.4 / 64], ['1/32"', 25.4 / 32], ['1/16"', 25.4 / 16], ['1/8"', 25.4 / 8], ['1/4"', 25.4 / 4], ['1/2"', 12.7], ['1"', 25.4]]
      : [['0.1', 0.1], ['0.5', 0.5], ['1', 1], ['2', 2], ['5', 5], ['10', 10]];
  },
};

// "12", "1/4", "1 3/8", "2*3+1", "1,5"
export function parseExpr(text) {
  if (typeof text === 'number') return text;
  let s = String(text ?? '').trim().replace(',', '.').replace(/["″]|in$|mm$/i, '').trim();
  if (!s) return NaN;
  const mixed = s.match(/^(-?\d+)\s+(\d+)\/(\d+)$/);
  if (mixed) { const w = +mixed[1]; return w + Math.sign(w || 1) * (+mixed[2] / +mixed[3]); }
  if (/^[0-9+\-*/(). ]+$/.test(s)) {
    try { const r = Function(`"use strict";return (${s})`)(); if (Number.isFinite(r)) return r; } catch { }
  }
  const f = parseFloat(s);
  return Number.isFinite(f) ? f : NaN;
}
