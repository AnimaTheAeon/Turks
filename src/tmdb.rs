//! TMDb metadata: search, movie details, collections, lists and images.
//! Accepts either a v3 API key or a v4 read access token.

use std::path::PathBuf;
use std::time::Duration;

use chrono::NaiveDate;
use serde_json::Value;

use crate::model::{CollectionPart, CollectionRef, Credit, Movie, MovieStatus};

fn base() -> String {
    std::env::var("TURKS_TMDB_URL").unwrap_or_else(|_| "https://api.themoviedb.org/3".into())
}
pub const IMG: &str = "https://image.tmdb.org/t/p";

fn img_base() -> String {
    std::env::var("TURKS_TMDB_IMG").unwrap_or_else(|_| IMG.into())
}

#[derive(Clone)]
pub struct Tmdb {
    key: String,
    language: String,
    country: String,
    http: reqwest::blocking::Client,
}

#[derive(Clone, Debug, Default)]
pub struct SearchHit {
    pub tmdb_id: i64,
    pub title: String,
    pub original_title: String,
    pub year: i32,
    pub overview: String,
    pub poster_path: String,
    pub popularity: f32,
    pub vote: f32,
}

pub fn http_client(timeout_secs: u64) -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(timeout_secs))
        .user_agent(format!("Turks/{}", crate::updater::APP_VERSION))
        .build()
        .unwrap_or_default()
}

fn date(v: &Value) -> Option<NaiveDate> {
    v.as_str().and_then(|s| NaiveDate::parse_from_str(s.get(..10)?, "%Y-%m-%d").ok())
}

fn s(v: &Value, k: &str) -> String {
    v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string()
}

fn year_of(v: &Value) -> i32 {
    v.get("release_date").and_then(|d| d.as_str()).and_then(|d| d.get(..4)).and_then(|y| y.parse().ok()).unwrap_or(0)
}

impl Tmdb {
    pub fn new(key: &str, language: &str, country: &str) -> Self {
        Self {
            key: key.trim().to_string(),
            language: if language.is_empty() { "en".into() } else { language.into() },
            country: if country.is_empty() { "US".into() } else { country.to_uppercase() },
            http: http_client(30),
        }
    }

    fn get(&self, path: &str, query: &[(&str, String)]) -> Result<Value, String> {
        if self.key.is_empty() {
            return Err("No TMDb API key yet. Add one in Settings › General.".into());
        }
        let mut req = self.http.get(format!("{}{path}", base()));
        let mut q: Vec<(&str, String)> = query.to_vec();
        if !q.iter().any(|(k, _)| *k == "language") {
            q.push(("language", self.language.clone()));
        }
        if self.key.starts_with("eyJ") {
            req = req.bearer_auth(&self.key);
        } else {
            q.push(("api_key", self.key.clone()));
        }
        let mut last_err = String::new();
        for attempt in 0..3 {
            let resp = req.try_clone().unwrap().query(&q).send();
            match resp {
                Ok(r) if r.status().as_u16() == 429 => {
                    std::thread::sleep(Duration::from_millis(1500 * (attempt + 1)));
                    last_err = "TMDb is rate limiting".into();
                }
                Ok(r) if r.status().as_u16() == 401 => return Err("TMDb rejected the API key.".into()),
                Ok(r) if r.status().as_u16() == 404 => return Err("Not found on TMDb".into()),
                Ok(r) if !r.status().is_success() => return Err(format!("TMDb returned HTTP {}", r.status().as_u16())),
                Ok(r) => return r.json::<Value>().map_err(|e| e.to_string()),
                Err(e) => {
                    last_err = format!("Couldn't reach TMDb: {e}");
                    std::thread::sleep(Duration::from_millis(800));
                }
            }
        }
        Err(last_err)
    }

    pub fn test(&self) -> Result<(), String> {
        self.get("/configuration", &[]).map(|_| ())
    }

    fn hits(v: &Value) -> Vec<SearchHit> {
        v.get("results")
            .or_else(|| v.get("items"))
            .or_else(|| v.get("cast"))
            .and_then(|r| r.as_array())
            .map(|a| a.iter().filter(|m| m.get("media_type").and_then(|t| t.as_str()).unwrap_or("movie") == "movie").map(Self::hit).collect())
            .unwrap_or_default()
    }

    fn hit(m: &Value) -> SearchHit {
        SearchHit {
            tmdb_id: m.get("id").and_then(|i| i.as_i64()).unwrap_or(0),
            title: s(m, "title"),
            original_title: s(m, "original_title"),
            year: year_of(m),
            overview: s(m, "overview"),
            poster_path: s(m, "poster_path"),
            popularity: m.get("popularity").and_then(|p| p.as_f64()).unwrap_or(0.0) as f32,
            vote: m.get("vote_average").and_then(|p| p.as_f64()).unwrap_or(0.0) as f32,
        }
    }

    /// Accepts plain titles, "Title 2019", "tmdb:123" and "imdb:tt123" / "tt123".
    pub fn search(&self, term: &str) -> Result<Vec<SearchHit>, String> {
        let t = term.trim();
        let lower = t.to_ascii_lowercase();
        if let Some(id) = lower.strip_prefix("tmdb:").or_else(|| lower.strip_prefix("tmdbid:")) {
            let id: i64 = id.trim().parse().map_err(|_| "That TMDb id isn't a number.")?;
            let m = self.get(&format!("/movie/{id}"), &[])?;
            return Ok(vec![Self::hit(&m)]);
        }
        let imdb = lower.strip_prefix("imdb:").unwrap_or(&lower).trim().to_string();
        if imdb.starts_with("tt") && imdb[2..].chars().all(|c| c.is_ascii_digit()) && imdb.len() > 3 {
            return self.find_imdb(&imdb).map(|h| h.into_iter().collect());
        }
        // "Movie Title 2019" or "Movie Title (2019)"
        let mut query = t.to_string();
        let mut year = None;
        let cleaned = t.trim_end_matches(')').trim_end();
        if cleaned.len() > 5 {
            let tail = &cleaned[cleaned.len() - 4..];
            if tail.chars().all(|c| c.is_ascii_digit()) {
                let y: i32 = tail.parse().unwrap_or(0);
                if (1880..=2100).contains(&y) {
                    let head = cleaned[..cleaned.len() - 4].trim_end().trim_end_matches('(').trim_end();
                    if !head.is_empty() {
                        query = head.to_string();
                        year = Some(y);
                    }
                }
            }
        }
        let mut q = vec![("query", query.clone()), ("include_adult", "false".to_string())];
        if let Some(y) = year {
            q.push(("year", y.to_string()));
        }
        let mut hits = Self::hits(&self.get("/search/movie", &q)?);
        if hits.is_empty() && year.is_some() {
            hits = Self::hits(&self.get("/search/movie", &[("query", query), ("include_adult", "false".into())])?);
        }
        Ok(hits)
    }

    pub fn search_title_year(&self, title: &str, year: i32) -> Result<Vec<SearchHit>, String> {
        let mut q = vec![("query", title.to_string()), ("include_adult", "false".to_string())];
        if year > 0 {
            q.push(("primary_release_year", year.to_string()));
        }
        let mut hits = Self::hits(&self.get("/search/movie", &q)?);
        if hits.is_empty() && year > 0 {
            q.pop();
            q.push(("year", year.to_string()));
            hits = Self::hits(&self.get("/search/movie", &q)?);
        }
        Ok(hits)
    }

    pub fn find_imdb(&self, imdb: &str) -> Result<Option<SearchHit>, String> {
        let v = self.get(&format!("/find/{imdb}"), &[("external_source", "imdb_id".into())])?;
        Ok(v.get("movie_results").and_then(|r| r.as_array()).and_then(|a| a.first()).map(Self::hit))
    }

    /// Full details, filled into a Movie (library fields left default).
    pub fn movie(&self, tmdb_id: i64) -> Result<Movie, String> {
        let v = self.get(
            &format!("/movie/{tmdb_id}"),
            &[("append_to_response", "credits,release_dates,external_ids,videos,recommendations,alternative_titles,keywords".into())],
        )?;
        let mut m = Movie {
            tmdb_id,
            imdb_id: v.get("imdb_id").and_then(|x| x.as_str()).unwrap_or("").to_string(),
            title: s(&v, "title"),
            original_title: s(&v, "original_title"),
            original_language: crate::parser::language::from_iso(&s(&v, "original_language")).unwrap_or("English").to_string(),
            overview: s(&v, "overview"),
            runtime: v.get("runtime").and_then(|x| x.as_i64()).unwrap_or(0) as i32,
            year: year_of(&v),
            genres: v.get("genres").and_then(|g| g.as_array()).map(|a| a.iter().map(|g| s(g, "name")).collect()).unwrap_or_default(),
            studio: v
                .get("production_companies")
                .and_then(|g| g.as_array())
                .and_then(|a| a.first())
                .map(|c| s(c, "name"))
                .unwrap_or_default(),
            popularity: v.get("popularity").and_then(|p| p.as_f64()).unwrap_or(0.0) as f32,
            poster_path: s(&v, "poster_path"),
            fanart_path: s(&v, "backdrop_path"),
            website: s(&v, "homepage"),
            ..Default::default()
        };
        m.ratings.tmdb = v.get("vote_average").and_then(|x| x.as_f64()).unwrap_or(0.0) as f32;
        m.ratings.tmdb_votes = v.get("vote_count").and_then(|x| x.as_i64()).unwrap_or(0);
        if let Some(c) = v.get("belongs_to_collection").filter(|c| !c.is_null()) {
            m.collection = Some(CollectionRef { tmdb_id: c.get("id").and_then(|i| i.as_i64()).unwrap_or(0), title: s(c, "name") });
        }
        if let Some(alts) = v.pointer("/alternative_titles/titles").and_then(|a| a.as_array()) {
            let mut seen = std::collections::BTreeSet::new();
            for a in alts {
                let t = s(a, "title");
                if !t.is_empty() && t != m.title && seen.insert(t.clone()) {
                    m.alt_titles.push(t);
                }
            }
        }
        if !m.original_title.is_empty() && m.original_title != m.title && !m.alt_titles.contains(&m.original_title) {
            m.alt_titles.insert(0, m.original_title.clone());
        }
        if let Some(k) = v.pointer("/keywords/keywords").and_then(|a| a.as_array()) {
            m.keywords = k.iter().map(|k| s(k, "name")).collect();
        }
        if let Some(vids) = v.pointer("/videos/results").and_then(|a| a.as_array()) {
            if let Some(t) = vids.iter().find(|x| s(x, "site") == "YouTube" && s(x, "type") == "Trailer") {
                m.youtube_trailer = s(t, "key");
            }
        }
        if let Some(r) = v.pointer("/recommendations/results").and_then(|a| a.as_array()) {
            m.recommendations = r.iter().filter_map(|x| x.get("id").and_then(|i| i.as_i64())).take(20).collect();
        }
        if let Some(cast) = v.pointer("/credits/cast").and_then(|a| a.as_array()) {
            m.cast = cast
                .iter()
                .take(20)
                .map(|c| Credit { name: s(c, "name"), role: s(c, "character"), department: "Acting".into(), person_id: c.get("id").and_then(|i| i.as_i64()).unwrap_or(0), profile_path: s(c, "profile_path") })
                .collect();
        }
        if let Some(crew) = v.pointer("/credits/crew").and_then(|a| a.as_array()) {
            m.crew = crew
                .iter()
                .filter(|c| matches!(s(c, "job").as_str(), "Director" | "Screenplay" | "Writer" | "Producer" | "Original Music Composer" | "Director of Photography"))
                .take(15)
                .map(|c| Credit { name: s(c, "name"), role: s(c, "job"), department: s(c, "department"), person_id: c.get("id").and_then(|i| i.as_i64()).unwrap_or(0), profile_path: s(c, "profile_path") })
                .collect();
        }
        // Release dates: prefer the configured country, fall back to the earliest anywhere.
        let mut theatrical: Option<NaiveDate> = None;
        let mut digital: Option<NaiveDate> = None;
        let mut physical: Option<NaiveDate> = None;
        let (mut any_t, mut any_d, mut any_p) = (None::<NaiveDate>, None::<NaiveDate>, None::<NaiveDate>);
        if let Some(countries) = v.pointer("/release_dates/results").and_then(|a| a.as_array()) {
            for c in countries {
                let mine = s(c, "iso_3166_1") == self.country;
                for rd in c.get("release_dates").and_then(|a| a.as_array()).into_iter().flatten() {
                    let Some(d) = date(rd.get("release_date").unwrap_or(&Value::Null)) else { continue };
                    let kind = rd.get("type").and_then(|t| t.as_i64()).unwrap_or(0);
                    if mine && m.certification.is_empty() {
                        let cert = s(rd, "certification");
                        if !cert.is_empty() {
                            m.certification = cert;
                        }
                    }
                    let (slot, any) = match kind {
                        1..=3 => (&mut theatrical, &mut any_t),
                        4 => (&mut digital, &mut any_d),
                        5 => (&mut physical, &mut any_p),
                        _ => continue,
                    };
                    if mine {
                        *slot = Some(slot.map_or(d, |x| x.min(d)));
                    }
                    *any = Some(any.map_or(d, |x| x.min(d)));
                }
            }
        }
        m.in_cinemas = theatrical.or(any_t).or_else(|| date(v.get("release_date").unwrap_or(&Value::Null)));
        m.digital_release = digital.or(any_d);
        m.physical_release = physical.or(any_p);
        m.status = derive_status(&m, &s(&v, "status"));
        m.clean_title = crate::parser::clean_title(&m.title);
        m.sort_title = crate::parser::sort_title(&m.title);
        Ok(m)
    }

    pub fn collection(&self, id: i64) -> Result<(String, String, String, Vec<CollectionPart>), String> {
        let v = self.get(&format!("/collection/{id}"), &[])?;
        let mut parts: Vec<CollectionPart> = v
            .get("parts")
            .and_then(|p| p.as_array())
            .map(|a| {
                a.iter()
                    .map(|p| CollectionPart { tmdb_id: p.get("id").and_then(|i| i.as_i64()).unwrap_or(0), title: s(p, "title"), year: year_of(p), poster_path: s(p, "poster_path"), overview: s(p, "overview") })
                    .collect()
            })
            .unwrap_or_default();
        parts.sort_by_key(|p| if p.year == 0 { i32::MAX } else { p.year });
        Ok((s(&v, "name"), s(&v, "overview"), s(&v, "poster_path"), parts))
    }

    fn paged(&self, path: &str, query: &[(&str, String)], max_pages: u32) -> Result<Vec<SearchHit>, String> {
        let mut out = Vec::new();
        for page in 1..=max_pages {
            let mut q = query.to_vec();
            q.push(("page", page.to_string()));
            let v = self.get(path, &q)?;
            let hits = Self::hits(&v);
            let total = v.get("total_pages").and_then(|t| t.as_u64()).unwrap_or(1) as u32;
            out.extend(hits);
            if page >= total {
                break;
            }
        }
        Ok(out)
    }

    /// "popular", "top_rated", "now_playing", "upcoming" or "trending".
    pub fn chart(&self, which: &str, pages: u32) -> Result<Vec<SearchHit>, String> {
        if which == "trending" {
            return self.paged("/trending/movie/week", &[], pages);
        }
        self.paged(&format!("/movie/{which}"), &[("region", self.country.clone())], pages)
    }

    pub fn list(&self, list_id: &str) -> Result<Vec<SearchHit>, String> {
        let v = self.get(&format!("/list/{}", list_id.trim()), &[])?;
        Ok(Self::hits(&v))
    }

    pub fn person_movies(&self, person_id: &str, cast: bool, crew: bool) -> Result<Vec<SearchHit>, String> {
        let v = self.get(&format!("/person/{}/movie_credits", person_id.trim()), &[])?;
        let mut out = Vec::new();
        if cast {
            out.extend(v.get("cast").and_then(|a| a.as_array()).into_iter().flatten().map(Self::hit));
        }
        if crew {
            out.extend(
                v.get("crew")
                    .and_then(|a| a.as_array())
                    .into_iter()
                    .flatten()
                    .filter(|c| matches!(s(c, "job").as_str(), "Director" | "Screenplay" | "Writer" | "Producer"))
                    .map(Self::hit),
            );
        }
        out.sort_by_key(|h| h.tmdb_id);
        out.dedup_by_key(|h| h.tmdb_id);
        Ok(out)
    }

    pub fn discover(&self, key: &str, value: &str, pages: u32) -> Result<Vec<SearchHit>, String> {
        self.paged("/discover/movie", &[(key, value.trim().to_string()), ("sort_by", "popularity.desc".into())], pages)
    }

}

pub fn derive_status(m: &Movie, tmdb_status: &str) -> MovieStatus {
    if tmdb_status == "Canceled" {
        return MovieStatus::Deleted;
    }
    let today = chrono::Local::now().date_naive();
    let released = [m.digital_release, m.physical_release].into_iter().flatten().any(|d| d <= today)
        || m.in_cinemas.map(|d| d + chrono::Duration::days(90) <= today).unwrap_or(false);
    if released {
        return MovieStatus::Released;
    }
    if m.in_cinemas.map(|d| d <= today).unwrap_or(false) {
        return MovieStatus::InCinemas;
    }
    if m.in_cinemas.is_some() || m.digital_release.is_some() || m.physical_release.is_some() || matches!(tmdb_status, "In Production" | "Post Production" | "Planned") {
        return MovieStatus::Announced;
    }
    if tmdb_status == "Released" {
        return MovieStatus::Released;
    }
    MovieStatus::Tba
}

// ---------------------------------------------------------------- images

pub fn image_path(kind: &str, tmdb_id: i64) -> PathBuf {
    crate::config::cache_dir().join(tmdb_id.to_string()).join(format!("{kind}.jpg"))
}

pub fn person_image_path(person_id: i64) -> PathBuf {
    crate::config::cache_dir().join("people").join(format!("{person_id}.jpg"))
}

/// Downloads an image once; returns where it is.
pub fn fetch_image(url_path: &str, size: &str, dest: &PathBuf) -> Option<PathBuf> {
    if url_path.is_empty() {
        return None;
    }
    if dest.exists() {
        return Some(dest.clone());
    }
    let url = if url_path.starts_with("http") { url_path.to_string() } else { format!("{}/{size}{url_path}", img_base()) };
    let bytes = http_client(30).get(url).send().ok().filter(|r| r.status().is_success())?.bytes().ok()?;
    if bytes.len() < 200 {
        return None;
    }
    if let Some(parent) = dest.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let tmp = dest.with_extension("part");
    std::fs::write(&tmp, &bytes).ok()?;
    std::fs::rename(&tmp, dest).ok()?;
    Some(dest.clone())
}
