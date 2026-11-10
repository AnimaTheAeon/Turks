//! Everything Turks keeps: movies, files, profiles, providers and history.
//! Field names follow Radarr's where they map one to one.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub type Id = i64;

// ---------------------------------------------------------------- movies

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
pub enum MovieStatus {
    #[default]
    Tba,
    Announced,
    InCinemas,
    Released,
    Deleted,
}

impl MovieStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Tba => "TBA",
            Self::Announced => "Announced",
            Self::InCinemas => "In Cinemas",
            Self::Released => "Released",
            Self::Deleted => "Removed from TMDb",
        }
    }
}

/// When a movie counts as "available" to search for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
pub enum Availability {
    Announced,
    InCinemas,
    #[default]
    Released,
}

impl Availability {
    pub const ALL: [Availability; 3] = [Self::Announced, Self::InCinemas, Self::Released];
    pub fn label(self) -> &'static str {
        match self {
            Self::Announced => "Announced",
            Self::InCinemas => "In Cinemas",
            Self::Released => "Released",
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Ratings {
    pub tmdb: f32,
    pub tmdb_votes: i64,
    pub imdb: f32,
    pub imdb_votes: i64,
    pub rotten_tomatoes: i32,
    pub metacritic: i32,
    pub trakt: f32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CollectionRef {
    pub tmdb_id: i64,
    pub title: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Credit {
    pub name: String,
    pub role: String,
    pub department: String,
    pub person_id: i64,
    pub profile_path: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Movie {
    pub id: Id,
    pub tmdb_id: i64,
    pub imdb_id: String,
    pub title: String,
    pub original_title: String,
    pub original_language: String,
    pub sort_title: String,
    pub clean_title: String,
    pub alt_titles: Vec<String>,
    pub year: i32,
    pub overview: String,
    pub runtime: i32,
    pub genres: Vec<String>,
    pub keywords: Vec<String>,
    pub studio: String,
    pub certification: String,
    pub status: MovieStatus,
    pub in_cinemas: Option<NaiveDate>,
    pub digital_release: Option<NaiveDate>,
    pub physical_release: Option<NaiveDate>,
    pub ratings: Ratings,
    pub popularity: f32,
    pub poster_path: String,
    pub fanart_path: String,
    pub youtube_trailer: String,
    pub website: String,
    pub collection: Option<CollectionRef>,
    pub cast: Vec<Credit>,
    pub crew: Vec<Credit>,
    pub recommendations: Vec<i64>,

    pub monitored: bool,
    pub minimum_availability: Availability,
    pub quality_profile_id: Id,
    pub root_folder: String,
    pub path: String,
    pub tags: Vec<Id>,
    pub added: Option<DateTime<Utc>>,
    pub last_info_sync: Option<DateTime<Utc>>,
    pub last_search: Option<DateTime<Utc>>,
    pub movie_file_id: Option<Id>,
}

impl Movie {
    pub fn display(&self) -> String {
        if self.year > 0 {
            format!("{} ({})", self.title, self.year)
        } else {
            self.title.clone()
        }
    }

    /// The release date that makes a movie "available" for its setting.
    pub fn available_date(&self, delay_days: i64) -> Option<NaiveDate> {
        let base = match self.minimum_availability {
            Availability::Announced => return Some(NaiveDate::MIN),
            Availability::InCinemas => self.in_cinemas.or(self.digital_release).or(self.physical_release),
            Availability::Released => {
                let d = [self.digital_release, self.physical_release].into_iter().flatten().min();
                d.or_else(|| self.in_cinemas.map(|c| c + chrono::Duration::days(90)))
            }
        }?;
        Some(base + chrono::Duration::days(delay_days))
    }

    pub fn is_available(&self, delay_days: i64) -> bool {
        if self.minimum_availability == Availability::Announced {
            return true;
        }
        if self.minimum_availability == Availability::InCinemas && self.status >= MovieStatus::InCinemas {
            return true;
        }
        if self.status == MovieStatus::Released && self.minimum_availability <= Availability::Released
            && self.digital_release.is_none() && self.physical_release.is_none() && self.in_cinemas.is_none()
        {
            return true;
        }
        let today = chrono::Local::now().date_naive();
        self.available_date(delay_days).map(|d| d <= today).unwrap_or(false)
    }

    pub fn initials(&self) -> String {
        self.title
            .split(|c: char| c.is_whitespace() || c == ':' || c == '-')
            .filter(|w| !w.is_empty() && !matches!(w.to_ascii_lowercase().as_str(), "the" | "a" | "an" | "of"))
            .take(2)
            .filter_map(|w| w.chars().next())
            .collect::<String>()
            .to_uppercase()
    }
}

// ---------------------------------------------------------------- qualities

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Revision {
    pub version: i32,
    pub real: i32,
    pub is_repack: bool,
}

impl Revision {
    pub fn new() -> Self {
        Self { version: 1, real: 0, is_repack: false }
    }
    pub fn cmp_key(&self) -> (i32, i32) {
        (self.real, self.version)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct QualityModel {
    pub quality: i32,
    pub revision: Revision,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MediaInfo {
    pub video_codec: String,
    pub video_bit_depth: i32,
    pub video_dynamic_range: String,
    pub resolution: String,
    pub audio_codec: String,
    pub audio_channels: f32,
    pub audio_languages: Vec<String>,
    pub subtitles: Vec<String>,
    pub runtime_secs: i64,
    pub from_ffprobe: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MovieFile {
    pub id: Id,
    pub movie_id: Id,
    pub relative_path: String,
    pub path: String,
    pub size: u64,
    pub quality: QualityModel,
    pub languages: Vec<String>,
    pub release_group: String,
    pub edition: String,
    pub scene_name: String,
    pub original_path: String,
    pub indexer_flags: Vec<String>,
    pub date_added: Option<DateTime<Utc>>,
    pub media_info: MediaInfo,
}

// ---------------------------------------------------------------- profiles

/// One row in a profile: a single quality or a named group of them.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ProfileItem {
    /// Group id (1000+) or the single quality's id.
    pub id: i32,
    /// Set for groups.
    pub name: String,
    pub qualities: Vec<i32>,
    pub allowed: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct FormatScore {
    pub format_id: Id,
    pub score: i32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct QualityProfile {
    pub id: Id,
    pub name: String,
    pub upgrade_allowed: bool,
    /// Item id (quality id or group id) where upgrades stop.
    pub cutoff: i32,
    /// Lowest quality first, best last (same order Radarr stores them).
    pub items: Vec<ProfileItem>,
    pub min_format_score: i32,
    pub cutoff_format_score: i32,
    pub min_upgrade_format_score: i32,
    pub format_items: Vec<FormatScore>,
    /// "Any", "Original" or a language name.
    pub language: String,
}

impl QualityProfile {
    /// Position of a quality in the profile (higher is better), if listed.
    pub fn rank(&self, quality: i32) -> Option<usize> {
        self.items.iter().position(|i| i.qualities.contains(&quality))
    }
    pub fn allows(&self, quality: i32) -> bool {
        self.items.iter().any(|i| i.allowed && i.qualities.contains(&quality))
    }
    pub fn cutoff_rank(&self) -> usize {
        self.items.iter().position(|i| i.id == self.cutoff).unwrap_or(self.items.len().saturating_sub(1))
    }
    pub fn score_of(&self, format_ids: &[Id]) -> i32 {
        self.format_items.iter().filter(|f| format_ids.contains(&f.format_id)).map(|f| f.score).sum()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct QualitySize {
    pub quality: i32,
    pub title: String,
    /// MB per minute of runtime.
    pub min: f64,
    pub preferred: Option<f64>,
    pub max: Option<f64>,
}

impl Default for QualitySize {
    fn default() -> Self {
        Self { quality: 0, title: String::new(), min: 0.0, preferred: None, max: None }
    }
}

// ---------------------------------------------------------------- custom formats

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum CfKind {
    ReleaseTitle { pattern: String },
    Source { source: i32 },
    Resolution { resolution: i32 },
    QualityModifier { modifier: i32 },
    Size { min_gb: f64, max_gb: f64 },
    Language { language: String, except: bool },
    ReleaseGroup { pattern: String },
    Edition { pattern: String },
    IndexerFlag { flag: String },
    Year { min: i32, max: i32 },
}

impl CfKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::ReleaseTitle { .. } => "Release Title",
            Self::Source { .. } => "Source",
            Self::Resolution { .. } => "Resolution",
            Self::QualityModifier { .. } => "Quality Modifier",
            Self::Size { .. } => "Size",
            Self::Language { .. } => "Language",
            Self::ReleaseGroup { .. } => "Release Group",
            Self::Edition { .. } => "Edition",
            Self::IndexerFlag { .. } => "Indexer Flag",
            Self::Year { .. } => "Year",
        }
    }
    pub fn templates() -> Vec<CfKind> {
        vec![
            Self::ReleaseTitle { pattern: String::new() },
            Self::Source { source: 6 },
            Self::Resolution { resolution: 1080 },
            Self::QualityModifier { modifier: 5 },
            Self::Size { min_gb: 0.0, max_gb: 100.0 },
            Self::Language { language: "English".into(), except: false },
            Self::ReleaseGroup { pattern: String::new() },
            Self::Edition { pattern: String::new() },
            Self::IndexerFlag { flag: "freeleech".into() },
            Self::Year { min: 1900, max: 2100 },
        ]
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CfSpec {
    pub name: String,
    pub kind: CfKind,
    pub negate: bool,
    pub required: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CustomFormat {
    pub id: Id,
    pub name: String,
    pub include_when_renaming: bool,
    pub specs: Vec<CfSpec>,
}

// ---------------------------------------------------------------- delay / release profiles

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Protocol {
    #[default]
    Torrent,
    Usenet,
}

impl Protocol {
    pub fn label(self) -> &'static str {
        match self {
            Self::Torrent => "Torrent",
            Self::Usenet => "Usenet",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct DelayProfile {
    pub id: Id,
    pub enable_usenet: bool,
    pub enable_torrent: bool,
    pub preferred: Protocol,
    pub usenet_delay: i64,
    pub torrent_delay: i64,
    pub bypass_if_highest_quality: bool,
    pub bypass_if_above_score: bool,
    pub minimum_score: i32,
    pub order: i32,
    pub tags: Vec<Id>,
}

impl Default for DelayProfile {
    fn default() -> Self {
        Self {
            id: 0,
            enable_usenet: true,
            enable_torrent: true,
            preferred: Protocol::Usenet,
            usenet_delay: 0,
            torrent_delay: 0,
            bypass_if_highest_quality: true,
            bypass_if_above_score: false,
            minimum_score: 0,
            order: i32::MAX,
            tags: vec![],
        }
    }
}

/// Radarr's "Restrictions": words a release must or must not contain.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ReleaseProfile {
    pub id: Id,
    pub name: String,
    pub enabled: bool,
    pub required: Vec<String>,
    pub ignored: Vec<String>,
    /// 0 = any indexer.
    pub indexer_id: Id,
    pub tags: Vec<Id>,
}

// ---------------------------------------------------------------- providers

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum IndexerKind {
    #[default]
    Torznab,
    Newznab,
}

impl IndexerKind {
    pub fn protocol(self) -> Protocol {
        match self {
            Self::Torznab => Protocol::Torrent,
            Self::Newznab => Protocol::Usenet,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Indexer {
    pub id: Id,
    pub name: String,
    pub kind: IndexerKind,
    pub url: String,
    pub api_path: String,
    pub api_key: String,
    pub categories: Vec<i32>,
    pub extra_params: String,
    pub enable_rss: bool,
    pub enable_auto_search: bool,
    pub enable_interactive_search: bool,
    pub priority: i32,
    pub minimum_seeders: i32,
    pub seed_ratio: Option<f64>,
    pub seed_time: Option<i64>,
    pub required_flags: Vec<String>,
    pub multi_languages: Vec<String>,
    pub download_client_id: Id,
    pub tags: Vec<Id>,
    /// Set by Prowlarr sync, so a later sync can update or remove it.
    pub prowlarr_id: Option<i64>,
}

impl Default for Indexer {
    fn default() -> Self {
        Self {
            id: 0,
            name: String::new(),
            kind: IndexerKind::Torznab,
            url: String::new(),
            api_path: "/api".into(),
            api_key: String::new(),
            categories: vec![2000, 2010, 2020, 2030, 2040, 2045, 2050, 2060],
            extra_params: String::new(),
            enable_rss: true,
            enable_auto_search: true,
            enable_interactive_search: true,
            priority: 25,
            minimum_seeders: 1,
            seed_ratio: None,
            seed_time: None,
            required_flags: vec![],
            multi_languages: vec![],
            download_client_id: 0,
            tags: vec![],
            prowlarr_id: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClientKind {
    #[default]
    Transmission,
    QBittorrent,
    Deluge,
    Sabnzbd,
    Nzbget,
    TorrentBlackhole,
    UsenetBlackhole,
    DownloadStation,
}

impl ClientKind {
    pub const ALL: [ClientKind; 8] = [
        Self::Transmission,
        Self::QBittorrent,
        Self::Deluge,
        Self::Sabnzbd,
        Self::Nzbget,
        Self::TorrentBlackhole,
        Self::UsenetBlackhole,
        Self::DownloadStation,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Transmission => "Transmission / Lifestream",
            Self::QBittorrent => "qBittorrent",
            Self::Deluge => "Deluge",
            Self::Sabnzbd => "SABnzbd",
            Self::Nzbget => "NZBGet",
            Self::TorrentBlackhole => "Torrent Blackhole",
            Self::UsenetBlackhole => "Usenet Blackhole",
            Self::DownloadStation => "Synology Download Station",
        }
    }
    pub fn protocol(self) -> Protocol {
        match self {
            Self::Transmission | Self::QBittorrent | Self::Deluge | Self::TorrentBlackhole | Self::DownloadStation => Protocol::Torrent,
            _ => Protocol::Usenet,
        }
    }
    /// Download Station takes both torrents and NZBs.
    pub fn supports(self, p: Protocol) -> bool {
        self == Self::DownloadStation || self.protocol() == p
    }
    pub fn default_port(self) -> u16 {
        match self {
            Self::Transmission => 9091,
            Self::QBittorrent => 8080,
            Self::Deluge => 8112,
            Self::Sabnzbd => 8080,
            Self::Nzbget => 6789,
            Self::DownloadStation => 5000,
            _ => 0,
        }
    }
    pub fn is_blackhole(self) -> bool {
        matches!(self, Self::TorrentBlackhole | Self::UsenetBlackhole)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct DownloadClient {
    pub id: Id,
    pub name: String,
    pub kind: ClientKind,
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    pub use_ssl: bool,
    pub url_base: String,
    pub username: String,
    pub password: String,
    pub api_key: String,
    pub category: String,
    pub directory: String,
    pub priority: i32,
    pub add_paused: bool,
    pub remove_completed: bool,
    pub remove_failed: bool,
    /// Blackhole: where .torrent / .nzb files are dropped.
    pub drop_folder: String,
    /// Blackhole: where finished downloads appear.
    pub watch_folder: String,
    pub save_magnet_files: bool,
    pub tags: Vec<Id>,
}

impl Default for DownloadClient {
    fn default() -> Self {
        Self {
            id: 0,
            name: String::new(),
            kind: ClientKind::Transmission,
            enabled: true,
            host: "localhost".into(),
            port: 9091,
            use_ssl: false,
            url_base: String::new(),
            username: String::new(),
            password: String::new(),
            api_key: String::new(),
            category: "turks".into(),
            directory: String::new(),
            priority: 1,
            add_paused: false,
            remove_completed: true,
            remove_failed: true,
            drop_folder: String::new(),
            watch_folder: String::new(),
            save_magnet_files: true,
            tags: vec![],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum NotifyKind {
    Discord { webhook: String, username: String },
    Telegram { bot_token: String, chat_id: String, silent: bool },
    Pushover { user_key: String, api_token: String, devices: String, priority: i32 },
    Ntfy { server: String, topic: String, token: String, priority: i32 },
    Gotify { server: String, app_token: String, priority: i32 },
    Webhook { url: String, method_put: bool, username: String, password: String },
    Email { server: String, port: u16, require_tls: bool, username: String, password: String, from: String, to: String },
    Plex { host: String, port: u16, token: String, use_ssl: bool },
    Emby { host: String, port: u16, api_key: String, use_ssl: bool, jellyfin: bool },
    Kodi { host: String, port: u16, username: String, password: String },
    CustomScript { path: String, arguments: String },
    Slack { webhook: String, channel: String },
    Pushbullet { api_key: String, device: String },
    Apprise { server: String, urls: String, tag: String },
}

impl NotifyKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Discord { .. } => "Discord",
            Self::Telegram { .. } => "Telegram",
            Self::Pushover { .. } => "Pushover",
            Self::Ntfy { .. } => "ntfy",
            Self::Gotify { .. } => "Gotify",
            Self::Webhook { .. } => "Webhook",
            Self::Email { .. } => "Email",
            Self::Plex { .. } => "Plex Media Server",
            Self::Emby { jellyfin: true, .. } => "Jellyfin",
            Self::Emby { .. } => "Emby",
            Self::Kodi { .. } => "Kodi",
            Self::CustomScript { .. } => "Custom Script",
            Self::Slack { .. } => "Slack",
            Self::Pushbullet { .. } => "Pushbullet",
            Self::Apprise { .. } => "Apprise",
        }
    }
    pub fn templates() -> Vec<NotifyKind> {
        vec![
            Self::Discord { webhook: String::new(), username: "Turks".into() },
            Self::Telegram { bot_token: String::new(), chat_id: String::new(), silent: false },
            Self::Pushover { user_key: String::new(), api_token: String::new(), devices: String::new(), priority: 0 },
            Self::Ntfy { server: "https://ntfy.sh".into(), topic: String::new(), token: String::new(), priority: 3 },
            Self::Gotify { server: String::new(), app_token: String::new(), priority: 5 },
            Self::Webhook { url: String::new(), method_put: false, username: String::new(), password: String::new() },
            Self::Email { server: String::new(), port: 587, require_tls: true, username: String::new(), password: String::new(), from: String::new(), to: String::new() },
            Self::Plex { host: "localhost".into(), port: 32400, token: String::new(), use_ssl: false },
            Self::Emby { host: "localhost".into(), port: 8096, api_key: String::new(), use_ssl: false, jellyfin: true },
            Self::Emby { host: "localhost".into(), port: 8096, api_key: String::new(), use_ssl: false, jellyfin: false },
            Self::Kodi { host: "localhost".into(), port: 8080, username: "kodi".into(), password: String::new() },
            Self::CustomScript { path: String::new(), arguments: String::new() },
            Self::Slack { webhook: String::new(), channel: String::new() },
            Self::Pushbullet { api_key: String::new(), device: String::new() },
            Self::Apprise { server: "http://localhost:8000".into(), urls: String::new(), tag: String::new() },
        ]
    }
    /// Media servers refresh their library instead of showing a message.
    pub fn is_media_server(&self) -> bool {
        matches!(self, Self::Plex { .. } | Self::Emby { .. } | Self::Kodi { .. })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Notification {
    pub id: Id,
    pub name: String,
    pub kind: NotifyKind,
    pub on_grab: bool,
    pub on_download: bool,
    pub on_upgrade: bool,
    pub on_rename: bool,
    pub on_movie_added: bool,
    pub on_movie_delete: bool,
    pub on_movie_file_delete: bool,
    pub on_health_issue: bool,
    pub on_health_restored: bool,
    pub include_health_warnings: bool,
    pub on_manual_interaction: bool,
    pub on_app_update: bool,
    pub tags: Vec<Id>,
}

impl Default for Notification {
    fn default() -> Self {
        Self {
            id: 0,
            name: String::new(),
            kind: NotifyKind::Discord { webhook: String::new(), username: "Turks".into() },
            on_grab: true,
            on_download: true,
            on_upgrade: true,
            on_rename: false,
            on_movie_added: false,
            on_movie_delete: false,
            on_movie_file_delete: false,
            on_health_issue: true,
            on_health_restored: false,
            include_health_warnings: false,
            on_manual_interaction: true,
            on_app_update: true,
            tags: vec![],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ListKind {
    TmdbList { list_id: String },
    TmdbCollection { collection_id: String },
    TmdbPerson { person_id: String, cast: bool, crew: bool },
    TmdbCompany { company_id: String },
    TmdbKeyword { keyword_id: String },
    TmdbPopular { which: String },
    ImdbList { list_id: String },
    TraktList { client_id: String, username: String, list: String },
    RssList { url: String },
    PlexWatchlist { rss_url: String },
    TurksList { url: String, api_key: String },
}

impl ListKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::TmdbList { .. } => "TMDb List",
            Self::TmdbCollection { .. } => "TMDb Collection",
            Self::TmdbPerson { .. } => "TMDb Person",
            Self::TmdbCompany { .. } => "TMDb Company",
            Self::TmdbKeyword { .. } => "TMDb Keyword",
            Self::TmdbPopular { .. } => "TMDb Popular",
            Self::ImdbList { .. } => "IMDb List",
            Self::TraktList { .. } => "Trakt List",
            Self::RssList { .. } => "RSS List",
            Self::PlexWatchlist { .. } => "Plex Watchlist (RSS)",
            Self::TurksList { .. } => "Another Turks / Radarr",
        }
    }
    pub fn templates() -> Vec<ListKind> {
        vec![
            Self::TmdbList { list_id: String::new() },
            Self::TmdbCollection { collection_id: String::new() },
            Self::TmdbPerson { person_id: String::new(), cast: true, crew: false },
            Self::TmdbCompany { company_id: String::new() },
            Self::TmdbKeyword { keyword_id: String::new() },
            Self::TmdbPopular { which: "popular".into() },
            Self::ImdbList { list_id: String::new() },
            Self::TraktList { client_id: String::new(), username: String::new(), list: "watchlist".into() },
            Self::RssList { url: String::new() },
            Self::PlexWatchlist { rss_url: String::new() },
            Self::TurksList { url: "http://localhost:7878".into(), api_key: String::new() },
        ]
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ListMonitor {
    #[default]
    MovieOnly,
    MovieAndCollection,
    None,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ImportList {
    pub id: Id,
    pub name: String,
    pub kind: ListKind,
    pub enabled: bool,
    pub auto_add: bool,
    pub monitor: ListMonitor,
    pub search_on_add: bool,
    pub root_folder: String,
    pub quality_profile_id: Id,
    pub minimum_availability: Availability,
    pub tags: Vec<Id>,
    pub last_sync: Option<DateTime<Utc>>,
}

impl Default for ImportList {
    fn default() -> Self {
        Self {
            id: 0,
            name: String::new(),
            kind: ListKind::TmdbPopular { which: "popular".into() },
            enabled: true,
            auto_add: true,
            monitor: ListMonitor::MovieOnly,
            search_on_add: false,
            root_folder: String::new(),
            quality_profile_id: 0,
            minimum_availability: Availability::Released,
            tags: vec![],
            last_sync: None,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ListExclusion {
    pub id: Id,
    pub tmdb_id: i64,
    pub title: String,
    pub year: i32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Tag {
    pub id: Id,
    pub label: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RootFolder {
    pub id: Id,
    pub path: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RemotePathMapping {
    pub id: Id,
    pub host: String,
    pub remote: String,
    pub local: String,
}

/// A TMDb collection Turks is following.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MovieCollection {
    pub id: Id,
    pub tmdb_id: i64,
    pub title: String,
    pub overview: String,
    pub poster_path: String,
    pub monitored: bool,
    pub quality_profile_id: Id,
    pub root_folder: String,
    pub minimum_availability: Availability,
    pub search_on_add: bool,
    pub tags: Vec<Id>,
    /// Members as (tmdb id, title, year, poster).
    pub parts: Vec<CollectionPart>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CollectionPart {
    pub tmdb_id: i64,
    pub title: String,
    pub year: i32,
    pub poster_path: String,
    pub overview: String,
}

// ---------------------------------------------------------------- history / blocklist / queue

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum HistoryEvent {
    #[default]
    Grabbed,
    Imported,
    DownloadFailed,
    FileDeleted,
    FileRenamed,
    Ignored,
}

impl HistoryEvent {
    pub fn label(self) -> &'static str {
        match self {
            Self::Grabbed => "GRABBED",
            Self::Imported => "IMPORTED",
            Self::DownloadFailed => "FAILED",
            Self::FileDeleted => "DELETED",
            Self::FileRenamed => "RENAMED",
            Self::Ignored => "IGNORED",
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct History {
    pub id: Id,
    pub movie_id: Id,
    pub event: HistoryEvent,
    pub date: Option<DateTime<Utc>>,
    pub source_title: String,
    pub quality: QualityModel,
    pub languages: Vec<String>,
    pub custom_format_score: i32,
    pub download_id: String,
    pub data: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Blocklist {
    pub id: Id,
    pub movie_id: Id,
    pub source_title: String,
    pub quality: QualityModel,
    pub date: Option<DateTime<Utc>>,
    pub indexer: String,
    pub protocol: Protocol,
    pub message: String,
    pub info_hash: String,
    pub size: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DownloadState {
    #[default]
    Queued,
    Downloading,
    Paused,
    Completed,
    ImportPending,
    Importing,
    Imported,
    Failed,
    Warning,
    Delayed,
}

impl DownloadState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Queued => "QUEUED",
            Self::Downloading => "DOWNLOADING",
            Self::Paused => "PAUSED",
            Self::Completed => "COMPLETED",
            Self::ImportPending => "IMPORT PENDING",
            Self::Importing => "IMPORTING",
            Self::Imported => "IMPORTED",
            Self::Failed => "FAILED",
            Self::Warning => "WARNING",
            Self::Delayed => "DELAYED",
        }
    }
    pub fn finished(self) -> bool {
        matches!(self, Self::Imported | Self::Failed)
    }
}

/// A download Turks sent to a client and is watching.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Download {
    pub id: Id,
    pub download_id: String,
    pub client_id: Id,
    pub movie_id: Id,
    pub title: String,
    pub quality: QualityModel,
    pub languages: Vec<String>,
    pub custom_format_score: i32,
    pub protocol: Protocol,
    pub indexer: String,
    pub size: u64,
    pub remaining: u64,
    pub eta_secs: Option<i64>,
    pub state: DownloadState,
    pub messages: Vec<String>,
    pub output_path: String,
    pub added: Option<DateTime<Utc>>,
    pub from_interactive: bool,
    pub seen_in_client: bool,
}

/// A release waiting out a delay profile.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Pending {
    pub id: Id,
    pub movie_id: Id,
    pub release: crate::release::Release,
    pub release_at: Option<DateTime<Utc>>,
}
