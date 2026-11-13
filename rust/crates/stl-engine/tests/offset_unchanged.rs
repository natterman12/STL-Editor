//! Bit-exact meshes for every parity case, golden check and kernel corpus case that runs
//! Offset or Minkowski (the code the speed pass is allowed to change). Hashes were recorded
//! before that pass, on the stock path. Positions and indices are hashed as little-endian bits.
//! `synthetic/batch_notch_*` is the one case over the 4,000-triangle batch floor; its hashes
//! were recorded with the stock Minkowski, and the test runs the batch path.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use stl_engine::params::Param;
use stl_engine::{Document, Feature, Registry};
use stl_kernel::Solid;
use stl_mesh::offset::offset_solid;

/// `name -> sha256` of the output recorded before the speed pass (release, 2026-11-13).
const EXPECTED: &[(&str, &str)] = &[
    ("combine/combine_cut_expr_clearance", "39dd23b5b02873efbe8608df8afaa4f22e80ac3a45e8a40c98df5a4ce21c040a"),
    ("kernel/minkowski_difference_cube_sphere", "520ed8060535294ddd1becb4878f59e483ce28825b3485986678ef01f8aedc0b"),
    ("kernel/minkowski_sum_cube_sphere", "4b4e9b1de8bed4b815c0e01ca080cc2a08991330902fb52051451d096bb6261d"),
    ("offset/offset_detailed_body", "295d041fcbc1fbe1ebe7a8275a451f477c76759bd7b4b20a0a2ea5dc60b48c6e"),
    ("offset/offset_grow", "62bd16ccfc9b10bec1313d8af7dbdaff465d57a0d987fdcb61e0d9f70eadd1d8"),
    ("offset/offset_no_body", "1726b74cab1bef4ca66a5fdf3dae322e4fd2e01a4883952da2f173e70a2a361a"),
    ("offset/offset_shrink", "553d9f177513b5f43e218439c16247392712448c15314f563f2ae57406c43367"),
    ("offset/offset_shrink_nothing", "adf3498c8dda73ed4f06d15c2e7c8ad02d9633c8d5e73d27b7dfa61f47c9be84"),
    ("offset/offset_two_bodies", "216e0826e1750c0229d12aa492bb58e957253cb71932cbcb310aa19d0e84d7be"),
    ("offset/offset_zero_distance", "0bbbe9adaabc835bdefd9ebe2dadcef50342307c2a615ab2d66787f2302361d2"),
    ("synthetic/batch_notch_grow", "3ac712873786e5ac0704f69b5b24ca0074bb21df974ac6fcccde0e8b6e6f1d36"),
    ("synthetic/batch_notch_shrink", "24670a69f4c9d0be794c2347c503ce329c9a1245c015d1826bb3d95d40aeb99b"),
    ("synthetic/notched_grow", "32f00d59645a61e36b74ceb0c4e31edeb51c853f60e226ab7292794095728a4c"),
    ("synthetic/notched_shrink", "eeb3be65ae34e126e90feaa0f53a538c23a1e29df5e2ed2b44c83197cf76d5f8"),
];

fn parity_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/parity")
}

fn read_json(path: &PathBuf) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))).unwrap()
}

fn hex(data: &[u8]) -> String {
    data.iter().map(|b| format!("{b:02x}")).collect()
}

fn feed_mesh(h: &mut Sha256, solid: &Solid) {
    let m = solid.to_mesh();
    h.update(&(m.positions.len() as u64).to_le_bytes());
    for p in &m.positions {
        h.update(&p.to_le_bytes());
    }
    h.update(&(m.triangles.len() as u64).to_le_bytes());
    for t in &m.triangles {
        h.update(&t.to_le_bytes());
    }
}

fn hash_solid(solid: &Solid) -> String {
    let mut h = Sha256::new();
    feed_mesh(&mut h, solid);
    hex(&h.finalize())
}

/// One feature case, evaluated the way `tests/parity.rs` does, hashed as result errors plus
/// each body's positions and indices in timeline order.
fn hash_case(case: &Value) -> String {
    let features = case["features"].as_array().cloned().unwrap_or_default();
    let mut doc = Document::new(Registry::builtin());
    doc.params = case["params"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|q| Param {
            name: q["name"].as_str().unwrap_or_default().to_string(),
            expr: q["expr"].as_str().unwrap_or_default().to_string(),
            note: q["note"].as_str().unwrap_or_default().to_string(),
        })
        .collect();
    doc.features = features
        .iter()
        .enumerate()
        .map(|(i, raw)| Feature {
            id: format!("f{}", i + 1),
            kind: raw["type"].as_str().unwrap_or_default().to_string(),
            name: raw["name"].as_str().filter(|n| !n.is_empty()).map(str::to_string),
            params: raw["params"].as_object().cloned().unwrap_or_else(Map::new),
            suppressed: false,
            extra: Map::new(),
        })
        .collect();
    let results = doc.evaluate();
    let mut h = Sha256::new();
    for r in &results {
        h.update(r.id.as_bytes());
        h.update([0]);
        h.update(r.error.as_deref().unwrap_or("").as_bytes());
        h.update([0]);
    }
    h.update([1]);
    for (id, body) in &doc.state.bodies {
        h.update(id.as_bytes());
        h.update([0]);
        h.update(body.name.as_bytes());
        h.update([0]);
        match &body.solid {
            Some(s) => feed_mesh(&mut h, s),
            None => h.update(b"none"),
        }
    }
    hex(&h.finalize())
}

fn uses_offset_code(case: &Value) -> bool {
    case["requires"].as_array().is_some_and(|a| a.iter().any(|v| v == "offset"))
        || case["features"].as_array().is_some_and(|fs| fs.iter().any(|f| f["type"] == "offset"))
}

fn collect() -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for file in ["offset.json", "combine.json"] {
        let cases = read_json(&parity_dir().join("features").join(file));
        for case in cases.as_array().expect("case file is a list") {
            if file == "combine.json" && !uses_offset_code(case) {
                continue;
            }
            let name = case["name"].as_str().unwrap();
            let key = format!("{}/{}", file.trim_end_matches(".json"), name);
            out.insert(key, hash_case(case));
        }
    }

    // Kernel corpus (`kernel_cases.json`): the two Minkowski ops. Same solids, same calls.
    let cube = Solid::cube([4.0, 4.0, 4.0], true);
    let sp = Solid::sphere(1.0, 8);
    out.insert("kernel/minkowski_sum_cube_sphere".into(), hash_solid(&cube.minkowski_sum(&sp)));
    out.insert(
        "kernel/minkowski_difference_cube_sphere".into(),
        hash_solid(&cube.minkowski_difference(&sp)),
    );

    // Sphere 48 with a notch: over one batch (1000 triangles) and under the
    // simplify cap, but under the 4,000-triangle floor, so this stays on the
    // stock path. The batch path is `batch_notch` below.
    let sphere = Solid::sphere(10.0, 48);
    let notch = Solid::cube([8.0, 8.0, 8.0], false).translate([2.0, 2.0, 6.0]);
    let part = sphere.difference(&notch);
    assert!(
        part.num_tri() > 1000 && part.num_tri() < 4_000,
        "small notch must stay under the batch-path floor, got {} tris",
        part.num_tri()
    );
    let grow = offset_solid(&part, 0.2, 12_000).unwrap_or_else(|e| panic!("grow: {e}"));
    let shrink = offset_solid(&part, -0.2, 12_000).unwrap_or_else(|e| panic!("shrink: {e}"));
    out.insert("synthetic/notched_grow".into(), hash_solid(&grow));
    out.insert("synthetic/notched_shrink".into(), hash_solid(&shrink));

    let wide = batch_notch();
    let before = stl_kernel::minkowski_batch_path_hits();
    let wide_grow = offset_solid(&wide, 0.2, 12_000).unwrap_or_else(|e| panic!("wide grow: {e}"));
    let mid = stl_kernel::minkowski_batch_path_hits();
    let wide_shrink = offset_solid(&wide, -0.2, 12_000).unwrap_or_else(|e| panic!("wide shrink: {e}"));
    let after = stl_kernel::minkowski_batch_path_hits();
    assert!(mid > before, "wide grow stayed on the stock path ({} tris)", wide.num_tri());
    assert!(after > mid, "wide shrink stayed on the stock path ({} tris)", wide.num_tri());
    out.insert("synthetic/batch_notch_grow".into(), hash_solid(&wide_grow));
    out.insert("synthetic/batch_notch_shrink".into(), hash_solid(&wide_shrink));
    out
}

#[test]
fn offset_unchanged() {
    let got = collect();
    assert_eq!(got.len(), EXPECTED.len(), "the case list changed");
    for (name, hash) in EXPECTED {
        match got.get(*name) {
            Some(g) if g == hash => {}
            Some(g) => panic!("{name}: hash changed\n  before {hash}\n  now    {g}"),
            None => panic!("{name}: case is gone"),
        }
    }
}

/// Sphere 96 with a notch. At least 4,000 triangles, so it is over the batch
/// floor, and at most 12,000, so Offset does not simplify it first.
fn batch_notch() -> Solid {
    let sphere = Solid::sphere(10.0, 96);
    let notch = Solid::cube([8.0, 8.0, 8.0], false).translate([2.0, 2.0, 6.0]);
    let part = sphere.difference(&notch);
    assert!(
        part.num_tri() >= 4_000 && part.num_tri() <= 12_000,
        "batch notch must stay on the unsimplified batch path, got {} tris",
        part.num_tri()
    );
    part
}

/// The batch path on one thread and on several threads, bit for bit.
/// `RAYON_NUM_THREADS` does not reach this pool.
#[test]
fn offset_batch_threads_match() {
    let part = batch_notch();
    let one = stl_kernel::with_minkowski_batch_threads(1, || offset_on_batch_path(&part, 0.2));
    let many = stl_kernel::with_minkowski_batch_threads(4, || offset_on_batch_path(&part, 0.2));
    assert_eq!(one.to_mesh(), many.to_mesh());
}

fn offset_on_batch_path(part: &Solid, distance: f64) -> Solid {
    let before = stl_kernel::minkowski_batch_path_hits();
    let solid = offset_solid(part, distance, 12_000).unwrap_or_else(|e| panic!("{distance}: {e}"));
    assert!(
        stl_kernel::minkowski_batch_path_hits() > before,
        "offset {distance} stayed on the stock path"
    );
    solid
}

/// Full bench mesh, grow 0.2 mm. Slow, so CI skips it. The hash of the batch
/// path must match the stock path.
#[test]
#[ignore = "full bench mesh, about 10 s"]
fn offset_bench_mesh_matches_stock() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../dev/bench/test_mesh.stl");
    let data = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let n = u32::from_le_bytes(data[80..84].try_into().unwrap()) as usize;
    let mut idx: HashMap<[u32; 3], u32> = HashMap::new();
    let mut positions = Vec::new();
    let mut triangles = Vec::new();
    for t in 0..n {
        let rec = &data[84 + 50 * t + 12..84 + 50 * t + 48];
        for c in 0..3 {
            let x: [f32; 3] = std::array::from_fn(|k| {
                f32::from_le_bytes(rec[c * 12 + k * 4..c * 12 + k * 4 + 4].try_into().unwrap())
            });
            let key = [x[0].to_bits(), x[1].to_bits(), x[2].to_bits()];
            let next = idx.len() as u32;
            let id = *idx.entry(key).or_insert_with(|| {
                positions.extend_from_slice(&x);
                next
            });
            triangles.push(id);
        }
    }
    let solid = Solid::from_mesh(&stl_kernel::MeshArrays { positions, triangles }).unwrap();
    let before = stl_kernel::minkowski_batch_path_hits();
    let fast = offset_solid(&solid, 0.2, 12_000).unwrap_or_else(|e| panic!("fast: {e}"));
    assert!(
        stl_kernel::minkowski_batch_path_hits() > before,
        "bench offset stayed on the stock path"
    );
    let stock = stl_kernel::with_stock_minkowski(|| {
        let at = stl_kernel::minkowski_batch_path_hits();
        let out = offset_solid(&solid, 0.2, 12_000).unwrap_or_else(|e| panic!("stock: {e}"));
        assert_eq!(at, stl_kernel::minkowski_batch_path_hits(), "stock bench took the batch path");
        out
    });
    assert_eq!(hash_solid(&fast), hash_solid(&stock));
    assert_eq!(fast.to_mesh(), stock.to_mesh());
    println!("bench 0.2 mm: {} tris, matches stock", fast.num_tri());
}
