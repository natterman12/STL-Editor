//! `SketchError`: a plain-English message the user can act on, shown as-is (1.x `GeometryError`).

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SketchError(pub String);

impl SketchError {
    pub fn new(message: impl Into<String>) -> Self {
        SketchError(message.into())
    }
    pub fn message(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SketchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for SketchError {}
