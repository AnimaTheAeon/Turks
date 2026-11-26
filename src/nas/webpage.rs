//! Turks's own web page on the NAS, so it can be used from any browser at
//! http://<nas>:<api port> without opening DSM. It is the same page as the
//! DSM window, served from the package's `ui` folder, behind a sign-in page
//! in the look picked in Turks. The username and password are set in
//! Settings › General; only a salted hash of the password is kept.
//!
//! Sign-ins last 30 days with "Remember me" (12 hours otherwise). Only
//! SHA-256 hashes of the cookies are stored, in `web-sessions.json`, so
//! sign-ins survive restarts and updates. Changing the username or password
//! signs everyone out.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tiny_http::{Header, Method, Request, Response};

use crate::core::Core;

const PAGE: &str = include_str!("login.html");
const COOKIE: &str = "turks_session";
const REMEMBER_SECS: u64 = 30 * 24 * 3600;
const SHORT_SECS: u64 = 12 * 3600;
const FAILED: &str = r#"<div id="login-failed" class="failed hidden">Incorrect username or password</div>"#;

#[derive(Serialize, Deserialize, Clone)]
struct Entry {
    /// SHA-256 of the cookie value (the cookie itself is never stored).
    token: String,
    /// Which username and password it was made with.
    cred: String,
    expires: u64,
}

static SESSIONS: Mutex<Option<Vec<Entry>>> = Mutex::new(None);

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn sha(s: &str) -> String {
    Sha256::digest(s.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

/// "salt:hash" for the settings file.
pub fn hash_password(pass: &str) -> String {
    let salt = format!("{:032x}", rand::random::<u128>());
    let hash = stretch(&salt, pass);
    format!("{salt}:{hash}")
}

/// Many rounds so a copied settings file can't be guessed through quickly.
fn stretch(salt: &str, pass: &str) -> String {
    let mut h = sha(&format!("{salt}{pass}"));
    for _ in 0..20_000 {
        h = sha(&format!("{salt}{h}"));
    }
    h
}

fn password_ok(stored: &str, pass: &str) -> bool {
    let Some((salt, hash)) = stored.split_once(':') else { return false };
    let got = stretch(salt, pass);
    // Same time whatever matches.
    got.len() == hash.len() && got.bytes().zip(hash.bytes()).fold(0u8, |acc, (a, b)| acc | (a ^ b)) == 0
}

fn cred_of(user: &str, stored: &str) -> String {
    sha(&format!("{user}\n{stored}"))
}

fn sessions_path() -> PathBuf {
    super::var_dir().join("web-sessions.json")
}

fn with_sessions<T>(f: impl FnOnce(&mut Vec<Entry>) -> T) -> T {
    let mut guard = SESSIONS.lock().unwrap_or_else(|e| e.into_inner());
    let list = guard.get_or_insert_with(|| std::fs::read(sessions_path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default());
    let t = now();
    list.retain(|e| e.expires > t);
    f(list)
}

fn save(list: &[Entry]) {
    if let Ok(b) = serde_json::to_vec(list) {
        let path = sessions_path();
        let tmp = path.with_extension("tmp");
        if std::fs::write(&tmp, b).is_ok() {
            let _ = std::fs::rename(tmp, path);
        }
    }
}

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).unwrap()
}

fn cookie_value(req: &Request) -> Option<String> {
    let h = req.headers().iter().find(|h| h.field.equiv("Cookie"))?;
    h.value.as_str().split(';').find_map(|part| {
        let (k, v) = part.trim().split_once('=')?;
        (k == COOKIE).then(|| v.to_string())
    })
}

fn signed_in(req: &Request, user: &str, stored: &str) -> bool {
    if user.is_empty() || stored.is_empty() {
        return false;
    }
    let Some(c) = cookie_value(req) else { return false };
    let (token, cred) = (sha(&c), cred_of(user, stored));
    with_sessions(|list| list.iter().any(|e| e.token == token && e.cred == cred))
}

/// Paths the Turks web page answers (everything but the Radarr API, its ping and the calendar feed).
pub fn wants(path: &str) -> bool {
    let p = path.to_ascii_lowercase();
    !(p == "/ping" || p == "/api" || p.starts_with("/api/") || p.starts_with("/feed/"))
}

/// Where the page's files are: the package's `ui` folder, next to `bin`.
fn ui_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("TURKS_UI_DIR") {
        return d.into();
    }
    std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().and_then(Path::parent).map(|p| p.join("ui")))
        .unwrap_or_else(|| "/var/packages/Turks/target/ui".into())
}

/// A file of the page, or None for anything outside the folder or DSM-only.
fn ui_file(path: &str) -> Option<PathBuf> {
    let rel = path.trim_start_matches('/');
    let rel = if rel.is_empty() { "index.html" } else { rel };
    let rel = Path::new(rel);
    if !rel.components().all(|c| matches!(c, Component::Normal(_))) {
        return None;
    }
    let first = rel.components().next()?.as_os_str().to_str()?;
    // The DSM bridge and the DSM window's own files aren't part of the web page.
    if matches!(first, "api.cgi" | "Turks.js" | "config" | "texts") {
        return None;
    }
    let full = ui_dir().join(rel);
    full.is_file().then_some(full)
}

fn kind_of(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase().as_str() {
        "html" => "text/html; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// The sign-in page needs these before anyone has signed in.
fn is_public(path: &str) -> bool {
    path.starts_with("/fonts/") || path == "/images/turks_64.png" || path == "/images/turks_256.png"
}

fn send_page(req: Request, core: &Core, base: &str, notice: Option<&str>) {
    let cfg = core.cfg();
    let theme = match cfg.theme.as_str() {
        t @ ("green" | "blades" | "glass") => t,
        _ => "ff",
    };
    let mut html = PAGE.replace("_THEME_", theme);
    if !base.is_empty() {
        html = html.replace("url('/", &format!("url('{base}/")).replace("=\"/", &format!("=\"{base}/"));
    }
    if let Some(n) = notice {
        html = html.replace(FAILED, &format!(r#"<div id="login-failed" class="failed">{n}</div>"#));
    }
    let resp = Response::from_string(html)
        .with_header(header("Content-Type", "text/html; charset=utf-8"))
        .with_header(header("Cache-Control", "no-store"))
        .with_header(header("X-Frame-Options", "DENY"));
    let _ = req.respond(resp);
}

fn redirect(req: Request, to: &str, cookie: Option<String>) {
    let mut resp = Response::empty(303).with_header(header("Location", to)).with_header(header("Cache-Control", "no-store"));
    if let Some(c) = cookie {
        resp = resp.with_header(header("Set-Cookie", &c));
    }
    let _ = req.respond(resp);
}

fn json_reply(req: Request, code: u16, body: &str) {
    let resp = Response::from_string(body)
        .with_status_code(code)
        .with_header(header("Content-Type", "application/json; charset=utf-8"))
        .with_header(header("Cache-Control", "no-store"));
    let _ = req.respond(resp);
}

fn form_value(body: &str, key: &str) -> Option<String> {
    body.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        (k == key).then(|| decode(v))
    })
}

fn decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < b.len() => match u8::from_str_radix(&s[i + 1..i + 3], 16) {
                Ok(v) => {
                    out.push(v);
                    i += 2;
                }
                Err(_) => out.push(b'%'),
            },
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

const NOT_SET_UP: &str = "The web page isn't set up yet. Open Turks in DSM and choose a username and password in Settings › General.";

fn login(mut req: Request, core: &Core, base: &str) {
    let cfg = core.cfg();
    if cfg.web_user.is_empty() || cfg.web_pass.is_empty() {
        return send_page(req, core, base, Some(NOT_SET_UP));
    }
    let mut body = String::new();
    let _ = req.as_reader().take(16 * 1024).read_to_string(&mut body);
    let (u, p) = (form_value(&body, "username").unwrap_or_default(), form_value(&body, "password").unwrap_or_default());
    let pass_ok = password_ok(&cfg.web_pass, &p);
    if !(pass_ok && u.trim() == cfg.web_user) {
        crate::applog::warn("Web page: a sign-in with the wrong username or password".to_string());
        std::thread::sleep(Duration::from_millis(800)); // slow down guessing
        return redirect(req, &format!("{base}/?loginFailed=true"), None);
    }
    let remember = form_value(&body, "rememberMe").is_some();
    let token = format!("{:032x}{:032x}", rand::random::<u128>(), rand::random::<u128>());
    let entry = Entry {
        token: sha(&token),
        cred: cred_of(&cfg.web_user, &cfg.web_pass),
        expires: now() + if remember { REMEMBER_SECS } else { SHORT_SECS },
    };
    with_sessions(|list| {
        list.push(entry);
        save(list);
    });
    let path = if base.is_empty() { "/" } else { base };
    let mut cookie = format!("{COOKIE}={token}; Path={path}; HttpOnly; SameSite=Strict");
    if remember {
        cookie.push_str(&format!("; Max-Age={REMEMBER_SECS}"));
    }
    crate::applog::info(format!("Web page: {} signed in", cfg.web_user));
    redirect(req, &format!("{base}/"), Some(cookie));
}

fn logout(req: Request, base: &str) {
    if let Some(c) = cookie_value(&req) {
        let token = sha(&c);
        with_sessions(|list| {
            list.retain(|e| e.token != token);
            save(list);
        });
    }
    let path = if base.is_empty() { "/" } else { base };
    redirect(req, &format!("{base}/"), Some(format!("{COOKIE}=; Path={path}; HttpOnly; SameSite=Strict; Max-Age=0")));
}

fn query(url: &str) -> HashMap<String, String> {
    crate::webapi::query(url)
}

/// `path` has the URL base already taken off; `base` is that base ("" or "/turks").
pub fn handle(core: Core, req: Request, path: &str, base: &str) {
    let cfg = core.cfg();
    let post = *req.method() == Method::Post;
    match (post, path) {
        (true, "/login") => return login(req, &core, base),
        (_, "/logout") => return logout(req, base),
        _ => {}
    }
    if is_public(path) {
        if let Some(file) = ui_file(path) {
            return send_file(req, &file, path);
        }
    }
    let ok = signed_in(&req, &cfg.web_user, &cfg.web_pass);
    if path == "/api.cgi" {
        if !ok {
            return json_reply(req, 401, r#"{"error":"Signed out","signin":true}"#);
        }
        let q = query(req.url());
        let route = q.get("r").cloned().unwrap_or_default();
        return crate::webapi::handle_signed_in(core, req, &route, q);
    }
    let is_page = path.is_empty() || path == "/" || path == "/index.html";
    if !ok {
        if is_page || path == "/login" {
            let notice = if cfg.web_user.is_empty() || cfg.web_pass.is_empty() { Some(NOT_SET_UP) } else { None };
            return send_page(req, &core, base, notice);
        }
        return json_reply(req, 401, r#"{"error":"Signed out","signin":true}"#);
    }
    if path == "/login" {
        return redirect(req, &format!("{base}/"), None);
    }
    match ui_file(path) {
        Some(file) => send_file(req, &file, path),
        None => {
            let _ = req.respond(Response::from_string("Not found").with_status_code(404));
        }
    }
}

fn send_file(req: Request, file: &Path, path: &str) {
    let Ok(bytes) = std::fs::read(file) else {
        let _ = req.respond(Response::from_string("Not found").with_status_code(404));
        return;
    };
    let is_index = path.is_empty() || path == "/" || path == "/index.html";
    // Everything but the page itself is asked for with ?v=<version>.
    let cache = if is_index { "no-store" } else { "max-age=86400" };
    let resp = Response::from_data(bytes)
        .with_header(header("Content-Type", kind_of(file)))
        .with_header(header("Cache-Control", cache))
        .with_header(header("X-Content-Type-Options", "nosniff"));
    let resp = if is_index { resp.with_header(header("X-Frame-Options", "SAMEORIGIN")) } else { resp };
    let _ = req.respond(resp);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passwords_check() {
        let h = hash_password("hunter22");
        assert!(password_ok(&h, "hunter22"));
        assert!(!password_ok(&h, "hunter23"));
        assert!(!password_ok("", "hunter22"));
        assert_ne!(hash_password("same"), hash_password("same"), "salted");
    }

    #[test]
    fn page_has_slots() {
        assert!(PAGE.contains("'_THEME_'"));
        assert!(PAGE.contains("action=\"/login\""));
        assert!(PAGE.contains(FAILED));
    }

    #[test]
    fn routes() {
        assert!(wants("/") && wants("") && wants("/api.cgi") && wants("/app.js") && wants("/login"));
        assert!(!wants("/api/v3/movie") && !wants("/API/v3/system/status") && !wants("/ping") && !wants("/feed/calendar/turks.ics") && !wants("/api"));
    }

    #[test]
    fn files_stay_in_the_folder() {
        assert!(ui_file("/../bin/turks").is_none());
        assert!(ui_file("/fonts/../../x").is_none());
        assert!(ui_file("/api.cgi").is_none());
        assert!(ui_file("/Turks.js").is_none());
        assert!(ui_file("/config").is_none());
    }

    #[test]
    fn form_values_decode() {
        let body = "username=tu+rks&password=p%40ss%26word%25&rememberMe=on";
        assert_eq!(form_value(body, "username").as_deref(), Some("tu rks"));
        assert_eq!(form_value(body, "password").as_deref(), Some("p@ss&word%"));
    }
}
