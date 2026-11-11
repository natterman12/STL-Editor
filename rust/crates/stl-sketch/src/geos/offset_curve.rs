//! Port of GEOS 3.14.1 `OffsetCurve` as shapely 2.2 calls it for
//! `LineString.offset_curve(d, join_style="mitre")` (quadrant segments 16, as the `LineString.offset_curve` method defaults to; mitre limit 5, round
//! end caps for the helper buffer):
//!
//! - `OffsetCurveBuilder` / `OffsetSegmentGenerator` / `OffsetSegmentString` /
//!   `BufferInputLineSimplifier`: the raw offset curve of the line, and the raw (closed) curve of
//!   the line's buffer;
//! - the buffer polygon from that curve (noding, then the shells and holes that bound the area it
//!   winds around; see `polygon::outline_all_rings`);
//! - `OffsetCurve::computeSections` / `matchSegments` / `extractSections` and
//!   `OffsetCurveSection`: the parts of the buffer outline that lie on the raw curve.

use crate::entities::P2;
use crate::polygon::{self, orientation, point_in_ring, ring_area, segment_meets};
use std::f64::consts::PI;

const QUADRANT_SEGMENTS: f64 = 16.0;
const MITRE_LIMIT: f64 = 5.0;
const SIMPLIFY_FACTOR: f64 = 100.0;
const CURVE_VERTEX_SNAP_DISTANCE_FACTOR: f64 = 1.0e-4;
const OFFSET_SEGMENT_SEPARATION_FACTOR: f64 = 1.0e-3;
const INSIDE_TURN_VERTEX_SNAP_DISTANCE_FACTOR: f64 = 1.0e-3;
const MATCH_DISTANCE_FACTOR: f64 = 10000.0;
const NUM_PTS_TO_CHECK: usize = 10;
const NOT_IN_CURVE: f64 = -1.0;

fn dist(a: P2, b: P2) -> f64 {
    let (dx, dy) = (a[0] - b[0], a[1] - b[1]);
    (dx * dx + dy * dy).sqrt()
}

/// GEOS `Distance::pointToSegment`.
fn point_to_segment(p: P2, a: P2, b: P2) -> f64 {
    if a == b {
        return dist(p, a);
    }
    let len2 = (b[0] - a[0]) * (b[0] - a[0]) + (b[1] - a[1]) * (b[1] - a[1]);
    let r = ((p[0] - a[0]) * (b[0] - a[0]) + (p[1] - a[1]) * (b[1] - a[1])) / len2;
    if r <= 0.0 {
        return dist(p, a);
    }
    if r >= 1.0 {
        return dist(p, b);
    }
    let s = ((a[1] - p[1]) * (b[0] - a[0]) - (a[0] - p[0]) * (b[1] - a[1])) / len2;
    s.abs() * len2.sqrt()
}

/// GEOS `RepeatedPointRemover`: drops points equal to the one before.
fn remove_repeated(pts: &[P2]) -> Vec<P2> {
    let mut out: Vec<P2> = vec![];
    for &p in pts {
        if out.last() != Some(&p) {
            out.push(p);
        }
    }
    out
}

// ---------------------------------------------------------------- BufferInputLineSimplifier

/// GEOS `BufferInputLineSimplifier::simplify`: drops vertices of shallow concavities on one side
/// (`tol > 0`: left side, `tol < 0`: right side); the end vertices stay.
fn simplify(input: &[P2], tol: f64) -> Vec<P2> {
    let angle_orientation = if tol < 0.0 { -1 } else { 1 };
    let tol = tol.abs();
    let n = input.len();
    let mut deleted = vec![false; n];
    let next_kept = |deleted: &[bool], index: usize| {
        let mut next = index + 1;
        while next < n && deleted[next] {
            next += 1;
        }
        next
    };
    let is_deletable = |i0: usize, i1: usize, i2: usize| -> bool {
        let (p0, p1, p2) = (input[i0], input[i1], input[i2]);
        if orientation(p0, p1, p2) != angle_orientation {
            return false;
        }
        if point_to_segment(p1, p0, p2) >= tol {
            return false;
        }
        // isShallowSampled(p0, p1, i0, i2): every n'th point (GEOS tests the middle vertex against
        // the segment from p0 to that point)
        let mut inc = (i2 - i0) / NUM_PTS_TO_CHECK;
        if inc == 0 {
            inc = 1;
        }
        let mut i = i0;
        while i < i2 {
            if point_to_segment(p1, p0, input[i]) >= tol {
                return false;
            }
            i += inc;
        }
        true
    };
    loop {
        let mut changed = false;
        let mut index = 1;
        let mut mid = next_kept(&deleted, index);
        let mut last = next_kept(&deleted, mid);
        while last < n {
            let mut mid_deleted = false;
            if is_deletable(index, mid, last) {
                deleted[mid] = true;
                mid_deleted = true;
                changed = true;
            }
            index = if mid_deleted { last } else { mid };
            mid = next_kept(&deleted, index);
            last = next_kept(&deleted, mid);
        }
        if !changed {
            break;
        }
    }
    input.iter().zip(&deleted).filter(|&(_, &d)| !d).map(|(&p, _)| p).collect()
}

// ---------------------------------------------------------------- OffsetSegmentGenerator

type Seg = [P2; 2];

/// Offset of a segment to its left (`sign` 1) or right (-1) by `d`.
fn offset_segment(seg: Seg, sign: f64, d: f64) -> Seg {
    let dx = seg[1][0] - seg[0][0];
    let dy = seg[1][1] - seg[0][1];
    let len = (dx * dx + dy * dy).sqrt();
    let ux = sign * d * dx / len;
    let uy = sign * d * dy / len;
    [[seg[0][0] - uy, seg[0][1] + ux], [seg[1][0] - uy, seg[1][1] + ux]]
}

fn sin_cos_snap(a: f64) -> (f64, f64) {
    let (mut s, mut c) = (a.sin(), a.cos());
    if s.abs() < 5e-16 {
        s = 0.0;
    }
    if c.abs() < 5e-16 {
        c = 0.0;
    }
    (s, c)
}

fn angle_of(from: P2, to: P2) -> f64 {
    (to[1] - from[1]).atan2(to[0] - from[0])
}

fn normalize_angle(mut a: f64) -> f64 {
    while a > PI {
        a -= 2.0 * PI;
    }
    while a <= -PI {
        a += 2.0 * PI;
    }
    a
}

/// GEOS `Angle::angleBetweenOriented`.
fn angle_between_oriented(tip1: P2, tail: P2, tip2: P2) -> f64 {
    let del = angle_of(tail, tip2) - angle_of(tail, tip1);
    if del <= -PI {
        del + 2.0 * PI
    } else if del > PI {
        del - 2.0 * PI
    } else {
        del
    }
}

fn project(pt: P2, d: f64, dir: f64) -> P2 {
    let (s, c) = sin_cos_snap(dir);
    [pt[0] + d * c, pt[1] + d * s]
}

/// Intersection of the infinite lines p1-p2 and q1-q2 (None when parallel).
fn line_intersection(p1: P2, p2: P2, q1: P2, q2: P2) -> Option<P2> {
    let (d1, d2) = ([p2[0] - p1[0], p2[1] - p1[1]], [q2[0] - q1[0], q2[1] - q1[1]]);
    let den = d1[0] * d2[1] - d1[1] * d2[0];
    if den == 0.0 {
        return None;
    }
    let t = ((q1[0] - p1[0]) * d2[1] - (q1[1] - p1[1]) * d2[0]) / den;
    Some([p1[0] + t * d1[0], p1[1] + t * d1[1]])
}

/// GEOS `Intersection::intersectionLineSegment`: the line `l1-l2` against the segment `s1-s2`.
fn intersection_line_segment(l1: P2, l2: P2, s1: P2, s2: P2) -> Option<P2> {
    let o1 = orientation(l1, l2, s1);
    if o1 == 0 {
        return Some(s1);
    }
    let o2 = orientation(l1, l2, s2);
    if o2 == 0 {
        return Some(s2);
    }
    if (o1 > 0 && o2 > 0) || (o1 < 0 && o2 < 0) {
        return None;
    }
    if let Some(p) = line_intersection(l1, l2, s1, s2) {
        return Some(p);
    }
    let perp = |p: P2| {
        let (dx, dy) = (l2[0] - l1[0], l2[1] - l1[1]);
        (((l1[1] - p[1]) * dx - (l1[0] - p[0]) * dy) / (dx * dx + dy * dy).sqrt()).abs()
    };
    Some(if perp(s1) < perp(s2) { s1 } else { s2 })
}

/// The mitre-join `OffsetSegmentGenerator` (offsets to the left only, as every curve here is).
struct SegGen {
    dist: f64,
    quantum: f64,
    pts: Vec<P2>,
    min_vertex_dist: f64,
    s0: P2,
    s1: P2,
    s2: P2,
    seg0: Seg,
    seg1: Seg,
    off0: Seg,
    off1: Seg,
}

impl SegGen {
    fn new(d: f64) -> SegGen {
        SegGen {
            dist: d,
            quantum: (PI / 2.0) / QUADRANT_SEGMENTS,
            pts: vec![],
            min_vertex_dist: d * CURVE_VERTEX_SNAP_DISTANCE_FACTOR,
            s0: [0.0; 2],
            s1: [0.0; 2],
            s2: [0.0; 2],
            seg0: [[0.0; 2]; 2],
            seg1: [[0.0; 2]; 2],
            off0: [[0.0; 2]; 2],
            off1: [[0.0; 2]; 2],
        }
    }

    fn add_pt(&mut self, p: P2) {
        if let Some(&l) = self.pts.last() {
            if dist(p, l) < self.min_vertex_dist {
                return;
            }
        }
        self.pts.push(p);
    }

    fn close_ring(&mut self) {
        if let (Some(&f), Some(&l)) = (self.pts.first(), self.pts.last()) {
            if f != l {
                self.pts.push(f);
            }
        }
    }

    fn init_side_segments(&mut self, s1: P2, s2: P2) {
        self.s1 = s1;
        self.s2 = s2;
        self.seg1 = [s1, s2];
        self.off1 = offset_segment(self.seg1, 1.0, self.dist);
    }

    fn add_first_segment(&mut self) {
        let p = self.off1[0];
        self.add_pt(p);
    }

    fn add_last_segment(&mut self) {
        let p = self.off1[1];
        self.add_pt(p);
    }

    fn add_next_segment(&mut self, p: P2, add_start_point: bool) {
        if self.s2 == p {
            return;
        }
        self.s0 = self.s1;
        self.s1 = self.s2;
        self.s2 = p;
        self.seg0 = [self.s0, self.s1];
        self.off0 = offset_segment(self.seg0, 1.0, self.dist);
        self.seg1 = [self.s1, self.s2];
        self.off1 = offset_segment(self.seg1, 1.0, self.dist);
        let turn = orientation(self.s0, self.s1, self.s2);
        if turn == 0 {
            // collinear: only a reversal needs anything (the end cap all the way round, which
            // for mitre joins is the two offset points)
            let d0 = [self.s1[0] - self.s0[0], self.s1[1] - self.s0[1]];
            let d1 = [self.s2[0] - self.s1[0], self.s2[1] - self.s1[1]];
            if d0[0] * d1[0] + d0[1] * d1[1] < 0.0 {
                if add_start_point {
                    let p = self.off0[1];
                    self.add_pt(p);
                }
                let p = self.off1[0];
                self.add_pt(p);
            }
        } else if turn == -1 {
            self.add_outside_turn();
        } else {
            self.add_inside_turn();
        }
    }

    fn add_outside_turn(&mut self) {
        if dist(self.off0[1], self.off1[0]) < self.dist * OFFSET_SEGMENT_SEPARATION_FACTOR {
            let p = self.off0[1];
            self.add_pt(p);
            return;
        }
        // mitre join
        let limit = MITRE_LIMIT * self.dist;
        if let Some(ip) = line_intersection(self.off0[0], self.off0[1], self.off1[0], self.off1[1]) {
            if dist(ip, self.s1) <= limit {
                self.add_pt(ip);
                return;
            }
        }
        let bevel_dist = point_to_segment(self.s1, self.off0[1], self.off1[0]);
        if bevel_dist >= limit {
            self.add_bevel();
            return;
        }
        // a limited mitre bevel
        let corner = self.seg0[1];
        let ang_interior = angle_between_oriented(self.seg0[0], corner, self.seg1[1]);
        let dir0 = angle_of(corner, self.seg0[0]);
        let dir_bisector = normalize_angle(dir0 + ang_interior / 2.0);
        let dir_out = normalize_angle(dir_bisector + PI);
        let mid = project(corner, limit, dir_out);
        let dir_bevel = normalize_angle(dir_out + PI / 2.0);
        let bevel0 = project(mid, self.dist, dir_bevel);
        let bevel1 = project(mid, self.dist, dir_bevel + PI);
        let i0 = intersection_line_segment(self.off0[0], self.off0[1], bevel0, bevel1);
        let i1 = intersection_line_segment(self.off1[0], self.off1[1], bevel0, bevel1);
        if let (Some(a), Some(b)) = (i0, i1) {
            self.add_pt(a);
            self.add_pt(b);
            return;
        }
        self.add_bevel();
    }

    fn add_bevel(&mut self) {
        let (a, b) = (self.off0[1], self.off1[0]);
        self.add_pt(a);
        self.add_pt(b);
    }

    fn add_inside_turn(&mut self) {
        if let Some(&p) = segment_meets(self.off0[0], self.off0[1], self.off1[0], self.off1[1]).first() {
            self.add_pt(p);
            return;
        }
        let (o0, o1, s1) = (self.off0[1], self.off1[0], self.s1);
        if dist(o0, o1) < self.dist * INSIDE_TURN_VERTEX_SNAP_DISTANCE_FACTOR {
            self.add_pt(o0);
        } else {
            self.add_pt(o0);
            // the closing segment (closingSegLengthFactor 1 for mitre joins)
            self.add_pt([(o0[0] + s1[0]) / 2.0, (o0[1] + s1[1]) / 2.0]);
            self.add_pt([(o1[0] + s1[0]) / 2.0, (o1[1] + s1[1]) / 2.0]);
            self.add_pt(o1);
        }
    }

    /// Round end cap around `p1`, ending a segment that comes from `p0`.
    fn add_line_end_cap(&mut self, p0: P2, p1: P2) {
        let seg = [p0, p1];
        let off_l = offset_segment(seg, 1.0, self.dist);
        let off_r = offset_segment(seg, -1.0, self.dist);
        let angle = (p1[1] - p0[1]).atan2(p1[0] - p0[0]);
        self.add_pt(off_l[1]);
        self.add_directed_fillet_cw(p1, angle + PI / 2.0, angle - PI / 2.0, self.dist);
        self.add_pt(off_r[1]);
    }

    fn add_directed_fillet_cw(&mut self, p: P2, start: f64, end: f64, radius: f64) {
        let total = (start - end).abs();
        let n = (total / self.quantum + 0.5) as i64;
        if n < 1 {
            return;
        }
        let inc = total / n as f64;
        for i in 0..n {
            let (s, c) = sin_cos_snap(start + -1.0 * i as f64 * inc);
            self.add_pt([p[0] + radius * c, p[1] + radius * s]);
        }
    }

    /// `segList.getCoordinates()`: the ring is closed first.
    fn finish(mut self) -> Vec<P2> {
        self.close_ring();
        self.pts
    }
}

/// `OffsetCurveBuilder::getOffsetCurve`: the raw offset of an open line (left for `d > 0`).
fn raw_offset_curve(input: &[P2], d: f64) -> Vec<P2> {
    if d == 0.0 {
        return vec![];
    }
    let right = d < 0.0;
    let pos = d.abs();
    let mut g = SegGen::new(pos);
    let tol = pos / SIMPLIFY_FACTOR;
    if right {
        let s = simplify(input, -tol);
        let n2 = s.len() - 1;
        g.init_side_segments(s[n2], s[n2 - 1]);
        g.add_first_segment();
        for i in (1..n2).rev() {
            g.add_next_segment(s[i - 1], true);
        }
    } else {
        let s = simplify(input, tol);
        let n1 = s.len() - 1;
        g.init_side_segments(s[0], s[1]);
        g.add_first_segment();
        for i in 2..=n1 {
            g.add_next_segment(s[i], true);
        }
    }
    g.add_last_segment();
    let mut pts = g.finish();
    if right {
        pts.reverse();
    }
    pts
}

/// `OffsetCurveBuilder::computeLineBufferCurve`: the closed raw curve of the buffer of an open
/// line, round end caps.
fn line_buffer_curve(input: &[P2], d: f64) -> Vec<P2> {
    let mut g = SegGen::new(d);
    let tol = d / SIMPLIFY_FACTOR;
    let s1 = simplify(input, tol);
    let n1 = s1.len() - 1;
    g.init_side_segments(s1[0], s1[1]);
    for i in 2..=n1 {
        g.add_next_segment(s1[i], true);
    }
    g.add_last_segment();
    g.add_line_end_cap(s1[n1 - 1], s1[n1]);
    let s2 = simplify(input, -tol);
    let n2 = s2.len() - 1;
    g.init_side_segments(s2[n2], s2[n2 - 1]);
    for i in (1..n2).rev() {
        g.add_next_segment(s2[i - 1], true);
    }
    g.add_last_segment();
    g.add_line_end_cap(s2[1], s2[0]);
    g.finish()
}

// ---------------------------------------------------------------- the buffer polygon

/// The buffer polygon of an open line (shell, then holes) as GEOS `BufferOp` gives it, the one
/// with the largest area when there are several; rings are closed (first point repeated).
fn buffer_polygon(line: &[P2], d: f64) -> Vec<Vec<P2>> {
    let mut curve = line_buffer_curve(line, d);
    if curve.len() > 1 && curve.first() == curve.last() {
        curve.pop();
    }
    let rings = polygon::outline_all_rings(&curve, false);
    let shells: Vec<&Vec<P2>> = rings.iter().filter(|r| ring_area(r) < 0.0).collect();
    let holes: Vec<&Vec<P2>> = rings.iter().filter(|r| ring_area(r) > 0.0).collect();
    // each hole belongs to the smallest shell around it
    let mut owner: Vec<Option<usize>> = vec![None; holes.len()];
    for (hi, h) in holes.iter().enumerate() {
        let mut best: Option<usize> = None;
        for (si, s) in shells.iter().enumerate() {
            let inside = h.iter().map(|&p| point_in_ring(s, p)).find(|&r| r != 0) == Some(1);
            if inside && best.is_none_or(|b| ring_area(s).abs() < ring_area(shells[b]).abs()) {
                best = Some(si);
            }
        }
        owner[hi] = best;
    }
    let mut best: Option<(f64, usize)> = None;
    for (si, s) in shells.iter().enumerate() {
        let area = ring_area(s).abs()
            - holes.iter().enumerate().filter(|(hi, _)| owner[*hi] == Some(si)).map(|(_, h)| ring_area(h).abs()).sum::<f64>();
        if best.is_none_or(|(a, _)| area > a) {
            best = Some((area, si));
        }
    }
    let Some((_, si)) = best else { return vec![] };
    let close = |r: &Vec<P2>| {
        let mut c = r.clone();
        c.push(r[0]);
        c
    };
    let mut out = vec![close(shells[si])];
    for (hi, h) in holes.iter().enumerate() {
        if owner[hi] == Some(si) {
            out.push(close(h));
        }
    }
    out
}

// ---------------------------------------------------------------- OffsetCurve sections

struct Section {
    pts: Vec<P2>,
    loc: f64,
}

fn next_index(i: usize, size: usize) -> usize {
    if i + 1 >= size { 0 } else { i + 1 }
}

fn prev_index(i: usize, size: usize) -> usize {
    if i == 0 { size - 1 } else { i - 1 }
}

/// `LineSegment::segmentFraction` of `p` on the segment `a-b`.
fn segment_fraction(a: P2, b: P2, p: P2) -> f64 {
    let f = if p == a {
        0.0
    } else if p == b {
        1.0
    } else if a == b {
        0.0
    } else {
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / (dx * dx + dy * dy)
    };
    f.clamp(0.0, 1.0)
}

fn compute_curve_sections(ring: &[P2], raw: &[P2], match_distance: f64, sections: &mut Vec<Section>) {
    let m = ring.len() - 1;
    let mut raw_pos = vec![NOT_IN_CURVE; m];
    let mut first_index: Option<usize> = None;
    let mut min_raw_pos = -1.0;
    for i in 0..raw.len() - 1 {
        let (r0, r1) = (raw[i], raw[i + 1]);
        let (lo_x, hi_x) = (r0[0].min(r1[0]) - match_distance, r0[0].max(r1[0]) + match_distance);
        let (lo_y, hi_y) = (r0[1].min(r1[1]) - match_distance, r0[1].max(r1[1]) + match_distance);
        let mut seg_min: Option<(usize, f64)> = None;
        for j in 0..m {
            let (b0, b1) = (ring[j], ring[j + 1]);
            if b0[0].min(b1[0]) > hi_x || b0[0].max(b1[0]) < lo_x || b0[1].min(b1[1]) > hi_y || b0[1].max(b1[1]) < lo_y {
                continue;
            }
            if match_distance < point_to_segment(b0, r0, r1) || match_distance < point_to_segment(b1, r0, r1) {
                continue;
            }
            let location = i as f64 + segment_fraction(r0, r1, b0);
            raw_pos[j] = location;
            if seg_min.is_none_or(|(_, l)| location < l) {
                seg_min = Some((j, location));
            }
        }
        if let Some((j, _)) = seg_min {
            let pos = raw_pos[j];
            if first_index.is_none() || pos < min_raw_pos {
                min_raw_pos = pos;
                first_index = Some(j);
            }
        }
    }
    let Some(start) = first_index else { return };
    extract_sections(ring, &raw_pos, start, sections);
}

fn find_section_start(loc: &[f64], end: usize) -> usize {
    let mut start = end;
    loop {
        let next = next_index(start, loc.len());
        if loc[start] == NOT_IN_CURVE {
            start = next;
        } else {
            let prev = prev_index(start, loc.len());
            if loc[prev] == NOT_IN_CURVE {
                return start;
            }
            start = next;
        }
        if start == end {
            return start;
        }
    }
}

fn find_section_end(loc: &[f64], start: usize, first_start: usize) -> usize {
    let mut end = start;
    loop {
        let next = next_index(end, loc.len());
        if loc[next] == NOT_IN_CURVE {
            return next;
        }
        end = next;
        if end == start || end == first_start {
            return end;
        }
    }
}

fn extract_sections(ring: &[P2], loc: &[f64], start_index: usize, sections: &mut Vec<Section>) {
    let mut section_start = start_index;
    let mut count = 0usize;
    loop {
        let section_end = find_section_end(loc, section_start, start_index);
        let location = loc[section_start];
        // OffsetCurveSection::create
        let len = if section_end <= section_start { ring.len() - section_start + section_end } else { section_end - section_start + 1 };
        let pts: Vec<P2> = (0..len).map(|i| ring[(section_start + i) % (ring.len() - 1)]).collect();
        sections.push(Section { pts, loc: location });
        section_start = find_section_start(loc, section_end);
        count += 1;
        if count > ring.len() {
            // GEOS asserts "Too many sections for ring - probable bug"
            return;
        }
        if section_start == start_index || section_end == start_index {
            break;
        }
    }
}

// ---------------------------------------------------------------- OffsetCurve

/// `OffsetCurve::computeCurve` for a line string: the pieces of the offset curve, in order along
/// it (an empty list for an empty result).
pub fn offset_curve(line: &[P2], d: f64) -> Vec<Vec<P2>> {
    let length: f64 = line.windows(2).map(|w| dist(w[0], w[1])).sum();
    if line.len() < 2 || length == 0.0 {
        return vec![];
    }
    if d == 0.0 {
        return vec![line.to_vec()];
    }
    if line.len() == 2 {
        let seg = offset_segment([line[0], line[1]], 1.0, d);
        return vec![vec![seg[0], seg[1]]];
    }
    let clean = remove_repeated(line);
    if clean.len() < 2 {
        return vec![];
    }
    let raw = raw_offset_curve(&clean, d);
    if raw.is_empty() {
        return vec![];
    }
    let match_distance = d.abs() / MATCH_DISTANCE_FACTOR;
    let mut rings = buffer_polygon(&clean, d.abs());
    if rings.is_empty() {
        return vec![];
    }
    if d < 0.0 {
        for r in rings.iter_mut() {
            r.reverse();
        }
    }
    let mut sections: Vec<Section> = vec![];
    for r in &rings {
        compute_curve_sections(r, &raw, match_distance, &mut sections);
    }
    if sections.len() > 1 {
        sections.sort_by(|a, b| a.loc.total_cmp(&b.loc));
    }
    sections.into_iter().map(|s| s.pts).collect()
}
