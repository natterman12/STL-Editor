//! The 2.x updater (M5 Task 4): which release is offered, the download checks (host, size,
//! digest) and the exe swap. Every network test runs against the local fake release server
//! (`support/fake_releases.rs`), never GitHub. Every test sets `STLEDITOR2_CONFIG_DIR` to a temp
//! folder; tests that change the environment are serialised by `ENV`.

#[path = "support/fake_releases.rs"]
mod fake_releases;

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use fake_releases::{FakeReleases, Faults};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use stl_platform::update::{check_host, download, finish_swap, newest_2x, rollback_failed, swap, Release, ASSET_NAME, NOT_GITHUB};

static ENV: Mutex<()> = Mutex::new(());

/// The env lock, with a temp config folder set and the test-host switch on or off.
fn env(test_host: bool) -> (MutexGuard<'static, ()>, PathBuf) {
    let g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let d = temp("config");
    // SAFETY: serialised by ENV; no other thread in this binary reads the environment meanwhile.
    unsafe {
        std::env::set_var("STLEDITOR2_CONFIG_DIR", &d);
        if test_host {
            std::env::set_var("STLEDITOR2_UPDATE_TEST_HOST", "127.0.0.1");
        } else {
            std::env::remove_var("STLEDITOR2_UPDATE_TEST_HOST");
        }
    }
    (g, d)
}

fn temp(name: &str) -> PathBuf {
    static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let d = std::env::temp_dir().join(format!("stle2-update-{name}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn sha(data: &[u8]) -> String {
    Sha256::digest(data).iter().map(|b| format!("{b:02x}")).collect()
}

/// One release in GitHub's list format.
fn rel(tag: &str, assets: &[(&str, &str, u64, Option<&str>)]) -> Value {
    json!({
        "tag_name": tag,
        "name": tag,
        "draft": false,
        "prerelease": tag.contains('-'),
        "html_url": format!("https://github.com/natterman12/STL-Editor/releases/tag/{tag}"),
        "body": format!("Notes for {tag}"),
        "assets": assets.iter().map(|(name, url, size, digest)| {
            let mut a = json!({ "name": name, "browser_download_url": url, "size": size });
            if let Some(d) = digest {
                a["digest"] = json!(d);
            }
            a
        }).collect::<Vec<_>>(),
    })
}

fn asset_url(tag: &str) -> String {
    format!("https://github.com/natterman12/STL-Editor/releases/download/{tag}/{ASSET_NAME}")
}

/// A local server that answers every request with a redirect to `target`. Returns its URL.
fn redirector(target: String) -> String {
    use std::io::{BufRead, BufReader, Write};
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/{ASSET_NAME}", l.local_addr().unwrap());
    std::thread::spawn(move || {
        for s in l.incoming() {
            let Ok(mut s) = s else { continue };
            let mut r = BufReader::new(s.try_clone().unwrap());
            let mut line = String::new();
            while r.read_line(&mut line).is_ok_and(|n| n > 0) && line != "\r\n" {
                line.clear();
            }
            let _ = write!(s, "HTTP/1.1 302 Found\r\nLocation: {target}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        }
    });
    url
}

fn names(dir: &Path) -> Vec<String> {
    let mut v: Vec<_> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    v.sort();
    v
}

#[test]
fn finds_newer_2x() {
    let (_g, _d) = env(false);
    let long = "x".repeat(4000);
    let mut beta = rel("v2.0.2-beta.1", &[(ASSET_NAME, &asset_url("v2.0.2-beta.1"), 1234, Some("sha256:abcd"))]);
    beta["body"] = json!(long);
    let releases = json!([
        rel("v2.0.1", &[(ASSET_NAME, &asset_url("v2.0.1"), 999, None)]),
        beta,
        rel("v1.14.0", &[("STL-Editor-Windows-portable.zip", "https://github.com/x/y.zip", 5, None)]),
    ]);
    let srv = FakeReleases::start(releases, vec![], Faults::default());
    let api = format!("{}/releases", srv.base_url());

    // the newest by semver, not the first listed: 2.0.2-beta.1 > 2.0.1
    let r = newest_2x(&api, "2.0.0").unwrap().expect("a newer 2.x release");
    assert_eq!(r.tag, "v2.0.2-beta.1");
    assert_eq!(r.version, "2.0.2-beta.1");
    assert_eq!(r.url, asset_url("v2.0.2-beta.1"));
    assert_eq!(r.size, 1234);
    assert_eq!(r.digest.as_deref(), Some("sha256:abcd"));
    assert_eq!(r.notes.chars().count(), 1500, "notes are cut at 1.x's 1500 characters");

    // pre-release order: numeric identifiers compare as numbers, a release beats its pre-releases
    let srv = FakeReleases::start(
        json!([
            rel("v2.1.0-beta.2", &[(ASSET_NAME, &asset_url("v2.1.0-beta.2"), 1, None)]),
            rel("v2.1.0-beta.10", &[(ASSET_NAME, &asset_url("v2.1.0-beta.10"), 1, None)]),
        ]),
        vec![],
        Faults::default(),
    );
    let api = format!("{}/releases", srv.base_url());
    assert_eq!(newest_2x(&api, "2.1.0-beta.2").unwrap().unwrap().tag, "v2.1.0-beta.10");
    assert!(newest_2x(&api, "2.1.0").unwrap().is_none(), "2.1.0 is newer than its betas");
    assert!(newest_2x(&api, "2.1.0-beta.10").unwrap().is_none());
}

#[test]
fn ignores_1x_releases() {
    let (_g, _d) = env(false);
    let mut draft = rel("v2.3.0", &[(ASSET_NAME, &asset_url("v2.3.0"), 1, None)]);
    draft["draft"] = json!(true);
    let releases = json!([
        // 1.x, even with an asset of the 2.0 name
        rel("v1.20.0", &[(ASSET_NAME, &asset_url("v1.20.0"), 1, None), ("STL-Editor-Windows-portable.zip", "https://github.com/a.zip", 1, None)]),
        // a 2.x tag without the 2.0 Windows asset
        rel("v2.0.5", &[("STL-Editor-Windows-portable.zip", "https://github.com/a.zip", 1, None)]),
        // not "v2."
        rel("v20.0.0", &[(ASSET_NAME, &asset_url("v20.0.0"), 1, None)]),
        rel("2.9.0", &[(ASSET_NAME, &asset_url("2.9.0"), 1, None)]),
        draft,
    ]);
    let srv = FakeReleases::start(releases, vec![], Faults::default());
    let api = format!("{}/releases", srv.base_url());
    assert!(newest_2x(&api, "2.0.0-dev").unwrap().is_none());
}

#[test]
fn ignores_older_releases() {
    let (_g, _d) = env(false);
    let releases = json!([
        rel("v2.0.0", &[(ASSET_NAME, &asset_url("v2.0.0"), 1, None)]),
        rel("v2.0.0-beta.3", &[(ASSET_NAME, &asset_url("v2.0.0-beta.3"), 1, None)]),
    ]);
    let srv = FakeReleases::start(releases, vec![], Faults::default());
    let api = format!("{}/releases", srv.base_url());
    assert!(newest_2x(&api, "2.0.0").unwrap().is_none(), "the same version is not newer");
    assert!(newest_2x(&api, "2.0.1").unwrap().is_none());
    // 2.0.0-dev: "beta" sorts before "dev", so only v2.0.0 itself is newer
    assert_eq!(newest_2x(&api, "2.0.0-dev").unwrap().unwrap().tag, "v2.0.0");
}

#[test]
fn unreachable_api_is_an_error_in_plain_words() {
    let (_g, _d) = env(false);
    // a port nothing listens on
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let e = newest_2x(&format!("http://127.0.0.1:{port}/releases"), "2.0.0").unwrap_err();
    assert!(e.starts_with("Couldn't reach GitHub ("), "{e}");
}

fn release_for(base: &str, data: &[u8], digest: Option<String>) -> Release {
    Release {
        tag: "v2.0.1".into(),
        version: "2.0.1".into(),
        url: format!("{base}/assets/{ASSET_NAME}"),
        size: data.len() as u64,
        digest,
        notes: String::new(),
    }
}

#[test]
fn download_checks_host_size_digest() {
    let data: Vec<u8> = (0..200_000u32).map(|i| (i * 7 % 251) as u8).collect();
    let good = Some(format!("sha256:{}", sha(&data)));

    // host: only https to github.com / objects.githubusercontent.com; refused before any request
    {
        let (_g, _d) = env(false);
        let dir = temp("host");
        let to = dir.join("stl-editor-2.new.exe");
        for url in [
            "https://github.com.evil.example/a.exe",
            "https://evilgithub.com/a.exe",
            "http://github.com/a.exe",
            "https://example.com/a.exe",
            "ftp://github.com/a.exe",
            "https://user@evil.example/github.com/a.exe",
        ] {
            let mut r = release_for("", &data, good.clone());
            r.url = url.into();
            let e = download(&r, &to).unwrap_err();
            assert!(e.contains("GitHub"), "{url}: {e}");
            assert!(!to.exists());
        }
        // the fake server is refused while the test switch is off
        let srv = FakeReleases::start(json!([]), vec![(ASSET_NAME.into(), data.clone())], Faults::default());
        assert!(download(&release_for(&srv.base_url(), &data, good.clone()), &to).is_err());
        assert!(!to.exists());
    }

    let (_g, _d) = env(true);
    let dir = temp("download");
    let to = dir.join("stl-editor-2.new.exe");

    // a good download, digest listed
    let srv = FakeReleases::start(json!([]), vec![(ASSET_NAME.into(), data.clone())], Faults::default());
    download(&release_for(&srv.base_url(), &data, good.clone()), &to).unwrap();
    assert_eq!(std::fs::read(&to).unwrap(), data);
    std::fs::remove_file(&to).unwrap();
    // upper-case digest is the same digest
    download(&release_for(&srv.base_url(), &data, good.clone().map(|d| d.to_uppercase().replacen("SHA256", "sha256", 1))), &to).unwrap();
    std::fs::remove_file(&to).unwrap();

    // wrong size
    let srv = FakeReleases::start(json!([]), vec![(ASSET_NAME.into(), data.clone())], Faults { wrong_size: true, ..Faults::default() });
    let e = download(&release_for(&srv.base_url(), &data, good.clone()), &to).unwrap_err();
    assert!(e.contains("wrong size"), "{e}");
    assert!(!to.exists());

    // wrong digest
    let srv = FakeReleases::start(json!([]), vec![(ASSET_NAME.into(), data.clone())], Faults { wrong_digest: true, ..Faults::default() });
    let e = download(&release_for(&srv.base_url(), &data, good.clone()), &to).unwrap_err();
    assert_eq!(e, "The download didn't match its checksum; nothing was changed. Try again later.");
    assert!(!to.exists());
    // no digest listed: only the size can be checked (1.x does the same)
    download(&release_for(&srv.base_url(), &data, None), &to).unwrap();
    std::fs::remove_file(&to).unwrap();
    // a digest that isn't sha256 can't be checked, so it is refused
    let e = download(&release_for(&srv.base_url(), &data, Some("md5:0123".into())), &to).unwrap_err();
    assert!(e.contains("checksum"), "{e}");
    assert!(!to.exists());

    // cut off half way
    let srv = FakeReleases::start(json!([]), vec![(ASSET_NAME.into(), data.clone())], Faults { drop_connection: true, ..Faults::default() });
    let e = download(&release_for(&srv.base_url(), &data, good), &to).unwrap_err();
    assert!(e.contains("nothing was changed"), "{e}");
    assert!(!to.exists());
    assert!(names(&dir).is_empty(), "no partial files left: {:?}", names(&dir));
}

#[test]
fn redirects_checked_every_hop() {
    let (_g, _d) = env(true);
    let data: Vec<u8> = (0..20_000u32).map(|i| (i % 249) as u8).collect();
    let digest = Some(format!("sha256:{}", sha(&data)));
    let srv = FakeReleases::start(json!([]), vec![(ASSET_NAME.into(), data.clone())], Faults::default());
    let dir = temp("redirect");
    let running = dir.join("stl-editor-2.exe");
    std::fs::write(&running, b"installed").unwrap();
    let new = dir.join("stl-editor-2.v2.0.1.new.exe");

    // GitHub's hosts pass, https only; anything else is refused
    for ok in ["https://github.com/a", "https://objects.githubusercontent.com/a", "https://release-assets.githubusercontent.com/a?x=1"] {
        assert!(check_host(ok).is_ok(), "{ok}");
    }
    for bad in ["http://release-assets.githubusercontent.com/a", "https://evil.example/a", "https://github.com:8443/a", "http://localhost:1/a"] {
        assert!(check_host(bad).is_err(), "{bad}");
    }

    // a redirect to a foreign host is refused before it is fetched; the exe is untouched
    let mut r = release_for("", &data, digest.clone());
    r.url = redirector("https://evil.example/stl-editor-2.exe".into());
    assert_eq!(download(&r, &new).unwrap_err(), NOT_GITHUB);
    // a plain-HTTP redirect to another loopback name is refused too (only 127.0.0.1 in tests)
    r.url = redirector(format!("http://localhost:{}/assets/{ASSET_NAME}", srv.base_url().rsplit(':').next().unwrap()));
    assert_eq!(download(&r, &new).unwrap_err(), NOT_GITHUB);
    assert_eq!(std::fs::read(&running).unwrap(), b"installed");
    assert_eq!(names(&dir), vec!["stl-editor-2.exe".to_string()]);

    // an allowed redirect is followed
    r.url = redirector(format!("{}/assets/{ASSET_NAME}", srv.base_url()));
    download(&r, &new).unwrap();
    assert_eq!(std::fs::read(&new).unwrap(), data);
}

#[test]
fn rollback_failure_names_old_exe() {
    let (_g, _d) = env(false);
    let e = std::io::Error::other("disk full");
    let dir = Path::new("Apps");
    let old = dir.join("stl-editor-2.old.exe");
    let m = rollback_failed(&e, &std::io::Error::other("access denied"), &old, &dir.join("stl-editor-2.exe"));
    assert!(m.contains(&old.display().to_string()) && m.contains("rename it to stl-editor-2.exe"), "{m}");
}

#[test]
fn failed_download_leaves_exe() {
    let (_g, _d) = env(true);
    let data: Vec<u8> = (0..100_000u32).map(|i| (i % 253) as u8).collect();
    let digest = Some(format!("sha256:{}", sha(&data)));
    for faults in [
        Faults { wrong_size: true, ..Faults::default() },
        Faults { wrong_digest: true, ..Faults::default() },
        Faults { drop_connection: true, ..Faults::default() },
    ] {
        let dir = temp("leave");
        let running = dir.join("stl-editor-2.exe");
        let installed = b"the installed exe, byte for byte".to_vec();
        std::fs::write(&running, &installed).unwrap();
        let new = dir.join("stl-editor-2.new.exe");
        let srv = FakeReleases::start(json!([]), vec![(ASSET_NAME.into(), data.clone())], faults);
        // the app swaps only after a good download, and this one fails
        assert!(download(&release_for(&srv.base_url(), &data, digest.clone()), &new).is_err());
        assert_eq!(std::fs::read(&running).unwrap(), installed);
        assert_eq!(names(&dir), vec!["stl-editor-2.exe".to_string()]);
    }
}

#[test]
fn swap_keeps_old_until_first_frame() {
    let (_g, _d) = env(false);
    let dir = temp("swap");
    let running = dir.join("stl-editor-2.exe");
    let new = dir.join("stl-editor-2.new.exe");
    let old = dir.join("stl-editor-2.old.exe");
    std::fs::write(&running, b"OLD").unwrap();
    std::fs::write(&new, b"NEW").unwrap();
    std::fs::write(&old, b"an even older one, left from a swap whose new copy never started").unwrap();

    swap(&new, &running).unwrap();
    assert_eq!(std::fs::read(&running).unwrap(), b"NEW");
    assert_eq!(std::fs::read(&old).unwrap(), b"OLD", "the previous exe is kept until the new one draws");
    assert!(!new.exists());

    finish_swap(&running);
    assert!(!old.exists());
    assert_eq!(std::fs::read(&running).unwrap(), b"NEW");
    finish_swap(&running); // nothing to remove: no error, no change
    assert_eq!(names(&dir), vec!["stl-editor-2.exe".to_string()]);

    // a missing new exe: an error, and the running exe stays where it was
    let e = swap(&dir.join("missing.exe"), &running).unwrap_err();
    assert!(!e.is_empty());
    assert_eq!(std::fs::read(&running).unwrap(), b"NEW");
    assert_eq!(names(&dir), vec!["stl-editor-2.exe".to_string()]);
}
