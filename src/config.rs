//! App settings, kept as settings.json in %APPDATA%\Turks.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub fn data_dir() -> PathBuf {
    let dir = std::env::var_os("TURKS_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("Turks"));
    let _ = std::fs::create_dir_all(&dir);
    dir
}

pub fn cache_dir() -> PathBuf {
    let d = data_dir().join("MediaCover");
    let _ = std::fs::create_dir_all(&d);
    d
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ColonReplacement {
    Delete,
    Dash,
    SpaceDash,
    SpaceDashSpace,
    #[default]
    Smart,
}

impl ColonReplacement {
    pub const ALL: [Self; 5] = [Self::Delete, Self::Dash, Self::SpaceDash, Self::SpaceDashSpace, Self::Smart];
    pub fn label(self) -> &'static str {
        match self {
            Self::Delete => "Delete",
            Self::Dash => "Replace with dash",
            Self::SpaceDash => "Replace with space dash",
            Self::SpaceDashSpace => "Replace with space dash space",
            Self::Smart => "Smart replace",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Propers {
    #[default]
    PreferAndUpgrade,
    DoNotUpgrade,
    DoNotPrefer,
}

impl Propers {
    pub const ALL: [Self; 3] = [Self::PreferAndUpgrade, Self::DoNotUpgrade, Self::DoNotPrefer];
    pub fn label(self) -> &'static str {
        match self {
            Self::PreferAndUpgrade => "Prefer and upgrade",
            Self::DoNotUpgrade => "Do not upgrade automatically",
            Self::DoNotPrefer => "Do not prefer",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RescanAfterRefresh {
    #[default]
    Always,
    AfterManual,
    Never,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileDate {
    #[default]
    None,
    InCinemas,
    PhysicalRelease,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    // --- General
    pub tmdb_api_key: String,
    pub metadata_language: String,
    pub certification_country: String,
    pub api_enabled: bool,
    pub api_port: u16,
    pub api_bind_all: bool,
    pub api_key: String,
    pub url_base: String,
    pub backup_dir: String,
    pub backup_interval_days: i64,
    pub backup_retention_days: i64,
    pub last_backup: i64,
    pub check_updates: bool,
    /// Synology: let the root Task Scheduler task install new versions.
    pub auto_update: bool,
    pub start_with_windows: bool,
    pub close_to_tray: bool,
    pub advanced_mode: bool,
    pub log_debug: bool,
    pub proxy_url: String,
    pub ffprobe_path: String,

    // --- UI
    pub first_day_of_week: u8,
    pub short_date_format: String,
    pub long_date_format: String,
    pub time_24h: bool,
    pub show_relative_dates: bool,
    pub runtime_hours_minutes: bool,
    pub movie_view: u8,
    pub poster_size: f32,
    pub calendar_week_view: bool,

    // --- Media management / naming
    pub rename_movies: bool,
    pub replace_illegal_chars: bool,
    pub colon_replacement: ColonReplacement,
    pub standard_movie_format: String,
    pub movie_folder_format: String,
    pub create_empty_folders: bool,
    pub delete_empty_folders: bool,
    pub skip_free_space_check: bool,
    pub minimum_free_space_mb: u64,
    pub use_hardlinks: bool,
    pub import_extra_files: bool,
    pub extra_file_extensions: String,
    pub unmonitor_deleted_movies: bool,
    pub propers: Propers,
    pub analyse_video_files: bool,
    pub rescan_after_refresh: RescanAfterRefresh,
    pub file_date: FileDate,
    pub recycle_bin: String,
    pub recycle_bin_cleanup_days: i64,
    pub minimum_age_for_import_secs: i64,

    // --- Indexers
    pub minimum_age_minutes: i64,
    pub retention_days: i64,
    pub maximum_size_mb: u64,
    pub rss_sync_interval_minutes: i64,
    pub prefer_indexer_flags: bool,
    pub availability_delay_days: i64,
    pub allow_hardcoded_subs: bool,
    pub whitelisted_hardcoded_subs: String,

    // --- Download clients
    pub completed_download_handling: bool,
    pub redownload_failed: bool,
    pub redownload_failed_from_interactive: bool,
    pub check_downloads_seconds: u64,

    // --- Lists / collections
    pub list_sync_interval_hours: i64,
    pub list_sync_clean_library: String,

    // --- Kodi/Emby metadata
    pub kodi_metadata: bool,
    pub kodi_movie_images: bool,
    pub kodi_url_in_nfo: bool,
    pub kodi_collection_name: bool,
    pub kodi_language: String,

    // --- Remembered choices from the Add Movie window
    pub last_add_profile: i64,
    pub last_add_root: String,
    pub last_add_availability: crate::model::Availability,
    pub last_add_search: bool,

    // --- First-run
    pub setup_done: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            tmdb_api_key: String::new(),
            metadata_language: "en".into(),
            certification_country: "US".into(),
            api_enabled: true,
            api_port: 7878,
            api_bind_all: false,
            api_key: random_key(),
            url_base: String::new(),
            backup_dir: data_dir().join("Backups").display().to_string(),
            backup_interval_days: 7,
            backup_retention_days: 28,
            last_backup: 0,
            check_updates: true,
            auto_update: true,
            start_with_windows: false,
            close_to_tray: true,
            advanced_mode: false,
            log_debug: false,
            proxy_url: String::new(),
            ffprobe_path: String::new(),

            first_day_of_week: 0,
            short_date_format: "%b %-d %Y".into(),
            long_date_format: "%A, %B %-d, %Y".into(),
            time_24h: false,
            show_relative_dates: true,
            runtime_hours_minutes: true,
            movie_view: 0,
            poster_size: 150.0,
            calendar_week_view: false,

            rename_movies: true,
            replace_illegal_chars: true,
            colon_replacement: ColonReplacement::Smart,
            standard_movie_format: "{Movie Title} ({Release Year}) {Quality Full}".into(),
            movie_folder_format: "{Movie Title} ({Release Year})".into(),
            create_empty_folders: false,
            delete_empty_folders: false,
            skip_free_space_check: false,
            minimum_free_space_mb: 100,
            use_hardlinks: true,
            import_extra_files: true,
            extra_file_extensions: "srt,ass,ssa,sub,idx,nfo".into(),
            unmonitor_deleted_movies: false,
            propers: Propers::PreferAndUpgrade,
            analyse_video_files: true,
            rescan_after_refresh: RescanAfterRefresh::Always,
            file_date: FileDate::None,
            recycle_bin: String::new(),
            recycle_bin_cleanup_days: 7,
            minimum_age_for_import_secs: 60,

            minimum_age_minutes: 0,
            retention_days: 0,
            maximum_size_mb: 0,
            rss_sync_interval_minutes: 60,
            prefer_indexer_flags: false,
            availability_delay_days: 0,
            allow_hardcoded_subs: false,
            whitelisted_hardcoded_subs: String::new(),

            completed_download_handling: true,
            redownload_failed: true,
            redownload_failed_from_interactive: false,
            check_downloads_seconds: 60,

            list_sync_interval_hours: 24,
            list_sync_clean_library: "Disabled".into(),

            kodi_metadata: false,
            kodi_movie_images: true,
            kodi_url_in_nfo: false,
            kodi_collection_name: true,
            kodi_language: "en".into(),

            last_add_profile: 0,
            last_add_root: String::new(),
            last_add_availability: crate::model::Availability::Released,
            last_add_search: true,
            setup_done: false,
        }
    }
}

pub fn random_key() -> String {
    use rand::Rng;
    let mut r = rand::rng();
    (0..32).map(|_| format!("{:x}", r.random_range(0..16u8))).collect()
}

impl Config {
    pub fn path() -> PathBuf {
        data_dir().join("settings.json")
    }

    pub fn load() -> Self {
        std::fs::read_to_string(Self::path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let tmp = data_dir().join("settings.json.tmp");
        if let Ok(s) = serde_json::to_string_pretty(self) {
            if std::fs::write(&tmp, s).is_ok() {
                let _ = std::fs::rename(&tmp, Self::path());
            }
        }
    }

    pub fn tmdb_ready(&self) -> bool {
        !self.tmdb_api_key.trim().is_empty()
    }
}
