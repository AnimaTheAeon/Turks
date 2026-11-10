//! SQLite storage. Everything except history is loaded into memory at start
//! (a library of a few thousand movies is small) and written through on change.

use std::collections::BTreeMap;
use std::sync::Mutex;

use rusqlite::{params, Connection};
use serde::{de::DeserializeOwned, Serialize};

use crate::model::*;

pub struct Db {
    conn: Mutex<Connection>,
}

pub trait Entity: Serialize + DeserializeOwned + Clone {
    const KIND: &'static str;
    fn id(&self) -> Id;
    fn set_id(&mut self, id: Id);
    fn map(lib: &Library) -> &BTreeMap<Id, Self>;
    fn map_mut(lib: &mut Library) -> &mut BTreeMap<Id, Self>;
}

macro_rules! entity {
    ($t:ty, $kind:literal, $field:ident) => {
        impl Entity for $t {
            const KIND: &'static str = $kind;
            fn id(&self) -> Id {
                self.id
            }
            fn set_id(&mut self, id: Id) {
                self.id = id;
            }
            fn map(lib: &Library) -> &BTreeMap<Id, Self> {
                &lib.$field
            }
            fn map_mut(lib: &mut Library) -> &mut BTreeMap<Id, Self> {
                &mut lib.$field
            }
        }
    };
}

#[derive(Default, Clone)]
pub struct Library {
    pub movies: BTreeMap<Id, Movie>,
    pub files: BTreeMap<Id, MovieFile>,
    pub profiles: BTreeMap<Id, QualityProfile>,
    pub formats: BTreeMap<Id, CustomFormat>,
    pub delay_profiles: BTreeMap<Id, DelayProfile>,
    pub release_profiles: BTreeMap<Id, ReleaseProfile>,
    pub indexers: BTreeMap<Id, Indexer>,
    pub clients: BTreeMap<Id, DownloadClient>,
    pub notifications: BTreeMap<Id, Notification>,
    pub lists: BTreeMap<Id, ImportList>,
    pub exclusions: BTreeMap<Id, ListExclusion>,
    pub tags: BTreeMap<Id, Tag>,
    pub root_folders: BTreeMap<Id, RootFolder>,
    pub remote_paths: BTreeMap<Id, RemotePathMapping>,
    pub collections: BTreeMap<Id, MovieCollection>,
    pub downloads: BTreeMap<Id, Download>,
    pub pending: BTreeMap<Id, Pending>,
    pub blocklist: BTreeMap<Id, Blocklist>,
    /// Quality sizes keyed by quality id.
    pub sizes: BTreeMap<Id, QualitySize>,
}

entity!(Movie, "movie", movies);
entity!(MovieFile, "file", files);
entity!(QualityProfile, "profile", profiles);
entity!(CustomFormat, "format", formats);
entity!(DelayProfile, "delay", delay_profiles);
entity!(ReleaseProfile, "restriction", release_profiles);
entity!(Indexer, "indexer", indexers);
entity!(DownloadClient, "client", clients);
entity!(Notification, "notification", notifications);
entity!(ImportList, "list", lists);
entity!(ListExclusion, "exclusion", exclusions);
entity!(Tag, "tag", tags);
entity!(RootFolder, "root", root_folders);
entity!(RemotePathMapping, "remotepath", remote_paths);
entity!(MovieCollection, "collection", collections);
entity!(Download, "download", downloads);
entity!(Pending, "pending", pending);
entity!(Blocklist, "blocklist", blocklist);

impl Entity for QualitySize {
    const KIND: &'static str = "size";
    fn id(&self) -> Id {
        self.quality as Id
    }
    fn set_id(&mut self, id: Id) {
        self.quality = id as i32;
    }
    fn map(lib: &Library) -> &BTreeMap<Id, Self> {
        &lib.sizes
    }
    fn map_mut(lib: &mut Library) -> &mut BTreeMap<Id, Self> {
        &mut lib.sizes
    }
}

impl Library {
    pub fn movie_by_tmdb(&self, tmdb_id: i64) -> Option<&Movie> {
        self.movies.values().find(|m| m.tmdb_id == tmdb_id)
    }
    pub fn file_for(&self, movie: &Movie) -> Option<&MovieFile> {
        movie.movie_file_id.and_then(|id| self.files.get(&id))
    }
    pub fn profile_for(&self, movie: &Movie) -> Option<&QualityProfile> {
        self.profiles.get(&movie.quality_profile_id).or_else(|| self.profiles.values().next())
    }
    pub fn tag_label(&self, id: Id) -> String {
        self.tags.get(&id).map(|t| t.label.clone()).unwrap_or_default()
    }
    pub fn tag_id(&self, label: &str) -> Option<Id> {
        self.tags.values().find(|t| t.label.eq_ignore_ascii_case(label)).map(|t| t.id)
    }
    /// Downloads not yet imported or failed.
    pub fn active_downloads(&self) -> impl Iterator<Item = &Download> {
        self.downloads.values().filter(|d| !d.state.finished())
    }
    pub fn size_for(&self, quality: i32) -> QualitySize {
        self.sizes.get(&(quality as Id)).cloned().unwrap_or_else(|| QualitySize { quality, ..Default::default() })
    }
    pub fn missing_count(&self) -> usize {
        self.movies.values().filter(|m| m.monitored && m.movie_file_id.is_none()).count()
    }
}

impl Db {
    pub fn open(path: &std::path::Path) -> Result<Self, String> {
        let conn = Connection::open(path).map_err(|e| e.to_string())?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
             CREATE TABLE IF NOT EXISTS items (kind TEXT NOT NULL, id INTEGER NOT NULL, json TEXT NOT NULL, PRIMARY KEY(kind, id));
             CREATE TABLE IF NOT EXISTS history (id INTEGER PRIMARY KEY AUTOINCREMENT, movie_id INTEGER NOT NULL, date TEXT NOT NULL, event TEXT NOT NULL, download_id TEXT NOT NULL DEFAULT '', json TEXT NOT NULL);
             CREATE INDEX IF NOT EXISTS history_movie ON history(movie_id);
             CREATE INDEX IF NOT EXISTS history_date ON history(date);
             CREATE INDEX IF NOT EXISTS history_dl ON history(download_id);
             CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
        )
        .map_err(|e| e.to_string())?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn load_all<T: Entity>(&self, lib: &mut Library) {
        let conn = self.lock();
        let Ok(mut st) = conn.prepare("SELECT id, json FROM items WHERE kind = ?1") else { return };
        let rows = st.query_map(params![T::KIND], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)));
        if let Ok(rows) = rows {
            for (id, json) in rows.flatten() {
                match serde_json::from_str::<T>(&json) {
                    Ok(mut v) => {
                        v.set_id(id);
                        T::map_mut(lib).insert(id, v);
                    }
                    Err(e) => crate::applog::warn(format!("Skipped a damaged {} record ({id}): {e}", T::KIND)),
                }
            }
        }
    }

    pub fn load_library(&self) -> Library {
        let mut lib = Library::default();
        self.load_all::<Movie>(&mut lib);
        self.load_all::<MovieFile>(&mut lib);
        self.load_all::<QualityProfile>(&mut lib);
        self.load_all::<CustomFormat>(&mut lib);
        self.load_all::<DelayProfile>(&mut lib);
        self.load_all::<ReleaseProfile>(&mut lib);
        self.load_all::<Indexer>(&mut lib);
        self.load_all::<DownloadClient>(&mut lib);
        self.load_all::<Notification>(&mut lib);
        self.load_all::<ImportList>(&mut lib);
        self.load_all::<ListExclusion>(&mut lib);
        self.load_all::<Tag>(&mut lib);
        self.load_all::<RootFolder>(&mut lib);
        self.load_all::<RemotePathMapping>(&mut lib);
        self.load_all::<MovieCollection>(&mut lib);
        self.load_all::<Download>(&mut lib);
        self.load_all::<Pending>(&mut lib);
        self.load_all::<Blocklist>(&mut lib);
        self.load_all::<QualitySize>(&mut lib);
        lib
    }

    pub fn put<T: Entity>(&self, item: &T) {
        let json = match serde_json::to_string(item) {
            Ok(j) => j,
            Err(e) => {
                crate::applog::error(format!("Couldn't save {}: {e}", T::KIND));
                return;
            }
        };
        let r = self.lock().execute(
            "INSERT INTO items(kind, id, json) VALUES (?1, ?2, ?3) ON CONFLICT(kind, id) DO UPDATE SET json = excluded.json",
            params![T::KIND, item.id(), json],
        );
        if let Err(e) = r {
            crate::applog::error(format!("Database write failed: {e}"));
        }
    }

    pub fn put_many<T: Entity>(&self, items: &[T]) {
        let mut conn = self.lock();
        let Ok(tx) = conn.transaction() else { return };
        for item in items {
            if let Ok(json) = serde_json::to_string(item) {
                let _ = tx.execute(
                    "INSERT INTO items(kind, id, json) VALUES (?1, ?2, ?3) ON CONFLICT(kind, id) DO UPDATE SET json = excluded.json",
                    params![T::KIND, item.id(), json],
                );
            }
        }
        let _ = tx.commit();
    }

    pub fn delete<T: Entity>(&self, id: Id) {
        let _ = self.lock().execute("DELETE FROM items WHERE kind = ?1 AND id = ?2", params![T::KIND, id]);
    }

    // ------------------------------------------------------------ history

    pub fn add_history(&self, h: &History) -> Id {
        let json = serde_json::to_string(h).unwrap_or_default();
        let date = h.date.unwrap_or_else(chrono::Utc::now).to_rfc3339();
        let conn = self.lock();
        let _ = conn.execute(
            "INSERT INTO history(movie_id, date, event, download_id, json) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![h.movie_id, date, format!("{:?}", h.event), h.download_id, json],
        );
        conn.last_insert_rowid()
    }

    fn read_history(&self, sql: &str, p: &[&dyn rusqlite::ToSql]) -> Vec<History> {
        let conn = self.lock();
        let Ok(mut st) = conn.prepare(sql) else { return vec![] };
        let rows = st.query_map(p, |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)));
        let mut out = Vec::new();
        if let Ok(rows) = rows {
            for (id, json) in rows.flatten() {
                if let Ok(mut h) = serde_json::from_str::<History>(&json) {
                    h.id = id;
                    out.push(h);
                }
            }
        }
        out
    }

    pub fn history_page(&self, offset: i64, limit: i64, event: Option<HistoryEvent>) -> Vec<History> {
        match event {
            Some(e) => self.read_history(
                "SELECT id, json FROM history WHERE event = ?1 ORDER BY date DESC LIMIT ?2 OFFSET ?3",
                &[&format!("{e:?}"), &limit, &offset],
            ),
            None => self.read_history("SELECT id, json FROM history ORDER BY date DESC LIMIT ?1 OFFSET ?2", &[&limit, &offset]),
        }
    }

    pub fn history_count(&self, event: Option<HistoryEvent>) -> i64 {
        let conn = self.lock();
        match event {
            Some(e) => conn.query_row("SELECT COUNT(*) FROM history WHERE event = ?1", params![format!("{e:?}")], |r| r.get(0)),
            None => conn.query_row("SELECT COUNT(*) FROM history", [], |r| r.get(0)),
        }
        .unwrap_or(0)
    }

    pub fn history_for_movie(&self, movie_id: Id) -> Vec<History> {
        self.read_history("SELECT id, json FROM history WHERE movie_id = ?1 ORDER BY date DESC", &[&movie_id])
    }

    pub fn history_for_download(&self, download_id: &str) -> Vec<History> {
        self.read_history("SELECT id, json FROM history WHERE download_id = ?1 ORDER BY date DESC", &[&download_id])
    }

    pub fn history_since(&self, since: chrono::DateTime<chrono::Utc>) -> Vec<History> {
        self.read_history("SELECT id, json FROM history WHERE date >= ?1 ORDER BY date DESC", &[&since.to_rfc3339()])
    }

    pub fn delete_history_for_movie(&self, movie_id: Id) {
        let _ = self.lock().execute("DELETE FROM history WHERE movie_id = ?1", params![movie_id]);
    }

    pub fn clear_history(&self) {
        let _ = self.lock().execute("DELETE FROM history", []);
    }

    pub fn trim_history(&self, older_than: chrono::DateTime<chrono::Utc>) {
        let _ = self.lock().execute("DELETE FROM history WHERE date < ?1", params![older_than.to_rfc3339()]);
    }

    // ------------------------------------------------------------ meta

    pub fn meta(&self, key: &str) -> Option<String> {
        self.lock().query_row("SELECT value FROM meta WHERE key = ?1", params![key], |r| r.get(0)).ok()
    }

    pub fn set_meta(&self, key: &str, value: &str) {
        let _ = self.lock().execute(
            "INSERT INTO meta(key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        );
    }

    /// Consistent copy for backups.
    pub fn backup_to(&self, path: &std::path::Path) -> Result<(), String> {
        let _ = std::fs::remove_file(path);
        self.lock()
            .execute("VACUUM INTO ?1", params![path.display().to_string()])
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

/// Next free id for a kind.
pub fn next_id<T: Entity>(lib: &Library) -> Id {
    T::map(lib).keys().next_back().copied().unwrap_or(0) + 1
}
