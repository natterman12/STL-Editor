//! The pure-Rust check: no C build tools in any shipped target's dependency graph.

use serde_json::Value;
use std::collections::{BTreeSet, HashMap, VecDeque};

/// Packages that need a C toolchain or system libraries to build.
pub const FORBIDDEN: [&str; 5] = ["cc", "cmake", "bindgen", "pkg-config", "vcpkg"];

/// The one exception to the rule (spec section 4), on `x86_64-unknown-linux-gnu` only: wry's
/// Linux backend links the system WebKitGTK, and these crates (exact names from
/// `cargo tree -p stl-platform --target x86_64-unknown-linux-gnu -e build`) find it with
/// `pkg-config` / `system-deps` at build time. Only their build edges to those two crates are
/// excused; anything else they or any other crate pull in (cc, cmake, ...) still fails.
pub const LINUX_ALLOW: [&str; 14] = [
    "atk-sys",
    "cairo-sys-rs",
    "gdk-pixbuf-sys",
    "gdk-sys",
    "gdkwayland-sys",
    "gio-sys",
    "glib-sys",
    "gobject-sys",
    "gtk",
    "gtk-sys",
    "javascriptcore-rs-sys",
    "pango-sys",
    "soup3-sys",
    "webkit2gtk-sys",
];
pub const LINUX_ALLOW_TARGET: &str = "x86_64-unknown-linux-gnu";
pub const LINUX_ALLOW_REASON: &str = "system WebKitGTK for Support Fins";
/// What the allow-listed crates may use.
const SYSTEM_LIB_FINDERS: [&str; 2] = ["pkg-config", "system-deps"];

/// Sorted, de-duplicated names of forbidden packages reachable from the workspace
/// members through normal or build edges. Dev-only edges are skipped.
#[cfg_attr(not(test), allow(dead_code))]
pub fn forbidden(metadata: &Value) -> Vec<String> {
    forbidden_except(metadata, &[])
}

/// As [`forbidden`], but a build-only edge from a crate named in `allow` to `pkg-config` or
/// `system-deps` is not followed.
pub fn forbidden_except(metadata: &Value, allow: &[&str]) -> Vec<String> {
    let mut names: HashMap<&str, &str> = HashMap::new();
    for p in metadata["packages"].as_array().into_iter().flatten() {
        if let (Some(id), Some(name)) = (p["id"].as_str(), p["name"].as_str()) {
            names.insert(id, name);
        }
    }
    let mut pkgs: HashMap<&str, &Value> = HashMap::new();
    for p in metadata["packages"].as_array().into_iter().flatten() {
        if let Some(id) = p["id"].as_str() {
            pkgs.insert(id, p);
        }
    }
    let mut deps: HashMap<&str, Vec<&str>> = HashMap::new();
    for n in metadata["resolve"]["nodes"].as_array().into_iter().flatten() {
        let Some(id) = n["id"].as_str() else { continue };
        let list = deps.entry(id).or_default();
        // `cargo metadata` lists every enabled feature on the node. Absent means this
        // graph doesn't say, so optional deps are followed as before.
        let enabled = n.get("features").and_then(Value::as_array);
        for d in n["deps"].as_array().into_iter().flatten() {
            let Some(pkg) = d["pkg"].as_str() else { continue };
            let kinds = d["dep_kinds"].as_array().map(Vec::as_slice).unwrap_or(&[]);
            // A dep with no kinds listed is treated as a normal edge.
            let dev_only = !kinds.is_empty() && kinds.iter().all(|k| k["kind"].as_str() == Some("dev"));
            let build_only = !kinds.is_empty() && kinds.iter().all(|k| k["kind"].as_str() == Some("build"));
            let excused = build_only
                && names.get(id).is_some_and(|n| allow.contains(n))
                && names.get(pkg).is_some_and(|n| SYSTEM_LIB_FINDERS.contains(n));
            // A weak feature (`ring?/alloc`) does not enable the optional dep, but metadata
            // still draws the edge. Follow it only when an enabled feature really turns it on.
            let inactive = enabled.is_some_and(|feats| {
                inactive_optional(pkgs.get(id).copied(), feats, d["name"].as_str().unwrap_or(""), kinds)
            });
            if !dev_only && !excused && !inactive {
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

/// `true` when every non-dev kind of this edge is an optional dependency that the node's
/// enabled features do not activate. `dep:name` and a feature named `name` do; `name?/feat` does not.
/// `name/feat` does too: that is Cargo's strong syntax, and it really does build the dep.
fn inactive_optional(pkg: Option<&Value>, enabled: &[Value], dep_name: &str, kinds: &[Value]) -> bool {
    let Some(pkg) = pkg else { return false };
    let Some(specs) = pkg.get("dependencies").and_then(Value::as_array) else { return false };
    let matching: Vec<&Value> = specs.iter().filter(|s| s["name"].as_str() == Some(dep_name)).collect();
    if matching.is_empty() || dep_name.is_empty() {
        return false;
    }
    let activated = optional_activated(pkg, enabled, dep_name);
    let edge_kinds: Vec<Option<&str>> = if kinds.is_empty() {
        vec![None]
    } else {
        kinds.iter().map(|k| k["kind"].as_str()).filter(|k| *k != Some("dev")).collect()
    };
    if edge_kinds.is_empty() {
        return false;
    }
    let mut live = false;
    for kind in edge_kinds {
        let mut matched = false;
        for spec in &matching {
            if spec["kind"].as_str() != kind {
                continue;
            }
            matched = true;
            let optional = spec["optional"].as_bool().unwrap_or(false);
            if !optional || activated {
                live = true;
            }
        }
        if !matched {
            live = true;
        }
    }
    !live
}

fn optional_activated(pkg: &Value, enabled: &[Value], dep: &str) -> bool {
    if enabled.iter().any(|f| f.as_str() == Some(dep)) {
        return true;
    }
    let Some(features) = pkg.get("features").and_then(Value::as_object) else { return false };
    for feat in enabled {
        let Some(name) = feat.as_str() else { continue };
        let Some(items) = features.get(name).and_then(Value::as_array) else { continue };
        if items.iter().any(|item| item.as_str().is_some_and(|s| feature_activates(s, dep))) {
            return true;
        }
    }
    false
}

/// Does this one feature entry turn on the optional dependency `dep`?
fn feature_activates(spec: &str, dep: &str) -> bool {
    if let Some(name) = spec.strip_prefix("dep:") {
        return name == dep;
    }
    // Weak: `ring?/alloc` enables a feature of ring only when ring is already on.
    if spec.split_once("?/").is_some() {
        return false;
    }
    if let Some((name, _)) = spec.split_once('/') {
        return name == dep;
    }
    spec == dep
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

    /// A small `cargo metadata` graph: `(name, [(dep, kind)])`, the first package the member.
    fn graph(pkgs: &[(&str, &[(&str, &str)])]) -> Value {
        let id = |n: &str| format!("registry+x#{n}@1.0.0");
        let packages: Vec<Value> = pkgs.iter().map(|(n, _)| serde_json::json!({"id": id(n), "name": n})).collect();
        let nodes: Vec<Value> = pkgs
            .iter()
            .map(|(n, deps)| {
                let deps: Vec<Value> = deps
                    .iter()
                    .map(|(d, k)| {
                        let kind = if *k == "normal" { Value::Null } else { Value::String(k.to_string()) };
                        serde_json::json!({"name": d, "pkg": id(d), "dep_kinds": [{"kind": kind}]})
                    })
                    .collect();
                serde_json::json!({"id": id(n), "deps": deps})
            })
            .collect();
        serde_json::json!({"packages": packages, "workspace_members": [id(pkgs[0].0)], "resolve": {"nodes": nodes}})
    }

    #[test]
    fn linux_allow_list_excuses_webkitgtk_only() {
        let m = graph(&[
            ("stl-platform", &[("wry", "normal")]),
            ("wry", &[("webkit2gtk-sys", "normal"), ("gtk", "normal")]),
            ("webkit2gtk-sys", &[("system-deps", "build"), ("pkg-config", "build")]),
            ("gtk", &[("pkg-config", "build")]),
            ("system-deps", &[("pkg-config", "normal")]),
            ("pkg-config", &[]),
        ]);
        assert_eq!(forbidden(&m), ["pkg-config"]);
        assert!(forbidden_except(&m, &LINUX_ALLOW).is_empty());
    }

    #[test]
    fn linux_allow_list_still_fails_for_cc_elsewhere() {
        // cc as a build dependency of a crate not on the list.
        let m = graph(&[
            ("stl-platform", &[("wry", "normal"), ("other", "normal")]),
            ("wry", &[("webkit2gtk-sys", "normal")]),
            ("webkit2gtk-sys", &[("pkg-config", "build")]),
            ("other", &[("cc", "build")]),
            ("pkg-config", &[]),
            ("cc", &[]),
        ]);
        assert_eq!(forbidden_except(&m, &LINUX_ALLOW), ["cc"]);
        // cc, or pkg-config through a normal edge, from a listed crate is not excused either.
        let m = graph(&[
            ("stl-platform", &[("webkit2gtk-sys", "normal"), ("glib-sys", "normal")]),
            ("webkit2gtk-sys", &[("cc", "build")]),
            ("glib-sys", &[("pkg-config", "normal")]),
            ("pkg-config", &[]),
            ("cc", &[]),
        ]);
        assert_eq!(forbidden_except(&m, &LINUX_ALLOW), ["cc", "pkg-config"]);
        // pkg-config used by any other crate still fails.
        let m = graph(&[("stl-platform", &[("foo-sys", "normal")]), ("foo-sys", &[("pkg-config", "build")]), ("pkg-config", &[])]);
        assert_eq!(forbidden_except(&m, &LINUX_ALLOW), ["pkg-config"]);
    }

    /// `ring?/alloc` is on, the `ring` feature is not. Metadata still has the edge; the build does not.
    #[test]
    fn weak_feature_does_not_follow_optional_dep() {
        let m = webpki_ring(&["alloc", "std"], &["ring?/alloc", "pki-types/alloc"]);
        assert!(forbidden(&m).is_empty(), "{:?}", forbidden(&m));
        // A feature named like the dep, or `dep:name` inside an enabled feature, does build it.
        let named = webpki_ring(&["ring"], &["ring?/alloc"]);
        assert_eq!(forbidden(&named), ["cc"]);
        let explicit = webpki_ring(&["alloc"], &["dep:ring"]);
        assert_eq!(forbidden(&explicit), ["cc"]);
        // Strong `name/feat` enables the optional dep too.
        let strong = webpki_ring(&["alloc"], &["ring/alloc"]);
        assert_eq!(forbidden(&strong), ["cc"]);
    }

    #[test]
    fn weak_feature_does_not_hide_a_real_build_tool() {
        for tool in ["cc", "cmake", "bindgen", "pkg-config", "vcpkg"] {
            let m = graph(&[("app", &[(tool, "build")]), (tool, &[])]);
            assert_eq!(forbidden(&m), [tool.to_string()], "{tool}");
            assert_eq!(forbidden_except(&m, &LINUX_ALLOW), [tool.to_string()], "{tool} stays forbidden off the allow-list");
        }
    }

    /// rustls-webpki shape: optional `ring`, enabled features only `alloc` and `std`.
    fn webpki_ring(enabled: &[&str], alloc: &[&str]) -> Value {
        let id = |n: &str| format!("registry+x#{n}@1.0.0");
        let edge = |name: &str, kind: &str| {
            let kind = if kind == "normal" { Value::Null } else { Value::String(kind.into()) };
            serde_json::json!({"name": name, "pkg": id(name), "dep_kinds": [{"kind": kind}]})
        };
        serde_json::json!({
            "packages": [
                {"id": id("app"), "name": "app", "dependencies": [
                    {"name": "rustls-webpki", "kind": null, "optional": false}
                ]},
                {"id": id("rustls-webpki"), "name": "rustls-webpki", "dependencies": [
                    {"name": "ring", "kind": null, "optional": true}
                ], "features": {"alloc": alloc, "std": ["alloc"], "ring": ["dep:ring"]}},
                {"id": id("ring"), "name": "ring", "dependencies": [
                    {"name": "cc", "kind": "build", "optional": false}
                ]},
                {"id": id("cc"), "name": "cc"}
            ],
            "workspace_members": [id("app")],
            "resolve": {"nodes": [
                {"id": id("app"), "features": [], "deps": [edge("rustls-webpki", "normal")]},
                {"id": id("rustls-webpki"), "features": enabled, "deps": [edge("ring", "normal")]},
                {"id": id("ring"), "features": ["alloc"], "deps": [edge("cc", "build")]},
                {"id": id("cc"), "features": [], "deps": []}
            ]}
        })
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
