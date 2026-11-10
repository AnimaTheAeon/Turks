//! Poster/fanart textures: fetched and decoded on worker threads, uploaded
//! on the UI thread.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex};

use eframe::egui;

struct Job {
    key: String,
    url_path: String,
    size: &'static str,
    dest: PathBuf,
    max_w: u32,
}

#[derive(Default)]
struct Shared {
    jobs: VecDeque<Job>,
    done: Vec<(String, Option<egui::ColorImage>)>,
}

pub struct Images {
    textures: HashMap<String, egui::TextureHandle>,
    failed: HashSet<String>,
    queued: HashSet<String>,
    shared: Arc<(Mutex<Shared>, Condvar)>,
}

impl Images {
    pub fn new(ctx: &egui::Context) -> Self {
        let shared: Arc<(Mutex<Shared>, Condvar)> = Arc::new((Mutex::new(Shared::default()), Condvar::new()));
        for _ in 0..4 {
            let shared = Arc::clone(&shared);
            let ctx = ctx.clone();
            std::thread::spawn(move || loop {
                let job = {
                    let (lock, cv) = &*shared;
                    let mut s = lock.lock().unwrap();
                    while s.jobs.is_empty() {
                        s = cv.wait(s).unwrap();
                    }
                    s.jobs.pop_front().unwrap()
                };
                let img = load(&job);
                shared.0.lock().unwrap().done.push((job.key, img));
                ctx.request_repaint();
            });
        }
        Self { textures: HashMap::new(), failed: HashSet::new(), queued: HashSet::new(), shared }
    }

    /// Call once per frame to upload finished images.
    pub fn pump(&mut self, ctx: &egui::Context) {
        let done = std::mem::take(&mut self.shared.0.lock().unwrap().done);
        for (key, img) in done {
            self.queued.remove(&key);
            match img {
                Some(i) => {
                    let t = ctx.load_texture(&key, i, egui::TextureOptions::LINEAR);
                    self.textures.insert(key, t);
                }
                None => {
                    self.failed.insert(key);
                }
            }
        }
        // Keep memory in check on huge libraries.
        if self.textures.len() > 1500 {
            self.textures.clear();
        }
    }

    fn get(&mut self, key: String, url_path: &str, size: &'static str, dest: PathBuf, max_w: u32) -> Option<egui::TextureId> {
        if let Some(t) = self.textures.get(&key) {
            return Some(t.id());
        }
        if url_path.is_empty() && !dest.exists() {
            return None;
        }
        if self.failed.contains(&key) || self.queued.contains(&key) {
            return None;
        }
        self.queued.insert(key.clone());
        let (lock, cv) = &*self.shared;
        let mut s = lock.lock().unwrap();
        // Newest requests first: they're what's on screen.
        s.jobs.push_front(Job { key, url_path: url_path.to_string(), size, dest, max_w });
        cv.notify_one();
        None
    }

    pub fn poster(&mut self, tmdb_id: i64, poster_path: &str) -> Option<egui::TextureId> {
        self.get(format!("poster-{tmdb_id}"), poster_path, "w342", crate::tmdb::image_path("poster", tmdb_id), 342)
    }

    pub fn fanart(&mut self, tmdb_id: i64, path: &str) -> Option<egui::TextureId> {
        self.get(format!("fanart-{tmdb_id}"), path, "w1280", crate::tmdb::image_path("fanart", tmdb_id), 1280)
    }

    /// Posters for search results / discover (not in the library yet).
    pub fn remote_poster(&mut self, tmdb_id: i64, poster_path: &str) -> Option<egui::TextureId> {
        let dest = crate::config::cache_dir().join("discover").join(format!("{tmdb_id}.jpg"));
        self.get(format!("remote-{tmdb_id}"), poster_path, "w185", dest, 185)
    }

    pub fn collection(&mut self, tmdb_id: i64, poster_path: &str) -> Option<egui::TextureId> {
        self.get(format!("collection-{tmdb_id}"), poster_path, "w342", crate::tmdb::image_path("collection", tmdb_id), 342)
    }

    pub fn person(&mut self, person_id: i64, path: &str) -> Option<egui::TextureId> {
        self.get(format!("person-{person_id}"), path, "w185", crate::tmdb::person_image_path(person_id), 185)
    }

}

fn load(job: &Job) -> Option<egui::ColorImage> {
    let path = crate::tmdb::fetch_image(&job.url_path, job.size, &job.dest).or_else(|| job.dest.exists().then(|| job.dest.clone()))?;
    let bytes = std::fs::read(&path).ok()?;
    let img = match image::load_from_memory(&bytes) {
        Ok(i) => i,
        Err(_) => {
            let _ = std::fs::remove_file(&path);
            return None;
        }
    };
    let img = if img.width() > job.max_w { img.resize(job.max_w, u32::MAX, image::imageops::FilterType::Triangle) } else { img };
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    Some(egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], rgba.as_raw()))
}
