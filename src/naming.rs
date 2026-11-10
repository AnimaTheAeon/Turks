//! Folder and file names from Radarr-style templates, e.g.
//! "{Movie Title} ({Release Year}) {Quality Full}".
//! Token case sets output case ({MOVIE TITLE} is upper case) and the
//! separator inside a token sets word spacing ({Movie.Title}).

use once_cell::sync::Lazy;
use regex::Regex;

use crate::config::{ColonReplacement, Config};
use crate::model::{Movie, MovieFile};

static TOKEN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\{(?P<prefix>[- ._\[(]*)(?P<token>[A-Za-z0-9]+(?:[- ._][A-Za-z0-9]+)*)(?::(?P<fmt>[^}]+))?(?P<suffix>[- ._)\]]*)\}").unwrap()
});
static MULTI_SPACE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\s{2,}").unwrap());
static EMPTY_BRACKETS: Lazy<Regex> = Lazy::new(|| Regex::new(r"\(\s*\)|\[\s*\]|\{\s*\}").unwrap());

pub struct NameContext<'a> {
    pub movie: &'a Movie,
    pub file: Option<&'a MovieFile>,
    pub custom_formats: Vec<String>,
    pub collection: String,
}

fn title_the(t: &str) -> String {
    for article in ["The ", "A ", "An "] {
        if let Some(rest) = t.strip_prefix(article) {
            return format!("{rest}, {}", article.trim());
        }
    }
    t.to_string()
}

fn clean(t: &str) -> String {
    let s: String = t.chars().filter(|c| c.is_alphanumeric() || c.is_whitespace() || *c == '\'' || *c == '-').collect();
    MULTI_SPACE.replace_all(s.replace('\'', "").trim(), " ").to_string()
}

fn channels(ch: f32) -> String {
    if ch <= 0.0 {
        String::new()
    } else {
        format!("{ch:.1}")
    }
}

fn value(ctx: &NameContext, key: &str, fmt: Option<&str>) -> Option<String> {
    let m = ctx.movie;
    let f = ctx.file;
    let mi = f.map(|f| f.media_info.clone()).unwrap_or_default();
    Some(match key {
        "movie title" => m.title.clone(),
        "movie cleantitle" => clean(&m.title),
        "movie titlethe" => title_the(&m.title),
        "movie cleantitlethe" => clean(&title_the(&m.title)),
        "movie titlefirstcharacter" => {
            let t = title_the(&m.title);
            t.chars().find(|c| c.is_alphanumeric()).map(|c| c.to_uppercase().to_string()).unwrap_or_default()
        }
        "movie originaltitle" => if m.original_title.is_empty() { m.title.clone() } else { m.original_title.clone() },
        "movie cleanoriginaltitle" => clean(if m.original_title.is_empty() { &m.title } else { &m.original_title }),
        "movie titleyear" => m.display(),
        "movie collection" | "movie collectionthe" => {
            let c = if ctx.collection.is_empty() { m.collection.as_ref().map(|c| c.title.clone()).unwrap_or_default() } else { ctx.collection.clone() };
            if key.ends_with("the") { title_the(&c) } else { c }
        }
        "movie certification" => m.certification.clone(),
        "release year" => if m.year > 0 { m.year.to_string() } else { String::new() },
        "imdbid" => m.imdb_id.clone(),
        "tmdbid" => m.tmdb_id.to_string(),
        "edition tags" => f.map(|f| f.edition.clone()).unwrap_or_default(),
        "quality full" => f.map(|f| crate::quality::full_name(&f.quality)).unwrap_or_default(),
        "quality title" => f.map(|f| crate::quality::name(f.quality.quality).to_string()).unwrap_or_default(),
        "quality proper" => f.filter(|f| f.quality.revision.version > 1).map(|f| if f.quality.revision.is_repack { "Repack" } else { "Proper" }.to_string()).unwrap_or_default(),
        "quality real" => f.filter(|f| f.quality.revision.real > 0).map(|_| "REAL".to_string()).unwrap_or_default(),
        "mediainfo simple" => [mi.video_codec.as_str(), mi.audio_codec.as_str()].iter().filter(|s| !s.is_empty()).cloned().collect::<Vec<_>>().join(" "),
        "mediainfo full" => {
            let mut s = [mi.video_codec.as_str(), mi.audio_codec.as_str()].iter().filter(|s| !s.is_empty()).cloned().collect::<Vec<_>>().join(" ");
            if !mi.audio_languages.is_empty() {
                s.push_str(&format!(" [{}]", mi.audio_languages.join("+")));
            }
            if !mi.subtitles.is_empty() {
                s.push_str(&format!(" [{}]", mi.subtitles.join("+")));
            }
            s
        }
        "mediainfo videocodec" => mi.video_codec.clone(),
        "mediainfo videobitdepth" => if mi.video_bit_depth > 0 { mi.video_bit_depth.to_string() } else { String::new() },
        "mediainfo videodynamicrange" => if mi.video_dynamic_range.is_empty() { String::new() } else { "HDR".into() },
        "mediainfo videodynamicrangetype" => mi.video_dynamic_range.clone(),
        "mediainfo audiocodec" => mi.audio_codec.clone(),
        "mediainfo audiochannels" => channels(mi.audio_channels),
        "mediainfo audiolanguages" | "mediainfo audiolanguagesall" => if mi.audio_languages.is_empty() { String::new() } else { format!("[{}]", mi.audio_languages.join("+")) },
        "mediainfo subtitlelanguages" | "mediainfo subtitlelanguagesall" => if mi.subtitles.is_empty() { String::new() } else { format!("[{}]", mi.subtitles.join("+")) },
        "mediainfo 3d" => if f.map(|f| f.scene_name.to_lowercase().contains("3d")).unwrap_or(false) { "3D".into() } else { String::new() },
        "release group" => f.map(|f| f.release_group.clone()).unwrap_or_default(),
        "original title" => f.map(|f| f.scene_name.clone()).unwrap_or_default(),
        "original filename" => f
            .map(|f| std::path::Path::new(&f.original_path).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default())
            .unwrap_or_default(),
        "custom formats" => ctx.custom_formats.join(" "),
        "custom format" => {
            let want = fmt.unwrap_or("");
            ctx.custom_formats.iter().find(|c| c.eq_ignore_ascii_case(want)).cloned().unwrap_or_default()
        }
        _ => return None,
    })
}

fn apply_case(token: &str, value: &str) -> String {
    let letters: String = token.chars().filter(|c| c.is_alphabetic()).collect();
    if letters.len() > 1 && letters.chars().all(|c| c.is_uppercase()) {
        value.to_uppercase()
    } else if letters.chars().all(|c| c.is_lowercase()) {
        value.to_lowercase()
    } else {
        value.to_string()
    }
}

pub fn render(template: &str, ctx: &NameContext, cfg: &Config) -> String {
    // Plex/Jellyfin style nested ids.
    let t = template
        .replace("{imdb-{ImdbId}}", &if ctx.movie.imdb_id.is_empty() { String::new() } else { format!("{{imdb-{}}}", ctx.movie.imdb_id) })
        .replace("{tmdb-{TmdbId}}", &format!("{{tmdb-{}}}", ctx.movie.tmdb_id))
        .replace("{tmdbid-{TmdbId}}", &format!("{{tmdbid-{}}}", ctx.movie.tmdb_id))
        .replace(
            "{edition-{Edition Tags}}",
            &ctx.file.filter(|f| !f.edition.is_empty()).map(|f| format!("{{edition-{}}}", f.edition)).unwrap_or_default(),
        );
    // Protect literal {xxx-yyy} ids from the token pass.
    let mut literals: Vec<String> = Vec::new();
    let protected = Regex::new(r"\{(imdb|tmdb|tmdbid|edition)-[^{}]+\}").unwrap().replace_all(&t, |c: &regex::Captures| {
        literals.push(c[0].to_string());
        format!("\u{0}{}\u{0}", literals.len() - 1)
    });
    let out = TOKEN.replace_all(&protected, |c: &regex::Captures| {
        let raw = &c["token"];
        let sep = raw.chars().find(|ch| matches!(ch, ' ' | '.' | '_' | '-')).unwrap_or(' ');
        let key = raw.replace(['.', '_', '-'], " ").to_lowercase();
        let fmt = c.name("fmt").map(|m| m.as_str());
        match value(ctx, &key, fmt) {
            Some(v) if !v.trim().is_empty() => {
                let mut v = apply_case(raw, &v);
                if sep != ' ' {
                    v = v.split_whitespace().collect::<Vec<_>>().join(&sep.to_string());
                }
                format!("{}{}{}", &c["prefix"], v, &c["suffix"])
            }
            Some(_) => String::new(),
            None => c[0].to_string(),
        }
    });
    let mut s = out.to_string();
    for (i, lit) in literals.iter().enumerate() {
        s = s.replace(&format!("\u{0}{i}\u{0}"), lit);
    }
    finish(&s, cfg)
}

fn finish(s: &str, cfg: &Config) -> String {
    let mut s = replace_colons(s, cfg.colon_replacement);
    if cfg.replace_illegal_chars {
        s = s.replace(['\\', '/'], "+").replace('*', "-").replace('?', "!").replace('"', "'").replace(['<', '>', '|'], "");
    } else {
        s = s.chars().filter(|c| !r#"\/*?"<>|"#.contains(*c)).collect();
    }
    let s = EMPTY_BRACKETS.replace_all(&s, "");
    let s = MULTI_SPACE.replace_all(&s, " ");
    s.trim().trim_end_matches(['.', ' ']).trim_start_matches(['.', ' ']).to_string()
}

fn replace_colons(s: &str, mode: ColonReplacement) -> String {
    match mode {
        ColonReplacement::Delete => s.replace(':', ""),
        ColonReplacement::Dash => s.replace(':', "-"),
        ColonReplacement::SpaceDash => s.replace(':', " -"),
        ColonReplacement::SpaceDashSpace => s.replace(':', " - "),
        // "Title: Subtitle" -> "Title - Subtitle", "12:30" -> "12-30"
        ColonReplacement::Smart => {
            let chars: Vec<char> = s.chars().collect();
            let mut out = String::new();
            for (i, c) in chars.iter().enumerate() {
                if *c == ':' {
                    let next_space = chars.get(i + 1).map(|n| n.is_whitespace()).unwrap_or(true);
                    out.push_str(if next_space { " -" } else { "-" });
                } else {
                    out.push(*c);
                }
            }
            out
        }
    }
}

pub fn folder_name(movie: &Movie, cfg: &Config) -> String {
    let ctx = NameContext { movie, file: None, custom_formats: vec![], collection: String::new() };
    let n = render(&cfg.movie_folder_format, &ctx, cfg);
    if n.is_empty() {
        finish(&movie.display(), cfg)
    } else {
        n
    }
}

pub fn file_name(movie: &Movie, file: &MovieFile, custom_formats: Vec<String>, cfg: &Config) -> String {
    let ctx = NameContext { movie, file: Some(file), custom_formats, collection: String::new() };
    let n = render(&cfg.standard_movie_format, &ctx, cfg);
    if n.is_empty() {
        finish(&movie.display(), cfg)
    } else {
        n
    }
}

/// A sample shown under the naming settings.
pub fn example(cfg: &Config, folder: bool) -> String {
    let movie = Movie {
        title: "The Movie: Title".into(),
        original_title: "The Original Movie Title".into(),
        year: 2010,
        imdb_id: "tt0066921".into(),
        tmdb_id: 345691,
        certification: "R".into(),
        ..Default::default()
    };
    let file = MovieFile {
        quality: crate::model::QualityModel { quality: crate::quality::id::BLURAY1080P, revision: crate::model::Revision { version: 2, real: 0, is_repack: false } },
        release_group: "RlsGrp".into(),
        edition: "IMAX".into(),
        scene_name: "The.Movie.Title.2010.1080p.BluRay.DTS.x264-RlsGrp".into(),
        media_info: crate::model::MediaInfo {
            video_codec: "x264".into(),
            audio_codec: "DTS".into(),
            audio_channels: 5.1,
            audio_languages: vec!["EN".into(), "DE".into()],
            video_bit_depth: 10,
            video_dynamic_range: "HDR10".into(),
            ..Default::default()
        },
        ..Default::default()
    };
    if folder {
        folder_name(&movie, cfg)
    } else {
        format!("{}.mkv", file_name(&movie, &file, vec!["Surround Sound".into(), "x264".into()], cfg))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates() {
        let mut cfg = Config::default();
        assert_eq!(example(&cfg, true), "The Movie - Title (2010)");
        assert_eq!(example(&cfg, false), "The Movie - Title (2010) Bluray-1080p Proper.mkv");
        cfg.standard_movie_format = "{Movie.CleanTitle}.{Release.Year}.{Edition.Tags}.{Quality.Full}.{MediaInfo.VideoCodec}-{Release Group}".into();
        assert_eq!(example(&cfg, false), "The.Movie.Title.2010.IMAX.Bluray-1080p.Proper.x264-RlsGrp.mkv");
        cfg.movie_folder_format = "{Movie CleanTitle} ({Release Year}) {imdb-{ImdbId}}".into();
        assert_eq!(example(&cfg, true), "The Movie Title (2010) {imdb-tt0066921}");
        cfg.movie_folder_format = "{MOVIE TITLE} [{Movie Certification}]".into();
        assert_eq!(example(&cfg, true), "THE MOVIE - TITLE [R]");
    }
}
