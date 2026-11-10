//! OFF reader matching trimesh 5.2.0 `exchange.off.load_off`.

use super::{Part, ReadError};

fn bad() -> ReadError {
    ReadError("Could not read this OFF file.".into())
}

/// Strip `#…` comments the way trimesh `util.comment_strip` does.
fn comment_strip(text: &str) -> String {
    if !text.contains('#') {
        return text.to_string();
    }
    let padded = format!("{text}\n");
    let split: Vec<&str> = padded.split('#').collect();
    let lead = if text.starts_with('#') { "" } else { split[0] };
    let mut rest = Vec::new();
    for chunk in &split {
        if let Some((_, after)) = chunk.split_once('\n') {
            if !after.is_empty() {
                rest.push(after);
            }
        }
    }
    format!("{lead}\n{}", rest.join("\n")).trim().to_string()
}

fn triangulate_faces(faces: &[Vec<u32>]) -> Vec<u32> {
    if faces.is_empty() {
        return Vec::new();
    }
    let all_tri = faces.iter().all(|f| f.len() == 3);
    if all_tri {
        let mut out = Vec::with_capacity(faces.len().saturating_mul(3));
        for f in faces {
            out.extend_from_slice(f);
        }
        return out;
    }
    let all_quad = faces.iter().all(|f| f.len() == 4);
    if all_quad {
        let mut out = Vec::with_capacity(faces.len().saturating_mul(6));
        for f in faces {
            out.extend_from_slice(&[f[0], f[1], f[2]]);
        }
        for f in faces {
            out.extend_from_slice(&[f[2], f[3], f[0]]);
        }
        return out;
    }
    let mut out = Vec::new();
    for f in faces.iter().filter(|f| f.len() == 3) {
        out.extend_from_slice(f);
    }
    let quads: Vec<&Vec<u32>> = faces.iter().filter(|f| f.len() == 4).collect();
    for f in &quads {
        out.extend_from_slice(&[f[0], f[1], f[2]]);
    }
    for f in &quads {
        out.extend_from_slice(&[f[2], f[3], f[0]]);
    }
    for f in faces.iter().filter(|f| f.len() > 4) {
        for i in 0..f.len() - 2 {
            out.extend_from_slice(&[f[0], f[i + 1], f[i + 2]]);
        }
    }
    out
}

/// See the contract in `read/mod.rs`.
pub fn read(data: &[u8], base: &str) -> Result<Vec<Part>, ReadError> {
    let text = std::str::from_utf8(data).map_err(|_| bad())?;
    let text = comment_strip(text.trim());

    let upper = text.to_ascii_uppercase();
    let after = if let Some(i) = upper.find("COFF") {
        &text[i + 4..]
    } else if let Some(i) = upper.find("OFF") {
        &text[i + 3..]
    } else {
        return Err(bad());
    };

    let splits: Vec<&str> = after
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    if splits.is_empty() {
        return Err(bad());
    }
    let header: Vec<&str> = splits[0].split_whitespace().collect();
    if header.len() < 2 {
        return Err(bad());
    }
    let vertex_count: usize = header[0].parse().map_err(|_| bad())?;
    let face_count: usize = header[1].parse().map_err(|_| bad())?;

    // Need the count line + every vertex; fewer faces than declared is OK (trimesh slice).
    let vert_end = 1usize.checked_add(vertex_count).ok_or_else(bad)?;
    if splits.len() < vert_end {
        return Err(bad());
    }
    let available_faces = splits.len() - vert_end;
    let n_faces = face_count.min(available_faces);

    let pos_cap = vertex_count.checked_mul(3).ok_or_else(bad)?;
    let mut positions = Vec::with_capacity(pos_cap);
    for line in &splits[1..vert_end] {
        let mut tok = line.split_whitespace();
        for _ in 0..3 {
            let t = tok.next().ok_or_else(bad)?;
            positions.push(t.parse::<f64>().map_err(|_| bad())?);
        }
    }

    let mut faces: Vec<Vec<u32>> = Vec::with_capacity(n_faces);
    for line in &splits[vert_end..vert_end + n_faces] {
        let toks: Vec<&str> = line.split_whitespace().collect();
        if toks.is_empty() {
            return Err(bad());
        }
        let n: usize = toks[0].parse().map_err(|_| bad())?;
        // Bound by remaining tokens — never allocate from an untrusted count alone.
        let max_n = toks.len() - 1;
        if n > max_n {
            return Err(bad());
        }
        let mut face = Vec::with_capacity(n);
        for t in &toks[1..1 + n] {
            let idx: i64 = t.parse().map_err(|_| bad())?;
            if idx < 0 || idx as usize >= vertex_count {
                return Err(bad());
            }
            face.push(idx as u32);
        }
        faces.push(face);
    }

    let triangles = triangulate_faces(&faces);
    if triangles.is_empty() {
        return Ok(Vec::new());
    }
    Ok(vec![Part { name: base.to_string(), positions, triangles }])
}
