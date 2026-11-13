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
        let ctx = make_ctx(&self.bodies, &self.planes, &self.sketches, &self.timeline_view.items, &self.shell.selection);
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
        let ctx = make_ctx(&self.bodies, &self.planes, &self.sketches, &self.timeline_view.items, &self.shell.selection);
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
            let fins: Vec<Vec<f32>> = self.sf.fin_meshes.iter().map(|m| soup_of(m)).collect();
            let input = CoverageInput {
                positions: mesh.positions.clone(),
                triangles: mesh.triangles.clone(),
                over_faces: over,
                fins,
                gap: if gap > 0.0 { gap } else { 0.2 },
                reach,
                max_sub: 250_000,
            };
            self.sf.cover.start(seq, input);
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
        false
    }

    // ------------------------------------------------------------------ Move / Scale: let go = done

    /// `autoAccept`: letting go of a handle adds the step the first time, then edits it.
    pub(super) fn auto_accept(&mut self) {
        let ctx = make_ctx(&self.bodies, &self.planes, &self.sketches, &self.timeline_view.items, &self.shell.selection);
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
        if let Some(t) = self.tool.as_mut() {
            if let Some(before) = t.accepting.clone() {
                if let Some(idx) = items.iter().position(|id| !before.contains(id)) {
                    t.edit_id = Some(items[idx].clone());
                    t.prev_rollback = t.prev_rollback.map(|m| m + 1);
                    t.accepting = None;
                    again = std::mem::take(&mut t.accept_again);
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
