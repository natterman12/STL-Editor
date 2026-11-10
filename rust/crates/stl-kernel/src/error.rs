use std::fmt;

/// Why a mesh could not be turned into a [`crate::Solid`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KernelError {
    /// The mesh is well formed but is not a closed 2-manifold (it has holes, or edges shared
    /// by more than two triangles).
    NotClosed,
    /// The arrays themselves are wrong; the text says how.
    Invalid(String),
}

impl fmt::Display for KernelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KernelError::NotClosed => f.write_str("the mesh is not a closed solid"),
            KernelError::Invalid(why) => write!(f, "the mesh is not valid: {why}"),
        }
    }
}

impl std::error::Error for KernelError {}
