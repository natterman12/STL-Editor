//! The voxel grid size Hollow, Shell and Lattice use (1.x `_hollow_pitch`), and Python's
//! `.3g` number format for Repair's warning.

use stl_kernel::Solid;

/// 1.x `_hollow_pitch`: from the solid's own bounding box at full precision. `q` is the
/// `resolution` setting (`draft`, `normal` or `fine`; anything else counts as normal).
pub fn hollow_pitch(s: &Solid, t: f64, q: Option<&str>) -> f64 {
    let bb = s.bounding_box();
    let d: [f64; 3] = std::array::from_fn(|i| bb.max[i] - bb.min[i]);
    let diag = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    let div = match q.filter(|x| !x.is_empty()).unwrap_or("normal") {
        "draft" => 80.0,
        "fine" => 260.0,
        _ => 150.0,
    };
    (t / 2.5).min(diag / div).max(diag / (div * 2.5))
}

/// Python `f"{x:.3g}"`.
pub fn fmt_g3(x: f64) -> String {
    if x == 0.0 {
        return "0".into();
    }
    if !x.is_finite() {
        return if x.is_nan() { "nan".into() } else if x < 0.0 { "-inf".into() } else { "inf".into() };
    }
    let e = format!("{:.2e}", x);
    let (mant, exp) = e.split_once('e').unwrap();
    let exp: i32 = exp.parse().unwrap();
    if exp < -4 || exp >= 3 {
        let mant = if mant.contains('.') { mant.trim_end_matches('0').trim_end_matches('.') } else { mant };
        return format!("{}e{}{:02}", mant, if exp < 0 { '-' } else { '+' }, exp.abs());
    }
    let s = format!("{:.*}", (2 - exp).max(0) as usize, x);
    if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn g3() {
        assert_eq!(fmt_g3(0.5), "0.5");
        assert_eq!(fmt_g3(0.4123456), "0.412");
        assert_eq!(fmt_g3(1.0), "1");
        assert_eq!(fmt_g3(12.3456), "12.3");
        assert_eq!(fmt_g3(123.456), "123");
        assert_eq!(fmt_g3(999.9), "1e+03");
        assert_eq!(fmt_g3(1234.5), "1.23e+03");
        assert_eq!(fmt_g3(0.00001234), "1.23e-05");
        assert_eq!(fmt_g3(0.0001), "0.0001");
    }
}
