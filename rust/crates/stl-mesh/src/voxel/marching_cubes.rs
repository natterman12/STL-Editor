//! Marching cubes (Lewiner et al.), ported from scikit-image v0.26.0
//! (`skimage/measure/_marching_cubes_lewiner.py` and `_marching_cubes_lewiner_cy.pyx`;
//! the tables are in `marching_cubes/tables.rs`). scikit-image is Copyright 2009-2022 the
//! scikit-image team, BSD-3-Clause. The Cython code was ported by Almar Klein from Thomas
//! Lewiner's C++ implementation.
//!
//! Only what `skimage.measure.marching_cubes(volume, level, spacing=...)` returns with its other
//! defaults is ported: method `lewiner`, `step_size=1`, `gradient_direction="descent"`,
//! `allow_degenerate=True`, no mask. Normals and values are not computed: they do not change
//! the vertices or faces.

mod tables;

use self::tables::*;
use super::Grid3;

/// `np.spacing(1.0)`, the Cython file's `FLT_EPSILON`.
const FLT_EPSILON: f64 = f64::EPSILON;

/// The Cython `Cell`: the current cube, its corner values (level subtracted) and the vertices
/// and faces found so far. `x` runs along the volume's last axis, `z` along its first.
struct Cell {
    x: usize,
    y: usize,
    z: usize,
    v: [f64; 8],
    vv: [f64; 8],
    v12: [f64; 3],
    v12_calculated: bool,
    index: usize,
    nx: usize,
    /// Vertex indices of the edges in the current (`layer1`) and next (`layer2`) z layer:
    /// 4 per cell, as `get_index_in_facelayer`.
    layer1: Vec<i32>,
    layer2: Vec<i32>,
    vertices: Vec<[f32; 3]>,
    faces: Vec<u32>,
}

impl Cell {
    fn new(nx: usize, ny: usize) -> Cell {
        Cell {
            x: 0,
            y: 0,
            z: 0,
            v: [0.0; 8],
            vv: [0.0; 8],
            v12: [0.0; 3],
            v12_calculated: false,
            index: 0,
            nx,
            layer1: vec![-1; nx * ny * 4],
            layer2: vec![-1; nx * ny * 4],
            vertices: Vec::new(),
            faces: Vec::new(),
        }
    }

    fn new_z_value(&mut self) {
        std::mem::swap(&mut self.layer1, &mut self.layer2);
        self.layer2.fill(-1);
    }

    fn set_cube(&mut self, isovalue: f64, x: usize, y: usize, z: usize, corners: [f32; 8]) {
        self.x = x;
        self.y = y;
        self.z = z;
        let mut index = 0;
        for (c, &val) in corners.iter().enumerate() {
            self.v[c] = val as f64 - isovalue;
            if self.v[c] > 0.0 {
                index += 1 << c;
            }
        }
        self.index = index;
        self.v12_calculated = false;
    }

    fn add_triangles(&mut self, lut: &Lut, lut_index: usize, nt: usize) {
        self.prepare_for_adding_triangles();
        for i in 0..nt {
            for j in 0..3 {
                let vi = lut.get2(lut_index, i * 3 + j);
                self.add_face_from_edge_index(vi as usize);
            }
        }
    }

    fn add_triangles2(&mut self, lut: &Lut, lut_index: usize, lut_index2: usize, nt: usize) {
        self.prepare_for_adding_triangles();
        for i in 0..nt {
            for j in 0..3 {
                let vi = lut.get3(lut_index, lut_index2, i * 3 + j);
                self.add_face_from_edge_index(vi as usize);
            }
        }
    }

    fn add_face_from_edge_index(&mut self, vi: usize) {
        let (upper, at) = self.get_index_in_facelayer(vi);
        let existing = if upper { self.layer2[at] } else { self.layer1[at] };
        let index = if existing >= 0 {
            existing as u32
        } else {
            let p = if vi == 12 {
                if !self.v12_calculated {
                    self.calculate_center_vertex();
                }
                self.v12
            } else {
                let (dx1, dx2) = (EDGESRELX.get2(vi, 0), EDGESRELX.get2(vi, 1));
                let (dy1, dy2) = (EDGESRELY.get2(vi, 0), EDGESRELY.get2(vi, 1));
                let (dz1, dz2) = (EDGESRELZ.get2(vi, 0), EDGESRELZ.get2(vi, 1));
                let index1 = (dz1 * 4 + dy1 * 2 + dx1) as usize;
                let index2 = (dz2 * 4 + dy2 * 2 + dx2) as usize;
                let tmpf1 = 1.0 / (FLT_EPSILON + self.vv[index1].abs());
                let tmpf2 = 1.0 / (FLT_EPSILON + self.vv[index2].abs());
                let (mut fx, mut fy, mut fz, mut ff) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
                fx += dx1 as f64 * tmpf1;
                fy += dy1 as f64 * tmpf1;
                fz += dz1 as f64 * tmpf1;
                ff += tmpf1;
                fx += dx2 as f64 * tmpf2;
                fy += dy2 as f64 * tmpf2;
                fz += dz2 as f64 * tmpf2;
                ff += tmpf2;
                [self.x as f64 + fx / ff, self.y as f64 + fy / ff, self.z as f64 + fz / ff]
            };
            self.vertices.push([p[0] as f32, p[1] as f32, p[2] as f32]);
            let created = (self.vertices.len() - 1) as i32;
            if upper {
                self.layer2[at] = created;
            } else {
                self.layer1[at] = created;
            }
            created as u32
        };
        self.faces.push(index);
    }

    /// Which layer (`true` for the next z layer) and which slot hold the vertex on edge `vi`.
    fn get_index_in_facelayer(&self, vi: usize) -> (bool, usize) {
        let mut i = self.nx * self.y + self.x;
        let mut j = 0;
        let upper;
        if vi < 8 {
            let e = if vi < 4 { vi } else { vi - 4 };
            upper = vi >= 4;
            match e {
                1 => {
                    i += 1;
                    j = 1;
                }
                2 => i += self.nx,
                3 => j = 1,
                _ => {}
            }
        } else if vi < 12 {
            upper = false;
            j = 2;
            match vi {
                9 => i += 1,
                10 => i += self.nx + 1,
                11 => i += self.nx,
                _ => {}
            }
        } else {
            upper = false;
            j = 3;
        }
        (upper, 4 * i + j)
    }

    fn prepare_for_adding_triangles(&mut self) {
        let v = &self.v;
        // the corner numbering is not the xyz bit order: 2 and 3, and 6 and 7, swap
        self.vv = [v[0], v[1], v[3], v[2], v[4], v[5], v[7], v[6]];
    }

    fn calculate_center_vertex(&mut self) {
        let w: [f64; 8] = self.v.map(|c| 1.0 / (FLT_EPSILON + c.abs()));
        let (mut fx, mut fy, mut fz, mut ff) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
        const POS: [[f64; 3]; 8] = [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            [0.0, 1.0, 1.0],
        ];
        for c in 0..8 {
            fx += POS[c][0] * w[c];
            fy += POS[c][1] * w[c];
            fz += POS[c][2] * w[c];
            ff += w[c];
        }
        self.v12 = [self.x as f64 + fx / ff, self.y as f64 + fy / ff, self.z as f64 + fz / ff];
        self.v12_calculated = true;
    }
}

/// `test_face`: whether the surface crosses face `face` (sign gives the orientation).
fn test_face(cell: &Cell, face: i32) -> bool {
    let v = &cell.v;
    let (a, b, c, d) = match face.abs() {
        1 => (v[0], v[4], v[5], v[1]),
        2 => (v[1], v[5], v[6], v[2]),
        3 => (v[2], v[6], v[7], v[3]),
        4 => (v[3], v[7], v[4], v[0]),
        5 => (v[0], v[3], v[2], v[1]),
        6 => (v[4], v[7], v[6], v[5]),
        _ => (0.0, 0.0, 0.0, 0.0),
    };
    let ac_bd = a * c - b * d;
    if ac_bd > -FLT_EPSILON && ac_bd < FLT_EPSILON {
        face >= 0
    } else {
        face as f64 * a * ac_bd >= 0.0
    }
}

/// `test_internal`: whether the cube's interior joins the two parts of an ambiguous case.
fn test_internal(cell: &Cell, case: i32, config: usize, subconfig: usize, s: i32) -> bool {
    let v = &cell.v;
    let (mut at, mut bt, mut ct, mut dt) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
    if case == 4 || case == 10 {
        let a = (v[4] - v[0]) * (v[6] - v[2]) - (v[7] - v[3]) * (v[5] - v[1]);
        let b = v[2] * (v[4] - v[0]) + v[0] * (v[6] - v[2]) - v[1] * (v[7] - v[3]) - v[3] * (v[5] - v[1]);
        let t = -b / (2.0 * a + FLT_EPSILON);
        #[allow(clippy::manual_range_contains)] // a NaN `t` must fall through, as in C
        if t < 0.0 || t > 1.0 {
            return s > 0;
        }
        at = v[0] + (v[4] - v[0]) * t;
        bt = v[3] + (v[7] - v[3]) * t;
        ct = v[2] + (v[6] - v[2]) * t;
        dt = v[1] + (v[5] - v[1]) * t;
    } else if case == 6 || case == 7 || case == 12 || case == 13 {
        let edge = match case {
            6 => TEST6.get2(config, 2),
            7 => TEST7.get2(config, 4),
            12 => TEST12.get2(config, 3),
            _ => TILING13_5_1.get3(config, subconfig, 0),
        };
        // (corner on the edge, its neighbour, then the three edges parallel to it)
        let (p, q, [b0, b1], [c0, c1], [d0, d1]) = match edge {
            0 => (0, 1, [3, 2], [7, 6], [4, 5]),
            1 => (1, 2, [0, 3], [4, 7], [5, 6]),
            2 => (2, 3, [1, 0], [5, 4], [6, 7]),
            3 => (3, 0, [2, 1], [6, 5], [7, 4]),
            4 => (4, 5, [7, 6], [3, 2], [0, 1]),
            5 => (5, 6, [4, 7], [0, 3], [1, 2]),
            6 => (6, 7, [5, 4], [1, 0], [2, 3]),
            7 => (7, 4, [6, 5], [2, 1], [3, 0]),
            8 => (0, 4, [3, 7], [2, 6], [1, 5]),
            9 => (1, 5, [0, 4], [3, 7], [2, 6]),
            10 => (2, 6, [1, 5], [0, 4], [3, 7]),
            11 => (3, 7, [2, 6], [1, 5], [0, 4]),
            _ => return test_result(0.0, 0.0, 0.0, 0.0, s),
        };
        let t = v[p] / (v[p] - v[q] + FLT_EPSILON);
        at = 0.0;
        bt = v[b0] + (v[b1] - v[b0]) * t;
        ct = v[c0] + (v[c1] - v[c0]) * t;
        dt = v[d0] + (v[d1] - v[d0]) * t;
    }
    test_result(at, bt, ct, dt, s)
}

fn test_result(at: f64, bt: f64, ct: f64, dt: f64, s: i32) -> bool {
    let mut test = 0;
    if at >= 0.0 {
        test += 1;
    }
    if bt >= 0.0 {
        test += 2;
    }
    if ct >= 0.0 {
        test += 4;
    }
    if dt >= 0.0 {
        test += 8;
    }
    match test {
        // Cython returns 0 when the inner test of 5 or 10 fails
        5 => at * ct - bt * dt < FLT_EPSILON && s > 0,
        10 => at * ct - bt * dt >= FLT_EPSILON && s > 0,
        7 | 11 | 13 | 14 | 15 => s < 0,
        _ => s > 0,
    }
}

/// `the_big_switch`: add the triangles for the cube's case and configuration.
fn the_big_switch(cell: &mut Cell, case: i32, config: usize) {
    let mut subconfig = 0usize;
    match case {
        1 => cell.add_triangles(&TILING1, config, 1),
        2 => cell.add_triangles(&TILING2, config, 2),
        3 => {
            if test_face(cell, TEST3.get1(config)) {
                cell.add_triangles(&TILING3_2, config, 4);
            } else {
                cell.add_triangles(&TILING3_1, config, 2);
            }
        }
        4 => {
            if test_internal(cell, case, config, subconfig, TEST4.get1(config)) {
                cell.add_triangles(&TILING4_1, config, 2);
            } else {
                cell.add_triangles(&TILING4_2, config, 6);
            }
        }
        5 => cell.add_triangles(&TILING5, config, 3),
        6 => {
            if test_face(cell, TEST6.get2(config, 0)) {
                cell.add_triangles(&TILING6_2, config, 5);
            } else if test_internal(cell, case, config, subconfig, TEST6.get2(config, 1)) {
                cell.add_triangles(&TILING6_1_1, config, 3);
            } else {
                cell.add_triangles(&TILING6_1_2, config, 9);
            }
        }
        7 => {
            for (bit, t) in [1usize, 2, 4].into_iter().zip(0..3) {
                if test_face(cell, TEST7.get2(config, t)) {
                    subconfig += bit;
                }
            }
            match subconfig {
                0 => cell.add_triangles(&TILING7_1, config, 3),
                1 => cell.add_triangles2(&TILING7_2, config, 0, 5),
                2 => cell.add_triangles2(&TILING7_2, config, 1, 5),
                3 => cell.add_triangles2(&TILING7_3, config, 0, 9),
                4 => cell.add_triangles2(&TILING7_2, config, 2, 5),
                5 => cell.add_triangles2(&TILING7_3, config, 1, 9),
                6 => cell.add_triangles2(&TILING7_3, config, 2, 9),
                _ => {
                    if test_internal(cell, case, config, subconfig, TEST7.get2(config, 3)) {
                        cell.add_triangles(&TILING7_4_2, config, 9);
                    } else {
                        cell.add_triangles(&TILING7_4_1, config, 5);
                    }
                }
            }
        }
        8 => cell.add_triangles(&TILING8, config, 2),
        9 => cell.add_triangles(&TILING9, config, 4),
        10 | 12 => {
            let (test, t1_1_, t2, t2_, t1_1, t1_2) = if case == 10 {
                (&TEST10, &TILING10_1_1_, &TILING10_2, &TILING10_2_, &TILING10_1_1, &TILING10_1_2)
            } else {
                (&TEST12, &TILING12_1_1_, &TILING12_2, &TILING12_2_, &TILING12_1_1, &TILING12_1_2)
            };
            if test_face(cell, test.get2(config, 0)) {
                if test_face(cell, test.get2(config, 1)) {
                    cell.add_triangles(t1_1_, config, 4);
                } else {
                    cell.add_triangles(t2, config, 8);
                }
            } else if test_face(cell, test.get2(config, 1)) {
                cell.add_triangles(t2_, config, 8);
            } else if test_internal(cell, case, config, subconfig, test.get2(config, 2)) {
                cell.add_triangles(t1_1, config, 4);
            } else {
                cell.add_triangles(t1_2, config, 8);
            }
        }
        11 => cell.add_triangles(&TILING11, config, 4),
        13 => {
            for t in 0..6 {
                if test_face(cell, TEST13.get2(config, t)) {
                    subconfig += 1 << t;
                }
            }
            let sub = SUBCONFIG13.get1(subconfig);
            match sub {
                0 => cell.add_triangles(&TILING13_1, config, 4),
                1..=6 => cell.add_triangles2(&TILING13_2, config, (sub - 1) as usize, 6),
                7..=18 => cell.add_triangles2(&TILING13_3, config, (sub - 7) as usize, 10),
                19..=22 => cell.add_triangles2(&TILING13_4, config, (sub - 19) as usize, 12),
                23..=26 => {
                    let sc = (sub - 23) as usize;
                    if test_internal(cell, case, config, sc, TEST13.get2(config, 6)) {
                        cell.add_triangles2(&TILING13_5_1, config, sc, 6);
                    } else {
                        cell.add_triangles2(&TILING13_5_2, config, sc, 10);
                    }
                }
                27..=38 => cell.add_triangles2(&TILING13_3_, config, (sub - 27) as usize, 10),
                39..=44 => cell.add_triangles2(&TILING13_2_, config, (sub - 39) as usize, 6),
                45 => cell.add_triangles(&TILING13_1_, config, 4),
                _ => {}
            }
        }
        14 => cell.add_triangles(&TILING14, config, 4),
        _ => {}
    }
}

/// `skimage.measure.marching_cubes(field, level, spacing=(spacing,) * 3)`: vertices in index
/// order `(i, j, k)` scaled by `spacing`, and faces wound for "greater than `level` is inside"
/// (`gradient_direction="descent"`), in skimage's order. Where skimage raises (a dimension
/// under 2, `level` outside the field's range, or no surface), this returns no vertices.
pub fn marching_cubes(field: &Grid3<f32>, level: f32, spacing: f64) -> (Vec<[f64; 3]>, Vec<[u32; 3]>) {
    let [n0, n1, n2] = field.n;
    if n0 < 2 || n1 < 2 || n2 < 2 || field.data.is_empty() {
        return (Vec::new(), Vec::new());
    }
    let (mut lo, mut hi) = (f32::INFINITY, f32::NEG_INFINITY);
    for &x in &field.data {
        lo = lo.min(x);
        hi = hi.max(x);
    }
    if level < lo || level > hi {
        return (Vec::new(), Vec::new());
    }
    let isovalue = level as f64;
    let (nx, ny, nz) = (n2, n1, n0);
    let im = |z: usize, y: usize, x: usize| field.data[(z * n1 + y) * n2 + x];
    let mut cell = Cell::new(nx, ny);
    for z in 0..nz - 1 {
        cell.new_z_value();
        for y in 0..ny - 1 {
            for x in 0..nx - 1 {
                let corners = [
                    im(z, y, x),
                    im(z, y, x + 1),
                    im(z, y + 1, x + 1),
                    im(z, y + 1, x),
                    im(z + 1, y, x),
                    im(z + 1, y, x + 1),
                    im(z + 1, y + 1, x + 1),
                    im(z + 1, y + 1, x),
                ];
                cell.set_cube(isovalue, x, y, z, corners);
                let case = CASES.get2(cell.index, 0);
                if case > 0 {
                    let config = CASES.get2(cell.index, 1) as usize;
                    the_big_switch(&mut cell, case, config);
                }
            }
        }
    }
    let verts: Vec<[f64; 3]> =
        cell.vertices.iter().map(|p| [p[2] as f64 * spacing, p[1] as f64 * spacing, p[0] as f64 * spacing]).collect();
    let faces: Vec<[u32; 3]> = cell.faces.chunks_exact(3).map(|t| [t[2], t[1], t[0]]).collect();
    (verts, faces)
}
