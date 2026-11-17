//! The origin planes while a tool field takes a plane (spec 2.10): shown and pickable while that
//! field is active, then back to how they were.

use super::StlApp;
use crate::tools::fields::plane::wants_planes;

impl StlApp {
    /// Called whenever a tool opens or closes, its active field changes, or a project opens.
    /// While the open tool's active field takes a plane (or the tool places on a face), the origin
    /// planes are shown; `planes_before_tool` remembers the eye state from before, so the browser's
    /// Origin toggle is put back exactly as it was when the field is filled or the tool closes.
    pub fn sync_tool_planes(&mut self) {
        let want = self
            .tool()
            .is_some_and(|t| wants_planes(t.def, t.active_field.as_deref()));
        match (want, self.planes_before_tool) {
            (true, None) => {
                self.planes_before_tool = Some(self.viewport.base_planes_visible());
                self.viewport.set_base_planes_visible(true);
            }
            (true, Some(_)) => self.viewport.set_base_planes_visible(true),
            (false, Some(before)) => {
                self.planes_before_tool = None;
                self.viewport.set_base_planes_visible(before);
            }
            (false, None) => {}
        }
    }
}
