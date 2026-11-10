//! Windows integration: autostart and the installer's single-instance mutex.

#![cfg(windows)]

use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;

fn exe() -> String {
    std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_default()
}

pub fn set_autostart(on: bool) {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if let Ok((run, _)) = hkcu.create_subkey(r"Software\Microsoft\Windows\CurrentVersion\Run") {
        if on {
            let _ = run.set_value("Turks", &format!("\"{}\" --tray", exe()));
        } else {
            let _ = run.delete_value("Turks");
        }
    }
}

/// Named mutex the installer checks so it can close Turks before updating.
pub fn hold_install_mutex() {
    use windows::Win32::System::Threading::CreateMutexW;
    unsafe {
        // Leaked on purpose: it must live until the process exits.
        let _ = CreateMutexW(None, false, windows::core::w!("TurksSingleInstance"));
    }
}
