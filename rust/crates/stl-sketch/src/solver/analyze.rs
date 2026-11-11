//! How defined the sketch is: the rank of the constraints' Jacobian (`analyze`, `isRedundant`,
//! `unsatisfied` in `web/js/constraints.js`).

use super::constraints::{
    get_param, js_max, params_of, refs_exist, residual, set_param, Constraint, World,
};
use crate::entities::Entity;
use serde::Serialize;
use std::collections::{BTreeSet, HashMap};

/// What is left free in the sketch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Analysis {
    /// Degrees of freedom left.
    pub dof: usize,
    /// Rank of the constraints' Jacobian.
    pub rank: usize,
    /// Ids of the shapes that can still move.
    pub under: BTreeSet<String>,
}

type Vars = Vec<(usize, usize)>;

fn jacobian(w: &mut World, vars: &Vars, cons: &[&Constraint]) -> Vec<Vec<f64>> {
    let base: Vec<Vec<f64>> = cons.iter().map(|c| residual(c, w).unwrap_or_default()).collect();
    let mut off = Vec::new();
    let mut m = 0;
    for r in &base {
        off.push(m);
        m += r.len();
    }
    // which constraints touch which entity (entity id -> constraint indices, ascending)
    let mut by_ent: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, c) in cons.iter().enumerate() {
        for r in [&c.a, &c.b, &c.c].into_iter().flatten() {
            let v = by_ent.entry(r.e.as_str()).or_default();
            if !v.contains(&i) {
                v.push(i);
            }
        }
    }
    let mut j = vec![vec![0.0_f64; vars.len()]; m];
    for (col, &(ei, pi)) in vars.iter().enumerate() {
        let id = w.ents[ei].id().unwrap_or("").to_string();
        let x0 = get_param(&w.ents[ei], pi);
        let h = 1e-6 * js_max(1.0, x0.abs());
        let Some(list) = by_ent.get(id.as_str()) else { continue };
        for &i in list {
            set_param(&mut w.ents[ei], pi, x0 + h);
            let fp = residual(cons[i], w).unwrap_or_default();
            set_param(&mut w.ents[ei], pi, x0 - h);
            let fm = residual(cons[i], w).unwrap_or_default();
            set_param(&mut w.ents[ei], pi, x0);
            for r in 0..base[i].len() {
                j[off[i] + r][col] = (fp.get(r).copied().unwrap_or(0.0) - fm.get(r).copied().unwrap_or(0.0)) / (2.0 * h);
            }
        }
    }
    j
}

struct RankNull {
    rank: usize,
    nulls: Vec<Vec<f64>>,
}

fn rank_null(jac: &[Vec<f64>], n: usize) -> RankNull {
    let mut r: Vec<Vec<f64>> = jac
        .iter()
        .map(|row| {
            let mx = row.iter().fold(0.0_f64, |a, &x| js_max(a, x.abs()));
            if mx > 0.0 {
                row.iter().map(|x| x / mx).collect()
            } else {
                row.clone()
            }
        })
        .collect();
    let mut piv: Vec<usize> = Vec::new();
    let mut row = 0;
    let mut col = 0;
    while col < n && row < r.len() {
        let mut best = row;
        for k in row + 1..r.len() {
            if r[k][col].abs() > r[best][col].abs() {
                best = k;
            }
        }
        if r[best][col].abs() < 1e-7 {
            col += 1;
            continue;
        }
        r.swap(row, best);
        let pv = r[row][col];
        for k in col..n {
            r[row][k] /= pv;
        }
        for k in 0..r.len() {
            if k == row {
                continue;
            }
            let f = r[k][col];
            if f == 0.0 || f.is_nan() {
                continue;
            }
            for c in col..n {
                let v = r[row][c];
                r[k][c] -= f * v;
            }
        }
        piv.push(col);
        row += 1;
        col += 1;
    }
    let mut nulls = Vec::new();
    for f in 0..n {
        if piv.contains(&f) {
            continue;
        }
        let mut v = vec![0.0_f64; n];
        v[f] = 1.0;
        for (i, &p) in piv.iter().enumerate() {
            v[p] = -r[i][f];
        }
        let mx = v.iter().fold(0.0_f64, |a, &x| js_max(a, x.abs()));
        let mx = if mx == 0.0 || mx.is_nan() { 1.0 } else { mx };
        nulls.push(v.iter().map(|x| x / mx).collect());
    }
    RankNull { rank: piv.len(), nulls }
}

fn find(parent: &mut HashMap<String, String>, x: &str) -> String {
    let mut x = x.to_string();
    loop {
        let p = parent[&x].clone();
        if p == x {
            return x;
        }
        let gp = parent[&p].clone();
        parent.insert(x.clone(), gp.clone());
        x = gp;
    }
}

/// Rank of the constraints' Jacobian: what is left free (degrees of freedom), and which
/// shapes can still move. Shapes in `fixed` (origin, axes, part edges) and polylines (text
/// outlines) don't count.
pub fn analyze(entities: &[Entity], constraints: &[Constraint], fixed: &[Entity]) -> Analysis {
    let mut w = World::new(entities);
    w.add_missing(fixed);
    let fx: BTreeSet<&str> = fixed.iter().filter_map(|f| f.id()).collect();
    let cons: Vec<&Constraint> = constraints.iter().filter(|c| !c.has_ref() && refs_exist(c, &w)).collect();
    // shapes tied together by constraints form groups; each group is worked out on its own
    let mut parent: HashMap<String, String> = HashMap::new();
    let free: Vec<&Entity> = entities
        .iter()
        .filter(|e| e.id().is_some_and(|id| !fx.contains(id)) && !matches!(e, Entity::Polyline(_)))
        .collect();
    for e in &free {
        let id = e.id().unwrap().to_string();
        parent.insert(id.clone(), id);
    }
    let refs_in = |c: &Constraint| -> Vec<String> { [&c.a, &c.b, &c.c].into_iter().flatten().map(|r| r.e.clone()).collect() };
    for c in &cons {
        let ids: Vec<String> = refs_in(c).into_iter().filter(|e| parent.contains_key(e)).collect();
        for i in 1..ids.len() {
            let a = find(&mut parent, &ids[0]);
            let b = find(&mut parent, &ids[i]);
            if a != b {
                parent.insert(a, b);
            }
        }
    }
    let mut group_order: Vec<String> = Vec::new();
    let mut by_group: HashMap<String, Vec<&Constraint>> = HashMap::new();
    for c in &cons {
        if let Some(r) = refs_in(c).into_iter().find(|e| parent.contains_key(e)) {
            let g = find(&mut parent, &r);
            by_group.entry(g).or_default().push(c);
        }
    }
    let mut members: HashMap<String, Vec<&Entity>> = HashMap::new();
    for e in &free {
        let g = find(&mut parent, e.id().unwrap());
        if !members.contains_key(&g) {
            group_order.push(g.clone());
        }
        members.entry(g).or_default().push(e);
    }
    let (mut dof, mut rank) = (0usize, 0usize);
    let mut under: BTreeSet<String> = BTreeSet::new();
    for g in &group_order {
        let ents = &members[g];
        let mut vars: Vars = Vec::new();
        for e in ents {
            let ei = w.index(e.id().unwrap()).unwrap();
            for pi in 0..params_of(&w.ents[ei]).len() {
                vars.push((ei, pi));
            }
        }
        let gc = by_group.get(g).cloned().unwrap_or_default();
        if gc.is_empty() {
            dof += vars.len();
            for e in ents {
                under.insert(e.id().unwrap().to_string());
            }
            continue;
        }
        let rn = rank_null(&jacobian(&mut w, &vars, &gc), vars.len());
        rank += rn.rank;
        dof += vars.len() - rn.rank;
        for v in &rn.nulls {
            for (j, x) in v.iter().enumerate() {
                if x.abs() > 1e-6 {
                    under.insert(w.ents[vars[j].0].id().unwrap().to_string());
                }
            }
        }
    }
    Analysis { dof, rank, under }
}

/// Would `c` add nothing new (the other constraints already settle what it measures)?
pub fn is_redundant(entities: &[Entity], constraints: &[Constraint], c: &Constraint, fixed: &[Entity]) -> bool {
    let a = analyze(entities, constraints, fixed);
    let mut more = constraints.to_vec();
    more.push(c.clone());
    let b = analyze(entities, &more, fixed);
    b.rank == a.rank
}

/// Constraints that don't hold (after a failed solve, or in a sketch opened that way).
pub fn unsatisfied(entities: &[Entity], constraints: &[Constraint], fixed: &[Entity]) -> Vec<Constraint> {
    let mut w = World::new(entities);
    w.add_missing(fixed);
    constraints
        .iter()
        .filter(|c| !c.has_ref() && refs_exist(c, &w))
        .filter(|c| residual(c, &w).unwrap_or_default().iter().any(|x| x.abs() > 1e-5))
        .cloned()
        .collect()
}

/// The `id`s of [`unsatisfied`] constraints (ids that are not strings are written as JSON).
pub fn unsatisfied_ids(entities: &[Entity], constraints: &[Constraint], fixed: &[Entity]) -> Vec<String> {
    unsatisfied(entities, constraints, fixed)
        .iter()
        .filter_map(|c| c.id.as_ref())
        .map(|v| v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string()))
        .collect()
}
