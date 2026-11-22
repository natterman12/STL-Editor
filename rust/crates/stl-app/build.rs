//! Records the build date (UTC, yyyy-mm-dd) and the short git commit for the window title and
//! About, as `STL_BUILD_DATE` and `STL_BUILD_COMMIT`. Standard library only, so the date is UTC
//! (the standard library cannot ask for the local time zone).

use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

/// Days since 1970-01-01 to (year, month, day), proleptic Gregorian (Howard Hinnant's algorithm).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8(out.stdout).ok()?.trim().to_owned();
    (!s.is_empty()).then_some(s)
}

/// Windows: the exe carries `assets/icon/icon.ico` (drawn by `assets/icon/make_icon.py`), so
/// Explorer, the taskbar and shortcuts show it. This needs the resource compiler of the
/// toolchain in use. When it is missing the build goes on with a warning and the exe has no
/// icon, unless `STL_REQUIRE_ICON` is set (CI's release build), where that fails the build.
#[cfg(windows)]
fn exe_icon() {
    println!("cargo:rerun-if-changed=assets/icon/icon.ico");
    println!("cargo:rerun-if-env-changed=STL_REQUIRE_ICON");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return; // the web build, made on Windows
    }
    let mut res = winresource::WindowsResource::new();
    res.set_icon("assets/icon/icon.ico");
    res.set("ProductName", "STL Editor");
    res.set("FileDescription", "STL Editor");
    if let Err(e) = res.compile() {
        if std::env::var_os("STL_REQUIRE_ICON").is_some() {
            panic!("could not put the icon in the exe: {e}");
        }
        println!("cargo:warning=the exe has no icon: {e}");
    }
}

fn main() {
    #[cfg(windows)]
    exe_icon();

    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (y, m, d) = civil_from_days((secs / 86_400) as i64);
    println!("cargo:rustc-env=STL_BUILD_DATE={y:04}-{m:02}-{d:02}");

    let commit = git(&["rev-parse", "--short", "HEAD"]).unwrap_or_else(|| "unknown".into());
    println!("cargo:rustc-env=STL_BUILD_COMMIT={commit}");

    // Re-run when the sources or the checked-out commit change. Only paths that exist are
    // named: cargo treats a missing one as always changed and would rebuild every time.
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src");
    let manifest = std::env::var_os("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .unwrap_or_default();
    let git_path = |name: &str| git(&["rev-parse", "--git-path", name]).map(|p| manifest.join(p));
    let watch = |p: Option<PathBuf>| match p {
        Some(p) if p.exists() => {
            println!("cargo:rerun-if-changed={}", p.display());
            true
        }
        _ => false,
    };
    watch(git_path("HEAD"));
    watch(git_path("packed-refs"));
    if let Some(r) = git(&["symbolic-ref", "-q", "HEAD"]) {
        // A branch's ref file appears the first time it is committed to after packing; until
        // then its folder changes instead.
        if !watch(git_path(&r))
            && let Some(dir) = git_path(&r).and_then(|p| p.parent().map(PathBuf::from))
        {
            watch(Some(dir));
        }
    }
}
