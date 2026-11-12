//! Faceted STEP (ISO 10303-21, AP214), a port of `web/engine/step.js` `stepBytes`: every
//! triangle is one planar ADVANCED_FACE; vertices and edges are shared. All arithmetic is f64
//! like the JavaScript (the f32 coordinates are widened first).


use super::jsfmt::{js_hypot3, js_number_string};
use super::{ExportBody, ExportError, ExportOptions};

/// A STEP REAL literal: always has a decimal point ("1.", "1.E-7").
fn real(x: f64) -> String {
    if !x.is_finite() || x == 0.0 {
        return "0.".into();
    }
    let s = js_number_string(x);
    if let Some((m, e)) = s.split_once('e') {
        let m = if m.contains('.') { m.to_string() } else { format!("{m}.") };
        let neg = e.starts_with('-');
        let digits = e.trim_start_matches(['+', '-']);
        format!("{m}E{}{digits}", if neg { "-" } else { "" })
    } else if s.contains('.') {
        s
    } else {
        format!("{s}.")
    }
}

/// A STEP string: ASCII only (each UTF-16 unit outside 0x20..=0x7e becomes `?`), apostrophes doubled.
fn str_lit(s: &str) -> String {
    let mut out = String::from("'");
    for u in s.encode_utf16() {
        match u {
            0x27 => out.push_str("''"),
            0x20..=0x7e => out.push(u as u8 as char),
            _ => out.push('?'),
        }
    }
    out.push('\'');
    out
}

type V3 = [f64; 3];

fn vec(p: V3, q: V3) -> V3 {
    [q[0] - p[0], q[1] - p[1], q[2] - p[2]]
}
fn cross(p: V3, q: V3) -> V3 {
    [p[1] * q[2] - p[2] * q[1], p[2] * q[0] - p[0] * q[2], p[0] * q[1] - p[1] * q[0]]
}
fn unit(p: V3) -> V3 {
    let l = js_hypot3(p);
    [p[0] / l, p[1] / l, p[2] / l]
}
/// Appends `real(x)` to `out`.
fn real_into(out: &mut Vec<u8>, x: f64) {
    out.extend_from_slice(real(x).as_bytes());
}
fn list_into(out: &mut Vec<u8>, v: V3) {
    real_into(out, v[0]);
    out.push(b',');
    real_into(out, v[1]);
    out.push(b',');
    real_into(out, v[2]);
}
fn push_u32(out: &mut Vec<u8>, mut n: u32) {
    let mut buf = [0u8; 10];
    let mut i = buf.len();
    loop {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
        if n == 0 {
            break;
        }
    }
    out.extend_from_slice(&buf[i..]);
}
fn push_ref(out: &mut Vec<u8>, id: u32) {
    out.push(b'#');
    push_u32(out, id);
}

/// Entity writer: numbers entities from 1 and appends each straight to one byte buffer.
struct Lines {
    out: Vec<u8>,
    n: u32,
}

impl Lines {
    /// Starts a new entity (`#id=`) and returns its id; the caller writes the body then `end()`.
    fn begin(&mut self) -> u32 {
        self.n += 1;
        push_ref(&mut self.out, self.n);
        self.out.push(b'=');
        self.n
    }
    fn end(&mut self) {
        self.out.extend_from_slice(b";
");
    }
    fn add(&mut self, body: &str) -> u32 {
        let id = self.begin();
        self.out.extend_from_slice(body.as_bytes());
        self.end();
        id
    }
    fn s(&mut self, t: &str) {
        self.out.extend_from_slice(t.as_bytes());
    }
    fn r(&mut self, id: u32) {
        push_ref(&mut self.out, id);
    }
    /// `DIRECTION('',(x,y,z))`
    fn direction(&mut self, v: V3) -> u32 {
        let id = self.begin();
        self.s("DIRECTION('',(");
        list_into(&mut self.out, v);
        self.s("))");
        self.end();
        id
    }
}

const EMPTY: u64 = u64::MAX;

/// Open-addressing table from an edge key (lo<<32 | hi) to (use count, forward uses, edge id).
struct EdgeTable {
    keys: Vec<u64>,
    vals: Vec<(u32, u32, u32)>,
    mask: usize,
}

impl EdgeTable {
    fn new(expected: usize) -> Self {
        let cap = (expected * 2).max(16).next_power_of_two();
        EdgeTable { keys: vec![EMPTY; cap], vals: vec![(0, 0, 0); cap], mask: cap - 1 }
    }
    fn slot(&self, key: u64) -> usize {
        let mut i = (key.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 20) as usize & self.mask;
        while self.keys[i] != EMPTY && self.keys[i] != key {
            i = (i + 1) & self.mask;
        }
        i
    }
    fn entry(&mut self, key: u64) -> &mut (u32, u32, u32) {
        let i = self.slot(key);
        self.keys[i] = key;
        &mut self.vals[i]
    }
    fn find(&self, key: u64) -> Option<(u32, u32, u32)> {
        let i = self.slot(key);
        (self.keys[i] == key).then(|| self.vals[i])
    }
}

pub(super) fn write(bodies: &[ExportBody], opts: &ExportOptions) -> Result<Vec<u8>, ExportError> {
    let title: &str = if opts.step_title.is_empty() { "model" } else { &opts.step_title };
    let date: String = opts.step_date.chars().take(19).collect();
    let tris: usize = bodies.iter().map(|b| b.mesh.triangles.len() / 3).sum();
    let mut l = Lines { out: Vec::with_capacity(1024 + tris * 950), n: 0 };

    l.s("ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION(('faceted STEP: each triangle is a flat face'),'2;1');\n");
    l.s(&format!(
        "FILE_NAME({},{},(''),(''),'STL Editor','STL Editor','');\n",
        str_lit(&format!("{title}.step")),
        str_lit(&date)
    ));
    l.s("FILE_SCHEMA(('AUTOMOTIVE_DESIGN { 1 0 10303 214 1 1 1 1 }'));\nENDSEC;\nDATA;\n");

    let actx = l.add("APPLICATION_CONTEXT('core data for automotive mechanical design processes')");
    l.add(&format!("APPLICATION_PROTOCOL_DEFINITION('international standard','automotive_design',2001,#{actx})"));
    let pctx = l.add(&format!("PRODUCT_CONTEXT('',#{actx},'mechanical')"));
    let prod = l.add(&format!("PRODUCT({},{},'',(#{pctx}))", str_lit(title), str_lit(title)));
    let pdf = l.add(&format!("PRODUCT_DEFINITION_FORMATION('','',#{prod})"));
    let pdc = l.add(&format!("PRODUCT_DEFINITION_CONTEXT('part definition',#{actx},'design')"));
    let pd = l.add(&format!("PRODUCT_DEFINITION('design','',#{pdf},#{pdc})"));
    let pds = l.add(&format!("PRODUCT_DEFINITION_SHAPE('','',#{pd})"));

    let origin = l.add("CARTESIAN_POINT('',(0.,0.,0.))");
    let zdir = l.add("DIRECTION('',(0.,0.,1.))");
    let xdir = l.add("DIRECTION('',(1.,0.,0.))");
    let place0 = l.add(&format!("AXIS2_PLACEMENT_3D('',#{origin},#{zdir},#{xdir})"));
    let lu = l.add("( LENGTH_UNIT() NAMED_UNIT(*) SI_UNIT(.MILLI.,.METRE.) )");
    let au = l.add("( NAMED_UNIT(*) PLANE_ANGLE_UNIT() SI_UNIT($,.RADIAN.) )");
    let su = l.add("( NAMED_UNIT(*) SI_UNIT($,.STERADIAN.) SOLID_ANGLE_UNIT() )");
    let unc = l.add(&format!("UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE(1.E-06),#{lu},'distance_accuracy_value','')"));
    let ctx = l.add(&format!("( GEOMETRIC_REPRESENTATION_CONTEXT(3) GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT((#{unc})) GLOBAL_UNIT_ASSIGNED_CONTEXT((#{lu},#{au},#{su})) REPRESENTATION_CONTEXT('Context3D','3D') )"));

    let mut breps: Vec<u32> = Vec::new();
    for body in bodies {
        let label: &str = if body.name.is_empty() { title } else { &body.name };
        let pos = &body.mesh.positions;
        let f = &body.mesh.triangles;
        let nt = f.len() / 3;
        let p = |i: u32| -> V3 {
            let i = i as usize * 3;
            [pos[i] as f64, pos[i + 1] as f64, pos[i + 2] as f64]
        };

        // check first: every edge used by exactly two faces, once in each direction; no zero-area faces
        let mut uses = EdgeTable::new(nt * 3 / 2 + 1);
        for t in 0..nt {
            let tri = [f[3 * t], f[3 * t + 1], f[3 * t + 2]];
            let c = cross(vec(p(tri[0]), p(tri[1])), vec(p(tri[0]), p(tri[2])));
            let distinct = tri[0] != tri[1] && tri[0] != tri[2] && tri[1] != tri[2];
            if !(js_hypot3(c) > 0.0) || !distinct {
                return Err(ExportError::Geometry(format!(
                    "{label} has faces with no area. Run Repair on it first, then export."
                )));
            }
            for k in 0..3 {
                let (a, b) = (tri[k], tri[(k + 1) % 3]);
                let u = uses.entry((a.min(b) as u64) << 32 | a.max(b) as u64);
                u.0 += 1;
                if a < b {
                    u.1 += 1;
                }
            }
        }
        if uses.keys.iter().zip(&uses.vals).any(|(&k, u)| k != EMPTY && (u.0 != 2 || u.1 != 1)) {
            return Err(ExportError::Geometry(format!(
                "{label} isn't a closed solid (it has holes, or faces that overlap or face the wrong way). Run Repair on it first, then export."
            )));
        }

        // reuse the table for edge entity ids (counts are all checked; id 0 = not written yet)
        for v in uses.vals.iter_mut() {
            *v = (0, 0, 0);
        }
        let mut ecs = uses;
        let nv = pos.len() / 3;
        let mut cps = vec![0u32; nv];
        let mut vps = vec![0u32; nv];
        let mut faces: Vec<u32> = Vec::with_capacity(nt);
        for t in 0..nt {
            let (a, b, c) = (f[3 * t], f[3 * t + 1], f[3 * t + 2]);
            let ab = vec(p(a), p(b));
            let ac = vec(p(a), p(c));
            let nrm = unit(cross(ab, ac));
            let xd = unit(ab);
            let dn = l.direction(nrm);
            let dx = l.direction(xd);
            let cpa = cp(&mut l, &mut cps, p(a), a);
            let ax = l.begin();
            l.s("AXIS2_PLACEMENT_3D('',");
            l.r(cpa);
            l.s(",");
            l.r(dn);
            l.s(",");
            l.r(dx);
            l.s(")");
            l.end();
            let pl = l.begin();
            l.s("PLANE('',");
            l.r(ax);
            l.s(")");
            l.end();
            let mut oes = [0u32; 3];
            for (k, (pi, qi)) in [(a, b), (b, c), (c, a)].into_iter().enumerate() {
                let (lo, hi) = (pi.min(qi), pi.max(qi));
                let key = (lo as u64) << 32 | hi as u64;
                let mut edge = ecs.find(key).map_or(0, |v| v.2);
                if edge == 0 {
                    let d = vec(p(lo), p(hi));
                    let len = js_hypot3(d);
                    let u = [d[0] / len, d[1] / len, d[2] / len];
                    let dir = l.direction(u);
                    let vc = l.begin();
                    l.s("VECTOR('',");
                    l.r(dir);
                    l.s(",");
                    real_into(&mut l.out, len);
                    l.s(")");
                    l.end();
                    let cplo = cp(&mut l, &mut cps, p(lo), lo);
                    let line = l.begin();
                    l.s("LINE('',");
                    l.r(cplo);
                    l.s(",");
                    l.r(vc);
                    l.s(")");
                    l.end();
                    let vlo = vp(&mut l, &mut cps, &mut vps, p(lo), lo);
                    let vhi = vp(&mut l, &mut cps, &mut vps, p(hi), hi);
                    edge = l.begin();
                    l.s("EDGE_CURVE('',");
                    l.r(vlo);
                    l.s(",");
                    l.r(vhi);
                    l.s(",");
                    l.r(line);
                    l.s(",.T.)");
                    l.end();
                    ecs.entry(key).2 = edge;
                }
                oes[k] = l.begin();
                l.s("ORIENTED_EDGE('',*,*,");
                l.r(edge);
                l.s(if pi < qi { ",.T.)" } else { ",.F.)" });
                l.end();
            }
            let loop_ = l.begin();
            l.s("EDGE_LOOP('',(");
            for (k, &o) in oes.iter().enumerate() {
                if k > 0 {
                    l.s(",");
                }
                l.r(o);
            }
            l.s("))");
            l.end();
            let bound = l.begin();
            l.s("FACE_OUTER_BOUND('',");
            l.r(loop_);
            l.s(",.T.)");
            l.end();
            let face = l.begin();
            l.s("ADVANCED_FACE('',(");
            l.r(bound);
            l.s("),");
            l.r(pl);
            l.s(",.T.)");
            l.end();
            faces.push(face);
        }
        let shell = l.begin();
        l.s("CLOSED_SHELL('',(");
        for (k, &fc) in faces.iter().enumerate() {
            if k > 0 {
                l.s(",");
            }
            l.r(fc);
        }
        l.s("))");
        l.end();
        let brep = l.begin();
        l.s("MANIFOLD_SOLID_BREP(");
        l.s(&str_lit(label));
        l.s(",");
        l.r(shell);
        l.s(")");
        l.end();
        breps.push(brep);
    }

    let mut members = format!("#{place0}");
    for b in breps {
        members.push_str(&format!(",#{b}"));
    }
    let rep = l.add(&format!("ADVANCED_BREP_SHAPE_REPRESENTATION({},({}),#{ctx})", str_lit(title), members));
    l.add(&format!("SHAPE_DEFINITION_REPRESENTATION(#{pds},#{rep})"));
    l.s("ENDSEC;\nEND-ISO-10303-21;\n");
    Ok(l.out)
}

/// The shared CARTESIAN_POINT of vertex `i` (written on first use).
fn cp(l: &mut Lines, cps: &mut [u32], pt: V3, i: u32) -> u32 {
    let c = cps[i as usize];
    if c != 0 {
        return c;
    }
    let id = l.begin();
    l.s("CARTESIAN_POINT('',(");
    list_into(&mut l.out, pt);
    l.s("))");
    l.end();
    cps[i as usize] = id;
    id
}

/// The shared VERTEX_POINT of vertex `i` (written on first use, after its point).
fn vp(l: &mut Lines, cps: &mut [u32], vps: &mut [u32], pt: V3, i: u32) -> u32 {
    let v = vps[i as usize];
    if v != 0 {
        return v;
    }
    let c = cp(l, cps, pt, i);
    let id = l.begin();
    l.s("VERTEX_POINT('',");
    l.r(c);
    l.s(")");
    l.end();
    vps[i as usize] = id;
    id
}
