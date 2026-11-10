//! Download clients. Each one can add a release, list what it has in Turks'
//! category, remove an item and test its connection.

use std::path::{Path, PathBuf};
use std::time::Duration;

use base64::Engine;
use serde_json::{json, Value};

use crate::model::{ClientKind, DownloadClient, Protocol};
use crate::release::Release;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemStatus {
    Queued,
    Downloading,
    Paused,
    Completed,
    Failed,
    Warning,
}

#[derive(Clone, Debug)]
pub struct ClientItem {
    pub id: String,
    pub title: String,
    pub size: u64,
    pub remaining: u64,
    pub eta_secs: Option<i64>,
    pub status: ItemStatus,
    pub message: String,
    pub output_path: String,
    /// Seeding goals are met (or not a torrent), so it can be removed.
    pub can_be_removed: bool,
}

/// What gets sent to the client.
pub enum Payload {
    Magnet(String),
    TorrentFile(Vec<u8>),
    Nzb(Vec<u8>),
}

fn http(timeout: u64) -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(timeout))
        .cookie_store(true)
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(format!("Turks/{}", crate::updater::APP_VERSION))
        .danger_accept_invalid_certs(true)
        .build()
        .unwrap_or_default()
}

fn base_url(c: &DownloadClient) -> String {
    let scheme = if c.use_ssl { "https" } else { "http" };
    let host = c.host.trim().trim_end_matches('/');
    let host = host.strip_prefix("http://").or_else(|| host.strip_prefix("https://")).unwrap_or(host);
    let base = c.url_base.trim().trim_matches('/');
    if base.is_empty() {
        format!("{scheme}://{host}:{}", c.port)
    } else {
        format!("{scheme}://{host}:{}/{base}", c.port)
    }
}

// ---------------------------------------------------------------- fetching the release

/// Downloads the .torrent / .nzb (following redirects that may land on a magnet link).
pub fn fetch_payload(release: &Release) -> Result<Payload, String> {
    if release.protocol == Protocol::Torrent && !release.magnet_url.is_empty() && (release.download_url.is_empty() || release.download_url.starts_with("magnet:")) {
        return Ok(Payload::Magnet(release.magnet_url.clone()));
    }
    let mut url = release.download_url.clone();
    let client = http(60);
    for _ in 0..6 {
        if url.starts_with("magnet:") {
            return Ok(Payload::Magnet(url));
        }
        let resp = client.get(&url).send().map_err(|e| format!("Couldn't download the release from {}: {e}", release.indexer))?;
        if resp.status().is_redirection() {
            url = resp.headers().get("location").and_then(|l| l.to_str().ok()).unwrap_or("").to_string();
            continue;
        }
        if !resp.status().is_success() {
            if !release.magnet_url.is_empty() {
                return Ok(Payload::Magnet(release.magnet_url.clone()));
            }
            return Err(format!("{} returned HTTP {} for the download", release.indexer, resp.status().as_u16()));
        }
        let bytes = resp.bytes().map_err(|e| e.to_string())?.to_vec();
        return match release.protocol {
            Protocol::Torrent => {
                if bytes.first() == Some(&b'd') {
                    Ok(Payload::TorrentFile(bytes))
                } else if let Ok(t) = std::str::from_utf8(&bytes) {
                    if t.trim_start().starts_with("magnet:") {
                        Ok(Payload::Magnet(t.trim().to_string()))
                    } else {
                        Err("The indexer didn't send a valid torrent file.".into())
                    }
                } else {
                    Err("The indexer didn't send a valid torrent file.".into())
                }
            }
            Protocol::Usenet => {
                if String::from_utf8_lossy(&bytes[..bytes.len().min(2048)]).to_lowercase().contains("<nzb") {
                    Ok(Payload::Nzb(bytes))
                } else {
                    Err("The indexer didn't send a valid NZB file.".into())
                }
            }
        };
    }
    Err("Too many redirects while downloading the release".into())
}

/// SHA-1 of the bencoded "info" dictionary.
pub fn torrent_info_hash(data: &[u8]) -> Option<String> {
    fn skip(d: &[u8], mut i: usize) -> Option<usize> {
        match *d.get(i)? {
            b'i' => {
                while *d.get(i)? != b'e' {
                    i += 1;
                }
                Some(i + 1)
            }
            b'l' | b'd' => {
                i += 1;
                while *d.get(i)? != b'e' {
                    i = skip(d, i)?;
                }
                Some(i + 1)
            }
            b'0'..=b'9' => {
                let colon = i + d[i..].iter().position(|c| *c == b':')?;
                let len: usize = std::str::from_utf8(&d[i..colon]).ok()?.parse().ok()?;
                Some(colon + 1 + len)
            }
            _ => None,
        }
    }
    if data.first() != Some(&b'd') {
        return None;
    }
    let mut i = 1;
    while *data.get(i)? != b'e' {
        let key_start = i;
        let key_end = skip(data, i)?;
        let colon = key_start + data[key_start..].iter().position(|c| *c == b':')?;
        let key = &data[colon + 1..key_end];
        let val_end = skip(data, key_end)?;
        if key == b"info" {
            use sha1::Digest;
            let h = sha1::Sha1::digest(&data[key_end..val_end]);
            return Some(h.iter().map(|b| format!("{b:02x}")).collect());
        }
        i = val_end;
    }
    None
}

pub fn payload_hash(p: &Payload) -> Option<String> {
    match p {
        Payload::Magnet(m) => crate::indexers::magnet_hash(m),
        Payload::TorrentFile(b) => torrent_info_hash(b),
        Payload::Nzb(_) => None,
    }
}

// ---------------------------------------------------------------- dispatch

pub fn add(c: &DownloadClient, release: &Release, payload: Payload) -> Result<String, String> {
    match c.kind {
        ClientKind::Transmission => transmission::add(c, &payload),
        ClientKind::QBittorrent => qbit::add(c, &payload),
        ClientKind::Deluge => deluge::add(c, &payload),
        ClientKind::Sabnzbd => sab::add(c, release, &payload),
        ClientKind::Nzbget => nzbget::add(c, release, &payload),
        ClientKind::TorrentBlackhole | ClientKind::UsenetBlackhole => blackhole::add(c, release, &payload),
        ClientKind::DownloadStation => dstation::add(c, release, &payload),
    }
}

pub fn items(c: &DownloadClient) -> Result<Vec<ClientItem>, String> {
    match c.kind {
        ClientKind::Transmission => transmission::items(c),
        ClientKind::QBittorrent => qbit::items(c),
        ClientKind::Deluge => deluge::items(c),
        ClientKind::Sabnzbd => sab::items(c),
        ClientKind::Nzbget => nzbget::items(c),
        ClientKind::TorrentBlackhole | ClientKind::UsenetBlackhole => blackhole::items(c),
        ClientKind::DownloadStation => dstation::items(c),
    }
}

pub fn remove(c: &DownloadClient, id: &str, delete_data: bool) -> Result<(), String> {
    match c.kind {
        ClientKind::Transmission => transmission::remove(c, id, delete_data),
        ClientKind::QBittorrent => qbit::remove(c, id, delete_data),
        ClientKind::Deluge => deluge::remove(c, id, delete_data),
        ClientKind::Sabnzbd => sab::remove(c, id, delete_data),
        ClientKind::Nzbget => nzbget::remove(c, id, delete_data),
        ClientKind::TorrentBlackhole | ClientKind::UsenetBlackhole => blackhole::remove(c, id, delete_data),
        ClientKind::DownloadStation => dstation::remove(c, id, delete_data),
    }
}

pub fn test(c: &DownloadClient) -> Result<String, String> {
    match c.kind {
        ClientKind::Transmission => transmission::test(c),
        ClientKind::QBittorrent => qbit::test(c),
        ClientKind::Deluge => deluge::test(c),
        ClientKind::Sabnzbd => sab::test(c),
        ClientKind::Nzbget => nzbget::test(c),
        ClientKind::TorrentBlackhole | ClientKind::UsenetBlackhole => blackhole::test(c),
        ClientKind::DownloadStation => dstation::test(c),
    }
}

// ---------------------------------------------------------------- Transmission (and Lifestream)

mod transmission {
    use super::*;

    fn rpc(c: &DownloadClient, method: &str, args: Value) -> Result<Value, String> {
        let url = format!("{}/transmission/rpc", base_url(c).trim_end_matches("/transmission/rpc").trim_end_matches("/transmission"));
        let client = http(30);
        let body = json!({"method": method, "arguments": args});
        let mut session = String::new();
        for _ in 0..2 {
            let mut req = client.post(&url).json(&body).header("X-Transmission-Session-Id", &session);
            if !c.username.is_empty() {
                req = req.basic_auth(&c.username, Some(&c.password));
            }
            let resp = req.send().map_err(|e| format!("Couldn't connect to {}: {e}", c.name))?;
            match resp.status().as_u16() {
                409 => {
                    session = resp.headers().get("X-Transmission-Session-Id").and_then(|h| h.to_str().ok()).unwrap_or("").to_string();
                    continue;
                }
                401 => return Err(format!("{}: wrong username or password", c.name)),
                s if !(200..300).contains(&s) => return Err(format!("{}: HTTP {s}", c.name)),
                _ => {}
            }
            let v: Value = resp.json().map_err(|e| e.to_string())?;
            if v.get("result").and_then(|r| r.as_str()) != Some("success") {
                return Err(format!("{}: {}", c.name, v.get("result").and_then(|r| r.as_str()).unwrap_or("error")));
            }
            return Ok(v.get("arguments").cloned().unwrap_or(Value::Null));
        }
        Err(format!("{}: couldn't get a session id", c.name))
    }

    pub fn add(c: &DownloadClient, p: &Payload) -> Result<String, String> {
        let mut args = json!({"paused": c.add_paused, "labels": [c.category]});
        if !c.directory.trim().is_empty() {
            args["download-dir"] = json!(c.directory.trim());
        }
        match p {
            Payload::Magnet(m) => args["filename"] = json!(m),
            Payload::TorrentFile(b) => args["metainfo"] = json!(base64::engine::general_purpose::STANDARD.encode(b)),
            Payload::Nzb(_) => return Err("Transmission can't download usenet releases".into()),
        }
        let r = rpc(c, "torrent-add", args)?;
        let t = r.get("torrent-added").or_else(|| r.get("torrent-duplicate")).ok_or("Transmission didn't add the torrent")?;
        let hash = t.get("hashString").and_then(|h| h.as_str()).map(|s| s.to_lowercase()).or_else(|| payload_hash(p)).ok_or("No hash returned")?;
        // Older Transmission ignores labels on add.
        let _ = rpc(c, "torrent-set", json!({"ids": [hash], "labels": [c.category]}));
        Ok(hash)
    }

    pub fn items(c: &DownloadClient) -> Result<Vec<ClientItem>, String> {
        let r = rpc(
            c,
            "torrent-get",
            json!({"fields": ["hashString","name","downloadDir","totalSize","leftUntilDone","eta","status","errorString","error","labels","isFinished","percentDone","uploadRatio","seedRatioLimit","seedRatioMode","secondsSeeding"]}),
        )?;
        let mut out = Vec::new();
        for t in r.get("torrents").and_then(|t| t.as_array()).into_iter().flatten() {
            let labels: Vec<String> = t.get("labels").and_then(|l| l.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
            let dir = t.get("downloadDir").and_then(|d| d.as_str()).unwrap_or("");
            let in_cat = c.category.is_empty()
                || labels.iter().any(|l| l.eq_ignore_ascii_case(&c.category))
                || Path::new(dir).file_name().map(|n| n.to_string_lossy().eq_ignore_ascii_case(&c.category)).unwrap_or(false);
            if !in_cat {
                continue;
            }
            let name = t.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string();
            let left = t.get("leftUntilDone").and_then(|x| x.as_u64()).unwrap_or(0);
            let status = t.get("status").and_then(|x| x.as_i64()).unwrap_or(0);
            let error = t.get("error").and_then(|x| x.as_i64()).unwrap_or(0);
            let finished = t.get("isFinished").and_then(|x| x.as_bool()).unwrap_or(false);
            let st = if error > 0 {
                ItemStatus::Warning
            } else if left == 0 {
                ItemStatus::Completed
            } else if status == 0 {
                ItemStatus::Paused
            } else if status == 4 {
                ItemStatus::Downloading
            } else {
                ItemStatus::Queued
            };
            out.push(ClientItem {
                id: t.get("hashString").and_then(|h| h.as_str()).unwrap_or("").to_lowercase(),
                output_path: Path::new(dir).join(&name).display().to_string(),
                title: name,
                size: t.get("totalSize").and_then(|x| x.as_u64()).unwrap_or(0),
                remaining: left,
                eta_secs: t.get("eta").and_then(|x| x.as_i64()).filter(|e| *e >= 0),
                status: st,
                message: t.get("errorString").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                can_be_removed: finished || (left == 0 && status == 0),
            });
        }
        Ok(out)
    }

    pub fn remove(c: &DownloadClient, id: &str, delete: bool) -> Result<(), String> {
        rpc(c, "torrent-remove", json!({"ids": [id], "delete-local-data": delete})).map(|_| ())
    }

    pub fn test(c: &DownloadClient) -> Result<String, String> {
        let r = rpc(c, "session-get", json!({"fields": ["version", "download-dir"]}))?;
        Ok(format!("Connected to {}.", r.get("version").and_then(|v| v.as_str()).map(|v| format!("Transmission {v}")).unwrap_or_else(|| "the client".into())))
    }
}

// ---------------------------------------------------------------- qBittorrent

mod qbit {
    use super::*;

    fn session(c: &DownloadClient) -> Result<(reqwest::blocking::Client, String), String> {
        let client = http(30);
        let base = base_url(c);
        let r = client
            .post(format!("{base}/api/v2/auth/login"))
            .header("Referer", &base)
            .form(&[("username", c.username.as_str()), ("password", c.password.as_str())])
            .send()
            .map_err(|e| format!("Couldn't connect to {}: {e}", c.name))?;
        let status = r.status().as_u16();
        let text = r.text().unwrap_or_default();
        if status == 403 {
            return Err(format!("{}: too many failed logins, qBittorrent has banned this address for a while", c.name));
        }
        if !(200..300).contains(&status) || text.trim() == "Fails." {
            return Err(format!("{}: wrong username or password", c.name));
        }
        Ok((client, base))
    }

    pub fn add(c: &DownloadClient, p: &Payload) -> Result<String, String> {
        let (client, base) = session(c)?;
        let _ = client.post(format!("{base}/api/v2/torrents/createCategory")).form(&[("category", c.category.as_str()), ("savePath", "")]).send();
        let mut form = reqwest::blocking::multipart::Form::new().text("category", c.category.clone());
        if c.add_paused {
            form = form.text("paused", "true").text("stopped", "true");
        }
        if !c.directory.trim().is_empty() {
            form = form.text("savepath", c.directory.trim().to_string());
        }
        form = match p {
            Payload::Magnet(m) => form.text("urls", m.clone()),
            Payload::TorrentFile(b) => form.part("torrents", reqwest::blocking::multipart::Part::bytes(b.clone()).file_name("release.torrent").mime_str("application/x-bittorrent").unwrap()),
            Payload::Nzb(_) => return Err("qBittorrent can't download usenet releases".into()),
        };
        let r = client.post(format!("{base}/api/v2/torrents/add")).multipart(form).send().map_err(|e| e.to_string())?;
        if !r.status().is_success() {
            return Err(format!("{}: add failed (HTTP {})", c.name, r.status().as_u16()));
        }
        let text = r.text().unwrap_or_default();
        if text.trim() == "Fails." {
            return Err(format!("{}: qBittorrent refused the torrent", c.name));
        }
        payload_hash(p).ok_or_else(|| "Couldn't work out the torrent's hash".into())
    }

    pub fn items(c: &DownloadClient) -> Result<Vec<ClientItem>, String> {
        let (client, base) = session(c)?;
        let v: Value = client
            .get(format!("{base}/api/v2/torrents/info"))
            .query(&[("category", c.category.as_str())])
            .send()
            .map_err(|e| e.to_string())?
            .json()
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for t in v.as_array().into_iter().flatten() {
            let state = t.get("state").and_then(|s| s.as_str()).unwrap_or("");
            let left = t.get("amount_left").and_then(|x| x.as_u64()).unwrap_or(0);
            let (st, msg) = match state {
                "error" => (ItemStatus::Warning, "qBittorrent reports an error"),
                "missingFiles" => (ItemStatus::Warning, "The download is missing files"),
                "pausedDL" | "stoppedDL" => (ItemStatus::Paused, ""),
                "queuedDL" | "checkingDL" | "checkingResumeData" | "allocating" | "moving" => (ItemStatus::Queued, ""),
                "metaDL" | "forcedMetaDL" => (ItemStatus::Queued, "Downloading metadata"),
                "stalledDL" => (ItemStatus::Warning, "The download is stalled with no connections"),
                "downloading" | "forcedDL" => (ItemStatus::Downloading, ""),
                _ if left == 0 => (ItemStatus::Completed, ""),
                _ => (ItemStatus::Downloading, ""),
            };
            let seeding_done = matches!(state, "pausedUP" | "stoppedUP");
            let content = t.get("content_path").and_then(|s| s.as_str()).map(String::from).unwrap_or_else(|| {
                Path::new(t.get("save_path").and_then(|s| s.as_str()).unwrap_or("")).join(t.get("name").and_then(|s| s.as_str()).unwrap_or("")).display().to_string()
            });
            out.push(ClientItem {
                id: t.get("hash").and_then(|h| h.as_str()).unwrap_or("").to_lowercase(),
                title: t.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string(),
                size: t.get("size").and_then(|x| x.as_u64()).unwrap_or(0),
                remaining: left,
                eta_secs: t.get("eta").and_then(|x| x.as_i64()).filter(|e| *e >= 0 && *e < 8640000),
                status: st,
                message: msg.into(),
                output_path: content,
                can_be_removed: seeding_done,
            });
        }
        Ok(out)
    }

    pub fn remove(c: &DownloadClient, id: &str, delete: bool) -> Result<(), String> {
        let (client, base) = session(c)?;
        client
            .post(format!("{base}/api/v2/torrents/delete"))
            .form(&[("hashes", id), ("deleteFiles", if delete { "true" } else { "false" })])
            .send()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    pub fn test(c: &DownloadClient) -> Result<String, String> {
        let (client, base) = session(c)?;
        let v = client.get(format!("{base}/api/v2/app/version")).send().map_err(|e| e.to_string())?.text().unwrap_or_default();
        Ok(format!("Connected to qBittorrent {}.", v.trim()))
    }
}

// ---------------------------------------------------------------- Deluge

mod deluge {
    use super::*;

    struct Session {
        client: reqwest::blocking::Client,
        url: String,
    }

    impl Session {
        fn call(&self, method: &str, params: Value) -> Result<Value, String> {
            let v: Value = self
                .client
                .post(&self.url)
                .json(&json!({"method": method, "params": params, "id": 1}))
                .send()
                .map_err(|e| e.to_string())?
                .json()
                .map_err(|e| e.to_string())?;
            if let Some(err) = v.get("error").filter(|e| !e.is_null()) {
                return Err(err.get("message").and_then(|m| m.as_str()).unwrap_or("Deluge error").to_string());
            }
            Ok(v.get("result").cloned().unwrap_or(Value::Null))
        }
    }

    fn login(c: &DownloadClient) -> Result<Session, String> {
        let s = Session { client: http(30), url: format!("{}/json", base_url(c)) };
        let ok = s.call("auth.login", json!([c.password])).map_err(|e| format!("Couldn't connect to {}: {e}", c.name))?;
        if ok != Value::Bool(true) {
            return Err(format!("{}: wrong password", c.name));
        }
        if s.call("web.connected", json!([])).ok() != Some(Value::Bool(true)) {
            if let Ok(Value::Array(hosts)) = s.call("web.get_hosts", json!([])) {
                if let Some(id) = hosts.first().and_then(|h| h.get(0)).and_then(|i| i.as_str()) {
                    let _ = s.call("web.connect", json!([id]));
                }
            }
        }
        Ok(s)
    }

    pub fn add(c: &DownloadClient, p: &Payload) -> Result<String, String> {
        let s = login(c)?;
        let mut opts = json!({"add_paused": c.add_paused});
        if !c.directory.trim().is_empty() {
            opts["download_location"] = json!(c.directory.trim());
        }
        let r = match p {
            Payload::Magnet(m) => s.call("core.add_torrent_magnet", json!([m, opts]))?,
            Payload::TorrentFile(b) => s.call("core.add_torrent_file", json!(["release.torrent", base64::engine::general_purpose::STANDARD.encode(b), opts]))?,
            Payload::Nzb(_) => return Err("Deluge can't download usenet releases".into()),
        };
        let hash = r.as_str().map(|h| h.to_lowercase()).or_else(|| payload_hash(p)).ok_or("Deluge didn't add the torrent (already added?)")?;
        if !c.category.is_empty() {
            let _ = s.call("label.add", json!([c.category.to_lowercase()]));
            let _ = s.call("label.set_torrent", json!([hash, c.category.to_lowercase()]));
        }
        Ok(hash)
    }

    pub fn items(c: &DownloadClient) -> Result<Vec<ClientItem>, String> {
        let s = login(c)?;
        let filter = if c.category.is_empty() { json!({}) } else { json!({"label": c.category.to_lowercase()}) };
        let r = s.call(
            "core.get_torrents_status",
            json!([filter, ["name", "hash", "total_size", "total_done", "eta", "state", "save_path", "download_location", "message", "is_finished", "is_auto_managed", "stop_at_ratio", "ratio", "stop_ratio"]]),
        )?;
        let mut out = Vec::new();
        for (hash, t) in r.as_object().into_iter().flatten() {
            let size = t.get("total_size").and_then(|x| x.as_u64()).unwrap_or(0);
            let done = t.get("total_done").and_then(|x| x.as_u64()).unwrap_or(0);
            let state = t.get("state").and_then(|x| x.as_str()).unwrap_or("");
            let st = match state {
                "Error" => ItemStatus::Warning,
                "Paused" if done >= size && size > 0 => ItemStatus::Completed,
                "Paused" => ItemStatus::Paused,
                "Queued" | "Checking" | "Moving" | "Allocating" => ItemStatus::Queued,
                "Seeding" => ItemStatus::Completed,
                _ => ItemStatus::Downloading,
            };
            let dir = t.get("download_location").or_else(|| t.get("save_path")).and_then(|x| x.as_str()).unwrap_or("");
            let name = t.get("name").and_then(|x| x.as_str()).unwrap_or("").to_string();
            out.push(ClientItem {
                id: hash.to_lowercase(),
                output_path: Path::new(dir).join(&name).display().to_string(),
                title: name,
                size,
                remaining: size.saturating_sub(done),
                eta_secs: t.get("eta").and_then(|x| x.as_f64()).map(|e| e as i64).filter(|e| *e > 0),
                status: st,
                message: t.get("message").and_then(|x| x.as_str()).filter(|m| *m != "OK").unwrap_or("").to_string(),
                can_be_removed: state == "Paused" && done >= size && t.get("is_finished").and_then(|x| x.as_bool()).unwrap_or(false),
            });
        }
        Ok(out)
    }

    pub fn remove(c: &DownloadClient, id: &str, delete: bool) -> Result<(), String> {
        login(c)?.call("core.remove_torrent", json!([id, delete])).map(|_| ())
    }

    pub fn test(c: &DownloadClient) -> Result<String, String> {
        let s = login(c)?;
        let v = s.call("daemon.info", json!([])).unwrap_or(Value::Null);
        Ok(format!("Connected to Deluge {}.", v.as_str().unwrap_or("")))
    }
}

// ---------------------------------------------------------------- SABnzbd

mod sab {
    use super::*;

    fn api(c: &DownloadClient, params: &[(&str, &str)]) -> Result<Value, String> {
        let mut q: Vec<(&str, &str)> = params.to_vec();
        q.push(("apikey", c.api_key.as_str()));
        q.push(("output", "json"));
        let r = http(60).get(format!("{}/api", base_url(c))).query(&q).send().map_err(|e| format!("Couldn't connect to {}: {e}", c.name))?;
        let v: Value = r.json().map_err(|e| e.to_string())?;
        if let Some(err) = v.get("error").and_then(|e| e.as_str()) {
            return Err(format!("{}: {err}", c.name));
        }
        Ok(v)
    }

    pub fn add(c: &DownloadClient, release: &Release, p: &Payload) -> Result<String, String> {
        let Payload::Nzb(bytes) = p else { return Err("SABnzbd only takes usenet releases".into()) };
        let name = format!("{}.nzb", release.title);
        let form = reqwest::blocking::multipart::Form::new().part("name", reqwest::blocking::multipart::Part::bytes(bytes.clone()).file_name(name).mime_str("application/x-nzb").unwrap());
        let v: Value = http(60)
            .post(format!("{}/api", base_url(c)))
            .query(&[("mode", "addfile"), ("cat", c.category.as_str()), ("apikey", c.api_key.as_str()), ("output", "json"), ("priority", if c.add_paused { "-2" } else { "-100" })])
            .multipart(form)
            .send()
            .map_err(|e| e.to_string())?
            .json()
            .map_err(|e| e.to_string())?;
        v.get("nzo_ids").and_then(|a| a.as_array()).and_then(|a| a.first()).and_then(|i| i.as_str()).map(String::from).ok_or_else(|| format!("{}: SABnzbd didn't accept the NZB", c.name))
    }

    pub fn items(c: &DownloadClient) -> Result<Vec<ClientItem>, String> {
        let mut out = Vec::new();
        let q = api(c, &[("mode", "queue"), ("limit", "500")])?;
        let paused_all = q.pointer("/queue/paused").and_then(|p| p.as_bool()).unwrap_or(false);
        for s in q.pointer("/queue/slots").and_then(|x| x.as_array()).into_iter().flatten() {
            if !c.category.is_empty() && s.get("cat").and_then(|x| x.as_str()).map(|x| !x.eq_ignore_ascii_case(&c.category)).unwrap_or(false) {
                continue;
            }
            let mb = |k: &str| s.get(k).and_then(|x| x.as_str()).and_then(|x| x.parse::<f64>().ok()).unwrap_or(0.0);
            let status = s.get("status").and_then(|x| x.as_str()).unwrap_or("");
            let eta = s.get("timeleft").and_then(|x| x.as_str()).map(|t| {
                t.split(':').filter_map(|p| p.parse::<i64>().ok()).fold(0, |acc, p| acc * 60 + p)
            });
            out.push(ClientItem {
                id: s.get("nzo_id").and_then(|x| x.as_str()).unwrap_or("").into(),
                title: s.get("filename").and_then(|x| x.as_str()).unwrap_or("").into(),
                size: (mb("mb") * 1_048_576.0) as u64,
                remaining: (mb("mbleft") * 1_048_576.0) as u64,
                eta_secs: eta,
                status: if paused_all || status == "Paused" { ItemStatus::Paused } else if status == "Downloading" { ItemStatus::Downloading } else { ItemStatus::Queued },
                message: String::new(),
                output_path: String::new(),
                can_be_removed: false,
            });
        }
        let h = api(c, &[("mode", "history"), ("limit", "200"), ("category", c.category.as_str())])?;
        for s in h.pointer("/history/slots").and_then(|x| x.as_array()).into_iter().flatten() {
            let status = s.get("status").and_then(|x| x.as_str()).unwrap_or("");
            let st = match status {
                "Completed" => ItemStatus::Completed,
                "Failed" => ItemStatus::Failed,
                _ => ItemStatus::Queued,
            };
            let size = s.get("bytes").and_then(|x| x.as_u64()).unwrap_or(0);
            out.push(ClientItem {
                id: s.get("nzo_id").and_then(|x| x.as_str()).unwrap_or("").into(),
                title: s.get("name").and_then(|x| x.as_str()).unwrap_or("").into(),
                size,
                remaining: 0,
                eta_secs: None,
                status: st,
                message: s.get("fail_message").and_then(|x| x.as_str()).unwrap_or("").into(),
                output_path: s.get("storage").and_then(|x| x.as_str()).unwrap_or("").into(),
                can_be_removed: true,
            });
        }
        Ok(out)
    }

    pub fn remove(c: &DownloadClient, id: &str, delete: bool) -> Result<(), String> {
        let del = if delete { "1" } else { "0" };
        let _ = api(c, &[("mode", "queue"), ("name", "delete"), ("value", id), ("del_files", del)]);
        api(c, &[("mode", "history"), ("name", "delete"), ("value", id), ("del_files", del)]).map(|_| ())
    }

    pub fn test(c: &DownloadClient) -> Result<String, String> {
        let v = api(c, &[("mode", "version")])?;
        // Version needs no key; check the key with a queue call.
        api(c, &[("mode", "queue"), ("limit", "1")])?;
        Ok(format!("Connected to SABnzbd {}.", v.get("version").and_then(|x| x.as_str()).unwrap_or("")))
    }
}

// ---------------------------------------------------------------- NZBGet

mod nzbget {
    use super::*;

    fn rpc(c: &DownloadClient, method: &str, params: Value) -> Result<Value, String> {
        let mut req = http(60).post(format!("{}/jsonrpc", base_url(c))).json(&json!({"method": method, "params": params, "id": 1}));
        if !c.username.is_empty() {
            req = req.basic_auth(&c.username, Some(&c.password));
        }
        let resp = req.send().map_err(|e| format!("Couldn't connect to {}: {e}", c.name))?;
        if resp.status().as_u16() == 401 {
            return Err(format!("{}: wrong username or password", c.name));
        }
        let v: Value = resp.json().map_err(|e| e.to_string())?;
        if let Some(e) = v.get("error").filter(|e| !e.is_null()) {
            return Err(format!("{}: {}", c.name, e.get("message").and_then(|m| m.as_str()).unwrap_or("error")));
        }
        Ok(v.get("result").cloned().unwrap_or(Value::Null))
    }

    pub fn add(c: &DownloadClient, release: &Release, p: &Payload) -> Result<String, String> {
        let Payload::Nzb(bytes) = p else { return Err("NZBGet only takes usenet releases".into()) };
        let r = rpc(
            c,
            "append",
            json!([format!("{}.nzb", release.title), base64::engine::general_purpose::STANDARD.encode(bytes), c.category, 0, false, c.add_paused, "", 0, "SCORE", []]),
        )?;
        match r.as_i64() {
            Some(id) if id > 0 => Ok(id.to_string()),
            _ => Err(format!("{}: NZBGet didn't accept the NZB", c.name)),
        }
    }

    pub fn items(c: &DownloadClient) -> Result<Vec<ClientItem>, String> {
        let mut out = Vec::new();
        let in_cat = |v: &Value| c.category.is_empty() || v.get("Category").and_then(|x| x.as_str()).map(|x| x.eq_ignore_ascii_case(&c.category)).unwrap_or(false);
        let mb = |v: &Value, k: &str| v.get(k).and_then(|x| x.as_f64()).unwrap_or(0.0);
        for g in rpc(c, "listgroups", json!([0]))?.as_array().into_iter().flatten().filter(|g| in_cat(g)) {
            let status = g.get("Status").and_then(|x| x.as_str()).unwrap_or("");
            out.push(ClientItem {
                id: g.get("NZBID").and_then(|x| x.as_i64()).unwrap_or(0).to_string(),
                title: g.get("NZBName").and_then(|x| x.as_str()).unwrap_or("").into(),
                size: (mb(g, "FileSizeMB") * 1_048_576.0) as u64,
                remaining: (mb(g, "RemainingSizeMB") * 1_048_576.0) as u64,
                eta_secs: None,
                status: if status.starts_with("PAUSED") { ItemStatus::Paused } else if status == "DOWNLOADING" { ItemStatus::Downloading } else { ItemStatus::Queued },
                message: String::new(),
                output_path: String::new(),
                can_be_removed: false,
            });
        }
        for h in rpc(c, "history", json!([false]))?.as_array().into_iter().flatten().filter(|g| in_cat(g)) {
            let status = h.get("Status").and_then(|x| x.as_str()).unwrap_or("");
            let st = if status.starts_with("SUCCESS") {
                ItemStatus::Completed
            } else if status.starts_with("FAILURE") || status.starts_with("DELETED") {
                ItemStatus::Failed
            } else if status.starts_with("WARNING") {
                ItemStatus::Warning
            } else {
                ItemStatus::Queued
            };
            let dir = h.get("FinalDir").and_then(|x| x.as_str()).filter(|s| !s.is_empty()).or_else(|| h.get("DestDir").and_then(|x| x.as_str())).unwrap_or("");
            out.push(ClientItem {
                id: h.get("NZBID").and_then(|x| x.as_i64()).unwrap_or(0).to_string(),
                title: h.get("Name").and_then(|x| x.as_str()).unwrap_or("").into(),
                size: (mb(h, "FileSizeMB") * 1_048_576.0) as u64,
                remaining: 0,
                eta_secs: None,
                status: st,
                message: if st == ItemStatus::Completed { String::new() } else { status.to_string() },
                output_path: dir.into(),
                can_be_removed: true,
            });
        }
        Ok(out)
    }

    pub fn remove(c: &DownloadClient, id: &str, delete: bool) -> Result<(), String> {
        let id: i64 = id.parse().unwrap_or(0);
        let _ = rpc(c, "editqueue", json!([if delete { "GroupFinalDelete" } else { "GroupDelete" }, "", [id]]));
        rpc(c, "editqueue", json!([if delete { "HistoryFinalDelete" } else { "HistoryDelete" }, "", [id]])).map(|_| ())
    }

    pub fn test(c: &DownloadClient) -> Result<String, String> {
        let v = rpc(c, "version", json!([]))?;
        Ok(format!("Connected to NZBGet {}.", v.as_str().unwrap_or("")))
    }
}

// ---------------------------------------------------------------- Synology Download Station

mod dstation {
    use super::*;

    struct Session {
        client: reqwest::blocking::Client,
        base: String,
        sid: String,
    }

    fn login(c: &DownloadClient) -> Result<Session, String> {
        let client = http(60);
        let base = base_url(c);
        let v: Value = client
            .get(format!("{base}/webapi/auth.cgi"))
            .query(&[("api", "SYNO.API.Auth"), ("version", "6"), ("method", "login"), ("account", c.username.as_str()), ("passwd", c.password.as_str()), ("session", "DownloadStation"), ("format", "sid")])
            .send()
            .map_err(|e| format!("Couldn't connect to {}: {e}", c.name))?
            .json()
            .map_err(|e| e.to_string())?;
        if v.get("success").and_then(|s| s.as_bool()) != Some(true) {
            let code = v.pointer("/error/code").and_then(|c| c.as_i64()).unwrap_or(0);
            return Err(match code {
                400 => format!("{}: wrong username or password", c.name),
                403 => format!("{}: this account uses 2-step verification; use an account without it", c.name),
                _ => format!("{}: sign-in failed (code {code})", c.name),
            });
        }
        let sid = v.pointer("/data/sid").and_then(|s| s.as_str()).unwrap_or("").to_string();
        Ok(Session { client, base, sid })
    }

    fn list(s: &Session) -> Result<Vec<Value>, String> {
        let v: Value = s
            .client
            .get(format!("{}/webapi/DownloadStation/task.cgi", s.base))
            .query(&[("api", "SYNO.DownloadStation.Task"), ("version", "1"), ("method", "list"), ("additional", "detail,transfer"), ("_sid", s.sid.as_str())])
            .send()
            .map_err(|e| e.to_string())?
            .json()
            .map_err(|e| e.to_string())?;
        Ok(v.pointer("/data/tasks").and_then(|t| t.as_array()).cloned().unwrap_or_default())
    }

    pub fn add(c: &DownloadClient, release: &Release, p: &Payload) -> Result<String, String> {
        let s = login(c)?;
        let before: std::collections::HashSet<String> = list(&s)?.iter().filter_map(|t| t.get("id").and_then(|i| i.as_str()).map(String::from)).collect();
        let url = format!("{}/webapi/DownloadStation/task.cgi", s.base);
        let dest = c.directory.trim().trim_start_matches('/').to_string();
        let resp: Value = match p {
            Payload::Magnet(m) => {
                let mut form = vec![("api", "SYNO.DownloadStation.Task".to_string()), ("version", "1".into()), ("method", "create".into()), ("uri", m.clone()), ("_sid", s.sid.clone())];
                if !dest.is_empty() {
                    form.push(("destination", dest.clone()));
                }
                s.client.post(&url).form(&form).send().map_err(|e| e.to_string())?.json().map_err(|e| e.to_string())?
            }
            Payload::TorrentFile(b) | Payload::Nzb(b) => {
                let ext = if matches!(p, Payload::Nzb(_)) { "nzb" } else { "torrent" };
                let mut form = reqwest::blocking::multipart::Form::new()
                    .text("api", "SYNO.DownloadStation.Task")
                    .text("version", "1")
                    .text("method", "create")
                    .text("_sid", s.sid.clone());
                if !dest.is_empty() {
                    form = form.text("destination", dest.clone());
                }
                form = form.part("file", reqwest::blocking::multipart::Part::bytes(b.clone()).file_name(format!("{}.{ext}", release.title)));
                s.client.post(&url).multipart(form).send().map_err(|e| e.to_string())?.json().map_err(|e| e.to_string())?
            }
        };
        if resp.get("success").and_then(|x| x.as_bool()) != Some(true) {
            return Err(format!("{}: Download Station refused the task (code {})", c.name, resp.pointer("/error/code").and_then(|x| x.as_i64()).unwrap_or(0)));
        }
        // v1 'create' doesn't return the id: find the new task.
        for _ in 0..10 {
            std::thread::sleep(Duration::from_millis(700));
            if let Some(t) = list(&s)?.into_iter().find(|t| t.get("id").and_then(|i| i.as_str()).map(|i| !before.contains(i)).unwrap_or(false)) {
                return Ok(t.get("id").and_then(|i| i.as_str()).unwrap_or("").to_string());
            }
        }
        Err(format!("{}: the task was added but Turks couldn't find it", c.name))
    }

    pub fn items(c: &DownloadClient) -> Result<Vec<ClientItem>, String> {
        let s = login(c)?;
        let dest = c.directory.trim().trim_start_matches('/').trim_end_matches('/').to_lowercase();
        let mut out = Vec::new();
        for t in list(&s)? {
            let destination = t.pointer("/additional/detail/destination").and_then(|d| d.as_str()).unwrap_or("").to_string();
            if !dest.is_empty() && !destination.to_lowercase().trim_end_matches('/').eq(&dest) {
                continue;
            }
            let size = t.get("size").and_then(|x| x.as_u64()).unwrap_or(0);
            let done = t.pointer("/additional/transfer/size_downloaded").and_then(|x| x.as_u64()).unwrap_or(0);
            let status = t.get("status").and_then(|x| x.as_str()).unwrap_or("");
            let st = match status {
                "finished" | "seeding" => ItemStatus::Completed,
                "error" => ItemStatus::Failed,
                "paused" => ItemStatus::Paused,
                "downloading" => ItemStatus::Downloading,
                _ => ItemStatus::Queued,
            };
            let title = t.get("title").and_then(|x| x.as_str()).unwrap_or("").to_string();
            out.push(ClientItem {
                id: t.get("id").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                output_path: format!("{}/{}", destination.trim_end_matches('/'), title),
                title,
                size,
                remaining: size.saturating_sub(done),
                eta_secs: None,
                status: st,
                message: t.pointer("/status_extra/error_detail").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                can_be_removed: status == "finished",
            });
        }
        Ok(out)
    }

    pub fn remove(c: &DownloadClient, id: &str, _delete: bool) -> Result<(), String> {
        let s = login(c)?;
        s.client
            .get(format!("{}/webapi/DownloadStation/task.cgi", s.base))
            .query(&[("api", "SYNO.DownloadStation.Task"), ("version", "1"), ("method", "delete"), ("id", id), ("force_complete", "false"), ("_sid", s.sid.as_str())])
            .send()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    pub fn test(c: &DownloadClient) -> Result<String, String> {
        let s = login(c)?;
        let n = list(&s)?.len();
        Ok(format!("Connected to Download Station ({n} tasks)."))
    }
}

// ---------------------------------------------------------------- Blackhole folders

mod blackhole {
    use super::*;

    fn safe_name(s: &str) -> String {
        s.chars().map(|c| if r#"\/:*?"<>|"#.contains(c) { '_' } else { c }).collect()
    }

    pub fn add(c: &DownloadClient, release: &Release, p: &Payload) -> Result<String, String> {
        let dir = PathBuf::from(c.drop_folder.trim());
        if c.drop_folder.trim().is_empty() || !dir.is_dir() {
            return Err(format!("{}: the drop folder doesn't exist", c.name));
        }
        let name = safe_name(&release.title);
        match p {
            Payload::TorrentFile(b) => std::fs::write(dir.join(format!("{name}.torrent")), b).map_err(|e| e.to_string())?,
            Payload::Magnet(m) => {
                if !c.save_magnet_files {
                    return Err(format!("{}: this release is a magnet link and saving .magnet files is off", c.name));
                }
                std::fs::write(dir.join(format!("{name}.magnet")), m).map_err(|e| e.to_string())?
            }
            Payload::Nzb(b) => std::fs::write(dir.join(format!("{name}.nzb")), b).map_err(|e| e.to_string())?,
        }
        Ok(format!("blackhole:{}", name.to_lowercase()))
    }

    pub fn items(c: &DownloadClient) -> Result<Vec<ClientItem>, String> {
        let dir = PathBuf::from(c.watch_folder.trim());
        if c.watch_folder.trim().is_empty() {
            return Ok(vec![]);
        }
        let mut out = Vec::new();
        for e in std::fs::read_dir(&dir).map_err(|e| format!("{}: can't read the watch folder ({e})", c.name))?.flatten() {
            let path = e.path();
            let name = path.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            if path.is_file() && !crate::quality::is_video(&path) {
                continue;
            }
            let (size, newest) = folder_size_and_mtime(&path);
            // Treat it as finished once nothing has changed for a minute.
            let settled = newest.elapsed().map(|d| d > Duration::from_secs(60)).unwrap_or(true);
            out.push(ClientItem {
                id: format!("blackhole:{}", name.to_lowercase()),
                title: name,
                size,
                remaining: if settled { 0 } else { 1 },
                eta_secs: None,
                status: if settled { ItemStatus::Completed } else { ItemStatus::Downloading },
                message: String::new(),
                output_path: path.display().to_string(),
                can_be_removed: true,
            });
        }
        Ok(out)
    }

    fn folder_size_and_mtime(p: &Path) -> (u64, std::time::SystemTime) {
        let mut size = 0;
        let mut newest = std::time::UNIX_EPOCH;
        for e in walkdir::WalkDir::new(p).into_iter().flatten() {
            if let Ok(m) = e.metadata() {
                if m.is_file() {
                    size += m.len();
                }
                if let Ok(t) = m.modified() {
                    newest = newest.max(t);
                }
            }
        }
        (size, newest)
    }

    pub fn remove(c: &DownloadClient, id: &str, delete: bool) -> Result<(), String> {
        if !delete {
            return Ok(());
        }
        let name = id.trim_start_matches("blackhole:");
        for e in std::fs::read_dir(c.watch_folder.trim()).map_err(|e| e.to_string())?.flatten() {
            let stem = e.path().file_stem().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
            if stem == name {
                let p = e.path();
                let _ = if p.is_dir() { std::fs::remove_dir_all(&p) } else { std::fs::remove_file(&p) };
            }
        }
        Ok(())
    }

    pub fn test(c: &DownloadClient) -> Result<String, String> {
        for (label, d) in [("drop", &c.drop_folder), ("watch", &c.watch_folder)] {
            if d.trim().is_empty() || !Path::new(d.trim()).is_dir() {
                return Err(format!("The {label} folder doesn't exist."));
            }
        }
        let probe = Path::new(c.drop_folder.trim()).join(".turks-write-test");
        std::fs::write(&probe, b"ok").map_err(|e| format!("Can't write to the drop folder: {e}"))?;
        let _ = std::fs::remove_file(probe);
        Ok("Both folders are reachable.".into())
    }
}

/// Applies remote path mappings to a path the client reported.
pub fn map_remote_path(maps: &[crate::model::RemotePathMapping], client_host: &str, path: &str) -> String {
    for m in maps {
        if !m.host.eq_ignore_ascii_case(client_host) || m.remote.is_empty() {
            continue;
        }
        let norm = |s: &str| s.replace('\\', "/").trim_end_matches('/').to_lowercase();
        if norm(path).starts_with(&norm(&m.remote)) {
            let rest = &path.replace('\\', "/")[m.remote.replace('\\', "/").trim_end_matches('/').len()..];
            let local = Path::new(&m.local).join(rest.trim_start_matches('/'));
            return local.display().to_string();
        }
    }
    path.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info_hash_of_minimal_torrent() {
        // d4:infod4:name1:aee  -> info dict is "d4:name1:ae"
        let t = b"d8:announce3:url4:infod4:name1:aee";
        let h = torrent_info_hash(t).unwrap();
        use sha1::Digest;
        let want: String = sha1::Sha1::digest(b"d4:name1:ae").iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(h, want);
    }

    #[test]
    fn remote_paths() {
        let maps = vec![crate::model::RemotePathMapping { id: 1, host: "nas".into(), remote: "/volume1/downloads/".into(), local: r"\\nas\downloads".into() }];
        let p = map_remote_path(&maps, "NAS", "/volume1/downloads/movies/X");
        assert!(p.contains("nas") && p.ends_with("X"));
    }
}
