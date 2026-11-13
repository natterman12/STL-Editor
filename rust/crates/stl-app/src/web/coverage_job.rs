//! Desktop page: [`CoverageJob`] posts work to `coverage_worker.js` (wasm).

#![cfg(target_arch = "wasm32")]

use crate::supports::coverage::{publish_result, CoverageInput, CoverageJobResult, CoverageOutput, JobSlot};
use js_sys::{Array, Float32Array, Object, Reflect, Uint32Array};
use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{MessageEvent, Url, Worker, WorkerOptions, WorkerType};

thread_local! {
    static WORKER: RefCell<Option<Worker>> = const { RefCell::new(None) };
}

/// Wasm half of [`crate::supports::coverage::CoverageJob`].
pub struct WasmCoverageSide;

impl WasmCoverageSide {
    pub fn new() -> Self {
        Self
    }

    pub fn stop_worker(&self) {
        WORKER.with(|w| {
            if let Some(worker) = w.borrow_mut().take() {
                let _ = worker.terminate();
            }
        });
    }

    pub fn start(
        &self,
        generation: Arc<AtomicU64>,
        slot: Arc<JobSlot>,
        seq: u64,
        input: CoverageInput,
    ) {
        let msg = pack_message(seq, &input);
        let post = WORKER.with(|cell| {
            if cell.borrow().is_none() {
                match spawn_worker(generation.clone(), slot.clone()) {
                    Ok(w) => *cell.borrow_mut() = Some(w),
                    Err(e) => return WorkerPost::Spawn(e),
                }
            }
            let worker = cell.borrow();
            let Some(w) = worker.as_ref() else {
                return WorkerPost::Failed;
            };
            if w.post_message(&msg).is_ok() {
                WorkerPost::Ok
            } else {
                WorkerPost::Failed
            }
        });
        match post {
            WorkerPost::Ok => {}
            WorkerPost::Spawn(e) => publish_result(&slot, &generation, seq, e),
            WorkerPost::Failed => publish_result(
                &slot,
                &generation,
                seq,
                CoverageJobResult::Error("Could not post to the coverage worker.".into()),
            ),
        }
    }
}

enum WorkerPost {
    Ok,
    Spawn(CoverageJobResult),
    Failed,
}

fn spawn_worker(generation: Arc<AtomicU64>, slot: Arc<JobSlot>) -> Result<Worker, CoverageJobResult> {
    let href = web_sys::window()
        .ok_or(CoverageJobResult::Error(
            "Could not start the coverage worker.".into(),
        ))?
        .location()
        .href()
        .map_err(|_| CoverageJobResult::Error("Could not start the coverage worker.".into()))?;
    let url = Url::new_with_base("coverage_worker.js", &href)
        .map_err(|_| CoverageJobResult::Error("Could not start the coverage worker.".into()))?;
    let opts = WorkerOptions::new();
    opts.set_type(WorkerType::Module);
    let worker = Worker::new_with_options(&url.href(), &opts).map_err(|_| {
        CoverageJobResult::Error("Could not start the coverage worker.".into())
    })?;
    let slot_cb = slot.clone();
    let gen_cb = generation.clone();
    let closure = Closure::<dyn Fn(MessageEvent)>::new(move |ev: MessageEvent| {
        let data = ev.data();
        let Ok(seq_js) = Reflect::get(&data, &JsValue::from_str("seq")) else {
            return;
        };
        let msg_seq = seq_js.as_f64().unwrap_or(-1.0) as u64;
        if msg_seq != gen_cb.load(Ordering::SeqCst) {
            return;
        }
        if Reflect::has(&data, &JsValue::from_str("error")).unwrap_or(false) {
            let err = Reflect::get(&data, &JsValue::from_str("error"))
                .ok()
                .and_then(|v| v.as_string())
                .unwrap_or_else(|| "coverage error".into());
            publish_result(&slot_cb, &gen_cb, msg_seq, CoverageJobResult::Error(err));
            return;
        }
        let Ok(out_js) = Reflect::get(&data, &JsValue::from_str("out")) else {
            return;
        };
        if let Some(out) = parse_output(&out_js) {
            publish_result(&slot_cb, &gen_cb, msg_seq, CoverageJobResult::Done(out));
        }
    });
    worker.set_onmessage(Some(closure.as_ref().unchecked_ref()));
    closure.forget();
    Ok(worker)
}

fn pack_message(seq: u64, input: &CoverageInput) -> JsValue {
    let root = Object::new();
    let _ = Reflect::set(&root, &JsValue::from_str("seq"), &JsValue::from_f64(seq as f64));
    let args = Object::new();
    let pos = Float32Array::new_with_length(input.positions.len() as u32);
    for (i, &v) in input.positions.iter().enumerate() {
        pos.set_index(i as u32, v);
    }
    let idx = Uint32Array::new_with_length(input.triangles.len() as u32);
    for (i, &v) in input.triangles.iter().enumerate() {
        idx.set_index(i as u32, v);
    }
    let over = Array::new();
    for &f in &input.over_faces {
        over.push(&JsValue::from_f64(f as f64));
    }
    let fins = Array::new();
    for fp in &input.fins {
        let a = Float32Array::new_with_length(fp.len() as u32);
        for (i, &v) in fp.iter().enumerate() {
            a.set_index(i as u32, v);
        }
        fins.push(&a);
    }
    let _ = Reflect::set(&args, &JsValue::from_str("pos"), &pos);
    let _ = Reflect::set(&args, &JsValue::from_str("idx"), &idx);
    let _ = Reflect::set(&args, &JsValue::from_str("over"), &over);
    let _ = Reflect::set(&args, &JsValue::from_str("fins"), &fins);
    let _ = Reflect::set(&args, &JsValue::from_str("gap"), &JsValue::from_f64(input.gap));
    let _ = Reflect::set(&args, &JsValue::from_str("reach"), &JsValue::from_f64(input.reach));
    let _ = Reflect::set(&root, &JsValue::from_str("args"), &args);
    root.into()
}

fn parse_output(v: &JsValue) -> Option<CoverageOutput> {
    let o = v.dyn_ref::<Object>()?;
    let total = Reflect::get(o, &JsValue::from_str("total"))
        .ok()?
        .as_f64()?;
    let held = Reflect::get(o, &JsValue::from_str("held"))
        .ok()?
        .as_f64()?;
    let mut out = CoverageOutput {
        total,
        held,
        ..Default::default()
    };
    if let Ok(tris) = Reflect::get(o, &JsValue::from_str("tris")) {
        if let Some(a) = tris.dyn_ref::<Float32Array>() {
            out.tris = (0..a.length()).map(|i| a.get_index(i)).collect();
        }
    }
    if let Ok(dist) = Reflect::get(o, &JsValue::from_str("dist")) {
        if let Some(a) = dist.dyn_ref::<Float32Array>() {
            out.dist = (0..a.length()).map(|i| a.get_index(i)).collect();
        }
    }
    let spots_js = Reflect::get(o, &JsValue::from_str("spots")).ok()?;
    let spots = spots_js.dyn_ref::<Array>()?;
    for i in 0..spots.length() {
        let s = spots.get(i);
        let so = s.dyn_ref::<Object>()?;
        let area = Reflect::get(so, &JsValue::from_str("area"))
            .ok()?
            .as_f64()?;
        let span = Reflect::get(so, &JsValue::from_str("span"))
            .ok()?
            .as_f64()?;
        let far = Reflect::get(so, &JsValue::from_str("far")).ok().and_then(|v| {
            if v.is_null() || v.is_undefined() {
                None
            } else {
                v.as_f64()
            }
        });
        let at_js = Reflect::get(so, &JsValue::from_str("at")).ok()?;
        let at_arr = at_js.dyn_ref::<Array>()?;
        let mut at = [0.0; 3];
        for k in 0..3 {
            at[k] = at_arr.get(k as u32).as_f64()?;
        }
        out.spots.push(crate::supports::coverage::BareSpot {
            area,
            span,
            far,
            at,
            ends: parse_ends(so),
        });
    }
    Some(out)
}

/// Spot `ends`: two points, or null when 1.x has no wall across the spot.
fn parse_ends(so: &Object) -> Option<[[f64; 3]; 2]> {
    let v = Reflect::get(so, &JsValue::from_str("ends")).ok()?;
    if v.is_null() || v.is_undefined() {
        return None;
    }
    let arr = v.dyn_ref::<Array>()?;
    if arr.length() < 2 {
        return None;
    }
    let mut pts = [[0.0; 3]; 2];
    for i in 0..2u32 {
        let p = arr.get(i);
        let pa = p.dyn_ref::<Array>()?;
        if pa.length() < 3 {
            return None;
        }
        for k in 0..3u32 {
            pts[i as usize][k as usize] = pa.get(k).as_f64()?;
        }
    }
    Some(pts)
}
