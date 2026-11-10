//! Port of Radarr's LanguageParser (the parts that matter for movies).

use fancy_regex::Regex;
use once_cell::sync::Lazy;

pub const LANGUAGES: &[&str] = &[
    "English", "French", "Spanish", "German", "Italian", "Danish", "Dutch", "Japanese", "Icelandic", "Chinese",
    "Russian", "Polish", "Vietnamese", "Swedish", "Norwegian", "Finnish", "Turkish", "Portuguese", "Flemish",
    "Greek", "Korean", "Hungarian", "Hebrew", "Lithuanian", "Czech", "Hindi", "Romanian", "Thai", "Bulgarian",
    "Portuguese (Brazil)", "Arabic", "Ukrainian", "Persian", "Bengali", "Slovak", "Latvian", "Spanish (Latino)",
    "Catalan", "Croatian", "Serbian", "Bosnian", "Estonian", "Tamil", "Indonesian", "Telugu", "Macedonian",
    "Slovenian", "Malayalam", "Kannada", "Albanian", "Afrikaans", "Marathi", "Tagalog", "Urdu", "Romansh",
    "Mongolian", "Georgian", "Irish",
];

/// TMDb's ISO 639-1 code to Turks' language name.
pub fn from_iso(code: &str) -> Option<&'static str> {
    Some(match code.to_ascii_lowercase().as_str() {
        "en" => "English",
        "fr" => "French",
        "es" => "Spanish",
        "de" => "German",
        "it" => "Italian",
        "da" => "Danish",
        "nl" => "Dutch",
        "ja" => "Japanese",
        "is" => "Icelandic",
        "zh" | "cn" => "Chinese",
        "ru" => "Russian",
        "pl" => "Polish",
        "vi" => "Vietnamese",
        "sv" => "Swedish",
        "no" | "nb" | "nn" => "Norwegian",
        "fi" => "Finnish",
        "tr" => "Turkish",
        "pt" => "Portuguese",
        "el" => "Greek",
        "ko" => "Korean",
        "hu" => "Hungarian",
        "he" => "Hebrew",
        "lt" => "Lithuanian",
        "cs" => "Czech",
        "hi" => "Hindi",
        "ro" => "Romanian",
        "th" => "Thai",
        "bg" => "Bulgarian",
        "ar" => "Arabic",
        "uk" => "Ukrainian",
        "fa" => "Persian",
        "bn" => "Bengali",
        "sk" => "Slovak",
        "lv" => "Latvian",
        "ca" => "Catalan",
        "hr" => "Croatian",
        "sr" => "Serbian",
        "bs" => "Bosnian",
        "et" => "Estonian",
        "ta" => "Tamil",
        "id" => "Indonesian",
        "te" => "Telugu",
        "mk" => "Macedonian",
        "sl" => "Slovenian",
        "ml" => "Malayalam",
        "kn" => "Kannada",
        "sq" => "Albanian",
        "af" => "Afrikaans",
        "mr" => "Marathi",
        "tl" => "Tagalog",
        "ur" => "Urdu",
        "rm" => "Romansh",
        "mn" => "Mongolian",
        "ka" => "Georgian",
        "ga" => "Irish",
        _ => return None,
    })
}

static LANGUAGE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(concat!(
        r"(?i)(?:\W|_|^)(?<english>\beng\b)|",
        r"(?<italian>\b(?:ita|italian)\b)|",
        r"(?<german>(?:swiss)?german\b|videomann|ger[. ]dub|\bger\b)|",
        r"(?<flemish>flemish)|",
        r"(?<bulgarian>bgaudio)|",
        r"(?<romanian>rodubbed)|",
        r"(?<brazilian>\b(dublado|pt-BR)\b)|",
        r"(?<greek>greek)|",
        r"(?<french>\b(?:FR|VO|VF|VFF|VFQ|VFI|VF2|TRUEFRENCH|FRENCH|FRE|FRA)\b)|",
        r"(?<russian>\b(?:rus|ru)\b)|",
        r"(?<hungarian>\b(?:HUNDUB|HUN)\b)|",
        r"(?<hebrew>\b(?:HebDub|HebDubbed)\b)|",
        r"(?<polish>\b(?:PL\W?DUB|DUB\W?PL|LEK\W?PL|PL\W?LEK)\b)|",
        r"(?<chinese>\[(?:CH[ST]|BIG5|GB)\]|简|繁|字幕)|",
        r"(?<ukrainian>(?:(?:\dx)?UKR))|",
        r"(?<spanish>\b(?:español|castellano)\b)|",
        r"(?<catalan>\b(?:catalan?|catalán|català)\b)|",
        r"(?<latvian>\b(?:lat|lav|lv)\b)|",
        r"(?<telugu>\btel\b)|",
        r"(?<vietnamese>\bVIE\b)|",
        r"(?<japanese>\bJAP\b)|",
        r"(?<korean>\bKOR\b)|",
        r"(?<urdu>\burdu\b)|",
        r"(?<romansh>\b(?:romansh|rumantsch|romansch)\b)|",
        r"(?<mongolian>\b(?:mongolian|khalkha)\b)|",
        r"(?<georgian>\b(?:georgian|geo|ka|kat)\b)|",
        r"(?<irish>\b(?:irish|gaeilge)\b)|",
        r"(?<original>\b(?:orig|original)\b)"
    ))
    .unwrap()
});

static CASE_SENSITIVE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?<![Ss][Uu][Bb][\W_])(?:(?<english>\bEN\b)|(?<lithuanian>\bLT\b)|(?<czech>\bCZ\b)|(?<polish>\bPL\b)|(?<bulgarian>\bBG\b)|(?<slovak>\bSK\b)|(?<german>\bDE\b)|(?<spanish>\b(?<!DTS[._ -])ES\b))(?![\W_][Ss][Uu][Bb])").unwrap()
});

static GERMAN_DL: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)(?<!WEB[-_. ])(?<!WEB)\bDL\b").unwrap());
static GERMAN_ML: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\bML\b").unwrap());
static MULTI: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)[_. ](?<multi>multi)[_. ]").unwrap());

const WORDS: &[(&str, &str)] = &[
    ("english", "English"),
    ("spanish", "Spanish"),
    ("danish", "Danish"),
    ("dutch", "Dutch"),
    ("japanese", "Japanese"),
    ("icelandic", "Icelandic"),
    ("mandarin", "Chinese"),
    ("cantonese", "Chinese"),
    ("chinese", "Chinese"),
    ("korean", "Korean"),
    ("russian", "Russian"),
    ("romanian", "Romanian"),
    ("hindi", "Hindi"),
    ("arabic", "Arabic"),
    ("thai", "Thai"),
    ("bulgarian", "Bulgarian"),
    ("polish", "Polish"),
    ("vietnamese", "Vietnamese"),
    ("swedish", "Swedish"),
    ("norwegian", "Norwegian"),
    ("finnish", "Finnish"),
    ("turkish", "Turkish"),
    ("portuguese", "Portuguese"),
    ("brazilian", "Portuguese (Brazil)"),
    ("hungarian", "Hungarian"),
    ("hebrew", "Hebrew"),
    ("ukrainian", "Ukrainian"),
    ("persian", "Persian"),
    ("bengali", "Bengali"),
    ("slovak", "Slovak"),
    ("latvian", "Latvian"),
    ("latino", "Spanish (Latino)"),
    ("tamil", "Tamil"),
    ("telugu", "Telugu"),
    ("malayalam", "Malayalam"),
    ("kannada", "Kannada"),
    ("albanian", "Albanian"),
    ("afrikaans", "Afrikaans"),
    ("marathi", "Marathi"),
    ("tagalog", "Tagalog"),
];

const GROUPS: &[(&str, &str)] = &[
    ("english", "English"),
    ("italian", "Italian"),
    ("german", "German"),
    ("flemish", "Flemish"),
    ("greek", "Greek"),
    ("french", "French"),
    ("russian", "Russian"),
    ("bulgarian", "Bulgarian"),
    ("brazilian", "Portuguese (Brazil)"),
    ("hungarian", "Hungarian"),
    ("hebrew", "Hebrew"),
    ("polish", "Polish"),
    ("chinese", "Chinese"),
    ("spanish", "Spanish"),
    ("catalan", "Catalan"),
    ("ukrainian", "Ukrainian"),
    ("latvian", "Latvian"),
    ("romanian", "Romanian"),
    ("telugu", "Telugu"),
    ("vietnamese", "Vietnamese"),
    ("japanese", "Japanese"),
    ("korean", "Korean"),
    ("urdu", "Urdu"),
    ("romansh", "Romansh"),
    ("mongolian", "Mongolian"),
    ("georgian", "Georgian"),
    ("irish", "Irish"),
    ("original", "Original"),
    ("lithuanian", "Lithuanian"),
    ("czech", "Czech"),
    ("slovak", "Slovak"),
];

/// Languages named in a release title. Empty means "not stated" (Radarr's
/// Unknown), which the decision engine treats as the movie's own language.
pub fn parse_languages(title: &str) -> Vec<String> {
    let lower = title.to_lowercase();
    let mut out: Vec<String> = Vec::new();
    let add = |l: &str, out: &mut Vec<String>| {
        if !out.iter().any(|x| x == l) {
            out.push(l.to_string());
        }
    };
    for (word, lang) in WORDS {
        if lower.contains(word) {
            add(lang, &mut out);
        }
    }
    for r in [&*CASE_SENSITIVE, &*LANGUAGE] {
        for c in r.captures_iter(title).flatten() {
            for (g, lang) in GROUPS {
                if c.name(g).is_some() {
                    add(lang, &mut out);
                }
            }
        }
    }
    if out.iter().any(|l| l == "German") {
        if GERMAN_DL.is_match(title).unwrap_or(false) {
            add("Original", &mut out);
        }
        if GERMAN_ML.is_match(title).unwrap_or(false) {
            add("Original", &mut out);
            add("English", &mut out);
        }
    }
    out
}

pub fn has_multiple_languages(title: &str) -> bool {
    MULTI.is_match(title).unwrap_or(false)
}
