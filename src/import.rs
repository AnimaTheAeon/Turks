//! Getting files into the library: deciding whether a file is wanted,
//! moving/hardlinking it into place, renaming it and handling upgrades.

use std::path::{Path, PathBuf};

use chrono::Utc;

use crate::config::{Config, FileDate};
use crate::core::Core;
use crate::db::Library;
use crate::model::*;
use crate::notify::{self, Event};
use crate::parser::{self, ParsedMovieInfo};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transfer {
    Move,
    Copy,
    HardlinkOrCopy,
    /// Already in the movie folder: just record it.
    InPlace,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct ImportDecision {
    pub path: PathBuf,
    pub size: u64,
    pub parsed: ParsedMovieInfo,
    pub movie_id: Option<Id>,
    pub quality: QualityModel,
    pub languages: Vec<String>,
    pub release_group: String,
    pub edition: String,
    pub rejections: Vec<String>,
    pub is_upgrade: bool,
}

impl ImportDecision {
    pub fn approved(&self) -> bool {
        self.rejections.is_empty()
    }
}

// ---------------------------------------------------------------- file helpers

pub fn video_files(dir: &Path) -> Vec<PathBuf> {
    if dir.is_file() {
        return if crate::quality::is_video(dir) { vec![dir.to_path_buf()] } else { vec![] };
    }
    let mut out: Vec<PathBuf> = walkdir::WalkDir::new(dir)
        .max_depth(4)
        .into_iter()
        .flatten()
        .filter(|e| e.file_type().is_file())
        .map(|e| e.into_path())
        .filter(|p| crate::quality::is_video(p))
        .filter(|p| {
            // Skip extras folders like Radarr does.
            !p.components().any(|c| {
                let s = c.as_os_str().to_string_lossy().to_lowercase();
                matches!(s.as_str(), "extras" | "featurettes" | "behind the scenes" | "deleted scenes" | "trailers" | "samples" | "sample" | "@eadir" | ".@__thumb")
            })
        })
        .collect();
    out.sort();
    out
}

fn is_sample(path: &Path, size: u64, runtime: i32) -> bool {
    let name = path.file_stem().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
    let named = name.split(|c: char| !c.is_alphanumeric()).any(|w| w == "sample");
    let small = runtime >= 10 && size < 40 * 1_048_576;
    named && size < 300 * 1_048_576 || small
}

pub fn free_space(path: &Path) -> Option<u64> {
    let mut p = path.to_path_buf();
    while !p.exists() {
        p = p.parent()?.to_path_buf();
    }
    fs2::available_space(&p).ok()
}

fn same_volume(a: &Path, b: &Path) -> bool {
    #[cfg(windows)]
    {
        let root = |p: &Path| p.components().next().map(|c| c.as_os_str().to_string_lossy().to_lowercase());
        root(a) == root(b)
    }
    #[cfg(not(windows))]
    {
        use std::os::unix::fs::MetadataExt;
        let dev = |p: &Path| {
            let mut p = p.to_path_buf();
            loop {
                if let Ok(m) = std::fs::metadata(&p) {
                    return Some(m.dev());
                }
                p = p.parent()?.to_path_buf();
            }
        };
        dev(a) == dev(b)
    }
}

pub fn transfer(src: &Path, dst: &Path, mode: Transfer) -> Result<&'static str, String> {
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("Couldn't create {}: {e}", parent.display()))?;
    }
    if dst.exists() && src != dst {
        // Never overwrite silently; keep the old one aside until this finishes.
        let backup = dst.with_extension("turks-old");
        std::fs::rename(dst, &backup).map_err(|e| e.to_string())?;
        let r = transfer(src, dst, mode);
        if r.is_ok() {
            let _ = std::fs::remove_file(&backup);
        } else {
            let _ = std::fs::rename(&backup, dst);
        }
        return r;
    }
    match mode {
        Transfer::InPlace => Ok("in place"),
        Transfer::HardlinkOrCopy => {
            if same_volume(src, dst) && std::fs::hard_link(src, dst).is_ok() {
                return Ok("hardlinked");
            }
            copy_verified(src, dst)?;
            Ok("copied")
        }
        Transfer::Copy => {
            copy_verified(src, dst)?;
            Ok("copied")
        }
        Transfer::Move => {
            if std::fs::rename(src, dst).is_ok() {
                return Ok("moved");
            }
            copy_verified(src, dst)?;
            std::fs::remove_file(src).map_err(|e| format!("Copied, but couldn't remove the original: {e}"))?;
            Ok("moved")
        }
    }
}

fn copy_verified(src: &Path, dst: &Path) -> Result<(), String> {
    let tmp = dst.with_extension("partial~");
    std::fs::copy(src, &tmp).map_err(|e| format!("Copy failed: {e}"))?;
    let a = std::fs::metadata(src).map(|m| m.len()).unwrap_or(0);
    let b = std::fs::metadata(&tmp).map(|m| m.len()).unwrap_or(1);
    if a != b {
        let _ = std::fs::remove_file(&tmp);
        return Err("Copy was incomplete (sizes differ)".into());
    }
    std::fs::rename(&tmp, dst).map_err(|e| e.to_string())
}

/// Moves a file or folder to the recycle bin. With no recycle bin folder set,
/// it goes to the system's own: the Windows Recycle Bin, or the shared
/// folder's #recycle on a Synology. Only if neither exists is it deleted.
pub fn recycle(path: &Path, cfg: &Config) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    if cfg.recycle_bin.trim().is_empty() {
        return system_recycle(path);
    }
    let bin = Path::new(cfg.recycle_bin.trim());
    let parent_name = path.parent().and_then(|p| p.file_name()).map(|n| n.to_os_string()).unwrap_or_default();
    let dest_dir = if path.is_dir() { bin.to_path_buf() } else { bin.join(parent_name) };
    std::fs::create_dir_all(&dest_dir).map_err(|e| e.to_string())?;
    let mut dest = dest_dir.join(path.file_name().unwrap_or_default());
    if dest.exists() {
        dest = dest_dir.join(format!("{}.{}", path.file_name().unwrap_or_default().to_string_lossy(), Utc::now().timestamp()));
    }
    transfer(path, &dest, Transfer::Move).map(|_| ())?;
    // Recycle bin cleanup goes by modification time.
    let _ = filetime_now(&dest);
    Ok(())
}

#[cfg(windows)]
fn system_recycle(path: &Path) -> Result<(), String> {
    match trash::delete(path) {
        Ok(()) => Ok(()),
        // Network drives have no Recycle Bin.
        Err(e) => {
            crate::applog::warn(format!("Couldn't use the Recycle Bin for {} ({e}); deleting it", path.display()));
            delete_for_good(path)
        }
    }
}

#[cfg(not(windows))]
fn system_recycle(path: &Path) -> Result<(), String> {
    match share_recycle_dir(path) {
        Some(bin) => {
            // Keep the folder layout under #recycle, like File Station does.
            let rel = path.strip_prefix(bin.parent().unwrap_or(Path::new("/"))).unwrap_or(path);
            let mut dest = bin.join(rel);
            if dest.exists() {
                dest = PathBuf::from(format!("{}.{}", dest.display(), Utc::now().timestamp()));
            }
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            transfer(path, &dest, Transfer::Move).map(|_| ())
        }
        None => delete_for_good(path),
    }
}

/// The Synology shared folder's recycle bin (/volumeN/share/#recycle), when
/// it's turned on for that share.
#[cfg(not(windows))]
pub fn share_recycle_dir(path: &Path) -> Option<PathBuf> {
    let mut parts = path.components();
    parts.next()?; // "/"
    let volume = parts.next()?.as_os_str().to_str()?;
    if !volume.starts_with("volume") {
        return None;
    }
    let share = parts.next()?.as_os_str();
    let bin = Path::new("/").join(volume).join(share).join("#recycle");
    bin.is_dir().then_some(bin)
}

fn delete_for_good(path: &Path) -> Result<(), String> {
    if path.is_dir() { std::fs::remove_dir_all(path) } else { std::fs::remove_file(path) }.map_err(|e| e.to_string())
}

fn filetime_now(p: &Path) -> std::io::Result<()> {
    let f = std::fs::OpenOptions::new().append(true).open(p)?;
    f.set_modified(std::time::SystemTime::now())
}

fn set_file_date(path: &Path, movie: &Movie, cfg: &Config) {
    let date = match cfg.file_date {
        FileDate::None => return,
        FileDate::InCinemas => movie.in_cinemas,
        FileDate::PhysicalRelease => movie.physical_release.or(movie.digital_release),
    };
    if let Some(d) = date.and_then(|d| d.and_hms_opt(12, 0, 0)) {
        let ts = std::time::UNIX_EPOCH + std::time::Duration::from_secs(d.and_utc().timestamp().max(0) as u64);
        if let Ok(f) = std::fs::OpenOptions::new().append(true).open(path) {
            let _ = f.set_modified(ts);
        }
    }
}

// ---------------------------------------------------------------- decisions

pub fn movie_folder(movie: &Movie, cfg: &Config) -> PathBuf {
    if !movie.path.is_empty() {
        return PathBuf::from(&movie.path);
    }
    Path::new(&movie.root_folder).join(crate::naming::folder_name(movie, cfg))
}

/// Should this file be imported (and into which movie)?
pub fn evaluate(lib: &Library, cfg: &Config, path: &Path, movie_hint: Option<Id>, download: Option<&Download>, forced: bool) -> ImportDecision {
    let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let mut parsed = parser::parse_movie_path(path).unwrap_or_default();
    // A download's own title usually parses better than an obfuscated file name.
    if let Some(d) = download {
        if let Some(p) = parser::parse_movie_title(&d.title, false) {
            if parsed.titles.is_empty() || parsed.quality.quality == crate::quality::id::UNKNOWN {
                let q = if parsed.quality.quality == crate::quality::id::UNKNOWN { p.quality } else { parsed.quality };
                parsed = ParsedMovieInfo { quality: q, ..p };
            }
        }
    }
    let mut dec = ImportDecision { path: path.to_path_buf(), size, ..Default::default() };
    let movie_id = movie_hint.or(download.map(|d| d.movie_id)).or_else(|| {
        let r = crate::release::Release::default();
        crate::decision::map_movie(lib, &parsed, &r, None)
    });
    dec.movie_id = movie_id;
    dec.quality = parsed.quality;
    // Trust the grabbed quality when the file name says nothing.
    if let Some(d) = download {
        if dec.quality.quality == crate::quality::id::UNKNOWN {
            dec.quality = d.quality;
        }
    }
    dec.languages = if parsed.languages.is_empty() { download.map(|d| d.languages.clone()).unwrap_or_default() } else { parsed.languages.clone() };
    dec.release_group = parsed.release_group.clone();
    dec.edition = parsed.edition.clone();
    dec.parsed = parsed;

    let Some(movie) = movie_id.and_then(|id| lib.movies.get(&id)) else {
        dec.rejections.push("Couldn't tell which movie this is".into());
        return dec;
    };
    if is_sample(path, size, movie.runtime) {
        dec.rejections.push("Sample file".into());
    }
    let lower = path.to_string_lossy().to_lowercase();
    if lower.contains("_unpack_") || lower.contains("_failed_") {
        dec.rejections.push("Still unpacking".into());
    }
    if !forced {
        if let Ok(m) = std::fs::metadata(path).and_then(|m| m.modified()) {
            if m.elapsed().map(|e| e.as_secs() < cfg.minimum_age_for_import_secs.max(0) as u64).unwrap_or(false) {
                dec.rejections.push("File was changed moments ago (still being written?)".into());
            }
        }
    }
    if !cfg.skip_free_space_check {
        let dest = movie_folder(movie, cfg);
        if let Some(free) = free_space(&dest) {
            let need = size + cfg.minimum_free_space_mb * 1_048_576;
            if !same_volume(path, &dest) && free < need {
                dec.rejections.push(format!("Not enough free space at {} ({} free)", dest.display(), crate::decision::human_size(free)));
            }
        }
    }
    if let (Some(profile), Some(existing)) = (lib.profile_for(movie), lib.file_for(movie)) {
        if Path::new(&existing.path) == path {
            dec.rejections.push("This is already the movie's file".into());
        } else {
            let cur: Vec<Id> = crate::decision::file_formats(lib, existing, movie).into_iter().map(|f| f.0).collect();
            let tmp = MovieFile { quality: dec.quality, languages: dec.languages.clone(), release_group: dec.release_group.clone(), edition: dec.edition.clone(), scene_name: dec.parsed.release_title.clone(), size, ..Default::default() };
            let new: Vec<Id> = crate::decision::file_formats(lib, &tmp, movie).into_iter().map(|f| f.0).collect();
            match crate::decision::is_upgradable(cfg, profile, &existing.quality, &cur, &dec.quality, &new) {
                Ok(()) => dec.is_upgrade = true,
                Err(why) if !forced => dec.rejections.push(format!("Not an upgrade: {why}")),
                Err(_) => dec.is_upgrade = true,
            }
        }
    }
    dec
}

// ---------------------------------------------------------------- importing

const EXTRA_LANGS: &[&str] = &["en", "eng", "english", "de", "ger", "fr", "fre", "es", "spa", "it", "ita", "nl", "pt", "forced", "sdh", "cc", "hi"];

fn extras_for(video: &Path, cfg: &Config) -> Vec<(PathBuf, String)> {
    if !cfg.import_extra_files {
        return vec![];
    }
    let exts: Vec<String> = cfg.extra_file_extensions.split(',').map(|e| e.trim().trim_start_matches('.').to_lowercase()).filter(|e| !e.is_empty()).collect();
    let Some(dir) = video.parent() else { return vec![] };
    let stem = video.file_stem().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
    let videos_in_dir = std::fs::read_dir(dir).map(|r| r.flatten().filter(|e| crate::quality::is_video(&e.path())).count()).unwrap_or(1);
    let mut out = Vec::new();
    let mut consider = |p: PathBuf| {
        let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        if !exts.contains(&ext) {
            return;
        }
        let name = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let lname = name.to_lowercase();
        // Belongs to this video if it shares its name, or it's the only video around.
        if !lname.starts_with(&stem) && videos_in_dir > 1 {
            return;
        }
        // Keep language/forced suffixes: Movie.en.forced.srt
        let suffix: Vec<&str> = name.rsplit('.').take_while(|part| EXTRA_LANGS.contains(&part.to_lowercase().as_str()) || part.len() == 2).collect();
        let mut tail = suffix.into_iter().rev().collect::<Vec<_>>().join(".");
        if !tail.is_empty() {
            tail = format!(".{tail}");
        }
        out.push((p, format!("{tail}.{ext}")));
    };
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        if e.path().is_file() {
            consider(e.path());
        }
    }
    for sub in ["Subs", "subs", "Subtitles", "subtitles"] {
        for e in std::fs::read_dir(dir.join(sub)).into_iter().flatten().flatten() {
            if e.path().is_file() {
                consider(e.path());
            }
        }
    }
    out
}

/// Imports one approved file. Returns the new MovieFile.
pub fn import(core: &Core, dec: &ImportDecision, mode: Transfer, download: Option<&Download>) -> Result<MovieFile, String> {
    let cfg = core.cfg();
    let movie_id = dec.movie_id.ok_or("No movie")?;
    let mut movie = core.movie(movie_id).ok_or("Movie no longer exists")?;
    let folder = movie_folder(&movie, &cfg);
    if movie.path.is_empty() {
        movie.path = folder.display().to_string();
    }
    let ext = dec.path.extension().map(|e| e.to_string_lossy().to_string()).unwrap_or_else(|| "mkv".into());
    let scene_name = download.map(|d| d.title.clone()).filter(|t| !t.is_empty()).unwrap_or_else(|| dec.parsed.release_title.clone());

    let mut file = MovieFile {
        id: 0,
        movie_id,
        quality: dec.quality,
        languages: dec.languages.clone(),
        release_group: dec.release_group.clone(),
        edition: dec.edition.clone(),
        scene_name: scene_name.clone(),
        original_path: dec.path.display().to_string(),
        size: dec.size,
        date_added: Some(Utc::now()),
        ..Default::default()
    };
    let ffprobe = if cfg.analyse_video_files { crate::mediainfo::ffprobe_path(&cfg.ffprobe_path) } else { None };
    file.media_info = crate::mediainfo::analyse(&dec.path, &scene_name, ffprobe.as_deref());

    let file_name = if cfg.rename_movies && mode != Transfer::InPlace {
        let formats: Vec<String> = {
            let lib = core.lib();
            lib.formats
                .values()
                .filter(|f| f.include_when_renaming && crate::formats::format_matches(f, &crate::decision::input_for_file(&file, &movie)))
                .map(|f| f.name.clone())
                .collect()
        };
        format!("{}.{ext}", crate::naming::file_name(&movie, &file, formats, &cfg))
    } else {
        dec.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
    };
    let dest = if mode == Transfer::InPlace { dec.path.clone() } else { folder.join(&file_name) };

    // Out with the old file (upgrade).
    let old = core.lib().file_for(&movie).cloned();
    if let Some(old) = &old {
        if Path::new(&old.path) != dest && Path::new(&old.path) != dec.path {
            if let Err(e) = recycle(Path::new(&old.path), &cfg) {
                crate::applog::warn(format!("Couldn't remove the old file {}: {e}", old.path));
            }
            core.history(History {
                movie_id,
                event: HistoryEvent::FileDeleted,
                source_title: old.relative_path.clone(),
                quality: old.quality,
                data: [("reason".to_string(), "Upgrade".to_string())].into(),
                ..Default::default()
            });
            notify::dispatch(core, Event::FileDelete { movie: movie.clone(), file: old.clone(), reason: "Upgrade".into() });
        }
    }

    let how = transfer(&dec.path, &dest, mode)?;
    set_file_date(&dest, &movie, &cfg);
    file.path = dest.display().to_string();
    file.relative_path = dest.strip_prefix(&folder).map(|p| p.display().to_string()).unwrap_or_else(|_| file_name.clone());
    file.size = std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(dec.size);

    // Subtitles and other extras.
    if mode != Transfer::InPlace {
        let new_stem = dest.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        for (extra, tail) in extras_for(&dec.path, &cfg) {
            let target = folder.join(format!("{new_stem}{tail}"));
            let m = if mode == Transfer::Move { Transfer::Move } else { Transfer::Copy };
            if let Err(e) = transfer(&extra, &target, m) {
                crate::applog::warn(format!("Couldn't import extra file {}: {e}", extra.display()));
            }
        }
    }

    if let Some(old) = old {
        core.remove::<MovieFile>(old.id);
    }
    let file = core.save(file);
    movie.movie_file_id = Some(file.id);
    let movie = core.save(movie);

    let mut data = std::collections::BTreeMap::new();
    data.insert("importedPath".to_string(), file.path.clone());
    data.insert("droppedPath".to_string(), dec.path.display().to_string());
    data.insert("how".to_string(), how.to_string());
    if let Some(d) = download {
        data.insert("downloadClient".into(), core.lib().clients.get(&d.client_id).map(|c| c.name.clone()).unwrap_or_default());
        data.insert("indexer".into(), d.indexer.clone());
    }
    core.history(History {
        movie_id,
        event: HistoryEvent::Imported,
        source_title: scene_name.clone(),
        quality: file.quality,
        languages: file.languages.clone(),
        download_id: download.map(|d| d.download_id.clone()).unwrap_or_default(),
        custom_format_score: download.map(|d| d.custom_format_score).unwrap_or(0),
        data,
        ..Default::default()
    });
    crate::metadata::write(&movie, Some(&file), &cfg);
    crate::applog::info(format!("Imported {} → {} ({how})", dec.path.display(), file.path));
    notify::dispatch(core, Event::Download { movie, file: file.clone(), upgrade: dec.is_upgrade, source_title: scene_name });
    Ok(file)
}

// ---------------------------------------------------------------- folder scans

/// "Scan downloaded movies": import whatever in a folder matches the library.
pub fn scan_folder(core: &Core, folder: &str, cid: u64) -> Result<String, String> {
    let root = Path::new(folder);
    if !root.exists() {
        return Err(format!("{folder} doesn't exist"));
    }
    let cfg = core.cfg();
    let files = video_files(root);
    let mut imported = 0;
    let mut skipped = Vec::new();
    for (i, f) in files.iter().enumerate() {
        core.progress(cid, format!("Checking {}/{}", i + 1, files.len()));
        let dec = evaluate(&core.lib(), &cfg, f, None, None, false);
        if !dec.approved() {
            skipped.push(format!("{}: {}", f.display(), dec.rejections.join("; ")));
            continue;
        }
        match import(core, &dec, Transfer::Move, None) {
            Ok(_) => imported += 1,
            Err(e) => skipped.push(format!("{}: {e}", f.display())),
        }
    }
    for s in &skipped {
        crate::applog::info(format!("Not imported: {s}"));
    }
    Ok(format!("Imported {imported}, skipped {}", skipped.len()))
}

/// Candidates for the Manual Import window.
pub fn manual_candidates(core: &Core, folder: &Path, movie_hint: Option<Id>) -> Vec<ImportDecision> {
    let cfg = core.cfg();
    let lib = core.lib();
    let download = lib.downloads.values().find(|d| !d.output_path.is_empty() && folder.starts_with(&d.output_path)).cloned();
    video_files(folder).iter().map(|f| evaluate(&lib, &cfg, f, movie_hint.or(download.as_ref().map(|d| d.movie_id)), download.as_ref(), true)).collect()
}

/// A folder in a root folder that isn't a movie yet (Library Import).
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct Unmapped {
    pub path: PathBuf,
    pub name: String,
    pub parsed_title: String,
    pub parsed_year: i32,
    pub tmdb_id_hint: i64,
    pub video_count: usize,
}

pub fn unmapped_folders(lib: &Library, root: &Path) -> Vec<Unmapped> {
    let known: std::collections::HashSet<String> = lib.movies.values().map(|m| m.path.to_lowercase().replace('\\', "/").trim_end_matches('/').to_string()).collect();
    let mut out = Vec::new();
    for e in std::fs::read_dir(root).into_iter().flatten().flatten() {
        let p = e.path();
        if !p.is_dir() {
            continue;
        }
        let name = e.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || name.starts_with('@') || name.starts_with('$') || name.eq_ignore_ascii_case("System Volume Information") {
            continue;
        }
        let key = p.display().to_string().to_lowercase().replace('\\', "/").trim_end_matches('/').to_string();
        if known.contains(&key) {
            continue;
        }
        let vids = video_files(&p);
        if vids.is_empty() {
            continue;
        }
        let parsed = parser::parse_movie_title(&name, true).or_else(|| vids.first().and_then(|v| parser::parse_movie_path(v)));
        let tmdb_hint = Regex_tmdb(&name);
        out.push(Unmapped {
            path: p,
            parsed_title: parsed.as_ref().map(|p| p.primary_title().to_string()).unwrap_or_else(|| name.clone()),
            parsed_year: parsed.as_ref().map(|p| p.year).unwrap_or(0),
            name,
            tmdb_id_hint: tmdb_hint,
            video_count: vids.len(),
        });
    }
    out.sort_by_key(|a| a.name.to_lowercase());
    out
}

#[allow(non_snake_case)]
fn Regex_tmdb(name: &str) -> i64 {
    let lower = name.to_lowercase();
    for key in ["tmdbid-", "tmdb-", "{tmdb-", "[tmdbid-", "tmdbid="] {
        if let Some(i) = lower.find(key) {
            let digits: String = lower[i + key.len()..].chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(n) = digits.parse() {
                return n;
            }
        }
    }
    0
}
