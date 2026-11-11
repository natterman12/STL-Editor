# STL Editor 2.0 Milestone 2, Batches 4 and 5 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task by task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The nine printing tools, Support Fins and Draw Walls work in Rust and match 1.x on golden files. Support Fins runs its own engine file fast on every target, falls back safely, and updates itself. Milestone 2 ends with all 37 feature types ported.

**Architecture:**

- **Printing tools:** ported as one feature file each, with shared helpers in `stl-engine/src/features/common/` and the thread and fitting geometry in `stl-mesh`.
- **Support Fins calls:** the port of `supportfins.py` sits on a small `FinsHost` trait in `stl-engine`.
- **Engine hosts:** desktop hosts live in `stl-platform::fins` (a hidden `wry` webview, or Boa as the fallback); the web host is a `wasm-bindgen` bridge in `stl-app`.
- **Updates:** go through a new pure-Rust HTTPS client in `stl-platform::net`.

**Tech Stack:**

- Everything from batches 1–3.
- New, allowed: `wry =0.57.2` and `tao =0.37.2` (desktop only, in `stl-platform`), `boa_engine =0.22.0` (desktop only), and `rustls` with a pure-Rust crypto provider, plus `webpki-roots`, for `net.rs`.
- No other new dependency without the lead's approval.

**Spec:** `dev/specs/2026-11-11-stl-editor-2-m2-batch45-design.md`, under the main 2.0 spec and the batch 1–3 sub-specs.

## Global Constraints

- **Earlier plans still apply.** Everything in the batch 1–3 plans' Global Constraints carries over:
  - one file per feature
  - messages copied word for word
  - `EXCEPTIONS.md` and `requires`
  - the tool paths
  - pure Rust (apart from section 4 of the spec) and the wasm32 builds of the engine crates
  - volume and area within 0.1 percent
  - no golden input from `dev/bench/test_mesh.stl`
  - no commits or pushes
- **Run rules (spec section 1):** don't stop for decisions; park the item and report it at the end. Read-only reviews by a second worker are for Rust code ports only.
- **Support Fins engine:**
  - Golden files and tests use the bundled `web/vendor/support-fins/engine.iife.js` (v1.0.5) only, never a downloaded one, with update checks off.
  - The host calls are the same names and string arguments 1.x passes to `ctx.call(f"SupportFinsEngine.{name}", ...)`.
- **Crate boundaries:**
  - `wry`, `tao` and `boa_engine` appear only in `rust/crates/stl-platform`.
  - `wasm-bindgen` and `js-sys` for Support Fins appear only in `stl-app`'s web target.
  - `stl-engine` gains no new dependency.
- **New user-facing text, verbatim:**
  - the Boa notice: `"Support Fins is running in slow mode because this computer's built-in web engine isn't available."`, plus on Windows `" Install the Microsoft Edge WebView2 Runtime to make it fast."`
  - the timeout: `"Support Fins: the engine stopped responding."`
- **Matching for engine results across hosts:** integer stats exact; float stats within 1e-6 relative; `report` text and piece count exact; solids per main spec 5.3.

## Review Focus

1. **The webview host dies mid-call** (the WebView2 process is killed, or the window fails to create). The call returns an error and the next call starts a fresh host. If the host can't start at all, Boa takes over. Nothing panics or hangs. Test: Task 2 `webview_crash_recovers`, which kills the browser child process between two calls.
2. **Two features call Support Fins at the same time**, for example the live preview and OK. Calls are serialized through one host, as 1.x serializes with `_lock`, and both return correct results. Test: Task 2 `concurrent_calls_serialize`.
3. **A downloaded engine is corrupt, cut off, or for another `api`.** It's never used, the bundled or previous valid engine stays active, and `STATE.error` records why. Test: Task 3 `bad_update_refused`, with three variants.
4. **A body that isn't a watertight solid gets Support Fins.** The supports are added as a separate body with 1.x's warning. Test: a Task 1 golden case `fins_raw_body`.
5. **Plate Layout of a body taller than the bed, or of too many bodies.** It gives 1.x's exact messages, including the `_mm` number formatting. Tests: Task 1 golden cases `plate_too_tall` and `plate_no_room`.

---

### Task 0: Sculpt speed (left over from batch 3)

**Owner:** Worker 6. **Review:** Sonnet A. It runs alongside Task 1, because it touches only `rust/crates/stl-kernel/src/solid.rs` and, if needed, `rust/crates/stl-mesh/src/sculpt.rs`.

The batch 3 bench row "Sculpt Smooth, 20 pts" measured 0.87 to 1.11 across quiet runs; 1.x's own time varies between 0.31 and 0.41 s. The remaining time is in `Solid::import_mesh` in `stl-kernel`, the step that turns the edited mesh back into a solid.

- [ ] **Step 1:** Profile Sculpt on the bench mesh, then make `import_mesh` faster with identical results. Every existing test, golden check and parity case must be unchanged.
- [ ] **Step 2:** Run the bench with nothing else running, 3 times. Expected: Sculpt at or below 0.9 in every run, and no other row slower than before.

### Task 1: Test models, golden scripts, case files and CI

**Owner:** Cursor. **Check:** the lead.

**Files:**
- **Modify:**
  - `dev/parity/make_models.py`: add `t_bracket.stl`, `lipped_shelf.stl`, `bridge_ring.stl` (each under 5,000 triangles, with overhangs when z is up) and `hole_and_pin.stl`
  - `dev/parity/make_golden.py`: a case's `"ctx"` object is passed to the evaluator as `ctx`; update checks are off while it runs
  - `rust/crates/stl-engine/tests/parity.rs`: the same `ctx` field; the new feature types are added to the batch list
  - `.github/workflows/rust.yml`: the golden job runs `make_fins_golden.py --check`; the Linux job runs `sudo apt-get install -y libwebkit2gtk-4.1-dev xvfb`
- **Create:**
  - `dev/parity/make_fins_golden.py` → `rust/tests/parity/fins_golden.json`
  - `dev/parity/fins_update_server.py`: a local HTTP server for Task 3 that serves a `VERSION.json` and engine from a folder given on the command line
  - `rust/tests/parity/features/{bedsplit,layflat,plate,pattern,thread,resizehole,fittest,snapfit,hinge,supportfins,supportwall}.json`

**Interfaces:**
- Produces the fins golden format: `{case: {"model": str, "options": {...}, "stats": {...}, "pieces": n, "report": str, "triangles_sha256": str, "over": n, "small": n}}`.
  - It comes from 1.x `SF._call("computeFinsB64", SF._soup(v, f), SF._options(options))` and `reportLine`.
  - `triangles_sha256` is of the decoded `<f4` bytes.
  - Case names come from spec section 6.

- [ ] **Step 1:** Add the four models and all cases from spec section 6, plus the Review Focus cases `fins_raw_body`, `plate_too_tall` and `plate_no_room`. Each feature has at least 4 cases and at least one 1.x error. Remove `requires: ["thread"]` from the batch 2 Hole cases.
- [ ] **Step 2:** Write `make_fins_golden.py` and run it, then run it with `--check` (exit 0). Then run `make_golden.py` and `make_golden.py --check`.
- [ ] **Step 3:** Run `cargo test -p stl-engine --test parity -- --nocapture`. Expected: the new cases are pending and 0 failed.

### Task 2: Support Fins hosts (`FinsHost`, webview, Boa, host choice)

**Owner:** Worker 6. **Review:** Sonnet A.

**Files:**
- **Create:**
  - `rust/crates/stl-engine/src/fins_host.rs` (the trait, `HostKind`, `NullHost`, which errors "Support Fins isn't available in this build.")
  - `rust/crates/stl-platform/src/fins/{mod.rs,webview.rs,boa.rs}`
- **Modify:** `rust/crates/stl-platform/Cargo.toml` (target-specific `wry`, `tao`, `boa_engine`) and `rust/xtask/src/pure_rust.rs` (spec section 4)
- **Test:** `rust/crates/stl-platform/tests/fins_host.rs`

**Interfaces:**
- Produces:

```rust
// stl-engine
pub enum HostKind { Webview, Browser, Boa, None }
pub trait FinsHost: Send + Sync {
    fn call(&self, name: &str, args: &[&str]) -> Result<String, String>;  // SupportFinsEngine.<name>(...args), string result
    fn has(&self, name: &str) -> bool;                                    // typeof SupportFinsEngine.<name> === 'function'
    fn host_kind(&self) -> HostKind;
}
// stl-platform
pub fn start_fins_host(engine_js: &Path) -> Arc<dyn FinsHost>;   // webview first, Boa if it can't start
pub struct WebviewHost { /* hidden wry window on its own thread, requests over a channel */ }
pub struct BoaHost { /* one boa Context behind a Mutex */ }
```

- **The webview host:**
  - Serves `engine.js` and per-call inputs through a custom protocol (`sf://localhost/...`), as the 2026-11-11 measurement did, so large base64 inputs aren't pasted into scripts.
  - Each call has a 120 s limit. On a timeout or a dead process it returns `Err("Support Fins: the engine stopped responding.")` and the host is rebuilt on the next call.
  - **Windows and Linux:** the host thread uses `tao`'s `with_any_thread(true)`.
  - **macOS:** `start_fins_host` returns a `BoaHost` until milestone 4 adds a main-thread hook. This is noted in the code and in `EXCEPTIONS.md`.
- **The pure-Rust check:**
  - Add the Linux-only allow-list, naming each `-sys` crate from `cargo tree -p stl-platform --target x86_64-unknown-linux-gnu -e build`.
  - Add a test that the check still fails for a `cc` build dependency on any other crate.

- [ ] **Step 1:** Write the failing tests:
  - `webview_matches_fins_golden`: every `fins_golden.json` case on the webview host, using the spec section 5 rules
  - `boa_matches_fins_golden_t_bracket`
  - `webview_crash_recovers`
  - `concurrent_calls_serialize`
  - `has_reports_extras`: every extra listed in the bundled `VERSION.json` gives `true`
- [ ] **Step 2:** Run `cargo test -p stl-platform --test fins_host --release`. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run the same command, then:
  - `cargo run -p xtask -- pure-rust`
  - the wasm32 build of the engine crates

  Expected: PASS.

### Task 3: Update check and HTTPS client

**Owner:** Worker 6, after Task 2. **Review:** Worker 7.

**Files:**
- **Create:** `rust/crates/stl-platform/src/net.rs`, `rust/crates/stl-platform/src/fins/update.rs`, and `dev/parity/fins_update_server.py`, a local test server that Cursor writes in Task 1
- **Test:** `rust/crates/stl-platform/tests/fins_update.rs`

**Interfaces:**

```rust
pub fn https_get(url: &str, timeout: Duration) -> Result<Vec<u8>, NetError>;   // rustls + pure-Rust crypto, webpki roots
pub struct FinsState { pub pending: Option<String>, pub error: Option<String>, pub checked: Option<i64> }
pub fn choose_engine(bundled: &Path, user_dir: &Path) -> EngineInfo;           // _choose / _read_info / _digest / _semver
pub fn check_update(settings: &Settings, force: bool) -> FinsState;            // check_update, at most once a day
pub fn published_url(repo: Option<&str>) -> String;                            // published_url
```

- Copy `_digest` exactly: SHA-256 of the bytes with `\r\n` replaced by `\n`.
- A newer valid engine is written to `<config dir>/support-fins/` and becomes active only at the next `choose_engine`.
- Tests may point `published_url` at the local server with an `http://127.0.0.1` URL. The real one is always HTTPS.

- [ ] **Step 1:** Write the failing tests:
  - `newer_engine_used_next_start`
  - `bad_update_refused`, for a bad checksum, a cut-off file and a different `api`
  - `offline_check_is_quiet`
  - `once_a_day`
- [ ] **Step 2:** Run `cargo test -p stl-platform --test fins_update`. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run it, plus the pure-Rust check. Expected: PASS.

### Task 4: Support Fins and Draw Walls features

**Owner:** Sonnet B. It can start after Task 1, using a test `FinsHost` that replays `fins_golden.json`, and run the real-host check once Task 2 lands. **Review:** Worker 6.

**Files:**
- **Create:**
  - `rust/crates/stl-engine/src/features/common/fins/{mod.rs,results.rs,walls.rs,orient.rs}`
  - `features/supportfins.rs`, `features/supportwall.rs`
  - `rust/crates/stl-mesh/src/nearest.rs`: `closest_point_on_triangles`

**Interfaces:**
- Consumes: `FinsHost` (Task 2) and batch 1's `split`.
- Produces: the `supportfins.py` port with the same names and returns, for example `fins(host, verts, faces, values) -> FinsResult` and `walls(host, verts, faces, wall_list, values) -> WallsResult`. It also produces `suggest`, `strength`, `wall`, `info` and `nearest_face` for milestone 4.
- The cache sizes 6 and 64 are keyed as `_mesh_key` does: a hash of the float32 vertices and the int64 faces, plus the options string.
- `SUPPORT_COLOR` is `"#59d98e"`; a removed ghost is `"#d64545"`.

- [ ] **Step 1:** Port `f_supportfins`, `f_supportwall`, `_add_supports` and `drawn_walls`, with every message and info string word for word. Include the preview paths (`~sf`, `~p{i}`, `~w{i}` ids, ghosts and `previewOnly`).
- [ ] **Step 2:** Run `cargo test -p stl-engine --test parity -- --nocapture` with the real webview host. Expected: every `supportfins` and `supportwall` case passes, including `fins_raw_body`.

### Task 5: Fit to Bed, Lay Flat, Plate Layout, Pattern

**Owner:** Sonnet A. **Review:** Worker 7.

**Files:**
- Create: `features/common/{printers,bed,pose}.rs` and `features/{bedsplit,layflat,plate,pattern}.rs`

**Interfaces:**
- `PRINTERS` and the connector table use the 1.x keys and values exactly.
- **Ported from `printing.py`, with the same arguments:** `plan_cuts`, `split_to_bed`, `best_rest_normal` and `lay_flat_matrix`.
- **Plate Layout's ordering:** the sort `(-dy, -dx, i)`, the row rule and the `1e-9` tolerances are copied exactly.
- **Pattern** reuses batch 1's `_repeat_features` for `source: "features"`.

- [ ] **Step 1:** Port each feature, with messages word for word, including `_mm`'s `f"{round(x, 1):g}"` formatting.
- [ ] **Step 2:** Run `cargo test -p stl-engine --test parity -- --nocapture`. Expected: all four features' cases pass, including `plate_too_tall` and `plate_no_room`.

### Task 6: Thread, Resize Hole, Fit Test, Snap-fit, Living hinge

**Owners:**
- Worker 6: `stl-mesh/src/thread.rs` and Thread
- Sonnet B: `stl-mesh/src/fit.rs`, `features/common/round.rs`, Resize Hole, Fit Test, Snap-fit, Living hinge

**Review:** Worker 7 reviews Thread; Worker 6 reviews the rest.

**Interfaces:**

```rust
pub fn thread_tool(kind: &str, d: f64, p: f64, l: f64, tolerance: f64, hand: &str, chamfer: bool) -> Result<Solid, MeshError>;  // printing.thread_tool
pub fn nearest_triangle(v: &[[f32; 3]], f: &[[u32; 3]], point: [f64; 3]) -> usize;
pub fn fit_cylinder(v: &[[f32; 3]], f: &[[u32; 3]], tri: usize, max_angle: f64) -> Result<Cylinder, MeshError>;  // max_angle 35.0
pub fn resize_round(s: &Solid, cyl: &Cylinder, diameter: Option<f64>, fill: bool) -> Solid;
```

- [ ] **Step 1:** Port each, with messages word for word.
- [ ] **Step 2:** Run `cargo test -p stl-engine --test parity -- --nocapture`. Expected: these five features' cases pass, plus the batch 2 Hole thread cases.

### Task 7: Web `FinsHost` bridge

**Owner:** Worker 7, after Task 2. **Review:** Sonnet B.

**Files:**
- Create: `rust/crates/stl-app/src/web/fins.rs`
- Modify: `rust/crates/stl-app/Cargo.toml` (wasm32-only `wasm-bindgen`, `js-sys`)

**Interfaces:**
- `pub struct BrowserHost;` implements `FinsHost` by calling `globalThis.SupportFinsEngine[name](...args)`.
- `host_kind()` is `Browser`.
- Loading the engine file stays as 1.x web does it. The bridge assumes it's already loaded, and `has()` returns false when it isn't.

- [ ] **Step 1:** Implement it, and add a `wasm-bindgen-test` run in headless Chrome (the CI golden job already has Chrome) that calls `optionsSchemaJson` and `computeFinsB64` on the T-bracket. Compare the result with `fins_golden.json`.
- [ ] **Step 2:** Run `wasm-pack test --headless --chrome rust/crates/stl-app` (or the repo's equivalent runner) and the wasm32 build. Expected: PASS.

### Task 8: Bench rows

**Owner:** Cursor. **Check:** the lead.

- [ ] **Step 1:** Add the spec section 7 item 3 rows to `rust/xtask/src/bench.rs` and `dev/parity/bench_1x.py`. Support Fins uses the webview host against 1.x mini-racer.
- [ ] **Step 2:** Run `cargo run --release -p xtask -- bench` while nothing else is running. Any row above 1.0 goes to that tool's owner for one speed-up pass.

### Task 9: Close-out

**Owner:** the lead.

- [ ] **Step 1:** Run:
  - `cargo test --workspace --release`
  - the pure-Rust check
  - the wasm32 builds
  - the parity runner (0 failed, 0 pending, 37 feature types registered)
  - every `--check` script
  - `open_check.py`
  - the bench
  - the 1.x `pytest`
- [ ] **Step 2:** Update `EXCEPTIONS.md` with:
  - the Linux WebKitGTK entry
  - the macOS Boa entry
  - the 120 s limit
  - any entries pending Nathan's OK

  Update `PERF_GAPS.md`.
- [ ] **Step 3:** Report the result lines, the bench table, the host each Support Fins test used, and every parked question. Do not commit.
