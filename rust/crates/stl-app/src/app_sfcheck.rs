//! The support-section check and tilt hint after a Support Fins build: asked of the engine once
//! the preview is in (`FinsAsk::Check`), shown by `supports/check.rs`, and the flagged spot
//! outlined in the view. A child of `app.rs`, beside `app_wiring.rs` (which calls in from
//! `sf_on_preview`, `on_fins` and `sf_panel_events`).

use super::*;
use crate::engine_link::FinsAsk;
use crate::supports::check::{self, CheckUi};
use crate::supports::{self, PanelEvent};
use egui::Color32;
use serde_json::Value;

/// The outline's overlay tag.
pub(super) const FINDING_TAG: &str = "sf-finding";
/// `0xffa828`, the website's outline.
const FINDING_COLOR: Color32 = Color32::from_rgb(0xff, 0xa8, 0x28);

impl StlApp {
    /// A Support Fins preview is in: check its supports (and look for a better tilt) in the
    /// background. Any older answer is dropped by its `seq`.
    pub(super) fn sf_check(&mut self) {
        self.viewport.clear_overlay(FINDING_TAG);
        let Some(t) = self.tool.as_ref() else { return };
        if t.def.key != "supportfins" {
            return;
        }
        let Ok(ask) = supports::suggest_ask(t.def.key, &t.values) else {
            return;
        };
        let nonempty = |k: &str| t.values.get(k).and_then(Value::as_array).is_some_and(|a| !a.is_empty());
        let edited = nonempty("removed") || nonempty("walls");
        let seq = self.fins_ask(FinsAsk::Check {
            body: ask.body,
            options: ask.options,
        });
        self.sf.check_seq = seq;
        if let Some(s) = self.tool.as_mut().and_then(|t| t.sf.as_mut()) {
            s.check = CheckUi::Busy;
            s.finding = None;
            s.check_edited = edited;
        }
    }

    /// The preview failed or the tool is closing: nothing to show.
    pub(super) fn sf_check_clear(&mut self) {
        self.sf.check_seq = 0;
        self.viewport.clear_overlay(FINDING_TAG);
        if let Some(s) = self.tool.as_mut().and_then(|t| t.sf.as_mut()) {
            s.check = CheckUi::Idle;
            s.finding = None;
        }
    }

    pub(super) fn sf_on_check(&mut self, seq: u64, result: Result<Value, String>) {
        if seq != self.sf.check_seq {
            return;
        }
        // a new answer: any spot outlined from the one before is gone
        self.viewport.clear_overlay(FINDING_TAG);
        if let Some(s) = self.tool.as_mut().and_then(|t| t.sf.as_mut()) {
            s.finding = None;
            // a failed check says nothing: the build itself went through, and its own line stands
            s.check = match result {
                Ok(v) => CheckUi::Ready(check::parse(&v)),
                Err(_) => CheckUi::Idle,
            };
        }
    }

    pub(super) fn sf_check_event(&mut self, e: &PanelEvent) {
        match e {
            PanelEvent::UseTilt => {
                let rot = self
                    .tool
                    .as_ref()
                    .and_then(|t| t.sf.as_ref())
                    .and_then(|s| match &s.check {
                        CheckUi::Ready(r) => r.tilt.as_ref().map(|t| t.rot.clone()),
                        _ => None,
                    });
                if let Some(r) = rot {
                    self.use_pose(&r);
                }
            }
            PanelEvent::Finding(i) => {
                self.viewport.clear_overlay(FINDING_TAG);
                let Some(s) = self.tool.as_mut().and_then(|t| t.sf.as_mut()) else {
                    return;
                };
                let found = match (&s.check, i) {
                    (CheckUi::Ready(r), Some(i)) => r.findings.get(*i).map(|f| f.bx),
                    _ => None,
                };
                s.finding = found.and(*i);
                if let Some(b) = found {
                    for (a, c) in box_edges(b) {
                        self.viewport.add_line(a, c, FINDING_COLOR, FINDING_TAG);
                    }
                }
            }
            _ => {}
        }
    }
}

/// The twelve edges of `[x0, y0, z0, x1, y1, z1]`.
fn box_edges(b: [f64; 6]) -> Vec<([f64; 3], [f64; 3])> {
    let p = |i: usize| {
        [
            if i & 1 == 0 { b[0] } else { b[3] },
            if i & 2 == 0 { b[1] } else { b[4] },
            if i & 4 == 0 { b[2] } else { b[5] },
        ]
    };
    let mut out = Vec::new();
    for i in 0..8usize {
        for bit in [1usize, 2, 4] {
            if i & bit == 0 {
                out.push((p(i), p(i | bit)));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::box_edges;

    #[test]
    fn a_box_has_twelve_edges_each_along_one_axis() {
        let e = box_edges([0.0, 0.0, 0.0, 1.0, 2.0, 3.0]);
        assert_eq!(e.len(), 12);
        for (a, b) in e {
            let moved = (0..3).filter(|&k| a[k] != b[k]).count();
            assert_eq!(moved, 1);
        }
    }
}
