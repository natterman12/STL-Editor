//! `.stlx` project files: a zip with `document.json` and the imported meshes in `sources/`.
//!
//! The layout is the one `Document.save` / `Document.open` in `stleditor/document.py` use, so
//! 1.x opens what this writes and this opens what 1.x wrote. Keys this version doesn't know
//! (top level, in `ui`, in a feature) are kept and written back.

use serde_json::{Map, Value, json};
use std::collections::BTreeSet;
use std::io::{Cursor, Read, Write};
use std::path::Path;

use crate::params::Param;
use crate::{Document, Feature, Registry, Source};

/// The format number this version writes and the newest it reads.
pub const FORMAT_VERSION: u64 = 1;

/// A message for the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileError(pub String);

impl std::fmt::Display for FileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for FileError {}

const NOT_A_PROJECT: &str = "This isn't an STL Editor project.";
const KNOWN: [&str; 7] = ["format", "app", "features", "rollback", "ui", "params", "sources"];

fn err<T>(m: &str) -> Result<T, FileError> {
    Err(FileError(m.to_string()))
}

/// The project as file bytes.
pub fn to_bytes(doc: &Document) -> Result<Vec<u8>, FileError> {
    let used: BTreeSet<&str> = doc
        .features
        .iter()
        .filter(|f| f.kind == "import")
        .filter_map(|f| f.params.get("source").and_then(Value::as_str))
        .collect();
    let mut sources = Map::new();
    for (sid, s) in &doc.sources {
        if used.contains(sid.as_str()) {
            sources.insert(sid.clone(), json!({"name": s.name, "ext": s.ext}));
        }
    }
    let mut d = Map::new();
    d.insert("format".into(), json!(FORMAT_VERSION));
    d.insert("app".into(), json!("STL Editor"));
    d.insert("features".into(), serde_json::to_value(&doc.features).map_err(|e| FileError(e.to_string()))?);
    d.insert("rollback".into(), doc.rollback.map_or(Value::Null, |r| json!(r)));
    d.insert("ui".into(), Value::Object(doc.ui.clone()));
    d.insert("params".into(), serde_json::to_value(&doc.params).map_err(|e| FileError(e.to_string()))?);
    d.insert("sources".into(), Value::Object(sources));
    for (k, v) in &doc.extra {
        d.entry(k.clone()).or_insert_with(|| v.clone());
    }
    let text = serde_json::to_vec_pretty(&Value::Object(d)).map_err(|e| FileError(e.to_string()))?;

    let io = |e: &dyn std::fmt::Display| FileError(format!("Couldn't write the project: {e}"));
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
    w.start_file("document.json", opts).map_err(|e| io(&e))?;
    w.write_all(&text).map_err(|e| io(&e))?;
    for (sid, s) in &doc.sources {
        if used.contains(sid.as_str()) {
            w.start_file(format!("sources/{sid}{}", s.ext), opts).map_err(|e| io(&e))?;
            w.write_all(&s.data).map_err(|e| io(&e))?;
        }
    }
    Ok(w.finish().map_err(|e| io(&e))?.into_inner())
}

/// Read a project and evaluate it. Errors in single features are in the document's results,
/// not here.
pub fn from_bytes(data: &[u8], registry: Registry) -> Result<Document, FileError> {
    let Ok(mut z) = zip::ZipArchive::new(Cursor::new(data)) else {
        return err(NOT_A_PROJECT);
    };
    let mut text = Vec::new();
    match z.by_name("document.json") {
        Ok(mut f) => {
            if f.read_to_end(&mut text).is_err() {
                return err(NOT_A_PROJECT);
            }
        }
        Err(_) => return err(NOT_A_PROJECT),
    }
    let Ok(Value::Object(mut d)) = serde_json::from_slice::<Value>(&text) else {
        return err(NOT_A_PROJECT);
    };
    if d.get("format").and_then(Value::as_u64).is_some_and(|f| f > FORMAT_VERSION) {
        return err("This project was made by a newer version of STL Editor.");
    }

    let mut doc = Document::new(registry);
    let Some(features) = d.remove("features").and_then(|v| serde_json::from_value::<Vec<Feature>>(v).ok()) else {
        return err(NOT_A_PROJECT);
    };
    doc.features = features;
    doc.rollback = d.get("rollback").and_then(Value::as_u64).map(|r| r as usize);
    if let Some(Value::Object(ui)) = d.remove("ui") {
        doc.ui = ui;
    }
    if let Some(p) = d.remove("params") {
        let Ok(p) = serde_json::from_value::<Vec<Param>>(p) else {
            return err(NOT_A_PROJECT);
        };
        doc.params = p;
    }
    if let Some(Value::Object(sources)) = d.get("sources") {
        for (sid, meta) in sources {
            let text = |k: &str| meta.get(k).and_then(Value::as_str).map(str::to_string);
            let (Some(name), Some(ext)) = (text("name"), text("ext")) else {
                return err(NOT_A_PROJECT);
            };
            let mut bytes = Vec::new();
            let read = match z.by_name(&format!("sources/{sid}{ext}")) {
                Ok(mut f) => f.read_to_end(&mut bytes).is_ok(),
                Err(_) => false,
            };
            if !read {
                return err("The imported file is missing from the project.");
            }
            doc.sources.insert(sid.clone(), Source { name, ext, data: bytes });
        }
    }
    doc.extra = d.into_iter().filter(|(k, _)| !KNOWN.contains(&k.as_str())).collect();
    doc.resume_ids();
    doc.evaluate();
    Ok(doc)
}

pub fn save(doc: &Document, path: &Path) -> Result<(), FileError> {
    let bytes = to_bytes(doc)?;
    std::fs::write(path, bytes).map_err(|e| FileError(format!("Couldn't save the project: {e}")))
}

pub fn open(path: &Path, registry: Registry) -> Result<Document, FileError> {
    let bytes = std::fs::read(path).map_err(|e| FileError(format!("Couldn't open the file: {e}")))?;
    from_bytes(&bytes, registry)
}
