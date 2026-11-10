use std::sync::Arc;
use std::panic::{AssertUnwindSafe, catch_unwind};

use manifold_rust::linalg::{Mat3x4, Vec3};
use manifold_rust::manifold::Manifold;
use manifold_rust::types::{Error, MeshGL, OpType};

use crate::{Aabb, KernelError, MeshArrays, Shape2d};

/// A closed solid. Every operation returns a new solid and leaves its inputs alone. An
/// operation that cannot give a result (a cube with a negative size, say) gives an empty
/// solid: check [`Solid::is_empty`].
#[derive(Clone)]
pub struct Solid(Arc<Manifold>);

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

impl Solid {
    pub(crate) fn wrap(m: Manifold) -> Solid {
        Solid(Arc::new(m))
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
        let gl = MeshGL {
            num_prop: 3,
            vert_properties: m.positions.clone(),
            tri_verts: m.triangles.clone(),
            ..Default::default()
        };
        // The arrays are well formed here; this is for a kernel bug on some odd mesh. On wasm32
        // a panic still aborts (see rust/tests/EXCEPTIONS.md).
        let built = catch_unwind(AssertUnwindSafe(|| Manifold::from_mesh_gl(&gl)))
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
    pub fn union_all(parts: &[Solid]) -> Solid {
        Solid::wrap(Manifold::batch_boolean(&Solid::inner(parts), OpType::Add))
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

    /// Split triangles until no edge is longer than `len`. The shape does not change.
    pub fn refine_to_length(&self, len: f64) -> Solid {
        Solid::wrap(self.0.refine_to_length(len))
    }

    /// Cut by the plane `normal . p = offset`. The first solid is the side the normal points
    /// to, the second is the other side.
    pub fn split_by_plane(&self, normal: [f64; 3], offset: f64) -> (Solid, Solid) {
        let (a, b) = self.0.split_by_plane(v3(normal), offset);
        (Solid::wrap(a), Solid::wrap(b))
    }

    /// Keep only the side of the plane `normal . p = offset` that the normal points to.
    pub fn trim_by_plane(&self, normal: [f64; 3], offset: f64) -> Solid {
        Solid::wrap(self.0.trim_by_plane(v3(normal), offset))
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

    /// The separate bodies of this solid, largest volume first. Bodies of equal volume keep
    /// the order the kernel gave them, so the result is the same on every run.
    pub fn decompose(&self) -> Vec<Solid> {
        let mut parts: Vec<(f64, Solid)> = self.0.decompose().into_iter().map(|m| (m.volume(), Solid::wrap(m))).collect();
        parts.sort_by(|a, b| b.0.total_cmp(&a.0));
        parts.into_iter().map(|(_, s)| s).collect()
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
}
