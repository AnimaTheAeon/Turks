//! Backups: settings.json plus a consistent copy of the database, zipped.
//! Restores are staged and applied on the next start.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::config::{data_dir, Config};
use crate::core::Core;

fn pending_path() -> PathBuf {
    data_dir().join("restore_pending.zip")
}

pub fn create(core: &Core, kind: &str) -> Result<PathBuf, String> {
    let cfg = core.cfg();
    let dir = PathBuf::from(&cfg.backup_dir);
    std::fs::create_dir_all(&dir).map_err(|e| format!("Couldn't create the backup folder: {e}"))?;
    let db_copy = data_dir().join("backup-temp.db");
    core.db.backup_to(&db_copy)?;
    let name = format!("Turks_{kind}_{}.zip", chrono::Local::now().format("%Y.%m.%d_%H.%M.%S"));
    let path = dir.join(name);
    let file = std::fs::File::create(&path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, p) in [("settings.json", Config::path()), ("turks.db", db_copy.clone())] {
        if let Ok(data) = std::fs::read(&p) {
            zip.start_file(name, opts).map_err(|e| e.to_string())?;
            zip.write_all(&data).map_err(|e| e.to_string())?;
        }
    }
    zip.finish().map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(db_copy);
    let mut c = cfg;
    c.last_backup = chrono::Utc::now().timestamp();
    core.set_cfg(c);
    crate::applog::info(format!("Backup saved: {}", path.display()));
    prune(&core.cfg());
    Ok(path)
}

pub fn list(cfg: &Config) -> Vec<(PathBuf, u64, std::time::SystemTime)> {
    let mut out: Vec<_> = std::fs::read_dir(&cfg.backup_dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("Turks_") && e.path().extension().map(|x| x == "zip").unwrap_or(false))
        .filter_map(|e| {
            let m = e.metadata().ok()?;
            Some((e.path(), m.len(), m.modified().ok()?))
        })
        .collect();
    out.sort_by_key(|x| std::cmp::Reverse(x.2));
    out
}

/// Scheduled backups older than the retention period are removed.
pub fn prune(cfg: &Config) {
    let keep = std::time::Duration::from_secs(cfg.backup_retention_days.max(1) as u64 * 86400);
    for (p, _, t) in list(cfg) {
        let scheduled = p.file_name().map(|n| n.to_string_lossy().contains("_scheduled_")).unwrap_or(false);
        if scheduled && t.elapsed().map(|e| e > keep).unwrap_or(false) {
            let _ = std::fs::remove_file(p);
        }
    }
}

pub fn stage_restore(from: &Path) -> Result<(), String> {
    let data = std::fs::read(from).map_err(|e| format!("Couldn't read the backup: {e}"))?;
    let zip = zip::ZipArchive::new(std::io::Cursor::new(&data)).map_err(|_| "That isn't a Turks backup".to_string())?;
    let names: Vec<String> = zip.file_names().filter_map(|n| n.ok()).map(|n| n.to_string()).collect();
    if !names.iter().any(|n| n == "turks.db") {
        return Err("That backup has no database in it".into());
    }
    std::fs::write(pending_path(), &data).map_err(|e| e.to_string())
}

/// Called at startup before the database is opened.
pub fn apply_pending_restore() -> Option<String> {
    let path = pending_path();
    let data = std::fs::read(&path).ok()?;
    let _ = std::fs::remove_file(&path);
    let mut zip = match zip::ZipArchive::new(std::io::Cursor::new(data)) {
        Ok(z) => z,
        Err(e) => return Some(format!("Backup couldn't be restored: {e}")),
    };
    let dir = data_dir();
    for f in ["turks.db-wal", "turks.db-shm"] {
        let _ = std::fs::remove_file(dir.join(f));
    }
    let mut count = 0;
    for i in 0..zip.len() {
        let Ok(mut f) = zip.by_index(i) else { continue };
        let name = f.name().map(|n| n.to_string()).unwrap_or_default();
        if name != "turks.db" && name != "settings.json" {
            continue;
        }
        let mut buf = Vec::new();
        if f.read_to_end(&mut buf).is_ok() && std::fs::write(dir.join(&name), buf).is_ok() {
            count += 1;
        }
    }
    Some(format!("Restored {count} files from backup."))
}
