// face_op.rs — Phase 7a: Face normals, coplanarity, vertex normals
//
// Ports src/face_op.cpp, the face-normal and coplanarity portions of
// src/impl.cpp (SetNormalsAndCoplanar, CalculateVertNormals), and the
// GetAxisAlignedProjection utility from src/shared.h.

use std::collections::BTreeMap;

use crate::impl_mesh::ManifoldImpl;
use crate::linalg::{cross, dot, length2, lerp3, normalize, Vec2, Vec3};
use crate::math;
use crate::types::{next_halfedge, Halfedge, PolyVert, PolygonsIdx};

// -----------------------------------------------------------------------
// Proj2x3 — 2-row, 3-column projection matrix (drops one axis)
//
// Used to project 3D mesh positions onto a 2D plane for CCW tests and
// triangulation. Mirrors `mat2x3` in the C++ linalg.h library.
// -----------------------------------------------------------------------

/// A 2×3 projection matrix: maps Vec3 → Vec2 via dot products with two rows.
#[derive(Clone, Copy, Debug)]
pub struct Proj2x3 {
    pub row0: Vec3,
    pub row1: Vec3,
}

impl Proj2x3 {
    /// Apply the projection: `[dot(row0, v), dot(row1, v)]`.
    #[inline]
    pub fn apply(&self, v: Vec3) -> Vec2 {
        Vec2::new(dot(self.row0, v), dot(self.row1, v))
    }
}

// -----------------------------------------------------------------------
// GetAxisAlignedProjection
// -----------------------------------------------------------------------

/// Returns a projection matrix that drops the largest-magnitude axis of
/// `normal`, producing a 2D view aligned with the face plane.
///
/// Mirrors `GetAxisAlignedProjection` in `src/shared.h`.
pub fn get_axis_aligned_projection(normal: Vec3) -> Proj2x3 {
    let abs = Vec3::new(normal.x.abs(), normal.y.abs(), normal.z.abs());

    // mat3x2 columns (each col is a Vec3); transposed to get mat2x3 rows.
    let (row0, row1, xyz_max) = if abs.z > abs.x && abs.z > abs.y {
        // Drop Z, keep X and Y
        (Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, 1.0, 0.0), normal.z)
    } else if abs.y > abs.x {
        // Drop Y, keep Z and X
        (Vec3::new(0.0, 0.0, 1.0), Vec3::new(1.0, 0.0, 0.0), normal.y)
    } else {
        // Drop X, keep Y and Z
        (Vec3::new(0.0, 1.0, 0.0), Vec3::new(0.0, 0.0, 1.0), normal.x)
    };

    // If the dominant axis is negative, flip the first row so that the
    // projected winding order is consistent.
    if xyz_max < 0.0 {
        Proj2x3 {
            row0: Vec3::new(-row0.x, -row0.y, -row0.z),
            row1,
        }
    } else {
        Proj2x3 { row0, row1 }
    }
}

// -----------------------------------------------------------------------
// SetNormalsAndCoplanar
// -----------------------------------------------------------------------

/// Computes face normals and flood-fills coplanar face groups, then
/// calls `calculate_vert_normals` to compute per-vertex normals.
///
/// Mirrors `Manifold::Impl::SetNormalsAndCoplanar()` in `src/impl.cpp`.
pub fn set_normals_and_coplanar(mesh: &mut ManifoldImpl) {
    let t = crate::timing::start();
    let num_tri = mesh.num_tri();
    mesh.face_normal.resize(num_tri, Vec3::new(0.0, 0.0, 1.0));

    // Face normals and areas: each triangle depends only on its own corners,
    // so they are worked out in parallel.
    {
        let tri_ref = &mut mesh.mesh_relation.tri_ref;
        // Mark coplanarID as unset
        let n = num_tri.min(tri_ref.len());
        crate::par::maybe_par_for_each_mut(&mut tri_ref[..n], 50_000, |r| r.coplanar_id = -1);
    }
    let (tri_area2, new_normals): (Vec<f64>, Vec<Vec3>) = {
        let halfedge = &mesh.halfedge;
        let vert_pos = &mesh.vert_pos;
        let old_normals = &mesh.face_normal;
        crate::par::maybe_par_map_unzip(num_tri, 50_000, |tri| {
            if halfedge[3 * tri].start_vert < 0 {
                return (0.0, old_normals[tri]);
            }
            let v = vert_pos[halfedge[3 * tri].start_vert as usize];
            let n = cross(
                vert_pos[halfedge[3 * tri].end_vert as usize] - v,
                vert_pos[halfedge[3 * tri + 1].end_vert as usize] - v,
            );
            let normal = normalize(n);
            (
                length2(n),
                if normal.x.is_nan() {
                    Vec3::new(0.0, 0.0, 1.0)
                } else {
                    normal
                },
            )
        })
    };
    crate::scratch::give(std::mem::replace(&mut mesh.face_normal, new_normals));
    crate::timing::print("    snc: priority", t);
    let t = crate::timing::start();

    // Sort by area descending (largest triangles first → better coplanar seeds),
    // equal areas in index order. An area is a sum of squares, so it is never
    // negative and the bit patterns of the areas sort the same way as the
    // numbers: sorting (inverted bits, index) pairs gives that order, with no
    // two pairs equal. An area that is NaN has no place in an order, so the
    // comparison sort of the earlier code is kept for that case.
    let order: Vec<u32> = if tri_area2.iter().any(|a| a.is_nan()) {
        let mut idx: Vec<u32> = (0..num_tri as u32).collect();
        idx.sort_by(|&a, &b| {
            tri_area2[b as usize]
                .partial_cmp(&tri_area2[a as usize])
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        idx
    } else {
        let mut keyed: Vec<(u64, u32)> = crate::par::maybe_par_map(num_tri, 50_000, |tri| {
            (!tri_area2[tri].to_bits(), tri as u32)
        });
        crate::par::maybe_par_sort_distinct(&mut keyed, 50_000);
        let order = keyed.iter().map(|&(_, tri)| tri).collect();
        crate::scratch::give(keyed);
        order
    };
    crate::scratch::give(tri_area2);

    // Flood-fill coplanar groups from each unassigned face
    let mut interior_halfedges: Vec<usize> = Vec::new();
    for &tri in &order {
        let tri = tri as usize;
        if tri >= mesh.mesh_relation.tri_ref.len() {
            continue;
        }
        if mesh.mesh_relation.tri_ref[tri].coplanar_id >= 0 {
            continue;
        }

        mesh.mesh_relation.tri_ref[tri].coplanar_id = tri as i32;
        if mesh.halfedge[3 * tri].start_vert < 0 {
            continue;
        }

        let base = mesh.vert_pos[mesh.halfedge[3 * tri].start_vert as usize];
        let normal = mesh.face_normal[tri];

        interior_halfedges.clear();
        interior_halfedges.push(3 * tri);
        interior_halfedges.push(3 * tri + 1);
        interior_halfedges.push(3 * tri + 2);

        while let Some(h) = interior_halfedges.pop() {
            let paired = mesh.halfedge[h].paired_halfedge;
            if paired < 0 {
                continue;
            }
            let h2 = next_halfedge(paired) as usize;
            let h2_tri = h2 / 3;
            if h2_tri >= mesh.mesh_relation.tri_ref.len() {
                continue;
            }
            if mesh.mesh_relation.tri_ref[h2_tri].coplanar_id >= 0 {
                continue;
            }

            let v = mesh.vert_pos[mesh.halfedge[h2].end_vert as usize];
            if (dot(v - base, normal)).abs() < mesh.tolerance {
                mesh.mesh_relation.tri_ref[h2_tri].coplanar_id = tri as i32;
                mesh.face_normal[h2_tri] = normal;

                // Avoid re-pushing paired interior halfedges (cancel out)
                let last = interior_halfedges.last().copied();
                if last == Some(mesh.halfedge[h2].paired_halfedge as usize) {
                    interior_halfedges.pop();
                } else {
                    interior_halfedges.push(h2);
                }
                interior_halfedges.push(next_halfedge(h2 as i32) as usize);
            }
        }
    }

    crate::timing::print("    snc: flood", t);
    let t = crate::timing::start();
    calculate_vert_normals(mesh);
    crate::timing::print("    snc: vert_normals", t);
}

// -----------------------------------------------------------------------
// CalculateVertNormals
// -----------------------------------------------------------------------

/// Computes per-vertex normals as angle-weighted averages of incident face normals.
///
/// Mirrors `Manifold::Impl::CalculateVertNormals()` in `src/impl.cpp`.
pub fn calculate_vert_normals(mesh: &mut ManifoldImpl) {
    let num_vert = mesh.vert_pos.len();

    // For each vertex, find the first halfedge that starts there: the least
    // index, which is the same whatever order the halfedges are looked at in.
    let mut vert_first_edge: crate::scratch::Scratch<i32> =
        crate::scratch::Scratch::with_capacity(num_vert);
    vert_first_edge.resize(num_vert, i32::MAX);
    {
        // Walking the halfedges from the last to the first, the value left in
        // each slot is the least index: the same whatever order a parallel
        // pass would have looked in. One pass over the halfedges is cheaper
        // than starting threads for it.
        let halfedge = &mesh.halfedge;
        for i in (0..halfedge.len()).rev() {
            let sv = halfedge[i].start_vert;
            if sv >= 0 && (sv as usize) < num_vert {
                vert_first_edge[sv as usize] = i as i32;
            }
        }
    }

    // Each vertex's normal reads only shared mesh data and its own walk, so
    // the per-vertex work parallelizes with results identical to sequential
    // (the accumulation order WITHIN a vertex is the fixed ForVert walk).
    let halfedge = &mesh.halfedge;
    let vert_pos = &mesh.vert_pos;
    let face_normal = &mesh.face_normal;
    let normals: Vec<Vec3> = crate::par::maybe_par_map(num_vert, 10_000, |vert| {
        let first_edge = vert_first_edge[vert];
        if first_edge == i32::MAX {
            return Vec3::new(0.0, 0.0, 0.0);
        }

        let mut normal = Vec3::new(0.0, 0.0, 0.0);

        // ForVert equivalent: walk CW around the vertex. C++ ForVert (impl.h)
        // STEPS FIRST and calls func after, so first_edge is processed LAST.
        // The visit order fixes the float accumulation order of the
        // angle-weighted normal sum — it must match C++ bit-for-bit because
        // vertex normals feed the Boolean3 SOS tie-breaks.
        let mut current = first_edge as usize;
        loop {
            let paired = halfedge[current].paired_halfedge;
            if paired < 0 {
                break;
            }
            current = next_halfedge(paired) as usize;
            let h = &halfedge[current];
            let tri_verts = [
                h.start_vert as usize,
                h.end_vert as usize,
                halfedge[next_halfedge(current as i32) as usize].end_vert as usize,
            ];

            // Avoid degenerate triangles
            if tri_verts[0] < vert_pos.len()
                && tri_verts[1] < vert_pos.len()
                && tri_verts[2] < vert_pos.len()
            {
                let curr_edge_dir = vert_pos[tri_verts[1]] - vert_pos[tri_verts[0]];
                let prev_edge_dir = vert_pos[tri_verts[0]] - vert_pos[tri_verts[2]];
                let curr_len = length2(curr_edge_dir).sqrt();
                let prev_len = length2(prev_edge_dir).sqrt();

                if curr_len > 0.0 && prev_len > 0.0 {
                    let curr_norm = curr_edge_dir / curr_len;
                    let prev_norm = prev_edge_dir / prev_len;
                    if curr_norm.x.is_finite() && prev_norm.x.is_finite() {
                        let d = dot(prev_norm, curr_norm).clamp(-1.0, 1.0);
                        // Negate because prevEdge points into vert and currEdge points away
                        let phi = math::acos(-d);
                        if phi.is_finite() && current / 3 < face_normal.len() {
                            normal = normal + face_normal[current / 3] * phi;
                        }
                    }
                }
            }

            if current == first_edge as usize {
                break;
            }
        }

        let len = length2(normal).sqrt();
        if len > 0.0 {
            normal / len
        } else {
            Vec3::new(0.0, 0.0, 0.0)
        }
    });
    crate::scratch::give(std::mem::replace(&mut mesh.vert_normal, normals));
}

// -----------------------------------------------------------------------
// GetBarycentric — barycentric coordinates of point in triangle
// -----------------------------------------------------------------------

/// Compute barycentric coordinates of `v` with respect to triangle `tri_pos`.
/// Returns [u, v, w] where vertex i has weight uvw[i].
/// Returns exact 1.0 for vertices within `tolerance` of a triangle vertex,
/// and exact 0.0 for points within tolerance of an edge.
///
/// Mirrors `GetBarycentric` in `src/shared.h`.
pub fn get_barycentric(v: Vec3, tri_pos: [Vec3; 3], tolerance: f64) -> Vec3 {
    let edges = [
        tri_pos[2] - tri_pos[1],
        tri_pos[0] - tri_pos[2],
        tri_pos[1] - tri_pos[0],
    ];
    let d2 = [
        dot(edges[0], edges[0]),
        dot(edges[1], edges[1]),
        dot(edges[2], edges[2]),
    ];
    let long_side = if d2[0] > d2[1] && d2[0] > d2[2] {
        0
    } else if d2[1] > d2[2] {
        1
    } else {
        2
    };
    let cross_p = cross(edges[0], edges[1]);
    let area2 = dot(cross_p, cross_p);
    let tol2 = tolerance * tolerance;

    let mut uvw = Vec3::splat(0.0);
    for i in 0..3 {
        let dv = v - tri_pos[i];
        if dot(dv, dv) < tol2 {
            uvw = Vec3::splat(0.0);
            match i {
                0 => uvw.x = 1.0,
                1 => uvw.y = 1.0,
                _ => uvw.z = 1.0,
            }
            return uvw;
        }
    }

    if d2[long_side] < tol2 {
        // Degenerate point
        Vec3::new(1.0, 0.0, 0.0)
    } else if area2 > d2[long_side] * tol2 {
        // Triangle case
        for i in 0..3 {
            let j = (i + 1) % 3;
            let cross_pv = cross(edges[i], v - tri_pos[j]);
            let area2v = dot(cross_pv, cross_pv);
            let val = if area2v < d2[i] * tol2 {
                0.0
            } else {
                dot(cross_pv, cross_p)
            };
            match i {
                0 => uvw.x = val,
                1 => uvw.y = val,
                _ => uvw.z = val,
            }
        }
        let sum = uvw.x + uvw.y + uvw.z;
        uvw = uvw / sum;
        uvw
    } else {
        // Line case
        let next_v = (long_side + 1) % 3;
        let alpha = dot(v - tri_pos[next_v], edges[long_side]) / d2[long_side];
        uvw = Vec3::splat(0.0);
        let last_v = (next_v + 1) % 3;
        match next_v {
            0 => uvw.x = 1.0 - alpha,
            1 => uvw.y = 1.0 - alpha,
            _ => uvw.z = 1.0 - alpha,
        }
        match last_v {
            0 => uvw.x = alpha,
            1 => uvw.y = alpha,
            _ => uvw.z = alpha,
        }
        uvw
    }
}

// -----------------------------------------------------------------------
// AssembleHalfedges — group halfedges into polygon loops
// -----------------------------------------------------------------------

/// Given a slice of halfedges (from a polygonal face), group them into polygon
/// loops by following start_vert → end_vert chains. Returns a vec of polygon
/// loops, where each loop is a vec of halfedge indices (offset by
/// `start_halfedge_idx`).
///
/// Mirrors `AssembleHalfedges` in `src/face_op.cpp`.
pub fn assemble_halfedges(halfedges: &[Halfedge], start_halfedge_idx: i32) -> Vec<Vec<i32>> {
    // Build multimap: start_vert → local edge index
    let mut vert_edge: BTreeMap<i32, Vec<usize>> = BTreeMap::new();
    for (i, he) in halfedges.iter().enumerate() {
        vert_edge.entry(he.start_vert).or_default().push(i);
    }

    let mut polys: Vec<Vec<i32>> = Vec::new();
    let mut start_edge: usize = 0;
    let mut this_edge: usize = start_edge;

    loop {
        if this_edge == start_edge {
            // Find next unvisited edge
            if vert_edge.is_empty() {
                break;
            }
            let (&_vert, edges) = vert_edge.iter().next().unwrap();
            start_edge = edges[0];
            this_edge = start_edge;
            polys.push(Vec::new());
        }
        polys
            .last_mut()
            .unwrap()
            .push(start_halfedge_idx + this_edge as i32);
        let end_vert = halfedges[this_edge].end_vert;
        let edges = vert_edge.get_mut(&end_vert).expect("non-manifold edge");
        // Remove the first occurrence
        let pos = 0; // take first available
        this_edge = edges.remove(pos);
        if edges.is_empty() {
            vert_edge.remove(&end_vert);
        }
    }
    polys
}

/// Project polygon loops into 2D using projection matrix and vertex positions.
///
/// Mirrors `ProjectPolygons` in `src/face_op.cpp`.
pub fn project_polygons(
    polys: &[Vec<i32>],
    halfedge: &[Halfedge],
    vert_pos: &[Vec3],
    projection: &Proj2x3,
) -> PolygonsIdx {
    let mut polygons: PolygonsIdx = Vec::new();
    for poly in polys {
        let mut simple_poly = Vec::new();
        for &edge in poly {
            let vert = halfedge[edge as usize].start_vert;
            simple_poly.push(PolyVert {
                pos: projection.apply(vert_pos[vert as usize]),
                idx: edge,
            });
        }
        polygons.push(simple_poly);
    }
    polygons
}

// -----------------------------------------------------------------------
// Face2Tri — extracted to face_op_triangulate.rs
// -----------------------------------------------------------------------

#[path = "face_op_triangulate.rs"]
mod face_op_triangulate;
pub use face_op_triangulate::{face2tri, face2tri_ct, face2tri_face_refs};

// -----------------------------------------------------------------------
// ReorderHalfedges — canonical ordering within each triangle
// -----------------------------------------------------------------------

/// Reorders halfedges within each face so the one with the smallest start_vert
/// is first, then fixes paired_halfedge references.
///
/// Mirrors `Manifold::Impl::ReorderHalfedges` in `src/sort.cpp`.
pub fn reorder_halfedges(mesh: &mut ManifoldImpl) {
    /// Halfedge count from which both steps run in parallel. Each triangle is
    /// rotated on its own, and each new pair index is read from the rotated
    /// start and end verts only, so the order of the work changes nothing.
    const PAR_THRESHOLD: usize = 100_000;

    // Step 1: rotate each triangle so smallest start_vert is first
    crate::par::maybe_par_for_each_chunk_mut(&mut mesh.halfedge, 3, PAR_THRESHOLD, |tri| {
        if tri.len() < 3 {
            return;
        }
        let face = [tri[0], tri[1], tri[2]];
        if face[0].start_vert < 0 {
            return;
        }
        let mut index = 0;
        for i in 1..3 {
            if face[i].start_vert < face[index].start_vert {
                index = i;
            }
        }
        for i in 0..3 {
            tri[i] = face[(index + i) % 3];
        }
    });

    // Step 2: fix paired_halfedge references. `-2` leaves a halfedge as it is:
    // one after a removed halfedge in its triangle, or one without a pair.
    const KEEP: i32 = -2;
    let halfedge = &mesh.halfedge;
    let num_tri = halfedge.len() / 3;
    let paired = crate::scratch::Scratch::from_vec(crate::par::maybe_par_map(
        3 * num_tri,
        PAR_THRESHOLD,
        |edge| {
            let tri = edge / 3;
            // A collapsed triangle is skipped from its first removed halfedge.
            for before in 3 * tri..=edge {
                if halfedge[before].start_vert < 0 {
                    return KEEP;
                }
            }
            let curr = halfedge[edge];
            if curr.paired_halfedge < 0 {
                return KEEP; // unpaired halfedge
            }
            let opp_face = curr.paired_halfedge as usize / 3;
            let mut index = -1i32;
            for j in 0..3 {
                if curr.start_vert == halfedge[opp_face * 3 + j].end_vert {
                    index = j as i32;
                }
            }
            opp_face as i32 * 3 + index
        },
    ));
    crate::par::maybe_par_for_each_indexed_mut(
        &mut mesh.halfedge[..3 * num_tri],
        PAR_THRESHOLD,
        |edge, h| {
            if paired[edge] != KEEP {
                h.paired_halfedge = paired[edge];
            }
        },
    );
}

// -----------------------------------------------------------------------
// Slice & Project — cross-section operations on ManifoldImpl
// -----------------------------------------------------------------------

use crate::types::{Box as BBox, Polygons, SimplePolygon};

impl ManifoldImpl {
    /// Slice the mesh at the given Z height, returning 2D polygon loops.
    /// Mirrors `Manifold::Impl::Slice` in `src/face_op.cpp`.
    pub fn slice(&self, height: f64) -> Polygons {
        let num_tri = self.num_tri();
        if num_tri == 0 {
            return vec![];
        }

        // Build plane query box spanning the full XY extent at the given Z
        let mut plane = self.bbox;
        plane.min.z = height;
        plane.max.z = height;

        // Query the cached face BVH (C++ Slice uses collider_).
        let collider = &self.collider;

        // Find all triangles that straddle the slice plane. C++ holds them in
        // a `std::unordered_set<int>` and starts each contour at
        // `*tris.begin()`, whose order is implementation-defined. A BTreeSet
        // keeps the same algorithm with a pinned, documented order: each
        // contour starts at the lowest-indexed untraced triangle, and the
        // contours come out in that ascending order. Each contour is the same
        // cyclic vertex sequence as the C++, bit for bit and in the same
        // orientation; only the cycle's rotation and the contour order can
        // differ from a given C++ toolchain (MSVC, libstdc++ and libc++ each
        // iterate the set in their own order), so no single C++ output exists
        // to match. `Manifold::slice` unions the contours, which hides both.
        // Pinned by `cross_section::ctor_tests::test_raw_slice_matches_cpp_lerp_bits`
        // (cyclic comparison against MSVC) and
        // `test_raw_slice_contour_order_is_deterministic`. manifold-sharp must
        // start at the ascending triangle index too (a sorted set, or the
        // minimum remaining index), and use the `lerp` form below, to keep its
        // raw slice bit-equal.
        let mut tris: std::collections::BTreeSet<usize> = std::collections::BTreeSet::new();
        let query = vec![BBox::from_points(plane.min, plane.max)];
        collider.collisions_with_boxes(&query, false, |_query_idx, tri| {
            let mut min_z = f64::INFINITY;
            let mut max_z = f64::NEG_INFINITY;
            for j in 0..3 {
                let z = self.vert_pos[self.halfedge[3 * tri + j].start_vert as usize].z;
                min_z = min_z.min(z);
                max_z = max_z.max(z);
            }
            if min_z <= height && max_z > height {
                tris.insert(tri);
            }
        });

        // Trace polygon loops through intersected triangles
        let mut polys: Polygons = Vec::new();
        while !tris.is_empty() {
            let Some(&start_tri) = tris.first() else {
                break;
            };
            let mut poly: SimplePolygon = Vec::new();

            // Find the edge where the slice enters (above→below transition)
            let mut k = 0usize;
            for j in 0..3usize {
                let next_j = (j + 1) % 3;
                if self.vert_pos[self.halfedge[3 * start_tri + j].start_vert as usize].z > height
                    && self.vert_pos[self.halfedge[3 * start_tri + next_j].start_vert as usize].z
                        <= height
                {
                    k = next_j;
                    break;
                }
            }

            let mut tri = start_tri;
            loop {
                tris.remove(&tri);
                if self.vert_pos[self.halfedge[3 * tri + k].end_vert as usize].z <= height {
                    k = (k + 1) % 3;
                }

                let up = &self.halfedge[3 * tri + k];
                let below = self.vert_pos[up.start_vert as usize];
                let above = self.vert_pos[up.end_vert as usize];
                let a = (height - below.z) / (above.z - below.z);
                // C++ `vec2(la::lerp(below, above, a))` = below*(1-a) + above*a.
                let p = lerp3(below, above, a);
                poly.push(Vec2::new(p.x, p.y));

                let pair = up.paired_halfedge;
                tri = pair as usize / 3;
                k = ((pair as usize) % 3 + 1) % 3;

                if tri == start_tri {
                    break;
                }
            }

            polys.push(poly);
        }

        polys
    }

    /// Project the mesh silhouette onto the XY plane, returning 2D polygon loops.
    /// Mirrors `Manifold::Impl::Project` in `src/face_op.cpp`.
    pub fn project(&self) -> Polygons {
        if self.num_tri() == 0 || self.face_normal.is_empty() {
            return vec![];
        }

        let projection = get_axis_aligned_projection(Vec3::new(0.0, 0.0, 1.0));

        // Find cusp edges: silhouette edges where one adjacent face points up
        // and the other points down (z-component of normals)
        let mut cusps: Vec<crate::types::Halfedge> = Vec::new();
        for edge in &self.halfedge {
            let paired_face =
                self.halfedge[edge.paired_halfedge as usize].paired_halfedge as usize / 3;
            let this_face = edge.paired_halfedge as usize / 3;
            if self.face_normal[paired_face].z >= 0.0 && self.face_normal[this_face].z < 0.0 {
                cusps.push(*edge);
            }
        }

        if cusps.is_empty() {
            return vec![];
        }

        let loops = assemble_halfedges(&cusps, 0);
        let polys_indexed = project_polygons(&loops, &cusps, &self.vert_pos, &projection);

        // Convert PolygonsIdx to Polygons
        let mut polys: Polygons = Vec::new();
        for poly in &polys_indexed {
            let simple: SimplePolygon = poly.iter().map(|pv| pv.pos).collect();
            polys.push(simple);
        }

        polys
    }
}

// -----------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::impl_mesh::ManifoldImpl;
    use crate::linalg::Mat3x4;

    #[test]
    fn test_get_axis_aligned_projection_z() {
        // Normal primarily in Z: should project to XY plane
        let proj = get_axis_aligned_projection(Vec3::new(0.0, 0.0, 1.0));
        let v = Vec3::new(3.0, 4.0, 5.0);
        let p = proj.apply(v);
        assert!((p.x - 3.0).abs() < 1e-12);
        assert!((p.y - 4.0).abs() < 1e-12);
    }

    #[test]
    fn test_get_axis_aligned_projection_y() {
        // Normal primarily in Y: should project to ZX plane
        let proj = get_axis_aligned_projection(Vec3::new(0.0, 1.0, 0.0));
        let v = Vec3::new(3.0, 4.0, 5.0);
        let p = proj.apply(v);
        assert!((p.x - 5.0).abs() < 1e-12, "expected z=5, got {}", p.x);
        assert!((p.y - 3.0).abs() < 1e-12, "expected x=3, got {}", p.y);
    }

    #[test]
    fn test_get_axis_aligned_projection_x() {
        // Normal primarily in X: should project to YZ plane
        let proj = get_axis_aligned_projection(Vec3::new(1.0, 0.0, 0.0));
        let v = Vec3::new(3.0, 4.0, 5.0);
        let p = proj.apply(v);
        assert!((p.x - 4.0).abs() < 1e-12, "expected y=4, got {}", p.x);
        assert!((p.y - 5.0).abs() < 1e-12, "expected z=5, got {}", p.y);
    }

    #[test]
    fn test_get_axis_aligned_projection_negative_z() {
        // Normal primarily in -Z: row0 should be flipped
        let proj = get_axis_aligned_projection(Vec3::new(0.0, 0.0, -1.0));
        let v = Vec3::new(3.0, 4.0, 5.0);
        let p = proj.apply(v);
        // Flipped first row: x → -x
        assert!((p.x + 3.0).abs() < 1e-12, "expected -x=-3, got {}", p.x);
        assert!((p.y - 4.0).abs() < 1e-12);
    }

    #[test]
    fn test_set_normals_tetrahedron() {
        let mut m = ManifoldImpl::tetrahedron(&Mat3x4::identity());
        set_normals_and_coplanar(&mut m);
        // Every face normal should be unit length
        for n in &m.face_normal {
            let len = length2(*n).sqrt();
            assert!(
                (len - 1.0).abs() < 1e-10,
                "face normal not unit: len={}",
                len
            );
        }
        // Every vert normal should be nonzero (tetrahedron has no degenerate verts)
        for n in &m.vert_normal {
            let len = length2(*n).sqrt();
            assert!(len > 0.0, "vert normal is zero");
        }
    }

    #[test]
    fn test_set_normals_cube() {
        let mut m = ManifoldImpl::cube(&Mat3x4::identity());
        set_normals_and_coplanar(&mut m);
        assert_eq!(m.face_normal.len(), m.num_tri());
        // Coplanar IDs should be assigned
        let any_coplanar_id_set = m.mesh_relation.tri_ref.iter().any(|r| r.coplanar_id >= 0);
        assert!(any_coplanar_id_set, "no coplanar IDs were set");
    }
}
