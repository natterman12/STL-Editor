//! Tool framework: registry, panel, fields, preview, gizmo, Sculpt (1.x `tools.js` / `app.js`).

pub mod defs;
pub mod fields;
pub mod gizmo;
pub mod panel;
pub mod preview;
pub mod sculpt;

pub use defs::*;

/// The plane prompt of 1.x's `beginSketch`: a tool panel with one plane field, "Create Sketch",
/// whose Start button enters the sketch (no step is added). Not in the tool list.
pub fn sketch_prompt() -> &'static ToolDef {
    use std::sync::OnceLock;
    static DEF: OnceLock<ToolDef> = OnceLock::new();
    DEF.get_or_init(|| ToolDef {
        key: "sketch",
        label: "Create Sketch",
        icon: "sketch",
        kind: "sketch",
        fields: vec![Field { key: "plane", label: "Plane", kind: FieldKind::Plane { any_face: false }, help: None, show: None, placeholder: None }],
        hint: Some("Select a plane or flat face to sketch on"),
        extra: None,
        flags: ToolFlags { no_live_preview: true, ..Default::default() },
        defaults: |_| {
            let mut v = Values::new();
            v.insert("plane".into(), serde_json::Value::Null);
            v
        },
        to_params: |_, _| serde_json::Value::Null,
        from_params: |_| Values::new(),
        on_change: None,
    })
}
