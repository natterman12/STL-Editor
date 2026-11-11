//! The buffer pool is emptied by `release_scratch`. One test only: the pool is process-wide.

use stl_kernel::{release_scratch, scratch_held_bytes, Solid};

#[test]
fn release_scratch_empties_the_pool() {
    // big enough that its buffers (256 KB and up) are pooled
    let ball = Solid::sphere(20.0, 400);
    let cube = Solid::cube([25.0, 25.0, 25.0], true);
    let a = ball.difference(&cube);
    let b = ball.union(&cube);
    let before = a.to_mesh().triangles.len();
    drop((a, b));
    let held = scratch_held_bytes();
    eprintln!("pool after the booleans: {held} bytes");
    assert!(held > 0, "the pool holds buffers after the booleans");
    release_scratch();
    assert_eq!(scratch_held_bytes(), 0);
    // results do not depend on the pool
    let again = ball.difference(&cube);
    assert_eq!(again.to_mesh().triangles.len(), before);
    release_scratch();
    assert_eq!(scratch_held_bytes(), 0);
}
