//! File export: STL, 3MF, OBJ and faceted STEP. Byte-for-byte what 1.x writes (see
//! `dev/plans/2026-11-11-stl-editor-2-m3-files.md`).

use stl_kernel::MeshArrays;

pub mod jsfmt;
mod obj;
pub mod pyfmt;
mod step;
mod stl;
mod threemf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportFormat {
    Stl,
    ThreeMf,
    Obj,
    Step,
}

impl ExportFormat {
    /// `ext` is "stl", "3mf", "obj" or "step" in any case, without the dot.
    pub fn from_ext(ext: &str) -> Option<ExportFormat> {
        match ext.to_ascii_lowercase().as_str() {
            "stl" => Some(ExportFormat::Stl),
            "3mf" => Some(ExportFormat::ThreeMf),
            "obj" => Some(ExportFormat::Obj),
            "step" => Some(ExportFormat::Step),
            _ => None,
        }
    }

    pub fn ext(self) -> &'static str {
        match self {
            ExportFormat::Stl => "stl",
            ExportFormat::ThreeMf => "3mf",
            ExportFormat::Obj => "obj",
            ExportFormat::Step => "step",
        }
    }
}

pub struct ExportBody<'a> {
    pub name: String,
    pub mesh: &'a MeshArrays,
}

pub struct ExportOptions {
    pub uuid_seed: u64,
    pub step_title: String,
    pub step_date: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExportError {
    /// A plain-English message, shown as is.
    Geometry(String),
    Unsupported,
    Io(String),
}

impl std::fmt::Display for ExportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExportError::Geometry(m) | ExportError::Io(m) => f.write_str(m),
            ExportError::Unsupported => f.write_str("This file type can't be exported."),
        }
    }
}

impl std::error::Error for ExportError {}

pub fn export_bodies(
    bodies: &[ExportBody],
    format: ExportFormat,
    opts: &ExportOptions,
) -> Result<Vec<u8>, ExportError> {
    match format {
        ExportFormat::Stl => stl::write(bodies),
        ExportFormat::ThreeMf => threemf::write(bodies, opts),
        ExportFormat::Obj => obj::write(bodies, opts),
        ExportFormat::Step => step::write(bodies, opts),
    }
}
