//! Add Movie (search + library import), Discover and Collections.

use eframe::egui::{self, Color32, RichText, Ui, Vec2};

use super::common::{self, Dialog, Task};
use super::theme::{self as ff, BAD, GOOD, LABEL, TEXT, TEXT_DIM, WARN};
use super::widgets as w;
use super::App;
use crate::library::AddOptions;
use crate::model::*;
use crate::tmdb::SearchHit;

#[derive(Default)]
pub struct AddState {
    pub tab: u8,
    pub term: String,
    pub results: Vec<SearchHit>,
    pub searching: Option<Task<Result<Vec<SearchHit>, String>>>,
    pub error: Option<String>,
    // Library import
    pub root: String,
    pub unmapped: Vec<ImportRow>,
    pub scanning: Option<Task<Vec<ImportRow>>>,
    pub profile: Id,
    pub availability: Availability,
    pub monitor: bool,
    pub importing: bool,
}

#[derive(Clone, Default)]
pub struct ImportRow {
    pub include: bool,
    pub folder: crate::import::Unmapped,
    pub matches: Vec<SearchHit>,
    pub chosen: usize,
}

pub struct AddDialog {
    pub hit: SearchHit,
    pub opts: AddOptions,
    pub new_tag: String,
    pub adding: Option<Task<Result<Movie, String>>>,
    pub error: Option<String>,
}

#[derive(Default)]
pub struct DiscoverState {
    pub source: String,
    pub hits: Vec<SearchHit>,
    pub loading: Option<Task<Result<Vec<SearchHit>, String>>>,
    pub error: Option<String>,
    pub hide_owned: bool,
}

#[derive(Default)]
pub struct CollectionsState {
    pub search: String,
    pub open: Option<Id>,
}

impl App {
    pub fn default_add_options(&self) -> AddOptions {
        let lib = self.core.lib();
        let c = &self.cfg;
        let root = if lib.root_folders.values().any(|r| r.path == c.last_add_root) {
            c.last_add_root.clone()
        } else {
            lib.root_folders.values().next().map(|r| r.path.clone()).unwrap_or_default()
        };
        let profile = if lib.profiles.contains_key(&c.last_add_profile) {
            c.last_add_profile
        } else {
            lib.profiles.values().find(|p| p.name == "HD-1080p").or_else(|| lib.profiles.values().next()).map(|p| p.id).unwrap_or(0)
        };
        AddOptions { root_folder: root, quality_profile_id: profile, minimum_availability: c.last_add_availability, monitor: ListMonitor::MovieOnly, tags: vec![], search: c.last_add_search }
    }

    pub fn open_add_dialog(&mut self, hit: SearchHit) {
        self.dialog = Some(Dialog::AddMovie(AddDialog { hit, opts: self.default_add_options(), new_tag: String::new(), adding: None, error: None }));
    }

    pub fn page_add(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ff::title(ui, "Add Movie", 26.0, TEXT());
            ui.add_space(10.0);
            if w::pill(ui, "Search", self.add.tab == 0).clicked() {
                self.add.tab = 0;
            }
            if w::pill(ui, "Import existing library", self.add.tab == 1).clicked() {
                self.add.tab = 1;
            }
        });
        ff::divider(ui);
        if !self.cfg.tmdb_ready() {
            ui.label(RichText::new("Add a TMDb API key in Settings › General first. Turks uses it to look up movies.").color(WARN()));
            return;
        }
        if self.add.tab == 1 {
            return self.library_import(ui);
        }
        if let Some(r) = common::take(&self.add.searching) {
            self.add.searching = None;
            match r {
                Ok(h) => {
                    self.add.results = h;
                    self.add.error = None;
                }
                Err(e) => self.add.error = Some(e),
            }
        }
        let mut go = false;
        ui.horizontal(|ui| {
            let r = ui.add(egui::TextEdit::singleline(&mut self.add.term).hint_text("Movie title, \"title 2019\", tmdb:603 or tt0133093").desired_width(420.0));
            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                go = true;
            }
            if ui.button("Search").clicked() {
                go = true;
            }
            if self.add.searching.is_some() {
                ui.spinner();
            }
        });
        if go && !self.add.term.trim().is_empty() {
            let t = self.core.tmdb();
            let term = self.add.term.clone();
            self.add.searching = Some(common::spawn(ui.ctx(), move || t.search(&term)));
        }
        if let Some(e) = &self.add.error {
            ui.label(RichText::new(e).color(BAD()));
        }
        ui.add_space(6.0);
        let hits = self.add.results.clone();
        self.hit_list(ui, &hits, "add-results");
    }

    /// Search results as party rows with Add buttons.
    fn hit_list(&mut self, ui: &mut Ui, hits: &[SearchHit], id: &str) {
        let mut open = None;
        let mut add = None;
        egui::ScrollArea::vertical().id_salt(id).auto_shrink(false).show(ui, |ui| {
            for h in hits {
                let existing = self.core.lib().movie_by_tmdb(h.tmdb_id).map(|m| m.id);
                ui.horizontal(|ui| {
                    let tex = self.images.remote_poster(h.tmdb_id, &h.poster_path);
                    let initials: String = h.title.split_whitespace().filter_map(|w| w.chars().next()).take(2).collect();
                    common::poster(ui, tex, &initials, Vec2::new(66.0, 99.0), Color32::TRANSPARENT);
                    ui.add_space(8.0);
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ff::title(ui, &h.title, 20.0, TEXT());
                            if h.year > 0 {
                                ui.label(RichText::new(format!("({})", h.year)).color(TEXT_DIM()));
                            }
                            if h.vote > 0.0 {
                                ff::tag(ui, &format!("★ {:.1}", h.vote), WARN());
                            }
                        });
                        if !h.original_title.is_empty() && h.original_title != h.title {
                            ui.label(RichText::new(&h.original_title).color(TEXT_DIM()).size(12.0));
                        }
                        let ov: String = h.overview.chars().take(260).collect();
                        ui.add(egui::Label::new(RichText::new(if h.overview.len() > 260 { format!("{ov}…") } else { ov }).color(TEXT()).size(13.0)).wrap());
                        ui.horizontal(|ui| {
                            match existing {
                                Some(mid) => {
                                    ff::tag(ui, "IN LIBRARY", GOOD());
                                    if ui.small_button("Open").clicked() {
                                        open = Some(mid);
                                    }
                                }
                                None => {
                                    if ui.button(RichText::new("Add…").color(GOOD()).strong()).clicked() {
                                        add = Some(h.clone());
                                    }
                                }
                            }
                            ui.hyperlink_to(RichText::new("TMDb").size(12.0), format!("https://www.themoviedb.org/movie/{}", h.tmdb_id));
                        });
                    });
                });
                ff::divider(ui);
            }
        });
        if let Some(id) = open {
            self.open_movie(id);
        }
        if let Some(h) = add {
            self.open_add_dialog(h);
        }
    }

    pub fn add_movie_dialog(&mut self, ctx: &egui::Context, s: &mut AddDialog) -> bool {
        if let Some(r) = common::take(&s.adding) {
            s.adding = None;
            match r {
                Ok(m) => {
                    self.notify(format!("Added {}", m.display()));
                    return false;
                }
                Err(e) => s.error = Some(e),
            }
        }
        let lib = self.core.lib().clone();
        let mut result = None;
        let close = common::modal(ctx, "add-movie", 560.0, |ui| {
            common::heading(ui, &format!("{}{}", s.hit.title, if s.hit.year > 0 { format!(" ({})", s.hit.year) } else { String::new() }));
            if lib.root_folders.is_empty() {
                ui.label(RichText::new("Add a root folder in Settings › Media Management first.").color(WARN()));
            }
            w::row(ui, "Root folder", "Where the movie's folder will be made.", |ui| {
                common::root_combo(ui, "add-root", &lib, &mut s.opts.root_folder);
            });
            w::row(ui, "Monitor", "Monitored movies are searched for and upgraded. 'Movie and collection' also follows the rest of its collection.", |ui| {
                common::monitor_combo(ui, "add-mon", &mut s.opts.monitor);
            });
            w::row(ui, "Minimum availability", "Announced: search right away. In Cinemas: once it's in cinemas. Released: once it's out on disc or streaming.", |ui| {
                common::availability_combo(ui, "add-avail", &mut s.opts.minimum_availability);
            });
            w::row(ui, "Quality profile", "Which qualities you want.", |ui| {
                common::profile_combo(ui, "add-prof", &lib, &mut s.opts.quality_profile_id);
            });
            w::row(ui, "Tags", "Tags link movies to specific indexers, download clients, delay profiles and connections.", |_| {});
            common::tags_editor(ui, "add-tags", &self.core, &mut s.opts.tags, &mut s.new_tag);
            ui.checkbox(&mut s.opts.search, "Start searching for it right away");
            if let Some(e) = &s.error {
                ui.label(RichText::new(e).color(BAD()));
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let busy = s.adding.is_some();
                let ok = !s.opts.root_folder.is_empty() && s.opts.quality_profile_id != 0;
                if ui.add_enabled(ok && !busy, egui::Button::new(RichText::new(if busy { "Adding…" } else { "Add movie" }).color(GOOD()).strong())).clicked() {
                    result = Some(true);
                }
                if ui.button("Cancel").clicked() {
                    result = Some(false);
                }
            });
        });
        if result == Some(true) {
            self.cfg.last_add_profile = s.opts.quality_profile_id;
            self.cfg.last_add_root = s.opts.root_folder.clone();
            self.cfg.last_add_availability = s.opts.minimum_availability;
            self.cfg.last_add_search = s.opts.search;
            self.changed();
            let core = self.core.clone();
            let (id, opts) = (s.hit.tmdb_id, s.opts.clone());
            s.adding = Some(common::spawn(ctx, move || crate::library::add_movie(&core, id, &opts)));
            return true;
        }
        result.is_none() && !close
    }

    fn library_import(&mut self, ui: &mut Ui) {
        if let Some(rows) = common::take(&self.add.scanning) {
            self.add.scanning = None;
            self.add.unmapped = rows;
        }
        let lib = self.core.lib().clone();
        if self.add.profile == 0 {
            self.add.profile = lib.profiles.keys().next().copied().unwrap_or(0);
            self.add.monitor = true;
        }
        ui.add(egui::Label::new(common::dim_text("Turks looks through a root folder for movie folders it doesn't know yet, matches each one on TMDb, and adds them with the files already in place (nothing is moved).")).wrap());
        ui.horizontal(|ui| {
            common::root_combo(ui, "li-root", &lib, &mut self.add.root);
            let busy = self.add.scanning.is_some();
            if ui.add_enabled(!busy && !self.add.root.is_empty(), egui::Button::new(if busy { "Scanning…" } else { "Scan folder" })).clicked() {
                let core = self.core.clone();
                let root = self.add.root.clone();
                self.add.scanning = Some(common::spawn(ui.ctx(), move || {
                    let found = crate::import::unmapped_folders(&core.lib(), std::path::Path::new(&root));
                    let t = core.tmdb();
                    found
                        .into_iter()
                        .map(|f| {
                            let matches = if f.tmdb_id_hint > 0 {
                                t.search(&format!("tmdb:{}", f.tmdb_id_hint)).unwrap_or_default()
                            } else {
                                t.search_title_year(&f.parsed_title, f.parsed_year).unwrap_or_default()
                            };
                            ImportRow { include: !matches.is_empty(), folder: f, matches: matches.into_iter().take(8).collect(), chosen: 0 }
                        })
                        .collect()
                }));
            }
            if busy {
                ui.spinner();
            }
        });
        if lib.root_folders.is_empty() {
            ui.label(RichText::new("Add a root folder in Settings › Media Management first.").color(WARN()));
            return;
        }
        if self.add.unmapped.is_empty() {
            return;
        }
        ui.horizontal_wrapped(|ui| {
            ff::stat_label(ui, "Profile");
            common::profile_combo(ui, "li-prof", &lib, &mut self.add.profile);
            ff::stat_label(ui, "Availability");
            common::availability_combo(ui, "li-avail", &mut self.add.availability);
            ui.checkbox(&mut self.add.monitor, "Monitored");
            let n = self.add.unmapped.iter().filter(|r| r.include && !r.matches.is_empty()).count();
            if ui.add_enabled(n > 0 && !self.add.importing, egui::Button::new(RichText::new(format!("Import {n} movies")).color(GOOD()).strong())).clicked() {
                let core = self.core.clone();
                let rows: Vec<(i64, String)> = self.add.unmapped.iter().filter(|r| r.include && !r.matches.is_empty()).map(|r| (r.matches[r.chosen].tmdb_id, r.folder.path.display().to_string())).collect();
                let opts = AddOptions {
                    root_folder: self.add.root.clone(),
                    quality_profile_id: self.add.profile,
                    minimum_availability: self.add.availability,
                    monitor: if self.add.monitor { ListMonitor::MovieOnly } else { ListMonitor::None },
                    tags: vec![],
                    search: false,
                };
                std::thread::spawn(move || {
                    let mut ok = 0;
                    for (tmdb, path) in rows {
                        match crate::library::add_movie(&core, tmdb, &opts) {
                            Ok(m) => {
                                // Keep the existing folder name instead of renaming it.
                                if m.path != path {
                                    let _ = crate::library::move_movie(&core, m.id, &path, false);
                                }
                                let _ = crate::library::rescan_one(&core, m.id);
                                ok += 1;
                            }
                            Err(e) => core.toast(format!("Couldn't add {path}: {e}")),
                        }
                    }
                    core.toast(format!("Imported {ok} movies."));
                });
                self.add.unmapped.clear();
            }
        });
        ff::divider(ui);
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            egui::Grid::new("li-grid").striped(true).spacing([12.0, 6.0]).show(ui, |ui| {
                for h in ["", "Folder", "Files", "Match on TMDb"] {
                    ff::title(ui, h, 13.0, LABEL());
                }
                ui.end_row();
                for (i, r) in self.add.unmapped.iter_mut().enumerate() {
                    ui.add_enabled(!r.matches.is_empty(), egui::Checkbox::without_text(&mut r.include));
                    ui.label(RichText::new(&r.folder.name).color(TEXT()));
                    ui.label(r.folder.video_count.to_string());
                    if r.matches.is_empty() {
                        ui.label(RichText::new("No match found").color(BAD()));
                    } else {
                        let sel = &r.matches[r.chosen];
                        egui::ComboBox::from_id_salt(("li-match", i)).selected_text(format!("{} ({})", sel.title, sel.year)).width(300.0).show_ui(ui, |ui| {
                            for (j, m) in r.matches.iter().enumerate() {
                                ui.selectable_value(&mut r.chosen, j, format!("{} ({})", m.title, m.year));
                            }
                        });
                    }
                    ui.end_row();
                }
            });
        });
    }

    // ------------------------------------------------------------ discover

    pub fn page_discover(&mut self, ui: &mut Ui) {
        if let Some(r) = common::take(&self.discover.loading) {
            self.discover.loading = None;
            match r {
                Ok(h) => {
                    self.discover.hits = h;
                    self.discover.error = None;
                }
                Err(e) => self.discover.error = Some(e),
            }
        }
        let sources = [
            ("trending", "Trending"),
            ("popular", "Popular"),
            ("upcoming", "Upcoming"),
            ("now_playing", "In cinemas"),
            ("top_rated", "Top rated"),
            ("recommended", "Recommended for you"),
        ];
        let mut load = None;
        ui.horizontal_wrapped(|ui| {
            ff::title(ui, "Discover", 26.0, TEXT());
            ui.add_space(10.0);
            for (k, l) in sources {
                if w::pill(ui, l, self.discover.source == k).clicked() {
                    load = Some(k.to_string());
                }
            }
            ui.checkbox(&mut self.discover.hide_owned, "Hide movies I have");
            if self.discover.loading.is_some() {
                ui.spinner();
            }
        });
        ff::divider(ui);
        if !self.cfg.tmdb_ready() {
            ui.label(RichText::new("Add a TMDb API key in Settings › General first.").color(WARN()));
            return;
        }
        if self.discover.source.is_empty() && self.discover.loading.is_none() {
            load = Some("trending".into());
        }
        if let Some(src) = load {
            self.discover.source = src.clone();
            let t = self.core.tmdb();
            let core = self.core.clone();
            self.discover.loading = Some(common::spawn(ui.ctx(), move || {
                if src == "recommended" {
                    // Gather TMDb recommendations from the library, most-recommended first.
                    let lib = core.lib().clone();
                    let mut counts: std::collections::HashMap<i64, usize> = Default::default();
                    for m in lib.movies.values() {
                        for r in &m.recommendations {
                            if lib.movie_by_tmdb(*r).is_none() {
                                *counts.entry(*r).or_default() += 1;
                            }
                        }
                    }
                    let mut ids: Vec<(i64, usize)> = counts.into_iter().collect();
                    ids.sort_by_key(|x| std::cmp::Reverse(x.1));
                    let mut out = Vec::new();
                    for (id, _) in ids.into_iter().take(40) {
                        if let Ok(mut h) = t.search(&format!("tmdb:{id}")) {
                            out.append(&mut h);
                        }
                    }
                    Ok(out)
                } else {
                    t.chart(&src, 3)
                }
            }));
        }
        if let Some(e) = &self.discover.error {
            ui.label(RichText::new(e).color(BAD()));
        }
        let hits: Vec<SearchHit> = self.discover.hits.iter().filter(|h| !self.discover.hide_owned || self.core.lib().movie_by_tmdb(h.tmdb_id).is_none()).cloned().collect();
        let pw = 130.0;
        let ph = pw * 1.5;
        let gap = 14.0;
        let cols = (((ui.available_width() + gap) / (pw + gap)).floor() as usize).max(1);
        let rows = hits.len().div_ceil(cols);
        let mut add = None;
        let mut open = None;
        egui::ScrollArea::vertical().auto_shrink(false).show_rows(ui, ph + 70.0, rows, |ui, range| {
            for r in range {
                ui.horizontal(|ui| {
                    for h in hits.iter().skip(r * cols).take(cols) {
                        ui.vertical(|ui| {
                            ui.set_width(pw);
                            let owned = self.core.lib().movie_by_tmdb(h.tmdb_id).map(|m| m.id);
                            let tex = self.images.remote_poster(h.tmdb_id, &h.poster_path);
                            let initials: String = h.title.split_whitespace().filter_map(|w| w.chars().next()).take(2).collect();
                            let resp = common::poster(ui, tex, &initials, Vec2::new(pw, ph), if owned.is_some() { GOOD() } else { Color32::TRANSPARENT });
                            let ov: String = h.overview.chars().take(400).collect();
                            if resp.on_hover_text(format!("{} ({})\n★ {:.1}\n\n{ov}", h.title, h.year, h.vote)).clicked() {
                                match owned {
                                    Some(id) => open = Some(id),
                                    None => add = Some(h.clone()),
                                }
                            }
                            ui.add(egui::Label::new(RichText::new(&h.title).color(TEXT()).strong().size(13.0)).truncate());
                            ui.label(RichText::new(if h.year > 0 { h.year.to_string() } else { "TBA".into() }).color(TEXT_DIM()).size(12.0));
                        });
                        ui.add_space(gap - 8.0);
                    }
                });
            }
        });
        if let Some(h) = add {
            self.open_add_dialog(h);
        }
        if let Some(id) = open {
            self.open_movie(id);
        }
    }

    // ------------------------------------------------------------ collections

    pub fn page_collections(&mut self, ui: &mut Ui) {
        let lib = self.core.lib().clone();
        ui.horizontal(|ui| {
            ff::title(ui, "Collections", 26.0, TEXT());
            ui.add_space(10.0);
            ui.add(egui::TextEdit::singleline(&mut self.collections.search).hint_text("Find a collection").desired_width(200.0));
            if ui.button("Refresh collections").on_hover_text("Find collections your movies belong to, and add missing movies from monitored ones").clicked() {
                self.core.run(crate::core::Command::RefreshCollections);
            }
        });
        ff::divider(ui);
        if lib.collections.is_empty() {
            ui.add(egui::Label::new(common::dim_text("No collections yet. Collections appear once your movies are in one (press Refresh collections), or when you add a movie with 'Movie and collection'.")).wrap());
            return;
        }
        let q = self.collections.search.to_lowercase();
        let mut cols: Vec<&MovieCollection> = lib.collections.values().filter(|c| q.is_empty() || c.title.to_lowercase().contains(&q)).collect();
        cols.sort_by(|a, b| a.title.cmp(&b.title));
        let mut toggle = None;
        let mut add_missing = None;
        let mut open_movie = None;
        let mut add_one = None;
        let mut save = None;
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            for c in cols {
                let have = c.parts.iter().filter(|p| lib.movie_by_tmdb(p.tmdb_id).is_some()).count();
                ui.horizontal(|ui| {
                    let tex = self.images.collection(c.tmdb_id, &c.poster_path);
                    let resp = common::poster(ui, tex, &c.title.chars().take(2).collect::<String>(), Vec2::new(70.0, 105.0), Color32::TRANSPARENT);
                    if resp.clicked() {
                        self.collections.open = if self.collections.open == Some(c.id) { None } else { Some(c.id) };
                    }
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ff::title(ui, &c.title, 20.0, TEXT());
                            ff::tag(ui, &format!("{have}/{}", c.parts.len()), if have == c.parts.len() { GOOD() } else { WARN() });
                        });
                        ui.horizontal(|ui| {
                            ff::stat_label(ui, "HP");
                            ff::stat_bar(ui, have as f32 / c.parts.len().max(1) as f32, 220.0, Color32::from_rgb(70, 130, 230), GOOD());
                        });
                        ui.horizontal(|ui| {
                            let mut mon = c.monitored;
                            if w::toggle_labeled(ui, &mut mon, "MONITORED", "UNMONITORED").then_some(()).is_some() {
                                toggle = Some((c.id, mon));
                            }
                            if have < c.parts.len() && ui.button("Add missing movies").clicked() {
                                add_missing = Some(c.id);
                            }
                            let expanded = self.collections.open == Some(c.id);
                            if ui.small_button(if expanded { "Hide movies" } else { "Show movies" }).clicked() {
                                self.collections.open = if expanded { None } else { Some(c.id) };
                            }
                        });
                    });
                });
                if self.collections.open == Some(c.id) {
                    ui.indent(("col", c.id), |ui| {
                        let mut edited = c.clone();
                        let mut changed = false;
                        ui.horizontal_wrapped(|ui| {
                            ff::stat_label(ui, "Root");
                            changed |= common::root_combo(ui, ("col-root", c.id).0, &lib, &mut edited.root_folder);
                            ff::stat_label(ui, "Profile");
                            changed |= common::profile_combo(ui, "col-prof", &lib, &mut edited.quality_profile_id);
                            ff::stat_label(ui, "Availability");
                            changed |= common::availability_combo(ui, "col-avail", &mut edited.minimum_availability);
                            changed |= ui.checkbox(&mut edited.search_on_add, "Search when added").changed();
                        });
                        if changed {
                            save = Some(edited);
                        }
                        for p in &c.parts {
                            ui.horizontal(|ui| {
                                match lib.movie_by_tmdb(p.tmdb_id) {
                                    Some(m) => {
                                        ff::badge(ui, if m.movie_file_id.is_some() { ff::Badge::Good } else { ff::Badge::Warn }, 14.0);
                                        if ui.add(egui::Label::new(RichText::new(format!("{} ({})", p.title, p.year)).color(TEXT())).sense(egui::Sense::click())).clicked() {
                                            open_movie = Some(m.id);
                                        }
                                    }
                                    None => {
                                        ff::badge(ui, ff::Badge::Bad, 14.0);
                                        ui.label(RichText::new(format!("{} ({})", p.title, p.year)).color(TEXT_DIM()));
                                        if ui.small_button("Add…").clicked() {
                                            add_one = Some(SearchHit { tmdb_id: p.tmdb_id, title: p.title.clone(), year: p.year, poster_path: p.poster_path.clone(), overview: p.overview.clone(), ..Default::default() });
                                        }
                                    }
                                }
                            });
                        }
                    });
                }
                ff::divider(ui);
            }
        });
        if let Some((id, mon)) = toggle {
            if let Some(c) = lib.collections.get(&id) {
                self.core.save(MovieCollection { monitored: mon, ..c.clone() });
            }
        }
        if let Some(c) = save {
            self.core.save(c);
        }
        if let Some(id) = add_missing {
            if let Some(c) = lib.collections.get(&id).cloned() {
                let core = self.core.clone();
                if c.root_folder.is_empty() || c.quality_profile_id == 0 {
                    self.notify("Pick a root folder and quality profile for this collection first (Show movies).");
                } else {
                    std::thread::spawn(move || {
                        let n = crate::library::add_missing_from_collection(&core, &c);
                        core.toast(format!("Added {n} movies from {}", c.title));
                    });
                }
            }
        }
        if let Some(id) = open_movie {
            self.open_movie(id);
        }
        if let Some(h) = add_one {
            self.open_add_dialog(h);
        }
        let _ = BAD();
    }
}
