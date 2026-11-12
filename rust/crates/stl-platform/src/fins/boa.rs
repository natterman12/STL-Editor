//! The fallback Support Fins host: Boa, a pure-Rust JavaScript interpreter. Correct but slow;
//! used only when the system web engine is missing or won't start.
//!
//! The plan names "one boa Context behind a Mutex". A Boa `Context` is not `Send` (its
//! garbage collector is per thread), so the one Context lives on its own thread and calls
//! reach it over a channel; a Mutex around that channel serializes the calls the same way.

use std::path::Path;
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, Sender, channel};

use boa_engine::{Context, JsString, JsValue, Source};
use stl_engine::{FinsHost, HostKind};

enum Req {
    Call(String, Vec<String>),
    Has(String),
}

pub struct BoaHost {
    chan: Mutex<(Sender<Req>, Receiver<Result<String, String>>)>,
}

impl BoaHost {
    /// Loads the engine file into a fresh Boa context on its own thread.
    pub fn start(engine_js: &Path) -> Result<Self, String> {
        let code = std::fs::read(engine_js).map_err(|e| format!("could not read {}: {e}", engine_js.display()))?;
        let (req_tx, req_rx) = channel::<Req>();
        let (res_tx, res_rx) = channel::<Result<String, String>>();
        std::thread::Builder::new()
            .name("support-fins-boa".into())
            // Boa's interpreter recurses on the native stack; the engine's deep calls need room.
            .stack_size(256 << 20)
            .spawn(move || {
                let mut ctx = Context::default();
                let limits = ctx.runtime_limits_mut();
                limits.set_recursion_limit(100_000);
                limits.set_stack_size_limit(10 << 20);
                let loaded = ctx
                    .eval(Source::from_bytes(&code))
                    .map_err(|e| error_text(e, &mut ctx))
                    .and_then(|_| {
                        engine(&mut ctx).map(|_| String::new()).ok_or_else(|| "SupportFinsEngine is not defined".into())
                    });
                let ok = loaded.is_ok();
                if res_tx.send(loaded).is_err() || !ok {
                    return;
                }
                while let Ok(req) = req_rx.recv() {
                    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match req {
                        Req::Call(name, args) => call(&mut ctx, &name, &args),
                        Req::Has(name) => Ok(function(&mut ctx, &name).is_some().to_string()),
                    }))
                    .unwrap_or_else(|_| Err("Error: the Boa engine failed".into()));
                    if res_tx.send(r).is_err() {
                        return;
                    }
                }
            })
            .map_err(|e| format!("could not start the Boa thread: {e}"))?;
        match res_rx.recv() {
            Ok(Ok(_)) => Ok(BoaHost { chan: Mutex::new((req_tx, res_rx)) }),
            Ok(Err(e)) => Err(format!("Boa could not load the Support Fins engine: {e}")),
            Err(_) => Err("the Boa thread stopped while loading the engine".into()),
        }
    }

    fn request(&self, req: Req) -> Result<String, String> {
        let chan = self.chan.lock().unwrap_or_else(|e| e.into_inner());
        chan.0.send(req).map_err(|_| super::STOPPED.to_string())?;
        chan.1.recv().map_err(|_| super::STOPPED.to_string())?
    }
}

fn engine(ctx: &mut Context) -> Option<boa_engine::JsObject> {
    let g = ctx.global_object();
    g.get(JsString::from("SupportFinsEngine"), ctx).ok()?.as_object()
}

fn function(ctx: &mut Context, name: &str) -> Option<(boa_engine::JsObject, boa_engine::JsObject)> {
    let e = engine(ctx)?;
    let f = e.get(JsString::from(name), ctx).ok()?.as_callable()?;
    Some((e, f))
}

/// `String(e)` of a thrown value, as the webview host reports it ("Error: message").
fn error_text(e: boa_engine::JsError, ctx: &mut Context) -> String {
    match e.into_opaque(ctx) {
        Ok(v) => v.to_string(ctx).map(|s| s.to_std_string_escaped()).unwrap_or_else(|_| "Error".into()),
        Err(e) => e.to_string(),
    }
}

fn call(ctx: &mut Context, name: &str, args: &[String]) -> Result<String, String> {
    let Some((this, f)) = function(ctx, name) else {
        return Err(format!("TypeError: SupportFinsEngine.{name} is not a function"));
    };
    let mut values = Vec::with_capacity(args.len());
    for a in args {
        let j: serde_json::Value = serde_json::from_str(a).map_err(|e| format!("SyntaxError: {e}"))?;
        values.push(JsValue::from_json(&j, ctx).map_err(|e| error_text(e, ctx))?);
    }
    let r = f.call(&this.into(), &values, ctx).map_err(|e| error_text(e, ctx))?;
    if let Some(s) = r.as_string() {
        return Ok(s.to_std_string_escaped());
    }
    match r.to_json(ctx).map_err(|e| error_text(e, ctx))? {
        Some(j) => Ok(j.to_string()),
        None => Ok("null".into()),
    }
}

impl FinsHost for BoaHost {
    fn call(&self, name: &str, args: &[&str]) -> Result<String, String> {
        self.request(Req::Call(name.to_string(), args.iter().map(|s| s.to_string()).collect()))
    }

    fn has(&self, name: &str) -> bool {
        matches!(self.request(Req::Has(name.to_string())).as_deref(), Ok("true"))
    }

    fn host_kind(&self) -> HostKind {
        HostKind::Boa
    }
}
