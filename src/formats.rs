//! Custom formats: matching (same rules as Radarr) and Radarr/TRaSH JSON
//! import and export.

use serde_json::{json, Value};

use crate::model::{CfKind, CfSpec, CustomFormat, Id, QualityModel};
use crate::parser::language::LANGUAGES;
use crate::quality;

pub const INDEXER_FLAGS: &[(&str, i64)] = &[
    ("freeleech", 1),
    ("halfleech", 2),
    ("doubleupload", 4),
    ("ptp_golden", 8),
    ("ptp_approved", 16),
    ("internal", 32),
    ("ahd_internal", 64),
    ("scene", 128),
    ("freeleech75", 256),
    ("freeleech25", 512),
    ("ahd_userrelease", 1024),
    ("nuked", 2048),
];

/// What a format is judged against: a release or a file.
#[derive(Clone, Debug, Default)]
pub struct FormatInput {
    pub title: String,
    pub quality: QualityModel,
    pub languages: Vec<String>,
    pub release_group: String,
    pub edition: String,
    pub size: u64,
    pub indexer_flags: Vec<String>,
    pub year: i32,
    pub original_language: String,
}

fn regex_match(pattern: &str, text: &str) -> bool {
    if pattern.trim().is_empty() {
        return false;
    }
    // Radarr patterns are .NET regexes, case-insensitive.
    match fancy_regex::Regex::new(&format!("(?i){pattern}")) {
        Ok(r) => r.is_match(text).unwrap_or(false),
        Err(_) => text.to_lowercase().contains(&pattern.to_lowercase()),
    }
}

fn spec_matches(kind: &CfKind, i: &FormatInput) -> bool {
    let q = quality::get(i.quality.quality);
    match kind {
        CfKind::ReleaseTitle { pattern } => regex_match(pattern, &i.title),
        CfKind::Source { source } => q.source == *source,
        CfKind::Resolution { resolution } => q.resolution == *resolution,
        CfKind::QualityModifier { modifier } => q.modifier == *modifier,
        CfKind::Size { min_gb, max_gb } => {
            let gb = i.size as f64 / 1_073_741_824.0;
            i.size > 0 && gb > *min_gb && gb <= *max_gb
        }
        CfKind::Language { language, except } => {
            let langs: Vec<String> = if i.languages.is_empty() {
                vec![i.original_language.clone()]
            } else {
                i.languages.iter().map(|l| if l == "Original" { i.original_language.clone() } else { l.clone() }).collect()
            };
            let wanted = if language == "Original" { i.original_language.as_str() } else { language.as_str() };
            if *except {
                langs.iter().any(|l| !l.eq_ignore_ascii_case(wanted))
            } else {
                langs.iter().any(|l| l.eq_ignore_ascii_case(wanted))
            }
        }
        CfKind::ReleaseGroup { pattern } => regex_match(pattern, &i.release_group),
        CfKind::Edition { pattern } => regex_match(pattern, &i.edition),
        CfKind::IndexerFlag { flag } => i.indexer_flags.iter().any(|f| f.eq_ignore_ascii_case(flag)),
        CfKind::Year { min, max } => i.year >= *min && i.year <= *max,
    }
}

/// Radarr's rule: specs are grouped by type. In each group every required
/// spec must match and at least one spec must match.
pub fn format_matches(cf: &CustomFormat, input: &FormatInput) -> bool {
    if cf.specs.is_empty() {
        return false;
    }
    let mut groups: Vec<(&'static str, Vec<(bool, bool)>)> = Vec::new();
    for s in &cf.specs {
        let hit = spec_matches(&s.kind, input) != s.negate;
        let label = s.kind.label();
        match groups.iter_mut().find(|(l, _)| *l == label) {
            Some((_, v)) => v.push((s.required, hit)),
            None => groups.push((label, vec![(s.required, hit)])),
        }
    }
    groups.iter().all(|(_, v)| !(v.iter().any(|(req, hit)| *req && !hit) || v.iter().all(|(_, hit)| !hit)))
}

pub fn matching<'a>(formats: impl Iterator<Item = &'a CustomFormat>, input: &FormatInput) -> Vec<(Id, String)> {
    formats.filter(|f| format_matches(f, input)).map(|f| (f.id, f.name.clone())).collect()
}

// ---------------------------------------------------------------- Radarr JSON

fn field<'a>(spec: &'a Value, name: &str) -> Option<&'a Value> {
    match spec.get("fields") {
        Some(Value::Array(a)) => a.iter().find(|f| f.get("name").and_then(|n| n.as_str()) == Some(name)).and_then(|f| f.get("value")),
        Some(Value::Object(o)) => o.get(name),
        _ => None,
    }
}

fn num(v: Option<&Value>) -> i64 {
    v.and_then(|v| v.as_i64().or_else(|| v.as_f64().map(|f| f as i64)).or_else(|| v.as_str().and_then(|s| s.parse().ok()))).unwrap_or(0)
}

fn fnum(v: Option<&Value>) -> f64 {
    v.and_then(|v| v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))).unwrap_or(0.0)
}

fn text(v: Option<&Value>) -> String {
    v.and_then(|v| v.as_str()).unwrap_or("").to_string()
}

pub fn language_name(id: i64) -> String {
    match id {
        -2 => "Original".into(),
        -1 => "Any".into(),
        n if n >= 1 && (n as usize) <= LANGUAGES.len() => LANGUAGES[n as usize - 1].into(),
        _ => "Unknown".into(),
    }
}

pub fn language_id(name: &str) -> i64 {
    match name {
        "Original" => -2,
        "Any" => -1,
        _ => LANGUAGES.iter().position(|l| l.eq_ignore_ascii_case(name)).map(|i| i as i64 + 1).unwrap_or(0),
    }
}

fn flag_name(bit: i64) -> String {
    INDEXER_FLAGS.iter().find(|(_, b)| *b == bit).map(|(n, _)| n.to_string()).unwrap_or_else(|| bit.to_string())
}

fn flag_bit(name: &str) -> i64 {
    INDEXER_FLAGS.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, b)| *b).unwrap_or(0)
}

/// Accepts one format object or an array of them (Radarr export, TRaSH guides).
pub fn import_json(text_in: &str) -> Result<Vec<CustomFormat>, String> {
    let v: Value = serde_json::from_str(text_in).map_err(|e| format!("That isn't valid JSON: {e}"))?;
    let list = match v {
        Value::Array(a) => a,
        o @ Value::Object(_) => vec![o],
        _ => return Err("Expected a custom format object or a list of them.".into()),
    };
    let mut out = Vec::new();
    for f in list {
        let name = f.get("name").and_then(|n| n.as_str()).ok_or("A custom format is missing its name.")?.to_string();
        let mut cf = CustomFormat {
            id: 0,
            name,
            include_when_renaming: f.get("includeCustomFormatWhenRenaming").and_then(|b| b.as_bool()).unwrap_or(false),
            specs: vec![],
        };
        for s in f.get("specifications").and_then(|s| s.as_array()).into_iter().flatten() {
            let imp = s.get("implementation").and_then(|i| i.as_str()).unwrap_or("");
            let value = field(s, "value");
            let kind = match imp {
                "ReleaseTitleSpecification" => CfKind::ReleaseTitle { pattern: text(value) },
                "SourceSpecification" => CfKind::Source { source: num(value) as i32 },
                "ResolutionSpecification" => CfKind::Resolution { resolution: num(value) as i32 },
                "QualityModifierSpecification" => CfKind::QualityModifier { modifier: num(value) as i32 },
                "SizeSpecification" => CfKind::Size { min_gb: fnum(field(s, "min")), max_gb: fnum(field(s, "max")) },
                "LanguageSpecification" => CfKind::Language {
                    language: language_name(num(value)),
                    except: field(s, "exceptLanguage").and_then(|b| b.as_bool()).unwrap_or(false),
                },
                "ReleaseGroupSpecification" => CfKind::ReleaseGroup { pattern: text(value) },
                "EditionSpecification" => CfKind::Edition { pattern: text(value) },
                "IndexerFlagSpecification" => CfKind::IndexerFlag { flag: flag_name(num(value)) },
                "YearSpecification" => CfKind::Year { min: num(field(s, "min")) as i32, max: num(field(s, "max")) as i32 },
                other => {
                    crate::applog::warn(format!("Skipped an unsupported condition type in '{}': {other}", cf.name));
                    continue;
                }
            };
            cf.specs.push(CfSpec {
                name: s.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string(),
                kind,
                negate: s.get("negate").and_then(|b| b.as_bool()).unwrap_or(false),
                required: s.get("required").and_then(|b| b.as_bool()).unwrap_or(false),
            });
        }
        out.push(cf);
    }
    Ok(out)
}

pub fn export_json(cf: &CustomFormat) -> Value {
    let specs: Vec<Value> = cf
        .specs
        .iter()
        .map(|s| {
            let (imp, fields) = match &s.kind {
                CfKind::ReleaseTitle { pattern } => ("ReleaseTitleSpecification", json!({"value": pattern})),
                CfKind::Source { source } => ("SourceSpecification", json!({"value": source})),
                CfKind::Resolution { resolution } => ("ResolutionSpecification", json!({"value": resolution})),
                CfKind::QualityModifier { modifier } => ("QualityModifierSpecification", json!({"value": modifier})),
                CfKind::Size { min_gb, max_gb } => ("SizeSpecification", json!({"min": min_gb, "max": max_gb})),
                CfKind::Language { language, except } => ("LanguageSpecification", json!({"value": language_id(language), "exceptLanguage": except})),
                CfKind::ReleaseGroup { pattern } => ("ReleaseGroupSpecification", json!({"value": pattern})),
                CfKind::Edition { pattern } => ("EditionSpecification", json!({"value": pattern})),
                CfKind::IndexerFlag { flag } => ("IndexerFlagSpecification", json!({"value": flag_bit(flag)})),
                CfKind::Year { min, max } => ("YearSpecification", json!({"min": min, "max": max})),
            };
            json!({"name": s.name, "implementation": imp, "negate": s.negate, "required": s.required, "fields": fields})
        })
        .collect();
    json!({"name": cf.name, "includeCustomFormatWhenRenaming": cf.include_when_renaming, "specifications": specs})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trash_style_import_and_match() {
        let j = r#"{"name":"x265 (HD)","includeCustomFormatWhenRenaming":false,"specifications":[
            {"name":"x265","implementation":"ReleaseTitleSpecification","negate":false,"required":true,"fields":{"value":"[xh][ ._-]?265|\\bHEVC(\\b|\\d)"}},
            {"name":"Not 2160p","implementation":"ResolutionSpecification","negate":true,"required":true,"fields":{"value":2160}}]}"#;
        let cfs = import_json(j).unwrap();
        assert_eq!(cfs[0].specs.len(), 2);
        let mut input = FormatInput { title: "Movie.2020.1080p.BluRay.x265-GRP".into(), ..Default::default() };
        input.quality.quality = crate::quality::id::BLURAY1080P;
        assert!(format_matches(&cfs[0], &input));
        input.quality.quality = crate::quality::id::BLURAY2160P;
        assert!(!format_matches(&cfs[0], &input));
        let back = export_json(&cfs[0]);
        assert_eq!(import_json(&back.to_string()).unwrap()[0].specs, cfs[0].specs.iter().cloned().collect::<Vec<_>>().iter().map(|s| s.clone()).collect::<Vec<_>>());
    }
}
