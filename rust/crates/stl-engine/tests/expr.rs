use serde_json::Value;
use std::collections::BTreeMap;
use stl_engine::params::*;

fn load(name: &str) -> Value {
    let p = format!("{}/../../tests/parity/{name}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-12 * a.abs().max(b.abs()) || (a - b).abs() <= 1e-12
}

fn json_close(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => close(x.as_f64().unwrap(), y.as_f64().unwrap()),
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| json_close(p, q))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| json_close(v, w)))
        }
        _ => a == b,
    }
}

fn src_of(text: &str) -> String {
    text.trim().trim_start_matches('=').to_string()
}

fn vars_of(v: Option<&Value>) -> BTreeMap<String, f64> {
    v.and_then(|v| v.as_object())
        .map(|o| o.iter().map(|(k, v)| (k.clone(), v.as_f64().unwrap())).collect())
        .unwrap_or_default()
}

fn check_case(case: &Value, want: &Value) -> Result<(), String> {
    let text = case["text"].as_str().unwrap_or("");
    match case["kind"].as_str().unwrap() {
        "evaluate" => {
            let got = evaluate(text, &vars_of(case.get("vars")));
            if let Some(v) = want.get("value") {
                let v = v.as_f64().unwrap();
                match got {
                    Ok(g) if close(g, v) => Ok(()),
                    other => Err(format!("want {v}, got {other:?}")),
                }
            } else if let Some(m) = want.get("error") {
                match got {
                    Err(e) if e.0 == m.as_str().unwrap() => Ok(()),
                    other => Err(format!("want error {m}, got {other:?}")),
                }
            } else if let Some(x) = want.get("python_exception") {
                let msg = match x.as_str().unwrap() {
                    "ZeroDivisionError" => format!("{} doesn't give a number", src_of(text)),
                    "RecursionError" => format!("{} is nested too deeply", src_of(text)),
                    o => return Err(format!("unknown python exception {o}")),
                };
                match got {
                    Err(e) if e.0 == msg => Ok(()),
                    other => Err(format!("want error {msg:?}, got {other:?}")),
                }
            } else {
                Err(format!("unhandled golden record {want}"))
            }
        }
        "resolve" => {
            let list: Vec<Param> = serde_json::from_value(case["params"].clone()).unwrap();
            let r = resolve_params(&list);
            let wv = want["values"].as_object().unwrap();
            let we = want["errors"].as_object().unwrap();
            if r.values.len() != wv.len() || r.errors.len() != we.len() {
                return Err(format!("want {want}, got {r:?}"));
            }
            for (k, v) in wv {
                match r.values.get(k) {
                    Some(g) if close(*g, v.as_f64().unwrap()) => {}
                    g => return Err(format!("value {k}: want {v}, got {g:?}")),
                }
            }
            for (k, m) in we {
                if r.errors.get(k).map(String::as_str) != m.as_str() {
                    return Err(format!("error {k}: want {m}, got {:?}", r.errors.get(k)));
                }
            }
            Ok(())
        }
        "names_in" => {
            let got: Vec<String> = names_in(text).into_iter().collect();
            let w: Vec<String> = want["names"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_string())
                .collect();
            if got == w { Ok(()) } else { Err(format!("want {w:?}, got {got:?}")) }
        }
        "is_valid_name" => {
            let w = want["valid"].as_bool().unwrap();
            if is_valid_name(text) == w { Ok(()) } else { Err(format!("want {w}")) }
        }
        "resolve_deep" => {
            let got = resolve_deep(&case["obj"], &vars_of(case.get("values")));
            if let Some(w) = want.get("value") {
                match got {
                    Ok(g) if json_close(&g, w) => Ok(()),
                    other => Err(format!("want {w}, got {other:?}")),
                }
            } else if let Some(m) = want.get("error") {
                match got {
                    Err(e) if e.0 == m.as_str().unwrap() => Ok(()),
                    other => Err(format!("want error {m}, got {other:?}")),
                }
            } else {
                Err(format!("unhandled golden record {want}"))
            }
        }
        "uses_expr" => {
            let w = want["uses"].as_bool().unwrap();
            if uses_expr(&case["obj"]) == w { Ok(()) } else { Err(format!("want {w}")) }
        }
        k => Err(format!("unknown kind {k}")),
    }
}

#[test]
fn matches_golden() {
    let cases = load("expr_cases.json");
    let golden = load("expr_golden.json");
    let mut fails = Vec::new();
    for case in cases.as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        if let Err(e) = check_case(case, &golden[name]) {
            fails.push(format!("{name}: {e}"));
        }
    }
    assert!(fails.is_empty(), "{} golden mismatches:\n{}", fails.len(), fails.join("\n"));
}

fn eval(t: &str) -> Result<f64, ExprError> {
    evaluate(t, &BTreeMap::new())
}

#[test]
fn nan_inside_function_is_an_error() {
    assert_eq!(
        eval("max(sqrt(-1), 3)").unwrap_err().0,
        "max(sqrt(-1), 3) doesn't give a number"
    );
}

#[test]
fn zero_to_negative_power() {
    assert_eq!(eval("0^-1").unwrap_err().0, "0^-1 doesn't give a number");
}

#[test]
fn deep_nesting() {
    let t = format!("{}1{}", "(".repeat(500), ")".repeat(500));
    assert_eq!(eval(&t).unwrap_err().0, format!("{t} is nested too deeply"));
    let ok = format!("{}1{}", "(".repeat(150), ")".repeat(150));
    assert_eq!(eval(&ok).unwrap(), 1.0);
}

fn param(name: &str, expr: &str) -> Param {
    Param { name: name.into(), expr: expr.into(), note: String::new() }
}

#[test]
fn resolve_walks_list_order() {
    // Python visits `b` first, so the cycle is reported from `b`'s side.
    let r = resolve_params(&[param("b", "=a"), param("a", "=b")]);
    assert_eq!(r.errors["a"], "\"b\" depends on itself");
    assert_eq!(r.errors["b"], "\"b\" depends on itself");
    let r = resolve_params(&[param("a", "=b"), param("b", "=a")]);
    assert_eq!(r.errors["a"], "\"a\" depends on itself");
    assert_eq!(r.errors["b"], "\"a\" depends on itself");
}

#[test]
fn unicode_digits_and_python_whitespace() {
    assert_eq!(eval("\u{663}+1").unwrap(), 4.0);
    assert_eq!(eval("\u{966}.\u{967}e\u{968}").unwrap(), 10.0);
    assert_eq!(eval("1\u{a0}+\u{1c}2\u{85}").unwrap(), 3.0);
    assert_eq!(eval("\u{a0}=\u{1f}5\u{3000}").unwrap(), 5.0);
    assert!(is_expr(&Value::String("\u{1c}=1".into())));
}

#[test]
fn floor_ceil_round_have_no_negative_zero() {
    for t in ["ceil(-0.5)", "floor(-0)", "round(-0.5)", "round(-0.0)", "ceil(-0.0)"] {
        let v = eval(t).unwrap();
        assert!(v == 0.0 && v.is_sign_positive(), "{t} gave {v:?}");
    }
}

#[test]
fn long_parameter_chain_does_not_overflow() {
    let n = 5000;
    let mut list = vec![param("p0", "=1")];
    for i in 1..n {
        list.push(param(&format!("p{i}"), &format!("=p{}+1", i - 1)));
    }
    // reversed: the first parameter visited needs the whole chain at once
    list.reverse();
    let r = resolve_params(&list);
    assert!(r.errors.is_empty());
    assert_eq!(r.values[&format!("p{}", n - 1)], n as f64);
    // a cycle through a long chain still ends cleanly
    let mut list = vec![param("p0", &format!("=p{}", n - 1))];
    for i in 1..n {
        list.push(param(&format!("p{i}"), &format!("=p{}+1", i - 1)));
    }
    let r = resolve_params(&list);
    assert_eq!(r.errors.len(), n);
    assert_eq!(r.errors["p0"], "\"p0\" depends on itself");
}

#[test]
fn long_unary_and_power_chains_do_not_overflow() {
    assert_eq!(eval(&format!("{}1", "-".repeat(5000))).unwrap(), 1.0);
    assert_eq!(eval(&format!("{}1", "-".repeat(5001))).unwrap(), -1.0);
    assert_eq!(eval(&format!("{}1", "+-".repeat(3000))).unwrap(), 1.0);
    assert_eq!(eval(&format!("1{}", "^1".repeat(5000))).unwrap(), 1.0);
    assert_eq!(eval("2^3^2").unwrap(), 512.0);
    assert_eq!(eval("-2^2").unwrap(), 4.0);
}

#[test]
fn exact_long_chain_cases() {
    assert_eq!(eval(&format!("{}5", "-".repeat(5000))).unwrap(), 5.0);
    assert_eq!(eval(&format!("{}5", "-".repeat(5001))).unwrap(), -5.0);
    assert_eq!(eval(&format!("2{}", "^1".repeat(5000))).unwrap(), 2.0);
    let mut list = vec![param("p0", "1")];
    for i in 1..300 {
        list.push(param(&format!("p{i}"), &format!("=p{}+1", i - 1)));
    }
    let r = resolve_params(&list);
    assert!(r.errors.is_empty());
    assert_eq!(r.values["p299"], 300.0);
    list.reverse();
    let r = resolve_params(&list);
    assert!(r.errors.is_empty());
    assert_eq!(r.values["p299"], 300.0);
}
