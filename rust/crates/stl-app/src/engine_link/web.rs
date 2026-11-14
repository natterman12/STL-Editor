//! The engine in a Web Worker.
//!
//! `worker.js` loads a second instance of this wasm module and calls [`engine_worker`].
//! Commands are structured-clone messages. Meshes go back as transferred `ArrayBuffer`s
//! (detached on the worker afterwards). A panic or a worker error becomes [`Update::EngineLost`];
//! [`WebLink::restart`] reopens the last saved project, or a new document when there is none.
//!
//! What a command does is [`super::worker::Worker`], shared with the desktop link.
//! Cancel is a flag the running rebuild reads before each feature, as on the desktop.
//! A wasm call does not see worker messages. Where the browser has JS Promise
//! Integration, the worker yields between features and the queued cancel is copied
//! onto that flag. That needs no cross-origin isolation. Without it, the worker
//! script substitutes plain functions for `WebAssembly.Suspending` / `promising`
//! (the glue constructs them at startup) and this module does not yield, so the
//! command runs straight through and Cancel is handled at the next command boundary.

// wasm-bindgen's `suspending` / `jspi` attrs expand to experimental helpers; no non-allow fix
// without dropping JSPI cancel-between-features. Listed for Task 6.
#![allow(deprecated)]

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use js_sys::{Array, ArrayBuffer, Uint8Array};
use serde_json::{Map, Value};
use stl_engine::params::Param;
use stl_kernel::MeshArrays;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{DedicatedWorkerGlobalScope, ErrorEvent, MessageEvent, Url, Worker, WorkerOptions, WorkerType};

use super::worker::{self, Worker as EngineWorker};
use super::{BodyView, Command, EngineLink, FinsAsk, ExportFormat, Level, ParamView, PlaneView, SketchOutline, SketchView, TimelineItem, TimelineView, Update};

/// One turn of the worker event loop. The worker script resolves this after it has
/// recorded a cancel that arrived while the command was running.
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = globalThis, js_name = __stlYield, suspending)]
    fn stl_yield();
}

const WORKER_JS: &str = include_str!("../../web/worker.js");

thread_local! {
    static SCOPE: RefCell<Option<DedicatedWorkerGlobalScope>> = const { RefCell::new(None) };
    static ENGINE: RefCell<Option<EngineWorker>> = const { RefCell::new(None) };
    /// `(had a non-empty mesh buffer, every such buffer is detached)`.
    static PROBE: Cell<(bool, bool)> = const { Cell::new((false, true)) };
}

#[used]
static KEEP_ENGINE: fn() = engine_worker;
#[used]
static KEEP_HANDLE: fn(JsValue) -> JsValue = stl_handle;

#[wasm_bindgen]
pub fn engine_worker() {
    let scope: DedicatedWorkerGlobalScope = js_sys::global().unchecked_into();
    SCOPE.with(|s| *s.borrow_mut() = Some(scope));
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = cancel.clone();
    let sink: worker::Sink = Arc::new(move |update| {
        if matches!(update, Update::Busy(true) | Update::Progress { .. }) && js_jspi() {
            // Busy is before the first feature; Progress is after each one that ran.
            // The yield lets the worker script record Cancel, then the next feature sees it.
            // Without JSPI the import cannot suspend; calling it would still mark the
            // fiber as suspended, so the command runs straight through instead.
            stl_yield();
            if js_cancelled() {
                flag.store(true, Ordering::SeqCst);
            }
        }
        emit(update);
    });
    let mut engine = EngineWorker::new(sink, cancel);
    engine.set_autosave_out(Box::new(emit_autosave));
    ENGINE.with(|e| *e.borrow_mut() = Some(engine));
}

/// The autosave copy (or its removal), for the page to keep in IndexedDB.
fn emit_autosave(out: worker::AutosaveOut) {
    match out {
        worker::AutosaveOut::Copy { bytes, title, steps, key: _ } => {
            let msg = js_from_json(&Value::Object(Map::from_iter([
                ("t".into(), Value::String("autosaveCopy".into())),
                ("title".into(), Value::String(title)),
                ("steps".into(), Value::from(steps)),
            ])));
            let buf = bytes_to_buffer(&bytes);
            set(&msg, "bytes", buf.as_ref());
            post_transfer(&msg, &Array::of1(buf.as_ref()));
        }
        worker::AutosaveOut::Clear => post(&js_from_json(&Value::Object(Map::from_iter([("t".into(), Value::String("autosaveClear".into()))])))),
    }
}

/// One message from the page. Returns `{hadMesh, detached}` for the buffers this call transferred.
///
/// `jspi` so [`stl_yield`] can suspend this call between features.
#[wasm_bindgen(jspi)]
pub fn stl_handle(data: JsValue) -> JsValue {
    PROBE.with(|p| p.set((false, true)));
    let t = get(&data, "t").as_string().unwrap_or_default();
    ENGINE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(engine) = slot.as_mut() else {
            emit(Update::Error("The engine is not running.".into()));
            return;
        };
        match t.as_str() {
            "cancel" => {
                // The worker script keeps Cancel and does not deliver it here while a
                // command is running. A message that does arrive found the engine idle,
                // so it is dropped, matching the desktop link.
            }
            "restart" => engine.restart(),
            _ => match decode_command(&data) {
                Ok(cmd) => engine.handle(cmd),
                Err(text) => emit(Update::Error(text)),
            },
        }
    });
    let (had, detached) = PROBE.with(|p| p.get());
    let o = js_sys::Object::new();
    set(&o, "hadMesh", &JsValue::from_bool(had));
    set(&o, "detached", &JsValue::from_bool(detached));
    o.into()
}

fn emit(update: Update) {
    match update {
        Update::Bodies(bodies) => emit_bodies(bodies),
        Update::Saved { bytes } => emit_file("saved", None, &bytes, None),
        Update::Exported { name, bytes, count } => emit_file("exported", Some(name), &bytes, Some(count)),
        Update::Preview { seq, pack, msg, error, warn, result } => emit_preview(seq, pack, msg, error, warn, result),
        other => post(&js_from_json(&plain_update(&other))),
    }
}

fn plain_update(update: &Update) -> Value {
    match update {
        Update::Timeline(t) => json_timeline(t),
        Update::Params(p) => json_params(p),
        Update::Planes(p) => json_planes(p),
        Update::Sketches(s) => json_sketches(s),
        Update::Message { level, text } => Value::Object(Map::from_iter([
            ("t".into(), Value::String("message".into())),
            ("level".into(), Value::String(level_str(*level).into())),
            ("text".into(), Value::String(text.clone())),
        ])),
        Update::Progress { done, total, label } => Value::Object(Map::from_iter([
            ("t".into(), Value::String("progress".into())),
            ("done".into(), Value::from(*done)),
            ("total".into(), Value::from(*total)),
            ("label".into(), Value::String(label.clone())),
        ])),
        Update::Busy(b) => Value::Object(Map::from_iter([
            ("t".into(), Value::String("busy".into())),
            ("busy".into(), Value::Bool(*b)),
        ])),
        Update::Error(text) => Value::Object(Map::from_iter([
            ("t".into(), Value::String("error".into())),
            ("text".into(), Value::String(text.clone())),
        ])),
        Update::Autosaved { ok, error } => Value::Object(Map::from_iter([
            ("t".into(), Value::String("autosaved".into())),
            ("ok".into(), Value::Bool(*ok)),
            ("error".into(), error.clone().map(Value::String).unwrap_or(Value::Null)),
        ])),
        Update::EngineLost => Value::Object(Map::from_iter([("t".into(), Value::String("engineLost".into()))])),
        // Task 6 wires the web worker; this is the update as it would be sent.
        Update::Solved { seq, entities, ok, error } => Value::Object(Map::from_iter([
            ("t".into(), Value::String("solved".into())),
            ("seq".into(), Value::from(*seq)),
            ("entities".into(), serde_json::to_value(entities).unwrap_or_default()),
            ("ok".into(), Value::Bool(*ok)),
            ("error".into(), Value::from(*error)),
        ])),
        Update::SketchOp { seq, result } => Value::Object(Map::from_iter([
            ("t".into(), Value::String("sketchOp".into())),
            ("seq".into(), Value::from(*seq)),
            ("ok".into(), Value::Bool(result.is_ok())),
            ("data".into(), match result { Ok(v) => v.clone(), Err(e) => Value::String(e.clone()) }),
        ])),
        Update::Fins { seq, ask, result } => {
            let (ok, data) = match result {
                Ok(v) => (true, v.clone()),
                Err(e) => (false, Value::String(e.clone())),
            };
            Value::Object(Map::from_iter([
                ("t".into(), Value::String("fins".into())),
                ("seq".into(), Value::from(*seq)),
                ("ask".into(), Value::String(ask.clone())),
                ("ok".into(), Value::Bool(ok)),
                ("data".into(), data),
            ]))
        }
        Update::Bodies(_) | Update::Saved { .. } | Update::Exported { .. } | Update::Preview { .. } => Value::Null,
    }
}

fn json_timeline(t: &TimelineView) -> Value {
    let items: Vec<Value> = t
        .items
        .iter()
        .map(|i| {
            Value::Object(Map::from_iter([
                ("id".into(), Value::String(i.id.clone())),
                ("kind".into(), Value::String(i.kind.clone())),
                ("label".into(), Value::String(i.label.clone())),
                ("error".into(), i.error.clone().map(Value::String).unwrap_or(Value::Null)),
                ("suppressed".into(), Value::Bool(i.suppressed)),
                ("warning".into(), i.warning.clone().map(Value::String).unwrap_or(Value::Null)),
                ("active".into(), Value::Bool(i.active)),
                ("params".into(), i.params.clone()),
            ]))
        })
        .collect();
    Value::Object(Map::from_iter([
        ("t".into(), Value::String("timeline".into())),
        ("items".into(), Value::Array(items)),
        ("marker".into(), t.marker.map(|n| Value::from(n)).unwrap_or(Value::Null)),
        ("canUndo".into(), Value::Bool(t.can_undo)),
        ("canRedo".into(), Value::Bool(t.can_redo)),
    ]))
}

fn json_params(list: &[ParamView]) -> Value {
    let params: Vec<Value> = list
        .iter()
        .map(|p| {
            Value::Object(Map::from_iter([
                ("name".into(), Value::String(p.name.clone())),
                ("expr".into(), Value::String(p.expr.clone())),
                ("note".into(), Value::String(p.note.clone())),
                ("value".into(), p.value.map(Value::from).unwrap_or(Value::Null)),
                ("error".into(), p.error.clone().map(Value::String).unwrap_or(Value::Null)),
                ("used".into(), Value::Bool(p.used)),
            ]))
        })
        .collect();
    Value::Object(Map::from_iter([
        ("t".into(), Value::String("params".into())),
        ("params".into(), Value::Array(params)),
    ]))
}

fn json_planes(list: &[PlaneView]) -> Value {
    let planes: Vec<Value> = list
        .iter()
        .map(|p| {
            Value::Object(Map::from_iter([
                ("id".into(), Value::String(p.id.clone())),
                ("name".into(), Value::String(p.name.clone())),
                ("origin".into(), arr3(p.origin)),
                ("x".into(), arr3(p.x)),
                ("y".into(), arr3(p.y)),
                ("normal".into(), arr3(p.normal)),
                ("visible".into(), Value::Bool(p.visible)),
            ]))
        })
        .collect();
    Value::Object(Map::from_iter([
        ("t".into(), Value::String("planes".into())),
        ("planes".into(), Value::Array(planes)),
    ]))
}

fn json_outline(o: &SketchOutline) -> Value {
    let pts: Vec<Value> = o
        .pts
        .iter()
        .map(|p| Value::Array(vec![Value::from(p[0]), Value::from(p[1])]))
        .collect();
    Value::Object(Map::from_iter([
        ("pts".into(), Value::Array(pts)),
        ("closed".into(), Value::Bool(o.closed)),
        ("construction".into(), Value::Bool(o.construction)),
    ]))
}

fn json_sketches(list: &[SketchView]) -> Value {
    let sketches: Vec<Value> = list
        .iter()
        .map(|s| {
            Value::Object(Map::from_iter([
                ("id".into(), Value::String(s.id.clone())),
                ("name".into(), Value::String(s.name.clone())),
                ("plane".into(), Value::String(s.plane.clone())),
                ("hidden".into(), Value::Bool(s.hidden)),
                ("frame".into(), Value::Array(s.frame.iter().copied().map(arr3).collect())),
                ("outlines".into(), Value::Array(s.outlines.iter().map(json_outline).collect())),
            ]))
        })
        .collect();
    Value::Object(Map::from_iter([
        ("t".into(), Value::String("sketches".into())),
        ("sketches".into(), Value::Array(sketches)),
    ]))
}

fn arr3(v: [f64; 3]) -> Value {
    Value::Array(vec![Value::from(v[0]), Value::from(v[1]), Value::from(v[2])])
}

fn emit_bodies(bodies: Vec<BodyView>) {
    let mut rows = Vec::new();
    let mut bufs: Vec<(ArrayBuffer, u32)> = Vec::new();
    for b in bodies {
        let pos = f32_bytes(&b.mesh.positions);
        let tri = u32_bytes(&b.mesh.triangles);
        let pos_buf = bytes_to_buffer(&pos);
        let tri_buf = bytes_to_buffer(&tri);
        rows.push(Value::Object(Map::from_iter([
            ("id".into(), Value::String(b.id)),
            ("name".into(), Value::String(b.name)),
            ("color".into(), b.color.map(Value::String).unwrap_or(Value::Null)),
            ("hidden".into(), Value::Bool(b.hidden)),
            ("solid".into(), Value::Bool(b.solid)),
            ("revision".into(), Value::from(b.revision)),
        ])));
        bufs.push((pos_buf, pos.len() as u32));
        bufs.push((tri_buf, tri.len() as u32));
    }
    let msg = js_from_json(&Value::Object(Map::from_iter([
        ("t".into(), Value::String("bodies".into())),
        ("bodies".into(), Value::Array(rows)),
    ])));
    let arr = Array::from(&get(&msg, "bodies"));
    for (i, pair) in bufs.chunks(2).enumerate() {
        let item = arr.get(i as u32);
        set(&item, "positions", pair[0].0.as_ref());
        set(&item, "triangles", pair[1].0.as_ref());
    }
    let transfer = Array::new();
    for (buf, _) in &bufs {
        transfer.push(buf.as_ref());
    }
    post_transfer(&msg, &transfer);
    note_mesh(&bufs);
}

/// A preview: the tool's meshes cross as transferred buffers, like the bodies.
fn emit_preview(seq: u64, pack: super::PreviewPack, msg: Option<String>, error: Option<String>, warn: bool, result: Value) {
    let mut rows = Vec::new();
    let mut bufs: Vec<(ArrayBuffer, u32)> = Vec::new();
    for m in &pack.meshes {
        let pos = f32_bytes(&m.mesh.positions);
        let tri = u32_bytes(&m.mesh.triangles);
        rows.push(Value::Object(Map::from_iter([("id".into(), Value::String(m.id.clone()))])));
        bufs.push((bytes_to_buffer(&pos), pos.len() as u32));
        bufs.push((bytes_to_buffer(&tri), tri.len() as u32));
    }
    let strs = |v: &[String]| Value::Array(v.iter().cloned().map(Value::String).collect());
    let colors: Map<String, Value> = pack.colors.iter().map(|(k, v)| (k.clone(), Value::String(v.clone()))).collect();
    let msg = js_from_json(&Value::Object(Map::from_iter([
        ("t".into(), Value::String("preview".into())),
        ("seq".into(), Value::from(seq)),
        ("msg".into(), msg.map(Value::String).unwrap_or(Value::Null)),
        ("error".into(), error.map(Value::String).unwrap_or(Value::Null)),
        ("warn".into(), Value::Bool(warn)),
        ("result".into(), result),
        ("meshes".into(), Value::Array(rows)),
        ("removed".into(), strs(&pack.removed)),
        ("hide".into(), strs(&pack.hide)),
        ("ghosts".into(), strs(&pack.ghosts)),
        ("colors".into(), Value::Object(colors)),
    ])));
    let arr = Array::from(&get(&msg, "meshes"));
    for (i, pair) in bufs.chunks(2).enumerate() {
        let item = arr.get(i as u32);
        set(&item, "positions", pair[0].0.as_ref());
        set(&item, "triangles", pair[1].0.as_ref());
    }
    let transfer = Array::new();
    for (buf, _) in &bufs {
        transfer.push(buf.as_ref());
    }
    post_transfer(&msg, &transfer);
    note_mesh(&bufs);
}

fn emit_file(kind: &str, name: Option<String>, bytes: &[u8], count: Option<usize>) {
    let mut map = Map::new();
    map.insert("t".into(), Value::String(kind.into()));
    if let Some(name) = name {
        map.insert("name".into(), Value::String(name));
    }
    if let Some(count) = count {
        map.insert("count".into(), Value::from(count));
    }
    let msg = js_from_json(&Value::Object(map));
    let buf = bytes_to_buffer(bytes);
    set(&msg, "bytes", buf.as_ref());
    let transfer = Array::of1(buf.as_ref());
    post_transfer(&msg, &transfer);
}

fn note_mesh(bufs: &[(ArrayBuffer, u32)]) {
    PROBE.with(|p| {
        let (mut had, mut ok) = p.get();
        for (buf, len) in bufs {
            if *len > 0 {
                had = true;
                if buf.byte_length() != 0 {
                    ok = false;
                }
            }
        }
        p.set((had, ok));
    });
}

fn post(msg: &JsValue) {
    SCOPE.with(|s| {
        if let Some(scope) = s.borrow().as_ref() {
            let _ = scope.post_message(msg);
        }
    });
}

fn post_transfer(msg: &JsValue, transfer: &Array) {
    SCOPE.with(|s| {
        if let Some(scope) = s.borrow().as_ref() {
            let _ = scope.post_message_with_transfer(msg, transfer.as_ref());
        }
    });
}

fn bytes_to_buffer(data: &[u8]) -> ArrayBuffer {
    let u = Uint8Array::new_with_length(data.len() as u32);
    u.copy_from(data);
    u.buffer()
}

fn f32_bytes(v: &[f32]) -> Vec<u8> {
    let mut o = Vec::with_capacity(v.len() * 4);
    for x in v {
        o.extend_from_slice(&x.to_le_bytes());
    }
    o
}

fn u32_bytes(v: &[u32]) -> Vec<u8> {
    let mut o = Vec::with_capacity(v.len() * 4);
    for x in v {
        o.extend_from_slice(&x.to_le_bytes());
    }
    o
}

fn decode_command(data: &JsValue) -> Result<Command, String> {
    let t = get(data, "t").as_string().unwrap_or_default();
    match t.as_str() {
        "open" => Ok(Command::Open { name: req_string(data, "name")?, bytes: bytes_field(data, "bytes") }),
        "import" => Ok(Command::Import { name: req_string(data, "name")?, bytes: bytes_field(data, "bytes") }),
        "add" => Ok(Command::AddFeature {
            kind: req_string(data, "kind")?,
            params: json_field(data, "params"),
            index: opt_usize(&get(data, "index")),
            name: get(data, "name").as_string().filter(|n| !n.is_empty()),
        }),
        "edit" => Ok(Command::EditFeature {
            id: req_string(data, "id")?,
            params: json_field(data, "params"),
            rollback_after: get(data, "setRollback").as_bool().unwrap_or(false).then(|| opt_usize(&get(data, "rollbackAfter"))),
        }),
        "delete" => Ok(Command::DeleteFeature { id: req_string(data, "id")? }),
        "suppress" => Ok(Command::SetSuppressed {
            id: req_string(data, "id")?,
            suppressed: get(data, "suppressed").as_bool().unwrap_or(false),
        }),
        "move" => Ok(Command::MoveFeature {
            id: req_string(data, "id")?,
            index: opt_usize(&get(data, "index")).unwrap_or(0),
        }),
        "params" => Ok(Command::SetParams(param_list(&json_field(data, "list")))),
        "rollback" => Ok(Command::Rollback(opt_usize(&get(data, "index")))),
        "undo" => Ok(Command::Undo),
        "redo" => Ok(Command::Redo),
        "body" => Ok(Command::BodyProps {
            id: req_string(data, "id")?,
            name: opt_string(&get(data, "name")),
            color: opt_string(&get(data, "color")),
            hidden: opt_bool(&get(data, "hidden")),
        }),
        "preview" => Ok(Command::Preview {
            seq: get(data, "seq").as_f64().unwrap_or(0.0) as u64,
            kind: req_string(data, "kind")?,
            params: json_field(data, "params"),
            edit_id: opt_string(&get(data, "editId")),
        }),
        "clearPreview" => Ok(Command::ClearPreview),
        "sketchOp" => Ok(Command::SketchOp {
            seq: get(data, "seq").as_f64().unwrap_or(0.0) as u64,
            op: req_string(data, "op")?,
            data: json_field(data, "data"),
        }),
        "solve" => Ok(Command::Solve {
            seq: get(data, "seq").as_f64().unwrap_or(0.0) as u64,
            entities: decode_entities(&json_field(data, "entities"))?,
            constraints: decode_constraints(&json_field(data, "constraints"))?,
            locks: decode_locks(&json_field(data, "locks"))?,
            fixed: decode_entities(&json_field(data, "fixed"))?,
        }),
        "fins" => {
            let ask = match req_string(data, "ask")?.as_str() {
                "info" => FinsAsk::Info,
                "strength" => FinsAsk::Strength { body: req_string(data, "body")?, options: json_field(data, "options"), load: json_field(data, "load") },
                "suggest" => FinsAsk::Suggest {
                    body: req_string(data, "body")?,
                    options: json_field(data, "options"),
                    top: get(data, "top").as_f64().unwrap_or(3.0) as i64,
                },
                other => return Err(format!("The engine got a message it doesn't understand (fins {other}).")),
            };
            Ok(Command::Fins { seq: get(data, "seq").as_f64().unwrap_or(0.0) as u64, ask })
        }
        "save" => Ok(Command::Save),
        "autosave" => Ok(Command::Autosave { path: opt_string(&get(data, "path")), title: opt_string(&get(data, "title")) }),
        "export" => Ok(Command::Export {
            format: export_format(&req_string(data, "format")?)?,
            ids: opt_ids(&json_field(data, "ids")),
            date: req_string(data, "date")?,
        }),
        other => Err(format!("The engine got a message it doesn't understand ({other}).")),
    }
}

fn decode_entities(v: &Value) -> Result<Vec<stl_sketch::entities::Entity>, String> {
    match v {
        Value::Null => Ok(Vec::new()),
        Value::Array(a) => stl_sketch::entities::parse_entities(a),
        _ => Err("The engine got a message it doesn't understand (solve entities).".into()),
    }
}

fn decode_constraints(v: &Value) -> Result<Vec<stl_sketch::solver::Constraint>, String> {
    if v.is_null() {
        return Ok(Vec::new());
    }
    serde_json::from_value(v.clone()).map_err(|e| e.to_string())
}

fn decode_locks(v: &Value) -> Result<Vec<stl_sketch::solver::Lock>, String> {
    if v.is_null() {
        return Ok(Vec::new());
    }
    serde_json::from_value(v.clone()).map_err(|e| e.to_string())
}

fn export_format(s: &str) -> Result<ExportFormat, String> {
    ExportFormat::from_ext(s).ok_or_else(|| format!("Can't export .{s} files."))
}

fn param_list(v: &Value) -> Vec<Param> {
    v.as_array()
        .map(|a| {
            a.iter()
                .map(|p| Param {
                    name: p.get("name").and_then(Value::as_str).unwrap_or("").to_string(),
                    expr: p.get("expr").and_then(Value::as_str).unwrap_or("").to_string(),
                    note: p.get("note").and_then(Value::as_str).unwrap_or("").to_string(),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn opt_ids(v: &Value) -> Option<Vec<String>> {
    v.as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
}

fn encode_command(c: &Command) -> JsValue {
    let mut m = Map::new();
    let tag = match c {
        Command::Open { .. } => "open",
        Command::Import { .. } => "import",
        Command::AddFeature { .. } => "add",
        Command::EditFeature { .. } => "edit",
        Command::DeleteFeature { .. } => "delete",
        Command::SetSuppressed { .. } => "suppress",
        Command::MoveFeature { .. } => "move",
        Command::SetParams(_) => "params",
        Command::Rollback(_) => "rollback",
        Command::Undo => "undo",
        Command::Redo => "redo",
        Command::BodyProps { .. } => "body",
        Command::Save => "save",
        Command::Export { .. } => "export",
        Command::Cancel => "cancel",
        Command::Preview { .. } => "preview",
        Command::ClearPreview => "clearPreview",
        Command::Fins { .. } => "fins",
        Command::SketchOp { .. } => "sketchOp",
        Command::Solve { .. } => "solve",
        Command::Autosave { .. } => "autosave",
    };
    m.insert("t".into(), Value::String(tag.into()));
    if let Command::Open { name, .. } | Command::Import { name, .. } = c {
        m.insert("name".into(), Value::String(name.clone()));
    }
    match c {
        Command::Autosave { path, title } => {
            m.insert("path".into(), path.clone().map(Value::String).unwrap_or(Value::Null));
            m.insert("title".into(), title.clone().map(Value::String).unwrap_or(Value::Null));
        }
        Command::AddFeature { kind, params, index, name } => {
            m.insert("name".into(), name.clone().map(Value::String).unwrap_or(Value::Null));
            m.insert("kind".into(), Value::String(kind.clone()));
            m.insert("params".into(), params.clone());
            m.insert("index".into(), index.map(Value::from).unwrap_or(Value::Null));
        }
        Command::EditFeature { id, params, rollback_after } => {
            m.insert("id".into(), Value::String(id.clone()));
            m.insert("params".into(), params.clone());
            m.insert("setRollback".into(), Value::Bool(rollback_after.is_some()));
            m.insert("rollbackAfter".into(), rollback_after.flatten().map(Value::from).unwrap_or(Value::Null));
        }
        Command::Preview { seq, kind, params, edit_id } => {
            m.insert("seq".into(), Value::from(*seq));
            m.insert("kind".into(), Value::String(kind.clone()));
            m.insert("params".into(), params.clone());
            m.insert("editId".into(), edit_id.clone().map(Value::String).unwrap_or(Value::Null));
        }
        Command::Solve { seq, entities, constraints, locks, fixed } => {
            m.insert("seq".into(), Value::from(*seq));
            m.insert("entities".into(), serde_json::to_value(entities).unwrap_or_default());
            m.insert("constraints".into(), serde_json::to_value(constraints).unwrap_or_default());
            m.insert("locks".into(), serde_json::to_value(locks).unwrap_or_default());
            m.insert("fixed".into(), serde_json::to_value(fixed).unwrap_or_default());
        }
        Command::SketchOp { seq, op, data } => {
            m.insert("seq".into(), Value::from(*seq));
            m.insert("op".into(), Value::String(op.clone()));
            m.insert("data".into(), data.clone());
        }
        Command::Fins { seq, ask } => {
            m.insert("seq".into(), Value::from(*seq));
            m.insert("ask".into(), Value::String(ask.name().into()));
            match ask {
                FinsAsk::Info => {}
                FinsAsk::Strength { body, options, load } => {
                    m.insert("body".into(), Value::String(body.clone()));
                    m.insert("options".into(), options.clone());
                    m.insert("load".into(), load.clone());
                }
                FinsAsk::Suggest { body, options, top } => {
                    m.insert("body".into(), Value::String(body.clone()));
                    m.insert("options".into(), options.clone());
                    m.insert("top".into(), Value::from(*top));
                }
            }
        }
        Command::DeleteFeature { id } => {
            m.insert("id".into(), Value::String(id.clone()));
        }
        Command::SetSuppressed { id, suppressed } => {
            m.insert("id".into(), Value::String(id.clone()));
            m.insert("suppressed".into(), Value::Bool(*suppressed));
        }
        Command::MoveFeature { id, index } => {
            m.insert("id".into(), Value::String(id.clone()));
            m.insert("index".into(), Value::from(*index));
        }
        Command::SetParams(list) => {
            m.insert("list".into(), Value::Array(list.iter().map(param_json).collect()));
        }
        Command::Rollback(i) => {
            m.insert("index".into(), i.map(Value::from).unwrap_or(Value::Null));
        }
        Command::BodyProps { id, name, color, hidden } => {
            m.insert("id".into(), Value::String(id.clone()));
            m.insert("name".into(), name.clone().map(Value::String).unwrap_or(Value::Null));
            m.insert("color".into(), color.clone().map(Value::String).unwrap_or(Value::Null));
            m.insert("hidden".into(), hidden.map(Value::Bool).unwrap_or(Value::Null));
        }
        Command::Export { format, ids, date } => {
            m.insert("format".into(), Value::String(format.ext().into()));
            m.insert(
                "ids".into(),
                ids.clone()
                    .map(|v| Value::Array(v.into_iter().map(Value::String).collect()))
                    .unwrap_or(Value::Null),
            );
            m.insert("date".into(), Value::String(date.clone()));
        }
        _ => {}
    }
    let msg = js_from_json(&Value::Object(m));
    if let Command::Open { bytes, .. } | Command::Import { bytes, .. } = c {
        set(&msg, "bytes", &Uint8Array::from(bytes.as_slice()));
    }
    msg
}

fn param_json(p: &Param) -> Value {
    Value::Object(Map::from_iter([
        ("name".into(), Value::String(p.name.clone())),
        ("expr".into(), Value::String(p.expr.clone())),
        ("note".into(), Value::String(p.note.clone())),
    ]))
}

fn level_str(level: Level) -> &'static str {
    match level {
        Level::Info => "info",
        Level::Success => "success",
        Level::Warning => "warning",
        Level::Error => "error",
    }
}

fn level_of(s: &str) -> Level {
    match s {
        "success" => Level::Success,
        "warning" => Level::Warning,
        "error" => Level::Error,
        _ => Level::Info,
    }
}

struct Inner {
    js: Worker,
    updates: Vec<Update>,
    queue: Vec<Queued>,
    ready: bool,
    lost: bool,
    generation: u32,
    /// Last project that finished opening, or the last `Saved` bytes.
    /// An Open that fails does not replace this.
    restart_bytes: Option<Vec<u8>>,
    /// Commands posted to the worker, in the order it will run them.
    sent: VecDeque<Pending>,
    /// The command in flight sent [`Update::Error`].
    command_failed: bool,
    mesh_detached: Option<bool>,
    fault: String,
    /// Asks the page to draw again when an update arrives (the web UI does not poll).
    wake: Option<Box<dyn Fn()>>,
    onmessage: Option<Closure<dyn FnMut(MessageEvent)>>,
    onerror: Option<Closure<dyn FnMut(ErrorEvent)>>,
}

enum Queued {
    Cmd(Command),
    Restart,
}

/// What a posted command should do to [`Inner::restart_bytes`] if it succeeds.
enum Pending {
    /// `.stlx` bytes to restore after the worker dies.
    Project(Vec<u8>),
    /// A mesh open starts a new unsaved document.
    Clear,
    Other,
}

/// `EngineLink` over the Web Worker.
pub struct WebLink {
    inner: Rc<RefCell<Inner>>,
}

impl WebLink {
    pub fn new() -> Result<WebLink, String> {
        let _ = std::hint::black_box(KEEP_ENGINE);
        let _ = std::hint::black_box(KEEP_HANDLE);
        let inner = Rc::new(RefCell::new(Inner {
            js: spawn_js_worker()?,
            updates: Vec::new(),
            queue: Vec::new(),
            ready: false,
            lost: false,
            generation: 0,
            restart_bytes: None,
            sent: VecDeque::new(),
            command_failed: false,
            mesh_detached: None,
            fault: String::new(),
            wake: None,
            onmessage: None,
            onerror: None,
        }));
        attach(&inner);
        // This tab owns an autosave session while it is open; a tab that is hidden or closing
        // saves what it has (1.x does the same on `visibilitychange`).
        crate::web::store::hold_session_lock();
        watch_visibility(&inner);
        Ok(WebLink { inner })
    }

    /// `Some(true)` once a mesh `ArrayBuffer` was detached on the worker after it was sent.
    pub fn mesh_was_transferred(&self) -> Option<bool> {
        self.inner.borrow().mesh_detached
    }

    pub fn fault(&self) -> String {
        self.inner.borrow().fault.clone()
    }

    /// `f` runs on the page thread when an update is waiting (`Context::request_repaint`).
    pub fn set_waker(&self, f: impl Fn() + 'static) {
        self.inner.borrow_mut().wake = Some(Box::new(f));
    }
}

impl Drop for WebLink {
    fn drop(&mut self) {
        self.inner.borrow().js.terminate();
    }
}

/// The page is being hidden: write the autosave copy now.
fn watch_visibility(inner: &Rc<RefCell<Inner>>) {
    let Some(doc) = web_sys::window().and_then(|w| w.document()) else { return };
    let weak = Rc::downgrade(inner);
    let handler = Closure::wrap(Box::new(move || {
        let Some(inner) = weak.upgrade() else { return };
        if web_sys::window().and_then(|w| w.document()).is_some_and(|d| d.hidden()) {
            let ready = inner.borrow().ready && !inner.borrow().lost;
            if ready {
                post_cmd(&inner, &Command::Autosave { path: None, title: None });
            }
        }
    }) as Box<dyn FnMut()>);
    let _ = doc.add_event_listener_with_callback("visibilitychange", handler.as_ref().unchecked_ref());
    handler.forget();
}

fn attach(inner: &Rc<RefCell<Inner>>) {
    let generation = inner.borrow().generation;
    let js = inner.borrow().js.clone();
    let msg_inner = inner.clone();
    let onmessage = Closure::wrap(Box::new(move |ev: MessageEvent| {
        if msg_inner.borrow().generation != generation {
            return;
        }
        on_message(&msg_inner, ev.data());
    }) as Box<dyn FnMut(MessageEvent)>);
    js.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
    let err_inner = inner.clone();
    let onerror = Closure::wrap(Box::new(move |ev: ErrorEvent| {
        if err_inner.borrow().generation != generation {
            return;
        }
        let text = ev.message();
        mark_lost(&err_inner, &text);
    }) as Box<dyn FnMut(ErrorEvent)>);
    js.set_onerror(Some(onerror.as_ref().unchecked_ref()));
    let mut g = inner.borrow_mut();
    g.onmessage = Some(onmessage);
    g.onerror = Some(onerror);
}

fn on_message(inner: &Rc<RefCell<Inner>>, data: JsValue) {
    let t = get(&data, "t").as_string().unwrap_or_default();
    match t.as_str() {
        "ready" => {
            inner.borrow_mut().ready = true;
            flush(inner);
        }
        "autosaveCopy" => keep_autosave_copy(inner, &data),
        "autosaveClear" => {
            wasm_bindgen_futures::spawn_local(async {
                let _ = crate::web::store::clear_copy().await;
            });
        }
        "meshDetached" => {
            inner.borrow_mut().mesh_detached = Some(get(&data, "ok").as_bool().unwrap_or(false));
        }
        "engineLost" => {
            {
                let mut g = inner.borrow_mut();
                g.sent.clear();
                g.command_failed = true;
            }
            let text = opt_string(&get(&data, "text")).unwrap_or_default();
            mark_lost(inner, &text);
        }
        _ => {
            if let Some(update) = decode_update(&data) {
                note_update(inner, &update);
                inner.borrow_mut().updates.push(update);
                poke(inner);
            }
        }
    }
}

/// The worker made an autosave copy: keep it in this browser, then tell the app how it went.
fn keep_autosave_copy(inner: &Rc<RefCell<Inner>>, data: &JsValue) {
    let title = opt_string(&get(data, "title")).unwrap_or_else(|| "Untitled".into());
    let steps = num_usize(&get(data, "steps"));
    let bytes = bytes_field(data, "bytes");
    let saved = js_sys::Date::now() / 1000.0;
    let inner = inner.clone();
    wasm_bindgen_futures::spawn_local(async move {
        let update = match crate::web::store::put_copy(&title, steps, saved, &bytes).await {
            Ok(()) => Update::Autosaved { ok: true, error: None },
            Err(e) => Update::Autosaved { ok: false, error: Some(format!("Couldn't keep an autosave copy in this browser: {e}")) },
        };
        inner.borrow_mut().updates.push(update);
        poke(&inner);
    });
}

fn poke(inner: &Rc<RefCell<Inner>>) {
    let wake = inner.borrow_mut().wake.take();
    if let Some(f) = wake.as_ref() {
        f();
    }
    inner.borrow_mut().wake = wake;
}

fn mark_lost(inner: &Rc<RefCell<Inner>>, text: &str) {
    let mut g = inner.borrow_mut();
    if g.lost {
        if g.fault.is_empty() {
            g.fault = text.to_string();
        }
        return;
    }
    g.lost = true;
    g.fault = text.to_string();
    g.updates.push(Update::EngineLost);
    drop(g);
    poke(inner);
}

fn flush(inner: &Rc<RefCell<Inner>>) {
    let queued = std::mem::take(&mut inner.borrow_mut().queue);
    for item in queued {
        match item {
            Queued::Cmd(c) => post_cmd(inner, &c),
            Queued::Restart => post_restart(inner),
        }
    }
}

fn post_cmd(inner: &Rc<RefCell<Inner>>, c: &Command) {
    let pending = match c {
        Command::Open { name, bytes } if name.to_lowercase().ends_with(".stlx") => Pending::Project(bytes.clone()),
        Command::Open { .. } => Pending::Clear,
        // Preview, fins, sketch ops and solves send no Busy: the worker does not count them.
        // An autosave is quiet too: no Busy comes back for it.
        Command::Preview { .. } | Command::ClearPreview | Command::Fins { .. } | Command::SketchOp { .. } | Command::Solve { .. } | Command::Autosave { .. } => {
            return post_js(inner, &encode_command(c))
        }
        _ => Pending::Other,
    };
    inner.borrow_mut().sent.push_back(pending);
    post_js(inner, &encode_command(c));
}

fn post_restart(inner: &Rc<RefCell<Inner>>) {
    inner.borrow_mut().sent.push_back(Pending::Other);
    post_js(inner, &js_from_json(&Value::Object(Map::from_iter([("t".into(), Value::String("restart".into()))]))));
}

/// `Saved` is already a finished save. An Open counts only when its command ends without an error.
fn note_update(inner: &Rc<RefCell<Inner>>, update: &Update) {
    match update {
        Update::Saved { bytes } => inner.borrow_mut().restart_bytes = Some(bytes.clone()),
        Update::Busy(true) => inner.borrow_mut().command_failed = false,
        Update::Error(_) => inner.borrow_mut().command_failed = true,
        Update::EngineLost => {
            let mut g = inner.borrow_mut();
            g.sent.clear();
            g.command_failed = true;
        }
        Update::Busy(false) => finish_command(inner),
        _ => {}
    }
}

fn finish_command(inner: &Rc<RefCell<Inner>>) {
    let (pending, failed) = {
        let mut g = inner.borrow_mut();
        (g.sent.pop_front(), g.command_failed)
    };
    if failed {
        return;
    }
    match pending {
        Some(Pending::Project(bytes)) => inner.borrow_mut().restart_bytes = Some(bytes),
        Some(Pending::Clear) => inner.borrow_mut().restart_bytes = None,
        _ => {}
    }
}

fn post_js(inner: &Rc<RefCell<Inner>>, msg: &JsValue) {
    let js = inner.borrow().js.clone();
    let _ = js.post_message(msg);
}

fn respawn(inner: &Rc<RefCell<Inner>>) -> Result<(), String> {
    let bytes = inner.borrow().restart_bytes.clone();
    {
        let mut g = inner.borrow_mut();
        g.generation = g.generation.wrapping_add(1);
        g.js.terminate();
        g.js = spawn_js_worker()?;
        g.ready = false;
        g.lost = false;
        g.mesh_detached = None;
        g.fault.clear();
        g.queue.clear();
        g.sent.clear();
        g.command_failed = false;
        g.onmessage = None;
        g.onerror = None;
        // `wake` stays: the new worker's updates still repaint the same window.
    }
    attach(inner);
    if let Some(bytes) = bytes {
        let mut g = inner.borrow_mut();
        g.queue.push(Queued::Cmd(Command::Open { name: "saved.stlx".into(), bytes }));
    }
    Ok(())
}

fn spawn_js_worker() -> Result<Worker, String> {
    let (url, revoke) = worker_script_url()?;
    let opts = WorkerOptions::new();
    opts.set_type(WorkerType::Module);
    let worker = Worker::new_with_options(&url, &opts).map_err(|e| {
        format!("Could not start the engine worker: {}", e.as_string().unwrap_or_else(|| "unknown error".into()))
    })?;
    // The worker fetches this blob URL asynchronously. Revoking it here would
    // pull the script out from under that fetch. One blob URL per link is fine.
    let _ = revoke;
    Ok(worker)
}

/// `(url, revoke blob url)`. The test runner serves its own glue; the app page uses `worker.js`.
fn worker_script_url() -> Result<(String, bool), String> {
    if in_wbg_test() {
        let loc = web_sys::window().and_then(|w| w.location().origin().ok()).unwrap_or_default();
        let glue = format!("{loc}/wasm-bindgen-test.js");
        let wasm = format!("{loc}/wasm-bindgen-test_bg.wasm");
        let no_jspi = get(&js_sys::global(), "__STL_NO_JSPI__").as_bool().unwrap_or(false);
        let force = if no_jspi { "self.__STL_NO_JSPI__ = true;\n" } else { "" };
        let src = format!(
            "{force}self.__STL_GLUE__ = {};\nself.__STL_WASM__ = {};\n{WORKER_JS}",
            serde_json::to_string(&glue).unwrap_or_else(|_| "\"\"".into()),
            serde_json::to_string(&wasm).unwrap_or_else(|_| "\"\"".into()),
        );
        let parts = Array::of1(&JsValue::from_str(&src));
        let bag = web_sys::BlobPropertyBag::new();
        bag.set_type("text/javascript");
        let blob = web_sys::Blob::new_with_str_sequence_and_options(&parts, &bag)
        .map_err(|_| "Could not build the engine worker script.".to_string())?;
        let url = Url::create_object_url_with_blob(&blob).map_err(|_| "Could not build the engine worker script.".to_string())?;
        return Ok((url, true));
    }
    let href = web_sys::window()
        .and_then(|w| w.location().href().ok())
        .ok_or_else(|| "This page has no location.".to_string())?;
    let url = Url::new_with_base("worker.js", &href).map_err(|_| "Could not find worker.js.".to_string())?;
    Ok((url.href(), false))
}

fn in_wbg_test() -> bool {
    js_sys::eval("!!document.querySelector('script[src=\"run.js\"]')")
        .ok()
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

impl EngineLink for WebLink {
    fn send(&self, c: Command) {
        if matches!(c, Command::Cancel) {
            post_js(&self.inner, &js_from_json(&Value::Object(Map::from_iter([("t".into(), Value::String("cancel".into()))]))));
            return;
        }
        if matches!(c, Command::ClearPreview) {
            return; // a late preview is dropped by the app; nothing queued can be skipped here
        }
        let ready = self.inner.borrow().ready && !self.inner.borrow().lost;
        if ready {
            post_cmd(&self.inner, &c);
        } else if !self.inner.borrow().lost {
            self.inner.borrow_mut().queue.push(Queued::Cmd(c));
        }
    }

    fn poll(&self) -> Vec<Update> {
        std::mem::take(&mut self.inner.borrow_mut().updates)
    }

    fn restart(&self) {
        if self.inner.borrow().lost {
            let _ = respawn(&self.inner);
            return;
        }
        if self.inner.borrow().ready {
            post_restart(&self.inner);
        } else {
            self.inner.borrow_mut().queue.push(Queued::Restart);
        }
    }
}

fn decode_update(data: &JsValue) -> Option<Update> {
    let t = get(data, "t").as_string()?;
    Some(match t.as_str() {
        "busy" => Update::Busy(get(data, "busy").as_bool().unwrap_or(false)),
        "progress" => Update::Progress {
            done: num_usize(&get(data, "done")),
            total: num_usize(&get(data, "total")),
            label: opt_string(&get(data, "label")).unwrap_or_default(),
        },
        "message" => Update::Message {
            level: level_of(&opt_string(&get(data, "level")).unwrap_or_default()),
            text: opt_string(&get(data, "text")).unwrap_or_default(),
        },
        "error" => Update::Error(opt_string(&get(data, "text")).unwrap_or_default()),
        "engineLost" => Update::EngineLost,
        "timeline" => Update::Timeline(decode_timeline(data)),
        "params" => Update::Params(decode_params(data)),
        "planes" => Update::Planes(decode_planes(data)),
        "sketches" => Update::Sketches(decode_sketches(data)),
        "bodies" => Update::Bodies(decode_bodies(data)),
        "preview" => Update::Preview {
            seq: get(data, "seq").as_f64().unwrap_or(0.0) as u64,
            pack: decode_pack(data),
            msg: opt_string(&get(data, "msg")),
            error: opt_string(&get(data, "error")),
            warn: get(data, "warn").as_bool().unwrap_or(false),
            result: json_field(data, "result"),
        },
        "sketchOp" => {
            let data_v = json_field(data, "data");
            Update::SketchOp {
                seq: get(data, "seq").as_f64().unwrap_or(0.0) as u64,
                result: if get(data, "ok").as_bool().unwrap_or(false) {
                    Ok(data_v)
                } else {
                    Err(data_v.as_str().unwrap_or("The sketch operation failed.").to_owned())
                },
            }
        }
        "solved" => Update::Solved {
            seq: get(data, "seq").as_f64().unwrap_or(0.0) as u64,
            entities: decode_entities(&json_field(data, "entities")).unwrap_or_default(),
            ok: get(data, "ok").as_bool().unwrap_or(false),
            error: get(data, "error").as_f64().unwrap_or(0.0),
        },
        "fins" => {
            let data_v = json_field(data, "data");
            Update::Fins {
                seq: get(data, "seq").as_f64().unwrap_or(0.0) as u64,
                ask: opt_string(&get(data, "ask")).unwrap_or_default(),
                result: if get(data, "ok").as_bool().unwrap_or(false) { Ok(data_v) } else { Err(data_v.as_str().unwrap_or("Support Fins failed.").to_owned()) },
            }
        }
        "saved" => Update::Saved { bytes: bytes_field(data, "bytes") },
        "exported" => Update::Exported {
            name: opt_string(&get(data, "name")).unwrap_or_else(|| "model".into()),
            bytes: bytes_field(data, "bytes"),
            count: num_usize(&get(data, "count")),
        },
        _ => return None,
    })
}

fn decode_timeline(data: &JsValue) -> TimelineView {
    let items = array_of(&get(data, "items"))
        .iter()
        .map(|item| TimelineItem {
            id: opt_string(&get(&item, "id")).unwrap_or_default(),
            kind: opt_string(&get(&item, "kind")).unwrap_or_default(),
            label: opt_string(&get(&item, "label")).unwrap_or_default(),
            error: opt_string(&get(&item, "error")),
            suppressed: get(&item, "suppressed").as_bool().unwrap_or(false),
            warning: opt_string(&get(&item, "warning")),
            active: get(&item, "active").as_bool().unwrap_or(true),
            params: json_field(&item, "params"),
        })
        .collect();
    TimelineView {
        items,
        marker: opt_usize(&get(data, "marker")),
        can_undo: get(data, "canUndo").as_bool().unwrap_or(false),
        can_redo: get(data, "canRedo").as_bool().unwrap_or(false),
    }
}

fn decode_params(data: &JsValue) -> Vec<ParamView> {
    array_of(&get(data, "params"))
        .iter()
        .map(|p| ParamView {
            name: opt_string(&get(&p, "name")).unwrap_or_default(),
            expr: opt_string(&get(&p, "expr")).unwrap_or_default(),
            note: opt_string(&get(&p, "note")).unwrap_or_default(),
            value: get(&p, "value").as_f64(),
            error: opt_string(&get(&p, "error")),
            used: get(&p, "used").as_bool().unwrap_or(false),
        })
        .collect()
}

fn decode_planes(data: &JsValue) -> Vec<PlaneView> {
    array_of(&get(data, "planes"))
        .iter()
        .map(|p| PlaneView {
            id: opt_string(&get(&p, "id")).unwrap_or_default(),
            name: opt_string(&get(&p, "name")).unwrap_or_default(),
            origin: arr3_of(&get(&p, "origin")),
            x: arr3_of(&get(&p, "x")),
            y: arr3_of(&get(&p, "y")),
            normal: arr3_of(&get(&p, "normal")),
            visible: get(&p, "visible").as_bool().unwrap_or(false),
        })
        .collect()
}

fn decode_outline(o: &JsValue) -> SketchOutline {
    let pts = array_of(&get(o, "pts"))
        .iter()
        .map(|p| {
            let a = array_of(&p);
            [num_f64(&a.get(0)), num_f64(&a.get(1))]
        })
        .collect();
    SketchOutline {
        pts,
        closed: get(o, "closed").as_bool().unwrap_or(false),
        construction: get(o, "construction").as_bool().unwrap_or(false),
    }
}

fn decode_frame(v: &JsValue) -> [[f64; 3]; 4] {
    let a = array_of(v);
    [arr3_of(&a.get(0)), arr3_of(&a.get(1)), arr3_of(&a.get(2)), arr3_of(&a.get(3))]
}

fn decode_sketches(data: &JsValue) -> Vec<SketchView> {
    array_of(&get(data, "sketches"))
        .iter()
        .map(|s| {
            let outlines: Arc<[SketchOutline]> = array_of(&get(&s, "outlines")).iter().map(|o| decode_outline(&o)).collect::<Vec<_>>().into();
            SketchView {
                id: opt_string(&get(&s, "id")).unwrap_or_default(),
                name: opt_string(&get(&s, "name")).unwrap_or_default(),
                plane: opt_string(&get(&s, "plane")).unwrap_or_default(),
                hidden: get(&s, "hidden").as_bool().unwrap_or(false),
                frame: decode_frame(&get(&s, "frame")),
                outlines,
            }
        })
        .collect()
}

fn js_cancelled() -> bool {
    get(&js_sys::global(), "__STL_CANCEL__").as_bool().unwrap_or(false)
}

/// Set by `worker.js` after it feature-detects `WebAssembly.Suspending`.
fn js_jspi() -> bool {
    get(&js_sys::global(), "__STL_JSPI__").as_bool().unwrap_or(false)
}

fn decode_bodies(data: &JsValue) -> Vec<BodyView> {
    array_of(&get(data, "bodies"))
        .iter()
        .map(|b| BodyView {
            id: opt_string(&get(&b, "id")).unwrap_or_default(),
            name: opt_string(&get(&b, "name")).unwrap_or_default(),
            color: opt_string(&get(&b, "color")),
            hidden: get(&b, "hidden").as_bool().unwrap_or(false),
            solid: get(&b, "solid").as_bool().unwrap_or(true),
            revision: num_usize(&get(&b, "revision")) as u64,
            mesh: Arc::new(MeshArrays {
                positions: f32s(&bytes_of(&get(&b, "positions"))),
                triangles: u32s(&bytes_of(&get(&b, "triangles"))),
            }),
        })
        .collect()
}

fn strings_of(v: &JsValue) -> Vec<String> {
    array_of(v).iter().filter_map(|x| x.as_string()).collect()
}

fn decode_pack(data: &JsValue) -> super::PreviewPack {
    let meshes = array_of(&get(data, "meshes"))
        .iter()
        .map(|m| crate::viewport::PreviewMesh {
            id: opt_string(&get(&m, "id")).unwrap_or_default(),
            mesh: Arc::new(MeshArrays { positions: f32s(&bytes_of(&get(&m, "positions"))), triangles: u32s(&bytes_of(&get(&m, "triangles"))) }),
        })
        .collect();
    let colors = match json_field(data, "colors") {
        Value::Object(o) => o.into_iter().filter_map(|(k, v)| v.as_str().map(|s| (k, s.to_string()))).collect(),
        _ => Default::default(),
    };
    super::PreviewPack {
        meshes,
        removed: strings_of(&get(data, "removed")),
        hide: strings_of(&get(data, "hide")),
        colors,
        ghosts: strings_of(&get(data, "ghosts")),
    }
}

fn arr3_of(v: &JsValue) -> [f64; 3] {
    let a = array_of(v);
    [num_f64(&a.get(0)), num_f64(&a.get(1)), num_f64(&a.get(2))]
}

fn num_f64(v: &JsValue) -> f64 {
    v.as_f64().unwrap_or(0.0)
}

fn f32s(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()
}

fn u32s(b: &[u8]) -> Vec<u32> {
    b.chunks_exact(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()
}

fn bytes_of(v: &JsValue) -> Vec<u8> {
    if v.is_null() || v.is_undefined() {
        return Vec::new();
    }
    let u = Uint8Array::new(v);
    let mut out = vec![0u8; u.length() as usize];
    u.copy_to(&mut out);
    out
}

fn bytes_field(data: &JsValue, key: &str) -> Vec<u8> {
    bytes_of(&get(data, key))
}

fn array_of(v: &JsValue) -> Array {
    if Array::is_array(v) { Array::from(v) } else { Array::new() }
}

fn get(obj: &JsValue, key: &str) -> JsValue {
    js_sys::Reflect::get(obj, &JsValue::from_str(key)).unwrap_or(JsValue::UNDEFINED)
}

fn set(obj: &JsValue, key: &str, val: &JsValue) {
    let _ = js_sys::Reflect::set(obj, &JsValue::from_str(key), val);
}

fn js_from_json(v: &Value) -> JsValue {
    let s = serde_json::to_string(v).unwrap_or_else(|_| "null".into());
    js_sys::JSON::parse(&s).unwrap_or(JsValue::NULL)
}

fn json_field(data: &JsValue, key: &str) -> Value {
    let v = get(data, key);
    if v.is_undefined() || v.is_null() {
        return Value::Null;
    }
    let Some(s) = js_sys::JSON::stringify(&v).ok().and_then(|j| j.as_string()) else {
        return Value::Null;
    };
    serde_json::from_str(&s).unwrap_or(Value::Null)
}

fn req_string(data: &JsValue, key: &str) -> Result<String, String> {
    opt_string(&get(data, key)).ok_or_else(|| format!("The engine got a message with no {key}."))
}

fn opt_string(v: &JsValue) -> Option<String> {
    if v.is_null() || v.is_undefined() { None } else { v.as_string() }
}

fn opt_bool(v: &JsValue) -> Option<bool> {
    if v.is_null() || v.is_undefined() { None } else { v.as_bool() }
}

fn opt_usize(v: &JsValue) -> Option<usize> {
    v.as_f64().map(|n| n as usize)
}

fn num_usize(v: &JsValue) -> usize {
    v.as_f64().unwrap_or(0.0) as usize
}
