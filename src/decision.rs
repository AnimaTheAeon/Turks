//! Which releases to grab, ported from Radarr's DecisionEngine: the same
//! specifications, upgrade rules and sort order.

use std::cmp::Ordering;
use std::collections::HashSet;

use crate::config::{Config, Propers};
use crate::db::Library;
use crate::formats::{self, FormatInput};
use crate::model::*;
use crate::parser::{self, ParsedMovieInfo};
use crate::release::{Candidate, Rejection, Release};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Rss,
    AutoSearch,
    Interactive,
}

pub struct Ctx<'a> {
    pub lib: &'a Library,
    pub cfg: &'a Config,
    pub mode: Mode,
    /// Source titles already imported (AlreadyImportedSpecification).
    pub imported_titles: HashSet<String>,
    /// Lower-cased titles grabbed recently (HistorySpecification).
    pub grabbed_titles: HashSet<String>,
}

// ---------------------------------------------------------------- matching

fn title_matches(movie: &Movie, parsed: &ParsedMovieInfo) -> bool {
    let wanted: Vec<String> = std::iter::once(movie.title.clone())
        .chain(movie.alt_titles.iter().cloned())
        .chain(std::iter::once(movie.original_title.clone()))
        .filter(|t| !t.is_empty())
        .map(|t| parser::clean_title(&t))
        .collect();
    parsed.titles.iter().map(|t| parser::clean_title(t)).any(|t| wanted.contains(&t))
}

fn year_ok(movie: &Movie, parsed: &ParsedMovieInfo) -> bool {
    parsed.year == 0 || movie.year == 0 || (parsed.year - movie.year).abs() <= 1
}

/// Finds the library movie a release is for (Radarr's ParsingService.Map).
pub fn map_movie(lib: &Library, parsed: &ParsedMovieInfo, release: &Release, searched: Option<Id>) -> Option<Id> {
    let tmdb = if release.tmdb_id > 0 { release.tmdb_id } else { parsed.tmdb_id };
    let imdb = if !release.imdb_id.is_empty() { release.imdb_id.clone() } else { parsed.imdb_id.clone() };
    if let Some(id) = searched {
        let m = lib.movies.get(&id)?;
        let id_match = (tmdb > 0 && tmdb == m.tmdb_id) || (!imdb.is_empty() && imdb == m.imdb_id);
        if id_match || (title_matches(m, parsed) && year_ok(m, parsed)) {
            return Some(id);
        }
        return None;
    }
    if tmdb > 0 {
        if let Some(m) = lib.movie_by_tmdb(tmdb) {
            return Some(m.id);
        }
    }
    if !imdb.is_empty() {
        if let Some(m) = lib.movies.values().find(|m| m.imdb_id == imdb) {
            return Some(m.id);
        }
    }
    let mut best: Option<&Movie> = None;
    for m in lib.movies.values() {
        if title_matches(m, parsed) && year_ok(m, parsed) {
            // Prefer an exact year.
            match best {
                Some(b) if (b.year - parsed.year).abs() <= (m.year - parsed.year).abs() => {}
                _ => best = Some(m),
            }
        }
    }
    best.map(|m| m.id)
}

pub fn input_for_release(parsed: &ParsedMovieInfo, release: &Release, movie: Option<&Movie>) -> FormatInput {
    FormatInput {
        title: release.title.clone(),
        quality: parsed.quality,
        languages: parsed.languages.clone(),
        release_group: parsed.release_group.clone(),
        edition: parsed.edition.clone(),
        size: release.size,
        indexer_flags: release.indexer_flags.clone(),
        year: movie.map(|m| m.year).unwrap_or(parsed.year),
        original_language: movie.map(|m| m.original_language.clone()).unwrap_or_else(|| "English".into()),
    }
}

pub fn input_for_file(file: &MovieFile, movie: &Movie) -> FormatInput {
    let title = if !file.scene_name.is_empty() {
        file.scene_name.clone()
    } else {
        std::path::Path::new(&file.relative_path).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
    };
    FormatInput {
        title,
        quality: file.quality,
        languages: file.languages.clone(),
        release_group: file.release_group.clone(),
        edition: file.edition.clone(),
        size: file.size,
        indexer_flags: file.indexer_flags.clone(),
        year: movie.year,
        original_language: movie.original_language.clone(),
    }
}

static FORMATS_REV: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Call when a custom format is added, changed or removed, so remembered
/// matches are worked out again.
pub fn formats_changed() {
    FORMATS_REV.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
}

type FormatMemo = std::collections::HashMap<Id, (u64, u64, Vec<(Id, String)>)>;

/// The custom formats a file matches. The answer is remembered per file and
/// only worked out again when the file, its movie or the formats change, so
/// lists of thousands of movies don't re-run every pattern on every refresh.
pub fn file_formats(lib: &Library, file: &MovieFile, movie: &Movie) -> Vec<(Id, String)> {
    use std::hash::{Hash, Hasher};
    static MEMO: std::sync::LazyLock<std::sync::Mutex<FormatMemo>> = std::sync::LazyLock::new(Default::default);
    let input = input_for_file(file, movie);
    if file.id == 0 {
        // Not a saved file (an import being weighed up).
        return formats::matching(lib.formats.values(), &input);
    }
    let rev = FORMATS_REV.load(std::sync::atomic::Ordering::SeqCst);
    let mut h = std::collections::hash_map::DefaultHasher::new();
    format!("{input:?}").hash(&mut h);
    let print = h.finish();
    if let Some((r, p, hit)) = MEMO.lock().unwrap_or_else(|e| e.into_inner()).get(&file.id) {
        if *r == rev && *p == print {
            return hit.clone();
        }
    }
    let out = formats::matching(lib.formats.values(), &input);
    let mut memo = MEMO.lock().unwrap_or_else(|e| e.into_inner());
    if memo.len() > 50_000 {
        memo.clear();
    }
    memo.insert(file.id, (rev, print, out.clone()));
    out
}

/// Builds a scored candidate for a release (no rejections yet).
pub fn candidate(lib: &Library, release: Release, searched: Option<Id>) -> Candidate {
    let parsed = parser::parse_movie_title(&release.title, false).unwrap_or_else(|| ParsedMovieInfo {
        quality: parser::quality::parse_quality(&release.title),
        languages: parser::language::parse_languages(&release.title),
        release_group: parser::parse_release_group(&release.title),
        ..Default::default()
    });
    let movie_id = map_movie(lib, &parsed, &release, searched);
    let movie = movie_id.and_then(|id| lib.movies.get(&id));
    let matched = formats::matching(lib.formats.values(), &input_for_release(&parsed, &release, movie));
    let ids: Vec<Id> = matched.iter().map(|m| m.0).collect();
    let score = movie.and_then(|m| lib.profile_for(m)).map(|p| p.score_of(&ids)).unwrap_or(0);
    Candidate {
        release,
        parsed,
        movie_id,
        custom_formats: ids,
        format_names: matched.into_iter().map(|m| m.1).collect(),
        score,
        rejections: vec![],
        temporarily_rejected: false,
    }
}

// ---------------------------------------------------------------- upgrades

fn revision_cmp(a: &Revision, b: &Revision) -> Ordering {
    a.cmp_key().cmp(&b.cmp_key())
}

pub fn quality_cutoff_not_met(p: &QualityProfile, current: &QualityModel, new: Option<&QualityModel>) -> bool {
    let cutoff_rank = if p.upgrade_allowed {
        p.cutoff_rank()
    } else {
        p.items.iter().position(|i| i.allowed).unwrap_or(0)
    };
    let cur = p.rank(current.quality).unwrap_or(0);
    if cur < cutoff_rank {
        return true;
    }
    if let Some(n) = new {
        if n.quality == current.quality && revision_cmp(&n.revision, &current.revision) == Ordering::Greater {
            return true;
        }
    }
    false
}

pub fn cutoff_not_met(p: &QualityProfile, current: &QualityModel, current_formats: &[Id], new: Option<&QualityModel>) -> bool {
    quality_cutoff_not_met(p, current, new) || p.score_of(current_formats) < p.cutoff_format_score
}

/// Radarr's IsUpgradable. Ok(()) means the new release is an upgrade.
pub fn is_upgradable(cfg: &Config, p: &QualityProfile, cur: &QualityModel, cur_formats: &[Id], new: &QualityModel, new_formats: &[Id]) -> Result<(), String> {
    let rank_new = p.rank(new.quality).unwrap_or(0);
    let rank_cur = p.rank(cur.quality).unwrap_or(0);
    if rank_new > rank_cur && quality_cutoff_not_met(p, cur, Some(new)) {
        return Ok(());
    }
    if rank_new < rank_cur {
        return Err(format!("Existing file has better quality ({})", crate::quality::name(cur.quality)));
    }
    let rev = revision_cmp(&new.revision, &cur.revision);
    if cfg.propers != Propers::DoNotPrefer && rev == Ordering::Greater {
        return Ok(());
    }
    if !p.upgrade_allowed {
        return Err("Upgrades are turned off for this quality profile".into());
    }
    if cfg.propers != Propers::DoNotPrefer && rev == Ordering::Less {
        return Err("Existing file is a newer revision (Proper/Repack)".into());
    }
    if rank_new > rank_cur {
        return Err("Existing file already meets the quality cutoff".into());
    }
    let cur_score = p.score_of(cur_formats);
    let new_score = p.score_of(new_formats);
    if new_score <= cur_score {
        return Err(format!("Custom format score {new_score} isn't better than the existing file's {cur_score}"));
    }
    if cur_score >= p.cutoff_format_score {
        return Err(format!("Existing file's custom format score {cur_score} already meets the cutoff {}", p.cutoff_format_score));
    }
    if new_score < cur_score + p.min_upgrade_format_score {
        return Err(format!("Score {new_score} doesn't beat {cur_score} by the minimum upgrade of {}", p.min_upgrade_format_score));
    }
    Ok(())
}

// ---------------------------------------------------------------- delay profiles

pub fn delay_profile_for(lib: &Library, movie: &Movie) -> DelayProfile {
    let mut profiles: Vec<&DelayProfile> = lib.delay_profiles.values().collect();
    profiles.sort_by_key(|d| d.order);
    profiles
        .iter()
        .find(|d| !d.tags.is_empty() && d.tags.iter().any(|t| movie.tags.contains(t)))
        .or_else(|| profiles.iter().find(|d| d.tags.is_empty()))
        .map(|d| (*d).clone())
        .unwrap_or_default()
}

fn tags_overlap(provider_tags: &[Id], movie_tags: &[Id]) -> bool {
    provider_tags.is_empty() || provider_tags.iter().any(|t| movie_tags.contains(t))
}

// ---------------------------------------------------------------- specifications

pub fn evaluate(ctx: &Ctx, c: &mut Candidate) {
    let lib = ctx.lib;
    let cfg = ctx.cfg;
    let mut rej: Vec<Rejection> = Vec::new();
    let r = &c.release;
    let parsed = &c.parsed;

    if parsed.titles.is_empty() && parsed.year == 0 {
        rej.push(Rejection::perm("Couldn't parse the release name"));
    }
    let Some(movie) = c.movie_id.and_then(|id| lib.movies.get(&id)) else {
        if rej.is_empty() {
            rej.push(Rejection::perm("Unknown movie: doesn't match anything in your library"));
        }
        c.rejections = rej;
        return;
    };
    let Some(profile) = lib.profile_for(movie) else {
        c.rejections = vec![Rejection::perm("The movie has no quality profile")];
        return;
    };

    // Monitored / availability (automatic only)
    if ctx.mode != Mode::Interactive {
        if !movie.monitored {
            rej.push(Rejection::perm("Movie isn't monitored"));
        }
        if !movie.is_available(cfg.availability_delay_days) {
            rej.push(Rejection::perm(format!("Movie isn't available yet (minimum availability: {})", movie.minimum_availability.label())));
        }
    }

    // Quality allowed by profile
    if !profile.allows(parsed.quality.quality) {
        rej.push(Rejection::perm(format!("{} isn't wanted in profile '{}'", crate::quality::name(parsed.quality.quality), profile.name)));
    }

    // Custom format minimum score
    if c.score < profile.min_format_score {
        rej.push(Rejection::perm(format!("Custom format score {} is below the minimum {}", c.score, profile.min_format_score)));
    }

    // Language
    let lang_wanted = profile.language.as_str();
    if lang_wanted != "Any" && !lang_wanted.is_empty() {
        let wanted = if lang_wanted == "Original" { movie.original_language.as_str() } else { lang_wanted };
        let langs: Vec<String> = if parsed.languages.is_empty() {
            vec![movie.original_language.clone()]
        } else {
            parsed.languages.iter().map(|l| if l == "Original" { movie.original_language.clone() } else { l.clone() }).collect()
        };
        let multi = parser::language::has_multiple_languages(&r.title);
        if !multi && !langs.iter().any(|l| l.eq_ignore_ascii_case(wanted)) {
            rej.push(Rejection::perm(format!("Language {} isn't wanted (profile wants {wanted})", langs.join(", "))));
        }
    }

    // Hardcoded subs
    if !parsed.hardcoded_subs.is_empty() && !cfg.allow_hardcoded_subs {
        let whitelisted = cfg
            .whitelisted_hardcoded_subs
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .any(|w| parsed.hardcoded_subs.to_lowercase().contains(&w.to_lowercase()));
        if !whitelisted {
            rej.push(Rejection::perm(format!("Hardcoded subs found: {}", parsed.hardcoded_subs)));
        }
    }

    // Size (quality definitions × runtime)
    if r.size > 0 {
        let def = lib.size_for(parsed.quality.quality);
        let runtime = if movie.runtime > 0 { movie.runtime } else { 110 } as f64;
        let mb = r.size as f64 / 1_048_576.0;
        if def.min > 0.0 && mb < def.min * runtime {
            rej.push(Rejection::perm(format!("{} is smaller than the minimum {}", human_size(r.size), human_size((def.min * runtime * 1_048_576.0) as u64))));
        }
        if let Some(max) = def.max.filter(|m| *m > 0.0) {
            if mb > max * runtime {
                rej.push(Rejection::perm(format!("{} is larger than the maximum {}", human_size(r.size), human_size((max * runtime * 1_048_576.0) as u64))));
            }
        }
        if cfg.maximum_size_mb > 0 && mb > cfg.maximum_size_mb as f64 {
            rej.push(Rejection::perm(format!("{} is over your maximum size of {} MB", human_size(r.size), cfg.maximum_size_mb)));
        }
    }

    // Not a sample
    if r.title.to_lowercase().split(|ch: char| !ch.is_alphanumeric()).any(|w| w == "sample") && r.size > 0 && r.size < 250 * 1_048_576 {
        rej.push(Rejection::perm("Looks like a sample"));
    }

    // Protocol and delay profile
    let delay = delay_profile_for(lib, movie);
    match r.protocol {
        Protocol::Usenet if !delay.enable_usenet => rej.push(Rejection::perm("Usenet is turned off in the delay profile")),
        Protocol::Torrent if !delay.enable_torrent => rej.push(Rejection::perm("Torrents are turned off in the delay profile")),
        _ => {}
    }

    // Usenet retention / minimum age
    if r.protocol == Protocol::Usenet {
        if cfg.retention_days > 0 && r.age_days() > cfg.retention_days as f64 {
            rej.push(Rejection::perm(format!("Older than retention ({} days)", cfg.retention_days)));
        }
        if cfg.minimum_age_minutes > 0 && r.age_minutes() < cfg.minimum_age_minutes {
            rej.push(Rejection::temp(format!("Only {} minutes old, minimum is {}", r.age_minutes(), cfg.minimum_age_minutes)));
        }
    }

    // Torrent seeders and required flags
    let indexer = lib.indexers.get(&r.indexer_id);
    if r.protocol == Protocol::Torrent {
        let min = indexer.map(|i| i.minimum_seeders).unwrap_or(1);
        if let Some(s) = r.seeders {
            if s < min {
                rej.push(Rejection::perm(format!("Not enough seeders: {s} (minimum {min})")));
            }
        }
    }
    if let Some(ix) = indexer {
        for flag in &ix.required_flags {
            if !r.indexer_flags.iter().any(|f| f.eq_ignore_ascii_case(flag)) {
                rej.push(Rejection::perm(format!("Indexer flag '{flag}' is required")));
            }
        }
        // Indexer tags must overlap the movie's
        if ctx.mode != Mode::Interactive && !tags_overlap(&ix.tags, &movie.tags) {
            rej.push(Rejection::perm("Indexer's tags don't match the movie's"));
        }
    }

    // Release profiles (restrictions)
    for rp in lib.release_profiles.values().filter(|p| p.enabled) {
        if rp.indexer_id != 0 && rp.indexer_id != r.indexer_id {
            continue;
        }
        if !tags_overlap(&rp.tags, &movie.tags) {
            continue;
        }
        let t = r.title.to_lowercase();
        let term_hit = |term: &str| -> bool {
            let term = term.trim();
            if term.starts_with('/') && term.ends_with('/') && term.len() > 2 {
                fancy_regex::Regex::new(&format!("(?i){}", &term[1..term.len() - 1])).map(|re| re.is_match(&r.title).unwrap_or(false)).unwrap_or(false)
            } else {
                t.contains(&term.to_lowercase())
            }
        };
        let required: Vec<&String> = rp.required.iter().filter(|s| !s.trim().is_empty()).collect();
        if !required.is_empty() && !required.iter().any(|w| term_hit(w)) {
            rej.push(Rejection::perm(format!("Doesn't contain any of: {}", rp.required.join(", "))));
        }
        if let Some(w) = rp.ignored.iter().filter(|s| !s.trim().is_empty()).find(|w| term_hit(w)) {
            rej.push(Rejection::perm(format!("Contains '{w}', which is ignored")));
        }
    }

    // Blocklist
    let blocked = lib.blocklist.values().any(|b| {
        b.movie_id == movie.id
            && ((!r.info_hash.is_empty() && b.info_hash.eq_ignore_ascii_case(&r.info_hash))
                || (b.source_title == r.title && b.indexer == r.indexer && (b.size == 0 || r.size == 0 || b.size == r.size)))
    });
    if blocked {
        rej.push(Rejection::perm("Release is blocklisted"));
    }

    // Already imported / grabbed (RSS & automatic)
    if ctx.mode != Mode::Interactive
        && ctx.imported_titles.contains(&r.title) {
            rej.push(Rejection::perm("This release was already imported"));
        }

    // Existing file: upgrade rules
    if let Some(file) = lib.file_for(movie) {
        let cur_formats: Vec<Id> = file_formats(lib, file, movie).into_iter().map(|f| f.0).collect();
        if !cutoff_not_met(profile, &file.quality, &cur_formats, Some(&parsed.quality)) {
            rej.push(Rejection::perm("Existing file meets the cutoff"));
        } else if let Err(why) = is_upgradable(cfg, profile, &file.quality, &cur_formats, &parsed.quality, &c.custom_formats) {
            rej.push(Rejection::perm(why));
        }
        // Propers: don't auto-upgrade when told not to
        if cfg.propers == Propers::DoNotUpgrade
            && ctx.mode != Mode::Interactive
            && parsed.quality.quality == file.quality.quality
            && revision_cmp(&parsed.quality.revision, &file.quality.revision) == Ordering::Greater
        {
            rej.push(Rejection::perm("Proper/Repack upgrades are turned off"));
        }
        // Repack only from the same group
        if parsed.quality.revision.is_repack
            && parsed.quality.quality == file.quality.quality
            && !file.release_group.is_empty()
            && !parsed.release_group.is_empty()
            && !file.release_group.eq_ignore_ascii_case(&parsed.release_group)
        {
            rej.push(Rejection::perm(format!("Repack is from {}, the existing file is from {}", parsed.release_group, file.release_group)));
        }
    }

    // Queue: something equal or better already downloading
    for d in lib.active_downloads().filter(|d| d.movie_id == movie.id) {
        if d.state == DownloadState::Failed {
            continue;
        }
        let better = match is_upgradable(cfg, profile, &d.quality, &[], &parsed.quality, &c.custom_formats) {
            Ok(()) => !d.title.eq_ignore_ascii_case(&r.title),
            Err(_) => false,
        };
        if !better || !cutoff_not_met(profile, &d.quality, &[], Some(&parsed.quality)) {
            rej.push(Rejection::perm(format!("Already in the download queue: {}", d.title)));
            break;
        }
    }

    // Recently grabbed (RSS)
    if ctx.mode == Mode::Rss && ctx.grabbed_titles.contains(&r.title.to_lowercase()) {
        rej.push(Rejection::perm("Already grabbed recently"));
    }

    // Delay profile (temporary)
    if ctx.mode != Mode::Interactive && rej.is_empty() {
        let delay_minutes = match r.protocol {
            Protocol::Usenet => delay.usenet_delay,
            Protocol::Torrent => delay.torrent_delay,
        };
        if delay_minutes > 0 {
            let highest = profile.items.iter().rposition(|i| i.allowed).map(|top| profile.rank(parsed.quality.quality) == Some(top)).unwrap_or(false);
            let bypass = (delay.bypass_if_highest_quality && highest) || (delay.bypass_if_above_score && c.score >= delay.minimum_score);
            if !bypass && r.age_minutes() < delay_minutes {
                c.temporarily_rejected = true;
                rej.push(Rejection::temp(format!("Delay profile: waiting {} more minutes", delay_minutes - r.age_minutes())));
            }
        }
    }

    c.rejections = rej;
}

// ---------------------------------------------------------------- order

fn flag_score(flags: &[String]) -> i32 {
    flags
        .iter()
        .map(|f| match f.as_str() {
            "freeleech" => 4,
            "freeleech75" => 3,
            "halfleech" => 2,
            "freeleech25" => 1,
            "doubleupload" => 1,
            "internal" | "ahd_internal" | "ptp_golden" | "ptp_approved" => 1,
            "nuked" => -10,
            _ => 0,
        })
        .sum()
}

fn log_bucket(v: Option<i32>) -> i64 {
    match v {
        Some(n) if n > 0 => (n as f64).log10().round() as i64,
        _ => 0,
    }
}

/// Best first, like Radarr's DownloadDecisionComparer.
pub fn compare(lib: &Library, cfg: &Config, a: &Candidate, b: &Candidate) -> Ordering {
    let movie_a = a.movie_id.and_then(|id| lib.movies.get(&id));
    let profile = movie_a.and_then(|m| lib.profile_for(m));
    let rank = |c: &Candidate| profile.and_then(|p| p.rank(c.parsed.quality.quality)).map(|r| r as i64).unwrap_or(-1);
    let mut ord = rank(b).cmp(&rank(a));
    if ord == Ordering::Equal && cfg.propers != Propers::DoNotPrefer {
        ord = revision_cmp(&b.parsed.quality.revision, &a.parsed.quality.revision);
    }
    if ord == Ordering::Equal {
        ord = b.score.cmp(&a.score);
    }
    if ord == Ordering::Equal {
        let pref = movie_a.map(|m| delay_profile_for(lib, m).preferred).unwrap_or(Protocol::Usenet);
        ord = (b.release.protocol == pref).cmp(&(a.release.protocol == pref));
    }
    if ord == Ordering::Equal {
        ord = a.release.indexer_priority.cmp(&b.release.indexer_priority);
    }
    if ord == Ordering::Equal && cfg.prefer_indexer_flags {
        ord = flag_score(&b.release.indexer_flags).cmp(&flag_score(&a.release.indexer_flags));
    }
    if ord == Ordering::Equal && a.release.protocol == Protocol::Torrent && b.release.protocol == Protocol::Torrent {
        ord = log_bucket(b.release.seeders).cmp(&log_bucket(a.release.seeders)).then(log_bucket(b.release.peers).cmp(&log_bucket(a.release.peers)));
    }
    if ord == Ordering::Equal && a.release.protocol == Protocol::Usenet && b.release.protocol == Protocol::Usenet {
        let age_score = |c: &Candidate| {
            let h = c.release.age_minutes() as f64 / 60.0;
            if h < 1.0 {
                1000
            } else if h <= 24.0 {
                100
            } else if h <= 168.0 {
                10
            } else {
                -((h / 24.0).log10().round() as i64)
            }
        };
        ord = age_score(b).cmp(&age_score(a));
    }
    if ord == Ordering::Equal {
        let size_score = |c: &Candidate| -> i64 {
            let def = lib.size_for(c.parsed.quality.quality);
            let runtime = movie_a.map(|m| m.runtime).unwrap_or(0);
            let round = 200 * 1_048_576_i64;
            match def.preferred {
                Some(p) if runtime > 0 => {
                    let preferred = (runtime as f64 * p * 1_048_576.0) as i64;
                    -((c.release.size as i64 - preferred).abs() / round)
                }
                _ => c.release.size as i64 / round,
            }
        };
        ord = size_score(b).cmp(&size_score(a));
    }
    ord
}

pub fn human_size(bytes: u64) -> String {
    let b = bytes as f64;
    if b >= 1_099_511_627_776.0 {
        format!("{:.2} TB", b / 1_099_511_627_776.0)
    } else if b >= 1_073_741_824.0 {
        format!("{:.1} GB", b / 1_073_741_824.0)
    } else if b >= 1_048_576.0 {
        format!("{:.0} MB", b / 1_048_576.0)
    } else if b >= 1024.0 {
        format!("{:.0} KB", b / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quality::id;

    fn lib_with_movie() -> Library {
        let mut lib = Library::default();
        let mut p = crate::quality::default_profiles().remove(3); // HD-1080p
        p.id = 1;
        lib.profiles.insert(1, p);
        lib.movies.insert(
            1,
            Movie {
                id: 1,
                tmdb_id: 603,
                title: "The Matrix".into(),
                clean_title: parser::clean_title("The Matrix"),
                year: 1999,
                runtime: 136,
                original_language: "English".into(),
                monitored: true,
                status: MovieStatus::Released,
                quality_profile_id: 1,
                ..Default::default()
            },
        );
        for s in crate::quality::default_sizes() {
            lib.sizes.insert(s.quality as Id, s);
        }
        lib
    }

    fn rel(title: &str, gb: f64) -> Release {
        Release { title: title.into(), size: (gb * 1_073_741_824.0) as u64, protocol: Protocol::Torrent, seeders: Some(50), indexer: "Test".into(), ..Default::default() }
    }

    #[test]
    fn picks_best_allowed_release() {
        let lib = lib_with_movie();
        let cfg = Config::default();
        let ctx = Ctx { lib: &lib, cfg: &cfg, mode: Mode::AutoSearch, imported_titles: HashSet::new(), grabbed_titles: HashSet::new() };
        let mut cands: Vec<Candidate> = [
            rel("The.Matrix.1999.720p.BluRay.x264-GRP", 6.0),
            rel("The.Matrix.1999.1080p.BluRay.x264-GRP", 12.0),
            rel("The.Matrix.1999.2160p.UHD.BluRay.x265-GRP", 40.0),
            rel("The.Matrix.1999.1080p.WEB-DL.DD5.1.H.264-GRP", 8.0),
            rel("Some.Other.Movie.1999.1080p.BluRay.x264-GRP", 10.0),
        ]
        .into_iter()
        .map(|r| candidate(&lib, r, None))
        .collect();
        for c in cands.iter_mut() {
            evaluate(&ctx, c);
        }
        let mut ok: Vec<&Candidate> = cands.iter().filter(|c| c.approved()).collect();
        ok.sort_by(|a, b| compare(&lib, &cfg, a, b));
        assert_eq!(ok.len(), 2, "{:#?}", cands.iter().map(|c| (&c.release.title, &c.rejections)).collect::<Vec<_>>());
        assert_eq!(ok[0].parsed.quality.quality, id::BLURAY1080P);
        assert!(cands[4].movie_id.is_none());
    }

    #[test]
    fn upgrade_rules() {
        let cfg = Config::default();
        let p = crate::quality::default_profiles().remove(3);
        let cur = QualityModel { quality: id::WEBDL1080P, revision: Revision::new() };
        let better = QualityModel { quality: id::BLURAY1080P, revision: Revision::new() };
        assert!(is_upgradable(&cfg, &p, &cur, &[], &better, &[]).is_ok());
        assert!(is_upgradable(&cfg, &p, &better, &[], &cur, &[]).is_err());
        let proper = QualityModel { quality: id::WEBDL1080P, revision: Revision { version: 2, real: 0, is_repack: false } };
        assert!(is_upgradable(&cfg, &p, &cur, &[], &proper, &[]).is_ok());
        // HD-1080p cutoff is Bluray-1080p, so Remux isn't an upgrade from Bluray.
        let remux = QualityModel { quality: id::REMUX1080P, revision: Revision::new() };
        assert!(!cutoff_not_met(&p, &better, &[], Some(&remux)));
    }
}

#[cfg(test)]
mod memo_tests {
    use super::*;

    fn format(id: Id, name: &str, pattern: &str) -> CustomFormat {
        CustomFormat {
            id,
            name: name.into(),
            include_when_renaming: false,
            specs: vec![CfSpec { name: name.into(), kind: CfKind::ReleaseTitle { pattern: pattern.into() }, negate: false, required: false }],
        }
    }

    #[test]
    fn remembered_matches_follow_changes() {
        let mut lib = Library::default();
        lib.formats.insert(1, format(1, "x265", r"\b(x265|hevc)\b"));
        let movie = Movie::default();
        let mut file = MovieFile { id: 900_001, scene_name: "Movie.2010.1080p.BluRay.x265-GRP".into(), ..Default::default() };
        let names = |lib: &Library, f: &MovieFile| file_formats(lib, f, &movie).into_iter().map(|x| x.1).collect::<Vec<_>>();
        assert_eq!(names(&lib, &file), ["x265"]);
        assert_eq!(names(&lib, &file), ["x265"]); // remembered

        // A changed format is noticed…
        lib.formats.insert(2, format(2, "BluRay", r"\bbluray\b"));
        formats_changed();
        assert_eq!(names(&lib, &file), ["x265", "BluRay"]);

        // …and so is a changed file.
        file.scene_name = "Movie.2010.1080p.WEB-DL.x264-GRP".into();
        assert!(names(&lib, &file).is_empty());
    }

    /// 2,000 files against 60 formats, the way the movie list asks.
    #[test]
    fn big_library_is_quick() {
        let mut lib = Library::default();
        for i in 1..=60 {
            lib.formats.insert(i, format(i, &format!("f{i}"), &format!(r"\b(group{i}|tag{i}|(?<!not)word{i})\b")));
        }
        let movie = Movie::default();
        let files: Vec<MovieFile> = (0..2000).map(|i| MovieFile { id: 1_000_000 + i, scene_name: format!("Movie.{i}.2010.1080p.BluRay.x264-group{}", i % 70), ..Default::default() }).collect();
        let pass = |lib: &Library| {
            let t = std::time::Instant::now();
            let hits: usize = files.iter().map(|f| file_formats(lib, f, &movie).len()).sum();
            (t.elapsed(), hits)
        };
        let (first, hits) = pass(&lib);
        let (second, hits2) = pass(&lib);
        println!("first pass {first:?}, repeat pass {second:?}");
        assert_eq!(hits, hits2);
        assert!(second < std::time::Duration::from_millis(250), "repeat pass took {second:?}");
    }
}
