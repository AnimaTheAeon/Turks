//! Newznab (usenet) and Torznab (torrent) indexers, and importing indexers
//! from Prowlarr.

use chrono::{DateTime, Utc};

use crate::model::{Indexer, IndexerKind, Movie, Protocol};
use crate::release::Release;

#[derive(Clone, Debug, Default)]
pub struct Caps {
    pub movie_search: bool,
    pub imdb: bool,
    pub tmdb: bool,
    pub q: bool,
    pub categories: Vec<(i32, String)>,
}

fn client() -> reqwest::blocking::Client {
    crate::tmdb::http_client(60)
}

fn api_url(ix: &Indexer) -> String {
    let base = ix.url.trim().trim_end_matches('/');
    let path = if ix.api_path.trim().is_empty() { "/api" } else { ix.api_path.trim() };
    format!("{base}{}", if path.starts_with('/') { path.to_string() } else { format!("/{path}") })
}

fn get(ix: &Indexer, params: &[(&str, String)]) -> Result<String, String> {
    let mut q: Vec<(String, String)> = params.iter().map(|(k, v)| (k.to_string(), v.clone())).collect();
    if !ix.api_key.is_empty() {
        q.push(("apikey".into(), ix.api_key.clone()));
    }
    for extra in ix.extra_params.split('&').filter(|s| s.contains('=')) {
        let (k, v) = extra.split_once('=').unwrap();
        q.push((k.trim().into(), v.trim().into()));
    }
    let resp = client().get(api_url(ix)).query(&q).send().map_err(|e| format!("{}: couldn't connect ({e})", ix.name))?;
    let status = resp.status();
    let body = resp.text().map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("{}: HTTP {}", ix.name, status.as_u16()));
    }
    // Newznab errors come back as <error code=".." description=".."/>
    if let Ok(doc) = roxmltree::Document::parse(&body) {
        let root = doc.root_element();
        if root.tag_name().name() == "error" {
            let code = root.attribute("code").unwrap_or("");
            let desc = root.attribute("description").unwrap_or("unknown error");
            return Err(format!("{}: {desc} (code {code})", ix.name));
        }
    } else if body.trim_start().starts_with('<') {
        return Err(format!("{}: the response wasn't valid XML", ix.name));
    } else {
        return Err(format!("{}: unexpected response (is the URL right?)", ix.name));
    }
    Ok(body)
}

pub fn caps(ix: &Indexer) -> Result<Caps, String> {
    let body = get(ix, &[("t", "caps".into())])?;
    let doc = roxmltree::Document::parse(&body).map_err(|e| e.to_string())?;
    let mut caps = Caps::default();
    for n in doc.descendants() {
        match n.tag_name().name() {
            "movie-search" => {
                caps.movie_search = n.attribute("available") == Some("yes");
                let p = n.attribute("supportedParams").unwrap_or("q").to_lowercase();
                caps.imdb = caps.movie_search && p.split(',').any(|x| x.trim() == "imdbid");
                caps.tmdb = caps.movie_search && p.split(',').any(|x| x.trim() == "tmdbid");
                caps.q = p.split(',').any(|x| x.trim() == "q");
            }
            "category" | "subcat" => {
                if let (Some(id), Some(name)) = (n.attribute("id").and_then(|i| i.parse().ok()), n.attribute("name")) {
                    caps.categories.push((id, name.to_string()));
                }
            }
            _ => {}
        }
    }
    Ok(caps)
}

pub fn test(ix: &Indexer) -> Result<String, String> {
    let caps = caps(ix)?;
    let rss = fetch(ix, &[("t", "search".into())])?;
    Ok(format!(
        "Connected. Movie search: {}{}. Latest feed has {} releases.",
        if caps.movie_search { "yes" } else { "no (title search only)" },
        if caps.imdb || caps.tmdb { ", ID search supported" } else { "" },
        rss.len()
    ))
}

fn cats(ix: &Indexer) -> String {
    ix.categories.iter().map(|c| c.to_string()).collect::<Vec<_>>().join(",")
}

fn fetch(ix: &Indexer, params: &[(&str, String)]) -> Result<Vec<Release>, String> {
    let mut p = params.to_vec();
    p.push(("cat", cats(ix)));
    p.push(("extended", "1".into()));
    p.push(("limit", "100".into()));
    let body = get(ix, &p)?;
    Ok(parse_feed(ix, &body))
}

/// Latest releases (RSS sync).
pub fn rss(ix: &Indexer) -> Result<Vec<Release>, String> {
    fetch(ix, &[("t", "search".into())])
}

/// Searches for one movie, by ID where supported, else by title and year.
pub fn search(ix: &Indexer, movie: &Movie, caps: &Caps) -> Result<Vec<Release>, String> {
    let mut out = Vec::new();
    if caps.movie_search && (caps.imdb || caps.tmdb) {
        let mut p = vec![("t", "movie".to_string())];
        if caps.imdb && !movie.imdb_id.is_empty() {
            p.push(("imdbid", movie.imdb_id.trim_start_matches("tt").to_string()));
        }
        if caps.tmdb && movie.tmdb_id > 0 {
            p.push(("tmdbid", movie.tmdb_id.to_string()));
        }
        if p.len() > 1 {
            out = fetch(ix, &p)?;
        }
    }
    if out.is_empty() {
        let mut titles = vec![movie.title.clone()];
        if !movie.original_title.is_empty() && movie.original_title != movie.title && movie.original_title.is_ascii() {
            titles.push(movie.original_title.clone());
        }
        for t in titles {
            let q = format!("{} {}", clean_query(&t), movie.year);
            let kind = if caps.movie_search { "movie" } else { "search" };
            let found = fetch(ix, &[("t", kind.into()), ("q", q)])?;
            out.extend(found);
            if !out.is_empty() {
                break;
            }
        }
    }
    Ok(out)
}

fn clean_query(t: &str) -> String {
    let s: String = t.chars().map(|c| if c.is_alphanumeric() || c == ' ' || c == '\'' { c } else { ' ' }).collect();
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn parse_feed(ix: &Indexer, body: &str) -> Vec<Release> {
    let Ok(doc) = roxmltree::Document::parse(body) else { return vec![] };
    let protocol = ix.kind.protocol();
    let mut out = Vec::new();
    for item in doc.descendants().filter(|n| n.has_tag_name("item")) {
        let child = |name: &str| item.children().find(|c| c.tag_name().name() == name && c.tag_name().namespace().is_none()).and_then(|c| c.text()).unwrap_or("").trim().to_string();
        let mut r = Release {
            title: child("title"),
            guid: child("guid"),
            download_url: child("link"),
            comments_url: child("comments"),
            indexer_id: ix.id,
            indexer: ix.name.clone(),
            indexer_priority: ix.priority,
            protocol,
            minimum_ratio: ix.seed_ratio,
            minimum_seed_time: ix.seed_time,
            ..Default::default()
        };
        r.publish_date = DateTime::parse_from_rfc2822(&child("pubDate")).ok().map(|d| d.with_timezone(&Utc));
        if let Some(enc) = item.children().find(|c| c.has_tag_name("enclosure")) {
            if let Some(u) = enc.attribute("url") {
                if r.download_url.is_empty() || protocol == Protocol::Usenet {
                    r.download_url = u.to_string();
                }
            }
            if let Some(len) = enc.attribute("length").and_then(|l| l.parse().ok()) {
                r.size = len;
            }
        }
        if r.size == 0 {
            r.size = child("size").parse().unwrap_or(0);
        }
        let mut dvf: Option<f64> = None;
        let mut uvf: Option<f64> = None;
        for a in item.children().filter(|c| c.tag_name().name() == "attr") {
            let (Some(name), Some(value)) = (a.attribute("name"), a.attribute("value")) else { continue };
            match name {
                "size" => r.size = value.parse().unwrap_or(r.size),
                "seeders" => r.seeders = value.parse().ok(),
                "peers" => r.peers = value.parse().ok(),
                "leechers" => {
                    if r.peers.is_none() {
                        r.peers = value.parse::<i32>().ok().map(|l| l + r.seeders.unwrap_or(0));
                    }
                }
                "infohash" => r.info_hash = value.to_lowercase(),
                "magneturl" => r.magnet_url = value.to_string(),
                "imdb" | "imdbid" => {
                    if let Some(id) = crate::parser::normalize_imdb_id(value) {
                        r.imdb_id = id;
                    }
                }
                "tmdbid" | "tmdb" => r.tmdb_id = value.parse().unwrap_or(0),
                "category" => {
                    if let Ok(c) = value.parse() {
                        r.categories.push(c);
                    }
                }
                "downloadvolumefactor" => dvf = value.parse().ok(),
                "uploadvolumefactor" => uvf = value.parse().ok(),
                "minimumratio" => r.minimum_ratio = value.parse().ok().or(r.minimum_ratio),
                "minimumseedtime" => r.minimum_seed_time = value.parse().ok().or(r.minimum_seed_time),
                "grabs" => r.grabs = value.parse().ok(),
                "tag" => {
                    let v = value.to_lowercase();
                    if ["freeleech", "internal", "scene", "nuked", "doubleupload", "halfleech"].contains(&v.as_str()) && !r.indexer_flags.contains(&v) {
                        r.indexer_flags.push(v);
                    }
                }
                _ => {}
            }
        }
        if let Some(d) = dvf {
            let flag = if d == 0.0 {
                "freeleech"
            } else if d <= 0.25 {
                "freeleech75"
            } else if d <= 0.5 {
                "halfleech"
            } else if d <= 0.75 {
                "freeleech25"
            } else {
                ""
            };
            if !flag.is_empty() && !r.indexer_flags.iter().any(|f| f == flag) {
                r.indexer_flags.push(flag.into());
            }
        }
        if uvf.map(|u| u >= 2.0).unwrap_or(false) {
            r.indexer_flags.push("doubleupload".into());
        }
        if r.download_url.starts_with("magnet:") {
            r.magnet_url = r.download_url.clone();
        }
        if r.info_hash.is_empty() && !r.magnet_url.is_empty() {
            r.info_hash = magnet_hash(&r.magnet_url).unwrap_or_default();
        }
        if r.guid.is_empty() {
            r.guid = format!("{}-{}", ix.id, r.download_url);
        }
        if !r.title.is_empty() && (!r.download_url.is_empty() || !r.magnet_url.is_empty()) {
            out.push(r);
        }
    }
    out
}

pub fn magnet_hash(magnet: &str) -> Option<String> {
    let lower = magnet.to_lowercase();
    let i = lower.find("xt=urn:btih:")?;
    let rest = &magnet[i + 12..];
    let h: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
    if h.len() == 40 {
        Some(h.to_lowercase())
    } else if h.len() == 32 {
        base32_to_hex(&h)
    } else {
        None
    }
}

fn base32_to_hex(s: &str) -> Option<String> {
    let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut bits: u64 = 0;
    let mut n = 0;
    let mut out = Vec::new();
    for c in s.to_uppercase().bytes() {
        let v = alphabet.iter().position(|a| *a == c)? as u64;
        bits = (bits << 5) | v;
        n += 5;
        if n >= 8 {
            n -= 8;
            out.push(((bits >> n) & 0xff) as u8);
        }
    }
    Some(out.iter().map(|b| format!("{b:02x}")).collect())
}

/// Indexers set up in Prowlarr, as Torznab/Newznab entries going through it.
pub fn from_prowlarr(url: &str, api_key: &str) -> Result<Vec<Indexer>, String> {
    let base = url.trim().trim_end_matches('/');
    let resp = client()
        .get(format!("{base}/api/v1/indexer"))
        .header("X-Api-Key", api_key.trim())
        .send()
        .map_err(|e| format!("Couldn't reach Prowlarr: {e}"))?;
    if resp.status().as_u16() == 401 {
        return Err("Prowlarr rejected the API key.".into());
    }
    if !resp.status().is_success() {
        return Err(format!("Prowlarr returned HTTP {}", resp.status().as_u16()));
    }
    let list: Vec<serde_json::Value> = resp.json().map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for v in list {
        if !v.get("enable").and_then(|e| e.as_bool()).unwrap_or(true) {
            continue;
        }
        let id = v.get("id").and_then(|i| i.as_i64()).unwrap_or(0);
        let name = v.get("name").and_then(|n| n.as_str()).unwrap_or("Prowlarr indexer").to_string();
        let kind = if v.get("protocol").and_then(|p| p.as_str()) == Some("usenet") { IndexerKind::Newznab } else { IndexerKind::Torznab };
        let mut cats: Vec<i32> = v
            .pointer("/capabilities/categories")
            .and_then(|c| c.as_array())
            .map(|a| {
                let mut ids = Vec::new();
                for c in a {
                    let cid = c.get("id").and_then(|i| i.as_i64()).unwrap_or(0) as i32;
                    if (2000..3000).contains(&cid) {
                        ids.push(cid);
                    }
                    for sc in c.get("subCategories").and_then(|s| s.as_array()).into_iter().flatten() {
                        let sid = sc.get("id").and_then(|i| i.as_i64()).unwrap_or(0) as i32;
                        if (2000..3000).contains(&sid) {
                            ids.push(sid);
                        }
                    }
                }
                ids
            })
            .unwrap_or_default();
        if cats.is_empty() {
            cats = Indexer::default().categories;
        }
        out.push(Indexer {
            name: format!("{name} (Prowlarr)"),
            kind,
            url: format!("{base}/{id}/"),
            api_path: "/api".into(),
            api_key: api_key.trim().to_string(),
            categories: cats,
            priority: v.get("priority").and_then(|p| p.as_i64()).unwrap_or(25) as i32,
            prowlarr_id: Some(id),
            ..Default::default()
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_torznab_feed() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0" xmlns:torznab="http://torznab.com/schemas/2015/feed"><channel>
<item><title>The.Matrix.1999.1080p.BluRay.x264-GRP</title><guid>abc</guid>
<link>https://x/dl/1</link><pubDate>Mon, 09 Nov 2026 10:00:00 +0000</pubDate>
<enclosure url="https://x/dl/1" length="12345" type="application/x-bittorrent"/>
<torznab:attr name="seeders" value="42"/><torznab:attr name="peers" value="50"/>
<torznab:attr name="imdbid" value="0133093"/><torznab:attr name="downloadvolumefactor" value="0"/>
<torznab:attr name="infohash" value="ABCDEF0123456789ABCDEF0123456789ABCDEF01"/></item>
</channel></rss>"#;
        let ix = Indexer { id: 3, name: "T".into(), ..Default::default() };
        let r = parse_feed(&ix, xml);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].seeders, Some(42));
        assert_eq!(r[0].imdb_id, "tt0133093");
        assert_eq!(r[0].indexer_flags, vec!["freeleech".to_string()]);
        assert_eq!(r[0].size, 12345);
        assert!(r[0].publish_date.is_some());
        assert_eq!(magnet_hash("magnet:?xt=urn:btih:ABCDEF0123456789ABCDEF0123456789ABCDEF01&dn=x").unwrap(), "abcdef0123456789abcdef0123456789abcdef01");
    }
}
