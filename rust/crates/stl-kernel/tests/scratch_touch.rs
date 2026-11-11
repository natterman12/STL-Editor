//! A solid dropped outside any operation leaves buffers in the pool; `touch_scratch` arms the
//! idle timer so they are still released. One test only: the pool is process-wide.

use std::time::{Duration, Instant};
use stl_kernel::{release_scratch, scratch_held_bytes, set_idle_timeout, touch_scratch, Solid};

#[test]
fn touch_arms_the_timer_after_a_drop_outside_a_guard() {
    set_idle_timeout(Duration::from_millis(500));
    let ball = Solid::sphere(20.0, 400);
    let cube = Solid::cube([25.0, 25.0, 25.0], true);
    let a = ball.difference(&cube);
    let b = ball.union(&cube);
    release_scratch(); // as at the end of a rebuild: pool empty, no timer armed
    assert_eq!(scratch_held_bytes(), 0);
    drop((a, b)); // dropped outside a guard: buffers return to the pool
    assert!(scratch_held_bytes() > 0, "dropping solids refills the pool");
    touch_scratch();
    let start = Instant::now();
    while scratch_held_bytes() > 0 && start.elapsed() < Duration::from_secs(10) {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(scratch_held_bytes(), 0, "released after the idle time");
}
