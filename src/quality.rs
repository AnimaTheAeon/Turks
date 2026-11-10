//! Radarr's quality list, default sizes and default profiles.

use crate::model::{ProfileItem, QualityProfile, QualitySize};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Quality {
    pub id: i32,
    pub name: &'static str,
    pub source: i32,
    pub resolution: i32,
    pub modifier: i32,
    /// Default ranking weight; equal weights form a group (WEB-DL + WEBRip).
    pub weight: i32,
    pub group: &'static str,
}

pub mod source {
    pub const UNKNOWN: i32 = 0;
    pub const CAM: i32 = 1;
    pub const TELESYNC: i32 = 2;
    pub const TELECINE: i32 = 3;
    pub const WORKPRINT: i32 = 4;
    pub const DVD: i32 = 5;
    pub const TV: i32 = 6;
    pub const WEBDL: i32 = 7;
    pub const WEBRIP: i32 = 8;
    pub const BLURAY: i32 = 9;
    pub const NAMES: [&str; 10] = ["Unknown", "CAM", "Telesync", "Telecine", "Workprint", "DVD", "TV", "WEB-DL", "WEBRip", "Bluray"];
}

pub mod modifier {
    pub const NONE: i32 = 0;
    pub const REGIONAL: i32 = 1;
    pub const SCREENER: i32 = 2;
    pub const RAWHD: i32 = 3;
    pub const BRDISK: i32 = 4;
    pub const REMUX: i32 = 5;
    pub const NAMES: [&str; 6] = ["None", "Regional", "Screener", "Raw-HD", "BR-DISK", "Remux"];
}

pub mod id {
    pub const UNKNOWN: i32 = 0;
    pub const SDTV: i32 = 1;
    pub const DVD: i32 = 2;
    pub const WEBDL1080P: i32 = 3;
    pub const HDTV720P: i32 = 4;
    pub const WEBDL720P: i32 = 5;
    pub const BLURAY720P: i32 = 6;
    pub const BLURAY1080P: i32 = 7;
    pub const WEBDL480P: i32 = 8;
    pub const HDTV1080P: i32 = 9;
    pub const RAWHD: i32 = 10;
    pub const WEBRIP480P: i32 = 12;
    pub const WEBRIP720P: i32 = 14;
    pub const WEBRIP1080P: i32 = 15;
    pub const HDTV2160P: i32 = 16;
    pub const WEBRIP2160P: i32 = 17;
    pub const WEBDL2160P: i32 = 18;
    pub const BLURAY2160P: i32 = 19;
    pub const BLURAY480P: i32 = 20;
    pub const BLURAY576P: i32 = 21;
    pub const BRDISK: i32 = 22;
    pub const DVDR: i32 = 23;
    pub const WORKPRINT: i32 = 24;
    pub const CAM: i32 = 25;
    pub const TELESYNC: i32 = 26;
    pub const TELECINE: i32 = 27;
    pub const DVDSCR: i32 = 28;
    pub const REGIONAL: i32 = 29;
    pub const REMUX1080P: i32 = 30;
    pub const REMUX2160P: i32 = 31;
}

use modifier as m;
use source as s;

const fn q(id: i32, name: &'static str, source: i32, resolution: i32, modifier: i32, weight: i32, group: &'static str) -> Quality {
    Quality { id, name, source, resolution, modifier, weight, group }
}

/// In Radarr's default order (worst to best).
pub const ALL: [Quality; 30] = [
    q(id::UNKNOWN, "Unknown", s::UNKNOWN, 0, m::NONE, 1, ""),
    q(id::WORKPRINT, "WORKPRINT", s::WORKPRINT, 0, m::NONE, 2, ""),
    q(id::CAM, "CAM", s::CAM, 0, m::NONE, 3, ""),
    q(id::TELESYNC, "TELESYNC", s::TELESYNC, 0, m::NONE, 4, ""),
    q(id::TELECINE, "TELECINE", s::TELECINE, 0, m::NONE, 5, ""),
    q(id::REGIONAL, "REGIONAL", s::DVD, 480, m::REGIONAL, 6, ""),
    q(id::DVDSCR, "DVDSCR", s::DVD, 480, m::SCREENER, 7, ""),
    q(id::SDTV, "SDTV", s::TV, 480, m::NONE, 8, ""),
    q(id::DVD, "DVD", s::DVD, 0, m::NONE, 9, ""),
    q(id::DVDR, "DVD-R", s::DVD, 480, m::REMUX, 10, ""),
    q(id::WEBDL480P, "WEBDL-480p", s::WEBDL, 480, m::NONE, 11, "WEB 480p"),
    q(id::WEBRIP480P, "WEBRip-480p", s::WEBRIP, 480, m::NONE, 11, "WEB 480p"),
    q(id::BLURAY480P, "Bluray-480p", s::BLURAY, 480, m::NONE, 12, ""),
    q(id::BLURAY576P, "Bluray-576p", s::BLURAY, 576, m::NONE, 13, ""),
    q(id::HDTV720P, "HDTV-720p", s::TV, 720, m::NONE, 14, ""),
    q(id::WEBDL720P, "WEBDL-720p", s::WEBDL, 720, m::NONE, 15, "WEB 720p"),
    q(id::WEBRIP720P, "WEBRip-720p", s::WEBRIP, 720, m::NONE, 15, "WEB 720p"),
    q(id::BLURAY720P, "Bluray-720p", s::BLURAY, 720, m::NONE, 16, ""),
    q(id::HDTV1080P, "HDTV-1080p", s::TV, 1080, m::NONE, 17, ""),
    q(id::WEBDL1080P, "WEBDL-1080p", s::WEBDL, 1080, m::NONE, 18, "WEB 1080p"),
    q(id::WEBRIP1080P, "WEBRip-1080p", s::WEBRIP, 1080, m::NONE, 18, "WEB 1080p"),
    q(id::BLURAY1080P, "Bluray-1080p", s::BLURAY, 1080, m::NONE, 19, ""),
    q(id::REMUX1080P, "Remux-1080p", s::BLURAY, 1080, m::REMUX, 20, ""),
    q(id::HDTV2160P, "HDTV-2160p", s::TV, 2160, m::NONE, 21, ""),
    q(id::WEBDL2160P, "WEBDL-2160p", s::WEBDL, 2160, m::NONE, 22, "WEB 2160p"),
    q(id::WEBRIP2160P, "WEBRip-2160p", s::WEBRIP, 2160, m::NONE, 22, "WEB 2160p"),
    q(id::BLURAY2160P, "Bluray-2160p", s::BLURAY, 2160, m::NONE, 23, ""),
    q(id::REMUX2160P, "Remux-2160p", s::BLURAY, 2160, m::REMUX, 24, ""),
    q(id::BRDISK, "BR-DISK", s::BLURAY, 1080, m::BRDISK, 25, ""),
    q(id::RAWHD, "Raw-HD", s::TV, 1080, m::RAWHD, 26, ""),
];

pub fn get(id: i32) -> Quality {
    ALL.iter().copied().find(|q| q.id == id).unwrap_or(ALL[0])
}

pub fn name(id: i32) -> &'static str {
    get(id).name
}

/// "Bluray-1080p Proper" style label.
pub fn full_name(qm: &crate::model::QualityModel) -> String {
    let mut s = name(qm.quality).to_string();
    if qm.revision.real > 0 {
        s.push_str(" REAL");
    }
    if qm.revision.version > 1 {
        s.push_str(if qm.revision.is_repack { " Repack" } else { " Proper" });
    }
    s
}

pub fn find(source: i32, resolution: i32, modifier: i32) -> Quality {
    ALL.iter()
        .copied()
        .find(|q| q.source == source && q.resolution == resolution && q.modifier == modifier)
        .unwrap_or(ALL[0])
}

pub fn default_sizes() -> Vec<QualitySize> {
    ALL.iter()
        .map(|q| {
            let unlimited = q.weight >= 19;
            QualitySize {
                quality: q.id,
                title: q.name.to_string(),
                min: 0.0,
                preferred: if unlimited { None } else { Some(95.0) },
                max: if unlimited { None } else { Some(100.0) },
            }
        })
        .collect()
}

/// Builds a profile the same way Radarr does: equal weights become a group.
pub fn default_profile(name: &str, cutoff: i32, allowed: &[i32]) -> QualityProfile {
    let mut items: Vec<ProfileItem> = Vec::new();
    let mut group_id = 1000;
    let mut profile_cutoff = cutoff;
    let mut i = 0;
    while i < ALL.len() {
        let w = ALL[i].weight;
        let members: Vec<Quality> = ALL.iter().copied().filter(|q| q.weight == w).collect();
        if members.len() == 1 {
            items.push(ProfileItem { id: members[0].id, name: String::new(), qualities: vec![members[0].id], allowed: allowed.contains(&members[0].id) });
        } else {
            let group_allowed = members.iter().any(|q| allowed.contains(&q.id));
            if members.iter().any(|q| q.id == cutoff) {
                profile_cutoff = group_id;
            }
            items.push(ProfileItem {
                id: group_id,
                name: members[0].group.to_string(),
                qualities: members.iter().map(|q| q.id).collect(),
                allowed: group_allowed,
            });
            group_id += 1;
        }
        i += members.len();
    }
    QualityProfile {
        id: 0,
        name: name.to_string(),
        upgrade_allowed: true,
        cutoff: profile_cutoff,
        items,
        min_format_score: 0,
        cutoff_format_score: 0,
        min_upgrade_format_score: 1,
        format_items: vec![],
        language: "Original".into(),
    }
}

pub fn default_profiles() -> Vec<QualityProfile> {
    use id::*;
    vec![
        default_profile(
            "Any",
            BLURAY480P,
            &[WORKPRINT, CAM, TELESYNC, TELECINE, DVDSCR, REGIONAL, SDTV, DVD, DVDR, HDTV720P, HDTV1080P, HDTV2160P, WEBDL480P, WEBRIP480P, WEBDL720P, WEBRIP720P, WEBDL1080P, WEBRIP1080P, WEBDL2160P, WEBRIP2160P, BLURAY480P, BLURAY576P, BLURAY720P, BLURAY1080P, BLURAY2160P, REMUX1080P, REMUX2160P, BRDISK],
        ),
        default_profile("SD", BLURAY480P, &[WORKPRINT, CAM, TELESYNC, TELECINE, DVDSCR, REGIONAL, SDTV, DVD, WEBDL480P, WEBRIP480P, BLURAY480P, BLURAY576P]),
        default_profile("HD-720p", BLURAY720P, &[HDTV720P, WEBDL720P, WEBRIP720P, BLURAY720P]),
        default_profile("HD-1080p", BLURAY1080P, &[HDTV1080P, WEBDL1080P, WEBRIP1080P, BLURAY1080P, REMUX1080P]),
        default_profile("Ultra-HD", REMUX2160P, &[HDTV2160P, WEBDL2160P, WEBRIP2160P, BLURAY2160P, REMUX2160P]),
        default_profile("HD - 720p/1080p", BLURAY720P, &[HDTV720P, HDTV1080P, WEBDL720P, WEBRIP720P, WEBDL1080P, WEBRIP1080P, BLURAY720P, BLURAY1080P, REMUX1080P]),
    ]
}

/// Quality to assume from a file extension when the name says nothing.
pub fn from_extension(ext: &str) -> i32 {
    match ext.trim_start_matches('.').to_ascii_lowercase().as_str() {
        "webm" => id::UNKNOWN,
        "m4v" | "3gp" | "nsv" | "ty" | "strm" | "rm" | "rmvb" | "m3u" | "ifo" | "mov" | "qt" | "divx" | "xvid" | "bivx" | "nrg" | "pva" | "wmv" | "asf" | "asx" | "ogm" | "ogv" | "m2v" | "avi" | "bin" | "dat" | "dvr-ms" | "mpg" | "mpeg" | "mp4" | "avc" | "vp3" | "svq3" | "nuv" | "viv" | "dv" | "fli" | "flv" | "wpl" | "ts" | "wtv" | "f4v" | "3g2" => id::SDTV,
        "img" | "iso" | "vob" => id::DVD,
        "mkv" | "mk3d" => id::WEBDL720P,
        "m2ts" => id::BLURAY720P,
        _ => id::UNKNOWN,
    }
}

pub const VIDEO_EXTENSIONS: &[&str] = &[
    "webm", "m4v", "3gp", "nsv", "ty", "strm", "rm", "rmvb", "m3u", "ifo", "mov", "qt", "divx", "xvid", "bivx", "nrg", "pva", "wmv", "asf", "asx", "ogm", "ogv", "m2v", "avi", "bin", "dat", "dvr-ms", "mpg", "mpeg", "mp4", "avc", "vp3", "svq3", "nuv", "viv", "dv", "fli", "flv", "wpl", "img", "iso", "vob", "mkv", "mk3d", "ts", "wtv", "m2ts", "f4v", "3g2",
];

pub fn is_video(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| VIDEO_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}
