use super::pyfmt::c_fixed8;
use super::{ExportBody, ExportError, ExportOptions};

/// trimesh's `export_obj` of the bodies concatenated in order: a header, `v` lines, `f` lines
/// (1-based, offset per body), no normals.
pub(super) fn write(bodies: &[ExportBody], _opts: &ExportOptions) -> Result<Vec<u8>, ExportError> {
    let mut out = String::from("# https://github.com/mikedh/trimesh\n");
    for b in bodies {
        for p in b.mesh.positions.chunks_exact(3) {
            out.push_str(&format!("v {} {} {}\n", c_fixed8(p[0] as f64), c_fixed8(p[1] as f64), c_fixed8(p[2] as f64)));
        }
    }
    let mut off = 1u64;
    for b in bodies {
        for t in b.mesh.triangles.chunks_exact(3) {
            out.push_str(&format!("f {} {} {}\n", t[0] as u64 + off, t[1] as u64 + off, t[2] as u64 + off));
        }
        off += (b.mesh.positions.len() / 3) as u64;
    }
    out.push('\n');
    Ok(out.into_bytes())
}
