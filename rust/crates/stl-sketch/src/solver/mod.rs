//! The sketch constraint solver (`web/js/constraints.js`).
//!
//! Same algorithm, tolerances, iteration caps and rank test as the 1.x JavaScript:
//! minimum-norm Gauss-Newton steps with a Levenberg-style damping fallback, and a
//! row-reduction rank test for the degrees of freedom.
//!
//! Differences in shape from the JS (the numbers are the same):
//! - `fixed` is a list of entities (the origin, the axes, part edges), as in 1.x, not ids;
//! - `unsatisfied` returns the constraints themselves (`unsatisfied_ids` gives their ids);
//! - `make_constraint` and `infer_dimension` return an [`Outcome`] (a constraint, a
//!   plain-English message, or "wait for another pick"), the three things the JS returns.

pub mod analyze;
pub mod constraints;
pub mod infer;
pub mod solve;

pub use analyze::{analyze, is_redundant, unsatisfied, unsatisfied_ids, Analysis};
pub use constraints::{glyph_spots, measure, refs_exist, Constraint, Ref, World};
pub use infer::{angle_to, infer_dimension, line_meet, make_constraint, Outcome};
pub use solve::{solve, Lock, SolveReport};
