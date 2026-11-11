//! The shapely operations 1.x `sketch.py` uses: noding a set of line strings
//! (`unary_union`), `polygonize`, point in polygon, line string intersection and distance,
//! `linemerge`, and mitre offsets (`buffer` / `offset_curve` with `join_style="mitre"`).
//!
//! Plain `f64` geometry, no dependencies. Points closer than `NODE_TOL` count as the same node.

use crate::entities::P2;
use std::collections::HashMap;

const NODE_TOL: f64 = 1e-9;
const END_EPS: f64 = 1e-12;
/// shapely's default mitre limit.
const MITRE_LIMIT: f64 = 5.0;

/// A polygon the way polygonize returns it: an outline and the holes (islands) inside it.
#[derive(Debug, Clone, PartialEq)]
pub struct Face {
    /// Clockwise like GEOS, not closed (the last point is not repeated).
    pub exterior: Vec<P2>,
    /// Counter-clockwise like GEOS, not closed.
    pub holes: Vec<Vec<P2>>,
}

impl Face {
    pub fn area(&self) -> f64 {
        ring_area(&self.exterior).abs() - self.holes.iter().map(|h| ring_area(h).abs()).sum::<f64>()
    }

    /// shapely `contains(Point)`: strictly inside the outline and not inside or on a hole.
    pub fn contains(&self, p: P2) -> bool {
        point_in_ring(&self.exterior, p) == 1 && self.holes.iter().all(|h| point_in_ring(h, p) == -1)
    }
}

/// Signed area of a ring (positive when counter-clockwise).
pub fn ring_area(r: &[P2]) -> f64 {
    let n = r.len();
    let mut s = 0.0;
    for i in 0..n {
        let (a, b) = (r[i], r[(i + 1) % n]);
        s += a[0] * b[1] - b[0] * a[1];
    }
    s / 2.0
}

fn on_segment(a: P2, b: P2, p: P2) -> bool {
    let cross = (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]);
    let len = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
    if cross.abs() > 1e-12 * len.max(1.0) {
        return false;
    }
    let (lx, hx) = (a[0].min(b[0]) - 1e-12, a[0].max(b[0]) + 1e-12);
    let (ly, hy) = (a[1].min(b[1]) - 1e-12, a[1].max(b[1]) + 1e-12);
    p[0] >= lx && p[0] <= hx && p[1] >= ly && p[1] <= hy
}

/// 1 inside, -1 outside, 0 on the boundary.
pub fn point_in_ring(r: &[P2], p: P2) -> i32 {
    let n = r.len();
    if n < 3 {
        return -1;
    }
    let mut inside = false;
    for i in 0..n {
        let (a, b) = (r[i], r[(i + 1) % n]);
        if on_segment(a, b, p) {
            return 0;
        }
        if (a[1] > p[1]) != (b[1] > p[1]) {
            let x = a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
            if p[0] < x {
                inside = !inside;
            }
        }
    }
    if inside { 1 } else { -1 }
}

/// Shortest distance from a point to a path (shapely `LineString.distance(Point)`).
pub fn distance_to_path(path: &[P2], p: P2) -> f64 {
    let mut best = f64::INFINITY;
    for w in path.windows(2) {
        best = best.min(point_segment_distance(w[0], w[1], p));
    }
    if path.len() == 1 {
        best = ((path[0][0] - p[0]).powi(2) + (path[0][1] - p[1]).powi(2)).sqrt();
    }
    best
}

pub(crate) fn point_segment_distance(a: P2, b: P2, p: P2) -> f64 {
    let d = [b[0] - a[0], b[1] - a[1]];
    let l2 = d[0] * d[0] + d[1] * d[1];
    let t = if l2 == 0.0 { 0.0 } else { (((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / l2).clamp(0.0, 1.0) };
    let q = [a[0] + t * d[0], a[1] + t * d[1]];
    ((q[0] - p[0]).powi(2) + (q[1] - p[1]).powi(2)).sqrt()
}

/// Where two segments meet: one point for a crossing or a touch, the two ends of the shared
/// piece when they overlap along a line.
pub(crate) fn segment_meets(p: P2, q: P2, r: P2, s: P2) -> Vec<P2> {
    // GEOS `LineIntersector`: segments that share an end point exactly and are not exactly in
    // line meet at that end point, whatever the (ill-conditioned) crossing arithmetic says.
    let exact = |a: P2, b: P2| a[0] == b[0] && a[1] == b[1];
    if exact(p, r) || exact(p, s) || exact(q, r) || exact(q, s) {
        let in_line = orientation(p, q, r) == 0 && orientation(p, q, s) == 0;
        if !in_line {
            return vec![if exact(p, r) || exact(p, s) { p } else { q }];
        }
    }
    let d1 = [q[0] - p[0], q[1] - p[1]];
    let d2 = [s[0] - r[0], s[1] - r[1]];
    let den = d1[0] * d2[1] - d1[1] * d2[0];
    let rp = [r[0] - p[0], r[1] - p[1]];
    let scale = (d1[0].hypot(d1[1])) * (d2[0].hypot(d2[1]));
    if den.abs() <= 1e-14 * scale.max(1e-300) {
        // parallel: only a shared piece counts
        let cr = rp[0] * d1[1] - rp[1] * d1[0];
        if cr.abs() > 1e-12 * d1[0].hypot(d1[1]).max(1.0) {
            return vec![];
        }
        let mut out: Vec<P2> = vec![];
        for (pt, a, b) in [(p, r, s), (q, r, s), (r, p, q), (s, p, q)] {
            if on_segment(a, b, pt) && !out.iter().any(|o| same(*o, pt)) {
                out.push(pt);
            }
        }
        return out;
    }
    let t = (rp[0] * d2[1] - rp[1] * d2[0]) / den;
    let u = (rp[0] * d1[1] - rp[1] * d1[0]) / den;
    if t < -END_EPS || t > 1.0 + END_EPS || u < -END_EPS || u > 1.0 + END_EPS {
        return vec![];
    }
    // an end of either segment is the meeting point exactly, so the graph closes
    if t.abs() <= END_EPS {
        return vec![p];
    }
    if (t - 1.0).abs() <= END_EPS {
        return vec![q];
    }
    if u.abs() <= END_EPS {
        return vec![r];
    }
    if (u - 1.0).abs() <= END_EPS {
        return vec![s];
    }
    vec![[p[0] + t * d1[0], p[1] + t * d1[1]]]
}

fn same(a: P2, b: P2) -> bool {
    (a[0] - b[0]).abs() <= NODE_TOL && (a[1] - b[1]).abs() <= NODE_TOL
}

/// shapely `a.intersection(b)` for two line strings, as the list of meeting points (the ends of
/// a shared piece included). Empty when they do not meet.
pub fn intersection_points(a: &[P2], b: &[P2]) -> Vec<P2> {
    let mut out: Vec<P2> = vec![];
    for wa in a.windows(2) {
        for wb in b.windows(2) {
            for p in segment_meets(wa[0], wa[1], wb[0], wb[1]) {
                if !out.iter().any(|o| same(*o, p)) {
                    out.push(p);
                }
            }
        }
    }
    out
}

/// Gives each distinct point (within `NODE_TOL`) one index.
#[derive(Default)]
struct NodeMap {
    pts: Vec<P2>,
    index: HashMap<(i64, i64), usize>,
}

impl NodeMap {
    fn key(p: P2) -> (i64, i64) {
        ((p[0] / NODE_TOL).round() as i64, (p[1] / NODE_TOL).round() as i64)
    }
    fn id(&mut self, p: P2) -> usize {
        let (kx, ky) = Self::key(p);
        for dx in -1..=1 {
            for dy in -1..=1 {
                if let Some(&i) = self.index.get(&(kx + dx, ky + dy)) {
                    if same(self.pts[i], p) {
                        return i;
                    }
                }
            }
        }
        let i = self.pts.len();
        self.pts.push(p);
        self.index.insert((kx, ky), i);
        i
    }
}

/// Pairs of segments (`i < j`, sorted) whose bounding boxes touch or overlap (padded by
/// `NODE_TOL`), found by sweeping along x. Only these can meet, and they come in the order a
/// plain double loop would visit them, so noding gives the same result.
fn candidate_pairs(segs: &[[P2; 2]]) -> Vec<(usize, usize)> {
    let bb: Vec<[f64; 4]> = segs
        .iter()
        .map(|s| {
            [
                s[0][0].min(s[1][0]) - NODE_TOL,
                s[0][1].min(s[1][1]) - NODE_TOL,
                s[0][0].max(s[1][0]) + NODE_TOL,
                s[0][1].max(s[1][1]) + NODE_TOL,
            ]
        })
        .collect();
    let mut order: Vec<usize> = (0..segs.len()).collect();
    order.sort_by(|&a, &b| bb[a][0].total_cmp(&bb[b][0]));
    let mut pairs = vec![];
    for (k, &i) in order.iter().enumerate() {
        for &j in &order[k + 1..] {
            if bb[j][0] > bb[i][2] {
                break;
            }
            if bb[j][1] <= bb[i][3] && bb[i][1] <= bb[j][3] {
                pairs.push((i.min(j), i.max(j)));
            }
        }
    }
    pairs.sort_unstable();
    pairs
}

/// shapely `unary_union` of line strings: every crossing and touch becomes a vertex of the
/// pieces, and pieces that lie on top of each other appear once. Returns the pieces the way
/// GEOS does: one line string per input line between the points where something else meets it
/// (a ring is broken at its own start vertex), in the order they are first met.
pub fn node_lines(lines: &[Vec<P2>]) -> Vec<Vec<P2>> {
    // (segment, index of its input line, is the previous segment of that line)
    let mut segs: Vec<[P2; 2]> = vec![];
    let mut line_of: Vec<usize> = vec![];
    for (li, l) in lines.iter().enumerate() {
        for w in l.windows(2) {
            if !same(w[0], w[1]) {
                segs.push([w[0], w[1]]);
                line_of.push(li);
            }
        }
    }
    let n = segs.len();
    let mut splits: Vec<Vec<P2>> = vec![vec![]; n];
    for (i, j) in candidate_pairs(&segs) {
        let mut pts = segment_meets(segs[i][0], segs[i][1], segs[j][0], segs[j][1]);
        // consecutive segments of one line meet at their shared vertex, which is no node
        if line_of[i] == line_of[j] && j == i + 1 {
            pts.retain(|&p| !same(p, segs[i][1]));
        }
        for p in pts {
            splits[i].push(p);
            splits[j].push(p);
        }
    }
    let mut nodes = NodeMap::default();
    let mut node_ids: std::collections::HashSet<usize> = Default::default();
    for sp in &splits {
        for &p in sp {
            node_ids.insert(nodes.id(p));
        }
    }
    let mut seen: std::collections::HashSet<(usize, usize)> = Default::default();
    let mut out: Vec<Vec<P2>> = vec![];
    let mut chain: Vec<P2> = vec![];
    let mut chain_line = usize::MAX;
    for (i, s) in segs.iter().enumerate() {
        if line_of[i] != chain_line && !chain.is_empty() {
            out.push(std::mem::take(&mut chain));
        }
        chain_line = line_of[i];
        let a = s[0];
        let dir = [s[1][0] - a[0], s[1][1] - a[1]];
        let l2 = dir[0] * dir[0] + dir[1] * dir[1];
        let mut cuts: Vec<(f64, P2)> = splits[i]
            .iter()
            .map(|&p| ((((p[0] - a[0]) * dir[0] + (p[1] - a[1]) * dir[1]) / l2), p))
            .collect();
        cuts.push((0.0, s[0]));
        cuts.push((1.0, s[1]));
        cuts.sort_by(|x, y| x.0.total_cmp(&y.0));
        for w in cuts.windows(2) {
            let (u, v) = (nodes.id(w[0].1), nodes.id(w[1].1));
            if u == v {
                continue;
            }
            let key = (u.min(v), u.max(v));
            if !seen.insert(key) {
                // a piece already met: it is shared, so the line breaks here
                if !chain.is_empty() {
                    out.push(std::mem::take(&mut chain));
                }
                continue;
            }
            let continues = !chain.is_empty()
                && nodes.id(*chain.last().unwrap()) == u
                && !node_ids.contains(&u);
            if !continues {
                if !chain.is_empty() {
                    out.push(std::mem::take(&mut chain));
                }
                chain.push(nodes.pts[u]);
            }
            chain.push(nodes.pts[v]);
        }
    }
    if !chain.is_empty() {
        out.push(chain);
    }
    out
}

/// The two-point pieces of noded line strings.
pub fn chain_segments(chains: &[Vec<P2>]) -> Vec<[P2; 2]> {
    chains.iter().flat_map(|c| c.windows(2).map(|w| [w[0], w[1]])).collect()
}

/// Planar graph of noded segments.
struct Graph {
    pts: Vec<P2>,
    /// Undirected edges (node ids), `alive` marks the ones still in the graph.
    edges: Vec<(usize, usize)>,
    /// The points of each edge from its first to its last node.
    paths: Vec<Vec<P2>>,
    alive: Vec<bool>,
}

impl Graph {
    /// Edges that are whole line strings (several points), as GEOS keeps them.
    fn from_chains(chains: &[Vec<P2>]) -> Graph {
        let mut nodes = NodeMap::default();
        let mut edges = vec![];
        let mut paths = vec![];
        for c in chains {
            if c.len() < 2 {
                continue;
            }
            let (a, b) = (nodes.id(c[0]), nodes.id(c[c.len() - 1]));
            if a == b && c.len() < 4 {
                continue;
            }
            edges.push((a, b));
            paths.push(c.clone());
        }
        let alive = vec![true; edges.len()];
        Graph { pts: nodes.pts, edges, paths, alive }
    }

    fn prune_dangles(&mut self) {
        loop {
            let mut deg = vec![0usize; self.pts.len()];
            for (i, &(a, b)) in self.edges.iter().enumerate() {
                if self.alive[i] {
                    deg[a] += 1;
                    deg[b] += 1;
                }
            }
            let mut changed = false;
            for (i, &(a, b)) in self.edges.iter().enumerate() {
                if self.alive[i] && (deg[a] <= 1 || deg[b] <= 1) {
                    self.alive[i] = false;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
    }

    /// Face boundaries as rings of directed edges (the face is on the right of each step), in
    /// GEOS's order: edges in the order given, forward then reverse. Directed edge
    /// `2*i` goes `a -> b`, `2*i + 1` goes `b -> a`.
    fn trace(&self) -> Vec<Vec<usize>> {
        let n = self.pts.len();
        // outgoing directed edges per node, sorted counter-clockwise by angle
        let mut out: Vec<Vec<usize>> = vec![vec![]; n];
        for (i, &(a, b)) in self.edges.iter().enumerate() {
            if self.alive[i] {
                out[a].push(2 * i);
                out[b].push(2 * i + 1);
            }
        }
        let head = |d: usize| if d % 2 == 0 { self.edges[d / 2].1 } else { self.edges[d / 2].0 };
        
        // direction of the first step of a directed edge
        let angle = |d: usize| {
            let p = &self.paths[d / 2];
            let (t, h) = if d % 2 == 0 { (p[0], p[1]) } else { (p[p.len() - 1], p[p.len() - 2]) };
            (h[1] - t[1]).atan2(h[0] - t[0])
        };
        for o in out.iter_mut() {
            o.sort_by(|&x, &y| angle(x).total_cmp(&angle(y)));
        }
        let mut done = vec![false; self.edges.len() * 2];
        let mut rings = vec![];
        for start in 0..self.edges.len() * 2 {
            if !self.alive[start / 2] || done[start] {
                continue;
            }
            let mut ring = vec![];
            let mut d = start;
            loop {
                done[d] = true;
                ring.push(d);
                let v = head(d);
                let twin = d ^ 1;
                let list = &out[v];
                let k = list.iter().position(|&x| x == twin).unwrap();
                // the face on the right of each step, so bounded faces run clockwise like GEOS
                d = list[(k + 1) % list.len()];
                if d == start {
                    break;
                }
            }
            rings.push(ring);
        }
        rings
    }
}

/// shapely `polygonize` of noded segments: every bounded region, as an outline with the
/// islands inside it as holes. Dangling pieces and pieces with the same region on both sides
/// are dropped.
pub fn polygonize(chains: &[Vec<P2>]) -> Vec<Face> {
    let mut g = Graph::from_chains(chains);
    let rings = loop {
        g.prune_dangles();
        let rings = g.trace();
        let mut face_of = vec![usize::MAX; g.edges.len() * 2];
        for (k, r) in rings.iter().enumerate() {
            for &d in r {
                face_of[d] = k;
            }
        }
        let mut cut = false;
        for i in 0..g.edges.len() {
            if g.alive[i] && face_of[2 * i] == face_of[2 * i + 1] {
                g.alive[i] = false;
                cut = true;
            }
        }
        if !cut {
            break rings;
        }
    };
    let ring_pts = |r: &Vec<usize>| -> Vec<P2> {
        let mut out = vec![];
        for &d in r {
            let p = &g.paths[d / 2];
            if d % 2 == 0 {
                out.extend_from_slice(&p[..p.len() - 1]);
            } else {
                out.extend(p[1..].iter().rev().copied());
            }
        }
        out
    };
    let mut shells: Vec<Vec<P2>> = vec![];
    let mut outers: Vec<Vec<P2>> = vec![];
    for r in &rings {
        let pts = ring_pts(r);
        let a = ring_area(&pts);
        if a < 0.0 {
            shells.push(pts);
        } else if a > 0.0 {
            outers.push(pts);
        }
    }
    let mut holes: Vec<Vec<Vec<P2>>> = vec![vec![]; shells.len()];
    for o in outers {
        let mut best: Option<(usize, f64)> = None;
        for (k, s) in shells.iter().enumerate() {
            let inside = o.iter().map(|&p| point_in_ring(s, p)).find(|&c| c != 0) == Some(1);
            if inside {
                let area = ring_area(s).abs();
                if best.is_none_or(|(_, ba)| area < ba) {
                    best = Some((k, area));
                }
            }
        }
        if let Some((k, _)) = best {
            holes[k].push(o);
        }
    }
    shells.into_iter().zip(holes).map(|(exterior, holes)| Face { exterior, holes }).collect()
}

/// Direction of the first step of a path, as an angle in `[0, 2π)` (GEOS orders the edges at a
/// node this way).
fn start_angle(from: P2, to: P2) -> f64 {
    let a = (to[1] - from[1]).atan2(to[0] - from[0]) + 0.0;
    if a < 0.0 { a + 2.0 * std::f64::consts::PI } else { a }
}

/// shapely `linemerge` (GEOS `LineMerger`): joins pieces that meet end to end, at nodes where
/// exactly two pieces meet, into longer line strings. Every piece keeps its own direction; a
/// merged line runs the way most of its pieces point (the other way when more of them point
/// backwards). A closed result repeats its first point at the end.
///
/// Like GEOS, lines start at the lowest (x, then y) node that does not have exactly two pieces,
/// taking its pieces in counter-clockwise order from the +x axis; loops with only two-piece
/// nodes are done last the same way.
pub fn linemerge(chains: &[Vec<P2>]) -> Vec<Vec<P2>> {
    let mut nodes = NodeMap::default();
    let mut edges: Vec<(usize, usize)> = vec![];
    let mut paths: Vec<Vec<P2>> = vec![];
    for c in chains {
        let mut p: Vec<P2> = vec![];
        for &q in c {
            if p.last().is_none_or(|&l| !same(l, q)) {
                p.push(q);
            }
        }
        if p.len() < 2 {
            continue;
        }
        let (a, b) = (nodes.id(p[0]), nodes.id(p[p.len() - 1]));
        edges.push((a, b));
        paths.push(p);
    }
    let n = nodes.pts.len();
    // directed edge 2*i runs a -> b along the piece, 2*i + 1 runs b -> a
    let mut out: Vec<Vec<usize>> = vec![vec![]; n];
    for (i, &(a, b)) in edges.iter().enumerate() {
        out[a].push(2 * i);
        out[b].push(2 * i + 1);
    }
    let angle = |d: usize| {
        let p = &paths[d / 2];
        if d % 2 == 0 { start_angle(p[0], p[1]) } else { start_angle(p[p.len() - 1], p[p.len() - 2]) }
    };
    for o in out.iter_mut() {
        o.sort_by(|&x, &y| angle(x).total_cmp(&angle(y)));
    }
    let head = |d: usize| if d % 2 == 0 { edges[d / 2].1 } else { edges[d / 2].0 };
    let next = |d: usize| -> Option<usize> {
        let list = &out[head(d)];
        if list.len() != 2 {
            return None;
        }
        Some(if list[0] == (d ^ 1) { list[1] } else { list[0] })
    };
    let mut marked = vec![false; edges.len()];
    let mut strings: Vec<Vec<usize>> = vec![];
    let start_at = |v: usize, marked: &mut Vec<bool>, strings: &mut Vec<Vec<usize>>| {
        for &d0 in &out[v] {
            if marked[d0 / 2] {
                continue;
            }
            let mut string = vec![];
            let mut cur = d0;
            loop {
                string.push(cur);
                marked[cur / 2] = true;
                match next(cur) {
                    Some(nx) if nx != d0 => cur = nx,
                    _ => break,
                }
            }
            strings.push(string);
        }
    };
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| {
        let (p, q) = (nodes.pts[a], nodes.pts[b]);
        p[0].total_cmp(&q[0]).then(p[1].total_cmp(&q[1]))
    });
    for &v in &order {
        if out[v].len() != 2 {
            start_at(v, &mut marked, &mut strings);
        }
    }
    for &v in &order {
        start_at(v, &mut marked, &mut strings);
    }
    strings
        .iter()
        .map(|s| {
            let mut line: Vec<P2> = vec![];
            let (mut fwd, mut rev) = (0, 0);
            for &d in s {
                let p = &paths[d / 2];
                let pts: Vec<P2> = if d % 2 == 0 {
                    fwd += 1;
                    p.clone()
                } else {
                    rev += 1;
                    p.iter().rev().copied().collect()
                };
                for q in pts {
                    if line.last().is_none_or(|&l| !same(l, q)) {
                        line.push(q);
                    }
                }
            }
            if rev > fwd {
                line.reverse();
            }
            line
        })
        .collect()
}

/// A path with its first point repeated at the end is a ring.
pub fn is_ring(path: &[P2]) -> bool {
    path.len() >= 4 && same(path[0], path[path.len() - 1])
}

/// Shapely `is_ring`: closed and not crossing or touching itself anywhere else.
pub fn is_simple_ring(path: &[P2]) -> bool {
    is_ring(path) && node_lines(&[path.to_vec()]).len() == 1
}

/// Exact sign of the turn a -> b -> c: 1 left (counter-clockwise), -1 right, 0 straight. Done in
/// double-double arithmetic, as GEOS does, so near-collinear points the plain formula would call
/// straight are told apart.
pub(crate) fn orientation(a: P2, b: P2, c: P2) -> i32 {
    type Dd = (f64, f64);
    fn two_sum(x: f64, y: f64) -> Dd {
        let s = x + y;
        let bb = s - x;
        (s, (x - (s - bb)) + (y - bb))
    }
    fn add(x: Dd, y: Dd) -> Dd {
        let (s, e) = two_sum(x.0, y.0);
        let e = e + x.1 + y.1;
        two_sum(s, e)
    }
    fn neg(x: Dd) -> Dd {
        (-x.0, -x.1)
    }
    fn mul(x: Dd, y: Dd) -> Dd {
        let p = x.0 * y.0;
        let e = x.0.mul_add(y.0, -p) + (x.0 * y.1 + x.1 * y.0);
        two_sum(p, e)
    }
    let dx1 = add((b[0], 0.0), neg((a[0], 0.0)));
    let dy1 = add((b[1], 0.0), neg((a[1], 0.0)));
    let dx2 = add((c[0], 0.0), neg((b[0], 0.0)));
    let dy2 = add((c[1], 0.0), neg((b[1], 0.0)));
    let det = add(mul(dx1, dy2), neg(mul(dy1, dx2)));
    let v = if det.0 != 0.0 { det.0 } else { det.1 };
    if v > 0.0 { 1 } else if v < 0.0 { -1 } else { 0 }
}

fn unit(a: P2, b: P2) -> Option<P2> {
    let d = [b[0] - a[0], b[1] - a[1]];
    let l = d[0].hypot(d[1]);
    if l == 0.0 { None } else { Some([d[0] / l, d[1] / l]) }
}

/// The points GEOS adds where a path turns at `v`, offset to the left by `d` (negative: right)
/// with a mitre join (`OffsetSegmentGenerator::addNextSegment`):
/// - straight on: nothing; a U-turn: the two offset end points;
/// - outside of the turn: the mitre point, or past the mitre limit the corner cut off square
///   `MITRE_LIMIT * |d|` from the vertex along the bisector;
/// - inside of the turn: where the two offset segments cross, or when they do not reach each
///   other (offset longer than the segments), the end of the first, the middle between it and
///   the vertex, the same for the second, and the start of the second.
fn mitre_corner(prev: Option<P2>, v: P2, next: Option<P2>, d: f64) -> Vec<P2> {
    let left = |u: P2| [-u[1], u[0]];
    let at = |n: P2, p: P2| [p[0] + n[0] * d, p[1] + n[1] * d];
    match (prev, next) {
        (None, None) => vec![v],
        (Some(a), None) => vec![at(left(unit(a, v).unwrap_or([1.0, 0.0])), v)],
        (None, Some(c)) => vec![at(left(unit(v, c).unwrap_or([1.0, 0.0])), v)],
        (Some(a), Some(c)) => {
            let (Some(u0), Some(u1)) = (unit(a, v), unit(v, c)) else { return vec![v] };
            let (n0, n1) = (left(u0), left(u1));
            let turn = orientation(a, v, c);
            let (o0, o1) = (at(n0, v), at(n1, v));
            if turn == 0 {
                if u0[0] * u1[0] + u0[1] * u1[1] > 0.0 {
                    return vec![];
                }
                return vec![o0, o1];
            }
            if (turn > 0) == (d < 0.0) {
                // outside of the turn
                if (o0[0] - o1[0]).hypot(o0[1] - o1[1]) < d.abs() * 1e-3 {
                    return vec![o0];
                }
                let sum = [n0[0] + n1[0], n0[1] + n1[1]];
                let denom = 1.0 + n0[0] * n1[0] + n0[1] * n1[1];
                let scale = d / denom;
                let m = [v[0] + sum[0] * scale, v[1] + sum[1] * scale];
                let reach = (sum[0] * scale).hypot(sum[1] * scale);
                if reach / d.abs() <= MITRE_LIMIT {
                    return vec![m];
                }
                // cut the two offset lines where they cross the line MITRE_LIMIT * |d| out
                let axis = [sum[0] * scale / reach, sum[1] * scale / reach];
                let limit = MITRE_LIMIT * d.abs();
                let cut = |n: P2, u: P2| -> P2 {
                    let b = at(n, v);
                    let along = (b[0] - v[0]) * axis[0] + (b[1] - v[1]) * axis[1];
                    let t = (limit - along) / (u[0] * axis[0] + u[1] * axis[1]);
                    [b[0] + u[0] * t, b[1] + u[1] * t]
                };
                return vec![cut(n0, u0), cut(n1, u1)];
            }
            // inside of the turn
            let hits = segment_meets(at(n0, a), o0, o1, at(n1, c));
            if let Some(&p) = hits.first() {
                return vec![p];
            }
            if (o0[0] - o1[0]).hypot(o0[1] - o1[1]) < d.abs() * 1e-3 {
                return vec![o0];
            }
            let mid = |p: P2| [(p[0] + v[0]) / 2.0, (p[1] + v[1]) / 2.0];
            vec![o0, mid(o0), mid(o1), o1]
        }
    }
}

/// A curve with every crossing and touch made a vertex: node ids in order from the first point
/// (a closed curve is not repeated at its end). `pts` has the node coordinates.
struct NodedCurve {
    pts: Vec<P2>,
    seq: Vec<usize>,
    /// Per node id: the curve is cut here (its start, or a crossing or touch).
    is_node: Vec<bool>,
}

/// Node a curve (`closed`: the last point joins back to the first) against itself.
fn node_curve(raw: &[P2], closed: bool) -> NodedCurve {
    let mut pts: Vec<P2> = vec![];
    for &p in raw {
        if pts.last().is_none_or(|&l| !same(l, p)) {
            pts.push(p);
        }
    }
    if closed && pts.len() > 1 && same(pts[0], pts[pts.len() - 1]) {
        pts.pop();
    }
    let n = pts.len();
    let nseg = if closed { n } else { n.saturating_sub(1) };
    let segs: Vec<[P2; 2]> = (0..nseg).map(|i| [pts[i], pts[(i + 1) % n]]).collect();
    let mut cuts: Vec<Vec<P2>> = vec![vec![]; nseg];
    for (i, j) in candidate_pairs(&segs) {
        let mut hits = segment_meets(segs[i][0], segs[i][1], segs[j][0], segs[j][1]);
        // consecutive segments meet at their shared vertex, which is no new node
        if j == i + 1 {
            hits.retain(|&p| !same(p, segs[i][1]));
        } else if closed && i == 0 && j == nseg - 1 {
            hits.retain(|&p| !same(p, segs[0][0]));
        }
        for p in hits {
            cuts[i].push(p);
            cuts[j].push(p);
        }
    }
    let mut map = NodeMap::default();
    let mut seq: Vec<usize> = vec![];
    let push = |seq: &mut Vec<usize>, map: &mut NodeMap, p: P2| {
        let id = map.id(p);
        if seq.last() != Some(&id) {
            seq.push(id);
        }
    };
    for (i, s) in segs.iter().enumerate() {
        push(&mut seq, &mut map, s[0]);
        let dir = [s[1][0] - s[0][0], s[1][1] - s[0][1]];
        let l2 = dir[0] * dir[0] + dir[1] * dir[1];
        let mut c: Vec<(f64, P2)> =
            cuts[i].iter().map(|&p| (((p[0] - s[0][0]) * dir[0] + (p[1] - s[0][1]) * dir[1]) / l2, p)).collect();
        c.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (_, p) in c {
            push(&mut seq, &mut map, p);
        }
    }
    if !closed {
        if let Some(&l) = pts.last() {
            push(&mut seq, &mut map, l);
        }
    } else if seq.len() > 1 && seq[0] == seq[seq.len() - 1] {
        seq.pop();
    }
    let mut is_node = vec![false; map.pts.len()];
    if let Some(&f) = seq.first() {
        is_node[f] = true;
    }
    for p in cuts.iter().flatten() {
        is_node[map.id(*p)] = true;
    }
    NodedCurve { pts: map.pts, seq, is_node }
}

/// Winding number of a closed curve around a point (counter-clockwise is positive).
fn winding(curve: &[P2], q: P2) -> i32 {
    let n = curve.len();
    let mut w = 0;
    for i in 0..n {
        let (a, b) = (curve[i], curve[(i + 1) % n]);
        let side = (b[0] - a[0]) * (q[1] - a[1]) - (q[0] - a[0]) * (b[1] - a[1]);
        if a[1] <= q[1] {
            if b[1] > q[1] && side > 0.0 {
                w += 1;
            }
        } else if b[1] <= q[1] && side < 0.0 {
            w -= 1;
        }
    }
    w
}

/// shapely `LineString.offset_curve(d, join_style="mitre")`: positive `d` is the left side. A
/// port of GEOS `OffsetCurve` (see `crate::geos`). Returns the pieces in order along the curve.
pub fn offset_curve_pieces(path: &[P2], d: f64) -> Vec<Vec<P2>> {
    crate::geos::offset_curve::offset_curve(path, d)
}

/// shapely `Polygon(ring).buffer(d, join_style="mitre")` for a ring: the outlines of the result,
/// one per piece (none when the shape shrinks away, two or more when it splits). Holes are not
/// returned (1.x only keeps outlines). Each outline runs clockwise, not closed.
///
/// The raw mitre ring is noded against itself and only the parts that bound a positive winding
/// area are kept (what GEOS does after offsetting), so an over-shrunk ring gives nothing instead
/// of a flipped ring. Vertices GEOS keeps, even when collinear, are kept here. Outlines start at
/// the offset image of the ring's first vertex and the pieces come in order of where they start.
pub fn buffer_mitre(ring: &[P2], d: f64) -> Vec<Vec<P2>> {
    let mut r: Vec<P2> = vec![];
    for &p in ring {
        if r.last().is_none_or(|&l| !same(l, p)) {
            r.push(p);
        }
    }
    if r.len() > 1 && same(r[0], r[r.len() - 1]) {
        r.pop();
    }
    let n = r.len();
    if n < 3 {
        return vec![];
    }
    let area = ring_area(&r);
    if area == 0.0 {
        return vec![];
    }
    // GEOS skips a ring that shrinks away completely: a triangle whose incircle is smaller
    // than the shrink, or a ring whose smaller side is under twice the shrink
    if d < 0.0 && eroded_completely(&r, -d) {
        return vec![];
    }
    let ccw = area > 0.0;
    // growing is offsetting to the right of a counter-clockwise ring, to the left of a clockwise one
    let side = if ccw { -d } else { d };
    let mut raw: Vec<P2> = vec![];
    for i in 0..n {
        raw.extend(mitre_corner(Some(r[(i + n - 1) % n]), r[i], Some(r[(i + 1) % n]), side));
    }
    if ring_curve_inverted(&r, d.abs(), &raw) {
        return vec![];
    }
    outline_pieces(&raw, ccw)
}

/// GEOS `OffsetCurveSetBuilder::isRingCurveInverted`: the offset of a small ring (up to 7
/// vertices) that has turned inside out and should be dropped, because no point of it lies
/// near the distance it should be from the ring.
fn ring_curve_inverted(ring: &[P2], distance: f64, curve: &[P2]) -> bool {
    let n = ring.len();
    if distance == 0.0 || n + 1 <= 3 || n + 1 >= 9 || curve.len() + 1 > 4 * (n + 1) {
        return false;
    }
    let mut closed_ring = ring.to_vec();
    closed_ring.push(ring[0]);
    let tol = 0.99 * distance;
    for i in 0..curve.len() {
        let (v, next) = (curve[i], curve[(i + 1) % curve.len()]);
        if distance_to_path(&closed_ring, v) > tol {
            return false;
        }
        let mid = [(v[0] + next[0]) / 2.0, (v[1] + next[1]) / 2.0];
        if distance_to_path(&closed_ring, mid) > tol {
            return false;
        }
    }
    true
}

/// GEOS `OffsetCurveSetBuilder::isErodedCompletely` for a ring (not repeated at its end) shrunk
/// by `shrink`.
fn eroded_completely(r: &[P2], shrink: f64) -> bool {
    if r.len() == 3 {
        let (p0, p1, p2) = (r[0], r[1], r[2]);
        let dist = |a: P2, b: P2| (a[0] - b[0]).hypot(a[1] - b[1]);
        let (a, b, c) = (dist(p1, p2), dist(p0, p2), dist(p0, p1));
        let sum = a + b + c;
        let incentre = [(a * p0[0] + b * p1[0] + c * p2[0]) / sum, (a * p0[1] + b * p1[1] + c * p2[1]) / sum];
        return point_segment_distance(p0, p1, incentre) < shrink;
    }
    let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    for p in r {
        for k in 0..2 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    2.0 * shrink > (hi[0] - lo[0]).min(hi[1] - lo[1])
}

/// The outlines of the area a closed raw curve winds around positively (`ccw`) or negatively.
///
/// Follows the way GEOS builds a buffer: the curve is cut into edges at its crossings and
/// touches (and at its start); an edge bounds the result when the wound area is on one side of
/// it only; the edges are visited from the lowest node (x, then y), taking the edges at each
/// node counter-clockwise from the +x axis; and each outline starts with the first bounding edge
/// met that way and turns at every node onto the next edge counter-clockwise from the way back.
fn outline_pieces(raw: &[P2], ccw: bool) -> Vec<Vec<P2>> {
    outline_rings(raw, ccw, false)
}

/// Every ring of the area a closed raw curve winds around (`ccw`: positively): the clockwise
/// shells and, with `holes`, the counter-clockwise holes, in the order GEOS finds them.
pub(crate) fn outline_all_rings(raw: &[P2], ccw: bool) -> Vec<Vec<P2>> {
    outline_rings(raw, ccw, true)
}

fn outline_rings(raw: &[P2], ccw: bool, holes: bool) -> Vec<Vec<P2>> {
    let noded = node_curve(raw, true);
    let m = noded.seq.len();
    if m < 3 {
        return vec![];
    }
    let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    for p in raw {
        for k in 0..2 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    let eps = 1e-9 * (hi[0] - lo[0]).max(hi[1] - lo[1]).max(1.0);
    let sign = if ccw { 1 } else { -1 };

    // edges: runs of the curve between nodes
    let starts: Vec<usize> = (0..m).filter(|&k| noded.is_node[noded.seq[k]]).collect();
    let mut paths: Vec<Vec<usize>> = vec![]; // node ids along each edge
    for (i, &k0) in starts.iter().enumerate() {
        let k1 = if i + 1 < starts.len() { starts[i + 1] } else { m };
        let mut path: Vec<usize> = noded.seq[k0..k1].to_vec();
        path.push(noded.seq[k1 % m]);
        paths.push(path);
    }
    let pt = |id: usize| noded.pts[id];
    // directed edge 2*i runs along edge i, 2*i + 1 against it; in result when the wound area
    // is on its right and not on its left
    let mut in_result = vec![false; paths.len() * 2];
    for (i, path) in paths.iter().enumerate() {
        let (a, b) = (pt(path[0]), pt(path[1]));
        let len = (b[0] - a[0]).hypot(b[1] - a[1]);
        if len == 0.0 {
            continue;
        }
        let mid = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
        let nx = (b[1] - a[1]) / len * eps;
        let ny = (b[0] - a[0]) / len * eps;
        let wl = winding(raw, [mid[0] - nx, mid[1] + ny]) * sign;
        let wr = winding(raw, [mid[0] + nx, mid[1] - ny]) * sign;
        if wr >= 1 && wl <= 0 {
            in_result[2 * i] = true;
        } else if wl >= 1 && wr <= 0 {
            in_result[2 * i + 1] = true;
        }
    }
    if !in_result.iter().any(|&b| b) {
        return vec![];
    }
    let tail = |d: usize| if d % 2 == 0 { paths[d / 2][0] } else { *paths[d / 2].last().unwrap() };
    let head = |d: usize| tail(d ^ 1);
    let first_step = |d: usize| -> f64 {
        let p = &paths[d / 2];
        if d % 2 == 0 {
            start_angle(pt(p[0]), pt(p[1]))
        } else {
            start_angle(pt(p[p.len() - 1]), pt(p[p.len() - 2]))
        }
    };
    let mut out_of: HashMap<usize, Vec<usize>> = HashMap::new();
    for d in 0..paths.len() * 2 {
        out_of.entry(tail(d)).or_default().push(d);
    }
    for list in out_of.values_mut() {
        list.sort_by(|&x, &y| first_step(x).total_cmp(&first_step(y)));
    }
    // depth first from the lowest node, as GEOS lists the directed edges of a subgraph
    let lowest = *out_of
        .keys()
        .min_by(|&&a, &&b| pt(a)[0].total_cmp(&pt(b)[0]).then(pt(a)[1].total_cmp(&pt(b)[1])))
        .unwrap();
    let mut visited: std::collections::HashSet<usize> = Default::default();
    let mut stack = vec![lowest];
    let mut listed: Vec<usize> = vec![];
    while let Some(node) = stack.pop() {
        visited.insert(node);
        for &d in &out_of[&node] {
            listed.push(d);
            if !visited.contains(&head(d)) {
                stack.push(head(d));
            }
        }
    }
    let mut used = vec![false; paths.len() * 2];
    let mut rings: Vec<Vec<P2>> = vec![];
    for &s in &listed {
        if !in_result[s] || used[s] {
            continue;
        }
        let mut ring: Vec<P2> = vec![];
        let mut cur = s;
        loop {
            used[cur] = true;
            let ids: Vec<usize> = if cur % 2 == 0 {
                paths[cur / 2].clone()
            } else {
                paths[cur / 2].iter().rev().copied().collect()
            };
            for &id in &ids[..ids.len() - 1] {
                ring.push(pt(id));
            }
            // next: the bounding edge out of the head met first turning counter-clockwise from the way back
            let h = head(cur);
            let back = first_step(cur ^ 1);
            let nxt = out_of[&h].iter().copied().filter(|&e| in_result[e]).min_by(|&x, &y| {
                let ang = |e: usize| {
                    let a = first_step(e) - back;
                    if a <= 0.0 { a + 2.0 * std::f64::consts::PI } else { a }
                };
                ang(x).total_cmp(&ang(y))
            });
            match nxt {
                Some(e) if e != s && !used[e] => cur = e,
                _ => break,
            }
        }
        if ring.len() >= 3 {
            let a = ring_area(&ring);
            if a < 0.0 || (holes && a > 0.0) {
                rings.push(ring);
            }
        }
    }
    rings
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sq(x: f64, y: f64, s: f64) -> Vec<P2> {
        vec![[x, y], [x + s, y], [x + s, y + s], [x, y + s], [x, y]]
    }

    #[test]
    fn touching_squares_are_two_faces() {
        let f = polygonize(&node_lines(&[sq(0.0, 0.0, 10.0), sq(10.0, 0.0, 10.0)]));
        assert_eq!(f.len(), 2);
        assert!(f.iter().all(|f| (f.area() - 100.0).abs() < 1e-9));
    }

    #[test]
    fn crossing_lines_split_a_square_and_dangles_go() {
        let lines = vec![sq(0.0, 0.0, 10.0), vec![[5.0, -3.0], [5.0, 13.0]], vec![[2.0, 2.0], [4.0, 4.0]]];
        let f = polygonize(&node_lines(&lines));
        assert_eq!(f.len(), 2);
        assert!(f.iter().all(|f| (f.area() - 50.0).abs() < 1e-9));
    }

    #[test]
    fn island_becomes_a_hole_and_its_own_face() {
        let f = polygonize(&node_lines(&[sq(0.0, 0.0, 10.0), sq(3.0, 3.0, 2.0)]));
        assert_eq!(f.len(), 2);
        let outer = f.iter().find(|f| f.exterior.len() == 4 && !f.holes.is_empty()).unwrap();
        assert!((outer.area() - 96.0).abs() < 1e-9);
        assert!(!outer.contains([4.0, 4.0]) && outer.contains([1.0, 1.0]) && !outer.contains([0.0, 5.0]));
    }

    /// Timing of the noding step: `cargo test -p stl-sketch --release noding_10k -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn noding_10k_segments() {
        let mut state = 12345u64;
        let mut rnd = move || {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((state >> 33) as f64) / ((1u64 << 31) as f64)
        };
        let lines: Vec<Vec<P2>> = (0..10_000)
            .map(|_| {
                let (x, y) = (rnd() * 1000.0, rnd() * 1000.0);
                vec![[x, y], [x + rnd() * 20.0 - 10.0, y + rnd() * 20.0 - 10.0]]
            })
            .collect();
        let t = std::time::Instant::now();
        let chains = node_lines(&lines);
        let dt = t.elapsed();
        let h: f64 = chains.iter().flatten().map(|p| p[0] * 1.000001 + p[1]).sum();
        println!("noding 10000 segments: {:?}, {} chains, checksum {:.6}", dt, chains.len(), h);
    }

    #[test]
    fn mitre_buffer_of_a_square() {
        let b = buffer_mitre(&sq(0.0, 0.0, 10.0)[..4], 1.0);
        assert_eq!(b, vec![vec![[-1.0, -1.0], [-1.0, 11.0], [11.0, 11.0], [11.0, -1.0]]]);
        assert_eq!(
            offset_curve_pieces(&[[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]], 1.0),
            vec![vec![[0.0, 1.0], [9.0, 1.0], [9.0, 10.0]]]
        );
    }

    #[test]
    fn over_shrunk_ring_is_empty_and_a_dumbbell_splits() {
        // 4 x 2 shrunk by 1.5: gone, not a flipped ring
        assert!(buffer_mitre(&[[0.0, 0.0], [4.0, 0.0], [4.0, 2.0], [0.0, 2.0]], -1.5).is_empty());
        // a thin neck cut through: two pieces, each clockwise
        let d = [
            [0.0, 0.0], [10.0, 0.0], [10.0, 4.0], [6.0, 4.0], [6.0, 6.0], [10.0, 6.0],
            [10.0, 10.0], [0.0, 10.0], [0.0, 6.0], [4.0, 6.0], [4.0, 4.0], [0.0, 4.0],
        ];
        let pieces = buffer_mitre(&d, -1.0);
        assert_eq!(pieces.len(), 2);
        assert!(pieces.iter().all(|p| ring_area(p) < 0.0));
        let total: f64 = pieces.iter().map(|p| ring_area(p).abs()).sum();
        assert!((total - 32.0).abs() < 1e-9, "{total}");
        // the same ring the other way round gives the same pieces
        let mut rev = d.to_vec();
        rev.reverse();
        let back = buffer_mitre(&rev, -1.0);
        assert_eq!(back.len(), 2);
        assert!((back.iter().map(|p| ring_area(p).abs()).sum::<f64>() - 32.0).abs() < 1e-9);
    }

    #[test]
    fn sharp_corner_is_cut_five_distances_out() {
        // a thin triangle grown by 1: the tip is cut off 5 away from the tip vertex
        let b = buffer_mitre(&[[0.0, 0.0], [20.0, 1.0], [20.0, -1.0]], 1.0);
        assert_eq!(b.len(), 1);
        let tip: Vec<&P2> = b[0].iter().filter(|p| (p[0] + 5.0).abs() < 1e-9).collect();
        assert_eq!(tip.len(), 2, "{b:?}");
    }

    #[test]
    fn linemerge_keeps_directions() {
        // three pieces pointing against the order they are met in: the line runs the way they point
        let pieces = vec![vec![[10.0, 0.0], [0.0, 0.0]], vec![[10.0, 10.0], [10.0, 0.0]], vec![[0.0, 10.0], [10.0, 10.0]]];
        let m = linemerge(&pieces);
        assert_eq!(m, vec![vec![[0.0, 10.0], [10.0, 10.0], [10.0, 0.0], [0.0, 0.0]]]);
        // a line another line ends on is two pieces, each keeping its direction
        let t = linemerge(&node_lines(&[vec![[0.0, 0.0], [20.0, 0.0]], vec![[10.0, 0.0], [10.0, 10.0]]]));
        assert_eq!(t.len(), 3);
        assert!(t.contains(&vec![[0.0, 0.0], [10.0, 0.0]]) && t.contains(&vec![[10.0, 0.0], [20.0, 0.0]]));
        // a closed loop: whichever way most pieces point
        let ring = linemerge(&[vec![[0.0, 0.0], [0.0, 10.0]], vec![[10.0, 10.0], [0.0, 10.0]], vec![[10.0, 0.0], [10.0, 10.0]], vec![[10.0, 0.0], [0.0, 0.0]]]);
        assert_eq!(ring.len(), 1);
        assert!(is_simple_ring(&ring[0]));
    }

    #[test]
    fn inner_corner_loops_are_cut_away_by_offset_curve() {
        // offset longer than the second piece: the curve must not fold back on itself
        let path = [[0.0, 0.0], [10.0, 0.0], [10.0, 2.0]];
        let pieces = offset_curve_pieces(&path, 5.0);
        for p in &pieces {
            for w in p.windows(2) {
                let mid = [(w[0][0] + w[1][0]) / 2.0, (w[0][1] + w[1][1]) / 2.0];
                assert!(distance_to_path(&path, mid) >= 5.0 - 1e-9, "{p:?}");
            }
        }
        assert!(!pieces.is_empty());
    }

    #[test]
    fn orientation_is_exact() {
        assert_eq!(orientation([0.0, 0.0], [1.0, 1.0], [2.0, 2.0]), 0);
        assert_eq!(orientation([0.0, 0.0], [1.0, 0.0], [1.0, 1.0]), 1);
        assert_eq!(orientation([0.0, 0.0], [1.0, 1.0], [1.0, 0.0]), -1);
        // a hair off the line is told apart from the line
        assert_eq!(orientation([0.0, 0.0], [1.0, 1.0], [2.0, 2.0 + 4.0e-16]), 1);
    }
}
