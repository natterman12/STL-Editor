use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

use manifold_rust::boolean3;
use manifold_rust::csg_tree::CsgLeafNode;
use manifold_rust::edge_op;
use manifold_rust::face_op;
use manifold_rust::impl_mesh::{self, ManifoldImpl};
use manifold_rust::linalg::{IVec3, Mat3, Mat3x4, Vec3, mat3x4_to_mat4};
use manifold_rust::manifold::Manifold;
use manifold_rust::math;
use manifold_rust::types::{BooleanConfig, Box as Bounds, Error, K_PRECISION, OpType, Relation, TriRef, cosd, sind};

use crate::{Aabb, KernelError, MeshArrays, Shape2d};

/// A closed solid. Every operation returns a new solid and leaves its inputs alone. An
/// operation that cannot give a result (a cube with a negative size, say) gives an empty
/// solid: check [`Solid::is_empty`].
#[derive(Clone)]
pub struct Solid(Arc<Manifold>, Option<Arc<Pending>>);

/// What 1.x still holds for a solid made by [`Solid::transform_pending`]: a mesh and the
/// matrix that has not been applied to it yet.
struct Pending {
    base: Manifold,
    transform: Mat3x4,
}

fn v3(v: [f64; 3]) -> Vec3 {
    Vec3::new(v[0], v[1], v[2])
}

fn arr(v: Vec3) -> [f64; 3] {
    [v.x, v.y, v.z]
}

/// Our segment counts are unsigned; manifold-rust takes an `i32`, where 0 means "choose".
pub(crate) fn segments(n: u32) -> i32 {
    i32::try_from(n).unwrap_or(i32::MAX)
}

/// Manifold `DedupeEdges`: where more than one pair of triangles shares an edge, give all but
/// the first pair their own copy of a vertex.
///
/// This is the loop of Manifold 3.5.4 (the kernel of 1.x) with one bug fixed. 3.5.4 lists the
/// duplicates at the start of a pass and then repairs every one of them, but an earlier repair
/// in the same pass can leave a later entry no longer a duplicate, and repairing it anyway
/// moves triangle corners onto a copy of another vertex: the solid changes shape. Here such an
/// entry is skipped (see [`still_duplicated`]), as manifold-rust's own `dedupe_edges` does; the
/// next pass lists whatever is left. So a mesh with edges that more than two pairs of triangles
/// share can import as a different solid than in 1.x: see `tests/EXCEPTIONS.md`.
///
/// 3.5.4 repeats until a pass finds nothing. Here the passes are counted, so that a mesh on
/// which a repair keeps making new duplicates cannot hang the import: `false` means it gave up
/// (the mesh is then reported as not closed). Meshes that import need a handful of passes.
fn dedupe_edges(imp: &mut ManifoldImpl) -> bool {
    for _ in 0..DEDUPE_PASSES {
        let n = imp.halfedge.len();
        let live = |imp: &ManifoldImpl, e: usize| imp.halfedge[e].start_vert != -1 && imp.halfedge[e].end_vert != -1;
        let mut seen = vec![false; n];
        let mut duplicates: Vec<usize> = Vec::new();
        let mut orbit: Vec<usize> = Vec::new();
        let mut first: HashMap<i32, usize> = HashMap::new();
        for i in 0..n {
            if seen[i] || !live(imp, i) {
                continue;
            }
            // the halfedges that leave this vertex; for each end vertex the lowest one stays
            orbit.clear();
            first.clear();
            imp.for_vert(i, |e| orbit.push(e));
            for &e in &orbit {
                seen[e] = true;
                if live(imp, e) {
                    let low = first.entry(imp.halfedge[e].end_vert).or_insert(e);
                    *low = (*low).min(e);
                }
            }
            for &e in &orbit {
                if live(imp, e) && first[&imp.halfedge[e].end_vert] != e {
                    duplicates.push(e);
                }
            }
        }
        if duplicates.is_empty() {
            return true;
        }
        // above 10 000 halfedges 3.5.4 collects them on several threads and sorts the list
        if n > 10_000 {
            duplicates.sort_unstable();
            duplicates.dedup();
        }
        for e in duplicates {
            if still_duplicated(imp, e) {
                edge_op::dedupe_edge(imp, e);
            }
        }
    }
    false
}

/// Whether another halfedge that leaves the start vertex of `edge` ends at the same vertex as
/// `edge` does: whether `edge` is a duplicate right now.
fn still_duplicated(imp: &ManifoldImpl, edge: usize) -> bool {
    let end = imp.halfedge[edge].end_vert;
    if imp.halfedge[edge].start_vert < 0 || end < 0 {
        return false;
    }
    let mut current = edge;
    for _ in 0..=imp.halfedge.len() {
        let pair = imp.halfedge[current].paired_halfedge;
        if pair < 0 {
            return false;
        }
        // the next halfedge in the triangle of `pair`: it leaves the same vertex as `current`
        let pair = pair as usize;
        current = if pair % 3 == 2 { pair - 2 } else { pair + 1 };
        if current == edge {
            return false;
        }
        if imp.halfedge[current].end_vert == end {
            return true;
        }
    }
    false
}

/// The most passes [`dedupe_edges`] makes before it gives up.
const DEDUPE_PASSES: usize = 100;

/// Manifold 3.5.4 `CleanupTopology`. `false` when [`dedupe_edges`] gave up.
fn cleanup_topology(imp: &mut ManifoldImpl) -> bool {
    if imp.halfedge.is_empty() {
        return true;
    }
    edge_op::split_pinched_verts(imp);
    dedupe_edges(imp)
}

/// A mesh of positions as a kernel solid, step for step what Manifold 3.5.4 (the kernel of 1.x)
/// does in `Impl::Impl(const MeshGL&)`. manifold-rust 0.19.2 has its own importer, but it runs
/// these steps in another order and without the single-precision tolerance, so on a mesh with
/// pinched vertices or folded triangles it keeps triangles that 1.x removes. `m` has passed
/// `MeshArrays::validate`.
fn import_mesh(m: &MeshArrays) -> Manifold {
    let num_vert = m.positions.len() / 3;
    let num_tri = m.triangles.len() / 3;
    if num_vert < 4 || num_tri < 4 {
        return Manifold::make_empty(Error::NotManifold);
    }
    let mut imp = ManifoldImpl::new();
    imp.vert_pos = m.positions.chunks_exact(3).map(|p| Vec3::new(p[0] as f64, p[1] as f64, p[2] as f64)).collect();

    // one run: every triangle belongs to one freshly reserved mesh id
    let id = impl_mesh::reserve_ids(1) as i32;
    imp.mesh_relation.mesh_id_transform.insert(
        id,
        Relation { original_id: id, transform: Mat3x4::identity(), back_side: false, has_normals: false },
    );
    let mut tri_prop: Vec<IVec3> = Vec::with_capacity(num_tri);
    imp.mesh_relation.tri_ref.clear();
    for (i, t) in m.triangles.chunks_exact(3).enumerate() {
        if t[0] != t[1] && t[1] != t[2] && t[2] != t[0] {
            tri_prop.push(IVec3::new(t[0] as i32, t[1] as i32, t[2] as i32));
            imp.mesh_relation.tri_ref.push(TriRef { mesh_id: id, original_id: id, face_id: -1, coplanar_id: i as i32 });
        }
    }

    imp.create_halfedges(&tri_prop, &[]);
    if !imp.is_manifold() {
        return Manifold::make_empty(Error::NotManifold);
    }
    imp.calculate_bbox();
    // positions came in as f32, so the tolerance is at least f32 epsilon times the size
    imp.set_epsilon(-1.0, true);
    // pinched vertices are split before the normals are worked out
    if !cleanup_topology(&mut imp) {
        return Manifold::make_empty(Error::NotManifold);
    }
    imp.dedupe_prop_verts();
    imp.set_normals_and_coplanar();
    // RemoveDegenerates
    if !cleanup_topology(&mut imp) {
        return Manifold::make_empty(Error::NotManifold);
    }
    edge_op::collapse_short_edges(&mut imp, 0);
    edge_op::swap_degenerates(&mut imp, 0);
    face_op::calculate_vert_normals(&mut imp);
    imp.remove_unreferenced_verts();
    imp.sort_geometry();
    if !imp.is_finite() {
        return Manifold::make_empty(Error::NonFiniteVertex);
    }
    // a solid made from a mesh is never an original: the mesh is
    imp.mesh_relation.original_id = -1;
    Manifold::from_impl(imp)
}

/// Manifold 3.5.4 `Halfspace`: a cube big enough to cover `solid` on the side of the plane
/// `normal . p = offset` that the normal points to. `None` when the normal has no direction.
///
/// 3.5.4 (the kernel of 1.x) keeps the cutter's moves (translate, scale, translate, rotate) as
/// one matrix and applies it to the cube once. manifold-rust 0.19.2 applies each move as it
/// comes, so its cutter's corners are rounded four times and the cut vertices can differ from
/// 1.x in the last bit. That is enough to change a later boolean on a face that lies in the
/// cut (a connector pin that starts on a tilted cut face), so the matrix is built here as
/// 3.5.4 builds it.
fn halfspace(solid: &Manifold, normal: Vec3, offset: f64) -> Option<Manifold> {
    let length = |v: Vec3| (v.x * v.x + v.y * v.y + v.z * v.z).sqrt();
    let len = length(normal);
    if !(len.is_finite() && len > 0.0) {
        return None;
    }
    let n = normal / len;
    let bbox = solid.bounding_box();
    let centre = bbox.center();
    let size = length(Vec3::new(centre.x - n.x * offset, centre.y - n.y * offset, centre.z - n.z * offset))
        + 0.5 * length(bbox.size());
    // each move goes in front of the ones before it: `m * Mat4(transform)`
    let then = |m: Mat3x4, before: Mat3x4| m * mat3x4_to_mat4(before);
    let shift = |x: f64| {
        let mut m = Mat3x4::identity();
        m.w = m.w + Vec3::new(x, 0.0, 0.0);
        m
    };
    let mut scale = Mat3x4::identity();
    scale.x.x = size;
    scale.y.y = size;
    scale.z.z = size;
    let y_deg = -math::asin(n.z).to_degrees();
    let z_deg = math::atan2(n.y, n.x).to_degrees();
    let (sx, cx) = (sind(0.0), cosd(0.0));
    let (sy, cy) = (sind(y_deg), cosd(y_deg));
    let (sz, cz) = (sind(z_deg), cosd(z_deg));
    let rx = Mat3::from_cols(Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, cx, sx), Vec3::new(0.0, -sx, cx));
    let ry = Mat3::from_cols(Vec3::new(cy, 0.0, -sy), Vec3::new(0.0, 1.0, 0.0), Vec3::new(sy, 0.0, cy));
    let rz = Mat3::from_cols(Vec3::new(cz, sz, 0.0), Vec3::new(-sz, cz, 0.0), Vec3::new(0.0, 0.0, 1.0));
    let r = rz * ry * rx;
    let turn = Mat3x4::from_cols(r.x, r.y, r.z, Vec3::new(0.0, 0.0, 0.0));

    let mut t = then(shift(1.0), Mat3x4::identity());
    t = then(scale, t);
    t = then(shift(offset), t);
    t = then(turn, t);
    Some(Manifold::cube(Vec3::new(2.0, 2.0, 2.0), true).transform(&t))
}

/// The cutter for a plane cut of `solid`, when this crate builds it (see [`halfspace`]).
/// `None` leaves the cut to manifold-rust: an empty or failed solid, or a normal of no length.
fn plane_cutter(solid: &Manifold, normal: [f64; 3], offset: f64) -> Option<Manifold> {
    if solid.status() != Error::NoError || solid.is_empty() {
        return None;
    }
    halfspace(solid, v3(normal), offset)
}

/// Manifold 3.5.4 `BatchUnion` for parts of which some have a matrix still to apply
/// ([`Solid::transform_pending`]). Parts whose boxes do not overlap are put together first
/// (`CsgLeafNode::Compose`), and there 3.5.4 works out the precision of a part with a pending
/// matrix in its own way; manifold-rust 0.19.2 applies the matrix first and so gives the group
/// another precision, and the union that follows other triangles. See [`compose_leaves`].
fn union_pending(parts: &[Solid]) -> Manifold {
    const MAX_UNION: usize = 1000;
    let mut children: Vec<CsgLeafNode> = parts
        .iter()
        .map(|s| match &s.1 {
            Some(p) => CsgLeafNode::with_transform(p.base.as_impl().clone(), p.transform),
            None => CsgLeafNode::new(s.0.as_impl().clone()),
        })
        .collect();
    while children.len() > 1 {
        // the last thousand at a time, so that the overlap check stays small
        let start = children.len().saturating_sub(MAX_UNION);
        let chunk: Vec<CsgLeafNode> = children.drain(start..).collect();
        let boxes: Vec<Bounds> = chunk.iter().map(leaf_box).collect();
        // each part joins the first group in which it overlaps nothing
        let mut groups: Vec<Vec<usize>> = Vec::new();
        for i in 0..chunk.len() {
            match groups.iter_mut().find(|g| !g.iter().any(|&j| boxes[i].does_overlap_box(&boxes[j]))) {
                Some(g) => g.push(i),
                None => groups.push(vec![i]),
            }
        }
        let composed: Vec<CsgLeafNode> = groups
            .iter()
            .map(|g| match g.as_slice() {
                [one] => chunk[*one].clone(),
                _ => compose_leaves(&g.iter().map(|&i| &chunk[i]).collect::<Vec<_>>()),
            })
            .collect();
        children.push(batch_add(composed));
        let last = children.len() - 1;
        children.swap(0, last);
    }
    match children.pop() {
        Some(leaf) => Manifold::from_impl(leaf.get_impl()),
        None => Manifold::empty(),
    }
}

/// A part of [`union_plain`]: a mesh lent by the caller, or one made on the way.
enum Part<'a> {
    Lent(&'a ManifoldImpl),
    Made(ManifoldImpl),
}

impl Part<'_> {
    fn mesh(&self) -> &ManifoldImpl {
        match self {
            Part::Lent(m) => m,
            Part::Made(m) => m,
        }
    }
}

/// The order of `BatchBoolean`'s heap: the most vertices first, of equals the one added last.
struct HeapPart<'a>(Part<'a>, u64);

impl PartialEq for HeapPart<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == std::cmp::Ordering::Equal
    }
}
impl Eq for HeapPart<'_> {}
impl PartialOrd for HeapPart<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for HeapPart<'_> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.mesh().num_vert().cmp(&other.0.mesh().num_vert()).then(self.1.cmp(&other.1))
    }
}

/// manifold-rust's union of a list of meshes (`CsgNode` with `OpType::Add`: `batch_union`, then
/// `batch_boolean`), step for step, but on the meshes as the caller holds them. The tree takes
/// its leaves by value, so building it copied every part first. Here a part is copied only when
/// it is put together with others without a union (`compose_meshes`), as before. The results are
/// the same triangles. Needs two parts or more, none with a matrix still to apply.
fn union_plain(parts: Vec<&ManifoldImpl>) -> ManifoldImpl {
    const MAX_UNION: usize = 1000;
    let engine = BooleanConfig::default_engine();
    let add = |a: &Part, b: &Part| Part::Made(boolean3::boolean_dispatch(a.mesh(), b.mesh(), OpType::Add, engine, None));

    let mut children: Vec<Part> = parts.into_iter().map(Part::Lent).collect();
    while children.len() > 1 {
        // the last thousand at a time, so that the overlap check stays small
        let start = children.len().saturating_sub(MAX_UNION);
        let mut chunk: Vec<Option<Part>> = children.drain(start..).map(Some).collect();
        let boxes: Vec<Bounds> = chunk.iter().map(|c| c.as_ref().unwrap().mesh().bbox).collect();
        // each part joins the first group in which it overlaps nothing
        let mut groups: Vec<Vec<usize>> = Vec::new();
        for i in 0..chunk.len() {
            match groups.iter_mut().find(|g| !g.iter().any(|&j| boxes[i].does_overlap_box(&boxes[j]))) {
                Some(g) => g.push(i),
                None => groups.push(vec![i]),
            }
        }
        let mut results: Vec<Part> = Vec::with_capacity(groups.len());
        for g in &groups {
            if let [one] = g.as_slice() {
                results.push(chunk[*one].take().unwrap());
            } else {
                let meshes: Vec<ManifoldImpl> = g.iter().map(|&i| chunk[i].as_ref().unwrap().mesh().clone()).collect();
                results.push(Part::Made(boolean3::compose_meshes(&meshes)));
            }
        }

        // BatchBoolean(Add) of the results
        let result = match results.len() {
            1 => results.pop().unwrap(),
            2 => {
                let b = results.pop().unwrap();
                let a = results.pop().unwrap();
                add(&a, &b)
            }
            n => {
                let mut next_serial = n as u64;
                let mut heap: std::collections::BinaryHeap<HeapPart> =
                    results.into_iter().zip(0u64..).map(|(p, i)| HeapPart(p, i)).collect();
                // up to four pairs a round, chosen before any runs; the results go back in
                // together, in pair order
                while heap.len() > 1 {
                    let mut pairs: Vec<(HeapPart, HeapPart)> = Vec::with_capacity(4);
                    for _ in 0..4 {
                        if heap.len() <= 1 {
                            break;
                        }
                        let a = heap.pop().unwrap();
                        let b = heap.pop().unwrap();
                        pairs.push((a, b));
                    }
                    let done = manifold_rust::par::maybe_par_map(pairs.len(), 2, |i| {
                        boolean3::boolean_dispatch(pairs[i].0 .0.mesh(), pairs[i].1 .0.mesh(), OpType::Add, engine, None)
                    });
                    for m in done {
                        heap.push(HeapPart(Part::Made(m), next_serial));
                        next_serial += 1;
                    }
                }
                heap.pop().unwrap().0
            }
        };
        // the new, complicated part goes to the front, and the old front to the back
        children.push(result);
        let last = children.len() - 1;
        children.swap(0, last);
    }
    match children.pop() {
        Some(Part::Made(m)) => m,
        Some(Part::Lent(m)) => m.clone(),
        None => ManifoldImpl::new(),
    }
}

/// Manifold 3.5.4 `CsgLeafNode::GetBoundingBox`: a box round a part without applying its
/// pending matrix (the box of the moved box, so it can be larger than the part). The sums
/// are in the order 3.5.4 makes them: manifold-rust 0.19.2 moves the centre and the half
/// sizes instead, which rounds differently, and two parts that touch (a pin and its tip) can
/// then pass for apart and be put together without a union.
fn leaf_box(node: &CsgLeafNode) -> Bounds {
    let bbox = node.p_impl.bbox;
    let t = node.transform;
    if t == Mat3x4::identity() {
        return bbox;
    }
    let cols = [arr(t.x), arr(t.y), arr(t.z)];
    let (lo, hi) = (arr(bbox.min), arr(bbox.max));
    let mut min = arr(t.w);
    let mut max = min;
    for i in 0..3 {
        for j in 0..3 {
            let (a, b) = (cols[j][i] * lo[j], cols[j][i] * hi[j]);
            // std::min(a, b) and std::max(a, b): the first one unless the second is past it
            min[i] += if b < a { b } else { a };
            max[i] += if a < b { b } else { a };
        }
    }
    Bounds { min: v3(min), max: v3(max) }
}

/// Manifold 3.5.4 `CsgLeafNode::Compose`: parts that do not overlap as one mesh. Where a part
/// has a pending matrix, 3.5.4 differs from applying the matrix first (which is what
/// manifold-rust 0.19.2 does) in two ways, both put right here:
///
/// - the vertices and triangles are sorted inside the union of the parts' loose boxes
///   ([`leaf_box`]), not inside the box of the moved vertices, so they come in another order,
///   and the union that follows then gives other triangles;
/// - the epsilon of a part is scaled by how much its box grows under the matrix (the box of
///   the two moved corners, as 3.5.4 takes it) and is at least the precision at that box; the
///   tolerance is that of the meshes before their matrices.
///
/// For parts without a pending matrix this is what manifold-rust's `compose_meshes` gives.
fn compose_leaves(nodes: &[&CsgLeafNode]) -> CsgLeafNode {
    let (mut epsilon, mut tolerance) = (-1.0f64, -1.0f64);
    let mut loose: Option<Bounds> = None;
    for node in nodes {
        let imp = &node.p_impl;
        if imp.status != Error::NoError {
            return CsgLeafNode::new(Manifold::make_empty(imp.status).into_impl());
        }
        let old_scale = imp.bbox.scale();
        let new_scale = imp.bbox.transform(&node.transform).scale();
        let mut e = imp.epsilon * f64::max(1.0, new_scale / old_scale);
        e = e.max(K_PRECISION * new_scale);
        if !e.is_finite() {
            e = -1.0;
        }
        epsilon = epsilon.max(e);
        tolerance = tolerance.max(imp.tolerance);
        let b = leaf_box(node);
        loose = Some(match loose {
            None => b,
            Some(a) => Bounds {
                min: Vec3::new(a.min.x.min(b.min.x), a.min.y.min(b.min.y), a.min.z.min(b.min.z)),
                max: Vec3::new(a.max.x.max(b.max.x), a.max.y.max(b.max.y), a.max.z.max(b.max.z)),
            },
        });
    }
    let meshes: Vec<ManifoldImpl> = nodes.iter().map(|n| n.get_impl()).collect();
    let mut all = boolean3::compose_meshes(&meshes);
    if let Some(loose) = loose
        && nodes.iter().any(|n| n.transform != Mat3x4::identity())
    {
        // sorting again leaves the box as the sort found it: the box of the triangles
        all.bbox = loose;
        all.sort_geometry();
    }
    all.epsilon = epsilon;
    all.tolerance = tolerance;
    CsgLeafNode::new(all)
}

/// Manifold 3.5.4 `BatchBoolean(Add)`: unite two at a time, the two with the most vertices
/// first (of equals, the one added last), up to four pairs a round; the results of a round go
/// back in together.
fn batch_add(mut nodes: Vec<CsgLeafNode>) -> CsgLeafNode {
    let add = |a: &CsgLeafNode, b: &CsgLeafNode| {
        let engine = BooleanConfig::default_engine();
        CsgLeafNode::new(boolean3::boolean_dispatch(&a.get_impl(), &b.get_impl(), OpType::Add, engine, None))
    };
    match nodes.len() {
        0 => return CsgLeafNode::empty(),
        1 => return nodes.remove(0),
        2 => return add(&nodes[0], &nodes[1]),
        _ => {}
    }
    let mut next_serial = nodes.len() as u64;
    let mut heap: Vec<(CsgLeafNode, u64)> = nodes.into_iter().zip(0u64..).collect();
    // the order (vertex count, then serial) has no ties, so this is the heap's order
    let pop = |heap: &mut Vec<(CsgLeafNode, u64)>| {
        let mut top = 0;
        for i in 1..heap.len() {
            if (heap[i].0.num_vert(), heap[i].1) > (heap[top].0.num_vert(), heap[top].1) {
                top = i;
            }
        }
        heap.swap_remove(top).0
    };
    while heap.len() > 1 {
        let mut pairs = Vec::with_capacity(4);
        while pairs.len() < 4 && heap.len() > 1 {
            let a = pop(&mut heap);
            let b = pop(&mut heap);
            pairs.push((a, b));
        }
        for (a, b) in &pairs {
            heap.push((add(a, b), next_serial));
            next_serial += 1;
        }
    }
    match heap.pop() {
        Some((leaf, _)) => leaf,
        None => CsgLeafNode::empty(),
    }
}

impl Solid {
    pub(crate) fn wrap(m: Manifold) -> Solid {
        Solid(Arc::new(m), None)
    }

    fn inner(parts: &[Solid]) -> Vec<Manifold> {
        parts.iter().map(|s| (*s.0).clone()).collect()
    }

    /// A box. With `center` it sits around the origin, otherwise its corner is at the origin.
    pub fn cube(size: [f64; 3], center: bool) -> Solid {
        Solid::wrap(Manifold::cube(v3(size), center))
    }

    /// A sphere around the origin. `segments` is the count around the equator; 0 lets the
    /// kernel choose.
    pub fn sphere(radius: f64, segments: u32) -> Solid {
        Solid::wrap(Manifold::sphere(radius, self::segments(segments)))
    }

    /// A cylinder or cone along Z. With `center` it is centred on z = 0, otherwise it stands
    /// on z = 0.
    pub fn cylinder(height: f64, r_low: f64, r_high: f64, segments: u32, center: bool) -> Solid {
        Solid::wrap(Manifold::cylinder_centered(height, r_low, r_high, self::segments(segments), center))
    }

    /// Build a solid from a mesh, like `manifold_from_arrays` in 1.x. `Invalid` means the
    /// arrays are wrong (checked before the kernel sees them); `NotClosed` means the kernel
    /// would not accept the mesh as a closed solid. It does not panic.
    pub fn from_mesh(m: &MeshArrays) -> Result<Solid, KernelError> {
        m.validate()?;
        // The arrays are well formed here; this is for a kernel bug on some odd mesh. On wasm32
        // a panic still aborts (see rust/tests/EXCEPTIONS.md).
        let built = catch_unwind(AssertUnwindSafe(|| import_mesh(m)))
            .map_err(|_| KernelError::Invalid("the kernel could not read this mesh".into()))?;
        if built.status() != Error::NoError || built.is_empty() {
            return Err(KernelError::NotClosed);
        }
        Ok(Solid::wrap(built))
    }

    /// The triangles of the solid: positions only, no normals or other per-vertex values.
    pub fn to_mesh(&self) -> MeshArrays {
        let gl = self.0.get_mesh_gl(-1);
        let stride = gl.num_prop as usize;
        let positions = if stride <= 3 {
            gl.vert_properties
        } else {
            gl.vert_properties.chunks_exact(stride).flat_map(|v| [v[0], v[1], v[2]]).collect()
        };
        MeshArrays { positions, triangles: gl.tri_verts }
    }

    pub fn union(&self, o: &Solid) -> Solid {
        Solid::wrap(self.0.union(&o.0))
    }

    /// This solid with `o` cut away.
    pub fn difference(&self, o: &Solid) -> Solid {
        Solid::wrap(self.0.difference(&o.0))
    }

    pub fn intersection(&self, o: &Solid) -> Solid {
        Solid::wrap(self.0.intersection(&o.0))
    }

    /// The union of all the parts; empty for an empty list.
    ///
    /// This is Manifold 3.5.4 `BatchBoolean(parts, Add)`, which 1.x calls: parts whose boxes
    /// do not overlap are put together without a boolean, and the rest are united two at a
    /// time, the ones with the most vertices first. manifold-rust's own `batch_boolean` unites
    /// them in list order instead, which gives other triangles (and, where three or more parts
    /// overlap, a slightly different shape) than 1.x.
    ///
    /// The order of the parts matters for the triangles. A chain `a + b + c` that 1.x leaves
    /// to the kernel (nothing reads `a + b` first) is this union of the parts in reverse:
    /// `[c, b, a]`. So is the cutter of a chain `a - b - c`: the union of `[c, b]`.
    pub fn union_all(parts: &[Solid]) -> Solid {
        match parts {
            [] => Solid::wrap(Manifold::empty()),
            [one] => one.clone(),
            _ if parts.iter().any(|s| s.1.is_some()) => Solid::wrap(union_pending(parts)),
            _ => Solid::wrap(Manifold::from_impl(union_plain(parts.iter().map(|s| s.0.as_impl()).collect()))),
        }
    }

    pub fn translate(&self, v: [f64; 3]) -> Solid {
        Solid::wrap(self.0.translate(v3(v)))
    }

    /// Rotate by degrees about X, then Y, then Z.
    pub fn rotate_deg(&self, xyz: [f64; 3]) -> Solid {
        Solid::wrap(self.0.rotate(xyz[0], xyz[1], xyz[2]))
    }

    pub fn scale(&self, v: [f64; 3]) -> Solid {
        Solid::wrap(self.0.scale(v3(v)))
    }

    /// Mirror across the plane through the origin with this normal.
    pub fn mirror(&self, normal: [f64; 3]) -> Solid {
        Solid::wrap(self.0.mirror(v3(normal)))
    }

    /// Apply a 3x4 matrix given as three rows of four: `[r00, r01, r02, tx]` and so on.
    pub fn transform(&self, m: [[f64; 4]; 3]) -> Solid {
        let col = |c: usize| Vec3::new(m[0][c], m[1][c], m[2][c]);
        Solid::wrap(self.0.transform(&Mat3x4::from_cols(col(0), col(1), col(2), col(3))))
    }

    /// [`Solid::transform`] for a solid that 1.x passes on without ever looking at it: its
    /// `Manifold.transform` only notes the matrix, and the kernel applies it when it first
    /// needs the triangles. The solid is the same one that `transform` gives. The difference
    /// shows in [`Solid::union_all`], which treats a part made this way as Manifold 3.5.4
    /// treats a part whose matrix is still to apply (see `union_pending`). Any other operation
    /// on it gives an ordinary solid.
    pub fn transform_pending(&self, m: [[f64; 4]; 3]) -> Solid {
        let col = |c: usize| Vec3::new(m[0][c], m[1][c], m[2][c]);
        let mut transform = Mat3x4::from_cols(col(0), col(1), col(2), col(3));
        let base = match &self.1 {
            // a second matrix goes in front of the first, and the two are applied as one
            Some(p) => {
                transform = transform * mat3x4_to_mat4(p.transform);
                p.base.clone()
            }
            None => (*self.0).clone(),
        };
        Solid(Arc::new(base.transform(&transform)), Some(Arc::new(Pending { base, transform })))
    }

    /// Split triangles until no edge is longer than `len`. The shape does not change.
    pub fn refine_to_length(&self, len: f64) -> Solid {
        Solid::wrap(self.0.refine_to_length(len))
    }

    /// Merge nearly coplanar triangles within `tolerance` (1.x `Manifold.simplify`).
    pub fn simplify(&self, tolerance: f64) -> Solid {
        Solid::wrap(self.0.simplify(tolerance))
    }

    /// Cut by the plane `normal . p = offset`. The first solid is the side the normal points
    /// to, the second is the other side.
    pub fn split_by_plane(&self, normal: [f64; 3], offset: f64) -> (Solid, Solid) {
        let (a, b) = match plane_cutter(&self.0, normal, offset) {
            // one pass finds where the plane crosses the mesh and builds both halves
            Some(cutter) => self.0.split(&cutter),
            None => self.0.split_by_plane(v3(normal), offset),
        };
        (Solid::wrap(a), Solid::wrap(b))
    }

    /// Keep only the side of the plane `normal . p = offset` that the normal points to.
    pub fn trim_by_plane(&self, normal: [f64; 3], offset: f64) -> Solid {
        Solid::wrap(match plane_cutter(&self.0, normal, offset) {
            Some(cutter) => self.0.intersection(&cutter),
            None => self.0.trim_by_plane(v3(normal), offset),
        })
    }

    /// The convex hull of this solid.
    pub fn hull(&self) -> Solid {
        Solid::wrap(self.0.convex_hull())
    }

    /// The convex hull of a set of points.
    pub fn hull_points(points: &[[f64; 3]]) -> Solid {
        let points: Vec<Vec3> = points.iter().map(|&p| v3(p)).collect();
        Solid::wrap(Manifold::hull(&points))
    }

    /// The Minkowski sum of this solid and `o` (1.x `Manifold.minkowski_sum`).
    pub fn minkowski_sum(&self, o: &Solid) -> Solid {
        Solid::wrap(self.0.minkowski_sum(&o.0))
    }

    /// The Minkowski difference of this solid and `o` (1.x `Manifold.minkowski_difference`).
    pub fn minkowski_difference(&self, o: &Solid) -> Solid {
        Solid::wrap(self.0.minkowski_difference(&o.0))
    }

    /// The separate bodies of this solid, largest volume first. Bodies of equal volume keep
    /// the order the kernel gave them, so the result is the same on every run.
    pub fn decompose(&self) -> Vec<Solid> {
        let mut parts: Vec<(f64, Solid)> = self.kernel_parts().into_iter().map(|m| (m.volume(), Solid::wrap(m))).collect();
        parts.sort_by(|a, b| b.0.total_cmp(&a.0));
        parts.into_iter().map(|(_, s)| s).collect()
    }

    /// The separate bodies of this solid in the kernel's own order (the order of each body's
    /// first vertex inside the kernel), which is the order 1.x gets from `Manifold.decompose()`.
    /// A union of the parts can give different triangles for a different order, so code that
    /// follows 1.x step by step (Repair) uses this one.
    pub fn decompose_in_kernel_order(&self) -> Vec<Solid> {
        self.kernel_parts().into_iter().map(Solid::wrap).collect()
    }

    /// Manifold 3.5.4 `Decompose`. There every body inherits the precision of the solid it
    /// came from (`Finish` calls `SetEpsilon(epsilon_)`); manifold-rust 0.19.2 passes on the
    /// tolerance but leaves each body's epsilon at -1, and a boolean on such bodies can then
    /// give a wrong shape (two overlapping blocks with holes lose part of a hole). This puts
    /// the epsilon back.
    fn kernel_parts(&self) -> Vec<Manifold> {
        let epsilon = self.0.as_impl().epsilon;
        let parts = self.0.decompose();
        if parts.len() < 2 {
            // one body is the solid itself, as it was
            return parts;
        }
        parts
            .into_iter()
            .map(|m| {
                let mut imp = m.into_impl();
                imp.set_epsilon(epsilon, false);
                Manifold::from_impl(imp)
            })
            .collect()
    }

    /// Put bodies that do not overlap into one solid, without the cost of a union.
    pub fn compose(parts: &[Solid]) -> Solid {
        Solid::wrap(Manifold::compose(&Solid::inner(parts)))
    }

    /// True when both solids are the same shared object (a clone and its original).
    pub fn shares_with(&self, other: &Solid) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    pub fn volume(&self) -> f64 {
        self.0.volume()
    }

    /// The volume with its sign: negative for a solid that is inside out (its triangles face
    /// inwards), as 1.x `Manifold.volume()` gives it. [`Solid::volume`] is never negative.
    pub fn signed_volume(&self) -> f64 {
        let imp = self.0.as_impl();
        // Kahan sum of the tetrahedra between each triangle and the origin
        let (mut sum, mut comp) = (0.0f64, 0.0f64);
        for t in imp.halfedge.chunks_exact(3) {
            if t[0].start_vert < 0 {
                continue;
            }
            let p = |k: usize| imp.vert_pos[t[k].start_vert as usize];
            let (a, b, c) = (p(0), p(1), p(2));
            let v = a.x * (b.y * c.z - b.z * c.y) + a.y * (b.z * c.x - b.x * c.z) + a.z * (b.x * c.y - b.y * c.x);
            let y = v - comp;
            let total = sum + y;
            comp = (total - sum) - y;
            sum = total;
        }
        sum / 6.0
    }

    pub fn surface_area(&self) -> f64 {
        self.0.surface_area()
    }

    pub fn bounding_box(&self) -> Aabb {
        let b = self.0.bounding_box();
        Aabb { min: arr(b.min), max: arr(b.max) }
    }

    pub fn num_tri(&self) -> usize {
        self.0.num_tri()
    }

    pub fn num_vert(&self) -> usize {
        self.0.num_vert()
    }

    /// The number of handles (a ball is 0, a ring is 1).
    pub fn genus(&self) -> i32 {
        self.0.genus()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The outline of the solid where the plane z = `z` cuts it.
    pub fn slice(&self, z: f64) -> Shape2d {
        Shape2d::from_cross_section(&self.0.slice(z))
    }

    /// The silhouette of the solid seen from above (along z): 1.x `Manifold.project()`.
    pub fn project(&self) -> Shape2d {
        Shape2d::from_cross_section(&self.0.project())
    }
}
