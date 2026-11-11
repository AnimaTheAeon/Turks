//! The engine: owns the library and settings, runs commands on worker
//! threads and keeps the scheduled tasks ticking.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::{Arc, Mutex, RwLock, RwLockReadGuard};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};

use crate::clients::ClientItem;
use crate::config::Config;
use crate::db::{next_id, Db, Entity, Library};
use crate::indexers::Caps;
use crate::model::*;
use crate::release::Candidate;

#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    RefreshMovies(Vec<Id>),
    RescanMovies(Vec<Id>),
    SearchMovies(Vec<Id>),
    MissingSearch,
    CutoffUnmetSearch,
    RssSync,
    RefreshDownloads,
    ImportListSync,
    RefreshCollections,
    RenameMovies(Vec<Id>),
    Backup,
    Housekeeping,
    CheckHealth,
    InteractiveSearch(Id),
    ScanFolder(String),
    CleanRecycleBin,
}

impl Command {
    pub fn label(&self) -> String {
        match self {
            Self::RefreshMovies(v) if v.is_empty() => "Refresh all movies".into(),
            Self::RefreshMovies(_) => "Refresh movie".into(),
            Self::RescanMovies(v) if v.is_empty() => "Rescan all movie folders".into(),
            Self::RescanMovies(_) => "Rescan movie folder".into(),
            Self::SearchMovies(v) if v.len() == 1 => "Movie search".into(),
            Self::SearchMovies(v) => format!("Search {} movies", v.len()),
            Self::MissingSearch => "Search all missing".into(),
            Self::CutoffUnmetSearch => "Search all cutoff unmet".into(),
            Self::RssSync => "RSS sync".into(),
            Self::RefreshDownloads => "Check downloads".into(),
            Self::ImportListSync => "Import list sync".into(),
            Self::RefreshCollections => "Refresh collections".into(),
            Self::RenameMovies(_) => "Rename files".into(),
            Self::Backup => "Backup".into(),
            Self::Housekeeping => "Housekeeping".into(),
            Self::CheckHealth => "Check health".into(),
            Self::InteractiveSearch(_) => "Interactive search".into(),
            Self::ScanFolder(_) => "Scan downloaded movies".into(),
            Self::CleanRecycleBin => "Clean up recycle bin".into(),
        }
    }
    /// Only one of these runs at a time.
    fn exclusive_key(&self) -> String {
        match self {
            Self::InteractiveSearch(id) => format!("interactive-{id}"),
            Self::SearchMovies(v) => format!("search-{v:?}"),
            Self::RefreshMovies(v) => format!("refresh-{v:?}"),
            Self::RescanMovies(v) => format!("rescan-{v:?}"),
            Self::RenameMovies(v) => format!("rename-{v:?}"),
            Self::ScanFolder(p) => format!("scan-{p}"),
            other => format!("{other:?}"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct RunningCommand {
    pub id: u64,
    pub command: Command,
    pub started: DateTime<Utc>,
    pub finished: Option<DateTime<Utc>>,
    pub message: String,
    pub failed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum HealthLevel {
    Notice,
    Warning,
    Error,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HealthIssue {
    pub source: String,
    pub level: HealthLevel,
    pub message: String,
}

#[derive(Clone, Debug)]
pub struct ScheduledTask {
    pub name: &'static str,
    pub interval: Duration,
    pub command: Command,
    pub last_run: Option<DateTime<Utc>>,
    pub last_duration: Option<Duration>,
}

/// Results of an interactive search, kept until the next one.
#[derive(Clone, Debug, Default)]
#[allow(dead_code)]
pub struct InteractiveResult {
    pub running: bool,
    pub candidates: Vec<Candidate>,
    pub errors: Vec<String>,
    pub finished: Option<DateTime<Utc>>,
}

#[derive(Default)]
pub struct Status {
    pub commands: VecDeque<RunningCommand>,
    pub health: Vec<HealthIssue>,
    pub toasts: VecDeque<(String, Instant)>,
    pub tasks: Vec<ScheduledTask>,
    pub interactive: HashMap<Id, InteractiveResult>,
    pub client_items: HashMap<Id, (Instant, Result<Vec<ClientItem>, String>)>,
    pub indexer_failures: HashMap<Id, (Instant, String, u32)>,
    pub next_command_id: u64,
    pub started: Option<Instant>,
}

pub struct Inner {
    pub db: Db,
    pub lib: RwLock<Library>,
    pub cfg: RwLock<Config>,
    pub status: Mutex<Status>,
    pub caps: Mutex<HashMap<Id, (Caps, Instant)>>,
    pub repaint: Mutex<Option<Box<dyn Fn() + Send + Sync>>>,
    pub shutting_down: std::sync::atomic::AtomicBool,
}

#[derive(Clone)]
pub struct Core(pub Arc<Inner>);

impl std::ops::Deref for Core {
    type Target = Inner;
    fn deref(&self) -> &Inner {
        &self.0
    }
}

impl Core {
    pub fn open() -> Result<Core, String> {
        let db = Db::open(&crate::config::data_dir().join("turks.db"))?;
        let mut lib = db.load_library();
        let mut cfg = Config::load();
        if cfg.calendar_key.is_empty() {
            cfg.calendar_key = crate::config::random_key();
            let _ = cfg.save();
        }
        // First run: Radarr's defaults.
        if lib.profiles.is_empty() {
            let mut list = crate::quality::default_profiles();
            for (i, p) in list.iter_mut().enumerate() {
                p.id = i as Id + 1;
                db.put(p);
                lib.profiles.insert(p.id, p.clone());
            }
        }
        if lib.sizes.is_empty() {
            for s in crate::quality::default_sizes() {
                db.put(&s);
                lib.sizes.insert(s.quality as Id, s);
            }
        }
        if lib.delay_profiles.is_empty() {
            let d = DelayProfile { id: 1, ..Default::default() };
            db.put(&d);
            lib.delay_profiles.insert(1, d);
        }
        let core = Core(Arc::new(Inner {
            db,
            lib: RwLock::new(lib),
            cfg: RwLock::new(cfg.clone()),
            status: Mutex::new(Status { started: Some(Instant::now()), ..Default::default() }),
            caps: Mutex::new(HashMap::new()),
            repaint: Mutex::new(None),
            shutting_down: std::sync::atomic::AtomicBool::new(false),
        }));
        core.setup_tasks();
        cfg.save();
        Ok(core)
    }

    // ------------------------------------------------------------ access

    pub fn cfg(&self) -> Config {
        self.cfg.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn set_cfg(&self, c: Config) {
        c.save();
        *self.cfg.write().unwrap_or_else(|e| e.into_inner()) = c;
    }

    pub fn lib(&self) -> RwLockReadGuard<'_, Library> {
        self.lib.read().unwrap_or_else(|e| e.into_inner())
    }

    pub fn with_lib<R>(&self, f: impl FnOnce(&mut Library) -> R) -> R {
        let mut l = self.lib.write().unwrap_or_else(|e| e.into_inner());
        f(&mut l)
    }

    pub fn status(&self) -> std::sync::MutexGuard<'_, Status> {
        self.status.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn tmdb(&self) -> crate::tmdb::Tmdb {
        let c = self.cfg();
        crate::tmdb::Tmdb::new(&c.tmdb_api_key, &c.metadata_language, &c.certification_country)
    }

    /// The window asks to be redrawn when something changes.
    pub fn set_repaint(&self, f: impl Fn() + Send + Sync + 'static) {
        *self.repaint.lock().unwrap() = Some(Box::new(f));
    }

    pub fn repaint(&self) {
        if let Some(f) = self.repaint.lock().unwrap().as_ref() {
            f();
        }
    }

    pub fn toast(&self, msg: impl Into<String>) {
        let msg = msg.into();
        crate::applog::info(msg.clone());
        let mut s = self.status();
        s.toasts.push_back((msg, Instant::now()));
        while s.toasts.len() > 4 {
            s.toasts.pop_front();
        }
        drop(s);
        self.repaint();
    }

    /// Insert or update; assigns an id when it's new. Returns the saved item.
    pub fn save<T: Entity>(&self, mut item: T) -> T {
        self.with_lib(|lib| {
            if item.id() == 0 {
                item.set_id(next_id::<T>(lib));
            }
            T::map_mut(lib).insert(item.id(), item.clone());
        });
        self.db.put(&item);
        if T::KIND == "format" {
            crate::decision::formats_changed();
        }
        self.repaint();
        item
    }

    pub fn save_many<T: Entity>(&self, items: Vec<T>) -> Vec<T> {
        let items = self.with_lib(|lib| {
            let mut out = Vec::new();
            for mut item in items {
                if item.id() == 0 {
                    item.set_id(next_id::<T>(lib));
                }
                T::map_mut(lib).insert(item.id(), item.clone());
                out.push(item);
            }
            out
        });
        self.db.put_many(&items);
        if T::KIND == "format" {
            crate::decision::formats_changed();
        }
        self.repaint();
        items
    }

    pub fn remove<T: Entity>(&self, id: Id) -> Option<T> {
        let r = self.with_lib(|lib| T::map_mut(lib).remove(&id));
        self.db.delete::<T>(id);
        if T::KIND == "format" {
            crate::decision::formats_changed();
        }
        self.repaint();
        r
    }

    pub fn history(&self, mut h: History) {
        if h.date.is_none() {
            h.date = Some(Utc::now());
        }
        self.db.add_history(&h);
    }

    pub fn movie(&self, id: Id) -> Option<Movie> {
        self.lib().movies.get(&id).cloned()
    }

    // ------------------------------------------------------------ commands

    pub fn is_running(&self, cmd: &Command) -> bool {
        let key = cmd.exclusive_key();
        self.status().commands.iter().any(|c| c.finished.is_none() && c.command.exclusive_key() == key)
    }

    pub fn running_commands(&self) -> Vec<RunningCommand> {
        self.status().commands.iter().filter(|c| c.finished.is_none()).cloned().collect()
    }

    /// Runs a command on a worker thread (skipped if the same one is running).
    pub fn run(&self, cmd: Command) {
        if self.is_running(&cmd) {
            return;
        }
        let id = {
            let mut s = self.status();
            s.next_command_id += 1;
            let id = s.next_command_id;
            s.commands.push_back(RunningCommand { id, command: cmd.clone(), started: Utc::now(), finished: None, message: String::new(), failed: false });
            while s.commands.len() > 60 {
                if let Some(pos) = s.commands.iter().position(|c| c.finished.is_some()) {
                    s.commands.remove(pos);
                } else {
                    break;
                }
            }
            id
        };
        self.repaint();
        let core = self.clone();
        std::thread::Builder::new()
            .name(cmd.label())
            .spawn(move || {
                let started = Instant::now();
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| core.execute(&cmd, id)));
                let (failed, message) = match result {
                    Ok(Ok(msg)) => (false, msg),
                    Ok(Err(e)) => {
                        crate::applog::error(format!("{} failed: {e}", cmd.label()));
                        (true, e)
                    }
                    Err(_) => {
                        crate::applog::error(format!("{} crashed", cmd.label()));
                        (true, "Crashed; see the log".into())
                    }
                };
                {
                    let mut s = core.status();
                    if let Some(c) = s.commands.iter_mut().find(|c| c.id == id) {
                        c.finished = Some(Utc::now());
                        c.failed = failed;
                        if !message.is_empty() {
                            c.message = message;
                        }
                    }
                    if let Some(t) = s.tasks.iter_mut().find(|t| t.command == cmd) {
                        t.last_run = Some(Utc::now());
                        t.last_duration = Some(started.elapsed());
                    }
                }
                core.save_task_times();
                core.repaint();
            })
            .ok();
    }

    pub fn progress(&self, command_id: u64, msg: impl Into<String>) {
        let msg = msg.into();
        if let Some(c) = self.status().commands.iter_mut().find(|c| c.id == command_id) {
            c.message = msg;
        }
        self.repaint();
    }

    fn execute(&self, cmd: &Command, cid: u64) -> Result<String, String> {
        match cmd {
            Command::RefreshMovies(ids) => crate::library::refresh_movies(self, ids, cid),
            Command::RescanMovies(ids) => crate::library::rescan_movies(self, ids, cid),
            Command::SearchMovies(ids) => crate::search::search_movies(self, ids, cid),
            Command::MissingSearch => {
                let ids: Vec<Id> = {
                    let lib = self.lib();
                    let delay = self.cfg().availability_delay_days;
                    lib.movies.values().filter(|m| m.monitored && m.movie_file_id.is_none() && m.is_available(delay)).map(|m| m.id).collect()
                };
                crate::search::search_movies(self, &ids, cid)
            }
            Command::CutoffUnmetSearch => {
                let ids = crate::library::cutoff_unmet(self).into_iter().map(|m| m.id).collect::<Vec<_>>();
                crate::search::search_movies(self, &ids, cid)
            }
            Command::RssSync => crate::search::rss_sync(self, cid),
            Command::RefreshDownloads => crate::tracked::refresh(self, cid),
            Command::ImportListSync => crate::lists::sync_all(self, cid),
            Command::RefreshCollections => crate::library::refresh_collections(self, cid),
            Command::RenameMovies(ids) => crate::library::rename_movies(self, ids, cid),
            Command::Backup => crate::backup::create(self, "scheduled").map(|p| format!("Saved {}", p.display())),
            Command::Housekeeping => crate::library::housekeeping(self),
            Command::CheckHealth => {
                crate::health::check(self);
                Ok(String::new())
            }
            Command::InteractiveSearch(id) => crate::search::interactive(self, *id),
            Command::ScanFolder(p) => crate::import::scan_folder(self, p, cid),
            Command::CleanRecycleBin => crate::library::clean_recycle_bin(self),
        }
    }

    // ------------------------------------------------------------ scheduler

    fn setup_tasks(&self) {
        let cfg = self.cfg();
        let mins = |m: u64| Duration::from_secs(m * 60);
        let saved: BTreeMap<String, String> = self
            .db
            .meta("task_times")
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        let last = |name: &str| saved.get(name).and_then(|t| DateTime::parse_from_rfc3339(t).ok()).map(|d| d.with_timezone(&Utc));
        let mut tasks = vec![
            ScheduledTask { name: "Check Downloads", interval: Duration::from_secs(cfg.check_downloads_seconds.max(15)), command: Command::RefreshDownloads, last_run: None, last_duration: None },
            ScheduledTask { name: "RSS Sync", interval: mins(cfg.rss_sync_interval_minutes.max(10) as u64), command: Command::RssSync, last_run: last("RSS Sync"), last_duration: None },
            ScheduledTask { name: "Import List Sync", interval: mins(cfg.list_sync_interval_hours.max(6) as u64 * 60), command: Command::ImportListSync, last_run: last("Import List Sync"), last_duration: None },
            ScheduledTask { name: "Refresh Movies", interval: mins(24 * 60), command: Command::RefreshMovies(vec![]), last_run: last("Refresh Movies"), last_duration: None },
            ScheduledTask { name: "Refresh Collections", interval: mins(24 * 60), command: Command::RefreshCollections, last_run: last("Refresh Collections"), last_duration: None },
            ScheduledTask { name: "Housekeeping", interval: mins(24 * 60), command: Command::Housekeeping, last_run: last("Housekeeping"), last_duration: None },
            ScheduledTask { name: "Check Health", interval: mins(6 * 60), command: Command::CheckHealth, last_run: None, last_duration: None },
            ScheduledTask { name: "Clean Recycle Bin", interval: mins(24 * 60), command: Command::CleanRecycleBin, last_run: last("Clean Recycle Bin"), last_duration: None },
            ScheduledTask {
                name: "Backup",
                interval: mins(cfg.backup_interval_days.max(1) as u64 * 24 * 60),
                command: Command::Backup,
                last_run: last("Backup"),
                last_duration: None,
            },
        ];
        // Stagger the first runs a little after startup.
        for t in tasks.iter_mut() {
            if t.last_run.is_none() && !matches!(t.command, Command::RefreshDownloads | Command::CheckHealth) {
                t.last_run = Some(Utc::now() - chrono::Duration::from_std(t.interval).unwrap_or_default() + chrono::Duration::minutes(3));
            }
        }
        self.status().tasks = tasks;
    }

    pub fn update_task_intervals(&self) {
        let cfg = self.cfg();
        let mut s = self.status();
        for t in s.tasks.iter_mut() {
            match t.command {
                Command::RssSync => t.interval = Duration::from_secs(cfg.rss_sync_interval_minutes.max(10) as u64 * 60),
                Command::RefreshDownloads => t.interval = Duration::from_secs(cfg.check_downloads_seconds.max(15)),
                Command::ImportListSync => t.interval = Duration::from_secs(cfg.list_sync_interval_hours.max(6) as u64 * 3600),
                Command::Backup => t.interval = Duration::from_secs(cfg.backup_interval_days.max(1) as u64 * 86400),
                _ => {}
            }
        }
    }

    fn save_task_times(&self) {
        let map: BTreeMap<String, String> = self
            .status()
            .tasks
            .iter()
            .filter_map(|t| t.last_run.map(|d| (t.name.to_string(), d.to_rfc3339())))
            .collect();
        if let Ok(s) = serde_json::to_string(&map) {
            self.db.set_meta("task_times", &s);
        }
    }

    pub fn next_run(t: &ScheduledTask) -> DateTime<Utc> {
        t.last_run.map(|l| l + chrono::Duration::from_std(t.interval).unwrap_or_default()).unwrap_or_else(Utc::now)
    }

    /// Starts the scheduler and the API server.
    pub fn start(&self) {
        let core = self.clone();
        std::thread::Builder::new()
            .name("scheduler".into())
            .spawn(move || loop {
                if core.shutting_down.load(std::sync::atomic::Ordering::SeqCst) {
                    return;
                }
                let due: Vec<Command> = {
                    let s = core.status();
                    let now = Utc::now();
                    s.tasks.iter().filter(|t| Core::next_run(t) <= now).map(|t| t.command.clone()).collect()
                };
                for cmd in due {
                    let no_tmdb = !core.cfg().tmdb_ready();
                    if no_tmdb && matches!(cmd, Command::RefreshMovies(_) | Command::ImportListSync | Command::RefreshCollections) {
                        if let Some(t) = core.status().tasks.iter_mut().find(|t| t.command == cmd) {
                            t.last_run = Some(Utc::now());
                        }
                        continue;
                    }
                    core.run(cmd);
                }
                std::thread::sleep(Duration::from_secs(5));
            })
            .ok();
        crate::api::start(self.clone());
    }

    pub fn shutdown(&self) {
        self.shutting_down.store(true, std::sync::atomic::Ordering::SeqCst);
        self.save_task_times();
    }

    // ------------------------------------------------------------ indexer caps & backoff

    pub fn caps_for(&self, ix: &Indexer) -> Caps {
        if let Some((c, at)) = self.caps.lock().unwrap().get(&ix.id) {
            if at.elapsed() < Duration::from_secs(6 * 3600) {
                return c.clone();
            }
        }
        match crate::indexers::caps(ix) {
            Ok(c) => {
                self.caps.lock().unwrap().insert(ix.id, (c.clone(), Instant::now()));
                c
            }
            Err(_) => crate::indexers::Caps { q: true, ..Default::default() },
        }
    }

    /// Radarr backs off indexers that keep failing.
    pub fn indexer_blocked(&self, id: Id) -> Option<String> {
        let s = self.status();
        let (at, msg, n) = s.indexer_failures.get(&id)?;
        let wait = Duration::from_secs((60 * 2u64.pow((*n).min(8))).min(6 * 3600));
        if at.elapsed() < wait {
            Some(msg.clone())
        } else {
            None
        }
    }

    pub fn indexer_result(&self, id: Id, err: Option<String>) {
        let mut s = self.status();
        match err {
            None => {
                s.indexer_failures.remove(&id);
            }
            Some(e) => {
                let n = s.indexer_failures.get(&id).map(|x| x.2 + 1).unwrap_or(0);
                s.indexer_failures.insert(id, (Instant::now(), e, n));
            }
        }
    }
}

pub fn human_rate_eta(secs: Option<i64>) -> String {
    match secs {
        None => String::new(),
        Some(s) if s < 60 => format!("{s}s"),
        Some(s) if s < 3600 => format!("{}m", s / 60),
        Some(s) if s < 86400 => format!("{}h {}m", s / 3600, (s / 60) % 60),
        Some(s) => format!("{}d {}h", s / 86400, (s / 3600) % 24),
    }
}
