//! Library operations: adding, refreshing, rescanning, renaming, deleting
//! movies and following collections.

use std::path::{Path, PathBuf};

use chrono::Utc;

use crate::core::{Command, Core};
use crate::import::{self, Transfer};
use crate::model::*;
use crate::notify::{self, Event};

#[derive(Clone, Debug)]
pub struct AddOptions {
    pub root_folder: String,
    pub quality_profile_id: Id,
    pub minimum_availability: Availability,
    pub monitor: ListMonitor,
    pub tags: Vec<Id>,
    pub search: bool,
}

/// Adds a movie by TMDb id. Returns the new movie (or the existing one).
pub fn add_movie(core: &Core, tmdb_id: i64, opts: &AddOptions) -> Result<Movie, String> {
    if let Some(m) = core.lib().movie_by_tmdb(tmdb_id) {
        return Ok(m.clone());
    }
    let cfg = core.cfg();
    let mut movie = core.tmdb().movie(tmdb_id)?;
    movie.monitored = opts.monitor != ListMonitor::None;
    movie.minimum_availability = opts.minimum_availability;
    movie.quality_profile_id = opts.quality_profile_id;
    movie.root_folder = opts.root_folder.clone();
    movie.tags = opts.tags.clone();
    movie.added = Some(Utc::now());
    movie.last_info_sync = Some(Utc::now());
    movie.path = Path::new(&opts.root_folder).join(crate::naming::folder_name(&movie, &cfg)).display().to_string();
    if cfg.create_empty_folders {
        let _ = std::fs::create_dir_all(&movie.path);
    }
    let movie = core.save(movie);
    cache_images(&movie);
    crate::applog::info(format!("Added {}", movie.display()));
    notify::dispatch(core, Event::MovieAdded { movie: movie.clone() });

    if opts.monitor == ListMonitor::MovieAndCollection {
        if let Some(c) = &movie.collection {
            let _ = follow_collection(core, c.tmdb_id, opts, true);
        }
    }
    // Pick up a file that's already sitting in the folder.
    if Path::new(&movie.path).is_dir() {
        let _ = rescan_one(core, movie.id);
    }
    if opts.search && movie.monitored {
        core.run(Command::SearchMovies(vec![movie.id]));
    }
    Ok(core.movie(movie.id).unwrap_or(movie))
}

pub fn cache_images(movie: &Movie) {
    let (p, f, id) = (movie.poster_path.clone(), movie.fanart_path.clone(), movie.tmdb_id);
    std::thread::spawn(move || {
        crate::tmdb::fetch_image(&p, "w342", &crate::tmdb::image_path("poster", id));
        crate::tmdb::fetch_image(&f, "w1280", &crate::tmdb::image_path("fanart", id));
    });
}

/// Updates metadata from TMDb, keeping library settings.
pub fn refresh_one(core: &Core, id: Id) -> Result<(), String> {
    let Some(old) = core.movie(id) else { return Ok(()) };
    let fresh = match core.tmdb().movie(old.tmdb_id) {
        Ok(m) => m,
        Err(e) if e.contains("Not found") => {
            let mut m = old.clone();
            m.status = MovieStatus::Deleted;
            core.save(m);
            return Err(format!("{} was removed from TMDb", old.display()));
        }
        Err(e) => return Err(e),
    };
    let poster_changed = fresh.poster_path != old.poster_path;
    let m = Movie {
        id: old.id,
        monitored: old.monitored,
        minimum_availability: old.minimum_availability,
        quality_profile_id: old.quality_profile_id,
        root_folder: old.root_folder.clone(),
        path: old.path.clone(),
        tags: old.tags.clone(),
        added: old.added,
        last_search: old.last_search,
        movie_file_id: old.movie_file_id,
        last_info_sync: Some(Utc::now()),
        ..fresh
    };
    if poster_changed {
        let _ = std::fs::remove_file(crate::tmdb::image_path("poster", m.tmdb_id));
    }
    cache_images(&m);
    let file = m.movie_file_id.and_then(|f| core.lib().files.get(&f).cloned());
    crate::metadata::write(&m, file.as_ref(), &core.cfg());
    core.save(m);
    Ok(())
}

pub fn refresh_movies(core: &Core, ids: &[Id], cid: u64) -> Result<String, String> {
    let ids: Vec<Id> = if ids.is_empty() { core.lib().movies.keys().copied().collect() } else { ids.to_vec() };
    let mut errors = 0;
    for (i, id) in ids.iter().enumerate() {
        if core.shutting_down.load(std::sync::atomic::Ordering::SeqCst) {
            break;
        }
        // Scheduled refresh skips movies refreshed in the last 12 hours unless they're new.
        if ids.len() > 1 {
            if let Some(m) = core.movie(*id) {
                let recent = m.last_info_sync.map(|t| Utc::now() - t < chrono::Duration::hours(12)).unwrap_or(false);
                if recent {
                    continue;
                }
            }
        }
        core.progress(cid, format!("Refreshing {}/{}", i + 1, ids.len()));
        if let Err(e) = refresh_one(core, *id) {
            errors += 1;
            crate::applog::warn(e);
        }
        let rescan = match core.cfg().rescan_after_refresh {
            crate::config::RescanAfterRefresh::Always => true,
            crate::config::RescanAfterRefresh::AfterManual => ids.len() == 1,
            crate::config::RescanAfterRefresh::Never => false,
        };
        if rescan {
            let _ = rescan_one(core, *id);
        }
    }
    Ok(if errors > 0 { format!("{errors} couldn't be refreshed") } else { String::new() })
}

/// Checks the movie folder: notices deleted files and picks up new ones.
pub fn rescan_one(core: &Core, id: Id) -> Result<(), String> {
    let cfg = core.cfg();
    let Some(mut movie) = core.movie(id) else { return Ok(()) };
    let folder = PathBuf::from(&movie.path);
    // Existing file gone?
    if let Some(file) = core.lib().file_for(&movie).cloned() {
        if !Path::new(&file.path).exists() {
            core.remove::<MovieFile>(file.id);
            movie.movie_file_id = None;
            if cfg.unmonitor_deleted_movies {
                movie.monitored = false;
            }
            core.save(movie.clone());
            core.history(History {
                movie_id: id,
                event: HistoryEvent::FileDeleted,
                source_title: file.relative_path.clone(),
                quality: file.quality,
                data: [("reason".to_string(), "MissingFromDisk".to_string())].into(),
                ..Default::default()
            });
            notify::dispatch(core, Event::FileDelete { movie: movie.clone(), file, reason: "Missing from disk".into() });
        } else {
            // Size changes (e.g. replaced by hand) are refreshed.
            let size = std::fs::metadata(&file.path).map(|m| m.len()).unwrap_or(file.size);
            if size != file.size {
                core.save(MovieFile { size, ..file });
            }
            return Ok(());
        }
    }
    if !folder.is_dir() {
        return Ok(());
    }
    // No file recorded: adopt the largest video in the folder.
    let mut vids = import::video_files(&folder);
    vids.sort_by_key(|p| std::cmp::Reverse(std::fs::metadata(p).map(|m| m.len()).unwrap_or(0)));
    if let Some(v) = vids.first() {
        let mut dec = import::evaluate(&core.lib(), &cfg, v, Some(id), None, true);
        dec.rejections.retain(|r| !r.starts_with("Not an upgrade") && !r.starts_with("This is already"));
        if dec.rejections.iter().any(|r| r == "Sample file") {
            return Ok(());
        }
        dec.rejections.clear();
        import::import(core, &dec, Transfer::InPlace, None)?;
    }
    Ok(())
}

pub fn rescan_movies(core: &Core, ids: &[Id], cid: u64) -> Result<String, String> {
    let ids: Vec<Id> = if ids.is_empty() { core.lib().movies.keys().copied().collect() } else { ids.to_vec() };
    for (i, id) in ids.iter().enumerate() {
        core.progress(cid, format!("Rescanning {}/{}", i + 1, ids.len()));
        if let Err(e) = rescan_one(core, *id) {
            crate::applog::warn(e);
        }
    }
    Ok(String::new())
}

pub fn delete_movie(core: &Core, id: Id, delete_files: bool, add_exclusion: bool) -> Result<(), String> {
    let Some(movie) = core.movie(id) else { return Ok(()) };
    let cfg = core.cfg();
    if let Some(f) = movie.movie_file_id {
        core.remove::<MovieFile>(f);
    }
    if delete_files && !movie.path.is_empty() && Path::new(&movie.path).exists() {
        import::recycle(Path::new(&movie.path), &cfg)?;
    }
    if add_exclusion {
        core.save(ListExclusion { id: 0, tmdb_id: movie.tmdb_id, title: movie.title.clone(), year: movie.year });
    }
    let pending: Vec<Id> = core.lib().pending.values().filter(|p| p.movie_id == id).map(|p| p.id).collect();
    for p in pending {
        core.remove::<Pending>(p);
    }
    core.remove::<Movie>(id);
    core.db.delete_history_for_movie(id);
    let _ = std::fs::remove_dir_all(crate::config::cache_dir().join(movie.tmdb_id.to_string()));
    crate::applog::info(format!("Removed {}", movie.display()));
    notify::dispatch(core, Event::MovieDelete { movie, files_deleted: delete_files });
    Ok(())
}

pub fn delete_file(core: &Core, file_id: Id, reason: &str) -> Result<(), String> {
    let cfg = core.cfg();
    let Some(file) = core.lib().files.get(&file_id).cloned() else { return Ok(()) };
    import::recycle(Path::new(&file.path), &cfg)?;
    core.remove::<MovieFile>(file_id);
    if let Some(mut m) = core.movie(file.movie_id) {
        m.movie_file_id = None;
        if cfg.unmonitor_deleted_movies {
            m.monitored = false;
        }
        let m = core.save(m);
        core.history(History {
            movie_id: m.id,
            event: HistoryEvent::FileDeleted,
            source_title: file.relative_path.clone(),
            quality: file.quality,
            data: [("reason".to_string(), reason.to_string())].into(),
            ..Default::default()
        });
        notify::dispatch(core, Event::FileDelete { movie: m, file, reason: reason.into() });
    }
    Ok(())
}

/// Moves a movie to another folder (e.g. a new root folder).
pub fn move_movie(core: &Core, id: Id, new_path: &str, move_files: bool) -> Result<(), String> {
    let Some(mut m) = core.movie(id) else { return Ok(()) };
    let old = PathBuf::from(&m.path);
    let new = PathBuf::from(new_path);
    if move_files && old.exists() && old != new {
        if new.exists() && std::fs::read_dir(&new).map(|mut d| d.next().is_some()).unwrap_or(false) {
            return Err(format!("{} already exists and isn't empty", new.display()));
        }
        if std::fs::rename(&old, &new).is_err() {
            for e in walkdir::WalkDir::new(&old).into_iter().flatten() {
                let rel = e.path().strip_prefix(&old).unwrap();
                let target = new.join(rel);
                if e.file_type().is_dir() {
                    std::fs::create_dir_all(&target).map_err(|e| e.to_string())?;
                } else {
                    import::transfer(e.path(), &target, Transfer::Move)?;
                }
            }
            let _ = std::fs::remove_dir_all(&old);
        }
    }
    m.path = new.display().to_string();
    if let Some(root) = new.parent() {
        m.root_folder = root.display().to_string();
    }
    if let Some(mut f) = core.lib().file_for(&m).cloned() {
        f.path = new.join(&f.relative_path).display().to_string();
        core.save(f);
    }
    core.save(m);
    Ok(())
}

// ---------------------------------------------------------------- renaming

#[derive(Clone, Debug, serde::Serialize)]
pub struct RenamePreview {
    pub movie_id: Id,
    pub file_id: Id,
    pub existing: String,
    pub new: String,
}

pub fn rename_preview(core: &Core, ids: &[Id]) -> Vec<RenamePreview> {
    let cfg = core.cfg();
    let lib = core.lib();
    let mut out = Vec::new();
    for id in ids {
        let Some(m) = lib.movies.get(id) else { continue };
        let Some(f) = lib.file_for(m) else { continue };
        let ext = Path::new(&f.relative_path).extension().map(|e| e.to_string_lossy().to_string()).unwrap_or_default();
        let formats: Vec<String> = lib.formats.values().filter(|cf| cf.include_when_renaming && crate::formats::format_matches(cf, &crate::decision::input_for_file(f, m))).map(|cf| cf.name.clone()).collect();
        let new = format!("{}.{ext}", crate::naming::file_name(m, f, formats, &cfg));
        if new != f.relative_path {
            out.push(RenamePreview { movie_id: m.id, file_id: f.id, existing: f.relative_path.clone(), new });
        }
    }
    out
}

pub fn rename_movies(core: &Core, ids: &[Id], cid: u64) -> Result<String, String> {
    let ids: Vec<Id> = if ids.is_empty() { core.lib().movies.keys().copied().collect() } else { ids.to_vec() };
    let previews = rename_preview(core, &ids);
    let mut done = 0;
    for (i, p) in previews.iter().enumerate() {
        core.progress(cid, format!("Renaming {}/{}", i + 1, previews.len()));
        let Some(m) = core.movie(p.movie_id) else { continue };
        let Some(mut f) = core.lib().files.get(&p.file_id).cloned() else { continue };
        let folder = PathBuf::from(&m.path);
        let src = PathBuf::from(&f.path);
        let dst = folder.join(&p.new);
        match import::transfer(&src, &dst, Transfer::Move) {
            Ok(_) => {
                // Extras that shared the old name follow along.
                let old_stem = src.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                let new_stem = dst.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                for e in std::fs::read_dir(&folder).into_iter().flatten().flatten() {
                    let name = e.file_name().to_string_lossy().to_string();
                    if name.starts_with(&format!("{old_stem}.")) && e.path() != dst {
                        let _ = std::fs::rename(e.path(), folder.join(name.replacen(&old_stem, &new_stem, 1)));
                    }
                }
                core.history(History {
                    movie_id: m.id,
                    event: HistoryEvent::FileRenamed,
                    source_title: f.relative_path.clone(),
                    quality: f.quality,
                    data: [("path".to_string(), dst.display().to_string())].into(),
                    ..Default::default()
                });
                f.relative_path = p.new.clone();
                f.path = dst.display().to_string();
                core.save(f);
                done += 1;
                notify::dispatch(core, Event::Rename { movie: m });
            }
            Err(e) => crate::applog::warn(format!("Couldn't rename {}: {e}", src.display())),
        }
    }
    Ok(format!("Renamed {done} files"))
}

// ---------------------------------------------------------------- wanted

pub fn cutoff_unmet(core: &Core) -> Vec<Movie> {
    let lib = core.lib();
    lib.movies
        .values()
        .filter(|m| m.monitored)
        .filter_map(|m| {
            let f = lib.file_for(m)?;
            let p = lib.profile_for(m)?;
            let fmts: Vec<Id> = crate::decision::file_formats(&lib, f, m).into_iter().map(|x| x.0).collect();
            crate::decision::cutoff_not_met(p, &f.quality, &fmts, None).then(|| m.clone())
        })
        .collect()
}

// ---------------------------------------------------------------- collections

pub fn follow_collection(core: &Core, tmdb_id: i64, opts: &AddOptions, monitored: bool) -> Result<MovieCollection, String> {
    let existing = core.lib().collections.values().find(|c| c.tmdb_id == tmdb_id).cloned();
    let (title, overview, poster, parts) = core.tmdb().collection(tmdb_id)?;
    let c = MovieCollection {
        id: existing.as_ref().map(|c| c.id).unwrap_or(0),
        tmdb_id,
        title,
        overview,
        poster_path: poster,
        monitored: existing.as_ref().map(|c| c.monitored).unwrap_or(monitored),
        quality_profile_id: existing.as_ref().map(|c| c.quality_profile_id).unwrap_or(opts.quality_profile_id),
        root_folder: existing.as_ref().map(|c| c.root_folder.clone()).unwrap_or(opts.root_folder.clone()),
        minimum_availability: existing.as_ref().map(|c| c.minimum_availability).unwrap_or(opts.minimum_availability),
        search_on_add: existing.as_ref().map(|c| c.search_on_add).unwrap_or(opts.search),
        tags: existing.map(|c| c.tags).unwrap_or(opts.tags.clone()),
        parts,
    };
    let c = core.save(c);
    let poster = c.poster_path.clone();
    let id = c.tmdb_id;
    std::thread::spawn(move || {
        crate::tmdb::fetch_image(&poster, "w342", &crate::tmdb::image_path("collection", id));
    });
    if c.monitored {
        add_missing_from_collection(core, &c);
    }
    Ok(c)
}

pub fn add_missing_from_collection(core: &Core, c: &MovieCollection) -> usize {
    if c.root_folder.is_empty() || c.quality_profile_id == 0 {
        return 0;
    }
    let excluded: Vec<i64> = core.lib().exclusions.values().map(|e| e.tmdb_id).collect();
    let opts = AddOptions {
        root_folder: c.root_folder.clone(),
        quality_profile_id: c.quality_profile_id,
        minimum_availability: c.minimum_availability,
        monitor: ListMonitor::MovieOnly,
        tags: c.tags.clone(),
        search: c.search_on_add,
    };
    let mut added = 0;
    for p in &c.parts {
        if core.lib().movie_by_tmdb(p.tmdb_id).is_some() || excluded.contains(&p.tmdb_id) {
            continue;
        }
        match add_movie(core, p.tmdb_id, &opts) {
            Ok(_) => added += 1,
            Err(e) => crate::applog::warn(format!("Couldn't add {} from {}: {e}", p.title, c.title)),
        }
    }
    added
}

pub fn refresh_collections(core: &Core, cid: u64) -> Result<String, String> {
    // Any collection a library movie belongs to is listed.
    let refs: Vec<i64> = {
        let lib = core.lib();
        let mut ids: Vec<i64> = lib.movies.values().filter_map(|m| m.collection.as_ref().map(|c| c.tmdb_id)).collect();
        ids.extend(lib.collections.values().map(|c| c.tmdb_id));
        ids.sort();
        ids.dedup();
        ids
    };
    let default_opts = {
        let lib = core.lib();
        AddOptions {
            root_folder: lib.root_folders.values().next().map(|r| r.path.clone()).unwrap_or_default(),
            quality_profile_id: lib.profiles.keys().next().copied().unwrap_or(0),
            minimum_availability: Availability::Released,
            monitor: ListMonitor::MovieOnly,
            tags: vec![],
            search: false,
        }
    };
    for (i, id) in refs.iter().enumerate() {
        core.progress(cid, format!("Collection {}/{}", i + 1, refs.len()));
        if let Err(e) = follow_collection(core, *id, &default_opts, false) {
            crate::applog::warn(format!("Collection {id}: {e}"));
        }
    }
    Ok(format!("{} collections", refs.len()))
}

// ---------------------------------------------------------------- housekeeping

pub fn housekeeping(core: &Core) -> Result<String, String> {
    // Orphaned files, finished downloads and stale pending releases.
    let (orphans, old_downloads, stale_pending): (Vec<Id>, Vec<Id>, Vec<Id>) = {
        let lib = core.lib();
        let orphans = lib.files.values().filter(|f| !lib.movies.contains_key(&f.movie_id)).map(|f| f.id).collect();
        let cutoff = Utc::now() - chrono::Duration::days(14);
        let old = lib.downloads.values().filter(|d| d.state.finished() && d.added.map(|a| a < cutoff).unwrap_or(true)).map(|d| d.id).collect();
        let stale = lib.pending.values().filter(|p| !lib.movies.contains_key(&p.movie_id) || p.release_at.map(|r| r < Utc::now() - chrono::Duration::days(7)).unwrap_or(true)).map(|p| p.id).collect();
        (orphans, old, stale)
    };
    for id in orphans {
        core.remove::<MovieFile>(id);
    }
    for id in old_downloads {
        core.remove::<Download>(id);
    }
    for id in stale_pending {
        core.remove::<Pending>(id);
    }
    // Images for movies no longer in the library.
    let keep: std::collections::HashSet<String> = {
        let lib = core.lib();
        let mut k: std::collections::HashSet<String> = lib.movies.values().map(|m| m.tmdb_id.to_string()).collect();
        k.extend(lib.collections.values().map(|c| c.tmdb_id.to_string()));
        k.insert("people".into());
        k.insert("discover".into());
        k
    };
    for e in std::fs::read_dir(crate::config::cache_dir()).into_iter().flatten().flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if !keep.contains(&name) {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
    core.db.trim_history(Utc::now() - chrono::Duration::days(365 * 3));
    crate::backup::prune(&core.cfg());
    Ok(String::new())
}

pub fn clean_recycle_bin(core: &Core) -> Result<String, String> {
    let cfg = core.cfg();
    if cfg.recycle_bin.trim().is_empty() || cfg.recycle_bin_cleanup_days <= 0 {
        return Ok(String::new());
    }
    let limit = std::time::Duration::from_secs(cfg.recycle_bin_cleanup_days as u64 * 86400);
    let mut removed = 0;
    for e in std::fs::read_dir(cfg.recycle_bin.trim()).into_iter().flatten().flatten() {
        let old = e.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).map(|age| age > limit).unwrap_or(false);
        if old {
            let p = e.path();
            if if p.is_dir() { std::fs::remove_dir_all(&p) } else { std::fs::remove_file(&p) }.is_ok() {
                removed += 1;
            }
        }
    }
    Ok(format!("Removed {removed} items"))
}
