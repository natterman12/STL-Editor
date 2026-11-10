//! PLY reader matching trimesh 5.2.0 `exchange.ply.load_ply` for mesh fields
//! (xyz vertices + triangulated faces; extra properties are skipped).

use super::{Part, ReadError};

fn bad() -> ReadError {
    ReadError("Could not read this PLY file.".into())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Endian {
    Little,
    Big,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scalar {
    I8,
    U8,
    I16,
    U16,
    I32,
    U32,
    I64,
    U64,
    F32,
    F64,
}

impl Scalar {
    fn parse(name: &str) -> Result<Self, ReadError> {
        Ok(match name {
            "char" | "int8" => Self::I8,
            "uchar" | "uint8" => Self::U8,
            "short" | "int16" => Self::I16,
            "ushort" | "uint16" => Self::U16,
            "int" | "int32" => Self::I32,
            "uint" | "uint32" => Self::U32,
            "int64" => Self::I64,
            "uint64" => Self::U64,
            "float" | "float32" => Self::F32,
            "double" | "float64" => Self::F64,
            "float16" => Self::F32,
            other => return Err(ReadError(format!("Unsupported PLY type '{other}'."))),
        })
    }

    fn size(self) -> usize {
        match self {
            Self::I8 | Self::U8 => 1,
            Self::I16 | Self::U16 => 2,
            Self::I32 | Self::U32 | Self::F32 => 4,
            Self::I64 | Self::U64 | Self::F64 => 8,
        }
    }
}

#[derive(Clone, Debug)]
enum PropType {
    Scalar(Scalar),
    List { count: Scalar, item: Scalar },
}

#[derive(Clone, Debug)]
struct Property {
    name: String,
    kind: PropType,
}

#[derive(Clone, Debug)]
struct Element {
    name: String,
    count: usize,
    properties: Vec<Property>,
}

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

fn parse_header(data: &[u8]) -> Result<(Vec<Element>, bool, Endian, usize), ReadError> {
    let end_marker = b"end_header";
    let Some(pos) = data.windows(end_marker.len()).position(|w| w == end_marker) else {
        return Err(bad());
    };
    let mut body_at = pos + end_marker.len();
    if body_at < data.len() && (data[body_at] == b'\n' || data[body_at] == b'\r') {
        if data[body_at] == b'\r' && body_at + 1 < data.len() && data[body_at + 1] == b'\n' {
            body_at += 2;
        } else {
            body_at += 1;
        }
    }
    let header_txt = std::str::from_utf8(&data[..pos]).map_err(|_| bad())?;
    let mut lines = header_txt.lines();
    let first = lines.next().ok_or_else(bad)?;
    if !first.trim().eq_ignore_ascii_case("ply") {
        return Err(bad());
    }
    let fmt = lines.next().ok_or_else(bad)?.trim().to_ascii_lowercase();
    let (is_ascii, endian) = if fmt.contains("ascii") {
        (true, Endian::Little)
    } else if fmt.contains("binary_little_endian") {
        (false, Endian::Little)
    } else if fmt.contains("binary_big_endian") {
        (false, Endian::Big)
    } else {
        return Err(bad());
    };

    let mut elements: Vec<Element> = Vec::new();
    for line in lines {
        let line = line.trim();
        if line.is_empty() || line.starts_with("comment") || line.starts_with("obj_info") {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }
        match parts[0] {
            "element" => {
                if parts.len() < 3 {
                    return Err(bad());
                }
                elements.push(Element {
                    name: parts[1].to_string(),
                    count: parts[2].parse().map_err(|_| bad())?,
                    properties: Vec::new(),
                });
            }
            "property" => {
                let el = elements.last_mut().ok_or_else(bad)?;
                if parts.len() == 3 {
                    el.properties.push(Property {
                        name: parts[2].to_string(),
                        kind: PropType::Scalar(Scalar::parse(parts[1])?),
                    });
                } else if parts.len() == 5 && parts[1] == "list" {
                    el.properties.push(Property {
                        name: parts[4].to_string(),
                        kind: PropType::List {
                            count: Scalar::parse(parts[2])?,
                            item: Scalar::parse(parts[3])?,
                        },
                    });
                } else {
                    return Err(bad());
                }
            }
            _ => {}
        }
    }
    Ok((elements, is_ascii, endian, body_at))
}

fn read_ascii_num(tok: &str) -> Result<f64, ReadError> {
    tok.parse().map_err(|_| bad())
}

fn read_scalar_bin(data: &[u8], at: &mut usize, ty: Scalar, endian: Endian) -> Result<f64, ReadError> {
    let n = ty.size();
    if *at + n > data.len() {
        return Err(bad());
    }
    let b = &data[*at..*at + n];
    *at += n;
    Ok(match (ty, endian) {
        (Scalar::I8, _) => i8::from_le_bytes([b[0]]) as f64,
        (Scalar::U8, _) => b[0] as f64,
        (Scalar::I16, Endian::Little) => i16::from_le_bytes([b[0], b[1]]) as f64,
        (Scalar::I16, Endian::Big) => i16::from_be_bytes([b[0], b[1]]) as f64,
        (Scalar::U16, Endian::Little) => u16::from_le_bytes([b[0], b[1]]) as f64,
        (Scalar::U16, Endian::Big) => u16::from_be_bytes([b[0], b[1]]) as f64,
        (Scalar::I32, Endian::Little) => i32::from_le_bytes(b.try_into().unwrap()) as f64,
        (Scalar::I32, Endian::Big) => i32::from_be_bytes(b.try_into().unwrap()) as f64,
        (Scalar::U32, Endian::Little) => u32::from_le_bytes(b.try_into().unwrap()) as f64,
        (Scalar::U32, Endian::Big) => u32::from_be_bytes(b.try_into().unwrap()) as f64,
        (Scalar::I64, Endian::Little) => i64::from_le_bytes(b.try_into().unwrap()) as f64,
        (Scalar::I64, Endian::Big) => i64::from_be_bytes(b.try_into().unwrap()) as f64,
        (Scalar::U64, Endian::Little) => u64::from_le_bytes(b.try_into().unwrap()) as f64,
        (Scalar::U64, Endian::Big) => u64::from_be_bytes(b.try_into().unwrap()) as f64,
        (Scalar::F32, Endian::Little) => f32::from_le_bytes(b.try_into().unwrap()) as f64,
        (Scalar::F32, Endian::Big) => f32::from_be_bytes(b.try_into().unwrap()) as f64,
        (Scalar::F64, Endian::Little) => f64::from_le_bytes(b.try_into().unwrap()),
        (Scalar::F64, Endian::Big) => f64::from_be_bytes(b.try_into().unwrap()),
    })
}

fn face_index_name(props: &[Property]) -> Option<&str> {
    for n in ["vertex_indices", "vertex_index"] {
        if props.iter().any(|p| p.name == n) {
            return Some(n);
        }
    }
    None
}

fn take_list_ascii<'a>(row: &[&'a str], at: &mut usize) -> Result<Vec<&'a str>, ReadError> {
    let n: usize = row.get(*at).ok_or_else(bad)?.parse().map_err(|_| bad())?;
    *at = at.checked_add(1).ok_or_else(bad)?;
    let end = at.checked_add(n).ok_or_else(bad)?;
    if end > row.len() {
        return Err(bad());
    }
    let items = row[*at..end].to_vec();
    *at = end;
    Ok(items)
}

/// Resolve a PLY face index: negatives wrap from the end (numpy-style); out of range → error.
fn resolve_face_idx(v: i64, nverts: usize) -> Result<u32, ReadError> {
    let i = if v < 0 { nverts as i64 + v } else { v };
    if i < 0 || (i as usize) >= nverts {
        return Err(bad());
    }
    Ok(i as u32)
}

fn list_count_bounded(n: usize, remaining_bytes: usize, item_size: usize) -> Result<usize, ReadError> {
    if item_size == 0 {
        return Err(bad());
    }
    let max = remaining_bytes / item_size;
    if n > max {
        return Err(bad());
    }
    Ok(n)
}

/// See the contract in `read/mod.rs`.
pub fn read(data: &[u8], base: &str) -> Result<Vec<Part>, ReadError> {
    let (elements, is_ascii, endian, body_at) = parse_header(data)?;
    let body = &data[body_at..];
    let nverts_hdr = elements
        .iter()
        .find(|e| e.name == "vertex")
        .map(|e| e.count)
        .unwrap_or(0);

    let mut positions: Vec<f64> = Vec::new();
    let mut triangles: Vec<u32> = Vec::new();

    if is_ascii {
        let text = std::str::from_utf8(body).map_err(|_| bad())?;
        let mut rows = text
            .lines()
            .map(str::trim)
            .filter(|s| !s.is_empty());

        for el in &elements {
            match el.name.as_str() {
                "vertex" => {
                    for _ in 0..el.count {
                        let line = rows.next().ok_or_else(bad)?;
                        let row: Vec<&str> = line.split_whitespace().collect();
                        let mut at = 0usize;
                        let mut x = None;
                        let mut y = None;
                        let mut z = None;
                        for prop in &el.properties {
                            match &prop.kind {
                                PropType::Scalar(_) => {
                                    let t = *row.get(at).ok_or_else(bad)?;
                                    let v = read_ascii_num(t)?;
                                    at += 1;
                                    match prop.name.as_str() {
                                        "x" => x = Some(v),
                                        "y" => y = Some(v),
                                        "z" => z = Some(v),
                                        _ => {}
                                    }
                                }
                                PropType::List { .. } => {
                                    let _ = take_list_ascii(&row, &mut at)?;
                                }
                            }
                        }
                        positions.push(x.ok_or_else(bad)?);
                        positions.push(y.ok_or_else(bad)?);
                        positions.push(z.ok_or_else(bad)?);
                    }
                }
                "face" => {
                    let idx_name = face_index_name(&el.properties).ok_or_else(bad)?;
                    for _ in 0..el.count {
                        let line = rows.next().ok_or_else(bad)?;
                        let row: Vec<&str> = line.split_whitespace().collect();
                        let mut at = 0usize;
                        let mut face_idx: Option<Vec<u32>> = None;
                        for prop in &el.properties {
                            match &prop.kind {
                                PropType::Scalar(_) => {
                                    let _ = row.get(at).ok_or_else(bad)?;
                                    at += 1;
                                }
                                PropType::List { .. } => {
                                    let items = take_list_ascii(&row, &mut at)?;
                                    if prop.name == idx_name {
                                        let mut idxs = Vec::with_capacity(items.len());
                                        for t in items {
                                            let v = read_ascii_num(t)? as i64;
                                            idxs.push(resolve_face_idx(v, nverts_hdr)?);
                                        }
                                        face_idx = Some(idxs);
                                    }
                                }
                            }
                        }
                        triangulate(&face_idx.ok_or_else(bad)?, &mut triangles);
                    }
                }
                _ => {
                    for _ in 0..el.count {
                        let _ = rows.next().ok_or_else(bad)?;
                    }
                }
            }
        }
    } else {
        let mut at = 0usize;
        for el in &elements {
            match el.name.as_str() {
                "vertex" => {
                    for _ in 0..el.count {
                        let mut x = None;
                        let mut y = None;
                        let mut z = None;
                        for prop in &el.properties {
                            match &prop.kind {
                                PropType::Scalar(ty) => {
                                    let v = read_scalar_bin(body, &mut at, *ty, endian)?;
                                    match prop.name.as_str() {
                                        "x" => x = Some(v),
                                        "y" => y = Some(v),
                                        "z" => z = Some(v),
                                        _ => {}
                                    }
                                }
                                PropType::List { count, item } => {
                                    let n = read_scalar_bin(body, &mut at, *count, endian)? as usize;
                                    let n = list_count_bounded(
                                        n,
                                        body.len().saturating_sub(at),
                                        item.size(),
                                    )?;
                                    for _ in 0..n {
                                        let _ = read_scalar_bin(body, &mut at, *item, endian)?;
                                    }
                                }
                            }
                        }
                        positions.push(x.ok_or_else(bad)?);
                        positions.push(y.ok_or_else(bad)?);
                        positions.push(z.ok_or_else(bad)?);
                    }
                }
                "face" => {
                    let idx_name = face_index_name(&el.properties).ok_or_else(bad)?;
                    for _ in 0..el.count {
                        let mut face_idx: Option<Vec<u32>> = None;
                        for prop in &el.properties {
                            match &prop.kind {
                                PropType::Scalar(ty) => {
                                    let _ = read_scalar_bin(body, &mut at, *ty, endian)?;
                                }
                                PropType::List { count, item } => {
                                    let n = read_scalar_bin(body, &mut at, *count, endian)? as usize;
                                    let n = list_count_bounded(
                                        n,
                                        body.len().saturating_sub(at),
                                        item.size(),
                                    )?;
                                    let mut idxs = Vec::with_capacity(n);
                                    for _ in 0..n {
                                        let v = read_scalar_bin(body, &mut at, *item, endian)? as i64;
                                        idxs.push(resolve_face_idx(v, nverts_hdr)?);
                                    }
                                    if prop.name == idx_name {
                                        face_idx = Some(idxs);
                                    }
                                }
                            }
                        }
                        triangulate(&face_idx.ok_or_else(bad)?, &mut triangles);
                    }
                }
                _ => {
                    for _ in 0..el.count {
                        for prop in &el.properties {
                            match &prop.kind {
                                PropType::Scalar(ty) => {
                                    let _ = read_scalar_bin(body, &mut at, *ty, endian)?;
                                }
                                PropType::List { count, item } => {
                                    let n = read_scalar_bin(body, &mut at, *count, endian)? as usize;
                                    let n = list_count_bounded(
                                        n,
                                        body.len().saturating_sub(at),
                                        item.size(),
                                    )?;
                                    for _ in 0..n {
                                        let _ = read_scalar_bin(body, &mut at, *item, endian)?;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if positions.is_empty() {
        return Ok(Vec::new());
    }
    Ok(vec![Part { name: base.to_string(), positions, triangles }])
}
