//! Document: feature timeline, rollback, parameters and expressions, evaluation and per-feature cache, undo/redo, .stlx format.

pub mod document;
pub mod error;
pub mod feature;
pub mod features;
pub mod fins_host;
pub mod labels;
pub mod params;
pub mod registry;
pub mod sketch_op;
pub mod state;
pub mod stlx;

pub use document::{iso_from_unix_secs, iso_utc_now, Document};
pub use error::FeatureError;
pub use stl_io::{ExportFormat, ExportOptions};
pub use features::common::{
    affine, apply_tool, base_planes, float, mirror_matrix, pivot, place, plane_frame, repeat_features, rotation,
    transform_body, Frame, PlaneDef, ToolRecord, Vec3,
};
pub use feature::{Ctx, Feature, FeatureResult, RollbackAfter, Source};
pub use fins_host::{FinsHost, HostKind, NullHost};
pub use labels::LABELS;
pub use registry::{Evaluator, Registry};
pub use state::{Body, State};
