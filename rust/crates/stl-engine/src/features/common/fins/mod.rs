//! Support Fins on a [`FinsHost`]: the port of 1.x `stleditor/supportfins.py` (everything except
//! choosing and updating the engine, which the host does).
//!
//! * `mod.rs` (here): calling the host, the engine's info, the result caches and the mesh helpers.
//! * `results.rs`: `fins`, `match_removed`, the solids a result turns into.
//! * `walls.rs`: `walls`, `wall`, `arc_path`, `wall_sections`.
//! * `orient.rs`: `suggest`, `strength`, `nearest_face`.
//!
//! The engine file is `web/vendor/support-fins/engine.iife.js`; the calls are the same names and
//! string arguments 1.x passes to `ctx.call("SupportFinsEngine.<name>", ...)`. The host also
//! answers one extra name, `__info`, with the JSON of the active engine's `VERSION.json` fields
//! (`version`, `tag`, `commit`, `source`) and `pending`: the engine itself doesn't know them.

pub mod orient;
pub mod results;
pub mod walls;

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, OnceLock};

use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use stl_kernel::{MeshArrays, Solid};

use crate::state::Body;
use crate::{FeatureError, FinsHost, HostKind};

/// The integration contract (see `packaging/update_support_fins.py`).
pub const API: u32 = 1;
/// Support Fins' own fin green.
pub const SUPPORT_COLOR: &str = "#59d98e";
/// A removed fin's see-through ghost.
pub const GHOST_COLOR: &str = "#d64545";
/// The Boa notice, word for word (milestone 4 shows it in the panel).
pub const BOA_NOTICE: &str = "Support Fins is running in slow mode because this computer's built-in web engine isn't available.";
pub const BOA_NOTICE_WINDOWS: &str = " Install the Microsoft Edge WebView2 Runtime to make it fast.";

pub type P3 = [f64; 3];

/// Python's truthiness for a JSON value.
pub fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|x| x != 0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(o)) => !o.is_empty(),
    }
}

/// An engine error as 1.x shows it: `Support Fins: <first line>`.
pub fn engine_error(msg: &str) -> FeatureError {
    let msg = msg.trim();
    let msg = msg.strip_prefix("Error: ").unwrap_or(msg);
    if msg.starts_with("Support Fins") {
        return FeatureError(msg.lines().next().unwrap_or("Support Fins: error").to_string());
    }
    FeatureError(format!("Support Fins: {}", msg.lines().next().unwrap_or("error")))
}

/// `_call`: `SupportFinsEngine.<name>(...args)` with string arguments, which is how 1.x passes
/// almost all of them. The host takes every argument as JSON text, so each goes over quoted.
pub fn call(host: &dyn FinsHost, name: &str, args: &[&str]) -> Result<String, FeatureError> {
    let quoted: Vec<String> = args.iter().map(|a| serde_json::to_string(a).unwrap_or_default()).collect();
    call_json(host, name, &quoted.iter().map(String::as_str).collect::<Vec<_>>())
}

/// Like [`call`], for arguments that are already JSON text (`reportLine`'s stats object, the
/// numbers of `anchorFinB64` and `suggestPosesJson`).
pub fn call_json(host: &dyn FinsHost, name: &str, args: &[&str]) -> Result<String, FeatureError> {
    host.call(name, args).map_err(|e| engine_error(&e))
}

/// Put an [`Extras`] into the result under 1.x's key names.
pub fn apply_extras(res: &mut crate::FeatureResult, ex: Extras) {
    let preview = ex.pieces.is_some();
    if let Some(v) = ex.paint {
        res.extra.insert("paint".into(), v);
    }
    if let Some(v) = ex.walls {
        res.extra.insert("walls".into(), v);
    }
    if let Some(v) = ex.support {
        res.extra.insert("support".into(), v);
    }
    if let Some(v) = ex.pieces {
        res.extra.insert("pieces".into(), Value::Object(v));
    }
    if preview || !ex.colors.is_empty() {
        res.extra.insert("colors".into(), Value::Object(ex.colors));
    }
    if preview {
        res.extra.insert("ghosts".into(), Value::Array(ex.ghosts.into_iter().map(Value::String).collect()));
    }
}

/// `_has`: this engine has the extra `name`.
pub fn has(host: &dyn FinsHost, name: &str) -> bool {
    host.has(name)
}

/// The fields of the active engine's `VERSION.json` (empty when the host can't say).
pub fn engine_info(host: &dyn FinsHost) -> Map<String, Value> {
    match call_json(host, "__info", &[]) {
        Ok(s) => serde_json::from_str::<Value>(&s).ok().and_then(|v| v.as_object().cloned()).unwrap_or_default(),
        Err(_) => Map::new(),
    }
}

/// `version()`: the engine's release, as in "Support Fins 1.0.5: ...".
pub fn version(host: &dyn FinsHost) -> String {
    match engine_info(host).get("version") {
        Some(Value::String(s)) => s.clone(),
        Some(v) if !v.is_null() => v.to_string(),
        _ => "unknown".to_string(),
    }
}

/// `features()`: the extras this engine has (a release that broke one leaves it out).
pub fn features(host: &dyn FinsHost) -> Value {
    json!({
        "curves": has(host, "drawWallThroughB64"),
        "gussets": has(host, "gussetWallB64"),
        "anchors": has(host, "anchorFinB64"),
        "orient": has(host, "suggestPosesJson"),
    })
}

/// `info()`: the options schema, the engine's version and the extras, plus where it runs
/// (`"host": "webview" | "browser" | "boa"`) and, in Boa mode, the slow-mode notice.
pub fn info(host: &dyn FinsHost) -> Result<Value, FeatureError> {
    let schema: Value = serde_json::from_str(&call(host, "optionsSchemaJson", &[])?)
        .map_err(|e| FeatureError(format!("Support Fins: {e}")))?;
    let a = engine_info(host);
    let get = |k: &str| a.get(k).cloned().unwrap_or(Value::Null);
    let kind = host.host_kind();
    let mut out = Map::new();
    out.insert("schema".into(), schema);
    for k in ["version", "tag", "commit", "source", "pending"] {
        out.insert(k.into(), get(k));
    }
    out.insert("curves".into(), Value::Bool(has(host, "drawWallThroughB64")));
    out.insert("features".into(), features(host));
    out.insert("host".into(), Value::String(kind.as_str().to_string()));
    if kind == HostKind::Boa {
        let mut notice = BOA_NOTICE.to_string();
        if cfg!(windows) {
            notice.push_str(BOA_NOTICE_WINDOWS);
        }
        out.insert("notice".into(), Value::String(notice));
    }
    Ok(Value::Object(out))
}

/// `_options`: the engine's options from the panel's values.
pub fn options(host: &dyn FinsHost, values: &Value) -> Result<String, FeatureError> {
    let v = if truthy(Some(values)) { values.clone() } else { json!({}) };
    call(host, "optionsFromDialogJson", &[&serde_json::to_string(&v).unwrap_or_else(|_| "{}".into())])
}

// -- the mesh ---------------------------------------------------------------------------------

/// A body's triangles: `b.mesh()` in 1.x.
#[derive(Clone)]
pub struct MeshData {
    pub verts: Vec<P3>,
    pub faces: Vec<[usize; 3]>,
    /// The soup and key worked out once for the body this came from (`body_mesh`); `None`
    /// for a mesh made any other way. Only ever filled from `verts` and `faces`.
    pub memo: Option<Arc<MeshMemo>>,
}

/// [`soup_json`] and [`mesh_key`] of one body's mesh, each made the first time it is asked for.
#[derive(Default)]
pub struct MeshMemo {
    soup_json: OnceLock<String>,
    key: OnceLock<String>,
}

/// What a body's mesh comes from. Held, so the object can't be freed and its address reused.
enum MeshSource {
    Solid(Solid),
    Mesh(Arc<MeshArrays>),
}

impl MeshSource {
    fn of(b: &Body) -> Option<MeshSource> {
        match (&b.solid, &b.mesh) {
            (Some(s), _) => Some(MeshSource::Solid(s.clone())),
            (None, Some(m)) => Some(MeshSource::Mesh(m.clone())),
            (None, None) => None,
        }
    }
    fn same(&self, o: &MeshSource) -> bool {
        match (self, o) {
            (MeshSource::Solid(a), MeshSource::Solid(b)) => a.shares_with(b),
            (MeshSource::Mesh(a), MeshSource::Mesh(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }
}

/// The meshes of the last [`MESH_MEMO`] bodies asked for, so drawing more walls on the same
/// body doesn't make its mesh, soup and key again. A body's solid or mesh is never changed in
/// place (a feature makes a new one), so the same object means the same triangles. One on the
/// web build, whose memory never shrinks.
#[cfg(not(target_arch = "wasm32"))]
const MESH_MEMO: usize = 2;
#[cfg(target_arch = "wasm32")]
const MESH_MEMO: usize = 1;

fn mesh_memo() -> &'static Mutex<VecDeque<(MeshSource, MeshData)>> {
    static M: OnceLock<Mutex<VecDeque<(MeshSource, MeshData)>>> = OnceLock::new();
    M.get_or_init(|| Mutex::new(VecDeque::new()))
}

pub fn body_mesh(b: &Body) -> MeshData {
    let src = MeshSource::of(b);
    if let Some(src) = &src {
        let memo = mesh_memo().lock().unwrap_or_else(|e| e.into_inner());
        if let Some((_, m)) = memo.iter().find(|(s, _)| s.same(src)) {
            return m.clone();
        }
    }
    let m = match (&b.solid, &b.mesh) {
        (Some(s), _) => s.to_mesh(),
        (None, Some(m)) => (**m).clone(),
        (None, None) => Default::default(),
    };
    let verts = m.positions.chunks_exact(3).map(|p| [p[0] as f64, p[1] as f64, p[2] as f64]).collect();
    let faces = m.triangles.chunks_exact(3).map(|t| [t[0] as usize, t[1] as usize, t[2] as usize]).collect();
    let Some(src) = src else { return MeshData { verts, faces, memo: None } };
    let md = MeshData { verts, faces, memo: Some(Arc::new(MeshMemo::default())) };
    let mut memo = mesh_memo().lock().unwrap_or_else(|e| e.into_inner());
    memo.push_back((src, md.clone()));
    while memo.len() > MESH_MEMO {
        memo.pop_front();
    }
    md
}

/// `_mesh_key`: a hash of the float32 vertices and the int64 faces (only ever a key for this
/// process's caches, never compared with 1.x).
pub fn mesh_key(m: &MeshData) -> String {
    match &m.memo {
        Some(memo) => memo.key.get_or_init(|| make_mesh_key(m)).clone(),
        None => make_mesh_key(m),
    }
}

/// Vertices or faces per leaf of [`make_mesh_key`]'s hash.
const KEY_LEAF: usize = 65_536;

/// SHA-256 of fixed-size runs of the vertices and the faces (on several threads where there
/// are any), then of the counts and those hashes: the same key whatever the threads.
fn make_mesh_key(m: &MeshData) -> String {
    let (nv, nf) = (m.verts.len().div_ceil(KEY_LEAF), m.faces.len().div_ceil(KEY_LEAF));
    let leaves = crate::features::common::pose::par_map(nv + nf, |i| {
        let mut buf: Vec<u8> = Vec::with_capacity(KEY_LEAF * 24);
        if i < nv {
            for v in &m.verts[i * KEY_LEAF..((i + 1) * KEY_LEAF).min(m.verts.len())] {
                v.iter().for_each(|x| buf.extend_from_slice(&(*x as f32).to_le_bytes()));
            }
        } else {
            let i = i - nv;
            for f in &m.faces[i * KEY_LEAF..((i + 1) * KEY_LEAF).min(m.faces.len())] {
                f.iter().for_each(|x| buf.extend_from_slice(&(*x as i64).to_le_bytes()));
            }
        }
        Sha256::digest(&buf)
    });
    let mut h = Sha256::new();
    h.update(format!("v{}f{}", m.verts.len(), m.faces.len()).as_bytes());
    leaves.iter().for_each(|d| h.update(d));
    h.finalize().iter().take(16).map(|b| format!("{b:02x}")).collect()
}

/// `_soup`: the triangles as little-endian float64 xyz per corner, base64.
pub fn soup(m: &MeshData) -> String {
    soup_text(m, false)
}

/// Triangles per block of [`soup_text`]'s work (each block is encoded on its own).
const SOUP_BLOCK: usize = 16_384;

/// The soup, in quotes when `quoted`. A triangle is 72 bytes, a whole number of base64 groups
/// (96 characters), so each block of triangles is encoded straight into its own part of the
/// text, on several threads where there are any. Same text as `b64_encode` of all the bytes.
fn soup_text(m: &MeshData, quoted: bool) -> String {
    let q = usize::from(quoted);
    let mut out = vec![b'"'; m.faces.len() * 96 + 2 * q];
    let body_len = out.len() - 2 * q;
    let body = &mut out[q..q + body_len];
    let encode = |faces: &[[usize; 3]], dst: &mut [u8]| {
        let mut tri = [0u8; 72];
        for (f, d) in faces.iter().zip(dst.chunks_exact_mut(96)) {
            for (k, &i) in f.iter().enumerate() {
                for (c, x) in m.verts[i].iter().enumerate() {
                    tri[k * 24 + c * 8..k * 24 + c * 8 + 8].copy_from_slice(&x.to_le_bytes());
                }
            }
            for (src, o) in tri.chunks_exact(3).zip(d.chunks_exact_mut(4)) {
                let n = (src[0] as u32) << 16 | (src[1] as u32) << 8 | src[2] as u32;
                o[0] = B64[(n >> 18) as usize & 63];
                o[1] = B64[(n >> 12) as usize & 63];
                o[2] = B64[(n >> 6) as usize & 63];
                o[3] = B64[n as usize & 63];
            }
        }
    };
    let blocks: Vec<(&[[usize; 3]], &mut [u8])> = m.faces.chunks(SOUP_BLOCK).zip(body.chunks_mut(SOUP_BLOCK * 96)).collect();
    #[cfg(not(target_arch = "wasm32"))]
    {
        let workers = std::thread::available_parallelism().map_or(1, |p| p.get()).min(blocks.len()).min(8);
        if workers > 1 {
            let work = Mutex::new(blocks);
            std::thread::scope(|s| {
                for _ in 0..workers {
                    s.spawn(|| loop {
                        let job = work.lock().unwrap_or_else(|e| e.into_inner()).pop();
                        match job {
                            Some((f, d)) => encode(f, d),
                            None => break,
                        }
                    });
                }
            });
            return String::from_utf8(out).expect("base64 is ASCII");
        }
    }
    for (f, d) in blocks {
        encode(f, d);
    }
    String::from_utf8(out).expect("base64 is ASCII")
}

/// [`soup`] as JSON text (quoted), as a call argument; kept with the body's mesh.
pub fn soup_json(m: &MeshData) -> std::borrow::Cow<'_, str> {
    // base64 never needs escaping
    let make = || soup_text(m, true);
    match &m.memo {
        Some(memo) => std::borrow::Cow::Borrowed(memo.soup_json.get_or_init(make).as_str()),
        None => std::borrow::Cow::Owned(make()),
    }
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn b64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        out.push(B64[(n >> 18) as usize & 63] as char);
        out.push(B64[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 { B64[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if c.len() > 2 { B64[n as usize & 63] as char } else { '=' });
    }
    out
}

pub fn b64_decode(s: &str) -> Result<Vec<u8>, FeatureError> {
    let bad = || FeatureError("Support Fins: the engine's answer wasn't valid base64.".into());
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    let (mut acc, mut bits) = (0u32, 0u32);
    for ch in s.bytes() {
        let v = match ch {
            b'A'..=b'Z' => ch - b'A',
            b'a'..=b'z' => ch - b'a' + 26,
            b'0'..=b'9' => ch - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' | b'\r' | b'\n' => continue,
            _ => return Err(bad()),
        };
        acc = acc << 6 | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Ok(out)
}

/// `np.frombuffer(b64decode(s), "<f4").reshape(-1, 3, 3)`: the triangles, flat xyz in f32.
pub fn decode_triangles(s: &str) -> Result<Vec<[[f32; 3]; 3]>, FeatureError> {
    let bytes = b64_decode(s)?;
    let f: Vec<f32> = bytes.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
    Ok(f.chunks_exact(9)
        .map(|t| [[t[0], t[1], t[2]], [t[3], t[4], t[5]], [t[6], t[7], t[8]]])
        .collect())
}

// -- the result caches ---------------------------------------------------------------------------

/// The engine takes a second or more on a big part, and the same build is asked for more than
/// once (the live preview, then OK; each new wall while drawing), so its raw output is kept
/// for the last few inputs: 6 fin builds and 64 walls.
pub struct Lru<V> {
    max: usize,
    items: VecDeque<(String, V)>,
}

impl<V: Clone> Lru<V> {
    pub const fn new(max: usize) -> Self {
        Lru { max, items: VecDeque::new() }
    }
    pub fn get(&mut self, key: &str) -> Option<V> {
        let i = self.items.iter().position(|(k, _)| k == key)?;
        let item = self.items.remove(i)?;
        let v = item.1.clone();
        self.items.push_back(item);
        Some(v)
    }
    pub fn put(&mut self, key: String, v: V) {
        self.items.retain(|(k, _)| *k != key);
        self.items.push_back((key, v));
        while self.items.len() > self.max {
            self.items.pop_front();
        }
    }
}

#[derive(Clone)]
pub enum Cached {
    Fins(std::sync::Arc<results::RawFins>),
    Json(Value),
}

pub const CACHE_FINS: usize = 6;
pub const CACHE_WALL: usize = 64;

pub struct Caches {
    pub fins: Lru<Cached>,
    pub wall: Lru<Value>,
}

pub fn caches() -> &'static Mutex<Caches> {
    static C: OnceLock<Mutex<Caches>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(Caches { fins: Lru::new(CACHE_FINS), wall: Lru::new(CACHE_WALL) }))
}

/// Empty both caches (tests; or after the engine changes).
pub fn clear_caches() {
    let mut c = caches().lock().unwrap_or_else(|e| e.into_inner());
    c.fins = Lru::new(CACHE_FINS);
    c.wall = Lru::new(CACHE_WALL);
    drop(c);
    mesh_memo().lock().unwrap_or_else(|e| e.into_inner()).clear();
}

/// `_cached("wall", key, fn)`.
pub fn cached_wall(key: String, build: impl FnOnce() -> Result<Value, FeatureError>) -> Result<Value, FeatureError> {
    if let Some(v) = caches().lock().unwrap_or_else(|e| e.into_inner()).wall.get(&key) {
        return Ok(v);
    }
    let v = build()?;
    caches().lock().unwrap_or_else(|e| e.into_inner()).wall.put(key, v.clone());
    Ok(v)
}

/// `_cached("fins", key, fn)` for a JSON answer (Suggest).
pub fn cached_json(key: String, build: impl FnOnce() -> Result<Value, FeatureError>) -> Result<Value, FeatureError> {
    if let Some(Cached::Json(v)) = caches().lock().unwrap_or_else(|e| e.into_inner()).fins.get(&key) {
        return Ok(v);
    }
    let v = build()?;
    caches().lock().unwrap_or_else(|e| e.into_inner()).fins.put(key, Cached::Json(v.clone()));
    Ok(v)
}

pub fn parse_json(s: &str) -> Result<Value, FeatureError> {
    serde_json::from_str(s).map_err(|e| FeatureError(format!("Support Fins: {e}")))
}

/// `[float(x) for x in v]` of a JSON array (missing or odd entries count as 0).
pub fn floats(v: Option<&Value>) -> Vec<f64> {
    v.and_then(Value::as_array).map(|a| a.iter().map(|x| x.as_f64().unwrap_or(0.0)).collect()).unwrap_or_default()
}

// -- shared by Support Fins and Draw Walls ---------------------------------------------------------

/// What 1.x also puts in a result besides the fields `FeatureResult` has: `paint` (the overhang
/// faces to colour), `walls` (how each drawn wall went), `support` (the plastic the supports
/// cost), and for the live preview `pieces`, `colors` and `ghosts`. `FeatureResult` has no place
/// for them yet (milestone 4 adds one), so the features hand them back to their caller.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Extras {
    pub paint: Option<Value>,
    pub walls: Option<Value>,
    pub support: Option<Value>,
    pub pieces: Option<Map<String, Value>>,
    pub colors: Map<String, Value>,
    pub ghosts: Vec<String>,
}

/// `drawn_walls`: the walls of a Draw Walls feature (older projects keep one wall in `points`).
pub fn drawn_walls(p: &Map<String, Value>) -> Vec<Value> {
    if let Some(Value::Array(w)) = p.get("walls") {
        return w.clone();
    }
    let pts = match p.get("points") {
        Some(Value::Array(a)) => a.clone(),
        _ => Vec::new(),
    };
    if pts.len() == 2 { vec![json!({"shape": "line", "points": pts})] } else { Vec::new() }
}

/// The drawn walls that have enough points to build.
pub fn usable_walls(p: &Map<String, Value>) -> Vec<Value> {
    drawn_walls(p)
        .into_iter()
        .filter(|w| {
            let n = w.get("points").and_then(Value::as_array).map_or(0, Vec::len);
            n >= if w.get("shape").and_then(Value::as_str) == Some("anchor") { 1 } else { 2 }
        })
        .collect()
}

/// `_add_supports`: put the supports into the design, beside it (the live preview or a separate
/// body) or joined to it.
pub fn add_supports(
    state: &mut crate::State,
    fid: &str,
    res: &mut crate::FeatureResult,
    extras: &mut Extras,
    bid: &str,
    solids: &[stl_kernel::Solid],
    output: Option<&str>,
    suffix: &str,
    preview: bool,
) -> Result<(), FeatureError> {
    let b = state.body(bid)?.clone();
    let fins = if solids.len() == 1 { solids[0].clone() } else { stl_kernel::Solid::union_all(solids) };
    let is_solid = b.solid.is_some();
    if preview && output != Some("separate") && is_solid {
        // the live preview shows the supports next to the untouched part (no union to wait
        // for, and only the supports are sent to the screen); OK joins them for real
        let nid = format!("{fid}~sf");
        state.bodies.insert(nid.clone(), Body { name: format!("{} {suffix}", b.name), solid: Some(fins), mesh: None });
        res.created.push(nid.clone());
        res.preview_only = true;
        extras.colors.insert(nid, Value::String(SUPPORT_COLOR.into()));
        return Ok(());
    }
    if output == Some("separate") || !is_solid {
        let mut nid = fid.to_string();
        let mut n = 1;
        while state.bodies.contains_key(&nid) {
            n += 1;
            nid = format!("{fid}.{n}");
        }
        state.bodies.insert(nid.clone(), Body { name: format!("{} {suffix}", b.name), solid: Some(fins), mesh: None });
        res.created.push(nid.clone());
        if preview {
            extras.colors.insert(nid, Value::String(SUPPORT_COLOR.into()));
        }
        if !is_solid && output != Some("separate") {
            res.warning = Some(format!("\"{}\" isn't a watertight solid, so the supports were added as a separate body.", b.name));
        }
    } else {
        let joined = b.solid.as_ref().map(|s| s.union(&fins));
        state.bodies.insert(bid.to_string(), Body { name: b.name.clone(), solid: joined, mesh: None });
        res.modified.push(bid.to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_round_trip() {
        for n in 0..10usize {
            let data: Vec<u8> = (0..n as u8).map(|i| i.wrapping_mul(37)).collect();
            assert_eq!(b64_decode(&b64_encode(&data)).unwrap(), data);
        }
        assert_eq!(b64_encode(b"Man"), "TWFu");
        assert_eq!(b64_encode(b"Ma"), "TWE=");
    }

    #[test]
    fn lru_keeps_the_last_few() {
        let mut c: Lru<u32> = Lru::new(2);
        c.put("a".into(), 1);
        c.put("b".into(), 2);
        assert_eq!(c.get("a"), Some(1)); // a is now the newest
        c.put("c".into(), 3);
        assert_eq!(c.get("b"), None);
        assert_eq!(c.get("a"), Some(1));
        assert_eq!(c.get("c"), Some(3));
    }

    #[test]
    fn soup_blocks_match_one_encode() {
        // more triangles than one block, so the blocks (and threads) are used
        let n = SOUP_BLOCK.max(KEY_LEAF) * 2 + 7;
        let verts: Vec<P3> = (0..n + 2).map(|i| [i as f64 * 0.37, -(i as f64) / 3.0, (i % 11) as f64 * 1e-3]).collect();
        let faces: Vec<[usize; 3]> = (0..n).map(|i| [i, i + 1, i + 2]).collect();
        let m = MeshData { verts, faces, memo: None };
        let mut bytes = Vec::new();
        for f in &m.faces {
            for &i in f {
                for x in m.verts[i] {
                    bytes.extend_from_slice(&x.to_le_bytes());
                }
            }
        }
        let want = b64_encode(&bytes);
        assert_eq!(soup(&m), want);
        assert_eq!(soup_json(&m), format!("\"{want}\""));
        // the key: the same for equal meshes, another when a vertex moves or a face changes
        let k = mesh_key(&m);
        assert_eq!(k.len(), 32);
        assert_eq!(mesh_key(&MeshData { memo: None, ..m.clone() }), k);
        let mut moved = m.clone();
        moved.verts[KEY_LEAF + 3][1] += 0.5;
        assert_ne!(mesh_key(&moved), k);
        let mut flipped = m.clone();
        flipped.faces[n - 1].swap(0, 1);
        assert_ne!(mesh_key(&flipped), k);
        let mut fewer = m.clone();
        fewer.verts.pop();
        assert_ne!(mesh_key(&fewer), k);
        let empty = MeshData { verts: vec![], faces: vec![], memo: None };
        assert_eq!(soup(&empty), "");
        assert_eq!(soup_json(&empty), "\"\"");
    }

    #[test]
    fn body_mesh_keeps_soup_and_key_per_body() {
        let arrays = MeshArrays { positions: vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.5], triangles: vec![0, 1, 2] };
        let body = Body { name: "b".into(), solid: None, mesh: Some(Arc::new(arrays.clone())) };
        let a = body_mesh(&body);
        let plain = MeshData { verts: a.verts.clone(), faces: a.faces.clone(), memo: None };
        assert_eq!(soup_json(&a), serde_json::to_string(&soup(&plain)).unwrap());
        assert_eq!(mesh_key(&a), mesh_key(&plain));
        // the same body again: the same memo, so nothing is made twice
        let b = body_mesh(&body);
        assert!(Arc::ptr_eq(a.memo.as_ref().unwrap(), b.memo.as_ref().unwrap()));
        assert_eq!(soup_json(&b), soup_json(&a));
        // another body with equal triangles: its own memo, the same answers
        let other = Body { name: "c".into(), solid: None, mesh: Some(Arc::new(arrays)) };
        let c = body_mesh(&other);
        assert!(!Arc::ptr_eq(a.memo.as_ref().unwrap(), c.memo.as_ref().unwrap()));
        assert_eq!(mesh_key(&c), mesh_key(&a));
    }

    #[test]
    fn engine_errors_get_the_prefix_once() {
        assert_eq!(engine_error("boom\n  at x").0, "Support Fins: boom");
        assert_eq!(engine_error("Support Fins: the engine stopped responding.").0, "Support Fins: the engine stopped responding.");
    }
}
