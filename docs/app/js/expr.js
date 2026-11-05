// Parameters: named values a project keeps, and the expressions that use them.
// A feature's number can be "=width/2 + 3" instead of a number; the engine works it out from
// the project's parameters each time the timeline is rebuilt, so changing a parameter
// updates every step that uses it. Lengths are millimetres and angles degrees.
// The same rules are in stleditor/params.py (the desktop engine); tests check they agree.

export class ExprError extends Error {}

const DEG = Math.PI / 180;
export const FUNCS = {
  sqrt: Math.sqrt, abs: Math.abs, min: Math.min, max: Math.max,
  round: x => Math.floor(x + 0.5), floor: Math.floor, ceil: Math.ceil,
  sin: d => Math.sin(d * DEG), cos: d => Math.cos(d * DEG), tan: d => Math.tan(d * DEG),
  asin: x => Math.asin(x) / DEG, acos: x => Math.acos(x) / DEG, atan: x => Math.atan(x) / DEG,
};
export const CONSTS = { pi: Math.PI };
export const NAME_RE = /^[A-Za-z_][A-Za-z0-9_]*$/;
export const RESERVED = new Set([...Object.keys(FUNCS), ...Object.keys(CONSTS)]);

/** Is this value an expression (a string starting with "=")? */
export const isExpr = v => typeof v === 'string' && v.trim().startsWith('=');

function tokens(text) {
  const s = String(text).trim().replace(/^=/, ''), out = [];
  let i = 0;
  while (i < s.length) {
    const c = s[i];
    if (/\s/.test(c)) { i++; continue; }
    const num = /^(\d+\.?\d*|\.\d+)(e[+-]?\d+)?/i.exec(s.slice(i));
    if (num) { out.push({ t: 'n', v: parseFloat(num[0]) }); i += num[0].length; continue; }
    const id = /^[A-Za-z_][A-Za-z0-9_]*/.exec(s.slice(i));
    if (id) { out.push({ t: 'id', v: id[0] }); i += id[0].length; continue; }
    if ('+-*/%^(),'.includes(c)) { out.push({ t: c }); i++; continue; }
    throw new ExprError(`can't read "${c}" in ${s}`);
  }
  return out;
}

/** The parameter names an expression uses. */
export function namesIn(text) {
  const out = new Set();
  try { for (const k of tokens(text)) if (k.t === 'id' && !RESERVED.has(k.v)) out.add(k.v); } catch { /* reported when evaluated */ }
  return out;
}

/** Work out an expression ("=..." or plain) with `vars` {name: number}. */
export function evaluate(text, vars = {}) {
  const tk = tokens(text);
  let p = 0;
  const peek = () => tk[p], take = t => { if (tk[p]?.t === t) { p++; return true; } return false; };
  const src = String(text).trim().replace(/^=/, '');
  const expr = () => {
    let v = term();
    for (;;) { if (take('+')) v += term(); else if (take('-')) v -= term(); else return v; }
  };
  const term = () => {
    let v = power();
    for (;;) {
      if (take('*')) v *= power();
      else if (take('/')) { const d = power(); if (d === 0) throw new ExprError(`division by zero in ${src}`); v /= d; }
      else if (take('%')) v %= power();
      else return v;
    }
  };
  const power = () => { const b = unary(); return take('^') ? b ** power() : b; };
  const unary = () => (take('-') ? -unary() : take('+') ? unary() : primary());
  const primary = () => {
    const k = peek();
    if (!k) throw new ExprError(`${src} ends too soon`);
    p++;
    if (k.t === 'n') return k.v;
    if (k.t === '(') { const v = expr(); if (!take(')')) throw new ExprError(`a ")" is missing in ${src}`); return v; }
    if (k.t === 'id') {
      if (take('(')) {
        const fn = FUNCS[k.v]; if (!fn) throw new ExprError(`there's no function "${k.v}"`);
        const args = [];
        if (!take(')')) { do args.push(expr()); while (take(',')); if (!take(')')) throw new ExprError(`a ")" is missing in ${src}`); }
        if (!args.length) throw new ExprError(`${k.v}() needs a number`);
        return fn(...args);
      }
      if (k.v in CONSTS) return CONSTS[k.v];
      if (Object.prototype.hasOwnProperty.call(vars, k.v)) return vars[k.v];
      throw new ExprError(`there's no parameter "${k.v}"`);
    }
    throw new ExprError(`unexpected "${k.t}" in ${src}`);
  };
  if (!tk.length) throw new ExprError('an expression is empty');
  const v = expr();
  if (p < tk.length) throw new ExprError(`unexpected "${tk[p].v ?? tk[p].t}" in ${src}`);
  if (!Number.isFinite(v)) throw new ExprError(`${src} doesn't give a number`);
  return v;
}

/**
 * The value of every parameter in a list [{name, expr}], where a parameter may use others
 * (in any order). Returns { values: {name: number}, errors: {name: message} }.
 */
export function resolveParams(list) {
  const byName = new Map(), values = {}, errors = {};
  for (const q of list || []) {
    if (!NAME_RE.test(q.name || '')) { errors[q.name || '?'] = `"${q.name}" isn't a name (letters, digits and _; not starting with a digit)`; continue; }
    if (RESERVED.has(q.name)) { errors[q.name] = `"${q.name}" is a built-in name`; continue; }
    if (byName.has(q.name)) { errors[q.name] = `"${q.name}" is there twice`; continue; }
    byName.set(q.name, q);
  }
  const busy = new Set();
  const get = name => {
    if (name in values) return values[name];
    if (name in errors) throw new ExprError(errors[name]);
    const q = byName.get(name);
    if (!q) throw new ExprError(`there's no parameter "${name}"`);
    if (busy.has(name)) throw new ExprError(`"${name}" depends on itself`);
    busy.add(name);
    try {
      const vars = {};
      for (const n of namesIn(String(q.expr))) vars[n] = get(n);
      return (values[name] = evaluate(String(q.expr), vars));
    } catch (e) {
      errors[name] = e.message;
      throw new ExprError(e.message);
    } finally { busy.delete(name); }
  };
  for (const name of byName.keys()) { try { get(name); } catch { /* kept in errors */ } }
  return { values, errors };
}

/** A copy of a feature's parameters with every expression worked out (throws ExprError). */
export function resolveDeep(obj, values) {
  if (isExpr(obj)) {
    const vars = {};
    for (const n of namesIn(obj)) { if (!(n in values)) throw new ExprError(`there's no parameter "${n}" (in ${obj.trim()})`); vars[n] = values[n]; }
    return evaluate(obj, vars);
  }
  if (Array.isArray(obj)) return obj.map(x => resolveDeep(x, values));
  if (obj && typeof obj === 'object') return Object.fromEntries(Object.entries(obj).map(([k, x]) => [k, resolveDeep(x, values)]));
  return obj;
}

/** Does anything in these feature parameters use an expression? */
export function usesExpr(obj) {
  if (isExpr(obj)) return true;
  if (Array.isArray(obj)) return obj.some(usesExpr);
  if (obj && typeof obj === 'object') return Object.values(obj).some(usesExpr);
  return false;
}
