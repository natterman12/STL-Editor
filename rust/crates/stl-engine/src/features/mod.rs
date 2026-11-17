//! Feature evaluators. Each feature file imports only `common` and the lower crates.

pub mod bedsplit;
pub mod combine;
pub mod common;
pub mod delete;
pub mod extrude;
pub mod fittest;
pub mod hinge;
pub mod hole;
pub mod hollow;
pub mod import;
pub mod lattice;
pub mod layflat;
pub mod loft;
pub mod mirror;
pub mod move_;
pub mod offset;
pub mod pattern;
pub mod plane;
pub mod plate;
pub mod primitive;
pub mod pushpull;
pub mod remesh;
pub mod repair;
pub mod resizehole;
pub mod revolve;
pub mod scale;
pub mod sculpt;
pub mod separate;
pub mod shell;
pub mod simplify;
pub mod sketch;
pub mod snapfit;
pub mod split;
pub mod supportfins;
pub mod supports_follow;
pub mod supportwall;
pub mod sweep;
pub mod thincheck;
pub mod thread;

use crate::Registry;

pub fn register_all(r: Registry) -> Registry {
    let r = import::register(r);
    let r = primitive::register(r);
    let r = plane::register(r);
    let r = move_::register(r);
    let r = scale::register(r);
    let r = mirror::register(r);
    let r = combine::register(r);
    let r = split::register(r);
    let r = delete::register(r);
    let r = separate::register(r);
    let r = repair::register(r);
    let r = sketch::register(r);
    let r = extrude::register(r);
    let r = revolve::register(r);
    let r = loft::register(r);
    let r = sweep::register(r);
    let r = hole::register(r);
    let r = pushpull::register(r);
    let r = simplify::register(r);
    let r = remesh::register(r);
    let r = sculpt::register(r);
    let r = offset::register(r);
    let r = hollow::register(r);
    let r = shell::register(r);
    let r = lattice::register(r);
    let r = thincheck::register(r);
    let r = bedsplit::register(r);
    let r = layflat::register(r);
    let r = plate::register(r);
    let r = pattern::register(r);
    let r = thread::register(r);
    let r = resizehole::register(r);
    let r = fittest::register(r);
    let r = snapfit::register(r);
    let r = hinge::register(r);
    let r = supportfins::register(r);

    supportwall::register(r)
}
