//! Port of Radarr's QualityParser.

use fancy_regex::Regex;
use once_cell::sync::Lazy;

use crate::model::{QualityModel, Revision};
use crate::quality::{id, source as src, Quality};

fn re(p: &str) -> Regex {
    Regex::new(p).unwrap_or_else(|e| panic!("bad regex {p}: {e}"))
}

static SOURCE: Lazy<Regex> = Lazy::new(|| {
    re(concat!(
        r"(?i)\b(?:",
        r"(?<bluray>M?Blu[-_. ]?Ray|HD[-_. ]?DVD|BD(?!$)|UHD2?BD|BDISO|BDMux|BD25|BD50|BR[-_. ]?DISK)|",
        r"(?<webdl>WEB[-_. ]?DL(?:mux)?|AmazonHD|AmazonSD|iTunesHD|MaxdomeHD|NetflixU?HD|WebHD|HBOMaxHD|DisneyHD|[. ]WEB[. ](?:[xh][ .]?26[45]|AVC|HEVC|DDP?5[. ]1)|[. ](?-i:WEB)$|(?:\d{3,4}0p)[-. ](?:Hybrid[-_. ]?)?WEB[-. ]|[-. ]WEB[-. ]\d{3,4}0p|\b\s/\sWEB\s/\s\b|(?:AMZN|NF|DP)[. -]WEB[. -](?!Rip))|",
        r"(?<webrip>WebRip|Web-Rip|WEBMux)|",
        r"(?<hdtv>HDTV)|",
        r"(?<bdrip>BDRip|BDLight|HD[-_. ]?DVDRip|UHDBDRip)|",
        r"(?<brrip>BRRip)|",
        r"(?<dvdr>\d?x?M?DVD-?[R59])|",
        r"(?<dvd>DVD(?!-R)|DVDRip|xvidvd)|",
        r"(?<dsr>WS[-_. ]DSR|DSR)|",
        r"(?<regional>R[0-9]{1}|REGIONAL)|",
        r"(?<scr>SCR|SCREENER|DVDSCR|DVDSCREENER)|",
        r"(?<ts>TS[-_. ]|TELESYNCH?|HD-TS|HDTS|PDVD|TSRip|HDTSRip)|",
        r"(?<tc>TC|TELECINE|HD-TC|HDTC)|",
        r"(?<cam>CAMRIP|(?:NEW)?CAM|HD-?CAM(?:Rip)?|HQCAM)|",
        r"(?<wp>WORKPRINT|WP)|",
        r"(?<pdtv>PDTV)|",
        r"(?<sdtv>SDTV)|",
        r"(?<tvrip>TVRip)",
        r")(?:\b|$|[ .])"
    ))
});

static RAWHD: Lazy<Regex> = Lazy::new(|| re(r"(?i)\b(?<rawhd>RawHD|Raw[-_. ]HD)\b"));
static MPEG2: Lazy<Regex> = Lazy::new(|| re(r"\b(?<mpeg2>MPEG[-_. ]?2)\b"));
static BRDISK: Lazy<Regex> = Lazy::new(|| {
    re(r"(?i)^(?!.*\b((?<!HD[._ -])(?<!HD)DVD|BDRip|720p|MKV|XviD|WMV|d3g|(BD)?REMUX|^(?=.*1080p)(?=.*HEVC)|[xh][-_. ]?26[45]|German.*[DM]L|((?<=\d{4}).*German.*([DM]L)?)(?=.*\b(AVC|HEVC|VC[-_. ]?1|MVC|MPEG[-_. ]?2)\b))\b)(((?=.*\b(Blu[-_. ]?ray|BD|HD[-_. ]?DVD)\b)(?=.*\b(AVC|HEVC|VC[-_. ]?1|MVC|MPEG[-_. ]?2|BDMV|ISO)\b))|^((?=.*\b(((?=.*\b((.*_)?COMPLETE.*|Dis[ck])\b)(?=.*(Blu[-_. ]?ray|HD[-_. ]?DVD)))|3D[-_. ]?BD|BR[-_. ]?DISK|Full[-_. ]?Blu[-_. ]?ray|^((?=.*((BD|UHD)[-_. ]?(25|50|66|100|ISO)))))))).*")
});
static PROPER: Lazy<Regex> = Lazy::new(|| re(r"(?i)\b(?<proper>proper)\b"));
static REPACK: Lazy<Regex> = Lazy::new(|| re(r"(?i)\b(?<repack>repack\d?|rerip\d?)\b"));
static VERSION: Lazy<Regex> = Lazy::new(|| re(r"(?i)\d[-._ ]?v(?<v1>\d)[-._ ]|\[v(?<v2>\d)\]|repack(?<v3>\d)|rerip(?<v4>\d)"));
static REAL: Lazy<Regex> = Lazy::new(|| re(r"\b(?<real>REAL)\b"));
static RESOLUTION: Lazy<Regex> = Lazy::new(|| {
    re(r"(?i)\b(?:(?<R360p>360p)|(?<R480p>480p|480i|640x480|848x480)|(?<R540p>540p)|(?<R576p>576p)|(?<R720p>720p|1280x720|960p)|(?<R1080p>1080p|1920x1080|1440p|FHD|1080i|4kto1080p)|(?<R2160p>2160p|3840x2160|4k[-_. ](?:UHD|HEVC|BD|H\.?265)|(?:UHD|HEVC|BD|H\.?265)[-_. ]4k))\b")
});
static ALT_RESOLUTION: Lazy<Regex> = Lazy::new(|| re(r"(?i)\b(?<A>UHD)\b|(?<B>\[4K\])"));
static CODEC: Lazy<Regex> = Lazy::new(|| re(r"(?i)\b(?:(?<x264>x264)|(?<h264>h264)|(?<xvidhd>XvidHD)|(?<xvid>X-?vid)|(?<divx>divx))\b"));
static OTHER_SOURCE: Lazy<Regex> = Lazy::new(|| re(r"(?i)(?<hdtv>HD[-_. ]TV)|(?<sdtv>SD[-_. ]TV)"));
static ANIME_BLURAY: Lazy<Regex> = Lazy::new(|| re(r"(?i)bd(?:720|1080|2160)|(?<=[-_. (\[])bd(?=[-_. )\]])"));
static ANIME_WEBDL: Lazy<Regex> = Lazy::new(|| re(r"(?i)\[WEB\]|[\[\(]WEB[ .]"));
static HIGHDEF_PDTV: Lazy<Regex> = Lazy::new(|| re(r"(?i)hr[-_. ]ws"));
static REMUX: Lazy<Regex> = Lazy::new(|| {
    re(r"(?i)(?:[_. \[]|\d{4}p-|\bHybrid-)(?<remux>(?:(BD|UHD)[-_. ]?)?Remux)\b|(?<remux2>(?:(BD|UHD)[-_. ]?)?Remux[_. ]\d{4}p)")
});
static GERMAN_REMUX: Lazy<Regex> = Lazy::new(|| {
    re(r"(?i)((?<=\d{4}).*German.*([DM]L)?)(?=.*\b(AVC|HEVC|VC[_. -]?1|MVC|MPEG[_. -]?2))(?=.*Blu-?ray)")
});

fn is_match(r: &Regex, s: &str) -> bool {
    r.is_match(s).unwrap_or(false)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Resolution {
    Unknown,
    R360,
    R480,
    R540,
    R576,
    R720,
    R1080,
    R2160,
}

pub fn parse_resolution(name: &str) -> Resolution {
    if let Ok(Some(c)) = RESOLUTION.captures(name) {
        for (g, r) in [
            ("R360p", Resolution::R360),
            ("R480p", Resolution::R480),
            ("R540p", Resolution::R540),
            ("R576p", Resolution::R576),
            ("R720p", Resolution::R720),
            ("R1080p", Resolution::R1080),
            ("R2160p", Resolution::R2160),
        ] {
            if c.name(g).is_some() {
                return r;
            }
        }
    }
    if is_match(&ALT_RESOLUTION, name) {
        return Resolution::R2160;
    }
    Resolution::Unknown
}

pub fn parse_quality(name: &str) -> QualityModel {
    let name = name.trim();
    if name.is_empty() {
        return QualityModel { quality: id::UNKNOWN, revision: Revision::new() };
    }
    let mut result = parse_quality_name(name);
    if result.quality == id::UNKNOWN {
        if let Some(ext) = std::path::Path::new(name).extension().and_then(|e| e.to_str()) {
            if ext.len() <= 5 {
                result.quality = crate::quality::from_extension(ext);
            }
        }
    }
    result
}

fn modifiers(name: &str, normalized: &str) -> QualityModel {
    let mut rev = Revision::new();
    let version = VERSION.captures(normalized).ok().flatten().and_then(|c| {
        ["v1", "v2", "v3", "v4"].iter().find_map(|g| c.name(g)).and_then(|m| m.as_str().parse::<i32>().ok())
    });
    if let Some(v) = version {
        rev.version = v;
    }
    if is_match(&PROPER, normalized) {
        rev.version = version.map(|v| v + 1).unwrap_or(2);
    }
    if is_match(&REPACK, normalized) {
        rev.version = version.map(|v| v + 1).unwrap_or(2);
        rev.is_repack = true;
    }
    rev.real = REAL.find_iter(name).filter(|m| m.is_ok()).count() as i32;
    QualityModel { quality: id::UNKNOWN, revision: rev }
}

fn contains_ci(hay: &str, needle: &str) -> bool {
    hay.to_ascii_lowercase().contains(&needle.to_ascii_lowercase())
}

pub fn parse_quality_name(name: &str) -> QualityModel {
    let normalized = name.replace('_', " ").trim().to_string();
    let n = normalized.as_str();
    let mut result = modifiers(name, n);
    let set = |mut r: QualityModel, q: i32| {
        r.quality = q;
        r
    };

    let source_caps = {
        let mut last = None;
        for c in SOURCE.captures_iter(n).flatten() {
            last = Some(c);
        }
        last
    };
    let resolution = parse_resolution(n);
    let codec = CODEC.captures(n).ok().flatten();
    let codec_has = |g: &str| codec.as_ref().map(|c| c.name(g).is_some()).unwrap_or(false);
    let remux = is_match(&REMUX, n) || is_match(&GERMAN_REMUX, n);
    let brdisk = is_match(&BRDISK, n);

    if is_match(&RAWHD, n) && !brdisk {
        return set(result, id::RAWHD);
    }

    use Resolution as R;
    if let Some(c) = &source_caps {
        let has = |g: &str| c.name(g).is_some();
        if has("bluray") {
            if brdisk {
                return set(result, id::BRDISK);
            }
            if codec_has("xvid") || codec_has("divx") {
                return set(result, id::BLURAY480P);
            }
            return set(
                result,
                match resolution {
                    R::R2160 => if remux { id::REMUX2160P } else { id::BLURAY2160P },
                    R::R1080 => if remux { id::REMUX1080P } else { id::BLURAY1080P },
                    R::R720 => id::BLURAY720P,
                    R::R576 => id::BLURAY576P,
                    R::R360 | R::R480 | R::R540 => id::BLURAY480P,
                    R::Unknown => if remux { id::REMUX1080P } else { id::BLURAY720P },
                },
            );
        }
        if has("webdl") {
            return set(
                result,
                match resolution {
                    R::R2160 => id::WEBDL2160P,
                    R::R1080 => id::WEBDL1080P,
                    R::R720 => id::WEBDL720P,
                    _ if name.contains("[WEBDL]") => id::WEBDL720P,
                    _ => id::WEBDL480P,
                },
            );
        }
        if has("webrip") {
            return set(
                result,
                match resolution {
                    R::R2160 => id::WEBRIP2160P,
                    R::R1080 => id::WEBRIP1080P,
                    R::R720 => id::WEBRIP720P,
                    _ => id::WEBRIP480P,
                },
            );
        }
        if has("scr") {
            return set(result, id::DVDSCR);
        }
        if has("cam") {
            return set(result, id::CAM);
        }
        if has("ts") {
            return set(result, id::TELESYNC);
        }
        if has("tc") {
            return set(result, id::TELECINE);
        }
        if has("wp") {
            return set(result, id::WORKPRINT);
        }
        if has("regional") {
            return set(result, id::REGIONAL);
        }
        if has("hdtv") {
            if is_match(&MPEG2, n) {
                return set(result, id::RAWHD);
            }
            return set(
                result,
                match resolution {
                    R::R2160 => id::HDTV2160P,
                    R::R1080 => id::HDTV1080P,
                    R::R720 => id::HDTV720P,
                    _ if name.contains("[HDTV]") => id::HDTV720P,
                    _ => id::SDTV,
                },
            );
        }
        if has("bdrip") || has("brrip") {
            return set(
                result,
                match resolution {
                    R::R720 => id::BLURAY720P,
                    R::R1080 => id::BLURAY1080P,
                    R::R2160 => id::BLURAY2160P,
                    R::R576 => id::BLURAY576P,
                    _ => id::BLURAY480P,
                },
            );
        }
        if has("dvdr") {
            return set(result, id::DVDR);
        }
        if has("dvd") {
            return set(result, id::DVD);
        }
        if has("pdtv") || has("sdtv") || has("dsr") || has("tvrip") {
            if resolution == R::R1080 || contains_ci(n, "1080p") {
                return set(result, id::HDTV1080P);
            }
            if resolution == R::R720 || contains_ci(n, "720p") || is_match(&HIGHDEF_PDTV, n) {
                return set(result, id::HDTV720P);
            }
            return set(result, id::SDTV);
        }
    }

    if source_caps.is_none() && remux && resolution != R::Unknown {
        match resolution {
            R::R480 => return set(result, id::BLURAY480P),
            R::R720 => return set(result, id::BLURAY720P),
            R::R2160 => return set(result, id::REMUX2160P),
            R::R1080 => return set(result, id::REMUX1080P),
            _ => {}
        }
    }

    if is_match(&ANIME_BLURAY, n) {
        if matches!(resolution, R::R360 | R::R480 | R::R540 | R::R576) || contains_ci(n, "480p") {
            return set(result, id::DVD);
        }
        if resolution == R::R1080 || contains_ci(n, "1080p") {
            return set(result, if remux { id::REMUX1080P } else { id::BLURAY1080P });
        }
        if resolution == R::R2160 || contains_ci(n, "2160p") {
            return set(result, if remux { id::REMUX2160P } else { id::BLURAY2160P });
        }
        if remux && resolution != R::R720 {
            return set(result, id::REMUX1080P);
        }
        return set(result, id::BLURAY720P);
    }

    if is_match(&ANIME_WEBDL, n) {
        if matches!(resolution, R::R360 | R::R480 | R::R540 | R::R576) || contains_ci(n, "480p") {
            return set(result, id::WEBDL480P);
        }
        if resolution == R::R1080 || contains_ci(n, "1080p") {
            return set(result, id::WEBDL1080P);
        }
        if resolution == R::R2160 || contains_ci(n, "2160p") {
            return set(result, id::WEBDL2160P);
        }
        return set(result, id::WEBDL720P);
    }

    if resolution != R::Unknown {
        let (mut source, mut modifier) = (src::UNKNOWN, crate::quality::modifier::NONE);
        if remux {
            source = src::BLURAY;
            modifier = crate::quality::modifier::REMUX;
        } else if let Some(ext) = std::path::Path::new(name).extension().and_then(|e| e.to_str()) {
            let q = crate::quality::get(crate::quality::from_extension(ext));
            if q.id != id::UNKNOWN {
                source = q.source;
            }
        }
        let pick = |res: i32, fallback: i32| -> i32 {
            if source == src::UNKNOWN {
                fallback
            } else {
                let q: Quality = crate::quality::find(source, res, modifier);
                if q.id == id::UNKNOWN { fallback } else { q.id }
            }
        };
        match resolution {
            R::R2160 => return set(result, pick(2160, id::HDTV2160P)),
            R::R1080 => return set(result, pick(1080, id::HDTV1080P)),
            R::R720 => return set(result, pick(720, id::HDTV720P)),
            _ => return set(result, pick(480, id::SDTV)),
        }
    }

    if codec_has("x264") {
        return set(result, id::SDTV);
    }
    if contains_ci(n, "848x480") {
        let q = if n.contains("dvd") {
            id::DVD
        } else if contains_ci(n, "bluray") {
            id::BLURAY480P
        } else {
            id::SDTV
        };
        return set(result, q);
    }
    if contains_ci(n, "1280x720") {
        return set(result, if contains_ci(n, "bluray") { id::BLURAY720P } else { id::HDTV720P });
    }
    if contains_ci(n, "1920x1080") {
        return set(result, if contains_ci(n, "bluray") { id::BLURAY1080P } else { id::HDTV1080P });
    }
    if contains_ci(n, "bluray720p") {
        return set(result, id::BLURAY720P);
    }
    if contains_ci(n, "bluray1080p") {
        return set(result, id::BLURAY1080P);
    }
    if contains_ci(n, "bluray2160p") {
        return set(result, id::BLURAY2160P);
    }
    if let Ok(Some(c)) = OTHER_SOURCE.captures(n) {
        if c.name("sdtv").is_some() {
            result.quality = id::SDTV;
        } else if c.name("hdtv").is_some() {
            result.quality = id::HDTV720P;
        }
    }
    result.quality = result.quality.max(id::UNKNOWN);
    result
}
