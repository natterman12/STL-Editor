//! Milestone 4b inside the app: the gizmo, Sculpt, Support Fins (its panel, previews, coverage,
//! strength and suggestions) and Draw Walls, wired to the open tool. This is a child of `app.rs`
//! so it reads the app's own state; `app.rs` calls in at the places 1.x's `app.js` does.

use super::*;
use crate::engine_link::FinsAsk;
use crate::supports::coverage::{CoverageInput, CoverageJob, CoverageJobResult, CoverageOutput};
use crate::supports::strength::{self, Cover, Plastic, Spot, SuggestUi};
use stl_engine::params::resolve_deep;
use crate::supports::walls::{self, Wall, WallClickHit, WallClickMsg, WallDrawer};
use crate::supports::{self, PanelEvent, Session};
use crate::tools::gizmo::{self, Gizmo, GizmoBody, GizmoDrop, GizmoEvent, GizmoKind};
use crate::tools::preview::{MsgKind, ToolMsg};
use crate::tools::sculpt::{Sculpt, SculptEvent};
use crate::viewport::camera::{self, M4};
use crate::viewport::pick::Hit;
use egui::Color32;
use serde_json::{json, Value};
use std::sync::Arc;

/// 1.x's `localStorage` keys.
pub const MATERIAL_KEY: &str = "stle_sf_material";
pub const REACH_KEY: &str = "stle_sf_reach";

const IDENT: M4 = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];

/// The Support Fins turn applied so far (`t._pose`).
pub(super) struct Pose {
    fid: Option<String>,
    m: M4,
    c0: [f64; 3],
}

/// A turn sent and not yet seen back in the timeline.
struct PoseWait {
    before: Vec<String>,
    whole: M4,
    turn: M4,
}

/// What belongs to the Support Fins tools and Draw Walls, beside the open tool.
pub(super) struct Sf {
    /// Support Fins was asked for while its settings list was still being read.
    pub waiting: Option<(String, Option<String>)>,
    pub about_wanted: bool,
    seq: u64,
    strength_seq: u64,
    suggest_seq: u64,
    suggest_gen: u64,
    pub cover: CoverageJob,
    /// The record the last preview came with (its paint, walls, pieces).
    cover_last: Option<(String, Vec<u32>)>,
    /// The pieces of the last preview, minus the part and ghosts: what coverage measures against.
    fin_meshes: Vec<Arc<stl_kernel::MeshArrays>>,
    spots: Vec<crate::supports::coverage::BareSpot>,
    pub drawer: WallDrawer,
    pieces: Value,
    /// The folding sections as last saved: the session's own copy where the platform keeps nothing.
    sections: Option<String>,
    pose: Option<Pose>,
    posing: bool,
    pose_gen: u64,
    pose_wait: Option<PoseWait>,
    /// A Move, Scale or Lay Flat put before the part's supports is on its way: the steps there were,
    /// the note to show once it is in (1.x `addFeatureSmart`).
    insert_note: Option<(Vec<String>, Level, String)>,
    /// A handle's edit of the step is on its way (so a failure can be told to the tool panel).
    edit_wait: bool,
    /// What the strokes and walls overlays were last drawn for.
    sculpt_sig: String,
    walls_sig: String,
    hover_pt: Option<[f64; 3]>,
}

impl Default for Sf {
    fn default() -> Self {
        Sf {
            waiting: None,
            about_wanted: false,
            seq: 0,
            strength_seq: 0,
            suggest_seq: 0,
            suggest_gen: 0,
            cover: CoverageJob::new(),
            cover_last: None,
            fin_meshes: Vec::new(),
            spots: Vec::new(),
            drawer: WallDrawer::default(),
            pieces: Value::Null,
            sections: None,
            pose: None,
            posing: false,
            pose_gen: 0,
            pose_wait: None,
            insert_note: None,
            edit_wait: false,
            sculpt_sig: String::new(),
            walls_sig: String::new(),
            hover_pt: None,
        }
    }
}

fn col(c: u32) -> Color32 {
    Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8)
}

fn is_sf(key: &str) -> bool {
    matches!(key, "supportfins" | "supportwall")
}

fn wall_of(v: &Value) -> Option<Wall> {
    let shape = v.get("shape")?.as_str()?.to_owned();
    let pt = |p: &Value| -> Option<[f64; 3]> {
        let a = p.as_array()?;
        Some([a.first()?.as_f64()?, a.get(1)?.as_f64()?, a.get(2)?.as_f64()?])
    };
    let points = v.get("points")?.as_array()?.iter().filter_map(pt).collect();
    let normal = v.get("normal").and_then(pt);
    Some(Wall { shape, points, normal })
}

fn walls_of(values: &Values) -> Vec<Wall> {
    values.get("walls").and_then(Value::as_array).map(|a| a.iter().filter_map(wall_of).collect()).unwrap_or_default()
}

/// 1.x `insertBeforeSupports`: a Move, Scale or Lay Flat of bodies that already have supports goes
/// before the first of those support steps (so the fins are rebuilt for the new position instead
/// of being carried along with the part). `None`: at the end, as usual.
pub(super) fn insert_before_supports(tl: &TimelineView, marker: Option<usize>, kind: &str, params: &Value) -> Option<usize> {
    if !matches!(kind, "move" | "scale" | "layflat") || params.get("copy").and_then(Value::as_bool).unwrap_or(false) {
        return None;
    }
    let mut ids: Vec<&str> = params.get("bodies").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
    if ids.is_empty() {
        ids.extend(params.get("body").and_then(Value::as_str));
    }
    let end = marker.unwrap_or(tl.items.len());
    tl.items.iter().take(end).position(|f| {
        matches!(f.kind.as_str(), "supportfins" | "supportwall")
            && !f.suppressed
            && ids.iter().any(|b| {
                f.params.get("body").and_then(Value::as_str) == Some(*b)
                    || f.params.get("bodies").and_then(Value::as_array).is_some_and(|a| a.iter().any(|x| x.as_str() == Some(*b)))
            })
    })
}

/// The "link" the About dialog's Restart now sits behind; `app.rs` takes it instead of opening a page.
pub(super) const RESTART_URL: &str = "stle:restart";
/// The tag of the "unsaved changes" question before a restart.
pub(super) const RESTART_TAG: &str = "restart-app";

/// A helper that waits for this process to end, then starts `exe` (no arguments, no project).
#[cfg(not(target_arch = "wasm32"))]
fn spawn_after_exit(exe: &std::path::Path) -> std::io::Result<()> {
    restart_helper(exe, std::process::id()).spawn().map(|_| ())
}

/// The helper command: waits for `pid` to end, then starts `exe`. The program's path never goes
/// into the script text: it travels in the environment (`STLE_EXE`, or the shell's `$2`), so a
/// quote, a bracket or a `$` in the folder name cannot change what runs.
#[cfg(not(target_arch = "wasm32"))]
pub fn restart_helper(exe: &std::path::Path, pid: u32) -> std::process::Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let script = format!("try {{ Wait-Process -Id {pid} -Timeout 90 -ErrorAction Stop }} catch {{ }}; Start-Sleep -Milliseconds 700; [Diagnostics.Process]::Start($env:STLE_EXE) | Out-Null");
        let root = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
        let ps = std::path::Path::new(&root).join("System32").join("WindowsPowerShell").join("v1.0").join("powershell.exe");
        let mut c = std::process::Command::new(ps);
        c.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-WindowStyle", "Hidden", "-Command", &script])
            .env("STLE_EXE", exe)
            // DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW
            .creation_flags(0x0000_0008 | 0x0000_0200 | 0x0800_0000);
        c
    }
    #[cfg(not(windows))]
    {
        let script = "i=0; while kill -0 \"$1\" 2>/dev/null && [ $i -lt 450 ]; do sleep 0.2; i=$((i + 1)); done; sleep 0.7; exec \"$2\"";
        let mut c = std::process::Command::new("sh");
        c.arg("-c").arg(script).arg("stle-restart").arg(pid.to_string()).arg(exe);
        c
    }
}

fn round4(x: f64) -> f64 {
    (x * 1e4).round() / 1e4
}

/// A mesh as a flat run of triangle corners (the fins coverage measures against).
fn soup_of(m: &stl_kernel::MeshArrays) -> Vec<f32> {
    let mut out = Vec::with_capacity(m.triangles.len() * 3);
    for &i in &m.triangles {
        out.extend_from_slice(&m.positions[i as usize * 3..i as usize * 3 + 3]);
    }
    out
}

fn u32_list(v: Option<&Value>) -> Vec<u32> {
    v.and_then(Value::as_array).map(|a| a.iter().filter_map(|x| x.as_u64().map(|n| n as u32)).collect()).unwrap_or_default()
}

impl StlApp {
    pub(super) fn param_map(&self) -> BTreeMap<String, f64> {
        self.param_views.iter().filter_map(|p| p.value.map(|v| (p.name.clone(), v))).collect()
    }

    /// The tool's values with the project's parameters worked out (`resolveDeep(t.values)`).
    fn resolved_values(&self) -> Values {
        let Some(t) = self.tool.as_ref() else { return Values::new() };
        match resolve_deep(&Value::Object(t.values.clone()), &self.param_map()) {
            Ok(Value::Object(m)) => m,
            _ => t.values.clone(),
        }
    }

    fn tool_msg(&mut self, kind: MsgKind, text: impl Into<String>) {
        if let Some(t) = self.tool.as_mut() {
            t.msg = Some(ToolMsg::new(kind, text));
        }
    }

    fn fins_ask(&mut self, ask: FinsAsk) -> u64 {
        self.sf.seq += 1;
        let seq = self.sf.seq;
        self.link.send(Command::Fins { seq, ask });
        seq
    }

    fn sf_tool(&self) -> bool {
        self.tool.as_ref().is_some_and(|t| is_sf(t.def.key))
    }

    fn sf_mode(&self) -> String {
        self.tool.as_ref().map(|t| supports::mode_of(&t.values).to_owned()).unwrap_or_default()
    }

    /// The wall editor is on: Draw Walls, or Support Fins in Add fins.
    fn walls_active(&self) -> bool {
        self.tool.as_ref().is_some_and(|t| t.def.flags.wall_tool || (t.def.key == "supportfins" && supports::mode_of(&t.values) == "add"))
    }

    // ------------------------------------------------------------------ opening

    /// `openSupportTool`: read Support Fins' settings first (once), then open.
    pub(super) fn open_support(&mut self, key: &str, edit: Option<String>, values: Option<Values>) {
        if !supports::is_installed() {
            self.sf.waiting = Some((key.to_owned(), edit));
            self.status_text = supports::LOADING.to_owned();
            self.fins_ask(FinsAsk::Info);
            return;
        }
        self.open_support_now(key, edit, values);
    }

    fn open_support_now(&mut self, key: &str, edit: Option<String>, values: Option<Values>) {
        let ctx = make_ctx(&self.bodies, &self.planes, &self.sketches, &self.timeline_view.items, &self.shell.selection, self.shell.inch);
        let tool_key = key.to_owned();
        if edit.is_none() {
            // a part that already has supports: open those again instead of stacking a second set
            if let Some(bid) = supports::body_id(&supports::defaults(key, &ctx)) {
                if self.sf_reopen(key, &bid, values.clone()) {
                    return;
                }
            }
        }
        let Some(def) = supports::support_tool(&tool_key) else { return };
        match edit {
            Some(id) => {
                let Some(item) = self.timeline_view.items.iter().find(|i| i.id == id).cloned() else { return };
                self.open_tool_def(def, Some((item.id, item.params)), values);
            }
            None => self.open_tool_def(def, None, values),
        }
    }

    /// The Support Fins (or Draw Walls) step a part already has: open it for editing (`reopenSupports`).
    pub(super) fn sf_reopen(&mut self, key: &str, bid: &str, values: Option<Values>) -> bool {
        let name = self.bodies.iter().find(|b| b.id == bid).map(|b| b.name.clone()).unwrap_or_default();
        let Some(plan) = supports::reopen_plan(&self.timeline_view.items, key, bid, &name) else { return false };
        let Some(def) = supports::support_tool(plan.tool_key) else { return false };
        let Some(item) = self.timeline_view.items.iter().find(|i| i.id == plan.feature_id).cloned() else { return false };
        self.toasts.push(Level::Info, plan.toast);
        let mut v = values.unwrap_or_default();
        if let Some(m) = plan.mode {
            v.insert("_mode".into(), json!(m));
        }
        self.open_tool_def(def, Some((item.id, item.params)), Some(v));
        true
    }

    /// What opening a tool sets up beside its form: handles, the Sculpt brush, Support Fins' session.
    pub(super) fn after_open(&mut self, def: &'static ToolDef) {
        self.gizmo = None;
        self.sculpt = Sculpt::new();
        self.sf.cover_last = None;
        self.sf.fin_meshes.clear();
        self.sf.spots.clear();
        self.sf.pieces = Value::Null;
        self.sf.pose = None;
        self.sf.posing = false;
        self.sf.pose_wait = None;
        self.sf.sculpt_sig.clear();
        self.sf.walls_sig.clear();
        if def.flags.gizmo {
            let kind = match def.key {
                "scale" => GizmoKind::Scale,
                "supportfins" => GizmoKind::Pose,
                _ => GizmoKind::Move,
            };
            self.gizmo = Some(Gizmo::new(kind));
        }
        if def.flags.sculpt {
            if let Some(t) = self.tool.as_ref() {
                Sculpt::open(&mut self.viewport, &t.values);
            }
        }
        if is_sf(def.key) {
            self.open_sf_session(def);
        }
    }

    fn open_sf_session(&mut self, def: &'static ToolDef) {
        let ctx = make_ctx(&self.bodies, &self.planes, &self.sketches, &self.timeline_view.items, &self.shell.selection, self.shell.inch);
        let mut s = Session::open(def.key, &ctx);
        s.material = strength::material_key(&self.platform.setting(MATERIAL_KEY).unwrap_or_default()).to_owned();
        if let Some(text) = self.platform.setting(supports::panel::SECTIONS_KEY).or_else(|| self.sf.sections.clone()) {
            s.sections = supports::SectionMemory::from_json(&text);
        }
        s.reach = self.platform.setting(REACH_KEY).and_then(|v| supports::panel::parse_float_js(&v)).filter(|v| v.is_finite() && *v > 0.0).unwrap_or(supports::coverage::DEFAULT_REACH);
        self.sf.drawer = WallDrawer::new_with_remembered(|k| self.platform.setting(k));
        s.wall_shape = self.sf.drawer.shape.clone();
        let hint = self.sf.drawer.hint().to_owned();
        let tool = def.key;
        let (body, rotate) = {
            let Some(t) = self.tool.as_mut() else { return };
            s.edit_id = t.edit_id.clone();
            s.set_values(t.values.clone());
            t.credit = Some((supports::credit_line(supports::engine_version()), supports::CREDIT_TITLE.to_owned()));
            t.wall_hint = hint.clone();
            t.sf = Some(s);
            (supports::body_id(&t.values), supports::mode_of(&t.values) == "rotate")
        };
        self.shell.hint = supports::mode_hint(tool, self.tool.as_ref().map(|t| &t.values).unwrap_or(&Values::new()), &hint);
        if body.is_some() {
            self.sf_quick_paint();
            if tool == "supportfins" && rotate {
                self.sf_strength();
            }
        }
    }

    /// Everything that goes when the tool closes.
    pub(super) fn before_close(&mut self, t: &OpenTool) {
        if let Some(g) = self.gizmo.as_mut() {
            g.hide(&mut self.viewport);
        }
        self.gizmo = None;
        if t.def.flags.sculpt {
            self.sculpt.close(&mut self.viewport);
        }
        if t.def.flags.paint {
            for tag in ["sf-over", "sf-small", "sf-bare", "sf-held", "walls"] {
                self.viewport.clear_overlay(tag);
            }
            self.viewport.set_band(&[], None, Color32::WHITE);
            self.viewport.set_preview_hover(None);
            self.sf.cover.bump_generation();
            self.sf.drawer.reset();
        }
        self.sf.cover_last = None;
        self.sf.pose = None;
        self.sf.posing = false;
        self.sf.pose_wait = None;
        self.sf.waiting = None;
    }

    #[doc(hidden)]
    pub fn show_sf_about(&mut self) {
        if !supports::is_installed() {
            self.sf.about_wanted = true;
            self.fins_ask(FinsAsk::Info);
            return;
        }
        let Some(mut a) = supports::about(self.platform.is_web()) else { return };
        // a downloaded engine waits for the next start: its last line ends in a "Restart now" link
        if let Some(last) = a.paragraphs.last_mut() {
            if let Some(cut) = last.strip_suffix(" Restart now").map(str::to_owned) {
                *last = cut;
                a.links.insert(0, ("Restart now".into(), RESTART_URL.into()));
            }
        }
        self.shell.dialogs.open_alert("sf-about", a.title, &a.paragraphs.join("\n\n"), a.links);
    }

    /// 1.x `post('restart')`: start STL Editor again and close this one. Desktop only (the web
    /// edition gets new engines without a restart).
    ///
    /// 1.x keeps unsaved work for Recover Unsaved Work; 2.0 has no autosave until milestone 5, so
    /// unsaved changes are asked about first.
    #[doc(hidden)]
    pub fn restart_app(&mut self, ctx: &egui::Context) {
        if self.shell.dirty {
            self.shell.dialogs.open_confirm(RESTART_TAG, "You have unsaved changes. Restart without saving them?");
            return;
        }
        self.restart_now(ctx);
    }

    /// Start the new copy once this one has closed (1.x's updater script waits for the old process
    /// before it starts the new one), then close. Like 1.x, it starts with no project open.
    /// The helper that starts the new copy once this one has ended (tests look at what it would run).
    #[cfg(not(target_arch = "wasm32"))]
    #[doc(hidden)]
    pub fn restart_helper_command(exe: &std::path::Path, pid: u32) -> std::process::Command {
        restart_helper(exe, pid)
    }

    pub(super) fn restart_now(&mut self, ctx: &egui::Context) {
        #[cfg(not(target_arch = "wasm32"))]
        if let Ok(exe) = std::env::current_exe() {
            match spawn_after_exit(&exe) {
                Ok(()) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                Err(e) => self.toasts.push(Level::Error, format!("Couldn't restart STL Editor: {e}")),
            }
        }
        #[cfg(target_arch = "wasm32")]
        let _ = ctx;
    }

    // ------------------------------------------------------------------ the engine's answers

    pub(super) fn on_fins(&mut self, seq: u64, ask: &str, result: Result<Value, String>) {
        match ask {
            "info" => match result {
                Ok(v) => {
                    let _ = supports::install(&v);
                    if let Some((key, edit)) = self.sf.waiting.take() {
                        self.status_text.clear();
                        self.open_support_now(&key, edit, None);
                    }
                    if std::mem::take(&mut self.sf.about_wanted) {
                        self.show_sf_about();
                    }
                }
                Err(e) => {
                    self.sf.waiting = None;
                    self.sf.about_wanted = false;
                    self.status_text = e.clone();
                    self.toasts.push(Level::Error, e);
                }
            },
            "strength" => {
                if seq == self.sf.strength_seq {
                    if let Some(s) = self.tool.as_mut().and_then(|t| t.sf.as_mut()) {
                        s.strength = result.ok();
                    }
                }
            }
            "suggest" => {
                if seq != self.sf.suggest_seq {
                    return;
                }
                let stale = self.sf.posing || self.sf.suggest_gen != self.sf.pose_gen;
                if let Some(s) = self.tool.as_mut().and_then(|t| t.sf.as_mut()) {
                    s.suggest = match result {
                        _ if stale => SuggestUi::Idle,
                        Ok(r) => SuggestUi::Ready {
                            confidence: r.get("confidence").and_then(Value::as_str).unwrap_or("").to_owned(),
                            candidates: r.get("candidates").and_then(Value::as_array).cloned().unwrap_or_default(),
                        },
                        Err(e) => SuggestUi::Error(e),
                    };
                }
            }
            _ => {}
        }
    }

    fn sf_strength(&mut self) {
        let Some(t) = self.tool.as_ref() else { return };
        if t.def.key != "supportfins" {
            return;
        }
        let Some(ask) = supports::strength_ask(t.def.key, &t.values) else { return };
        let seq = self.fins_ask(FinsAsk::Strength { body: ask.body, options: ask.options, load: ask.load });
        self.sf.strength_seq = seq;
    }

    fn sf_suggest(&mut self) {
        let Some(t) = self.tool.as_ref() else { return };
        match supports::suggest_ask(t.def.key, &t.values) {
            Err(m) => self.tool_msg(MsgKind::Warn, m),
            Ok(ask) => {
                let seq = self.fins_ask(FinsAsk::Suggest { body: ask.body, options: ask.options, top: ask.top });
                self.sf.suggest_seq = seq;
                self.sf.suggest_gen = self.sf.pose_gen;
                if let Some(s) = self.tool.as_mut().and_then(|t| t.sf.as_mut()) {
                    s.suggest = SuggestUi::Busy;
                }
            }
        }
    }

    // ------------------------------------------------------------------ the red overhangs

    /// `quickPaint`: the faces that need support, red at once from the mesh on screen.
    pub(super) fn sf_quick_paint(&mut self) {
        let Some(t) = self.tool.as_mut() else { return };
        if !t.def.flags.paint {
            return;
        }
        for tag in ["sf-over", "sf-small", "sf-bare", "sf-held"] {
            self.viewport.clear_overlay(tag);
        }
        self.sf.cover.bump_generation();
        if let Some(s) = t.sf.as_mut() {
            s.cover = None;
        }
        let Some(body) = supports::body_id(&t.values) else { return };
        let thr = t.values.get("sf:threshold").map(supports::panel::parse_num).unwrap_or(0.0);
        let thr = if thr.is_finite() && thr > 0.0 { thr } else { 45.0 };
        let wall = t.def.flags.wall_tool;
        if let Some(faces) = self.viewport.show_overhangs_async(&body, thr) {
            if faces.is_empty() && !wall {
                t.msg = Some(ToolMsg::new(MsgKind::Info, supports::NO_OVERHANG_MSG));
            }
        }
    }

    fn on_overhangs_ready(&mut self, body: &str, n: usize) {
        let Some(t) = self.tool.as_mut() else { return };
        if n == 0 && t.def.flags.paint && !t.def.flags.wall_tool && supports::body_id(&t.values).as_deref() == Some(body) {
            t.msg = Some(ToolMsg::new(MsgKind::Info, supports::NO_OVERHANG_MSG));
        }
    }

    // ------------------------------------------------------------------ coverage

    fn sf_coverage(&mut self) {
        let Some((body, over)) = self.sf.cover_last.clone() else { return };
        let Some(t) = self.tool.as_ref() else { return };
        if t.def.key != "supportfins" {
            return;
        }
        let Some(mesh) = self.viewport.body_mesh(&body).cloned() else { return };
        // faces the part on screen doesn't have yet (its newer mesh is still being prepared) can't be checked
        let nf = (mesh.triangles.len() / 3) as u32;
        let over: Vec<u32> = over.into_iter().filter(|&f| f < nf).collect();
        let gap = t.values.get("sf:gap").map(supports::panel::parse_num).unwrap_or(0.0);
        let reach = t.sf.as_ref().map_or(supports::coverage::DEFAULT_REACH, |s| s.reach);
        let seq = self.sf.cover.bump_generation();
        for tag in ["sf-bare", "sf-held"] {
            self.viewport.clear_overlay(tag);
        }
        let cover = if over.is_empty() {
            // no overhangs in this pose: nothing to check
            Cover { none: false, busy: false, error: None, total: 0.0, held: 0.0, spots: Vec::new() }
        } else {
            // the meshes are shared (Arc); their copies for the check are made on the worker
            let fin_meshes = self.sf.fin_meshes.clone();
            let gap = if gap > 0.0 { gap } else { 0.2 };
            self.sf.cover.start_with(
                seq,
                Box::new(move || CoverageInput {
                    positions: mesh.positions.clone(),
                    triangles: mesh.triangles.clone(),
                    over_faces: over,
                    fins: fin_meshes.iter().map(|m| soup_of(m)).collect(),
                    gap,
                    reach,
                    max_sub: 250_000,
                }),
            );
            Cover { none: false, busy: true, error: None, total: 0.0, held: 0.0, spots: Vec::new() }
        };
        if let Some(s) = self.tool.as_mut().and_then(|t| t.sf.as_mut()) {
            s.cover = Some(cover);
        }
    }

    fn poll_coverage(&mut self) {
        let Some(res) = self.sf.cover.poll() else { return };
        if !self.sf_tool() {
            return;
        }
        let reach = self.tool.as_ref().and_then(|t| t.sf.as_ref()).map_or(supports::coverage::DEFAULT_REACH, |s| s.reach);
        for tag in ["sf-bare", "sf-held"] {
            self.viewport.clear_overlay(tag);
        }
        let cover = match res {
            CoverageJobResult::Error(e) => Cover { none: false, busy: false, error: Some(e), total: 0.0, held: 0.0, spots: Vec::new() },
            CoverageJobResult::Done(out) => self.show_coverage(out, reach),
        };
        if let Some(s) = self.tool.as_mut().and_then(|t| t.sf.as_mut()) {
            s.cover = Some(cover);
        }
    }

    /// The overhangs: pale where a fin (or the part) holds them, bright red where nothing does.
    fn show_coverage(&mut self, out: CoverageOutput, reach: f64) -> Cover {
        // the quick red paint still being worked out must not land over the coverage colours
        self.viewport.cancel_overhangs();
        self.viewport.clear_overlay("sf-over");
        let dist: Vec<f64> = out.dist.iter().map(|&d| d as f64).collect();
        self.viewport.add_dist_tris(&out.tris, &dist, reach, "sf-bare", (col(0xe6a3a3), 0.3), (col(0xe8171d), 0.85));
        let cover = Cover {
            none: false,
            busy: false,
            error: None,
            total: out.total,
            held: out.held,
            spots: out.spots.iter().map(|s| Spot { far: s.far, ends: s.ends.is_some() }).collect(),
        };
        self.sf.spots = out.spots;
        cover
    }

    // ------------------------------------------------------------------ a preview came back

    /// What a Support Fins / Draw Walls preview carries: walls' status, pieces, the plastic, the
    /// exact red and amber faces, then the coverage check.
    pub(super) fn sf_on_preview(&mut self, result: &Value, pack: &crate::viewport::PreviewPack) {
        let Some(t) = self.tool.as_mut() else { return };
        if !t.def.flags.paint {
            return;
        }
        let fins = t.def.key == "supportfins";
        if let Some(s) = t.sf.as_mut() {
            if fins || t.def.flags.wall_tool {
                s.wall_ok = result.get("walls").and_then(Value::as_array).map(|a| a.iter().map(|w| w.get("ok").and_then(Value::as_bool).unwrap_or(true)).collect()).unwrap_or_default();
            }
            s.plastic = result.get("support").filter(|v| !v.is_null()).map(|v| Plastic {
                volume: v.get("volume").and_then(Value::as_f64).unwrap_or(0.0),
                pieces: v.get("pieces").and_then(Value::as_u64).unwrap_or(0) as u32,
            });
        }
        self.sf.pieces = result.get("pieces").cloned().unwrap_or(Value::Null);
        if let Some(paint) = result.get("paint").filter(|p| !p.is_null()) {
            if let Some(body) = paint.get("body").and_then(Value::as_str) {
                let over = u32_list(paint.get("over"));
                self.viewport.paint_overhangs(body, &over, &u32_list(paint.get("small")));
                self.sf.cover_last = Some((body.to_owned(), over));
                self.sf.fin_meshes = pack.meshes.iter().filter(|m| m.id != body && !pack.ghosts.contains(&m.id)).map(|m| m.mesh.clone()).collect();
                if fins {
                    self.sf_coverage();
                }
            }
        }
    }

    pub(super) fn sf_on_preview_error(&mut self) {
        let fins = self.tool.as_ref().is_some_and(|t| t.def.key == "supportfins");
        if !fins {
            return;
        }
        self.sf.cover.bump_generation();
        if let Some(s) = self.tool.as_mut().and_then(|t| t.sf.as_mut()) {
            if !s.cover.as_ref().is_some_and(|c| c.none) {
                s.cover = Some(Cover { none: true, busy: false, error: None, total: 0.0, held: 0.0, spots: Vec::new() });
            }
        }
    }

    // ------------------------------------------------------------------ per frame

    /// Right after `viewport.show`: the handles, the Sculpt brush, Draw Walls' hover and the
    /// answers of the background work. True when the click of this frame was the tool's own.
    pub(super) fn tool_frame(&mut self, ui: &egui::Ui, resp: &egui::Response) -> bool {
        if let Some((body, n)) = self.viewport.poll_overhangs() {
            self.on_overhangs_ready(&body, n);
        }
        self.poll_coverage();
        if self.viewport.overhangs_pending() || self.tool.as_ref().and_then(|t| t.sf.as_ref()).is_some_and(|s| s.cover.as_ref().is_some_and(|c| c.busy)) {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(30));
        }
        let Some(t) = self.tool.as_ref() else { return false };
        let (sculpt, wall_edit) = (t.def.flags.sculpt, self.walls_active());
        let mut consumed = false;
        if sculpt {
            // no other click handling in the viewport while Sculpt is open
            consumed = true;
            let vals = self.resolved_values();
            match self.sculpt.update(&mut self.viewport, ui, resp, &vals) {
                SculptEvent::Stroke(v) => {
                    self.tool_set_values(v);
                    if let Some(t) = self.tool.as_mut() {
                        t.preview.cancel();
                    }
                    self.run_preview_now(web_time::Instant::now());
                }
                SculptEvent::Warn(m) => self.tool_msg(MsgKind::Warn, m),
                _ => {}
            }
            self.sculpt_overlay();
        }
        if self.gizmo.is_some() {
            consumed |= self.gizmo_frame(ui, resp);
        }
        if wall_edit {
            self.walls_hover(ui, resp);
            self.walls_redraw();
        }
        consumed
    }

    /// The strokes overlay after an Undo stroke (a drawn stroke is already on screen).
    fn sculpt_overlay(&mut self) {
        if self.sculpt.drawing() {
            return;
        }
        let Some(t) = self.tool.as_ref() else { return };
        let sig = t.values.get("strokes").and_then(Value::as_array).map(|a| format!("{}:{}", a.len(), a.last().map(|s| s.to_string().len()).unwrap_or(0))).unwrap_or_default();
        if sig != self.sf.sculpt_sig {
            self.sf.sculpt_sig = sig;
            let v = t.values.clone();
            Sculpt::draw(&mut self.viewport, &v);
        }
    }

    fn gizmo_frame(&mut self, ui: &egui::Ui, resp: &egui::Response) -> bool {
        let vals = self.resolved_values();
        let hidden = self.sketcher_active();
        let Some(g) = self.gizmo.as_mut() else { return false };
        // hidden while a sketch is being drawn (4c), and in Support Fins' other modes (`sync`)
        if hidden {
            g.hide(&mut self.viewport);
            return false;
        }
        let ev = g.update(&mut self.viewport, ui, resp, &vals);
        let dragging = g.drag_active();
        if let Some(t) = self.tool.as_mut() {
            t.dragging = dragging;
        }
        match ev {
            GizmoEvent::None => false,
            GizmoEvent::Started => {
                // the pending preview is out of date now, and its timer too
                self.preview_seq += 1;
                let seq = self.preview_seq;
                if let Some(t) = self.tool.as_mut() {
                    t.preview.invalidate(seq);
                    t.preview.cancel();
                }
                true
            }
            GizmoEvent::Moved(v) => {
                self.live_numbers(v);
                true
            }
            GizmoEvent::Dropped(d) => {
                match d {
                    GizmoDrop::Values { patch, auto_accept } => {
                        // the numbers go in as 1.x's `Object.assign(t.values, ...)` does: no
                        // delayed preview; the step is added first and previewed after it
                        if let Some(t) = self.tool.as_mut() {
                            t.values.extend(patch);
                            t.preview.cancel();
                        }
                        if auto_accept {
                            self.auto_accept();
                        }
                    }
                    GizmoDrop::Pose(m) => self.pose_drop(m),
                    GizmoDrop::Nothing => {}
                }
                // a preview dropped while the handle was held is asked for again, unless this
                // drop already brings one (an Add or Edit on its way, or a preview due)
                let edit_wait = self.sf.edit_wait;
                if let Some(t) = self.tool.as_mut() {
                    if std::mem::take(&mut t.preview_dropped) && t.accepting.is_none() && !edit_wait && !t.preview.pending() && !t.preview.waiting {
                        t.schedule_preview(0);
                    }
                }
                true
            }
        }
    }

    /// The panel shows the numbers while a handle is dragged (`liveNumbers`); nothing is previewed.
    fn live_numbers(&mut self, v: Values) {
        let Some(t) = self.tool.as_mut() else { return };
        if t.def.key == "scale" {
            if let Some(k) = v.get("_k").and_then(Value::as_array) {
                let k: Vec<f64> = k.iter().filter_map(Value::as_f64).collect();
                if k.len() == 3 {
                    t.values.insert("factor".into(), gizmo::jnum(k[0]));
                    t.values.insert("factor3".into(), Value::Array(k.iter().map(|&x| gizmo::jnum(x)).collect()));
                }
            }
            return;
        }
        for key in ["translate", "rotate"] {
            if let Some(x) = v.get(key) {
                t.values.insert(key.to_owned(), x.clone());
            }
        }
    }

    /// The sketch editor arrives in 4c; its handles must not show while it is active.
    fn sketcher_active(&self) -> bool {
        self.sketch.is_some()
    }

    // ------------------------------------------------------------------ Move / Scale: let go = done

    /// `autoAccept`: letting go of a handle adds the step the first time, then edits it.
    pub(super) fn auto_accept(&mut self) {
        let ctx = make_ctx(&self.bodies, &self.planes, &self.sketches, &self.timeline_view.items, &self.shell.selection, self.shell.inch);
        let ids: Vec<String> = self.timeline_view.items.iter().map(|i| i.id.clone()).collect();
        let Some(t) = self.tool.as_mut() else { return };
        if !t.valid() {
            return;
        }
        if t.accepting.is_some() {
            // one at a time: this drop goes out as an edit once the step exists
            t.accept_again = true;
            return;
        }
        let params = t.params(&ctx);
        t.accepted = true;
        t.ok_label = "Done".into();
        t.cancel_label = "Close".into();
        match t.edit_id.clone() {
            Some(id) => {
                self.link.send(Command::EditFeature { id, params, rollback_after: None });
                t.schedule_preview(0);
                self.sf.edit_wait = true;
            }
            None => {
                t.accepting = Some(ids);
                let kind = t.def.kind.to_owned();
                self.add_step(kind, params);
            }
        }
    }

    /// `addFeatureSmart`: add a step, before the part's supports when it moves the part, and say so
    /// once it is in.
    pub(super) fn add_step(&mut self, kind: String, params: Value) {
        let index = insert_before_supports(&self.timeline_view, self.marker_now(), &kind, &params);
        self.sf.insert_note = index.and_then(|at| {
            let sf = self.timeline_view.items.get(at)?;
            let hand = sf.params.get("walls").and_then(Value::as_array).is_some_and(|a| !a.is_empty())
                || sf.params.get("removed").and_then(Value::as_array).is_some_and(|a| !a.is_empty())
                || sf.kind == "supportwall";
            let ids = self.timeline_view.items.iter().map(|i| i.id.clone()).collect();
            Some(if hand {
                (ids, Level::Warning, format!("{0} is rebuilt for the new position. Walls you drew by hand stay where they were drawn: check them in {0}.", sf.label))
            } else {
                (ids, Level::Info, format!("{} is rebuilt for the new position", sf.label))
            })
        });
        self.link.send(Command::AddFeature { kind, params, index, name: None });
    }

    /// The engine said no to a handle's Add or Edit: the tool can try again, and says why.
    /// True when the error was theirs (a handle's Add or Edit was the oldest command on its way).
    pub(super) fn accept_failed(&mut self, why: &str) -> bool {
        let waiting = std::mem::take(&mut self.sf.edit_wait);
        let Some(t) = self.tool.as_mut() else { return false };
        let adding = t.accepting.take().is_some();
        if !adding && !waiting {
            return false;
        }
        // an OK that waited for this Add doesn't go out: the tool stays open and says why
        t.ok_after_add = false;
        self.sf.insert_note = None;
        // a drop that came while this one was on its way still goes out (1.x do/while)
        let again = std::mem::take(&mut t.accept_again);
        if adding {
            // nothing was added: the tool is still a new one
            t.accepted = false;
            t.ok_label = "OK".into();
            t.cancel_label = "Cancel".into();
        }
        self.tool_msg(MsgKind::Err, why);
        if again {
            self.auto_accept();
        }
        true
    }

    /// The engine refused OK: the tool stays open with its values and says why; Cancel puts the
    /// timeline marker back.
    pub(super) fn commit_failed(&mut self, why: &str) {
        if !std::mem::take(&mut self.committing) {
            return;
        }
        self.sf.insert_note = None;
        self.tool_msg(MsgKind::Err, why);
    }

    /// The timeline changed: a step a handle added is the one the tool goes on editing, and a
    /// Support Fins turn that was on its way has landed.
    pub(super) fn after_timeline(&mut self) {
        let items: Vec<String> = self.timeline_view.items.iter().map(|i| i.id.clone()).collect();
        let mut again = false;
        let mut roll = None;
        let mut ok_now = false;
        if let Some(t) = self.tool.as_mut() {
            if let Some(before) = t.accepting.clone() {
                if let Some(idx) = items.iter().position(|id| !before.contains(id)) {
                    t.edit_id = Some(items[idx].clone());
                    t.prev_rollback = t.prev_rollback.map(|m| m + 1);
                    t.accepting = None;
                    again = std::mem::take(&mut t.accept_again);
                    ok_now = std::mem::take(&mut t.ok_after_add);
                    roll = Some(idx);
                    t.schedule_preview(0);
                }
            }
        }
        if let Some(idx) = roll {
            self.link.send(Command::Rollback(Some(idx)));
            self.pending_marker = Some(Some(idx));
        }
        if again {
            self.auto_accept();
        }
        self.sf.edit_wait = false;
        if self.committing {
            // the engine took OK (a refusal would have come first and cleared this)
            self.close_tool(true);
        }
        if let Some((before, _, _)) = &self.sf.insert_note {
            if items.iter().any(|id| !before.contains(id)) {
                if let Some((_, level, text)) = self.sf.insert_note.take() {
                    self.toasts.push(level, text);
                }
            }
        }
        self.pose_landed(&items);
        // OK that waited for the handle's Add: it goes out now, as an edit of the step just added
        if ok_now {
            self.tool_ok();
        }
    }

    // ------------------------------------------------------------------ Support Fins: turning

    /// `poseDrop(m)`: a turn of the rings is applied to the part at once, as one Move step for the
    /// whole session (built from the turns so far as one matrix).
    pub(super) fn pose_drop(&mut self, m: M4) {
        if self.sf.posing {
            return;
        }
        let Some(t) = self.tool.as_ref() else { return };
        let Some(bid) = supports::body_id(&t.values) else { return };
        let Some(b) = self.viewport.body_box(&bid) else { return };
        let ps = self.sf.pose.get_or_insert(Pose { fid: None, m: IDENT, c0: [0, 1, 2].map(|i| (b.min[i] + b.max[i]) / 2.0) });
        let whole = camera::mul(&m, &ps.m);
        let img = gizmo::apply(&whole, ps.c0);
        let c0 = ps.c0;
        let fid = ps.fid.clone();
        let rot = gizmo::rot_of(gizmo::quat_of_mat(&whole));
        let translate = [0usize, 1, 2].map(|i| gizmo::js_round((img[i] - c0[i]) * 1e5) / 1e5);
        let params = json!({
            "bodies": [bid],
            "translate": translate,
            "rotate": rot,
            "pivot": "center",
            "bed": false,
            "copy": false,
        });
        self.sf.posing = true;
        self.sf.pose_gen += 1;
        if let Some(g) = self.gizmo.as_mut() {
            g.set_enabled(false);
        }
        self.tool_msg(MsgKind::Busy, "Turning the part…");
        let before: Vec<String> = self.timeline_view.items.iter().map(|i| i.id.clone()).collect();
        self.sf.pose_wait = Some(PoseWait { before, whole, turn: m });
        match fid {
            Some(id) => self.link.send(Command::EditFeature { id, params, rollback_after: None }),
            None => self.link.send(Command::AddFeature { kind: "move".into(), params, index: None, name: Some("Rotate".into()) }),
        }
    }

    fn pose_landed(&mut self, items: &[String]) {
        let Some(w) = self.sf.pose_wait.take() else { return };
        if !self.sf_tool() {
            return;
        }
        let new_id = items.iter().find(|id| !w.before.contains(id)).cloned();
        if let Some(ps) = self.sf.pose.as_mut() {
            if ps.fid.is_none() {
                ps.fid = new_id;
            }
            ps.m = w.whole;
        }
        let m = w.turn;
        // walls drawn by hand turn with the part; removed fins are rebuilt, so they come back
        if let Some(t) = self.tool.as_mut() {
            let walls: Vec<Value> = walls_of(&t.values)
                .into_iter()
                .map(|mut w| {
                    w.points = w.points.iter().map(|p| gizmo::apply(&m, *p).map(round4)).collect();
                    w.to_value()
                })
                .collect();
            t.values.insert("walls".into(), Value::Array(walls));
            t.values.insert("removed".into(), json!([]));
            // a load points the same way on the part after a turn, so it turns with it
            if let Some(lv) = t.values.get("_load").and_then(strength::load_vector) {
                let rot = [[m[0][0], m[1][0], m[2][0]], [m[0][1], m[1][1], m[2][1]], [m[0][2], m[1][2], m[2][2]]];
                t.values.insert("_load".into(), strength::turn_load(lv, rot));
            }
            if let Some(s) = t.sf.as_mut() {
                s.suggest = SuggestUi::Idle;
                s.strength = None;
            }
            t.msg = None;
        }
        self.sf.posing = false;
        self.sf.pose_gen += 1;
        if let Some(g) = self.gizmo.as_mut() {
            g.reset(&mut self.viewport);
            g.set_enabled(true);
        }
        self.viewport.clear_preview();
        self.sf_quick_paint();
        if let Some(t) = self.tool.as_mut() {
            t.schedule_preview(0);
        }
        self.sf_strength();
    }

    /// The turn failed (the engine said so): the part goes back, the message says why.
    pub(super) fn pose_failed(&mut self, why: &str) -> bool {
        if self.sf.pose_wait.take().is_none() {
            return false;
        }
        self.sf.posing = false;
        self.sf.pose_gen += 1;
        if let Some(g) = self.gizmo.as_mut() {
            g.reset(&mut self.viewport);
            g.set_enabled(true);
        }
        self.tool_msg(MsgKind::Err, why);
        true
    }

    /// `sfUsePose(rot)`: turn the part by a Support Fins pose (`rot`: column-major 3x3).
    #[doc(hidden)]
    pub fn use_pose(&mut self, rot: &Value) {
        let r: Vec<f64> = rot.as_array().map(|a| a.iter().filter_map(Value::as_f64).collect()).unwrap_or_default();
        if r.len() != 9 {
            return;
        }
        let Some(bid) = self.tool.as_ref().and_then(|t| supports::body_id(&t.values)) else { return };
        let (Some(bbox), Some(mesh)) = (self.viewport.body_box(&bid), self.viewport.body_mesh(&bid).cloned()) else { return };
        let mut mm = IDENT;
        for c in 0..3 {
            for row in 0..3 {
                mm[c][row] = r[c * 3 + row];
            }
        }
        let q = gizmo::quat_of_mat(&mm);
        let vals: Values = json!({"translate": [0, 0, 0], "rotate": gizmo::rot_of(q), "pivot": "center", "bed": true}).as_object().cloned().unwrap_or_default();
        let Some(g) = self.gizmo.as_ref() else { return };
        let m = g.matrix_for(&GizmoBody { id: bid, bbox, mesh }, &vals);
        if self.sf_mode() != "rotate" {
            if let Some(t) = self.tool.as_mut() {
                t.set_value_plain("_mode", json!("rotate"));
            }
        }
        self.pose_drop(m);
    }

    // ------------------------------------------------------------------ the Support Fins form

    /// What the panel asked for this frame, and what a click in the part changed.
    pub(super) fn sf_panel_events(&mut self, events: Vec<PanelEvent>) {
        for e in events {
            match e {
                PanelEvent::Edited(_) => {}
                PanelEvent::QuickPaint => self.sf_quick_paint(),
                PanelEvent::ResetWalls => {
                    self.sf.drawer.reset();
                    self.viewport.set_band(&[], None, Color32::WHITE);
                    self.sync_session_walls();
                }
                PanelEvent::ClearHover => self.viewport.set_preview_hover(None),
                PanelEvent::Strength => self.sf_strength(),
                PanelEvent::Hint(h) => self.shell.hint = h,
                PanelEvent::Sections(json) => {
                    self.platform.set_setting(supports::panel::SECTIONS_KEY, &json);
                    self.sf.sections = Some(json);
                }
                PanelEvent::Suggest => self.sf_suggest(),
                PanelEvent::UseCandidate(i) => {
                    let rot = self.tool.as_ref().and_then(|t| t.sf.as_ref()).and_then(|s| match &s.suggest {
                        SuggestUi::Ready { candidates, .. } => candidates.get(i).and_then(|c| c.get("rot")).cloned(),
                        _ => None,
                    });
                    if let Some(r) = rot {
                        self.use_pose(&r);
                    }
                }
                PanelEvent::UseStrongest => {
                    let rot = self.tool.as_ref().and_then(|t| t.sf.as_ref()).and_then(|s| s.strength.as_ref()).and_then(|s| s.get("strongest")).and_then(|s| s.get("rot")).cloned();
                    if let Some(r) = rot {
                        self.use_pose(&r);
                    }
                }
                PanelEvent::FillGaps => {
                    let spots = self.sf.spots.clone();
                    self.sf_fill(&spots);
                }
                PanelEvent::RestoreRemoved => {
                    self.viewport.set_preview_hover(None);
                    if let Some(t) = self.tool.as_mut() {
                        t.schedule_preview(0);
                    }
                }
                PanelEvent::Material(m) => self.platform.set_setting(MATERIAL_KEY, &m),
                PanelEvent::Reach(v) => {
                    self.platform.set_setting(REACH_KEY, &gizmo::jnum(v).to_string());
                    self.sf_coverage();
                }
                PanelEvent::WallShape(k) => self.wall_shape(&k),
                PanelEvent::WallFinish => {
                    self.wall_edit(|d, w| {
                        d.finish_path(w);
                    });
                }
                PanelEvent::WallUndo => {
                    self.wall_edit(|d, w| {
                        d.undo(w);
                    });
                }
                PanelEvent::WallClear => {
                    self.sf.drawer.pending.clear();
                    self.set_walls(Vec::new());
                }
            }
        }
        self.sync_session_walls();
    }

    fn sync_session_walls(&mut self) {
        let n = self.sf.drawer.pending.len();
        let shape = self.sf.drawer.shape.clone();
        if let Some(s) = self.tool.as_mut().and_then(|t| t.sf.as_mut()) {
            s.wall_pending = n;
            s.wall_shape = shape;
        }
    }

    fn wall_shape(&mut self, k: &str) {
        let mut walls = self.tool.as_ref().map(|t| walls_of(&t.values)).unwrap_or_default();
        if self.sf.drawer.finish_path(&mut walls) {
            self.set_walls(walls);
        }
        let platform = &self.platform;
        self.sf.drawer.set_shape(k, |key, v| platform.set_setting(key, v));
        let hint = self.sf.drawer.hint().to_owned();
        if let Some(t) = self.tool.as_mut() {
            t.wall_hint = hint.clone();
        }
        let tool = self.tool.as_ref().map(|t| (t.def.key, t.values.clone()));
        if let Some((key, values)) = tool {
            self.shell.hint = supports::mode_hint(key, &values, &hint);
        }
    }

    fn wall_edit(&mut self, f: impl FnOnce(&mut WallDrawer, &mut Vec<Wall>)) {
        let mut walls = self.tool.as_ref().map(|t| walls_of(&t.values)).unwrap_or_default();
        let before = walls.clone();
        f(&mut self.sf.drawer, &mut walls);
        if walls != before {
            self.set_walls(walls);
        }
    }

    /// `commitWalls`: the list goes into the tool and the walls are built.
    fn set_walls(&mut self, walls: Vec<Wall>) {
        let v = walls::walls_to_params(&walls);
        if let Some(t) = self.tool.as_mut() {
            if let Some(s) = t.sf.as_mut() {
                s.wall_ok.truncate(walls.len());
            }
            let ctx_free = Values::new();
            let _ = ctx_free;
            t.set_value_plain("walls", v);
            t.schedule_preview(30);
        }
    }

    /// `sfFill`: one hand wall across each bare spot (its ends sit on the overhang).
    fn sf_fill(&mut self, spots: &[crate::supports::coverage::BareSpot]) {
        let add: Vec<Wall> = spots.iter().filter_map(|s| s.ends).map(|e| Wall { shape: "line".into(), points: vec![e[0].map(round4), e[1].map(round4)], normal: None }).collect();
        if add.is_empty() {
            self.tool_msg(MsgKind::Info, "No bare spot to fill.");
            return;
        }
        let mut walls = self.tool.as_ref().map(|t| walls_of(&t.values)).unwrap_or_default();
        walls.extend(add);
        self.set_walls(walls);
    }

    // ------------------------------------------------------------------ Draw Walls in the viewport

    /// A click in the viewport while the tool is open. True when the tool took it.
    pub(super) fn sf_click(&mut self, pos: Pos2) -> bool {
        if !self.sf_tool() {
            return false;
        }
        let mode = self.sf_mode();
        let fins = self.tool.as_ref().is_some_and(|t| t.def.key == "supportfins");
        if fins && mode == "add" && self.sf.drawer.shape == "drop" {
            let hit = self.viewport.pick(pos, true, false);
            self.sf_drop_at(hit);
            return true;
        }
        if self.walls_active() {
            self.wall_click(pos);
            return true;
        }
        if fins && mode == "remove" {
            self.remove_fin_at(pos);
            return true;
        }
        false
    }

    fn wall_click(&mut self, pos: Pos2) {
        let hit = self.viewport.pick(pos, true, false);
        let wh = match &hit {
            Some(Hit::Body { id, point, normal, .. }) => Some(WallClickHit { body_id: id.clone(), point: point.map(round4), normal: *normal }),
            _ => None,
        };
        let Some(t) = self.tool.as_ref() else { return };
        let body = supports::body_id(&t.values);
        let walls = walls_of(&t.values);
        let near = match (self.sf.drawer.pending.last(), &wh) {
            (Some(last), Some(h)) => self.viewport.world_to_screen(*last).distance(self.viewport.world_to_screen(h.point)) < 6.0,
            _ => false,
        };
        let names: Vec<(String, String)> = self.bodies.iter().map(|b| (b.id.clone(), b.name.clone())).collect();
        let name_of = |id: &str| names.iter().find(|(i, _)| i == id).map(|(_, n)| n.clone()).unwrap_or_else(|| id.to_owned());
        // the first click names the part when none is picked yet
        if body.is_none() {
            if let Some(h) = &wh {
                let id = h.body_id.clone();
                if let Some(t) = self.tool.as_mut() {
                    t.set_value_plain("body", json!([id]));
                }
                self.sf_quick_paint();
            }
        }
        let (new, msg) = self.sf.drawer.click(wh.as_ref(), &walls, body.as_deref(), name_of, near);
        match msg {
            WallClickMsg::Warn(m) => self.tool_msg(MsgKind::Warn, m),
            WallClickMsg::Info(m) => self.tool_msg(MsgKind::Info, m),
            WallClickMsg::None => {}
        }
        if let Some(list) = new {
            self.set_walls(list);
        }
        self.sync_session_walls();
    }

    fn sf_drop_at(&mut self, hit: Option<Hit>) {
        let reach = self.tool.as_ref().and_then(|t| t.sf.as_ref()).map_or(supports::coverage::DEFAULT_REACH, |s| s.reach);
        let near = match hit {
            Some(Hit::Body { point, .. }) => self
                .sf
                .spots
                .iter()
                .filter(|s| s.ends.is_some())
                .map(|s| ((0..3).map(|k| (point[k] - s.at[k]).powi(2)).sum::<f64>().sqrt(), s.clone()))
                .min_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal)),
            _ => None,
        };
        match near {
            Some((d, s)) if d <= (2.0 * reach).max(s.span) => self.sf_fill(&[s]),
            _ => self.tool_msg(MsgKind::Info, "Click on a red (bare) spot to drop a fin there."),
        }
    }

    /// Remove mode: a click on a fin takes it away; a click on a removed (red) one brings it back.
    fn remove_fin_at(&mut self, pos: Pos2) {
        let id = self.viewport.pick_preview(pos);
        let info = id.and_then(|id| self.sf.pieces.get(&id).cloned());
        let Some(info) = info else {
            self.tool_msg(MsgKind::Info, "Click a green fin to remove it.");
            return;
        };
        let Some(t) = self.tool.as_mut() else { return };
        let kind = info.get("kind").and_then(Value::as_str).unwrap_or("").to_owned();
        if kind == "hand" {
            let mut w = t.values.get("walls").and_then(Value::as_array).cloned().unwrap_or_default();
            let i = info.get("wall").and_then(Value::as_u64).unwrap_or(u64::MAX) as usize;
            if i < w.len() {
                w.remove(i);
            }
            if let Some(s) = t.sf.as_mut() {
                s.wall_ok.clear();
            }
            t.set_value_plain("walls", Value::Array(w));
        } else if info.get("removed").and_then(Value::as_bool).unwrap_or(false) {
            let mut list = t.values.get("removed").and_then(Value::as_array).cloned().unwrap_or_default();
            let c = info.get("c").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_f64).collect::<Vec<_>>()).unwrap_or_default();
            let mut best: Option<(usize, f64)> = None;
            for (i, r) in list.iter().enumerate() {
                let rc: Vec<f64> = r.get("c").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_f64).collect()).unwrap_or_default();
                if r.get("kind").and_then(Value::as_str) != Some(kind.as_str()) || rc.len() != c.len() {
                    continue;
                }
                let d = rc.iter().zip(&c).map(|(a, b)| (a - b).powi(2)).sum::<f64>().sqrt();
                if best.is_none_or(|(_, bd)| d < bd) {
                    best = Some((i, d));
                }
            }
            if let Some((i, _)) = best {
                list.remove(i);
            }
            t.set_value_plain("removed", Value::Array(list));
        } else {
            let mut list = t.values.get("removed").and_then(Value::as_array).cloned().unwrap_or_default();
            list.push(json!({"kind": kind, "c": info.get("c").cloned().unwrap_or(Value::Null), "size": info.get("size").cloned().unwrap_or(Value::Null)}));
            t.set_value_plain("removed", Value::Array(list));
        }
        self.viewport.set_preview_hover(None);
        if let Some(t) = self.tool.as_mut() {
            t.schedule_preview(0);
        }
    }

    fn walls_hover(&mut self, ui: &egui::Ui, resp: &egui::Response) {
        let pos = if resp.hovered() { ui.input(|i| i.pointer.hover_pos()) } else { None };
        let hit = pos.and_then(|p| self.viewport.pick(p, true, false));
        let pt = match hit {
            Some(Hit::Body { point, .. }) => Some(point),
            _ => None,
        };
        if pt != self.sf.hover_pt {
            self.sf.hover_pt = pt;
            self.sf.drawer.hover_pt = pt;
            self.sf.walls_sig.clear();
        }
    }

    /// The walls drawn so far and the one in progress, on the part.
    fn walls_redraw(&mut self) {
        let Some(t) = self.tool.as_ref() else { return };
        let walls = walls_of(&t.values);
        let ok: Vec<bool> = t.sf.as_ref().map(|s| s.wall_ok.clone()).unwrap_or_default();
        let sig = format!("{walls:?}|{ok:?}|{:?}|{:?}", self.sf.drawer.pending, self.sf.drawer.hover_pt);
        if sig == self.sf.walls_sig {
            return;
        }
        self.sf.walls_sig = sig;
        self.viewport.clear_overlay("walls");
        for (i, w) in walls.iter().enumerate() {
            let bad = ok.get(i).is_some_and(|ok| !ok);
            self.viewport.add_polyline(&walls::wall_line(w), col(if bad { walls::BAD_COL } else { walls::OK_COL }), "walls");
        }
        for q in &self.sf.drawer.pending {
            self.viewport.add_marker(*q, col(walls::PENDING_COL), "walls", None);
        }
        match self.sf.drawer.band_polyline() {
            Some(line) => self.viewport.set_band(&line, self.sf.drawer.hover_pt, col(walls::BAND_COL)),
            None => self.viewport.set_band(&[], self.sf.drawer.hover_pt, col(walls::BAND_COL)),
        }
        self.sync_session_walls();
    }

    /// Keys the wall editor and the Move gizmo take (`this.walls.key(e)`, W / R).
    pub(super) fn tool_key(&mut self, a: &keys::KeyAction) {
        use egui::Key;
        match a {
            keys::KeyAction::Walls(p) => {
                let name = match p.key {
                    Key::Enter => "Enter",
                    Key::Escape => "Escape",
                    Key::Backspace => "Backspace",
                    Key::Z => "z",
                    _ => return,
                };
                let ctrl = p.mods.command || p.mods.ctrl;
                self.wall_edit(|d, w| {
                    d.handle_key(name, ctrl, w);
                });
                self.sync_session_walls();
            }
            keys::KeyAction::Gizmo { rotate } => {
                if let Some(t) = self.tool.as_mut() {
                    t.set_value_plain("_gizmo", json!(if *rotate { "rotate" } else { "move" }));
                }
            }
            _ => {}
        }
    }

    pub(super) fn walls_pending(&self) -> bool {
        !self.sf.drawer.pending.is_empty()
    }

    pub(super) fn walls_on(&self) -> bool {
        self.walls_active()
    }
}

impl StlApp {
    /// The thread the coverage check ran on (tests: it is never the window's).
    #[doc(hidden)]
    #[cfg(not(target_arch = "wasm32"))]
    pub fn coverage_worker_thread(&self) -> Option<std::thread::ThreadId> {
        self.sf.cover.worker_thread_id()
    }

    /// Draw Walls' remembered shape and the points of the wall being drawn (tests).
    #[doc(hidden)]
    pub fn wall_drawer(&self) -> &WallDrawer {
        &self.sf.drawer
    }
}

// ====================================================================== sketch mode (4c)
//
// `app.js` `beginSketch`, `enterSketch`, `finishSketch`, `cancelSketch`, the sketch branch of
// `onRibbon` and the sketcher's event handling, on top of `crate::sketch`.

use crate::sketch::session::{self as sk_session, Env, SketchMode};
use crate::sketch::{self, SketchDialog, SketchEffect, SketchInput, SketchSession};
use crate::viewport::{MouseMode, PlaneFrame};

/// "Click a flat face of a body, a construction plane, or one of the origin planes (XY, XZ, YZ)." (app.js:1500)
pub const PLANE_PROMPT_INFO: &str = "Click a flat face of a body, a construction plane, or one of the origin planes (XY, XZ, YZ).";

/// What the sketch keeps beside `SketchMode` on the app.
#[derive(Default)]
pub(super) struct Sk {
    /// Sequence for `Command::SketchOp`.
    pub seq: u64,
    /// The step ids that existed when Finish was pressed (to find the new one).
    pub known: Vec<String>,
    pub text: sketch::text::TextDialogState,
    /// An open file dialog for Import SVG / DXF.
    pub import: Option<crate::platform::Pending<Option<crate::platform::PickedFile>>>,
    /// The Offset prompt is open.
    pub offset_open: bool,
    /// The "Over defined" question is open for this dimension.
    pub over: Option<sketch::Constraint>,
    /// The Project outline dialog is open.
    pub project: Option<sketch::project::ProjectForm>,
    /// `Viewport::start_fly_in` has been called.
    pub fly_started: bool,
    /// The grid snap step set by the units (`setUnits`: 1/16 in or 1 mm); `None` until then (1 mm).
    pub snap_step: Option<f64>,
}

impl StlApp {
    pub fn sketch(&self) -> Option<&SketchSession> {
        self.sketch.as_ref().map(|m| &m.s)
    }

    /// The whole open sketch (tests read what the panel and the solver say).
    pub fn sketch_mode(&self) -> Option<&SketchMode> {
        self.sketch.as_ref()
    }

    pub fn sketch_mode_mut(&mut self) -> Option<&mut SketchMode> {
        self.sketch.as_mut()
    }

    fn sk_params(&self) -> BTreeMap<String, f64> {
        self.param_views.iter().filter_map(|p| p.value.map(|v| (p.name.clone(), v))).collect()
    }

    /// Run `f` on the open sketch with the viewport, units and parameters a tool needs.
    fn sk_call<R>(&mut self, f: impl FnOnce(&mut SketchMode, &Env) -> R) -> Option<R> {
        let params = self.sk_params();
        let inch = self.shell.inch;
        let m = self.sketch.as_mut()?;
        let env = Env { vp: &self.viewport, inch, params: &params };
        Some(f(m, &env))
    }

    /// `beginSketch(planeRef)`: no plane asks for one in the tool panel.
    pub fn begin_sketch(&mut self, plane: Option<Value>) {
        self.close_tool(false);
        if self.sketch.is_some() {
            return;
        }
        match plane {
            Some(p) => self.enter_sketch(Some(p), None),
            None => {
                self.open_tool_def(crate::tools::sketch_prompt(), None, None);
                if let Some(t) = self.tool.as_mut() {
                    t.ok_label = "Start".into();
                    t.msg = Some(ToolMsg::new(MsgKind::Info, PLANE_PROMPT_INFO));
                }
                self.viewport.set_base_planes_visible(true);
            }
        }
    }

    /// Start (or a picked plane) in the plane prompt: enter the sketch on the plane picked.
    pub(super) fn sketch_prompt_ok(&mut self) {
        let Some(plane) = self.tool.as_ref().filter(|t| t.def.key == "sketch").and_then(|t| t.values.get("plane").cloned()).filter(|p| !p.is_null()) else { return };
        self.close_tool(false);
        self.enter_sketch(Some(plane), None);
    }

    /// `enterSketch({plane, feature})`: a feature is a sketch step opened for editing.
    pub fn enter_sketch(&mut self, plane: Option<Value>, feature: Option<&crate::engine_link::TimelineItem>) {
        if self.sketch.is_some() {
            return;
        }
        let ctx = make_ctx(&self.bodies, &self.planes, &self.sketches, &self.timeline_view.items, &self.shell.selection, self.shell.inch);
        let mut saved: Vec<stl_sketch::entities::Entity> = Vec::new();
        let (frame, plane_ref, entities, constraints, name) = match feature {
            Some(f) => {
                let from_view = self.sketches.iter().find(|s| s.id == f.id).map(|s| PlaneFrame { origin: s.frame[0], x: s.frame[1], y: s.frame[2], normal: s.frame[3] });
                let plane_ref = f.params.get("plane").cloned().unwrap_or(Value::Null);
                let frame = from_view.or_else(|| crate::tools::fields::plane::frame_of(&plane_ref, &ctx));
                let list: Vec<Value> = f.params.get("refs").and_then(Value::as_array).cloned().unwrap_or_default();
                saved = stl_sketch::entities::parse_entities(&list.into_iter().filter(|v| v.is_object()).collect::<Vec<_>>()).unwrap_or_default();
                let ents: Vec<Value> = f.params.get("entities").and_then(Value::as_array).cloned().unwrap_or_default();
                let entities = stl_sketch::entities::parse_entities(&ents).unwrap_or_default();
                let constraints: Vec<sketch::Constraint> = f.params.get("constraints").and_then(|c| serde_json::from_value(c.clone()).ok()).unwrap_or_default();
                (frame, plane_ref, entities, constraints, Some(f.label.clone()))
            }
            None => {
                let p = plane.unwrap_or(Value::Null);
                (crate::tools::fields::plane::frame_of(&p, &ctx), p, Vec::new(), Vec::new(), None)
            }
        };
        let Some(frame) = frame else {
            self.toasts.push(Level::Error, "Could not use that plane.".to_owned());
            return;
        };
        let prev = self.marker_now();
        if let Some(f) = feature {
            if let Some(idx) = self.timeline_view.items.iter().position(|i| i.id == f.id) {
                self.link.send(Command::Rollback(Some(idx)));
                self.pending_marker = Some(Some(idx));
            }
        }
        let refs: Vec<sketch::RefEnt> = saved.iter().cloned().map(|ent| sketch::RefEnt { ent, missing: false }).collect();
        let s = SketchSession::new(frame, plane_ref, feature.map(|f| f.id.clone()), name, entities, constraints, refs, None);
        let mut m = SketchMode::new(s, prev, saved);
        // the part's edges on (or through, or under) the plane, to dimension from
        self.sk.seq += 1;
        m.refs_seq = Some(self.sk.seq);
        let fr = [frame.origin, frame.x, frame.y, frame.normal];
        self.link.send(Command::SketchOp { seq: self.sk.seq, op: "refs".into(), data: json!({ "op": "refs", "frame": fr, "entities": [] }) });
        self.shell.selection.clear();
        self.viewport.set_selection(&[]);
        self.viewport.set_hover(None);
        self.viewport.set_mouse_mode(MouseMode::Sketch);
        self.viewport.look_at_plane(&frame, None);
        self.viewport.set_base_planes_visible(false);
        self.viewport.sync_sketches(&self.sketches, feature.map(|f| f.id.as_str()));
        self.viewport.set_viewcube_inset(crate::tools::panel::VIEWCUBE_INSET);
        sketch::view::build_grid(&mut self.viewport, &m.s);
        // the panel's Grid snap starts on, at the units' step (1 mm, or 1/16 in after `setUnits('in')`)
        let step = self.sk.snap_step.unwrap_or(1.0);
        m.panel.set_snap(true, step);
        self.shell.hint = m.hint.clone();
        self.shell.hud = None;
        self.gizmo = None;
        self.sketch = Some(m);
    }

    /// `setUnits(m)`: show inches or millimetres; the sketch's grid snap follows (1/16 in or
    /// 1 mm), and a toast says what changed.
    pub fn set_units_inch(&mut self, inch: bool) {
        self.shell.inch = inch;
        let step = if inch { 25.4 / 16.0 } else { 1.0 };
        self.sk.snap_step = Some(step);
        if let Some(m) = self.sketch.as_mut() {
            m.panel.set_snap_step(step);
        }
        let text = if inch { "Showing inches. Models are still saved in millimetres." } else { "Showing millimetres." };
        self.toasts.push(Level::Info, text.to_owned());
    }

    /// `setTool(key)` of the sketcher.
    pub fn sketch_set_tool(&mut self, key: &str) {
        if let Some(fx) = self.sk_call(|m, env| m.set_tool(key, env)) {
            self.sketch_apply(fx);
        }
    }

    /// What the tool in use and the panel asked for.
    pub fn sketch_apply(&mut self, effects: Vec<SketchEffect>) {
        for e in effects {
            match e {
                SketchEffect::Hint(h) => {
                    if let Some(m) = self.sketch.as_mut() {
                        m.hint = h.clone();
                    }
                    self.shell.hint = h;
                }
                SketchEffect::Hud(h) => {
                    if let Some(m) = self.sketch.as_mut() {
                        m.hud = h.clone();
                    }
                    self.shell.hud = h;
                }
                SketchEffect::Toast(l, t) => self.toasts.push(l, t),
                SketchEffect::SketchOp { op, data } => {
                    self.sk.seq += 1;
                    let seq = self.sk.seq;
                    if let Some(m) = self.sketch.as_mut() {
                        m.op = Some((seq, op.clone()));
                    }
                    self.link.send(Command::SketchOp { seq, op, data });
                }
                SketchEffect::Changed => {
                    if let Some(c) = self.sketch.as_mut().and_then(|m| m.changed()) {
                        self.link.send(c);
                    }
                    self.sketch_drain_toast();
                }
                SketchEffect::Dialog(d) => self.sketch_dialog(d),
                SketchEffect::Finish { extrude } => self.finish_sketch(extrude),
                SketchEffect::Cancel => self.cancel_sketch(),
            }
        }
    }

    /// A toast the solver left ("Some constraints could not be kept.").
    fn sketch_drain_toast(&mut self) {
        if let Some(t) = self.sketch.as_mut().and_then(|m| m.toast.take()) {
            self.toasts.push(Level::Warning, t);
        }
    }

    fn sketch_dialog(&mut self, d: SketchDialog) {
        let inch = self.shell.inch;
        let Some(m) = self.sketch.as_mut() else { return };
        match d {
            SketchDialog::DimValue(i) => {
                if let Some(c) = m.s.constraints.get(i) {
                    let ents = sketch::solve::all_ents(&m.s);
                    let text = sketch::dims::expr_of_dim(c).map(str::to_owned).unwrap_or_else(|| sketch::dims::dim_text(c, &ents, inch, true));
                    m.s.sel_c = Some(i);
                    m.panel.modify = Some(sketch::panel::Modify { idx: i, text, fresh: true });
                }
            }
            SketchDialog::Modify(id) => {
                let idx = m
                    .s
                    .constraints
                    .iter()
                    .position(|c| c.id.as_ref().is_some_and(|v| v.as_str().map(str::to_owned).unwrap_or_else(|| v.to_string()) == id))
                    .or_else(|| id.parse::<usize>().ok());
                if let Some(i) = idx {
                    self.sketch_dialog(SketchDialog::DimValue(i));
                }
            }
            SketchDialog::Project => self.sketch_project(),
            SketchDialog::OverDefined(c) => self.sk.over = Some(*c),
            SketchDialog::Text { group } => {
                if group.is_none() {
                    // 1.x:273 `this.setTool('select')` before the dialog opens
                    self.sketch_set_tool("select");
                }
                let Some(m) = self.sketch.as_mut() else { return };
                match group.and_then(|_| sketch::text::selected_text(&m.s)) {
                    Some((g, meta)) => sketch::text::open_edit(&mut self.sk.text, &g, &meta, inch),
                    None => sketch::text::open_new(&mut self.sk.text, sketch::text::take_pending_uv().unwrap_or([0.0, 0.0]), inch),
                }
            }
            SketchDialog::Offset => self.sketch_offset(),
        }
    }

    /// `sketchProject()`: no solid bodies is a warning; else the Project outline dialog.
    fn sketch_project(&mut self) {
        let bodies: Vec<(String, String)> = self.bodies.iter().filter(|b| !b.hidden && b.solid).map(|b| (b.id.clone(), b.name.clone())).collect();
        if bodies.is_empty() {
            self.toasts.push(Level::Warning, "There are no solid bodies to project.".to_owned());
            return;
        }
        // 1.x pre-selects `sk.bodyId`, which nothing ever sets: "All visible bodies"
        self.sk.project = Some(sketch::project::ProjectForm::new(bodies, self.shell.inch));
    }

    /// `sketchOffset()`: ask the distance (the shapes must be selected first).
    fn sketch_offset(&mut self) {
        let Some(m) = self.sketch.as_ref() else { return };
        if m.s.sel.is_empty() {
            self.toasts.push(Level::Warning, "Select the shapes to offset first (Select tool, Shift-click for more).".to_owned());
            return;
        }
        self.shell.dialogs.open_prompt(SK_OFFSET_TAG, "Offset distance (mm, negative = inward)", "2");
    }

    /// The answer of the Offset prompt.
    pub(super) fn sketch_offset_answer(&mut self, text: &str) {
        let d = crate::tools::fields::number::parse_num_text(text);
        let Some(m) = self.sketch.as_ref() else { return };
        let entities: Vec<Value> = m.s.entities.iter().filter_map(|e| serde_json::to_value(e).ok()).collect();
        let ids: Vec<&String> = m.s.sel.iter().collect();
        let data = json!({ "op": "offset", "entities": entities, "ids": ids, "distance": d, "seed": m.s.seed });
        self.sketch_apply(vec![SketchEffect::SketchOp { op: "offset".into(), data }]);
    }

    /// A tool's answer to `sketch_op`, or the part-edge `refs`.
    pub(super) fn on_sketch_op(&mut self, seq: u64, result: Result<Value, String>) {
        let Some(m) = self.sketch.as_mut() else { return };
        if m.refs_seq == Some(seq) {
            m.refs_seq = None;
            // a refused `refs` is silent: the sketch just has no part edges to measure from (`catch { refs = [] }`)
            let Ok(v) = result else { return };
            let (fresh, kind) = sk_session::refs_of(&v);
            m.set_refs(&fresh, kind);
            let toast = sketch::relations::missing_toast(&m.s);
            let cmd = m.changed();
            if let Some(c) = cmd {
                self.link.send(c);
            }
            if let Some(t) = toast {
                self.sketch_apply(vec![t]);
            }
            return;
        }
        let Some((want, op)) = m.op.clone() else { return };
        if want != seq {
            return; // the tool that asked has changed since
        }
        m.op = None;
        match result {
            Err(e) => self.toasts.push(Level::Error, e),
            Ok(v) if op == "project" => {
                // the Project dialog has no tool to take the answer
                let fx = self.sketch.as_mut().map(|m| sketch::project::apply_result(&mut m.s, &v)).unwrap_or_default();
                self.sketch_apply(fx);
            }
            Ok(v) => {
                let before = self.sketch.as_ref().map(|m| m.s.history.len()).unwrap_or(0);
                let mut fx = self.sk_call(|m, env| m.op_result(&op, &v, env)).unwrap_or_default();
                // a one-shot op (Offset from the ribbon) has no tool to take the answer: replace the shapes
                let took = self.sketch.as_ref().map(|m| m.s.history.len() != before).unwrap_or(true);
                if !took && matches!(op.as_str(), "offset" | "mirror" | "trim" | "explode") {
                    if let Some(list) = v.get("entities").and_then(Value::as_array) {
                        if let Ok(ents) = stl_sketch::entities::parse_entities(list) {
                            if let Some(m) = self.sketch.as_mut() {
                                fx.extend(sketch_replace_entities(&mut m.s, ents));
                            }
                        }
                    }
                }
                self.sketch_apply(fx);
                if op == "text" {
                    self.sketch_set_tool("select");
                }
            }
        }
    }

    /// `Update::Solved`.
    pub(super) fn on_solved(&mut self, seq: u64, entities: Vec<stl_sketch::entities::Entity>, ok: bool, error: f64) {
        if let Some(c) = self.sketch.as_mut().and_then(|m| m.solved(seq, entities, ok, error)) {
            self.link.send(c);
        }
        self.sketch_drain_toast();
    }

    /// `finishSketch(thenExtrude)`: send the step; the sketch closes when the engine has it.
    pub fn finish_sketch(&mut self, extrude: bool) {
        let Some(m) = self.sketch.as_mut() else { return };
        if m.finishing.is_some() {
            return;
        }
        let params = m.s.step_params();
        let empty = m.s.entities.is_empty();
        match m.s.feature_id.clone() {
            Some(id) => self.link.send(Command::EditFeature { id, params, rollback_after: Some(m.prev_rollback) }),
            None => {
                if empty {
                    self.toasts.push(Level::Warning, "The sketch is empty.".to_owned());
                }
                self.link.send(Command::AddFeature { kind: "sketch".into(), params, index: None, name: None });
            }
        }
        m.finishing = Some(extrude);
        self.sk.known = self.timeline_view.items.iter().map(|i| i.id.clone()).collect();
    }

    /// The engine took the step (a refusal would have cleared `finishing` first).
    pub(super) fn sketch_after_timeline(&mut self) {
        let Some(m) = self.sketch.as_ref() else { return };
        let Some(extrude) = m.finishing else { return };
        let sid = match &m.s.feature_id {
            Some(id) => Some(id.clone()),
            None => self.timeline_view.items.iter().find(|i| !self.sk.known.contains(&i.id)).map(|i| i.id.clone()),
        };
        let Some(sid) = sid else { return };
        let profile = sk_session::has_profile(&m.s.entities);
        self.leave_sketch();
        if extrude && profile {
            let mut v = Values::new();
            v.insert("sketch".into(), json!(sid));
            self.open_tool("extrude", Some(v));
        }
    }

    /// `cancelSketch()`: an edited step gets its rollback back; nothing else changes.
    pub fn cancel_sketch(&mut self) {
        let Some(m) = self.sketch.as_ref() else { return };
        let (was_edit, prev) = (m.s.feature_id.is_some(), m.prev_rollback);
        self.leave_sketch();
        if was_edit {
            self.link.send(Command::Rollback(prev));
            self.pending_marker = Some(prev);
        }
    }

    /// Everything that goes when the sketch closes.
    fn leave_sketch(&mut self) {
        self.sketch = None;
        self.sk.text = Default::default();
        self.sk.over = None;
        self.sk.project = None;
        self.viewport.set_mouse_mode(MouseMode::Model);
        sketch::view::clear(&mut self.viewport);
        self.viewport.sync_sketches(&self.sketches, None);
        self.viewport.set_viewcube_inset(0.0);
        self.shell.hint.clear();
        self.shell.hud = None;
        sketch::dims::set_pending(Default::default());
    }

    /// The sketch branch of `onRibbon(k)`.
    pub(super) fn sketch_ribbon(&mut self, k: &str) {
        if k == "sk_project" {
            return self.sketch_project();
        }
        if sk_session::is_tool_key(k) {
            return self.sketch_set_tool(k);
        }
        match k {
            "sk_offset" => self.sketch_offset(),
            "sk_import" => self.sketch_import(),
            "construction" => {
                let fx = self.sketch.as_mut().map(|m| sketch::keys::toggle_construction(&mut m.s)).unwrap_or_default();
                self.sketch_apply(fx);
            }
            "sk_delete" => {
                let fx = self.sketch.as_mut().map(|m| sketch::keys::delete_selected(&mut m.s)).unwrap_or_default();
                self.sketch_apply(fx);
            }
            "look" => {
                if let Some(f) = self.sketch.as_ref().map(|m| m.s.frame) {
                    self.viewport.look_at_plane(&f, None);
                }
            }
            "finish" => self.finish_sketch(false),
            "finish_extrude" => self.finish_sketch(true),
            "sk_cancel" => self.cancel_sketch(),
            _ => {}
        }
    }

    /// `sketchImport()`: pick an SVG or DXF; its shapes join the sketch. Parked until Task 6's
    /// `import::import_file` lands.
    fn sketch_import(&mut self) {
        self.sk.import = Some(self.platform.open_file(&[("Vector drawings", &["svg", "dxf"])]));
    }

    /// A key of the sketcher (`KeyAction::Sketcher`).
    pub(super) fn sketch_key(&mut self, press: keys::KeyPress) {
        if self.sketch_modal_open() {
            return;
        }
        if let Some((_, fx)) = self.sk_call(|m, env| sketch::keys::on_key(m, env, press.key, press.mods)) {
            self.sketch_apply(fx);
        }
    }

    /// 1.x `modal()`: while the over-defined question, the Project dialog or the text dialog is
    /// open, the sketch tools get no pointer or key input (the dialog takes Esc/Enter itself).
    pub(super) fn sketch_modal_open(&self) -> bool {
        self.sk.over.is_some() || self.sk.project.is_some() || self.sk.text.open
    }

    /// Pointer and text events over the viewport while a sketch is open.
    pub(super) fn sketch_pointer(&mut self, ui: &egui::Ui, resp: &egui::Response) {
        if self.sketch.is_none() {
            return;
        }
        if self.sketch_modal_open() {
            if let Some(m) = self.sketch.as_mut() {
                m.down = false;
            }
            return;
        }
        let events: Vec<egui::Event> = ui.input(|i| i.events.clone());
        let mods = ui.input(|i| i.modifiers);
        let typing = ui.ctx().egui_wants_keyboard_input();
        let pointer = ui.input(|i| i.pointer.clone());
        let opts = ui.ctx().options(|o| o.input_options.clone());
        let hovered = resp.hovered();
        let mut inputs: Vec<SketchInput> = Vec::new();
        {
            let vp = &self.viewport;
            let Some(m) = self.sketch.as_mut() else { return };
            for ev in &events {
                match ev {
                    egui::Event::PointerMoved(pos) if hovered || m.down => {
                        if let Some(p) = vp.ray_to_plane(*pos, &m.s.frame) {
                            inputs.push(SketchInput::Move { uv: m.s.to_uv(p), pos: *pos, mods });
                        }
                    }
                    egui::Event::PointerButton { button: egui::PointerButton::Primary, pressed, pos, .. } => {
                        if *pressed {
                            if !hovered {
                                continue;
                            }
                            m.down = true;
                        } else {
                            if !m.down {
                                continue;
                            }
                            m.down = false;
                        }
                        let _ = pos;
                        if let Some(i) = sketch::touch::input(ev, vp, &m.s, &mut m.touch, mods, &pointer, &opts) {
                            inputs.push(i);
                        }
                    }
                    egui::Event::Touch { .. } if hovered || m.down => {
                        if let Some(i) = sketch::touch::input(ev, vp, &m.s, &mut m.touch, mods, &pointer, &opts) {
                            inputs.push(i);
                        }
                    }
                    egui::Event::Text(t) if !typing => {
                        let typed: String = t.chars().filter(|c| c.is_ascii_digit() || matches!(c, '.' | '-' | '/' | ',')).collect();
                        if !typed.is_empty() && m.s.tool != "select" {
                            inputs.push(SketchInput::Typed(typed));
                        }
                    }
                    _ => {}
                }
            }
        }
        for i in inputs {
            if let Some((_, fx)) = self.sk_call(|m, env| m.input(&i, env)) {
                self.sketch_apply(fx);
            }
        }
    }

    /// Draw the sketch's own parts each frame: the shapes when changed, relation glyphs, dimensions.
    pub(super) fn sketch_frame(&mut self, ui: &egui::Ui) {
        let inch = self.shell.inch;
        let params = self.sk_params();
        let mut fx = Vec::new();
        if let Some(m) = self.sketch.as_mut() {
            if m.dirty {
                m.dirty = false;
                sketch::view::redraw(&mut self.viewport, m);
            }
            fx.extend(sketch::relations::paint(ui, &mut m.s, &self.viewport, inch));
            fx.extend(sketch::dims::paint(ui, &mut m.s, &self.viewport, inch, &params));
        }
        // an import chosen in the file dialog
        if let Some(p) = self.sk.import.as_ref().and_then(|p| p.take()) {
            self.sk.import = None;
            if let Some(f) = p {
                fx.extend(self.sketch_import_file(f));
            }
        }
        self.sketch_apply(fx);
        if self.sketch.as_ref().is_some_and(|m| m.dirty) {
            ui.ctx().request_repaint();
        }
    }

    /// The panel, the dimension box and the text dialog.
    pub(super) fn sketch_overlays(&mut self, ctx: &egui::Context, tk: &Tokens, ws: Rect) {
        let inch = self.shell.inch;
        let params = self.sk_params();
        let mut fx = Vec::new();
        let (mut finish, mut cancel) = (false, false);
        let mut toasts = Vec::new();
        if let Some(m) = self.sketch.as_mut() {
            let out = sketch::panel::show(ctx, tk, m, ws, inch);
            finish = out.finish;
            cancel = out.cancel;
            fx.extend(out.effects);
            toasts.extend(out.toasts);
            if let Some((g, _)) = out.edit_text {
                fx.push(SketchEffect::Dialog(SketchDialog::Text { group: Some(g) }));
            }
            let anchor = m.panel.modify.as_ref().and_then(|md| sketch::dims::label_screen(&m.s, &self.viewport, md.idx));
            let (e2, t2) = sketch::panel::modify_box(ctx, tk, m, ws, inch, &params, anchor);
            fx.extend(e2);
            toasts.extend(t2);
        }
        for (l, t) in toasts {
            self.toasts.push(l, t);
        }
        // the text dialog, over a scrim
        if self.sk.text.open {
            Area::new(Id::new("sk-text-scrim")).order(Order::Foreground).fixed_pos(ctx.content_rect().min).show(ctx, |ui| {
                let (r, _) = ui.allocate_exact_size(ctx.content_rect().size(), Sense::click_and_drag());
                ui.painter().rect_filled(r, 0.0, tk.scrim);
            });
            let mut eff = None;
            Area::new(Id::new("sk-text-card")).order(Order::Tooltip).anchor(Align2::CENTER_CENTER, Vec2::ZERO).show(ctx, |ui| {
                Frame::new().fill(tk.panel).stroke(Stroke::new(1.0, tk.line)).corner_radius(CornerRadius::same(9)).shadow(tk.shadow_lg).inner_margin(Margin { left: 20, right: 20, top: 18, bottom: 14 }).show(ui, |ui| {
                    ui.set_max_width(420.0);
                    if let Some(m) = self.sketch.as_mut() {
                        eff = sketch::text::dialog(ui, tk, &mut m.s, &mut self.sk.text);
                    }
                });
            });
            fx.extend(eff);
        }
        fx.extend(self.sketch_modals(ctx, tk));
        self.sketch_apply(fx);
        if finish {
            self.finish_sketch(false);
        } else if cancel {
            self.cancel_sketch();
        }
    }

    /// "Over defined" and "Project outline", over a scrim (1.x `modal`).
    fn sketch_modals(&mut self, ctx: &egui::Context, tk: &Tokens) -> Vec<SketchEffect> {
        let mut fx = Vec::new();
        if self.sk.over.is_none() && self.sk.project.is_none() {
            return fx;
        }
        Area::new(Id::new("sk-modal-scrim")).order(Order::Foreground).fixed_pos(ctx.content_rect().min).show(ctx, |ui| {
            let (r, _) = ui.allocate_exact_size(ctx.content_rect().size(), Sense::click_and_drag());
            ui.painter().rect_filled(r, 0.0, tk.scrim);
        });
        let card = |ui: &mut egui::Ui, add: &mut dyn FnMut(&mut egui::Ui)| {
            Frame::new().fill(tk.panel).stroke(Stroke::new(1.0, tk.line)).corner_radius(CornerRadius::same(9)).shadow(tk.shadow_lg).inner_margin(Margin { left: 20, right: 20, top: 18, bottom: 14 }).show(ui, |ui| {
                ui.set_max_width(420.0);
                add(ui);
            });
        };
        if self.sk.over.is_some() {
            let mut ans: Option<bool> = None;
            Area::new(Id::new("sk-over-card")).order(Order::Tooltip).anchor(Align2::CENTER_CENTER, Vec2::ZERO).show(ctx, |ui| {
                card(ui, &mut |ui| {
                    ui.label(RichText::new("Over defined").font(egui::FontId::new(15.0, egui::FontFamily::Name("bold".into()))).color(tk.strong));
                    ui.add_space(8.0);
                    ui.add(egui::Label::new(RichText::new("The sketch's other dimensions and relations already set this size, so another dimension can't drive it.").size(12.0).color(tk.text)).wrap());
                    ui.add_space(6.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        ui.label(RichText::new("Add it as a ").size(12.0).color(tk.text));
                        ui.label(RichText::new("reference").size(12.0).strong().color(tk.strong));
                        ui.label(RichText::new(" dimension? It shows the size in brackets and updates by itself, but doesn't change anything.").size(12.0).color(tk.text));
                    });
                    ui.add_space(10.0);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        if crate::tools::panel::ctl_button(ui, tk, "Add as reference", true).clicked() {
                            ans = Some(true);
                        }
                        if crate::tools::panel::ctl_button(ui, tk, "Cancel", false).clicked() {
                            ans = Some(false);
                        }
                    });
                });
            });
            if ans.is_none() {
                if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)) {
                    ans = Some(true);
                } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
                    ans = Some(false);
                }
            }
            if let Some(yes) = ans {
                if let Some(c) = self.sk.over.take() {
                    if let (true, Some(m)) = (yes, self.sketch.as_mut()) {
                        fx.extend(sketch::dims::add_reference(&mut m.s, c));
                    }
                }
            }
        }
        if let Some(mut form) = self.sk.project.take() {
            let mut ans = None;
            Area::new(Id::new("sk-project-card")).order(Order::Tooltip).anchor(Align2::CENTER_CENTER, Vec2::ZERO).show(ctx, |ui| {
                card(ui, &mut |ui| ans = sketch::project::dialog(ui, tk, &mut form));
            });
            match ans {
                None => self.sk.project = Some(form),
                Some(sketch::project::ProjectAnswer::Cancel) => {}
                Some(sketch::project::ProjectAnswer::Project) => {
                    if let Some(m) = self.sketch.as_ref() {
                        let body = (!form.body.is_empty()).then_some(form.body.as_str());
                        fx.push(sketch::project::project_request(&m.s, &form.mode, body, form.offset_mm(), form.construction));
                    }
                }
            }
        }
        fx
    }

    /// Entities of a picked SVG or DXF (`importEntities`): new ids, selected.
    fn sketch_import_file(&mut self, f: crate::platform::PickedFile) -> Vec<SketchEffect> {
        match sketch::import::import_file(&f.name, &f.bytes) {
            Err(e) => vec![SketchEffect::Toast(Level::Error, e)],
            Ok(list) => {
                let Some(m) = self.sketch.as_mut() else { return Vec::new() };
                m.s.push();
                m.s.sel.clear();
                for mut e in list {
                    let id = m.s.new_id();
                    sketch_set_entity_id(&mut e, id.clone());
                    m.s.entities.push(e);
                    m.s.sel.insert(id);
                }
                vec![SketchEffect::Changed]
            }
        }
    }
}

pub const SK_OFFSET_TAG: &str = "sk-offset";

fn sketch_set_entity_id(e: &mut stl_sketch::entities::Entity, id: String) {
    use stl_sketch::entities::Entity::*;
    match e {
        Line(x) => x.id = Some(id),
        Arc(x) => x.id = Some(id),
        Circle(x) => x.id = Some(id),
        Rect(x) => x.id = Some(id),
        Polygon(x) => x.id = Some(id),
        Slot(x) => x.id = Some(id),
        Polyline(x) => x.id = Some(id),
        Point(x) => x.id = Some(id),
    }
}

/// `replaceEntities(ents)`: new shapes; relations that lose a shape go; the selection keeps what is left.
fn sketch_replace_entities(s: &mut SketchSession, ents: Vec<stl_sketch::entities::Entity>) -> Vec<SketchEffect> {
    s.push();
    s.entities = ents;
    let all = sketch::solve::all_ents(s);
    let w = stl_sketch::solver::World::new(&all);
    s.constraints.retain(|c| stl_sketch::solver::refs_exist(c, &w));
    let ids: std::collections::BTreeSet<String> = s.entities.iter().filter_map(|e| e.id().map(str::to_owned)).collect();
    s.sel.retain(|i| ids.contains(i));
    vec![SketchEffect::Changed]
}
