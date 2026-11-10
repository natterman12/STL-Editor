use std::fmt;

/// Something about a feature the user can fix (1.x: `GeometryError`). The text is shown as it
/// is, so it keeps the 1.x wording.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeatureError(pub String);

impl fmt::Display for FeatureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for FeatureError {}
