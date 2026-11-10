//! OBJ reader matching trimesh 5.2.0 `exchange.obj.load_obj` with defaults
//! (`process=False`, `split_objects=False`, `group_material=True`).
//!
//! Without `usemtl`, faces merge into one mesh named `base` (1.x: a single Trimesh).
//! With `usemtl`, faces group by material (faces before the first `usemtl` are their own
//! group); geometries emit in reverse first-appearance order (`face_tuples.pop()`), each
//! with unreferenced vertices dropped; names are `{base} {i+1}` when there is more than
//! one group (else `base`).

use super::{Part, ReadError};

fn bad() -> ReadError {
    ReadError("Could not read this OBJ file.".into())
}

/// Fan / quad triangulation used by trimesh's slow face fallback and `triangulate_quads`.
fn triangulate(poly: &[u32], out: &mut Vec<u32>) {
    match poly.len() {
        0..=2 => {}
        3 => out.extend_from_slice(poly),
        4 => {
            out.extend_from_slice(&[poly[0], poly[1], poly[2]]);
            out.extend_from_slice(&[poly[2], poly[3], poly[0]]);
        }
        _ => {
            for i in 0..poly.len() - 2 {
                out.extend_from_slice(&[poly[0], poly[i + 1], poly[i + 2]]);
            }
        }
    }
}

/// Resolve an OBJ face index against the full vertex list.
/// Positive: 1-based. Zero: left as vertex 0 (trimesh `faces[faces > 0] -= 1`). Negative:
/// count from the end.
fn resolve_index(raw: i64, nverts: usize) -> Result<u32, ReadError> {
    if nverts == 0 {
        return Err(bad());
    }
    if raw > 0 {
        let i = (raw - 1) as usize;
        if i >= nverts {
            return Err(bad());
        }
        Ok(i as u32)
    } else if raw < 0 {
        let i = nverts as i64 + raw;
        if i < 0 || i as usize >= nverts {
            return Err(bad());
        }
        Ok(i as u32)
    } else {
        Ok(0)
    }
}

fn parse_face_line(s: &str, nverts: usize, triangles: &mut Vec<u32>) -> Result<(), ReadError> {
    let mut idxs: Vec<u32> = Vec::new();
    for tok in s.split_whitespace().skip(1) {
        let head = tok.split('/').next().unwrap_or("");
        if head.is_empty() {
            return Err(bad());
        }
        let raw: i64 = head.parse().map_err(|_| bad())?;
        idxs.push(resolve_index(raw, nverts)?);
    }
    triangulate(&idxs, triangles);
    Ok(())
}

/// Drop unreferenced vertices and remap face indices (trimesh `mask_v[faces] = True`).
fn mask_vertices(positions: &[f64], triangles: &[u32]) -> (Vec<f64>, Vec<u32>) {
    let nverts = positions.len() / 3;
    let mut used = vec![false; nverts];
    for &i in triangles {
        if (i as usize) < nverts {
            used[i as usize] = true;
        }
    }
    let mut remap = vec![0u32; nverts];
    let mut kept = Vec::new();
    let mut next = 0u32;
    for i in 0..nverts {
        if used[i] {
            remap[i] = next;
            kept.extend_from_slice(&positions[3 * i..3 * i + 3]);
            next += 1;
        }
    }
    let mut out_tri = Vec::with_capacity(triangles.len());
    for &t in triangles {
        out_tri.push(remap[t as usize]);
    }
    (kept, out_tri)
}

fn build_part(base: &str, n_dump: usize, index: usize, positions: Vec<f64>, triangles: Vec<u32>) -> Part {
    let name = if n_dump == 1 {
        base.to_string()
    } else {
        format!("{} {}", base, index + 1)
    };
    Part { name, positions, triangles }
}

/// See the contract in `read/mod.rs`.
pub fn read(data: &[u8], base: &str) -> Result<Vec<Part>, ReadError> {
    let text = std::str::from_utf8(data).map_err(|_| bad())?;
    let text = text.replace("\r\n", "\n").replace("\\\n", "");

    let mut positions: Vec<f64> = Vec::new();
    for line in text.lines() {
        let s = line.trim();
        if !s.starts_with("v ") && s != "v" {
            continue;
        }
        let mut parts = s.split_whitespace();
        let _ = parts.next();
        let mut xyz = [0.0f64; 3];
        for slot in &mut xyz {
            let t = parts.next().ok_or_else(bad)?;
            *slot = t.parse().map_err(|_| bad())?;
        }
        positions.extend_from_slice(&xyz);
    }
    let nverts = positions.len() / 3;

    let has_faces = text.lines().any(|l| {
        let s = l.trim();
        s.starts_with("f ") || s == "f"
    });
    // trimesh: no vertices → empty geometry → "No triangles found…".
    if nverts == 0 || !has_faces {
        return Ok(Vec::new());
    }

    let has_usemtl = text.lines().any(|l| l.trim().starts_with("usemtl"));

    if !has_usemtl {
        let mut triangles = Vec::new();
        for line in text.lines() {
            let s = line.trim();
            if !s.starts_with("f ") && s != "f" {
                continue;
            }
            parse_face_line(s, nverts, &mut triangles)?;
        }
        if triangles.is_empty() {
            return Ok(Vec::new());
        }
        let (kept, tris) = mask_vertices(&positions, &triangles);
        return Ok(vec![build_part(base, 1, 0, kept, tris)]);
    }

    // Group face lines by material; `None` = faces before the first usemtl.
    // Insertion order = first appearance (trimesh `_group_by` defaultdict).
    let mut groups: Vec<(Option<String>, Vec<String>)> = Vec::new();
    let mut current: Option<String> = None;

    for line in text.lines() {
        let s = line.trim();
        if s.starts_with("usemtl") {
            let name = s.strip_prefix("usemtl").unwrap_or("").trim();
            current = Some(name.to_string());
            continue;
        }
        if s.starts_with("f ") || s == "f" {
            if let Some(i) = groups.iter().position(|(m, _)| m == &current) {
                groups[i].1.push(s.to_string());
            } else {
                groups.push((current.clone(), vec![s.to_string()]));
            }
        }
    }

    // trimesh `face_tuples.pop()` → reverse first-appearance order.
    let mut geoms: Vec<(Vec<f64>, Vec<u32>)> = Vec::new();
    for (_mat, face_lines) in groups.into_iter().rev() {
        let mut triangles = Vec::new();
        for s in &face_lines {
            parse_face_line(s, nverts, &mut triangles)?;
        }
        let (kept, tris) = mask_vertices(&positions, &triangles);
        geoms.push((kept, tris));
    }

    let n_dump = geoms.len();
    let mut parts = Vec::new();
    for (i, (pos, tris)) in geoms.into_iter().enumerate() {
        if tris.is_empty() {
            continue;
        }
        parts.push(build_part(base, n_dump, i, pos, tris));
    }
    Ok(parts)
}
