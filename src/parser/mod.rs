//! Port of Radarr's release-name parser: title, year, edition, group,
//! languages and quality from names like "Movie.Title.2019.1080p.BluRay-GRP".

pub mod language;
pub mod quality;

use fancy_regex::Regex;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

use crate::model::QualityModel;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ParsedMovieInfo {
    pub titles: Vec<String>,
    pub year: i32,
    pub edition: String,
    pub release_group: String,
    pub release_hash: String,
    pub quality: QualityModel,
    pub languages: Vec<String>,
    pub imdb_id: String,
    pub tmdb_id: i64,
    pub hardcoded_subs: String,
    pub release_title: String,
    pub simple_release_title: String,
}

impl ParsedMovieInfo {
    pub fn primary_title(&self) -> &str {
        self.titles.first().map(|s| s.as_str()).unwrap_or("")
    }
}

fn re(p: &str) -> Regex {
    Regex::new(p).unwrap_or_else(|e| panic!("bad regex {p}: {e}"))
}

const EDITION: &str = r"\(?\b(?<edition>(((Recut.|Extended.|Ultimate.)?(Director.?s|Collector.?s|Theatrical|Ultimate|Extended|Despecialized|(Special|Rouge|Final|Assembly|Imperial|Diamond|Signature|Hunter|Rekall)(?=(.(Cut|Edition|Version)))|\d{2,3}(th)?.Anniversary)(?:.(Cut|Edition|Version))?(.(Extended|Uncensored|Remastered|Unrated|Uncut|Open.?Matte|IMAX|Fan.?Edit))?|((Uncensored|Remastered|Unrated|Uncut|Open?.Matte|IMAX|Fan.?Edit|Restored|((2|3|4)in1))))))\b\)?";

static REPORT_EDITION: Lazy<Regex> = Lazy::new(|| re(&format!("(?i)^.+?{EDITION}")));
static HARDCODED_SUBS: Lazy<Regex> = Lazy::new(|| re(r"(?i)\b((?<hcsub>(\w+(?<!SOFT)(?<!MULTI)(?<!HORRIBLE)SUBS?))|(?<hc>(HC|SUBBED)))\b"));

static TITLE_REGEXES: Lazy<Vec<Regex>> = Lazy::new(|| {
    vec![
        // Anime [Subgroup] with year in parentheses
        re(r"(?i)^(?:\[(?<subgroup>.+?)\][-_. ]?)(?<title>(?![(\[]).+?)\s*\((?<year>(1(8|9)|20)\d{2})\).*?(?<hash>\[\w{8}\])?(?:$|\.)"),
        // Anime [Subgroup] and year
        re(r"(?i)^(?:\[(?<subgroup>.+?)\][-_. ]?)(?<title>(?![(\[]).+?)?(?:(?:[-_\W](?<![)\[!]))*(?<year>(1(8|9)|20)\d{2}(?!p|i|x|\d+|\]|\W\d+)))+.*?(?<hash>\[\w{8}\])?(?:$|\.)"),
        // Anime [Subgroup] no year, versioned title, hash
        re(r"(?i)^(?:\[(?<subgroup>.+?)\][-_. ]?)(?<title>(?![(\[]).+?)((v)(?:\d{1,2})(?:([-_. ])))(\[.*)?(?:[\[(][^])])?.*?(?<hash>\[\w{8}\])(?:$|\.)"),
        // Anime [Subgroup] no year, info in double brackets, hash
        re(r"(?i)^(?:\[(?<subgroup>.+?)\][-_. ]?)(?<title>(?![(\[]).+?)(\[.*).*?(?<hash>\[\w{8}\])(?:$|\.)"),
        // Anime [Subgroup] no year, info in parentheses or brackets, hash
        re(r"(?i)^(?:\[(?<subgroup>.+?)\][-_. ]?)(?<title>(?![(\[]).+)(?:[\[(][^])]).*?(?<hash>\[\w{8}\])(?:$|\.)"),
        // German or TrueFrench tracker formats (year check happens after matching)
        re(&format!(r"(?i)^(?<title>(?![(\[]).+?)((\W|_))({EDITION}.{{1,3}})?(?:(?<!Good[_ .-])(?<!The[_ .-])(?<lang>German|TrueFrench))(.+?)(?=((19|20)\d{{2}}|$))(?<year>(19|20)\d{{2}}(?!p|i|\d+|\]|\W\d+))?(\W+|_|$)(?!\\)")),
        // Special editions: Mission.Impossible.3.Special.Edition.2011
        re(&format!(r"(?i)^(?<title>(?![(\[]).+?)?(?:(?:[-_\W](?<![)\[!]))*{EDITION}.{{1,3}}(?<year>(1(8|9)|20)\d{{2}}(?!p|i|\d+|\]|\W\d+)))+(\W+|_|$)(?!\\)")),
        // Normal movie format: Mission.Impossible.3.2011
        re(r"(?i)^(?<title>(?![(\[]).+?)?(?:(?:[-_\W](?<![)\[!]))*(?<year>(1(8|9)|20)\d{2}(?!p|i|(1(8|9)|20)\d{2}|\]|\W(1(8|9)|20)\d{2})))+(\W+|_|$)(?!\\)"),
        // PassThePopcorn: Star.Wars[PassThePopcorn]
        re(r"(?i)^(?<title>.+?)?(?:(?:[-_\W](?<![()\[!]))*(?<year>(\[\w *\])))+(\W+|_|$)(?!\\)"),
        // Years in brackets
        re(r"(?i)^(?<title>(?![(\[]).+?)?(?:(?:[-_\W](?<![)!]))*(?<year>(1(8|9)|20)\d{2}(?!p|i|\d+|\W\d+)))+(\W+|_|$)(?!\\)"),
        // Last resort for titles with ( or [ in them
        re(r"(?i)^(?<title>.+?)?(?:(?:[-_\W](?<![)\[!]))*(?<year>(1(8|9)|20)\d{2}(?!p|i|\d+|\]|\W\d+)))+(\W+|_|$)(?!\\)"),
    ]
});

static FOLDER_REGEX: Lazy<Regex> = Lazy::new(|| re(r"^(?:(?:[-_\W](?<![)!]))*(?<year>(19|20)\d{2}(?!p|i|\d+|\W\d+)))+(\W+|_|$)(?<title>.+?)?$"));

static REJECT_HASHED: Lazy<Vec<Regex>> = Lazy::new(|| {
    vec![
        re(r"^[0-9a-zA-Z]{32}"),
        re(r"^[a-z0-9]{24}$"),
        re(r"^[A-Z]{11}\d{3}$"),
        re(r"^[a-z]{12}\d{3}$"),
        re(r"^Backup_\d{5,}S\d{2}-\d{2}$"),
        re(r"^123$"),
        re(r"(?i)^abc$"),
        re(r"(?i)^abc[-_. ]xyz"),
        re(r"(?i)^b00bs$"),
    ]
});

static YEAR_ANYWHERE: Lazy<regex::Regex> = Lazy::new(|| regex::Regex::new(r"(19|20)\d{2}").unwrap());
static REVERSED: Lazy<Regex> = Lazy::new(|| re(r"(?:^|[-._ ])(p027|p0801)[-._ ]"));
static ALT_TITLE_SPLIT: Lazy<regex::Regex> = Lazy::new(|| regex::Regex::new(r"(?i)[ ]+(?:AKA|/)[ ]+").unwrap());
static BRACKETED_ALT: Lazy<regex::Regex> = Lazy::new(|| regex::Regex::new(r"(?i)(.*) \([ ]*AKA[ ]+(.*)\)").unwrap());
static NORMALIZE_AKA: Lazy<regex::Regex> = Lazy::new(|| regex::Regex::new(r"(?i)[ ]+(?:A\.K\.A\.)[ ]+").unwrap());
static IMDB_ID: Lazy<regex::Regex> = Lazy::new(|| regex::Regex::new(r"(?i)(?P<imdbid>tt\d{7,8})").unwrap());
static TMDB_ID: Lazy<regex::Regex> = Lazy::new(|| regex::Regex::new(r"(?i)tmdb(id)?-(?P<tmdbid>\d+)").unwrap());
static SIMPLE_TITLE: Lazy<Regex> = Lazy::new(|| {
    re(r"(?i)(?:(480|540|576|720|1080|2160)[ip]|[xh][\W_]?26[45]|DD\W?5\W1|[<>?*]|848x480|1280x720|1920x1080|3840x2160|4096x2160|(8|10)b(it)?|10-bit)\s*?(?![a-b0-9])")
});
static SIMPLE_RELEASE_TITLE: Lazy<regex::Regex> = Lazy::new(|| regex::Regex::new(r"\s*(?:[<>?*|])").unwrap());
static WEBSITE_PREFIX: Lazy<Regex> = Lazy::new(|| {
    re(r"(?i)^(?:(?:\[|\()\s*)?(?:www\.)?[-a-z0-9-]{1,256}\.(?:[a-z]{2,6}\.[a-z]{2,6}|xn--[a-z0-9-]{4,}|[a-z]{2,})\b(?:\s*(?:\]|\))|[ -]{2,})[ -]*")
});
static WEBSITE_POSTFIX: Lazy<regex::Regex> = Lazy::new(|| {
    regex::Regex::new(r"(?i)(?:\[\s*)?(?:www\.)?[-a-z0-9-]{1,256}\.(?:xn--[a-z0-9-]{4,}|[a-z]{2,6})\b(?:\s*\])$").unwrap()
});
static CLEAN_TORRENT_SUFFIX: Lazy<regex::Regex> = Lazy::new(|| regex::Regex::new(r"(?i)\[(?:ettv|rartv|rarbg|cttv|publichd)\]$").unwrap());
static CLEAN_QUALITY_BRACKETS: Lazy<regex::Regex> = Lazy::new(|| regex::Regex::new(r"(?i)\[[a-z0-9 ._-]+\]$").unwrap());
static REQUEST_INFO: Lazy<regex::Regex> = Lazy::new(|| regex::Regex::new(r"^(?:\[.+?\])+").unwrap());

// Release groups
static CLEAN_RELEASE_GROUP: Lazy<regex::Regex> = Lazy::new(|| {
    regex::Regex::new(r"(?i)(-(RP|1|NZBGeek|Obfuscated|Obfuscation|Scrambled|sample|Pre|postbot|xpost|Rakuv[a-z0-9]*|WhiteRev|BUYMORE|AsRequested|AlternativeToRequested|GEROV|Z0iDS3N|Chamele0n|4P|4Planet|AlteZachen|RePACKPOST))+$").unwrap()
});
static EXCEPTION_GROUP_EXACT: Lazy<Regex> = Lazy::new(|| {
    re(r"(?i)\b(?<releasegroup>KRaLiMaRKo|E\.N\.D|D\-Z0N3|Koten_Gars|BluDragon|ZØNEHD|HQMUX|VARYG|YIFY|YTS(.(MX|LT|AG))?|TMd|Eml HDTeam|LMain|DarQ|BEN THE MEN|TAoE|QxR|126811)\b")
});
static EXCEPTION_GROUP: Lazy<Regex> = Lazy::new(|| {
    re(r"(?i)(?<=[._ \[])(?<releasegroup>(Silence|afm72|Panda|Ghost|MONOLITH|Tigole|Joy|ImE|UTR|t3nzin|Anime Time|Project Angel|Hakata Ramen|HONE|GiLG|Vyndros|SEV|Garshasp|Kappa|Natty|RCVR|SAMPA|YOGI|r00t|EDGE2020|RZeroX|FreetheFish|Anna|Bandi|Qman|theincognito|HDO|DusIctv|DHD|CtrlHD|-ZR-|ADC|XZVN|RH|Kametsu|Celdra)(?=\]|\)))")
});
static ANIME_GROUP: Lazy<Regex> = Lazy::new(|| re(r"(?i)^(?:\[(?<subgroup>(?!\s).+?(?<!\s))\](?:_|-|\s|\.)?)"));
static GROUP_CANDIDATE: Lazy<regex::Regex> = Lazy::new(|| regex::Regex::new(r"(?i)-([a-z0-9]+(?:-[a-z0-9]+)?)(?:\b|[-._ ]|$)").unwrap());
static GROUP_BRACKET: Lazy<regex::Regex> = Lazy::new(|| regex::Regex::new(r"(?i)[-._ ]\[([a-z0-9]+)\]$").unwrap());
static TRAILING_TAGS: Lazy<regex::Regex> = Lazy::new(|| regex::Regex::new(r"(?i)^(?:[-_. ]*(?:[\[(][^\])]*[\])]|english|french|german|spanish|italian|dutch|nordic|multi|subs?)\s*)+$").unwrap());
static INVALID_GROUP: Lazy<regex::Regex> = Lazy::new(|| regex::Regex::new(r"(?i)^([se]\d+|[0-9a-f]{8})$").unwrap());
static RESOLUTION_WORD: Lazy<regex::Regex> = Lazy::new(|| regex::Regex::new(r"(?i)(?:480p|576p|720p|1080p|2160p)").unwrap());

fn remove_extension(title: &str) -> String {
    if let Some(dot) = title.rfind('.') {
        let ext = title[dot + 1..].to_ascii_lowercase();
        let known = crate::quality::VIDEO_EXTENSIONS.contains(&ext.as_str())
            || matches!(ext.as_str(), "par2" | "nzb" | "torrent" | "srt" | "rar" | "zip" | "7z");
        if known {
            return title[..dot].to_string();
        }
    }
    title.to_string()
}

pub fn parse_release_group(title: &str) -> String {
    let title = remove_extension(title.trim());
    let title = CLEAN_RELEASE_GROUP.replace(&title, "").to_string();

    if let Some(c) = EXCEPTION_GROUP_EXACT.captures_iter(&title).flatten().last() {
        if let Some(m) = c.name("releasegroup") {
            return m.as_str().to_string();
        }
    }
    if let Ok(Some(c)) = EXCEPTION_GROUP.captures(&title) {
        if let Some(m) = c.name("releasegroup") {
            return m.as_str().to_string();
        }
    }
    if let Ok(Some(c)) = ANIME_GROUP.captures(&title) {
        if let Some(m) = c.name("subgroup") {
            return m.as_str().to_string();
        }
    }

    const NOT_GROUPS: &[&str] = &[
        "WEB-DL", "WEB-Rip", "Blu-Ray", "DTS-HD", "DTS-X", "DTS-MA", "DTS-ES", "HD-DVD", "HD-TS", "HD-TC", "HD-CAM",
        "Web-Rip",
    ];
    const NOT_SUFFIX: &[&str] = &["ES", "EN", "CAT", "ENG", "JAP", "GER", "FRA", "FRE", "ITA", "HDRip", "HD", "DL", "Ray", "Rip", "MA", "X"];
    let mut best: Option<String> = None;
    for c in GROUP_CANDIDATE.captures_iter(&title) {
        let m = c.get(1).unwrap();
        let mut cand = m.as_str().to_string();
        let start = m.start().saturating_sub(1);
        // Something resolution-like after it means this dash is mid-title.
        if RESOLUTION_WORD.is_match(&title[m.end()..]) {
            continue;
        }
        let before = &title[..start];
        let around = format!("{}-{}", before.rsplit(['.', ' ', '_']).next().unwrap_or(""), cand);
        if NOT_GROUPS.iter().any(|g| around.eq_ignore_ascii_case(g) || around.to_ascii_lowercase().starts_with(&format!("{}-", g.to_ascii_lowercase()))) {
            // "WEB-DL-GROUP": take what follows.
            if let Some((_, rest)) = cand.split_once('-') {
                cand = rest.to_string();
            } else {
                continue;
            }
        }
        if NOT_SUFFIX.iter().any(|s| cand.eq_ignore_ascii_case(s)) {
            continue;
        }
        if cand.chars().all(|ch| ch.is_ascii_digit()) && (cand.len() == 2 || before.ends_with(|ch: char| ch.is_ascii_digit())) {
            continue;
        }
        if cand.eq_ignore_ascii_case("bit") || cand.to_ascii_lowercase().starts_with("tt") && cand[2..].chars().all(|c| c.is_ascii_digit()) && cand.len() > 8 {
            continue;
        }
        if before.to_ascii_lowercase().ends_with("tmdb") || before.to_ascii_lowercase().ends_with("tmdbid") {
            continue;
        }
        if cand.split('-').any(|seg| INVALID_GROUP.is_match(seg) || RESOLUTION_WORD.is_match(seg)) {
            continue;
        }
        if cand.chars().all(|ch| ch.is_ascii_digit() || ch == '-') {
            continue;
        }
        const LANG_CODES: &[&str] = &["ES", "EN", "CAT", "ENG", "JAP", "GER", "FRA", "FRE", "ITA", "SPA", "RUS", "POR", "LAT"];
        if cand.contains('-') && cand.split('-').all(|seg| LANG_CODES.iter().any(|l| seg.eq_ignore_ascii_case(l))) {
            continue;
        }
        // Radarr's group sits at the end, maybe before [tags] or (tags).
        let rest = title[m.end()..].trim();
        let trailer_ok = rest.is_empty() || TRAILING_TAGS.is_match(rest);
        if !trailer_ok {
            continue;
        }
        if best.is_none() {
            best = Some(cand);
        }
    }
    if let Some(b) = best {
        return b;
    }
    if let Some(c) = GROUP_BRACKET.captures(&title) {
        return c[1].to_string();
    }
    String::new()
}

pub fn parse_edition(title: &str) -> String {
    REPORT_EDITION
        .captures(title)
        .ok()
        .flatten()
        .and_then(|c| c.name("edition").map(|m| m.as_str().replace('.', " ")))
        .unwrap_or_default()
}

pub fn parse_hardcoded_subs(title: &str) -> String {
    let mut last = None;
    for c in HARDCODED_SUBS.captures_iter(title).flatten() {
        last = Some(c);
    }
    match last {
        Some(c) if c.name("hcsub").is_some() => c.name("hcsub").unwrap().as_str().to_string(),
        Some(c) if c.name("hc").is_some() => "Generic Hardcoded Subs".into(),
        _ => String::new(),
    }
}

pub fn parse_imdb_id(title: &str) -> String {
    IMDB_ID
        .captures(title)
        .map(|c| c["imdbid"].to_string())
        .filter(|s| s.len() == 9 || s.len() == 10)
        .unwrap_or_default()
}

pub fn parse_tmdb_id(title: &str) -> i64 {
    TMDB_ID.captures(title).and_then(|c| c["tmdbid"].parse().ok()).unwrap_or(0)
}

fn valid_before_parsing(title: &str) -> bool {
    let lower = title.to_lowercase();
    if lower.contains("password") && lower.contains("yenc") {
        return false;
    }
    if !title.chars().any(|c| c.is_alphanumeric()) {
        return false;
    }
    let t = remove_extension(title);
    !REJECT_HASHED.iter().any(|r| r.is_match(&t).unwrap_or(false))
}

fn tidy_title(raw: &str) -> String {
    let movie_name = raw.replace('_', " ");
    let movie_name = NORMALIZE_AKA.replace_all(&movie_name, " AKA ").to_string();
    let movie_name = REQUEST_INFO.replace(&movie_name, "").trim().to_string();
    let parts: Vec<&str> = movie_name.split('.').collect();
    let mut out = String::new();
    let mut previous_acronym = false;
    for (n, part) in parts.iter().enumerate() {
        let next = parts.get(n + 1).copied().unwrap_or("");
        let is_num = |s: &str| !s.is_empty() && s.parse::<i64>().is_ok();
        if part.chars().count() == 1
            && part.to_lowercase() != "a"
            && !is_num(part)
            && (previous_acronym || n < parts.len() - 1)
            && (previous_acronym || next.chars().count() != 1 || !is_num(next))
        {
            out.push_str(part);
            out.push('.');
            previous_acronym = true;
        } else if part.to_lowercase() == "a" && (previous_acronym || next.chars().count() == 1) {
            out.push_str(part);
            out.push('.');
            previous_acronym = true;
        } else if part.to_lowercase() == "dr" {
            out.push_str(part);
            out.push('.');
            previous_acronym = true;
        } else {
            if previous_acronym {
                out.push(' ');
                previous_acronym = false;
            }
            out.push_str(part);
            out.push(' ');
        }
    }
    out.trim().to_string()
}

pub fn parse_movie_title(title: &str, is_dir: bool) -> Option<ParsedMovieInfo> {
    let original = title.to_string();
    if !valid_before_parsing(title) {
        return None;
    }
    let mut title = title.to_string();
    if REVERSED.is_match(&title).unwrap_or(false) {
        let stem = remove_extension(&title);
        let rev: String = stem.chars().rev().collect();
        title = format!("{rev}{}", &title[stem.len()..]);
    }
    let release_title = remove_extension(&title)
        .trim_matches(|c| c == '-' || c == '_')
        .replace('【', "[")
        .replace('】', "]");

    let mut simple = SIMPLE_TITLE.replace_all(&release_title, "").to_string();
    simple = WEBSITE_PREFIX.replace(&simple, "").to_string();
    simple = WEBSITE_POSTFIX.replace(&simple, "").to_string();
    simple = CLEAN_TORRENT_SUFFIX.replace(&simple, "").to_string();
    simple = CLEAN_QUALITY_BRACKETS
        .replace(&simple, |c: &regex::Captures| {
            if quality::parse_quality_name(&c[0]).quality != crate::quality::id::UNKNOWN {
                String::new()
            } else {
                c[0].to_string()
            }
        })
        .to_string();

    let mut regexes: Vec<&Regex> = TITLE_REGEXES.iter().collect();
    if is_dir {
        regexes.push(&FOLDER_REGEX);
    }
    for r in regexes {
        let Ok(Some(c)) = r.captures(&simple) else { continue };
        let Some(tm) = c.name("title") else { continue };
        // Radarr: the language word must not come after a year.
        if let Some(lang) = c.name("lang") {
            if YEAR_ANYWHERE.is_match(&simple[..lang.start()]) {
                continue;
            }
        }
        if tm.as_str() == "(" {
            continue;
        }
        let movie_name = tidy_title(tm.as_str());
        let year: i32 = c.name("year").and_then(|y| y.as_str().parse().ok()).unwrap_or(0);
        let mut info = ParsedMovieInfo { year, ..Default::default() };
        if let Some(e) = c.name("edition") {
            info.edition = e.as_str().replace('.', " ");
        }
        let mut titles = vec![movie_name.clone()];
        let unbracketed = BRACKETED_ALT.replace(&movie_name, "$1 AKA $2").to_string();
        for alt in ALT_TITLE_SPLIT.split(&unbracketed) {
            if !alt.trim().is_empty() && alt != movie_name && !titles.iter().any(|t| t == alt) {
                titles.push(alt.to_string());
            }
        }
        info.titles = titles;

        let mut simple_release = SIMPLE_RELEASE_TITLE.replace_all(&release_title, "").to_string();
        let replace_with = if tm.as_str().contains('.') { "A.Movie" } else { "A Movie" };
        if tm.end() <= simple_release.len() && simple_release.is_char_boundary(tm.start()) && simple_release.is_char_boundary(tm.end()) && &simple_release[tm.start()..tm.end()] == tm.as_str() {
            simple_release.replace_range(tm.start()..tm.end(), replace_with);
        } else if !tm.as_str().is_empty() {
            simple_release = simple_release.replacen(tm.as_str(), replace_with, 1);
        }
        info.release_group = parse_release_group(&simple_release);
        if let Some(sg) = c.name("subgroup") {
            info.release_group = sg.as_str().to_string();
        }
        info.hardcoded_subs = parse_hardcoded_subs(&title);
        let lang_source = if info.release_group.is_empty() {
            simple_release.clone()
        } else {
            simple_release.replace(&info.release_group, "RlsGrp")
        };
        info.languages = language::parse_languages(&lang_source);
        info.quality = quality::parse_quality(&title);
        if info.edition.is_empty() {
            info.edition = parse_edition(&simple_release);
        }
        if let Some(h) = c.name("hash") {
            let h = h.as_str().trim_matches(|c| c == '[' || c == ']');
            if h != "1280x720" {
                info.release_hash = h.to_string();
            }
        }
        info.imdb_id = parse_imdb_id(&simple_release);
        info.tmdb_id = parse_tmdb_id(&simple_release);
        info.release_title = release_title.clone();
        info.simple_release_title = simple_release;
        let _ = &original;
        return Some(info);
    }
    None
}

/// Parse a file on disk using its name, then folder + name, then folder.
pub fn parse_movie_path(path: &std::path::Path) -> Option<ParsedMovieInfo> {
    let name = path.file_name()?.to_string_lossy().to_string();
    let dir = path.parent().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let ext = path.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    parse_movie_title(&name, true)
        .or_else(|| parse_movie_title(&format!("{dir} {name}"), false))
        .or_else(|| parse_movie_title(&format!("{dir}{ext}"), false))
}

fn remove_accents(s: &str) -> String {
    s.nfd().filter(|c| !unicode_normalization::char::is_combining_mark(*c)).collect()
}

fn umlauts(s: &str) -> String {
    s.replace('ä', "ae").replace('ö', "oe").replace('ü', "ue").replace('Ä', "Ae").replace('Ö', "Oe").replace('Ü', "Ue").replace('ß', "ss")
}

/// Radarr's CleanMovieTitle: lowercase, no punctuation, no filler words
/// (except at the start), used to compare titles.
pub fn clean_title(title: &str) -> String {
    if title.trim().parse::<i64>().is_ok() {
        return title.trim().to_string();
    }
    let lower = umlauts(&title.to_lowercase());
    let words: Vec<String> = lower
        .split(|c: char| !(c.is_alphanumeric() || c == '\''))
        .filter(|w| !w.is_empty())
        .map(|w| w.replace('\'', ""))
        .collect();
    let mut out = String::new();
    for (i, w) in words.iter().enumerate() {
        let filler = matches!(w.as_str(), "a" | "à" | "an" | "the" | "and" | "or" | "of");
        // Keep a filler word that starts the title or ends it.
        if filler && i > 0 && i < words.len() - 1 {
            continue;
        }
        out.push_str(w);
    }
    remove_accents(&out)
}

/// For sort titles: lowercase, punctuation to spaces, articles dropped.
pub fn sort_title(title: &str) -> String {
    let t = title.to_lowercase();
    let t = t.strip_prefix("the ").or_else(|| t.strip_prefix("a ")).or_else(|| t.strip_prefix("an ")).unwrap_or(&t).to_string();
    let t: String = t.chars().map(|c| if c.is_alphanumeric() || c == ' ' { c } else { ' ' }).collect();
    t.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn normalize_imdb_id(s: &str) -> Option<String> {
    let s = s.trim();
    let digits = s.strip_prefix("tt").unwrap_or(s);
    if digits.is_empty() || digits.len() > 10 || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some(format!("tt{:0>7}", digits))
}

// ---------------------------------------------------------------- media info from names

static VIDEO_CODEC: Lazy<regex::Regex> = Lazy::new(|| {
    regex::Regex::new(r"(?i)\b(?:(?P<x265>x265|h\.?265|HEVC)|(?P<x264>x264|h\.?264|AVC)|(?P<av1>AV1)|(?P<xvid>XviD)|(?P<divx>DivX)|(?P<vc1>VC-?1)|(?P<mpeg2>MPEG-?2)|(?P<vp9>VP9))\b").unwrap()
});
static AUDIO: Lazy<regex::Regex> = Lazy::new(|| {
    regex::Regex::new(r"(?i)\b(?:(?P<atmos>Atmos)|(?P<truehd>TrueHD)|(?P<dtsx>DTS[-. ]?X)|(?P<dtshdma>DTS[-. ]?HD[-. ]?MA)|(?P<dtshd>DTS[-. ]?HD)|(?P<dts>DTS)|(?P<ddp>DDP|DD\+|E-?AC-?3)|(?P<dd>DD|AC-?3|Dolby ?Digital)|(?P<aac>AAC)|(?P<flac>FLAC)|(?P<opus>Opus)|(?P<mp3>MP3)|(?P<pcm>L?PCM))").unwrap()
});
static CHANNELS: Lazy<regex::Regex> = Lazy::new(|| regex::Regex::new(r"(?i)(?:DDP?|AAC|DTS[-. A-Z]*|TrueHD|FLAC|Atmos|\b)[ .]?(?P<ch>[1-9])[ .]([01])\b").unwrap());
static HDR: Lazy<regex::Regex> = Lazy::new(|| {
    regex::Regex::new(r"(?i)\b(?:(?P<dv>DV|DoVi|Dolby[ .]?Vision)|(?P<hdr10p>HDR10\+|HDR10Plus)|(?P<hdr10>HDR10)|(?P<hdr>HDR)|(?P<hlg>HLG)|(?P<pq>PQ))\b").unwrap()
});
static BITDEPTH: Lazy<regex::Regex> = Lazy::new(|| regex::Regex::new(r"(?i)\b(?P<b>8|10|12)[ .-]?bit\b").unwrap());

/// Best-effort media info from a release name (used when ffprobe isn't there).
pub fn media_info_from_name(name: &str) -> crate::model::MediaInfo {
    let mut mi = crate::model::MediaInfo::default();
    if let Some(c) = VIDEO_CODEC.captures(name) {
        mi.video_codec = ["x265", "x264", "av1", "xvid", "divx", "vc1", "mpeg2", "vp9"]
            .iter()
            .find(|g| c.name(g).is_some())
            .map(|g| match *g {
                "x265" => "x265",
                "x264" => "x264",
                "av1" => "AV1",
                "xvid" => "XviD",
                "divx" => "DivX",
                "vc1" => "VC1",
                "mpeg2" => "MPEG2",
                _ => "VP9",
            })
            .unwrap_or("")
            .to_string();
    }
    let mut audio = Vec::new();
    for c in AUDIO.captures_iter(name) {
        for (g, label) in [
            ("atmos", "Atmos"),
            ("truehd", "TrueHD"),
            ("dtsx", "DTS-X"),
            ("dtshdma", "DTS-HD MA"),
            ("dtshd", "DTS-HD"),
            ("dts", "DTS"),
            ("ddp", "EAC3"),
            ("dd", "AC3"),
            ("aac", "AAC"),
            ("flac", "FLAC"),
            ("opus", "Opus"),
            ("mp3", "MP3"),
            ("pcm", "PCM"),
        ] {
            if c.name(g).is_some() && !audio.contains(&label) {
                audio.push(label);
            }
        }
    }
    // "TrueHD Atmos" reads better than "Atmos TrueHD".
    if audio.contains(&"Atmos") && audio.len() > 1 {
        audio.retain(|a| *a != "Atmos");
        audio.push("Atmos");
    }
    mi.audio_codec = audio.join(" ");
    if let Some(c) = CHANNELS.captures(name) {
        let main: f32 = c["ch"].parse().unwrap_or(0.0);
        let lfe: f32 = c.get(2).and_then(|m| m.as_str().parse().ok()).unwrap_or(0.0);
        mi.audio_channels = main + lfe / 10.0;
    }
    let mut hdr = Vec::new();
    for c in HDR.captures_iter(name) {
        for (g, label) in [("dv", "DV"), ("hdr10p", "HDR10Plus"), ("hdr10", "HDR10"), ("hdr", "HDR"), ("hlg", "HLG"), ("pq", "PQ")] {
            if c.name(g).is_some() && !hdr.contains(&label) {
                hdr.push(label);
            }
        }
    }
    if hdr.len() > 1 {
        hdr.retain(|h| *h != "HDR");
    }
    mi.video_dynamic_range = hdr.join(" ");
    if let Some(c) = BITDEPTH.captures(name) {
        mi.video_bit_depth = c["b"].parse().unwrap_or(0);
    }
    mi.resolution = match quality::parse_resolution(name) {
        quality::Resolution::R2160 => "2160p",
        quality::Resolution::R1080 => "1080p",
        quality::Resolution::R720 => "720p",
        quality::Resolution::R576 => "576p",
        quality::Resolution::R480 | quality::Resolution::R360 | quality::Resolution::R540 => "480p",
        quality::Resolution::Unknown => "",
    }
    .to_string();
    mi
}

#[cfg(test)]
mod tests;
