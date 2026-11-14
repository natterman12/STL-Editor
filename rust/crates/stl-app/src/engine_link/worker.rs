//! What the engine does with a `Command`, shared by the desktop link (a thread) and the web
//! link (a Web Worker). No threads and no native-only APIs in here: the links run `Worker`
//! on their own thread and carry its updates to the UI.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde_json::{json, Map, Value};
use stl_engine::stlx;
use stl_engine::{Document, FeatureError, Registry, RollbackAfter};
use stl_io::ExportOptions;

use super::views::{self, Views};
use super::{Command, FinsAsk, Level, Update};
use stl_engine::FinsHost;

/// Where updates go. `Send + Sync` so the document's progress callback can hold a copy; the web
/// link, which is single threaded, wraps its postMessage call to satisfy this.
pub type Sink = Arc<dyn Fn(Update) + Send + Sync>;

/// Test hook: adding a feature of this kind makes the worker panic, to try the recovery path.
/// It is not a feature kind of the engine.
#[doc(hidden)]
pub const TEST_PANIC_KIND: &str = "__test_panic";

/// Test hook (native only): a slow command. It adds a box, reports `Progress` labelled
/// "slow", waits about 400 ms (or until cancelled), then adds a second box, so a cancel
/// lands in the middle of a command that has already changed the document.
#[doc(hidden)]
pub const TEST_SLOW_KIND: &str = "__test_slow";

pub struct Worker {
    sink: Sink,
    cancel: Arc<AtomicBool>,
    doc: Document,
    views: Views,
    /// The last saved (or opened) project file, for Restart.
    saved: Option<Vec<u8>>,
    lost: bool,
    /// The Support Fins engine, started the first time something asks for it.
    fins: Option<Arc<dyn FinsHost>>,
}

fn is_project(name: &str) -> bool {
    name.to_lowercase().ends_with(".stlx")
}

impl Worker {
    /// `cancel` is set by the link when the user presses Cancel; the worker clears it when the
    /// command it stopped has ended.
    pub fn new(sink: Sink, cancel: Arc<AtomicBool>) -> Worker {
        let mut w = Worker { sink, cancel, doc: Document::new(Registry::builtin()), views: Views::new(), saved: None, lost: false, fins: None };
        w.hook();
        w
    }

    /// Connects the cancel flag and the progress report to the current document.
    fn hook(&mut self) {
        if let Some(h) = &self.fins {
            self.doc.set_fins_host(h.clone());
        }
        self.doc.set_cancel(self.cancel.clone());
        let sink = self.sink.clone();
        self.doc.set_progress(Box::new(move |done, total, label| {
            sink(Update::Progress { done, total, label: label.to_string() });
        }));
    }

    pub fn is_lost(&self) -> bool {
        self.lost
    }

    fn send(&self, u: Update) {
        (self.sink)(u);
    }

    /// Run one command. A panic anywhere in it is caught: the worker sends `EngineLost` and
    /// ignores commands until `restart`.
    pub fn handle(&mut self, c: Command) {
        if self.lost || matches!(c, Command::Cancel | Command::ClearPreview) {
            return; // Cancel is a flag the link sets while the command runs
        }
        if let Command::Fins { seq, ask } = c {
            let run = catch_unwind(AssertUnwindSafe(|| self.fins_ask(seq, ask)));
            if run.is_err() {
                self.lost = true;
                self.send(Update::EngineLost);
            }
            return;
        }
        if let Command::Preview { seq, kind, params, edit_id } = c {
            self.ensure_fins(&kind);
            // A preview is not a change: no Busy overlay, no refresh, nothing in the history.
            let run = catch_unwind(AssertUnwindSafe(|| self.preview(seq, &kind, params, edit_id.as_deref())));
            if run.is_err() {
                self.lost = true;
                self.send(Update::EngineLost);
            }
            return;
        }
        if let Command::SketchOp { seq, op, data } = c {
            // Quiet, like 1.x `post('sketch_op', …, {quiet: true})`: no Busy overlay, no refresh.
            let run = catch_unwind(AssertUnwindSafe(|| self.answer_sketch(seq, &op, &data)));
            if run.is_err() {
                self.lost = true;
                self.send(Update::EngineLost);
            }
            return;
        }
        if let Command::Solve { seq, entities, constraints, locks, fixed } = c {
            // Quiet, like SketchOp: a solve does not change the document, so no Busy overlay and no refresh.
            let run = catch_unwind(AssertUnwindSafe(|| self.answer_solve(seq, &entities, &constraints, &locks, &fixed)));
            if run.is_err() {
                self.lost = true;
                self.send(Update::EngineLost);
            }
            return;
        }
        self.send(Update::Busy(true));
        let run = catch_unwind(AssertUnwindSafe(|| self.run(c)));
        self.cancel.store(false, Ordering::SeqCst);
        if run.is_err() {
            self.lost = true;
            self.send(Update::EngineLost);
        }
        self.send(Update::Busy(false));
    }

    /// Reopen the last saved project, or start a new document, and tell the UI everything.
    pub fn restart(&mut self) {
        self.lost = false;
        self.send(Update::Busy(true));
        self.doc = Document::new(Registry::builtin());
        self.views.clear();
        let saved = self.saved.clone();
        let opened = saved.and_then(|b| stlx::from_bytes(&b, Registry::builtin()).ok());
        if let Some(d) = opened {
            self.doc = d;
        }
        self.hook();
        self.fins_for_loaded();
        self.cancel.store(false, Ordering::SeqCst);
        if catch_unwind(AssertUnwindSafe(|| self.refresh())).is_err() {
            self.lost = true;
            self.send(Update::EngineLost);
        }
        self.send(Update::Busy(false));
    }

    /// The Support Fins engine is started when a Support Fins step is first needed, not at startup.
    fn ensure_fins(&mut self, kind: &str) {
        if self.fins.is_none() && matches!(kind, "supportfins" | "supportwall") {
            self.start_fins();
        }
    }

    /// A project that has Support Fins steps is read on the empty host; start the engine and run
    /// those steps again (1.x started it with the app).
    fn fins_for_loaded(&mut self) {
        if !self.doc.features.iter().any(|f| matches!(f.kind.as_str(), "supportfins" | "supportwall")) {
            return;
        }
        let host = match &self.fins {
            Some(h) => h.clone(),
            None => {
                let h = make_fins_host();
                self.fins = Some(h.clone());
                h
            }
        };
        self.doc.replace_fins_host(host);
    }

    fn start_fins(&mut self) -> Arc<dyn FinsHost> {
        if let Some(h) = &self.fins {
            return h.clone();
        }
        let h = make_fins_host();
        self.doc.set_fins_host(h.clone());
        self.fins = Some(h.clone());
        h
    }

    /// `supportfins/info`, `/strength`, `/suggest`: asked of the engine, never changing the document.
    fn fins_ask(&mut self, seq: u64, ask: FinsAsk) {
        use stl_engine::features::common::fins;
        let name = ask.name().to_owned();
        let host = self.start_fins();
        let result: Result<Value, String> = match ask {
            FinsAsk::Info => fins::info(&*host).map_err(|e| e.0),
            FinsAsk::Strength { body, options, load } => match self.doc.state.bodies.get(&body) {
                None => Err(format!("Body '{body}' no longer exists.")),
                Some(b) => {
                    let mesh = fins::body_mesh(b);
                    fins::orient::strength(&*host, &mesh, &options, Some(&load)).map_err(|e| e.0)
                }
            },
            FinsAsk::Suggest { body, options, top } => match self.doc.state.bodies.get(&body) {
                None => Err(format!("Body '{body}' no longer exists.")),
                Some(b) => {
                    let mesh = fins::body_mesh(b);
                    fins::orient::suggest(&*host, &mesh, &options, top).map_err(|e| e.0)
                }
            },
        };
        self.send(Update::Fins { seq, ask: name, result });
    }

    /// `Document::preview`, as 1.x's `preview` route: the meshes the feature would make, and its
    /// record. The document is not changed.
    fn preview(&mut self, seq: u64, kind: &str, params: Value, edit_id: Option<&str>) {
        let (state, res) = self.doc.preview(kind, object(params), edit_id);
        let result = views::result_json(&res);
        if let Some(e) = res.error.clone() {
            self.send(Update::Preview { seq, pack: Default::default(), msg: None, error: Some(e), warn: false, result });
            return;
        }
        let pack = views::preview_pack(&state, &res);
        let (msg, warn) = views::preview_msg(&res);
        self.send(Update::Preview { seq, pack, msg, error: None, warn, result });
    }

    /// The answer to `Command::SketchOp`. The document is not changed.
    fn answer_sketch(&self, seq: u64, op: &str, data: &serde_json::Value) {
        let result = self.doc.sketch_op(op, data).map_err(|e| e.0);
        self.send(Update::SketchOp { seq, result });
    }

    /// The answer to `Command::Solve`. The document is not changed.
    fn answer_solve(
        &self,
        seq: u64,
        entities: &[stl_sketch::entities::Entity],
        constraints: &[stl_sketch::solver::Constraint],
        locks: &[stl_sketch::solver::Lock],
        fixed: &[stl_sketch::entities::Entity],
    ) {
        let r = stl_sketch::solver::solve(entities, constraints, locks, fixed);
        self.send(Update::Solved { seq, entities: r.entities, ok: r.ok, error: r.error });
    }

    fn fail(&self, e: FeatureError) {
        if e.0 == "Cancelled." {
            self.send(Update::Message { level: Level::Info, text: e.0 });
        } else {
            self.send(Update::Error(e.0));
        }
    }

    fn run(&mut self, c: Command) {
        let result = self.apply(c);
        // The error goes first: the app counts a command as done when the timeline it sends next
        // shows it (a Support Fins turn, a handle's first Add), so a failure must be known by then.
        if let Err(e) = result {
            self.fail(e);
        }
        // whatever happened, the UI is told what the document looks like now
        self.refresh();
    }

    fn apply(&mut self, c: Command) -> Result<(), FeatureError> {
        match c {
            Command::Open { name, bytes } => self.open(&name, bytes),
            Command::Import { name, bytes } => self.doc.transact(|d| import(d, &name, bytes)),
            Command::AddFeature { kind, params, index, name } => {
                if kind == TEST_PANIC_KIND {
                    panic!("test panic");
                }
                #[cfg(not(target_arch = "wasm32"))]
                if kind == TEST_SLOW_KIND {
                    return self.slow();
                }
                self.ensure_fins(&kind);
                let params = object(params);
                let r = self.doc.transact(|d| {
                    let before = d.rollback;
                    let active = before.map_or(d.features.len(), |r| r.min(d.features.len()));
                    match index.filter(|&i| i < active) {
                        // 1.x addFeatureSmart: roll back to the place, add, and put the marker back
                        // where it was, one step further along
                        Some(at) => d.add_feature_at(at, &kind, params, name),
                        None => d.add_feature(&kind, params, name),
                    }
                })?;
                self.report(&r);
                Ok(())
            }
            Command::EditFeature { id, params, rollback_after } => {
                if let Some(k) = self.doc.features.iter().find(|f| f.id == id).map(|f| f.kind.clone()) {
                    self.ensure_fins(&k);
                }
                let params = object(params);
                self.doc.transact(|d| d.edit_feature(&id, Some(params), None, None, rollback_after.map_or(RollbackAfter::Keep, RollbackAfter::Set)))?;
                self.report(&id);
                Ok(())
            }
            Command::DeleteFeature { id } => self.doc.transact(|d| d.delete_feature(&id)),
            Command::SetSuppressed { id, suppressed } => {
                self.doc.transact(|d| d.edit_feature(&id, None, None, Some(suppressed), RollbackAfter::Keep))
            }
            Command::MoveFeature { id, index } => self.doc.transact(|d| d.move_feature(&id, index)),
            Command::SetParams(list) => self.doc.transact(|d| d.set_params(list)),
            Command::Rollback(i) => self.doc.transact(|d| {
                d.set_rollback(i);
                Ok(())
            }),
            Command::Undo => self.doc.transact(|d| {
                d.undo();
                Ok(())
            }),
            Command::Redo => self.doc.transact(|d| {
                d.redo();
                Ok(())
            }),
            Command::BodyProps { id, name, color, hidden } => self.doc.transact(|d| {
                d.set_body_props(&id, name, color, hidden);
                Ok(())
            }),
            Command::Save => {
                let bytes = stlx::to_bytes(&self.doc).map_err(|e| FeatureError(e.0))?;
                self.saved = Some(bytes.clone());
                self.doc.dirty = false;
                self.send(Update::Saved { bytes });
                Ok(())
            }
            Command::Export { format, ids, date } => {
                let ids = self.doc.export_bodies_for(ids.as_deref())?;
                let title = self.doc.export_base_name(&ids, None);
                let opts = ExportOptions { uuid_seed: seed(&title, &date), step_title: title.clone(), step_date: date };
                let (bytes, count) = self.doc.export_bytes(format, Some(&ids), &opts)?;
                self.send(Update::Exported { name: format!("{title}.{}", format.ext()), bytes, count });
                Ok(())
            }
            Command::SketchOp { seq, op, data } => {
                self.answer_sketch(seq, &op, &data);
                Ok(())
            }
            Command::Solve { seq, entities, constraints, locks, fixed } => {
                self.answer_solve(seq, &entities, &constraints, &locks, &fixed);
                Ok(())
            }
            Command::Cancel | Command::ClearPreview | Command::Preview { .. } | Command::Fins { .. } => Ok(()),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn slow(&mut self) -> Result<(), FeatureError> {
        let (sink, cancel) = (self.sink.clone(), self.cancel.clone());
        self.doc.transact(|d| {
            let b = |w: f64| object(json!({"shape": "box", "size": [w, 10.0, 10.0]}));
            d.add_feature("primitive", b(3.0), None)?;
            sink(Update::Progress { done: 0, total: 1, label: "slow".into() });
            let end = std::time::Instant::now() + std::time::Duration::from_millis(400);
            while std::time::Instant::now() < end && !cancel.load(Ordering::SeqCst) {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            d.add_feature("primitive", b(5.0), None)?;
            Ok(())
        })
    }

    /// A project file replaces the document; a mesh file starts a new one with that mesh in it.
    fn open(&mut self, name: &str, bytes: Vec<u8>) -> Result<(), FeatureError> {
        if is_project(name) {
            // The file is evaluated while it loads, so this can't be cancelled and reports its
            // progress afterwards (it needs a hook in stlx::from_bytes to do better).
            let doc = stlx::from_bytes(&bytes, Registry::builtin()).map_err(|e| FeatureError(e.0))?;
            self.doc = doc;
            self.views.clear();
            self.hook();
            self.fins_for_loaded();
            self.saved = Some(bytes);
            let end = self.doc.rollback.map_or(self.doc.features.len(), |r| r.min(self.doc.features.len()));
            for (i, f) in self.doc.features[..end].iter().enumerate() {
                let label = f.name.clone().unwrap_or_else(|| stl_engine::labels::label(&f.kind).to_string());
                self.send(Update::Progress { done: i + 1, total: end, label });
            }
            return Ok(());
        }
        let mut doc = Document::new(Registry::builtin());
        doc.title = Some(name.to_string());
        let old = std::mem::replace(&mut self.doc, doc);
        self.hook();
        match self.doc.transact(|d| import(d, name, bytes)) {
            Ok(()) => {
                self.views.clear();
                self.saved = None;
                self.doc.dirty = false;
                Ok(())
            }
            Err(e) => {
                self.doc = old; // a file that fails to open leaves the project as it was
                self.hook();
                Err(e)
            }
        }
    }

    /// A feature's warning or note, as a message.
    fn report(&self, id: &str) {
        let Some(i) = self.doc.features.iter().position(|f| f.id == id) else { return };
        let Some(r) = self.doc.results().get(i) else { return };
        if let Some(w) = &r.warning {
            self.send(Update::Message { level: Level::Warning, text: w.clone() });
        }
        if let Some(t) = &r.info {
            self.send(Update::Message { level: Level::Info, text: t.clone() });
        }
    }

    /// Everything the UI shows, from the document as it is.
    fn refresh(&mut self) {
        let bodies = self.views.bodies(&self.doc);
        self.send(Update::Bodies(bodies));
        self.send(Update::Timeline(views::timeline(&self.doc)));
        let p = views::params(&mut self.doc);
        self.send(Update::Params(p));
        self.send(Update::Planes(views::planes(&self.doc)));
        let sk = self.views.sketches(&self.doc);
        self.send(Update::Sketches(sk));
    }
}

fn object(v: Value) -> Map<String, Value> {
    match v {
        Value::Object(m) => m,
        _ => Map::new(),
    }
}

/// Bring a mesh file in as an Import feature, put on the bed.
fn import(d: &mut Document, name: &str, bytes: Vec<u8>) -> Result<(), FeatureError> {
    let sid = d.add_source(name, bytes);
    let params = object(json!({"source": sid, "repair": true, "place": "center"}));
    match d.add_feature("import", params, None) {
        Ok(_) => Ok(()),
        Err(e) => {
            d.sources.remove(&sid);
            Err(e)
        }
    }
}

/// A fixed seed from the title and date, so the same export is the same file.
fn seed(title: &str, date: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in title.bytes().chain([0]).chain(date.bytes()) {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// Desktop: the engine files beside the program (or the newest downloaded one), on the system web
/// engine or Boa. Web: the `SupportFinsEngine` the page loaded.
#[cfg(not(target_arch = "wasm32"))]
fn make_fins_host() -> Arc<dyn FinsHost> {
    use std::path::PathBuf;
    let rel = ["web", "vendor", "support-fins"];
    let mut starts: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        starts.extend(exe.parent().map(|p| p.to_path_buf()));
    }
    if let Ok(cwd) = std::env::current_dir() {
        starts.push(cwd);
    }
    starts.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    let mut bundled = None;
    for s in starts {
        for a in s.ancestors() {
            let d: PathBuf = rel.iter().fold(a.to_path_buf(), |p, x| p.join(x));
            if d.join("engine.iife.js").is_file() {
                bundled = Some(d);
                break;
            }
        }
        if bundled.is_some() {
            break;
        }
    }
    let user = std::env::var_os("APPDATA")
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME"))
        .map(|p| PathBuf::from(p).join("STL Editor").join("support-fins"))
        .unwrap_or_default();
    let bundled = bundled.unwrap_or_default();
    match stl_platform::fins::choose_engine(&bundled, &user) {
        Ok(info) => stl_platform::fins::start_fins_host(&info.path),
        Err(_) => Arc::new(stl_engine::NullHost),
    }
}

#[cfg(target_arch = "wasm32")]
fn make_fins_host() -> Arc<dyn FinsHost> {
    Arc::new(crate::web::fins::BrowserHost)
}
