//! Glyph flattening matching `meshtools._FlatPen` / `text_contours` / `text_extent`.

use ttf_parser::{Face, GlyphId, OutlineBuilder};

use crate::entities::P2;
use crate::error::SketchError;

/// Options for `text_contours` / `text_extent` (spacing, alignment, line gap).
#[derive(Debug, Clone, PartialEq)]
pub struct TextOpts {
    pub spacing: f64,
    pub align: String,
    pub line_gap: f64,
}

impl Default for TextOpts {
    fn default() -> Self {
        Self {
            spacing: 0.0,
            align: "left".into(),
            line_gap: 1.2,
        }
    }
}

/// Same subdivision count as `meshtools._FlatPen(steps=8)`.
const STEPS: i32 = 8;

/// Python 3 `str.strip` whitespace (CPython `Py_UNICODE_ISSPACE`), not Rust `trim()`.
fn py_is_whitespace(c: char) -> bool {
    matches!(
        c,
        '\t'
            | '\n'
            | '\x0B'
            | '\x0C'
            | '\r'
            | '\x1C'
            | '\x1D'
            | '\x1E'
            | '\x1F'
            | ' '
            | '\u{0085}'
            | '\u{00A0}'
            | '\u{1680}'
            | '\u{2000}'..='\u{200A}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{202F}'
            | '\u{205F}'
            | '\u{3000}'
    )
}

fn py_strip(s: &str) -> &str {
    s.trim_matches(py_is_whitespace)
}

/// Outline contours of `text` in mm; baseline of the first line is y = 0.
/// Even-odd fill. Matches `meshtools.text_contours` when given the same font bytes.
pub fn text_contours(
    font: &[u8],
    text: &str,
    size: f64,
    opts: &TextOpts,
) -> Result<Vec<Vec<P2>>, SketchError> {
    if py_strip(text).is_empty() {
        return Ok(Vec::new());
    }
    if !size.is_finite() || size <= 0.0 {
        return Err(SketchError::new(
            "The text size must be more than 0.",
        ));
    }
    let face = Face::parse(font, 0).map_err(|_| SketchError::new("Could not read this font."))?;
    let upm = face.units_per_em() as f64;
    if upm <= 0.0 {
        return Err(SketchError::new("Could not read this font."));
    }
    let scale = size / upm;
    let spacing = opts.spacing;
    let align = opts.align.as_str();
    let line_gap = opts.line_gap;

    let lines: Vec<&str> = text.split('\n').collect();
    let mut widths = Vec::with_capacity(lines.len());
    let mut per_line: Vec<Vec<Vec<P2>>> = Vec::with_capacity(lines.len());

    for line in &lines {
        let mut x = 0.0_f64;
        let mut conts: Vec<Vec<P2>> = Vec::new();
        for ch in line.chars() {
            // 1.x: cmap miss → ".notdef" if in the glyph set, else advance 0.5 em and skip.
            let gid = match face.glyph_index(ch) {
                Some(id) => id,
                None => {
                    if let Some(id) = face.glyph_index_by_name(".notdef") {
                        id
                    } else if face.number_of_glyphs() > 0 {
                        // TrueType .notdef is glyph 0 when the post name is absent.
                        GlyphId(0)
                    } else {
                        x += upm * 0.5;
                        continue;
                    }
                }
            };
            let mut pen = FlatPen::new();
            let _ = face.outline_glyph(gid, &mut pen);
            for c in pen.contours {
                let shifted: Vec<P2> = c.iter().map(|p| [p[0] + x, p[1]]).collect();
                conts.push(shifted);
            }
            let adv = face
                .glyph_hor_advance(gid)
                .map(|a| a as f64)
                .unwrap_or(upm * 0.5);
            x += adv + spacing / scale;
        }
        let width = if line.is_empty() {
            x
        } else {
            x - spacing / scale
        };
        widths.push(width);
        per_line.push(conts);
    }

    let maxw = widths.iter().cloned().fold(0.0_f64, f64::max);
    let mut out = Vec::new();
    for (i, conts) in per_line.into_iter().enumerate() {
        let dx = match align {
            "center" => (maxw - widths[i]) / 2.0,
            "right" => maxw - widths[i],
            _ => 0.0,
        };
        let dy = -(i as f64) * upm * line_gap;
        for c in conts {
            out.push(
                c.into_iter()
                    .map(|p| [(p[0] + dx) * scale, (p[1] + dy) * scale])
                    .collect(),
            );
        }
    }
    Ok(out)
}

/// Width and height of the text outlines in mm (`hi - lo` of 1.x `text_extent`).
pub fn text_extent(
    font: &[u8],
    text: &str,
    size: f64,
    opts: &TextOpts,
) -> Result<[f64; 2], SketchError> {
    let contours = text_contours(font, text, size, opts)?;
    if contours.is_empty() {
        return Ok([0.0, 0.0]);
    }
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for c in &contours {
        for p in c {
            min_x = min_x.min(p[0]);
            min_y = min_y.min(p[1]);
            max_x = max_x.max(p[0]);
            max_y = max_y.max(p[1]);
        }
    }
    Ok([max_x - min_x, max_y - min_y])
}

/// Flattening pen matching `meshtools._FlatPen`.
struct FlatPen {
    contours: Vec<Vec<P2>>,
    cur: Vec<P2>,
    last_op: LastOp,
    /// Drop the last point on `close` only if it was a synthetic closing line
    /// (ttf-parser glyf `line_to(start)` before `close`), not an explicit end-at-start.
    drop_closing_line: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LastOp {
    None,
    Move,
    Line,
    Quad,
    Curve,
}

impl FlatPen {
    fn new() -> Self {
        Self {
            contours: Vec::new(),
            cur: Vec::new(),
            last_op: LastOp::None,
            drop_closing_line: false,
        }
    }

    fn push(&mut self, x: f64, y: f64) {
        self.cur.push([x, y]);
    }
}

impl OutlineBuilder for FlatPen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.cur = vec![[x as f64, y as f64]];
        self.last_op = LastOp::Move;
        self.drop_closing_line = false;
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let p = [x as f64, y as f64];
        if let Some(&first) = self.cur.first() {
            let already_at_start = self
                .cur
                .last()
                .map(|l| points_eq(*l, first))
                .unwrap_or(false);
            if points_eq(p, first) && already_at_start {
                // Redundant glyf close after an explicit end-at-start: ignore it so
                // the explicit closing point is kept (fontTools keeps it).
                self.last_op = LastOp::Line;
                self.drop_closing_line = false;
                return;
            }
            // Candidate synthetic close: line to start from elsewhere. Drop on close().
            self.drop_closing_line = points_eq(p, first);
        } else {
            self.drop_closing_line = false;
        }
        self.push(p[0], p[1]);
        self.last_op = LastOp::Line;
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        // Atomic quadratic: same as FlatPen.qCurveTo after multi-off-curve split, steps=8.
        let p0 = *self.cur.last().expect("quad_to without move_to");
        let c = [x1 as f64, y1 as f64];
        let p2 = [x as f64, y as f64];
        for k in 1..=STEPS {
            let t = k as f64 / STEPS as f64;
            let mt = 1.0 - t;
            self.push(
                mt * mt * p0[0] + 2.0 * mt * t * c[0] + t * t * p2[0],
                mt * mt * p0[1] + 2.0 * mt * t * c[1] + t * t * p2[1],
            );
        }
        self.last_op = LastOp::Quad;
        self.drop_closing_line = false;
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let p0 = *self.cur.last().expect("curve_to without move_to");
        let c1 = [x1 as f64, y1 as f64];
        let c2 = [x2 as f64, y2 as f64];
        let p3 = [x as f64, y as f64];
        for k in 1..=STEPS {
            let t = k as f64 / STEPS as f64;
            let mt = 1.0 - t;
            self.push(
                mt * mt * mt * p0[0]
                    + 3.0 * mt * mt * t * c1[0]
                    + 3.0 * mt * t * t * c2[0]
                    + t * t * t * p3[0],
                mt * mt * mt * p0[1]
                    + 3.0 * mt * mt * t * c1[1]
                    + 3.0 * mt * t * t * c2[1]
                    + t * t * t * p3[1],
            );
        }
        self.last_op = LastOp::Curve;
        self.drop_closing_line = false;
    }

    fn close(&mut self) {
        // Only drop when the pen/library added an implicit closing line (glyf).
        // A contour whose last explicit point equals its first keeps that point.
        if self.drop_closing_line && self.cur.len() >= 2 {
            let first = self.cur[0];
            let last = *self.cur.last().unwrap();
            if points_eq(first, last) {
                self.cur.pop();
            }
        }
        self.drop_closing_line = false;
        if self.cur.len() >= 3 {
            self.contours.push(std::mem::take(&mut self.cur));
        } else {
            self.cur.clear();
        }
        self.last_op = LastOp::None;
    }
}

fn points_eq(a: P2, b: P2) -> bool {
    a[0] == b[0] && a[1] == b[1]
}
