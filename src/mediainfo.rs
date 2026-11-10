//! Real media info from ffprobe when it's installed, else from the file name.

use std::path::Path;
use std::process::Command;

use serde_json::Value;

use crate::model::MediaInfo;

pub fn ffprobe_path(configured: &str) -> Option<String> {
    if !configured.trim().is_empty() && Path::new(configured.trim()).exists() {
        return Some(configured.trim().to_string());
    }
    let name = if cfg!(windows) { "ffprobe.exe" } else { "ffprobe" };
    if let Some(paths) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&paths) {
            let p = dir.join(name);
            if p.exists() {
                return Some(p.display().to_string());
            }
        }
    }
    // Next to turks.exe.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(p) = exe.parent().map(|d| d.join(name)).filter(|p| p.exists()) {
            return Some(p.display().to_string());
        }
    }
    None
}

fn lang(code: &str) -> String {
    let c = code.to_lowercase();
    let short = match c.as_str() {
        "eng" => "EN",
        "ger" | "deu" => "DE",
        "fre" | "fra" => "FR",
        "spa" => "ES",
        "ita" => "IT",
        "jpn" => "JA",
        "kor" => "KO",
        "chi" | "zho" => "ZH",
        "rus" => "RU",
        "por" => "PT",
        "dut" | "nld" => "NL",
        "swe" => "SV",
        "nor" => "NO",
        "dan" => "DA",
        "fin" => "FI",
        "pol" => "PL",
        "tur" => "TR",
        "hin" => "HI",
        "ara" => "AR",
        "heb" => "HE",
        "und" | "" => return String::new(),
        other => return other.to_uppercase(),
    };
    short.to_string()
}

pub fn probe(file: &Path, ffprobe: &str) -> Option<MediaInfo> {
    let mut cmd = Command::new(ffprobe);
    cmd.args(["-v", "quiet", "-print_format", "json", "-show_streams", "-show_format"]).arg(file);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let out = cmd.output().ok()?;
    if !out.status.success() {
        return None;
    }
    let v: Value = serde_json::from_slice(&out.stdout).ok()?;
    let streams = v.get("streams")?.as_array()?;
    let mut mi = MediaInfo { from_ffprobe: true, ..Default::default() };
    let s = |x: &Value, k: &str| x.get(k).and_then(|y| y.as_str()).unwrap_or("").to_string();
    if let Some(video) = streams.iter().find(|x| s(x, "codec_type") == "video" && x.pointer("/disposition/attached_pic").and_then(|a| a.as_i64()) != Some(1)) {
        let codec = s(video, "codec_name");
        let encoder = video.pointer("/tags/ENCODER").or_else(|| video.pointer("/tags/encoder")).and_then(|e| e.as_str()).unwrap_or("").to_lowercase();
        let lib = v.pointer("/format/tags/encoder").and_then(|e| e.as_str()).unwrap_or("").to_lowercase();
        mi.video_codec = match codec.as_str() {
            "h264" => if encoder.contains("x264") || lib.contains("x264") { "x264" } else { "h264" },
            "hevc" => if encoder.contains("x265") || lib.contains("x265") { "x265" } else { "h265" },
            "av1" => "AV1",
            "vc1" => "VC1",
            "mpeg2video" => "MPEG2",
            "mpeg4" => if s(video, "codec_tag_string").eq_ignore_ascii_case("xvid") { "XviD" } else { "DivX" },
            "vp9" => "VP9",
            other => other,
        }
        .to_string();
        let pix = s(video, "pix_fmt");
        mi.video_bit_depth = if pix.contains("12") { 12 } else if pix.contains("10") { 10 } else { 8 };
        let transfer = s(video, "color_transfer");
        let mut hdr = Vec::new();
        let side = video.get("side_data_list").and_then(|x| x.as_array()).cloned().unwrap_or_default();
        if side.iter().any(|d| s(d, "side_data_type").to_lowercase().contains("dovi")) {
            hdr.push("DV");
        }
        if transfer == "smpte2084" {
            if side.iter().any(|d| s(d, "side_data_type").contains("HDR10+") || s(d, "side_data_type").contains("HDR Dynamic Metadata")) {
                hdr.push("HDR10Plus");
            } else {
                hdr.push("HDR10");
            }
        } else if transfer == "arib-std-b67" {
            hdr.push("HLG");
        }
        mi.video_dynamic_range = hdr.join(" ");
        let h = video.get("height").and_then(|x| x.as_i64()).unwrap_or(0);
        let w = video.get("width").and_then(|x| x.as_i64()).unwrap_or(0);
        mi.resolution = if w >= 3200 || h >= 2000 {
            "2160p"
        } else if w >= 1800 || h >= 1000 {
            "1080p"
        } else if w >= 1200 || h >= 700 {
            "720p"
        } else if h > 0 {
            "480p"
        } else {
            ""
        }
        .to_string();
    }
    let audio: Vec<&Value> = streams.iter().filter(|x| s(x, "codec_type") == "audio").collect();
    if let Some(a) = audio.first() {
        let profile = s(a, "profile");
        let title = a.pointer("/tags/title").and_then(|t| t.as_str()).unwrap_or("").to_lowercase();
        mi.audio_codec = match s(a, "codec_name").as_str() {
            "truehd" if title.contains("atmos") || profile.to_lowercase().contains("atmos") => "TrueHD Atmos",
            "truehd" => "TrueHD",
            "eac3" if title.contains("atmos") || profile.contains("JOC") => "EAC3 Atmos",
            "eac3" => "EAC3",
            "ac3" => "AC3",
            "dts" if profile.contains("DTS:X") => "DTS-X",
            "dts" if profile.contains("MA") => "DTS-HD MA",
            "dts" if profile.contains("HRA") || profile.contains("HD") => "DTS-HD HRA",
            "dts" => "DTS",
            "aac" => "AAC",
            "flac" => "FLAC",
            "opus" => "Opus",
            "mp3" => "MP3",
            c if c.starts_with("pcm") => "PCM",
            other => other,
        }
        .to_string();
        let ch = a.get("channels").and_then(|c| c.as_i64()).unwrap_or(0) as f32;
        let layout = s(a, "channel_layout");
        mi.audio_channels = if layout.contains(".1") || ch >= 6.0 { (ch - 1.0) + 0.1 } else { ch };
    }
    for a in &audio {
        let l = lang(a.pointer("/tags/language").and_then(|t| t.as_str()).unwrap_or(""));
        if !l.is_empty() && !mi.audio_languages.contains(&l) {
            mi.audio_languages.push(l);
        }
    }
    for st in streams.iter().filter(|x| s(x, "codec_type") == "subtitle") {
        let l = lang(st.pointer("/tags/language").and_then(|t| t.as_str()).unwrap_or(""));
        if !l.is_empty() && !mi.subtitles.contains(&l) {
            mi.subtitles.push(l);
        }
    }
    mi.runtime_secs = v.pointer("/format/duration").and_then(|d| d.as_str()).and_then(|d| d.parse::<f64>().ok()).map(|d| d as i64).unwrap_or(0);
    Some(mi)
}

/// ffprobe if available, else the name-based guess.
pub fn analyse(file: &Path, scene_name: &str, ffprobe: Option<&str>) -> MediaInfo {
    if let Some(fp) = ffprobe {
        if let Some(mi) = probe(file, fp) {
            return mi;
        }
    }
    let name = if scene_name.is_empty() { file.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default() } else { scene_name.to_string() };
    crate::parser::media_info_from_name(&name)
}
