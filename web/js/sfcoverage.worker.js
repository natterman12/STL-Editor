// Works out the Support Fins coverage off the page's thread (see sfcoverage.js).
import { coverage } from './sfcoverage.js';

self.onmessage = e => {
  const { seq, args } = e.data;
  try {
    const out = coverage(args);
    self.postMessage({ seq, out }, [out.tris.buffer, out.dist.buffer]);
  } catch (err) { self.postMessage({ seq, error: String(err?.message || err) }); }
};
