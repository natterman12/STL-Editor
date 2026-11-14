//! A local plain-HTTP stand-in for GitHub's releases API and asset downloads (M5 Task 0).
//!
//! Serves the releases JSON at `/releases` and each asset at `/assets/<name>`, on
//! `127.0.0.1` with an OS-chosen port. Std only: one accept thread, one thread per request.
//! Tests point the updater's base URL at [`FakeReleases::base_url`]; they never reach GitHub.

#![allow(dead_code)]

use std::io::{BufRead, BufReader, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

/// Failures to inject into asset downloads. All off by default.
#[derive(Default, Clone)]
pub struct Faults {
    /// Serve each asset with extra bytes appended (Content-Length matches the altered body).
    pub wrong_size: bool,
    /// Serve each asset with the same length but altered bytes (the digest no longer matches).
    pub wrong_digest: bool,
    /// Send the headers and half the body, then close the connection.
    pub drop_connection: bool,
}

pub struct FakeReleases {
    base: String,
}

struct Served {
    releases: String,
    assets: Vec<(String, Vec<u8>)>,
    faults: Faults,
}

impl FakeReleases {
    /// Starts the server. `releases` is returned as-is at `/releases` (GitHub's list format).
    pub fn start(releases: serde_json::Value, assets: Vec<(String, Vec<u8>)>, faults: Faults) -> FakeReleases {
        let listener = TcpListener::bind("127.0.0.1:0").expect("fake release server: bind 127.0.0.1:0");
        let addr = listener.local_addr().expect("fake release server: local address");
        let served = Arc::new(Served { releases: releases.to_string(), assets, faults });
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let served = Arc::clone(&served);
                thread::spawn(move || handle(stream, &served));
            }
        });
        FakeReleases { base: format!("http://{addr}") }
    }

    /// `http://127.0.0.1:<port>`, with no trailing slash.
    pub fn base_url(&self) -> String {
        self.base.clone()
    }
}

fn handle(stream: TcpStream, served: &Served) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut reader = BufReader::new(match stream.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    });
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() {
        return;
    }
    // Drain the headers; the body of a GET is ignored.
    loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) if line == "\r\n" || line == "\n" => break,
            Ok(_) => {}
        }
    }
    let path = request_line.split_whitespace().nth(1).unwrap_or("/").to_string();
    let mut out = stream;

    if path == "/releases" {
        respond(&mut out, 200, "application/json", served.releases.as_bytes(), false);
    } else if let Some(name) = path.strip_prefix("/assets/") {
        match served.assets.iter().find(|(n, _)| n == name) {
            Some((_, data)) => {
                let body = faulty(data, &served.faults);
                respond(&mut out, 200, "application/octet-stream", &body, served.faults.drop_connection);
            }
            None => respond(&mut out, 404, "text/plain", b"not found", false),
        }
    } else {
        respond(&mut out, 404, "text/plain", b"not found", false);
    }
    let _ = out.shutdown(Shutdown::Both);
}

/// The bytes actually served for an asset, with the faults applied.
fn faulty(data: &[u8], faults: &Faults) -> Vec<u8> {
    let mut body = data.to_vec();
    if faults.wrong_digest {
        if let Some(first) = body.first_mut() {
            *first ^= 0xFF;
        }
    }
    if faults.wrong_size {
        body.extend_from_slice(b"\0extra bytes the release did not have");
    }
    body
}

/// Writes a complete response. With `drop_after_half`, the headers announce the full length
/// but only half the body is sent before the caller closes the connection.
fn respond(out: &mut TcpStream, status: u16, content_type: &str, body: &[u8], drop_after_half: bool) {
    let reason = if status == 200 { "OK" } else { "Not Found" };
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = out.write_all(head.as_bytes());
    let sent = if drop_after_half { &body[..body.len() / 2] } else { body };
    let _ = out.write_all(sent);
    let _ = out.flush();
}
