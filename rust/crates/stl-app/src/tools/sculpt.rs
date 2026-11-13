//! The Sculpt brush (1.x `app.js`: the viewport's sculpt pointer handlers, `viewer.js`
//! `setBrush` and `drawStrokes`). Mouse mode Sculpt: the left button brushes, the middle
//! button pans, the right button orbits (`MouseMode::Sculpt`, set by the app on opening the tool).
//!
//! How the app drives it while the Sculpt tool is open: after `viewport.show(ui)`, call
//! [`Sculpt::update`] with the response and the tool's values. It shows the brush ring, grows
//! the stroke being drawn, and on release returns the values to set (`body` and `strokes`);
//! the app sets them and previews at once (`schedulePreview(0)`). A [`SculptEvent::Warn`]
//! goes to the tool's message line as a warning. On closing the tool call [`Sculpt::close`].

use egui::Pos2;
use serde_json::{json, Map, Value};

use crate::viewport::overlays::Stroke;
use crate::viewport::pick::Hit;
use crate::viewport::{MouseMode, Viewport};

use super::gizmo::jnum;

pub type Values = Map<String, Value>;

/// The warning when a stroke starts on another body (word for word).
pub const OTHER_BODY: &str = "Finish (OK) the strokes on the other body first.";

/// JS `parseFloat`: the longest number at the start of the text.
pub fn parse_float(v: Option<&Value>) -> f64 {
    match v {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(f64::NAN),
        Some(Value::String(s)) => {
            let t = s.trim_start();
            let mut best = f64::NAN;
            for end in (1..=t.len()).rev() {
                if !t.is_char_boundary(end) {
                    continue;
                }
                let head = &t[..end];
                if head.ends_with(|c: char| c == 'e' || c == 'E' || c == '+' || c == '-') {
                    continue;
                }
                if let Ok(x) = head.parse::<f64>() {
                    if !head.contains(|c: char| c.is_alphabetic() && c != 'e' && c != 'E') {
                        best = x;
                        break;
                    }
                }
            }
            best
        }
        _ => f64::NAN,
    }
}

fn or(x: f64, d: f64) -> f64 {
    if x.is_nan() || x == 0.0 { d } else { x }
}

/// The brush size: `Math.max(0.05, parseFloat(radius) || 5)`.
pub fn brush_radius(values: &Values) -> f64 {
    or(parse_float(values.get("radius")), 5.0).max(0.05)
}

/// `Math.min(1, Math.max(0.01, parseFloat(strength) || 0.5))`.
pub fn brush_strength(values: &Values) -> f64 {
    or(parse_float(values.get("strength")), 0.5).max(0.01).min(1.0)
}

/// `+x.toFixed(3)`. `toFixed` rounds the exact value, a tie away from zero; Rust's formatting
/// rounds a tie to even. A tie at three decimals is a value of an odd number of sixteenths.
pub fn fixed3(x: f64) -> f64 {
    let s16 = x * 16.0;
    let r = if s16 == s16.trunc() && (s16 as i64) % 2 != 0 {
        x.signum() * ((x.abs() * 1000.0 + 0.5).floor() / 1000.0)
    } else {
        format!("{x:.3}").parse().unwrap_or(x)
    };
    if r == 0.0 { 0.0 } else { r }
}

#[derive(Clone, Debug, PartialEq)]
struct Live {
    body: String,
    mode: String,
    radius: f64,
    strength: f64,
    points: Vec<[f64; 3]>,
}

/// What a frame of sculpt input did.
#[derive(Clone, Debug, PartialEq)]
pub enum SculptEvent {
    None,
    /// A stroke started or grew (the strokes overlay is already drawn).
    Drawing,
    /// Show this in the tool's message line as a warning.
    Warn(&'static str),
    /// A stroke is finished: set these values (`body`, `strokes`) and preview at once.
    Stroke(Values),
}

#[derive(Default)]
pub struct Sculpt {
    stroke: Option<Live>,
}

fn strokes_of(values: &Values) -> Vec<Stroke> {
    let Some(Value::Array(a)) = values.get("strokes") else { return Vec::new() };
    a.iter()
        .map(|s| Stroke {
            mode: s.get("mode").and_then(Value::as_str).unwrap_or("").to_owned(),
            points: s
                .get("points")
                .and_then(Value::as_array)
                .map(|p| p.iter().filter_map(|q| q.as_array().map(|c| [0, 1, 2].map(|i| c.get(i).and_then(Value::as_f64).unwrap_or(0.0)))).collect())
                .unwrap_or_default(),
        })
        .collect()
}

impl Sculpt {
    pub fn new() -> Self {
        Self::default()
    }

    /// A stroke is being drawn.
    pub fn drawing(&self) -> bool {
        self.stroke.is_some()
    }

    /// Opening the tool: mouse mode Sculpt and the tool's strokes drawn.
    pub fn open(vp: &mut Viewport, values: &Values) {
        vp.set_mouse_mode(MouseMode::Sculpt);
        Self::draw(vp, values);
    }

    /// `drawStrokes(values.strokes)` (renderOverlayForTool).
    pub fn draw(vp: &mut Viewport, values: &Values) {
        vp.draw_strokes(&strokes_of(values));
    }

    /// `closeTool`: model mouse mode, no brush ring, no strokes.
    pub fn close(&mut self, vp: &mut Viewport) {
        self.stroke = None;
        vp.set_mouse_mode(MouseMode::Model);
        vp.set_brush(None);
        vp.clear_overlay("strokes");
    }

    fn body_hit(vp: &Viewport, pos: Pos2) -> Option<(String, [f64; 3], [f64; 3])> {
        match vp.pick(pos, true, false) {
            Some(Hit::Body { id, point, normal, .. }) => Some((id, point, normal)),
            _ => None,
        }
    }

    /// `pointerdown` with the left button.
    pub fn press(&mut self, vp: &mut Viewport, pos: Pos2, values: &Values) -> SculptEvent {
        let Some((id, point, _)) = Self::body_hit(vp, pos) else { return SculptEvent::None };
        let has_strokes = matches!(values.get("strokes"), Some(Value::Array(a)) if !a.is_empty());
        let body = values.get("body").and_then(Value::as_str).filter(|s| !s.is_empty());
        if body.is_some_and(|b| b != id) && has_strokes {
            return SculptEvent::Warn(OTHER_BODY);
        }
        self.stroke = Some(Live {
            body: id,
            mode: values.get("mode").and_then(Value::as_str).unwrap_or("").to_owned(),
            radius: brush_radius(values),
            strength: brush_strength(values),
            points: vec![point],
        });
        SculptEvent::Drawing
    }

    /// `pointermove`: the brush ring on the surface under the pointer (hidden off the part),
    /// and the stroke grows by a point once the pointer is more than a fifth of the brush away.
    pub fn hover(&mut self, vp: &mut Viewport, pos: Option<Pos2>, values: &Values) -> SculptEvent {
        let hit = pos.and_then(|p| Self::body_hit(vp, p));
        let r = brush_radius(values);
        vp.set_brush(hit.as_ref().map(|(_, p, n)| (*p, *n, r)));
        let (Some(st), Some((id, point, _))) = (self.stroke.as_mut(), hit) else { return SculptEvent::None };
        if id != st.body {
            return SculptEvent::None;
        }
        let last = *st.points.last().unwrap_or(&point);
        let d = ((point[0] - last[0]).powi(2) + (point[1] - last[1]).powi(2) + (point[2] - last[2]).powi(2)).sqrt();
        if d > r / 5.0 {
            st.points.push(point);
            let mut all = strokes_of(values);
            all.push(Stroke { mode: st.mode.clone(), points: st.points.clone() });
            vp.draw_strokes(&all);
            return SculptEvent::Drawing;
        }
        SculptEvent::None
    }

    /// `pointerup`: the finished stroke, its points to 0.001 mm, without its body (the body
    /// goes to the tool's `body`).
    pub fn release(&mut self, values: &Values) -> Option<Values> {
        let st = self.stroke.take()?;
        let mut stroke = Map::new();
        stroke.insert("mode".into(), json!(st.mode));
        stroke.insert("radius".into(), jnum(st.radius));
        stroke.insert("strength".into(), jnum(st.strength));
        stroke.insert(
            "points".into(),
            Value::Array(st.points.iter().map(|p| Value::Array(p.iter().map(|&x| jnum(fixed3(x))).collect())).collect()),
        );
        let mut strokes = match values.get("strokes") {
            Some(Value::Array(a)) => a.clone(),
            _ => Vec::new(),
        };
        strokes.push(Value::Object(stroke));
        let mut patch = Values::new();
        patch.insert("body".into(), json!(st.body));
        patch.insert("strokes".into(), Value::Array(strokes));
        Some(patch)
    }

    /// Everything for one frame. Call after `viewport.show(ui)`.
    pub fn update(&mut self, vp: &mut Viewport, ui: &egui::Ui, response: &egui::Response, values: &Values) -> SculptEvent {
        let (hover, pressed, released) = ui.input(|i| (i.pointer.hover_pos(), i.pointer.primary_pressed(), i.pointer.primary_released()));
        let inside = response.contains_pointer();
        if released && self.stroke.is_some() {
            return match self.release(values) {
                Some(v) => SculptEvent::Stroke(v),
                None => SculptEvent::None,
            };
        }
        if pressed && inside {
            if let Some(p) = hover {
                let e = self.press(vp, p, values);
                if e != SculptEvent::None {
                    // pressed and let go in the same frame: the stroke is finished now, not left open
                    if released && self.stroke.is_some() {
                        if let Some(v) = self.release(values) {
                            return SculptEvent::Stroke(v);
                        }
                    }
                    return e;
                }
            }
        }
        if response.hovered() || self.stroke.is_some() || inside {
            return self.hover(vp, hover.filter(|_| inside || self.stroke.is_some()), values);
        }
        vp.set_brush(None);
        SculptEvent::None
    }
}
