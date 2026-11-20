//! Property tests for expressions and parameters: thousands of generated inputs, right and
//! wrong, and rules that must hold for all of them. A failure prints the smallest input that
//! breaks the rule and saves it beside this file (`prop_expr.proptest-regressions`, to commit),
//! so it is tried first on every later run.
#![cfg(not(target_arch = "wasm32"))]

use proptest::prelude::*;
use proptest::sample::{Index, select};
use proptest::test_runner::FileFailurePersistence;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use stl_engine::params::*;

const NUMS: &[&str] = &[
    "0",
    "1",
    "2",
    "3",
    "7",
    "10",
    "2.5",
    ".5",
    "3.",
    "1e3",
    "1E-2",
    "2.e3",
    "007",
    "0.1",
    "90",
    "360",
    "1e400",
    "1e-400",
    "5e-324",
    "1e",
    "1..2",
    "0x10",
    "1_000",
    "\u{661}\u{662}",
    "\u{ff11}\u{ff12}",
];
const NAMES: &[&str] = &[
    "w", "h", "width", "_a", "n", "big", "tiny", "pi", "nope", "W", "sqrt", "e",
];
const FUNCS: &[&str] = &[
    "sqrt", "abs", "min", "max", "round", "floor", "ceil", "sin", "cos", "tan", "asin", "acos", "atan", "log", "w",
];
const OPS: &[&str] = &["+", "-", "*", "/", "%", "^", " + ", " * ", "\t/\n", "\u{a0}-\u{2003}"];
const SIGNS: &[&str] = &["-", "+", "--", "-+", "- "];
const JUNK: &[&str] = &[
    "(", ")", "+", "*", "/", "%", "^", ",", ".", "=", "e", "$", " ", "#", "\u{e9}", "\u{b2}", "\u{1c}", "\n",
];

/// Failing inputs are saved beside this file (the path is from the crate folder, where Cargo runs tests).
fn config(cases: u32) -> ProptestConfig {
    ProptestConfig {
        cases,
        failure_persistence: Some(Box::new(FileFailurePersistence::Direct(
            "tests/prop_expr.proptest-regressions",
        ))),
        ..ProptestConfig::default()
    }
}

fn vars() -> BTreeMap<String, f64> {
    [
        ("w", 10.0),
        ("h", 2.5),
        ("width", 80.0),
        ("_a", -3.0),
        ("n", 0.0),
        ("big", 1e300),
        ("tiny", 1e-300),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect()
}

/// Expression text built from the grammar: mostly well formed, often with missing names,
/// wrong argument counts and numbers that overflow.
fn expr_text() -> impl Strategy<Value = String> {
    let leaf = prop_oneof![
        select(NUMS).prop_map(String::from),
        select(NAMES).prop_map(String::from),
        (0u32..5000, 0u32..1000).prop_map(|(a, b)| format!("{a}.{b}")),
    ];
    leaf.prop_recursive(4, 24, 3, |inner| {
        prop_oneof![
            (inner.clone(), select(OPS), inner.clone()).prop_map(|(a, o, b)| format!("{a}{o}{b}")),
            inner.clone().prop_map(|a| format!("({a})")),
            (select(FUNCS), prop::collection::vec(inner.clone(), 0..3))
                .prop_map(|(f, a)| format!("{f}({})", a.join(","))),
            (select(SIGNS), inner).prop_map(|(s, a)| format!("{s}{a}")),
        ]
    })
}

/// Grammar text with a few characters removed or junk put in.
fn damaged_text() -> impl Strategy<Value = String> {
    (
        expr_text(),
        prop::collection::vec((any::<Index>(), prop::option::of(select(JUNK))), 1..4),
    )
        .prop_map(|(text, edits)| {
            let mut s: Vec<char> = text.chars().collect();
            for (at, junk) in edits {
                let i = at.index(s.len() + 1);
                match junk {
                    Some(j) => s.splice(i..i, j.chars()).for_each(drop),
                    None if i < s.len() => drop(s.remove(i)),
                    None => {}
                }
            }
            s.into_iter().collect()
        })
}

fn any_text() -> impl Strategy<Value = String> {
    prop_oneof![expr_text(), damaged_text(), "\\PC{0,40}", ".{0,40}"]
}

/// What every answer must look like: a finite number, or a message to show.
fn check_answer(r: &Result<f64, ExprError>) -> Result<(), TestCaseError> {
    match r {
        Ok(v) => prop_assert!(v.is_finite(), "gave {v}"),
        Err(e) => prop_assert!(!e.0.is_empty(), "an error without a message"),
    }
    Ok(())
}

fn same_number(a: f64, b: f64) -> bool {
    a.to_bits() == b.to_bits()
}

proptest! {
    #![proptest_config(config(3000))]

    /// Any text at all gives a finite number or a message: never a crash, NaN or infinity.
    #[test]
    fn any_text_gives_a_number_or_a_message(text in any_text()) {
        check_answer(&evaluate(&text, &vars()))?;
        check_answer(&evaluate(&text, &BTreeMap::new()))?;
        for name in names_in(&text) {
            prop_assert!(!RESERVED.contains(&name.as_str()), "names_in gave the built-in {name}");
        }
    }

    /// Working an expression out twice gives the same answer (features re-run on every rebuild).
    #[test]
    fn the_same_text_gives_the_same_answer(text in any_text()) {
        let (a, b) = (evaluate(&text, &vars()), evaluate(&text, &vars()));
        match (a, b) {
            (Ok(x), Ok(y)) => prop_assert!(same_number(x, y)),
            (Err(x), Err(y)) => prop_assert_eq!(x, y),
            _ => prop_assert!(false, "a number once and an error once"),
        }
    }

    /// A leading "=", spaces around the text and a pair of brackets change nothing.
    #[test]
    fn equals_sign_spaces_and_brackets_change_nothing(text in expr_text()) {
        let plain = evaluate(&text, &vars());
        for other in [format!("={text}"), format!("  = {text}\t"), format!("({text})"), format!("=( {text} )")] {
            match (&plain, evaluate(&other, &vars())) {
                (Ok(x), Ok(y)) => prop_assert!(same_number(*x, y), "{other:?} gave {y}, plain gave {x}"),
                (Err(_), Err(_)) => {}
                (p, o) => prop_assert!(false, "{other:?} gave {o:?}, plain gave {p:?}"),
            }
        }
    }

    /// "-(x)" is the number x with its sign turned.
    #[test]
    fn minus_turns_the_sign(text in expr_text()) {
        if let Ok(x) = evaluate(&text, &vars()) {
            let y = evaluate(&format!("-({text})"), &vars());
            prop_assert!(matches!(y, Ok(y) if same_number(y, -x)), "-({text}) gave {y:?}, plain gave {x}");
        }
    }

    /// A piece that has no answer (a function outside its range, a missing name, a division by
    /// zero) makes the whole expression fail wherever it stands: `min` and `max` can't step over
    /// it and multiplying by zero can't hide it.
    #[test]
    fn a_failing_piece_fails_the_whole(
        x in expr_text(),
        bad in select(&[
            "sqrt(-1)", "asin(2)", "acos(-1.5)", "5%0", "0^-1", "(-8)^0.5", "1/0", "nope", "log(1)", "min()",
            "sin(1,2)", "sqrt(_a)", "asin(w)", "w/n", "w%n", "n^_a",
        ][..]),
        shape in select(&[
            "min(({x}),{bad})", "min({bad},({x}))", "max(({x}),{bad})", "max({bad},({x}))", "({x})+{bad}", "{bad}-({x})",
            "abs({bad})*0", "0*{bad}", "({bad})^0", "0^({bad})", "min(({x}),max({bad},({x})),({x}))", "-({bad})",
            "floor({bad})", "min(1,{bad},2)", "max(1,2,{bad})",
        ][..]),
    ) {
        let text = shape.replace("{x}", &x).replace("{bad}", bad);
        let r = evaluate(&text, &vars());
        prop_assert!(r.is_err(), "{text:?} gave {r:?}");
    }

    /// A value the expression doesn't use can't change its answer.
    #[test]
    fn unused_values_change_nothing(text in expr_text(), extra in -1e6f64..1e6) {
        let mut more = vars();
        more.insert("unused_name".to_string(), extra);
        match (evaluate(&text, &vars()), evaluate(&text, &more)) {
            (Ok(x), Ok(y)) => prop_assert!(same_number(x, y)),
            (Err(x), Err(y)) => prop_assert_eq!(x, y),
            (a, b) => prop_assert!(false, "{a:?} without the extra value, {b:?} with it"),
        }
    }
}

const PARAM_NAMES: &[&str] = &["a", "b", "c", "d", "len", "_x", "A", "w", "h"];
const BAD_NAMES: &[&str] = &["", "1a", "a-b", "a b", "pi", "sqrt", "min", "\u{e9}", "a\n"];

/// A parameter's expression: grammar text, with some of its names swapped for parameter names so
/// the list has chains and loops.
fn param_expr() -> impl Strategy<Value = String> {
    prop_oneof![
        3 => (expr_text(), prop::collection::vec(select(PARAM_NAMES), 3)).prop_map(|(mut e, names)| {
            for (from, to) in ["width", "tiny", "big"].iter().zip(names) {
                e = e.replace(from, to);
            }
            e
        }),
        2 => (select(PARAM_NAMES), select(&["", "+1", "*2", "/2", "^2", "/0"][..])).prop_map(|(n, t)| format!("{n}{t}")),
        1 => select(&["5", "=5", " 2.5 ", "", "=", "1/0", "$"][..]).prop_map(String::from),
        1 => damaged_text(),
    ]
}

fn param(names: &'static [&'static str]) -> impl Strategy<Value = Param> {
    (select(names), param_expr()).prop_map(|(name, expr)| Param {
        name: name.to_string(),
        expr,
        note: String::new(),
    })
}

/// A parameter list as a project could hold it: some names twice, some not names at all.
fn param_list() -> impl Strategy<Value = Vec<Param>> {
    prop::collection::vec(prop_oneof![5 => param(PARAM_NAMES), 1 => param(BAD_NAMES)], 0..8)
}

/// A list where every name is there once, in a shuffled order.
fn unique_param_list() -> impl Strategy<Value = Vec<Param>> {
    prop::collection::vec(param_expr(), PARAM_NAMES.len())
        .prop_map(|exprs| {
            PARAM_NAMES
                .iter()
                .zip(exprs)
                .map(|(name, expr)| Param {
                    name: name.to_string(),
                    expr,
                    note: String::new(),
                })
                .collect::<Vec<_>>()
        })
        .prop_shuffle()
}

proptest! {
    #![proptest_config(config(2000))]

    /// Every parameter ends with a value or a message, never both and never neither; values are
    /// finite and are what the parameter's own expression gives from the other values.
    #[test]
    fn every_parameter_gets_a_value_or_a_message(list in param_list()) {
        let r = resolve_params(&list);
        for q in &list {
            let key = if q.name.is_empty() { "?" } else { q.name.as_str() };
            let (has_value, has_error) = (r.values.contains_key(key), r.errors.contains_key(key));
            prop_assert!(has_value != has_error, "{key:?}: value {has_value}, message {has_error}");
        }
        for (name, v) in &r.values {
            prop_assert!(v.is_finite(), "{name} is {v}");
            let Some(q) = list.iter().find(|q| &q.name == name) else {
                return Err(TestCaseError::fail(format!("{name} has a value but isn't in the list")));
            };
            let again = evaluate(&q.expr, &r.values);
            prop_assert!(matches!(again, Ok(x) if same_number(x, *v)), "{name} = {v}, its expression gives {again:?}");
        }
        for (name, message) in &r.errors {
            prop_assert!(!message.is_empty(), "{name} has an empty message");
        }
    }

    /// The order of the list doesn't change which parameters work or their values.
    #[test]
    fn list_order_changes_no_value(list in unique_param_list()) {
        let forward = resolve_params(&list);
        let mut back = list.clone();
        back.reverse();
        let backward = resolve_params(&back);
        prop_assert_eq!(forward.values.keys().collect::<Vec<_>>(), backward.values.keys().collect::<Vec<_>>());
        for (name, v) in &forward.values {
            prop_assert!(same_number(*v, backward.values[name]), "{name}: {v} and {}", backward.values[name]);
        }
        prop_assert_eq!(forward.errors.keys().collect::<Vec<_>>(), backward.errors.keys().collect::<Vec<_>>());
    }
}

/// A feature's params as JSON: numbers, text, lists and objects, with expressions in places.
fn params_json() -> impl Strategy<Value = Value> {
    let leaf = prop_oneof![
        1 => Just(Value::Null),
        1 => any::<bool>().prop_map(Value::Bool),
        1 => (-1e6f64..1e6).prop_map(|x| json!(x)),
        1 => select(&["", "text", "hole", " =", "= ", "not = an expr"][..]).prop_map(|s| json!(s)),
        3 => expr_text().prop_map(|e| json!(format!("={e}"))),
        1 => damaged_text().prop_map(|e| json!(format!(" = {e}"))),
    ];
    leaf.prop_recursive(3, 16, 4, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..4).prop_map(Value::Array),
            prop::collection::vec((select(&["depth", "r", "name", "at", "=k"][..]), inner), 0..4)
                .prop_map(|kv| Value::Object(kv.into_iter().map(|(k, v)| (k.to_string(), v)).collect())),
        ]
    })
}

proptest! {
    #![proptest_config(config(2000))]

    /// Params without expressions come back unchanged; with expressions they come back with
    /// every one of them replaced by a number, or with a message.
    #[test]
    fn params_are_resolved_all_or_not_at_all(v in params_json()) {
        match resolve_deep(&v, &vars()) {
            Ok(out) => {
                prop_assert!(!uses_expr(&out), "an expression is left in {out}");
                if !uses_expr(&v) {
                    prop_assert_eq!(&out, &v);
                }
            }
            Err(e) => {
                prop_assert!(uses_expr(&v), "params without expressions gave {e}");
                prop_assert!(!e.0.is_empty());
            }
        }
    }
}
