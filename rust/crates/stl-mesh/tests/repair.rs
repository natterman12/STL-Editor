//! The repair steps against desktop 1.x: `rust/tests/parity/repair_golden.json`, written by
//! `dev/parity/make_repair_golden.py`.

use std::path::PathBuf;

use serde_json::Value;
use stl_kernel::{MeshArrays, Solid};
use stl_mesh::{
    cap_holes, fill_holes, fix_inversion, fix_winding, local_repair, merge_overlapping_shells, process,
    quick_repair, touching_shells, weld,
};
use stl_parity::{Exactness, MeshStats, compare_mesh, mesh_stats};

fn tests_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests")
}

/// A binary STL as a triangle soup: f32 positions, three per triangle.
fn read_binary_stl(name: &str) -> Vec<f32> {
    let data = std::fs::read(tests_dir().join("models").join(name)).expect("model file");
    let n = u32::from_le_bytes(data[80..84].try_into().unwrap()) as usize;
    assert_eq!(data.len(), 84 + 50 * n, "{name} is not a binary STL");
    let mut out = Vec::with_capacity(n * 9);
    for t in 0..n {
        let rec = &data[84 + 50 * t + 12..84 + 50 * t + 48];
        out.extend(rec.chunks_exact(4).map(|b| f32::from_le_bytes(b.try_into().unwrap())));
    }
    out
}

/// The same variants as `variant()` in `make_repair_golden.py`.
fn variant(soup: &[f32], name: &str) -> (Vec<f64>, Vec<u32>) {
    let mut v: Vec<f32> = soup.to_vec();
    let mut f: Vec<u32> = (0..(soup.len() / 3) as u32).collect();
    if name.starts_with("x5") {
        // five copies, each 2.5 bounding boxes further along x
        let xs = || soup.chunks_exact(3).map(|p| p[0]);
        let step = 2.5f32 * (xs().fold(f32::NEG_INFINITY, f32::max) - xs().fold(f32::INFINITY, f32::min));
        v.clear();
        f.clear();
        let nv = (soup.len() / 3) as u32;
        for k in 0..5u32 {
            for p in soup.chunks_exact(3) {
                v.extend_from_slice(&[p[0] + k as f32 * step, p[1], p[2]]);
            }
            f.extend((0..nv).map(|i| i + k * nv));
        }
    }
    if name.contains("lap3") {
        // three copies, each a quarter of the bounding box further along x, y and z
        let axis = |a: usize| soup.chunks_exact(3).map(move |p| p[a]);
        let step: Vec<f32> = (0..3)
            .map(|a| 0.25f32 * (axis(a).fold(f32::NEG_INFINITY, f32::max) - axis(a).fold(f32::INFINITY, f32::min)))
            .collect();
        v.clear();
        f.clear();
        let nv = (soup.len() / 3) as u32;
        for k in 0..3u32 {
            for p in soup.chunks_exact(3) {
                v.extend((0..3).map(|a| p[a] + k as f32 * step[a]));
            }
            f.extend((0..nv).map(|i| i + k * nv));
        }
    }
    let mut tris: Vec<[u32; 3]> = f.chunks_exact(3).map(|t| [t[0], t[1], t[2]]).collect();
    if name.contains("drop7") {
        // leave out every triangle whose index is 3 mod 7
        tris = tris.into_iter().enumerate().filter(|(i, _)| i % 7 != 3).map(|(_, t)| t).collect();
    }
    if name.contains("drop4") {
        // leave out every triangle whose index is 2 mod 4
        tris = tris.into_iter().enumerate().filter(|(i, _)| i % 4 != 2).map(|(_, t)| t).collect();
    }
    if name.contains("flip3") {
        // reverse every triangle whose index is 1 mod 3
        for (i, t) in tris.iter_mut().enumerate() {
            if i % 3 == 1 {
                t.swap(0, 2);
            }
        }
    }
    if name.contains("nan") {
        // one more triangle, on three new vertices whose coordinates are all NaN
        let n = (v.len() / 3) as u32;
        v.extend_from_slice(&[f32::NAN; 9]);
        tris.push([n, n + 1, n + 2]);
    }
    (v.iter().map(|&x| x as f64).collect(), tris.into_iter().flatten().collect())
}

/// As `_stats` in `make_repair_golden.py`: a vertex that no triangle uses and that is not
/// finite (the "nan" variants) is left out.
fn stats(v: &[f64], f: &[u32]) -> MeshStats {
    let nv = v.len() / 3;
    let mut drop: Vec<bool> = v.chunks_exact(3).map(|p| !p.iter().all(|x| x.is_finite())).collect();
    for &i in f {
        drop[i as usize] = false;
    }
    let mut index = vec![0u32; nv];
    let mut p: Vec<f32> = Vec::with_capacity(v.len());
    for i in 0..nv {
        index[i] = (p.len() / 3) as u32;
        if !drop[i] {
            p.extend(v[3 * i..3 * i + 3].iter().map(|&x| x as f32));
        }
    }
    let f: Vec<u32> = f.iter().map(|&i| index[i as usize]).collect();
    mesh_stats(&p, &f)
}

fn solid_stats(s: &Solid) -> MeshStats {
    let m = s.to_mesh();
    mesh_stats(&m.positions, &m.triangles)
}

fn as_solid(v: &[f64], f: &[u32]) -> Option<Solid> {
    let m = MeshArrays { positions: v.iter().map(|&x| x as f32).collect(), triangles: f.to_vec() };
    Solid::from_mesh(&m).ok()
}

/// Compare one step with its golden record; the differences go into `failures`.
fn check(failures: &mut Vec<String>, case: &str, step: &str, golden: &Value, got: Option<MeshStats>) {
    let want: Option<MeshStats> =
        if golden.is_null() { None } else { Some(serde_json::from_value(golden.clone()).expect("MeshStats")) };
    match (want, got) {
        (None, None) => {}
        (Some(w), Some(g)) => {
            let diffs = compare_mesh(&w, &g, Exactness::Exact);
            if !diffs.is_empty() {
                failures.push(format!("{case} {step}: {}", diffs.join("; ")));
            }
        }
        (Some(_), None) => failures.push(format!("{case} {step}: golden has a mesh, got none")),
        (None, Some(g)) => failures.push(format!("{case} {step}: golden has none, got {} triangles", g.triangles)),
    }
}

/// A step where 2.0 differs from 1.x on purpose (`rust/tests/EXCEPTIONS.md`, "Turning a mesh into
/// a solid, and the repair steps it changes").
/// The numbers are 2.0's result. The test fails when 2.0 gives anything else, and when the step
/// matches the 1.x golden again (the entry is then stale and has to go).
struct Exception {
    case: &'static str,
    step: &'static str,
    watertight: bool,
    triangles: usize,
    area: f64,
    /// Every step listed so far gives a solid, so all of them have a volume.
    volume: Option<f64>,
}

const fn exc(case: &'static str, triangles: usize, area: f64, volume: f64) -> Exception {
    Exception { case, step: "local_repair", watertight: true, triangles, area, volume: Some(volume) }
}

/// All of these are the last try of `local_repair` (close the holes, then `Solid::from_mesh`)
/// on a mesh with many triangles left out. Capping the holes leaves edges that four or more
/// triangles use. Manifold 3.5.4, the kernel of 1.x, moves triangle corners while it separates
/// such edges (a bug in its `DedupeEdges`); 2.0 separates them and keeps the shape.
///
/// The three `two_blobs.stl@nan_*` cases are a little different: their holes were capped three
/// times over, which leaves edges that four, six or eight triangles use, and they did not match
/// 1.x even while 2.0 still had the 3.5.4 bug. Nathan agreed on 2026-11-10 to list them too.
const EXCEPTIONS: [Exception; 12] = [
    exc("holed_block.stl@drop4", 362, 3555.988152843922, 5119.26953568131),
    exc("holed_block.stl@nan_drop4", 398, 3684.319332926697, 5120.083131037832),
    exc("holed_block.stl@nan_drop7", 370, 3039.110675062361, 4772.937645639244),
    exc("holed_block.stl@nan_flip3_drop7", 356, 3076.2166897348643, 4754.6374873557115),
    exc("touching_cubes.stl@drop4", 28, 1194.6402240201066, 1397.4359035491943),
    exc("touching_cubes.stl@nan_drop4", 28, 1194.6402240201066, 1397.4359035491943),
    exc("two_blobs.stl@drop4", 1436, 529.7561167535657, 610.9778847351932),
    exc("two_blobs.stl@flip3_drop7", 1294, 438.3077748192302, 399.9885095540398),
    exc("two_blobs.stl@nan_drop4", 1540, 534.7157494749523, 611.0262675430705),
    exc("two_blobs.stl@nan_drop7", 1318, 429.00274888315107, 621.7575590671026),
    exc("two_blobs.stl@nan_flip3_drop7", 1336, 448.0995339360819, 620.8840702129651),
    exc("two_blobs.stl@x5_drop7", 6464, 2148.8970050015664, 3108.5454920097277),
];

/// Two numbers that 2.0 has to reproduce: equal to one part in a million.
fn same(pinned: f64, got: f64) -> bool {
    (got - pinned).abs() <= 1e-6 * pinned.abs().max(1.0)
}

/// What differs between an exception's pinned numbers and what 2.0 gave.
fn exception_diffs(e: &Exception, got: Option<&MeshStats>) -> Vec<String> {
    let Some(g) = got else { return vec!["got no mesh".to_string()] };
    let mut out = Vec::new();
    if g.watertight != e.watertight {
        out.push(format!("watertight: pinned {} got {}", e.watertight, g.watertight));
    }
    if g.triangles != e.triangles {
        out.push(format!("triangles: pinned {} got {}", e.triangles, g.triangles));
    }
    if !same(e.area, g.area) {
        out.push(format!("area: pinned {} got {}", e.area, g.area));
    }
    if let Some(v) = e.volume {
        if !same(v, g.volume) {
            out.push(format!("volume: pinned {v} got {}", g.volume));
        }
    }
    out
}

#[test]
fn steps_match_golden() {
    let text = std::fs::read_to_string(tests_dir().join("parity/repair_golden.json")).expect("golden file");
    let golden: Value = serde_json::from_str(&text).expect("golden json");
    let cases = golden.as_object().expect("an object of cases");
    assert!(!cases.is_empty());
    let mut failures = Vec::new();
    let mut counts = Vec::new();
    let mut steps = 0;
    let mut exceptions = 0;
    for (case, rec) in cases {
        let (file, var) = case.split_once('@').expect("file@variant");
        let (rv, rf) = variant(&read_binary_stl(file), var);
        let mut step = |name: &str, got: Option<MeshStats>| {
            steps += 1;
            let Some(e) = EXCEPTIONS.iter().find(|e| e.case == case && e.step == name) else {
                check(&mut failures, case, name, &rec[name], got);
                return;
            };
            exceptions += 1;
            let mut vs_golden = Vec::new();
            check(&mut vs_golden, case, name, &rec[name], got.clone());
            if vs_golden.is_empty() {
                failures.push(format!("{case} {name}: matches 1.x again, take it out of EXCEPTIONS"));
                return;
            }
            let diffs = exception_diffs(e, got.as_ref());
            if !diffs.is_empty() {
                failures.push(format!("{case} {name}: 2.0's result changed: {}", diffs.join("; ")));
            }
            let was = vs_golden.join("; ").replacen(&format!("{case} {name}: "), "", 1);
            match got {
                Some(g) => println!(
                    "exception: {case} {name}: 2.0 gives {} triangles, area {}, volume {}, watertight {} ({was})",
                    g.triangles, g.area, g.volume, g.watertight
                ),
                None => println!("exception: {case} {name}: 2.0 gives no mesh ({was})"),
            }
        };

        let (wv, wf) = weld(&rv, &rf, 1e-5);
        step("weld", Some(stats(&wv, &wf)));

        let (pv, pf) = process(&wv, &wf);
        step("process", Some(stats(&pv, &pf)));

        let mut f = wf.clone();
        fix_winding(&wv, &mut f);
        step("fix_winding", Some(stats(&wv, &f)));

        let mut f = wf.clone();
        fix_inversion(&wv, &mut f, true);
        step("fix_inversion", Some(stats(&wv, &f)));

        let mut f = wf.clone();
        fix_inversion(&wv, &mut f, false);
        step("fix_inversion_single", Some(stats(&wv, &f)));

        let (mut v, mut f) = (wv.clone(), wf.clone());
        fill_holes(&mut v, &mut f);
        step("fill_holes", Some(stats(&v, &f)));

        let (cv, cf, loops) = cap_holes(&wv, &wf);
        step("cap_holes", Some(stats(&cv, &cf)));
        if rec["cap_loops"].as_u64() != Some(loops as u64) {
            counts.push(format!("{case} cap_loops: golden {} got {loops}", rec["cap_loops"]));
        }

        step("touching_shells", touching_shells(&wv, &wf).as_ref().map(solid_stats));
        step("local_repair", local_repair(&rv, &rf).as_ref().map(solid_stats));

        let (qv, qf) = quick_repair(&rv, &rf);
        step("quick_repair", Some(stats(&qv, &qf)));

        let (merged, count) = match as_solid(&wv, &wf) {
            Some(s) => {
                let (m, n) = merge_overlapping_shells(&s);
                (Some(solid_stats(&m)), n)
            }
            None => (None, 0),
        };
        step("merge_overlapping_shells", merged);
        if rec["merge_count"].as_u64() != Some(count as u64) {
            counts.push(format!("{case} merge_count: golden {} got {count}", rec["merge_count"]));
        }
    }
    failures.append(&mut counts);
    for e in &EXCEPTIONS {
        if !cases.contains_key(e.case) || cases[e.case].get(e.step).is_none() {
            failures.push(format!("{} {}: EXCEPTIONS names a step that is not in the golden file", e.case, e.step));
        }
    }
    println!(
        "repair steps: {} cases, {steps} steps, {} mismatches, {exceptions} exceptions",
        cases.len(),
        failures.len()
    );
    assert!(failures.is_empty(), "{} mismatches:\n{}", failures.len(), failures.join("\n"));
}

/// Review Focus 3: a mesh no step can fix gives "no solid" from every step, never a panic.
#[test]
fn repair_gives_up_cleanly() {
    // one stray triangle
    let (v, f) = variant(&read_binary_stl("stray_triangle.stl"), "plain");
    assert!(local_repair(&v, &f).is_none());
    let (qv, qf) = quick_repair(&v, &f);
    assert_eq!(qf.len(), 3);
    assert!(as_solid(&qv, &qf).is_none());

    // nothing at all, a triangle with no area, and coordinates that are not numbers
    let nan = f64::NAN;
    let inputs: [(Vec<f64>, Vec<u32>); 4] = [
        (vec![], vec![]),
        (vec![0.0; 9], vec![0, 1, 2]),
        (vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 2.0, 0.0, 0.0], vec![0, 1, 2]),
        (vec![nan, 0.0, 0.0, 1.0, nan, 0.0, 0.0, 1.0, f64::INFINITY], vec![0, 1, 2]),
    ];
    // a triangle that names a vertex that is not there
    let square = vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 0.0];
    for f in [vec![0, 1, 4], vec![0, 1, 2, 0, 2, u32::MAX], vec![7, 8, 9]] {
        assert!(local_repair(&square, &f).is_none());
        let (qv, qf) = quick_repair(&square, &f);
        assert!(qf.is_empty() && as_solid(&qv, &qf).is_none());
        assert_eq!(weld(&square, &f, 1e-5), (vec![], vec![]));
        assert_eq!(process(&square, &f), (vec![], vec![]));
    }

    for (v, f) in &inputs {
        assert!(local_repair(v, f).is_none());
        let (qv, qf) = quick_repair(v, f);
        assert!(as_solid(&qv, &qf).is_none());
        let (wv, wf) = weld(v, f, 1e-5);
        assert!(touching_shells(&wv, &wf).is_none());
        let (pv, pf) = process(&wv, &wf);
        let (mut hv, mut hf) = (pv.clone(), pf.clone());
        fill_holes(&mut hv, &mut hf);
        fix_winding(&hv, &mut hf);
        fix_inversion(&hv, &mut hf, true);
        fix_inversion(&hv, &mut hf, false);
        let _ = cap_holes(&hv, &hf);
    }
}
