//! Turks for Synology DSM.
//!
//! `turks daemon`   background service (started by DSM)
//! `turks cgi`      the DSM web bridge (run by DSM as api.cgi)
//! `turks update`   install the latest GitHub release (root, Task Scheduler)
//! `turks version`  print the version

pub mod cgi;
pub mod update;

use crate::{applog, config, core, notify, APP_VERSION};

pub fn main() {
    let mut args = std::env::args().skip(1);
    let mode = args.next().unwrap_or_else(|| "daemon".into());
    let rest: Vec<String> = args.collect();
    match mode.as_str() {
        "cgi" => cgi::run(),
        "update" => std::process::exit(update::run_cli(&rest)),
        "version" | "--version" => println!("{APP_VERSION}"),
        _ => daemon(rest.iter().any(|a| a == "--restarted")),
    }
}

/// Where the package keeps its data: DSM's var folder for the package.
pub fn var_dir() -> std::path::PathBuf {
    if let Some(d) = std::env::var_os("TURKS_DATA_DIR") {
        return d.into();
    }
    if let Some(d) = std::env::var_os("SYNOPKG_PKGVAR") {
        return d.into();
    }
    "/var/packages/Turks/var".into()
}

fn daemon(restarted: bool) {
    let var = var_dir();
    let _ = std::fs::create_dir_all(&var);
    // SAFETY: single-threaded at this point; nothing else reads the env yet.
    unsafe { std::env::set_var("TURKS_DATA_DIR", &var) };
    let _ = rustls::crypto::ring::default_provider().install_default();
    if restarted {
        // The old process is still letting go of its ports.
        std::thread::sleep(std::time::Duration::from_secs(2));
    }
    let _ = std::fs::write(var.join("turks.pid"), std::process::id().to_string());

    if let Some(msg) = crate::backup::apply_pending_restore() {
        applog::info(msg);
    }
    // First start on the NAS: let other apps on the network reach the
    // Radarr-compatible API (Overseerr, Prowlarr, Jellyseerr…).
    let first_run = !config::Config::path().exists();
    let core = match core::Core::open() {
        Ok(c) => c,
        Err(e) => {
            applog::error(format!("Couldn't open the database: {e}"));
            eprintln!("Turks couldn't open its database: {e}");
            std::process::exit(1);
        }
    };
    if first_run {
        let mut cfg = core.cfg();
        cfg.api_bind_all = true;
        cfg.close_to_tray = false;
        cfg.start_with_windows = false;
        core.set_cfg(cfg);
    }
    applog::info(format!("Turks {APP_VERSION} for Synology starting"));
    let last = core.db.meta("last_version").unwrap_or_default();
    if !last.is_empty() && last != APP_VERSION {
        notify::dispatch(&core, notify::Event::AppUpdate { previous: last, new: APP_VERSION.to_string() });
    }
    core.db.set_meta("last_version", APP_VERSION);
    core.start();
    stop_on_signal(core.clone());
    update::start_checker();
    crate::webapi::serve(core);
}

/// Start a fresh copy of the service and exit this one (after a restore).
pub fn restart() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::process::Command::new(exe)
            .args(["daemon", "--restarted"])
            .stdin(std::process::Stdio::null())
            .spawn();
    }
    std::process::exit(0);
}

static STOP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

extern "C" fn on_signal(_: libc::c_int) {
    STOP.store(true, std::sync::atomic::Ordering::SeqCst);
}

/// DSM stops packages with SIGTERM: save task times and exit cleanly.
fn stop_on_signal(core: core::Core) {
    // SAFETY: the handler only stores to an atomic.
    unsafe {
        libc::signal(libc::SIGTERM, on_signal as *const () as libc::sighandler_t);
        libc::signal(libc::SIGINT, on_signal as *const () as libc::sighandler_t);
    }
    std::thread::spawn(move || loop {
        if STOP.load(std::sync::atomic::Ordering::SeqCst) {
            applog::info("Turks stopping");
            core.shutdown();
            let _ = std::fs::remove_file(var_dir().join("turks.pid"));
            std::process::exit(0);
        }
        std::thread::sleep(std::time::Duration::from_millis(300));
    });
}
