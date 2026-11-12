//! A `FinsHost` that answers from `rust/tests/parity/fins_replay.json`, which
//! `dev/parity/record_fins_replay.py` records by running 1.x on every Support Fins and Draw Walls
//! parity case. It lets the parity tests run Support Fins without a JavaScript engine.
//!
//! A call is found by its name and the SHA-256 of its arguments (JSON arguments are first written
//! with every number as its float64 bits, so Python and Rust spell them the same). When the exact
//! arguments weren't recorded, which happens if the Rust kernel orders a mesh's triangles
//! differently from 1.x's, the call is matched by name, the arguments after the triangle soup,
//! and the nearest soup length; [`ReplayHost::loose_hits`] counts how often that was needed.

#![allow(dead_code)]

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;

use serde_json::Value;
use sha2::{Digest, Sha256};
use stl_engine::{FinsHost, HostKind};

struct Entry {
    name: String,
    exact: String,
    rest: String,
    soup_len: usize,
    result: String,
}

struct Recording {
    info: Value,
    has: HashSet<String>,
    entries: Vec<Entry>,
}

fn recording() -> &'static Recording {
    static R: OnceLock<Recording> = OnceLock::new();
    R.get_or_init(|| {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/parity/fins_replay.json");
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let v: Value = serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let s = |e: &Value, k: &str| e[k].as_str().unwrap_or_default().to_string();
        Recording {
            info: v["info"].clone(),
            has: v["has"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default(),
            entries: v["calls"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .map(|e| Entry {
                            name: s(e, "n"),
                            exact: s(e, "x"),
                            rest: s(e, "r"),
                            soup_len: e["l"].as_u64().unwrap_or(0) as usize,
                            result: s(e, "v"),
                        })
                        .collect()
                })
                .unwrap_or_default(),
        }
    })
}

/// `record_fins_replay.py`'s `_canon_value`: JSON with every number as its float64 bits.
fn canon_value(v: &Value, out: &mut String) {
    match v {
        Value::Number(n) => out.push_str(&format!("f{:016x}", n.as_f64().unwrap_or(0.0).to_bits())),
        Value::Array(a) => {
            out.push('[');
            for (i, x) in a.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                canon_value(x, out);
            }
            out.push(']');
        }
        Value::Object(o) => {
            out.push('{');
            for (i, (k, x)) in o.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::to_string(k).unwrap_or_default());
                out.push(':');
                canon_value(x, out);
            }
            out.push('}');
        }
        other => out.push_str(&serde_json::to_string(other).unwrap_or_default()),
    }
}

fn canon(arg: &str) -> String {
    if (arg.starts_with('[') || arg.starts_with('{')) && arg.len() < 100_000 {
        if let Ok(v) = serde_json::from_str::<Value>(arg) {
            let mut out = String::new();
            canon_value(&v, &mut out);
            return out;
        }
    }
    arg.to_string()
}

fn sha(s: &str) -> String {
    Sha256::digest(s.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

const NO_SOUP: [&str; 3] = ["optionsFromDialogJson", "reportLine", "optionsSchemaJson"];

static LOOSE_TOTAL: AtomicUsize = AtomicUsize::new(0);

/// Loose matches over every replay host in this process.
pub fn loose_hits_total() -> usize {
    LOOSE_TOTAL.load(Ordering::Relaxed)
}

/// An argument arrives as JSON text: a string is quoted (decode it), an object or array is
/// written in the recording's canonical form, a number as its integer text.
fn decode_arg(arg: &str) -> String {
    match serde_json::from_str::<Value>(arg) {
        Ok(Value::String(s)) => canon(&s),
        Ok(Value::Number(n)) => match n.as_f64() {
            Some(f) if f.fract() == 0.0 => format!("{}", f as i64),
            _ => n.to_string(),
        },
        Ok(v @ (Value::Array(_) | Value::Object(_))) => {
            let mut out = String::new();
            canon_value(&v, &mut out);
            out
        }
        _ => canon(arg),
    }
}

pub struct ReplayHost {
    loose: AtomicUsize,
}

impl ReplayHost {
    pub fn new() -> Self {
        recording(); // fail early when the recording is missing
        ReplayHost { loose: AtomicUsize::new(0) }
    }

    /// How many calls were matched without their exact arguments.
    pub fn loose_hits(&self) -> usize {
        self.loose.load(Ordering::Relaxed)
    }
}

impl Default for ReplayHost {
    fn default() -> Self {
        Self::new()
    }
}

impl FinsHost for ReplayHost {
    fn call(&self, name: &str, args: &[&str]) -> Result<String, String> {
        let rec = recording();
        if name == "__info" {
            let mut info = rec.info.clone();
            info["pending"] = Value::Null;
            return Ok(info.to_string());
        }
        let c: Vec<String> = args.iter().map(|a| decode_arg(a)).collect();
        let exact = sha(&c.join("\0"));
        if let Some(e) = rec.entries.iter().find(|e| e.name == name && e.exact == exact) {
            return Ok(e.result.clone());
        }
        if !NO_SOUP.contains(&name) && !c.is_empty() {
            let rest = sha(&c[1..].join("\0"));
            let len = c[0].len();
            let best = rec
                .entries
                .iter()
                .filter(|e| e.name == name && e.rest == rest && e.soup_len > 0)
                .min_by_key(|e| e.soup_len.abs_diff(len));
            if let Some(e) = best {
                self.loose.fetch_add(1, Ordering::Relaxed);
                LOOSE_TOTAL.fetch_add(1, Ordering::Relaxed);
                return Ok(e.result.clone());
            }
        }
        Err(format!("Support Fins: replay has no recording for {name} with these arguments"))
    }

    fn has(&self, name: &str) -> bool {
        recording().has.contains(name)
    }

    fn host_kind(&self) -> HostKind {
        HostKind::Webview
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_arguments_are_spelled_by_their_numbers() {
        assert_eq!(canon("[5, 10.0]"), canon("[5.0,10]"));
        assert_ne!(canon("[5, 10.5]"), canon("[5.0,10]"));
        assert_eq!(canon("plain text"), "plain text");
    }

    #[test]
    fn answers_info_and_extras() {
        let h = ReplayHost::new();
        assert!(h.has("computeFinsB64"));
        assert!(!h.has("noSuchThing"));
        let info: Value = serde_json::from_str(&h.call("__info", &[]).unwrap()).unwrap();
        assert_eq!(info["version"], "1.0.5");
        assert!(h.call("computeFinsB64", &["\"nothing recorded\""]).is_err());
    }
}
