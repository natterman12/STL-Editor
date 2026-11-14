//! Build date and commit for the title and About (milestone 5, Task 5).
//! `build.rs` records them; a build outside a git checkout says "unknown" for the commit.

/// The UTC date of the build, `yyyy-mm-dd`.
pub fn build_date() -> &'static str {
    env!("STL_BUILD_DATE")
}

/// The short git commit the build was made from.
pub fn commit() -> &'static str {
    env!("STL_BUILD_COMMIT")
}

/// The window title and the About heading: "STL Editor 2.0 (test build 2026-11-14 4fa1d90)".
pub fn title() -> String {
    format!("STL Editor 2.0 (test build {} {})", build_date(), commit())
}
