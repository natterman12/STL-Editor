//! The 2.x updater: check GitHub releases, download, swap the exe (a port of the parts of
//! `stleditor/updates.py` 2.0 needs; spec 2.4 and 2.7 decisions 1, 3 and 5).
//!
//! - [`newest_2x`] lists the releases (not `releases/latest`, which can be a 1.x release) and
//!   offers only `v2.*` tags that carry [`ASSET_NAME`], newer than the running version by
//!   semver (pre-releases included: `v2.0.1-beta.1`). Drafts are skipped.
//! - [`download`] fetches the asset over HTTPS from GitHub's hosts only ([`GITHUB_HOSTS`]; every
//!   redirect is checked too), checks its size and, when the release lists one, its `sha256:`
//!   digest, and only then puts the file at `to`. On any failure nothing is left there.
//! - [`swap`] renames the running exe to [`OLD_NAME`] and moves the new one in; [`finish_swap`]
//!   (after the new copy has drawn its first frame) deletes the old one.
//!
//! The update tests use a local plain-HTTP server. `http://127.0.0.1` download URLs are accepted
//! only while the environment variable [`TEST_HOST_ENV`] is `127.0.0.1`; nothing else is ever
//! accepted outside GitHub.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::net::{NetError, https_get, https_get_checked};

pub const RELEASES_API: &str = "https://api.github.com/repos/natterman12/STL-Editor/releases";
pub const ASSET_NAME: &str = "STL-Editor-2-Windows.exe";
/// Where [`swap`] keeps the previous exe until the new one has started.
pub const OLD_NAME: &str = "stl-editor-2.old.exe";
/// The hosts an update may come from: the asset link (`github.com`) and GitHub's download
/// servers it redirects to.
pub const GITHUB_HOSTS: [&str; 3] = ["github.com", "objects.githubusercontent.com", "release-assets.githubusercontent.com"];
/// Test switch: `127.0.0.1` lets [`download`] fetch from the local fake release server.
pub const TEST_HOST_ENV: &str = "STLEDITOR2_UPDATE_TEST_HOST";

/// 1.x keeps the first 1500 characters of the release notes.
const NOTES_MAX: usize = 1500;
/// No 2.0 exe is anywhere near this; a bigger `size` is not believed.
const MAX_EXE: u64 = 1024 * 1024 * 1024;

/// 1.x's message when the bytes don't match the release's checksum.
pub const CHECKSUM_FAILED: &str = "The download didn't match its checksum; nothing was changed. Try again later.";
pub const WRONG_SIZE: &str = "The download was the wrong size; nothing was changed. Try again later.";
pub const NOT_GITHUB: &str = "The update isn't coming from GitHub, so it wasn't downloaded. Nothing was changed.";

#[derive(Clone, Debug)]
pub struct Release {
    pub tag: String,
    pub version: String,
    pub url: String,
    pub size: u64,
    pub digest: Option<String>,
    pub notes: String,
}

/// `v2.*` tags with [`ASSET_NAME`], newer than `current`. `Ok(None)`: nothing newer.
/// A failure reads like 1.x's: "Couldn't reach GitHub (URLError)."
pub fn newest_2x(api: &str, current: &str) -> Result<Option<Release>, String> {
    let cur = Semver::parse(current).unwrap_or_default();
    let body = https_get(api, Duration::from_secs(8)).map_err(|e| reach_error(&e))?;
    let list: Value = serde_json::from_slice(&body).map_err(|_| "Couldn't reach GitHub (JSONDecodeError).".to_string())?;
    let Some(list) = list.as_array() else {
        return Err("Couldn't reach GitHub (JSONDecodeError).".into());
    };
    let mut best: Option<(Semver, Release)> = None;
    for r in list {
        if r.get("draft").and_then(Value::as_bool).unwrap_or(false) {
            continue;
        }
        let tag = r.get("tag_name").and_then(Value::as_str).unwrap_or("");
        if !tag.starts_with("v2.") {
            continue;
        }
        let Some(ver) = Semver::parse(tag) else { continue };
        if ver <= cur || best.as_ref().is_some_and(|(b, _)| ver <= *b) {
            continue;
        }
        let assets = r.get("assets").and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[]);
        let Some(asset) = assets.iter().find(|a| a.get("name").and_then(Value::as_str) == Some(ASSET_NAME)) else {
            continue;
        };
        let Some(url) = asset.get("browser_download_url").and_then(Value::as_str) else { continue };
        let notes: String = r.get("body").and_then(Value::as_str).unwrap_or("").chars().take(NOTES_MAX).collect();
        let rel = Release {
            tag: tag.to_string(),
            version: tag[1..].to_string(),
            url: url.to_string(),
            size: asset.get("size").and_then(Value::as_u64).unwrap_or(0),
            digest: asset.get("digest").and_then(Value::as_str).map(str::to_string),
            notes,
        };
        best = Some((ver, rel));
    }
    Ok(best.map(|(_, r)| r))
}

/// Is `latest` newer than `current` (semver, pre-releases below their release)?
pub fn is_newer(latest: &str, current: &str) -> bool {
    match (Semver::parse(latest), Semver::parse(current)) {
        (Some(l), Some(c)) => l > c,
        (Some(_), None) => true,
        _ => false,
    }
}

/// Host, size and digest checks. The bytes are written to `to` only when all pass.
pub fn download(rel: &Release, to: &Path) -> Result<(), String> {
    check_host(&rel.url)?;
    let want = match rel.digest.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(d) => match d.split_once(':') {
            Some((alg, hex)) if alg.eq_ignore_ascii_case("sha256") => Some(hex.trim().to_ascii_lowercase()),
            // a digest we can't check is not trusted
            _ => return Err(CHECKSUM_FAILED.into()),
        },
    };
    if rel.size == 0 || rel.size > MAX_EXE {
        return Err(WRONG_SIZE.into());
    }
    let limit = (rel.size + 1024 * 1024) as usize;
    let bytes = https_get_checked(&rel.url, Duration::from_secs(60), limit, &check_host).map_err(|e| match e {
        NetError::Refused(m) => m,
        e => format!("Couldn't download the update ({e}); nothing was changed. Try again later."),
    })?;
    if bytes.len() as u64 != rel.size {
        return Err(WRONG_SIZE.into());
    }
    if let Some(want) = want {
        let got: String = Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect();
        if got != want {
            return Err(CHECKSUM_FAILED.into());
        }
    }
    let mut part = to.as_os_str().to_owned();
    part.push(".part");
    let part = PathBuf::from(part);
    let written = fs::write(&part, &bytes).and_then(|()| fs::rename(&part, to));
    if let Err(e) = written {
        let _ = fs::remove_file(&part);
        return Err(format!("Couldn't save the update ({e}); nothing was changed."));
    }
    Ok(())
}

/// Running exe -> [`OLD_NAME`], new exe -> running. On failure the running exe is put back.
pub fn swap(new_exe: &Path, running_exe: &Path) -> Result<(), String> {
    if !new_exe.is_file() {
        return Err(format!("The new version isn't there ({}); nothing was changed.", new_exe.display()));
    }
    let old = old_path(running_exe);
    if old.exists() {
        fs::remove_file(&old).map_err(|e| format!("Couldn't remove the previous copy ({}): {e}. Nothing was changed.", old.display()))?;
    }
    fs::rename(running_exe, &old).map_err(|e| format!("Couldn't move STL Editor aside to update it ({e}); nothing was changed."))?;
    if fs::rename(new_exe, running_exe).is_ok() {
        return Ok(());
    }
    // A different drive: copy instead.
    match fs::copy(new_exe, running_exe) {
        Ok(_) => {
            let _ = fs::remove_file(new_exe);
            Ok(())
        }
        Err(e) => {
            let _ = fs::remove_file(running_exe);
            match fs::rename(&old, running_exe) {
                Ok(()) => Err(format!("Couldn't put the new version in place ({e}); nothing was changed.")),
                Err(back) => Err(rollback_failed(&e, &back, &old, running_exe)),
            }
        }
    }
}

/// First frame drawn: delete the `.old.exe`. Nothing there is fine.
pub fn finish_swap(running_exe: &Path) {
    let _ = fs::remove_file(old_path(running_exe));
}

/// Where the previous exe is kept after a [`swap`].
pub fn old_path(running_exe: &Path) -> PathBuf {
    running_exe.with_file_name(OLD_NAME)
}

/// Where the app downloads the new exe for `tag`: beside the running one, so the swap is a
/// rename on the same drive; one file per version, so a cancelled download never shares a file
/// with the next one.
pub fn new_path(running_exe: &Path, tag: &str) -> PathBuf {
    let tag: String = tag.chars().map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' { c } else { '_' }).collect();
    running_exe.with_file_name(format!("stl-editor-2.{tag}.new.exe"))
}

/// The swap failed half way and the old exe couldn't be put back: say where it is.
pub fn rollback_failed(e: &std::io::Error, back: &std::io::Error, old: &Path, running_exe: &Path) -> String {
    let name = running_exe.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    format!(
        "Couldn't put the new version in place ({e}), and couldn't put the old one back ({back}).          Your copy of STL Editor is still there as {}: rename it to {name} to use it again.",
        old.display()
    )
}

/// 1.x's "Couldn't reach GitHub (<Python error type>)."
fn reach_error(e: &NetError) -> String {
    let kind = match e {
        NetError::Status(..) => "HTTPError",
        NetError::Timeout => "TimeoutError",
        _ => "URLError",
    };
    format!("Couldn't reach GitHub ({kind}).")
}

/// HTTPS to one of [`GITHUB_HOSTS`] (default port, no user info), or `http://127.0.0.1` while
/// the test switch is on. Asked again for every redirect.
pub fn check_host(url: &str) -> Result<(), String> {
    let refuse = || Err(NOT_GITHUB.to_string());
    let Some((scheme, rest)) = url.split_once("://") else { return refuse() };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.contains('@') || authority.is_empty() {
        return refuse();
    }
    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p)) => (h, Some(p)),
        None => (authority, None),
    };
    let host = host.to_ascii_lowercase();
    match scheme.to_ascii_lowercase().as_str() {
        "https" if GITHUB_HOSTS.contains(&host.as_str()) && matches!(port, None | Some("443")) => Ok(()),
        "http" if host == "127.0.0.1" && std::env::var(TEST_HOST_ENV).is_ok_and(|v| v == "127.0.0.1") => Ok(()),
        _ => refuse(),
    }
}

/// A semantic version: `v2.0.1`, `2.0.1-beta.1` (`+build` is ignored). Missing minor and patch
/// numbers are 0.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Semver {
    core: [u64; 3],
    pre: Vec<Ident>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Ident {
    // numeric identifiers sort below alphanumeric ones
    Num(u64),
    Alpha(String),
}

impl Semver {
    fn parse(s: &str) -> Option<Semver> {
        let s = s.trim();
        let s = s.strip_prefix(['v', 'V']).unwrap_or(s);
        let s = s.split('+').next().unwrap_or("");
        let (core, pre) = match s.split_once('-') {
            Some((c, p)) => (c, Some(p)),
            None => (s, None),
        };
        let mut nums = [0u64; 3];
        let parts: Vec<&str> = core.split('.').collect();
        if parts.is_empty() || parts.len() > 3 {
            return None;
        }
        for (i, p) in parts.iter().enumerate() {
            if p.is_empty() || !p.bytes().all(|c| c.is_ascii_digit()) {
                return None;
            }
            nums[i] = p.parse().ok()?;
        }
        let pre = match pre {
            None => Vec::new(),
            Some(p) => {
                let mut v = Vec::new();
                for id in p.split('.') {
                    if id.is_empty() {
                        return None;
                    }
                    v.push(if id.bytes().all(|c| c.is_ascii_digit()) { Ident::Num(id.parse().ok()?) } else { Ident::Alpha(id.to_string()) });
                }
                v
            }
        };
        Some(Semver { core: nums, pre })
    }
}

impl Ord for Semver {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        self.core.cmp(&other.core).then_with(|| match (self.pre.is_empty(), other.pre.is_empty()) {
            (true, true) => Ordering::Equal,
            (true, false) => Ordering::Greater,
            (false, true) => Ordering::Less,
            // element by element; a longer list wins when one is a prefix of the other
            (false, false) => self.pre.cmp(&other.pre),
        })
    }
}

impl PartialOrd for Semver {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semver_order() {
        let order = ["1.9.9", "2.0.0-alpha", "2.0.0-alpha.1", "2.0.0-alpha.beta", "2.0.0-beta", "2.0.0-beta.2", "2.0.0-beta.11", "2.0.0-rc.1", "2.0.0", "2.0.1-beta.1", "2.0.1", "2.1.0", "10.0.0"];
        for w in order.windows(2) {
            assert!(Semver::parse(w[0]).unwrap() < Semver::parse(w[1]).unwrap(), "{} < {}", w[0], w[1]);
        }
        assert_eq!(Semver::parse("v2.0.1+build.5"), Semver::parse("2.0.1"));
        assert!(Semver::parse("v2.x").is_none());
        assert!(is_newer("2.0.0", "2.0.0-dev"));
        assert!(!is_newer("2.0.0-beta.1", "2.0.0-dev"));
    }
}
