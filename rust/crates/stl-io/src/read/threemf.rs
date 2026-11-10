//! 3MF reader. Port of trimesh 5.2.0 `exchange/threemf.py` `load_3MF` + `Scene.dump()`.
//!
//! One mesh per `<object>`; build items and `<component>`s form a graph from a "world" node.
//! Every root-to-leaf path is one instance (transforms multiplied along the path), visited in
//! trimesh's `multigraph_paths` order, which is last-in-first-out for sibling branches.

use std::collections::HashMap;
use std::io::{Cursor, Read};

use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;

use super::{Part, ReadError};

type Mat = [[f64; 4]; 4];

const IDENTITY: Mat = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

fn bad() -> ReadError {
    ReadError("This 3MF file couldn't be read.".into())
}

#[derive(Default)]
struct Mesh {
    v: Vec<f64>,
    f: Vec<i64>,
}

struct Comp {
    objectid: String,
    transform: Option<String>,
    path: Option<String>,
}

#[derive(Default)]
struct Obj {
    id: String,
    meshes: Vec<Mesh>,
    comps: Vec<Comp>,
}

struct Item {
    objectid: String,
    transform: Option<String>,
    partnumber: Option<String>,
}

#[derive(Default)]
struct Model {
    objects: Vec<Obj>,
    /// meshes found outside any object (only used for component files)
    loose: Vec<Mesh>,
    items: Vec<Item>,
}

fn local(name: &[u8]) -> &[u8] {
    match name.iter().rposition(|&b| b == b':') {
        Some(i) => &name[i + 1..],
        None => name,
    }
}

fn attrs(e: &BytesStart) -> Result<HashMap<String, String>, ReadError> {
    let mut m = HashMap::new();
    for a in e.attributes() {
        let a = a.map_err(|_| bad())?;
        let k = String::from_utf8_lossy(local(a.key.as_ref())).into_owned();
        let v = a.unescape_value().map_err(|_| bad())?.into_owned();
        // first wins for same local name (xmlns variants are not expected)
        m.entry(k).or_insert(v);
    }
    Ok(m)
}

fn num(a: &HashMap<String, String>, k: &str) -> Result<f64, ReadError> {
    a.get(k).and_then(|s| s.trim().parse::<f64>().ok()).ok_or_else(bad)
}

fn idx(a: &HashMap<String, String>, k: &str) -> Result<i64, ReadError> {
    a.get(k).and_then(|s| s.trim().parse::<i64>().ok()).ok_or_else(bad)
}

fn parse_model(data: &[u8]) -> Result<Model, ReadError> {
    let mut rd = Reader::from_reader(data);
    let mut buf = Vec::new();
    let mut m = Model::default();
    let mut cur: Option<Obj> = None;
    let mut mesh: Option<Mesh> = None;
    let mut in_build = false;
    loop {
        let ev = rd.read_event_into(&mut buf).map_err(|_| bad())?;
        let (e, is_start, is_empty) = match &ev {
            Event::Start(e) => (Some(e), true, false),
            Event::Empty(e) => (Some(e), false, true),
            Event::End(e) => {
                match local(e.name().as_ref()) {
                    b"mesh" => {
                        if let Some(ms) = mesh.take() {
                            match cur.as_mut() {
                                Some(o) => o.meshes.push(ms),
                                None => m.loose.push(ms),
                            }
                        }
                    }
                    b"object" => {
                        if let Some(o) = cur.take() {
                            m.objects.push(o);
                        }
                    }
                    b"build" => in_build = false,
                    _ => {}
                }
                (None, false, false)
            }
            Event::Eof => break,
            _ => (None, false, false),
        };
        if let Some(e) = e {
            let name = local(e.name().as_ref()).to_vec();
            match name.as_slice() {
                b"object" => {
                    let a = attrs(e)?;
                    let id = a.get("id").cloned().ok_or_else(bad)?;
                    let o = Obj { id, ..Default::default() };
                    if is_empty {
                        m.objects.push(o);
                    } else if is_start {
                        cur = Some(o);
                    }
                }
                b"mesh" => {
                    if is_empty {
                        match cur.as_mut() {
                            Some(o) => o.meshes.push(Mesh::default()),
                            None => m.loose.push(Mesh::default()),
                        }
                    } else {
                        mesh = Some(Mesh::default());
                    }
                }
                b"vertex" => {
                    if let Some(ms) = mesh.as_mut() {
                        let a = attrs(e)?;
                        ms.v.extend([num(&a, "x")?, num(&a, "y")?, num(&a, "z")?]);
                    }
                }
                b"triangle" => {
                    if let Some(ms) = mesh.as_mut() {
                        let a = attrs(e)?;
                        ms.f.extend([idx(&a, "v1")?, idx(&a, "v2")?, idx(&a, "v3")?]);
                    }
                }
                b"component" => {
                    if let Some(o) = cur.as_mut() {
                        let a = attrs(e)?;
                        // `path` may be namespaced (p:path); `attrs` keeps the local name
                        o.comps.push(Comp {
                            objectid: a.get("objectid").cloned().ok_or_else(bad)?,
                            transform: a.get("transform").cloned(),
                            path: a.get("path").cloned(),
                        });
                    }
                }
                b"build" => in_build = is_start,
                b"item" if in_build => {
                    let a = attrs(e)?;
                    m.items.push(Item {
                        objectid: a.get("objectid").cloned().ok_or_else(bad)?,
                        transform: a.get("transform").cloned(),
                        partnumber: a.get("partnumber").cloned(),
                    });
                }
                _ => {}
            }
        }
        buf.clear();
    }
    Ok(m)
}

/// `_attrib_to_transform`: 12 numbers, reshape (4, 3), transpose.
fn transform(s: &Option<String>) -> Result<Mat, ReadError> {
    let mut t = IDENTITY;
    if let Some(s) = s {
        let vals: Vec<f64> = s
            .split_whitespace()
            .map(|x| x.parse::<f64>().map_err(|_| bad()))
            .collect::<Result<_, _>>()?;
        if vals.len() != 12 {
            return Err(bad());
        }
        for r in 0..4 {
            for c in 0..3 {
                t[c][r] = vals[r * 3 + c];
            }
        }
    }
    Ok(t)
}

fn mul(a: &Mat, b: &Mat) -> Mat {
    let mut o = [[0.0; 4]; 4];
    for i in 0..4 {
        for j in 0..4 {
            o[i][j] = (0..4).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    o
}

/// `util.append_faces` for the meshes of one object.
fn merge(meshes: &[Mesh]) -> Result<Mesh, ReadError> {
    let mut out = Mesh::default();
    for m in meshes {
        let off = (out.v.len() / 3) as i64;
        out.v.extend_from_slice(&m.v);
        out.f.extend(m.f.iter().map(|&i| i + off));
    }
    let n = (out.v.len() / 3) as i64;
    if out.f.iter().any(|&i| i < 0 || i >= n) {
        return Err(bad());
    }
    Ok(out)
}

#[derive(Clone, PartialEq, Eq)]
enum Key {
    Int(usize),
    Str(String),
}

/// A networkx MultiDiGraph with insertion-ordered adjacency.
#[derive(Default)]
struct Graph {
    nodes: Vec<String>,
    /// per node: ordered neighbours, each with ordered (key, matrix) edges
    adj: Vec<Vec<(usize, Vec<(Key, Mat)>)>>,
    edge_count: usize,
}

impl Graph {
    fn node(&mut self, name: &str) -> usize {
        if let Some(i) = self.nodes.iter().position(|n| n == name) {
            return i;
        }
        self.nodes.push(name.to_string());
        self.adj.push(Vec::new());
        self.nodes.len() - 1
    }

    fn add_edge(&mut self, u: &str, v: &str, key: Option<String>, m: Mat) {
        let ui = self.node(u);
        let vi = self.node(v);
        let slot = match self.adj[ui].iter().position(|(n, _)| *n == vi) {
            Some(p) => p,
            None => {
                self.adj[ui].push((vi, Vec::new()));
                self.adj[ui].len() - 1
            }
        };
        let edges = &mut self.adj[ui][slot].1;
        let key = match key {
            Some(s) => Key::Str(s),
            None => {
                let mut n = edges.len();
                while edges.iter().any(|(k, _)| *k == Key::Int(n)) {
                    n += 1;
                }
                Key::Int(n)
            }
        };
        if let Some(e) = edges.iter_mut().find(|(k, _)| *k == key) {
            e.1 = m;
        } else {
            edges.push((key, m));
            self.edge_count += 1;
        }
    }
}

/// trimesh `graph.multigraph_paths`: paths as (node, edge index within the pair) lists.
fn paths(g: &Graph, source: usize) -> Vec<Vec<(usize, usize)>> {
    let cutoff = g.edge_count * g.nodes.len() + 1;
    let mut current = vec![(source, 0usize)];
    let mut queue: Vec<Vec<(usize, usize)>> = Vec::new();
    let mut out = Vec::new();
    for _ in 0..cutoff {
        let node = current.last().unwrap().0;
        let child = &g.adj[node];
        if child.is_empty() {
            out.push(current.clone());
            match queue.pop() {
                Some(q) => current = q,
                None => break,
            }
        } else {
            let mut start = true;
            for (nb, edges) in child {
                for inst in 0..edges.len() {
                    if start {
                        current.push((*nb, inst));
                        start = false;
                    } else {
                        let mut q = current[..current.len() - 1].to_vec();
                        q.push((*nb, inst));
                        queue.push(q);
                    }
                }
            }
        }
    }
    out
}

/// See the contract in `read/mod.rs`.
pub fn read(data: &[u8], base: &str) -> Result<Vec<Part>, ReadError> {
    let mut zip = zip::ZipArchive::new(Cursor::new(data)).map_err(|_| bad())?;
    let mut files: Vec<(String, usize)> = Vec::new();
    for i in 0..zip.len() {
        let f = zip.by_index(i).map_err(|_| bad())?;
        files.push((f.name().map(|n| n.into_owned()).unwrap_or_default(), i));
    }
    let mut read_entry = |i: usize| -> Result<Vec<u8>, ReadError> {
        let mut f = zip.by_index(i).map_err(|_| bad())?;
        let mut b = Vec::new();
        f.read_to_end(&mut b).map_err(|_| bad())?;
        Ok(b)
    };
    let main = files
        .iter()
        .find(|(n, _)| n.to_lowercase().contains("3d/3dmodel.model"))
        .ok_or_else(bad)?
        .1;
    let model = parse_model(&read_entry(main)?)?;

    // geometry per object id, components, graph
    let mut ids: Vec<String> = Vec::new(); // objects defined (id_name)
    let mut v_seq: HashMap<String, Vec<Mesh>> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    let mut comp_edges: Vec<(String, String, Mat)> = Vec::new();
    for obj in model.objects {
        ids.push(obj.id.clone());
        if !obj.meshes.is_empty() {
            if !v_seq.contains_key(&obj.id) {
                order.push(obj.id.clone());
            }
            v_seq.entry(obj.id.clone()).or_default().extend(obj.meshes);
        }
        for c in obj.comps {
            let t = transform(&c.transform)?;
            if let Some(p) = c
                .path
                .as_ref()
                .map(|p| p.trim_matches('/').to_string())
                .and_then(|p| files.iter().find(|(n, _)| *n == p).map(|(_, i)| *i))
            {
                ids.push(c.objectid.clone());
                let sub = parse_model(&read_entry(p)?)?;
                let all = sub.objects.into_iter().flat_map(|o| o.meshes).chain(sub.loose);
                for ms in all {
                    if !v_seq.contains_key(&c.objectid) {
                        order.push(c.objectid.clone());
                    }
                    v_seq.entry(c.objectid.clone()).or_default().push(ms);
                }
            }
            comp_edges.push((obj.id.clone(), c.objectid, t));
        }
    }
    // graph insertion order in trimesh: build items first, then components
    let mut g = Graph::default();
    for it in &model.items {
        g.add_edge("\u{0}world", &it.objectid, it.partnumber.clone(), transform(&it.transform)?);
    }
    for (u, v, m) in &comp_edges {
        g.add_edge(u, v, None, *m);
    }

    let Some(world) = g.nodes.iter().position(|n| n == "\u{0}world") else {
        return Ok(Vec::new());
    };
    let merged: HashMap<&String, Mesh> = {
        let mut h = HashMap::new();
        for id in &order {
            h.insert(id, merge(&v_seq[id])?);
        }
        h
    };

    // (vertices, faces) per instance, in dump order
    let mut dump: Vec<(Vec<f64>, Vec<u32>)> = Vec::new();
    for path in paths(&g, world) {
        let mut tf = IDENTITY;
        let mut first = true;
        for w in path.windows(2) {
            let m = &g.adj[w[0].0].iter().find(|(n, _)| *n == w[1].0).ok_or_else(bad)?.1[w[1].1].1;
            tf = if first { *m } else { mul(&tf, m) };
            first = false;
        }
        let last = &g.nodes[path.last().unwrap().0];
        if !ids.contains(last) {
            continue; // included but never defined
        }
        let Some(mesh) = merged.get(last) else {
            return Err(bad());
        };
        let mut v = Vec::with_capacity(mesh.v.len());
        for p in mesh.v.chunks_exact(3) {
            for r in 0..3 {
                v.push(p[0] * tf[r][0] + p[1] * tf[r][1] + p[2] * tf[r][2] + tf[r][3]);
            }
        }
        dump.push((v, mesh.f.iter().map(|&i| i as u32).collect()));
    }

    let n = dump.len();
    let mut parts = Vec::new();
    for (i, (v, f)) in dump.into_iter().enumerate() {
        if !f.is_empty() {
            let name = if n == 1 { base.to_string() } else { format!("{base} {}", i + 1) };
            parts.push(Part { name, positions: v, triangles: f });
        }
    }
    Ok(parts)
}
