//! Feature evaluators. Each feature file imports only `common` and the lower crates.

pub mod common;
pub mod import;
pub mod primitive;
pub mod plane;
pub mod move_;
pub mod scale;
pub mod mirror;
pub mod combine;
pub mod split;
pub mod delete;
pub mod separate;
pub mod repair;
pub mod sketch;
pub mod extrude;
pub mod revolve;
pub mod loft;
pub mod sweep;
pub mod hole;
pub mod pushpull;

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
    r
}
