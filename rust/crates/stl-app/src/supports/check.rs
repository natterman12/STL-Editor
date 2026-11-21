//! What the Support Fins panel says under its result once the fins are built: the support-section
//! check and the tilt hint (`stl_engine::features::common::fins::check`, asked of the engine as
//! `FinsAsk::Check` after each preview). Kept out of the build itself, so building stays as fast
//! as before; the panel shows "Checking the supports…" until the answer is in.
//!
//! The words are this app's, for people who print things: the website's own wording
//! ("support-section findings need slicer inspection") is for slicer users. The engine's info line
//! (`reportLine`) is left as it is.

use serde_json::Value;

use super::panel::{PanelEvent, Session};
use crate::theme::Tokens;

/// The busy line while the engine checks.
pub const CHECK_BUSY: &str = "Checking the supports…";

/// One spot the check flagged, as a box in the part's frame.
#[derive(Clone, Debug, PartialEq)]
pub struct Finding {
    /// `narrow` (thinner than one line of the nozzle) or `start` (begins in mid-air).
    pub kind: String,
    /// Height above the bed, mm.
    pub z: f64,
    /// `[x0, y0, z0, x1, y1, z1]`.
    pub bx: [f64; 6],
}

/// A pose that leaves fewer overhangs too shallow for a fin.
#[derive(Clone, Debug, PartialEq)]
pub struct Tilt {
    /// "laying it on its right side (+X)", "tilting it 31° toward the back (+Y)", ...
    pub phrase: String,
    /// How many are still too shallow that way.
    pub left: u64,
    /// Column-major 3x3, as Suggest's candidates (`App::use_pose`).
    pub rot: Value,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CheckReady {
    /// The engine's status: `complete`, `incomplete`, `unavailable`, or empty for an engine
    /// without the check.
    pub status: String,
    pub findings: Vec<Finding>,
    pub unserved: u64,
    pub tilt: Option<Tilt>,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub enum CheckUi {
    #[default]
    Idle,
    Busy,
    Ready(CheckReady),
}

/// The engine's answer to `check`.
pub fn parse(v: &Value) -> CheckReady {
    let num = |x: Option<&Value>| x.and_then(Value::as_f64).unwrap_or(0.0);
    let findings = v
        .get("findings")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|f| {
                    let b: Vec<f64> = f.get("box")?.as_array()?.iter().filter_map(Value::as_f64).collect();
                    let bx: [f64; 6] = b.try_into().ok()?;
                    Some(Finding {
                        kind: f.get("kind").and_then(Value::as_str).unwrap_or("narrow").to_owned(),
                        z: num(f.get("z")),
                        bx,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let unserved = v.get("unserved").and_then(Value::as_u64).unwrap_or(0);
    let tilt = v.get("tilt").filter(|t| t.is_object()).and_then(|t| {
        let phrase = t.get("phrase").and_then(Value::as_str)?.to_owned();
        let rot = t
            .get("rot")
            .filter(|r| r.as_array().is_some_and(|a| a.len() == 9))?
            .clone();
        let left = t.get("unserved").and_then(Value::as_u64).unwrap_or(0);
        (left < unserved).then_some(Tilt { phrase, left, rot })
    });
    CheckReady {
        status: v.get("status").and_then(Value::as_str).unwrap_or("").to_owned(),
        findings,
        unserved,
        tilt,
    }
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

fn capital(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
        .unwrap_or_default()
}

/// What the panel says about the support check, or `None` when there is nothing to say.
/// `edited`: the user removed fins or drew walls by hand, which the check doesn't see.
pub fn findings_line(r: &CheckReady, edited: bool) -> Option<String> {
    let tail = if edited {
        " Fins you removed or drew by hand aren't checked."
    } else {
        ""
    };
    let n = r.findings.len();
    let text = match r.status.as_str() {
        "incomplete" => {
            "Not every support could be checked. Look them over in your slicer's preview before printing.".to_owned()
        }
        "complete" if n > 0 => format!(
            "{} may not print well: thinner than one line of your nozzle, or starting in mid-air. Look at {} in your slicer's preview before printing.",
            plural(n, "support spot", "support spots"),
            if n == 1 { "it" } else { "them" }
        ),
        _ => return None,
    };
    Some(text + tail)
}

/// The tilt hint's sentence.
pub fn tilt_line(r: &CheckReady) -> Option<String> {
    let t = r.tilt.as_ref()?;
    let left = if t.left == 0 {
        "none of them".to_owned()
    } else {
        format!("{} instead of {}", t.left, r.unserved)
    };
    Some(format!(
        "{} overhang{} too shallow for a fin this way up. {} leaves {left}.",
        r.unserved,
        if r.unserved == 1 { " is" } else { "s are" },
        capital(&t.phrase)
    ))
}

/// Where the selected spot is, for the line under the buttons.
pub fn finding_where(r: &CheckReady, i: usize) -> Option<String> {
    let f = r.findings.get(i)?;
    let what = if f.kind == "start" {
        "starts in mid-air"
    } else {
        "too thin"
    };
    Some(format!(
        "Spot {} of {}: {what}, {:.1} mm up.",
        i + 1,
        r.findings.len(),
        f.z
    ))
}

/// The lines and buttons under the result cards.
pub fn draw(ui: &mut egui::Ui, tk: &Tokens, session: &mut Session, out: &mut Vec<PanelEvent>) {
    use crate::tools::panel::ctl_button;
    let note = |ui: &mut egui::Ui, text: &str, col: egui::Color32| {
        ui.add_space(6.0);
        ui.add(egui::Label::new(egui::RichText::new(text).size(12.0).color(col)).wrap());
    };
    match &session.check {
        CheckUi::Idle => {}
        CheckUi::Busy => {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new().size(11.0).color(tk.accent));
                ui.add_space(8.0);
                ui.add(egui::Label::new(egui::RichText::new(CHECK_BUSY).size(12.0).color(tk.muted)).wrap());
            });
        }
        CheckUi::Ready(r) => {
            let r = r.clone();
            if let Some(t) = tilt_line(&r) {
                note(ui, &t, tk.muted);
                ui.add_space(4.0);
                if ctl_button(ui, tk, "Turn it that way", true).clicked() {
                    out.push(PanelEvent::UseTilt);
                }
            }
            if let Some(t) = findings_line(&r, session.check_edited) {
                note(ui, &t, tk.warn_text);
                if !r.findings.is_empty() {
                    ui.add_space(4.0);
                    let sel = session.finding;
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        let label = if sel.is_some() { "Hide" } else { "Show" };
                        if ctl_button(ui, tk, label, true).clicked() {
                            out.push(PanelEvent::Finding(if sel.is_some() { None } else { Some(0) }));
                        }
                        if let Some(i) = sel
                            && r.findings.len() > 1
                        {
                            let n = r.findings.len();
                            if ctl_button(ui, tk, "Previous", true).clicked() {
                                out.push(PanelEvent::Finding(Some((i + n - 1) % n)));
                            }
                            if ctl_button(ui, tk, "Next", true).clicked() {
                                out.push(PanelEvent::Finding(Some((i + 1) % n)));
                            }
                        }
                    });
                    if let Some(w) = sel.and_then(|i| finding_where(&r, i)) {
                        note(ui, &w, tk.muted);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_tilt_that_helps_is_named_with_how_many_it_leaves() {
        let r = parse(&json!({
            "status": "complete", "findings": [], "unserved": 3, "floating": 0,
            "tilt": {"phrase": "laying it on its right side (+X)", "unserved": 0, "rot": [0,0,-1,0,1,0,1,0,0]}
        }));
        assert_eq!(
            tilt_line(&r).unwrap(),
            "3 overhangs are too shallow for a fin this way up. Laying it on its right side (+X) leaves none of them."
        );
        assert_eq!(findings_line(&r, false), None);
        let r = parse(&json!({"status": "complete", "unserved": 1,
            "tilt": {"phrase": "tilting it 31° toward the back (+Y)", "unserved": 1, "rot": [1,0,0,0,1,0,0,0,1]}}));
        assert_eq!(r.tilt, None, "a pose that leaves as many isn't offered");
        let r = parse(&json!({"status": "complete", "unserved": 4,
            "tilt": {"phrase": "turning it upside down", "unserved": 1, "rot": [1,0,0,0,-1,0,0,0,-1]}}));
        assert!(
            tilt_line(&r)
                .unwrap()
                .ends_with("Turning it upside down leaves 1 instead of 4.")
        );
    }

    #[test]
    fn findings_are_counted_and_located() {
        let r = parse(&json!({
            "status": "complete", "unserved": 0, "tilt": null,
            "findings": [
                {"kind": "narrow", "piece": "0", "z": 0.4, "box": [-4, -0.25, 0.2, 4, 0.25, 0.4]},
                {"kind": "start", "piece": "2", "z": 12.25, "box": [0, 0, 12.05, 1, 1, 12.25]}
            ]
        }));
        assert_eq!(r.findings.len(), 2);
        assert!(
            findings_line(&r, false)
                .unwrap()
                .starts_with("2 support spots may not print well")
        );
        assert!(
            findings_line(&r, true)
                .unwrap()
                .ends_with("Fins you removed or drew by hand aren't checked.")
        );
        assert_eq!(finding_where(&r, 0).unwrap(), "Spot 1 of 2: too thin, 0.4 mm up.");
        assert_eq!(
            finding_where(&r, 1).unwrap(),
            "Spot 2 of 2: starts in mid-air, 12.2 mm up."
        );
        assert_eq!(finding_where(&r, 2), None);
    }

    #[test]
    fn nothing_to_say() {
        let r = parse(&json!({"status": "complete", "findings": [], "unserved": 0, "tilt": null}));
        assert_eq!((findings_line(&r, false), tilt_line(&r)), (None, None));
        // an engine without the check
        let r = parse(&json!({"status": null, "unserved": 0}));
        assert_eq!(findings_line(&r, false), None);
        let r = parse(&json!({"status": "incomplete", "findings": []}));
        assert!(
            findings_line(&r, false)
                .unwrap()
                .starts_with("Not every support could be checked")
        );
    }
}
