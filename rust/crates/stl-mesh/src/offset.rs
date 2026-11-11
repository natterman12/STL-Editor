//! Grow or shrink a whole body (1.x `meshtools.offset_solid`): a Minkowski sum or difference
//! with a small sphere.

use stl_kernel::Solid;

use crate::MeshError;

/// Python's `f"{x:g}"`.
fn fmt_g(x: f64) -> String {
    if x.is_nan() {
        return "nan".into();
    }
    if x.is_infinite() {
        return if x < 0.0 { "-inf" } else { "inf" }.into();
    }
    if x == 0.0 {
        return "0".into();
    }
    let e = format!("{:.5e}", x);
    let (mant, exp) = e.split_once('e').unwrap();
    let exp: i32 = exp.parse().unwrap();
    if exp < -4 || exp >= 6 {
        let mant = if mant.contains('.') { mant.trim_end_matches('0').trim_end_matches('.') } else { mant };
        return format!("{}e{}{:02}", mant, if exp < 0 { '-' } else { '+' }, exp.abs());
    }
    let s = format!("{:.*}", (5 - exp).max(0) as usize, x);
    if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s }
}

/// Grow (`distance` > 0) or shrink (< 0) by `distance` in every direction. Bodies over
/// `max_tris` triangles are simplified first, within at most 40 % of the distance; growing adds
/// that back so the offset is never less than `distance`.
pub fn offset_solid(s: &Solid, distance: f64, max_tris: usize) -> Result<Solid, MeshError> {
    let d = distance;
    if d.abs() < 1e-6 {
        return Ok(s.clone());
    }
    let mut work = s.clone();
    let mut slack = 0.0;
    if work.num_tri() > max_tris {
        for frac in [0.1, 0.2, 0.3, 0.4] {
            slack = d.abs() * frac;
            work = s.simplify(slack);
            if work.num_tri() <= max_tris {
                break;
            }
        }
    }
    // a 32-triangle sphere sits inside the true sphere, so scale it up to keep the full distance
    let segs = 8u32;
    let r = d.abs() / (std::f64::consts::PI / segs as f64).cos() + slack;
    let ball = Solid::sphere(r, segs);
    if d > 0.0 {
        let out = work.minkowski_sum(&ball);
        return Ok(out.union(s));
    }
    let out = work.minkowski_difference(&ball);
    if out.is_empty() {
        return Err(MeshError::Message(format!("Shrinking by {} mm leaves nothing of this body.", fmt_g(d.abs()))));
    }
    Ok(out.intersection(s))
}

#[cfg(test)]
mod tests {
    use super::fmt_g;

    #[test]
    fn g_format() {
        assert_eq!(fmt_g(5.0), "5");
        assert_eq!(fmt_g(0.5), "0.5");
        assert_eq!(fmt_g(0.0001), "0.0001");
        assert_eq!(fmt_g(0.00001), "1e-05");
        assert_eq!(fmt_g(1234567.0), "1.23457e+06");
        assert_eq!(fmt_g(2.5), "2.5");
        assert_eq!(fmt_g(f64::INFINITY), "inf");
        assert_eq!(fmt_g(f64::NAN), "nan");
    }
}
