use std::fmt;

/// Something the user can fix; the text is shown as it is (1.x: `GeometryError`).
#[derive(Debug, Clone, PartialEq)]
pub enum MeshError {
    Message(String),
}

impl fmt::Display for MeshError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MeshError::Message(m) => f.write_str(m),
        }
    }
}

impl std::error::Error for MeshError {}
