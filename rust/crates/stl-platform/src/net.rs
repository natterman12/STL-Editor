//! A small HTTPS GET client for the app updater.
//!
//! TLS is `rustls` with the RustCrypto provider (`rustls-rustcrypto`) and Mozilla's
//! root certificates (`webpki-roots`). `rustls` is built without its default features,
//! so neither `ring` nor `aws-lc-rs` is linked: both need a C toolchain, which the
//! pure-Rust check rejects. RustCrypto's primitives are ordinary Rust.
//!
//! `http://127.0.0.1` (and `localhost`) is accepted so the update tests can use
//! `dev/parity/fins_update_server.py`. Every other URL has to be `https`.

use std::io::{ErrorKind, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};

const MAX_BODY: usize = 32 * 1024 * 1024;
const MAX_HEADERS: usize = 64 * 1024;

#[derive(Debug)]
pub enum NetError {
    Url(String),
    Dns(String),
    Connect(String),
    Tls(String),
    Protocol(String),
    Status(u16, String),
    Timeout,
    Io(String),
    /// The caller's URL check (see [`https_get_checked`]) refused this URL or a redirect to it.
    Refused(String),
}

impl std::fmt::Display for NetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NetError::Url(s) | NetError::Dns(s) | NetError::Connect(s) | NetError::Tls(s) | NetError::Protocol(s) | NetError::Io(s) | NetError::Refused(s) => {
                write!(f, "{s}")
            }
            NetError::Status(code, reason) => write!(f, "HTTP {code} {reason}"),
            NetError::Timeout => write!(f, "timed out"),
        }
    }
}

impl std::error::Error for NetError {}

struct Url {
    https: bool,
    host: String,
    port: u16,
    path: String,
}

/// Fetch `url` and return the response body. Redirects (up to five) are followed.
/// `timeout` bounds the connection and each read. Bodies over 32 MB are refused.
pub fn https_get(url: &str, timeout: Duration) -> Result<Vec<u8>, NetError> {
    https_get_limited(url, timeout, MAX_BODY)
}

/// [`https_get`] with its own body limit (the app updater downloads a whole exe).
pub fn https_get_limited(url: &str, timeout: Duration, max_body: usize) -> Result<Vec<u8>, NetError> {
    https_get_checked(url, timeout, max_body, &|_| Ok(()))
}

/// [`https_get_limited`] that asks `allow` about every URL before connecting to it: the first
/// one and each redirect. A refusal ends the request with [`NetError::Refused`].
pub fn https_get_checked(url: &str, timeout: Duration, max_body: usize, allow: &dyn Fn(&str) -> Result<(), String>) -> Result<Vec<u8>, NetError> {
    let mut current = url.to_string();
    for _ in 0..5 {
        allow(&current).map_err(NetError::Refused)?;
        let parsed = parse_url(&current)?;
        let (code, headers, body) = exchange(&parsed, timeout, max_body)?;
        if matches!(code, 301 | 302 | 303 | 307 | 308) {
            let loc = header_value(&headers, "location")
                .ok_or_else(|| NetError::Protocol("redirect with no location".into()))?;
            current = join_url(&parsed, loc.trim())?;
            continue;
        }
        if code != 200 {
            let n = body.len().min(200);
            return Err(NetError::Status(code, String::from_utf8_lossy(&body[..n]).into_owned()));
        }
        return Ok(body);
    }
    Err(NetError::Protocol("too many redirects".into()))
}

fn parse_url(url: &str) -> Result<Url, NetError> {
    let (scheme, rest) = url.split_once("://").ok_or_else(|| NetError::Url(format!("not a url: {url}")))?;
    let https = match scheme.to_ascii_lowercase().as_str() {
        "https" => true,
        "http" => false,
        other => return Err(NetError::Url(format!("unsupported scheme: {other}"))),
    };
    let (authority, path) = match rest.split_once('/') {
        Some((a, p)) => (a, format!("/{p}")),
        None => (rest, "/".to_string()),
    };
    let path = path.split('#').next().unwrap_or("/").to_string();
    if authority.is_empty() {
        return Err(NetError::Url("missing host".into()));
    }
    let (host, port) = if authority.starts_with('[') {
        return Err(NetError::Url("ipv6 urls are not supported".into()));
    } else if let Some((h, p)) = authority.rsplit_once(':') {
        if !p.is_empty() && p.bytes().all(|c| c.is_ascii_digit()) {
            let port: u16 = p.parse().map_err(|_| NetError::Url(format!("bad port: {p}")))?;
            (h.to_string(), port)
        } else {
            (authority.to_string(), if https { 443 } else { 80 })
        }
    } else {
        (authority.to_string(), if https { 443 } else { 80 })
    };
    if host.is_empty() {
        return Err(NetError::Url("missing host".into()));
    }
    if !https && !is_loopback(&host) {
        return Err(NetError::Url("plain HTTP is only used for 127.0.0.1 test servers".into()));
    }
    Ok(Url { https, host, port, path })
}

fn is_loopback(host: &str) -> bool {
    host == "127.0.0.1" || host.eq_ignore_ascii_case("localhost")
}

fn join_url(base: &Url, loc: &str) -> Result<String, NetError> {
    if loc.contains("://") {
        return Ok(loc.to_string());
    }
    let scheme = if base.https { "https" } else { "http" };
    let origin = format!("{scheme}://{}:{}", base.host, base.port);
    if let Some(path) = loc.strip_prefix('/') {
        Ok(format!("{origin}/{path}"))
    } else {
        let dir = base.path.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
        Ok(format!("{origin}{dir}/{loc}"))
    }
}

fn tls_config() -> Arc<ClientConfig> {
    static CFG: OnceLock<Arc<ClientConfig>> = OnceLock::new();
    CFG.get_or_init(|| {
        let roots = RootCertStore { roots: webpki_roots::TLS_SERVER_ROOTS.to_vec() };
        let config = ClientConfig::builder_with_provider(Arc::new(rustls_rustcrypto::provider()))
            .with_safe_default_protocol_versions()
            .expect("rustls-rustcrypto supports the default TLS versions")
            .with_root_certificates(roots)
            .with_no_client_auth();
        Arc::new(config)
    })
    .clone()
}

fn exchange(url: &Url, timeout: Duration, max: usize) -> Result<(u16, Vec<(String, String)>, Vec<u8>), NetError> {
    let addr = format!("{}:{}", url.host, url.port);
    let mut last = NetError::Dns(format!("no address for {addr}"));
    let mut sock = None;
    for a in addr.to_socket_addrs().map_err(|e| NetError::Dns(e.to_string()))? {
        match TcpStream::connect_timeout(&a, timeout) {
            Ok(s) => {
                sock = Some(s);
                break;
            }
            Err(e) => last = NetError::Connect(e.to_string()),
        }
    }
    let sock = sock.ok_or(last)?;
    let _ = sock.set_read_timeout(Some(timeout));
    let _ = sock.set_write_timeout(Some(timeout));
    if url.https {
        let name = ServerName::try_from(url.host.as_str())
            .map_err(|_| NetError::Url(format!("bad server name {}", url.host)))?
            .to_owned();
        let conn = ClientConnection::new(tls_config(), name).map_err(|e| NetError::Tls(e.to_string()))?;
        let mut tls = StreamOwned::new(conn, sock);
        http_roundtrip(&mut tls, url, max)
    } else {
        let mut sock = sock;
        http_roundtrip(&mut sock, url, max)
    }
}

fn host_header(url: &Url) -> String {
    let default = if url.https { 443 } else { 80 };
    if url.port == default { url.host.clone() } else { format!("{}:{}", url.host, url.port) }
}

fn http_roundtrip(stream: &mut (impl Read + Write), url: &Url, max: usize) -> Result<(u16, Vec<(String, String)>, Vec<u8>), NetError> {
    let req = format!(
        "GET {path} HTTP/1.1\r\nHost: {host}\r\nUser-Agent: STL-Editor/{ver}\r\nAccept: */*\r\nConnection: close\r\n\r\n",
        path = url.path,
        host = host_header(url),
        ver = env!("CARGO_PKG_VERSION"),
    );
    write_all(stream, req.as_bytes())?;
    let raw = read_headers(stream)?;
    let split = raw.windows(4).position(|w| w == b"\r\n\r\n").ok_or_else(|| NetError::Protocol("no header end".into()))?;
    let text = std::str::from_utf8(&raw[..split]).map_err(|_| NetError::Protocol("headers are not utf-8".into()))?;
    let mut lines = text.split("\r\n");
    let status = lines.next().unwrap_or("");
    let mut parts = status.splitn(3, ' ');
    let _version = parts.next();
    let code: u16 = parts
        .next()
        .ok_or_else(|| NetError::Protocol(format!("bad status line: {status}")))?
        .parse()
        .map_err(|_| NetError::Protocol(format!("bad status line: {status}")))?;
    let mut headers = Vec::new();
    for line in lines {
        if line.is_empty() {
            continue;
        }
        let Some((k, v)) = line.split_once(':') else { continue };
        headers.push((k.trim().to_string(), v.trim().to_string()));
    }
    let rest = &raw[split + 4..];
    let body = if header_value(&headers, "transfer-encoding").is_some_and(|v| v.to_ascii_lowercase().contains("chunked")) {
        read_chunked(stream, rest, max)?
    } else if let Some(n) = header_value(&headers, "content-length").and_then(|v| v.parse::<usize>().ok()) {
        read_counted(stream, rest, n, max)?
    } else {
        read_until_close(stream, rest, max)?
    };
    Ok((code, headers, body))
}

fn header_value<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
}

fn read_headers(stream: &mut impl Read) -> Result<Vec<u8>, NetError> {
    let mut buf = Vec::new();
    while buf.windows(4).position(|w| w == b"\r\n\r\n").is_none() {
        if read_some(stream, &mut buf)? == 0 {
            return Err(NetError::Protocol("connection closed before the headers".into()));
        }
        if buf.len() > MAX_HEADERS {
            return Err(NetError::Protocol("headers too large".into()));
        }
    }
    Ok(buf)
}

fn read_counted(stream: &mut impl Read, already: &[u8], len: usize, max: usize) -> Result<Vec<u8>, NetError> {
    if len > max {
        return Err(NetError::Protocol("response too large".into()));
    }
    let mut buf = already.to_vec();
    while buf.len() < len {
        if read_some(stream, &mut buf)? == 0 {
            return Err(NetError::Protocol("cut-off download".into()));
        }
    }
    buf.truncate(len);
    Ok(buf)
}

fn read_until_close(stream: &mut impl Read, already: &[u8], max: usize) -> Result<Vec<u8>, NetError> {
    let mut buf = already.to_vec();
    while read_some(stream, &mut buf)? != 0 {
        if buf.len() > max {
            return Err(NetError::Protocol("response too large".into()));
        }
    }
    Ok(buf)
}

fn read_chunked(stream: &mut impl Read, already: &[u8], max: usize) -> Result<Vec<u8>, NetError> {
    let mut buf = already.to_vec();
    let mut out = Vec::new();
    loop {
        let line_end = loop {
            if let Some(i) = find_crlf(&buf) {
                break i;
            }
            if read_some(stream, &mut buf)? == 0 {
                return Err(NetError::Protocol("cut-off download".into()));
            }
        };
        let line = std::str::from_utf8(&buf[..line_end]).unwrap_or("");
        let size_hex = line.split(';').next().unwrap_or("").trim();
        let size = usize::from_str_radix(size_hex, 16).map_err(|_| NetError::Protocol(format!("bad chunk size: {line}")))?;
        buf.drain(..line_end + 2);
        if size == 0 {
            return Ok(out);
        }
        while buf.len() < size + 2 {
            if read_some(stream, &mut buf)? == 0 {
                return Err(NetError::Protocol("cut-off download".into()));
            }
        }
        out.extend_from_slice(&buf[..size]);
        buf.drain(..size + 2);
        if out.len() > max {
            return Err(NetError::Protocol("response too large".into()));
        }
    }
}

fn find_crlf(buf: &[u8]) -> Option<usize> {
    buf.windows(2).position(|w| w == b"\r\n")
}

fn read_some(stream: &mut impl Read, buf: &mut Vec<u8>) -> Result<usize, NetError> {
    let mut tmp = [0u8; 8192];
    loop {
        match stream.read(&mut tmp) {
            Ok(n) => {
                buf.extend_from_slice(&tmp[..n]);
                return Ok(n);
            }
            Err(e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(e) if e.kind() == ErrorKind::TimedOut || e.kind() == ErrorKind::WouldBlock => return Err(NetError::Timeout),
            Err(e) => return Err(NetError::Io(e.to_string())),
        }
    }
}

fn write_all(stream: &mut impl Write, mut data: &[u8]) -> Result<(), NetError> {
    while !data.is_empty() {
        match stream.write(data) {
            Ok(0) => return Err(NetError::Io("the connection closed while sending the request".into())),
            Ok(n) => data = &data[n..],
            Err(e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(e) if e.kind() == ErrorKind::TimedOut || e.kind() == ErrorKind::WouldBlock => return Err(NetError::Timeout),
            Err(e) => return Err(NetError::Io(e.to_string())),
        }
    }
    stream.flush().map_err(|e| {
        if e.kind() == ErrorKind::TimedOut { NetError::Timeout } else { NetError::Io(e.to_string()) }
    })
}
