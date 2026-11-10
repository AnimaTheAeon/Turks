//! api.cgi: DSM runs this for requests from the Turks window. It forwards
//! the request, with the caller's DSM session, to the service on loopback.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

/// Big enough for a custom-format JSON import or a backup upload.
const MAX_BODY: usize = 32 * 1024 * 1024;

pub fn run() {
    let method = std::env::var("REQUEST_METHOD").unwrap_or_else(|_| "GET".into());
    let query = std::env::var("QUERY_STRING").unwrap_or_default();
    let cookie = std::env::var("HTTP_COOKIE").unwrap_or_default();
    // <img> tags can't send headers, so the token may come in the query.
    let token = std::env::var("HTTP_X_SYNO_TOKEN")
        .ok()
        .filter(|t| !t.is_empty())
        .or_else(|| query.split('&').find_map(|kv| kv.strip_prefix("SynoToken=").map(str::to_string)))
        .unwrap_or_default();
    let remote = std::env::var("REMOTE_ADDR").unwrap_or_default();
    let length: usize = std::env::var("CONTENT_LENGTH").ok().and_then(|v| v.parse().ok()).unwrap_or(0).min(MAX_BODY);
    let mut body = vec![0u8; length];
    if length > 0 {
        let _ = std::io::stdin().read_exact(&mut body);
    }
    let method = if method == "POST" { "POST" } else { "GET" };
    match forward(method, &query, &cookie, &token, &remote, &body) {
        Ok((status, head, payload)) => reply(status, &head, &payload),
        Err(_) => reply(
            503,
            &[("Content-Type".into(), "application/json; charset=utf-8".into())],
            br#"{"error":"The Turks service isn't running. Start it in Package Center."}"#,
        ),
    }
}

fn clean(value: &str) -> String {
    value.chars().filter(|c| *c != '\r' && *c != '\n').collect()
}

type Head = Vec<(String, String)>;

fn forward(method: &str, query: &str, cookie: &str, token: &str, remote: &str, body: &[u8]) -> std::io::Result<(u16, Head, Vec<u8>)> {
    let addr = format!("127.0.0.1:{}", crate::webapi::port());
    let mut stream = TcpStream::connect_timeout(&addr.parse().map_err(std::io::Error::other)?, Duration::from_secs(3))?;
    stream.set_read_timeout(Some(Duration::from_secs(120)))?;
    let head = format!(
        "{method} /api?{} HTTP/1.0\r\nHost: localhost\r\nX-Turks-Cookie: {}\r\nX-Turks-Token: {}\r\nX-Turks-Remote: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
        clean(query),
        clean(cookie),
        clean(token),
        clean(remote),
        body.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(body)?;
    let mut response = Vec::new();
    stream.read_to_end(&mut response)?;
    let split = response.windows(4).position(|w| w == b"\r\n\r\n").ok_or_else(|| std::io::Error::other("bad response"))?;
    let text = String::from_utf8_lossy(&response[..split]).to_string();
    let mut lines = text.lines();
    let status = lines.next().and_then(|l| l.split_whitespace().nth(1)).and_then(|c| c.parse().ok()).unwrap_or(502);
    // Pass through only the headers the window needs.
    let keep = ["content-type", "cache-control", "content-disposition"];
    let headers = lines
        .filter_map(|l| l.split_once(':'))
        .filter(|(k, _)| keep.contains(&k.trim().to_ascii_lowercase().as_str()))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect();
    Ok((status, headers, response[split + 4..].to_vec()))
}

fn reply(status: u16, head: &[(String, String)], body: &[u8]) {
    let mut out = std::io::stdout().lock();
    let _ = write!(out, "Status: {status}\r\n");
    let mut has_type = false;
    let mut has_cache = false;
    for (k, v) in head {
        has_type |= k.eq_ignore_ascii_case("content-type");
        has_cache |= k.eq_ignore_ascii_case("cache-control");
        let _ = write!(out, "{k}: {v}\r\n");
    }
    if !has_type {
        let _ = write!(out, "Content-Type: application/json; charset=utf-8\r\n");
    }
    if !has_cache {
        let _ = write!(out, "Cache-Control: no-store\r\n");
    }
    let _ = write!(out, "X-Content-Type-Options: nosniff\r\n\r\n");
    let _ = out.write_all(body);
    let _ = out.flush();
}
