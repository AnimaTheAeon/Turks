//! Connections (Radarr's "Connect"): messages, webhooks, scripts and media
//! server library refreshes.

use serde_json::{json, Value};

use crate::core::{Core, HealthIssue};
use crate::model::*;

#[derive(Clone, Debug)]
pub enum Event {
    Grab { movie: Movie, title: String, quality: String, indexer: String, client: String, size: u64, score: i32 },
    Download { movie: Movie, file: MovieFile, upgrade: bool, source_title: String },
    Rename { movie: Movie },
    MovieAdded { movie: Movie },
    MovieDelete { movie: Movie, files_deleted: bool },
    FileDelete { movie: Movie, file: MovieFile, reason: String },
    Health { issue: HealthIssue },
    HealthRestored { issue: HealthIssue },
    ManualInteraction { title: String, message: String, movie: Option<Movie> },
    AppUpdate { previous: String, new: String },
    Test,
}

impl Event {
    fn kind(&self) -> &'static str {
        match self {
            Self::Grab { .. } => "Grab",
            Self::Download { .. } => "Download",
            Self::Rename { .. } => "Rename",
            Self::MovieAdded { .. } => "MovieAdded",
            Self::MovieDelete { .. } => "MovieDelete",
            Self::FileDelete { .. } => "MovieFileDelete",
            Self::Health { .. } => "Health",
            Self::HealthRestored { .. } => "HealthRestored",
            Self::ManualInteraction { .. } => "ManualInteractionRequired",
            Self::AppUpdate { .. } => "ApplicationUpdate",
            Self::Test => "Test",
        }
    }

    fn movie(&self) -> Option<&Movie> {
        match self {
            Self::Grab { movie, .. } | Self::Download { movie, .. } | Self::Rename { movie } | Self::MovieAdded { movie } | Self::MovieDelete { movie, .. } | Self::FileDelete { movie, .. } => Some(movie),
            Self::ManualInteraction { movie, .. } => movie.as_ref(),
            _ => None,
        }
    }

    fn wanted_by(&self, n: &Notification) -> bool {
        match self {
            Self::Grab { .. } => n.on_grab,
            Self::Download { upgrade: false, .. } => n.on_download,
            Self::Download { upgrade: true, .. } => n.on_upgrade,
            Self::Rename { .. } => n.on_rename,
            Self::MovieAdded { .. } => n.on_movie_added,
            Self::MovieDelete { .. } => n.on_movie_delete,
            Self::FileDelete { .. } => n.on_movie_file_delete,
            Self::Health { issue } => n.on_health_issue && (issue.level == crate::core::HealthLevel::Error || n.include_health_warnings),
            Self::HealthRestored { .. } => n.on_health_restored,
            Self::ManualInteraction { .. } => n.on_manual_interaction,
            Self::AppUpdate { .. } => n.on_app_update,
            Self::Test => true,
        }
    }

    pub fn title(&self) -> String {
        match self {
            Self::Grab { .. } => "Movie Grabbed".into(),
            Self::Download { upgrade: true, .. } => "Movie Upgraded".into(),
            Self::Download { .. } => "Movie Downloaded".into(),
            Self::Rename { .. } => "Movie Renamed".into(),
            Self::MovieAdded { .. } => "Movie Added".into(),
            Self::MovieDelete { .. } => "Movie Deleted".into(),
            Self::FileDelete { .. } => "Movie File Deleted".into(),
            Self::Health { .. } => "Health Issue".into(),
            Self::HealthRestored { .. } => "Health Issue Resolved".into(),
            Self::ManualInteraction { .. } => "Manual Interaction Required".into(),
            Self::AppUpdate { .. } => "Turks Updated".into(),
            Self::Test => "Test Notification".into(),
        }
    }

    pub fn body(&self) -> String {
        match self {
            Self::Grab { movie, title, quality, indexer, client, size, .. } => {
                format!("{}\n{quality} · {} from {indexer}, sent to {client}\n{title}", movie.display(), crate::decision::human_size(*size))
            }
            Self::Download { movie, file, source_title, .. } => format!("{}\n{}\n{}", movie.display(), crate::quality::full_name(&file.quality), if source_title.is_empty() { &file.relative_path } else { source_title }),
            Self::Rename { movie } => format!("{} was renamed", movie.display()),
            Self::MovieAdded { movie } => format!("{} was added to your library", movie.display()),
            Self::MovieDelete { movie, files_deleted } => format!("{} was removed{}", movie.display(), if *files_deleted { " along with its files" } else { "" }),
            Self::FileDelete { movie, file, reason } => format!("{}\n{} was deleted ({reason})", movie.display(), file.relative_path),
            Self::Health { issue } | Self::HealthRestored { issue } => format!("{}: {}", issue.source, issue.message),
            Self::ManualInteraction { title, message, .. } => format!("{title}\n{message}"),
            Self::AppUpdate { previous, new } => format!("Turks was updated from {previous} to {new}"),
            Self::Test => "This is a test message from Turks.".into(),
        }
    }

    fn payload(&self) -> Value {
        let mut v = json!({"eventType": self.kind(), "instanceName": "Turks", "applicationUrl": ""});
        if let Some(m) = self.movie() {
            v["movie"] = json!({
                "id": m.id, "title": m.title, "year": m.year, "releaseDate": m.digital_release.or(m.physical_release).map(|d| d.to_string()),
                "folderPath": m.path, "tmdbId": m.tmdb_id, "imdbId": m.imdb_id, "overview": m.overview, "genres": m.genres,
                "tags": m.tags, "originalLanguage": {"name": m.original_language}
            });
            v["remoteMovie"] = json!({"tmdbId": m.tmdb_id, "imdbId": m.imdb_id, "title": m.title, "year": m.year});
        }
        match self {
            Self::Grab { title, quality, indexer, client, size, score, .. } => {
                v["release"] = json!({"releaseTitle": title, "quality": quality, "indexer": indexer, "size": size, "customFormatScore": score});
                v["downloadClient"] = json!(client);
            }
            Self::Download { file, upgrade, .. } => {
                v["movieFile"] = json!({"id": file.id, "relativePath": file.relative_path, "path": file.path, "quality": crate::quality::name(file.quality.quality), "releaseGroup": file.release_group, "sceneName": file.scene_name, "size": file.size});
                v["isUpgrade"] = json!(upgrade);
            }
            Self::FileDelete { file, reason, .. } => {
                v["movieFile"] = json!({"id": file.id, "relativePath": file.relative_path, "path": file.path});
                v["deleteReason"] = json!(reason);
            }
            Self::MovieDelete { files_deleted, .. } => v["deletedFiles"] = json!(files_deleted),
            Self::Health { issue } | Self::HealthRestored { issue } => {
                v["level"] = json!(format!("{:?}", issue.level).to_lowercase());
                v["message"] = json!(issue.message);
                v["type"] = json!(issue.source);
            }
            Self::ManualInteraction { title, message, .. } => {
                v["downloadInfo"] = json!({"title": title});
                v["message"] = json!(message);
            }
            Self::AppUpdate { previous, new } => {
                v["previousVersion"] = json!(previous);
                v["newVersion"] = json!(new);
            }
            _ => {}
        }
        v
    }
}

/// Sends an event to every connection that wants it (in the background).
pub fn dispatch(core: &Core, ev: Event) {
    let targets: Vec<Notification> = {
        let lib = core.lib();
        lib.notifications
            .values()
            .filter(|n| ev.wanted_by(n))
            .filter(|n| n.tags.is_empty() || ev.movie().map(|m| n.tags.iter().any(|t| m.tags.contains(t))).unwrap_or(true))
            .cloned()
            .collect()
    };
    if targets.is_empty() {
        return;
    }
    std::thread::spawn(move || {
        for n in targets {
            if let Err(e) = send(&n, &ev) {
                crate::applog::warn(format!("Notification '{}' failed: {e}", n.name));
            }
        }
    });
}

fn http() -> reqwest::blocking::Client {
    crate::tmdb::http_client(20)
}

fn check(r: reqwest::Result<reqwest::blocking::Response>) -> Result<(), String> {
    let r = r.map_err(|e| e.to_string())?;
    if r.status().is_success() {
        Ok(())
    } else {
        let code = r.status().as_u16();
        let body = r.text().unwrap_or_default();
        Err(format!("HTTP {code} {}", body.chars().take(200).collect::<String>()))
    }
}

pub fn send(n: &Notification, ev: &Event) -> Result<(), String> {
    let title = ev.title();
    let body = ev.body();
    match &n.kind {
        NotifyKind::Discord { webhook, username } => {
            let color = match ev {
                Event::Grab { .. } => 0xffe260,
                Event::Download { .. } => 0x78e68c,
                Event::Health { .. } | Event::ManualInteraction { .. } => 0xff6e6e,
                _ => 0x70ccff,
            };
            let mut embed = json!({"title": title, "description": body, "color": color});
            if let Some(m) = ev.movie() {
                if !m.poster_path.is_empty() {
                    embed["thumbnail"] = json!({"url": format!("{}/w185{}", crate::tmdb::IMG, m.poster_path)});
                }
                embed["url"] = json!(format!("https://www.themoviedb.org/movie/{}", m.tmdb_id));
            }
            check(http().post(webhook).json(&json!({"username": if username.is_empty() { "Turks" } else { username }, "embeds": [embed]})).send())
        }
        NotifyKind::Telegram { bot_token, chat_id, silent } => check(
            http()
                .post(format!("https://api.telegram.org/bot{bot_token}/sendMessage"))
                .json(&json!({"chat_id": chat_id, "text": format!("{title}\n{body}"), "disable_notification": silent}))
                .send(),
        ),
        NotifyKind::Pushover { user_key, api_token, devices, priority } => check(
            http()
                .post("https://api.pushover.net/1/messages.json")
                .form(&[("token", api_token.as_str()), ("user", user_key.as_str()), ("title", title.as_str()), ("message", body.as_str()), ("device", devices.as_str()), ("priority", &priority.to_string())])
                .send(),
        ),
        NotifyKind::Ntfy { server, topic, token, priority } => {
            let mut req = http().post(format!("{}/{}", server.trim_end_matches('/'), topic)).header("Title", title.as_str()).header("Priority", priority.to_string()).body(body);
            if !token.is_empty() {
                req = req.bearer_auth(token);
            }
            check(req.send())
        }
        NotifyKind::Gotify { server, app_token, priority } => check(
            http()
                .post(format!("{}/message", server.trim_end_matches('/')))
                .header("X-Gotify-Key", app_token.as_str())
                .json(&json!({"title": title, "message": body, "priority": priority}))
                .send(),
        ),
        NotifyKind::Webhook { url, method_put, username, password } => {
            let mut req = if *method_put { http().put(url) } else { http().post(url) };
            if !username.is_empty() {
                req = req.basic_auth(username, Some(password));
            }
            check(req.json(&ev.payload()).send())
        }
        NotifyKind::Email { server, port, require_tls, username, password, from, to } => send_email(server, *port, *require_tls, username, password, from, to, &title, &body),
        NotifyKind::Plex { host, port, token, use_ssl } => {
            if !matches!(ev, Event::Download { .. } | Event::Rename { .. } | Event::MovieDelete { .. } | Event::FileDelete { .. } | Event::Test) {
                return Ok(());
            }
            let base = format!("{}://{host}:{port}", if *use_ssl { "https" } else { "http" });
            let sections: Value = http().get(format!("{base}/library/sections")).query(&[("X-Plex-Token", token)]).header("Accept", "application/json").send().map_err(|e| e.to_string())?.json().map_err(|e| e.to_string())?;
            let path = ev.movie().map(|m| m.path.clone()).unwrap_or_default();
            for s in sections.pointer("/MediaContainer/Directory").and_then(|d| d.as_array()).into_iter().flatten() {
                if s.get("type").and_then(|t| t.as_str()) != Some("movie") {
                    continue;
                }
                let key = s.get("key").and_then(|k| k.as_str()).unwrap_or("");
                let mut q = vec![("X-Plex-Token", token.clone())];
                if !path.is_empty() {
                    q.push(("path", path.clone()));
                }
                check(http().get(format!("{base}/library/sections/{key}/refresh")).query(&q).send())?;
            }
            Ok(())
        }
        NotifyKind::Emby { host, port, api_key, use_ssl, .. } => {
            let base = format!("{}://{host}:{port}", if *use_ssl { "https" } else { "http" });
            if matches!(ev, Event::Test) {
                return check(http().get(format!("{base}/System/Info")).header("X-MediaBrowser-Token", api_key.as_str()).send());
            }
            let Some(m) = ev.movie() else { return Ok(()) };
            if !matches!(ev, Event::Download { .. } | Event::Rename { .. } | Event::MovieDelete { .. } | Event::FileDelete { .. }) {
                return Ok(());
            }
            let kind = if matches!(ev, Event::MovieDelete { .. }) { "Deleted" } else { "Created" };
            check(
                http()
                    .post(format!("{base}/Library/Media/Updated"))
                    .header("X-MediaBrowser-Token", api_key.as_str())
                    .json(&json!({"Updates": [{"Path": m.path, "UpdateType": kind}]}))
                    .send(),
            )
        }
        NotifyKind::Kodi { host, port, username, password } => {
            let url = format!("http://{host}:{port}/jsonrpc");
            let call = |method: &str, params: Value| {
                let mut req = http().post(&url).json(&json!({"jsonrpc": "2.0", "method": method, "params": params, "id": 1}));
                if !username.is_empty() {
                    req = req.basic_auth(username, Some(password));
                }
                check(req.send())
            };
            match ev {
                Event::Download { movie, .. } | Event::Rename { movie } => call("VideoLibrary.Scan", json!({"directory": movie.path})),
                Event::MovieDelete { .. } | Event::FileDelete { .. } => call("VideoLibrary.Clean", json!({})),
                _ => call("GUI.ShowNotification", json!({"title": title, "message": body.lines().next().unwrap_or("")})),
            }
        }
        NotifyKind::Slack { webhook, channel } => {
            let mut v = json!({"text": format!("*{title}*\n{body}"), "username": "Turks"});
            if !channel.is_empty() {
                v["channel"] = json!(channel);
            }
            check(http().post(webhook).json(&v).send())
        }
        NotifyKind::Pushbullet { api_key, device } => {
            let mut v = json!({"type": "note", "title": title, "body": body});
            if !device.is_empty() {
                v["device_iden"] = json!(device);
            }
            check(http().post("https://api.pushbullet.com/v2/pushes").header("Access-Token", api_key.as_str()).json(&v).send())
        }
        NotifyKind::Apprise { server, urls, tag } => {
            let mut v = json!({"title": title, "body": body, "type": "info"});
            if !urls.trim().is_empty() {
                v["urls"] = json!(urls);
            }
            if !tag.trim().is_empty() {
                v["tag"] = json!(tag);
            }
            check(http().post(format!("{}/notify", server.trim_end_matches('/'))).json(&v).send())
        }
        NotifyKind::CustomScript { path, arguments } => {
            let mut cmd = std::process::Command::new(path);
            if !arguments.trim().is_empty() {
                cmd.args(arguments.split_whitespace());
            }
            cmd.env("radarr_eventtype", ev.kind());
            cmd.env("turks_eventtype", ev.kind());
            if let Some(m) = ev.movie() {
                for (k, v) in [
                    ("movie_id", m.id.to_string()),
                    ("movie_title", m.title.clone()),
                    ("movie_year", m.year.to_string()),
                    ("movie_path", m.path.clone()),
                    ("movie_imdbid", m.imdb_id.clone()),
                    ("movie_tmdbid", m.tmdb_id.to_string()),
                ] {
                    cmd.env(format!("radarr_{k}"), &v);
                    cmd.env(format!("turks_{k}"), &v);
                }
            }
            if let Event::Download { file, upgrade, .. } = ev {
                cmd.env("radarr_moviefile_path", &file.path);
                cmd.env("radarr_moviefile_relativepath", &file.relative_path);
                cmd.env("radarr_moviefile_quality", crate::quality::name(file.quality.quality));
                cmd.env("radarr_moviefile_releasegroup", &file.release_group);
                cmd.env("radarr_moviefile_scenename", &file.scene_name);
                cmd.env("radarr_isupgrade", if *upgrade { "True" } else { "False" });
            }
            if let Event::Grab { title, indexer, quality, size, .. } = ev {
                cmd.env("radarr_release_title", title);
                cmd.env("radarr_release_indexer", indexer);
                cmd.env("radarr_release_quality", quality);
                cmd.env("radarr_release_size", size.to_string());
            }
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                cmd.creation_flags(0x0800_0000);
            }
            let out = cmd.output().map_err(|e| format!("Couldn't run the script: {e}"))?;
            if out.status.success() {
                Ok(())
            } else {
                Err(format!("Script exited with {}", out.status))
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn send_email(server: &str, port: u16, tls: bool, user: &str, pass: &str, from: &str, to: &str, subject: &str, body: &str) -> Result<(), String> {
    use lettre::transport::smtp::authentication::Credentials;
    use lettre::{Message, SmtpTransport, Transport};
    let mut builder = Message::builder().from(from.parse().map_err(|e| format!("From address: {e}"))?).subject(subject);
    for addr in to.split([',', ';']).map(|s| s.trim()).filter(|s| !s.is_empty()) {
        builder = builder.to(addr.parse().map_err(|e| format!("To address '{addr}': {e}"))?);
    }
    let email = builder.body(body.to_string()).map_err(|e| e.to_string())?;
    let mut t = if tls {
        if port == 465 {
            SmtpTransport::relay(server).map_err(|e| e.to_string())?
        } else {
            SmtpTransport::starttls_relay(server).map_err(|e| e.to_string())?
        }
    } else {
        SmtpTransport::builder_dangerous(server)
    }
    .port(port);
    if !user.is_empty() {
        t = t.credentials(Credentials::new(user.to_string(), pass.to_string()));
    }
    t.build().send(&email).map(|_| ()).map_err(|e| e.to_string())
}
