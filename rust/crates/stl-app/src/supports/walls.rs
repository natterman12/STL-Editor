//! Draw Walls — port of `web/js/drawwalls.js`.

use serde_json::{json, Value};

/// Session storage key for the remembered shape (1.x `SHAPE_KEY`).
pub const SHAPE_STORAGE_KEY: &str = "stle_draw_shape";

pub const OK_COL: u32 = 0x2f9e5a;
pub const BAD_COL: u32 = 0xd64545;
pub const BAND_COL: u32 = 0x59d98e;
pub const PENDING_COL: u32 = 0x1f9e55;

pub const ALL_SHAPES: &[(&str, &str)] = &[
    ("line", "Line"),
    ("path", "Multi-line"),
    ("curve", "Curve"),
    ("gusset", "Gusset"),
    ("anchor", "Anchor"),
    ("drop", "Drop fin"),
];

pub fn hint_for(shape: &str) -> &'static str {
    match shape {
        "drop" => "Click a red (bare) spot under an overhang: a wall is dropped across it.",
        "line" => {
            "Click two points under an overhang for each wall. Keep clicking to add more walls."
        }
        "path" => {
            "Click each corner of the wall; double-click (or press Enter) on the last point to finish it."
        }
        "curve" => "Click the two ends of the wall, then a point on the curve between them.",
        "gusset" => "Gusset: a fin off a side of the part instead of the plate. Click the side (low enough for the fin to climb out at 50°), then the overhang it should reach.",
        "anchor" => {
            "Anchor fin: a rib that holds a tall side or a back that leans up. Click the side where it should stand."
        }
        _ => "",
    }
}

/// One wall in the tool's `walls` field (1.x JSON shape).
#[derive(Clone, Debug, PartialEq)]
pub struct Wall {
    pub shape: String,
    pub points: Vec<[f64; 3]>,
    pub normal: Option<[f64; 3]>,
}

impl Wall {
    pub fn to_value(&self) -> Value {
        let mut o = json!({ "shape": self.shape, "points": self.points });
        if let Some(n) = self.normal {
            o["normal"] = json!(n);
        }
        o
    }
}

pub fn walls_to_params(walls: &[Wall]) -> Value {
    Value::Array(walls.iter().map(Wall::to_value).collect())
}

/// The circle through `a`, `b` and `m` in plan, sampled from `a` to `b` (`null` when colinear).
pub fn arc_path(a: [f64; 3], b: [f64; 3], m: [f64; 3], step: f64) -> Option<(Vec<[f64; 3]>, f64)> {
    let d = 2.0
        * (a[0] * (b[1] - m[1]) + b[0] * (m[1] - a[1]) + m[0] * (a[1] - b[1]));
    if d.abs() < 1e-9 {
        return None;
    }
    let sa = a[0].powi(2) + a[1].powi(2);
    let sb = b[0].powi(2) + b[1].powi(2);
    let sm = m[0].powi(2) + m[1].powi(2);
    let cx = (sa * (b[1] - m[1]) + sb * (m[1] - a[1]) + sm * (a[1] - b[1])) / d;
    let cy = (sa * (m[0] - b[0]) + sb * (a[0] - m[0]) + sm * (b[0] - a[0])) / d;
    let r = ((a[0] - cx).powi(2) + (a[1] - cy).powi(2)).sqrt();
    if !(r < 1e4) {
        return None;
    }
    let tau = std::f64::consts::TAU;
    let ang = |p: [f64; 3]| (p[1] - cy).atan2(p[0] - cx);
    let a0 = ang(a);
    let to_b = ((ang(b) - a0) % tau + tau) % tau;
    let to_m = ((ang(m) - a0) % tau + tau) % tau;
    let sweep = if to_m < to_b { to_b } else { to_b - tau };
    let fm = if sweep > 0.0 {
        to_m / sweep
    } else {
        (tau - to_m) / -sweep
    };
    let n = (128.min((sweep.abs() * r / step).ceil() as usize)).max(8);
    let mut out = Vec::with_capacity(n + 1);
    for k in 0..=n {
        let t = k as f64 / n as f64;
        let th = a0 + sweep * t;
        let z = if t <= fm {
            a[2] + (m[2] - a[2]) * (if fm > 0.0 { t / fm } else { 1.0 })
        } else {
            m[2] + (b[2] - m[2]) * ((t - fm) / (1.0 - fm))
        };
        out.push([cx + r * th.cos(), cy + r * th.sin(), z]);
    }
    Some((out, r))
}

/// Polyline to draw a wall (1.x `wallLine`).
pub fn wall_line(w: &Wall) -> Vec<[f64; 3]> {
    if w.shape == "anchor" && !w.points.is_empty() {
        let n = w.normal.unwrap_or([0.0, 0.0, 1.0]);
        let p = w.points[0];
        return vec![p, [p[0] + n[0] * 6.0, p[1] + n[1] * 6.0, p[2] + n[2] * 6.0]];
    }
    if w.shape == "curve" && w.points.len() == 3 {
        if let Some((pts, _)) = arc_path(w.points[0], w.points[1], w.points[2], 1.5) {
            return pts;
        }
        return vec![w.points[0], w.points[1]];
    }
    w.points.clone()
}

/// Which shapes the current engine exposes (`gusset` / `anchor` / `drop` may be missing).
pub fn visible_shapes(gussets: bool, anchors: bool, drop: bool) -> Vec<&'static str> {
    ALL_SHAPES
        .iter()
        .filter(|(k, _)| {
            (*k != "gusset" || gussets)
                && (*k != "anchor" || anchors)
                && (*k != "drop" || drop)
        })
        .map(|(k, _)| *k)
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WallClickMsg {
    None,
    Warn(String),
    Info(String),
}

#[derive(Clone, Debug)]
pub struct WallClickHit {
    pub body_id: String,
    pub point: [f64; 3],
    pub normal: [f64; 3],
}

/// Click handling for Draw Walls (`drawwalls.js` `click`).
pub struct WallDrawer {
    pub shape: String,
    pub pending: Vec<[f64; 3]>,
    pub hover_pt: Option<[f64; 3]>,
    gusset_side: Option<[f64; 3]>,
}

impl Default for WallDrawer {
    fn default() -> Self {
        Self::new_with_remembered(|_| None)
    }
}

impl WallDrawer {
    pub fn new_with_remembered(read_stored: impl FnOnce(&str) -> Option<String>) -> Self {
        let stored = read_stored(SHAPE_STORAGE_KEY);
        let shape = stored
            .filter(|s| ALL_SHAPES.iter().any(|(k, _)| k == s))
            .unwrap_or_else(|| "line".into());
        Self {
            shape,
            pending: Vec::new(),
            hover_pt: None,
            gusset_side: None,
        }
    }

    pub fn set_shape(&mut self, k: &str, persist: impl Fn(&str, &str)) {
        self.shape = k.to_string();
        self.pending.clear();
        self.gusset_side = None;
        persist(SHAPE_STORAGE_KEY, k);
    }

    pub fn hint(&self) -> &'static str {
        hint_for(&self.shape)
    }

    pub fn reset(&mut self) {
        self.pending.clear();
        self.hover_pt = None;
        self.gusset_side = None;
    }

    pub fn finish_path(&mut self, walls: &mut Vec<Wall>) -> bool {
        if self.shape != "path" || self.pending.len() < 2 {
            if self.shape == "path" {
                self.pending.clear();
            }
            return false;
        }
        let pts = std::mem::take(&mut self.pending);
        let w = if pts.len() == 2 {
            Wall { shape: "line".into(), points: pts, normal: None }
        } else {
            Wall { shape: "path".into(), points: pts, normal: None }
        };
        walls.push(w);
        true
    }

    pub fn undo(&mut self, walls: &mut Vec<Wall>) -> bool {
        if !self.pending.is_empty() {
            self.pending.pop();
            return true;
        }
        walls.pop().is_some()
    }

    /// Returns updated wall list when a wall was committed.
    pub fn click(
        &mut self,
        hit: Option<&WallClickHit>,
        walls: &[Wall],
        body: Option<&str>,
        body_name: impl Fn(&str) -> String,
        screen_near: bool,
    ) -> (Option<Vec<Wall>>, WallClickMsg) {
        let Some(hit) = hit else {
            return (
                None,
                WallClickMsg::Warn("Click on the part, under an overhang.".into()),
            );
        };
        if let Some(b) = body {
            if b != hit.body_id && !walls.is_empty() {
                return (
                    None,
                    WallClickMsg::Warn(format!(
                        "These walls are on {}. Finish (OK) them first, then draw on another body.",
                        body_name(b)
                    )),
                );
            }
        }
        let p = hit.point.map(|x| (x * 1e4).round() / 1e4);
        let n = hit.normal.map(|x| (x * 1e5).round() / 1e5);
        if self.shape == "anchor" {
            if n[2].abs() > 0.98 {
                return (
                    None,
                    WallClickMsg::Warn(
                        "Click a side of the part (or a back that leans up), not a floor or a ceiling."
                            .into(),
                    ),
                );
            }
            let mut list = walls.to_vec();
            list.push(Wall {
                shape: "anchor".into(),
                points: vec![p],
                normal: Some(n),
            });
            self.pending.clear();
            return (Some(list), WallClickMsg::None);
        }
        if self.shape == "gusset" && self.pending.is_empty() {
            if n[2].abs() > 0.5 {
                return (
                    None,
                    WallClickMsg::Warn(
                        "Start a gusset on a side of the part: an upright face below the overhang.".into(),
                    ),
                );
            }
            self.gusset_side = Some(n);
        }
        if self.pending.last().is_some() && screen_near {
            if self.shape == "path" {
                let mut list = walls.to_vec();
                self.finish_path(&mut list);
                return (Some(list), WallClickMsg::None);
            }
            return (None, WallClickMsg::None);
        }
        self.pending.push(p);
        let mut list = walls.to_vec();
        match self.shape.as_str() {
            "line" if self.pending.len() == 2 => {
                let pts = std::mem::take(&mut self.pending);
                list.push(Wall { shape: "line".into(), points: pts, normal: None });
                (Some(list), WallClickMsg::None)
            }
            "curve" if self.pending.len() == 3 => {
                let pts = std::mem::take(&mut self.pending);
                list.push(Wall { shape: "curve".into(), points: pts, normal: None });
                (Some(list), WallClickMsg::None)
            }
            "gusset" if self.pending.len() == 2 => {
                let pts = std::mem::take(&mut self.pending);
                let normal = self.gusset_side.take();
                list.push(Wall {
                    shape: "gusset".into(),
                    points: pts,
                    normal,
                });
                (Some(list), WallClickMsg::None)
            }
            _ => (None, WallClickMsg::None),
        }
    }

    /// `None` when nothing is pending (1.x `drawBand` with an empty `pending`).
    pub fn band_polyline(&self) -> Option<Vec<[f64; 3]>> {
        if self.pending.is_empty() {
            return None;
        }
        let Some(h) = self.hover_pt else {
            return Some(self.pending.clone());
        };
        if self.shape == "curve" && self.pending.len() == 2 {
            if let Some((pts, _)) = arc_path(self.pending[0], self.pending[1], h, 1.5) {
                return Some(pts);
            }
            return Some(vec![self.pending[0], self.pending[1]]);
        }
        let mut line = self.pending.clone();
        line.push(h);
        Some(line)
    }

    pub fn handle_key(&mut self, key: &str, ctrl: bool, walls: &mut Vec<Wall>) -> bool {
        match key {
            "Enter" if !self.pending.is_empty() => {
                self.finish_path(walls);
                true
            }
            "Escape" if !self.pending.is_empty() => {
                self.pending.clear();
                true
            }
            "Backspace" => {
                self.undo(walls);
                true
            }
            _ if ctrl && key.eq_ignore_ascii_case("z") => {
                self.undo(walls);
                true
            }
            _ => false,
        }
    }
}
