use crate::KernelError;

/// A triangle mesh as flat arrays: `positions` is x, y, z per vertex and `triangles` is three
/// vertex indices per triangle, counter-clockwise seen from outside.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MeshArrays {
    pub positions: Vec<f32>,
    pub triangles: Vec<u32>,
}

/// An axis-aligned bounding box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    pub min: [f64; 3],
    pub max: [f64; 3],
}

impl MeshArrays {
    /// The checks that do not need the kernel. `Solid::from_mesh` runs them first, so
    /// manifold-rust only ever sees arrays it can index safely.
    pub(crate) fn validate(&self) -> Result<(), KernelError> {
        let invalid = |why: String| Err(KernelError::Invalid(why));
        if self.positions.len() % 3 != 0 {
            return invalid(format!("{} position values is not a multiple of 3", self.positions.len()));
        }
        if self.triangles.len() % 3 != 0 {
            return invalid(format!("{} triangle indices is not a multiple of 3", self.triangles.len()));
        }
        if self.positions.is_empty() || self.triangles.is_empty() {
            return invalid("the mesh is empty".into());
        }
        if let Some(i) = self.positions.iter().position(|p| !p.is_finite()) {
            return invalid(format!("vertex {} has a coordinate that is not a number", i / 3));
        }
        let vertices = self.positions.len() / 3;
        if vertices > u32::MAX as usize {
            return invalid(format!("{vertices} vertices is more than the kernel can index"));
        }
        if let Some(i) = self.triangles.iter().position(|&v| v as usize >= vertices) {
            return invalid(format!(
                "triangle {} uses vertex {} but there are only {vertices} vertices",
                i / 3,
                self.triangles[i]
            ));
        }
        Ok(())
    }
}
