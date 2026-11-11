//! The pool is emptied after the kernel has been idle, never during an operation.
//! One test only: the pool is process-wide.

use std::time::{Duration, Instant};
use stl_kernel::{scratch_held_bytes, set_idle_timeout, ScratchGuard, Solid};

fn booleans() -> usize {
    let ball = Solid::sphere(20.0, 400);
    let cube = Solid::cube([25.0, 25.0, 25.0], true);
    let a = ball.difference(&cube);
    let n = a.to_mesh().triangles.len();
    drop((a, ball.union(&cube)));
    n
}

#[test]
fn pool_is_released_after_idle_but_not_during_an_operation() {
    set_idle_timeout(Duration::from_millis(800));
    let op = ScratchGuard::begin();
    let before = booleans();
    assert!(scratch_held_bytes() > 0, "the pool holds buffers right after an operation");
    // an operation that runs longer than the timeout keeps its pool
    std::thread::sleep(Duration::from_millis(1600));
    assert!(scratch_held_bytes() > 0, "not released while an operation is running");
    drop(op);
    assert!(scratch_held_bytes() > 0, "kept right after the operation ends");
    let start = Instant::now();
    while scratch_held_bytes() > 0 && start.elapsed() < Duration::from_secs(10) {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(scratch_held_bytes(), 0, "released after the idle time");
    assert!(start.elapsed() >= Duration::from_millis(400), "not released straight away");
    // results do not depend on the pool
    let _op = ScratchGuard::begin();
    assert_eq!(booleans(), before);
}
