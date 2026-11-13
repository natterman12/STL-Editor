//! The pure-Rust check: no C build tools in any shipped target's dependency graph.

use serde_json::Value;
use std::collections::{BTreeSet, HashMap, VecDeque};

/// Packages that need a C toolchain or system libraries to build.
pub const FORBIDDEN: [&str; 5] = ["cc", "cmake", "bindgen", "pkg-config", "vcpkg"];

/// Exceptions on `x86_64-unknown-linux-gnu` only. Only a build edge from one of these crates
/// to `pkg-config` or `system-deps` is excused. Anything else they pull in (cc, cmake, ...)
/// still fails, and so does the same edge from any crate not on the list.
///
/// * wry's Linux backend links system WebKitGTK. These `-sys` crates (names from
///   `cargo tree -p stl-platform --target x86_64-unknown-linux-gnu -e build`) find it at build time.
/// * `x11-dl` (accepted by Nathan on 2026-11-12) is winit's X11 backend. It dlopens libX11 and friends. Its build script asks
///   `pkg-config` for each library directory and treats a missing tool as "unknown path".
///   There is no feature that drops that build dependency. Wayland's dlopen path still
///   build-depends on `pkg-config` (`wayland-sys`) and `cc` (`wayland-backend`).
pub const LINUX_ALLOW: [&str; 15] = [
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
    "x11-dl",
];
pub const LINUX_ALLOW_TARGET: &str = "x86_64-unknown-linux-gnu";
pub const LINUX_ALLOW_REASON: &str =
    "system WebKitGTK for Support Fins; x11-dl probes X11 paths and dlopens the libraries";
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
    // `cargo metadata --filter-platform` still stamps features onto a node that only some
    // other target turned on (wry's apple-only `objc2` features show up on macOS, where wry
    // is not even a dependency). Where the manifest specs in this resolve say which features
    // they asked for, trust that instead of the node's feature list. `cargo tree --target`
    // agrees with the recomputed set.
    let reconciled = reconciled_features(metadata);
    let mut deps: HashMap<&str, Vec<&str>> = HashMap::new();
    for n in metadata["resolve"]["nodes"].as_array().into_iter().flatten() {
        let Some(id) = n["id"].as_str() else { continue };
        let list = deps.entry(id).or_default();
        // Absent `features` means this graph doesn't say, so optional deps are followed as before.
        // A reconciled set (even an empty one) is authoritative for this platform.
        let enabled_node = n.get("features").and_then(Value::as_array);
        let enabled_owned = reconciled.get(id).cloned();
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
            let inactive = if let Some(owned) = &enabled_owned {
                inactive_optional(pkgs.get(id).copied(), owned, d["name"].as_str().unwrap_or(""), kinds)
            } else {
                enabled_node.is_some_and(|feats| {
                    let owned: Vec<String> = feats.iter().filter_map(|f| f.as_str().map(str::to_string)).collect();
                    inactive_optional(pkgs.get(id).copied(), &owned, d["name"].as_str().unwrap_or(""), kinds)
                })
            };
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
fn inactive_optional(pkg: Option<&Value>, enabled: &[String], dep_name: &str, kinds: &[Value]) -> bool {
    let Some(pkg) = pkg else { return false };
    let Some(specs) = pkg.get("dependencies").and_then(Value::as_array) else { return false };
    let matching: Vec<&Value> = specs.iter().filter(|s| spec_is_edge(s, dep_name)).collect();
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

fn optional_activated(pkg: &Value, enabled: &[String], dep: &str) -> bool {
    if enabled.iter().any(|f| same_dep(f, dep)) {
        return true;
    }
    let Some(features) = pkg.get("features").and_then(Value::as_object) else { return false };
    for feat in enabled {
        let Some(items) = features.get(feat).and_then(Value::as_array) else { continue };
        if items.iter().any(|item| item.as_str().is_some_and(|s| feature_activates(s, dep))) {
            return true;
        }
    }
    false
}

/// Does this one feature entry turn on the optional dependency `dep`?
fn feature_activates(spec: &str, dep: &str) -> bool {
    if let Some(name) = spec.strip_prefix("dep:") {
        return same_dep(name, dep);
    }
    // Weak: `ring?/alloc` enables a feature of ring only when ring is already on.
    if spec.split_once("?/").is_some() {
        return false;
    }
    if let Some((name, _)) = spec.split_once('/') {
        return same_dep(name, dep);
    }
    same_dep(spec, dep)
}

/// Cargo's resolve edge names use underscores (`objc2_exception_helper`). Manifest dependency
/// names keep hyphens (`objc2-exception-helper`). They are the same dependency.
fn same_dep(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes().zip(b.bytes()).all(|(x, y)| x == y || matches!((x, y), (b'-', b'_') | (b'_', b'-')))
}

fn spec_is_edge(spec: &Value, edge_name: &str) -> bool {
    spec["name"].as_str().is_some_and(|n| same_dep(n, edge_name))
        || spec["rename"].as_str().is_some_and(|n| same_dep(n, edge_name))
}

/// The name a feature string uses for this dependency: the rename, or the package name.
fn dep_feature_name(spec: &Value) -> &str {
    spec["rename"].as_str().or_else(|| spec["name"].as_str()).unwrap_or("")
}

fn feature_names(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(|f| f.as_str().map(str::to_string)).collect())
        .unwrap_or_default()
}

fn expand_features(pkg: Option<&Value>, seeds: BTreeSet<String>) -> BTreeSet<String> {
    let Some(map) = pkg.and_then(|p| p.get("features")).and_then(Value::as_object) else {
        return seeds;
    };
    let mut set = seeds;
    let mut grew = true;
    while grew {
        grew = false;
        for feat in set.clone() {
            let Some(items) = map.get(&feat).and_then(Value::as_array) else { continue };
            for item in items {
                let Some(item) = item.as_str() else { continue };
                // `dep:name`, `name/feat` and `name?/feat` talk about another crate.
                if item.starts_with("dep:") || item.contains('/') || item.contains('?') {
                    continue;
                }
                if set.insert(item.to_string()) {
                    grew = true;
                }
            }
        }
    }
    set
}

/// Features actually requested by dependencies that are in this platform's resolve.
///
/// Only nodes whose incoming manifest specs carry a `features` array are returned. Every
/// other node keeps the feature list on the resolve node (the unit-test graphs, and crates
/// whose specs don't say).
fn reconciled_features(metadata: &Value) -> HashMap<&str, Vec<String>> {
    let nodes: Vec<&Value> = metadata["resolve"]["nodes"].as_array().into_iter().flatten().collect();
    let mut pkgs: HashMap<&str, &Value> = HashMap::new();
    for p in metadata["packages"].as_array().into_iter().flatten() {
        if let Some(id) = p["id"].as_str() {
            pkgs.insert(id, p);
        }
    }
    let members: BTreeSet<&str> = metadata["workspace_members"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    let mut incoming: HashMap<&str, Vec<(&str, &Value)>> = HashMap::new();
    for n in &nodes {
        let Some(id) = n["id"].as_str() else { continue };
        for d in n["deps"].as_array().into_iter().flatten() {
            let Some(pkg) = d["pkg"].as_str() else { continue };
            incoming.entry(pkg).or_default().push((id, d));
        }
    }
    let explicit: BTreeSet<&str> = incoming
        .iter()
        .filter(|(id, edges)| !members.contains(*id) && edges.iter().any(|(parent, edge)| edge_has_feature_spec(pkgs.get(parent).copied(), edge)))
        .map(|(id, _)| *id)
        .collect();

    let mut computed: HashMap<&str, BTreeSet<String>> = HashMap::new();
    for _ in 0..64 {
        let mut changed = false;
        for id in &explicit {
            let mut seeds = BTreeSet::new();
            for (parent, edge) in incoming.get(id).into_iter().flatten() {
                let kinds = edge["dep_kinds"].as_array().map(Vec::as_slice).unwrap_or(&[]);
                let dev_only = !kinds.is_empty() && kinds.iter().all(|k| k["kind"].as_str() == Some("dev"));
                if dev_only {
                    continue;
                }
                let parent_pkg = pkgs.get(parent).copied();
                let parent_feats = parent_feature_names(&computed, &members, parent_pkg, parent, &nodes);
                if inactive_optional(parent_pkg, &parent_feats, edge["name"].as_str().unwrap_or(""), kinds) {
                    continue;
                }
                for spec in matching_specs(parent_pkg, edge["name"].as_str().unwrap_or("")) {
                    if spec.get("features").and_then(Value::as_array).is_none() {
                        continue;
                    }
                    let use_kinds: Vec<&Value> = if kinds.is_empty() {
                        Vec::new()
                    } else {
                        kinds.iter().filter(|k| k["kind"].as_str() != Some("dev")).collect()
                    };
                    let kind_ok = |spec: &Value, kind: Option<&Value>| -> bool {
                        let sk = spec["kind"].as_str();
                        let st = spec.get("target").and_then(Value::as_str);
                        let (kk, kt) = kind
                            .map(|k| (k["kind"].as_str(), k.get("target").and_then(Value::as_str)))
                            .unwrap_or((None, None));
                        sk == kk && st == kt
                    };
                    let applies = if use_kinds.is_empty() {
                        kind_ok(spec, None)
                    } else {
                        use_kinds.iter().any(|k| kind_ok(spec, Some(k)))
                    };
                    if !applies {
                        continue;
                    }
                    for feat in spec["features"].as_array().into_iter().flatten().filter_map(Value::as_str) {
                        seeds.insert(feat.to_string());
                    }
                    if spec["uses_default_features"].as_bool() == Some(true) {
                        seeds.insert("default".to_string());
                    }
                    let dep_name = dep_feature_name(spec);
                    if let Some(map) = parent_pkg.and_then(|p| p.get("features")).and_then(Value::as_object) {
                        for feat in &parent_feats {
                            let Some(items) = map.get(feat).and_then(Value::as_array) else { continue };
                            for item in items.iter().filter_map(Value::as_str) {
                                if let Some((name, rest)) = item.split_once("?/") {
                                    if same_dep(name, dep_name) {
                                        seeds.insert(rest.to_string());
                                    }
                                } else if let Some((name, rest)) = item.split_once('/') {
                                    if same_dep(name, dep_name) {
                                        seeds.insert(rest.to_string());
                                    }
                                }
                            }
                        }
                    }
                }
            }
            let next = expand_features(pkgs.get(id).copied(), seeds);
            if computed.get(id) != Some(&next) {
                computed.insert(id, next);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    computed.into_iter().map(|(id, set)| (id, set.into_iter().collect())).collect()
}

fn edge_has_feature_spec(parent: Option<&Value>, edge: &Value) -> bool {
    let kinds = edge["dep_kinds"].as_array().map(Vec::as_slice).unwrap_or(&[]);
    let dev_only = !kinds.is_empty() && kinds.iter().all(|k| k["kind"].as_str() == Some("dev"));
    if dev_only {
        return false;
    }
    matching_specs(parent, edge["name"].as_str().unwrap_or(""))
        .into_iter()
        .any(|s| s.get("features").and_then(Value::as_array).is_some())
}

fn matching_specs<'a>(pkg: Option<&'a Value>, edge_name: &str) -> Vec<&'a Value> {
    pkg.and_then(|p| p.get("dependencies"))
        .and_then(Value::as_array)
        .map(|specs| specs.iter().filter(|s| spec_is_edge(s, edge_name)).collect())
        .unwrap_or_default()
}

/// Features of `id` as known so far. Workspace members use the resolve node's list (what this
/// package itself asked for). A dependency uses the set we have recomputed, or nothing yet.
fn parent_feature_names(
    computed: &HashMap<&str, BTreeSet<String>>,
    members: &BTreeSet<&str>,
    pkg: Option<&Value>,
    id: &str,
    nodes: &[&Value],
) -> Vec<String> {
    if members.contains(id) {
        let node = nodes.iter().find(|n| n["id"].as_str() == Some(id));
        return expand_features(pkg, feature_names(node.and_then(|n| n.get("features"))).into_iter().collect())
            .into_iter()
            .collect();
    }
    computed.get(id).map(|s| s.iter().cloned().collect()).unwrap_or_default()
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

    /// `cargo metadata --filter-platform` can list a feature that no dependency in this resolve
    /// asked for (objc2's `exception`, unified from a crate that is not in the macOS graph).
    /// The resolve edge name uses underscores; the manifest name keeps hyphens.
    #[test]
    fn cross_target_feature_does_not_follow_optional_dep() {
        let phantom = objc_exception(&[]);
        assert!(forbidden(&phantom).is_empty(), "{:?}", forbidden(&phantom));
        let real = objc_exception(&["exception"]);
        assert_eq!(forbidden(&real), ["cc"]);
    }

    /// A feature of the parent (`turn-on = ["leaf/bad"]`) still enables the optional dep.
    #[test]
    fn parent_feature_map_still_enables_optional_dep() {
        let id = |n: &str| format!("registry+x#{n}@1.0.0");
        let edge = |name: &str, kind: &str| {
            let kind = if kind == "normal" { Value::Null } else { Value::String(kind.into()) };
            serde_json::json!({"name": name, "pkg": id(name), "dep_kinds": [{"kind": kind}]})
        };
        let m = serde_json::json!({
            "packages": [
                {"id": id("app"), "name": "app", "dependencies": [
                    {"name": "mid", "kind": null, "optional": false, "features": ["turn-on"], "uses_default_features": false}
                ]},
                {"id": id("mid"), "name": "mid", "dependencies": [
                    {"name": "leaf", "kind": null, "optional": false, "features": [], "uses_default_features": false}
                ], "features": {"turn-on": ["leaf/bad"]}},
                {"id": id("leaf"), "name": "leaf", "dependencies": [
                    {"name": "cc", "kind": null, "optional": true, "features": [], "uses_default_features": false}
                ], "features": {"bad": ["dep:cc"]}},
                {"id": id("cc"), "name": "cc"}
            ],
            "workspace_members": [id("app")],
            "resolve": {"nodes": [
                {"id": id("app"), "features": [], "deps": [edge("mid", "normal")]},
                {"id": id("mid"), "features": ["turn-on", "not-from-this-platform"], "deps": [edge("leaf", "normal")]},
                {"id": id("leaf"), "features": ["bad", "not-from-this-platform"], "deps": [edge("cc", "normal")]},
                {"id": id("cc"), "features": [], "deps": []}
            ]}
        });
        assert_eq!(forbidden(&m), ["cc"]);
    }

    #[test]
    fn x11_dl_pkg_config_probe_is_excused() {
        let m = graph(&[
            ("stl-app", &[("winit", "normal")]),
            ("winit", &[("x11-dl", "normal")]),
            ("x11-dl", &[("pkg-config", "build")]),
            ("pkg-config", &[]),
        ]);
        assert_eq!(forbidden(&m), ["pkg-config"]);
        assert!(forbidden_except(&m, &LINUX_ALLOW).is_empty());
        // cc from x11-dl is not excused, and neither is pkg-config from a crate off the list.
        let cc = graph(&[("stl-app", &[("x11-dl", "normal")]), ("x11-dl", &[("cc", "build")]), ("cc", &[])]);
        assert_eq!(forbidden_except(&cc, &LINUX_ALLOW), ["cc"]);
    }

    /// objc2 shape: optional helper, hyphen in the manifest, underscore on the resolve edge.
    /// `requested` is what this platform's dependency spec actually asks for.
    fn objc_exception(requested: &[&str]) -> Value {
        let id = |n: &str| format!("registry+x#{n}@1.0.0");
        let edge = |name: &str, pkg: &str| serde_json::json!({"name": name, "pkg": id(pkg), "dep_kinds": [{"kind": null}]});
        serde_json::json!({
            "packages": [
                {"id": id("app"), "name": "app", "dependencies": [
                    {"name": "objc2", "kind": null, "optional": false, "features": requested, "uses_default_features": true}
                ]},
                {"id": id("objc2"), "name": "objc2", "dependencies": [
                    {"name": "objc2-exception-helper", "kind": null, "optional": true, "features": [], "uses_default_features": false}
                ], "features": {
                    "default": ["std"],
                    "std": ["alloc"],
                    "alloc": [],
                    "exception": ["dep:objc2-exception-helper"],
                    "disable-encoding-assertions": []
                }},
                {"id": id("objc2-exception-helper"), "name": "objc2-exception-helper", "dependencies": [
                    {"name": "cc", "kind": "build", "optional": false, "features": [], "uses_default_features": false}
                ]},
                {"id": id("cc"), "name": "cc"}
            ],
            "workspace_members": [id("app")],
            "resolve": {"nodes": [
                {"id": id("app"), "features": [], "deps": [edge("objc2", "objc2")]},
                {"id": id("objc2"), "features": ["alloc", "default", "std", "exception", "disable-encoding-assertions"],
                    "deps": [edge("objc2_exception_helper", "objc2-exception-helper")]},
                {"id": id("objc2-exception-helper"), "features": [], "deps": [edge("cc", "cc")]},
                {"id": id("cc"), "features": [], "deps": []}
            ]}
        })
    }
}
