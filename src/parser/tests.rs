//! Cases lifted from Radarr's own parser test suite (radarr_cases.json).

use super::*;

#[derive(serde::Deserialize)]
struct Cases {
    quality: Vec<(String, i32, bool)>,
    titles: Vec<(String, String)>,
    years: Vec<(String, i32)>,
    groups: Vec<(String, String)>,
    editions: Vec<(String, String)>,
    no_edition: Vec<String>,
}

fn cases() -> Cases {
    serde_json::from_str(include_str!("radarr_cases.json")).unwrap()
}

fn report(name: &str, failures: &[String], total: usize, min_rate: f64) {
    let rate = 1.0 - failures.len() as f64 / total.max(1) as f64;
    eprintln!("{name}: {}/{} passed ({:.1}%)", total - failures.len(), total, rate * 100.0);
    for f in failures {
        eprintln!("  FAIL {f}");
    }
    assert!(rate >= min_rate, "{name} pass rate {:.1}% below {:.1}%", rate * 100.0, min_rate * 100.0);
}

#[test]
fn radarr_quality_cases() {
    let c = cases();
    let mut fails = vec![];
    for (title, want, proper) in &c.quality {
        let got = quality::parse_quality(title);
        let ok = got.quality == *want && (!*proper || got.revision.version >= 2);
        if !ok {
            fails.push(format!("{title:?}: want {} got {} (v{})", crate::quality::name(*want), crate::quality::name(got.quality), got.revision.version));
        }
    }
    report("quality", &fails, c.quality.len(), 0.985);
}

#[test]
fn radarr_title_cases() {
    let c = cases();
    let mut fails = vec![];
    for (title, want) in &c.titles {
        let got = parse_movie_title(title, false).map(|p| p.primary_title().to_string()).unwrap_or_default();
        if &got != want {
            fails.push(format!("{title:?}: want {want:?} got {got:?}"));
        }
    }
    for (title, want) in &c.years {
        let got = parse_movie_title(title, false).map(|p| p.year).unwrap_or(0);
        if got != *want {
            fails.push(format!("{title:?}: want year {want} got {got}"));
        }
    }
    report("titles", &fails, c.titles.len() + c.years.len(), 0.97);
}

#[test]
fn radarr_release_group_cases() {
    let c = cases();
    let mut fails = vec![];
    for (title, want) in &c.groups {
        let got = parse_release_group(title);
        if !got.eq_ignore_ascii_case(want) {
            fails.push(format!("{title:?}: want {want:?} got {got:?}"));
        }
    }
    report("groups", &fails, c.groups.len(), 0.97);
}

#[test]
fn radarr_edition_cases() {
    let c = cases();
    let mut fails = vec![];
    for (title, want) in &c.editions {
        let got = parse_movie_title(title, false).map(|p| p.edition).unwrap_or_default();
        if !got.eq_ignore_ascii_case(want) {
            fails.push(format!("{title:?}: want {want:?} got {got:?}"));
        }
    }
    for title in &c.no_edition {
        let got = parse_movie_title(title, false).map(|p| p.edition).unwrap_or_default();
        if !got.is_empty() {
            fails.push(format!("{title:?}: want no edition, got {got:?}"));
        }
    }
    report("editions", &fails, c.editions.len() + c.no_edition.len(), 0.97);
}

#[test]
fn common_releases() {
    let p = parse_movie_title("Blade.Runner.2049.2017.2160p.UHD.BluRay.REMUX.HDR.HEVC.Atmos-EPSiLON", false).unwrap();
    assert_eq!(p.primary_title(), "Blade Runner 2049");
    assert_eq!(p.year, 2017);
    assert_eq!(p.quality.quality, crate::quality::id::REMUX2160P);
    assert_eq!(p.release_group, "EPSiLON");
    let p = parse_movie_title("Dune.Part.Two.2024.1080p.AMZN.WEB-DL.DDP5.1.Atmos.H.264-FLUX", false).unwrap();
    assert_eq!(p.primary_title(), "Dune Part Two");
    assert_eq!(p.quality.quality, crate::quality::id::WEBDL1080P);
    assert_eq!(p.release_group, "FLUX");
    let mi = media_info_from_name("Dune.Part.Two.2024.2160p.WEB-DL.DV.HDR10.DDP5.1.Atmos.H.265-FLUX");
    assert_eq!(mi.video_codec, "x265");
    assert_eq!(mi.audio_channels, 5.1);
    assert!(mi.video_dynamic_range.contains("DV"));
    assert_eq!(clean_title("The Lord of the Rings: The Return of the King"), "thelordringsreturnking");
}
