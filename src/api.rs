//! A Radarr-compatible API (v3 subset) so Prowlarr can sync indexers and
//! Overseerr/Jellyseerr/Ombi can add movies. Auth: X-Api-Key or ?apikey=.

use serde_json::{json, Value};
use tiny_http::{Header, Method, Request, Response, Server};

use crate::core::{Command, Core};
use crate::library::{self, AddOptions};
use crate::model::*;

pub fn start(core: Core) {
    let cfg = core.cfg();
    if !cfg.api_enabled {
        return;
    }
    let host = if cfg.api_bind_all { "0.0.0.0" } else { "127.0.0.1" };
    let addr = format!("{host}:{}", cfg.api_port);
    let server = match Server::http(&addr) {
        Ok(s) => s,
        Err(e) => {
            crate::applog::warn(format!("API couldn't start on {addr}: {e}. Is Radarr or something else using that port? Change it in Settings › General."));
            return;
        }
    };
    crate::applog::info(format!("API listening on http://{addr}"));
    std::thread::Builder::new()
        .name("api".into())
        .spawn(move || {
            for req in server.incoming_requests() {
                let core = core.clone();
                std::thread::spawn(move || handle(core, req));
            }
        })
        .ok();
}

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).unwrap()
}

fn respond(req: Request, status: u16, body: Value) {
    let data = serde_json::to_vec(&body).unwrap_or_default();
    let resp = Response::from_data(data)
        .with_status_code(status)
        .with_header(header("Content-Type", "application/json; charset=utf-8"))
        .with_header(header("Access-Control-Allow-Origin", "*"));
    let _ = req.respond(resp);
}

fn query_params(url: &str) -> Vec<(String, String)> {
    url.split_once('?')
        .map(|(_, q)| {
            q.split('&')
                .filter_map(|kv| {
                    let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
                    Some((urldecode(k).to_lowercase(), urldecode(v)))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn urldecode(s: &str) -> String {
    let bytes = s.replace('+', " ").into_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or(""), 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

fn handle(core: Core, mut req: Request) {
    let cfg = core.cfg();
    let url = req.url().to_string();
    let path_full = url.split('?').next().unwrap_or("").trim_end_matches('/').to_string();
    let base = cfg.url_base.trim().trim_end_matches('/');
    let path = path_full.strip_prefix(base).unwrap_or(&path_full).to_lowercase();
    let q = query_params(&url);
    let param = |k: &str| q.iter().find(|(kk, _)| kk == k).map(|(_, v)| v.clone());

    if *req.method() == Method::Options {
        let resp = Response::empty(204)
            .with_header(header("Access-Control-Allow-Origin", "*"))
            .with_header(header("Access-Control-Allow-Headers", "X-Api-Key, Content-Type"))
            .with_header(header("Access-Control-Allow-Methods", "GET, POST, PUT, DELETE"));
        let _ = req.respond(resp);
        return;
    }
    if path == "/ping" {
        return respond(req, 200, json!({"status": "OK"}));
    }
    if path.is_empty() || path == "/" {
        let html = format!(
            "<html><body style='background:#06083a;color:#f4f4f8;font-family:sans-serif;padding:40px'><h1>Turks {}</h1><p>The API is running. Use it with the API key from Settings › General.</p></body></html>",
            crate::updater::APP_VERSION
        );
        let _ = req.respond(Response::from_string(html).with_header(header("Content-Type", "text/html")));
        return;
    }
    let key = req
        .headers()
        .iter()
        .find(|h| h.field.equiv("X-Api-Key"))
        .map(|h| h.value.to_string())
        .or_else(|| param("apikey"))
        .unwrap_or_default();
    if key != cfg.api_key {
        return respond(req, 401, json!({"error": "Unauthorized"}));
    }
    if path.starts_with("/feed/") {
        let past: i64 = param("pastdays").and_then(|v| v.parse().ok()).unwrap_or(14);
        let future: i64 = param("futuredays").and_then(|v| v.parse().ok()).unwrap_or(90);
        let unmonitored = param("unmonitored").map(|v| v == "true").unwrap_or(false);
        let ics = ical(&core, past, future, unmonitored);
        let resp = Response::from_string(ics).with_header(header("Content-Type", "text/calendar; charset=utf-8"));
        let _ = req.respond(resp);
        return;
    }
    let mut body = String::new();
    let _ = req.as_reader().read_to_string(&mut body);
    let body: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
    let method = req.method().clone();
    let parts: Vec<&str> = path.trim_start_matches('/').split('/').collect();
    // Accept /api/v3/... and /api/... alike.
    let parts: Vec<&str> = if parts.first() == Some(&"api") {
        if parts.get(1) == Some(&"v3") { parts[2..].to_vec() } else { parts[1..].to_vec() }
    } else {
        parts
    };
    let id_at = |i: usize| parts.get(i).and_then(|s| s.parse::<i64>().ok());

    let result: Result<(u16, Value), String> = (|| {
        Ok(match (method, parts.first().copied().unwrap_or(""), parts.get(1).copied()) {
            (Method::Get, "", None) => (200, json!({"current": "v3", "deprecated": []})),
            (Method::Get, "system", Some("status")) => (
                200,
                json!({
                    "appName": "Turks", "instanceName": "Turks", "version": format!("5.{}", crate::updater::APP_VERSION),
                    "isWindows": cfg!(windows), "isProduction": true, "authentication": "external", "urlBase": cfg.url_base,
                    "startTime": chrono::Utc::now().to_rfc3339(), "branch": "main", "runtimeName": "turks"
                }),
            ),
            (Method::Get, "health", None) => {
                let h = core.status().health.clone();
                (200, json!(h.iter().map(|i| json!({"source": i.source, "type": format!("{:?}", i.level).to_lowercase(), "message": i.message})).collect::<Vec<_>>()))
            }
            (Method::Get, "movie", Some("lookup")) | (Method::Get, "movie", None) if parts.get(1) == Some(&"lookup") => lookup(&core, &parts, &param)?,
            (Method::Get, "movie", None) => {
                let lib = core.lib();
                let tmdb = param("tmdbid").and_then(|t| t.parse::<i64>().ok());
                let list: Vec<Value> = lib.movies.values().filter(|m| tmdb.map(|t| m.tmdb_id == t).unwrap_or(true)).map(|m| movie_json(&lib, m, &cfg)).collect();
                (200, json!(list))
            }
            (Method::Get, "movie", Some(_)) => {
                let lib = core.lib();
                let m = id_at(1).and_then(|id| lib.movies.get(&id)).ok_or("Movie not found")?;
                (200, movie_json(&lib, m, &cfg))
            }
            (Method::Post, "movie", None) => {
                let tmdb = body.get("tmdbId").and_then(|v| v.as_i64()).ok_or("tmdbId is required")?;
                if core.lib().movie_by_tmdb(tmdb).is_some() {
                    return Ok((400, json!([{"propertyName": "TmdbId", "errorMessage": "This movie has already been added"}])));
                }
                let monitored = body.get("monitored").and_then(|v| v.as_bool()).unwrap_or(true);
                let monitor = match body.pointer("/addOptions/monitor").and_then(|v| v.as_str()) {
                    Some("movieAndCollection") => ListMonitor::MovieAndCollection,
                    Some("none") => ListMonitor::None,
                    _ if !monitored => ListMonitor::None,
                    _ => ListMonitor::MovieOnly,
                };
                let root = body.get("rootFolderPath").and_then(|v| v.as_str()).map(String::from).or_else(|| {
                    body.get("path").and_then(|v| v.as_str()).and_then(|p| std::path::Path::new(p).parent().map(|x| x.display().to_string()))
                });
                let opts = AddOptions {
                    root_folder: root.or_else(|| core.lib().root_folders.values().next().map(|r| r.path.clone())).ok_or("rootFolderPath is required")?,
                    quality_profile_id: body.get("qualityProfileId").and_then(|v| v.as_i64()).unwrap_or_else(|| core.lib().profiles.keys().next().copied().unwrap_or(1)),
                    minimum_availability: availability_from(body.get("minimumAvailability").and_then(|v| v.as_str()).unwrap_or("released")),
                    monitor,
                    tags: body.get("tags").and_then(|t| t.as_array()).map(|a| a.iter().filter_map(|x| x.as_i64()).collect()).unwrap_or_default(),
                    search: body.pointer("/addOptions/searchForMovie").and_then(|v| v.as_bool()).unwrap_or(false),
                };
                let m = library::add_movie(&core, tmdb, &opts)?;
                let lib = core.lib();
                (201, movie_json(&lib, &m, &cfg))
            }
            (Method::Put, "movie", Some(_)) => {
                let id = id_at(1).ok_or("Bad id")?;
                let mut m = core.movie(id).ok_or("Movie not found")?;
                if let Some(v) = body.get("monitored").and_then(|v| v.as_bool()) {
                    m.monitored = v;
                }
                if let Some(v) = body.get("qualityProfileId").and_then(|v| v.as_i64()) {
                    m.quality_profile_id = v;
                }
                if let Some(v) = body.get("minimumAvailability").and_then(|v| v.as_str()) {
                    m.minimum_availability = availability_from(v);
                }
                if let Some(t) = body.get("tags").and_then(|t| t.as_array()) {
                    m.tags = t.iter().filter_map(|x| x.as_i64()).collect();
                }
                let m = core.save(m);
                let lib = core.lib();
                (202, movie_json(&lib, &m, &cfg))
            }
            (Method::Delete, "movie", Some(_)) => {
                let id = id_at(1).ok_or("Bad id")?;
                let del = param("deletefiles").map(|v| v == "true").unwrap_or(false);
                let excl = param("addimportexclusion").map(|v| v == "true").unwrap_or(false);
                library::delete_movie(&core, id, del, excl)?;
                (200, json!({}))
            }
            (Method::Get, "qualityprofile", _) => {
                let lib = core.lib();
                (200, json!(lib.profiles.values().map(|p| json!({"id": p.id, "name": p.name, "upgradeAllowed": p.upgrade_allowed, "cutoff": p.cutoff})).collect::<Vec<_>>()))
            }
            (Method::Get, "rootfolder", _) => {
                let lib = core.lib();
                (
                    200,
                    json!(lib
                        .root_folders
                        .values()
                        .map(|r| json!({"id": r.id, "path": r.path, "accessible": std::path::Path::new(&r.path).is_dir(), "freeSpace": crate::import::free_space(std::path::Path::new(&r.path)).unwrap_or(0), "unmappedFolders": []}))
                        .collect::<Vec<_>>()),
                )
            }
            (Method::Get, "tag", _) => {
                let lib = core.lib();
                (200, json!(lib.tags.values().map(|t| json!({"id": t.id, "label": t.label})).collect::<Vec<_>>()))
            }
            (Method::Post, "tag", _) => {
                let label = body.get("label").and_then(|v| v.as_str()).ok_or("label is required")?.to_lowercase();
                let existing = core.lib().tag_id(&label);
                let t = match existing {
                    Some(id) => Tag { id, label },
                    None => core.save(Tag { id: 0, label }),
                };
                (201, json!({"id": t.id, "label": t.label}))
            }
            (Method::Get, "queue", _) => {
                let lib = core.lib();
                let records: Vec<Value> = lib
                    .active_downloads()
                    .map(|d| {
                        json!({"id": d.id, "movieId": d.movie_id, "title": d.title, "size": d.size, "sizeleft": d.remaining,
                            "status": format!("{:?}", d.state).to_lowercase(), "trackedDownloadState": format!("{:?}", d.state).to_lowercase(),
                            "statusMessages": d.messages.iter().map(|m| json!({"title": d.title, "messages": [m]})).collect::<Vec<_>>(),
                            "downloadId": d.download_id, "protocol": d.protocol.label().to_lowercase(), "indexer": d.indexer,
                            "quality": {"quality": {"id": d.quality.quality, "name": crate::quality::name(d.quality.quality)}},
                            "estimatedCompletionTime": d.eta_secs.map(|e| (chrono::Utc::now() + chrono::Duration::seconds(e)).to_rfc3339()),
                            "timeleft": d.eta_secs.map(|e| format!("{:02}:{:02}:{:02}", e / 3600, (e / 60) % 60, e % 60))})
                    })
                    .collect();
                (200, json!({"page": 1, "pageSize": records.len(), "totalRecords": records.len(), "records": records}))
            }
            (Method::Get, "history", _) => {
                let page: i64 = param("page").and_then(|p| p.parse().ok()).unwrap_or(1).max(1);
                let size: i64 = param("pagesize").and_then(|p| p.parse().ok()).unwrap_or(20).clamp(1, 1000);
                let items = core.db.history_page((page - 1) * size, size, None);
                let records: Vec<Value> = items
                    .iter()
                    .map(|h| json!({"id": h.id, "movieId": h.movie_id, "sourceTitle": h.source_title, "eventType": format!("{:?}", h.event).to_lowercase(), "date": h.date, "downloadId": h.download_id, "data": h.data, "quality": {"quality": {"id": h.quality.quality, "name": crate::quality::name(h.quality.quality)}}}))
                    .collect();
                (200, json!({"page": page, "pageSize": size, "totalRecords": core.db.history_count(None), "records": records}))
            }
            (Method::Get, "calendar", _) => {
                let start = param("start").and_then(|s| chrono::NaiveDate::parse_from_str(&s[..10.min(s.len())], "%Y-%m-%d").ok()).unwrap_or_else(|| chrono::Local::now().date_naive());
                let end = param("end").and_then(|s| chrono::NaiveDate::parse_from_str(&s[..10.min(s.len())], "%Y-%m-%d").ok()).unwrap_or(start + chrono::Duration::days(7));
                let lib = core.lib();
                let list: Vec<Value> = lib
                    .movies
                    .values()
                    .filter(|m| [m.in_cinemas, m.digital_release, m.physical_release].iter().flatten().any(|d| *d >= start && *d <= end))
                    .map(|m| movie_json(&lib, m, &cfg))
                    .collect();
                (200, json!(list))
            }
            (Method::Post, "command", _) => {
                let name = body.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let ids: Vec<Id> = body.get("movieIds").and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|x| x.as_i64()).collect()).unwrap_or_default();
                let cmd = match name {
                    "MoviesSearch" => Command::SearchMovies(ids),
                    "RefreshMovie" => Command::RefreshMovies(ids),
                    "RescanMovie" => Command::RescanMovies(ids),
                    "RssSync" => Command::RssSync,
                    "MissingMoviesSearch" => Command::MissingSearch,
                    "CutoffUnmetMoviesSearch" => Command::CutoffUnmetSearch,
                    "RenameMovie" | "RenameFiles" => Command::RenameMovies(ids),
                    "ImportListSync" => Command::ImportListSync,
                    "RefreshMonitoredDownloads" => Command::RefreshDownloads,
                    "Backup" => Command::Backup,
                    "CheckHealth" => Command::CheckHealth,
                    "DownloadedMoviesScan" => Command::ScanFolder(body.get("path").and_then(|v| v.as_str()).unwrap_or("").to_string()),
                    other => return Ok((400, json!({"message": format!("Unknown command {other}")}))),
                };
                core.run(cmd);
                (201, json!({"name": name, "status": "queued", "queued": chrono::Utc::now()}))
            }
            (Method::Get, "diskspace", _) => {
                let lib = core.lib();
                (200, json!(lib.root_folders.values().map(|r| json!({"path": r.path, "label": r.path, "freeSpace": crate::import::free_space(std::path::Path::new(&r.path)).unwrap_or(0), "totalSpace": fs2::total_space(&r.path).unwrap_or(0)})).collect::<Vec<_>>()))
            }
            // ---------- indexers (Prowlarr app sync)
            (Method::Get, "indexer", Some("schema")) => (200, json!([indexer_schema("Torznab"), indexer_schema("Newznab")])),
            (Method::Post, "indexer", Some("test")) => {
                let ix = indexer_from_json(&body, Indexer::default());
                match crate::indexers::test(&ix) {
                    Ok(_) => (200, json!({})),
                    Err(e) => (400, json!([{"propertyName": "", "errorMessage": e}])),
                }
            }
            (Method::Get, "indexer", None) => {
                let lib = core.lib();
                (200, json!(lib.indexers.values().map(indexer_json).collect::<Vec<_>>()))
            }
            (Method::Get, "indexer", Some(_)) => {
                let lib = core.lib();
                let ix = id_at(1).and_then(|id| lib.indexers.get(&id)).ok_or("Indexer not found")?;
                (200, indexer_json(ix))
            }
            (Method::Post, "indexer", None) => {
                let ix = core.save(indexer_from_json(&body, Indexer::default()));
                crate::applog::info(format!("Indexer '{}' added through the API", ix.name));
                (201, indexer_json(&ix))
            }
            (Method::Put, "indexer", Some(_)) => {
                let id = id_at(1).ok_or("Bad id")?;
                let existing = core.lib().indexers.get(&id).cloned().ok_or("Indexer not found")?;
                let ix = core.save(indexer_from_json(&body, existing));
                (202, indexer_json(&ix))
            }
            (Method::Delete, "indexer", Some(_)) => {
                let id = id_at(1).ok_or("Bad id")?;
                core.remove::<Indexer>(id);
                (200, json!({}))
            }
            (Method::Get, "downloadclient", _) => {
                let lib = core.lib();
                (200, json!(lib.clients.values().map(|c| json!({"id": c.id, "name": c.name, "enable": c.enabled, "protocol": c.kind.protocol().label().to_lowercase(), "implementation": c.kind.label(), "priority": c.priority})).collect::<Vec<_>>()))
            }
            _ => (404, json!({"message": "NotFound"})),
        })
    })();
    match result {
        Ok((status, v)) => respond(req, status, v),
        Err(e) => respond(req, 400, json!({"message": e})),
    }
}

fn availability_from(s: &str) -> Availability {
    match s.to_lowercase().as_str() {
        "announced" | "tba" | "preda" => Availability::Announced,
        "incinemas" => Availability::InCinemas,
        _ => Availability::Released,
    }
}

fn status_str(s: MovieStatus) -> &'static str {
    match s {
        MovieStatus::Tba => "tba",
        MovieStatus::Announced => "announced",
        MovieStatus::InCinemas => "inCinemas",
        MovieStatus::Released => "released",
        MovieStatus::Deleted => "deleted",
    }
}

fn date(d: Option<chrono::NaiveDate>) -> Value {
    d.map(|d| json!(format!("{d}T00:00:00Z"))).unwrap_or(Value::Null)
}

pub fn movie_json(lib: &crate::db::Library, m: &Movie, cfg: &crate::config::Config) -> Value {
    let file = lib.file_for(m);
    let mut v = json!({
        "id": m.id, "title": m.title, "originalTitle": m.original_title, "originalLanguage": {"id": crate::formats::language_id(&m.original_language), "name": m.original_language},
        "sortTitle": m.sort_title, "sizeOnDisk": file.map(|f| f.size).unwrap_or(0), "status": status_str(m.status), "overview": m.overview,
        "inCinemas": date(m.in_cinemas), "physicalRelease": date(m.physical_release), "digitalRelease": date(m.digital_release),
        "images": [
            {"coverType": "poster", "remoteUrl": if m.poster_path.is_empty() { String::new() } else { format!("{}/original{}", crate::tmdb::IMG, m.poster_path) }},
            {"coverType": "fanart", "remoteUrl": if m.fanart_path.is_empty() { String::new() } else { format!("{}/original{}", crate::tmdb::IMG, m.fanart_path) }}
        ],
        "website": m.website, "year": m.year, "youTubeTrailerId": m.youtube_trailer, "studio": m.studio, "path": m.path,
        "qualityProfileId": m.quality_profile_id, "hasFile": file.is_some(), "movieFileId": m.movie_file_id.unwrap_or(0),
        "monitored": m.monitored, "minimumAvailability": match m.minimum_availability { Availability::Announced => "announced", Availability::InCinemas => "inCinemas", Availability::Released => "released" },
        "isAvailable": m.is_available(cfg.availability_delay_days), "folderName": m.path, "runtime": m.runtime, "cleanTitle": m.clean_title,
        "imdbId": m.imdb_id, "tmdbId": m.tmdb_id, "titleSlug": m.tmdb_id.to_string(), "rootFolderPath": m.root_folder,
        "certification": m.certification, "genres": m.genres, "keywords": m.keywords, "tags": m.tags, "added": m.added,
        "ratings": {"tmdb": {"value": m.ratings.tmdb, "votes": m.ratings.tmdb_votes, "type": "user"}}, "popularity": m.popularity,
        "lastSearchTime": m.last_search,
        "alternateTitles": m.alt_titles.iter().map(|t| json!({"title": t, "sourceType": "tmdb"})).collect::<Vec<_>>()
    });
    if let Some(c) = &m.collection {
        v["collection"] = json!({"title": c.title, "tmdbId": c.tmdb_id});
    }
    if let Some(f) = file {
        v["movieFile"] = json!({"id": f.id, "movieId": m.id, "relativePath": f.relative_path, "path": f.path, "size": f.size, "dateAdded": f.date_added,
            "sceneName": f.scene_name, "releaseGroup": f.release_group, "edition": f.edition,
            "quality": {"quality": {"id": f.quality.quality, "name": crate::quality::name(f.quality.quality)}, "revision": {"version": f.quality.revision.version, "real": f.quality.revision.real, "isRepack": f.quality.revision.is_repack}},
            "mediaInfo": {"videoCodec": f.media_info.video_codec, "audioCodec": f.media_info.audio_codec, "audioChannels": f.media_info.audio_channels, "videoDynamicRangeType": f.media_info.video_dynamic_range}});
    }
    v
}

fn hit_json(h: &crate::tmdb::SearchHit, existing: Option<&Movie>) -> Value {
    json!({
        "title": h.title, "originalTitle": h.original_title, "year": h.year, "tmdbId": h.tmdb_id, "overview": h.overview,
        "titleSlug": h.tmdb_id.to_string(), "id": existing.map(|m| m.id).unwrap_or(0), "monitored": existing.map(|m| m.monitored).unwrap_or(false),
        "hasFile": existing.map(|m| m.movie_file_id.is_some()).unwrap_or(false), "popularity": h.popularity,
        "images": [{"coverType": "poster", "remoteUrl": if h.poster_path.is_empty() { String::new() } else { format!("{}/original{}", crate::tmdb::IMG, h.poster_path) }}],
        "ratings": {"tmdb": {"value": h.vote}}
    })
}

fn lookup(core: &Core, parts: &[&str], param: &dyn Fn(&str) -> Option<String>) -> Result<(u16, Value), String> {
    let t = core.tmdb();
    let lib = core.lib().clone();
    match parts.get(2).copied() {
        Some("tmdb") => {
            let id: i64 = param("tmdbid").and_then(|v| v.parse().ok()).ok_or("tmdbId is required")?;
            if let Some(m) = lib.movie_by_tmdb(id) {
                return Ok((200, movie_json(&lib, m, &core.cfg())));
            }
            let m = t.movie(id)?;
            Ok((200, movie_json(&lib, &m, &core.cfg())))
        }
        Some("imdb") => {
            let id = param("imdbid").ok_or("imdbId is required")?;
            let h = t.find_imdb(&id)?.ok_or("Not found")?;
            Ok((200, hit_json(&h, lib.movie_by_tmdb(h.tmdb_id))))
        }
        _ => {
            let term = param("term").unwrap_or_default();
            let hits = t.search(&term)?;
            Ok((200, json!(hits.iter().map(|h| hit_json(h, lib.movie_by_tmdb(h.tmdb_id))).collect::<Vec<_>>())))
        }
    }
}

// ---------------------------------------------------------------- indexer resources

fn indexer_schema(kind: &str) -> Value {
    let ix = Indexer { kind: if kind == "Newznab" { IndexerKind::Newznab } else { IndexerKind::Torznab }, ..Default::default() };
    let mut v = indexer_json(&ix);
    v["id"] = json!(0);
    v
}

fn indexer_json(ix: &Indexer) -> Value {
    let kind = match ix.kind {
        IndexerKind::Torznab => "Torznab",
        IndexerKind::Newznab => "Newznab",
    };
    let mut fields = vec![
        json!({"name": "baseUrl", "value": ix.url}),
        json!({"name": "apiPath", "value": ix.api_path}),
        json!({"name": "apiKey", "value": ix.api_key}),
        json!({"name": "categories", "value": ix.categories}),
        json!({"name": "additionalParameters", "value": ix.extra_params}),
        json!({"name": "multiLanguages", "value": []}),
        json!({"name": "requiredFlags", "value": []}),
    ];
    if ix.kind == IndexerKind::Torznab {
        fields.push(json!({"name": "minimumSeeders", "value": ix.minimum_seeders}));
        fields.push(json!({"name": "seedCriteria.seedRatio", "value": ix.seed_ratio}));
        fields.push(json!({"name": "seedCriteria.seedTime", "value": ix.seed_time}));
    }
    json!({
        "id": ix.id, "name": ix.name, "enableRss": ix.enable_rss, "enableAutomaticSearch": ix.enable_auto_search,
        "enableInteractiveSearch": ix.enable_interactive_search, "supportsRss": true, "supportsSearch": true,
        "protocol": ix.kind.protocol().label().to_lowercase(), "priority": ix.priority, "downloadClientId": ix.download_client_id,
        "implementation": kind, "implementationName": kind, "configContract": format!("{kind}Settings"), "fields": fields, "tags": ix.tags
    })
}

fn indexer_from_json(v: &Value, mut ix: Indexer) -> Indexer {
    let field = |name: &str| v.get("fields").and_then(|f| f.as_array()).and_then(|a| a.iter().find(|x| x.get("name").and_then(|n| n.as_str()) == Some(name))).and_then(|x| x.get("value")).cloned();
    if let Some(n) = v.get("name").and_then(|x| x.as_str()) {
        ix.name = n.to_string();
    }
    ix.kind = match v.get("implementation").and_then(|x| x.as_str()) {
        Some("Newznab") => IndexerKind::Newznab,
        Some("Torznab") => IndexerKind::Torznab,
        _ => ix.kind,
    };
    if let Some(b) = v.get("enableRss").and_then(|x| x.as_bool()) {
        ix.enable_rss = b;
    }
    if let Some(b) = v.get("enableAutomaticSearch").and_then(|x| x.as_bool()) {
        ix.enable_auto_search = b;
    }
    if let Some(b) = v.get("enableInteractiveSearch").and_then(|x| x.as_bool()) {
        ix.enable_interactive_search = b;
    }
    if let Some(p) = v.get("priority").and_then(|x| x.as_i64()) {
        ix.priority = p as i32;
    }
    if let Some(t) = v.get("tags").and_then(|x| x.as_array()) {
        ix.tags = t.iter().filter_map(|x| x.as_i64()).collect();
    }
    if let Some(s) = field("baseUrl").and_then(|x| x.as_str().map(String::from)) {
        ix.url = s;
    }
    if let Some(s) = field("apiPath").and_then(|x| x.as_str().map(String::from)) {
        ix.api_path = s;
    }
    if let Some(s) = field("apiKey").and_then(|x| x.as_str().map(String::from)) {
        ix.api_key = s;
    }
    if let Some(a) = field("categories").and_then(|x| x.as_array().cloned()) {
        ix.categories = a.iter().filter_map(|c| c.as_i64().map(|i| i as i32)).collect();
    }
    if let Some(s) = field("additionalParameters").and_then(|x| x.as_str().map(String::from)) {
        ix.extra_params = s;
    }
    if let Some(n) = field("minimumSeeders").and_then(|x| x.as_i64()) {
        ix.minimum_seeders = n as i32;
    }
    ix.seed_ratio = field("seedCriteria.seedRatio").and_then(|x| x.as_f64()).or(ix.seed_ratio);
    ix.seed_time = field("seedCriteria.seedTime").and_then(|x| x.as_i64()).or(ix.seed_time);
    ix
}

/// iCal feed for phone/desktop calendars: /feed/v3/calendar/turks.ics?apikey=KEY
fn ical(core: &Core, past_days: i64, future_days: i64, unmonitored: bool) -> String {
    let lib = core.lib();
    let today = chrono::Local::now().date_naive();
    let (start, end) = (today - chrono::Duration::days(past_days), today + chrono::Duration::days(future_days));
    let esc = |s: &str| s.replace('\\', "\\\\").replace(';', "\\;").replace(',', "\\,").replace('\n', "\\n");
    let mut out = String::from("BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Turks//Movies//EN\r\nX-WR-CALNAME:Turks Movies\r\nCALSCALE:GREGORIAN\r\n");
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    for m in lib.movies.values().filter(|m| m.monitored || unmonitored) {
        for (d, kind, key) in [(m.in_cinemas, "In Cinemas", "cinemas"), (m.digital_release, "Digital Release", "digital"), (m.physical_release, "Physical Release", "physical")] {
            let Some(d) = d else { continue };
            if d < start || d > end {
                continue;
            }
            let next = d + chrono::Duration::days(1);
            out.push_str("BEGIN:VEVENT\r\n");
            out.push_str(&format!("UID:turks_{key}_{}\r\nDTSTAMP:{stamp}\r\n", m.tmdb_id));
            out.push_str(&format!("DTSTART;VALUE=DATE:{}\r\nDTEND;VALUE=DATE:{}\r\n", d.format("%Y%m%d"), next.format("%Y%m%d")));
            out.push_str(&format!("SUMMARY:{} ({kind})\r\n", esc(&m.title)));
            let ov: String = m.overview.chars().take(400).collect();
            out.push_str(&format!("DESCRIPTION:{}\r\n", esc(&ov)));
            out.push_str(&format!("STATUS:{}\r\nEND:VEVENT\r\n", if m.movie_file_id.is_some() { "CONFIRMED" } else { "TENTATIVE" }));
        }
    }
    out.push_str("END:VCALENDAR\r\n");
    out
}
