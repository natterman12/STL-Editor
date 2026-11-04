// Quadric edge-collapse decimation: a port of Sven Forstmann's Fast-Quadric-Mesh-Simplification
// (MIT), the library the desktop app uses through fast_simplification, so both editions reduce
// meshes the same way. Works on any triangle soup, watertight or not.

/**
 * @param {Float32Array|Float64Array} v  vertex positions (x, y, z, ...)
 * @param {Uint32Array} f  triangles
 * @param {number} targetCount  triangles to keep
 * @returns {{v: Float32Array, f: Uint32Array}}
 */
export function decimate(v, f, targetCount, aggressiveness = 7) {
  const nv = v.length / 3;
  let nt = f.length / 3;
  // vertices
  const P = Float64Array.from(v);
  const Q = new Float64Array(nv * 10);
  const border = new Uint8Array(nv);
  const tstartV = new Int32Array(nv), tcountV = new Int32Array(nv);
  // triangles (compacted in place as the algorithm does)
  let TV = Int32Array.from(f);
  let ERR = new Float64Array(nt * 4), TN = new Float64Array(nt * 3);
  let DEL = new Uint8Array(nt), DIRTY = new Uint8Array(nt);
  // references
  let refT = new Int32Array(nt * 3 * 2), refS = new Int32Array(nt * 3 * 2), nref = 0;
  const pushRef = (t, s) => {
    if (nref === refT.length) {
      const a = new Int32Array(refT.length * 2); a.set(refT); refT = a;
      const b = new Int32Array(refS.length * 2); b.set(refS); refS = b;
    }
    refT[nref] = t; refS[nref] = s; nref++;
  };

  const det = (q, o, a11, a12, a13, a21, a22, a23, a31, a32, a33) =>
    q[o + a11] * q[o + a22] * q[o + a33] + q[o + a13] * q[o + a21] * q[o + a32] + q[o + a12] * q[o + a23] * q[o + a31]
    - q[o + a13] * q[o + a22] * q[o + a31] - q[o + a11] * q[o + a23] * q[o + a32] - q[o + a12] * q[o + a21] * q[o + a33];
  const vErr = (q, x, y, z) => q[0] * x * x + 2 * q[1] * x * y + 2 * q[2] * x * z + 2 * q[3] * x + q[4] * y * y
    + 2 * q[5] * y * z + 2 * q[6] * y + q[7] * z * z + 2 * q[8] * z + q[9];
  const qs = new Float64Array(10), pr = new Float64Array(3);
  /** error of collapsing edge (a, b); the best point goes to pr */
  const calcError = (a, b) => {
    for (let k = 0; k < 10; k++) qs[k] = Q[a * 10 + k] + Q[b * 10 + k];
    const bord = border[a] & border[b];
    const d = det(qs, 0, 0, 1, 2, 1, 4, 5, 2, 5, 7);
    if (d !== 0 && !bord) {
      pr[0] = -1 / d * det(qs, 0, 1, 2, 3, 4, 5, 6, 5, 7, 8);
      pr[1] = 1 / d * det(qs, 0, 0, 2, 3, 1, 5, 6, 2, 7, 8);
      pr[2] = -1 / d * det(qs, 0, 0, 1, 3, 1, 4, 6, 2, 5, 8);
      return vErr(qs, pr[0], pr[1], pr[2]);
    }
    const x1 = P[a * 3], y1 = P[a * 3 + 1], z1 = P[a * 3 + 2], x2 = P[b * 3], y2 = P[b * 3 + 1], z2 = P[b * 3 + 2];
    const x3 = (x1 + x2) / 2, y3 = (y1 + y2) / 2, z3 = (z1 + z2) / 2;
    const e1 = vErr(qs, x1, y1, z1), e2 = vErr(qs, x2, y2, z2), e3 = vErr(qs, x3, y3, z3);
    const e = Math.min(e1, Math.min(e2, e3));
    if (e1 === e) { pr[0] = x1; pr[1] = y1; pr[2] = z1; }
    if (e2 === e) { pr[0] = x2; pr[1] = y2; pr[2] = z2; }
    if (e3 === e) { pr[0] = x3; pr[1] = y3; pr[2] = z3; }
    return e;
  };
  const triErr = t => {
    const a = TV[t * 3], b = TV[t * 3 + 1], c = TV[t * 3 + 2];
    ERR[t * 4] = calcError(a, b); ERR[t * 4 + 1] = calcError(b, c); ERR[t * 4 + 2] = calcError(c, a);
    ERR[t * 4 + 3] = Math.min(ERR[t * 4], Math.min(ERR[t * 4 + 1], ERR[t * 4 + 2]));
  };

  const updateMesh = iteration => {
    if (iteration > 0) {   // compact triangles
      let dst = 0;
      for (let i = 0; i < nt; i++) if (!DEL[i]) {
        if (dst !== i) {
          TV[dst * 3] = TV[i * 3]; TV[dst * 3 + 1] = TV[i * 3 + 1]; TV[dst * 3 + 2] = TV[i * 3 + 2];
          for (let k = 0; k < 4; k++) ERR[dst * 4 + k] = ERR[i * 4 + k];
          for (let k = 0; k < 3; k++) TN[dst * 3 + k] = TN[i * 3 + k];
          DIRTY[dst] = DIRTY[i];
        }
        DEL[dst] = 0;
        dst++;
      }
      nt = dst;
    }
    if (iteration === 0) {
      Q.fill(0); border.fill(0);
      for (let t = 0; t < nt; t++) {
        const a = TV[t * 3] * 3, b = TV[t * 3 + 1] * 3, c = TV[t * 3 + 2] * 3;
        const ux = P[b] - P[a], uy = P[b + 1] - P[a + 1], uz = P[b + 2] - P[a + 2];
        const wx = P[c] - P[a], wy = P[c + 1] - P[a + 1], wz = P[c + 2] - P[a + 2];
        let nx = uy * wz - uz * wy, ny = uz * wx - ux * wz, nz = ux * wy - uy * wx;
        const l = Math.sqrt(nx * nx + ny * ny + nz * nz);
        nx /= l; ny /= l; nz /= l;
        TN[t * 3] = nx; TN[t * 3 + 1] = ny; TN[t * 3 + 2] = nz;
        const dd = -(nx * P[a] + ny * P[a + 1] + nz * P[a + 2]);
        const m = [nx * nx, nx * ny, nx * nz, nx * dd, ny * ny, ny * nz, ny * dd, nz * nz, nz * dd, dd * dd];
        for (let j = 0; j < 3; j++) { const o = TV[t * 3 + j] * 10; for (let k = 0; k < 10; k++) Q[o + k] += m[k]; }
      }
    }
    // reference lists
    tcountV.fill(0);
    for (let i = 0; i < nt * 3; i++) tcountV[TV[i]]++;
    let ts = 0;
    for (let i = 0; i < nv; i++) { tstartV[i] = ts; ts += tcountV[i]; tcountV[i] = 0; }
    if (refT.length < nt * 3) { refT = new Int32Array(nt * 3 * 2); refS = new Int32Array(nt * 3 * 2); }
    nref = nt * 3;
    for (let t = 0; t < nt; t++) for (let j = 0; j < 3; j++) {
      const w = TV[t * 3 + j], r = tstartV[w] + tcountV[w];
      refT[r] = t; refS[r] = j; tcountV[w]++;
    }
    if (iteration === 0) {
      // border vertices: a neighbour seen only once means an open edge
      const ids = [], cnt = [];
      for (let i = 0; i < nv; i++) {
        ids.length = 0; cnt.length = 0;
        for (let j = 0; j < tcountV[i]; j++) {
          const t = refT[tstartV[i] + j];
          for (let k = 0; k < 3; k++) {
            const id = TV[t * 3 + k];
            const o = ids.indexOf(id);
            if (o < 0) { ids.push(id); cnt.push(1); } else cnt[o]++;
          }
        }
        for (let j = 0; j < ids.length; j++) if (cnt[j] === 1) border[ids[j]] = 1;
      }
      for (let t = 0; t < nt; t++) triErr(t);
    }
  };

  let deleted0 = new Uint8Array(64), deleted1 = new Uint8Array(64);
  const flipped = (px, py, pz, i0, i1, deleted) => {
    for (let k = 0; k < tcountV[i0]; k++) {
      const r = tstartV[i0] + k, t = refT[r];
      if (DEL[t]) continue;
      const s = refS[r], id1 = TV[t * 3 + (s + 1) % 3], id2 = TV[t * 3 + (s + 2) % 3];
      if (id1 === i1 || id2 === i1) { deleted[k] = 1; continue; }
      let ax = P[id1 * 3] - px, ay = P[id1 * 3 + 1] - py, az = P[id1 * 3 + 2] - pz;
      let l = Math.sqrt(ax * ax + ay * ay + az * az); ax /= l; ay /= l; az /= l;
      let bx = P[id2 * 3] - px, by = P[id2 * 3 + 1] - py, bz = P[id2 * 3 + 2] - pz;
      l = Math.sqrt(bx * bx + by * by + bz * bz); bx /= l; by /= l; bz /= l;
      if (Math.abs(ax * bx + ay * by + az * bz) > 0.999) return true;
      let nx = ay * bz - az * by, ny = az * bx - ax * bz, nz = ax * by - ay * bx;
      l = Math.sqrt(nx * nx + ny * ny + nz * nz); nx /= l; ny /= l; nz /= l;
      deleted[k] = 0;
      if (nx * TN[t * 3] + ny * TN[t * 3 + 1] + nz * TN[t * 3 + 2] < 0.2) return true;
    }
    return false;
  };
  let deletedTris = 0;
  const updateTriangles = (i0, w, deleted) => {
    for (let k = 0; k < tcountV[w]; k++) {
      const r = tstartV[w] + k, t = refT[r];
      if (DEL[t]) continue;
      if (deleted[k]) { DEL[t] = 1; deletedTris++; continue; }
      TV[t * 3 + refS[r]] = i0;
      DIRTY[t] = 1;
      triErr(t);
      pushRef(t, refS[r]);
    }
  };

  const triangleCount = nt;
  DEL.fill(0);
  for (let iteration = 0; iteration < 100; iteration++) {
    if (triangleCount - deletedTris <= targetCount) break;
    if (iteration % 5 === 0) updateMesh(iteration);
    DIRTY.fill(0, 0, nt);
    const threshold = 0.000000001 * Math.pow(iteration + 3, aggressiveness);
    for (let i = 0; i < nt; i++) {
      if (ERR[i * 4 + 3] > threshold || DEL[i] || DIRTY[i]) continue;
      for (let j = 0; j < 3; j++) {
        if (!(ERR[i * 4 + j] < threshold)) continue;
        const i0 = TV[i * 3 + j], i1 = TV[i * 3 + (j + 1) % 3];
        if (border[i0] !== border[i1]) continue;
        calcError(i0, i1);
        const px = pr[0], py = pr[1], pz = pr[2];
        if (deleted0.length < tcountV[i0]) deleted0 = new Uint8Array(tcountV[i0] * 2);
        if (deleted1.length < tcountV[i1]) deleted1 = new Uint8Array(tcountV[i1] * 2);
        deleted0.fill(0, 0, tcountV[i0]); deleted1.fill(0, 0, tcountV[i1]);
        if (flipped(px, py, pz, i0, i1, deleted0)) continue;
        if (flipped(px, py, pz, i1, i0, deleted1)) continue;
        P[i0 * 3] = px; P[i0 * 3 + 1] = py; P[i0 * 3 + 2] = pz;
        for (let k = 0; k < 10; k++) Q[i0 * 10 + k] += Q[i1 * 10 + k];
        const tstart = nref;
        updateTriangles(i0, i0, deleted0);
        updateTriangles(i0, i1, deleted1);
        const tcount = nref - tstart;
        if (tcount <= tcountV[i0]) {
          if (tcount) { refT.copyWithin(tstartV[i0], tstart, tstart + tcount); refS.copyWithin(tstartV[i0], tstart, tstart + tcount); }
        } else tstartV[i0] = tstart;
        tcountV[i0] = tcount;
        break;
      }
      if (triangleCount - deletedTris <= targetCount) break;
    }
  }
  // compact
  const remap = new Int32Array(nv).fill(-1), outV = [], outF = [];
  for (let t = 0; t < nt; t++) {
    if (DEL[t]) continue;
    for (let j = 0; j < 3; j++) {
      const w = TV[t * 3 + j];
      if (remap[w] < 0) { remap[w] = outV.length / 3; outV.push(P[w * 3], P[w * 3 + 1], P[w * 3 + 2]); }
      outF.push(remap[w]);
    }
  }
  return { v: Float32Array.from(outV), f: Uint32Array.from(outF) };
}
