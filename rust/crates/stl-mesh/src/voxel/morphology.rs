//! Binary morphology with the 6-neighbour cross structure, as scipy.ndimage
//! (`binary_dilation`, `binary_erosion` with border_value 0, `binary_fill_holes`).
//! Behaviour follows scipy v1.18.1 (BSD-3-Clause, https://github.com/scipy/scipy).

use super::Grid3;
use crate::MeshError;

fn cap_error() -> MeshError {
    MeshError::Message("Resolution too fine for this model size. Use a coarser setting.".to_string())
}

fn step(g: &Grid3<bool>, dilate: bool) -> Result<Grid3<bool>, MeshError> {
    let [nx, ny, nz] = g.n;
    let mut out = Grid3::try_new(g.n, false)?;
    for i in 0..nx {
        for j in 0..ny {
            for k in 0..nz {
                let c = g.idx(i, j, k);
                let nb = [
                    (i > 0).then(|| g.data[g.idx(i - 1, j, k)]),
                    (i + 1 < nx).then(|| g.data[g.idx(i + 1, j, k)]),
                    (j > 0).then(|| g.data[g.idx(i, j - 1, k)]),
                    (j + 1 < ny).then(|| g.data[g.idx(i, j + 1, k)]),
                    (k > 0).then(|| g.data[g.idx(i, j, k - 1)]),
                    (k + 1 < nz).then(|| g.data[g.idx(i, j, k + 1)]),
                ];
                out.data[c] = if dilate {
                    g.data[c] || nb.iter().any(|v| *v == Some(true))
                } else {
                    // Outside the grid counts as empty.
                    g.data[c] && nb.iter().all(|v| *v == Some(true))
                };
            }
        }
    }
    Ok(out)
}

/// `iterations == 0` repeats until nothing changes (scipy's `iterations < 1`).
fn repeat(g: &Grid3<bool>, iterations: usize, dilate: bool) -> Result<Grid3<bool>, MeshError> {
    let mut cur = g.clone();
    if iterations == 0 {
        loop {
            let next = step(&cur, dilate)?;
            if next.data == cur.data {
                return Ok(cur);
            }
            cur = next;
        }
    }
    for _ in 0..iterations {
        cur = step(&cur, dilate)?;
    }
    Ok(cur)
}

pub fn binary_dilation(g: &Grid3<bool>, iterations: usize) -> Result<Grid3<bool>, MeshError> {
    repeat(g, iterations, true)
}

pub fn binary_erosion(g: &Grid3<bool>, iterations: usize) -> Result<Grid3<bool>, MeshError> {
    repeat(g, iterations, false)
}

/// Fills empty cells that cannot reach the grid border through empty cells (6-connected):
/// scipy dilates the complement from the border (border_value 1) inside the complement.
/// Cells are marked when pushed, so the stack never holds more entries than there are cells.
pub fn binary_fill_holes(g: &Grid3<bool>) -> Result<Grid3<bool>, MeshError> {
    let [nx, ny, nz] = g.n;
    let mut outside = Grid3::try_new(g.n, false)?;
    let mut stack: Vec<u32> = Vec::new();

    fn push(
        g: &Grid3<bool>,
        outside: &mut Grid3<bool>,
        stack: &mut Vec<u32>,
        i: usize,
        j: usize,
        k: usize,
    ) -> Result<(), MeshError> {
        let id = g.idx(i, j, k);
        if !g.data[id] && !outside.data[id] {
            outside.data[id] = true;
            if stack.len() == stack.capacity() {
                stack.try_reserve(stack.len().max(4096)).map_err(|_| cap_error())?;
            }
            stack.push(id as u32);
        }
        Ok(())
    }

    for i in 0..nx {
        for j in 0..ny {
            for k in 0..nz {
                if i == 0 || i + 1 == nx || j == 0 || j + 1 == ny || k == 0 || k + 1 == nz {
                    push(g, &mut outside, &mut stack, i, j, k)?;
                }
            }
        }
    }
    while let Some(id) = stack.pop() {
        let id = id as usize;
        let k = id % nz;
        let j = (id / nz) % ny;
        let i = id / (nz * ny);
        if i > 0 {
            push(g, &mut outside, &mut stack, i - 1, j, k)?;
        }
        if i + 1 < nx {
            push(g, &mut outside, &mut stack, i + 1, j, k)?;
        }
        if j > 0 {
            push(g, &mut outside, &mut stack, i, j - 1, k)?;
        }
        if j + 1 < ny {
            push(g, &mut outside, &mut stack, i, j + 1, k)?;
        }
        if k > 0 {
            push(g, &mut outside, &mut stack, i, j, k - 1)?;
        }
        if k + 1 < nz {
            push(g, &mut outside, &mut stack, i, j, k + 1)?;
        }
    }
    // Invert in place: filled = not reachable from the border.
    for o in outside.data.iter_mut() {
        *o = !*o;
    }
    Ok(outside)
}
