//! One running copy at a time. A second launch (for example from a magnet
//! link in the browser) hands its arguments to the first and exits.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::Sender;
use std::time::Duration;

const PORT: u16 = 47_824;

#[allow(dead_code)]
pub enum Incoming {
    Show,
    Open(String),
    /// A newer version was started: quit so it can take over.
    Replace,
}

/// File the installer drops to ask a running Turks to quit.
pub const QUIT_REQUEST: &str = "quit.request";

/// Remove a quit request left over from an earlier install.
pub fn clear_quit_request() {
    let _ = std::fs::remove_file(crate::config::data_dir().join(QUIT_REQUEST));
}

/// Watch for the installer's quit request. Asks the window to close first;
/// if that hasn't finished in 3 seconds (e.g. the window is hidden in the
/// tray), saves and exits directly.
pub fn watch_quit_request(core: crate::core::Core, tx: Sender<Incoming>, wake: impl Fn() + Send + 'static) {
    let path = crate::config::data_dir().join(QUIT_REQUEST);
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(400));
        if !path.exists() {
            continue;
        }
        let _ = std::fs::remove_file(&path);
        crate::applog::info("Installer asked Turks to close");
        let _ = tx.send(Incoming::Replace);
        wake();
        std::thread::sleep(Duration::from_secs(3));
        core.shutdown();
        std::process::exit(0);
    });
}

fn newer(v: &str) -> bool {
    crate::updater::is_newer(v, crate::updater::APP_VERSION)
}

fn wait_for_port() -> Option<TcpListener> {
    for _ in 0..60 {
        if let Ok(l) = TcpListener::bind(("127.0.0.1", PORT)) {
            return Some(l);
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    None
}

/// Returns a listener if we're the running instance, None if we handed off.
pub fn claim(args: &[String], after_update: bool) -> Option<TcpListener> {
    // Right after an update the old copy may still be saving and shutting down.
    if after_update {
        if let Some(l) = wait_for_port() {
            return Some(l);
        }
    }
    if let Ok(l) = TcpListener::bind(("127.0.0.1", PORT)) {
        return Some(l);
    }
    let Ok(mut s) = TcpStream::connect_timeout(&([127, 0, 0, 1], PORT).into(), Duration::from_secs(2)) else {
        // Port taken by something else; run anyway.
        return TcpListener::bind(("127.0.0.1", 0)).ok();
    };
    let _ = writeln!(s, "TURKS");
    let _ = writeln!(s, "VERSION {}", crate::updater::APP_VERSION);
    let _ = s.flush();
    // Newer copies of Turks answer; older ones (before 0.1.4) don't.
    let _ = s.set_read_timeout(Some(Duration::from_millis(1500)));
    let mut reply = String::new();
    let _ = BufReader::new(s.try_clone().ok()?).read_line(&mut reply);
    if reply.trim() == "BYE" {
        // The running copy is older and is quitting; take over.
        drop(s);
        return wait_for_port();
    }
    if args.is_empty() {
        let _ = writeln!(s, "SHOW");
    }
    for a in args {
        let _ = writeln!(s, "OPEN {a}");
    }
    None
}

pub fn serve(listener: TcpListener, tx: Sender<Incoming>, wake: impl Fn() + Send + 'static) {
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));
            let Ok(mut writer) = stream.try_clone() else { continue };
            let mut lines = BufReader::new(stream).lines();
            if lines.next().and_then(|l| l.ok()).as_deref() != Some("TURKS") {
                continue;
            }
            for line in lines.map_while(Result::ok) {
                if let Some(v) = line.strip_prefix("VERSION ") {
                    if newer(v.trim()) {
                        let _ = writeln!(writer, "BYE");
                        let _ = tx.send(Incoming::Replace);
                        break;
                    }
                    let _ = writeln!(writer, "OK");
                } else if line == "SHOW" {
                    let _ = tx.send(Incoming::Show);
                } else if let Some(arg) = line.strip_prefix("OPEN ") {
                    let _ = tx.send(Incoming::Open(arg.to_string()));
                }
            }
            wake();
        }
    });
}
