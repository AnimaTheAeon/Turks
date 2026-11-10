//! The Synology window's API. Listens on loopback only; DSM's api.cgi
//! forwards each request with the caller's DSM session, and every request
//! must belong to a DSM administrator.
//!
//! Requests are `/api?r=<route>&...`, GET to read and POST (JSON body) to
//! change something. Library records go back and forth as the engine's own
//! JSON, so the web UI edits exactly what the Windows app edits.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use tiny_http::{Header, Method, Request, Response, Server};

use crate::config::Config;
use crate::core::{Command, Core, HealthLevel};
use crate::db::Entity;
use crate::library::AddOptions;
use crate::model::*;

pub const DEFAULT_PORT: u16 = 47878;
const AUTH_TTL: Duration = Duration::from_secs(300);
const MAX_BODY: u64 = 32 * 1024 * 1024;

pub fn port() -> u16 {
    std::env::var("TURKS_WEB_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(DEFAULT_PORT)
}

type AuthCache = Arc<Mutex<HashMap<(String, String), (String, Instant)>>>;

pub fn serve(core: Core) {
    let addr = format!("127.0.0.1:{}", port());
    // Right after a restart the old process may still hold the port.
    let mut tries = 0;
    let server = loop {
        match Server::http(&addr) {
            Ok(s) => break s,
            Err(e) if tries < 20 => {
                tries += 1;
                if tries == 1 {
                    crate::applog::warn(format!("Waiting for {addr}: {e}"));
                }
                std::thread::sleep(Duration::from_millis(500));
            }
            Err(e) => {
                crate::applog::error(format!("Can't listen on {addr}: {e}"));
                std::process::exit(1);
            }
        }
    };
    crate::applog::info(format!("Window API listening on {addr}"));
    let cache: AuthCache = Default::default();
    for req in server.incoming_requests() {
        let core = core.clone();
        let cache = cache.clone();
        std::thread::spawn(move || handle(core, req, cache));
    }
}

// ------------------------------------------------------------------ plumbing

fn header(req: &Request, name: &str) -> String {
    req.headers()
        .iter()
        .find(|h| h.field.as_str().as_str().eq_ignore_ascii_case(name))
        .map(|h| h.value.as_str().to_string())
        .unwrap_or_default()
}

pub fn query(url: &str) -> HashMap<String, String> {
    url.split_once('?')
        .map(|(_, q)| q)
        .unwrap_or("")
        .split('&')
        .filter_map(|pair| {
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            (!k.is_empty()).then(|| (percent_decode(k), percent_decode(v)))
        })
        .collect()
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < bytes.len() => {
                if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                    out.push(v);
                    i += 2;
                } else {
                    out.push(b'%');
                }
            }
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Check the DSM session with DSM's own authenticate.cgi; admins only.
fn authenticate(cookie: &str, token: &str, remote: &str, cache: &AuthCache) -> Result<String, String> {
    if std::env::var("TURKS_NO_AUTH").is_ok_and(|v| v == "1") {
        return Ok("dev".into());
    }
    if cookie.is_empty() {
        return Err("Not signed in to DSM".into());
    }
    let key = (cookie.to_string(), token.to_string());
    if let Some((user, at)) = cache.lock().unwrap_or_else(|e| e.into_inner()).get(&key) {
        if at.elapsed() < AUTH_TTL {
            return Ok(user.clone());
        }
    }
    let script = ["/usr/syno/synoman/webman/authenticate.cgi", "/usr/syno/synoman/webman/modules/authenticate.cgi"]
        .into_iter()
        .find(|p| Path::new(p).exists())
        .ok_or("DSM sign-in check isn't available")?;
    let output = std::process::Command::new(script)
        .env_clear()
        .env("HTTP_COOKIE", cookie)
        .env("HTTP_X_SYNO_TOKEN", token)
        .env("REMOTE_ADDR", if remote.is_empty() { "127.0.0.1" } else { remote })
        .env("REQUEST_METHOD", "GET")
        .env("QUERY_STRING", format!("SynoToken={token}"))
        .output()
        .map_err(|e| e.to_string())?;
    let user = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if user.is_empty() {
        return Err("Your DSM session has expired. Sign in to DSM again.".into());
    }
    if !is_admin(&user) {
        return Err("Only DSM administrators can use Turks".into());
    }
    let mut cache = cache.lock().unwrap_or_else(|e| e.into_inner());
    cache.retain(|_, (_, at)| at.elapsed() < AUTH_TTL);
    cache.insert(key, (user.clone(), Instant::now()));
    Ok(user)
}

fn is_admin(user: &str) -> bool {
    if user == "root" {
        return true;
    }
    std::process::Command::new("id")
        .args(["-Gn", user])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).split_whitespace().any(|g| g == "administrators"))
        .unwrap_or(false)
}

fn hdr(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).unwrap()
}

fn respond(req: Request, code: u16, body: Value) {
    let response = Response::from_data(serde_json::to_vec(&body).unwrap_or_default())
        .with_status_code(code)
        .with_header(hdr("Content-Type", "application/json; charset=utf-8"))
        .with_header(hdr("Cache-Control", "no-store"));
    let _ = req.respond(response);
}

fn respond_bytes(req: Request, bytes: Vec<u8>, kind: &str, cache: &str) {
    let response = Response::from_data(bytes).with_header(hdr("Content-Type", kind)).with_header(hdr("Cache-Control", cache));
    let _ = req.respond(response);
}

/// What a route hands back.
enum Out {
    Json(Value),
    Bytes(Vec<u8>, &'static str, &'static str),
    Restart(Value),
}

type R = Result<Out, String>;

fn ok(v: Value) -> R {
    Ok(Out::Json(v))
}

fn to<T: DeserializeOwned>(v: Value) -> Result<T, String> {
    serde_json::from_value(v).map_err(|e| format!("Bad request: {e}"))
}

fn handle(core: Core, mut req: Request, cache: AuthCache) {
    let q = query(req.url());
    let route = q.get("r").cloned().unwrap_or_default();
    let cookie = header(&req, "X-Turks-Cookie");
    let token = header(&req, "X-Turks-Token");
    let remote = header(&req, "X-Turks-Remote");
    if let Err(message) = authenticate(&cookie, &token, &remote, &cache) {
        return respond(req, 401, json!({ "error": message }));
    }
    let post = *req.method() == Method::Post;
    let mut body = String::new();
    if post {
        let _ = req.as_reader().take(MAX_BODY).read_to_string(&mut body);
    }
    let input: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| route_request(&core, post, &route, &q, input)))
        .unwrap_or_else(|_| Err("Something went wrong inside Turks. The log has details.".into()));
    match result {
        Ok(Out::Json(v)) => respond(req, 200, v),
        Ok(Out::Bytes(b, kind, cache)) => respond_bytes(req, b, kind, cache),
        Ok(Out::Restart(v)) => {
            respond(req, 200, v);
            std::thread::sleep(Duration::from_millis(300));
            core.shutdown();
            #[cfg(feature = "nas")]
            crate::nas::restart();
        }
        Err(message) => respond(req, 400, json!({ "error": message })),
    }
}

fn qid(q: &HashMap<String, String>, key: &str) -> Id {
    q.get(key).and_then(|v| v.parse().ok()).unwrap_or(0)
}

fn ids(v: &Value) -> Vec<Id> {
    v.get("ids").and_then(|a| a.as_array()).map(|a| a.iter().filter_map(|x| x.as_i64()).collect()).unwrap_or_default()
}

fn s<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(|x| x.as_str()).unwrap_or("")
}

fn b(v: &Value, key: &str) -> bool {
    v.get(key).and_then(|x| x.as_bool()).unwrap_or(false)
}

fn i(v: &Value, key: &str) -> i64 {
    v.get(key).and_then(|x| x.as_i64()).unwrap_or(0)
}

// ------------------------------------------------------------------ generic records

fn list_of<T: Entity>(core: &Core) -> Value {
    let lib = core.lib();
    json!(T::map(&lib).values().collect::<Vec<_>>())
}

fn save_one<T: Entity>(core: &Core, v: Value) -> Result<Value, String> {
    let item: T = to(v)?;
    Ok(json!(core.save(item)))
}

fn delete_one<T: Entity>(core: &Core, id: Id) -> Result<Value, String> {
    core.remove::<T>(id);
    Ok(json!({ "ok": true }))
}

macro_rules! by_kind {
    ($kind:expr, $f:ident, $($a:expr),*) => {
        match $kind {
            "movie" => $f::<Movie>($($a),*),
            "file" => $f::<MovieFile>($($a),*),
            "profile" => $f::<QualityProfile>($($a),*),
            "format" => $f::<CustomFormat>($($a),*),
            "delay" => $f::<DelayProfile>($($a),*),
            "restriction" => $f::<ReleaseProfile>($($a),*),
            "indexer" => $f::<Indexer>($($a),*),
            "client" => $f::<DownloadClient>($($a),*),
            "notification" => $f::<Notification>($($a),*),
            "list" => $f::<ImportList>($($a),*),
            "exclusion" => $f::<ListExclusion>($($a),*),
            "tag" => $f::<Tag>($($a),*),
            "root" => $f::<RootFolder>($($a),*),
            "remotepath" => $f::<RemotePathMapping>($($a),*),
            "collection" => $f::<MovieCollection>($($a),*),
            "download" => $f::<Download>($($a),*),
            "pending" => $f::<Pending>($($a),*),
            "blocklist" => $f::<Blocklist>($($a),*),
            "size" => $f::<QualitySize>($($a),*),
            other => Err(format!("Unknown record type {other}")),
        }
    };
}

fn list_kind(core: &Core, kind: &str) -> Result<Value, String> {
    fn l<T: Entity>(core: &Core) -> Result<Value, String> {
        Ok(list_of::<T>(core))
    }
    by_kind!(kind, l, core)
}

// ------------------------------------------------------------------ routes

fn route_request(core: &Core, post: bool, route: &str, q: &HashMap<String, String>, v: Value) -> R {
    match (post, route) {
        // --- overview
        (false, "state") => ok(state(core)),
        (false, "meta") => ok(meta()),
        (false, "templates") => ok(templates(q.get("kind").map(String::as_str).unwrap_or(""))),

        // --- records
        (false, "list") => ok(list_kind(core, q.get("kind").map(String::as_str).unwrap_or(""))?),
        (true, "save") => {
            let kind = q.get("kind").cloned().unwrap_or_default();
            if kind == "movie" {
                return ok(save_movie(core, v)?);
            }
            ok(by_kind!(kind.as_str(), save_one, core, v)?)
        }
        (true, "delete") => {
            let kind = q.get("kind").cloned().unwrap_or_default();
            let id = qid(q, "id");
            match kind.as_str() {
                "movie" => {
                    crate::library::delete_movie(core, id, b(&v, "delete_files"), b(&v, "add_exclusion"))?;
                    ok(json!({ "ok": true }))
                }
                "file" => {
                    crate::library::delete_file(core, id, "Manual")?;
                    ok(json!({ "ok": true }))
                }
                "download" => {
                    crate::tracked::remove_from_queue(core, id, b(&v, "remove_from_client"), b(&v, "blocklist"), b(&v, "search_again"))?;
                    ok(json!({ "ok": true }))
                }
                k => ok(by_kind!(k, delete_one, core, id)?),
            }
        }

        // --- movies
        (false, "movies") => ok(movies(core)),
        (false, "movie") => ok(movie_detail(core, qid(q, "id"))?),
        (true, "movies/edit") => ok(edit_movies(core, v)?),
        (true, "movie/move") => {
            crate::library::move_movie(core, i(&v, "id"), s(&v, "path"), b(&v, "move_files"))?;
            ok(json!({ "ok": true }))
        }
        (false, "rename") => {
            let list: Vec<Id> = q.get("ids").map(|s| s.split(',').filter_map(|x| x.parse().ok()).collect()).unwrap_or_default();
            ok(json!(crate::library::rename_preview(core, &list)))
        }
        (false, "wanted/cutoff") => ok(json!(crate::library::cutoff_unmet(core).iter().map(|m| m.id).collect::<Vec<_>>())),

        // --- adding
        (false, "tmdb/search") => {
            let term = q.get("term").cloned().unwrap_or_default();
            ok(json!(tmdb_ready(core)?.search(&term)?))
        }
        (false, "tmdb/discover") => {
            let which = q.get("which").cloned().unwrap_or_else(|| "popular".into());
            ok(json!(tmdb_ready(core)?.chart(&which, 1)?))
        }
        (true, "add") => ok(add_movie(core, v)?),
        (true, "collection/follow") => {
            let opts = add_options(core, &v);
            let c = crate::library::follow_collection(core, i(&v, "tmdb_id"), &opts, b(&v, "monitored"))?;
            ok(json!(c))
        }
        (true, "collection/addmissing") => {
            let c = core.lib().collections.get(&i(&v, "id")).cloned().ok_or("That collection is gone")?;
            ok(json!({ "added": crate::library::add_missing_from_collection(core, &c) }))
        }
        (false, "import/unmapped") => {
            let root = q.get("root").cloned().unwrap_or_default();
            ok(json!(crate::import::unmapped_folders(&core.lib(), Path::new(&root))))
        }
        (false, "import/match") => {
            let t = tmdb_ready(core)?;
            let title = q.get("title").cloned().unwrap_or_default();
            let year: i32 = q.get("year").and_then(|y| y.parse().ok()).unwrap_or(0);
            let tmdb: i64 = q.get("tmdb").and_then(|y| y.parse().ok()).unwrap_or(0);
            let hits = if tmdb > 0 {
                t.movie(tmdb).map(|m| vec![crate::tmdb::SearchHit { tmdb_id: m.tmdb_id, title: m.title, original_title: m.original_title, year: m.year, overview: m.overview, poster_path: m.poster_path, ..Default::default() }])?
            } else {
                t.search_title_year(&title, year)?
            };
            ok(json!(hits))
        }
        (true, "import/library") => ok(library_import(core, v)?),
        (false, "import/manual") => {
            let folder = q.get("folder").cloned().unwrap_or_default();
            let hint = q.get("movie").and_then(|m| m.parse().ok()).filter(|m: &Id| *m > 0);
            ok(json!(crate::import::manual_candidates(core, Path::new(&folder), hint)))
        }
        (true, "import/manual") => ok(manual_import(core, v)?),

        // --- searching
        (true, "command") => ok(command(core, v)?),
        (false, "interactive") => {
            let id = qid(q, "id");
            let s = core.status();
            ok(match s.interactive.get(&id) {
                Some(r) => json!({ "running": r.running, "candidates": r.candidates, "errors": r.errors, "finished": r.finished }),
                None => json!({ "running": false, "candidates": [], "errors": [], "finished": null, "none": true }),
            })
        }
        (true, "grab") => {
            let c: crate::release::Candidate = to(v)?;
            crate::search::grab_in_background(core, c);
            ok(json!({ "ok": true }))
        }
        (false, "history") => {
            let offset = q.get("offset").and_then(|x| x.parse().ok()).unwrap_or(0);
            let limit = q.get("limit").and_then(|x| x.parse().ok()).unwrap_or(50).min(500);
            let event: Option<HistoryEvent> = q.get("event").filter(|e| !e.is_empty()).and_then(|e| serde_json::from_value(json!(e)).ok());
            if let Some(m) = q.get("movie").and_then(|x| x.parse().ok()) {
                return ok(json!({ "total": 0, "items": core.db.history_for_movie(m) }));
            }
            ok(json!({ "total": core.db.history_count(event), "items": core.db.history_page(offset, limit, event) }))
        }
        (true, "history/clear") => {
            core.db.clear_history();
            ok(json!({ "ok": true }))
        }

        // --- settings
        (false, "config") => ok(json!(core.cfg())),
        (true, "config") => {
            // Merge what the window sent over the current settings.
            let mut cur = serde_json::to_value(core.cfg()).unwrap_or_default();
            if let (Some(c), Some(n)) = (cur.as_object_mut(), v.as_object()) {
                for (k, val) in n {
                    c.insert(k.clone(), val.clone());
                }
            }
            let cfg: Config = to(cur)?;
            let ports_changed = { let o = core.cfg(); o.api_port != cfg.api_port || o.api_bind_all != cfg.api_bind_all || o.api_enabled != cfg.api_enabled };
            core.set_cfg(cfg.clone());
            core.update_task_intervals();
            ok(json!({ "config": cfg, "restart_needed": ports_changed }))
        }
        (true, "config/apikey") => {
            let mut cfg = core.cfg();
            cfg.api_key = crate::config::random_key();
            core.set_cfg(cfg.clone());
            ok(json!({ "api_key": cfg.api_key }))
        }
        (true, "naming/example") => {
            let mut cfg = core.cfg();
            if let Some(f) = v.get("standard_movie_format").and_then(|x| x.as_str()) {
                cfg.standard_movie_format = f.to_string();
            }
            if let Some(f) = v.get("movie_folder_format").and_then(|x| x.as_str()) {
                cfg.movie_folder_format = f.to_string();
            }
            if let Some(c) = v.get("colon_replacement") {
                if let Ok(c) = serde_json::from_value(c.clone()) {
                    cfg.colon_replacement = c;
                }
            }
            ok(json!({ "file": crate::naming::example(&cfg, false), "folder": crate::naming::example(&cfg, true) }))
        }
        (true, "test") => ok(test(core, q.get("kind").map(String::as_str).unwrap_or(""), v)?),
        (true, "prowlarr") => ok(json!(crate::indexers::from_prowlarr(s(&v, "url"), s(&v, "api_key"))?)),
        (true, "formats/import") => {
            let list = crate::formats::import_json(s(&v, "text"))?;
            let n = list.len();
            for mut f in list {
                if let Some(existing) = core.lib().formats.values().find(|x| x.name == f.name) {
                    f.id = existing.id;
                }
                core.save(f);
            }
            ok(json!({ "imported": n }))
        }
        (false, "formats/export") => {
            let f = core.lib().formats.get(&qid(q, "id")).cloned().ok_or("That format is gone")?;
            ok(crate::formats::export_json(&f))
        }
        (false, "parse") => ok(parse(core, q.get("title").map(String::as_str).unwrap_or(""))),

        // --- system
        (false, "logs") => ok(crate::applog::with_lines(|lines| {
            json!(lines
                .iter()
                .map(|l| json!({ "time": l.time, "level": match l.level { crate::applog::Level::Info => "info", crate::applog::Level::Warn => "warn", crate::applog::Level::Error => "error" }, "text": l.text }))
                .collect::<Vec<_>>())
        })),
        (true, "logs/clear") => {
            crate::applog::clear();
            ok(json!({ "ok": true }))
        }
        (false, "backups") => ok(json!(crate::backup::list(&core.cfg())
            .into_iter()
            .map(|(p, size, t)| json!({
                "path": p.display().to_string(),
                "name": p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
                "size": size,
                "time": t.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
            }))
            .collect::<Vec<_>>())),
        (true, "backup/restore") => {
            let path = PathBuf::from(s(&v, "path"));
            let cfg = core.cfg();
            if !path.starts_with(&cfg.backup_dir) {
                return Err("Pick one of the listed backups".into());
            }
            crate::backup::stage_restore(&path)?;
            Ok(Out::Restart(json!({ "ok": true, "message": "Restoring. Turks restarts in a few seconds." })))
        }
        (true, "backup/delete") => {
            let path = PathBuf::from(s(&v, "path"));
            if !path.starts_with(core.cfg().backup_dir) || path.extension().and_then(|e| e.to_str()) != Some("zip") {
                return Err("Pick one of the listed backups".into());
            }
            std::fs::remove_file(&path).map_err(|e| e.to_string())?;
            ok(json!({ "ok": true }))
        }
        (true, "restart") => Ok(Out::Restart(json!({ "ok": true }))),
        (false, "fs") => ok(browse(q.get("path").map(String::as_str).unwrap_or(""), q.get("files").is_some())?),
        (false, "image") => image(core, q),
        #[cfg(feature = "nas")]
        (false, "update") => ok(crate::nas::update::status_json()),
        #[cfg(feature = "nas")]
        (true, "update/check") => {
            crate::nas::update::request_check();
            ok(crate::nas::update::status_json())
        }
        _ => Err(format!("Unknown request {route}")),
    }
}

fn tmdb_ready(core: &Core) -> Result<crate::tmdb::Tmdb, String> {
    if !core.cfg().tmdb_ready() {
        return Err("Add your TMDb API key in Settings › General first.".into());
    }
    Ok(core.tmdb())
}

fn state(core: &Core) -> Value {
    let cfg = core.cfg();
    let lib = core.lib();
    let mut s = core.status();
    // Toasts are shown once by the window, then dropped.
    let toasts: Vec<String> = s.toasts.drain(..).map(|(t, _)| t).collect();
    let commands: Vec<Value> = s
        .commands
        .iter()
        .rev()
        .take(40)
        .map(|c| json!({ "id": c.id, "label": c.command.label(), "started": c.started, "finished": c.finished, "message": c.message, "failed": c.failed }))
        .collect();
    let health: Vec<Value> = s
        .health
        .iter()
        .map(|h| json!({ "source": h.source, "level": match h.level { HealthLevel::Notice => "notice", HealthLevel::Warning => "warning", HealthLevel::Error => "error" }, "message": h.message }))
        .collect();
    let tasks: Vec<Value> = s
        .tasks
        .iter()
        .map(|t| json!({ "running": s.commands.iter().any(|c| c.finished.is_none() && c.command == t.command), "name": t.name, "interval_secs": t.interval.as_secs(), "last_run": t.last_run, "next_run": Core::next_run(t), "last_duration_secs": t.last_duration.map(|d| d.as_secs_f64()) }))
        .collect();
    let client_errors: Vec<String> = s
        .client_items
        .iter()
        .filter_map(|(id, (_, r))| r.as_ref().err().map(|e| format!("{}: {e}", lib.clients.get(id).map(|c| c.name.as_str()).unwrap_or("client"))))
        .collect();
    let uptime = s.started.map(|t| t.elapsed().as_secs()).unwrap_or(0);
    drop(s);
    let roots: Vec<Value> = lib
        .root_folders
        .values()
        .map(|r| {
            let unmapped = crate::import::unmapped_folders(&lib, Path::new(&r.path)).len();
            json!({ "id": r.id, "path": r.path, "free": crate::import::free_space(Path::new(&r.path)), "exists": Path::new(&r.path).is_dir(), "unmapped": unmapped })
        })
        .collect();
    let queue = lib.active_downloads().count();
    json!({
        "version": crate::APP_VERSION,
        "uptime_secs": uptime,
        "data_dir": crate::config::data_dir().display().to_string(),
        "tmdb_ready": cfg.tmdb_ready(),
        "setup_done": cfg.setup_done,
        "counts": {
            "movies": lib.movies.len(),
            "files": lib.files.len(),
            "missing": lib.missing_count(),
            "queue": queue,
            "pending": lib.pending.len(),
            "blocklist": lib.blocklist.len(),
            "monitored": lib.movies.values().filter(|m| m.monitored).count(),
            "size": lib.files.values().map(|f| f.size).sum::<u64>(),
        },
        "roots": roots,
        "commands": commands,
        "health": health,
        "toasts": toasts,
        "tasks": tasks,
        "client_errors": client_errors,
        "api": { "enabled": cfg.api_enabled, "port": cfg.api_port, "bind_all": cfg.api_bind_all, "key": cfg.api_key },
        "ffprobe": crate::mediainfo::ffprobe_path(&cfg.ffprobe_path),
    })
}

/// Fixed lists the window needs: qualities, languages, flags…
fn meta() -> Value {
    let qualities: Vec<Value> = crate::quality::ALL
        .iter()
        .map(|q| json!({ "id": q.id, "name": q.name, "source": q.source, "resolution": q.resolution, "modifier": q.modifier, "group": q.group }))
        .collect();
    json!({
        "qualities": qualities,
        "sources": crate::quality::source::NAMES,
        "modifiers": crate::quality::modifier::NAMES,
        "languages": crate::parser::language::LANGUAGES,
        "indexer_flags": crate::formats::INDEXER_FLAGS.iter().map(|(n, _)| *n).collect::<Vec<_>>(),
        "cf_templates": CfKind::templates().into_iter().map(|k| json!({ "label": k.label(), "kind": k })).collect::<Vec<_>>(),
        "client_kinds": ClientKind::ALL.iter().map(|k| json!({ "kind": k, "label": k.label(), "port": k.default_port(), "protocol": k.protocol() })).collect::<Vec<_>>(),
        "colon_replacements": crate::config::ColonReplacement::ALL.iter().map(|c| json!({ "value": c, "label": c.label() })).collect::<Vec<_>>(),
        "propers": crate::config::Propers::ALL.iter().map(|c| json!({ "value": c, "label": c.label() })).collect::<Vec<_>>(),
        "naming_tokens": NAMING_TOKENS,
        "version": crate::APP_VERSION,
    })
}

const NAMING_TOKENS: &[&str] = &[
    "{Movie Title}", "{Movie CleanTitle}", "{Movie TitleThe}", "{Movie OriginalTitle}", "{Movie TitleFirstCharacter}", "{Movie Collection}",
    "{Movie Certification}", "{Release Year}", "{ImdbId}", "{TmdbId}", "{Quality Full}", "{Quality Title}", "{MediaInfo Simple}",
    "{MediaInfo Full}", "{MediaInfo VideoCodec}", "{MediaInfo VideoDynamicRangeType}", "{MediaInfo AudioCodec}", "{MediaInfo AudioChannels}",
    "{MediaInfo AudioLanguages}", "{MediaInfo SubtitleLanguages}", "{Edition Tags}", "{Custom Formats}", "{Release Group}",
    "{Original Title}", "{Original Filename}", "{imdb-{ImdbId}}", "{tmdb-{TmdbId}}",
];

/// Starting points for a new indexer, client, connection or list.
fn templates(kind: &str) -> Value {
    match kind {
        "indexer" => json!([
            { "label": "Torznab", "value": Indexer { kind: IndexerKind::Torznab, ..Default::default() } },
            { "label": "Newznab", "value": Indexer { kind: IndexerKind::Newznab, categories: vec![2000, 2010, 2020, 2030, 2040, 2045, 2050, 2060], ..Default::default() } },
        ]),
        "client" => json!(ClientKind::ALL
            .iter()
            .map(|k| {
                let mut c = DownloadClient { kind: *k, port: k.default_port(), name: k.label().split(" /").next().unwrap_or("").to_string(), ..Default::default() };
                if k.is_blackhole() {
                    c.host.clear();
                }
                json!({ "label": k.label(), "value": c })
            })
            .collect::<Vec<_>>()),
        "notification" => json!(NotifyKind::templates()
            .into_iter()
            .map(|k| {
                let media = k.is_media_server();
                let n = Notification {
                    name: k.label().to_string(),
                    on_grab: !media,
                    on_health_issue: !media,
                    on_manual_interaction: !media,
                    on_app_update: !media,
                    on_rename: media,
                    on_movie_delete: media,
                    on_movie_file_delete: media,
                    kind: k.clone(),
                    ..Default::default()
                };
                json!({ "label": k.label(), "value": n })
            })
            .collect::<Vec<_>>()),
        "list" => json!(ListKind::templates()
            .into_iter()
            .map(|k| json!({ "label": k.label(), "value": ImportList { name: k.label().to_string(), kind: k, ..Default::default() } }))
            .collect::<Vec<_>>()),
        "profile" => json!([{ "label": "New profile", "value": crate::quality::default_profile("New profile", crate::quality::id::BLURAY1080P, &[crate::quality::id::WEBDL1080P, crate::quality::id::WEBRIP1080P, crate::quality::id::BLURAY1080P]) }]),
        "delay" => json!([{ "label": "Delay profile", "value": DelayProfile { order: 0, ..Default::default() } }]),
        "restriction" => json!([{ "label": "Release profile", "value": ReleaseProfile { enabled: true, ..Default::default() } }]),
        "format" => json!([{ "label": "Custom format", "value": CustomFormat::default() }]),
        _ => json!([]),
    }
}

// ------------------------------------------------------------------ movies

/// The library list: everything the grid, table and filters need, without
/// cast lists and other heavy detail.
fn movies(core: &Core) -> Value {
    let lib = core.lib();
    let cfg = core.cfg();
    let downloading: std::collections::HashSet<Id> = lib.active_downloads().map(|d| d.movie_id).collect();
    let list: Vec<Value> = lib
        .movies
        .values()
        .map(|m| {
            let f = lib.file_for(m);
            json!({
                "id": m.id, "tmdb_id": m.tmdb_id, "imdb_id": m.imdb_id, "title": m.title, "sort_title": m.sort_title, "year": m.year,
                "overview": m.overview, "runtime": m.runtime, "genres": m.genres, "studio": m.studio, "certification": m.certification,
                "status": m.status, "in_cinemas": m.in_cinemas, "digital_release": m.digital_release, "physical_release": m.physical_release,
                "ratings": m.ratings, "popularity": m.popularity, "poster_path": m.poster_path, "monitored": m.monitored,
                "minimum_availability": m.minimum_availability, "quality_profile_id": m.quality_profile_id, "root_folder": m.root_folder,
                "path": m.path, "tags": m.tags, "added": m.added, "collection": m.collection, "original_language": m.original_language,
                "available": m.is_available(cfg.availability_delay_days),
                "downloading": downloading.contains(&m.id),
                "file": f.map(|f| json!({ "id": f.id, "size": f.size, "quality": f.quality, "quality_name": crate::quality::full_name(&f.quality), "relative_path": f.relative_path, "release_group": f.release_group, "languages": f.languages, "edition": f.edition, "media_info": f.media_info })),
                "cutoff_unmet": f.zip(lib.profile_for(m)).map(|(f, p)| {
                    let formats: Vec<Id> = crate::decision::file_formats(&lib, f, m).into_iter().map(|(id, _)| id).collect();
                    p.upgrade_allowed && crate::decision::cutoff_not_met(p, &f.quality, &formats, None)
                }).unwrap_or(false),
            })
        })
        .collect();
    json!(list)
}

fn movie_detail(core: &Core, id: Id) -> Result<Value, String> {
    let lib = core.lib();
    let m = lib.movies.get(&id).ok_or("That movie isn't in the library anymore")?;
    let file = lib.file_for(m).cloned();
    let formats = file.as_ref().map(|f| crate::decision::file_formats(&lib, f, m)).unwrap_or_default();
    let score = lib.profile_for(m).map(|p| p.score_of(&formats.iter().map(|(i, _)| *i).collect::<Vec<_>>())).unwrap_or(0);
    let downloads: Vec<&Download> = lib.downloads.values().filter(|d| d.movie_id == id && !d.state.finished()).collect();
    let collection = m.collection.as_ref().and_then(|c| lib.collections.values().find(|x| x.tmdb_id == c.tmdb_id)).cloned();
    Ok(json!({
        "movie": m,
        "file": file,
        "quality_name": file.as_ref().map(|f| crate::quality::full_name(&f.quality)),
        "formats": formats.iter().map(|(_, n)| n).collect::<Vec<_>>(),
        "format_score": score,
        "downloads": downloads,
        "collection": collection,
        "history": core.db.history_for_movie(id),
    }))
}

/// Saving a movie from the editor keeps fields the window doesn't send.
fn save_movie(core: &Core, v: Value) -> Result<Value, String> {
    let id = i(&v, "id");
    let cur = core.movie(id).ok_or("That movie isn't in the library anymore")?;
    let mut merged = serde_json::to_value(&cur).unwrap_or_default();
    if let (Some(c), Some(n)) = (merged.as_object_mut(), v.as_object()) {
        for (k, val) in n {
            if k != "path" && k != "movie_file_id" {
                c.insert(k.clone(), val.clone());
            }
        }
    }
    let m: Movie = to(merged)?;
    let new_path = s(&v, "path").to_string();
    let saved = core.save(m);
    if !new_path.is_empty() && new_path != cur.path {
        crate::library::move_movie(core, id, &new_path, b(&v, "move_files"))?;
    }
    Ok(json!(core.movie(id).unwrap_or(saved)))
}

/// Mass editor: change several movies at once.
fn edit_movies(core: &Core, v: Value) -> Result<Value, String> {
    let list = ids(&v);
    let mut changed = 0;
    for id in &list {
        let Some(mut m) = core.movie(*id) else { continue };
        if let Some(x) = v.get("monitored").and_then(|x| x.as_bool()) {
            m.monitored = x;
        }
        if let Some(x) = v.get("quality_profile_id").and_then(|x| x.as_i64()) {
            m.quality_profile_id = x;
        }
        if let Some(x) = v.get("minimum_availability") {
            if let Ok(a) = serde_json::from_value(x.clone()) {
                m.minimum_availability = a;
            }
        }
        if let Some(t) = v.get("tags").and_then(|x| x.as_array()) {
            let t: Vec<Id> = t.iter().filter_map(|x| x.as_i64()).collect();
            match s(&v, "tag_mode") {
                "remove" => m.tags.retain(|x| !t.contains(x)),
                "replace" => m.tags = t,
                _ => {
                    for x in t {
                        if !m.tags.contains(&x) {
                            m.tags.push(x);
                        }
                    }
                }
            }
        }
        let root = s(&v, "root_folder").to_string();
        core.save(m.clone());
        if !root.is_empty() && root != m.root_folder {
            let folder = Path::new(&m.path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            let new_path = Path::new(&root).join(folder).display().to_string();
            if let Some(mut mm) = core.movie(*id) {
                mm.root_folder = root.clone();
                core.save(mm);
            }
            crate::library::move_movie(core, *id, &new_path, b(&v, "move_files"))?;
        }
        changed += 1;
    }
    Ok(json!({ "changed": changed }))
}

fn add_options(core: &Core, v: &Value) -> AddOptions {
    let cfg = core.cfg();
    AddOptions {
        root_folder: if s(v, "root_folder").is_empty() { cfg.last_add_root.clone() } else { s(v, "root_folder").to_string() },
        quality_profile_id: v.get("quality_profile_id").and_then(|x| x.as_i64()).unwrap_or(cfg.last_add_profile),
        minimum_availability: v.get("minimum_availability").and_then(|x| serde_json::from_value(x.clone()).ok()).unwrap_or(cfg.last_add_availability),
        monitor: v.get("monitor").and_then(|x| serde_json::from_value(x.clone()).ok()).unwrap_or(ListMonitor::MovieOnly),
        tags: v.get("tags").and_then(|x| serde_json::from_value(x.clone()).ok()).unwrap_or_default(),
        search: b(v, "search"),
    }
}

fn add_movie(core: &Core, v: Value) -> Result<Value, String> {
    let opts = add_options(core, &v);
    if opts.root_folder.is_empty() {
        return Err("Add a root folder in Settings › Media Management first.".into());
    }
    let m = crate::library::add_movie(core, i(&v, "tmdb_id"), &opts)?;
    // Remember the choices for next time, like the Windows app.
    let mut cfg = core.cfg();
    cfg.last_add_root = opts.root_folder.clone();
    cfg.last_add_profile = opts.quality_profile_id;
    cfg.last_add_availability = opts.minimum_availability;
    cfg.last_add_search = opts.search;
    core.set_cfg(cfg);
    Ok(json!(m))
}

fn library_import(core: &Core, v: Value) -> Result<Value, String> {
    let opts = AddOptions { search: false, ..add_options(core, &v) };
    let rows: Vec<(i64, String)> = v
        .get("items")
        .and_then(|x| x.as_array())
        .map(|a| a.iter().map(|r| (i(r, "tmdb_id"), s(r, "path").to_string())).filter(|(t, _)| *t > 0).collect())
        .unwrap_or_default();
    let core = core.clone();
    let n = rows.len();
    std::thread::spawn(move || {
        let mut done = 0;
        for (tmdb, path) in rows {
            match crate::library::add_movie(&core, tmdb, &opts) {
                Ok(m) => {
                    // Keep the existing folder name instead of renaming it.
                    if m.path != path {
                        let _ = crate::library::move_movie(&core, m.id, &path, false);
                    }
                    let _ = crate::library::rescan_one(&core, m.id);
                    done += 1;
                }
                Err(e) => core.toast(format!("Couldn't add {path}: {e}")),
            }
        }
        core.toast(format!("Imported {done} movies."));
    });
    Ok(json!({ "started": n }))
}

fn manual_import(core: &Core, v: Value) -> Result<Value, String> {
    let mode = match s(&v, "mode") {
        "copy" => crate::import::Transfer::Copy,
        "hardlink" => crate::import::Transfer::HardlinkOrCopy,
        _ => crate::import::Transfer::Move,
    };
    let rows: Vec<crate::import::ImportDecision> = to(v.get("items").cloned().unwrap_or(json!([])))?;
    let core = core.clone();
    let n = rows.len();
    std::thread::spawn(move || {
        let mut done = 0;
        for mut d in rows.into_iter().filter(|d| d.movie_id.is_some()) {
            d.rejections.clear();
            let lib = core.lib().clone();
            if let Some(m) = d.movie_id.and_then(|id| lib.movies.get(&id)) {
                d.is_upgrade = lib.file_for(m).is_some();
            }
            match crate::import::import(&core, &d, mode, None) {
                Ok(_) => done += 1,
                Err(e) => core.toast(format!("Couldn't import {}: {e}", d.path.display())),
            }
        }
        core.toast(format!("Imported {done} file(s)."));
    });
    Ok(json!({ "started": n }))
}

fn command(core: &Core, v: Value) -> Result<Value, String> {
    let list = ids(&v);
    let cmd = match s(&v, "name") {
        "refresh" => Command::RefreshMovies(list),
        "rescan" => Command::RescanMovies(list),
        "search" => Command::SearchMovies(list),
        "missing" => Command::MissingSearch,
        "cutoff" => Command::CutoffUnmetSearch,
        "rss" => Command::RssSync,
        "downloads" => Command::RefreshDownloads,
        "lists" => Command::ImportListSync,
        "collections" => Command::RefreshCollections,
        "rename" => Command::RenameMovies(list),
        "backup" => Command::Backup,
        "housekeeping" => Command::Housekeeping,
        "health" => Command::CheckHealth,
        "interactive" => Command::InteractiveSearch(list.first().copied().ok_or("Which movie?")?),
        "scan" => Command::ScanFolder(s(&v, "path").to_string()),
        "recycle" => Command::CleanRecycleBin,
        other => return Err(format!("Unknown command {other}")),
    };
    let label = cmd.label();
    core.run(cmd);
    Ok(json!({ "ok": true, "label": label }))
}

fn test(core: &Core, kind: &str, v: Value) -> Result<Value, String> {
    let msg = match kind {
        "indexer" => crate::indexers::test(&to::<Indexer>(v)?)?,
        "client" => crate::clients::test(&to::<DownloadClient>(v)?)?,
        "notification" => {
            crate::notify::send(&to::<Notification>(v)?, &crate::notify::Event::Test)?;
            "Sent.".into()
        }
        "list" => {
            let l: ImportList = to(v)?;
            let hits = crate::lists::fetch(core, &l)?;
            format!("The list has {} movies.", hits.len())
        }
        "tmdb" => {
            let t = crate::tmdb::Tmdb::new(s(&v, "key"), &core.cfg().metadata_language, &core.cfg().certification_country);
            t.test()?;
            "TMDb accepted the key.".into()
        }
        other => return Err(format!("Can't test {other}")),
    };
    Ok(json!({ "message": msg }))
}

/// System › Parse: what Turks makes of a release title.
fn parse(core: &Core, title: &str) -> Value {
    let lib = core.lib();
    let parsed = crate::parser::parse_movie_title(title, false);
    let release = crate::release::Release { title: title.to_string(), ..Default::default() };
    let cand = crate::decision::candidate(&lib, release, None);
    let movie = cand.movie_id.and_then(|id| lib.movies.get(&id)).map(|m| json!({ "id": m.id, "title": m.display() }));
    json!({
        "parsed": parsed,
        "quality_name": crate::quality::full_name(&cand.parsed.quality),
        "movie": movie,
        "formats": cand.format_names,
        "score": cand.score,
    })
}

// ------------------------------------------------------------------ files

/// Folder picker: shared folders on the NAS volumes (any folder when the
/// service runs outside DSM, for development).
fn browse(path: &str, files: bool) -> Result<Value, String> {
    let root = std::env::var("TURKS_BROWSE_ROOT").unwrap_or_else(|_| "/".into());
    let on_dsm = Path::new("/usr/syno").exists() || std::env::var("TURKS_BROWSE_ROOT").is_ok();
    let path = path.trim_end_matches('/');
    let mut entries: Vec<Value> = Vec::new();
    let is_volume = |n: &str| n.starts_with("volume") && n.len() > 6 && n[6..].chars().all(|c| c.is_ascii_digit());
    if path.is_empty() && on_dsm {
        let mut volumes: Vec<PathBuf> = std::fs::read_dir(&root)
            .map_err(|e| e.to_string())?
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(is_volume))
            .collect();
        volumes.sort();
        for volume in volumes {
            list_dir(&volume, &mut entries, false, true);
        }
        return Ok(json!({ "path": "", "parent": null, "entries": entries }));
    }
    let p = if path.is_empty() { PathBuf::from("/") } else { PathBuf::from(path) };
    if p.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        return Err("That isn't a folder".into());
    }
    if on_dsm {
        let inside = p.strip_prefix(&root).ok().and_then(|rel| rel.components().next()).and_then(|c| c.as_os_str().to_str()).is_some_and(is_volume);
        if !inside {
            return Err("Pick a folder on one of the NAS volumes".into());
        }
    }
    list_dir(&p, &mut entries, files, false);
    let parent = p.parent().map(|pp| {
        let top = on_dsm && pp.strip_prefix(&root).map(|rel| rel.components().count() < 2).unwrap_or(true);
        if top { String::new() } else { pp.display().to_string() }
    });
    Ok(json!({ "path": p.display().to_string(), "parent": parent, "entries": entries }))
}

fn list_dir(dir: &Path, out: &mut Vec<Value>, files: bool, show_volume: bool) {
    let Ok(read) = std::fs::read_dir(dir) else { return };
    let mut items: Vec<(PathBuf, bool, u64)> = read
        .flatten()
        .filter_map(|e| {
            let t = e.file_type().ok()?;
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('@') || name.starts_with('#') || name.starts_with('.') || name == "lost+found" {
                return None;
            }
            if t.is_dir() {
                Some((e.path(), true, 0))
            } else if files && t.is_file() {
                Some((e.path(), false, e.metadata().map(|m| m.len()).unwrap_or(0)))
            } else {
                None
            }
        })
        .collect();
    items.sort_by_key(|(p, d, _)| (!*d, p.file_name().map(|n| n.to_string_lossy().to_lowercase())));
    for (path, is_dir, size) in items {
        let writable = is_dir && access(&path, true);
        out.push(json!({
            "name": path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
            "path": path.display().to_string(),
            "dir": is_dir,
            "size": size,
            "volume": show_volume.then(|| dir.file_name().map(|n| n.to_string_lossy().to_string())).flatten(),
            "readable": access(&path, false),
            "writable": writable,
        }));
    }
}

fn access(path: &Path, write: bool) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let Ok(c) = std::ffi::CString::new(path.as_os_str().as_bytes()) else { return false };
    let mode = if write { libc::W_OK | libc::X_OK } else { libc::R_OK | libc::X_OK };
    // SAFETY: c is a valid NUL-terminated path.
    unsafe { libc::access(c.as_ptr(), mode) == 0 }
}

/// Posters, fanart and headshots, cached in MediaCover like the Windows app.
fn image(core: &Core, q: &HashMap<String, String>) -> R {
    let kind = q.get("k").map(String::as_str).unwrap_or("poster");
    let id: i64 = q.get("id").and_then(|x| x.parse().ok()).unwrap_or(0);
    let mut url_path = q.get("p").cloned().unwrap_or_default();
    let (size, dest) = match kind {
        "fanart" => ("w1280", crate::tmdb::image_path("fanart", id)),
        "collection" => ("w342", crate::tmdb::image_path("collection", id)),
        "person" => ("w185", crate::tmdb::person_image_path(id)),
        _ => ("w342", crate::tmdb::image_path("poster", id)),
    };
    if url_path.is_empty() && kind != "person" {
        let lib = core.lib();
        if let Some(m) = lib.movie_by_tmdb(id) {
            url_path = if kind == "fanart" { m.fanart_path.clone() } else { m.poster_path.clone() };
        }
    }
    // Only TMDb image paths ("/abc.jpg") or full TMDb URLs.
    if !(url_path.starts_with('/') || url_path.starts_with("https://image.tmdb.org/")) || url_path.contains("..") {
        url_path.clear();
    }
    let path = if dest.exists() { Some(dest.clone()) } else { crate::tmdb::fetch_image(&url_path, size, &dest) };
    match path.and_then(|p| std::fs::read(p).ok()) {
        Some(bytes) => Ok(Out::Bytes(bytes, "image/jpeg", "private, max-age=86400")),
        None => Err("No image".into()),
    }
}
