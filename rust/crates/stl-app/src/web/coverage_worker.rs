//! Coverage Web Worker glue (wasm32): same contract as `web/js/sfcoverage.worker.js`.

#[cfg(target_arch = "wasm32")]
use js_sys::{Array, Float32Array, Object, Reflect, Uint32Array};
#[cfg(target_arch = "wasm32")]
use crate::supports::coverage::{compute, CoverageInput, CoverageOutput};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;

/// Worker entry (`onmessage` in `coverage_worker.js`).
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn coverage_worker_entry() {
    let scope: web_sys::DedicatedWorkerGlobalScope = js_sys::global().unchecked_into();
    let scope_post = scope.clone();
    let closure = Closure::<dyn Fn(web_sys::MessageEvent)>::new(move |ev: web_sys::MessageEvent| {
        let data = ev.data();
        let Some(obj) = data.dyn_ref::<Object>() else {
            return;
        };
        let seq = Reflect::get(obj, &JsValue::from_str("seq"))
            .ok()
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0) as u64;
        let args = Reflect::get(obj, &JsValue::from_str("args")).unwrap_or(JsValue::NULL);
        let out_obj = Object::new();
        let _ = Reflect::set(&out_obj, &JsValue::from_str("seq"), &JsValue::from_f64(seq as f64));
        match parse_args(&args).and_then(|a| {
            let out = compute(&a);
            Ok(post_out(&out))
        }) {
            Ok(out) => {
                let _ = Reflect::set(&out_obj, &JsValue::from_str("out"), &out);
            }
            Err(e) => {
                let _ = Reflect::set(&out_obj, &JsValue::from_str("error"), &JsValue::from_str(&e));
            }
        }
        let _ = scope_post.post_message(&out_obj);
    });
    scope.set_onmessage(Some(closure.as_ref().unchecked_ref()));
    closure.forget();
}

#[cfg(target_arch = "wasm32")]
fn parse_args(v: &JsValue) -> Result<CoverageInput, String> {
    let o = v
        .dyn_ref::<Object>()
        .ok_or_else(|| "coverage args must be an object".to_string())?;
    let pos = f32_arr(Reflect::get(o, &JsValue::from_str("pos")).map_err(|e| err_str(e))?)?;
    let idx = u32_arr(Reflect::get(o, &JsValue::from_str("idx")).map_err(|e| err_str(e))?)?;
    let over = u32_vec(Reflect::get(o, &JsValue::from_str("over")).map_err(|e| err_str(e))?)?;
    let gap = Reflect::get(o, &JsValue::from_str("gap"))
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(0.2);
    let reach = Reflect::get(o, &JsValue::from_str("reach"))
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(5.0);
    let fins_js = Reflect::get(o, &JsValue::from_str("fins")).unwrap_or(JsValue::NULL);
    let mut fins = Vec::new();
    if let Some(arr) = fins_js.dyn_ref::<Array>() {
        for i in 0..arr.length() {
            let fp = f32_vec(arr.get(i));
            fins.push(fp);
        }
    }
    Ok(CoverageInput {
        positions: pos,
        triangles: idx,
        over_faces: over,
        fins,
        gap,
        reach,
        max_sub: 250_000,
    })
}

#[cfg(target_arch = "wasm32")]
fn post_out(out: &CoverageOutput) -> JsValue {
    let o = Object::new();
    let _ = Reflect::set(&o, &JsValue::from_str("total"), &JsValue::from_f64(out.total));
    let _ = Reflect::set(&o, &JsValue::from_str("held"), &JsValue::from_f64(out.held));
    let tris = Float32Array::new_with_length(out.tris.len() as u32);
    for (i, &v) in out.tris.iter().enumerate() {
        tris.set_index(i as u32, v);
    }
    let dist = Float32Array::new_with_length(out.dist.len() as u32);
    for (i, &v) in out.dist.iter().enumerate() {
        dist.set_index(i as u32, v);
    }
    let _ = Reflect::set(&o, &JsValue::from_str("tris"), &tris);
    let _ = Reflect::set(&o, &JsValue::from_str("dist"), &dist);
    let spots = Array::new();
    for s in &out.spots {
        let spot = Object::new();
        let _ = Reflect::set(&spot, &JsValue::from_str("area"), &JsValue::from_f64(s.area));
        let _ = Reflect::set(&spot, &JsValue::from_str("span"), &JsValue::from_f64(s.span));
        match s.far {
            Some(f) => {
                let _ = Reflect::set(&spot, &JsValue::from_str("far"), &JsValue::from_f64(f));
            }
            None => {
                let _ = Reflect::set(&spot, &JsValue::from_str("far"), &JsValue::NULL);
            }
        }
        let at = Array::new();
        for &x in &s.at {
            at.push(&JsValue::from_f64(x));
        }
        let _ = Reflect::set(&spot, &JsValue::from_str("at"), &at);
        match &s.ends {
            Some([lo, hi]) => {
                let ends = Array::new();
                for p in [lo, hi] {
                    let pt = Array::new();
                    for &x in p {
                        pt.push(&JsValue::from_f64(x));
                    }
                    ends.push(&pt);
                }
                let _ = Reflect::set(&spot, &JsValue::from_str("ends"), &ends);
            }
            None => {
                let _ = Reflect::set(&spot, &JsValue::from_str("ends"), &JsValue::NULL);
            }
        }
        spots.push(&spot);
    }
    let _ = Reflect::set(&o, &JsValue::from_str("spots"), &spots);
    o.into()
}

#[cfg(target_arch = "wasm32")]
fn f32_arr(v: JsValue) -> Result<Vec<f32>, String> {
    let a = v
        .dyn_ref::<Float32Array>()
        .ok_or_else(|| "pos must be Float32Array".to_string())?;
    Ok((0..a.length()).map(|i| a.get_index(i)).collect())
}

#[cfg(target_arch = "wasm32")]
fn f32_vec(v: JsValue) -> Vec<f32> {
    v.dyn_ref::<Float32Array>()
        .map(|a| (0..a.length()).map(|i| a.get_index(i)).collect())
        .unwrap_or_default()
}

#[cfg(target_arch = "wasm32")]
fn u32_arr(v: JsValue) -> Result<Vec<u32>, String> {
    let a = v
        .dyn_ref::<Uint32Array>()
        .ok_or_else(|| "idx must be Uint32Array".to_string())?;
    Ok((0..a.length()).map(|i| a.get_index(i)).collect())
}

#[cfg(target_arch = "wasm32")]
fn u32_vec(v: JsValue) -> Result<Vec<u32>, String> {
    let a = v
        .dyn_ref::<Array>()
        .ok_or_else(|| "over must be an array".to_string())?;
    let mut out = Vec::with_capacity(a.length() as usize);
    for i in 0..a.length() {
        let x = a.get(i);
        out.push(x.as_f64().unwrap_or(0.0) as u32);
    }
    Ok(out)
}

#[cfg(target_arch = "wasm32")]
fn err_str(e: wasm_bindgen::JsValue) -> String {
    e.as_string().unwrap_or_else(|| "error".into())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn coverage_worker_entry() {}
