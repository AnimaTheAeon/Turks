//! Watching downloads: progress for the queue, completed download handling
//! (import) and failed download handling (blocklist + search again).

use std::path::Path;
use std::time::Instant;

use chrono::Utc;

use crate::clients::{self, ClientItem, ItemStatus};
use crate::core::{Command, Core};
use crate::import::{self, Transfer};
use crate::model::*;
use crate::notify::{self, Event};

pub fn refresh(core: &Core, _cid: u64) -> Result<String, String> {
    let clients: Vec<DownloadClient> = core.lib().clients.values().filter(|c| c.enabled).cloned().collect();
    let cfg = core.cfg();
    let maps: Vec<RemotePathMapping> = core.lib().remote_paths.values().cloned().collect();
    for client in clients {
        let items = clients::items(&client);
        core.status().client_items.insert(client.id, (Instant::now(), items.clone()));
        let items = match items {
            Ok(i) => i,
            Err(e) => {
                crate::applog::warn(format!("Download client {}: {e}", client.name));
                continue;
            }
        };
        adopt_unknown(core, &client, &items);
        let tracked: Vec<Download> = core.lib().downloads.values().filter(|d| d.client_id == client.id && !d.state.finished()).cloned().collect();
        for mut d in tracked {
            let item = items.iter().find(|i| i.id.eq_ignore_ascii_case(&d.download_id));
            let Some(item) = item else {
                if d.seen_in_client {
                    // You removed it in the client: treat it as cancelled, not failed.
                    crate::applog::info(format!("{} was removed from {}; dropped from the queue", d.title, client.name));
                    core.remove::<Download>(d.id);
                } else if d.added.map(|a| Utc::now() - a > chrono::Duration::minutes(15)).unwrap_or(false) {
                    d.state = DownloadState::Warning;
                    d.messages = vec!["The download client doesn't list this download".into()];
                    core.save(d);
                }
                continue;
            };
            d.seen_in_client = true;
            d.size = if item.size > 0 { item.size } else { d.size };
            if d.last_progress.is_none() || item.remaining < d.remaining {
                d.last_progress = Some(Utc::now());
            }
            d.remaining = item.remaining;
            d.eta_secs = item.eta_secs;
            d.output_path = clients::map_remote_path(&maps, &client.host, &item.output_path);
            match item.status {
                ItemStatus::Failed => {
                    let reason = if item.message.is_empty() { "The download client reported a failure".to_string() } else { item.message.clone() };
                    fail(core, &mut d, &client, &reason, &cfg);
                    continue;
                }
                ItemStatus::Completed => {
                    if d.state != DownloadState::Warning || d.messages.is_empty() {
                        d.state = DownloadState::ImportPending;
                    }
                }
                ItemStatus::Paused => d.state = DownloadState::Paused,
                ItemStatus::Queued => d.state = DownloadState::Queued,
                ItemStatus::Downloading => d.state = DownloadState::Downloading,
                ItemStatus::Warning => {
                    d.state = DownloadState::Warning;
                    d.messages = vec![item.message.clone()];
                }
            }
            if item.status != ItemStatus::Warning && item.status != ItemStatus::Completed {
                d.messages.clear();
            }
            // Stalled: downloading, but nothing has arrived for a long time.
            if item.status == ItemStatus::Downloading && cfg.stalled_hours > 0 {
                let idle = d.last_progress.map(|t| Utc::now() - t).unwrap_or_default();
                if idle > chrono::Duration::hours(cfg.stalled_hours as i64) {
                    let reason = format!("No progress for {} hours (no seeders?)", idle.num_hours());
                    if cfg.replace_stalled {
                        fail(core, &mut d, &client, &reason, &cfg);
                        continue;
                    }
                    d.state = DownloadState::Warning;
                    d.messages = vec![reason];
                }
            }
            let completed = item.status == ItemStatus::Completed;
            let d = core.save(d);
            if completed && cfg.completed_download_handling {
                import_download(core, d, &client, item);
            }
        }
        // Imported torrents still seeding: remove once seeding goals are met.
        if client.remove_completed {
            let done: Vec<Download> = core.lib().downloads.values().filter(|d| d.client_id == client.id && d.state == DownloadState::Imported).cloned().collect();
            for d in done {
                if let Some(item) = items.iter().find(|i| i.id.eq_ignore_ascii_case(&d.download_id)) {
                    if item.can_be_removed {
                        let delete_data = d.protocol == Protocol::Torrent;
                        if clients::remove(&client, &d.download_id, delete_data).is_ok() {
                            crate::applog::info(format!("Removed {} from {} (finished seeding)", d.title, client.name));
                        }
                    }
                }
            }
        }
    }
    Ok(String::new())
}

/// Items in Turks' category that Turks didn't grab (e.g. added by hand).
fn adopt_unknown(core: &Core, client: &DownloadClient, items: &[ClientItem]) {
    let known: std::collections::HashSet<String> = core.lib().downloads.values().map(|d| d.download_id.to_lowercase()).collect();
    for item in items {
        if known.contains(&item.id.to_lowercase()) || item.status != ItemStatus::Completed || client.kind.is_blackhole() && item.title.is_empty() {
            continue;
        }
        // Already imported earlier (e.g. Turks restarted).
        if core.db.history_for_download(&item.id).iter().any(|h| h.event == HistoryEvent::Imported) {
            continue;
        }
        let Some(parsed) = crate::parser::parse_movie_title(&item.title, false) else { continue };
        let movie_id = crate::decision::map_movie(&core.lib(), &parsed, &crate::release::Release::default(), None);
        let Some(mid) = movie_id else { continue };
        crate::applog::info(format!("Found '{}' in {} and matched it to a movie", item.title, client.name));
        core.save(Download {
            download_id: item.id.clone(),
            client_id: client.id,
            movie_id: mid,
            title: item.title.clone(),
            quality: parsed.quality,
            languages: parsed.languages,
            protocol: client.kind.protocol(),
            size: item.size,
            state: DownloadState::ImportPending,
            added: Some(Utc::now()),
            seen_in_client: true,
            ..Default::default()
        });
    }
}

fn import_download(core: &Core, mut d: Download, client: &DownloadClient, item: &ClientItem) {
    let cfg = core.cfg();
    let output = d.output_path.clone();
    let path = Path::new(&output);
    if d.output_path.is_empty() || !path.exists() {
        d.state = DownloadState::Warning;
        d.messages = vec![format!(
            "Turks can't see the download at '{}'. If the client runs on another machine, add a remote path mapping.",
            d.output_path
        )];
        core.save(d);
        return;
    }
    d.state = DownloadState::Importing;
    let d = core.save(d);
    let mut files = import::video_files(path);
    // Biggest first: that's the movie, the rest are extras or samples.
    files.sort_by_key(|p| std::cmp::Reverse(std::fs::metadata(p).map(|m| m.len()).unwrap_or(0)));
    let mut reasons = Vec::new();
    let mode = if d.protocol == Protocol::Torrent && !cfg.use_hardlinks {
        Transfer::Copy
    } else if d.protocol == Protocol::Torrent {
        Transfer::HardlinkOrCopy
    } else {
        Transfer::Move
    };
    let mut imported = false;
    for f in &files {
        let dec = import::evaluate(&core.lib(), &cfg, f, Some(d.movie_id), Some(&d), false);
        if !dec.approved() {
            reasons.push(format!("{}: {}", f.file_name().unwrap_or_default().to_string_lossy(), dec.rejections.join("; ")));
            continue;
        }
        match import::import(core, &dec, mode, Some(&d)) {
            Ok(_) => {
                imported = true;
                break;
            }
            Err(e) => reasons.push(format!("{}: {e}", f.display())),
        }
    }
    let mut d = core.lib().downloads.get(&d.id).cloned().unwrap_or(d);
    if imported {
        d.state = DownloadState::Imported;
        d.messages.clear();
        let d = core.save(d);
        let removable = d.protocol == Protocol::Usenet || item.can_be_removed;
        if client.remove_completed && removable {
            let delete_data = true;
            let _ = clients::remove(client, &d.download_id, delete_data);
        }
        if cfg.delete_empty_folders && d.protocol == Protocol::Usenet {
            let _ = std::fs::remove_dir(path);
        }
    } else {
        if files.is_empty() {
            reasons.push("No video files were found in the download".into());
        }
        let was_warning = d.state == DownloadState::Warning && d.messages == reasons;
        d.state = DownloadState::Warning;
        d.messages = reasons.clone();
        let d = core.save(d);
        if !was_warning {
            let movie = core.movie(d.movie_id);
            notify::dispatch(core, Event::ManualInteraction { title: d.title.clone(), message: reasons.join("\n"), movie });
        }
    }
}

fn fail(core: &Core, d: &mut Download, client: &DownloadClient, reason: &str, cfg: &crate::config::Config) {
    d.state = DownloadState::Failed;
    d.messages = vec![reason.to_string()];
    let saved = core.save(d.clone());
    core.history(History {
        movie_id: saved.movie_id,
        event: HistoryEvent::DownloadFailed,
        source_title: saved.title.clone(),
        quality: saved.quality,
        download_id: saved.download_id.clone(),
        data: [("message".to_string(), reason.to_string()), ("downloadClient".to_string(), client.name.clone())].into(),
        ..Default::default()
    });
    blocklist(core, &saved, reason);
    if client.remove_failed {
        let _ = clients::remove(client, &saved.download_id, true);
    }
    let again = cfg.redownload_failed && (!saved.from_interactive || cfg.redownload_failed_from_interactive);
    if again {
        core.run(Command::SearchMovies(vec![saved.movie_id]));
    }
    crate::applog::warn(format!("Download failed: {} ({reason})", saved.title));
}

pub fn blocklist(core: &Core, d: &Download, reason: &str) {
    core.save(Blocklist {
        id: 0,
        movie_id: d.movie_id,
        source_title: d.title.clone(),
        quality: d.quality,
        date: Some(Utc::now()),
        indexer: d.indexer.clone(),
        protocol: d.protocol,
        message: reason.to_string(),
        info_hash: if d.protocol == Protocol::Torrent { d.download_id.clone() } else { String::new() },
        size: d.size,
    });
}

/// Queue actions from the UI.
pub fn remove_from_queue(core: &Core, download_id: Id, remove_from_client: bool, add_to_blocklist: bool, search_again: bool) -> Result<(), String> {
    let Some(mut d) = core.lib().downloads.get(&download_id).cloned() else { return Ok(()) };
    let client = core.lib().clients.get(&d.client_id).cloned();
    if remove_from_client {
        if let Some(c) = &client {
            clients::remove(c, &d.download_id, true)?;
        }
    }
    if add_to_blocklist {
        blocklist(core, &d, "Removed from the queue and blocklisted");
    }
    d.state = DownloadState::Failed;
    d.messages = vec!["Removed from the queue".into()];
    let d = core.save(d);
    if search_again {
        core.run(Command::SearchMovies(vec![d.movie_id]));
    }
    Ok(())
}
