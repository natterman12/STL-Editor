//! JavaScript number formatting and `Math.hypot`, for the STEP writer (`web/engine/step.js`).

/// JavaScript `String(x)` for a number: the shortest digits that read back as `x`, in plain
/// decimal for 1e-7 < |x| < 1e21 and in exponent form otherwise.
pub fn js_number_string(x: f64) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x == 0.0 {
        return "0".into();
    }
    if x.is_infinite() {
        return if x < 0.0 { "-Infinity".into() } else { "Infinity".into() };
    }
    let mut d = [0u8; 24];
    let (k, exp) = super::pyfmt::shortest_digits_buf(x.abs(), &mut d);
    // digits are ASCII
    let digits = std::str::from_utf8(&d[..k]).expect("ascii");
    let k = k as i32;
    let n = exp + 1; // value = 0.digits * 10^n
    let mut o = String::with_capacity(32);
    if x < 0.0 {
        o.push('-');
    }
    if k <= n && n <= 21 {
        o.push_str(digits);
        o.extend(std::iter::repeat_n('0', (n - k) as usize));
    } else if 0 < n && n <= 21 {
        o.push_str(&digits[..n as usize]);
        o.push('.');
        o.push_str(&digits[n as usize..]);
    } else if -6 < n && n <= 0 {
        o.push_str("0.");
        o.extend(std::iter::repeat_n('0', (-n) as usize));
        o.push_str(digits);
    } else {
        let e = n - 1;
        o.push_str(&digits[..1]);
        if k > 1 {
            o.push('.');
            o.push_str(&digits[1..]);
        }
        o.push('e');
        o.push(if e < 0 { '-' } else { '+' });
        o.push_str(&e.abs().to_string());
    }
    o
}

/// V8's `Math.hypot(a, b, c)`: scale by the largest magnitude, sum the squares with
/// Kahan compensation, take the square root and scale back.
pub fn js_hypot3(v: [f64; 3]) -> f64 {
    if v.iter().any(|x| x.is_infinite()) {
        return f64::INFINITY;
    }
    if v.iter().any(|x| x.is_nan()) {
        return f64::NAN;
    }
    let max = v.iter().fold(0.0f64, |m, x| m.max(x.abs()));
    if max == 0.0 {
        return 0.0;
    }
    let (mut sum, mut comp) = (0.0f64, 0.0f64);
    for x in v {
        let n = x.abs() / max;
        let summand = n * n - comp;
        let prelim = sum + summand;
        comp = (prelim - sum) - summand;
        sum = prelim;
    }
    sum.sqrt() * max
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn number_strings_match_javascript() {
        let cases: &[(f64, &str)] = &[
            (0.0, "0"),
            (-0.0, "0"),
            (1.0, "1"),
            (-1.5, "-1.5"),
            (0.1, "0.1"),
            (0.1 + 0.2, "0.30000000000000004"),
            (123456792.0, "123456792"),
            (1e21, "1e+21"),
            (1e20, "100000000000000000000"),
            (1.5e-6, "0.0000015"),
            (1e-6, "0.000001"),
            (1e-7, "1e-7"),
            (9.999999747378752e-6, "0.000009999999747378752"),
            (1.2345e-9, "1.2345e-9"),
            (1.5e300, "1.5e+300"),
            (100.0, "100"),
            (0.5, "0.5"),
            (f64::NAN, "NaN"),
        ];
        for (x, want) in cases {
            assert_eq!(js_number_string(*x), *want, "{x:?}");
        }
    }

    #[test]
    fn last_digit_matches_v8() {
        assert_eq!(js_number_string(2.0f64.powi(-25)), "2.9802322387695312e-8");
        assert_eq!(js_number_string(-845.7361450195312), "-845.7361450195312");
    }

    #[test]
    fn hypot_matches_javascript() {
        assert_eq!(js_hypot3([0.0, 0.0, 0.0]), 0.0);
        assert_eq!(js_hypot3([3.0, 4.0, 0.0]), 5.0);
        assert_eq!(js_hypot3([1.0, 1.0, 1.0]), 1.7320508075688772);
        assert!(js_hypot3([f64::NAN, 1.0, 1.0]).is_nan());
    }
}
