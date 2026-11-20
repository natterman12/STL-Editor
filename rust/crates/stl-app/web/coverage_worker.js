// Support Fins coverage worker (1.x `sfcoverage.worker.js` contract).
// A second wasm instance; `__STL_COVERAGE_WORKER__` tells the module's `main` not to start the page.
self.__STL_COVERAGE_WORKER__ = true;

const here = new URL(self.location.href);
const glue = new URL("./stl-app.js", here).href;
const wasmUrl = new URL("./stl-app_bg.wasm", here).href;
const mod = await import(glue);
await mod.default(wasmUrl);
mod.coverage_worker_entry();
