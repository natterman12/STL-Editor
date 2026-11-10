//! Parameters: named values a project keeps, and the expressions that use them.
//!
//! A port of `stleditor/params.py`: same grammar, precedence, token rules, function table and
//! message text. Lengths are millimetres and angles degrees. Deliberate differences are listed
//! in `rust/tests/EXCEPTIONS.md`.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// An expression problem the user can fix. `Display` is the message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExprError(pub String);

impl fmt::Display for ExprError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ExprError {}

fn err<T>(m: String) -> Result<T, ExprError> {
    Err(ExprError(m))
}

/// A named value of a project (1.x keys: name, expr, note).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Param {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub expr: String,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Resolved {
    pub values: BTreeMap<String, f64>,
    pub errors: BTreeMap<String, String>,
}

/// Built-in names: the 13 functions and the constant `pi`.
pub const RESERVED: [&str; 14] = [
    "sqrt", "abs", "min", "max", "round", "floor", "ceil", "sin", "cos", "tan", "asin", "acos",
    "atan", "pi",
];

/// Deepest nesting of brackets / unary signs / powers before giving up.
const MAX_DEPTH: usize = 200;

/// First code point of each run of ten Unicode decimal digits (category Nd), as Python's `\d`
/// and `float()` accept them (Unicode 16, Python 3.14).
const DIGIT_ZEROS: [u32; 76] = [
    0x30, 0x660, 0x6F0, 0x7C0, 0x966, 0x9E6, 0xA66, 0xAE6,
    0xB66, 0xBE6, 0xC66, 0xCE6, 0xD66, 0xDE6, 0xE50, 0xED0,
    0xF20, 0x1040, 0x1090, 0x17E0, 0x1810, 0x1946, 0x19D0, 0x1A80,
    0x1A90, 0x1B50, 0x1BB0, 0x1C40, 0x1C50, 0xA620, 0xA8D0, 0xA900,
    0xA9D0, 0xA9F0, 0xAA50, 0xABF0, 0xFF10, 0x104A0, 0x10D30, 0x10D40,
    0x11066, 0x110F0, 0x11136, 0x111D0, 0x112F0, 0x11450, 0x114D0, 0x11650,
    0x116C0, 0x116D0, 0x116DA, 0x11730, 0x118E0, 0x11950, 0x11BF0, 0x11C50,
    0x11D50, 0x11DA0, 0x11F50, 0x16130, 0x16A60, 0x16AC0, 0x16B50, 0x16D70,
    0x1CCF0, 0x1D7CE, 0x1D7D8, 0x1D7E2, 0x1D7EC, 0x1D7F6, 0x1E140, 0x1E2F0,
    0x1E4F0, 0x1E5F1, 0x1E950, 0x1FBF0,
];

/// The value 0-9 of a Unicode decimal digit, or `None` (Python's `\d`).
fn digit_value(c: char) -> Option<u32> {
    let cp = c as u32;
    if cp < 0x80 {
        return c.to_digit(10);
    }
    let i = DIGIT_ZEROS.partition_point(|&z| z <= cp);
    (i > 0 && cp - DIGIT_ZEROS[i - 1] < 10).then(|| cp - DIGIT_ZEROS[i - 1])
}

/// Python's `str.isspace()`: Unicode White_Space plus U+001C..U+001F.
fn py_space(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}

/// Python's `str.strip()`.
fn py_strip(s: &str) -> &str {
    s.trim_matches(py_space)
}

/// Python `math.floor` / `ceil` give ints, so there is never a negative zero.
fn no_neg_zero(x: f64) -> f64 {
    if x == 0.0 { 0.0 } else { x }
}

fn is_reserved(s: &str) -> bool {
    RESERVED.contains(&s)
}

/// `^[A-Za-z_][A-Za-z0-9_]*$` (Python's `$` also lets one trailing newline through).
pub fn is_valid_name(s: &str) -> bool {
    let s = s.strip_suffix('\n').unwrap_or(s);
    let mut it = s.chars();
    match it.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    it.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

pub fn is_expr(v: &Value) -> bool {
    matches!(v, Value::String(s) if py_strip(s).starts_with('='))
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f64),
    Id(String),
    Sym(char),
}

/// Python's `repr(float)`, used where 1.x prints a number token.
fn py_float_repr(x: f64) -> String {
    let s = format!("{x:?}");
    match s.split_once('e') {
        Some((m, e)) => {
            let (sign, digits) = match e.strip_prefix('-') {
                Some(d) => ('-', d),
                None => ('+', e),
            };
            format!("{m}e{sign}{digits:0>2}")
        }
        None => s,
    }
}

fn tokens(text: &str) -> Result<Vec<Tok>, ExprError> {
    let t = py_strip(text);
    let s: Vec<char> = t.strip_prefix('=').unwrap_or(t).chars().collect();
    let whole: String = s.iter().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < s.len() {
        let c = s[i];
        if py_space(c) {
            i += 1;
            continue;
        }
        // number: (\d+\.?\d*|\.\d+)([eE][+-]?\d+)?
        let digits = |mut j: usize| {
            while j < s.len() && digit_value(s[j]).is_some() {
                j += 1;
            }
            j
        };
        let mut j = i;
        if digit_value(c).is_some() {
            j = digits(j);
            if j < s.len() && s[j] == '.' {
                j = digits(j + 1);
            }
        } else if c == '.' && i + 1 < s.len() && digit_value(s[i + 1]).is_some() {
            j = digits(i + 1);
        }
        if j > i {
            if j < s.len() && (s[j] == 'e' || s[j] == 'E') {
                let mut k = j + 1;
                if k < s.len() && (s[k] == '+' || s[k] == '-') {
                    k += 1;
                }
                let k2 = digits(k);
                if k2 > k {
                    j = k2;
                }
            }
            let mut lit: String = s[i..j]
                .iter()
                .map(|&ch| digit_value(ch).and_then(|d| char::from_digit(d, 10)).unwrap_or(ch))
                .collect();
            if lit.starts_with('.') {
                lit.insert(0, '0');
            }
            let lit = lit.replace(".e", ".0e").replace(".E", ".0E");
            let lit = if lit.ends_with('.') { format!("{lit}0") } else { lit };
            out.push(Tok::Num(lit.parse::<f64>().unwrap_or(f64::INFINITY)));
            i = j;
            continue;
        }
        if c.is_ascii_alphabetic() || c == '_' {
            let mut j = i + 1;
            while j < s.len() && (s[j].is_ascii_alphanumeric() || s[j] == '_') {
                j += 1;
            }
            out.push(Tok::Id(s[i..j].iter().collect()));
            i = j;
            continue;
        }
        if "+-*/%^(),".contains(c) {
            out.push(Tok::Sym(c));
            i += 1;
            continue;
        }
        return err(format!("can't read \"{c}\" in {whole}"));
    }
    Ok(out)
}

/// The parameter names an expression uses (functions and constants left out).
pub fn names_in(text: &str) -> BTreeSet<String> {
    match tokens(text) {
        Ok(tk) => tk
            .into_iter()
            .filter_map(|t| match t {
                Tok::Id(n) if !is_reserved(&n) => Some(n),
                _ => None,
            })
            .collect(),
        Err(_) => BTreeSet::new(),
    }
}

fn call(name: &str, a: &[f64]) -> Option<f64> {
    let x = a[0];
    Some(match name {
        "sqrt" => x.sqrt(),
        "abs" => x.abs(),
        "min" => a.iter().copied().fold(a[0], |b, v| if v < b { v } else { b }),
        "max" => a.iter().copied().fold(a[0], |b, v| if v > b { v } else { b }),
        "round" => no_neg_zero((x + 0.5).floor()),
        "floor" => no_neg_zero(x.floor()),
        "ceil" => no_neg_zero(x.ceil()),
        "sin" => x.to_radians().sin(),
        "cos" => x.to_radians().cos(),
        "tan" => x.to_radians().tan(),
        "asin" => x.asin().to_degrees(),
        "acos" => x.acos().to_degrees(),
        "atan" => x.atan().to_degrees(),
        _ => return None,
    })
}

struct Parser<'a> {
    tk: Vec<Tok>,
    pos: usize,
    src: String,
    vars: &'a BTreeMap<String, f64>,
    depth: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Tok> {
        self.tk.get(self.pos)
    }

    fn take(&mut self, c: char) -> bool {
        if self.tk.get(self.pos) == Some(&Tok::Sym(c)) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn nan_err<T>(&self) -> Result<T, ExprError> {
        err(format!("{} doesn't give a number", self.src))
    }

    /// Run `f` one level deeper; refuse past MAX_DEPTH.
    fn nested<T>(&mut self, f: impl FnOnce(&mut Self) -> Result<T, ExprError>) -> Result<T, ExprError> {
        if self.depth >= MAX_DEPTH {
            return err(format!("{} is nested too deeply", self.src));
        }
        self.depth += 1;
        let r = f(self);
        self.depth -= 1;
        r
    }

    fn expr(&mut self) -> Result<f64, ExprError> {
        let mut v = self.term()?;
        loop {
            if self.take('+') {
                v += self.term()?;
            } else if self.take('-') {
                v -= self.term()?;
            } else {
                return Ok(v);
            }
        }
    }

    fn term(&mut self) -> Result<f64, ExprError> {
        let mut v = self.power()?;
        loop {
            if self.take('*') {
                v *= self.power()?;
            } else if self.take('/') {
                let d = self.power()?;
                if d == 0.0 {
                    return err(format!("division by zero in {}", self.src));
                }
                v /= d;
            } else if self.take('%') {
                let d = self.power()?;
                v %= d; // C fmod
                if !v.is_finite() {
                    return self.nan_err();
                }
            } else {
                return Ok(v);
            }
        }
    }

    /// `unary ("^" unary)*`, right-associative; a loop, so a long chain can't overflow the stack.
    fn power(&mut self) -> Result<f64, ExprError> {
        let mut ops = vec![self.unary()?];
        while self.take('^') {
            ops.push(self.unary()?);
        }
        let mut r = ops.pop().unwrap_or(0.0);
        while let Some(b) = ops.pop() {
            r = b.powf(r);
            if !r.is_finite() {
                return self.nan_err();
            }
        }
        Ok(r)
    }

    /// Any run of `-` / `+` signs, then a primary (a loop for the same reason).
    fn unary(&mut self) -> Result<f64, ExprError> {
        let mut neg = false;
        loop {
            if self.take('-') {
                neg = !neg;
            } else if !self.take('+') {
                break;
            }
        }
        let v = self.primary()?;
        Ok(if neg { -v } else { v })
    }

    fn primary(&mut self) -> Result<f64, ExprError> {
        let Some(k) = self.peek().cloned() else {
            return err(format!("{} ends too soon", self.src));
        };
        self.pos += 1;
        match k {
            Tok::Num(v) => Ok(v),
            Tok::Sym('(') => {
                let r = self.nested(|p| p.expr())?;
                if !self.take(')') {
                    return err(format!("a \")\" is missing in {}", self.src));
                }
                Ok(r)
            }
            Tok::Id(v) => {
                if self.take('(') {
                    if !RESERVED[..13].contains(&v.as_str()) {
                        return err(format!("there's no function \"{v}\""));
                    }
                    let mut args = Vec::new();
                    if !self.take(')') {
                        args.push(self.nested(|p| p.expr())?);
                        while self.take(',') {
                            args.push(self.nested(|p| p.expr())?);
                        }
                        if !self.take(')') {
                            return err(format!("a \")\" is missing in {}", self.src));
                        }
                    }
                    if args.is_empty() {
                        return err(format!("{v}() needs a number"));
                    }
                    if v != "min" && v != "max" && args.len() != 1 {
                        return err(format!("{v}() takes 1 number"));
                    }
                    let r = call(&v, &args).unwrap_or(f64::NAN);
                    if !r.is_finite() {
                        return self.nan_err();
                    }
                    return Ok(r);
                }
                if v == "pi" {
                    return Ok(std::f64::consts::PI);
                }
                if let Some(x) = self.vars.get(&v) {
                    return Ok(*x);
                }
                err(format!("there's no parameter \"{v}\""))
            }
            Tok::Sym(c) => err(format!("unexpected \"{c}\" in {}", self.src)),
        }
    }
}

/// Work out an expression (with or without the leading `=`).
pub fn evaluate(text: &str, vars: &BTreeMap<String, f64>) -> Result<f64, ExprError> {
    let tk = tokens(text)?;
    let src = py_strip(text).trim_start_matches('=').to_string();
    if tk.is_empty() {
        return err("an expression is empty".to_string());
    }
    let mut p = Parser { tk, pos: 0, src, vars, depth: 0 };
    let r = p.expr()?;
    if let Some(t) = p.tk.get(p.pos) {
        let shown = match t {
            Tok::Num(n) => py_float_repr(*n),
            Tok::Id(n) => n.clone(),
            Tok::Sym(c) => c.to_string(),
        };
        return err(format!("unexpected \"{shown}\" in {}", p.src));
    }
    if !r.is_finite() {
        return p.nan_err();
    }
    Ok(r)
}

/// One parameter being worked out by `resolve_params`.
struct Frame<'a> {
    name: &'a str,
    expr: &'a str,
    deps: Vec<String>,
    next: usize,
    vs: BTreeMap<String, f64>,
}

enum Start<'a> {
    Done(Result<f64, ExprError>),
    Push(Frame<'a>),
}

/// Work out a project's parameters: `{values, errors}`. Parameters are visited in list order and
/// their dependencies with an explicit stack, so a very long chain can't overflow the stack.
pub fn resolve_params(list: &[Param]) -> Resolved {
    let mut by_name: BTreeMap<&str, &Param> = BTreeMap::new();
    let mut order: Vec<&str> = Vec::new();
    let mut res = Resolved::default();
    for q in list {
        let n = q.name.as_str();
        if !is_valid_name(n) {
            let key = if n.is_empty() { "?" } else { n };
            res.errors.insert(
                key.to_string(),
                format!("\"{n}\" isn't a name (letters, digits and _; not starting with a digit)"),
            );
            continue;
        }
        if is_reserved(n) {
            res.errors.insert(n.to_string(), format!("\"{n}\" is a built-in name"));
            continue;
        }
        if by_name.contains_key(n) {
            res.errors.insert(n.to_string(), format!("\"{n}\" is there twice"));
            continue;
        }
        by_name.insert(n, q);
        order.push(n);
    }

    let mut busy: BTreeSet<&str> = BTreeSet::new();

    fn start<'a>(
        name: &str,
        by_name: &BTreeMap<&'a str, &'a Param>,
        res: &Resolved,
        busy: &mut BTreeSet<&'a str>,
    ) -> Start<'a> {
        if let Some(v) = res.values.get(name) {
            return Start::Done(Ok(*v));
        }
        if let Some(m) = res.errors.get(name) {
            return Start::Done(err(m.clone()));
        }
        let Some((&key, q)) = by_name.get_key_value(name) else {
            return Start::Done(err(format!("there's no parameter \"{name}\"")));
        };
        if busy.contains(name) {
            return Start::Done(err(format!("\"{name}\" depends on itself")));
        }
        busy.insert(key);
        Start::Push(Frame {
            name: key,
            expr: &q.expr,
            deps: names_in(&q.expr).into_iter().collect(),
            next: 0,
            vs: BTreeMap::new(),
        })
    }

    for &root in &order {
        let mut stack: Vec<Frame> = Vec::new();
        let mut ret: Option<Result<f64, ExprError>> = None;
        match start(root, &by_name, &res, &mut busy) {
            Start::Done(_) => continue,
            Start::Push(f) => stack.push(f),
        }
        while let Some(top) = stack.last_mut() {
            if let Some(r) = ret.take() {
                match r {
                    Ok(v) => {
                        let dep = top.deps[top.next].clone();
                        top.vs.insert(dep, v);
                        top.next += 1;
                    }
                    Err(e) => {
                        res.errors.insert(top.name.to_string(), e.0.clone());
                        busy.remove(top.name);
                        stack.pop();
                        ret = Some(Err(e));
                        continue;
                    }
                }
            }
            if top.next < top.deps.len() {
                let dep = top.deps[top.next].clone();
                match start(&dep, &by_name, &res, &mut busy) {
                    Start::Done(r) => ret = Some(r),
                    Start::Push(f) => stack.push(f),
                }
                continue;
            }
            let out = evaluate(top.expr, &top.vs);
            match &out {
                Ok(v) => {
                    res.values.insert(top.name.to_string(), *v);
                }
                Err(e) => {
                    res.errors.insert(top.name.to_string(), e.0.clone());
                }
            }
            busy.remove(top.name);
            stack.pop();
            ret = Some(out);
        }
    }
    res
}

/// Replace every `"=..."` string inside a JSON value by its number.
pub fn resolve_deep(v: &Value, values: &BTreeMap<String, f64>) -> Result<Value, ExprError> {
    match v {
        Value::String(s) if is_expr(v) => {
            let mut vs = BTreeMap::new();
            for n in names_in(s) {
                match values.get(&n) {
                    Some(x) => {
                        vs.insert(n, *x);
                    }
                    None => {
                        return err(format!("there's no parameter \"{n}\" (in {})", py_strip(s)));
                    }
                }
            }
            let r = evaluate(s, &vs)?;
            Ok(serde_json::Number::from_f64(r).map(Value::Number).unwrap_or(Value::Null))
        }
        Value::Array(a) => Ok(Value::Array(
            a.iter().map(|x| resolve_deep(x, values)).collect::<Result<_, _>>()?,
        )),
        Value::Object(o) => Ok(Value::Object(
            o.iter()
                .map(|(k, x)| Ok((k.clone(), resolve_deep(x, values)?)))
                .collect::<Result<_, ExprError>>()?,
        )),
        other => Ok(other.clone()),
    }
}

pub fn uses_expr(v: &Value) -> bool {
    match v {
        Value::Array(a) => a.iter().any(uses_expr),
        Value::Object(o) => o.values().any(uses_expr),
        _ => is_expr(v),
    }
}
