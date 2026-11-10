//! The pure-Rust check: no C build tools in any shipped target's dependency graph.

use serde_json::Value;
use std::collections::{BTreeSet, HashMap, VecDeque};

/// Packages that need a C toolchain or system libraries to build.
pub const FORBIDDEN: [&str; 5] = ["cc", "cmake", "bindgen", "pkg-config", "vcpkg"];

/// Sorted, de-duplicated names of forbidden packages reachable from the workspace
/// members through normal or build edges. Dev-only edges are skipped.
pub fn forbidden(metadata: &Value) -> Vec<String> {
    let mut names: HashMap<&str, &str> = HashMap::new();
    for p in metadata["packages"].as_array().into_iter().flatten() {
        if let (Some(id), Some(name)) = (p["id"].as_str(), p["name"].as_str()) {
            names.insert(id, name);
        }
    }
    let mut deps: HashMap<&str, Vec<&str>> = HashMap::new();
    for n in metadata["resolve"]["nodes"].as_array().into_iter().flatten() {
        let Some(id) = n["id"].as_str() else { continue };
        let list = deps.entry(id).or_default();
        for d in n["deps"].as_array().into_iter().flatten() {
            let Some(pkg) = d["pkg"].as_str() else { continue };
            let kinds = d["dep_kinds"].as_array().map(Vec::as_slice).unwrap_or(&[]);
            // A dep with no kinds listed is treated as a normal edge.
            let dev_only = !kinds.is_empty() && kinds.iter().all(|k| k["kind"].as_str() == Some("dev"));
            if !dev_only {
                list.push(pkg);
            }
        }
    }
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut queue: VecDeque<&str> = VecDeque::new();
    for m in metadata["workspace_members"].as_array().into_iter().flatten() {
        if let Some(id) = m.as_str() {
            if seen.insert(id) {
                queue.push_back(id);
            }
        }
    }
    let mut found = BTreeSet::new();
    while let Some(id) = queue.pop_front() {
        if let Some(name) = names.get(id) {
            if FORBIDDEN.contains(name) {
                found.insert(name.to_string());
            }
        }
        for &next in deps.get(id).into_iter().flatten() {
            if seen.insert(next) {
                queue.push_back(next);
            }
        }
    }
    found.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Value {
        let path = format!("{}/tests/fixtures/{name}.json", env!("CARGO_MANIFEST_DIR"));
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    #[test]
    fn with_cc() {
        assert_eq!(forbidden(&fixture("with_cc")), ["cc"]);
    }

    #[test]
    fn windows_sys_only() {
        assert!(forbidden(&fixture("windows_sys_only")).is_empty());
    }

    #[test]
    fn dev_only_cc_edge_is_ignored() {
        let mut m = fixture("with_cc");
        for n in m["resolve"]["nodes"].as_array_mut().unwrap() {
            for d in n["deps"].as_array_mut().unwrap() {
                for k in d["dep_kinds"].as_array_mut().unwrap() {
                    k["kind"] = Value::String("dev".into());
                }
            }
        }
        assert!(forbidden(&m).is_empty());
    }
}
