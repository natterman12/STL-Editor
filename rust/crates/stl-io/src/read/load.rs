//! Port of 1.x `geometry.load_mesh_file`: dispatch by extension, weld, name.

use super::{Part, ReadError, gltf, obj, off, ply, stl, threemf};
use stl_mesh::weld;

const TOL: f64 = 1e-5;

/// `os.path.splitext(os.path.basename(filename))` -> (base, lower-cased extension).
fn split_name(filename: &str) -> (String, String) {
    let name = filename.rsplit(['/', '\\']).next().unwrap_or("");
    // splitext ignores leading dots
    let lead = name.len() - name.trim_start_matches('.').len();
    match name[lead..].rfind('.') {
        Some(i) => (name[..lead + i].to_string(), name[lead + i..].to_lowercase()),
        None => (name.to_string(), String::new()),
    }
}

/// numpy `np.round(x, 3)`.
fn round3(x: f64) -> f64 {
    (x * 1000.0).round_ties_even() / 1000.0
}

pub fn load_mesh_file(data: &[u8], filename: &str) -> Result<Vec<Part>, ReadError> {
    let (base, ext) = split_name(filename);
    let raw = match ext.as_str() {
        ".stl" => stl::read(data, &base)?,
        ".obj" => obj::read(data, &base)?,
        ".3mf" => threemf::read(data, &base)?,
        ".ply" => ply::read(data, &base)?,
        ".off" => off::read(data, &base)?,
        ".glb" | ".gltf" => gltf::read(data, &base)?,
        _ => return Err(ReadError("Unsupported file type. Use STL, OBJ, 3MF, PLY, OFF or GLB.".into())),
    };
    let mut parts: Vec<Part> = raw
        .into_iter()
        .map(|p| {
            let (positions, triangles) = weld(&p.positions, &p.triangles, TOL);
            Part { name: p.name, positions, triangles }
        })
        .collect();
    if (ext == ".glb" || ext == ".gltf") && parts.len() > 1 {
        // scene order differs between glTF readers: sort by position so both editions agree
        let key = |p: &Part| {
            let mut m = [f64::INFINITY; 3];
            for v in p.positions.chunks_exact(3) {
                for k in 0..3 {
                    if v[k] < m[k] {
                        m[k] = v[k];
                    }
                }
            }
            ([round3(m[0]), round3(m[1]), round3(m[2])], p.triangles.len() / 3)
        };
        parts.sort_by(|a, b| {
            let (ka, na) = key(a);
            let (kb, nb) = key(b);
            ka.partial_cmp(&kb).unwrap_or(std::cmp::Ordering::Equal).then(na.cmp(&nb))
        });
        for (i, p) in parts.iter_mut().enumerate() {
            p.name = format!("{base} {}", i + 1);
        }
    }
    if parts.is_empty() {
        return Err(ReadError("No triangles found in this file.".into()));
    }
    Ok(parts)
}

#[cfg(test)]
mod tests {
    use super::split_name;

    #[test]
    fn names() {
        assert_eq!(split_name("C:\\a\\b.Part.STL"), ("b.Part".into(), ".stl".into()));
        assert_eq!(split_name(".stl"), (".stl".into(), "".into()));
        assert_eq!(split_name("x"), ("x".into(), "".into()));
    }
}
