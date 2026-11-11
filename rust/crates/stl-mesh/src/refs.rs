//! Part-edge reference geometry for sketches (`stleditor/meshtools.py`).

use stl_kernel::{dot, Frame, Shape2d, Solid};

/// One ref entity a sketch can measure from (circle or line), in sketch coordinates.
#[derive(Debug, Clone, PartialEq)]
pub enum RefEntity {
    Circle { cx: f64, cy: f64, r: f64 },
    Line { p1: [f64; 2], p2: [f64; 2] },
}

/// Result of [`ref_geometry`]: the outline kind and the entities.
#[derive(Debug, Clone, PartialEq)]
pub struct RefGeometry {
    pub kind: Option<String>,
    pub entities: Vec<RefEntity>,
}

/// Sketch entity from [`outline_entities`] (circles / closed polylines with ids).
#[derive(Debug, Clone, PartialEq)]
pub enum OutlineEntity {
    Circle {
        id: i64,
        cx: f64,
        cy: f64,
        r: f64,
        construction: bool,
        projected: bool,
    },
    Polyline {
        id: i64,
        pts: Vec<[f64; 2]>,
        closed: bool,
        construction: bool,
        projected: bool,
    },
}

fn to_local(solid: &Solid, frame: &Frame) -> Solid {
    let (o, x, y, n) = (frame.o, frame.x, frame.y, frame.n);
    let t = [-dot(x, o), -dot(y, o), -dot(n, o)];
    let m = [
        [x[0], x[1], x[2], t[0]],
        [y[0], y[1], y[2], t[1]],
        [n[0], n[1], n[2], t[2]],
    ];
    solid.transform(m)
}

fn union_shapes(shapes: impl IntoIterator<Item = Shape2d>) -> Shape2d {
    let mut contours = Vec::new();
    for s in shapes {
        contours.extend(s.contours);
    }
    if contours.is_empty() {
        return Shape2d::default();
    }
    let shape = Shape2d { contours };
    if shape.is_empty() {
        return Shape2d::default();
    }
    // Force Clipper Positive union, then take every contour of every piece.
    let mut out = Vec::new();
    for piece in shape.decompose() {
        out.extend(piece.contours);
    }
    Shape2d { contours: out }
}

fn shape_polys(shape: Shape2d) -> Vec<Vec<[f64; 2]>> {
    shape.contours.into_iter().filter(|p| p.len() >= 3).collect()
}

/// Closed 2D contours of where `solids` meet the sketch plane, or their shadow.
pub fn outline(solids: &[&Solid], frame: &Frame, mode: &str, tolerance: f64) -> Result<Vec<Vec<[f64; 2]>>, String> {
    let locs: Vec<Solid> = solids.iter().map(|s| to_local(s, frame)).collect();
    let mut shape = Shape2d::default();
    if mode == "section" {
        let mut found = false;
        for h in [0.0, -0.01, 0.01] {
            let slices: Vec<Shape2d> = locs.iter().map(|loc| loc.slice(h)).collect();
            shape = union_shapes(slices);
            if !shape.is_empty() {
                found = true;
                break;
            }
        }
        if !found {
            return Err(
                "The sketch plane doesn't cut through the body here. Try Silhouette, or sketch on a plane that crosses the part."
                    .into(),
            );
        }
    } else {
        // Silhouette: 1.x `Manifold.project()`.
        let projections: Vec<Shape2d> = locs.iter().map(|loc| loc.project()).collect();
        shape = union_shapes(projections);
        if shape.is_empty() {
            return Err("Nothing to project.".into());
        }
    }
    if tolerance > 0.0 {
        shape = shape.simplify(tolerance);
    }
    let polys = shape_polys(shape);
    if polys.is_empty() {
        return Err("The outline came out empty.".into());
    }
    Ok(polys)
}

/// Python 3 `round` (half to even) to `nd` decimal places on the exact binary value.
///
/// Multiplying by `10^nd` first can move a tie off the half; format then parse matches
/// CPython / `round(v, nd)` (e.g. `0.12345`→`0.1235` at 4, `2.675`→`2.67` at 2).
pub fn round_py(v: f64, nd: i32) -> f64 {
    if !v.is_finite() {
        return v;
    }
    let nd = nd.max(0) as usize;
    format!("{:.*}", nd, v).parse().unwrap_or(v)
}

/// Least-squares circle through 2D points (Kasa fit), min-norm like `np.linalg.lstsq`.
/// Mean-centres the points before solving for stability far from the origin.
pub fn fit_circle(p: &[[f64; 2]]) -> (f64, f64, f64) {
    let n = p.len();
    if n == 0 {
        return (0.0, 0.0, -1.0);
    }
    let mx = p.iter().map(|q| q[0]).sum::<f64>() / n as f64;
    let my = p.iter().map(|q| q[1]).sum::<f64>() / n as f64;
    // Solve min ||A x - b|| with A = [x y 1], b = x²+y² on centred coords.
    let mut ata = [[0.0_f64; 3]; 3];
    let mut atb = [0.0_f64; 3];
    for q in p {
        let (x, y) = (q[0] - mx, q[1] - my);
        let rr = x * x + y * y;
        let row = [x, y, 1.0];
        for i in 0..3 {
            atb[i] += row[i] * rr;
            for j in 0..3 {
                ata[i][j] += row[i] * row[j];
            }
        }
    }
    let sol = solve3_min_norm(ata, atb).unwrap_or([0.0, 0.0, 0.0]);
    let cx = sol[0] / 2.0 + mx;
    let cy = sol[1] / 2.0 + my;
    let r2 = sol[2] + (sol[0] / 2.0) * (sol[0] / 2.0) + (sol[1] / 2.0) * (sol[1] / 2.0);
    (cx, cy, if r2 > 0.0 { r2.sqrt() } else { -1.0 })
}

fn solve3_min_norm(m: [[f64; 3]; 3], b: [f64; 3]) -> Option<[f64; 3]> {
    // Gaussian elimination with partial pivot on the 3×3 normal matrix (unique when full rank;
    // under-rank falls back to free vars = 0 → a min-norm-ish pick among basic solutions).
    let mut a = [
        [m[0][0], m[0][1], m[0][2], b[0]],
        [m[1][0], m[1][1], m[1][2], b[1]],
        [m[2][0], m[2][1], m[2][2], b[2]],
    ];
    let mut col_used = [false; 3];
    let mut pivot_row = [-1i32; 3];
    for col in 0..3 {
        let mut piv = None;
        let mut best = 1e-18;
        for r in 0..3 {
            if pivot_row.iter().any(|&pr| pr == r as i32) {
                continue;
            }
            let v = a[r][col].abs();
            if v > best {
                best = v;
                piv = Some(r);
            }
        }
        let Some(piv) = piv else {
            continue;
        };
        col_used[col] = true;
        pivot_row[col] = piv as i32;
        let div = a[piv][col];
        for c in col..4 {
            a[piv][c] /= div;
        }
        for r in 0..3 {
            if r == piv {
                continue;
            }
            let f = a[r][col];
            for c in col..4 {
                a[r][c] -= f * a[piv][c];
            }
        }
    }
    let mut x = [0.0_f64; 3];
    for col in 0..3 {
        let pr = pivot_row[col];
        if pr >= 0 {
            x[col] = a[pr as usize][3];
        }
    }
    let _ = col_used;
    Some(x)
}

/// Recognise a round loop; returns `(cx, cy, r)` rounded like 1.x, or `None`.
pub fn as_circle(p: &[[f64; 2]]) -> Option<(f64, f64, f64)> {
    if p.len() < 12 {
        return None;
    }
    let (cx, cy, r) = fit_circle(p);
    if r <= 0.0 {
        return None;
    }
    let mut max_dev = 0.0_f64;
    for q in p {
        let d = ((q[0] - cx).hypot(q[1] - cy) - r).abs();
        if d > max_dev {
            max_dev = d;
        }
    }
    if max_dev < (0.01_f64).max(r * 0.01) {
        Some((round_py(cx, 4), round_py(cy, 4), round_py(r, 4)))
    } else {
        None
    }
}

/// Turn contours into sketch entities: circles where round, closed polylines otherwise.
/// Matches 1.x `meshtools.outline_entities` (ids, 4-dp points, construction, projected).
pub fn outline_entities<F: FnMut() -> i64>(
    polys: &[Vec<[f64; 2]>],
    mut new_id: F,
    construction: bool,
) -> Vec<OutlineEntity> {
    let mut out = Vec::new();
    for p in polys {
        if let Some((cx, cy, r)) = as_circle(p) {
            out.push(OutlineEntity::Circle {
                id: new_id(),
                cx,
                cy,
                r,
                construction,
                projected: true,
            });
        } else {
            let pts: Vec<[f64; 2]> = p
                .iter()
                .map(|q| [round_py(q[0], 4), round_py(q[1], 4)])
                .collect();
            out.push(OutlineEntity::Polyline {
                id: new_id(),
                pts,
                closed: true,
                construction,
                projected: true,
            });
        }
    }
    out
}

/// EvenOdd simplify + circle/line entities, matching 1.x `ref_geometry` inner `ents(tol)`.
pub fn ents_from_polys(polys: &[Vec<[f64; 2]>], tol: f64) -> Vec<RefEntity> {
    let ps = if tol > 0.0 {
        // 1.x: CrossSection(polys, EvenOdd).simplify(tol)
        shape_polys(Shape2d::from_even_odd(polys).simplify(tol))
    } else {
        polys.to_vec()
    };
    let mut out = Vec::new();
    for p in &ps {
        if p.len() < 3 {
            continue;
        }
        if let Some((cx, cy, r)) = as_circle(p) {
            out.push(RefEntity::Circle { cx, cy, r });
            continue;
        }
        for i in 0..p.len() {
            let q0 = p[i];
            let q1 = p[(i + 1) % p.len()];
            if (q1[0] - q0[0]).hypot(q1[1] - q0[1]) > 1e-6 {
                out.push(RefEntity::Line {
                    p1: [round_py(q0[0], 5), round_py(q0[1], 5)],
                    p2: [round_py(q1[0], 5), round_py(q1[1], 5)],
                });
            }
        }
    }
    out
}

/// Part edges a sketch can measure from: faces on the plane, else section, else silhouette.
pub fn ref_geometry(bodies: &[(&str, &Solid)], frame: &Frame) -> RefGeometry {
    ref_geometry_max(bodies, frame, 3000)
}

pub fn ref_geometry_max(bodies: &[(&str, &Solid)], frame: &Frame, max_segs: usize) -> RefGeometry {
    let solids: Vec<&Solid> = bodies.iter().map(|(_, s)| *s).collect();
    let mut tris: Vec<Vec<[f64; 2]>> = Vec::new();
    let mut total = 0usize;
    for s in &solids {
        let mm = to_local(s, frame).to_mesh();
        let vp = &mm.positions;
        let tv = &mm.triangles;
        total += tv.len() / 3;
        if tv.is_empty() {
            continue;
        }
        for tri in tv.chunks_exact(3) {
            let mut t = [[0.0_f64; 3]; 3];
            for k in 0..3 {
                let i = tri[k] as usize;
                t[k] = [vp[3 * i] as f64, vp[3 * i + 1] as f64, vp[3 * i + 2] as f64];
            }
            if !t.iter().all(|p| p[2].abs() <= 1e-3) {
                continue;
            }
            let mut uv = [[t[0][0], t[0][1]], [t[1][0], t[1][1]], [t[2][0], t[2][1]]];
            let a = [uv[1][0] - uv[0][0], uv[1][1] - uv[0][1]];
            let b = [uv[2][0] - uv[0][0], uv[2][1] - uv[0][1]];
            let ar = a[0] * b[1] - a[1] * b[0];
            if ar.abs() < 1e-12 {
                continue;
            }
            if ar < 0.0 {
                uv.swap(1, 2);
            }
            tris.push(vec![uv[0], uv[1], uv[2]]);
        }
    }

    let mut kind = "face";
    let mut polys: Vec<Vec<[f64; 2]>> = Vec::new();
    if !tris.is_empty() {
        // 1.x: CrossSection(tris, NonZero).to_polygons()
        polys = shape_polys(union_shapes([Shape2d { contours: tris }]));
    }
    if polys.is_empty() {
        kind = "section";
        polys = outline(&solids, frame, "section", 0.02).unwrap_or_default();
    }
    if polys.is_empty() && total <= 300_000 {
        kind = "silhouette";
        polys = outline(&solids, frame, "silhouette", 0.02).unwrap_or_default();
    }

    let mut out = ents_from_polys(&polys, 0.005);
    for tol in [0.05, 0.2, 1.0] {
        if out.len() <= max_segs {
            break;
        }
        out = ents_from_polys(&polys, tol);
    }
    if out.len() > max_segs {
        out.truncate(max_segs);
    }
    RefGeometry {
        kind: if polys.is_empty() { None } else { Some(kind.into()) },
        entities: out,
    }
}
