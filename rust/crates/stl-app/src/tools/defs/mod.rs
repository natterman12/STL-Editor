//! Port of `TOOLS` in `web/js/tools.js`. Interfaces fixed by the M4b plan, Task 1.

use std::sync::OnceLock;

use crate::engine_link::{BodyView, PlaneView, SketchView, TimelineItem};

mod fields;
mod mesh;
mod model;
mod modify;
mod planes;
mod print;
pub mod tables;
mod util;

pub type Values = serde_json::Map<String, serde_json::Value>;

pub struct Ctx<'a> {
    pub bodies: &'a [BodyView],
    pub planes: &'a [PlaneView],
    pub sketches: &'a [SketchView],
    pub features: &'a [TimelineItem],
    pub selection: &'a [String],
    pub last_sketch: Option<&'a str>,
    pub sketch_on_body: bool,
}

pub enum FieldKind {
    Number { unit: Option<&'static str> },
    Select {
        /// `(id, label, group)`; `group` is set when `grouped`.
        options: Vec<(String, String, Option<String>)>,
        grouped: bool,
    },
    Bool,
    Vec3 { unit: Option<&'static str> },
    Text { placeholder: Option<String> },
    Bodies { multi: bool },
    Plane { any_face: bool },
    Sketch,
    Regions,
    Features,
    FacePoints,
    PFace { any_face: bool },
    CylFace,
    Points3,
    AxisLine,
}

pub struct Field {
    pub key: &'static str,
    pub label: &'static str,
    pub kind: FieldKind,
    pub help: Option<&'static str>,
    pub show: Option<fn(&Values) -> bool>,
    /// 1.x `placeholder` on bodies fields (e.g. targets → "Auto").
    pub placeholder: Option<&'static str>,
}

pub fn select_flat(options: Vec<(String, String)>) -> FieldKind {
    FieldKind::Select {
        options: options
            .into_iter()
            .map(|(a, b)| (a, b, None))
            .collect(),
        grouped: false,
    }
}

pub fn select_grouped(options: Vec<(String, String, String)>) -> FieldKind {
    FieldKind::Select {
        options: options
            .into_iter()
            .map(|(a, b, g)| (a, b, Some(g)))
            .collect(),
        grouped: true,
    }
}

#[derive(Default)]
pub struct ToolFlags {
    pub place_on_face: bool,
    pub no_live_preview: bool,
    pub gizmo: bool,
    pub plane_tool: bool,
    pub sculpt: bool,
    pub paint: bool,
    pub wall_tool: bool,
}

pub struct ToolDef {
    pub key: &'static str,
    pub label: &'static str,
    pub icon: &'static str,
    /// The engine feature type (`type` in tools.js).
    pub kind: &'static str,
    pub fields: Vec<Field>,
    pub hint: Option<&'static str>,
    pub extra: Option<&'static str>,
    pub flags: ToolFlags,
    pub defaults: fn(&Ctx) -> Values,
    pub to_params: fn(&Values, &Ctx) -> serde_json::Value,
    pub from_params: fn(serde_json::Value) -> Values,
    /// Returns a patch.
    pub on_change: Option<fn(&str, &Values, &Ctx) -> Option<Values>>,
}

static TOOL_LIST: OnceLock<Vec<ToolDef>> = OnceLock::new();

fn build_tools() -> Vec<ToolDef> {
    // the order of `TOOLS` in tools.js (a test checks it)
    vec![
        model::extrude(),
        model::revolve(),
        model::loft(),
        model::sweep(),
        model::primitive_box(),
        model::primitive_cylinder(),
        model::primitive_sphere(),
        model::primitive_cone(),
        model::primitive_torus(),
        model::snapfit(),
        model::hinge(),
        modify::hole(),
        modify::combine(),
        modify::split(),
        print::bedsplit(),
        print::plate(),
        print::layflat(),
        modify::thread(),
        modify::offset_tool(),
        modify::thincheck(),
        modify::fittest(),
        modify::resizehole(),
        modify::pushpull(),
        modify::mirror(),
        modify::move_tool(),
        modify::scale_tool(),
        modify::pattern(),
        modify::hollow(),
        modify::lattice(),
        modify::shell(),
        planes::plane_offset(),
        planes::plane_angle(),
        planes::plane_3pt(),
        planes::plane_mid(),
        planes::import_tool(),
        mesh::sculpt(),
        mesh::remesh(),
        mesh::repair(),
        mesh::simplify(),
        mesh::separate(),
        mesh::delete_tool(),
    ]
}

/// Every tool in `TOOLS` order.
pub fn tools() -> &'static [ToolDef] {
    TOOL_LIST.get_or_init(build_tools).as_slice()
}

pub fn tool(key: &str) -> Option<&'static ToolDef> {
    tools().iter().find(|t| t.key == key)
}
