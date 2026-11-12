//! Clicks every button on every page of the window, in both themes, and
//! reports buttons that panic or do nothing. Needs a seeded data folder:
//!
//!   TURKS_CLICKTEST_DATA=/path/to/data cargo test clicktest -- --ignored --nocapture
//!
//! The folder is copied first, so the run never changes it.

use std::collections::BTreeSet;
use std::panic::{catch_unwind, AssertUnwindSafe};

use eframe::egui::accesskit::Role;
use egui_kittest::kittest::{NodeT, Queryable};
use egui_kittest::Harness;

use super::{App, Page};

/// Not clicked by the crawl: they change or delete data, restart, or open a
/// native file dialog (which would wait for a person).
fn skipped(label: &str) -> bool {
    let l = label.to_lowercase();
    ["delete", "remove", "restart", "shut down", "reset", "restore", "update now", "install", "regenerate", "clear", "forget", "browse", "choose", "pick folder", "…", "quit"]
        .iter()
        .any(|w| l.contains(w))
}

fn fresh(theme: &str, page: Page, movie: Option<i64>) -> Harness<'static, App> {
    let core = crate::core::Core::open().expect("open data");
    let mut cfg = core.cfg();
    cfg.theme = theme.to_string();
    cfg.setup_done = true;
    core.set_cfg(cfg);
    let (_tx, rx) = std::sync::mpsc::channel();
    let mut h = Harness::builder().with_size([1366.0, 860.0]).with_max_steps(8).build_eframe(move |cc| App::new(cc, core, rx, false, None));
    h.run_steps(3);
    let app = h.state_mut();
    app.first_run = None;
    app.go(page);
    if let Some(id) = movie {
        app.open_movie(id);
    }
    app.green_in_page = true;
    app.green_zoom = None;
    app.green_reveal = None;
    app.green_leave = None;
    app.green_unzoom = None;
    h.run_steps(4);
    h
}

fn buttons(h: &Harness<'_, App>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for n in h.query_all_by_role(Role::Button) {
        let label = n.accesskit_node().label().unwrap_or_default().trim().to_string();
        if label.is_empty() {
            continue;
        }
        // Same label twice on a page (e.g. a row button) — click the first.
        if seen.insert(label.clone()) {
            out.push(label);
        }
    }
    out
}

fn snapshot(h: &Harness<'_, App>) -> String {
    let a = h.state();
    let mut s = format!("{:?}|{}|{}|{:?}|{:?}|{}", a.page, a.detail.is_some(), a.dialog.is_some(), a.notice.as_ref().map(|n| &n.0), a.status_line(), a.core.db.history_count(None));
    {
        let st = a.core.status();
        s.push_str(&format!("|cmds{}|{:?}", st.commands.len(), st.interactive.values().map(|i| i.finished).collect::<Vec<_>>()));
    }
    for n in h.root().children_recursive() {
        let k = n.accesskit_node();
        s.push_str(&format!("{:?}{}{}{:?};", k.role(), k.label().unwrap_or_default(), k.value().unwrap_or_default(), k.toggled()));
    }
    s
}

fn click(h: &mut Harness<'_, App>, label: &str) -> bool {
    let Some(n) = h.query_all_by_role(Role::Button).find(|n| n.accesskit_node().label().unwrap_or_default().trim() == label) else {
        return false;
    };
    n.scroll_to_me();
    h.run_steps(2);
    let Some(n) = h.query_all_by_role(Role::Button).find(|n| n.accesskit_node().label().unwrap_or_default().trim() == label) else {
        return false;
    };
    let enabled = !n.accesskit_node().is_disabled();
    n.click();
    h.run_steps(4);
    // Give background work (searches, refreshes, TMDb calls) time to show.
    std::thread::sleep(std::time::Duration::from_millis(1500));
    h.run_steps(3);
    enabled
}

#[test]
#[ignore]
fn clicktest() {
    let Some(src) = std::env::var_os("TURKS_CLICKTEST_DATA") else {
        eprintln!("TURKS_CLICKTEST_DATA not set; skipping");
        return;
    };
    let dir = std::env::temp_dir().join("turks-clicktest");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for e in std::fs::read_dir(&src).unwrap().flatten() {
        if e.path().is_file() {
            std::fs::copy(e.path(), dir.join(e.file_name())).unwrap();
        }
    }
    std::env::set_var("TURKS_DATA_DIR", &dir);
    let movie = crate::core::Core::open().unwrap().lib().movies.values().find(|m| m.movie_file_id.is_some()).map(|m| m.id);

    let pages = [
        (Page::Movies, None),
        (Page::Movies, movie),
        (Page::Add, None),
        (Page::Discover, None),
        (Page::Collections, None),
        (Page::Calendar, None),
        (Page::Activity, None),
        (Page::Settings, None),
        (Page::System, None),
    ];
    let mut panics = Vec::new();
    let mut dead = Vec::new();
    let mut total = 0;
    for theme in ["ff", "green"] {
        for (page, mv) in pages {
            let h = fresh(theme, page, mv);
            let top = buttons(&h);
            drop(h);
            println!("{theme}:{page:?}{}: {} buttons", if mv.is_some() { "/detail" } else { "" }, top.len());
            for b in &top {
                if skipped(b) {
                    continue;
                }
                let where_ = format!("{theme}:{page:?}{} [{b}]", if mv.is_some() { "/detail" } else { "" });
                let r = catch_unwind(AssertUnwindSafe(|| {
                    let mut h = fresh(theme, page, mv);
                    let before = snapshot(&h);
                    if !click(&mut h, b) {
                        return (true, Vec::new());
                    }
                    let changed = snapshot(&h) != before;
                    // Buttons that appeared (a tab, dialog or menu opened): click those too.
                    let after: Vec<String> = buttons(&h).into_iter().filter(|x| !top.contains(x)).collect();
                    (changed, after)
                }));
                total += 1;
                let (changed, inner) = match r {
                    Ok(v) => v,
                    Err(e) => {
                        panics.push(format!("{where_}: {}", e.downcast_ref::<String>().cloned().or(e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default()));
                        continue;
                    }
                };
                if !changed {
                    dead.push(where_.clone());
                }
                for sub in inner {
                    if skipped(&sub) || skipped(b) {
                        continue;
                    }
                    let sw = format!("{where_} > [{sub}]");
                    let r = catch_unwind(AssertUnwindSafe(|| {
                        let mut h = fresh(theme, page, mv);
                        click(&mut h, b);
                        let before = snapshot(&h);
                        if !click(&mut h, &sub) {
                            return true;
                        }
                        snapshot(&h) != before
                    }));
                    total += 1;
                    match r {
                        Ok(true) => {}
                        Ok(false) => dead.push(sw),
                        Err(e) => panics.push(format!("{sw}: {}", e.downcast_ref::<String>().cloned().or(e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default())),
                    }
                }
            }
        }
    }
    println!("clicked {total}");
    println!("panics: {}", panics.len());
    for p in &panics {
        println!("  {p}");
    }
    println!("no visible change: {}", dead.len());
    for d in &dead {
        println!("  {d}");
    }
    assert!(panics.is_empty(), "buttons panicked");
}

