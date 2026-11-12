//! Python number text: `repr(float)` (what trimesh's 3MF writer prints) and `"%.8f"` (its OBJ writer).

/// Fixed-size text buffer, so formatting a number does not allocate.
struct Stack {
    b: [u8; 40],
    n: usize,
}

impl std::fmt::Write for Stack {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        let e = self.n + s.len();
        self.b.get_mut(self.n..e).ok_or(std::fmt::Error)?.copy_from_slice(s.as_bytes());
        self.n = e;
        Ok(())
    }
}

impl Stack {
    fn new() -> Stack {
        Stack { b: [0; 40], n: 0 }
    }
    fn bytes(&self) -> &[u8] {
        &self.b[..self.n]
    }
}

/// Split `d.ddde-5` into its digits (written to `out`) and exponent; returns (digit count, exp).
fn split_sci(sci: &[u8], out: &mut [u8; 24]) -> (usize, i32) {
    let mut k = 0;
    let mut i = 0;
    while sci[i] != b'e' {
        if sci[i] != b'.' {
            out[k] = sci[i];
            k += 1;
        }
        i += 1;
    }
    i += 1;
    let neg = sci[i] == b'-';
    if neg {
        i += 1;
    }
    let mut e = 0i32;
    for c in &sci[i..] {
        e = e * 10 + (c - b'0') as i32;
    }
    (k, if neg { -e } else { e })
}

/// Digits and decimal exponent (`d.ddd` x 10^exp) into `out`; returns (digit count, exp).
/// They are the ones that CPython's `repr` and V8's
/// `Number.toString` pick: among the shortest digit strings that read back as `x`, the correctly
/// rounded one (exact ties to even). Rust's `{:e}` finds the right length but not always the
/// right last digit, so the length comes from it and the digits from the exact fixed-precision
/// `{:.*e}`. `x` must be finite and positive.
pub(crate) fn shortest_digits_buf(x: f64, out: &mut [u8; 24]) -> (usize, i32) {
    use std::fmt::Write;
    let mut s = Stack::new();
    write!(s, "{x:e}").expect("fits");
    let (k, exp) = split_sci(s.bytes(), out);
    // The exact k-digit rendering. When it is the same text, it is the answer (and reads back as x,
    // since the shortest does); otherwise it is used only if it still reads back as x.
    let mut t = Stack::new();
    write!(t, "{:.*e}", k - 1, x).expect("fits");
    let mut alt = [0u8; 24];
    let (k2, exp2) = split_sci(t.bytes(), &mut alt);
    if k2 == k && (exp2 != exp || alt[..k] != out[..k]) && std::str::from_utf8(t.bytes()).ok().and_then(|v| v.parse::<f64>().ok()) == Some(x) {
        out[..k].copy_from_slice(&alt[..k]);
        return (k, exp2);
    }
    (k, exp)
}

/// Append `repr(float)` of `x` to `o`.
pub fn py_float_repr_into(o: &mut Vec<u8>, x: f64) {
    if x.is_nan() {
        o.extend_from_slice(b"nan");
        return;
    }
    if x.is_infinite() {
        o.extend_from_slice(if x < 0.0 { b"-inf" } else { b"inf" });
        return;
    }
    if x.is_sign_negative() {
        o.push(b'-');
    }
    if x == 0.0 {
        o.extend_from_slice(b"0.0");
        return;
    }
    let mut d = [0u8; 24];
    let (n, exp) = shortest_digits_buf(x.abs(), &mut d);
    let digits = &d[..n];
    let n = n as i32;
    if (-4..16).contains(&exp) {
        if exp >= 0 {
            let int_len = exp + 1;
            if n <= int_len {
                o.extend_from_slice(digits);
                o.extend(std::iter::repeat_n(b'0', (int_len - n) as usize));
                o.extend_from_slice(b".0");
            } else {
                o.extend_from_slice(&digits[..int_len as usize]);
                o.push(b'.');
                o.extend_from_slice(&digits[int_len as usize..]);
            }
        } else {
            o.extend_from_slice(b"0.");
            o.extend(std::iter::repeat_n(b'0', (-exp - 1) as usize));
            o.extend_from_slice(digits);
        }
    } else {
        o.push(digits[0]);
        if n > 1 {
            o.push(b'.');
            o.extend_from_slice(&digits[1..]);
        }
        o.push(b'e');
        o.push(if exp < 0 { b'-' } else { b'+' });
        let a = exp.abs();
        if a < 10 {
            o.push(b'0');
        }
        o.extend_from_slice(a.to_string().as_bytes());
    }
}

/// Python's `repr(float)`: the shortest digits that round-trip, in fixed notation for decimal
/// exponents from -4 to 15 and in exponent form (`9.999999747378752e-06`, `1e+16`) otherwise.
pub fn py_float_repr(x: f64) -> String {
    let mut o = Vec::with_capacity(24);
    py_float_repr_into(&mut o, x);
    String::from_utf8(o).expect("ascii")
}

/// C's `"%.8f"`, which keeps the sign of `-0.0`.
pub fn c_fixed8(x: f64) -> String {
    format!("{x:.8}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn py_float_repr_cases() {
        assert_eq!(py_float_repr(0.1f32 as f64), "0.10000000149011612");
        assert_eq!(py_float_repr(123456789f32 as f64), "123456792.0");
        assert_eq!(py_float_repr(-0.0), "-0.0");
        assert_eq!(py_float_repr(1e-5f32 as f64), "9.999999747378752e-06");
        assert_eq!(py_float_repr(1e16), "1e+16");
        assert_eq!(py_float_repr(5.0), "5.0");
        assert_eq!(py_float_repr(0.0001), "0.0001");
        assert_eq!(py_float_repr(1e15), "1000000000000000.0");
        assert_eq!(py_float_repr(1.5e-7), "1.5e-07");
        assert_eq!(py_float_repr(-12.25), "-12.25");
    }

    #[test]
    fn last_digit_matches_cpython() {
        assert_eq!(py_float_repr(-845.7361450195312), "-845.7361450195312");
        assert_eq!(py_float_repr(2.0f64.powi(-25)), "2.9802322387695312e-08");
    }

    /// Lines of `bits-hex python-repr js-string`, made with CPython 3.14 and Node (seed 20261111).
    #[test]
    fn corpus_matches_cpython_and_v8() {
        let mut bad = Vec::new();
        let mut rows = 0;
        for line in include_str!("fixtures/float_corpus.txt").lines() {
            let mut it = line.split(' ');
            let x = f64::from_bits(u64::from_str_radix(it.next().unwrap(), 16).unwrap());
            let (py, js) = (it.next().unwrap(), it.next().unwrap());
            rows += 1;
            if py_float_repr(x) != py {
                bad.push(format!("py {x:?}: got {} want {py}", py_float_repr(x)));
            }
            if super::super::jsfmt::js_number_string(x) != js {
                bad.push(format!("js {x:?}: got {} want {js}", super::super::jsfmt::js_number_string(x)));
            }
        }
        assert!(rows > 1500);
        assert!(bad.is_empty(), "{} mismatches, first: {:?}", bad.len(), &bad[..bad.len().min(5)]);
    }

    #[test]
    fn c_fixed8_cases() {
        assert_eq!(c_fixed8(-0.0), "-0.00000000");
        assert_eq!(c_fixed8(123456792.0), "123456792.00000000");
    }
}
