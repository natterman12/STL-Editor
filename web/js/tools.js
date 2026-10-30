// Tool definitions: the form fields each feature needs and how they map to engine params.
const OPS = [['new', 'New body'], ['join', 'Join'], ['cut', 'Cut'], ['intersect', 'Intersect']];
const AXES = [['x', 'X'], ['y', 'Y'], ['z', 'Z']];
const AXV = { x: [1, 0, 0], y: [0, 1, 0], z: [0, 0, 1] };
const vecAxis = v => { if (!v) return 'z'; const i = v.map(Math.abs).indexOf(Math.max(...v.map(Math.abs))); return 'xyz'[i]; };
// Hole presets. Insert sizes follow common CNC Kitchen / Ruthex style M-inserts; check your
// insert's datasheet. Tolerance (mm) is added to diameters (and depths for pockets).
const r2 = x => Math.round(x * 100) / 100;
const insert = (m, d, len) => ({ id: `ins_${m}`, label: `Heat-set insert ${m}`, group: 'Heat-set inserts',
  apply: t => ({ kind: 'simple', diameter: d, through: false, depth: r2(len + 1), tip: false }) });
const clear = (m, d) => ({ id: `clr_${m}`, label: `Screw clearance ${m}`, group: 'Screws',
  apply: t => ({ kind: 'simple', diameter: r2(d + t), through: true }) });
const cbore = (m, d, hd, hh) => ({ id: `cb_${m}`, label: `Socket head ${m} (counterbore)`, group: 'Screws',
  apply: t => ({ kind: 'counterbore', diameter: r2(d + t), through: true, cb_diameter: r2(hd + 0.5 + t), cb_depth: r2(hh + 0.3) }) });
const nut = (m, d, af, h) => ({ id: `nut_${m}`, label: `Hex nut trap ${m}`, group: 'Screws',
  apply: t => ({ kind: 'hex', diameter: r2(d + t), through: true, hex_af: r2(af + t), hex_depth: r2(h + 0.2) }) });
const magnet = (dia, h) => ({ id: `mag_${dia}x${h}`, label: `Magnet ${dia} × ${h} mm`, group: 'Magnet pockets',
  apply: t => ({ kind: 'simple', diameter: r2(dia + t), through: false, depth: r2(h + t), tip: false }) });
export const HOLE_PRESETS = [
  { id: 'custom', label: 'Custom', group: '' },
  insert('M2', 3.2, 3), insert('M2.5', 3.6, 4), insert('M3', 4.0, 5.7), insert('M4', 5.6, 8.1), insert('M5', 6.4, 9.5),
  clear('M2', 2.4), clear('M2.5', 2.9), clear('M3', 3.4), clear('M4', 4.5), clear('M5', 5.5),
  cbore('M3', 3.4, 5.5, 3), cbore('M4', 4.5, 7, 4), cbore('M5', 5.5, 8.5, 5),
  nut('M3', 3.4, 5.5, 2.4), nut('M4', 4.5, 7, 3.2), nut('M5', 5.5, 8, 4),
  magnet(5, 2), magnet(6, 2), magnet(6, 3), magnet(8, 2), magnet(8, 3), magnet(10, 2), magnet(10, 3), magnet(12, 3), magnet(15, 3),
];
// ISO metric coarse threads: [name, major diameter, pitch]
export const THREAD_SIZES = [['M3', 3, 0.5], ['M4', 4, 0.7], ['M5', 5, 0.8], ['M6', 6, 1.0], ['M8', 8, 1.25], ['M10', 10, 1.5], ['M12', 12, 1.75],
  ['M14', 14, 2.0], ['M16', 16, 2.0], ['M20', 20, 2.5], ['M24', 24, 3.0], ['M30', 30, 3.5], ['custom', 0, 0]];
const PRINTER_OPTS = [['bambu_x1', 'Bambu X1C / P1S / P1P / A1 (256)'], ['bambu_a1mini', 'Bambu A1 mini (180)'], ['bambu_h2d', 'Bambu H2D (325×320)'],
  ['prusa_mk4', 'Prusa MK4 (250×210)'], ['prusa_xl', 'Prusa XL (360)'], ['voron_350', 'Voron 2.4 350'], ['voron_300', 'Voron 2.4 300'], ['ender3', 'Ender 3 (220)'], ['custom', 'Custom size']];
const CONNECTOR_OPTS = [['pins', 'Printed pins'], ['dowels', 'Holes + loose pins'], ['filament', 'Filament pins (1.75)'], ['magnets', 'Magnets 6×3'], ['none', 'None']];
const CONNECTOR_DEF = { pins: [5, 10], dowels: [5, 12], filament: [1.75, 10], magnets: [6, 3], none: [5, 10] };
const connectorFields = cond => [
  { key: 'connector', label: 'Connectors', kind: 'select', options: CONNECTOR_OPTS, show: cond },
  { key: 'pin_d', label: 'Pin / pocket Ø', kind: 'number', unit: 'mm', show: v => cond(v) && v.connector !== 'none' },
  { key: 'pin_len', label: 'Pin length', kind: 'number', unit: 'mm', show: v => cond(v) && v.connector !== 'none' },
  { key: 'clearance', label: 'Clearance', kind: 'number', unit: 'mm', show: v => cond(v) && v.connector !== 'none' },
];
const connectorChange = (key, v) => key === 'connector' ? { pin_d: CONNECTOR_DEF[v.connector][0], pin_len: CONNECTOR_DEF[v.connector][1] } : null;

const selBody = ctx => ctx.selection.filter(id => ctx.state.bodies.some(b => b.id === id));
// features whose result can be repeated (mirrored / patterned): anything that cut or added a tool shape
export const REPEATABLE = ['extrude', 'revolve', 'hole', 'primitive'];
export const repeatableFeatures = state => state.features.filter(f => REPEATABLE.includes(f.type) && f.active && !f.error && !f.suppressed);
const lastRepeatable = ctx => repeatableFeatures(ctx.state).at(-1)?.id;

export const TOOLS = {
  // ----------------------------------------------------------------- create
  extrude: {
    label: 'Extrude', icon: 'extrude', type: 'extrude',
    fields: [
      { key: 'sketch', label: 'Sketch', kind: 'sketch' },
      { key: 'regions', label: 'Profiles', kind: 'regions' },
      { key: 'operation', label: 'Operation', kind: 'select', options: OPS },
      { key: 'direction', label: 'Direction', kind: 'select', options: [['one', 'One side'], ['reverse', 'Flipped'], ['symmetric', 'Symmetric'], ['two', 'Two sides']] },
      { key: 'through_all', label: 'Through all', kind: 'bool', show: v => v.operation === 'cut' },
      { key: 'distance', label: 'Distance', kind: 'number', unit: 'mm', show: v => !(v.through_all && v.operation === 'cut') },
      { key: 'distance2', label: 'Distance 2', kind: 'number', unit: 'mm', show: v => v.direction === 'two' && !(v.through_all && v.operation === 'cut') },
      { key: 'start_offset', label: 'Start offset', kind: 'number', unit: 'mm' },
      { key: 'taper', label: 'Taper angle', kind: 'number', unit: '°' },
      { key: 'targets', label: 'Bodies', kind: 'bodies', multi: true, placeholder: 'Auto (bodies it touches)', show: v => v.operation !== 'new' },
    ],
    defaults: ctx => ({ sketch: ctx.lastSketch, regions: [], operation: ctx.sketchOnBody ? 'join' : 'new', direction: 'one', distance: 10, distance2: 10, start_offset: 0, taper: 0, through_all: false, targets: [] }),
    hint: 'Pick a sketch, set the distance · Cut goes into the material automatically',
  },
  revolve: {
    label: 'Revolve', icon: 'revolve', type: 'revolve',
    fields: [
      { key: 'sketch', label: 'Sketch', kind: 'sketch' },
      { key: 'regions', label: 'Profiles', kind: 'regions' },
      { key: 'axis', label: 'Axis', kind: 'axisline' },
      { key: 'angle', label: 'Angle', kind: 'number', unit: '°' },
      { key: 'operation', label: 'Operation', kind: 'select', options: OPS },
      { key: 'targets', label: 'Bodies', kind: 'bodies', multi: true, placeholder: 'Auto', show: v => v.operation !== 'new' },
    ],
    defaults: ctx => ({ sketch: ctx.lastSketch, regions: [], axis: 'y', angle: 360, operation: 'new', targets: [] }),
    hint: 'Revolves the sketch profile about the sketch X or Y axis, or a construction line in the sketch',
  },
  primitive_box: primitive('box', 'Box', 'box', [
    { key: 'length', label: 'Length (X)', kind: 'number', unit: 'mm' }, { key: 'width', label: 'Width (Y)', kind: 'number', unit: 'mm' },
    { key: 'height', label: 'Height (Z)', kind: 'number', unit: 'mm' }], { length: 20, width: 20, height: 20 }),
  primitive_cylinder: primitive('cylinder', 'Cylinder', 'cylinder', [
    { key: 'radius', label: 'Radius', kind: 'number', unit: 'mm' }, { key: 'height', label: 'Height', kind: 'number', unit: 'mm' }], { radius: 10, height: 20 }),
  primitive_sphere: primitive('sphere', 'Sphere', 'sphere', [{ key: 'radius', label: 'Radius', kind: 'number', unit: 'mm' }], { radius: 10 }),
  primitive_cone: primitive('cone', 'Cone', 'cone', [
    { key: 'radius', label: 'Base radius', kind: 'number', unit: 'mm' }, { key: 'radius2', label: 'Top radius', kind: 'number', unit: 'mm' },
    { key: 'height', label: 'Height', kind: 'number', unit: 'mm' }], { radius: 10, radius2: 0, height: 20 }),
  primitive_torus: primitive('torus', 'Torus', 'torus', [
    { key: 'radius', label: 'Ring radius', kind: 'number', unit: 'mm' }, { key: 'radius2', label: 'Tube radius', kind: 'number', unit: 'mm' }], { radius: 15, radius2: 4 }),

  // ----------------------------------------------------------------- modify
  hole: {
    label: 'Hole', icon: 'hole', type: 'hole',
    fields: [
      { key: 'points', label: 'Positions', kind: 'facepoints' },
      { key: 'preset', label: 'Preset', kind: 'select', options: HOLE_PRESETS.map(p => [p.id, p.label]), groups: true },
      { key: 'tol', label: 'Print tolerance', kind: 'number', unit: 'mm', show: v => v.preset !== 'custom' },
      { key: 'kind', label: 'Type', kind: 'select', options: [['simple', 'Simple'], ['counterbore', 'C-bore'], ['countersink', 'C-sink'], ['hex', 'Nut trap']] },
      { key: 'diameter', label: 'Diameter', kind: 'number', unit: 'mm' },
      { key: 'through', label: 'Through all', kind: 'bool' },
      { key: 'depth', label: 'Depth', kind: 'number', unit: 'mm', show: v => !v.through },
      { key: 'tip', label: 'Drill-point tip', kind: 'bool', show: v => !v.through },
      { key: 'cb_diameter', label: 'Bore Ø', kind: 'number', unit: 'mm', show: v => v.kind === 'counterbore' },
      { key: 'cb_depth', label: 'Bore depth', kind: 'number', unit: 'mm', show: v => v.kind === 'counterbore' },
      { key: 'cs_diameter', label: 'Sink Ø', kind: 'number', unit: 'mm', show: v => v.kind === 'countersink' },
      { key: 'cs_angle', label: 'Sink angle', kind: 'number', unit: '°', show: v => v.kind === 'countersink' },
      { key: 'hex_af', label: 'Nut across flats', kind: 'number', unit: 'mm', show: v => v.kind === 'hex' },
      { key: 'hex_depth', label: 'Nut depth', kind: 'number', unit: 'mm', show: v => v.kind === 'hex' },
      { key: 'targets', label: 'Bodies', kind: 'bodies', multi: true, placeholder: 'Auto' },
    ],
    defaults: () => ({ points: [], normal: null, preset: 'custom', tol: 0.2, kind: 'simple', diameter: 5, through: true, depth: 10, tip: true, cb_diameter: 9, cb_depth: 3, cs_diameter: 10, cs_angle: 90, hex_af: 5.7, hex_depth: 2.6, targets: [] }),
    toParams: v => ({ ...v, point: undefined }),
    onChange: (key, v) => {
      if (key !== 'preset' && key !== 'tol') { if (['diameter', 'depth', 'kind', 'through', 'cb_diameter', 'cb_depth', 'hex_af', 'hex_depth'].includes(key)) return { preset: 'custom' }; return null; }
      const pr = HOLE_PRESETS.find(p => p.id === v.preset);
      return pr && pr.apply ? pr.apply(+v.tol || 0) : null;
    },
    hint: 'Click faces to place holes (click again to add more). Presets size inserts, screws, nut traps and magnets with print tolerance',
  },
  combine: {
    label: 'Combine', icon: 'combine', type: 'combine',
    fields: [
      { key: 'target', label: 'Target body', kind: 'bodies', multi: false },
      { key: 'tools', label: 'Tool bodies', kind: 'bodies', multi: true },
      { key: 'operation', label: 'Operation', kind: 'select', options: [['join', 'Join'], ['cut', 'Cut'], ['intersect', 'Intersect']] },
      { key: 'keep_tools', label: 'Keep tools', kind: 'bool' },
    ],
    defaults: ctx => { const s = selBody(ctx); return { target: s[0] ? [s[0]] : [], tools: s.slice(1), operation: 'join', keep_tools: false }; },
    toParams: v => ({ ...v, target: v.target[0] }),
    fromParams: p => ({ ...p, target: p.target ? [p.target] : [] }),
    hint: 'Pick the body to keep, then the bodies to join, cut away or intersect with it',
  },
  split: {
    label: 'Split', icon: 'split', type: 'split',
    fields: [
      { key: 'body', label: 'Body', kind: 'bodies', multi: false },
      { key: 'plane', label: 'Split plane', kind: 'plane' },
      { key: 'keep', label: 'Keep', kind: 'select', options: [['both', 'Both'], ['above', 'Above'], ['below', 'Below']] },
      ...connectorFields(v => v.keep === 'both'),
    ],
    defaults: ctx => ({ body: selBody(ctx).slice(0, 1), plane: null, keep: 'both', connector: 'none', pin_d: 5, pin_len: 10, clearance: 0.15 }),
    toParams: v => ({ ...v, body: v.body[0] }),
    fromParams: p => ({ connector: 'none', pin_d: 5, pin_len: 10, clearance: 0.15, ...p, body: p.body ? [p.body] : [] }),
    onChange: connectorChange,
    hint: 'Pick a body, then a plane or flat face to cut it with · add connectors so the pieces line up when you glue them',
  },
  bedsplit: {
    label: 'Fit to Bed', icon: 'bedsplit', type: 'bedsplit',
    fields: [
      { key: 'body', label: 'Body', kind: 'bodies', multi: false },
      { key: 'printer', label: 'Printer', kind: 'select', options: PRINTER_OPTS },
      { key: 'bed', label: 'Bed X Y Z', kind: 'vec3', unit: 'mm', show: v => v.printer === 'custom' },
      { key: 'margin', label: 'Margin', kind: 'number', unit: 'mm' },
      ...connectorFields(() => true),
      { key: 'optimize', label: 'Smart cut positions', kind: 'bool' },
    ],
    defaults: ctx => ({ body: selBody(ctx).slice(0, 1), printer: 'bambu_x1', bed: [256, 256, 256], margin: 5, connector: 'pins', pin_d: 5, pin_len: 10, clearance: 0.15, optimize: true }),
    toParams: v => { const p = { ...v, body: v.body[0] }; if (v.printer !== 'custom') delete p.bed; return p; },
    fromParams: p => ({ bed: [256, 256, 256], ...p, body: p.body ? [p.body] : [] }),
    onChange: connectorChange,
    hint: 'Cuts the part into pieces that fit your printer, with pins so they line up. Smart cut positions puts seams where the part is thinnest',
  },
  layflat: {
    label: 'Lay Flat', icon: 'layflat', type: 'layflat',
    fields: [
      { key: 'how', label: 'Method', kind: 'select', options: [['face', 'Pick a face'], ['auto', 'Most stable side']] },
      { key: 'face', label: 'Face to put down', kind: 'pface', anyFace: true, show: v => v.how === 'face' },
      { key: 'bodies', label: 'Body', kind: 'bodies', multi: false, show: v => v.how === 'auto' },
    ],
    defaults: ctx => ({ how: 'face', face: null, body: null, bodies: selBody(ctx).slice(0, 1) }),
    toParams: v => v.how === 'auto' ? { auto: true, body: v.bodies[0] } : { body: v.body, normal: v.face?.normal },
    fromParams: p => p.auto ? { how: 'auto', bodies: [p.body], face: null } : { how: 'face', body: p.body, face: p.normal ? { normal: p.normal, label: 'Picked face' } : null, bodies: [] },
    hint: 'Click the face that should sit on the print bed, or let it pick the most stable side',
  },
  thread: {
    label: 'Thread', icon: 'thread', type: 'thread',
    fields: [
      { key: 'kind', label: 'Type', kind: 'select', options: [['internal', 'Tapped hole'], ['external', 'Screw / stud']] },
      { key: 'size', label: 'Size', kind: 'select', options: THREAD_SIZES.map(t => [t[0], t[0] === 'custom' ? 'Custom' : `${t[0]} × ${t[2]}`]) },
      { key: 'diameter', label: 'Major Ø', kind: 'number', unit: 'mm' },
      { key: 'pitch', label: 'Pitch', kind: 'number', unit: 'mm' },
      { key: 'length', label: 'Length', kind: 'number', unit: 'mm' },
      { key: 'tolerance', label: 'Print clearance', kind: 'number', unit: 'mm' },
      { key: 'hand', label: 'Hand', kind: 'select', options: [['right', 'Right'], ['left', 'Left']] },
      { key: 'chamfer', label: 'Chamfered ends', kind: 'bool' },
      { key: 'plane', label: 'On', kind: 'plane' },
      { key: 'offset', label: 'Position', kind: 'vec3', unit: 'mm' },
      { key: 'operation', label: 'Operation', kind: 'select', options: OPS },
      { key: 'targets', label: 'Bodies', kind: 'bodies', multi: true, placeholder: 'Auto', show: v => v.operation !== 'new' },
    ],
    defaults: () => ({ kind: 'internal', size: 'M8', diameter: 8, pitch: 1.25, length: 10, tolerance: 0.3, hand: 'right', chamfer: true, plane: 'XY', offset: [0, 0, 0], operation: 'cut', targets: [] }),
    toParams: v => { const p = { ...v }; delete p.size; return p; },
    fromParams: p => ({ size: 'custom', ...p }),
    onChange: (key, v) => {
      if (key === 'size') { const t = THREAD_SIZES.find(x => x[0] === v.size); return t && t[0] !== 'custom' ? { diameter: t[1], pitch: t[2] } : null; }
      if (key === 'kind') return { operation: v.kind === 'internal' ? 'cut' : (v.plane && typeof v.plane === 'object' ? 'join' : 'new') };
      if (key === 'diameter' || key === 'pitch') return { size: 'custom' };
      if (key === 'plane' && v.kind === 'external' && v.plane && typeof v.plane === 'object') return { operation: 'join' };
      return null;
    },
    placeOnFace: true,
    hint: 'Click a face to put the thread there (tapped holes go into the face, screws stand on it). For FDM, pitches of 1 mm or more and 0.2–0.4 mm clearance print best',
  },
  pushpull: {
    label: 'Push/Pull', icon: 'pushpull', type: 'pushpull',
    fields: [
      { key: 'face', label: 'Flat face', kind: 'pface' },
      { key: 'distance', label: 'Distance', kind: 'number', unit: 'mm' },
    ],
    defaults: () => ({ face: null, body: null, distance: 5 }),
    toParams: v => ({ body: v.body, face: v.face, distance: v.distance }),
    fromParams: p => ({ ...p }),
    hint: 'Click a flat face · positive distance pulls it out, negative pushes it in',
  },
  mirror: {
    label: 'Mirror', icon: 'mirror', type: 'mirror',
    fields: [
      { key: 'kind', label: 'Mirror', kind: 'select', options: [['features', 'Features'], ['bodies', 'Bodies']] },
      { key: 'features', label: 'Features', kind: 'features', show: v => v.kind === 'features' },
      { key: 'bodies', label: 'Bodies', kind: 'bodies', multi: true, show: v => v.kind === 'bodies' },
      { key: 'plane', label: 'Mirror plane', kind: 'plane' },
      { key: 'mode', label: 'Result', kind: 'select', options: [['join', 'Join'], ['copy', 'New body'], ['replace', 'Replace']], show: v => v.kind === 'bodies' },
    ],
    defaults: ctx => {
      const last = lastRepeatable(ctx);
      const kind = selBody(ctx).length || !last ? 'bodies' : 'features';
      return { kind, features: last ? [last] : [], bodies: selBody(ctx), plane: 'YZ', mode: 'copy' };
    },
    fromParams: p => ({ kind: 'bodies', features: [], bodies: [], mode: 'copy', ...p }),
    hint: 'Features: repeats a cut, hole or extrude on the other side of the plane (tick them below or click them on the timeline). Bodies: mirrors whole bodies',
  },
  move: {
    label: 'Move / Rotate', icon: 'move', type: 'move',
    fields: [
      { key: 'bodies', label: 'Bodies', kind: 'bodies', multi: true },
      { key: 'translate', label: 'Move X Y Z', kind: 'vec3', unit: 'mm' },
      { key: 'rotate', label: 'Rotate X Y Z', kind: 'vec3', unit: '°' },
      { key: 'pivot', label: 'Rotate about', kind: 'select', options: [['center', 'Centre'], ['base', 'Base'], ['origin', 'Origin']] },
      { key: 'copy', label: 'Make a copy', kind: 'bool' },
    ],
    defaults: ctx => ({ bodies: selBody(ctx), translate: [0, 0, 0], rotate: [0, 0, 0], pivot: 'center', copy: false }),
    extra: 'dropToBed',
    hint: 'Type distances and angles, or use Drop to bed / Centre on origin',
  },
  scale: {
    label: 'Scale', icon: 'scale', type: 'scale',
    fields: [
      { key: 'bodies', label: 'Bodies', kind: 'bodies', multi: true },
      { key: 'mode', label: 'Mode', kind: 'select', options: [['factor', 'Factor'], ['fit', 'To size'], ['units', 'Units']] },
      { key: 'factor', label: 'Factor', kind: 'number', show: v => v.mode === 'factor' && v.uniform },
      { key: 'factor3', label: 'Factor X Y Z', kind: 'vec3', show: v => v.mode === 'factor' && !v.uniform },
      { key: 'size', label: 'Size X Y Z', kind: 'vec3', unit: 'mm', show: v => v.mode === 'fit' },
      { key: 'uniform', label: 'Uniform', kind: 'bool', show: v => v.mode !== 'units' },
      { key: 'units', label: 'Convert', kind: 'select', options: [['in_to_mm', 'inch → mm'], ['mm_to_in', 'mm → inch'], ['cm_to_mm', 'cm → mm'], ['m_to_mm', 'm → mm']], show: v => v.mode === 'units' },
      { key: 'pivot', label: 'Fixed point', kind: 'select', options: [['base', 'Base'], ['center', 'Centre'], ['origin', 'Origin']] },
    ],
    defaults: ctx => {
      const b = ctx.state.bodies.find(x => x.id === selBody(ctx)[0]);
      const size = b?.bbox ? b.bbox[1].map((v, i) => +(v - b.bbox[0][i]).toFixed(3)) : [10, 10, 10];
      return { bodies: selBody(ctx), mode: 'factor', factor: 1, factor3: [1, 1, 1], size, uniform: true, units: 'in_to_mm', pivot: 'base', _size0: size };
    },
    toParams: v => {
      const p = { bodies: v.bodies, mode: v.mode, pivot: v.pivot, units: v.units, uniform: v.uniform };
      if (v.mode === 'factor') p.factor = v.uniform ? v.factor : v.factor3;
      if (v.mode === 'fit') {
        p.size = v.size;
        if (v.uniform && v._size0) { const d = v.size.map((s, i) => Math.abs(s - v._size0[i])); p.axis = d.indexOf(Math.max(...d)); }
      }
      return p;
    },
    fromParams: p => ({ ...p, factor: Array.isArray(p.factor) ? 1 : (p.factor ?? 1), factor3: Array.isArray(p.factor) ? p.factor : [1, 1, 1], size: p.size || [10, 10, 10] }),
    hint: 'To size with Uniform on: change one dimension and the others follow',
  },
  pattern: {
    label: 'Pattern', icon: 'pattern', type: 'pattern',
    fields: [
      { key: 'source', label: 'Pattern', kind: 'select', options: [['features', 'Features'], ['bodies', 'Bodies']] },
      { key: 'features', label: 'Features', kind: 'features', show: v => v.source === 'features' },
      { key: 'bodies', label: 'Bodies', kind: 'bodies', multi: true, show: v => v.source === 'bodies' },
      { key: 'kind', label: 'Type', kind: 'select', options: [['linear', 'Linear'], ['circular', 'Circular']] },
      { key: 'count', label: 'Count', kind: 'number' },
      { key: 'axisname', label: 'Direction', kind: 'select', options: AXES },
      { key: 'spacing', label: 'Spacing', kind: 'number', unit: 'mm', show: v => v.kind === 'linear' },
      { key: 'angle', label: 'Total angle', kind: 'number', unit: '°', show: v => v.kind === 'circular' },
      { key: 'center', label: 'Axis through', kind: 'vec3', unit: 'mm', show: v => v.kind === 'circular' },
      { key: 'join', label: 'Join copies', kind: 'bool', show: v => v.source === 'bodies' },
    ],
    defaults: ctx => {
      const last = lastRepeatable(ctx);
      const source = selBody(ctx).length || !last ? 'bodies' : 'features';
      return { source, features: last ? [last] : [], bodies: selBody(ctx), kind: 'linear', count: 3, axisname: 'x', spacing: 20, angle: 360, center: [0, 0, 0], join: false };
    },
    toParams: v => ({ source: v.source, features: v.features, bodies: v.bodies, kind: v.kind, count: v.count, spacing: v.spacing, angle: v.angle, center: v.center, join: v.join, direction: AXV[v.axisname], axis: AXV[v.axisname], axisname: v.axisname }),
    fromParams: p => ({ source: 'bodies', features: [], ...p, axisname: p.axisname || vecAxis(p.direction) }),
    hint: 'Features: repeats a cut, hole or extrude (tick them below or click them on the timeline). Bodies: copies whole bodies',
  },
  hollow: {
    label: 'Hollow', icon: 'hollow', type: 'hollow',
    fields: [
      { key: 'body', label: 'Body', kind: 'bodies', multi: false },
      { key: 'thickness', label: 'Wall', kind: 'number', unit: 'mm' },
      { key: 'resolution', label: 'Quality', kind: 'select', options: [['draft', 'Draft'], ['normal', 'Normal'], ['fine', 'Fine']] },
    ],
    defaults: ctx => ({ body: selBody(ctx).slice(0, 1), thickness: 2, resolution: 'normal' }),
    toParams: v => ({ ...v, body: v.body[0] }),
    fromParams: p => ({ ...p, body: p.body ? [p.body] : [] }),
    noLivePreview: true,
    hint: 'Shells the body inward. Add a hole afterwards to drain resin or save filament',
  },

  // ----------------------------------------------------------------- construct
  plane_offset: {
    label: 'Offset Plane', icon: 'plane', type: 'plane',
    fields: [{ key: 'base', label: 'From', kind: 'plane' }, { key: 'distance', label: 'Offset', kind: 'number', unit: 'mm' }, { key: 'flip', label: 'Flip normal', kind: 'bool' }, { key: 'name', label: 'Name', kind: 'text' }],
    defaults: () => ({ mode: 'offset', base: 'XY', distance: 10, flip: false, name: '' }),
    toParams: v => ({ ...v, mode: 'offset' }), planeTool: true,
    hint: 'Pick a base plane or a flat face, then set the offset',
  },
  plane_angle: {
    label: 'Plane at Angle', icon: 'plane_angle', type: 'plane',
    fields: [{ key: 'base', label: 'From', kind: 'plane' }, { key: 'axis', label: 'Hinge axis', kind: 'select', options: [['x', 'Plane X'], ['y', 'Plane Y']] },
      { key: 'angle', label: 'Angle', kind: 'number', unit: '°' }, { key: 'distance', label: 'Offset', kind: 'number', unit: 'mm' }, { key: 'name', label: 'Name', kind: 'text' }],
    defaults: () => ({ mode: 'angle', base: 'XY', axis: 'x', angle: 45, distance: 0, name: '' }),
    toParams: v => ({ ...v, mode: 'angle' }), planeTool: true,
  },
  plane_3pt: {
    label: '3-Point Plane', icon: 'plane_3pt', type: 'plane',
    fields: [{ key: 'points', label: 'Points', kind: 'points3' }, { key: 'flip', label: 'Flip normal', kind: 'bool' }, { key: 'name', label: 'Name', kind: 'text' }],
    defaults: () => ({ mode: 'three_points', points: [], flip: false, name: '' }),
    toParams: v => ({ ...v, mode: 'three_points' }), planeTool: true,
    hint: 'Click three points on the model',
  },
  plane_mid: {
    label: 'Midplane', icon: 'plane_mid', type: 'plane',
    fields: [{ key: 'base', label: 'First', kind: 'plane' }, { key: 'base2', label: 'Second', kind: 'plane' }, { key: 'name', label: 'Name', kind: 'text' }],
    defaults: () => ({ mode: 'midplane', base: null, base2: null, name: '' }),
    toParams: v => ({ ...v, mode: 'midplane' }), planeTool: true,
    hint: 'Pick two planes or flat faces',
  },

  // ----------------------------------------------------------------- import (edit only)
  import: {
    label: 'Imported Mesh', icon: 'import', type: 'import',
    fields: [
      { key: 'place', label: 'Position', kind: 'select', options: [['center', 'On bed at origin'], ['keep', 'As in file']] },
      { key: 'repair', label: 'Auto-repair', kind: 'bool' },
    ],
    defaults: () => ({ place: 'center', repair: true }),
    fromParams: p => ({ ...p, place: p.place || 'keep', repair: p.repair !== false }),
    hint: '"On bed at origin" centres the part and sets it on Z = 0. "As in file" keeps its original coordinates (use this to line up parts of an assembly)',
  },

  // ----------------------------------------------------------------- mesh
  sculpt: {
    label: 'Sculpt', icon: 'sculpt', type: 'sculpt',
    fields: [
      { key: 'mode', label: 'Brush', kind: 'select', options: [['smooth', 'Smooth'], ['flatten', 'Flatten'], ['inflate', 'Inflate'], ['deflate', 'Deflate']] },
      { key: 'radius', label: 'Size', kind: 'number', unit: 'mm' },
      { key: 'strength', label: 'Strength', kind: 'number', unit: '0–1' },
    ],
    defaults: ctx => ({ body: selBody(ctx)[0] || null, mode: 'smooth', radius: 5, strength: 0.5, strokes: [] }),
    toParams: v => ({ body: v.body, strokes: v.strokes }),
    fromParams: p => ({ mode: 'smooth', radius: 5, strength: 0.5, ...p }),
    sculpt: true,
    hint: 'Drag across the model to brush it · right-drag to orbit · works best on finely meshed bodies (use Remesh first on blocky parts)',
  },
  remesh: {
    label: 'Remesh', icon: 'remesh', type: 'remesh',
    fields: [{ key: 'body', label: 'Body', kind: 'bodies', multi: false }, { key: 'edge', label: 'Triangle size', kind: 'number', unit: 'mm' }],
    defaults: ctx => ({ body: selBody(ctx).slice(0, 1), edge: 1 }),
    toParams: v => ({ body: v.body[0], edge: v.edge }),
    fromParams: p => ({ ...p, body: p.body ? [p.body] : [] }),
    noLivePreview: true,
    hint: 'Rebuilds the surface with even triangles of this size, so brushes and smoothing work everywhere',
  },
  repair: {
    label: 'Repair', icon: 'repair', type: 'repair',
    fields: [
      { key: 'body', label: 'Body', kind: 'bodies', multi: false },
      { key: 'mode', label: 'Method', kind: 'select', options: [['auto', 'Auto'], ['quick', 'Quick fix'], ['rebuild', 'Rebuild']] },
      { key: 'resolution', label: 'Rebuild detail', kind: 'select', options: [['draft', 'Low'], ['normal', 'Medium'], ['fine', 'High']], show: v => v.mode !== 'quick' },
    ],
    defaults: ctx => ({ body: selBody(ctx).slice(0, 1).length ? selBody(ctx).slice(0, 1) : ctx.state.bodies.filter(b => !b.solid).map(b => b.id).slice(0, 1), mode: 'auto', resolution: 'normal' }),
    toParams: v => ({ ...v, body: v.body[0] }),
    fromParams: p => ({ ...p, body: p.body ? [p.body] : [] }),
    noLivePreview: true,
    hint: 'Quick fix closes small holes and flips bad triangles. Rebuild remakes the mesh as one watertight solid',
  },
  simplify: {
    label: 'Simplify', icon: 'simplify', type: 'simplify',
    fields: [{ key: 'body', label: 'Body', kind: 'bodies', multi: false }, { key: 'percent', label: 'Keep', kind: 'number', unit: '%' }],
    defaults: ctx => ({ body: selBody(ctx).slice(0, 1), percent: 50 }),
    toParams: v => ({ body: v.body[0], ratio: Math.max(0.1, Math.min(100, v.percent)) / 100 }),
    fromParams: p => ({ body: p.body ? [p.body] : [], percent: Math.round((p.ratio || 0.5) * 100) }),
    noLivePreview: true,
    hint: 'Reduces the triangle count while keeping the shape',
  },
  separate: {
    label: 'Separate', icon: 'separate', type: 'separate',
    fields: [{ key: 'body', label: 'Body', kind: 'bodies', multi: false }],
    defaults: ctx => ({ body: selBody(ctx).slice(0, 1) }),
    toParams: v => ({ body: v.body[0] }),
    fromParams: p => ({ body: p.body ? [p.body] : [] }),
    hint: 'Splits a body made of disconnected pieces into one body per piece',
  },
  delete: {
    label: 'Remove Bodies', icon: 'delete', type: 'delete',
    fields: [{ key: 'bodies', label: 'Bodies', kind: 'bodies', multi: true }],
    defaults: ctx => ({ bodies: selBody(ctx) }),
  },
};

function primitive(shape, label, icon, dims, dimDefaults) {
  return {
    label, icon, type: 'primitive',
    fields: [
      ...dims,
      { key: 'plane', label: 'On', kind: 'plane' },
      { key: 'offset', label: 'Position', kind: 'vec3', unit: 'mm' },
      { key: 'rotate', label: 'Rotate', kind: 'vec3', unit: '°' },
      { key: 'anchor', label: 'Anchor', kind: 'select', options: [['base', 'Base'], ['center', 'Centre'], ...(shape === 'box' ? [['corner', 'Corner']] : [])] },
      { key: 'operation', label: 'Operation', kind: 'select', options: OPS },
      { key: 'targets', label: 'Bodies', kind: 'bodies', multi: true, placeholder: 'Auto', show: v => v.operation !== 'new' },
    ],
    defaults: ctx => ({ shape, ...dimDefaults, plane: 'XY', offset: [0, 0, 0], rotate: [0, 0, 0], anchor: 'base', operation: ctx.state.bodies.length ? 'new' : 'new', targets: [], body_name: label }),
    hint: 'Click a flat face to place it there, or set the position · use Cut to subtract it',
    placeOnFace: true,
  };
}
