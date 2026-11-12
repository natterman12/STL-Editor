//! `fins`, `match_removed` and `_solids` from 1.x `supportfins.py`.

use std::collections::BTreeSet;
use std::sync::Arc;

use serde_json::Value;
use stl_kernel::{MeshArrays, Solid};

use super::{
    call, call_json, caches, decode_triangles, mesh_key, options, parse_json, soup, truthy, Cached, MeshData, P3,
};
use crate::{FeatureError, FinsHost};

/// The engine's own answer to `computeFinsB64`, plus the report line.
pub struct RawFins {
    /// The fin triangles in the seated frame.
    pub tri: Vec<[[f32; 3]; 3]>,
    pub offset: P3,
    /// Per piece: its kind and the triangle ranges it is made of.
    pub pieces: Vec<(String, Vec<(usize, usize)>)>,
    pub stats: Value,
    pub report: String,
    pub over: Value,
    pub small: Value,
}

/// Where a piece is: the middle of its box and its size (how a removed fin is found again).
#[derive(Clone, Debug, PartialEq)]
pub struct Piece {
    pub kind: String,
    pub c: P3,
    pub size: P3,
}

pub struct FinsResult {
    pub solids: Vec<Solid>,
    pub pieces: Vec<Piece>,
    pub skipped: usize,
    pub stats: Value,
    pub report: String,
    pub over: Value,
    pub small: Value,
    pub version: String,
}

/// Python `round(x, 3)`.
pub fn round3(x: f64) -> f64 {
    format!("{x:.3}").parse().unwrap_or(x)
}

fn where_of(v: &[P3]) -> (P3, P3) {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for p in v {
        for a in 0..3 {
            lo[a] = lo[a].min(p[a]);
            hi[a] = hi[a].max(p[a]);
        }
    }
    (
        [round3((lo[0] + hi[0]) / 2.0), round3((lo[1] + hi[1]) / 2.0), round3((lo[2] + hi[2]) / 2.0)],
        [round3(hi[0] - lo[0]), round3(hi[1] - lo[1]), round3(hi[2] - lo[2])],
    )
}

/// `match_removed`: which fins a list of removed ones means in this build, and how many of them
/// aren't there any more.
pub fn match_removed(pieces: &[Piece], removed: Option<&Value>) -> (BTreeSet<usize>, usize) {
    let mut gone = BTreeSet::new();
    let mut missing = 0;
    for r in removed.and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default() {
        let rkind = r.get("kind").and_then(Value::as_str).filter(|s| !s.is_empty());
        let rc = super::floats(r.get("c"));
        let rsize = super::floats(r.get("size"));
        let mut best: Option<usize> = None;
        let mut bd = f64::INFINITY;
        for (i, p) in pieces.iter().enumerate() {
            if gone.contains(&i) || rkind.is_some_and(|k| p.kind != k) {
                continue;
            }
            let d = p.c.iter().zip(&rc).map(|(a, b)| (a - b) * (a - b)).sum::<f64>().sqrt();
            let ds = if truthy(r.get("size")) {
                p.size.iter().zip(&rsize).map(|(a, b)| (a - b).abs()).fold(0.0, f64::max)
            } else {
                0.0
            };
            if d <= 1.0 && ds <= 2.0 && d < bd {
                bd = d;
                best = Some(i);
            }
        }
        match best {
            Some(i) => {
                gone.insert(i);
            }
            None => missing += 1,
        }
    }
    (gone, missing)
}

/// `manifold_from_arrays`: a solid, or `None` when the mesh isn't a closed 2-manifold.
fn solid_from_arrays(v: &[f64], f: &[u32]) -> Option<Solid> {
    if f.is_empty() {
        return None;
    }
    let mesh = MeshArrays { positions: v.iter().map(|x| *x as f32).collect(), triangles: f.to_vec() };
    Solid::from_mesh(&mesh).ok().filter(|s| !s.is_empty())
}

/// trimesh `split(only_watertight=False)` with `repair`: the connected shells of a mesh (faces
/// joined across an edge that exactly two faces share), each with its holes filled when it has
/// four faces or more. Shells are ordered by their lowest face.
fn shells(v: &[f64], f: &[u32]) -> Vec<(Vec<f64>, Vec<u32>)> {
    use std::collections::HashMap;
    let nf = f.len() / 3;
    let mut edges: HashMap<(u32, u32), Vec<u32>> = HashMap::new();
    for t in 0..nf {
        for k in 0..3 {
            let (a, b) = (f[3 * t + k], f[3 * t + (k + 1) % 3]);
            edges.entry((a.min(b), a.max(b))).or_default().push(t as u32);
        }
    }
    let mut parent: Vec<u32> = (0..nf as u32).collect();
    fn find(p: &mut [u32], mut x: u32) -> u32 {
        while p[x as usize] != x {
            p[x as usize] = p[p[x as usize] as usize];
            x = p[x as usize];
        }
        x
    }
    for fs in edges.values() {
        if let [a, b] = fs[..] {
            let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
            if ra != rb {
                parent[ra.max(rb) as usize] = ra.min(rb);
            }
        }
    }
    let mut label: HashMap<u32, usize> = HashMap::new();
    let mut groups: Vec<Vec<u32>> = Vec::new();
    for t in 0..nf as u32 {
        let r = find(&mut parent, t);
        let next = groups.len();
        let l = *label.entry(r).or_insert(next);
        if l == next {
            groups.push(Vec::new());
        }
        groups[l].push(t);
    }
    groups
        .into_iter()
        .map(|g| {
            let mut used: Vec<u32> = g.iter().flat_map(|&t| f[3 * t as usize..3 * t as usize + 3].iter().copied()).collect();
            used.sort_unstable();
            used.dedup();
            let mut sv: Vec<f64> = used.iter().flat_map(|&i| v[3 * i as usize..3 * i as usize + 3].iter().copied()).collect();
            let mut sf: Vec<u32> = g
                .iter()
                .flat_map(|&t| f[3 * t as usize..3 * t as usize + 3].iter().map(|i| used.binary_search(i).unwrap_or(0) as u32))
                .collect();
            if sf.len() / 3 >= 4 {
                stl_mesh::fill_holes(&mut sv, &mut sf);
            }
            (sv, sf)
        })
        .collect()
}

/// `_solids`: fin triangles (seated frame) -> one closed solid per piece, in the part's frame.
/// `info` collects, per piece, where it is (`None` for one that didn't close).
pub fn solids(
    tri: &[[[f32; 3]; 3]],
    offset: P3,
    ranges: &[Vec<(usize, usize)>],
    mut info: Option<&mut Vec<Option<(P3, P3)>>>,
) -> (Vec<Solid>, usize) {
    let mut out = Vec::new();
    let mut skipped = 0;
    for rs in ranges {
        let mut pos: Vec<f64> = Vec::new();
        for &(a, b) in rs {
            for t in tri.get(a..b.min(tri.len())).unwrap_or_default() {
                for c in t {
                    for k in 0..3 {
                        pos.push(c[k] as f64 - offset[k]);
                    }
                }
            }
        }
        if pos.is_empty() {
            continue;
        }
        let idx: Vec<u32> = (0..(pos.len() / 3) as u32).collect();
        let (v, f) = stl_mesh::weld(&pos, &idx, 1e-5);
        let mut m = solid_from_arrays(&v, &f);
        if m.is_none() {
            let parts: Vec<Solid> = shells(&v, &f).iter().filter_map(|(sv, sf)| solid_from_arrays(sv, sf)).collect();
            if !parts.is_empty() {
                m = Some(Solid::union_all(&parts));
            }
        }
        match m.filter(|m| !m.is_empty()) {
            Some(m) => {
                out.push(m);
                if let Some(info) = info.as_deref_mut() {
                    let pts: Vec<P3> = v.chunks_exact(3).map(|p| [p[0], p[1], p[2]]).collect();
                    info.push(Some(where_of(&pts)));
                }
            }
            None => {
                skipped += 1;
                if let Some(info) = info.as_deref_mut() {
                    info.push(None);
                }
            }
        }
    }
    (out, skipped)
}

fn parse_raw(host: &dyn FinsHost, text: &str) -> Result<RawFins, FeatureError> {
    let r = parse_json(text)?;
    let stats = r.get("stats").cloned().unwrap_or(Value::Null);
    let report = call_json(host, "reportLine", &[&serde_json::to_string(&stats).unwrap_or_default()])?;
    let off = |k: &str| r["offset"].get(k).and_then(Value::as_f64).unwrap_or(0.0);
    let pieces = r["pieces"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|p| {
                    let ranges = p["ranges"]
                        .as_array()
                        .map(|rs| {
                            rs.iter()
                                .map(|x| (x[0].as_u64().unwrap_or(0) as usize, x[1].as_u64().unwrap_or(0) as usize))
                                .collect()
                        })
                        .unwrap_or_default();
                    (p["kind"].as_str().unwrap_or_default().to_string(), ranges)
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(RawFins {
        tri: decode_triangles(r["triangles"].as_str().unwrap_or_default())?,
        offset: [off("x"), off("y"), off("z")],
        pieces,
        stats,
        report,
        over: r.get("overFaces").cloned().unwrap_or(Value::Null),
        small: r.get("smallFaces").cloned().unwrap_or(Value::Null),
    })
}

/// `fins`: Support Fins under a posed body (z up, as it will print).
pub fn fins(host: &dyn FinsHost, mesh: &MeshData, values: &Value) -> Result<FinsResult, FeatureError> {
    let opts = options(host, values)?;
    let key = format!("{}|{}", mesh_key(mesh), opts);
    let cached = caches().lock().unwrap_or_else(|e| e.into_inner()).fins.get(&key);
    let raw: Arc<RawFins> = match cached {
        Some(Cached::Fins(r)) => r,
        _ => {
            let text = call(host, "computeFinsB64", &[&soup(mesh), &opts])?;
            let r = Arc::new(parse_raw(host, &text)?);
            caches().lock().unwrap_or_else(|e| e.into_inner()).fins.put(key, Cached::Fins(r.clone()));
            r
        }
    };
    let ranges: Vec<Vec<(usize, usize)>> = raw.pieces.iter().map(|p| p.1.clone()).collect();
    let mut info = Vec::new();
    let (solids, skipped) = solids(&raw.tri, raw.offset, &ranges, Some(&mut info));
    let pieces = raw
        .pieces
        .iter()
        .zip(&info)
        .filter_map(|(p, w)| w.map(|(c, size)| Piece { kind: p.0.clone(), c, size }))
        .collect();
    Ok(FinsResult {
        solids,
        pieces,
        skipped,
        stats: raw.stats.clone(),
        report: raw.report.clone(),
        over: raw.over.clone(),
        small: raw.small.clone(),
        version: super::version(host),
    })
}
