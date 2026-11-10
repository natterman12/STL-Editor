//! STL, binary and ASCII: port of 1.x `read_stl_bytes` (not trimesh's reader).

use super::{Part, ReadError};

fn bad() -> ReadError {
    ReadError("Could not read this STL file.".into())
}

/// Python `str.isspace` for the ASCII range.
fn is_ws(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\x0b' | '\x0c' | '\x1c'..='\x1f')
}

/// Unwelded triangle soup: three vertices per triangle. Like 1.x, an STL always gives one
/// part, even a binary file with zero triangles.
pub fn read(data: &[u8], base: &str) -> Result<Vec<Part>, ReadError> {
    let pos = read_soup(data)?;
    let n = (pos.len() / 3) as u32;
    Ok(vec![Part { name: base.to_string(), positions: pos, triangles: (0..n).collect() }])
}

fn read_soup(b: &[u8]) -> Result<Vec<f64>, ReadError> {
    if b.len() >= 84 {
        let n = u32::from_le_bytes([b[80], b[81], b[82], b[83]]) as u64;
        if 84 + 50 * n == b.len() as u64 {
            let mut pos = Vec::with_capacity(n as usize * 9);
            for i in 0..n as usize {
                let rec = 84 + 50 * i + 12; // skip the normal
                for k in 0..9 {
                    let o = rec + 4 * k;
                    pos.push(f32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) as f64);
                }
            }
            return Ok(pos);
        }
    }
    // ASCII: decode("ascii", "ignore") drops non-ASCII bytes
    let txt: String = b.iter().filter(|c| **c < 128).map(|c| *c as char).collect();
    let mut pts: Vec<f64> = Vec::new();
    // str.splitlines(): \n \r \v \f \x1c \x1d \x1e
    for line in txt.split(|c| matches!(c, '\n' | '\r' | '\x0b' | '\x0c' | '\x1c' | '\x1d' | '\x1e')) {
        let s = line.trim_matches(is_ws);
        if s.starts_with("vertex") {
            let p: Vec<&str> = s.split(is_ws).filter(|t| !t.is_empty()).collect();
            if p.len() < 4 {
                return Err(bad());
            }
            for t in &p[1..4] {
                pts.push(t.parse::<f64>().map_err(|_| bad())?);
            }
        }
    }
    if pts.is_empty() || (pts.len() / 3) % 3 != 0 {
        return Err(bad());
    }
    Ok(pts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii() {
        let t = b"solid t\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 1 0 0\nvertex 0 1 0.5e0\nendloop\nendfacet\nendsolid t\n";
        let p = read(t, "t").unwrap();
        assert_eq!(p[0].positions, vec![0., 0., 0., 1., 0., 0., 0., 1., 0.5]);
        assert_eq!(p[0].triangles, vec![0, 1, 2]);
    }

    #[test]
    fn binary() {
        let mut b = vec![0u8; 80];
        b.extend_from_slice(&1u32.to_le_bytes());
        b.extend_from_slice(&[0u8; 12]);
        for v in [0f32, 0., 0., 1., 0., 0., 0., 1., 0.] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.extend_from_slice(&[0, 0]);
        let p = read(&b, "t").unwrap();
        assert_eq!(p[0].positions[3], 1.0);
    }

    #[test]
    fn garbage_is_an_error() {
        assert_eq!(read(b"hello", "x").unwrap_err().0, "Could not read this STL file.");
    }
}
