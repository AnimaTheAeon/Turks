//! Kodi/Emby/Jellyfin metadata: movie.nfo plus poster and fanart images.

use std::path::Path;

use crate::config::Config;
use crate::model::{Movie, MovieFile};

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

pub fn nfo(movie: &Movie, file: Option<&MovieFile>, cfg: &Config) -> String {
    let mut x = String::from("<?xml version=\"1.0\" encoding=\"utf-8\" standalone=\"yes\"?>\n<movie>\n");
    let mut tag = |k: &str, v: &str| {
        if !v.is_empty() {
            x.push_str(&format!("  <{k}>{}</{k}>\n", esc(v)));
        }
    };
    tag("title", &movie.title);
    tag("originaltitle", &movie.original_title);
    tag("sorttitle", &movie.sort_title);
    if movie.ratings.tmdb > 0.0 {
        tag("rating", &format!("{:.1}", movie.ratings.tmdb));
    }
    tag("year", &if movie.year > 0 { movie.year.to_string() } else { String::new() });
    tag("plot", &movie.overview);
    tag("outline", &movie.overview);
    tag("runtime", &if movie.runtime > 0 { movie.runtime.to_string() } else { String::new() });
    tag("mpaa", &movie.certification);
    tag("id", &movie.imdb_id);
    tag("tmdbid", &movie.tmdb_id.to_string());
    tag("imdbid", &movie.imdb_id);
    for g in &movie.genres {
        tag("genre", g);
    }
    tag("studio", &movie.studio);
    tag("premiered", &movie.in_cinemas.map(|d| d.to_string()).unwrap_or_default());
    if !movie.youtube_trailer.is_empty() {
        tag("trailer", &format!("plugin://plugin.video.youtube/?action=play_video&videoid={}", movie.youtube_trailer));
    }
    for c in movie.crew.iter().filter(|c| c.role == "Director") {
        tag("director", &c.name);
    }
    for c in movie.crew.iter().filter(|c| c.role == "Screenplay" || c.role == "Writer") {
        tag("credits", &c.name);
    }
    if cfg.kodi_collection_name {
        if let Some(c) = &movie.collection {
            x.push_str(&format!("  <set>\n    <name>{}</name>\n  </set>\n", esc(&c.title)));
        }
    }
    x.push_str(&format!("  <uniqueid type=\"tmdb\" default=\"true\">{}</uniqueid>\n", movie.tmdb_id));
    if !movie.imdb_id.is_empty() {
        x.push_str(&format!("  <uniqueid type=\"imdb\">{}</uniqueid>\n", esc(&movie.imdb_id)));
    }
    for a in movie.cast.iter().take(15) {
        x.push_str(&format!("  <actor>\n    <name>{}</name>\n    <role>{}</role>\n  </actor>\n", esc(&a.name), esc(&a.role)));
    }
    if let Some(f) = file {
        let mi = &f.media_info;
        x.push_str("  <fileinfo>\n    <streamdetails>\n");
        if !mi.video_codec.is_empty() {
            x.push_str(&format!("      <video>\n        <codec>{}</codec>\n", esc(&mi.video_codec)));
            if !mi.video_dynamic_range.is_empty() {
                x.push_str(&format!("        <hdrtype>{}</hdrtype>\n", esc(&mi.video_dynamic_range.to_lowercase())));
            }
            if mi.runtime_secs > 0 {
                x.push_str(&format!("        <durationinseconds>{}</durationinseconds>\n", mi.runtime_secs));
            }
            x.push_str("      </video>\n");
        }
        if !mi.audio_codec.is_empty() {
            x.push_str(&format!("      <audio>\n        <codec>{}</codec>\n        <channels>{}</channels>\n      </audio>\n", esc(&mi.audio_codec), mi.audio_channels.round() as i32));
        }
        for s in &mi.subtitles {
            x.push_str(&format!("      <subtitle>\n        <language>{}</language>\n      </subtitle>\n", esc(s)));
        }
        x.push_str("    </streamdetails>\n  </fileinfo>\n");
    }
    x.push_str("</movie>\n");
    if cfg.kodi_url_in_nfo {
        x.push_str(&format!("https://www.themoviedb.org/movie/{}\n", movie.tmdb_id));
    }
    x
}

/// Writes movie.nfo, poster.jpg and fanart.jpg into the movie folder.
pub fn write(movie: &Movie, file: Option<&MovieFile>, cfg: &Config) {
    if !cfg.kodi_metadata || movie.path.is_empty() {
        return;
    }
    let dir = Path::new(&movie.path);
    if !dir.is_dir() {
        return;
    }
    let nfo_name = file
        .and_then(|f| Path::new(&f.relative_path).file_stem().map(|s| format!("{}.nfo", s.to_string_lossy())))
        .unwrap_or_else(|| "movie.nfo".into());
    if let Err(e) = std::fs::write(dir.join(&nfo_name), nfo(movie, file, cfg)) {
        crate::applog::warn(format!("Couldn't write {nfo_name} for {}: {e}", movie.display()));
    }
    if cfg.kodi_movie_images {
        for (kind, size, path) in [("poster", "original", &movie.poster_path), ("fanart", "original", &movie.fanart_path)] {
            let cached = crate::tmdb::image_path(kind, movie.tmdb_id);
            let dest = dir.join(format!("{kind}.jpg"));
            if dest.exists() || path.is_empty() {
                continue;
            }
            if cached.exists() && kind == "fanart" {
                let _ = std::fs::copy(&cached, &dest);
            } else {
                crate::tmdb::fetch_image(path, size, &dest);
            }
        }
    }
}
