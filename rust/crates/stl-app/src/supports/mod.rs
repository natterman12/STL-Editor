//! Support Fins user interface (1.x `supportTools`, coverage, Draw Walls).
//!
//! The panel and the strength readouts are built here. Coverage and Draw Walls live in
//! their own modules (Task 6). The ribbon and the engine link stay with their owners;
//! this module only exposes the calls those pieces make.

pub mod coverage;
pub mod panel;
pub mod strength;
pub mod walls;

pub use panel::{
    about, body_id, credit_line, defaults, engine_features, engine_host, engine_version, field_meta,
    field_visible, fields, from_params, gizmo_mode, install, is_installed, mode_hint, mode_of,
    notice, on_form_change, orient_available, preview_ask, quick_paint, reopen_plan, reopen_supports,
    section_badge, section_default_open, set_mode, show, shown_keys, status_of, strength_ask,
    suggest_ask, support_tool, support_tools, to_params, warning_line, About, FieldSpec, FormEffects,
    OverhangOverlay, PanelEvent, PreviewAsk, QuickPaint, Reopen, SectionMemory, Session, SpecKind,
    BUSY_FINS, BUSY_WALLS, CREDIT_TITLE, FINS_HINT, LOADING, MODE_HELP, MODES, NO_OVERHANG_MSG,
    OVERHANG_RED, REMOVE_HINT, ROTATE_HINT,
};
pub use strength::{
    confidence_note, format_candidate, js_num, load_options, load_selected, load_vector, match_load,
    material_density, material_key, material_name, pose_view, posture_text, status_cards, strength_request,
    suggest_request, turn_load, CandRow, Card, Cover, Plastic, PoseView, Spot, Status, StrengthRequest,
    SuggestRequest, SuggestUi, Tone, LOAD_HELP, MATERIALS, PICK_PART, POSTURE_TIP, SUGGEST_BUSY,
};
