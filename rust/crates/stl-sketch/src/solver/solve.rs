//! The solver: minimum-norm Gauss-Newton with a Levenberg-style damping fallback
//! (`linsolve`, `run` and `solve` in `web/js/constraints.js`).

use super::constraints::{
    get_param, get_path, hypot, params_of, refs_exist, residual, set_param, Constraint, World,
};
use crate::entities::{point, Entity};
use serde::{Deserialize, Serialize};

/// A pull toward a target while dragging or after typing a value: a point (`k` and `at`) or a
/// parameter (`path` and `value`). Constraints always win over locks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Lock {
    pub e: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub k: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<Vec<f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SolveReport {
    /// New entities, in the order of the input (the input is not modified).
    pub entities: Vec<Entity>,
    pub ok: bool,
    /// The remaining residual norm (0 when there was nothing to solve).
    pub error: f64,
}

/// Gauss-Jordan with partial pivoting; a pivot under 1e-14 leaves that unknown at 0.
pub(crate) fn linsolve(a: &[Vec<f64>], b: &[f64]) -> Vec<f64> {
    let n = b.len();
    let mut m: Vec<Vec<f64>> = a
        .iter()
        .enumerate()
        .map(|(i, row)| {
            let mut r = row.clone();
            r.push(b[i]);
            r
        })
        .collect();
    for col in 0..n {
        let mut piv = col;
        for r in col + 1..n {
            if m[r][col].abs() > m[piv][col].abs() {
                piv = r;
            }
        }
        if m[piv][col].abs() < 1e-14 {
            continue;
        }
        m.swap(col, piv);
        for r in 0..n {
            if r == col {
                continue;
            }
            let f = m[r][col] / m[col][col];
            if f == 0.0 || f.is_nan() {
                continue;
            }
            for k in col..=n {
                let v = m[col][k];
                m[r][k] -= f * v;
            }
        }
    }
    (0..n).map(|i| if m[i][i].abs() < 1e-14 { 0.0 } else { m[i][n] / m[i][i] }).collect()
}

/// One equation group: a constraint's residuals, or a lock's pull.
enum Eq<'a> {
    Con(&'a Constraint),
    Lock(&'a Lock),
}

fn eval(eq: &Eq, w: &World) -> Vec<f64> {
    match eq {
        Eq::Con(c) => residual(c, w).unwrap_or_default(),
        Eq::Lock(l) => {
            let Some(e) = w.get(&l.e) else { return Vec::new() };
            if l.k.as_deref().is_some_and(|k| !k.is_empty()) {
                let p = point(e, l.k.as_deref().unwrap()).unwrap_or([f64::NAN; 2]);
                let at = |i: usize| l.at.as_ref().and_then(|v| v.get(i)).copied().unwrap_or(f64::NAN);
                return vec![(p[0] - at(0)) * 0.05, (p[1] - at(1)) * 0.05];
            }
            let path = l.path.as_deref().unwrap_or("");
            vec![(get_path(e, path) - l.value.unwrap_or(f64::NAN)) * 0.05]
        }
    }
}

fn eval_all(eqs: &[Eq], w: &World) -> Vec<f64> {
    eqs.iter().flat_map(|e| eval(e, w)).collect()
}

fn set_vars(w: &mut World, vars: &[(usize, usize)], x: &[f64]) {
    for (i, &(ei, pi)) in vars.iter().enumerate() {
        set_param(&mut w.ents[ei], pi, x[i]);
    }
}

fn norm(f: &[f64]) -> f64 {
    let mut v = f.to_vec();
    v.push(0.0);
    hypot(&v)
}

/// `run`: iterate until the residual norm is under 1e-10; returns the final norm.
fn run(w: &mut World, vars: &[(usize, usize)], eqs: &[Eq], iters: usize) -> f64 {
    let n = vars.len();
    let mut x: Vec<f64> = vars.iter().map(|&(ei, pi)| get_param(&w.ents[ei], pi)).collect();
    let mut f = eval_all(eqs, w);
    let mut nrm = norm(&f);
    let mut it = 0;
    while it < iters && nrm > 1e-10 {
        let m = f.len();
        let h = 1e-7;
        let mut j = vec![vec![0.0_f64; n]; m];
        for col in 0..n {
            let old = x[col];
            x[col] = old + h;
            set_vars(w, vars, &x);
            let fp = eval_all(eqs, w);
            for i in 0..m {
                j[i][col] = (fp.get(i).copied().unwrap_or(f64::NAN) - f[i]) / h;
            }
            x[col] = old;
        }
        set_vars(w, vars, &x);
        let mut lambda = 1e-9_f64;
        let mut improved = false;
        let mut tries = 0;
        while tries < 8 && !improved {
            let a: Vec<Vec<f64>> = (0..m)
                .map(|i| {
                    (0..m)
                        .map(|k| {
                            let mut s = 0.0;
                            for c in 0..n {
                                s += j[i][c] * j[k][c];
                            }
                            s + if i == k { lambda } else { 0.0 }
                        })
                        .collect()
                })
                .collect();
            let y = linsolve(&a, &f);
            let mut dx = vec![0.0_f64; n];
            for i in 0..m {
                if y[i] != 0.0 && !y[i].is_nan() {
                    for c in 0..n {
                        dx[c] -= j[i][c] * y[i];
                    }
                }
            }
            let xn: Vec<f64> = x.iter().zip(&dx).map(|(v, d)| v + d).collect();
            set_vars(w, vars, &xn);
            let fnv = eval_all(eqs, w);
            let nn = norm(&fnv);
            if nn < nrm || nn < 1e-10 {
                x = xn;
                f = fnv;
                nrm = nn;
                improved = true;
            } else {
                set_vars(w, vars, &x);
                lambda *= 100.0;
            }
            tries += 1;
        }
        if !improved {
            break;
        }
        it += 1;
    }
    set_vars(w, vars, &x);
    nrm
}

/// Solve the sketch. `locks` pull points or parameters toward targets (while dragging or
/// after typing a value); constraints always win over locks. `fixed` are entities that never
/// move (the origin, the axes, part edges). The input is not modified.
pub fn solve(entities: &[Entity], constraints: &[Constraint], locks: &[Lock], fixed: &[Entity]) -> SolveReport {
    let mut w = World::new(entities);
    w.add_missing(fixed);
    let is_fixed = |id: &str| fixed.iter().any(|f| f.id() == Some(id));
    let valid: Vec<&Constraint> = constraints.iter().filter(|c| !c.has_ref() && refs_exist(c, &w)).collect();
    let collect = |w: &World| -> Vec<Entity> {
        entities.iter().map(|e| e.id().and_then(|id| w.get(id)).unwrap_or(e).clone()).collect()
    };
    if valid.is_empty() && locks.is_empty() {
        return SolveReport { entities: collect(&w), ok: true, error: 0.0 };
    }
    let mut touched: Vec<String> = Vec::new();
    let mut touch = |id: &str| {
        if !touched.iter().any(|x| x == id) {
            touched.push(id.to_string());
        }
    };
    for c in &valid {
        for r in [&c.a, &c.b, &c.c].into_iter().flatten() {
            touch(&r.e);
        }
    }
    for l in locks {
        touch(&l.e);
    }
    // only entities connected to a constraint or lock move
    let mut vars: Vec<(usize, usize)> = Vec::new();
    for id in &touched {
        if let Some(ei) = w.index(id) {
            if !is_fixed(id) {
                for pi in 0..params_of(&w.ents[ei]).len() {
                    vars.push((ei, pi));
                }
            }
        }
    }
    let ceqs: Vec<Eq> = valid.iter().map(|c| Eq::Con(c)).collect();
    if !locks.is_empty() {
        let mut all: Vec<Eq> = valid.iter().map(|c| Eq::Con(c)).collect();
        all.extend(locks.iter().map(Eq::Lock));
        run(&mut w, &vars, &all, 40);
    }
    let err = if !ceqs.is_empty() { run(&mut w, &vars, &ceqs, 80) } else { 0.0 };
    // keep shapes sane
    for id in &touched {
        if let Some(ei) = w.index(id) {
            if is_fixed(id) {
                continue;
            }
            match &mut w.ents[ei] {
                Entity::Arc(a) if a.r < 0.0 => a.r = -a.r,
                Entity::Circle(c) if c.r < 0.0 => c.r = -c.r,
                Entity::Polygon(p) if p.r < 0.0 => p.r = -p.r,
                Entity::Slot(s) if s.r < 0.0 => s.r = -s.r,
                _ => {}
            }
        }
    }
    let ok = err < 1e-6;
    SolveReport { entities: collect(&w), ok, error: err }
}
