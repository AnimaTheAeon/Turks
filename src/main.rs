#![cfg_attr(all(windows, not(debug_assertions), feature = "gui"), windows_subsystem = "windows")]
#![recursion_limit = "512"]
#![cfg_attr(feature = "nas", allow(dead_code))]

//! Turks: a movie collection manager. Two builds share this engine:
//! `gui` (default) is the Windows desktop app; `nas` is the Synology DSM
//! service with its web interface (see nas/).

#[cfg(all(feature = "gui", feature = "nas"))]
compile_error!("Build either the Windows app (default) or the Synology service (--no-default-features --features nas), not both.");

pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

mod api;
mod applog;
mod backup;
mod clients;
mod config;
mod core;
mod db;
mod decision;
mod formats;
mod health;
mod import;
mod indexers;
#[cfg(feature = "gui")]
mod instance;
mod library;
mod lists;
mod mediainfo;
mod metadata;
mod model;
mod naming;
mod notify;
mod parser;
mod quality;
mod release;
mod search;
mod tmdb;
mod tracked;
#[cfg(all(windows, feature = "gui"))]
mod tray;
#[cfg(feature = "gui")]
mod ui;
#[cfg(feature = "gui")]
mod updater;
#[cfg(all(windows, feature = "gui"))]
mod winint;
#[cfg(feature = "nas")]
mod nas;
#[cfg(feature = "nas")]
mod webapi;

#[cfg(feature = "gui")]
use eframe::egui;

#[cfg(feature = "nas")]
fn main() {
    nas::main();
}

#[cfg(not(any(feature = "gui", feature = "nas")))]
fn main() {
    eprintln!("Build with the gui or nas feature.");
}

#[cfg(feature = "gui")]
fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let start_hidden = args.iter().any(|a| a == "--tray");
    let after_update = args.iter().any(|a| a == updater::AFTER_UPDATE_ARG);
    let headless = args.iter().any(|a| a == "--headless");

    // Hand off to a running copy if there is one.
    let Some(listener) = instance::claim(&[], after_update) else {
        return Ok(());
    };
    updater::cleanup_old_exe();
    let restored = backup::apply_pending_restore();
    if let Some(msg) = &restored {
        applog::info(msg.clone());
    }
    #[cfg(windows)]
    winint::hold_install_mutex();
    instance::clear_quit_request();

    let core = match core::Core::open() {
        Ok(c) => c,
        Err(e) => {
            applog::error(format!("Couldn't open the database: {e}"));
            eprintln!("Turks couldn't open its database: {e}");
            std::process::exit(1);
        }
    };
    let mut cfg = core.cfg();
    let marker = config::data_dir().join("installer-autostart");
    if marker.exists() {
        cfg.start_with_windows = true;
        core.set_cfg(cfg.clone());
        let _ = std::fs::remove_file(&marker);
    }
    #[cfg(windows)]
    winint::set_autostart(cfg.start_with_windows);
    applog::info(format!("Turks {} starting", APP_VERSION));
    // Tell connections when Turks was updated.
    let last = core.db.meta("last_version").unwrap_or_default();
    if !last.is_empty() && last != APP_VERSION {
        notify::dispatch(&core, notify::Event::AppUpdate { previous: last, new: APP_VERSION.to_string() });
    }
    core.db.set_meta("last_version", APP_VERSION);
    core.start();

    // Run without a window (used by tests and for running as a service).
    if headless {
        let (tx, _rx) = std::sync::mpsc::channel();
        instance::watch_quit_request(core.clone(), tx.clone(), || {});
        instance::serve(listener, tx, || {});
        loop {
            std::thread::sleep(std::time::Duration::from_secs(3600));
        }
    }

    let icon = image::load_from_memory(include_bytes!("../resources/icon_256.png")).ok().map(|i| {
        let i = i.to_rgba8();
        let (w, h) = i.dimensions();
        egui::IconData { rgba: i.into_raw(), width: w, height: h }
    });
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(format!("Turks {}", APP_VERSION))
        .with_inner_size([1320.0, 860.0])
        .with_min_inner_size([1000.0, 660.0]);
    if let Some(icon) = icon {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native(
        "Turks",
        options,
        Box::new(move |cc| {
            let (tx, rx) = std::sync::mpsc::channel();
            let ctx = cc.egui_ctx.clone();
            let ctx2 = cc.egui_ctx.clone();
            instance::watch_quit_request(core.clone(), tx.clone(), move || ctx2.request_repaint());
            instance::serve(listener, tx, move || ctx.request_repaint());
            Ok(Box::new(ui::App::new(cc, core, rx, start_hidden, restored)))
        }),
    )
}
