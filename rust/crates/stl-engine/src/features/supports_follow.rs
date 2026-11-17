//! Supports made as a body of their own follow the part they hold up (Nathan 2026-11-16, "fins
//! follow"): a Split shares the fins out between the halves, each fin going with the half it
//! sits under, and a Move of a part carries its fins along. `State::attached` says which part
//! each supports body holds up.

use stl_kernel::Solid;

use super::common::transform_body;
use crate::state::Body;
use crate::{FeatureResult, State};

/// How far around a fin to look for the part it touches (mm).
const REACH: f64 = 0.5;

/// How much of `half` lies in the box around `fin` (grown by [`REACH`]): the half a fin sits
/// under overlaps its box, the other half doesn't (or less, for a fin under the cut).
fn contact(fin: &Solid, half: &Solid) -> f64 {
    let bb = fin.bounding_box();
    let size = [0, 1, 2].map(|i| bb.max[i] - bb.min[i] + 2.0 * REACH);
    let around = Solid::cube(size, false).translate([bb.min[0] - REACH, bb.min[1] - REACH, bb.min[2] - REACH]);
    around.intersection(half).volume()
}

/// `part` was split: `halves` are the bodies it became, in order (the first kept the part's id),
/// with their solids; a half that was thrown away is `gone`. Each supports body that held up
/// `part` is shared out: every fin goes with the half it touches most, the first half's fins
/// stay in the supports body and each other half gets a new one (`<fid>.<n>s`). Fins under the
/// half that was thrown away go with it.
pub fn share_on_split(
    state: &mut State,
    part: &str,
    halves: &[(String, Solid)],
    gone: Option<&Solid>,
    fid: &str,
    res: &mut FeatureResult,
) {
    let sids: Vec<String> = state
        .attached
        .iter()
        .filter(|(_, p)| p.as_str() == part)
        .map(|(s, _)| s.clone())
        .collect();
    for sid in sids {
        let Some(body) = state.bodies.get(&sid).cloned() else {
            state.attached.shift_remove(&sid);
            continue;
        };
        let Some(fins) = body.solid.as_ref() else { continue };
        let mut groups: Vec<Vec<Solid>> = vec![Vec::new(); halves.len()];
        debug_assert!(!halves.is_empty());
        for fin in fins.decompose() {
            let mut scores: Vec<f64> = halves.iter().map(|(_, h)| contact(&fin, h)).collect();
            // the thrown-away half scores last: a fin under it is dropped
            scores.push(gone.map_or(0.0, |g| contact(&fin, g)));
            let best = scores
                .iter()
                .enumerate()
                .fold(None, |acc: Option<(usize, f64)>, (i, &s)| match acc {
                    Some((_, b)) if b >= s => acc,
                    _ if s > 0.0 => Some((i, s)),
                    _ => acc,
                });
            match best {
                Some((i, _)) if i < halves.len() => groups[i].push(fin),
                Some(_) => {}
                // touching no half: it stays with the first one when there is one
                None if !halves.is_empty() => groups[0].push(fin),
                None => {}
            }
        }
        // a supports body named after the part ("Box supports") is renamed after its half
        let suffix = body.name.rsplit(' ').next().unwrap_or("supports").to_string();
        for (i, ((hid, _), group)) in halves.iter().zip(groups).enumerate() {
            let hname = state.bodies.get(hid).map(|b| b.name.clone()).unwrap_or_default();
            let id = if i == 0 {
                sid.clone()
            } else {
                format!("{fid}.{}s", i + 1)
            };
            if group.is_empty() {
                if i == 0 {
                    state.bodies.shift_remove(&sid);
                    state.attached.shift_remove(&sid);
                    res.removed.push(sid.clone());
                }
                continue;
            }
            let solid = Solid::compose(&group);
            state.bodies.insert(
                id.clone(),
                Body {
                    name: format!("{hname} {suffix}"),
                    solid: Some(solid),
                    mesh: None,
                },
            );
            state.attached.insert(id.clone(), hid.clone());
            if i == 0 {
                res.modified.push(id);
            } else {
                res.created.push(id);
            }
        }
        if halves.is_empty() {
            state.bodies.shift_remove(&sid);
            state.attached.shift_remove(&sid);
            res.removed.push(sid);
        }
    }
}

/// `part` was moved by `m` (not copied): the supports that hold it up go with it, unless they
/// were moved themselves (`moved`).
pub fn carry_on_move(state: &mut State, part: &str, m: [[f64; 4]; 3], moved: &[String], res: &mut FeatureResult) {
    let sids: Vec<String> = state
        .attached
        .iter()
        .filter(|(s, p)| p.as_str() == part && !moved.contains(s))
        .map(|(s, _)| s.clone())
        .collect();
    for sid in sids {
        if let Some(b) = state.bodies.get(&sid).cloned() {
            state.bodies.insert(sid.clone(), transform_body(&b, m));
            res.modified.push(sid);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::{Value, json};

    use super::*;
    use crate::Ctx;

    fn cube(min: [f64; 3], size: [f64; 3]) -> Solid {
        Solid::cube(size, false).translate(min)
    }

    /// A 40 x 10 x 10 bar held 10 mm up (x -20..20), a fin under each end (x -18..-14 and
    /// 14..18), the fins one supports body "s" attached to the bar "b".
    fn bar() -> State {
        let mut st = State::default();
        st.bodies.insert(
            "b".into(),
            Body {
                name: "Bar".into(),
                solid: Some(cube([-20.0, 0.0, 10.0], [40.0, 10.0, 10.0])),
                mesh: None,
            },
        );
        let fins = Solid::compose(&[
            cube([-18.0, 3.0, 0.0], [4.0, 4.0, 10.0]),
            cube([14.0, 3.0, 0.0], [4.0, 4.0, 10.0]),
        ]);
        st.bodies.insert(
            "s".into(),
            Body {
                name: "Bar supports".into(),
                solid: Some(fins),
                mesh: None,
            },
        );
        st.attached.insert("s".into(), "b".into());
        st
    }

    fn run(kind: &str, st: &mut State, fid: &str, p: Value) -> FeatureResult {
        let src = BTreeMap::new();
        let ctx = Ctx {
            sources: &src,
            preview: false,
            fins: &crate::NullHost,
        };
        let mut res = FeatureResult::default();
        let reg = crate::Registry::builtin();
        (reg.get(kind).unwrap())(st, fid, p.as_object().unwrap(), &mut res, &ctx).unwrap();
        res
    }

    fn bb(st: &State, id: &str) -> ([f64; 3], [f64; 3]) {
        let b = st.bodies[id].solid.as_ref().unwrap().bounding_box();
        (b.min, b.max)
    }

    fn split_at_x0(st: &mut State, keep: &str) -> FeatureResult {
        run(
            "split",
            st,
            "f8",
            json!({"body": "b", "plane": {"origin": [0, 0, 0], "normal": [1, 0, 0]}, "keep": keep}),
        )
    }

    #[test]
    fn a_split_shares_the_fins_between_the_halves() {
        let mut st = bar();
        let res = split_at_x0(&mut st, "both");
        // "b" is the half on the normal's side (x > 0), "f8.2" the other one
        let (b_min, _) = bb(&st, "b");
        assert!(b_min[0] > -1e-6, "b is the x > 0 half");
        assert_eq!(st.attached.get("s").map(String::as_str), Some("b"));
        assert_eq!(st.attached.get("f8.2s").map(String::as_str), Some("f8.2"));
        let (s_min, s_max) = bb(&st, "s");
        assert!(
            (s_min[0] - 14.0).abs() < 1e-6 && (s_max[0] - 18.0).abs() < 1e-6,
            "s keeps the fin under b: {s_min:?} {s_max:?}"
        );
        let (t_min, t_max) = bb(&st, "f8.2s");
        assert!(
            (t_min[0] + 18.0).abs() < 1e-6 && (t_max[0] + 14.0).abs() < 1e-6,
            "the other fin goes with f8.2"
        );
        assert_eq!(st.bodies["f8.2s"].name, "Bar (2) supports");
        assert_eq!(st.bodies["s"].name, "Bar (1) supports");
        assert!(res.created.contains(&"f8.2s".to_string()));
    }

    #[test]
    fn moving_a_half_takes_its_fins_along() {
        let mut st = bar();
        split_at_x0(&mut st, "both");
        run(
            "move",
            &mut st,
            "f10",
            json!({"bodies": ["f8.2"], "translate": [0, 30, 0], "rotate": [0, 0, 0], "pivot": "center"}),
        );
        let (t_min, _) = bb(&st, "f8.2s");
        assert!((t_min[1] - 33.0).abs() < 1e-6, "f8.2's fin moved 30 mm: {t_min:?}");
        let (s_min, _) = bb(&st, "s");
        assert!((s_min[1] - 3.0).abs() < 1e-6, "b's fin stayed: {s_min:?}");
        // a copy leaves the fins where they are
        run(
            "move",
            &mut st,
            "f11",
            json!({"bodies": ["b"], "translate": [0, 50, 0], "copy": true}),
        );
        let (s_min, _) = bb(&st, "s");
        assert!((s_min[1] - 3.0).abs() < 1e-6);
        // moving the fins themselves with the part moves them once
        run(
            "move",
            &mut st,
            "f12",
            json!({"bodies": ["b", "s"], "translate": [0, 0, 5]}),
        );
        let (s_min, _) = bb(&st, "s");
        assert!((s_min[2] - 5.0).abs() < 1e-6, "once, not twice: {s_min:?}");
    }

    #[test]
    fn keeping_one_half_drops_the_other_halfs_fins() {
        let mut st = bar();
        split_at_x0(&mut st, "below");
        // "below" keeps x < 0: its fin is the one at x -18..-14
        let (s_min, _) = bb(&st, "s");
        assert!((s_min[0] + 18.0).abs() < 1e-6, "{s_min:?}");
        assert_eq!(
            st.bodies
                .keys()
                .filter(|k| k.ends_with('s') && k.as_str() != "s")
                .count(),
            0
        );
        assert_eq!(st.bodies["s"].solid.as_ref().unwrap().decompose().len(), 1);
    }
}
