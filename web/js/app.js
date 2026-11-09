import * as THREE from 'three';
import { post, get, upload, WEB, requestLocalFonts, addFontFiles, openLaunchFiles, flushAutosave } from './api.js';
import { Viewer } from './viewer.js';
import { Sketcher } from './sketcher.js';
import { TOOLS, repeatableFeatures, HOLE_PRESETS, selBody } from './tools.js';
import { icon } from './icons.js';
import { describe, ENTITY_LABEL } from './sketchgeom.js';
import { U, parseExpr } from './units.js';
import { CTYPES, CTOOLS, solve } from './constraints.js';
import { baseRefs as SKETCH_BASE_REFS } from './sketcher.js';
import { WallDrawer } from './drawwalls.js';
import { Gizmo, rotOf } from './gizmo.js';
import { isExpr, resolveDeep, resolveParams, ExprError, exprText, exprOf } from './expr.js';

const $ = s => document.querySelector(s);
// tools the web edition leaves to the desktop app (they run programs on your computer)
const DESKTOP_TOOLS = new Set(['to_step', 'send']);
// Ctrl on Windows / Linux, Cmd on a Mac
const mod = e => e.ctrlKey || e.metaKey;
const DESKTOP_URL = 'https://natterman12.github.io/STL-Editor/#download';
const KOFI = 'https://ko-fi.com/natterman12';
// filament densities (g/cm³) for the support plastic readout; PLA and PETG are Support Fins' own
const MATERIALS = { pla: ['PLA', 1.24], petg: ['PETG', 1.27], abs: ['ABS', 1.04], asa: ['ASA', 1.07], tpu: ['TPU', 1.21], pa: ['Nylon', 1.14] };
const LOADS = { none: null, up: [0, 0, 1], down: [0, 0, -1], left: [-1, 0, 0], right: [1, 0, 0], front: [0, -1, 0], back: [0, 1, 0] };
const POSTURE = {
  strong: 'Lying down: its longest side runs along the layers, the strong way to print it.',
  mixed: 'At an angle: its longest side crosses the layers on a slant.',
  weak: 'Standing up: its longest side runs up through the layers, where prints split first.',
};
const THANKS = `<br><br>STL Editor is free and made in spare time. If it saves you time, you can <a href="#" data-url="${KOFI}">buy me a coffee on Ko-fi</a> <span style="color:#ff5e5b">&hearts;</span>`;
const esc = s => String(s ?? '').replace(/[&<>"]/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));
const num = v => (Math.round(v * 1000) / 1000).toString();

const TAB_ICON = { solid: 'box', print: 'bedsplit', supports: 'fins', mesh: 'remesh', sketch: 'sketch' };
const FEATURE_ICON = { shell: 'hollow', import: 'import', primitive: 'primitive', plane: 'plane', sketch: 'sketch', extrude: 'extrude', revolve: 'revolve', hole: 'hole',
  combine: 'combine', split: 'split', mirror: 'mirror', move: 'move', scale: 'scale', hollow: 'hollow', repair: 'repair', simplify: 'simplify',
  pattern: 'pattern', delete: 'delete', separate: 'separate', bedsplit: 'bedsplit', pushpull: 'pushpull', sculpt: 'sculpt', remesh: 'remesh',
  layflat: 'layflat', thread: 'thread', offset: 'offset', resizehole: 'resizehole', supportfins: 'fins', supportwall: 'finwall' };

class App {
  constructor() {
    this.viewer = new Viewer($('#view3d'));
    this.sketcher = new Sketcher(this);
    this.walls = new WallDrawer(this);
    this.gizmo = new Gizmo(this);
    this.state = { features: [], bodies: [], planes: [], sketches: [], doc: {}, recent: [] };
    this.selection = [];
    this.tool = null;      // active tool session
    this.mode = 'model';   // model | sketch | measure | section | info
    this.tab = 'solid';
    try { const t = localStorage.getItem('stle.tab'); if (['solid', 'print', 'supports', 'mesh'].includes(t)) this.tab = t; } catch { /* storage unavailable */ }
    this.apps = {};
    this._previewTimer = null;
    this._previewSeq = 0;
    if (WEB) document.documentElement.classList.add('web');
    this.buildRibbon();
    this.bindUI();
    if (WEB) { this.webWelcome(); this.setupWebApp(); }
    this.setupDrawer();
    this.loadSettings().then(() => this.refresh()).then(() => { this.viewer.setView('iso'); this.checkRecovery(); });
    setTimeout(() => this.checkUpdates(false), 6000);
    setInterval(() => this.renderAutosave(), 15000);
    post('apps', {}, { quiet: true }).then(r => { this.apps = r.apps || {}; this.apps.cli = r.solidcraft_cli; this.apps.platform = r.platform; this.apps.step_supported = r.step_supported; this.buildRibbon(); }).catch(() => {});
    setInterval(() => post('status', {}, { quiet: true }).catch(() => {}), 3000);
    this.hint('');
  }

  // =================================================================== state
  async refresh(newState) {
    if (!newState) newState = await get('state');
    this.state = newState;
    this.selection = this.selection.filter(id => this.state.bodies.some(b => b.id === id));
    await this.viewer.syncBodies(this.state.bodies, id => get(`mesh?id=${encodeURIComponent(id)}`).then(p => p.meshes[0]));
    this.viewer.syncPlanes(this.state.planes);
    this.viewer.syncSketches(this.state.sketches, this.sketcher.active ? this.sketcher.featureId : null);
    this.viewer.setSelection(this.selection);
    this.renderTree();
    this.renderTimeline();
    this.renderTitle();
    this.renderEmpty();
    this.updateRibbonState();
  }

  apply(res) { return res && res.state ? this.refresh(res.state) : this.refresh(); }

  renderTitle() {
    const d = this.state.doc || {};
    $('#doc-title').textContent = `${d.name || 'Untitled'}${d.dirty ? ' •' : ''}`;
    document.title = `${d.name || 'Untitled'}${d.dirty ? ' •' : ''} — STL Editor${WEB ? ' (web)' : ''}`;
    $('#btn-undo').disabled = !d.can_undo; $('#btn-redo').disabled = !d.can_redo;
  }

  renderEmpty() {
    const empty = !this.state.features.length && !this.sketcher.active && !this.tool;
    $('#empty-state').classList.toggle('hidden', !empty);
    const rec = (this.state.recent || []).slice(0, 6);
    $('#empty-recent').innerHTML = rec.length ? `<div class="section-label">Recent</div>` + rec.map(p => `<div class="rec" data-path="${esc(p)}" title="${esc(WEB ? p.split('/').pop() : p)}">${esc(p.split(/[\\/]/).pop())}</div>`).join('') : '';
  }

  // =================================================================== ribbon
  // Ribbon items: [key, label, icon] is a big button; {stack: [...]} stacks up to three small buttons;
  // {drop: id, label, items: [...]} is a dropdown; {grid: [...], cols} is a block of icon-only buttons.
  ribbonDef(tab = this.tab) {
    if (this.sketcher.active) return {
      tabs: [['sketch', 'Sketch']],
      groups: [
        ['Draw', [['line', 'Line', 'line'], ['arc3', 'Arc', 'arc'],
          { drop: 'sk_rect', label: 'Rectangle', items: [['rect', 'Rectangle', 'rect'], ['rect_center', 'Center Rectangle', 'rect_center']] },
          ['circle', 'Circle', 'circle'], { stack: [['polygon', 'Polygon', 'polygon'], ['slot', 'Slot', 'slot'], ['text', 'Text', 'text']] },
          { stack: [['sk_project', 'Project', 'project'], ['sk_useedge', 'Use Edge', 'useedge']] }]],
        ['Modify', [['select', 'Select', 'select'], ['trim', 'Trim', 'trim'],
          { stack: [['sk_offset', 'Offset', 'offset'], ['sk_mirror', 'Mirror', 'sk_mirror'], ['sk_delete', 'Delete', 'delete']] },
          { stack: [['construction', 'Construction', 'construction'], ['look', 'Look At', 'look']] }]],
        ['Dimension', [['c_dimension', 'Smart Dimension', 'c_dimension']]],
        ['Relations', [{ grid: Object.entries(CTOOLS).filter(([k]) => k !== 'c_dimension').map(([k, v]) => [k, v.label, k]), cols: 5 }]],
        ['Finish', [['finish', 'Finish', 'finish'], ['finish_extrude', 'Finish & Extrude', 'extrude'], ['sk_cancel', 'Cancel', 'cancel']]],
      ] };
    // Inspect is the same everywhere, so it sits once at the right end of every tab
    const pinned = ['Inspect', [{ stack: [['measure', 'Measure', 'measure'], ['section', 'Section', 'section'], ['info', 'Properties', 'info']] }]];
    const planes = [['plane_offset', 'Offset Plane', 'plane'], ['plane_angle', 'Plane at Angle', 'plane_angle'], ['plane_3pt', '3-Point Plane', 'plane_3pt'], ['plane_mid', 'Midplane', 'plane_mid']];
    // each tool lives on one tab only (the tool search finds any of them from anywhere)
    const groups = {
      solid: [
        ['Create', [['sketch', 'Sketch', 'sketch'], ['extrude', 'Extrude', 'extrude'], ['revolve', 'Revolve', 'revolve'],
          { drop: 'primitive', label: 'Primitive', items: [['primitive_box', 'Box', 'box'], ['primitive_cylinder', 'Cylinder', 'cylinder'],
            ['primitive_sphere', 'Sphere', 'sphere'], ['primitive_cone', 'Cone', 'cone'], ['primitive_torus', 'Torus', 'torus']] }]],
        ['Modify', [['pushpull', 'Push/Pull', 'pushpull'], ['combine', 'Combine', 'combine'], ['split', 'Split', 'split'],
          ['mirror', 'Mirror', 'mirror'], ['pattern', 'Pattern', 'pattern']]],
        ['Holes', [['hole', 'Hole', 'hole'], ['thread', 'Thread', 'thread'], { stack: [['resizehole', 'Resize Hole', 'resizehole']] }]],
        ['Shape', [{ stack: [['hollow', 'Hollow', 'hollow'], ['shell', 'Shell', 'hollow'], ['offset', 'Offset Body', 'offset']] }]],
        ['Reference', [{ drop: 'plane', label: 'Plane', items: planes }, ['params', 'Parameters', 'params']]],
      ],
      print: [
        ['Position', [['layflat', 'Lay Flat', 'layflat'], { stack: [['move', 'Move', 'move'], ['rotate', 'Rotate', 'rotate'], ['scale', 'Scale', 'scale']] }]],
        ['Fit', [['bedsplit', 'Fit to Bed', 'bedsplit']]],
        ['Output', [['export', 'Export', 'export'], ['send', 'Send to Slicer', 'slicer']]],
      ],
      supports: [
        ['Supports', [['supportfins', 'Support Fins', 'fins'], ['supportwall', 'Draw Walls', 'finwall']]],
        ['Support Fins', [{ stack: [['sf_about', 'About Support Fins', 'info']] }]],
      ],
      mesh: [
        ['Mesh', [['repair', 'Repair', 'repair'], ['simplify', 'Simplify', 'simplify'], ['remesh', 'Remesh', 'remesh'],
          { stack: [['separate', 'Separate', 'separate'], ['delete', 'Remove Bodies', 'delete']] }]],
        ['Sculpt', [['sculpt', 'Brushes', 'sculpt']]],
        ['Convert', [['to_step', 'To STEP', 'step']]],
      ],
    };
    return { tabs: [['solid', 'Model'], ['print', 'Print'], ['supports', 'Supports'], ['mesh', 'Mesh']], groups: groups[tab] || groups.solid, pinned };
  }

  // every [key, label, icon] inside a list of ribbon items
  static flatItems(items) {
    return items.flatMap(it => Array.isArray(it) ? [it] : (it.stack || it.grid || it.items || []));
  }

  // all commands for the tool search, with the tab each one lives on
  allCommands() {
    const out = [], seen = new Set();
    const add = (k, l, ic, tab, group) => { if (!seen.has(k)) { seen.add(k); out.push({ k, l, ic, tab, group }); } };
    if (this.sketcher.active) {
      for (const [g, items] of this.ribbonDef().groups) for (const [k, l, ic] of App.flatItems(items)) add(k, l, ic, 'sketch', g);
      return out;
    }
    for (const [tab] of this.ribbonDef().tabs)
      for (const [g, items] of this.ribbonDef(tab).groups) for (const [k, l, ic] of App.flatItems(items)) add(k, l, ic, tab, g);
    { const [g, items] = this.ribbonDef().pinned; for (const [k, l, ic] of App.flatItems(items)) add(k, l, ic, null, g); }
    add('import', 'Insert Mesh', 'import', null, 'File');
    add('export', 'Export', 'export', null, 'File');
    add('send', 'Send to Slicer', 'slicer', null, 'File');
    for (const k of Object.keys(TOOLS)) if (!seen.has(k) && TOOLS[k].label && !['delete'].includes(k)) add(k, TOOLS[k].label, k, null, '');
    return out;
  }

  buildRibbon() {
    const def = this.ribbonDef();
    this._drops = {};
    this.dropLast ||= {};
    const btn = ([k, l, ic], cls = '') => `<button class="tool-btn ${cls} ${k.startsWith('finish') ? 'finish' : ''}" data-tool="${k}" title="${esc(l)}">${icon(ic)}<span>${esc(l)}</span></button>`;
    const item = it => {
      if (Array.isArray(it)) return btn(it);
      if (it.stack) return `<div class="rstack">${it.stack.map(x => btn(x, 'small')).join('')}</div>`;
      if (it.grid) return `<div class="rgrid" style="--cols:${it.cols || 5}">${it.grid.map(x => btn(x, 'mini')).join('')}</div>`;
      this._drops[it.drop] = it;
      const last = it.items.find(x => x[0] === this.dropLast[it.drop]) || it.items[0];
      return `<button class="tool-btn drop" data-drop="${it.drop}" title="${esc(it.label)}: ${it.items.map(x => esc(x[1])).join(', ')}">${icon(last[2])}<span>${esc(it.label)}<i class="caret">▾</i></span></button>`;
    };
    $('#ribbon-tabs').innerHTML = def.tabs.map(([k, l]) => `<button class="rtab ${k === 'sketch' ? 'sketch active' : (k === this.tab ? 'active' : '')}" data-tab="${k}">${l}</button>`).join('');
    // the workspace badge at the left: the tab you're on
    const cur = def.tabs.find(([k]) => k === 'sketch' || k === this.tab) || def.tabs[0];
    const badge = $('#ribbon-badge');
    badge.className = cur[0] === 'sketch' ? 'sketch' : '';
    badge.innerHTML = `${icon(TAB_ICON[cur[0]] || 'box')}<span>${esc(cur[1])}</span>`;
    const all = def.pinned ? [...def.groups, def.pinned] : def.groups;
    // a group with a dropdown gets a ⌄ by its name; the name then lists all of the group's tools
    $('#ribbon-body').innerHTML = all.map(([label, items], gi) => `<div class="rgroup ${def.pinned && gi === all.length - 1 ? 'pinned' : ''} ${items.some(it => it.drop) ? 'has-drop' : ''}" data-g="${gi}"><div class="rgroup-items">${items.map(item).join('')}</div><div class="rgroup-label"${items.some(it => it.drop) ? ` data-glabel="${gi}" title="All ${esc(label)} tools"` : ''}>${esc(label)}</div></div>`).join('');
    this._ribbonGroups = all;
    this.bindSearch();
    this.updateRibbonState();
    this.fitRibbon();
  }

  // shrink the ribbon to the window instead of scrolling: smaller buttons, then icons only, then a » menu
  fitRibbon() {
    const rb = $('#ribbon'), body = $('#ribbon-body');
    if (!body || !body.clientWidth) return;
    rb.classList.remove('compact', 'tight');
    body.querySelector('.rmore')?.remove();
    for (const g of body.querySelectorAll('.rgroup.folded')) g.classList.remove('folded');
    const over = () => body.scrollWidth > body.clientWidth + 1;
    if (over()) rb.classList.add('compact');
    if (over()) rb.classList.add('tight');
    if (!over()) return;
    const more = document.createElement('div');
    more.className = 'rgroup rmore';
    more.innerHTML = `<div class="rgroup-items"><button class="tool-btn" data-more="1" title="More tools">${icon('more')}<span>More</span></button></div><div class="rgroup-label">&nbsp;</div>`;
    const pin = body.querySelector('.rgroup.pinned');
    body.insertBefore(more, pin || null);
    const groups = [...body.querySelectorAll('.rgroup:not(.rmore):not(.pinned)')];
    for (let i = groups.length - 1; i > 0 && over(); i--) groups[i].classList.add('folded');
  }

  /** The tools of one ribbon group, from its name (groups with a dropdown). */
  groupMenu(gi, anchor) {
    const g = this._ribbonGroups?.[gi]; if (!g) return;
    this.showMenu(anchor, App.flatItems(g[1]).map(([k, l, ic]) => ({ label: l, ic, tool: k, fn: () => this.onRibbon(k) })));
  }

  /** The icon of a tool as the toolbar shows it (for the tool panel's title). */
  toolIcon() {
    const key = this.tool?.key;
    if (!key) return this.sketcher.active ? 'sketch' : ['measure', 'section', 'info'].includes(this.mode) ? this.mode : null;
    const c = this.allCommands().find(x => x.k === key);
    return c?.ic || FEATURE_ICON[TOOLS[key]?.type] || null;
  }

  /** The Support Fins credit, at the left of the tool panel's buttons (Support Fins and Draw Walls only). */
  toolCredit() {
    const el = $('#tool-credit'); if (!el) return;
    const sf = ['supportfins', 'supportwall'].includes(this.tool?.def.type) && this._sfInfo;
    el.textContent = sf ? `Support Fins ${this._sfInfo.version || ''} · gittrahan (MIT)` : '';
    el.title = sf ? 'Support Fins by gittrahan, MIT licence (Help › About Support Fins)' : '';
  }

  moreMenu(anchor) {
    const items = [];
    for (const g of $('#ribbon-body').querySelectorAll('.rgroup.folded')) {
      const [label, its] = this._ribbonGroups[+g.dataset.g];
      items.push({ hdr: label });
      for (const [k, l, ic] of App.flatItems(its)) items.push({ label: l, ic, tool: k, fn: () => this.onRibbon(k) });
    }
    this.showMenu(anchor, items);
  }

  dropMenu(id, anchor) {
    const d = this._drops[id]; if (!d) return;
    this.showMenu(anchor, d.items.map(([k, l, ic]) => ({ label: l, ic, tool: k, fn: () => { this.dropLast[id] = k; this.onRibbon(k); this.buildRibbon(); } })));
  }

  bindSearch() {
    const inp = $('#tool-search'); if (!inp) return;
    const TAB = { solid: 'Model', print: 'Print', supports: 'Supports', mesh: 'Mesh', sketch: 'Sketch' };
    const matches = () => {
      const q = inp.value.trim().toLowerCase();
      if (!q) return [];
      const words = q.split(/\s+/);
      return this.allCommands().map(c => {
        const hay = `${c.l} ${c.group} ${c.k.replace(/_/g, ' ')}`.toLowerCase();
        if (!words.every(w => hay.includes(w))) return null;
        const l = c.l.toLowerCase();
        return { c, score: l.startsWith(q) ? 0 : l.split(/[\s/]+/).some(w => w.startsWith(words[0])) ? 1 : 2 };
      }).filter(Boolean).sort((a, b) => a.score - b.score).slice(0, 12).map(x => x.c);
    };
    const run = c => {
      inp.value = ''; inp.blur(); this.hideMenu();
      if (c.tab && c.tab !== 'sketch' && c.tab !== this.tab && !this.ribbonDef().groups.some(([, its]) => App.flatItems(its).some(x => x[0] === c.k))) { this.tab = c.tab; this.saveTab(); this.buildRibbon(); }
      this.onRibbon(c.k);
    };
    const show = () => {
      const list = matches();
      if (!inp.value.trim()) return this.hideMenu();
      this.showMenu(inp, list.length ? list.map(c => ({ label: c.l, ic: c.ic, tool: c.k, k: c.tab ? TAB[c.tab] : c.group, fn: () => run(c) })) : [{ label: 'No matching tools', dis: true }]);
      $('#menu .mi:not(.dis)')?.classList.add('hot');
    };
    inp.oninput = show;
    inp.onkeydown = e => {
      const its = [...document.querySelectorAll('#menu .mi:not(.dis)')];
      const hot = its.findIndex(x => x.classList.contains('hot'));
      if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
        e.preventDefault(); if (!its.length) return;
        const n = (hot + (e.key === 'ArrowDown' ? 1 : -1) + its.length) % its.length;
        its.forEach((x, i) => x.classList.toggle('hot', i === n));
      } else if (e.key === 'Enter') {
        e.preventDefault(); e.stopPropagation();
        const list = matches(); const c = list[Math.max(0, hot)]; if (c) run(c);
      } else if (e.key === 'Escape') { e.stopPropagation(); inp.value = ''; this.hideMenu(); inp.blur(); }
    };
  }

  saveTab() { try { localStorage.setItem('stle.tab', this.tab); } catch { /* storage unavailable */ } }

  updateRibbonState() {
    const isOn = k => this.sketcher.active ? this.sketcher.tool === k : ((this.tool && this.tool.key === k) || this.mode === k);
    for (const b of document.querySelectorAll('.tool-btn[data-tool]')) {
      const k = b.dataset.tool;
      b.classList.toggle('active', !!isOn(k));
      if (k === 'to_step') b.disabled = !this.apps.cli;
      if (!WEB && DESKTOP_TOOLS.has(k)) b.hidden = !this.deskToolShown(k);
      if (WEB && DESKTOP_TOOLS.has(k)) { b.disabled = false; b.classList.add('desk-only'); b.title = `${b.title.replace(/ · desktop app$/, '')} · desktop app`; }
    }
    for (const b of document.querySelectorAll('.tool-btn[data-drop]')) {
      const d = this._drops?.[b.dataset.drop];
      b.classList.toggle('active', !!d?.items.some(([k]) => isOn(k)));
    }
  }

  onRibbon(k) {
    if (this.sketcher.active) {
      const sk = this.sketcher;
      if (['line', 'arc3', 'rect', 'rect_center', 'circle', 'polygon', 'slot', 'select', 'trim', 'sk_mirror', 'text', 'sk_useedge'].includes(k) || k.startsWith('c_')) sk.setTool(k);
      else if (k === 'sk_project') this.sketchProject();
      else if (k === 'sk_offset') this.sketchOffset();
      else if (k === 'construction') sk.toggleConstruction();
      else if (k === 'sk_delete') sk.deleteSelected();
      else if (k === 'look') this.viewer.lookAtPlane(sk.frameArr);
      else if (k === 'finish') this.finishSketch(false);
      else if (k === 'finish_extrude') this.finishSketch(true);
      else if (k === 'sk_cancel') this.cancelSketch();
      this.updateRibbonState();
      return;
    }
    if (WEB && DESKTOP_TOOLS.has(k)) return this.desktopOnly();
    if (k === 'sketch') return this.beginSketch();
    if (k === 'import') return this.cmd('import');
    if (k === 'export') return this.exportMenu();
    if (k === 'send') return this.sendMenu();
    if (k === 'measure') return this.startMeasure();
    if (k === 'section') return this.startSection();
    if (k === 'info') return this.startInfo();
    if (k === 'to_step') return this.toStep();
    if (k === 'supportfins' || k === 'supportwall') return this.openSupportTool(k);
    if (k === 'rotate') return this.openTool('move', { values: { _gizmo: 'rotate' } });
    if (k === 'sf_about') return this.aboutSupportFins();
    if (k === 'params') return this.editParams();
    if (TOOLS[k]) this.openTool(k);
  }

  // =================================================================== Support Fins
  /** The Support Fins tools are built from Support Fins' own settings list (its options.json,
   *  read from the engine), so settings a new Support Fins release adds appear here by themselves. */
  async supportTools() {
    if (this._sfInfo) return this._sfInfo;
    const inf = await post('supportfins/info', {}, { msg: 'Loading Support Fins…' });
    const schema = inf.schema, opts = schema.options;
    const fk = k => 'sf:' + k;
    const vis = (o, v) => {
      const get = k => { const x = v[fk(k)]; return x === undefined || x === null || x === '' ? opts.find(q => q.key === k)?.default : x; };
      const holds = c => typeof c === 'string' ? get(c) === true : c.any ? c.any.some(holds) : c.in.includes(get(c.key));
      return (o.showIf || []).every(holds);
    };
    const shown = o => o.percent ? Math.round(o.default * 100 * 1000) / 1000 : o.default;
    const fieldOf = o => {
      const base = { key: fk(o.key), label: o.label, help: o.tooltip, show: v => vis(o, v) };
      if (o.type === 'bool') return { ...base, kind: 'bool' };
      if (o.type === 'choice') return { ...base, kind: 'select', options: o.choices.map(c => [c.value, c.label]) };
      return { ...base, kind: 'number', unit: o.percent ? '%' : (/^mm\b/.test(o.hint || '') ? 'mm' : (/^[°%]/.test(o.hint || '') ? o.hint : '')) };
    };
    // Support Fins' sections, in its own order, as folding sections: the first `nOpen` start open
    const sections = (keys, nOpen) => {
      const out = [];
      for (const sec of schema.sections) {
        const items = opts.filter(o => o.section === sec.id && (!keys || keys.includes(o.key)));
        if (!items.length) continue;
        out.push({ key: '_hdr_' + sec.id, kind: 'hdr', label: sec.label, open: out.filter(f => f.kind === 'hdr').length < nOpen,
          show: v => items.some(o => vis(o, v)) }, ...items.map(fieldOf));
      }
      return out;
    };
    const defaults = keys => Object.fromEntries(opts.filter(o => !keys || keys.includes(o.key)).map(o => [fk(o.key), shown(o)]));
    const toOptions = (v, keys) => {
      const out = {};
      for (const o of opts) {
        if (keys && !keys.includes(o.key)) continue;
        let x = v[fk(o.key)];
        if (x === undefined || x === null || x === '') continue;
        if (o.type === 'number') { x = parseNum(x); if (o.percent) x = Math.round(x * 1e6 / 100) / 1e6; }
        // only what differs from Support Fins' defaults is kept, so a project follows the
        // defaults a newer Support Fins release brings
        if (x === o.default || (o.type === 'number' && Math.abs(x - o.default) < 1e-9)) continue;
        out[o.key] = x;
      }
      return out;
    };
    const fromOptions = (options = {}) => Object.fromEntries(Object.entries(options).map(([k, x]) => {
      const o = opts.find(q => q.key === k);
      return [fk(k), o?.percent && typeof x === 'number' ? Math.round(x * 100 * 1000) / 1000 : x];
    }));
    const output = { key: 'output', label: 'Add as', kind: 'select', options: [['join', 'Part of the body'], ['separate', 'Separate body']],
      help: 'Part of the body: the fins are joined to it, so it exports and slices as one object (what Support Fins itself does). Separate body: a body of its own you can hide or remove; export both together.' };
    // the last folding section, open to start with: how well the supports hold and what they
    // cost (the cards), the orientation and strength (Support Fins, Rotate), and the readout settings
    const checkSection = cover => [{ key: '_hdr_check', kind: 'hdr', label: 'Print check', open: true },
      { key: '_status', kind: 'sfstatus', cover },
      ...(cover ? [{ key: '_pose', label: 'Orientation', kind: 'sfpose', show: v => (v._mode || 'rotate') === 'rotate' }] : []),
      { key: '_mat', label: 'Material', kind: 'sfmat', help: 'The filament you print the supports in, for the weight under Support plastic.', changed: () => this.sfMaterial() !== 'pla' },
      ...(cover ? [{ key: '_reach', label: 'Bare beyond', kind: 'sfreach', changed: () => this.sfReach() !== 5,
        help: "Overhangs further than this from a fin, or from the part itself, are shown bright red: they'd sag or fail. Bridging over a few mm works; over 5 mm usually droops." }] : [])];
    // the settings a drawn wall uses (Support Fins' drawWall)
    const WALL_KEYS = ['threshold', 'nozzle', 'gap', 'tines', 'tineDensity', 'layerHeight', 'plateOnly'];
    const oneBody = ctx => {
      const sel = selBody(ctx).slice(0, 1);
      if (sel.length) return sel;
      const shown = ctx.state.bodies.filter(b => !b.hidden);
      return shown.length === 1 ? [shown[0].id] : [];
    };
    const MODES = [['rotate', 'Rotate'], ['add', 'Add fins'], ['remove', 'Remove fins']];
    TOOLS.supportfins = {
      label: 'Support Fins', icon: 'fins', type: 'supportfins', paint: true, gizmo: 'pose',
      fields: [{ key: 'body', label: 'Body', kind: 'bodies', multi: false }, output,
        { key: '_mode', label: 'On the part', kind: 'select', options: MODES, nolabel: true,
          help: 'Rotate: drag the rings to turn the part the way it will print (it stays on the bed, and the fins follow). Add fins: draw walls by hand where Auto left a gap. Remove fins: click a fin to take it away, click it again to bring it back.' },
        { key: 'walls', label: 'Hand walls', kind: 'walls', show: v => v._mode === 'add' },
        { key: 'removed', label: 'Removed', kind: 'removed', show: v => v._mode === 'remove' || (v.removed || []).length > 0 },
        ...sections(null, 2), ...checkSection(true)],
      defaults: ctx => ({ body: oneBody(ctx), _mode: 'rotate', walls: [], removed: [], output: 'join', ...defaults(null) }),
      toParams: v => {
        const p = { body: v.body[0], output: v.output, options: toOptions(v, null) };
        if ((v.walls || []).length) p.walls = v.walls;
        if ((v.removed || []).length) p.removed = v.removed;
        return p;
      },
      fromParams: p => ({ body: p.body ? [p.body] : [], _mode: 'rotate', walls: p.walls || [], removed: p.removed || [], output: p.output || 'join', ...defaults(null), ...fromOptions(p.options) }),
      hint: 'Red faces need support. Turn the part with the rings (it stays on the bed); Support Fins puts breakaway fins and a bed pad under the red.',
      modeHints: {
        rotate: 'Drag the rings to turn the part the way you want to print it (5° steps, Shift for any angle). It stays on the bed, and the fins are rebuilt.',
        add: null,
        remove: 'Click a fin to remove it; click a red one to bring it back. Walls you drew by hand are removed for good (Ctrl+Z in Add fins brings them back).',
      },
    };
    TOOLS.supportwall = {
      label: 'Draw Walls', icon: 'finwall', type: 'supportwall', wallTool: true, paint: true,
      fields: [{ key: 'body', label: 'Body', kind: 'bodies', multi: false, placeholder: 'The part you draw on' }, output, { key: 'walls', label: 'Walls', kind: 'walls' },
        ...sections(WALL_KEYS, 1), ...checkSection(false)],
      defaults: ctx => ({ body: oneBody(ctx), walls: [], output: 'join', ...defaults(WALL_KEYS) }),
      toParams: v => ({ body: v.body[0], walls: v.walls, output: v.output, options: toOptions(v, WALL_KEYS) }),
      fromParams: p => ({ body: p.body ? [p.body] : [], walls: Array.isArray(p.walls) ? p.walls : ((p.points || []).length === 2 ? [{ shape: 'line', points: p.points }] : []),
        output: p.output || 'join', ...defaults(WALL_KEYS), ...fromOptions(p.options) }),
      hint: this.walls.hint(),
    };
    this._sfInfo = inf;
    return inf;
  }

  async aboutSupportFins() {
    let inf;
    try { inf = await this.supportTools(); } catch (e) { this.toast(e.message, 'err'); return; }
    const pend = inf.pending ? `<br><br>Support Fins ${esc(String(inf.pending).replace(/^v/, ''))} has been downloaded and is used from the next time you start STL Editor. <a href="#" data-restart="1">Restart now</a>` : '';
    setTimeout(() => { const r = document.querySelector('#modal-body [data-restart]'); if (r) r.onclick = async ev => { ev.preventDefault(); try { await post('restart', {}, { quiet: true }); } catch (e) { this.toast(e.message, 'err'); } }; }, 0);
    this.alert('Support Fins', `Support Fins ${esc(inf.version || '')} (${esc((inf.commit || '').slice(0, 7))}) adds the breakaway support fins that let a part print tipped over, in its strong orientation. The fins come from Support Fins' own engine, unchanged.<br><br>
      By gittrahan, MIT licence: <a href="${esc(inf.source || 'https://github.com/gittrahan/support-fins')}" target="_blank" rel="noopener">github.com/gittrahan/support-fins</a> · <a href="https://printfins.com" target="_blank" rel="noopener">printfins.com</a>. The technique is Slant3D's designed-in supports.<br><br>
      ${WEB ? 'New Support Fins releases reach the web edition automatically.' : 'New Support Fins releases are downloaded automatically when update checks are on (Help › Update Settings).'}${pend}`);
  }

  async openSupportTool(k, opts) {
    try { await this.supportTools(); } catch (e) { this.toast(e.message, 'err'); return; }
    if (!opts?.editFeature) {
      // a part that already has supports: open those again instead of adding a second set on top
      const bid = TOOLS[k].defaults(this.ctx()).body?.[0];
      if (bid && this.reopenSupports(k, bid)) return;
    }
    return this.openTool(k, opts);
  }

  /** The Support Fins (or Draw Walls) step already on a body, the last one in the timeline. */
  supportFeatureFor(bid, types = ['supportfins', 'supportwall']) {
    return [...this.state.features].reverse().find(f => types.includes(f.type) && f.active !== false && !f.suppressed
      && (f.params?.body === bid || (f.params?.bodies || []).includes(bid))) || null;
  }

  /** Open the supports a body already has, for editing; true when there were some. */
  reopenSupports(k, bid) {
    const f = this.supportFeatureFor(bid, k === 'supportwall' ? ['supportwall', 'supportfins'] : ['supportfins', 'supportwall']);
    if (!f) return false;
    const key = f.type === 'supportwall' ? 'supportwall' : 'supportfins';
    this.toast(`${this.bodyName(bid)} already has ${f.name}: opened it to change it`, 'info');
    this.openTool(key, { editFeature: f, values: k === 'supportwall' && key === 'supportfins' ? { _mode: 'add' } : (k === 'supportfins' && key === 'supportfins' ? { _mode: 'rotate' } : undefined) });
    return true;
  }

  /** Where a Move, Scale or Lay Flat of these bodies goes in the timeline: before the supports
   *  they already have, so the fins are rebuilt for the new position instead of being carried
   *  along with the part (null: at the end, as usual). */
  insertBeforeSupports(type, params) {
    if (!['move', 'scale', 'layflat'].includes(type) || params.copy) return null;
    const ids = params.bodies || (params.body ? [params.body] : []);
    const end = this.state.rollback ?? this.state.features.length;
    let at = null;
    this.state.features.forEach((f, i) => {
      if (i >= end || at !== null) return;
      if (['supportfins', 'supportwall'].includes(f.type) && !f.suppressed && ids.some(b => f.params?.body === b || (f.params?.bodies || []).includes(b))) at = i;
    });
    return at;
  }

  /** Add a feature, before the part's supports when it moves the part (see insertBeforeSupports). */
  async addFeatureSmart(type, params, extra = {}) {
    const at = this.insertBeforeSupports(type, params);
    if (at === null) return { res: await post('feature/add', { type, params, ...extra }, extra.quiet ? { quiet: true } : { msg: extra.msg }), moved: false };
    const before = this.state.rollback ?? null;
    await post('rollback', { index: at }, { quiet: true });
    let res;
    try { res = await post('feature/add', { type, params, ...extra }, extra.quiet ? { quiet: true } : { msg: extra.msg }); }
    catch (e) {
      // not added: put the timeline back where it was, then report the error
      try { await this.apply(await post('rollback', { index: before }, { quiet: true })); } catch { /* the error below says enough */ }
      throw e;
    }
    // back to where the timeline was, one step further along
    const back = await post('rollback', { index: before === null ? null : before + 1 }, { quiet: true });
    const sf = this.state.features[at];
    if ((sf?.params?.walls || []).length || (sf?.params?.removed || []).length || sf?.type === 'supportwall')
      this.toast(`${sf.name} is rebuilt for the new position. Walls you drew by hand stay where they were drawn: check them in ${sf.name}.`, 'warn');
    else this.toast(`${sf.name} is rebuilt for the new position`, 'info');
    return { res: { ...res, state: back.state }, moved: true, at };
  }

  // =================================================================== browser tree
  renderTree() {
    const s = this.state;
    const eye = (hidden, attrs) => `<span class="eye ${hidden ? 'off' : ''}" ${attrs}>${icon(hidden ? 'eye_off' : 'eye')}</span>`;
    const node = (d, inner, cls = '', attrs = '') => `<div class="tnode ${cls}" style="--d:${d}" ${attrs}>${inner}</div>`;
    let h = node(0, `<span class="nm">${esc(s.doc.name || 'Untitled')}</span>`, 'folder');
    h += node(1, `${eye(!this.viewer.baseVisible, 'data-eye="origin"')}<span class="nm">Origin</span>`, 'folder');
    h += node(1, `<span class="nm">Bodies</span><span class="tag">${s.bodies.length}</span>`, 'folder');
    for (const b of s.bodies) {
      h += node(2, `${eye(b.hidden, `data-eye-body="${b.id}"`)}<span class="sw" style="background:${b.color}"></span><span class="nm" title="${esc(b.name)}">${esc(b.name)}</span>` +
        `<span class="tag ${b.solid ? '' : 'bad'}" title="${b.solid ? 'Watertight solid' : 'Not watertight: run Repair'}">${b.solid ? fmtTris(b.triangles) : 'needs repair'}</span>`,
        this.selection.includes(b.id) ? 'selected' : '', `data-body="${b.id}"`);
    }
    if (s.planes.length) {
      h += node(1, `<span class="nm">Planes</span>`, 'folder');
      for (const p of s.planes) h += node(2, `${eye(p.hidden, `data-eye-item="${p.id}"`)}<span class="nm">${esc(p.name)}</span>`, this.isActiveField('plane', p.id) ? 'selected' : '', `data-plane="${p.id}"`);
    }
    if ((s.params || []).length) {
      h += node(1, `<span class="nm">Parameters</span><span class="tag">${s.params.length}</span>`, 'folder', 'data-params="1"');
      for (const q of s.params) h += node(2, `<span class="nm" title="${esc(q.error || q.note || '')}">${esc(q.name)} = ${q.error ? '<span class="bad">?</span>' : esc(num(q.value))}</span>`, q.error ? 'err' : '', 'data-params="1"');
    }
    if (s.sketches.length) {
      h += node(1, `<span class="nm">Sketches</span>`, 'folder');
      for (const k of s.sketches) h += node(2, `${eye(k.hidden, `data-eye-item="${k.id}"`)}<span class="nm">${esc(k.name)}</span><span class="tag">${k.entities.length}</span>`, '', `data-sketch="${k.id}"`);
    }
    $('#tree').innerHTML = h;
    const n = s.bodies.length;
    $('#browser-count').textContent = n ? `${n} bod${n > 1 ? 'ies' : 'y'}` : '';
  }

  isActiveField() { return false; }

  // =================================================================== timeline
  renderTimeline() {
    const s = this.state;
    const editing = this.tool?.editId || (this.sketcher.active && this.sketcher.featureId);
    let h = '';
    s.features.forEach((f, i) => {
      if (s.rollback !== null && s.rollback !== undefined && i === s.rollback) h += `<div class="tl-marker" title="Rollback marker"></div>`;
      const picked = this.tool && this.tool.def.fields.some(x => x.kind === 'features' && (!x.show || x.show(this.tool.values)) && (this.tool.values[x.key] || []).includes(f.id));
      const cls = [f.error ? 'err' : '', f.warning ? 'warn' : '', f.suppressed ? 'supp' : '', f.active ? '' : 'inactive', editing === f.id || picked ? 'editing' : ''].join(' ');
      const tip = `${f.name}${f.error ? '\n⚠ ' + f.error : ''}${f.warning ? '\n' + f.warning : ''}${f.ms != null ? `\n${f.ms} ms` : ''}\nDouble-click to edit · right-click for more`;
      h += `<button class="tl-item ${cls}" data-fid="${f.id}" title="${esc(tip)}">${icon(FEATURE_ICON[f.type])}<span class="lbl">${esc(f.name)}</span></button>`;
    });
    if (s.rollback === null || s.rollback === undefined || s.rollback >= s.features.length) h += `<div class="tl-marker" title="End of history"></div>`;
    $('#timeline').innerHTML = h;
    const tl = $('#timeline'); tl.scrollLeft = tl.scrollWidth;
  }

  // =================================================================== tool panel
  ctx() {
    const last = [...this.state.features].reverse().find(f => f.type === 'sketch' && f.active);
    const sk = last && this.state.sketches.find(s => s.id === last.id);
    return { state: this.state, selection: this.selection, lastSketch: sk?.id || null,
      sketchOnBody: !!(last && last.params?.plane && typeof last.params.plane === 'object' && last.params.plane.body) };
  }

  async openTool(key, { editFeature, values } = {}) {
    await this.closeTool();
    this.exitModes();
    const def = TOOLS[key];
    let v = editFeature ? (def.fromParams ? def.fromParams(structuredClone(editFeature.params)) : structuredClone(editFeature.params)) : def.defaults(this.ctx());
    if (values) Object.assign(v, values);
    this.tool = { key, def, values: v, editId: editFeature?.id || null, activeField: null, prevRollback: this.state.rollback };
    if (editFeature) {
      const idx = this.state.features.findIndex(f => f.id === editFeature.id);
      await this.apply(await post('rollback', { index: idx }));
    }
    $('#tool-title').textContent = (editFeature ? 'Edit ' : '') + def.label;
    $('#tool-ok').textContent = 'OK'; $('#tool-cancel').textContent = 'Cancel';
    $('#tool-panel').classList.remove('hidden');
    this.tool.activeField = def.fields.find(f => isSelKind(f.kind) && !f.placeholder && f.kind !== 'regions' && (f.show ? f.show(v) : true) && isEmptySel(v[f.key]))?.key || null;
    this.renderForm();
    this.hint(this.modeHint());
    this.viewer.setMouseMode(def.sculpt ? 'sculpt' : 'model');
    if (def.paint) this.quickPaint();
    if (def.type === 'supportfins') this.sfStrength();
    this.updateRibbonState();
    this.renderTimeline();
    this.renderEmpty();
    this.viewer.resize();
    this.schedulePreview(0);
  }

  async closeTool(committed = false) {
    if (!this.tool) return;
    const t = this.tool;
    this.tool = null;
    clearTimeout(this._busyTimer);
    this.sfCoverageStop();
    this.walls.reset();
    this.gizmo.hide();
    this.viewer.clearPreview();
    this.viewer.clearOverlay();
    this.viewer.setBasePlanesVisible(false);
    this.viewer.setMouseMode('model'); this.viewer.setBrush(null); this.viewer.clearOverlay('strokes');
    $('#tool-panel').classList.add('hidden');
    $('#tool-msg').innerHTML = '';
    this.toolCredit();
    this.hint('');
    if (t.editId && !committed) await this.apply(await post('rollback', { index: t.prevRollback ?? null }));
    this.updateRibbonState();
    this.renderTimeline();
    this.renderEmpty();
    this.viewer.resize();
  }

  renderForm() {
    const t = this.tool; if (!t) return;
    const v = t.values;
    let h = '';
    // a header ('hdr') folds the fields after it, up to the next header
    const fields = t.def.fields, shown = f => !f.show || f.show(v);
    for (let i = 0; i < fields.length; i++) {
      const f = fields[i];
      if (f.kind === 'hdr') {
        if (!shown(f)) continue;
        let end = i + 1;
        while (end < fields.length && fields[end].kind !== 'hdr') end++;
        const open = this.sectionOpen(t.key, f);
        const changed = open ? 0 : fields.slice(i + 1, end).filter(x => shown(x) && this.fieldChanged(x)).length;
        h += this.foldHTML(f, open, changed);
        if (!open) i = end - 1;
        continue;
      }
      if (!shown(f)) continue;
      h += this.fieldHTML(f, v[f.key], t.activeField === f.key);
    }
    if (t.def.extra === 'dropToBed') h += `<div class="field"><label></label><div class="ctl"><button id="drop-bed">Drop to bed</button><button id="center-origin">Centre on origin</button></div></div>`;
    if (t.def.noLivePreview) h += `<div class="field"><label></label><div class="ctl"><button id="tool-preview">Preview</button></div></div>`;
    if (t.def.sculpt) h += `<div class="field"><label>Strokes</label><div class="ctl">${(v.strokes || []).length} on ${esc(v.body ? this.bodyName(v.body) : '—')}<button id="sc-undo" ${(v.strokes || []).length ? '' : 'disabled'}>Undo stroke</button></div></div>`;
    $('#tool-form').innerHTML = h;
    this.renderOverlayForTool();
    this.renderTree();
    this.renderTimeline();
  }

  /** Folding sections: open or closed per tool and header, remembered on this computer. */
  sectionStore() {
    if (!this._sections) {
      try { const s = JSON.parse(localStorage.getItem('stle_sections') || '{}'); this._sections = s && typeof s === 'object' && !Array.isArray(s) ? s : {}; }
      catch { this._sections = {}; }
    }
    return this._sections;
  }

  sectionOpen(tool, f) {
    const st = this.sectionStore(), k = `${tool}:${f.key}`;
    return typeof st[k] === 'boolean' ? st[k] : f.open !== false;
  }

  toggleSection(key) {
    const t = this.tool; if (!t) return;
    const f = t.def.fields.find(x => x.key === key && x.kind === 'hdr'); if (!f) return;
    const st = this.sectionStore();
    st[`${t.key}:${key}`] = !this.sectionOpen(t.key, f);
    try { localStorage.setItem('stle_sections', JSON.stringify(st)); } catch { /* storage off: kept for this session */ }
    this.renderForm();
  }

  /** Whether a field holds something other than the tool's default (shown on a closed section). */
  fieldChanged(f) {
    const t = this.tool; if (!t) return false;
    if (f.changed) { try { return !!f.changed(t.values); } catch { return false; } }
    if (!t._defs) { try { t._defs = t.def.defaults(this.ctx()) || {}; } catch { t._defs = {}; } }
    if (!(f.key in t._defs)) return false;
    const a = t.values[f.key], b = t._defs[f.key];
    const na = typeof a === 'string' && a.trim() !== '' && !isExpr(a) ? Number(a) : a, nb = typeof b === 'string' && b.trim() !== '' && !isExpr(b) ? Number(b) : b;
    if (typeof na === 'number' && typeof nb === 'number' && Number.isFinite(na) && Number.isFinite(nb)) return Math.abs(na - nb) > 1e-9;
    return JSON.stringify(a ?? null) !== JSON.stringify(b ?? null);
  }

  foldHTML(f, open, changed) {
    return `<button type="button" class="field-hdr fold${open ? ' open' : ''}" data-fold="${esc(f.key)}" aria-expanded="${open}"><span class="chev" aria-hidden="true">›</span>`
      + `<span class="lbl">${esc(f.label)}</span>${changed ? `<span class="chg">${changed} changed</span>` : ''}</button>`;
  }

  /** A segmented control only when every option fits on one line; otherwise a list. */
  segFits(options, wide) {
    if (options.some(([, l]) => String(l).length > 16)) return false;
    const form = $('#tool-form');
    const inner = (form?.clientWidth || 288) - 20;      // the form's 10px padding each side
    const avail = wide ? inner : inner - 92 - 6;       // the label column and the gap
    try {
      const c = this._segCtx || (this._segCtx = document.createElement('canvas').getContext('2d'));
      c.font = `11px ${getComputedStyle(document.body).fontFamily}`;
      const need = options.reduce((s, [, l]) => s + c.measureText(String(l)).width + 7, 2);   // 3px padding a side + divider; the border
      return need <= avail;
    } catch { return options.reduce((s, [, l]) => s + String(l).length * 5.6 + 7, 2) <= avail; }
  }

  fieldHTML(f, val, active) {
    const unit = f.unit ? `<span class="unit">${f.unit}</span>` : '';
    const lab = f.help ? `<label title="${esc(f.help)}">${f.label} <span class="help">?</span></label>` : `<label>${f.label}</label>`;
    const wrap = (ctl, cls = '') => f.nolabel
      ? `<div class="field wide nolabel ${cls}" data-key="${f.key}"><div class="ctl"${f.help ? ` title="${esc(f.help)}"` : ''}>${ctl}</div></div>`
      : `<div class="field ${cls}" data-key="${f.key}">${lab}<div class="ctl">${ctl}</div></div>`;
    switch (f.kind) {
      case 'number': {
        const isLen = f.unit === 'mm';
        if (isExpr(val)) return wrap(`<input type="text" class="expr" data-num="${f.key}" value="${esc(val)}" title="${esc(this.exprTitle(val, isLen))}">${isLen ? '<span class="unit">mm</span>' : unit}`);
        const shown = isLen && typeof val === 'number' ? U.toDisp(val) : (val ?? '');
        return wrap(`<input type="text" inputmode="decimal" data-num="${f.key}" value="${esc(shown)}">${isLen ? `<span class="unit">${U.label}</span>` : unit}`);
      }
      case 'text': return wrap(`<input type="text" data-text="${f.key}" value="${esc(val ?? '')}" placeholder="optional">`);
      case 'bool': return wrap(`<label class="check"><input type="checkbox" data-bool="${f.key}" ${val ? 'checked' : ''}></label>`);
      case 'select':
        if (f.options.length <= 4 && this.segFits(f.options, f.nolabel)) return wrap(`<div class="seg">${f.options.map(([k, l]) => `<button data-seg="${f.key}" data-val="${k}" class="${val === k ? 'on' : ''}">${l}</button>`).join('')}</div>`);
        if (f.groups) {
          // options [value, label, group], shown under their group's heading
          const groups = new Map();
          for (const [k, l, g = ''] of f.options) { if (!groups.has(g)) groups.set(g, []); groups.get(g).push([k, l]); }
          return wrap(`<select data-sel="${f.key}">${[...groups].map(([g, items]) => (g ? `<optgroup label="${esc(g)}">` : '') + items.map(([k, l]) => `<option value="${esc(k)}" ${val === k ? 'selected' : ''}>${esc(l)}</option>`).join('') + (g ? '</optgroup>' : '')).join('')}</select>`);
        }
        return wrap(`<select data-sel="${f.key}">${f.options.map(([k, l]) => `<option value="${k}" ${val === k ? 'selected' : ''}>${l}</option>`).join('')}</select>`);
      case 'vec3': {
        const isLen = f.unit === 'mm';
        return wrap([0, 1, 2].map(i => isExpr(val?.[i]) ? `<input type="text" class="expr" data-vec="${f.key}" data-i="${i}" value="${esc(val[i])}" title="${esc(this.exprTitle(val[i], isLen))}">`
          : `<input type="text" data-vec="${f.key}" data-i="${i}" value="${esc(isLen ? U.toDisp(val?.[i] ?? 0) : (val?.[i] ?? 0))}">`).join('') + (isLen ? `<span class="unit">${U.label}</span>` : unit), 'vec');
      }
      case 'sketch': {
        const opts = this.state.sketches.map(s => `<option value="${s.id}" ${val === s.id ? 'selected' : ''}>${esc(s.name)}</option>`).join('');
        return wrap(`<select data-sel="${f.key}"><option value="">— pick a sketch —</option>${opts}</select>`);
      }
      case 'axisline': {
        const sk = this.state.sketches.find(s => s.id === this.tool.values.sketch);
        const lines = (sk?.entities || []).filter(e => (e.type === 'polyline' && !e.closed) || e.type === 'line');
        const opts = [['x', 'Sketch X axis'], ['y', 'Sketch Y axis'], ...lines.map((e, i) => [e.id, `${e.construction ? 'Construction line' : 'Line'} ${i + 1}`])];
        return wrap(`<select data-sel="${f.key}">${opts.map(([k, l]) => `<option value="${k}" ${val === k ? 'selected' : ''}>${l}</option>`).join('')}</select>`);
      }
      case 'bodies': {
        const ids = Array.isArray(val) ? val : (val ? [val] : []);
        const chips = ids.map(id => `<span class="chip">${esc(this.bodyName(id))}<b data-unpick="${f.key}" data-id="${id}">×</b></span>`).join('');
        return wrap(`<div class="sel-box ${active ? 'active' : ''}" data-pick="${f.key}">${chips || `<span class="ph">${esc(f.placeholder || (f.multi ? 'Click bodies' : 'Click a body'))}</span>`}</div>`);
      }
      case 'plane': {
        const lbl = this.planeLabel(val);
        return wrap(`<div class="sel-box ${active ? 'active' : ''}" data-pick="${f.key}">${lbl ? `<span class="chip">${esc(lbl)}<b data-unpick="${f.key}">×</b></span>` : '<span class="ph">Click a plane or flat face</span>'}</div>`);
      }
      case 'facepoints': {
        const n = (val || []).length;
        return wrap(`<div class="sel-box ${active ? 'active' : ''}" data-pick="${f.key}">${n ? `<span class="chip">${n} point${n > 1 ? 's' : ''}<b data-unpick="${f.key}">×</b></span>` : '<span class="ph">Click on a face</span>'}</div>`);
      }
      case 'cylface': {
        const lbl = val?.label;
        return wrap(`<div class="sel-box ${active ? 'active' : ''}" data-pick="${f.key}">${lbl ? `<span class="chip">${esc(lbl)}<b data-unpick="${f.key}">×</b></span>` : '<span class="ph">Click the wall of a round hole or pin</span>'}</div>`);
      }
      case 'pface': {
        const lbl = val?.label;
        return wrap(`<div class="sel-box ${active ? 'active' : ''}" data-pick="${f.key}">${lbl ? `<span class="chip">${esc(lbl)}<b data-unpick="${f.key}">×</b></span>` : '<span class="ph">Click a flat face</span>'}</div>`);
      }
      case 'regions': {
        const n = (val || []).length;
        return wrap(`<div class="sel-box ${active ? 'active' : ''}" data-pick="${f.key}" title="Click to pick single regions of the sketch">${n ? `<span class="chip">${n} region${n > 1 ? 's' : ''}<b data-unpick="${f.key}">×</b></span>` : '<span class="ph">All (click to pick)</span>'}</div>`);
      }
      case 'features': {
        const sel = new Set(val || []);
        const list = repeatableFeatures(this.state).filter(x => x.id !== this.tool.editId);
        const rows = list.map(x => `<label class="ent" style="justify-content:flex-start;gap:6px"><input type="checkbox" data-feat="${f.key}" data-id="${x.id}" ${sel.has(x.id) ? 'checked' : ''}>${icon(FEATURE_ICON[x.type]).replace('<svg ', '<svg style="width:16px;height:16px" ')}<span>${esc(x.name)}</span><span class="t" style="margin-left:auto">${esc(x.params.operation || (x.type === 'hole' ? 'cut' : ''))}</span></label>`).join('');
        return `<div class="field wide" data-key="${f.key}"><label>${f.label}</label><div class="ent-list">${rows || '<div class="ent"><span class="t">No cuts, holes or extrudes yet</span></div>'}</div></div>`;
      }
      case 'points3': {
        const n = (val || []).length, c = f.count || 3;
        return wrap(`<div class="sel-box ${active ? 'active' : ''}" data-pick="${f.key}">${n ? `<span class="chip">${n} of ${c}<b data-unpick="${f.key}">×</b></span>` : `<span class="ph">Click ${c} points</span>`}</div>`);
      }
      case 'hdr': return this.foldHTML(f, true, 0);
      case 'walls': return this.walls.fieldHTML(f);
      case 'sfpose': return this.sfPoseHTML(f);
      case 'sfstatus': return this.sfStatusHTML(f);
      case 'sfmat': {
        const mat = this.sfMaterial();
        return wrap(`<select data-sfmat="1">${Object.entries(MATERIALS).map(([k, [l]]) => `<option value="${k}" ${k === mat ? 'selected' : ''}>${l}</option>`).join('')}</select>`);
      }
      case 'sfreach': return wrap(`<input type="text" inputmode="decimal" data-sfreach="1" value="${num(this.sfReach())}"><span class="unit">mm</span>`);
      case 'removed': {
        const n = (val || []).length;
        return `<div class="field" data-key="${f.key}"><label>${f.label}</label><div class="ctl walls-ctl"><span class="walls-n">${n ? `${n} fin${n > 1 ? 's' : ''}` : '<span class="ph">None: click a fin to remove it</span>'}</span>${n ? '<button data-sfact="restore">Bring all back</button>' : ''}</div></div>`;
      }
    }
    return '';
  }

  bodyName(id) { return this.state.bodies.find(b => b.id === id)?.name || id; }
  planeLabel(v) {
    if (!v) return '';
    if (typeof v === 'string') return ({ XY: 'XY plane', XZ: 'XZ plane', YZ: 'YZ plane' })[v] || this.state.planes.find(p => p.id === v)?.name || v;
    if (v.label) return v.label;
    return 'Face';
  }

  setValue(key, val, rerender = true) {
    const t = this.tool; if (!t) return;
    t.values[key] = val;
    if (t.def.onChange) { const patch = t.def.onChange(key, t.values); if (patch) { Object.assign(t.values, patch); rerender = true; } }
    if (t.def.paint && (key === 'body' || key === 'sf:threshold')) this.quickPaint();
    if (key === '_mode') { this.walls.reset(); this.viewer.setPreviewHover(null); this.hint(this.modeHint()); rerender = true; if (val === 'rotate') this.sfStrength(); }
    if (key === '_load') { this.sfStrength(); rerender = true; }
    if (t.key === 'scale' && key === 'size' && t.values.uniform && t.values._size0) {
      // keep proportions: scale the other axes from the edited one
      const s0 = t.values._size0, prev = t._lastSize || s0;
      const i = val.findIndex((x, j) => Math.abs(x - prev[j]) > 1e-9);
      if (i >= 0 && s0[i] > 0) { const k = val[i] / s0[i]; t.values.size = s0.map(x => +(x * k).toFixed(4)); }
      t._lastSize = t.values.size;
      rerender = true;
    }
    if (rerender) this.renderForm();
    else if (t.def.gizmo) this.gizmo.sync();
    this.schedulePreview();
  }

  validTool() {
    const t = this.tool; if (!t) return false;
    for (const f of t.def.fields) {
      if (f.show && !f.show(t.values)) continue;
      const v = t.values[f.key];
      if (['bodies'].includes(f.kind) && !f.placeholder && isEmptySel(v)) return false;
      if (['plane', 'sketch'].includes(f.kind) && !v) return false;
      if (f.kind === 'facepoints' && !(v || []).length) return false;
      if (f.kind === 'points3' && (v || []).length !== (f.count || 3)) return false;
      if (f.kind === 'walls' && !(v || []).length && t.def.wallTool) return false;
      if (f.kind === 'features' && !(v || []).length) return false;
      if ((f.kind === 'pface' || f.kind === 'cylface') && !v) return false;
    }
    return true;
  }

  params() {
    const t = this.tool;
    const v = structuredClone(t.values);
    for (const f of t.def.fields) if (f.kind === 'number' && !isExpr(v[f.key])) v[f.key] = parseNum(v[f.key]);
    const p = t.def.toParams ? t.def.toParams(v) : v;
    for (const k of Object.keys(p)) if (k.startsWith('_') || p[k] === undefined) delete p[k];
    return p;
  }

  schedulePreview(delay = 280) {
    clearTimeout(this._previewTimer);
    if (!this.tool || this.tool.def.noLivePreview) return;
    this._previewTimer = setTimeout(() => this.runPreview(), delay);
  }

  async runPreview() {
    const t = this.tool; if (!t) return;
    if (!this.validTool() || (t.def.sculpt && !(t.values.strokes || []).length)) {
      // a preview still on its way is out of date now: it mustn't land after this
      this._previewSeq++; clearTimeout(this._busyTimer);
      this.viewer.clearPreview(); this.toolMsg(''); this._sfPieces = null;
      if (t.def.wallTool) { t._wallStatus = []; }
      // no part picked: its red overhangs and the coverage go too (with a part, they stay)
      if (t.def.paint && !(Array.isArray(t.values.body) ? t.values.body[0] : t.values.body)) this.quickPaint();
      return;
    }
    const seq = ++this._previewSeq;
    clearTimeout(this._busyTimer);
    if (t.def.paint) this._busyTimer = setTimeout(() => { if (seq === this._previewSeq && this.tool === t) this.toolMsg(t.def.wallTool ? 'Building the walls…' : 'Building supports…', 'busy'); }, 150);
    try {
      const pack = await post('preview', { type: t.def.type, params: this.params(), edit_id: t.editId }, { quiet: true });
      if (seq !== this._previewSeq || this.tool !== t || this.gizmo.tc.dragging) return;
      this.gizmo.reset();
      clearTimeout(this._busyTimer);
      const r = pack.result;
      if (t.def.wallTool || t.def.type === 'supportfins') { t._wallStatus = r.walls || []; if (t.def.wallTool || t.values._mode === 'add') this.renderForm(); }
      this._sfPieces = r.pieces || null;
      if ('support' in r || t.def.paint) { const had = JSON.stringify(t._support || null); t._support = r.support || null; if (had !== JSON.stringify(t._support)) this.renderForm(); }
      if (r.error) {
        this.viewer.clearPreview(); this.toolMsg(r.error, 'err');
        if (t.def.type === 'supportfins' && !t._cover?.none) { this.sfCoverageStop(); t._cover = { none: true }; this.renderForm(); }
        return;
      }
      this.viewer.showPreview(pack);
      let warn = r.warning;
      // Support Fins' Rotate mode: "no overhangs in this position" is said by the Held card
      // ("Nothing to hold up", "Turn it to the way you'll print it"), not again up here
      if (warn && t.def.type === 'supportfins' && (t.values._mode || 'rotate') === 'rotate')
        warn = warn.replace(/"[^"]*" has no overhangs in this position\. [^.]*\.\s*/g, '').trim() || null;
      this.toolMsg(warn ? (t.def.wallTool && r.info ? `${r.info}. ${warn}` : warn) : (r.flipped ? 'Cutting into the material (direction flipped).' : (r.info || '')), warn ? 'warn' : 'info');
      if (r.paint && t.def.paint) {
        // the faces Support Fins holds up (red), and the small ones it leaves to bridge (amber)
        this.viewer.clearOverlay('sf-over'); this.viewer.clearOverlay('sf-small');
        this.viewer.highlightFace(r.paint.body, r.paint.over, 'sf-over', 0xd64545);
        this.viewer.highlightFace(r.paint.body, r.paint.small, 'sf-small', 0xe0a030);
        if (t.def.type === 'supportfins') this.sfCoverage(r);
      }
    } catch (e) { if (seq === this._previewSeq) { clearTimeout(this._busyTimer); this.toolMsg(e.message, 'err'); } }
  }

  /** The hint for the tool, or for the mode Support Fins is in. */
  modeHint() {
    const t = this.tool; if (!t) return '';
    if (t.def.wallTool) return this.walls.hint();
    if (t.def.modeHints) {
      const m = t.values._mode || 'rotate';
      return t.def.modeHints[m] || `Draw walls by hand where the auto fins left a gap. ${this.walls.hint()}`;
    }
    return t.def.hint || '';
  }

  /** Paint the faces that need support straight away, from the mesh on screen (Support Fins'
   *  own angle test); the exact faces replace them when the engine's build comes back. */
  quickPaint() {
    const t = this.tool; if (!t?.def.paint) return;
    const body = Array.isArray(t.values.body) ? t.values.body[0] : t.values.body;
    this.viewer.clearOverlay('sf-over'); this.viewer.clearOverlay('sf-small');
    this.viewer.clearOverlay('sf-bare'); this.viewer.clearOverlay('sf-held');
    if (t._cover) { t._cover = null; this.sfCoverageStop(); this.renderForm(); }
    if (!body) return;
    const thr = parseNum(t.values['sf:threshold']);
    const faces = this.viewer.overhangFaces(body, Number.isFinite(thr) && thr > 0 ? thr : 45);
    this.viewer.highlightFace(body, faces, 'sf-over', 0xd64545);
    if (!faces.length && !t.def.wallTool) this.toolMsg('No overhangs in this position: nothing needs support. Rotate the part the way you want to print it.', 'info');
  }

  /** How far an overhang may be from a support before it counts as bare (mm; kept per computer). */
  sfReach() { try { const v = parseFloat(localStorage.getItem('stle_sf_reach')); return v > 0 ? v : 5; } catch { return 5; } }

  /** After the fins are built: which overhangs they hold (pale) and which are left bare
   *  (bright red), worked out off the page's thread (sfcoverage.js). */
  sfCoverage(r = this._covLast) {
    const t = this.tool; if (!t || !r?.paint) return;
    this._covLast = r;
    const o = this.viewer.bodies.get(r.paint.body); if (!o) return;
    if (!(r.paint.over || []).length) {
      // no overhangs in this pose: nothing to check
      this.sfCoverageStop();
      this.viewer.clearOverlay('sf-bare'); this.viewer.clearOverlay('sf-held');
      t._cover = { total: 0, held: 0, spots: [] }; this.renderForm();
      return;
    }
    const ghosts = new Set(r.ghosts || []);
    const fins = [];
    for (const c of this.viewer.previewGroup.children) {
      if (!c.isMesh || c.userData.kind !== 'preview' || c.userData.id === r.paint.body || ghosts.has(c.userData.id)) continue;
      const g = c.geometry.index ? c.geometry.toNonIndexed() : c.geometry;
      fins.push(Float32Array.from(g.attributes.position.array));
      if (g !== c.geometry) g.dispose();
    }
    const gap = parseNum(t.values['sf:gap']);
    const pos = o.geo.attributes.position.array;
    const idx = o.geo.index ? Uint32Array.from(o.geo.index.array) : Uint32Array.from({ length: pos.length / 3 }, (_, i) => i);
    const args = { pos: Float32Array.from(pos), idx, over: r.paint.over,
      fins, gap: gap > 0 ? gap : 0.2, reach: this.sfReach() };
    this.sfCoverageStop();
    const seq = this._covSeq;
    t._cover = { busy: true }; this.renderForm();
    if (!this._covWorker) {
      this._covWorker = new Worker(new URL('./sfcoverage.worker.js', import.meta.url), { type: 'module' });
      this._covWorker.onmessage = e => this.sfCoverageDone(e.data);
    }
    this._covTool = t; this._covBusy = true;
    this._covWorker.postMessage({ seq, args }, [args.pos.buffer, args.idx.buffer, ...fins.map(f => f.buffer)]);
  }

  /** Drop the check that's still running: its answer is out of date. Without this, quick
   *  changes queue up full checks one after another and the colours lag seconds behind. */
  sfCoverageStop() {
    this._covSeq = (this._covSeq || 0) + 1;
    if (this._covBusy && this._covWorker) { this._covWorker.terminate(); this._covWorker = null; }
    this._covBusy = false;
  }

  sfCoverageDone({ seq, out, error }) {
    if (seq === this._covSeq) this._covBusy = false;
    const t = this.tool;
    if (seq !== this._covSeq || !t || t !== this._covTool) return;
    this.viewer.clearOverlay('sf-bare'); this.viewer.clearOverlay('sf-held');
    if (error) { t._cover = { error }; this.renderForm(); return; }
    // the overhangs: pale where a fin (or the part) holds them, bright red where nothing does
    this.viewer.clearOverlay('sf-over');
    this.viewer.addDistTris(out.tris, out.dist, this.sfReach(), 'sf-bare', [0xe6a3a3, 0.3], [0xe8171d, 0.85]);
    t._cover = { total: out.total, held: out.held, spots: out.spots, reach: this.sfReach() };
    this.renderForm();
  }

  /** Remove mode: a click on a fin takes it away; a click on a removed (red) one brings it back. */
  removeFinAt(e) {
    const t = this.tool, id = this.viewer.pickPreview(e), info = id && this._sfPieces?.[id];
    if (!info) { this.toolMsg('Click a green fin to remove it.', 'info'); return; }
    if (info.kind === 'hand') {
      const w = [...(t.values.walls || [])]; w.splice(info.wall, 1);
      t._wallStatus = []; this.setValue('walls', w);
    } else if (info.removed) {
      const list = [...(t.values.removed || [])];
      let best = -1, bd = Infinity;
      list.forEach((r, i) => { const d = Math.hypot(...r.c.map((x, k) => x - info.c[k])); if (r.kind === info.kind && d < bd) { bd = d; best = i; } });
      if (best >= 0) list.splice(best, 1);
      this.setValue('removed', list);
    } else {
      this.setValue('removed', [...(t.values.removed || []), { kind: info.kind, c: info.c, size: info.size }]);
    }
    this.viewer.setPreviewHover(null);
    this.schedulePreview(0);
  }

  /** A handle was let go in Move / Rotate or Scale: put the part there at once. The first time
   *  the step is added to the timeline, then the tool goes on editing that same step. */
  async autoAccept() {
    const t = this.tool; if (!t) return;
    // one at a time: a drop while the last one is still on its way is sent after it, as an
    // edit of the same step (the tool's numbers are the whole move, so the latest ones win)
    if (t._accepting) { t._acceptAgain = true; return; }
    t._accepting = true;
    try {
      do { t._acceptAgain = false; await this._autoAccept(t); } while (t._acceptAgain && this.tool === t);
    } finally { t._accepting = false; }
  }

  async _autoAccept(t) {
    if (this.tool !== t || !this.validTool()) return;
    const params = this.params();
    try {
      if (t.editId) await this.apply(await post('feature/edit', { id: t.editId, params }, { quiet: true }));
      else {
        const before = this.state.rollback ?? null;
        const { res: r } = await this.addFeatureSmart(t.def.type, params, { quiet: true });
        if (this.tool !== t) return;
        const idx = r.state.features.findIndex(f => f.id === r.feature.id);
        t.editId = r.feature.id;
        t.prevRollback = before === null ? null : before + 1;
        await this.apply(await post('rollback', { index: idx }));
      }
      if (this.tool !== t) return;
      t.accepted = true;
      $('#tool-ok').textContent = 'Done'; $('#tool-cancel').textContent = 'Close';
      this.renderTimeline();
      this.runPreview();
    } catch (e) { this.toolMsg(e.message, 'err'); }
  }

  /** Support Fins' Rotate mode: a turn of the rings is applied to the part straight away, as
   *  one Rotate step on the timeline for the whole session (exact: built from the turns so far
   *  as one matrix), and walls drawn by hand turn with it. */
  async poseDrop(m) {
    const t = this.tool; if (!t) return;
    // one turn at a time: while one is on its way the handles take no new drag (Gizmo.sync),
    // so the next turn starts from the pose this one gives
    if (t._posing) return;
    const bid = t.values.body?.[0], b = this.state.bodies.find(x => x.id === bid);
    if (!b?.bbox) return;
    const ps = t._pose || (t._pose = { fid: null, M: new THREE.Matrix4(),
      c0: new THREE.Vector3(...[0, 1, 2].map(i => (b.bbox[0][i] + b.bbox[1][i]) / 2)) });
    // the whole turn so far; kept only once the engine has it (a failed turn isn't counted)
    const M = ps.M.clone().premultiply(m);
    const c0 = ps.c0, img = c0.clone().applyMatrix4(M);
    const params = { bodies: [bid], translate: [img.x - c0.x, img.y - c0.y, img.z - c0.z].map(x => Math.round(x * 1e5) / 1e5),
      rotate: rotOf(new THREE.Quaternion().setFromRotationMatrix(M)), pivot: 'center', bed: false, copy: false };
    t._posing = true;
    t._poseGen = (t._poseGen || 0) + 1;   // suggestions worked out for the old pose are dropped
    this.gizmo.tc.enabled = false;
    this.toolMsg('Turning the part…', 'busy');
    try {
      let res;
      if (ps.fid) res = await post('feature/edit', { id: ps.fid, params }, { quiet: true });
      else { res = await post('feature/add', { type: 'move', params, name: 'Rotate' }, { quiet: true }); ps.fid = res.feature.id; }
      ps.M = M;
      if (this.tool !== t) { await this.apply(res); return; }
      // walls drawn by hand turn with the part; removed fins are rebuilt, so they come back
      t.values.walls = (t.values.walls || []).map(w => ({ ...w, points: w.points.map(p => new THREE.Vector3(...p).applyMatrix4(m).toArray().map(x => +x.toFixed(4))) }));
      t.values.removed = [];
      // a load points the same way on the part after a turn, so it turns with it
      const lv = this.sfLoad();
      if (lv) {
        const w = new THREE.Vector3(...lv).transformDirection(m).toArray().map(x => Math.abs(x) < 1e-6 ? 0 : +x.toFixed(4));
        t.values._load = Object.entries(LOADS).find(([, d]) => d && d.every((x, i) => Math.abs(x - w[i]) < 1e-3))?.[0] || w;
      }
      t._suggest = null;
      this.gizmo.reset();
      this.viewer.clearPreview();
      await this.apply(res);
      this.quickPaint();
      this.renderForm();
      this.schedulePreview(0);
      this.sfStrength();
    } catch (e) { this.gizmo.reset(); this.toolMsg(e.message, 'err'); }
    finally {
      t._posing = false; t._poseGen++;
      if (this.tool === t) this.gizmo.sync();
    }
  }

  /** The parameter values, for showing what an expression comes to. */
  paramValues() { return Object.fromEntries((this.state.params || []).filter(q => q.value != null).map(q => [q.name, q.value])); }

  exprTitle(x, isLen) {
    try { const v = resolveDeep(x, this.paramValues()); return `${x.trim().slice(1)} = ${num(v)}${isLen ? ' mm' : ''}`; }
    catch (e) { return e.message; }
  }

  /** The tool's values with every expression worked out (for previews on screen, the handles). */
  resolved(v) { try { return resolveDeep(v, this.paramValues()); } catch { return v; } }

  /** Parameters: named values (they can use each other) for any number in the timeline. */
  async editParams(rows = null) {
    rows = rows || (this.state.params || []).map(q => ({ name: q.name, expr: q.expr, note: q.note || '' }));
    if (!rows.length) rows = [{ name: '', expr: '', note: '' }];
    const used = new Set();
    for (const f of this.state.features) JSON.stringify(f.params || {}).replace(/"=([^"]*)"/g, (m, e) => { for (const n of e.match(/[A-Za-z_][A-Za-z0-9_]*/g) || []) used.add(n); return m; });
    const rowHTML = (q, i) => `<tr data-row="${i}"><td><input type="text" data-pn value="${esc(q.name)}" placeholder="name" spellcheck="false"></td><td><input type="text" data-pe value="${esc(q.expr)}" placeholder="40 or width/2" spellcheck="false"></td><td class="pv"></td><td><input type="text" data-pc value="${esc(q.note)}" placeholder="note"></td><td><button data-pdel title="Remove">×</button></td></tr>`;
    const html = `<h3>Parameters</h3><p class="small" style="margin:0 0 8px;color:var(--muted)">Name a value once and use it in any number: type the name (or <code>=width/2 + 3</code>) in a tool's field. Change a value here and every step that uses it is rebuilt. Lengths are in mm, angles in degrees; a parameter can use others.</p>
      <table class="params"><thead><tr><th>Name</th><th>Value</th><th></th><th>Note</th><th></th></tr></thead><tbody>${rows.map(rowHTML).join('')}</tbody></table>
      <button data-padd>+ Add parameter</button><div class="pmsg"></div>`;
    const read = () => [...document.querySelectorAll('#modal-body tr[data-row]')].map(tr => ({ name: tr.querySelector('[data-pn]').value.trim(), expr: tr.querySelector('[data-pe]').value.trim(), note: tr.querySelector('[data-pc]').value }));
    const live = () => {
      const list = read().filter(q => q.name || q.expr);
      const { values, errors } = resolveParams(list);
      document.querySelectorAll('#modal-body tr[data-row]').forEach(tr => {
        const n = tr.querySelector('[data-pn]').value.trim(), cell = tr.querySelector('.pv');
        cell.innerHTML = !n ? '' : errors[n] ? `<span class="bad" title="${esc(errors[n])}">${esc(errors[n])}</span>` : values[n] != null ? `= ${esc(num(values[n]))}${used.has(n) ? ' <span class="muted">· used</span>' : ''}` : '';
      });
    };
    const result = this.modal(html, [{ label: 'Cancel', value: () => null }, { label: 'Apply', primary: true, value: () => read() }]);
    const body = $('#modal-body');
    body.oninput = live;
    body.onclick = e => {
      if (e.target.closest('[data-padd]')) { const tb = body.querySelector('tbody'); tb.insertAdjacentHTML('beforeend', rowHTML({ name: '', expr: '', note: '' }, tb.children.length)); tb.lastElementChild.querySelector('input').focus(); }
      const del = e.target.closest('[data-pdel]'); if (del) { del.closest('tr').remove(); live(); }
    };
    live();
    const list = await result;
    body.oninput = body.onclick = null;
    if (!list) return;
    const missing = [...used].filter(n => (this.state.params || []).some(q => q.name === n) && !list.some(q => q.name === n));
    if (missing.length && !(await this.confirm(`${missing.join(', ')} ${missing.length > 1 ? 'are' : 'is'} used in the timeline. Remove ${missing.length > 1 ? 'them' : 'it'} anyway? Those steps will show an error until you fix them.`))) return this.editParams(list);
    try { await this.apply(await post('params/set', { params: list }, { msg: 'Rebuilding…' })); }
    catch (e) { this.toast(e.message, 'err'); return this.editParams(list); }
    await this.resolveSketchParams();
    const bad = this.state.features.filter(f => f.error && /^Parameter:/.test(f.error));
    if (bad.length) this.toast(`${bad.map(f => f.name).join(', ')}: ${bad[0].error}`, 'warn');
  }

  /** Sketch dimensions set by parameters: work the sketch out again with the new values. */
  async resolveSketchParams() {
    const vals = this.paramValues();
    let n = 0;
    for (const f of this.state.features) {
      const cons = f.type === 'sketch' ? f.params?.constraints || [] : [];
      if (!cons.some(c => c.expr)) continue;
      let changed = false;
      const next = cons.map(c => {
        if (!c.expr || c.ref) return c;
        try { const v = resolveDeep(c.expr, vals); if (Number.isFinite(v) && v > 0 && Math.abs(v - c.value) > 1e-9) { changed = true; return { ...c, value: v }; } } catch { /* the step shows the error */ }
        return c;
      });
      if (!changed) continue;
      const fixed = [...SKETCH_BASE_REFS(), ...(f.params.refs || []).filter(r => r && !Array.isArray(r))];
      const r = solve(f.params.entities || [], next, [], fixed);
      if (!r.ok) { this.toast(`${f.name}: the new value conflicts with the sketch's other dimensions and relations.`, 'warn'); continue; }
      try { await post('feature/edit', { id: f.id, params: { ...f.params, entities: r.entities, constraints: next } }, { msg: `Updating ${f.name}…` }); n++; }
      catch (e) { this.toast(e.message, 'err'); }
    }
    if (n) await this.refresh();
  }

  /** Open a web page in the browser (the desktop app hands it to Windows). */
  openUrl(url) { post('open_url', { url }, { quiet: true }).catch(e => this.toast(e.message, 'err')); }

  sfMaterial() { try { const m = localStorage.getItem('stle_sf_material'); return MATERIALS[m] ? m : 'pla'; } catch { return 'pla'; } }

  /** The load direction as a vector (the select's word, or a direction that turned with the part). */
  sfLoad() { const l = this.tool?.values._load; return Array.isArray(l) ? l : (LOADS[l] || null); }

  /** Support Fins' Rotate panel: Suggest, and how strong this pose is (for a load, if one is set). */
  sfPoseHTML(f) {
    const t = this.tool, feat = this._sfInfo?.features || {};
    if (!feat.orient) return '';
    const sg = t._suggest, st = t._strength;
    const fmt = c => {
      const bits = [`${Math.round(c.overArea)} mm² to support`, `${Math.round(c.height)} mm tall`];
      if (c.walls != null) bits.push(`${c.walls} fin${c.walls === 1 ? '' : 's'}`);
      if (c.volume) bits.push(`${(c.volume * MATERIALS[this.sfMaterial()][1] / 1000).toFixed(1)} g`);
      return bits.join(' · ') + (c.posture ? ` · ${{ strong: 'lying down', mixed: 'on a slant', weak: 'standing up' }[c.posture]}` : '');
    };
    const CONF = { high: 'A clear winner.', medium: 'A good pick, though the next one is close.', low: 'Several poses are about as good: choose by how the part is loaded.', none: 'No pose sits flat on the plate: every way it balances on a point.' };
    let list = '';
    if (sg?.busy) list = '<div class="sf-note busy">Trying the part every way down…</div>';
    else if (sg?.error) list = `<div class="sf-note bad">${esc(sg.error)}</div>`;
    else if (sg?.list) list = (CONF[sg.confidence] ? `<div class="sf-note">${CONF[sg.confidence]}</div>` : '')
      + sg.list.map((c, i) => `<div class="sf-cand"><span><b>${i ? i + 1 : 'Best'}</b> · ${esc(fmt(c))}</span><button data-sfact="use" data-i="${i}">Use</button></div>`).join('');
    const posture = st?.posture ? `<div class="sf-note" title="How the print's layers sit for this pose. Prints are weakest across their layers.">${POSTURE[st.posture]}</div>` : '';
    const loadSel = Array.isArray(t.values._load) ? 'turned' : (t.values._load || 'none');
    const loadOpts = [['none', 'Not set'], ['up', 'Pulled up'], ['down', 'Pushed down'], ['left', 'Pulled left'], ['right', 'Pulled right'], ['front', 'Pulled to the front'], ['back', 'Pulled to the back'], ...(loadSel === 'turned' ? [['turned', 'As set (turned with the part)']] : [])];
    const verdict = st?.load ? `<div class="sf-pose"><div class="sf-verdict ${st.load.quality}">${esc(st.load.text)}</div></div>` : '';
    return `<div class="sf-pose" data-key="${f.key}"><div class="sf-btns"><button data-sfact="suggest" ${sg?.busy ? 'disabled' : ''}>Suggest orientations</button>`
      + `${st?.strongest ? '<button data-sfact="strongest">Turn to strongest pose</button>' : ''}</div>${list}${posture}</div>
      <div class="field" data-key="_load"><label title="Which way the part is pulled or pushed when it's in use. The strength line then says how well this pose takes it.">Load in use <span class="help">?</span></label><div class="ctl"><select data-sel="_load">${loadOpts.map(([k, l]) => `<option value="${k}" ${k === loadSel ? 'selected' : ''}>${l}</option>`).join('')}</select></div></div>${verdict}`;
  }

  /** Support Fins' two readouts side by side: how much of the overhangs is held, and the plastic. */
  sfStatusHTML(f) {
    const t = this.tool; if (!t) return '';
    const hasBody = (t.values.body || []).length > 0;
    const card = (key, cap, val, sub, tip) => `<div class="sf-card" data-key="${key}"${tip ? ` title="${esc(tip)}"` : ''}><div class="cap">${cap}</div><div class="val">${val}</div>${sub ? `<div class="sub">${sub}</div>` : ''}</div>`;
    // the plastic the supports add
    const sp = t._support, mat = this.sfMaterial();
    let pv = '<span class="muted">—</span>', ps = '';
    if (hasBody && sp && !sp.pieces) pv = '<span class="muted">None needed</span>';
    else if (hasBody && sp) {
      const g = sp.volume * MATERIALS[mat][1] / 1000;
      pv = `<b>${g < 9.95 ? g.toFixed(1) : Math.round(g)} g</b>`;
      ps = `${MATERIALS[mat][0]} · ${(sp.volume / 1000).toFixed(1)} cm³ · ${sp.pieces} piece${sp.pieces === 1 ? '' : 's'}`;
    }
    const plastic = card('_plastic', 'Support plastic', pv, ps, 'The plastic the supports add, from their exact volume. Set your filament under Print check › Material.');
    if (!f.cover) return `<div class="sf-status" data-key="${f.key}"><div class="sf-cards">${plastic}</div></div>`;
    // which overhangs the fins (or the part itself) hold up
    const c = t._cover, reach = this.sfReach();
    let cv, cs = '', note = '';
    if (!hasBody || c?.none) cv = '<span class="muted">—</span>';
    else if (!c || c.busy) cv = '<span class="muted busy">Checking…</span>';
    else if (c.error) cv = `<span class="bad">${esc(c.error)}</span>`;
    else if (!(c.total > 0)) { cv = '<span class="muted">Nothing to hold up</span>'; if ((t.values._mode || 'rotate') === 'rotate') cs = 'Turn it to the way you’ll print it'; }
    else if (!c.spots.length) cv = '<b class="sf-ok">All held</b>';
    else {
      const w = c.spots[0], n = c.spots.length, pct = Math.min(99, Math.floor(100 * c.held / c.total));
      cv = `<b class="sf-bad">${pct}% held</b>`;
      cs = `${n} bare spot${n > 1 ? 's' : ''}`;
      const worst = w.far == null ? `Nothing holds the worst bare spot (bright red) up.` : `The worst bare spot (bright red) is ${w.far < 9.95 ? w.far.toFixed(1) : Math.round(w.far)} mm from a support.`;
      const fix = t.values['sf:mode'] === 'full' ? 'Try more Wide-face coverage, or Add fins there.' : 'Try Placement › Full coverage, more Wide-face coverage, or Add fins there.';
      note = `<div class="sf-note sf-cover-note">${worst} ${fix}</div>`;
    }
    const held = card('_cover', 'Held', cv, cs, `An overhang within ${num(reach)} mm of a fin, or of the part itself, is held. Further than that it would sag or fail: it shows bright red. Change the distance under Print check › Bare beyond.`);
    return `<div class="sf-status" data-key="${f.key}"><div class="sf-cards">${held}${plastic}</div>${note}</div>`;
  }

  async sfSuggest() {
    const t = this.tool; if (!t) return;
    const body = t.values.body?.[0]; if (!body) { this.toolMsg('Pick the part first.', 'warn'); return; }
    // the poses are worked out for the part as it sits now: once it's turned (or still being
    // turned) they don't apply, and are dropped
    const gen = t._poseGen || 0, posing = !!t._posing;
    const stale = () => this.tool !== t || posing || t._posing || (t._poseGen || 0) !== gen || t.values.body?.[0] !== body;
    const sg = t._suggest = { busy: true }; this.renderForm();
    try {
      const r = await post('supportfins/suggest', { body, options: this.params().options || {}, top: 3 }, { quiet: true });
      if (this.tool !== t || t._suggest !== sg) return;
      t._suggest = stale() ? null : { list: r.candidates, confidence: r.confidence };
    } catch (e) { if (this.tool !== t || t._suggest !== sg) return; t._suggest = stale() ? null : { error: e.message }; }
    this.renderForm();
  }

  async sfStrength() {
    const t = this.tool; if (!t || t.def.type !== 'supportfins' || !this._sfInfo?.features?.orient) return;
    const body = t.values.body?.[0]; if (!body) return;
    const seq = (this._sfStrSeq = (this._sfStrSeq || 0) + 1);
    try {
      const r = await post('supportfins/strength', { body, options: this.params().options || {}, load: this.sfLoad() }, { quiet: true });
      if (this.tool !== t || seq !== this._sfStrSeq) return;
      t._strength = r; this.renderForm();
    } catch { /* the readout is optional */ }
  }

  /** Turn the part by a Support Fins pose (`rot`: column-major 3x3, applied to the part as it sits). */
  sfUsePose(rot) {
    const t = this.tool; const list = this.gizmo.bodies(t);
    if (!list.length) return;
    const [, o, b] = list[0];
    const q = new THREE.Quaternion().setFromRotationMatrix(new THREE.Matrix4().setFromMatrix3(new THREE.Matrix3().fromArray(rot)));
    const m = this.gizmo.matrixFor(o, b, { translate: [0, 0, 0], rotate: rotOf(q), pivot: 'center', bed: true });
    if (t.values._mode !== 'rotate') t.values._mode = 'rotate';
    this.poseDrop(m);
  }

  clearPreviewTimer() { clearTimeout(this._previewTimer); this._previewSeq++; }

  /** The numbers in the panel while a handle is dragged (the form is rebuilt on release). */
  liveNumbers(v) {
    const put = (sel, x) => { const el = document.querySelector(sel); if (el && document.activeElement !== el) el.value = x; };
    if (this.tool?.def.gizmo === 'scale') {
      put('[data-num="factor"]', num(v._k[0]));
      v._k.forEach((k, i) => put(`[data-vec="factor3"][data-i="${i}"]`, num(k)));
      return;
    }
    (v.translate || []).forEach((x, i) => put(`[data-vec="translate"][data-i="${i}"]`, U.toDisp(x)));
    (v.rotate || []).forEach((x, i) => put(`[data-vec="rotate"][data-i="${i}"]`, num(x)));
  }

  toolMsg(m, kind = 'info') { $('#tool-msg').innerHTML = m ? `<div class="msg ${kind}">${esc(m)}</div>` : ''; }

  async commitTool() {
    const t = this.tool; if (!t) return;
    if (!this.validTool()) { this.toolMsg('Fill in the highlighted selections first.', 'err'); return; }
    try {
      const params = this.params();
      let res;
      if (t.editId) {
        res = await post('feature/edit', { id: t.editId, params, rollback_after: t.prevRollback ?? null }, { msg: `Updating ${t.def.label}…` });
      } else {
        const name = t.def.type === 'plane' && t.values.name ? t.values.name : undefined;
        res = (await this.addFeatureSmart(t.def.type, params, { name, msg: `${t.def.label}…` })).res;
        if (res.result?.warning) this.toast(res.result.warning, 'warn');
      }
      await this.closeTool(true);
      await this.apply(res);
    } catch (e) { this.toolMsg(e.message, 'err'); }
  }

  renderOverlayForTool() {
    const t = this.tool;
    this.viewer.clearOverlay('marker'); this.viewer.clearOverlay('plane-ghost');
    if (!t) return;
    const v = t.values;
    let needPlanes = false;
    for (const f of t.def.fields) {
      if (f.show && !f.show(v)) continue;
      if (f.kind === 'plane') {
        if (t.activeField === f.key) needPlanes = true;
        const fr = this.frameOf(v[f.key]);
        if (fr && !t.def.planeTool) this.viewer.addPlaneGhost(fr);
      }
      if (f.kind === 'facepoints') for (const p of v[f.key] || []) this.viewer.addMarker(p, 0x1f6fd1);
      if (f.kind === 'points3') for (const p of v[f.key] || []) this.viewer.addMarker(p, 0xd9480f);
    }
    this.viewer.setBasePlanesVisible(needPlanes || this.viewer.baseVisibleUser);
    this.drawRegionOverlay();
    if (t.def.sculpt) this.viewer.drawStrokes(v.strokes || []);
    this.walls.redraw();
    if (t.def.gizmo) this.gizmo.sync();
  }

  async sketchFaces(sk) {
    const key = JSON.stringify(sk.entities);
    if (this._facesKey !== key) { this._facesKey = key; this._faces = (await post('sketch_op', { op: 'faces', entities: sk.entities }, { quiet: true })).faces; }
    return this._faces;
  }

  async drawRegionOverlay() {
    const t = this.tool; this.viewer.clearOverlay('regions'); this.viewer.clearOverlay('sk');
    if (!t) return;
    const f = t.def.fields.find(x => x.kind === 'regions'); if (!f) return;
    const sk = this.state.sketches.find(x => x.id === t.values.sketch); if (!sk) return;
    const g = this.viewer.sketchLines(sk.frame, sk.entities, 0xd9480f, 0.9); g.userData.tag = 'sk'; this.viewer.overlay.add(g);
    const pts = t.values.regions || [];
    if (pts.length) {
      const faces = await this.sketchFaces(sk);
      const picked = faces.filter(fc => pts.some(q => inPoly(q, fc)));
      this.viewer.addRegions(sk.frame, picked, 'regions');
    }
    this.viewer.requestRender();
  }

  frameOf(ref) {
    if (!ref) return null;
    const base = { XY: [[0, 0, 0], [1, 0, 0], [0, 1, 0], [0, 0, 1]], XZ: [[0, 0, 0], [1, 0, 0], [0, 0, 1], [0, -1, 0]], YZ: [[0, 0, 0], [0, 1, 0], [0, 0, 1], [1, 0, 0]] };
    if (typeof ref === 'string') return base[ref] || this.state.planes.find(p => p.id === ref)?.frame || null;
    if (ref.origin) { const n = ref.normal, x = ref.xdir, y = cross(n, x); return [ref.origin, x, y, n]; }
    return null;
  }

  // ----- selection input from the viewport / tree
  async feedPick(hit, ev) {
    const t = this.tool; if (!t) return false;
    let key = t.activeField;
    let f = t.def.fields.find(x => x.key === key);
    // primitives: clicking a face places the shape there
    if (!f && t.def.placeOnFace && hit?.kind === 'body') { key = 'plane'; f = t.def.fields.find(x => x.key === 'plane'); }
    if (!f && t.def.placeOnFace && hit?.kind === 'plane') { key = 'plane'; f = t.def.fields.find(x => x.key === 'plane'); }
    if (!f) return false;
    const v = t.values;
    if (f.kind === 'bodies') {
      if (hit?.kind !== 'body') return true;
      let cur = Array.isArray(v[key]) ? [...v[key]] : [];
      if (f.multi) cur = cur.includes(hit.id) ? cur.filter(x => x !== hit.id) : [...cur, hit.id];
      else cur = [hit.id];
      v[key] = cur;
      if (!f.multi) this.advanceField(key);
      if ((t.key === 'supportfins' || t.key === 'supportwall') && !t.editId && this.reopenSupports(t.key, hit.id)) return true;
      if (t.def.paint) this.quickPaint();
    } else if (f.kind === 'plane') {
      if (!hit) return true;
      if (hit.kind === 'plane') v[key] = hit.id;
      else {
        const face = await post('face', { body: hit.id, tri: hit.tri, point: hit.point, normal: hit.normal }, { quiet: true });
        v[key] = { origin: face.origin, normal: face.normal, xdir: face.xdir, label: `Face of ${this.bodyName(hit.id)}`, body: hit.id };
        if (t.def.placeOnFace) { const o = face.origin, x = face.xdir, y = face.ydir; const d = sub(hit.point, o); v.offset = [+dot(d, x).toFixed(3), +dot(d, y).toFixed(3), 0]; }
        this.viewer.clearOverlay('face');
        this.viewer.highlightFace(hit.id, face.tris);
      }
      this.advanceField(key);
    } else if (f.kind === 'cylface') {
      if (hit?.kind !== 'body') return true;
      let cyl;
      try { cyl = await post('cylinder', { body: hit.id, tri: hit.tri }, { quiet: true }); }
      catch (e) { this.toolMsg(e.message, 'err'); return true; }
      const what = cyl.kind === 'pin' ? 'Pin' : 'Hole';
      v[key] = { label: `${what} Ø${U.str(cyl.diameter)} × ${U.str(cyl.length)} deep` };
      v.body = hit.id; v.point = hit.point.map(x => +x.toFixed(4)); v.measured = cyl.diameter; v.kind = cyl.kind;
      if (!v._sized) { v.diameter = cyl.diameter; v.size = 'custom'; }
      this.viewer.clearOverlay('face');
      this.viewer.highlightFace(hit.id, cyl.tris);
      this.toolMsg(`Measured ${what.toLowerCase()} Ø${U.str(cyl.diameter)}. Type the new diameter${cyl.kind === 'hole' ? ' or choose Fill' : ''}.`, 'info');
      this.advanceField(key);
    } else if (f.kind === 'pface') {
      if (hit?.kind !== 'body') return true;
      const face = await post('face', { body: hit.id, tri: hit.tri, outline: true }, { quiet: true });
      if (!face.planar && !f.anyFace) { this.toolMsg('That face isn\'t flat. Pick a flat face.', 'err'); return true; }
      v[key] = face.planar ? { origin: face.origin, normal: face.normal, xdir: face.xdir, contours: face.contours, label: `Face of ${this.bodyName(hit.id)}` }
        : { origin: hit.point, normal: hit.normal, contours: [], label: `Point on ${this.bodyName(hit.id)}` };
      v.body = hit.id;
      this.viewer.clearOverlay('face');
      this.viewer.highlightFace(hit.id, face.tris);
      this.advanceField(key);
    } else if (f.kind === 'facepoints') {
      if (hit?.kind !== 'body') return true;
      v[key] = [...(v[key] || []), hit.point.map(x => +x.toFixed(4))];
      if (!v.normal) {
        const face = await post('face', { body: hit.id, tri: hit.tri }, { quiet: true });
        v.normal = face.planar ? face.normal : hit.normal;
      }
      if (!v.targets?.length) v.targets = [];
    } else if (f.kind === 'points3') {
      if (!hit) return true;
      const pts = [...(v[key] || [])];
      if (pts.length >= (f.count || 3)) pts.length = 0;
      pts.push(hit.point.map(x => +x.toFixed(4)));
      v[key] = pts;
      // a tool with a body field takes the body the points are on
      if (hit.kind === 'body' && t.def.fields.some(x => x.key === 'body' && x.kind === 'bodies') && isEmptySel(v.body)) v.body = [hit.id];
    }
    this.renderForm();
    this.schedulePreview(60);
    return true;
  }

  advanceField(after) {
    const t = this.tool; const fields = t.def.fields.filter(f => isSelKind(f.kind) && (!f.show || f.show(t.values)));
    const i = fields.findIndex(f => f.key === after);
    const next = fields.slice(i + 1).find(f => isEmptySel(t.values[f.key]) && !f.placeholder && f.kind !== 'regions');
    t.activeField = next ? next.key : null;
  }

  // =================================================================== sketch flow
  async beginSketch(planeRef) {
    await this.closeTool();
    this.exitModes();
    if (!planeRef) {
      // ask for a plane: reuse the tool panel with one plane field
      this.tool = { key: 'sketch', def: { label: 'Sketch', type: 'sketch', fields: [{ key: 'plane', label: 'Plane', kind: 'plane' }], noLivePreview: true }, values: { plane: null }, activeField: 'plane', editId: null };
      $('#tool-title').textContent = 'Create Sketch';
      $('#tool-ok').textContent = 'Start';
      $('#tool-panel').classList.remove('hidden');
      this.renderForm();
      $('#tool-form').insertAdjacentHTML('beforeend', `<div class="msg info" style="margin-top:8px">Click a flat face of a body, a construction plane, or one of the origin planes (XY, XZ, YZ).</div>`);
      this.hint('Select a plane or flat face to sketch on');
      this.viewer.setBasePlanesVisible(true);
      this.tool.onPlane = async () => { const p = this.tool.values.plane; await this.closeTool(); this.enterSketch({ plane: p }); };
      this.updateRibbonState(); this.renderEmpty(); this.viewer.resize();
      return;
    }
    this.enterSketch({ plane: planeRef });
  }

  async enterSketch({ plane, feature }) {
    let frame, refs = [], entities = [], savedRefs = [];
    if (feature) {
      const s = this.state.sketches.find(x => x.id === feature.id);
      const idx = this.state.features.findIndex(f => f.id === feature.id);
      this._sketchPrevRollback = this.state.rollback;
      await this.apply(await post('rollback', { index: idx }));
      frame = s ? s.frame : this.frameOf(feature.params.plane);
      savedRefs = (feature.params.refs || []).filter(r => r && !Array.isArray(r));
      entities = feature.params.entities || [];
      plane = feature.params.plane;
    } else {
      frame = this.frameOf(plane);
    }
    if (!frame) { this.toast('Could not use that plane.', 'err'); return; }
    // the part's edges on (or through, or under) the sketch plane, to dimension from
    let refKind = null;
    try { const r = await post('sketch_op', { op: 'refs', frame, entities: [] }, { quiet: true }); refs = r.refs || []; refKind = r.kind; } catch { refs = []; }
    this.mode = 'sketch';
    this.selection = []; this.viewer.setSelection([]); this.viewer.setHover(null);
    this.sketcher.start({ frame, entities, refs, savedRefs, refKind, featureId: feature?.id || null, planeRef: plane, name: feature?.name, constraints: feature?.params?.constraints || [] });
    const lost = this.sketcher.refEnts.filter(e => e.missing).length;
    if (lost) this.toast(`${lost} part edge${lost > 1 ? 's' : ''} this sketch measured from ${lost > 1 ? 'have' : 'has'} changed (shown red). Its dimensions hold to where ${lost > 1 ? 'they were' : 'it was'}: delete them and dimension to the new edge${lost > 1 ? 's' : ''}.`, 'warn');
    this.viewer.setBasePlanesVisible(false);
    this.viewer.syncSketches(this.state.sketches, feature?.id);
    this.buildRibbon();
    this.openSketchPanel();
    this.renderTimeline();
    this.renderEmpty();
    this.hint(this.sketcher.toolHint());
  }

  openSketchPanel() {
    $('#tool-title').textContent = this.sketcher.featureId ? 'Edit Sketch' : 'Sketch';
    $('#tool-ok').textContent = 'Finish Sketch';
    $('#tool-panel').classList.remove('hidden');
    this.renderSketchPanel();
    this.viewer.resize();
  }

  onDefinition() {
    const el = $('#sk-def'); if (!el) return;
    const sk = this.sketcher;
    const t = sk.defText();
    el.className = t ? `sk-def ${sk.defState()}` : ''; el.textContent = t;
    sk.showStatus();
  }

  onSketchChanged(fromPanel) {
    this.updateRibbonState();
    if (!fromPanel) this.renderSketchPanel();
  }

  renderSketchPanel() {
    if (!this.sketcher.active) return;
    const sk = this.sketcher;
    const e = sk.selectedEntity();
    const dt = sk.defText();
    let h = dt ? `<div class="sk-def ${sk.defState()}" id="sk-def">${esc(dt)}</div>` : '<div id="sk-def"></div>';
    // the selected dimension: its value (or parameter), and whether it drives the size
    const sc = sk.selC && sk.constraints.find(c => c.id === sk.selC);
    if (sc && CTYPES[sc.type]?.dim) {
      const isAng = !!CTYPES[sc.type].angle;
      const val = sc.expr || (isAng ? String(Math.round(sc.value * 1e4) / 1e4) : U.fmt(sc.value, 4));
      h += `<div class="section-label">${esc(CTYPES[sc.type].label)}</div>
        <div class="field"><label>Value</label><div class="ctl"><input type="text" data-skdim="value" value="${esc(sc.ref ? sk.dimText(sc, undefined, true) : val)}" ${sc.ref ? 'disabled' : ''} title="A number, or a parameter: width, =width/2"><span class="unit">${isAng ? '°' : U.label}</span></div></div>
        <div class="field"><label title="A reference (driven) dimension shows a size without setting it">Reference</label><div class="ctl"><input type="checkbox" data-skdim="ref" ${sc.ref ? 'checked' : ''}><button data-sk-act="del-dim">Delete</button></div></div>`;
    }
    h += `<div class="section-label">Shapes</div>${sk.panelHTML()}`;
    const txt = sk.selectedText();
    if (txt) {
      h += `<div class="section-label">Text</div><div class="field"><label>Text</label><div class="ctl"><span style="flex:1;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">“${esc(txt.meta.text)}”</span><button data-sk-act="edit-text">Edit…</button></div></div>
        <div class="field"><label>Font</label><div class="ctl">${esc(txt.meta.font || '')} · ${U.str(txt.meta.size || 10)}</div></div>`;
    }
    if (e && !txt) {
      h += `<div class="section-label">${ENTITY_LABEL[e.type] || 'Shape'} · ${describe(e)}</div>`;
      const fields = {
        rect: [['x', 'Corner X'], ['y', 'Corner Y'], ['w', 'Width'], ['h', 'Height'], ['fillet', 'Corner radius'], ['angle', 'Rotation °']],
        circle: [['cx', 'Centre X'], ['cy', 'Centre Y'], ['d', 'Diameter']],
        polygon: [['cx', 'Centre X'], ['cy', 'Centre Y'], ['r', 'Radius'], ['n', 'Sides'], ['angle', 'Rotation °']],
        slot: [['x1', 'Start X'], ['y1', 'Start Y'], ['x2', 'End X'], ['y2', 'End Y'], ['w', 'Width']],
        line: [['p1x', 'Start X'], ['p1y', 'Start Y'], ['p2x', 'End X'], ['p2y', 'End Y'], ['len', 'Length']],
        arc: [['cx', 'Centre X'], ['cy', 'Centre Y'], ['r', 'Radius'], ['a0', 'Start angle °'], ['a1', 'End angle °']],
      }[e.type] || [];
      for (const [k, l] of fields) {
        const isAngle = ['angle', 'a0', 'a1', 'n'].includes(k);
        const val0 = k === 'd' ? e.r * 2 : k === 'w' && e.type === 'slot' ? e.r * 2
          : k === 'p1x' ? e.p1[0] : k === 'p1y' ? e.p1[1] : k === 'p2x' ? e.p2[0] : k === 'p2y' ? e.p2[1]
          : k === 'len' ? Math.hypot(e.p2[0] - e.p1[0], e.p2[1] - e.p1[1]) : (e[k] ?? 0);
        const val = isAngle ? val0 : U.toDisp(val0);
        h += `<div class="field"><label>${l}</label><div class="ctl"><input type="text" data-ent="${k}" value="${num(val)}"></div></div>`;
      }
      if (e.type === 'polyline' && (e.pts || []).length > 24) h += `<div class="field"><label>Points</label><div class="ctl">${e.pts.length}${e.projected ? ' (projected outline)' : ''} · use Offset, Trim or Explode to edit</div></div>`;
      else if (e.type === 'polyline') h += (e.pts || []).map((p, i) => `<div class="field vec"><label>Point ${i + 1}</label><div class="ctl"><input type="text" data-pt="${i}" data-i="0" value="${U.toDisp(p[0])}"><input type="text" data-pt="${i}" data-i="1" value="${U.toDisp(p[1])}"></div></div>`).join('');
      h += `<div class="field"><label>Construction</label><div class="ctl"><input type="checkbox" data-ent-bool="construction" ${e.construction ? 'checked' : ''}></div></div>`;
    }
    if (sk.constraints.length) {
      const desc = c => { const T = CTYPES[c.type] || { label: c.type }; return T.dim ? `${T.label}: ${c.ref ? `(${sk.dimText(c, undefined, true)})` : (T.angle ? Math.round(c.value * 100) / 100 + '°' : U.str(c.value))}${c.expr ? ` = ${c.expr.slice(1)}` : ''}` : T.label; };
      h += `<div class="section-label">Dimensions & relations (${sk.constraints.length})</div><div class="ent-list">${sk.constraints.map(c => `<div class="ent ${sk.selC === c.id ? 'sel' : ''}" data-cid="${c.id}"><span>${esc(desc(c))}${c.auto ? ' <span class="t">auto</span>' : ''}</span><span class="t" data-cdel="${c.id}" title="Remove" style="cursor:pointer">✕</span></div>`).join('')}</div>`;
    }
    h += `<div class="section-label">Snapping</div>
      <div class="field"><label>Grid snap</label><div class="ctl"><input type="checkbox" id="snap-on" ${sk.snapGrid ? 'checked' : ''}><select id="snap-step">${U.snapSteps().map(([l, v]) => `<option value="${v}" ${Math.abs(v - sk.snapStep) < 1e-9 ? 'selected' : ''}>${l}</option>`).join('')}</select><span class="unit">${U.label}</span></div></div>
      <div class="msg info" style="margin-top:8px">Shapes inside other shapes become holes; lines and arcs that close a loop make a shape too. Use Smart Dimension (D) to add sizes; double-click one to change it (a parameter name works too). Black shapes are fully defined, blue ones can still move; orange lines are the part's edges, there to dimension from. When extruding you can pick single regions under Profiles.</div>`;
    $('#tool-form').innerHTML = h;
    $('#tool-msg').innerHTML = '';
  }

  async sketchOffset() {
    const sk = this.sketcher;
    if (!sk.sel.size) { this.toast('Select the shapes to offset first (Select tool, Shift-click for more).', 'warn'); return; }
    const val = await this.prompt('Offset distance (mm, negative = inward)', '2');
    if (val == null) return;
    const d = parseNum(val);
    try {
      const r = await post('sketch_op', { op: 'offset', entities: sk.entities, ids: [...sk.sel], distance: d, seed: Date.now().toString(36) }, { quiet: true });
      sk.replaceEntities(r.entities);
    } catch (e) { this.toast(e.message, 'err'); }
  }

  async sketchText(opts = {}, group = null) {
    const sk = this.sketcher;
    if (!this._fonts) { try { this._fonts = await post('fonts', {}, { quiet: true }); } catch (e) { this.toast(e.message, 'err'); return; } }
    const F = this._fonts;
    const LOCAL = '__local__', FILE = '__file__';
    const o = { text: 'Text', font: this._lastFont || F.default, size: 10, bold: false, italic: false, align: 'left', angle: 0, x: 0, y: 0, spacing: 0, ...opts };
    const sel = (id, opts2, cur) => `<select id="${id}">${opts2.map(([v, l]) => `<option value="${esc(v)}" ${v === cur ? 'selected' : ''}>${esc(l)}</option>`).join('')}</select>`;
    const row = (l, c) => `<div class="field"><label>${l}</label><div class="ctl">${c}</div></div>`;
    const html = `<h3>${group ? 'Edit text' : 'Text'}</h3>
      <textarea id="tx-text" rows="2" style="width:100%;font:inherit;padding:6px;box-sizing:border-box">${esc(o.text)}</textarea>
      ${row('Font', sel('tx-font', [...F.fonts.map(f => [f, f]), ...(F.canAskLocal ? [[LOCAL, 'More: fonts on this computer…']] : []), ...(F.canAddFile ? [[FILE, 'Add a font file…']] : [])], o.font))}
      ${row('Height', `<input type="text" id="tx-size" value="${U.toDisp(o.size)}"><span class="unit">${U.label}</span>`)}
      ${row('Style', `<label class="check"><input type="checkbox" id="tx-bold" ${o.bold ? 'checked' : ''}> Bold</label><label class="check"><input type="checkbox" id="tx-italic" ${o.italic ? 'checked' : ''}> Italic</label>`)}
      ${row('Align', sel('tx-align', [['left', 'Left'], ['center', 'Centre'], ['right', 'Right']], o.align))}
      ${row('Letter spacing', `<input type="text" id="tx-spacing" value="${U.toDisp(o.spacing || 0)}"><span class="unit">${U.label}</span>`)}
      ${row('Position', `<input type="text" id="tx-x" value="${U.toDisp(o.x)}"><input type="text" id="tx-y" value="${U.toDisp(o.y)}"><span class="unit">${U.label}</span>`)}
      ${row('Rotation', `<input type="text" id="tx-angle" value="${o.angle}"><span class="unit">°</span>`)}
      <div class="msg info">Height is the font's full height (capitals come out a little smaller). Letters with holes, like O and A, keep their holes. Extrude the text to emboss it, or cut to engrave it.</div>`;
    const read = () => {
      const g = id => document.getElementById(id);
      return { text: g('tx-text').value, font: g('tx-font').value, size: U.parse(g('tx-size').value), bold: g('tx-bold').checked, italic: g('tx-italic').checked,
        align: g('tx-align').value, spacing: U.parse(g('tx-spacing').value) || 0, x: U.parse(g('tx-x').value) || 0, y: U.parse(g('tx-y').value) || 0, angle: parseFloat(g('tx-angle').value) || 0 };
    };
    setTimeout(() => {
      const fs = document.getElementById('tx-font');
      if (fs) fs.onchange = async () => {
        if (fs.value !== LOCAL && fs.value !== FILE) return;
        try {
          let pickFam;
          if (fs.value === LOCAL) { const fams = await requestLocalFonts(); pickFam = fams.includes('Arial') ? 'Arial' : null; }
          else { const added = await addFontFiles(); pickFam = added[0] || null; if (added.length) this.toast(`Added ${added.join(', ')}`); }
          this._fonts = await post('fonts', {}, { quiet: true });
          const F2 = this._fonts;
          fs.innerHTML = [...F2.fonts.map(f => [f, f]), ...(F2.canAskLocal ? [[LOCAL, 'More: fonts on this computer…']] : []), ...(F2.canAddFile ? [[FILE, 'Add a font file…']] : [])]
            .map(([v, l]) => `<option value="${esc(v)}">${esc(l)}</option>`).join('');
          fs.value = pickFam || F2.default;
        } catch (e) { this.toast(e.message, 'warn'); fs.value = this._fonts.default; }
      };
    }, 0);
    const v = await this.modal(html, [{ label: 'Cancel', value: () => null }, { label: group ? 'Update' : 'Add text', primary: true, value: read }]);
    if (!v) return;
    if (!v.text.trim()) { this.toast('Type some text first.', 'warn'); return; }
    if (!(v.size > 0)) { this.toast('Enter a text height.', 'warn'); return; }
    this._lastFont = v.font;
    try {
      const r = await this.sketchServerOp({ op: 'text', ...v, group });
      sk.setSel(r.added);
    } catch (e) { this.toast(e.message, 'err'); }
  }

  async sketchProject() {
    const sk = this.sketcher;
    const visible = this.state.bodies.filter(b => !b.hidden && b.solid);
    if (!visible.length) { this.toast('There are no solid bodies to project.', 'warn'); return; }
    const row = (l, c) => `<div class="field"><label>${l}</label><div class="ctl">${c}</div></div>`;
    const html = `<h3>Project outline</h3>
      <p style="margin:0 0 10px;color:var(--muted)">Adds the outline of the model to this sketch as shapes you can offset, trim or extrude.</p>
      ${row('Outline', `<select id="pj-mode"><option value="section">Where the sketch plane cuts the model</option><option value="silhouette">Shadow of the whole model on the plane</option></select>`)}
      ${row('Bodies', `<select id="pj-bodies"><option value="">All visible bodies</option>${visible.map(b => `<option value="${esc(b.id)}" ${b.id === sk.bodyId ? 'selected' : ''}>${esc(b.name)}</option>`).join('')}</select>`)}
      ${row('Offset', `<input type="text" id="pj-offset" value="0"><span class="unit">${U.label}</span>`)}
      ${row('Construction', `<label class="check"><input type="checkbox" id="pj-cons"> Reference only (not part of the profile)</label>`)}
      <div class="msg info">Offset grows (+) or shrinks (−) the outline, e.g. +0.3 for a lid that fits over the part. Round outlines come in as circles.</div>`;
    const v = await this.modal(html, [{ label: 'Cancel', value: () => null }, { label: 'Project', primary: true, value: () => {
      const g = id => document.getElementById(id);
      return { mode: g('pj-mode').value, body: g('pj-bodies').value, offset: U.parse(g('pj-offset').value) || 0, construction: g('pj-cons').checked };
    } }]);
    if (!v) return;
    try {
      const r = await this.sketchServerOp({ op: 'project', frame: sk.frameArr, mode: v.mode, bodies: v.body ? [v.body] : null, offset: v.offset, construction: v.construction });
      sk.setSel(r.added);
      this.toast(`Added ${r.added.length} outline${r.added.length === 1 ? '' : 's'}.`, 'ok');
    } catch (e) { this.toast(e.message, 'err'); }
  }

  async sketchServerOp(data) {
    const sk = this.sketcher;
    const r = await post('sketch_op', { ...data, entities: sk.entities, seed: Date.now().toString(36) }, { quiet: true });
    sk.replaceEntities(r.entities);
    return r;
  }

  async finishSketch(thenExtrude) {
    const sk = this.sketcher;
    const params = { plane: sk.planeRef, entities: sk.entities, refs: sk.usedRefs(), constraints: sk.constraints };
    if (params.plane && typeof params.plane === 'object') params.plane = { ...params.plane };
    try {
      let sid = sk.featureId;
      if (sid) {
        await post('feature/edit', { id: sid, params, rollback_after: this._sketchPrevRollback ?? null }, { msg: 'Updating sketch…' });
      } else {
        if (!sk.entities.length) { this.toast('The sketch is empty.', 'warn'); }
        const r = await post('feature/add', { type: 'sketch', params });
        sid = r.feature.id;
      }
      sk.stop();
      this.mode = 'model';
      $('#tool-panel').classList.add('hidden');
      this.buildRibbon();
      await this.refresh();
      this.hint('');
      const hasProfile = sk.entities.some(e => !e.construction && (e.type !== 'polyline' || e.closed));
      if (thenExtrude && hasProfile) this.openTool('extrude', { values: { sketch: sid } });
    } catch (e) { this.toast(e.message, 'err'); }
  }

  async cancelSketch() {
    const sk = this.sketcher;
    const wasEdit = sk.featureId;
    sk.stop();
    this.mode = 'model';
    $('#tool-panel').classList.add('hidden');
    this.buildRibbon();
    if (wasEdit) await this.apply(await post('rollback', { index: this._sketchPrevRollback ?? null }));
    else await this.refresh();
    this.hint('');
    this.viewer.resize();
  }

  // =================================================================== inspect modes
  exitModes() {
    if (this.mode === 'measure' || this.mode === 'info' || this.mode === 'section') {
      // leaving Section keeps the cut: it stays on (X or the view bar turns it off) until removed
      this.viewer.clearOverlay();
      $('#tool-panel').classList.add('hidden');
    }
    if (this.mode !== 'sketch') this.mode = 'model';
    this.hint('');
  }

  async startMeasure() {
    await this.closeTool(); this.exitModes();
    this.mode = 'measure'; this.measurePts = [];
    $('#tool-title').textContent = 'Measure'; $('#tool-ok').textContent = 'Done';
    $('#tool-panel').classList.remove('hidden');
    $('#tool-form').innerHTML = `<div class="msg info">Click two points on the model.</div><div id="measure-out"></div>`;
    this.hint('Click two points to measure the distance between them');
    this.updateRibbonState(); this.viewer.resize();
  }

  measureClick(hit) {
    if (!hit) return;
    if (this.measurePts.length >= 2) { this.measurePts = []; this.viewer.clearOverlay(); }
    this.measurePts.push(hit.point);
    this.viewer.addMarker(hit.point, 0x1f6fd1);
    if (this.measurePts.length === 2) {
      const [a, b] = this.measurePts, d = sub(b, a), L = Math.hypot(...d);
      this.viewer.addLine(a, b);
      $('#measure-out').innerHTML = `<div class="field"><label>Distance</label><div class="ctl"><b>${U.str(L)}</b></div></div>
        <div class="field"><label>ΔX</label><div class="ctl">${U.str(Math.abs(d[0]))}</div></div>
        <div class="field"><label>ΔY</label><div class="ctl">${U.str(Math.abs(d[1]))}</div></div>
        <div class="field"><label>ΔZ</label><div class="ctl">${U.str(Math.abs(d[2]))}</div></div>`;
    } else $('#measure-out').innerHTML = `<div class="field"><label>Point 1</label><div class="ctl">${hit.point.map(v => U.fmt(v)).join(', ')} ${U.label}</div></div>`;
  }

  async startSection() {
    await this.closeTool(); this.exitModes();
    this.mode = 'section';
    // Cancel puts back the cut as it was (the same plane object: no re-working-out of the kept half)
    this._sectionUndo = { state: this.sectionState && { ...this.sectionState }, on: this.sectionOn, plane: this.viewer.sectionPlane };
    // an existing cut is edited where it is (the slider's range follows the parts now in the scene);
    // a new one starts halfway up the parts
    const box = this.viewer.sceneBox();
    const lo = box.isEmpty() ? [-50, -50, -50] : box.min.toArray(), hi = box.isEmpty() ? [50, 50, 50] : box.max.toArray();
    if (!this.sectionState) this.sectionState = { axis: 'z', flip: false, lo, hi, t: 0.5 };
    else {
      const s = this.sectionState, i = 'xyz'.indexOf(s.axis), v = this.sectionValue();
      if (hi[i] > lo[i]) this.sectionState = { ...s, lo, hi, t: Math.min(1, Math.max(0, (v - lo[i]) / (hi[i] - lo[i]))) };
    }
    $('#tool-title').textContent = 'Section View'; $('#tool-ok').textContent = 'Done';
    $('#tool-panel').classList.remove('hidden');
    this.renderSection();
    this.hint('Drag the slider to look inside · only the view changes · the cut stays when you close this; X turns it off and on');
    this.updateRibbonState(); this.viewer.resize();
  }

  sectionValue() { const s = this.sectionState, i = 'xyz'.indexOf(s.axis); return s.lo[i] + (s.hi[i] - s.lo[i]) * s.t; }

  renderSection() {
    const s = this.sectionState;
    $('#tool-form').innerHTML = `<div class="field"><label>Axis</label><div class="ctl"><div class="seg">${['x', 'y', 'z'].map(a => `<button data-sec-axis="${a}" class="${a === s.axis ? 'on' : ''}">${a.toUpperCase()}</button>`).join('')}</div></div></div>
      <div class="field"><label>Position</label><div class="ctl"><input type="range" id="sec-pos" min="0" max="1000" value="${s.t * 1000}" style="width:100%"></div></div>
      <div class="field"><label></label><div class="ctl">${U.str(this.sectionValue())}</div></div>
      <div class="field"><label>Flip</label><div class="ctl"><input type="checkbox" id="sec-flip" ${s.flip ? 'checked' : ''}></div></div>
      <div class="msg info">The cut only changes what you see: export, Properties and Measure use the whole part. It stays on after you click Done; turn it off and on with <b>Section</b> in the view bar or the <b>X</b> key.</div>
      <div class="field"><label></label><div class="ctl"><button data-sec-clear>Remove cut</button></div></div>`;
    this.applyCut();
  }

  /** Shows the current cut (moving it turns it on) and updates the view bar's Section button. */
  applyCut() {
    const s = this.sectionState;
    if (s) { this.sectionOn = true; this.viewer.setSection(s.axis, this.sectionValue(), s.flip); }
    this.viewer.setSectionEnabled(!!s && this.sectionOn);
    this.updateSectionButton();
  }

  /** The view bar's Section button / X: no cut yet opens Section; otherwise turns the cut off and on. */
  toggleCut() {
    if (!this.sectionState) return this.startSection();
    this.sectionOn = !this.sectionOn;
    this.viewer.setSectionEnabled(this.sectionOn);
    this.updateSectionButton();
    this.hint(this.sectionOn ? 'Section view on (X turns it off)' : 'Section view off (X turns it back on)');
  }

  /** Removes the cut altogether (Remove cut, or a new / opened project). */
  clearCut() {
    this.sectionState = null; this.sectionOn = false;
    this.viewer.setSection(null);
    this.updateSectionButton();
    if (this.mode === 'section') { this.exitModes(); this.updateRibbonState(); this.viewer.resize(); }
  }

  updateSectionButton() {
    const b = $('#btn-section'); if (!b) return;
    const on = !!this.sectionState && this.sectionOn;
    b.classList.toggle('on', on);
    b.setAttribute('aria-pressed', on ? 'true' : 'false');
    b.title = !this.sectionState ? 'Section view: cut the view open to look inside (X)' : on ? 'Section view is on: click to turn it off (X)' : 'Section view is off: click to turn it back on (X)';
  }

  async startInfo(bid) {
    await this.closeTool(); this.exitModes();
    this.mode = 'info';
    $('#tool-title').textContent = 'Properties'; $('#tool-ok').textContent = 'Close';
    $('#tool-panel').classList.remove('hidden');
    this.updateRibbonState(); this.viewer.resize();
    const id = bid || this.selection[0];
    if (!id) { $('#tool-form').innerHTML = '<div class="msg info">Click a body to see its size, volume and triangle count.</div>'; this.hint('Click a body'); return; }
    this.showInfo(id);
  }

  async showInfo(id) {
    try {
      const { info: i } = await post('info', { id }, { quiet: true });
      const row = (l, v) => `<div class="field"><label>${l}</label><div class="ctl">${v}</div></div>`;
      const sz = i.size.map(num).join(' × ');
      let h = `<div class="section-label">${esc(i.name)}</div>` + row('Watertight', i.solid ? '✔ Yes' : '<span style="color:var(--warn)">No — run Repair</span>') +
        row('Size (X×Y×Z)', `${i.size.map(v => U.fmt(v, 3)).join(' × ')} ${U.label}`) + row('Triangles', i.triangles.toLocaleString()) + row('Surface area', U.mode === 'in' ? `${num(i.area / 645.16)} in²` : `${num(i.area / 100)} cm²`);
      if (i.volume != null) {
        const cm3 = i.volume / 1000;
        h += row('Volume', U.mode === 'in' ? `${num(i.volume / 16387.064)} in³` : `${num(cm3)} cm³`) + row('Weight (PLA)', `${num(cm3 * 1.24)} g`) + row('Weight (PETG)', `${num(cm3 * 1.27)} g`) + (i.genus >= 0 ? row('Through-holes', i.genus) : '');
      }
      h += row('Bounds min', i.bbox[0].map(v => U.fmt(v)).join(', ')) + row('Bounds max', i.bbox[1].map(v => U.fmt(v)).join(', '));
      $('#tool-form').innerHTML = h;
    } catch (e) { this.toast(e.message, 'err'); }
  }

  // =================================================================== commands
  async cmd(name, data = {}) {
    try {
      if (name === 'new' || name === 'open') {
        let r = await post(name, data);
        if (r.confirm) { if (!(await this.confirm(r.confirm))) return; r = await post(name, { ...data, force: true }); }
        if (r.cancelled) return;
        await this.closeTool(); if (this.sketcher.active) this.sketcher.stop();
        this.clearCut(); this.selection = []; this.mode = 'model'; this.buildRibbon();
        await this.apply(r);
        this.viewer.setView('iso');
        return;
      }
      if (name === 'import') {
        const r = await post('import', data, { msg: 'Importing…' });
        if (r.cancelled) return;
        await this.apply(r);
        const last = r.state.features.at(-1);
        if (last?.warning) this.toast(last.warning, 'warn');
        this.viewer.fit();
        return;
      }
      if (name === 'save' || name === 'save_as') {
        const r = await post('save', { save_as: name === 'save_as' });
        if (!r.cancelled) { await this.apply(r); this.toast(`Saved ${r.path.split(/[\\/]/).pop()}`); }
        return;
      }
      if (name === 'undo' || name === 'redo') { if (this.tool) await this.closeTool(); await this.apply(await post(name)); return; }
      if (name === 'quit') {
        let r = await post('quit', {});
        if (r.confirm && (await this.confirm(r.confirm))) await post('quit', { force: true });
      }
    } catch (e) { this.toast(e.message, 'err'); }
  }

  async exportBodies(format, bodies) {
    try {
      const r = await post('export', { format, bodies: bodies || (this.selection.length ? this.selection : null) }, { msg: 'Exporting…' });
      if (!r.cancelled) this.toast(`Exported ${r.count} bod${r.count > 1 ? 'ies' : 'y'} to ${r.path.split(/[\\/]/).pop()}`);
    } catch (e) { this.toast(e.message, 'err'); }
  }

  exportMenu(anchor) {
    const sel = this.selection.length;
    this.showMenu(anchor || $('[data-tool="export"]'), [
      { hdr: sel ? `Selected bodies (${sel})` : 'All visible bodies' },
      { label: 'STL (.stl)', fn: () => this.exportBodies('stl') },
      { label: '3MF (.3mf)', fn: () => this.exportBodies('3mf') },
      { label: 'OBJ (.obj)', fn: () => this.exportBodies('obj') },
    ]);
  }

  webWelcome() {
    let seen = false;
    try { seen = localStorage.getItem('stle.webWelcome') === '1'; } catch { /* storage off */ }
    const bar = document.createElement('div');
    bar.id = 'web-bar';
    bar.innerHTML = `<span><b>Web edition</b> · runs in your browser, your files stay on your computer.${seen ? '' : ' Send to Slicer and To STEP need the desktop app.'}</span>
      <a href="${DESKTOP_URL}" target="_blank" rel="noopener">Desktop app</a><button title="Hide" data-x>×</button>`;
    bar.querySelector('[data-x]').onclick = () => { bar.remove(); try { localStorage.setItem('stle.webWelcome', '1'); } catch { /* storage off */ } };
    document.getElementById('viewport').appendChild(bar);
    if (seen) setTimeout(() => bar.remove(), 6000);
  }

  /** On narrow screens the browser panel is a drawer, opened with a button over the model. */
  setupDrawer() {
    const b = document.createElement('button');
    b.id = 'browser-toggle'; b.textContent = 'Bodies';
    b.onclick = () => { document.documentElement.classList.toggle('browser-open'); b.textContent = document.documentElement.classList.contains('browser-open') ? 'Close' : 'Bodies'; };
    document.getElementById('viewport').appendChild(b);
  }

  /** Web edition as an app: offline copy (service worker), install, files opened with it. */
  setupWebApp() {
    const head = document.head;
    if (!head.querySelector('link[rel="manifest"]')) {
      const l = document.createElement('link'); l.rel = 'manifest'; l.href = 'manifest.webmanifest'; head.appendChild(l);
      const m = document.createElement('meta'); m.name = 'theme-color'; m.content = '#17191e'; head.appendChild(m);
    }
    window.addEventListener('beforeinstallprompt', e => { e.preventDefault(); this._install = e; });
    window.addEventListener('appinstalled', () => { this._install = null; this.toast('STL Editor is installed. Open it from your apps like any other program.'); });
    // not on a local test server (it would keep serving old files while you edit), unless ?sw
    const local = ['localhost', '127.0.0.1'].includes(location.hostname);
    if ('serviceWorker' in navigator && (local ? new URLSearchParams(location.search).has('sw') : location.protocol === 'https:')) {
      navigator.serviceWorker.register('sw.js').then(reg => {
        const offerUpdate = w => {
          if (!navigator.serviceWorker.controller) return;   // first install: nothing to replace
          w.addEventListener('statechange', () => { if (w.state === 'installed') this.updateReady(w); });
          if (w.state === 'installed') this.updateReady(w);
        };
        if (reg.waiting) this.updateReady(reg.waiting);
        reg.addEventListener('updatefound', () => reg.installing && offerUpdate(reg.installing));
      }).catch(() => { /* no offline copy: the app still works online */ });
      let reloading = false;
      navigator.serviceWorker.addEventListener('controllerchange', () => { if (this._reloadOnUpdate && !reloading) { reloading = true; location.reload(); } });
    }
    // files opened with the installed app ("Open with", double-click)
    if ('launchQueue' in window) {
      window.launchQueue.setConsumer(async params => {
        if (!params.files?.length) return;
        try {
          const r = await openLaunchFiles(params.files);
          await this.closeTool(); this.selection = []; this.clearCut(); await this.apply(r); this.viewer.setView('iso'); this.viewer.fit();
        } catch (e) { this.toast(e.message, 'err'); }
      });
    }
  }

  updateReady(worker) {
    if (this._updateShown) return;
    this._updateShown = true;
    const bar = document.createElement('div');
    bar.id = 'web-bar';
    bar.innerHTML = `<span><b>A new version of STL Editor is ready.</b> It starts next time you open it, or now if you reload.</span>
      <a href="#" data-reload>Reload now</a><button title="Later" data-x>×</button>`;
    bar.querySelector('[data-x]').onclick = () => bar.remove();
    bar.querySelector('[data-reload]').onclick = async e => {
      e.preventDefault();
      if (this.state.doc?.dirty && !(await this.confirm('Reload now? Save first if you want to keep your changes (an autosave copy is kept either way).'))) return;
      await flushAutosave();
      this._reloadOnUpdate = true;
      worker.postMessage('skip-waiting');
    };
    document.getElementById('web-bar')?.remove();
    document.getElementById('viewport').appendChild(bar);
  }

  desktopOnly(what = 'This tool') {
    this.alert('Desktop app tool', `${esc(what)} runs in the desktop app, which can open your slicer and SolidCraft directly.<br><br>
      Save your project (File › Save) and open it in the desktop app: everything carries over, timeline included.<br><br>
      <a href="${DESKTOP_URL}" target="_blank" rel="noopener">Get the desktop app (Windows, free)</a>`);
  }

  /** Off Windows, Send to Slicer is hidden when no slicer is installed and To STEP (SolidCraft is
   *  Windows-only) is hidden. On Windows both always show, as they always have. */
  deskToolShown(k) {
    const a = this.apps;
    if (!a.platform || a.platform === 'win32') return true;
    if (k === 'send') return !!(a.bambu || a.orca || a.prusa || a.solidcraft);
    if (k === 'to_step') return !!a.step_supported;
    return true;
  }

  /** The programs a model can be sent to (a menu, or the submenu of File › Send to Slicer). */
  slicerItems(bodies = null) {
    const go = app => post('send_to', { app, bodies: bodies || (this.selection.length ? this.selection : null) }, { msg: 'Sending…' }).then(() => this.toast('Opened')).catch(e => this.toast(e.message, 'err'));
    const items = [
      { hdr: bodies ? 'Send this body to' : (this.selection.length ? `Send ${this.selection.length} selected to` : 'Send all visible bodies to') },
      { label: 'Bambu Studio', ic: 'slicer', fn: () => go('bambu'), dis: !this.apps.bambu, title: this.apps.bambu ? '' : 'Bambu Studio wasn’t found on this computer' },
      { label: 'OrcaSlicer', ic: 'slicer', fn: () => go('orca'), dis: !this.apps.orca, title: this.apps.orca ? '' : 'OrcaSlicer wasn’t found on this computer' },
      ...(this.apps.prusa ? [{ label: 'PrusaSlicer', ic: 'slicer', fn: () => go('prusa') }] : []),
      { sep: true },
      { label: 'SolidCraft', fn: () => go('solidcraft'), dis: !this.apps.solidcraft, title: this.apps.solidcraft ? '' : 'SolidCraft wasn’t found on this computer' },
    ];
    if (!this.apps.bambu && !this.apps.orca && !this.apps.prusa && !this.apps.solidcraft) items.push({ sep: true }, { label: 'No slicer found: install Bambu Studio or OrcaSlicer', dis: true });
    return items;
  }

  sendMenu(anchor) {
    if (WEB) return this.desktopOnly('Send to Slicer');
    this.showMenu(anchor || $('[data-tool="send"]'), this.slicerItems());
  }

  async toStep() {
    const id = this.selection[0];
    if (!id) { this.toast('Select a body first.', 'warn'); return; }
    try {
      const r = await post('to_step', { body: id }, { msg: 'Converting with SolidCraft…' });
      if (!r.cancelled) this.toast(`Saved STEP: ${r.path.split(/[\\/]/).pop()}`);
    } catch (e) { this.toast(e.message, 'err'); }
  }

  async editFeature(fid) {
    const f = this.state.features.find(x => x.id === fid); if (!f) return;
    if (this.sketcher.active) return;
    if (f.type === 'sketch') { await this.closeTool(); return this.enterSketch({ feature: f }); }
    const key = toolKeyFor(f);
    if (!key) { this.renameFeature(f); return; }
    if (key === 'supportfins' || key === 'supportwall') return this.openSupportTool(key, { editFeature: f });
    this.openTool(key, { editFeature: f });
  }

  async renameFeature(f) {
    const name = await this.prompt('Rename', f.name);
    if (name) await this.apply(await post('feature/edit', { id: f.id, name }));
  }

  // =================================================================== UI wiring
  bindUI() {
    for (const [id, ic] of [['#btn-undo', 'undo'], ['#btn-redo', 'redo'], ['#btn-save', 'save'], ['#btn-insert', 'import'], ['#btn-export', 'export'], ['#tl-start', 'tl_start'], ['#tl-prev', 'tl_prev'], ['#tl-next', 'tl_next'], ['#tl-end', 'tl_end']]) $(id).innerHTML = icon(ic);
    $('#btn-undo').onclick = () => this.cmd('undo');
    $('#btn-redo').onclick = () => this.cmd('redo');
    $('#btn-save').onclick = () => this.cmd('save');
    $('#ribbon-tabs').onclick = e => { const t = e.target.closest('[data-tab]'); if (t && t.dataset.tab !== 'sketch') { this.tab = t.dataset.tab; this.saveTab(); this.buildRibbon(); } };
    $('#ribbon-body').onclick = e => {
      const d = e.target.closest('[data-drop]'); if (d) { e.stopPropagation(); return this.dropMenu(d.dataset.drop, d); }
      const m = e.target.closest('[data-more]'); if (m) { e.stopPropagation(); return this.moreMenu(m); }
      const gl = e.target.closest('[data-glabel]'); if (gl) { e.stopPropagation(); return this.groupMenu(+gl.dataset.glabel, gl); }
      const b = e.target.closest('[data-tool]');
      if (b && !b.disabled) { if (['export', 'send'].includes(b.dataset.tool)) e.stopPropagation(); this.onRibbon(b.dataset.tool); }
    };
    let fitRaf = 0;
    window.addEventListener('resize', () => { cancelAnimationFrame(fitRaf); fitRaf = requestAnimationFrame(() => this.fitRibbon()); });
    $('#btn-insert').onclick = () => this.cmd('import');
    // stopPropagation: otherwise the document's "click outside closes the menu" handler hides it at once
    $('#btn-export').onclick = e => { e.stopPropagation(); this.exportMenu($('#btn-export')); };
    for (const b of document.querySelectorAll('.menu-btn')) b.onclick = () => this.topMenu(b);
    // the tool panel's title shows the tool's toolbar icon (the title is set wherever a panel opens)
    new MutationObserver(() => { const ic = this.toolIcon(); $('#tool-icon').innerHTML = ic ? icon(ic) : ''; this.toolCredit(); })
      .observe($('#tool-title'), { childList: true, characterData: true, subtree: true });
    for (const b of document.querySelectorAll('#viewcube button[data-view]')) b.onclick = () => this.viewer.setView(b.dataset.view);
    $('#btn-section').onclick = () => this.toggleCut();
    this.updateSectionButton();
    $('#empty-open').onclick = () => this.cmd('import');
    $('#empty-box').onclick = () => this.openTool('primitive_box');
    $('#empty-recent').onclick = e => { const r = e.target.closest('[data-path]'); if (r) this.cmd('open', { path: r.dataset.path }); };

    // tool panel
    $('#tool-ok').onclick = () => this.panelOk();
    $('#tool-cancel').onclick = () => this.panelCancel();
    $('#tool-close').onclick = () => this.panelCancel();
    const form = $('#tool-form');
    form.addEventListener('click', e => this.onFormClick(e));
    form.addEventListener('change', e => this.onFormChange(e));
    form.addEventListener('input', e => this.onFormInput(e));
    form.addEventListener('keydown', e => {
      if (e.key === 'Enter' && e.target.tagName === 'INPUT') { e.preventDefault(); e.stopPropagation(); this.onFormChange(e); e.target.select?.(); }
      // Enter or Space on a section header folds it (and doesn't also press OK)
      else if ((e.key === 'Enter' || e.key === ' ') && e.target.closest?.('[data-fold]')) e.stopPropagation();
    });

    // tree
    $('#tree').addEventListener('click', e => this.onTreeClick(e));
    $('#tree').addEventListener('dblclick', e => { const n = e.target.closest('[data-sketch]'); if (n) this.editFeature(n.dataset.sketch); const b = e.target.closest('[data-body]'); if (b) this.renameBody(b.dataset.body); });
    $('#tree').addEventListener('contextmenu', e => { e.preventDefault(); const n = e.target.closest('[data-body]'); if (n) this.bodyMenu(n.dataset.body, e); const s = e.target.closest('[data-sketch],[data-plane]'); if (s) this.featureMenu(s.dataset.sketch || s.dataset.plane, e); });

    // timeline
    $('#timeline').addEventListener('dblclick', e => { const it = e.target.closest('[data-fid]'); if (it) this.editFeature(it.dataset.fid); });
    $('#timeline').addEventListener('click', e => {
      const it = e.target.closest('[data-fid]'); if (!it) return;
      const f = this.state.features.find(x => x.id === it.dataset.fid);
      const ff = this.tool?.def.fields.find(x => x.kind === 'features' && (!x.show || x.show(this.tool.values)));
      if (ff && repeatableFeatures(this.state).some(x => x.id === f.id)) {
        const cur = new Set(this.tool.values[ff.key] || []); cur.has(f.id) ? cur.delete(f.id) : cur.add(f.id);
        this.setValue(ff.key, [...cur]); return;
      }
      this.status(`${f.name}${f.error ? ' — ' + f.error : ''}`);
    });
    $('#timeline').addEventListener('contextmenu', e => { e.preventDefault(); const it = e.target.closest('[data-fid]'); if (it) this.featureMenu(it.dataset.fid, e); });
    const roll = async idx => { if (this.tool || this.sketcher.active) return; await this.apply(await post('rollback', { index: idx })); };
    $('#tl-start').onclick = () => roll(0);
    $('#tl-end').onclick = () => roll(null);
    $('#tl-prev').onclick = () => { const r = this.state.rollback ?? this.state.features.length; roll(Math.max(0, r - 1)); };
    $('#tl-next').onclick = () => { const r = this.state.rollback; if (r == null) return; roll(r + 1 >= this.state.features.length ? null : r + 1); };

    // viewport
    const c = this.viewer.canvas;
    // sculpt brush strokes (left button; orbit is on the right button while sculpting)
    let stroke = null;
    c.addEventListener('pointerdown', e => {
      const t = this.tool;
      if (!t?.def.sculpt || e.button !== 0) return;
      const hit = this.viewer.pick(e, { planes: false });
      if (hit?.kind !== 'body') return;
      if (t.values.body && t.values.body !== hit.id && (t.values.strokes || []).length) { this.toolMsg('Finish (OK) the strokes on the other body first.', 'warn'); return; }
      const r = Math.max(0.05, parseFloat(t.values.radius) || 5);
      stroke = { body: hit.id, mode: t.values.mode, radius: r, strength: Math.min(1, Math.max(0.01, parseFloat(t.values.strength) || 0.5)), points: [hit.point] };
      c.setPointerCapture(e.pointerId);
    });
    c.addEventListener('pointermove', e => {
      const t = this.tool;
      if (!t?.def.sculpt) return;
      const hit = this.viewer.pick(e, { planes: false });
      const r = Math.max(0.05, parseFloat(t.values.radius) || 5);
      this.viewer.setBrush(hit?.kind === 'body' ? { point: hit.point, normal: hit.normal, radius: r } : null);
      if (stroke && hit?.kind === 'body' && hit.id === stroke.body) {
        const last = stroke.points.at(-1);
        if (Math.hypot(...sub(hit.point, last)) > r / 5) { stroke.points.push(hit.point); this.viewer.drawStrokes([...(t.values.strokes || []), stroke]); }
      }
    });
    c.addEventListener('pointerup', e => {
      const t = this.tool;
      if (!stroke || !t?.def.sculpt) { stroke = null; return; }
      const st = stroke; stroke = null;
      t.values.body = st.body;
      const { body, ...clean } = st;
      clean.points = clean.points.map(p => p.map(x => +x.toFixed(3)));
      this.setValue('strokes', [...(t.values.strokes || []), clean]);
      this.schedulePreview(0);
    });
    let down = null;
    c.addEventListener('pointerdown', e => { down = { x: e.clientX, y: e.clientY, b: e.button }; });
    c.addEventListener('pointerup', async e => {
      if (this.gizmo.consumeClick()) { down = null; return; }
      if (!down) return;
      const moved = Math.hypot(e.clientX - down.x, e.clientY - down.y) > 5;
      const btn = down.b; down = null;
      if (moved || this.sketcher.active) return;
      if (btn === 2) { const hit = this.viewer.pick(e, { planes: false }); if (hit?.kind === 'body') this.bodyMenu(hit.id, e); else this.viewMenu(e); return; }
      if (btn !== 0) return;
      await this.onViewportClick(e);
    });
    c.addEventListener('contextmenu', e => e.preventDefault());
    let hoverRaf = null;
    c.addEventListener('pointermove', e => {
      if (this.sketcher.active || down) return;
      if (hoverRaf) return;
      hoverRaf = requestAnimationFrame(() => {
        hoverRaf = null;
        const hit = this.viewer.pick(e, { planes: !!this.tool && !this.walls.active });
        this.viewer.setHover(hit?.kind === 'body' && !this.walls.active ? hit.id : null);
        this.walls.hover(hit);
        if (this.tool?.def.type === 'supportfins' && this.tool.values._mode === 'remove') this.viewer.setPreviewHover(this.viewer.pickPreview(e));
        if (hit) this.status(hit.kind === 'body' ? `${this.bodyName(hit.id)}  ·  ${hit.point.map(v => U.fmt(v)).join(', ')} ${U.label}` : this.planeLabel(hit.id));
        else this.status('');
      });
    });
    c.addEventListener('dblclick', e => {
      if (this.sketcher.active || this.tool) return;
      const hit = this.viewer.pick(e, { planes: false });
      if (hit?.kind === 'body') this.quickSketchOnFace(hit);
    });

    // keyboard
    window.addEventListener('keydown', e => this.onKey(e));

    // drag & drop
    const vp = $('#viewport');
    let dragDepth = 0;
    vp.addEventListener('dragenter', e => { e.preventDefault(); dragDepth++; $('#drop-overlay').classList.remove('hidden'); });
    vp.addEventListener('dragleave', () => { if (--dragDepth <= 0) { dragDepth = 0; $('#drop-overlay').classList.add('hidden'); } });
    vp.addEventListener('dragover', e => e.preventDefault());
    vp.addEventListener('drop', async e => {
      e.preventDefault(); dragDepth = 0; $('#drop-overlay').classList.add('hidden');
      const files = [...e.dataTransfer.files];
      for (const f of files) {
        try {
          if (f.name.toLowerCase().endsWith('.stlx')) { this.toast('Use File › Open for projects.', 'warn'); continue; }
          const r = await upload(f); await this.apply(r);
          const last = r.state.features.at(-1); if (last?.warning) this.toast(last.warning, 'warn');
        } catch (err) { this.toast(`${f.name}: ${err.message}`, 'err'); }
      }
      if (files.length) this.viewer.fit();
    });
    // links that open in the browser, in the desktop app too
    document.addEventListener('click', e => { const a = e.target.closest('a[data-url]'); if (a) { e.preventDefault(); this.openUrl(a.dataset.url); } });
    document.addEventListener('click', e => { if (!e.target.closest('#menu') && !e.target.closest('.menu-btn') && !e.target.closest('#tool-search')) this.hideMenu(); });
    window.addEventListener('beforeunload', e => { if (this.state.doc?.dirty && !this._desktop) { e.preventDefault(); e.returnValue = ''; } });
  }

  async onViewportClick(e) {
    if (this.tool?.def.sculpt) return;
    if (this.walls.active) { this.walls.click(this.viewer.pick(e, { planes: false }), e); return; }
    if (this.tool?.def.type === 'supportfins' && this.tool.values._mode === 'remove') { this.removeFinAt(e); return; }
    if (this.tool && this.tool.activeField === 'regions') {
      const sk = this.state.sketches.find(x => x.id === this.tool.values.sketch);
      if (sk) {
        const p = this.viewer.rayToPlane(e, sk.frame);
        if (p) {
          const [o, x, y] = sk.frame; const d = sub(p.toArray(), o);
          const uv = [+dot(d, x).toFixed(4), +dot(d, y).toFixed(4)];
          const cur = [...(this.tool.values.regions || [])];
          // clicking a picked region again un-picks it
          const faces = await this.sketchFaces(sk);
          const fi = faces.findIndex(fc => inPoly(uv, fc));
          const same = cur.findIndex(q => faces.findIndex(fc => inPoly(q, fc)) === fi);
          if (fi < 0) { this.toolMsg('Click inside a closed region of the sketch.', 'warn'); return; }
          if (same >= 0) cur.splice(same, 1); else cur.push(uv);
          this.setValue('regions', cur);
        }
      }
      return;
    }
    const wantPlanes = !!(this.tool && (this.tool.def.fields.some(f => f.kind === 'plane' && this.tool.activeField === f.key) || this.tool.def.placeOnFace || this.tool.key === 'plane_3pt'));
    const hit = this.viewer.pick(e, { planes: wantPlanes });
    if (this.mode === 'measure') return this.measureClick(hit);
    if (this.mode === 'info') { if (hit?.kind === 'body') { this.selection = [hit.id]; this.viewer.setSelection(this.selection); this.renderTree(); this.showInfo(hit.id); } return; }
    if (this.tool) {
      const used = await this.feedPick(hit, e);
      if (this.tool?.onPlane && this.tool.values.plane) return this.tool.onPlane();
      if (used) return;
    }
    // plain selection
    if (hit?.kind === 'body') {
      if (e.shiftKey || mod(e)) this.selection = this.selection.includes(hit.id) ? this.selection.filter(x => x !== hit.id) : [...this.selection, hit.id];
      else this.selection = [hit.id];
    } else if (!e.shiftKey) this.selection = [];
    this.viewer.setSelection(this.selection);
    this.renderTree();
    this.status(this.selection.length ? `${this.selection.length} selected` : '');
  }

  async quickSketchOnFace(hit) {
    const face = await post('face', { body: hit.id, tri: hit.tri }, { quiet: true });
    if (!face.planar) { this.toast('That face isn\'t flat. Pick a flat face or a plane to sketch on.', 'warn'); return; }
    this.enterSketch({ plane: { origin: face.origin, normal: face.normal, xdir: face.xdir, label: `Face of ${this.bodyName(hit.id)}`, body: hit.id } });
  }

  onFormClick(e) {
    const sk = this.sketcher;
    if (sk.active) {
      const del = e.target.closest('[data-cdel]');
      if (del) { sk.deleteConstraint(del.dataset.cdel); return; }
      const ce = e.target.closest('.ent[data-cid]');
      if (ce) { sk.selC = sk.selC === ce.dataset.cid ? null : ce.dataset.cid; sk.sel.clear(); sk.redraw(); this.renderSketchPanel(); return; }
      if (e.target.closest('[data-sk-act="edit-text"]')) { const t = sk.selectedText(); if (t) this.sketchText(t.meta, t.group); return; }
      if (e.target.closest('[data-sk-act="del-dim"]') && sk.selC) { sk.deleteConstraint(sk.selC); return; }
      const ent = e.target.closest('.ent[data-id]');
      if (ent) sk.setSel(sk.groupOf(ent.dataset.id));
      return;
    }
    if (this.mode === 'section') {
      const a = e.target.closest('[data-sec-axis]'); if (a) { this.sectionState.axis = a.dataset.secAxis; this.renderSection(); }
      if (e.target.closest('[data-sec-clear]')) this.clearCut();
      return;
    }
    const t = this.tool; if (!t) return;
    const fold = e.target.closest('[data-fold]');
    if (fold) { this.toggleSection(fold.dataset.fold); return; }
    if (this.walls.active && this.walls.onFormClick(e)) return;
    if (e.target.closest('[data-sfact="restore"]')) { this.setValue('removed', []); return; }
    const sfa = e.target.closest('[data-sfact]');
    if (sfa?.dataset.sfact === 'suggest') { this.sfSuggest(); return; }
    if (sfa?.dataset.sfact === 'use') { this.sfUsePose(t._suggest.list[+sfa.dataset.i].rot); return; }
    if (sfa?.dataset.sfact === 'strongest') { this.sfUsePose(t._strength.strongest.rot); return; }
    const un = e.target.closest('[data-unpick]');
    if (un) {
      const k = un.dataset.unpick, f = t.def.fields.find(x => x.key === k);
      if (f.kind === 'bodies' && un.dataset.id) t.values[k] = (t.values[k] || []).filter(x => x !== un.dataset.id);
      else if (f.kind === 'bodies') t.values[k] = [];
      else if (f.kind === 'plane') { t.values[k] = null; this.viewer.clearOverlay('face'); }
      else { t.values[k] = []; if (f.kind === 'facepoints') t.values.normal = null; }
      t.activeField = k; this.renderForm(); this.schedulePreview(); e.stopPropagation(); return;
    }
    const pick = e.target.closest('[data-pick]');
    if (pick) { t.activeField = t.activeField === pick.dataset.pick ? null : pick.dataset.pick; this.renderForm(); return; }
    const seg = e.target.closest('[data-seg]');
    if (seg) { this.setValue(seg.dataset.seg, seg.dataset.val); return; }
    if (e.target.id === 'drop-bed') {
      const ids = t.values.bodies || [];
      const bs = this.state.bodies.filter(b => ids.includes(b.id) && b.bbox);
      const landed = t.values.bed && (t.values.rotate || []).some(a => Math.abs(+a) > 1e-9);   // Move Z is then the height above the bed
      if (bs.length) { const zmin = Math.min(...bs.map(b => b.bbox[0][2])); const tr = [...t.values.translate]; tr[2] = landed ? 0 : +(-zmin).toFixed(4); this.setValue('translate', tr); }
    }
    if (e.target.id === 'center-origin') {
      const ids = t.values.bodies || [];
      const bs = this.state.bodies.filter(b => ids.includes(b.id) && b.bbox);
      if (bs.length) {
        const lo = [0, 1, 2].map(i => Math.min(...bs.map(b => b.bbox[0][i]))), hi = [0, 1, 2].map(i => Math.max(...bs.map(b => b.bbox[1][i])));
        this.setValue('translate', [+(-(lo[0] + hi[0]) / 2).toFixed(4), +(-(lo[1] + hi[1]) / 2).toFixed(4), +(-lo[2]).toFixed(4)]);
      }
    }
    if (e.target.id === 'tool-preview') this.runPreview();
    if (e.target.id === 'sc-undo') { const st = [...(t.values.strokes || [])]; st.pop(); this.setValue('strokes', st); }
  }

  onFormInput(e) {
    if (this.mode === 'section' && e.target.id === 'sec-pos') { this.sectionState.t = e.target.value / 1000; this.applyCut(); e.target.closest('.field').nextElementSibling.querySelector('.ctl').textContent = U.str(this.sectionValue()); }
  }

  onFormChange(e) {
    const el = e.target;
    const sk = this.sketcher;
    if (sk.active) {
      if (el.id === 'snap-on') sk.snapGrid = el.checked;
      if (el.id === 'snap-step') { sk.snapStep = parseFloat(el.value); post('settings/set', { key: 'snap_step', value: sk.snapStep }, { quiet: true }).catch(() => {}); }
      if (el.dataset.skdim && sk.selC) {
        const c = sk.constraints.find(x => x.id === sk.selC);
        if (c && el.dataset.skdim === 'ref') sk.setDimRef(c.id, el.checked);
        if (c && el.dataset.skdim === 'value') {
          const r = sk.parseDim(el.value, !!CTYPES[c.type].angle);
          if (r.err) this.toast(r.err, 'warn');
          else if (!(r.v > 0)) this.toast('A dimension needs a size above zero.', 'warn');
          else sk.setDimValue(c.id, r.v, r.expr);
        }
        this.renderSketchPanel();
        return;
      }
      const ent = sk.selectedEntity();
      if (ent && el.dataset.ent) {
        const k = el.dataset.ent, v = ['angle', 'a0', 'a1', 'n'].includes(el.dataset.ent) ? parseNum(el.value) : U.parse(el.value);
        if (!isFinite(v)) return;
        const patch = {};
        if (k === 'd') patch.r = Math.abs(v) / 2; else if (k === 'w' && ent.type === 'slot') patch.r = Math.abs(v) / 2;
        else if (['p1x', 'p1y', 'p2x', 'p2y'].includes(k)) { const key = k.slice(0, 2), i = k[2] === 'x' ? 0 : 1; const pt = [...ent[key]]; pt[i] = v; patch[key] = pt; }
        else if (k === 'len') { const L = Math.hypot(ent.p2[0] - ent.p1[0], ent.p2[1] - ent.p1[1]) || 1; patch.p2 = [ent.p1[0] + (ent.p2[0] - ent.p1[0]) * v / L, ent.p1[1] + (ent.p2[1] - ent.p1[1]) * v / L]; }
        else if (k === 'n') patch.n = Math.max(3, Math.round(v)); else patch[k] = v;
        sk.updateEntity(ent.id, patch);
        this.renderSketchPanel();
      }
      if (ent && el.dataset.pt !== undefined) {
        const v = U.parse(el.value); if (!isFinite(v)) return;
        const pts = ent.pts.map(p => [...p]); pts[+el.dataset.pt][+el.dataset.i] = v;
        sk.updateEntity(ent.id, { pts });
      }
      if (ent && el.dataset.entBool) { sk.updateEntity(ent.id, { [el.dataset.entBool]: el.checked }); this.renderSketchPanel(); }
      return;
    }
    if (this.mode === 'section' && el.id === 'sec-flip') { this.sectionState.flip = el.checked; this.renderSection(); return; }
    const t = this.tool; if (!t) return;
    if (el.dataset.wshapesel && this.walls.active) { this.walls.finish(); this.walls.setShape(el.value); return; }
    if (el.dataset.sfreach) {
      const v = parseNum(el.value);
      if (v > 0) { try { localStorage.setItem('stle_sf_reach', String(v)); } catch { /* storage off */ } this.sfCoverage(); }
      else this.renderForm();
      return;
    }
    if (el.dataset.sfmat) { try { localStorage.setItem('stle_sf_material', el.value); } catch { /* storage off */ } this.renderForm(); return; }
    if (el.dataset.num && exprText(el.value)) { this.setValue(el.dataset.num, exprOf(el.value), false); this.renderForm(); return; }
    if (el.dataset.vec && exprText(el.value)) {
      const v = [...(t.values[el.dataset.vec] || [0, 0, 0])]; v[+el.dataset.i] = exprOf(el.value);
      this.setValue(el.dataset.vec, v, false); this.renderForm(); return;
    }
    if (el.dataset.num) {
      const f = t.def.fields.find(x => x.key === el.dataset.num);
      if (f?.unit === 'mm') { const v = U.parse(el.value); this.setValue(el.dataset.num, Number.isFinite(v) ? v : 0, false); }
      else this.setValue(el.dataset.num, el.value, false);
    }
    else if (el.dataset.text) this.setValue(el.dataset.text, el.value, false);
    else if (el.dataset.feat) { const k = el.dataset.feat, cur = new Set(t.values[k] || []); el.checked ? cur.add(el.dataset.id) : cur.delete(el.dataset.id); this.setValue(k, [...cur]); }
    else if (el.dataset.bool) this.setValue(el.dataset.bool, el.checked);
    else if (el.dataset.sel) this.setValue(el.dataset.sel, el.value);
    else if (el.dataset.vec) {
      const k = el.dataset.vec, f = t.def.fields.find(x => x.key === k);
      const v = [...(t.values[k] || [0, 0, 0])];
      v[+el.dataset.i] = f?.unit === 'mm' ? (U.parse(el.value) || 0) : parseNum(el.value);
      this.setValue(k, v, t.key === 'scale');
    }
  }

  async panelOk() {
    if (this.sketcher.active) return this.finishSketch(false);
    if (['measure', 'section', 'info'].includes(this.mode)) { this.exitModes(); this.updateRibbonState(); this.viewer.resize(); return; }
    if (this.tool?.onPlane) { if (this.tool.values.plane) return this.tool.onPlane(); return; }
    return this.commitTool();
  }

  async panelCancel() {
    if (this.sketcher.active) return this.cancelSketch();
    if (this.mode === 'section' && this._sectionUndo) {
      const u = this._sectionUndo; this._sectionUndo = null;
      this.sectionState = u.state; this.sectionOn = !!u.on;
      this.viewer.sectionPlane = u.plane || null; this.viewer.setSectionEnabled(!!u.state && this.sectionOn);
      this.updateSectionButton();
    }
    if (['measure', 'section', 'info'].includes(this.mode)) { this.exitModes(); this.updateRibbonState(); this.viewer.resize(); return; }
    await this.closeTool();
  }

  onTreeClick(e) {
    if (e.target.closest('[data-params]')) { this.editParams(); return; }
    const eyeB = e.target.closest('[data-eye-body]');
    if (eyeB) { const b = this.state.bodies.find(x => x.id === eyeB.dataset.eyeBody); post('body', { id: b.id, hidden: !b.hidden }, { quiet: true }).then(r => this.apply(r)); return; }
    const eyeI = e.target.closest('[data-eye-item]');
    if (eyeI) { const id = eyeI.dataset.eyeItem; const item = [...this.state.planes, ...this.state.sketches].find(x => x.id === id); post('body', { id, hidden: !item.hidden }, { quiet: true }).then(r => this.apply(r)); return; }
    if (e.target.closest('[data-eye="origin"]')) { this.viewer.baseVisibleUser = !this.viewer.baseVisible; this.viewer.setBasePlanesVisible(this.viewer.baseVisibleUser); this.renderTree(); return; }
    const bn = e.target.closest('[data-body]');
    if (bn) {
      const id = bn.dataset.body;
      if (this.tool) {
        // a body in the tree picks that body; it has no point or face, so only a body field takes it
        const f = this.tool.def.fields.find(x => x.key === this.tool.activeField);
        if (f?.kind === 'bodies') this.feedPick({ kind: 'body', id, point: [0, 0, 0] });
        return;
      }
      if (this.mode === 'info') { this.selection = [id]; this.showInfo(id); }
      else if (e.shiftKey || mod(e)) this.selection = this.selection.includes(id) ? this.selection.filter(x => x !== id) : [...this.selection, id];
      else this.selection = [id];
      this.viewer.setSelection(this.selection); this.renderTree();
      return;
    }
    const pn = e.target.closest('[data-plane]');
    if (pn && this.tool) { this.feedPick({ kind: 'plane', id: pn.dataset.plane }); return; }
    if (pn && !this.tool) { this.beginSketch(pn.dataset.plane); }
  }

  onKey(e) {
    const inField = ['INPUT', 'SELECT', 'TEXTAREA'].includes(document.activeElement?.tagName);
    const k = e.key, ctrl = mod(e);
    if (ctrl && k.toLowerCase() === 's') { e.preventDefault(); this.cmd(e.shiftKey ? 'save_as' : 'save'); return; }
    if (ctrl && k.toLowerCase() === 'o') { e.preventDefault(); this.cmd('open'); return; }
    if (ctrl && k.toLowerCase() === 'i') { e.preventDefault(); this.cmd('import'); return; }
    if (ctrl && k.toLowerCase() === 'n') { e.preventDefault(); this.cmd('new'); return; }
    if ((ctrl && k.toLowerCase() === 'k') || (k === '/' && !inField)) { e.preventDefault(); $('#tool-search')?.focus(); return; }
    if (this.sketcher.active) {
      if (inField && !['dyn-a', 'dyn-b'].includes(document.activeElement.id)) return;
      if (this.sketcher.onKey(e)) { e.preventDefault(); this.updateRibbonState(); }
      return;
    }
    if (inField) return;
    if (this.walls.key(e)) { e.preventDefault(); return; }
    if (this.tool?.def.gizmo === 'move' && !ctrl && ['w', 'r'].includes(k.toLowerCase())) { this.setValue('_gizmo', k.toLowerCase() === 'r' ? 'rotate' : 'move'); return; }
    if (ctrl && k.toLowerCase() === 'z') { e.preventDefault(); this.cmd('undo'); return; }
    if (ctrl && (k.toLowerCase() === 'y' || (e.shiftKey && k.toLowerCase() === 'z'))) { e.preventDefault(); this.cmd('redo'); return; }
    if (k === 'Escape') { if (this.tool || this.mode !== 'model') this.panelCancel(); else { this.selection = []; this.viewer.setSelection([]); this.renderTree(); } return; }
    if (k === 'Enter' && this.tool) { this.panelOk(); return; }
    if (k === 'Delete' && this.selection.length && !this.tool) { this.openTool('delete'); return; }
    const map = { s: () => this.beginSketch(), e: () => this.openTool('extrude'), h: () => this.openTool('hole'), m: () => this.openTool('move'), f: () => this.viewer.fit(), x: () => this.toggleCut() };
    if (!ctrl && map[k.toLowerCase()]) { map[k.toLowerCase()](); return; }
    const views = { 1: 'front', 2: 'back', 3: 'left', 4: 'right', 5: 'top', 6: 'bottom', 0: 'iso' };
    if (!ctrl && views[k]) this.viewer.setView(views[k]);
  }

  // =================================================================== menus & dialogs
  topMenu(btn) {
    const m = btn.dataset.menu;
    const rec = (this.state.recent || []).map(p => ({ label: p.split(/[\\/]/).pop(), title: WEB ? p.split('/').pop() : p, fn: () => this.cmd('open', { path: p }) }));
    const items = {
      file: [
        { label: 'New', k: 'Ctrl+N', fn: () => this.cmd('new') },
        { label: 'Open…', k: 'Ctrl+O', fn: () => this.cmd('open') },
        ...(rec.length ? [{ hdr: 'Recent' }, ...rec.slice(0, 6), { sep: true }] : [{ sep: true }]),
        { label: 'Insert Mesh…', k: 'Ctrl+I', fn: () => this.cmd('import') },
        { label: 'Save', k: 'Ctrl+S', fn: () => this.cmd('save') },
        { label: 'Save As…', k: 'Ctrl+Shift+S', fn: () => this.cmd('save_as') },
        { sep: true },
        { label: 'Export STL…', fn: () => this.exportBodies('stl') },
        { label: 'Export 3MF…', fn: () => this.exportBodies('3mf') },
        { label: 'Export OBJ…', fn: () => this.exportBodies('obj') },
        ...(WEB ? [] : [...(this.deskToolShown('send') ? [{ label: 'Send to Slicer', ic: 'slicer', sub: () => this.slicerItems() }] : []),
          { sep: true },
          { label: 'Quit', fn: () => this.cmd('quit') }]),
      ],
      edit: [
        { label: 'Undo', k: 'Ctrl+Z', fn: () => this.cmd('undo'), dis: !this.state.doc.can_undo },
        { label: 'Redo', k: 'Ctrl+Y', fn: () => this.cmd('redo'), dis: !this.state.doc.can_redo },
        { sep: true },
        { label: 'Parameters…', fn: () => this.editParams() },
        { sep: true },
        { label: 'Select All Bodies', fn: () => { this.selection = this.state.bodies.map(b => b.id); this.viewer.setSelection(this.selection); this.renderTree(); } },
        { label: 'Remove Selected Bodies', k: 'Del', fn: () => this.openTool('delete'), dis: !this.selection.length },
      ],
      view: [
        { label: 'Fit All', k: 'F', fn: () => this.viewer.fit() },
        { label: 'Isometric', k: '0', fn: () => this.viewer.setView('iso') },
        { label: 'Top', k: '5', fn: () => this.viewer.setView('top') },
        { label: 'Front', k: '1', fn: () => this.viewer.setView('front') },
        { label: 'Right', k: '4', fn: () => this.viewer.setView('right') },
        { label: `${this.sectionState && this.sectionOn ? '✓ ' : ''}Section View`, k: 'X', fn: () => this.toggleCut() },
        ...(this.sectionState ? [{ label: 'Edit Section…', fn: () => this.startSection() }, { label: 'Remove Section', fn: () => this.clearCut() }] : []),
        { sep: true },
        { hdr: 'Units' },
        { label: `${U.mode === 'mm' ? '✓ ' : ''}Millimetres`, fn: () => this.setUnits('mm') },
        { label: `${U.mode === 'in' ? '✓ ' : ''}Inches`, fn: () => this.setUnits('in') },
        { hdr: 'Theme' },
        { label: `${this.themePref === 'light' ? '✓ ' : ''}Light`, fn: () => this.setTheme('light') },
        { label: `${this.themePref === 'dark' ? '✓ ' : ''}Dark`, fn: () => this.setTheme('dark') },
        { label: `${this.themePref === 'system' ? '✓ ' : ''}Match Windows`, fn: () => this.setTheme('system') },
        { sep: true },
        { label: `${this.viewer.showEdges ? '✓ ' : ''}Show Edges`, fn: () => this.viewer.setEdgesVisible(!this.viewer.showEdges) },
        { label: `${this.viewer.grid?.visible !== false ? '✓ ' : ''}Show Grid`, fn: () => this.viewer.setGridVisible(!(this.viewer.grid?.visible !== false)) },
        { label: `${this.viewer.baseVisible ? '✓ ' : ''}Show Origin Planes`, fn: () => { this.viewer.baseVisibleUser = !this.viewer.baseVisible; this.viewer.setBasePlanesVisible(this.viewer.baseVisibleUser); this.renderTree(); } },
      ],
      help: [
        { label: 'Keyboard Shortcuts', fn: () => this.showHelp() },
        ...(WEB ? [{ label: 'User Guide', fn: () => window.open('https://github.com/natterman12/STL-Editor/blob/main/USER_GUIDE.md', '_blank', 'noopener') },
          { label: 'Get the Desktop App', fn: () => window.open(DESKTOP_URL, '_blank', 'noopener') },
          { label: 'Report a Problem', fn: () => window.open('https://github.com/natterman12/STL-Editor/issues/new/choose', '_blank', 'noopener') },
          { label: 'Recover Unsaved Work…', fn: () => this.checkRecovery(true) },
          ...(this._install ? [{ label: 'Install STL Editor…', fn: async () => { const p = this._install; this._install = null; await p.prompt(); } }] : [])] : [
          { label: 'Check for Updates…', fn: () => this.checkUpdates(true) },
          { label: 'Update Settings…', fn: () => this.updateSettings() },
          { label: 'Recover Unsaved Work…', fn: () => this.checkRecovery(true) }]),
        { label: 'About Support Fins…', fn: () => this.aboutSupportFins() },
        { label: 'Support STL Editor ♥', fn: () => this.openUrl(KOFI) },
        { label: 'About STL Editor', fn: () => this.alert('STL Editor', WEB
          ? `Web edition ${esc(this.state.doc.version || '')}<br>CAD-style editing for STL, 3MF and OBJ meshes, running entirely in your browser: your files never leave your computer.<br><br>Geometry by Manifold (Apache-2.0), viewport by three.js (MIT), fflate (MIT), opentype.js (MIT), DejaVu fonts (free licence). Free software under the GPL-3.0: <a href="https://github.com/natterman12/STL-Editor" target="_blank" rel="noopener">source on GitHub</a>.${THANKS}`
          : `Version ${esc(this.state.doc.version || '')}<br>CAD-style editing for STL, 3MF and OBJ meshes.<br><br>Geometry by Manifold (Apache-2.0), viewport by three.js (MIT), repair helpers from trimesh (MIT), scikit-image, SciPy, NumPy and Shapely (BSD). Licence texts are in the LICENSES folder.${THANKS}`) },
      ],
    }[m];
    this.showMenu(btn, items);
    btn.classList.add('open');
  }

  bodyMenu(id, ev) {
    if (!this.selection.includes(id)) { this.selection = [id]; this.viewer.setSelection(this.selection); this.renderTree(); }
    const b = this.state.bodies.find(x => x.id === id);
    this.showMenu(ev, [
      { hdr: b.name },
      { label: 'Rename…', fn: () => this.renameBody(id) },
      { label: b.hidden ? 'Show' : 'Hide', fn: () => post('body', { id, hidden: !b.hidden }, { quiet: true }).then(r => this.apply(r)) },
      { label: 'Colour…', fn: () => this.pickColor(id) },
      { label: 'Properties', fn: () => this.startInfo(id) },
      { sep: true },
      ...(b.solid ? [] : [{ label: 'Repair…', fn: () => this.openTool('repair', { values: { body: [id] } }) }]),
      { label: 'Move / Rotate…', fn: () => this.openTool('move') },
      { label: 'Scale…', fn: () => this.openTool('scale') },
      { label: 'Simplify…', fn: () => this.openTool('simplify') },
      { sep: true },
      { label: 'Export STL…', fn: () => this.exportBodies('stl', [id]) },
      { label: 'Export 3MF…', fn: () => this.exportBodies('3mf', [id]) },
      ...(WEB ? [] : [...(this.deskToolShown('send') ? [{ label: 'Send to Slicer', ic: 'slicer', sub: () => this.slicerItems([id]) }] : []),
        ...(this.deskToolShown('to_step') ? [{ label: 'Convert to STEP (SolidCraft)…', fn: () => this.toStep(), dis: !this.apps.cli }] : [])]),
      { sep: true },
      { label: 'Remove', fn: () => this.openTool('delete') },
    ]);
  }

  viewMenu(ev) {
    this.showMenu(ev, [
      { label: 'Fit All', k: 'F', fn: () => this.viewer.fit() },
      { label: 'Isometric', fn: () => this.viewer.setView('iso') },
      { label: 'Top', fn: () => this.viewer.setView('top') },
      { sep: true },
      { label: 'Insert Mesh…', fn: () => this.cmd('import') },
      { label: 'Create Sketch', fn: () => this.beginSketch() },
    ]);
  }

  featureMenu(fid, ev) {
    const f = this.state.features.find(x => x.id === fid); if (!f) return;
    const i = this.state.features.indexOf(f);
    const busy = this.tool || this.sketcher.active;
    this.showMenu(ev, [
      { hdr: f.name + (f.error ? ' — error' : '') },
      ...(f.error ? [{ label: f.error, dis: true }] : []),
      { label: f.type === 'import' ? 'Position / Repair…' : 'Edit…', fn: () => this.editFeature(fid), dis: busy },
      { label: 'Rename…', fn: () => this.renameFeature(f) },
      { label: f.suppressed ? 'Unsuppress' : 'Suppress', fn: () => post('feature/edit', { id: fid, suppressed: !f.suppressed }).then(r => this.apply(r)), dis: busy },
      { label: 'Roll Back to Here', fn: () => post('rollback', { index: i }).then(r => this.apply(r)), dis: busy },
      { label: 'Roll Forward to End', fn: () => post('rollback', { index: null }).then(r => this.apply(r)), dis: busy || this.state.rollback == null },
      ...(f.type === 'sketch' ? [{ label: 'Extrude This Sketch', fn: () => this.openTool('extrude', { values: { sketch: fid } }), dis: busy }] : []),
      { sep: true },
      { label: 'Delete', fn: () => post('feature/delete', { id: fid }).then(r => this.apply(r)).catch(e => this.toast(e.message, 'err')), dis: busy },
    ]);
  }

  showMenu(anchor, items) {
    const m = $('#menu');
    const row = (it, i) => it.sep ? '<div class="sep"></div>' : it.hdr ? `<div class="hdr">${esc(it.hdr)}</div>` :
      `<div class="mi ${it.dis ? 'dis' : ''} ${it.sub ? 'has-sub' : ''}" data-i="${i}" ${it.tool ? `data-mtool="${it.tool}"` : ''} title="${esc(it.title || '')}"><span class="ml">${it.ic ? icon(it.ic) : ''}<span>${esc(it.label)}</span></span>${it.sub ? '<span class="k arrow">▸</span>' : it.k ? `<span class="k">${it.k}</span>` : ''}</div>`;
    m.innerHTML = items.map(row).join('');
    m.classList.remove('hidden');
    let x, y;
    if (anchor instanceof Event || anchor?.clientX !== undefined) { x = anchor.clientX; y = anchor.clientY; }
    else { const r = anchor.getBoundingClientRect(); x = r.left; y = r.bottom + 2; }
    const mw = m.offsetWidth, mh = m.offsetHeight;
    m.style.left = `${Math.min(x, innerWidth - mw - 6)}px`; m.style.top = `${Math.min(y, innerHeight - mh - 6)}px`;
    // a submenu opens beside its item (on hover, or a click/tap) instead of replacing the menu
    let open = null;
    const openSub = el => {
      const it = items[+el.dataset.i];
      if (open?.el === el) return;
      m.querySelector('.submenu')?.remove();
      m.querySelectorAll('.mi.hot').forEach(x => x.classList.remove('hot'));
      if (!it?.sub) { open = null; return; }
      const subItems = typeof it.sub === 'function' ? it.sub() : it.sub;
      const sm = document.createElement('div');
      sm.className = 'submenu';
      sm.innerHTML = subItems.map(row).join('');
      m.appendChild(sm);
      el.classList.add('hot');
      const mr = m.getBoundingClientRect(), er = el.getBoundingClientRect();
      const left = mr.right + sm.offsetWidth + 6 > innerWidth ? -sm.offsetWidth - 2 : mr.width - 6;
      sm.style.left = `${left}px`;
      sm.style.top = `${Math.min(er.top - mr.top - 5, innerHeight - mr.top - sm.offsetHeight - 6)}px`;
      sm.onclick = e => { e.stopPropagation(); const r = e.target.closest('[data-i]'); if (!r) return; const s2 = subItems[+r.dataset.i]; if (s2.dis) return; this.hideMenu(); s2.fn?.(); };
      open = { el, sm };
    };
    m.onmouseover = e => { const el = e.target.closest('.mi[data-i]'); if (el && el.parentElement === m) openSub(el); };
    m.onclick = e => {
      const it = e.target.closest('[data-i]'); if (!it || it.parentElement !== m) return;
      const item = items[+it.dataset.i];
      if (item.sub) { openSub(it); return; }
      this.hideMenu(); item.fn?.();
    };
  }
  hideMenu() { $('#menu').classList.add('hidden'); for (const b of document.querySelectorAll('.menu-btn.open')) b.classList.remove('open'); }

  modal(html, buttons) {
    return new Promise(res => {
      $('#modal-body').innerHTML = html;
      $('#modal-actions').innerHTML = buttons.map((b, i) => `<button data-i="${i}" class="${b.primary ? 'primary' : ''}">${b.label}</button>`).join('');
      $('#modal').classList.remove('hidden');
      const inp = $('#modal-body input'); if (inp) { inp.focus(); inp.select(); inp.onkeydown = e => { if (e.key === 'Enter') done(buttons.findIndex(b => b.primary)); if (e.key === 'Escape') done(-1); }; }
      const done = i => { $('#modal').classList.add('hidden'); res(i >= 0 ? buttons[i].value(inp) : null); };
      $('#modal-actions').onclick = e => { const b = e.target.closest('[data-i]'); if (b) done(+b.dataset.i); };
    });
  }
  confirm(msg) { return this.modal(`<h3>STL Editor</h3><p>${esc(msg)}</p>`, [{ label: 'Cancel', value: () => false }, { label: 'OK', primary: true, value: () => true }]); }
  prompt(title, val) { return this.modal(`<h3>${esc(title)}</h3><input type="text" value="${esc(val)}">`, [{ label: 'Cancel', value: () => null }, { label: 'OK', primary: true, value: i => i.value.trim() }]); }
  alert(title, html) { return this.modal(`<h3>${esc(title)}</h3><div>${html}</div>`, [{ label: 'OK', primary: true, value: () => true }]); }

  async renameBody(id) {
    const name = await this.prompt('Rename body', this.bodyName(id));
    if (name) await this.apply(await post('body', { id, name }, { quiet: true }));
  }

  pickColor(id) {
    const inp = document.createElement('input'); inp.type = 'color';
    inp.value = this.state.bodies.find(b => b.id === id)?.color || '#9aa7b4';
    inp.oninput = () => { const o = this.viewer.bodies.get(id); if (o) { o.mesh.material.color.set(inp.value); this.viewer.requestRender(); } };
    inp.onchange = () => post('body', { id, color: inp.value }, { quiet: true }).then(r => this.apply(r));
    inp.click();
  }

  showHelp() {
    const rows = [['Left-drag', 'Orbit'], ['Right / middle-drag', 'Pan'], ['Wheel', 'Zoom'], ['Click', 'Select a body (Shift adds)'], ['Double-click a flat face', 'Sketch on it'],
      ['S', 'Sketch'], ['E', 'Extrude'], ['H', 'Hole'], ['M', 'Move'], ['F', 'Fit all'], ['0–6', 'Standard views'], ['Ctrl+K or /', 'Search tools'], ['Ctrl+Z / Ctrl+Y', 'Undo / Redo'], ['Ctrl+S', 'Save'],
      ['Esc', 'Cancel'], ['In a sketch: L R C P S', 'Line, Rectangle, Circle, Polygon, Slot'], ['In a sketch: type a number', 'Set an exact size, Tab for the next value, Enter to place'], ['In a sketch: D', 'Smart Dimension: click, move to place, click, type the value'], ['Double-click a dimension', 'Change it (a parameter name works too)']];
    this.alert('Keyboard & mouse', `<div class="help-grid">${rows.map(([a, b]) => `<kbd>${a}</kbd><span>${b}</span>`).join('')}</div>`);
  }

  // =================================================================== preferences
  async loadSettings() {
    try {
      const { settings: st } = await post('settings/get', {}, { quiet: true });
      U.mode = st.units === 'in' ? 'in' : 'mm';
      this.themePref = st.theme || 'dark';
      if (st.snap_step) this.sketcher.snapStep = +st.snap_step;
      this.settings = st;
    } catch { this.themePref = 'dark'; }
    this.applyTheme();
  }

  async setUnits(m) {
    U.mode = m;
    await post('settings/set', { key: 'units', value: m }, { quiet: true }).catch(() => {});
    this.sketcher.snapStep = m === 'in' ? 25.4 / 16 : 1;
    if (this.tool) this.renderForm();
    if (this.sketcher.active) { this.sketcher.redraw(); this.renderSketchPanel(); }
    if (this.mode === 'info' && this.selection[0]) this.showInfo(this.selection[0]);
    this.toast(m === 'in' ? 'Showing inches. Models are still saved in millimetres.' : 'Showing millimetres.');
  }

  async setTheme(t) {
    this.themePref = t;
    await post('settings/set', { key: 'theme', value: t }, { quiet: true }).catch(() => {});
    this.applyTheme();
  }

  applyTheme() {
    const mq = window.matchMedia?.('(prefers-color-scheme: dark)');
    const dark = this.themePref === 'dark' || (this.themePref === 'system' && mq?.matches);
    document.documentElement.dataset.theme = dark ? 'dark' : 'light';
    this.viewer.setTheme(dark);
    if (mq && !this._mqBound) { this._mqBound = true; mq.addEventListener?.('change', () => this.themePref === 'system' && this.applyTheme()); }
  }

  // =================================================================== crash recovery
  async checkRecovery(manual = false) {
    let items = [];
    try { items = (await post('recovery/list', {}, { quiet: true })).items || []; } catch { return; }
    if (!items.length) { if (manual) this.toast('There is no unsaved work to recover.'); return; }
    const when = t => t ? new Date(t * 1000).toLocaleString() : '';
    const html = `<h3>Recover unsaved work?</h3><p>${WEB ? 'This work wasn’t saved before STL Editor was last closed. A copy was kept in this browser:' : 'STL Editor didn’t close normally last time. This work was saved automatically:'}</p>` +
      items.map(it => `<div class="recov"><div><b>${esc(it.title)}</b><div class="t">${it.steps} step${it.steps === 1 ? '' : 's'} · autosaved ${esc(when(it.saved))}</div></div>
        <div style="display:flex;gap:6px"><button data-rdis="${it.id}">Discard</button><button class="primary" data-rec="${it.id}">Recover</button></div></div>`).join('');
    $('#modal-body').innerHTML = html;
    $('#modal-actions').innerHTML = `<button data-close>Later</button>`;
    $('#modal').classList.remove('hidden');
    const close = () => $('#modal').classList.add('hidden');
    $('#modal').onclick = async e => {
      const rec = e.target.closest('[data-rec]'), dis = e.target.closest('[data-rdis]');
      if (e.target.closest('[data-close]')) { close(); $('#modal').onclick = null; return; }
      if (dis) { await post('recovery/discard', { id: dis.dataset.rdis }, { quiet: true }); dis.closest('.recov').remove(); if (!$('#modal-body .recov')) { close(); $('#modal').onclick = null; } }
      if (rec) {
        close(); $('#modal').onclick = null;
        if (this.state.doc?.dirty && !(await this.confirm('Replace the current unsaved work with the recovered project?'))) return;
        const r = await post('recovery/open', { id: rec.dataset.rec }, { msg: 'Recovering…' });
        await this.closeTool(); this.selection = []; await this.apply(r); this.viewer.setView('iso');
        this.toast('Recovered. Save it to keep it.');
      }
    };
  }

  renderAutosave() {
    const t = this.state.doc?.autosaved;
    const el = $('#status');
    if (t && !el.textContent) el.textContent = `Autosaved ${new Date(t * 1000).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}`;
  }

  // =================================================================== updates
  async checkUpdates(manual) {
    let st;
    try { st = await post('update/status', { check: manual }, { quiet: !manual, msg: 'Checking for updates…' }); } catch { return; }
    if (!st.repo) { if (manual) this.updateSettings(); return; }
    if (st.available && (manual || !st.skipped)) return this.showUpdateBanner(st);
    if (manual) this.toast(st.error || `You have the latest version (${st.current}).`, st.error ? 'warn' : '');
  }

  showUpdateBanner(st) {
    let b = $('#banner');
    if (!b) { b = document.createElement('div'); b.id = 'banner'; document.body.appendChild(b); }
    const how = st.install_mode === 'git' ? 'Update now pulls it into this copy (git pull) and restarts.' : st.install_mode === 'portable' ? 'Update now downloads it, swaps it in and restarts STL Editor. Unsaved work is kept for Recover Unsaved Work.' : 'Download the new zip, unzip it, and use it instead of this copy.';
    b.innerHTML = `<b>STL Editor ${esc(st.latest)} is available</b><div class="t" style="color:var(--muted);margin-top:2px">You have ${esc(st.current)}. ${how}</div>
      <div class="b-progress hidden"><div class="bar"><i></i></div><span class="pt"></span></div>
      <div class="b-actions"><button data-u="skip">Skip this version</button><button data-u="later">Later</button>${st.install_mode ? '<button data-u="notes">What’s new</button><button class="primary" data-u="install">Update now</button>' : '<button class="primary" data-u="get">Download</button>'}</div>`;
    const progress = (frac, text) => { b.querySelector('.b-progress').classList.remove('hidden'); b.querySelector('.bar i').style.width = `${Math.round(frac * 100)}%`; b.querySelector('.pt').textContent = text; };
    b.onclick = async e => {
      const a = e.target.closest('[data-u]')?.dataset.u; if (!a) return;
      if (a === 'get' || a === 'notes') { await post('open_url', { url: st.url }, { quiet: true }).catch(err => this.toast(err.message, 'err')); if (a === 'notes') return; }
      if (a === 'skip') await post('settings/set', { key: 'update_skip', value: st.latest }, { quiet: true });
      if (a === 'install') {
        b.querySelector('.b-actions').innerHTML = '<button data-u="later">Cancel</button>';
        progress(0, st.install_mode === 'git' ? 'Pulling the new version…' : 'Downloading…');
        try {
          await post('update/install', {}, { quiet: true });
          for (;;) {
            await new Promise(r => setTimeout(r, 400));
            if (!b.isConnected) return;
            const s2 = await post('update/status', {}, { quiet: true });
            const ins = s2.install || {};
            if (ins.state === 'error') throw new Error(ins.error);
            if (ins.state === 'ready') break;
            progress(ins.progress || 0, ins.state === 'unpacking' ? 'Unpacking…' : `Downloading… ${Math.round((ins.progress || 0) / 0.95 * 100)}%`);
          }
          progress(1, 'Restarting STL Editor…');
          await post('update/apply', {}, { quiet: true });
        } catch (err) {
          progress(0, '');
          b.querySelector('.pt').textContent = err.message;
          b.querySelector('.b-actions').innerHTML = '<button data-u="later">Close</button><button class="primary" data-u="get">Download instead</button>';
        }
        return;
      }
      b.remove();
    };
  }

  async updateSettings() {
    const cur = (await post('update/status', {}, { quiet: true }).catch(() => ({}))).repo || '';
    const v = await this.prompt('GitHub repository for updates (owner/name), empty to turn update checks off', cur);
    if (v === null) return;
    const repo = v.replace(/^https?:\/\/github\.com\//, '').replace(/\/$/, '').trim();
    await post('settings/set', { key: 'update_repo', value: repo }, { quiet: true });
    await post('settings/set', { key: 'update_check', value: !!repo }, { quiet: true });
    if (repo) this.checkUpdates(true); else this.toast('Update checks are off.');
  }

  // =================================================================== feedback
  toast(msg, kind = '') {
    const t = document.createElement('div'); t.className = `toast ${kind}`; t.textContent = msg;
    $('#toasts').appendChild(t);
    setTimeout(() => t.remove(), kind === 'err' ? 7000 : 4000);
  }
  hint(m) { $('#hint').textContent = m || ''; }
  status(m) { $('#status').textContent = m || ''; }
}

// ------------------------------------------------------------------ helpers
function isSelKind(k) { return ['bodies', 'plane', 'facepoints', 'points3', 'pface', 'cylface', 'regions'].includes(k); }
function isEmptySel(v) { return v == null || v === '' || (Array.isArray(v) && !v.length); }
function parseNum(v) {
  if (typeof v === 'number') return v;
  const s = String(v ?? '').trim().replace(',', '.');
  if (!s) return 0;
  if (/^[0-9+\-*/(). ]+$/.test(s)) { try { const r = Function(`"use strict";return (${s})`)(); if (isFinite(r)) return r; } catch { } }
  const f = parseFloat(s); return isFinite(f) ? f : 0;
}
function fmtTris(n) { return n >= 1e6 ? `${(n / 1e6).toFixed(1)}M` : n >= 1e3 ? `${Math.round(n / 1e3)}k` : String(n); }
function inPoly(p, rings) {
  // rings[0] exterior, rest holes (even-odd)
  let c = false;
  for (const ring of rings) for (let i = 0, j = ring.length - 1; i < ring.length; j = i++) {
    const [xi, yi] = ring[i], [xj, yj] = ring[j];
    if (((yi > p[1]) !== (yj > p[1])) && p[0] < (xj - xi) * (p[1] - yi) / (yj - yi) + xi) c = !c;
  }
  return c;
}
function sub(a, b) { return [a[0] - b[0], a[1] - b[1], a[2] - b[2]]; }
function dot(a, b) { return a[0] * b[0] + a[1] * b[1] + a[2] * b[2]; }
function cross(a, b) { return [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]; }
function toolKeyFor(f) {
  if (f.type === 'primitive') return `primitive_${f.params.shape}`;
  if (f.type === 'plane') return { offset: 'plane_offset', face: 'plane_offset', angle: 'plane_angle', three_points: 'plane_3pt', midplane: 'plane_mid' }[f.params.mode];
  if (f.type === 'supportfins' || f.type === 'supportwall') return f.type;   // built on first use
  return TOOLS[f.type] ? f.type : null;
}


window.app = new App();
