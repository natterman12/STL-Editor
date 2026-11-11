//! A port of Fast Quadric Mesh Simplification, as bundled in `fast_simplification` 0.2.0
//! (`Simplify.h`, called with the defaults of `fast_simplification.simplify`).
//!
//! Fast Quadric Mesh Simplification (C) 2014 Sven Forstmann, MIT licence:
//! https://github.com/sp4cerat/Fast-Quadric-Mesh-Simplification
//!
//! All arithmetic is in f64 in the same order as the C++, so the collapses come out the same.

type V = [f64; 3];

fn sub(a: V, b: V) -> V {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// `vec3f::dot`: `a.x*x + a.y*y + a.z*z`.
fn dot(s: V, a: V) -> f64 {
    a[0] * s[0] + a[1] * s[1] + a[2] * s[2]
}

fn cross(a: V, b: V) -> V {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn normalize(a: V) -> V {
    let square = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
    [a[0] / square, a[1] / square, a[2] / square]
}

/// `SymetricMatrix`: the upper triangle of a symmetric 4x4 quadric.
#[derive(Clone, Copy)]
struct Quadric([f64; 10]);

impl Quadric {
    fn plane(a: f64, b: f64, c: f64, d: f64) -> Quadric {
        Quadric([a * a, a * b, a * c, a * d, b * b, b * c, b * d, c * c, c * d, d * d])
    }

    fn add(&self, n: &Quadric) -> Quadric {
        let mut m = self.0;
        for (x, y) in m.iter_mut().zip(n.0) {
            *x += y;
        }
        Quadric(m)
    }

    #[allow(clippy::too_many_arguments)]
    fn det(&self, a11: usize, a12: usize, a13: usize, a21: usize, a22: usize, a23: usize, a31: usize, a32: usize, a33: usize) -> f64 {
        let m = &self.0;
        m[a11] * m[a22] * m[a33] + m[a13] * m[a21] * m[a32] + m[a12] * m[a23] * m[a31]
            - m[a13] * m[a22] * m[a31]
            - m[a11] * m[a23] * m[a32]
            - m[a12] * m[a21] * m[a33]
    }
}

#[derive(Clone, Copy)]
struct Triangle {
    v: [usize; 3],
    err: [f64; 4],
    deleted: bool,
    dirty: bool,
    n: V,
}

struct Vertex {
    p: V,
    tstart: usize,
    tcount: usize,
    q: Quadric,
    border: bool,
}

#[derive(Clone, Copy)]
struct Ref {
    tid: usize,
    tvertex: usize,
}

struct Simplify {
    triangles: Vec<Triangle>,
    vertices: Vec<Vertex>,
    refs: Vec<Ref>,
}

fn vertex_error(q: &Quadric, x: f64, y: f64, z: f64) -> f64 {
    let q = &q.0;
    q[0] * x * x + 2.0 * q[1] * x * y + 2.0 * q[2] * x * z + 2.0 * q[3] * x + q[4] * y * y
        + 2.0 * q[5] * y * z
        + 2.0 * q[6] * y
        + q[7] * z * z
        + 2.0 * q[8] * z
        + q[9]
}

impl Simplify {
    /// The error of collapsing the edge, and the point to collapse to. `p` keeps its value when
    /// every candidate error is NaN, as the C++ leaves `p_result` untouched then.
    fn calculate_error(&self, id_v1: usize, id_v2: usize, p: &mut V) -> f64 {
        let v1 = &self.vertices[id_v1];
        let v2 = &self.vertices[id_v2];
        let q = v1.q.add(&v2.q);
        let border = v1.border && v2.border;
        let det = q.det(0, 1, 2, 1, 4, 5, 2, 5, 7);
        if det != 0.0 && !border {
            p[0] = -1.0 / det * q.det(1, 2, 3, 4, 5, 6, 5, 7, 8);
            p[1] = 1.0 / det * q.det(0, 2, 3, 1, 5, 6, 2, 7, 8);
            p[2] = -1.0 / det * q.det(0, 1, 3, 1, 4, 6, 2, 5, 8);
            vertex_error(&q, p[0], p[1], p[2])
        } else {
            let p1 = v1.p;
            let p2 = v2.p;
            let p3 = [(p1[0] + p2[0]) / 2.0, (p1[1] + p2[1]) / 2.0, (p1[2] + p2[2]) / 2.0];
            let error1 = vertex_error(&q, p1[0], p1[1], p1[2]);
            let error2 = vertex_error(&q, p2[0], p2[1], p2[2]);
            let error3 = vertex_error(&q, p3[0], p3[1], p3[2]);
            let error = error1.min(error2.min(error3));
            if error1 == error {
                *p = p1;
            }
            if error2 == error {
                *p = p2;
            }
            if error3 == error {
                *p = p3;
            }
            error
        }
    }

    /// Whether a triangle around `i0` flips (or becomes a sliver) when `i0` moves to `p`.
    /// Marks in `deleted` the triangles that also use `i1`.
    fn flipped(&self, p: V, i0: usize, i1: usize, deleted: &mut [bool]) -> bool {
        let v0 = &self.vertices[i0];
        for k in 0..v0.tcount {
            let r = self.refs[v0.tstart + k];
            let t = &self.triangles[r.tid];
            if t.deleted {
                continue;
            }
            let s = r.tvertex;
            let id1 = t.v[(s + 1) % 3];
            let id2 = t.v[(s + 2) % 3];
            if id1 == i1 || id2 == i1 {
                deleted[k] = true;
                continue;
            }
            let d1 = normalize(sub(self.vertices[id1].p, p));
            let d2 = normalize(sub(self.vertices[id2].p, p));
            if dot(d1, d2).abs() > 0.999 {
                return true;
            }
            let n = normalize(cross(d1, d2));
            deleted[k] = false;
            if dot(n, t.n) < 0.2 {
                return true;
            }
        }
        false
    }

    /// Point the triangles of vertex `v` at `i0` and refresh their edge errors.
    fn update_triangles(&mut self, i0: usize, v: usize, deleted: &[bool], deleted_triangles: &mut usize) {
        let mut p = [0.0; 3];
        let (tstart, tcount) = (self.vertices[v].tstart, self.vertices[v].tcount);
        for k in 0..tcount {
            let r = self.refs[tstart + k];
            if self.triangles[r.tid].deleted {
                continue;
            }
            if deleted[k] {
                self.triangles[r.tid].deleted = true;
                *deleted_triangles += 1;
                continue;
            }
            let mut t = self.triangles[r.tid];
            t.v[r.tvertex] = i0;
            t.dirty = true;
            t.err[0] = self.calculate_error(t.v[0], t.v[1], &mut p);
            t.err[1] = self.calculate_error(t.v[1], t.v[2], &mut p);
            t.err[2] = self.calculate_error(t.v[2], t.v[0], &mut p);
            t.err[3] = t.err[0].min(t.err[1].min(t.err[2]));
            self.triangles[r.tid] = t;
            self.refs.push(r);
        }
    }

    /// Compact the triangles, set up the quadrics, errors and borders (first time only) and
    /// rebuild the vertex-to-triangle references.
    fn update_mesh(&mut self, iteration: usize) {
        if iteration > 0 {
            self.triangles.retain(|t| !t.deleted);
        }
        if iteration == 0 {
            for v in &mut self.vertices {
                v.q = Quadric([0.0; 10]);
                v.border = false;
            }
            for i in 0..self.triangles.len() {
                let tv = self.triangles[i].v;
                let p = tv.map(|j| self.vertices[j].p);
                let n = normalize(cross(sub(p[1], p[0]), sub(p[2], p[0])));
                self.triangles[i].n = n;
                let plane = Quadric::plane(n[0], n[1], n[2], -dot(n, p[0]));
                for j in tv {
                    self.vertices[j].q = self.vertices[j].q.add(&plane);
                }
            }
            let mut p = [0.0; 3];
            for i in 0..self.triangles.len() {
                let tv = self.triangles[i].v;
                let mut err = [0.0; 4];
                for j in 0..3 {
                    err[j] = self.calculate_error(tv[j], tv[(j + 1) % 3], &mut p);
                }
                err[3] = err[0].min(err[1].min(err[2]));
                self.triangles[i].err = err;
            }
        }

        for v in &mut self.vertices {
            v.tstart = 0;
            v.tcount = 0;
        }
        for t in &self.triangles {
            for j in t.v {
                self.vertices[j].tcount += 1;
            }
        }
        let mut tstart = 0;
        for v in &mut self.vertices {
            v.tstart = tstart;
            tstart += v.tcount;
            v.tcount = 0;
        }
        self.refs.clear();
        self.refs.resize(self.triangles.len() * 3, Ref { tid: 0, tvertex: 0 });
        for (i, t) in self.triangles.iter().enumerate() {
            for (j, &vi) in t.v.iter().enumerate() {
                let v = &mut self.vertices[vi];
                self.refs[v.tstart + v.tcount] = Ref { tid: i, tvertex: j };
                v.tcount += 1;
            }
        }

        if iteration == 0 {
            // a vertex is on the border when some neighbour shares only one triangle with it
            let mut vcount: Vec<usize> = Vec::new();
            let mut vids: Vec<usize> = Vec::new();
            for i in 0..self.vertices.len() {
                vcount.clear();
                vids.clear();
                let (ts, tc) = (self.vertices[i].tstart, self.vertices[i].tcount);
                for j in 0..tc {
                    let t = &self.triangles[self.refs[ts + j].tid];
                    for id in t.v {
                        match vids.iter().position(|&x| x == id) {
                            Some(ofs) => vcount[ofs] += 1,
                            None => {
                                vcount.push(1);
                                vids.push(id);
                            }
                        }
                    }
                }
                for j in 0..vcount.len() {
                    if vcount[j] == 1 {
                        self.vertices[vids[j]].border = true;
                    }
                }
            }
        }
    }

    /// `simplify_mesh(target_count, agressiveness, verbose = false, preserve_border = false)`.
    fn simplify_mesh(&mut self, target_count: usize, agressiveness: f64) {
        for t in &mut self.triangles {
            t.deleted = false;
        }
        let mut deleted_triangles = 0usize;
        let mut deleted0: Vec<bool> = Vec::new();
        let mut deleted1: Vec<bool> = Vec::new();
        let triangle_count = self.triangles.len();
        let mut p = [0.0; 3];
        for iteration in 0..100 {
            if triangle_count - deleted_triangles <= target_count {
                break;
            }
            if iteration % 5 == 0 {
                self.update_mesh(iteration);
            }
            for t in &mut self.triangles {
                t.dirty = false;
            }
            // every triangle with an edge below the threshold is collapsed
            let threshold = 0.000000001 * ((iteration + 3) as f64).powf(agressiveness);

            for i in 0..self.triangles.len() {
                {
                    let t = &self.triangles[i];
                    if t.err[3] > threshold || t.deleted || t.dirty {
                        continue;
                    }
                }
                for j in 0..3 {
                    let t = self.triangles[i];
                    if !(t.err[j] < threshold) {
                        continue;
                    }
                    let i0 = t.v[j];
                    let i1 = t.v[(j + 1) % 3];
                    if self.vertices[i0].border != self.vertices[i1].border {
                        continue;
                    }

                    self.calculate_error(i0, i1, &mut p);
                    // `resize` keeps the earlier flags, as in the C++
                    deleted0.resize(self.vertices[i0].tcount, false);
                    deleted1.resize(self.vertices[i1].tcount, false);
                    if self.flipped(p, i0, i1, &mut deleted0) {
                        continue;
                    }
                    if self.flipped(p, i1, i0, &mut deleted1) {
                        continue;
                    }

                    // not flipped, so remove the edge: v0 <- v1
                    let q1 = self.vertices[i1].q;
                    let v0 = &mut self.vertices[i0];
                    v0.p = p;
                    v0.q = q1.add(&v0.q);
                    let tstart = self.refs.len();

                    self.update_triangles(i0, i0, &deleted0, &mut deleted_triangles);
                    self.update_triangles(i0, i1, &deleted1, &mut deleted_triangles);

                    let tcount = self.refs.len() - tstart;
                    let v0 = &mut self.vertices[i0];
                    if tcount <= v0.tcount {
                        // save memory
                        if tcount > 0 {
                            self.refs.copy_within(tstart..tstart + tcount, v0.tstart);
                        }
                    } else {
                        v0.tstart = tstart;
                    }
                    v0.tcount = tcount;
                    break;
                }
                if triangle_count - deleted_triangles <= target_count {
                    break;
                }
            }
        }
        self.compact_mesh();
    }

    /// Drop deleted triangles and unused vertices, keeping the order of both.
    fn compact_mesh(&mut self) {
        for v in &mut self.vertices {
            v.tcount = 0;
        }
        self.triangles.retain(|t| !t.deleted);
        for t in &self.triangles {
            for j in t.v {
                self.vertices[j].tcount = 1;
            }
        }
        let mut dst = 0;
        for i in 0..self.vertices.len() {
            if self.vertices[i].tcount != 0 {
                self.vertices[i].tstart = dst;
                self.vertices[dst].p = self.vertices[i].p;
                dst += 1;
            }
        }
        for t in &mut self.triangles {
            for j in 0..3 {
                t.v[j] = self.vertices[t.v[j]].tstart;
            }
        }
        self.vertices.truncate(dst);
    }
}

/// `fast_simplification.simplify(verts, faces, target_reduction)` with its defaults
/// (`agg = 7`, not lossless, borders not preserved). The positions go in as f64, as the
/// library converts them. Returns the kept positions and triangles.
pub fn fqms_simplify(verts: &[[f32; 3]], faces: &[[u32; 3]], target_reduction: f64) -> (Vec<[f64; 3]>, Vec<[u32; 3]>) {
    let n_faces = faces.len();
    // `int((1 - target_reduction) * n_faces)`; the library refuses a reduction outside [0, 1]
    let target_count = ((1.0 - target_reduction.clamp(0.0, 1.0)) * n_faces as f64) as usize;
    if faces.iter().flatten().any(|&i| i as usize >= verts.len()) {
        return (Vec::new(), Vec::new());
    }
    let mut s = Simplify {
        triangles: faces
            .iter()
            .map(|f| Triangle {
                v: f.map(|i| i as usize),
                err: [0.0; 4],
                deleted: false,
                dirty: false,
                n: [0.0; 3],
            })
            .collect(),
        vertices: verts
            .iter()
            .map(|p| Vertex {
                p: p.map(f64::from),
                tstart: 0,
                tcount: 0,
                q: Quadric([0.0; 10]),
                border: false,
            })
            .collect(),
        refs: Vec::new(),
    };
    s.simplify_mesh(target_count, 7.0);
    (
        s.vertices.iter().map(|v| v.p).collect(),
        s.triangles.iter().map(|t| t.v.map(|i| i as u32)).collect(),
    )
}
