//! Self-test for the fake release server (M5 Task 0). Raw TCP, no HTTP client.

#[path = "support/fake_releases.rs"]
mod fake_releases;

use fake_releases::{FakeReleases, Faults};
use std::io::{Read, Write};
use std::net::TcpStream;

/// Sends a bare GET and returns (header block, body bytes). The server closes after each reply.
fn get(base: &str, path: &str) -> (String, Vec<u8>) {
    let addr = base.strip_prefix("http://").expect("base url");
    let mut s = TcpStream::connect(addr).expect("connect");
    write!(s, "GET {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n").unwrap();
    let mut raw = Vec::new();
    // A dropped connection ends the read early; that is the point of the drop test.
    let _ = s.read_to_end(&mut raw);
    let cut = raw.windows(4).position(|w| w == b"\r\n\r\n").expect("header end");
    (String::from_utf8_lossy(&raw[..cut]).into_owned(), raw[cut + 4..].to_vec())
}

fn content_length(head: &str) -> usize {
    head.lines()
        .find_map(|l| l.strip_prefix("Content-Length: "))
        .expect("Content-Length")
        .trim()
        .parse()
        .expect("Content-Length number")
}

fn sample_releases() -> serde_json::Value {
    serde_json::json!([{
        "tag_name": "v2.0.1",
        "html_url": "https://github.com/natterman12/STL-Editor/releases/tag/v2.0.1",
        "body": "Notes",
        "assets": [{
            "name": "STL-Editor-2-Windows.exe",
            "browser_download_url": "https://github.com/natterman12/STL-Editor/releases/download/v2.0.1/STL-Editor-2-Windows.exe",
            "size": 11
        }]
    }])
}

#[test]
fn fake_server_serves_json_and_assets() {
    let exe = b"hello world".to_vec();
    let assets = vec![("STL-Editor-2-Windows.exe".to_string(), exe.clone())];
    let base = FakeReleases::start(sample_releases(), assets, Faults::default()).base_url();

    let (head, body) = get(&base, "/releases");
    assert!(head.starts_with("HTTP/1.1 200"), "releases status: {head}");
    assert_eq!(content_length(&head), body.len());
    let parsed: serde_json::Value = serde_json::from_slice(&body).expect("releases JSON");
    assert_eq!(parsed[0]["tag_name"], "v2.0.1");
    assert_eq!(parsed[0]["assets"][0]["name"], "STL-Editor-2-Windows.exe");

    let (head, body) = get(&base, "/assets/STL-Editor-2-Windows.exe");
    assert!(head.starts_with("HTTP/1.1 200"), "asset status: {head}");
    assert_eq!(content_length(&head), exe.len());
    assert_eq!(body, exe);

    let (head, _) = get(&base, "/assets/missing.exe");
    assert!(head.starts_with("HTTP/1.1 404"), "missing asset status: {head}");
}

#[test]
fn fake_server_injects_faults() {
    let exe = b"hello world".to_vec();
    let assets = vec![("a.exe".to_string(), exe.clone())];

    let size = FakeReleases::start(serde_json::Value::Null, assets.clone(), Faults { wrong_size: true, ..Faults::default() });
    let (head, body) = get(&size.base_url(), "/assets/a.exe");
    assert_eq!(content_length(&head), body.len());
    assert!(body.len() > exe.len(), "wrong_size should serve extra bytes");

    let digest = FakeReleases::start(serde_json::Value::Null, assets.clone(), Faults { wrong_digest: true, ..Faults::default() });
    let (head, body) = get(&digest.base_url(), "/assets/a.exe");
    assert_eq!(content_length(&head), exe.len());
    assert_eq!(body.len(), exe.len(), "wrong_digest keeps the length");
    assert_ne!(body, exe, "wrong_digest should alter the bytes");

    let dropped = FakeReleases::start(serde_json::Value::Null, assets, Faults { drop_connection: true, ..Faults::default() });
    let (head, body) = get(&dropped.base_url(), "/assets/a.exe");
    assert_eq!(content_length(&head), exe.len());
    assert!(body.len() < exe.len(), "drop_connection should cut the body short");
}
