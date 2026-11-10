//! A release found on an indexer, and the decision Turks made about it.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::model::{Id, Protocol};
use crate::parser::ParsedMovieInfo;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Release {
    pub guid: String,
    pub title: String,
    pub size: u64,
    pub download_url: String,
    pub magnet_url: String,
    pub info_hash: String,
    pub info_url: String,
    pub comments_url: String,
    pub indexer_id: Id,
    pub indexer: String,
    pub indexer_priority: i32,
    pub protocol: Protocol,
    pub publish_date: Option<DateTime<Utc>>,
    pub seeders: Option<i32>,
    pub peers: Option<i32>,
    pub imdb_id: String,
    pub tmdb_id: i64,
    pub categories: Vec<i32>,
    pub indexer_flags: Vec<String>,
    pub minimum_ratio: Option<f64>,
    pub minimum_seed_time: Option<i64>,
    pub grabs: Option<i32>,
}

impl Release {
    pub fn age_days(&self) -> f64 {
        self.publish_date.map(|d| (Utc::now() - d).num_minutes() as f64 / 1440.0).unwrap_or(0.0)
    }
    pub fn age_minutes(&self) -> i64 {
        self.publish_date.map(|d| (Utc::now() - d).num_minutes()).unwrap_or(i64::MAX)
    }
}

/// A release matched to a movie, scored and judged.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Candidate {
    pub release: Release,
    pub parsed: ParsedMovieInfo,
    pub movie_id: Option<Id>,
    pub custom_formats: Vec<Id>,
    pub format_names: Vec<String>,
    pub score: i32,
    pub rejections: Vec<Rejection>,
    /// Waiting on a delay profile.
    pub temporarily_rejected: bool,
}

impl Candidate {
    pub fn approved(&self) -> bool {
        self.rejections.is_empty()
    }
    pub fn permanently_rejected(&self) -> bool {
        self.rejections.iter().any(|r| !r.temporary)
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Rejection {
    pub reason: String,
    pub temporary: bool,
}

impl Rejection {
    pub fn perm(reason: impl Into<String>) -> Self {
        Self { reason: reason.into(), temporary: false }
    }
    pub fn temp(reason: impl Into<String>) -> Self {
        Self { reason: reason.into(), temporary: true }
    }
}
