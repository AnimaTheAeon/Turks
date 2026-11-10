//! Import lists: TMDb, IMDb, Trakt, RSS, Plex watchlist and other
//! Turks/Radarr instances. Movies on enabled lists are added automatically.

use std::collections::HashSet;

use serde_json::Value;

use crate::core::Core;
use crate::library::{self, AddOptions};
use crate::model::*;
use crate::tmdb::SearchHit;

fn http() -> reqwest::blocking::Client {
    crate::tmdb::http_client(45)
}

fn hit(tmdb_id: i64, title: &str, year: i32) -> SearchHit {
    SearchHit { tmdb_id, title: title.to_string(), year, ..Default::default() }
}

/// Resolves IMDb ids and title/year pairs to TMDb ids.
fn resolve(core: &Core, imdb: Vec<String>, titles: Vec<(String, i32)>) -> Vec<SearchHit> {
    let t = core.tmdb();
    let mut out = Vec::new();
    for id in imdb {
        if let Ok(Some(h)) = t.find_imdb(&id) {
            out.push(h);
        }
    }
    for (title, year) in titles {
        if let Ok(hits) = t.search_title_year(&title, year) {
            if let Some(h) = hits.into_iter().next() {
                out.push(h);
            }
        }
    }
    out
}

pub fn fetch(core: &Core, list: &ImportList) -> Result<Vec<SearchHit>, String> {
    let t = core.tmdb();
    match &list.kind {
        ListKind::TmdbList { list_id } => t.list(list_id),
        ListKind::TmdbCollection { collection_id } => {
            let id: i64 = collection_id.trim().parse().map_err(|_| "Collection id should be a number")?;
            Ok(t.collection(id)?.3.into_iter().map(|p| hit(p.tmdb_id, &p.title, p.year)).collect())
        }
        ListKind::TmdbPerson { person_id, cast, crew } => t.person_movies(person_id, *cast, *crew),
        ListKind::TmdbCompany { company_id } => t.discover("with_companies", company_id, 5),
        ListKind::TmdbKeyword { keyword_id } => t.discover("with_keywords", keyword_id, 5),
        ListKind::TmdbPopular { which } => t.chart(which, 2),
        ListKind::ImdbList { list_id } => {
            let id = list_id.trim();
            let url = if id.starts_with("http") { id.to_string() } else { format!("https://www.imdb.com/list/{id}/export") };
            let body = http().get(url).send().map_err(|e| e.to_string())?.text().map_err(|e| e.to_string())?;
            let ids: Vec<String> = regex::Regex::new(r"tt\d{7,8}").unwrap().find_iter(&body).map(|m| m.as_str().to_string()).collect::<HashSet<_>>().into_iter().collect();
            if ids.is_empty() {
                return Err("IMDb didn't return any movies (is the list public?)".into());
            }
            Ok(resolve(core, ids, vec![]))
        }
        ListKind::TraktList { client_id, username, list } => {
            let path = if list.eq_ignore_ascii_case("watchlist") {
                format!("users/{username}/watchlist/movies")
            } else {
                format!("users/{username}/lists/{list}/items/movies")
            };
            let v: Value = http()
                .get(format!("https://api.trakt.tv/{path}"))
                .header("trakt-api-version", "2")
                .header("trakt-api-key", client_id.as_str())
                .send()
                .map_err(|e| e.to_string())?
                .json()
                .map_err(|e| format!("Trakt: {e}"))?;
            Ok(v.as_array()
                .into_iter()
                .flatten()
                .filter_map(|i| {
                    let m = i.get("movie")?;
                    Some(hit(m.pointer("/ids/tmdb")?.as_i64()?, m.get("title")?.as_str()?, m.get("year").and_then(|y| y.as_i64()).unwrap_or(0) as i32))
                })
                .collect())
        }
        ListKind::RssList { url } => {
            let body = http().get(url).send().map_err(|e| e.to_string())?.text().map_err(|e| e.to_string())?;
            let doc = roxmltree::Document::parse(&body).map_err(|e| format!("Not a valid RSS feed: {e}"))?;
            let mut imdb = Vec::new();
            let mut titles = Vec::new();
            let re = regex::Regex::new(r"tt\d{7,8}").unwrap();
            for item in doc.descendants().filter(|n| n.has_tag_name("item")) {
                let text = |n: &str| item.children().find(|c| c.tag_name().name() == n).and_then(|c| c.text()).unwrap_or("").to_string();
                let joined = format!("{} {} {}", text("link"), text("guid"), text("description"));
                if let Some(m) = re.find(&joined) {
                    imdb.push(m.as_str().to_string());
                } else if let Some(p) = crate::parser::parse_movie_title(&text("title"), false) {
                    titles.push((p.primary_title().to_string(), p.year));
                }
            }
            Ok(resolve(core, imdb, titles))
        }
        ListKind::PlexWatchlist { rss_url } => {
            let body = http().get(rss_url).header("Accept", "application/json").send().map_err(|e| e.to_string())?.text().map_err(|e| e.to_string())?;
            let mut out = Vec::new();
            let mut imdb = Vec::new();
            if let Ok(v) = serde_json::from_str::<Value>(&body) {
                for item in v.get("items").and_then(|i| i.as_array()).into_iter().flatten() {
                    if item.get("category").and_then(|c| c.as_str()).map(|c| c != "movie").unwrap_or(false) {
                        continue;
                    }
                    let guids: Vec<String> = item.get("guids").and_then(|g| g.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
                    if let Some(t) = guids.iter().find_map(|g| g.strip_prefix("tmdb://").and_then(|x| x.parse::<i64>().ok())) {
                        out.push(hit(t, item.get("title").and_then(|t| t.as_str()).unwrap_or(""), 0));
                    } else if let Some(i) = guids.iter().find_map(|g| g.strip_prefix("imdb://")) {
                        imdb.push(i.to_string());
                    }
                }
            } else {
                let re = regex::Regex::new(r"tmdb://(\d+)").unwrap();
                for c in re.captures_iter(&body) {
                    out.push(hit(c[1].parse().unwrap_or(0), "", 0));
                }
            }
            out.extend(resolve(core, imdb, vec![]));
            Ok(out)
        }
        ListKind::TurksList { url, api_key } => {
            let v: Value = http()
                .get(format!("{}/api/v3/movie", url.trim_end_matches('/')))
                .header("X-Api-Key", api_key.as_str())
                .send()
                .map_err(|e| e.to_string())?
                .json()
                .map_err(|e| e.to_string())?;
            Ok(v.as_array()
                .into_iter()
                .flatten()
                .filter_map(|m| Some(hit(m.get("tmdbId")?.as_i64()?, m.get("title").and_then(|t| t.as_str()).unwrap_or(""), m.get("year").and_then(|y| y.as_i64()).unwrap_or(0) as i32)))
                .collect())
        }
    }
}

pub fn sync_all(core: &Core, cid: u64) -> Result<String, String> {
    let lists: Vec<ImportList> = core.lib().lists.values().filter(|l| l.enabled).cloned().collect();
    if lists.is_empty() {
        return Ok(String::new());
    }
    let excluded: HashSet<i64> = core.lib().exclusions.values().map(|e| e.tmdb_id).collect();
    let mut on_lists: HashSet<i64> = HashSet::new();
    let mut added = 0;
    let mut failures = 0;
    for (i, list) in lists.iter().enumerate() {
        core.progress(cid, format!("{} ({}/{})", list.name, i + 1, lists.len()));
        let hits = match fetch(core, list) {
            Ok(h) => h,
            Err(e) => {
                failures += 1;
                crate::applog::warn(format!("Import list '{}': {e}", list.name));
                continue;
            }
        };
        let mut l = list.clone();
        l.last_sync = Some(chrono::Utc::now());
        core.save(l);
        for h in hits {
            if h.tmdb_id <= 0 {
                continue;
            }
            on_lists.insert(h.tmdb_id);
            if !list.auto_add || excluded.contains(&h.tmdb_id) || core.lib().movie_by_tmdb(h.tmdb_id).is_some() {
                continue;
            }
            if list.root_folder.is_empty() || list.quality_profile_id == 0 {
                continue;
            }
            let opts = AddOptions {
                root_folder: list.root_folder.clone(),
                quality_profile_id: list.quality_profile_id,
                minimum_availability: list.minimum_availability,
                monitor: list.monitor,
                tags: list.tags.clone(),
                search: list.search_on_add,
            };
            match library::add_movie(core, h.tmdb_id, &opts) {
                Ok(m) => {
                    added += 1;
                    crate::applog::info(format!("Import list '{}' added {}", list.name, m.display()));
                }
                Err(e) => crate::applog::warn(format!("Couldn't add {} from '{}': {e}", h.title, list.name)),
            }
        }
    }
    // Library cleanup for movies that left every list.
    let mode = core.cfg().list_sync_clean_library;
    if mode != "Disabled" && failures == 0 {
        let gone: Vec<Movie> = core.lib().movies.values().filter(|m| !on_lists.contains(&m.tmdb_id)).cloned().collect();
        for m in gone {
            match mode.as_str() {
                "LogOnly" => crate::applog::info(format!("{} isn't on any import list", m.display())),
                "KeepAndUnmonitor" if m.monitored => {
                    core.save(Movie { monitored: false, ..m });
                }
                "RemoveAndKeep" => {
                    let _ = library::delete_movie(core, m.id, false, false);
                }
                "RemoveAndDelete" => {
                    let _ = library::delete_movie(core, m.id, true, false);
                }
                _ => {}
            }
        }
    }
    Ok(format!("Added {added}"))
}
