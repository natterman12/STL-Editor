//! File access and the clock, native or web.

#[cfg(not(target_arch = "wasm32"))]
pub mod native;
#[cfg(target_arch = "wasm32")]
pub mod web;

use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
pub struct PickedFile {
    pub name: String,
    pub bytes: Vec<u8>,
}

/// How a save went (see [`Platform::save_file_path`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SaveOutcome {
    /// The user closed the dialog.
    Cancelled,
    /// Written (or offered as a download). The path is the file's handle when the platform has
    /// one (desktop); `None` on the web, where a save is always a download.
    Done(Option<String>),
}

type Source<T> = Arc<dyn Fn() -> Option<T> + Send + Sync>;

/// A result that arrives later; poll with [`Pending::take`].
pub struct Pending<T> {
    slot: Arc<Mutex<Option<T>>>,
    /// Set by [`Pending::map`]: asks the pending this one was made from.
    source: Option<Source<T>>,
}

impl<T> Clone for Pending<T> {
    fn clone(&self) -> Self {
        Pending { slot: self.slot.clone(), source: self.source.clone() }
    }
}

impl<T> Default for Pending<T> {
    fn default() -> Self {
        Pending { slot: Arc::new(Mutex::new(None)), source: None }
    }
}

impl<T> Pending<T> {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn ready(v: T) -> Self {
        Pending { slot: Arc::new(Mutex::new(Some(v))), source: None }
    }
    pub fn set(&self, v: T) {
        *self.slot.lock().unwrap() = Some(v);
    }
    pub fn take(&self) -> Option<T> {
        if let Some(src) = &self.source {
            return src();
        }
        self.slot.lock().unwrap().take()
    }
}

impl<T: Send + 'static> Pending<T> {
    /// A pending that gives `f` of this one's answer.
    pub fn map<U: Send + 'static>(self, f: impl Fn(T) -> U + Send + Sync + 'static) -> Pending<U> {
        Pending { slot: Arc::new(Mutex::new(None)), source: Some(Arc::new(move || self.take().map(&f))) }
    }
}

pub trait Platform {
    fn open_file(&self, filters: &[(&str, &[&str])]) -> Pending<Option<PickedFile>>;
    fn save_file(&self, suggested: &str, bytes: Vec<u8>) -> Pending<Result<bool, String>>;
    /// Like [`Platform::open_file`], and also the picked file's path where the platform has one.
    /// Desktop Save writes back to it silently. Default: no path (web).
    fn open_file_path(&self, filters: &[(&str, &[&str])]) -> Pending<Option<(PickedFile, Option<String>)>> {
        self.open_file(filters).map(|p| p.map(|f| (f, None)))
    }
    /// Like [`Platform::save_file`] (always asks), and says where it wrote. Default: no path
    /// (web: a download).
    fn save_file_path(&self, suggested: &str, bytes: Vec<u8>) -> Pending<Result<SaveOutcome, String>> {
        self.save_file(suggested, bytes).map(|r| r.map(|ok| if ok { SaveOutcome::Done(None) } else { SaveOutcome::Cancelled }))
    }
    /// Write to a path from an earlier [`SaveOutcome::Done`] or [`Platform::open_file_path`]
    /// without asking. Default: [`Platform::save_file`] with the file's name.
    fn save_to(&self, path: &str, bytes: Vec<u8>) -> Pending<Result<bool, String>> {
        let name = std::path::Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        self.save_file(&name, bytes)
    }
    fn now_iso_utc(&self) -> String;
    fn is_web(&self) -> bool;
}

/// Returns nothing; for tests and the empty window.
pub struct NullPlatform;

impl Platform for NullPlatform {
    fn open_file(&self, _filters: &[(&str, &[&str])]) -> Pending<Option<PickedFile>> {
        Pending::ready(None)
    }
    fn save_file(&self, _suggested: &str, _bytes: Vec<u8>) -> Pending<Result<bool, String>> {
        Pending::ready(Ok(false))
    }
    fn now_iso_utc(&self) -> String {
        String::new()
    }
    fn is_web(&self) -> bool {
        false
    }
}
