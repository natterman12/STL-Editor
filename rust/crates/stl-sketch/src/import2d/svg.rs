//! SVG import via `usvg`, sampling like `web/js/import2d.js` (equal arc-length, mm, y up).
//!
//! Circles and ellipses: Chrome's `getTotalLength` is a shorter cubic-oval measure than
//! usvg's cubics, so those shapes use a Chrome-calibrated length and a geometric sampler
//! (task rule: own flattener for a path type when usvg misses the matching rule).

use serde_json::Map;
use usvg::tiny_skia_path::{Path as SkiaPath, PathSegment, Point as SkiaPoint};
use usvg::{Node, Options, Transform, Tree};

use crate::entities::{Entity, Polyline, P2};
use crate::SketchError;

const PX_TO_MM: f64 = 25.4 / 96.0;
const SVG_BAD: &str = "This SVG file can’t be read.";
type Point = [f64; 2];

const CONIC_W: f64 = std::f64::consts::FRAC_1_SQRT_2; // √2/2 — Skia quarter-oval weight
const PATH_MEASURE_TOL: f64 = 0.5; // Skia PathMeasure default tolerance (user units)

/// 1.x `querySelectorAll('path,line,polyline,polygon,rect,circle,ellipse')` element kinds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ShapeKind {
    Path,
    Line,
    Polyline,
    Polygon,
    Rect,
    Circle,
    Ellipse,
}

impl ShapeKind {
    fn closed_by_type(self) -> bool {
        matches!(self, ShapeKind::Rect | ShapeKind::Circle | ShapeKind::Ellipse | ShapeKind::Polygon)
    }
}

fn conic_at(p0: Point, p1: Point, p2: Point, w: f64, t: f64) -> Point {
    let u = 1.0 - t;
    let d = u * u + 2.0 * u * t * w + t * t;
    [
        (u * u * p0[0] + 2.0 * u * t * w * p1[0] + t * t * p2[0]) / d,
        (u * u * p0[1] + 2.0 * u * t * w * p1[1] + t * t * p2[1]) / d,
    ]
}

/// Flatten a quarter-oval conic like Skia PathMeasure (absolute chord flatness).
fn flatten_conic(p0: Point, p1: Point, p2: Point, w: f64, out: &mut Vec<Point>, t0: f64, t1: f64, depth: u32) {
    let a = conic_at(p0, p1, p2, w, t0);
    let b = conic_at(p0, p1, p2, w, t1);
    let tm = 0.5 * (t0 + t1);
    let m = conic_at(p0, p1, p2, w, tm);
    let c = [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5];
    let err = (m[0] - c[0]).hypot(m[1] - c[1]);
    if err <= PATH_MEASURE_TOL || depth >= 20 {
        out.push(b);
        return;
    }
    flatten_conic(p0, p1, p2, w, out, t0, tm, depth + 1);
    flatten_conic(p0, p1, p2, w, out, tm, t1, depth + 1);
}

/// Chrome-compatible oval polyline (4 conics, PathMeasure tol 0.5), then project to the oval.
fn sample_chrome_oval(cx: f64, cy: f64, rx: f64, ry: f64) -> Vec<Point> {
    let quarters = [
        ([rx, 0.0], [rx, ry], [0.0, ry]),
        ([0.0, ry], [-rx, ry], [-rx, 0.0]),
        ([-rx, 0.0], [-rx, -ry], [0.0, -ry]),
        ([0.0, -ry], [rx, -ry], [rx, 0.0]),
    ];
    let mut dense = vec![[rx, 0.0]];
    for (p0, p1, p2) in quarters {
        flatten_conic(p0, p1, p2, CONIC_W, &mut dense, 0.0, 1.0, 0);
    }
    let chrome_len = polyline_length(&dense);
    let mut sampled = sample_arc_length(&dense, true, Some(chrome_len));
    for p in &mut sampled {
        let dx = p[0];
        let dy = p[1];
        if rx > 1e-12 && ry > 1e-12 {
            let ux = dx / rx;
            let uy = dy / ry;
            let nrm = (ux * ux + uy * uy).sqrt();
            if nrm > 1e-12 {
                p[0] = cx + rx * ux / nrm;
                p[1] = cy + ry * uy / nrm;
            } else {
                p[0] += cx;
                p[1] += cy;
            }
        } else {
            p[0] += cx;
            p[1] += cy;
        }
    }
    sampled
}

/// Flatten a cubic Bezier to a polyline (chord-error style), appending points after the start.
fn flatten_cubic(p0: Point, p1: Point, p2: Point, p3: Point, tol: f64, out: &mut Vec<Point>) {
    fn dist_point_line(p: Point, a: Point, b: Point) -> f64 {
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let len2 = dx * dx + dy * dy;
        if len2 < 1e-24 {
            return (p[0] - a[0]).hypot(p[1] - a[1]);
        }
        let t = ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len2;
        let q = [a[0] + t * dx, a[1] + t * dy];
        (p[0] - q[0]).hypot(p[1] - q[1])
    }
    fn rec(p0: Point, p1: Point, p2: Point, p3: Point, tol: f64, out: &mut Vec<Point>, depth: u32) {
        let d = dist_point_line(p1, p0, p3).max(dist_point_line(p2, p0, p3));
        if d <= tol || depth >= 16 {
            out.push(p3);
            return;
        }
        let m01 = mid(p0, p1);
        let m12 = mid(p1, p2);
        let m23 = mid(p2, p3);
        let m012 = mid(m01, m12);
        let m123 = mid(m12, m23);
        let m0123 = mid(m012, m123);
        rec(p0, m01, m012, m0123, tol, out, depth + 1);
        rec(m0123, m123, m23, p3, tol, out, depth + 1);
    }
    rec(p0, p1, p2, p3, tol, out, 0);
}

fn mid(a: Point, b: Point) -> Point {
    [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5]
}

fn flatten_quad(p0: Point, p1: Point, p2: Point, tol: f64, out: &mut Vec<Point>) {
    let c1 = [p0[0] + 2.0 / 3.0 * (p1[0] - p0[0]), p0[1] + 2.0 / 3.0 * (p1[1] - p0[1])];
    let c2 = [p2[0] + 2.0 / 3.0 * (p1[0] - p2[0]), p2[1] + 2.0 / 3.0 * (p1[1] - p2[1])];
    flatten_cubic(p0, c1, c2, p2, tol, out);
}

/// Split a path into subpaths (no gap segments between MoveTos), like Chrome PathMeasure.
fn path_subpaths(path: &SkiaPath) -> Vec<Vec<Point>> {
    let mut subs: Vec<Vec<Point>> = Vec::new();
    let mut cur_pts: Vec<Point> = Vec::new();
    let mut start = [0.0_f64, 0.0];
    let mut cur = [0.0_f64, 0.0];
    let tol = 0.01;
    let flush = |pts: &mut Vec<Point>, subs: &mut Vec<Vec<Point>>| {
        if pts.len() >= 2 {
            subs.push(std::mem::take(pts));
        } else {
            pts.clear();
        }
    };
    for seg in path.segments() {
        match seg {
            PathSegment::MoveTo(p) => {
                flush(&mut cur_pts, &mut subs);
                cur = [p.x as f64, p.y as f64];
                start = cur;
                cur_pts.push(cur);
            }
            PathSegment::LineTo(p) => {
                cur = [p.x as f64, p.y as f64];
                cur_pts.push(cur);
            }
            PathSegment::QuadTo(p1, p) => {
                let end = [p.x as f64, p.y as f64];
                flatten_quad(cur, [p1.x as f64, p1.y as f64], end, tol, &mut cur_pts);
                cur = end;
            }
            PathSegment::CubicTo(p1, p2, p) => {
                let end = [p.x as f64, p.y as f64];
                flatten_cubic(
                    cur,
                    [p1.x as f64, p1.y as f64],
                    [p2.x as f64, p2.y as f64],
                    end,
                    tol,
                    &mut cur_pts,
                );
                cur = end;
            }
            PathSegment::Close => {
                if (cur[0] - start[0]).hypot(cur[1] - start[1]) > 1e-12 {
                    cur_pts.push(start);
                }
                cur = start;
            }
        }
    }
    flush(&mut cur_pts, &mut subs);
    subs
}

fn path_has_close(path: &SkiaPath) -> bool {
    path.segments().any(|s| matches!(s, PathSegment::Close))
}

/// usvg encodes `<circle>` / `<ellipse>` as Move + 4 cubics + Close. Bounds → centre/radii.
fn oval_from_bounds(path: &SkiaPath) -> Option<(Point, f64, f64)> {
    let mut start: Option<Point> = None;
    let mut cubics = 0usize;
    let mut closed = false;
    let mut other = false;
    for seg in path.segments() {
        match seg {
            PathSegment::MoveTo(p) => {
                if start.is_some() {
                    other = true;
                }
                start = Some([p.x as f64, p.y as f64]);
            }
            PathSegment::CubicTo(..) => cubics += 1,
            PathSegment::Close => closed = true,
            PathSegment::LineTo(..) | PathSegment::QuadTo(..) => other = true,
        }
    }
    if other || !closed || cubics != 4 {
        return None;
    }
    let b = path.bounds();
    let (w, h) = (b.width() as f64, b.height() as f64);
    if w < 1e-9 || h < 1e-9 {
        return None;
    }
    let cx = b.left() as f64 + w * 0.5;
    let cy = b.top() as f64 + h * 0.5;
    Some(([cx, cy], w * 0.5, h * 0.5))
}

fn polyline_length(pts: &[Point]) -> f64 {
    pts.windows(2).map(|w| (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1])).sum()
}

fn sample_count(total: f64, closed: bool) -> (usize, usize) {
    let n = ((total / 0.5).ceil() as usize).clamp(8, 4000);
    let count = if closed { n } else { n + 1 };
    (n, count)
}

fn point_at_poly_length(pts: &[Point], target: f64) -> Point {
    if pts.len() < 2 {
        return pts.first().copied().unwrap_or([0.0, 0.0]);
    }
    let mut cum = vec![0.0];
    for w in pts.windows(2) {
        cum.push(cum.last().unwrap() + (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]));
    }
    let total = *cum.last().unwrap();
    let target = target.clamp(0.0, total);
    let mut lo = 0usize;
    let mut hi = cum.len() - 1;
    while lo + 1 < hi {
        let mid = (lo + hi) / 2;
        if cum[mid] <= target {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let (i0, i1) = if lo + 1 < pts.len() {
        (lo, lo + 1)
    } else {
        (pts.len() - 2, pts.len() - 1)
    };
    let seg_len = cum[i1] - cum[i0];
    let t = if seg_len > 0.0 {
        ((target - cum[i0]) / seg_len).clamp(0.0, 1.0)
    } else {
        0.0
    };
    [
        pts[i0][0] + t * (pts[i1][0] - pts[i0][0]),
        pts[i0][1] + t * (pts[i1][1] - pts[i0][1]),
    ]
}

/// Sample `n` (closed) or `n+1` (open) points at equal arc length, like `getPointAtLength`.
fn sample_arc_length(pts: &[Point], closed: bool, total_override: Option<f64>) -> Vec<Point> {
    if pts.len() < 2 {
        return Vec::new();
    }
    let poly_total = polyline_length(pts);
    if !(poly_total > 0.0) {
        return Vec::new();
    }
    let total = total_override.unwrap_or(poly_total);
    let (n, count) = sample_count(total, closed);
    let mut out = Vec::with_capacity(count);
    for k in 0..count {
        let target = poly_total * (k as f64) / (n as f64);
        out.push(point_at_poly_length(pts, target));
    }
    out
}

/// Sample across multiple subpaths with no gap length (Chrome getTotalLength / getPointAtLength).
fn sample_subpaths(subs: &[Vec<Point>], closed: bool) -> Vec<Point> {
    let lengths: Vec<f64> = subs.iter().map(|s| polyline_length(s)).collect();
    let total: f64 = lengths.iter().sum();
    if !(total > 0.0) {
        return Vec::new();
    }
    let (n, count) = sample_count(total, closed);
    let mut out = Vec::with_capacity(count);
    for k in 0..count {
        let mut target = total * (k as f64) / (n as f64);
        // Walk subpaths; a sample never lands on a synthetic gap. At an exact subpath
        // boundary Chrome returns the end of the previous subpath (not the next start).
        let mut i = 0usize;
        while i + 1 < subs.len() && target > lengths[i] {
            target -= lengths[i];
            i += 1;
        }
        out.push(point_at_poly_length(&subs[i], target.min(lengths[i])));
    }
    out
}

fn transform_point(ts: Transform, p: Point) -> Point {
    let mut pt = SkiaPoint::from_xy(p[0] as f32, p[1] as f32);
    ts.map_point(&mut pt);
    [pt.x as f64, pt.y as f64]
}

/// JS `parseFloat`: leading whitespace, then a leading number (so `"  10em"` → 10).
pub fn parse_float_js(s: &str) -> f64 {
    let s = s.trim_start();
    if s.is_empty() {
        return f64::NAN;
    }
    let mut end = 0usize;
    let bytes = s.as_bytes();
    if bytes[0] == b'+' || bytes[0] == b'-' {
        end = 1;
    }
    let start_digits = end;
    while end < bytes.len() && bytes[end].is_ascii_digit() {
        end += 1;
    }
    if end < bytes.len() && bytes[end] == b'.' {
        end += 1;
        while end < bytes.len() && bytes[end].is_ascii_digit() {
            end += 1;
        }
    }
    if end < bytes.len() && (bytes[end] == b'e' || bytes[end] == b'E') {
        let mut e = end + 1;
        if e < bytes.len() && (bytes[e] == b'+' || bytes[e] == b'-') {
            e += 1;
        }
        let e_start = e;
        while e < bytes.len() && bytes[e].is_ascii_digit() {
            e += 1;
        }
        if e > e_start {
            end = e;
        }
    }
    if end == start_digits || (end == start_digits + 1 && (bytes[0] == b'+' || bytes[0] == b'-') && start_digits == 1) {
        // no digits — but "10em" with end past digits is fine; bare sign is NaN
        if end <= 1 && (s.starts_with('+') || s.starts_with('-') || end == 0) && !s.as_bytes().get(start_digits).map(|b| b.is_ascii_digit()).unwrap_or(false) {
            return f64::NAN;
        }
    }
    s[..end].parse().unwrap_or(f64::NAN)
}

/// Index of `>` that ends an open tag, skipping `>` inside quoted attribute values.
fn find_tag_gt(s: &str) -> Option<usize> {
    let mut quote: Option<char> = None;
    for (i, c) in s.char_indices() {
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            '"' | '\'' => quote = Some(c),
            '>' => return Some(i),
            _ => {}
        }
    }
    None
}

/// Whole-attribute lookup on an open tag (not `stroke-width` / `data-width`).
pub fn svg_tag_attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let lower = tag.to_ascii_lowercase();
    let key = format!("{}=", name.to_ascii_lowercase());
    let mut search_from = 0usize;
    while let Some(rel) = lower[search_from..].find(&key) {
        let i = search_from + rel;
        let ok_before = i == 0
            || lower.as_bytes()[i - 1].is_ascii_whitespace()
            || lower.as_bytes()[i - 1] == b'<';
        if ok_before {
            let rest = &tag[i + key.len()..];
            let q = rest.chars().next()?;
            if q == '"' || q == '\'' {
                let end = rest[1..].find(q)? + 1;
                return Some(&rest[1..end]);
            }
        }
        search_from = i + key.len();
    }
    None
}

/// Shared 1.x unit → mm scale (`px` / unknown → 25.4/96).
pub fn mm_per_unit(unit: &str) -> f64 {
    match unit {
        "mm" => 1.0,
        "cm" => 10.0,
        "in" => 25.4,
        "pt" => 25.4 / 72.0,
        "pc" => 25.4 / 6.0,
        _ => 25.4 / 96.0,
    }
}

fn trailing_svg_unit(w_attr: &str) -> String {
    // 1.x: (wAttr.match(/[a-z%]+$/i) || [''])[0]
    w_attr
        .trim_start()
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_alphabetic() || *c == '%')
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>()
        .to_ascii_lowercase()
}

fn svg_root_open_tag(text: &str) -> Option<&str> {
    let lower = text.to_ascii_lowercase();
    let svg_i = lower.find("<svg")?;
    let rel_end = find_tag_gt(&text[svg_i..])?;
    Some(&text[svg_i..svg_i + rel_end])
}

fn split_viewbox_nums(vb: &str) -> Vec<f64> {
    vb.split(|c: char| c.is_whitespace() || c == ',')
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse().ok())
        .collect()
}

/// 1.x `mmPer` from width / viewBox (including `width="100%"` → parseFloat 100, unit `%` → px scale).
pub fn mm_per_from_svg(text: &str) -> f64 {
    let Some(tag) = svg_root_open_tag(text) else {
        return PX_TO_MM;
    };
    let w_attr = svg_tag_attr(tag, "width").unwrap_or("");
    let wv = {
        let v = parse_float_js(w_attr);
        if v.is_finite() { v } else { 0.0 }
    };
    let per = mm_per_unit(&trailing_svg_unit(w_attr));
    let vb_w = svg_tag_attr(tag, "viewBox").and_then(|vb| {
        let parts = split_viewbox_nums(vb);
        (parts.len() == 4 && parts[2] > 0.0).then_some(parts[2])
    });
    match vb_w {
        Some(vb_w) => (if wv != 0.0 { wv * per } else { vb_w * 25.4 / 96.0 }) / vb_w,
        None => 25.4 / 96.0,
    }
}

/// Drawing size in mm for SVG tolerance: `mmPer * viewBox.width` (or width), matching 1.x.
pub fn svg_drawing_size_mm(text: &str) -> Option<f64> {
    let tag = svg_root_open_tag(text)?;
    let w_attr = svg_tag_attr(tag, "width").unwrap_or("");
    let wv = {
        let v = parse_float_js(w_attr);
        if v.is_finite() { v } else { 0.0 }
    };
    let per = mm_per_unit(&trailing_svg_unit(w_attr));
    let vb = svg_tag_attr(tag, "viewBox").and_then(|vb| {
        let parts = split_viewbox_nums(vb);
        (parts.len() == 4).then_some(parts)
    });
    let mm_per = match vb.as_ref().filter(|p| p[2] > 0.0) {
        Some(parts) => (if wv != 0.0 { wv * per } else { parts[2] * 25.4 / 96.0 }) / parts[2],
        None => 25.4 / 96.0,
    };
    let width_user = vb.as_ref().map(|p| p[2]).unwrap_or(if wv != 0.0 { wv } else { 0.0 });
    Some((width_user * mm_per).abs())
}

struct EmitCtx {
    /// Maps abs (canvas) coords back to SVG user/viewBox space: `root_ts.invert()`.
    to_user: Transform,
    mm_per: f64,
}

fn emit_polyline(pts: Vec<Point>, closed: bool, abs_ts: Transform, ctx: &EmitCtx, out: &mut Vec<Entity>) {
    if pts.len() < 2 {
        return;
    }
    let pts: Vec<P2> = pts
        .into_iter()
        .map(|p| {
            let canvas = transform_point(abs_ts, p);
            let user = transform_point(ctx.to_user, canvas);
            [user[0] * ctx.mm_per, -user[1] * ctx.mm_per]
        })
        .collect();
    out.push(Entity::Polyline(Polyline {
        id: None,
        pts,
        closed: Some(closed),
        construction: None,
        extra: Map::new(),
    }));
}

/// 1.x skips shapes inside defs/clipPath/mask/symbol/pattern/marker; `<use>` is not a shape.
fn source_shape_kinds(text: &str) -> Vec<ShapeKind> {
    // Lightweight scan: find tags, track forbidden ancestor depth via a stack of element names.
    let lower = text.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let mut i = 0usize;
    let mut stack: Vec<&str> = Vec::new();
    let mut out = Vec::new();
    let forbidden = ["defs", "clippath", "mask", "symbol", "pattern", "marker"];
    while i < bytes.len() {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        if i + 1 < bytes.len() && bytes[i + 1] == b'!' {
            // comment / doctype
            if lower[i..].starts_with("<!--") {
                if let Some(end) = lower[i + 4..].find("-->") {
                    i = i + 4 + end + 3;
                    continue;
                }
            }
            i += 1;
            continue;
        }
        if i + 1 < bytes.len() && bytes[i + 1] == b'/' {
            // closing tag
            let rest = &lower[i + 2..];
            let name_end = rest
                .find(|c: char| c.is_whitespace() || c == '>')
                .unwrap_or(0);
            let name = &rest[..name_end];
            if let Some(pos) = stack.iter().rposition(|n| *n == name) {
                stack.truncate(pos);
            }
            if let Some(end) = rest.find('>') {
                i = i + 2 + end + 1;
            } else {
                i += 1;
            }
            continue;
        }
        let rest = &lower[i + 1..];
        let name_end = rest
            .find(|c: char| !c.is_ascii_alphanumeric() && c != ':' && c != '-')
            .unwrap_or(0);
        if name_end == 0 {
            i += 1;
            continue;
        }
        let name = &rest[..name_end];
        let tag_end_rel = find_tag_gt(rest).unwrap_or(0);
        let tag_body = &rest[..tag_end_rel];
        let self_close = tag_body.trim_end().ends_with('/');
        let orig_tag = &text[i + 1..i + 1 + tag_end_rel];
        // Kind only (for circle/ellipse oval routing). Closed is taken from the usvg path
        // itself later — never by pairing scan closed-flags by position.
        let in_forbidden = stack.iter().any(|n| forbidden.contains(n));
        if !in_forbidden {
            let kind = match name {
                "path" => Some(ShapeKind::Path),
                "line" => Some(ShapeKind::Line),
                "polyline" => Some(ShapeKind::Polyline),
                "polygon" => Some(ShapeKind::Polygon),
                "rect" => Some(ShapeKind::Rect),
                "circle" => Some(ShapeKind::Circle),
                "ellipse" => Some(ShapeKind::Ellipse),
                _ => None,
            };
            if let Some(kind) = kind {
                // Skip empty `d=""` paths — usvg and 1.x both drop them; keeping them in
                // `kinds` would shift later circle/ellipse routing.
                let skip_empty_path = kind == ShapeKind::Path
                    && svg_tag_attr(orig_tag, "d").map(|d| d.trim().is_empty()).unwrap_or(true);
                if !skip_empty_path {
                    out.push(kind);
                }
            }
        }
        if !self_close && name != "path" && name != "line" && name != "polyline" && name != "polygon"
            && name != "rect" && name != "circle" && name != "ellipse" && name != "use"
            && name != "image" && !name.starts_with("?")
        {
            stack.push(name);
        } else if !self_close && matches!(name, "defs" | "clippath" | "mask" | "symbol" | "pattern" | "marker" | "g" | "svg" | "a") {
            stack.push(name);
        }
        i = i + 1 + tag_end_rel + 1;
    }
    out
}

/// Drop `<use>` elements so usvg does not expand them (1.x never samples `<use>`).
fn strip_use_elements(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let lower = text.to_ascii_lowercase();
    let mut i = 0usize;
    while i < text.len() {
        if let Some(rel) = lower[i..].find("<use") {
            let start = i + rel;
            out.push_str(&text[i..start]);
            let after = &lower[start..];
            // self-closing or paired
            if let Some(end_rel) = after.find('>') {
                let tag = &after[..end_rel + 1];
                let end = start + end_rel + 1;
                if tag.trim_end().ends_with("/>") || tag.contains("/>") {
                    i = end;
                    continue;
                }
                // find </use>
                if let Some(close) = lower[end..].find("</use") {
                    if let Some(gt) = lower[end + close..].find('>') {
                        i = end + close + gt + 1;
                        continue;
                    }
                }
                i = end;
                continue;
            }
            out.push(text.as_bytes()[start] as char);
            i = start + 1;
        } else {
            out.push_str(&text[i..]);
            break;
        }
    }
    out
}

fn walk_group(
    group: &usvg::Group,
    ctx: &EmitCtx,
    kinds: &[ShapeKind],
    kind_i: &mut usize,
    out: &mut Vec<Entity>,
) {
    for node in group.children() {
        match node {
            Node::Group(g) => walk_group(g, ctx, kinds, kind_i, out),
            Node::Path(path) => {
                // 1.x measures fill=none paths; do not skip usvg's visibility=false for that case.
                let kind = if *kind_i < kinds.len() {
                    let k = kinds[*kind_i];
                    *kind_i += 1;
                    k
                } else {
                    ShapeKind::Path
                };
                let ts = path.abs_transform();
                // Oval route for real <circle>/<ellipse> (Chrome's circle measure). A 4-cubic
                // blob is not marked as oval at the node level and is measured as its curve.
                // Standard-kappa <path> is also measured as cubics so it matches Chrome's
                // getTotalLength on path data (not the calibrated circle element measure).
                let take_oval = matches!(kind, ShapeKind::Circle | ShapeKind::Ellipse);
                if take_oval {
                    if let Some((center, rx, ry)) = oval_from_bounds(path.data()) {
                        let sampled = sample_chrome_oval(center[0], center[1], rx, ry);
                        emit_polyline(sampled, true, ts, ctx, out);
                        continue;
                    }
                }
                let subs = path_subpaths(path.data());
                if subs.is_empty() {
                    continue;
                }
                let total: f64 = subs.iter().map(|s| polyline_length(s)).sum();
                if !(total > 0.0) {
                    continue;
                }
                // Closed from the usvg path (Close segment) plus element-type override for
                // rect/polygon/circle/ellipse. Never pair a text-scan closed flag by position.
                let closed = kind.closed_by_type() || path_has_close(path.data());
                let sampled = if subs.len() == 1 {
                    sample_arc_length(&subs[0], closed, None)
                } else {
                    sample_subpaths(&subs, closed)
                };
                emit_polyline(sampled, closed, ts, ctx, out);
            }
            Node::Image(_) | Node::Text(_) => {}
        }
    }
}

/// 1.x still measures `display:none` shapes; usvg drops them. Neutralize for the parse copy.
/// Zero-size `<rect>` is also dropped by usvg while Chrome measures its perimeter — rewrite
/// those rects to an equivalent closed path so sample counts stay aligned.
fn svg_text_for_usvg(text: &str) -> String {
    let mut s = strip_use_elements(text);
    // display:none / display: none (style or attribute) → inline
    let lower = s.to_ascii_lowercase();
    let mut out = String::with_capacity(s.len() + 16);
    let mut i = 0usize;
    while i < s.len() {
        if let Some(rel) = lower[i..].find("display") {
            let start = i + rel;
            out.push_str(&s[i..start]);
            let after = &lower[start..];
            let rest = after.strip_prefix("display").unwrap_or(after);
            let mut j = 0usize;
            while j < rest.len() && rest.as_bytes()[j].is_ascii_whitespace() {
                j += 1;
            }
            if rest.as_bytes().get(j) == Some(&b':') || rest.as_bytes().get(j) == Some(&b'=') {
                let sep = rest.as_bytes()[j] as char;
                j += 1;
                while j < rest.len() && rest.as_bytes()[j].is_ascii_whitespace() {
                    j += 1;
                }
                if rest[j..].starts_with("none") {
                    let end = start + "display".len() + j + 4;
                    out.push_str("display");
                    out.push(sep);
                    out.push_str("inline");
                    i = end;
                    continue;
                }
            }
            out.push_str(&s[start..start + 7]);
            i = start + 7;
        } else {
            out.push_str(&s[i..]);
            break;
        }
    }
    s = out;
    rewrite_zero_size_rects(&s)
}

fn rewrite_zero_size_rects(text: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let mut out = String::with_capacity(text.len() + 32);
    let mut i = 0usize;
    while i < text.len() {
        if let Some(rel) = lower[i..].find("<rect") {
            let start = i + rel;
            out.push_str(&text[i..start]);
            let after = &text[start..];
            let Some(gt) = find_tag_gt(after) else {
                out.push_str(&text[start..]);
                break;
            };
            let tag = &after[5..gt]; // after "<rect"
            let x = svg_tag_attr(tag, "x").map(parse_float_js).filter(|v| v.is_finite()).unwrap_or(0.0);
            let y = svg_tag_attr(tag, "y").map(parse_float_js).filter(|v| v.is_finite()).unwrap_or(0.0);
            let w = svg_tag_attr(tag, "width").map(parse_float_js).filter(|v| v.is_finite()).unwrap_or(0.0);
            let h = svg_tag_attr(tag, "height").map(parse_float_js).filter(|v| v.is_finite()).unwrap_or(0.0);
            if w == 0.0 || h == 0.0 {
                // Perimeter path matching Chrome's zero-size rect measure.
                let d = format!(
                    "M{x},{y} H{x2} V{y2} H{x} Z",
                    x2 = x + w,
                    y2 = y + h
                );
                out.push_str(&format!(r#"<path d="{d}"/>"#));
                i = start + gt + 1;
                // skip optional closing </rect> is N/A for self-closing; if paired, leave it
                continue;
            }
            out.push_str(&after[..=gt]);
            i = start + gt + 1;
        } else {
            out.push_str(&text[i..]);
            break;
        }
    }
    out
}

/// SVG text → sketch polylines. Units/transforms match 1.x; `width="100%"` must not panic.
pub fn import_svg(text: &str) -> Result<Vec<Entity>, SketchError> {
    let trimmed = text.trim_start();
    let lower = trimmed.to_ascii_lowercase();
    if !lower.contains("<svg") {
        return Err(SketchError::new(SVG_BAD));
    }

    let kinds = source_shape_kinds(text);
    let stripped = svg_text_for_usvg(text);
    let mm_per = mm_per_from_svg(text);
    let opt = Options::default();
    let tree = match Tree::from_str(&stripped, &opt) {
        Ok(t) => t,
        Err(_) => return Err(SketchError::new(SVG_BAD)),
    };
    let size = tree.size();
    // Reconstruct usvg's viewBox→size fit (respect preserveAspectRatio). 1.x cancels via getCTM.
    let vb_fit = viewbox_fit_transform(text, size.width() as f64, size.height() as f64);
    let to_user = vb_fit.invert().unwrap_or_else(Transform::default);
    let ctx = EmitCtx { to_user, mm_per };

    let mut out = Vec::new();
    let mut kind_i = 0usize;
    walk_group(tree.root(), &ctx, &kinds, &mut kind_i, &mut out);
    let _ = kinds;
    Ok(out)
}

/// usvg viewBox fit into `size`, honouring `preserveAspectRatio` (default xMidYMid meet).
fn viewbox_fit_transform(text: &str, size_w: f64, size_h: f64) -> Transform {
    let (vb_x, vb_y, vb_w, vb_h) = parse_viewbox(text).unwrap_or((0.0, 0.0, size_w, size_h));
    if vb_w <= 0.0 || vb_h <= 0.0 || size_w <= 0.0 || size_h <= 0.0 {
        return Transform::default();
    }
    let (align, meet_or_slice) = parse_preserve_aspect_ratio(text);
    let sx = size_w / vb_w;
    let sy = size_h / vb_h;
    let (sx, sy) = if align == "none" {
        (sx, sy)
    } else if meet_or_slice == "slice" {
        let s = sx.max(sy);
        (s, s)
    } else {
        let s = sx.min(sy); // meet
        (s, s)
    };
    let (ax, ay) = align_frac(&align);
    let tx = -vb_x * sx + (size_w - vb_w * sx) * ax;
    let ty = -vb_y * sy + (size_h - vb_h * sy) * ay;
    Transform::from_row(sx as f32, 0.0, 0.0, sy as f32, tx as f32, ty as f32)
}

fn align_frac(align: &str) -> (f64, f64) {
    if align == "none" {
        return (0.0, 0.0);
    }
    let x = if align.contains("xMin") {
        0.0
    } else if align.contains("xMax") {
        1.0
    } else {
        0.5
    };
    let y = if align.contains("YMin") {
        0.0
    } else if align.contains("YMax") {
        1.0
    } else {
        0.5
    };
    (x, y)
}

fn parse_preserve_aspect_ratio(text: &str) -> (String, String) {
    let Some(tag) = svg_root_open_tag(text) else {
        return ("xMidYMid".into(), "meet".into());
    };
    let Some(raw) = svg_tag_attr(tag, "preserveAspectRatio") else {
        return ("xMidYMid".into(), "meet".into());
    };
    let parts: Vec<&str> = raw.split_whitespace().filter(|s| !s.is_empty()).collect();
    let mut align = "xMidYMid";
    let mut mos = "meet";
    for p in parts {
        let pl = p.to_ascii_lowercase();
        if pl == "defer" {
            continue;
        }
        if pl == "meet" || pl == "slice" {
            mos = if pl == "slice" { "slice" } else { "meet" };
        } else if pl == "none" {
            align = "none";
        } else {
            align = p; // keep original case for xMinYMin etc.
        }
    }
    (align.to_string(), mos.to_string())
}

fn parse_viewbox(text: &str) -> Option<(f64, f64, f64, f64)> {
    let tag = svg_root_open_tag(text)?;
    let vb = svg_tag_attr(tag, "viewBox")?;
    let parts = split_viewbox_nums(vb);
    (parts.len() == 4).then_some((parts[0], parts[1], parts[2], parts[3]))
}

#[cfg(test)]
mod tests {
    use super::{mm_per_unit, svg_tag_attr};

    #[test]
    fn unit_table_shared() {
        assert_eq!(mm_per_unit("mm"), 1.0);
        assert_eq!(mm_per_unit("cm"), 10.0);
        assert_eq!(mm_per_unit("in"), 25.4);
        assert!((mm_per_unit("pt") - 25.4 / 72.0).abs() < 1e-15);
        assert!((mm_per_unit("pc") - 25.4 / 6.0).abs() < 1e-15);
        assert!((mm_per_unit("") - 25.4 / 96.0).abs() < 1e-15);
        assert!((mm_per_unit("px") - 25.4 / 96.0).abs() < 1e-15);
    }

    #[test]
    fn tag_attr_allows_gt_inside_quotes() {
        let tag = r#"<svg width="100mm" data-note="a > b" viewBox="0 0 10 10""#;
        assert_eq!(svg_tag_attr(tag, "width"), Some("100mm"));
        assert_eq!(svg_tag_attr(tag, "viewBox"), Some("0 0 10 10"));
        assert_eq!(svg_tag_attr(tag, "data-note"), Some("a > b"));
    }
}
