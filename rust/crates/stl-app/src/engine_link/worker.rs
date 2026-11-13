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
use super::{Command, Level, Update};

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
}

fn is_project(name: &str) -> bool {
    name.to_lowercase().ends_with(".stlx")
}

impl Worker {
    /// `cancel` is set by the link when the user presses Cancel; the worker clears it when the
    /// command it stopped has ended.
    pub fn new(sink: Sink, cancel: Arc<AtomicBool>) -> Worker {
        let mut w = Worker { sink, cancel, doc: Document::new(Registry::builtin()), views: Views::new(), saved: None, lost: false };
        w.hook();
        w
    }

    /// Connects the cancel flag and the progress report to the current document.
    fn hook(&mut self) {
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
        if self.lost || matches!(c, Command::Cancel) {
            return; // Cancel is a flag the link sets while the command runs
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
        self.cancel.store(false, Ordering::SeqCst);
        if catch_unwind(AssertUnwindSafe(|| self.refresh())).is_err() {
            self.lost = true;
            self.send(Update::EngineLost);
        }
        self.send(Update::Busy(false));
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
        // whatever happened, the UI is told what the document looks like now
        self.refresh();
        if let Err(e) = result {
            self.fail(e);
        }
    }

    fn apply(&mut self, c: Command) -> Result<(), FeatureError> {
        match c {
            Command::Open { name, bytes } => self.open(&name, bytes),
            Command::Import { name, bytes } => self.doc.transact(|d| import(d, &name, bytes)),
            Command::AddFeature { kind, params, index } => {
                if kind == TEST_PANIC_KIND {
                    panic!("test panic");
                }
                #[cfg(not(target_arch = "wasm32"))]
                if kind == TEST_SLOW_KIND {
                    return self.slow();
                }
                let params = object(params);
                let r = self.doc.transact(|d| {
                    let id = d.add_feature(&kind, params, None)?;
                    if let Some(i) = index.filter(|&i| i < d.features.len() - 1) {
                        d.move_feature(&id, i)?;
                    }
                    Ok(id)
                })?;
                self.report(&r);
                Ok(())
            }
            Command::EditFeature { id, params } => {
                let params = object(params);
                self.doc.transact(|d| d.edit_feature(&id, Some(params), None, None, RollbackAfter::Keep))?;
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
            Command::Cancel => Ok(()),
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
