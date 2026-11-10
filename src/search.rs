//! Searching indexers, RSS sync, pending (delayed) releases and grabbing.

use std::collections::HashSet;

use chrono::Utc;

use crate::core::{Core, InteractiveResult};
use crate::decision::{self, Ctx, Mode};
use crate::model::*;
use crate::notify::{self, Event};
use crate::release::{Candidate, Release};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Purpose {
    Rss,
    Auto,
    Interactive,
}

fn indexers_for(core: &Core, purpose: Purpose) -> Vec<Indexer> {
    core.lib()
        .indexers
        .values()
        .filter(|i| match purpose {
            Purpose::Rss => i.enable_rss,
            Purpose::Auto => i.enable_auto_search,
            Purpose::Interactive => i.enable_interactive_search,
        })
        .cloned()
        .collect()
}

/// Queries indexers in parallel.
fn query(core: &Core, indexers: Vec<Indexer>, movie: Option<&Movie>) -> (Vec<Release>, Vec<String>) {
    let mut handles = Vec::new();
    let mut errors = Vec::new();
    for ix in indexers {
        if let Some(why) = core.indexer_blocked(ix.id) {
            errors.push(format!("{} is paused after errors: {why}", ix.name));
            continue;
        }
        let core = core.clone();
        let movie = movie.cloned();
        handles.push(std::thread::spawn(move || {
            let r = match &movie {
                Some(m) => {
                    let caps = core.caps_for(&ix);
                    crate::indexers::search(&ix, m, &caps)
                }
                None => crate::indexers::rss(&ix),
            };
            core.indexer_result(ix.id, r.as_ref().err().cloned());
            r
        }));
    }
    let mut out = Vec::new();
    for h in handles {
        match h.join() {
            Ok(Ok(v)) => out.extend(v),
            Ok(Err(e)) => errors.push(e),
            Err(_) => errors.push("An indexer search crashed".into()),
        }
    }
    // Same release from two indexers: keep the higher priority one.
    out.sort_by_key(|r| r.indexer_priority);
    let mut seen = HashSet::new();
    out.retain(|r| seen.insert((r.title.to_lowercase(), r.size / 1_048_576)));
    (out, errors)
}

fn context<'a>(core: &Core, lib: &'a crate::db::Library, cfg: &'a crate::config::Config, mode: Mode) -> Ctx<'a> {
    let imported: HashSet<String> = core
        .db
        .history_since(Utc::now() - chrono::Duration::days(365))
        .into_iter()
        .filter(|h| h.event == HistoryEvent::Imported)
        .map(|h| h.source_title)
        .collect();
    let grabbed: HashSet<String> = core
        .db
        .history_since(Utc::now() - chrono::Duration::hours(12))
        .into_iter()
        .filter(|h| h.event == HistoryEvent::Grabbed)
        .map(|h| h.source_title.to_lowercase())
        .collect();
    Ctx { lib, cfg, mode, imported_titles: imported, grabbed_titles: grabbed }
}

fn decide(core: &Core, releases: Vec<Release>, searched: Option<Id>, mode: Mode) -> Vec<Candidate> {
    let lib = core.lib().clone();
    let cfg = core.cfg();
    let ctx = context(core, &lib, &cfg, mode);
    let mut cands: Vec<Candidate> = releases.into_iter().map(|r| decision::candidate(&lib, r, searched)).collect();
    for c in cands.iter_mut() {
        decision::evaluate(&ctx, c);
    }
    cands.sort_by(|a, b| {
        // Approved first, then the decision engine's order.
        b.approved().cmp(&a.approved()).then_with(|| decision::compare(&lib, &cfg, a, b))
    });
    cands
}

pub fn interactive(core: &Core, movie_id: Id) -> Result<String, String> {
    let Some(movie) = core.movie(movie_id) else { return Err("Movie not found".into()) };
    core.status().interactive.insert(movie_id, InteractiveResult { running: true, ..Default::default() });
    core.repaint();
    let indexers = indexers_for(core, Purpose::Interactive);
    if indexers.is_empty() {
        core.status().interactive.insert(movie_id, InteractiveResult { running: false, errors: vec!["No indexers are set up for interactive search.".into()], finished: Some(Utc::now()), ..Default::default() });
        return Err("No indexers".into());
    }
    let (releases, errors) = query(core, indexers, Some(&movie));
    let found = releases.len();
    let cands = decide(core, releases, Some(movie_id), Mode::Interactive);
    core.status().interactive.insert(movie_id, InteractiveResult { running: false, candidates: cands, errors, finished: Some(Utc::now()) });
    core.repaint();
    Ok(format!("{found} releases"))
}

pub fn search_movies(core: &Core, ids: &[Id], cid: u64) -> Result<String, String> {
    let indexers = indexers_for(core, Purpose::Auto);
    if indexers.is_empty() {
        return Err("No indexers are set up for automatic search.".into());
    }
    let mut grabbed = 0;
    for (i, id) in ids.iter().enumerate() {
        if core.shutting_down.load(std::sync::atomic::Ordering::SeqCst) {
            break;
        }
        let Some(mut movie) = core.movie(*id) else { continue };
        core.progress(cid, format!("Searching {} ({}/{})", movie.title, i + 1, ids.len()));
        let (releases, errors) = query(core, indexers.clone(), Some(&movie));
        for e in errors {
            crate::applog::warn(e);
        }
        let cands = decide(core, releases, Some(*id), Mode::AutoSearch);
        movie.last_search = Some(Utc::now());
        core.save(movie.clone());
        if let Some(best) = cands.iter().find(|c| c.approved()) {
            match grab(core, best, false) {
                Ok(_) => grabbed += 1,
                Err(e) => crate::applog::warn(format!("Couldn't grab {}: {e}", best.release.title)),
            }
        } else if let Some(delayed) = cands.iter().find(|c| c.temporarily_rejected && !c.permanently_rejected()) {
            add_pending(core, delayed);
        } else {
            crate::applog::info(format!("No acceptable release found for {}", movie.display()));
        }
        // Be gentle with indexers on big batch searches.
        if ids.len() > 1 {
            std::thread::sleep(std::time::Duration::from_secs(2));
        }
    }
    let msg = format!("Grabbed {grabbed} of {}", ids.len());
    if ids.len() == 1 && grabbed == 0 {
        core.toast("Search finished: nothing acceptable was found.");
    }
    Ok(msg)
}

pub fn rss_sync(core: &Core, cid: u64) -> Result<String, String> {
    let indexers = indexers_for(core, Purpose::Rss);
    let mut grabbed = 0;
    if !indexers.is_empty() {
        core.progress(cid, "Fetching feeds");
        let (releases, errors) = query(core, indexers, None);
        for e in errors {
            crate::applog::warn(e);
        }
        let total = releases.len();
        let cands = decide(core, releases, None, Mode::Rss);
        let mut done: HashSet<Id> = HashSet::new();
        for c in cands.iter() {
            let Some(mid) = c.movie_id else { continue };
            if done.contains(&mid) {
                continue;
            }
            if c.approved() {
                if grab(core, c, false).is_ok() {
                    grabbed += 1;
                    done.insert(mid);
                }
            } else if c.temporarily_rejected && !c.permanently_rejected() {
                add_pending(core, c);
                done.insert(mid);
            }
        }
        crate::applog::info(format!("RSS sync: {total} releases checked, {grabbed} grabbed"));
    }
    grabbed += process_pending(core);
    Ok(format!("Grabbed {grabbed}"))
}

// ---------------------------------------------------------------- pending (delay profiles)

fn add_pending(core: &Core, c: &Candidate) {
    let Some(mid) = c.movie_id else { return };
    let exists = core.lib().pending.values().any(|p| p.movie_id == mid && p.release.title == c.release.title);
    if exists {
        return;
    }
    let delay = {
        let lib = core.lib();
        lib.movies.get(&mid).map(|m| decision::delay_profile_for(&lib, m)).unwrap_or_default()
    };
    let mins = match c.release.protocol {
        Protocol::Usenet => delay.usenet_delay,
        Protocol::Torrent => delay.torrent_delay,
    };
    let at = c.release.publish_date.unwrap_or_else(Utc::now) + chrono::Duration::minutes(mins);
    core.save(Pending { id: 0, movie_id: mid, release: c.release.clone(), release_at: Some(at) });
}

/// Grabs delayed releases whose wait is over (best per movie).
pub fn process_pending(core: &Core) -> usize {
    let due: Vec<Pending> = core.lib().pending.values().filter(|p| p.release_at.map(|t| t <= Utc::now()).unwrap_or(true)).cloned().collect();
    if due.is_empty() {
        return 0;
    }
    let mut grabbed = 0;
    let mut by_movie: std::collections::BTreeMap<Id, Vec<Pending>> = Default::default();
    for p in due {
        by_movie.entry(p.movie_id).or_default().push(p);
    }
    for (mid, list) in by_movie {
        for p in &list {
            core.remove::<Pending>(p.id);
        }
        // Re-judge without the delay (it has passed).
        let releases: Vec<Release> = list.into_iter().map(|p| p.release).collect();
        let lib = core.lib().clone();
        let cfg = core.cfg();
        let ctx = context(core, &lib, &cfg, Mode::Interactive);
        let mut cands: Vec<Candidate> = releases.into_iter().map(|r| decision::candidate(&lib, r, Some(mid))).collect();
        for c in cands.iter_mut() {
            decision::evaluate(&ctx, c);
        }
        cands.retain(|c| c.approved());
        cands.sort_by(|a, b| decision::compare(&lib, &cfg, a, b));
        if let Some(best) = cands.first() {
            if grab(core, best, false).is_ok() {
                grabbed += 1;
            }
        }
    }
    grabbed
}

// ---------------------------------------------------------------- grabbing

fn pick_client(core: &Core, release: &Release, movie: &Movie) -> Result<DownloadClient, String> {
    let lib = core.lib();
    if let Some(ix) = lib.indexers.get(&release.indexer_id) {
        if ix.download_client_id != 0 {
            if let Some(c) = lib.clients.get(&ix.download_client_id).filter(|c| c.enabled) {
                return Ok(c.clone());
            }
        }
    }
    let mut list: Vec<&DownloadClient> = lib
        .clients
        .values()
        .filter(|c| c.enabled && c.kind.supports(release.protocol))
        .filter(|c| c.tags.is_empty() || c.tags.iter().any(|t| movie.tags.contains(t)))
        .collect();
    list.sort_by_key(|c| c.priority);
    list.first().map(|c| (*c).clone()).ok_or_else(|| format!("No {} download client is set up", release.protocol.label().to_lowercase()))
}

pub fn grab(core: &Core, c: &Candidate, interactive: bool) -> Result<String, String> {
    let mid = c.movie_id.ok_or("This release doesn't match a movie in your library")?;
    let movie = core.movie(mid).ok_or("Movie not found")?;
    let client = pick_client(core, &c.release, &movie)?;
    let payload = crate::clients::fetch_payload(&c.release)?;
    let hash = crate::clients::payload_hash(&payload);
    let id = crate::clients::add(&client, &c.release, payload)?;
    let download_id = if id.is_empty() { hash.unwrap_or_default() } else { id };
    core.save(Download {
        id: 0,
        download_id: download_id.clone(),
        client_id: client.id,
        movie_id: mid,
        title: c.release.title.clone(),
        quality: c.parsed.quality,
        languages: c.parsed.languages.clone(),
        custom_format_score: c.score,
        protocol: c.release.protocol,
        indexer: c.release.indexer.clone(),
        size: c.release.size,
        remaining: c.release.size,
        eta_secs: None,
        state: DownloadState::Queued,
        messages: vec![],
        output_path: String::new(),
        added: Some(Utc::now()),
        from_interactive: interactive,
        seen_in_client: false,
    });
    let mut data = std::collections::BTreeMap::new();
    data.insert("indexer".into(), c.release.indexer.clone());
    data.insert("downloadClient".into(), client.name.clone());
    data.insert("size".into(), c.release.size.to_string());
    data.insert("protocol".into(), c.release.protocol.label().into());
    data.insert("releaseGroup".into(), c.parsed.release_group.clone());
    data.insert("guid".into(), c.release.guid.clone());
    data.insert("infoUrl".into(), c.release.info_url.clone());
    if !c.format_names.is_empty() {
        data.insert("customFormats".into(), c.format_names.join(", "));
    }
    core.history(History {
        movie_id: mid,
        event: HistoryEvent::Grabbed,
        source_title: c.release.title.clone(),
        quality: c.parsed.quality,
        languages: c.parsed.languages.clone(),
        custom_format_score: c.score,
        download_id: download_id.clone(),
        data,
        ..Default::default()
    });
    core.toast(format!("Grabbed {} → {}", c.release.title, client.name));
    notify::dispatch(
        core,
        Event::Grab {
            movie,
            title: c.release.title.clone(),
            quality: crate::quality::full_name(&c.parsed.quality),
            indexer: c.release.indexer.clone(),
            client: client.name.clone(),
            size: c.release.size,
            score: c.score,
        },
    );
    // Show it in the queue right away.
    core.run(crate::core::Command::RefreshDownloads);
    Ok(download_id)
}

/// Grab from the interactive search window (runs in the background).
pub fn grab_in_background(core: &Core, c: Candidate) {
    let core = core.clone();
    std::thread::spawn(move || {
        if let Err(e) = grab(&core, &c, true) {
            core.toast(format!("Couldn't grab: {e}"));
        }
    });
}
