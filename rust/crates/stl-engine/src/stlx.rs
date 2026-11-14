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

/// The project's `document.json` content: 1.x's format 1 fields (`format`, `app`, `features`,
/// `rollback`, `ui`, `params`, `sources` as name and extension only) and the keys this version
/// doesn't know. The imported meshes themselves are not in it. `to_bytes` writes this into the
/// zip, and autosave writes it (plus its own fields) into its folder.
pub fn document_json(doc: &Document) -> Result<Value, FileError> {
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
    Ok(Value::Object(d))
}

/// The project as file bytes.
pub fn to_bytes(doc: &Document) -> Result<Vec<u8>, FileError> {
    let used: BTreeSet<&str> = doc
        .features
        .iter()
        .filter(|f| f.kind == "import")
        .filter_map(|f| f.params.get("source").and_then(Value::as_str))
        .collect();
    let sources: Vec<(&str, &str, &[u8])> =
        doc.sources.iter().filter(|(sid, _)| used.contains(sid.as_str())).map(|(sid, s)| (sid.as_str(), s.ext.as_str(), s.data.as_slice())).collect();
    zip_project(&document_json(doc)?, &sources)
}

/// A project file from a `document.json` content and its meshes as they are, without evaluating
/// anything (Recover turns an autosave folder into the file the engine opens).
pub fn parts_to_bytes(json: &Value, sources: &[(String, String, String, Vec<u8>)]) -> Result<Vec<u8>, FileError> {
    let list: Vec<(&str, &str, &[u8])> = sources.iter().map(|(id, _, ext, data)| (id.as_str(), ext.as_str(), data.as_slice())).collect();
    zip_project(json, &list)
}

/// `(id, extension, bytes)` for each mesh.
fn zip_project(json: &Value, sources: &[(&str, &str, &[u8])]) -> Result<Vec<u8>, FileError> {
    let text = serde_json::to_vec_pretty(json).map_err(|e| FileError(e.to_string()))?;
    let io = |e: &dyn std::fmt::Display| FileError(format!("Couldn't write the project: {e}"));
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
    w.start_file("document.json", opts).map_err(|e| io(&e))?;
    w.write_all(&text).map_err(|e| io(&e))?;
    for (sid, ext, data) in sources {
        w.start_file(format!("sources/{sid}{ext}"), opts).map_err(|e| io(&e))?;
        w.write_all(data).map_err(|e| io(&e))?;
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
    let Ok(Value::Object(d)) = serde_json::from_slice::<Value>(&text) else {
        return err(NOT_A_PROJECT);
    };
    build(d, registry, |sid, _name, ext| {
        let mut bytes = Vec::new();
        let read = match z.by_name(&format!("sources/{sid}{ext}")) {
            Ok(mut f) => f.read_to_end(&mut bytes).is_ok(),
            Err(_) => false,
        };
        read.then_some(bytes)
    })
}

/// The same as [`from_bytes`] for a project that is not in a zip: the `document.json` content
/// and the imported meshes as `(id, name, extension, bytes)`. Autosave and Recover use it.
pub fn from_parts(json: &Value, sources: Vec<(String, String, String, Vec<u8>)>, registry: Registry) -> Result<Document, FileError> {
    let Value::Object(d) = json else {
        return err(NOT_A_PROJECT);
    };
    let mut sources = sources;
    build(d.clone(), registry, |sid, _name, ext| {
        let i = sources.iter().position(|(id, _, e, _)| id == sid && e == ext)?;
        Some(sources.swap_remove(i).3)
    })
}

/// What `from_bytes` and `from_parts` share. `source` gives an imported mesh's bytes from its
/// id, name and extension, or `None` when it is missing.
fn build(mut d: Map<String, Value>, registry: Registry, mut source: impl FnMut(&str, &str, &str) -> Option<Vec<u8>>) -> Result<Document, FileError> {
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
            let Some(bytes) = source(sid, &name, &ext) else {
                return err("The imported file is missing from the project.");
            };
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

#[cfg(test)]
mod tests {
    use super::*;

    fn basic() -> Vec<u8> {
        std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/projects/basic.stlx")).unwrap()
    }

    #[test]
    fn from_parts_is_from_bytes_without_the_zip() {
        let doc = from_bytes(&basic(), Registry::builtin()).unwrap();
        let json = document_json(&doc).unwrap();
        let sources: Vec<(String, String, String, Vec<u8>)> = doc.sources.iter().map(|(id, s)| (id.clone(), s.name.clone(), s.ext.clone(), s.data.clone())).collect();
        let again = from_parts(&json, sources.clone(), Registry::builtin()).unwrap();
        assert_eq!(document_json(&again).unwrap(), json);
        assert_eq!(again.sources.len(), 2);
        // the same project as file bytes, with and without evaluating it
        assert_eq!(from_bytes(&parts_to_bytes(&json, &sources).unwrap(), Registry::builtin()).map(|d| document_json(&d).unwrap()).unwrap(), json);
        assert_eq!(from_bytes(&to_bytes(&doc).unwrap(), Registry::builtin()).map(|d| document_json(&d).unwrap()).unwrap(), json);
    }

    #[test]
    fn from_parts_reports_a_missing_source() {
        let doc = from_bytes(&basic(), Registry::builtin()).unwrap();
        let e = from_parts(&document_json(&doc).unwrap(), Vec::new(), Registry::builtin()).err().unwrap();
        assert_eq!(e.0, "The imported file is missing from the project.");
        let e = from_parts(&json!([1]), Vec::new(), Registry::builtin()).err().unwrap();
        assert_eq!(e.0, NOT_A_PROJECT);
    }
}
