// Support Fins coverage worker (1.x `sfcoverage.worker.js` contract).
// A second wasm instance; `__STL_COVERAGE_WORKER__` tells the module's `main` not to start the page.
self.__STL_COVERAGE_WORKER__ = true;

// The page posts its first job as soon as it has made this worker, while the module below is
// still loading. A message that arrives before there is a handler is dropped, and the check
// then never finishes, so jobs are kept here until the module's own handler is in place.
const early = [];
self.onmessage = (event) => early.push(event);

const here = new URL(self.location.href);
const glue = new URL("./stl-app.js", here).href;
const wasmUrl = new URL("./stl-app_bg.wasm", here).href;
const mod = await import(glue);
await mod.default(wasmUrl);
mod.coverage_worker_entry();
// only the newest job matters: the page ignores answers to older ones
const last = early.pop();
if (last) self.onmessage(last);
