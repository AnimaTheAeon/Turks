//! System › Status health checks (Radarr's HealthCheck service).

use std::path::Path;

use crate::core::{Core, HealthIssue, HealthLevel};
use crate::model::{DownloadState, Protocol};
use crate::notify::{self, Event};

fn issue(source: &str, level: HealthLevel, message: impl Into<String>) -> HealthIssue {
    HealthIssue { source: source.into(), level, message: message.into() }
}

pub fn check(core: &Core) {
    let cfg = core.cfg();
    let mut out = Vec::new();
    {
        let lib = core.lib();
        if !cfg.tmdb_ready() {
            out.push(issue("Metadata", HealthLevel::Error, "No TMDb API key. Turks can't look up movies until you add one in Settings › General."));
        }
        if lib.root_folders.is_empty() {
            out.push(issue("Root folders", HealthLevel::Warning, "No root folder is set up. Add one in Settings › Media Management."));
        }
        for r in lib.root_folders.values() {
            if !Path::new(&r.path).is_dir() {
                out.push(issue("Root folders", HealthLevel::Error, format!("Root folder is missing: {}", r.path)));
            } else if let Some(free) = crate::import::free_space(Path::new(&r.path)) {
                if free < 5 * 1_073_741_824 {
                    out.push(issue("Disk space", HealthLevel::Warning, format!("Only {} free on {}", crate::decision::human_size(free), r.path)));
                }
            }
        }
        let missing_roots: Vec<String> = lib
            .movies
            .values()
            .filter(|m| !m.root_folder.is_empty() && !lib.root_folders.values().any(|r| r.path == m.root_folder))
            .map(|m| m.root_folder.clone())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        if !missing_roots.is_empty() {
            out.push(issue("Root folders", HealthLevel::Warning, format!("Movies use root folders that aren't listed: {}", missing_roots.join(", "))));
        }
        if lib.indexers.is_empty() {
            out.push(issue("Indexers", HealthLevel::Warning, "No indexers are set up, so Turks can't find releases."));
        } else {
            if !lib.indexers.values().any(|i| i.enable_rss) {
                out.push(issue("Indexers", HealthLevel::Warning, "No indexer has RSS turned on, so new releases won't be grabbed automatically."));
            }
            if !lib.indexers.values().any(|i| i.enable_auto_search) {
                out.push(issue("Indexers", HealthLevel::Warning, "No indexer has automatic search turned on."));
            }
        }
        let has = |p: Protocol| lib.clients.values().any(|c| c.enabled && c.kind.supports(p));
        if lib.clients.values().all(|c| !c.enabled) {
            out.push(issue("Download clients", HealthLevel::Warning, "No download client is set up, so nothing can be downloaded."));
        } else {
            for p in [Protocol::Torrent, Protocol::Usenet] {
                let ix = lib.indexers.values().any(|i| i.kind.protocol() == p);
                if ix && !has(p) {
                    out.push(issue("Download clients", HealthLevel::Warning, format!("You have {} indexers but no {} download client.", p.label().to_lowercase(), p.label().to_lowercase())));
                }
            }
        }
        let stuck = lib.downloads.values().filter(|d| d.state == DownloadState::Warning).count();
        if stuck > 0 {
            out.push(issue("Downloads", HealthLevel::Warning, format!("{stuck} download(s) need attention in Activity › Queue.")));
        }
        // Synology: with no recycle bin folder set, deletes go to the shared
        // folder's own recycle bin, if it's turned on.
        #[cfg(not(windows))]
        if cfg.recycle_bin.trim().is_empty() {
            for r in lib.root_folders.values() {
                let p = Path::new(&r.path);
                if r.path.starts_with("/volume") && p.is_dir() && crate::import::share_recycle_dir(p).is_none() {
                    out.push(issue("Recycle bin", HealthLevel::Notice, format!("The recycle bin is off for the shared folder holding {}, so deleted movies are gone for good. Turn it on in Control Panel › Shared Folder › Edit.", r.path)));
                }
            }
        }
        if !cfg.recycle_bin.trim().is_empty() && !Path::new(cfg.recycle_bin.trim()).is_dir() {
            out.push(issue("Recycle bin", HealthLevel::Warning, format!("Recycle bin folder doesn't exist: {}", cfg.recycle_bin)));
        }
    }
    {
        let s = core.status();
        for (id, (_, res)) in &s.client_items {
            if let Err(e) = res {
                let name = core.lib().clients.get(id).map(|c| c.name.clone()).unwrap_or_default();
                out.push(issue("Download clients", HealthLevel::Error, format!("Can't reach {name}: {e}")));
            }
        }
        for (id, (_, msg, n)) in &s.indexer_failures {
            if *n >= 1 {
                let name = core.lib().indexers.get(id).map(|i| i.name.clone()).unwrap_or_default();
                out.push(issue("Indexers", HealthLevel::Warning, format!("{name} is failing: {msg}")));
            }
        }
    }
    if cfg.analyse_video_files && crate::mediainfo::ffprobe_path(&cfg.ffprobe_path).is_none() {
        out.push(issue("Media info", HealthLevel::Notice, "ffprobe wasn't found, so media info comes from file names. Put ffprobe.exe next to turks.exe or set its path in Settings › General."));
    }

    let previous = std::mem::replace(&mut core.status().health, out.clone());
    for i in out.iter().filter(|i| !previous.contains(i)) {
        if i.level >= HealthLevel::Warning {
            notify::dispatch(core, Event::Health { issue: i.clone() });
        }
    }
    for i in previous.iter().filter(|i| !out.contains(i)) {
        notify::dispatch(core, Event::HealthRestored { issue: i.clone() });
    }
    core.repaint();
}
