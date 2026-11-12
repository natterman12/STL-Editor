use std::io::{Cursor, Write};

use super::pyfmt::py_float_repr_into;
use super::{ExportBody, ExportError, ExportOptions};

const NS_CORE: &str = "http://schemas.microsoft.com/3dmanufacturing/core/2015/02";
const NS_B: &str = "http://schemas.microsoft.com/3dmanufacturing/beamlattice/2017/02";
const NS_M: &str = "http://schemas.microsoft.com/3dmanufacturing/material/2015/02";
const NS_P: &str = "http://schemas.microsoft.com/3dmanufacturing/production/2015/06";
const NS_S: &str = "http://schemas.microsoft.com/3dmanufacturing/slice/2015/07";
const NS_SC: &str = "http://schemas.microsoft.com/3dmanufacturing/securecontent/2019/04";

const RELS: &str = "<?xml version='1.0' encoding='utf-8'?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Type=\"http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel\" Target=\"/3D/3dmodel.model\" Id=\"rel0\"/></Relationships>";
const CONTENT_TYPES: &str = "<?xml version='1.0' encoding='utf-8'?>\n<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"jpeg\" ContentType=\"image/jpeg\"/><Default Extension=\"jpg\" ContentType=\"image/jpeg\"/><Default Extension=\"model\" ContentType=\"application/vnd.ms-package.3dmanufacturing-3dmodel+xml\"/><Default Extension=\"png\" ContentType=\"image/png\"/><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"texture\" ContentType=\"application/vnd.ms-package.3dmanufacturing-3dmodeltexture\"/></Types>";

/// ElementTree's attribute escaping.
fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            '\n' => o.push_str("&#10;"),
            '\r' => o.push_str("&#13;"),
            '\t' => o.push_str("&#09;"),
            c => o.push(c),
        }
    }
    o
}

struct Uuids(u64);

impl Uuids {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A version 4 UUID: `{8}-{4}-4{3}-{y}{3}-{12}`, `y` one of 8, 9, a, b.
    fn v4(&mut self) -> String {
        let a = self.next_u64();
        let b = self.next_u64();
        let h = format!("{a:016x}{b:016x}");
        let y = ['8', '9', 'a', 'b'][(b >> 62) as usize & 3];
        format!("{}-{}-4{}-{}{}-{}", &h[0..8], &h[8..12], &h[13..16], y, &h[17..20], &h[20..32])
    }
}

/// Append the decimal digits of `v`.
fn push_u32(o: &mut Vec<u8>, mut v: u32) {
    let mut b = [0u8; 10];
    let mut i = b.len();
    loop {
        i -= 1;
        b[i] = b'0' + (v % 10) as u8;
        v /= 10;
        if v == 0 {
            break;
        }
    }
    o.extend_from_slice(&b[i..]);
}

pub(super) fn write(bodies: &[ExportBody], opts: &ExportOptions) -> Result<Vec<u8>, ExportError> {
    let mut ids = Uuids(opts.uuid_seed);
    let total: usize = bodies.iter().map(|b| b.mesh.positions.len() * 14 + b.mesh.triangles.len() * 18).sum();
    let mut x: Vec<u8> = Vec::with_capacity(total + 4096);
    x.extend_from_slice(b"<?xml version='1.0' encoding='utf-8'?>
");
    x.extend_from_slice(
        format!(
            "<model xmlns=\"{NS_CORE}\" xmlns:b=\"{NS_B}\" xmlns:m=\"{NS_M}\" xmlns:p=\"{NS_P}\" xmlns:s=\"{NS_S}\" xmlns:sc=\"{NS_SC}\" unit=\"millimeter\"><resources>"
        )
        .as_bytes(),
    );
    for (i, b) in bodies.iter().enumerate() {
        x.extend_from_slice(
            format!(
                "<object id=\"{}\" name=\"{}\" type=\"model\" p:UUID=\"{}\"><mesh><vertices>",
                i + 1,
                esc(&b.name),
                ids.v4()
            )
            .as_bytes(),
        );
        for p in b.mesh.positions.chunks_exact(3) {
            x.extend_from_slice(b"<vertex x=\"");
            py_float_repr_into(&mut x, p[0] as f64);
            x.extend_from_slice(b"\" y=\"");
            py_float_repr_into(&mut x, p[1] as f64);
            x.extend_from_slice(b"\" z=\"");
            py_float_repr_into(&mut x, p[2] as f64);
            x.extend_from_slice(b"\" />");
        }
        x.extend_from_slice(b"</vertices><triangles>");
        for t in b.mesh.triangles.chunks_exact(3) {
            x.extend_from_slice(b"<triangle v1=\"");
            push_u32(&mut x, t[0]);
            x.extend_from_slice(b"\" v2=\"");
            push_u32(&mut x, t[1]);
            x.extend_from_slice(b"\" v3=\"");
            push_u32(&mut x, t[2]);
            x.extend_from_slice(b"\" />");
        }
        x.extend_from_slice(b"</triangles></mesh></object>");
    }
    x.extend_from_slice(format!("</resources><build p:UUID=\"{}\">", ids.v4()).as_bytes());
    for (i, b) in bodies.iter().enumerate() {
        x.extend_from_slice(format!(
            "<item xmlns=\"{NS_CORE}\" xmlns:m=\"{NS_M}\" xmlns:p=\"{NS_P}\" xmlns:b=\"{NS_B}\" xmlns:s=\"{NS_S}\" xmlns:sc=\"{NS_SC}\" objectid=\"{}\" transform=\"1.0 0.0 0.0 0.0 1.0 0.0 0.0 0.0 1.0 0.0 0.0 0.0\" p:UUID=\"{}\" partnumber=\"{}\"/>",
            i + 1,
            ids.v4(),
            esc(&b.name)
        ).as_bytes());
    }
    x.extend_from_slice(b"</build></model>");

    let io = |e: &dyn std::fmt::Display| ExportError::Io(e.to_string());
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let o = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, data) in [("3D/3dmodel.model", x.as_slice()), ("_rels/.rels", RELS.as_bytes()), ("[Content_Types].xml", CONTENT_TYPES.as_bytes())] {
        zip.start_file(name, o).map_err(|e| io(&e))?;
        zip.write_all(data).map_err(|e| io(&e))?;
    }
    Ok(zip.finish().map_err(|e| io(&e))?.into_inner())
}
