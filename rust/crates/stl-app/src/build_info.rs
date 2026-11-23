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

/// The version in `rust/Cargo.toml`: "2.0.0-dev" between releases, "2.0.0" or "2.0.0-beta.1" in a
/// released build.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// A build made between releases (the version ends in `-dev`), not one that was published.
pub fn is_test_version(version: &str) -> bool {
    version.ends_with("-dev")
}

/// The window title and the About heading: "STL Editor 2.0 (test build 2026-11-14 4fa1d90)" between
/// releases, "STL Editor 2.0.0" in a released build.
pub fn title() -> String {
    title_for(version())
}

/// [`title`] for a given version.
pub fn title_for(version: &str) -> String {
    if is_test_version(version) {
        format!("STL Editor 2.0 (test build {} {})", build_date(), commit())
    } else {
        format!("STL Editor {version}")
    }
}

/// About's first line on the desktop: a test build says so; a released one says when it was built
/// and from which commit, which is what a bug report needs.
pub fn version_line_for(version: &str) -> String {
    let (date, commit) = (build_date(), commit());
    if is_test_version(version) {
        format!("Version {version} (test build {date} {commit})")
    } else {
        format!("Version {version} (built {date}, {commit})")
    }
}
