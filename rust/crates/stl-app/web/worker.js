// Engine worker. A second instance of the same wasm module owns the Document.
// The page sets __STL_ENGINE_WORKER__ so init does not run the canvas entry.
// Mesh ArrayBuffers are transferred (detached here) and the page is told.
//
// A command runs inside one wasm call, which cannot see worker messages.
// Where the browser has JS Promise Integration, the module yields through
// __stlYield between features so Cancel can stop the rebuild. Without it
// (or when __STL_NO_JSPI__ is set), the glue's `new WebAssembly.Suspending`
// is replaced with the plain function and the command runs straight through.
// Cancel is then handled at the next command boundary: the rebuild that was
// already running finishes, and the flag does not stick to the next command.
self.__STL_ENGINE_WORKER__ = true;
self.__STL_QUEUE__ = [];
self.__STL_BUSY__ = false;
self.__STL_CANCEL__ = false;

function turn() {
  return new Promise((resolve) => {
    const channel = self.__stlYieldCh || (self.__stlYieldCh = new MessageChannel());
    channel.port1.onmessage = () => {
      channel.port1.onmessage = null;
      resolve();
    };
    channel.port2.postMessage(0);
  });
}

const jspi = typeof WebAssembly.Suspending === "function"
  && typeof WebAssembly.promising === "function"
  && !self.__STL_NO_JSPI__;
self.__STL_JSPI__ = jspi;
if (!jspi) {
  const pass = function (fn) { return fn; };
  for (const key of ["Suspending", "promising"]) {
    try {
      Object.defineProperty(WebAssembly, key, { configurable: true, writable: true, value: pass });
    } catch (e) {
      WebAssembly[key] = pass;
    }
  }
  self.__stlYield = function () {};
} else {
  self.__stlYield = turn;
}

const here = new URL(self.location.href);
const glue = self.__STL_GLUE__ || here.searchParams.get("glue") || new URL("./stl-app.js", here).href;
const wasmUrl = self.__STL_WASM__ || here.searchParams.get("wasm") || new URL("./stl-app_bg.wasm", here).href;
// Support Fins' own engine (engine.iife.js, copied beside this file by `xtask web`) defines
// SupportFinsEngine; the web host in the wasm module looks it up on this worker's global.
// A module worker has no importScripts, so it is fetched and run as a classic script.
try {
  const r = await fetch(new URL("./support-fins/engine.iife.js", here));
  if (r.ok) {
    (0, self.eval)((await r.text()) + "\n;if (typeof SupportFinsEngine !== 'undefined') self.SupportFinsEngine = SupportFinsEngine;");
  }
} catch (e) {
  // without it Support Fins reports that its engine is missing; the rest of the app is unaffected
}
const mod = await import(glue);
await mod.default(wasmUrl);
mod.engine_worker();

function fail(err) {
  const text = err && (err.stack || err.message) ? String(err.stack || err.message) : String(err);
  self.postMessage({ t: "engineLost", text });
  self.close();
}

async function pump(first) {
  let data = first;
  try {
    while (data) {
      let probe;
      try {
        probe = mod.stl_handle(data);
        if (probe && typeof probe.then === "function") probe = await probe;
        // Without JSPI the call above never returned to the event loop, so a
        // Cancel posted during it is still sitting in the message queue.
        if (!self.__STL_JSPI__) await turn();
      } catch (err) {
        fail(err);
        return;
      } finally {
        // The command has finished. A cancel for it must not hit the next one.
        self.__STL_CANCEL__ = false;
      }
      if (probe && probe.hadMesh) {
        self.postMessage({ t: "meshDetached", ok: probe.detached === true });
      }
      data = self.__STL_QUEUE__.shift();
    }
  } finally {
    self.__STL_BUSY__ = false;
  }
}

self.onmessage = (event) => {
  const data = event.data;
  if (data && data.t === "cancel") {
    // Idle cancel is dropped, matching the desktop link. While a command runs
    // under JSPI, the yield between features observes this flag. Without JSPI
    // the flag is only visible at the boundary after that command returns.
    if (self.__STL_BUSY__) self.__STL_CANCEL__ = true;
    return;
  }
  if (self.__STL_BUSY__) {
    self.__STL_QUEUE__.push(data);
    return;
  }
  self.__STL_BUSY__ = true;
  pump(data);
};
self.postMessage({ t: "ready" });
