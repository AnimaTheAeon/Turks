//! Movies: library views, the movie details ("Status") screen, interactive
//! search, bulk editing and manual import.

use std::collections::HashSet;

use eframe::egui::{self, Color32, RichText, Ui, Vec2};

use super::common::{self, Dialog, MovieState};
use super::theme::{self as ff, BAD, GOOD, LABEL, TEXT, TEXT_DIM, WARN};
use super::widgets as w;
use super::App;
use crate::core::Command;
use crate::model::*;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum View {
    #[default]
    Posters,
    Party,
    Table,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Filter {
    #[default]
    All,
    Monitored,
    Unmonitored,
    Missing,
    Downloaded,
    Upgrades,
    Downloading,
    NotAvailable,
}

impl Filter {
    const ALL: [Filter; 8] = [Self::All, Self::Monitored, Self::Unmonitored, Self::Missing, Self::Downloaded, Self::Upgrades, Self::Downloading, Self::NotAvailable];
    fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Monitored => "Monitored",
            Self::Unmonitored => "Unmonitored",
            Self::Missing => "Missing",
            Self::Downloaded => "Downloaded",
            Self::Upgrades => "Upgrades",
            Self::Downloading => "Downloading",
            Self::NotAvailable => "Not available",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Sort {
    #[default]
    Title,
    Year,
    Added,
    Release,
    Rating,
    Size,
    Runtime,
    Popularity,
}

impl Sort {
    const ALL: [Sort; 8] = [Self::Title, Self::Year, Self::Added, Self::Release, Self::Rating, Self::Size, Self::Runtime, Self::Popularity];
    fn label(self) -> &'static str {
        match self {
            Self::Title => "Title",
            Self::Year => "Year",
            Self::Added => "Date added",
            Self::Release => "Release date",
            Self::Rating => "Rating",
            Self::Size => "Size on disk",
            Self::Runtime => "Runtime",
            Self::Popularity => "Popularity",
        }
    }
}

#[derive(Default)]
pub struct MoviesState {
    pub view: View,
    pub filter: Filter,
    pub sort: Sort,
    pub descending: bool,
    pub search: String,
    pub select_mode: bool,
    pub selected: HashSet<Id>,
    pub view_loaded: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Overview,
    Files,
    History,
    Search,
    Cast,
}

pub struct DetailState {
    pub id: Id,
    pub tab: Tab,
    pub history: Option<Vec<History>>,
    pub search_sort: u8,
    pub show_rejected: bool,
    pub new_tag: String,
}

impl DetailState {
    pub fn new(id: Id) -> Self {
        Self { id, tab: Tab::Overview, history: None, search_sort: 0, show_rejected: true, new_tag: String::new() }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tri {
    Keep,
    Yes,
    No,
}

pub struct EditMovies {
    pub ids: Vec<Id>,
    pub monitored: Tri,
    pub profile: Id,
    pub availability: Option<Availability>,
    pub root: String,
    pub move_files: bool,
    pub tags: Vec<Id>,
    pub tag_mode: u8,
    pub new_tag: String,
}

pub struct ManualImport {
    pub folder: String,
    pub movie_hint: Option<Id>,
    pub rows: Vec<(bool, crate::import::ImportDecision)>,
    pub mode: u8,
    pub scanning: Option<common::Task<Vec<crate::import::ImportDecision>>>,
    pub movie_search: Vec<String>,
}

fn movie_size(lib: &crate::db::Library, m: &Movie) -> u64 {
    lib.file_for(m).map(|f| f.size).unwrap_or(0)
}

impl App {
    fn filtered_movies(&self) -> Vec<Movie> {
        let lib = self.core.lib();
        let delay = self.cfg.availability_delay_days;
        let q = self.movies.search.trim().to_lowercase();
        let mut list: Vec<Movie> = lib
            .movies
            .values()
            .filter(|m| {
                q.is_empty()
                    || m.title.to_lowercase().contains(&q)
                    || m.original_title.to_lowercase().contains(&q)
                    || m.year.to_string() == q
                    || m.genres.iter().any(|g| g.to_lowercase() == q)
                    || m.tags.iter().any(|t| lib.tag_label(*t).to_lowercase() == q)
                    || m.imdb_id.to_lowercase() == q
            })
            .filter(|m| {
                let st = common::movie_state(&lib, m, delay);
                match self.movies.filter {
                    Filter::All => true,
                    Filter::Monitored => m.monitored,
                    Filter::Unmonitored => !m.monitored,
                    Filter::Missing => st == MovieState::Missing,
                    Filter::Downloaded => m.movie_file_id.is_some(),
                    Filter::Upgrades => st == MovieState::CutoffUnmet,
                    Filter::Downloading => st == MovieState::Downloading,
                    Filter::NotAvailable => st == MovieState::NotAvailable,
                }
            })
            .cloned()
            .collect();
        let key_date = |m: &Movie| m.digital_release.or(m.physical_release).or(m.in_cinemas);
        match self.movies.sort {
            Sort::Title => list.sort_by(|a, b| a.sort_title.cmp(&b.sort_title)),
            Sort::Year => list.sort_by_key(|m| (m.year, m.sort_title.clone())),
            Sort::Added => list.sort_by_key(|m| m.added),
            Sort::Release => list.sort_by_key(key_date),
            Sort::Rating => list.sort_by(|a, b| a.ratings.tmdb.partial_cmp(&b.ratings.tmdb).unwrap_or(std::cmp::Ordering::Equal)),
            Sort::Size => list.sort_by_key(|m| movie_size(&lib, m)),
            Sort::Runtime => list.sort_by_key(|m| m.runtime),
            Sort::Popularity => list.sort_by(|a, b| a.popularity.partial_cmp(&b.popularity).unwrap_or(std::cmp::Ordering::Equal)),
        }
        if self.movies.descending {
            list.reverse();
        }
        list
    }

    pub fn page_movies(&mut self, ui: &mut Ui) {
        if !self.movies.view_loaded {
            self.movies.view = match self.cfg.movie_view {
                1 => View::Party,
                2 => View::Table,
                _ => View::Posters,
            };
            self.movies.view_loaded = true;
        }
        let list = self.filtered_movies();
        let total = self.core.lib().movies.len();

        // Toolbar
        ui.horizontal_wrapped(|ui| {
            ff::title(ui, "Movies", 26.0, TEXT());
            ui.add_space(8.0);
            ui.add(egui::TextEdit::singleline(&mut self.movies.search).hint_text("Find: title, year, genre, tag").desired_width(200.0));
            egui::ComboBox::from_id_salt("movie-filter").selected_text(self.movies.filter.label()).show_ui(ui, |ui| {
                for f in Filter::ALL {
                    ui.selectable_value(&mut self.movies.filter, f, f.label());
                }
            });
            egui::ComboBox::from_id_salt("movie-sort").selected_text(format!("Sort: {}", self.movies.sort.label())).show_ui(ui, |ui| {
                for s in Sort::ALL {
                    ui.selectable_value(&mut self.movies.sort, s, s.label());
                }
            });
            if ui.button(if self.movies.descending { "▼ Desc" } else { "▲ Asc" }).on_hover_text("Sort direction").clicked() {
                self.movies.descending = !self.movies.descending;
            }
            for (v, label) in [(View::Posters, "Posters"), (View::Party, "Party"), (View::Table, "Table")] {
                if w::pill(ui, label, self.movies.view == v).clicked() {
                    self.movies.view = v;
                    self.cfg.movie_view = match v {
                        View::Posters => 0,
                        View::Party => 1,
                        View::Table => 2,
                    };
                    self.changed();
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            if ui.button("Refresh & scan").on_hover_text("Update info from TMDb and rescan every movie folder").clicked() {
                self.core.run(Command::RefreshMovies(vec![]));
            }
            if ui.button("RSS sync").clicked() {
                self.core.run(Command::RssSync);
            }
            if ui.button("Search all missing").clicked() {
                self.core.run(Command::MissingSearch);
            }
            if w::pill(ui, if self.movies.select_mode { "Selecting…" } else { "Select" }, self.movies.select_mode).on_hover_text("Pick several movies to edit, search, rename or delete together").clicked() {
                self.movies.select_mode = !self.movies.select_mode;
                self.movies.selected.clear();
            }
            if self.movies.select_mode {
                let n = self.movies.selected.len();
                if ui.small_button("All").clicked() {
                    self.movies.selected = list.iter().map(|m| m.id).collect();
                }
                if ui.small_button("None").clicked() {
                    self.movies.selected.clear();
                }
                ui.label(common::dim_text(&format!("{n} selected")));
                let ids: Vec<Id> = self.movies.selected.iter().copied().collect();
                ui.add_enabled_ui(n > 0, |ui| {
                    if ui.button("Edit").clicked() {
                        let first = self.core.lib().movies.get(&ids[0]).cloned();
                        self.dialog = Some(Dialog::EditMovies(EditMovies {
                            ids: ids.clone(),
                            monitored: Tri::Keep,
                            profile: 0,
                            availability: None,
                            root: first.map(|m| m.root_folder).unwrap_or_default(),
                            move_files: false,
                            tags: vec![],
                            tag_mode: 0,
                            new_tag: String::new(),
                        }));
                    }
                    if ui.button("Search").clicked() {
                        self.core.run(Command::SearchMovies(ids.clone()));
                    }
                    if ui.button("Refresh").clicked() {
                        self.core.run(Command::RefreshMovies(ids.clone()));
                    }
                    if ui.button("Rename").clicked() {
                        let preview = crate::library::rename_preview(&self.core, &ids);
                        self.dialog = Some(Dialog::Rename { ids: ids.clone(), preview });
                    }
                    if ui.button(RichText::new("Delete").color(BAD())).clicked() {
                        self.dialog = Some(Dialog::DeleteMovies { ids: ids.clone(), delete_files: false, exclusion: false });
                    }
                });
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(common::dim_text(&format!("{} of {total}", list.len())));
            });
        });
        ff::divider(ui);

        if total == 0 {
            ui.add_space(30.0);
            ui.vertical_centered(|ui| {
                ff::title(ui, "No movies yet", 22.0, TEXT());
                ui.label(common::dim_text("Add one, or import the movies you already have."));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.add_space(ui.available_width() / 2.0 - 150.0);
                    if ui.button("Add a movie").clicked() {
                        self.go(super::Page::Add);
                    }
                    if ui.button("Import existing library").clicked() {
                        self.go(super::Page::Add);
                        self.add.tab = 1;
                    }
                });
            });
            return;
        }
        match self.movies.view {
            View::Posters => self.movies_posters(ui, &list),
            View::Party => self.movies_party(ui, &list),
            View::Table => self.movies_table(ui, &list),
        }
    }

    fn click_movie(&mut self, id: Id) {
        if self.movies.select_mode {
            if !self.movies.selected.remove(&id) {
                self.movies.selected.insert(id);
            }
        } else {
            self.open_movie(id);
        }
    }

    fn movies_posters(&mut self, ui: &mut Ui, list: &[Movie]) {
        let pw = self.cfg.poster_size.clamp(100.0, 260.0);
        let ph = pw * 1.5;
        let gap = 14.0;
        let row_h = ph + 44.0 + gap;
        let cols = (((ui.available_width() + gap) / (pw + gap)).floor() as usize).max(1);
        let rows = list.len().div_ceil(cols);
        let delay = self.cfg.availability_delay_days;
        let mut clicked = None;
        egui::ScrollArea::vertical().auto_shrink(false).show_rows(ui, row_h, rows, |ui, range| {
            for r in range {
                ui.horizontal(|ui| {
                    for m in list.iter().skip(r * cols).take(cols) {
                        ui.vertical(|ui| {
                            ui.set_width(pw);
                            let st = {
                                let lib = self.core.lib();
                                common::movie_state(&lib, m, delay)
                            };
                            let tex = self.images.poster(m.tmdb_id, &m.poster_path);
                            let resp = common::poster(ui, tex, &m.initials(), Vec2::new(pw, ph), st.color());
                            if self.movies.select_mode && self.movies.selected.contains(&m.id) {
                                ui.painter().rect_stroke(resp.rect.expand(3.0), egui::CornerRadius::same(5), egui::Stroke::new(3.0, WARN()), egui::StrokeKind::Outside);
                            }
                            if resp.on_hover_text(format!("{}\n{}", m.display(), st.label())).clicked() {
                                clicked = Some(m.id);
                            }
                            ui.add(egui::Label::new(RichText::new(&m.title).color(TEXT()).strong().size(13.0)).truncate());
                            ui.label(RichText::new(if m.year > 0 { m.year.to_string() } else { "TBA".into() }).color(TEXT_DIM()).size(12.0));
                        });
                        ui.add_space(gap - 8.0);
                    }
                });
                ui.add_space(gap - 6.0);
            }
        });
        if let Some(id) = clicked {
            self.click_movie(id);
        }
    }

    /// The FFVII party list: portrait, name, LV (profile), HP (file status).
    fn movies_party(&mut self, ui: &mut Ui, list: &[Movie]) {
        let row_h = 104.0;
        let delay = self.cfg.availability_delay_days;
        let mut clicked = None;
        let mut search = None;
        egui::ScrollArea::vertical().auto_shrink(false).show_rows(ui, row_h, list.len(), |ui, range| {
            for m in &list[range] {
                let (st, profile, file, dl) = {
                    let lib = self.core.lib();
                    let dl = lib.active_downloads().find(|d| d.movie_id == m.id).cloned();
                    (common::movie_state(&lib, m, delay), lib.profile_for(m).map(|p| p.name.clone()).unwrap_or_default(), lib.file_for(m).cloned(), dl)
                };
                let row = ui.horizontal(|ui| {
                    ui.set_height(row_h - 8.0);
                    let tex = self.images.poster(m.tmdb_id, &m.poster_path);
                    let r = common::poster(ui, tex, &m.initials(), Vec2::new(62.0, 92.0), Color32::TRANSPARENT);
                    if r.clicked() {
                        clicked = Some(m.id);
                    }
                    ui.add_space(8.0);
                    ui.vertical(|ui| {
                        ui.set_width((ui.available_width() - 300.0).max(160.0));
                        let title = ff::title(ui, &m.title, 20.0, if m.monitored { TEXT() } else { TEXT_DIM() });
                        if title.interact(egui::Sense::click()).clicked() {
                            clicked = Some(m.id);
                        }
                        ui.label(RichText::new(format!("{} · {} · {}", if m.year > 0 { m.year.to_string() } else { "TBA".into() }, common::runtime(m.runtime, &self.cfg), m.genres.iter().take(3).cloned().collect::<Vec<_>>().join(", "))).color(TEXT_DIM()).size(13.0));
                        ui.horizontal(|ui| {
                            ff::stat_label(ui, "LV");
                            ui.label(RichText::new(&profile).color(TEXT()).size(13.0));
                            if let Some(f) = &file {
                                ui.add_space(6.0);
                                common::quality_chip(ui, &f.quality);
                            }
                        });
                    });
                    ui.vertical(|ui| {
                        ui.set_width(270.0);
                        ui.horizontal(|ui| {
                            ff::stat_label(ui, "HP");
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ff::tag(ui, st.label(), st.color());
                            });
                        });
                        let (frac, caption) = match (&file, &dl) {
                            (_, Some(d)) if d.size > 0 => (1.0 - d.remaining as f32 / d.size as f32, format!("{} · {}", d.state.label().to_lowercase(), crate::core::human_rate_eta(d.eta_secs))),
                            (Some(f), _) => (1.0, format!("{} · {}", crate::decision::human_size(f.size), f.release_group)),
                            _ => (0.0, if m.monitored { "Monitored · no file yet".into() } else { "Not monitored".into() }),
                        };
                        ff::stat_bar(ui, frac, 260.0, Color32::from_rgb(70, 130, 230), st.color());
                        ui.label(RichText::new(caption).color(TEXT_DIM()).size(12.0));
                        if m.monitored && file.is_none() && dl.is_none() && ui.small_button("Search").clicked() {
                            search = Some(m.id);
                        }
                    });
                });
                if self.movies.select_mode && self.movies.selected.contains(&m.id) {
                    ui.painter().rect_stroke(row.response.rect.expand(2.0), egui::CornerRadius::same(4), egui::Stroke::new(2.0, WARN()), egui::StrokeKind::Outside);
                }
                ff::divider(ui);
            }
        });
        if let Some(id) = clicked {
            self.click_movie(id);
        }
        if let Some(id) = search {
            self.core.run(Command::SearchMovies(vec![id]));
        }
    }

    fn movies_table(&mut self, ui: &mut Ui, list: &[Movie]) {
        let delay = self.cfg.availability_delay_days;
        let mut clicked = None;
        egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
            egui::Grid::new("movie-table").striped(true).spacing([14.0, 6.0]).show(ui, |ui| {
                for h in ["", "Title", "Year", "Profile", "Quality", "Status", "Size", "Release", "Added"] {
                    ff::title(ui, h, 13.0, LABEL());
                }
                ui.end_row();
                let lib = self.core.lib().clone();
                for m in list {
                    let st = common::movie_state(&lib, m, delay);
                    if self.movies.select_mode {
                        let mut sel = self.movies.selected.contains(&m.id);
                        if ui.checkbox(&mut sel, "").changed() {
                            clicked = Some(m.id);
                        }
                    } else {
                        ff::badge(ui, if m.monitored { ff::Badge::Good } else { ff::Badge::Warn }, 12.0).on_hover_text(if m.monitored { "Monitored" } else { "Not monitored" });
                    }
                    if ui.add(egui::Label::new(RichText::new(&m.title).color(TEXT()).strong()).sense(egui::Sense::click())).clicked() {
                        clicked = Some(m.id);
                    }
                    ui.label(if m.year > 0 { m.year.to_string() } else { "TBA".into() });
                    ui.label(lib.profile_for(m).map(|p| p.name.clone()).unwrap_or_default());
                    ui.label(lib.file_for(m).map(|f| crate::quality::full_name(&f.quality)).unwrap_or_default());
                    ff::tag(ui, st.label(), st.color());
                    ui.label(lib.file_for(m).map(|f| crate::decision::human_size(f.size)).unwrap_or_default());
                    ui.label(common::date_label(m.digital_release.or(m.physical_release).or(m.in_cinemas), &self.cfg));
                    ui.label(common::when(m.added, &self.cfg));
                    ui.end_row();
                }
            });
        });
        if let Some(id) = clicked {
            self.click_movie(id);
        }
    }

    // ------------------------------------------------------------ details

    pub fn page_detail(&mut self, ui: &mut Ui) {
        let Some(mut st) = self.detail.take() else { return };
        let Some(movie) = self.core.movie(st.id) else {
            ui.label("This movie isn't in the library any more.");
            if ui.button("Back").clicked() {
                self.detail = None;
            } else {
                self.detail = Some(st);
            }
            return;
        };
        let lib = self.core.lib().clone();
        let file = lib.file_for(&movie).cloned();
        let profile = lib.profile_for(&movie).cloned();
        let state = common::movie_state(&lib, &movie, self.cfg.availability_delay_days);
        let mut back = false;

        // Fanart behind the header.
        let header_rect = egui::Rect::from_min_size(ui.max_rect().min, Vec2::new(ui.available_width(), 330.0));
        if let Some(tex) = self.images.fanart(movie.tmdb_id, &movie.fanart_path) {
            let p = ui.painter();
            p.image(tex, header_rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::from_white_alpha(38));
        }

        ui.horizontal_wrapped(|ui| {
            if ui.button("‹ Movies").clicked() {
                back = true;
            }
            ff::title(ui, "Status", 24.0, TEXT());
            ui.add_space(10.0);
            if ui.button(RichText::new("Search").color(GOOD()).strong()).on_hover_text("Automatic search: grab the best release").clicked() {
                self.core.run(Command::SearchMovies(vec![movie.id]));
            }
            if ui.button("Interactive search").clicked() {
                st.tab = Tab::Search;
                self.core.run(Command::InteractiveSearch(movie.id));
            }
            if ui.button("Refresh & scan").clicked() {
                self.core.run(Command::RefreshMovies(vec![movie.id]));
            }
            if ui.button("Rename").clicked() {
                let preview = crate::library::rename_preview(&self.core, &[movie.id]);
                self.dialog = Some(Dialog::Rename { ids: vec![movie.id], preview });
            }
            if ui.button("Manual import").clicked() {
                self.dialog = Some(Dialog::ManualImport(ManualImport { folder: String::new(), movie_hint: Some(movie.id), rows: vec![], mode: 0, scanning: None, movie_search: vec![] }));
            }
            if ui.button("Edit").clicked() {
                self.dialog = Some(Dialog::EditMovies(EditMovies {
                    ids: vec![movie.id],
                    monitored: if movie.monitored { Tri::Yes } else { Tri::No },
                    profile: movie.quality_profile_id,
                    availability: Some(movie.minimum_availability),
                    root: movie.root_folder.clone(),
                    move_files: false,
                    tags: movie.tags.clone(),
                    tag_mode: 2,
                    new_tag: String::new(),
                }));
            }
            if !movie.path.is_empty() && ui.button("Open folder").clicked() {
                super::open_path(&movie.path);
            }
            if ui.button(RichText::new("Delete").color(BAD())).clicked() {
                self.dialog = Some(Dialog::DeleteMovies { ids: vec![movie.id], delete_files: false, exclusion: false });
            }
        });
        ff::divider(ui);

        ui.horizontal_top(|ui| {
            let tex = self.images.poster(movie.tmdb_id, &movie.poster_path);
            common::poster(ui, tex, &movie.initials(), Vec2::new(180.0, 270.0), Color32::TRANSPARENT);
            ui.add_space(14.0);
            ui.vertical(|ui| {
                ui.set_width((ui.available_width() - 290.0).max(260.0));
                ff::title(ui, &movie.title, 28.0, TEXT());
                let mut line = vec![];
                if movie.year > 0 {
                    line.push(movie.year.to_string());
                }
                if movie.runtime > 0 {
                    line.push(common::runtime(movie.runtime, &self.cfg));
                }
                if !movie.certification.is_empty() {
                    line.push(movie.certification.clone());
                }
                line.push(movie.genres.join(", "));
                ui.label(RichText::new(line.join(" · ")).color(TEXT_DIM()));
                ui.horizontal(|ui| {
                    ff::tag(ui, state.label(), state.color());
                    let mut mon = movie.monitored;
                    if w::toggle_labeled(ui, &mut mon, "MONITORED", "UNMONITORED") {
                        self.core.save(Movie { monitored: mon, ..movie.clone() });
                    }
                    ff::tag(ui, movie.status.label(), LABEL());
                });
                ui.add_space(4.0);
                egui::ScrollArea::vertical().id_salt("overview").max_height(80.0).show(ui, |ui| {
                    ui.add(egui::Label::new(RichText::new(&movie.overview).color(TEXT())).wrap());
                });
                ui.add_space(4.0);
                let stat = |ui: &mut Ui, k: &str, v: String| {
                    ui.horizontal(|ui| {
                        ui.add_sized([150.0, 18.0], egui::Label::new(RichText::new(k).color(LABEL()).strong()));
                        ui.add(egui::Label::new(RichText::new(v).color(TEXT())).truncate());
                    });
                };
                stat(ui, "Profile", profile.as_ref().map(|p| p.name.clone()).unwrap_or_default());
                stat(ui, "Min. availability", movie.minimum_availability.label().into());
                stat(ui, "Path", if movie.path.is_empty() { "—".into() } else { movie.path.clone() });
                stat(ui, "Size on disk", file.as_ref().map(|f| crate::decision::human_size(f.size)).unwrap_or_else(|| "—".into()));
                stat(ui, "In cinemas", common::date_label(movie.in_cinemas, &self.cfg));
                stat(ui, "Digital / physical", format!("{} / {}", common::date_label(movie.digital_release, &self.cfg), common::date_label(movie.physical_release, &self.cfg)));
                if movie.ratings.tmdb > 0.0 {
                    stat(ui, "TMDb rating", format!("{:.1} ({} votes)", movie.ratings.tmdb, movie.ratings.tmdb_votes));
                }
                if !movie.studio.is_empty() {
                    stat(ui, "Studio", movie.studio.clone());
                }
                if let Some(c) = &movie.collection {
                    stat(ui, "Collection", c.title.clone());
                }
                ui.horizontal(|ui| {
                    ui.add_sized([150.0, 18.0], egui::Label::new(RichText::new("Tags").color(LABEL()).strong()));
                    let mut tags = movie.tags.clone();
                    if common::tags_editor(ui, "detail-tags", &self.core, &mut tags, &mut st.new_tag) {
                        self.core.save(Movie { tags, ..movie.clone() });
                    }
                });
                ui.horizontal(|ui| {
                    ui.hyperlink_to("TMDb", format!("https://www.themoviedb.org/movie/{}", movie.tmdb_id));
                    if !movie.imdb_id.is_empty() {
                        ui.hyperlink_to("IMDb", format!("https://www.imdb.com/title/{}", movie.imdb_id));
                    }
                    if !movie.youtube_trailer.is_empty() {
                        ui.hyperlink_to("Trailer", format!("https://www.youtube.com/watch?v={}", movie.youtube_trailer));
                    }
                    if !movie.website.is_empty() {
                        ui.hyperlink_to("Website", &movie.website);
                    }
                });
            });
            // Materia: custom formats on the current file.
            ui.vertical(|ui| {
                ui.set_width(270.0);
                ff::window(ui, |ui| {
                    ui.set_width(238.0);
                    ff::title(ui, "Materia", 18.0, TEXT());
                    ui.label(RichText::new("custom formats on this file").color(TEXT_DIM()).size(12.0));
                    ui.add_space(4.0);
                    match (&file, &profile) {
                        (Some(f), Some(p)) => {
                            let fmts = crate::decision::file_formats(&lib, f, &movie);
                            if fmts.is_empty() {
                                ui.label(common::dim_text("No custom formats match."));
                            }
                            for (i, (id, name)) in fmts.iter().enumerate() {
                                let score = p.format_items.iter().find(|x| x.format_id == *id).map(|x| x.score).unwrap_or(0);
                                ui.horizontal(|ui| {
                                    materia_orb(ui, i, score);
                                    ui.label(RichText::new(name).color(TEXT()).size(13.0));
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        ui.label(RichText::new(format!("{score:+}")).color(if score >= 0 { GOOD() } else { BAD() }).size(12.0));
                                    });
                                });
                            }
                            let ids: Vec<Id> = fmts.iter().map(|x| x.0).collect();
                            let total = p.score_of(&ids);
                            ff::divider(ui);
                            ui.horizontal(|ui| {
                                ff::stat_label(ui, "Format score");
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ff::title(ui, &format!("{total:+}"), 18.0, if total >= 0 { GOOD() } else { BAD() });
                                });
                            });
                            let target = p.cutoff_format_score.max(1) as f32;
                            ff::stat_bar(ui, (total as f32 / target).clamp(0.0, 1.0), 230.0, Color32::from_rgb(70, 130, 230), GOOD());
                            let cutoff_met = !crate::decision::cutoff_not_met(p, &f.quality, &ids, None);
                            ui.label(RichText::new(if cutoff_met { "Cutoff met. Upgrades stop here." } else { "Below cutoff. Turks will look for upgrades." }).color(TEXT_DIM()).size(12.0));
                        }
                        _ => {
                            ui.label(common::dim_text("No file yet."));
                        }
                    }
                });
            });
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            for (t, label) in [(Tab::Overview, "File"), (Tab::Files, "Files on disk"), (Tab::History, "History"), (Tab::Search, "Interactive search"), (Tab::Cast, "Cast & crew")] {
                if w::pill(ui, label, st.tab == t).clicked() {
                    st.tab = t;
                    if t == Tab::Search && !self.core.status().interactive.contains_key(&movie.id) {
                        self.core.run(Command::InteractiveSearch(movie.id));
                    }
                    if t == Tab::History {
                        st.history = None;
                    }
                }
            }
        });
        ff::divider(ui);
        match st.tab {
            Tab::Overview => self.detail_file(ui, &movie, file.as_ref()),
            Tab::Files => self.detail_disk(ui, &movie),
            Tab::History => {
                if st.history.is_none() {
                    st.history = Some(self.core.db.history_for_movie(movie.id));
                }
                self.history_list(ui, st.history.as_deref().unwrap_or(&[]), false);
            }
            Tab::Search => self.detail_search(ui, &movie, &mut st),
            Tab::Cast => self.detail_cast(ui, &movie),
        }
        if back {
            self.detail = None;
        } else if self.detail.is_none() {
            self.detail = Some(st);
        }
    }

    fn detail_file(&mut self, ui: &mut Ui, movie: &Movie, file: Option<&MovieFile>) {
        let Some(f) = file else {
            ui.label(common::dim_text("No file yet. Search for one, or use Manual import if it's already downloaded."));
            return;
        };
        let row = |ui: &mut Ui, k: &str, v: String| {
            ui.horizontal(|ui| {
                ui.add_sized([150.0, 18.0], egui::Label::new(RichText::new(k).color(LABEL()).strong()));
                ui.add(egui::Label::new(RichText::new(v).color(TEXT())).wrap());
            });
        };
        row(ui, "File", f.relative_path.clone());
        row(ui, "Quality", crate::quality::full_name(&f.quality));
        row(ui, "Size", crate::decision::human_size(f.size));
        row(ui, "Release group", f.release_group.clone());
        if !f.edition.is_empty() {
            row(ui, "Edition", f.edition.clone());
        }
        row(ui, "Languages", if f.languages.is_empty() { movie.original_language.clone() } else { f.languages.join(", ") });
        let mi = &f.media_info;
        row(ui, "Video", [mi.video_codec.clone(), if mi.video_bit_depth > 0 { format!("{}-bit", mi.video_bit_depth) } else { String::new() }, mi.video_dynamic_range.clone(), mi.resolution.clone()].iter().filter(|s| !s.is_empty()).cloned().collect::<Vec<_>>().join(" · "));
        row(ui, "Audio", format!("{} {}", mi.audio_codec, if mi.audio_channels > 0.0 { format!("{:.1}", mi.audio_channels) } else { String::new() }));
        if !mi.audio_languages.is_empty() || !mi.subtitles.is_empty() {
            row(ui, "Audio / subs", format!("{} / {}", mi.audio_languages.join(", "), mi.subtitles.join(", ")));
        }
        row(ui, "Media info from", if mi.from_ffprobe { "ffprobe".into() } else { "the file name".to_string() });
        row(ui, "Scene name", f.scene_name.clone());
        row(ui, "Added", common::when(f.date_added, &self.cfg));
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui.button(RichText::new("Delete file").color(BAD())).clicked() {
                self.dialog = Some(Dialog::Confirm {
                    title: "Delete file".into(),
                    text: format!("Delete {}? It goes to the recycle bin if one is set.", f.relative_path),
                    action: common::Confirm::DeleteFile(f.id),
                });
            }
            if ui.button("Show in folder").clicked() {
                super::open_path(&movie.path);
            }
        });
    }

    fn detail_disk(&mut self, ui: &mut Ui, movie: &Movie) {
        let dir = std::path::Path::new(&movie.path);
        if !dir.is_dir() {
            ui.label(common::dim_text("The movie folder doesn't exist yet."));
            return;
        }
        egui::ScrollArea::vertical().show(ui, |ui| {
            for e in walkdir::WalkDir::new(dir).max_depth(3).into_iter().flatten().filter(|e| e.file_type().is_file()) {
                let size = e.metadata().map(|m| m.len()).unwrap_or(0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(e.path().strip_prefix(dir).unwrap_or(e.path()).display().to_string()).color(TEXT()));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(common::dim_text(&crate::decision::human_size(size)));
                    });
                });
            }
        });
    }

    fn detail_cast(&mut self, ui: &mut Ui, movie: &Movie) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (title, people) in [("Cast", &movie.cast), ("Crew", &movie.crew)] {
                w::section(ui, title);
                ui.horizontal_wrapped(|ui| {
                    for p in people.iter() {
                        ui.vertical(|ui| {
                            ui.set_width(96.0);
                            let tex = self.images.person(p.person_id, &p.profile_path);
                            let initials: String = p.name.split_whitespace().filter_map(|w| w.chars().next()).take(2).collect();
                            common::poster(ui, tex, &initials, Vec2::new(86.0, 120.0), Color32::TRANSPARENT);
                            ui.add(egui::Label::new(RichText::new(&p.name).color(TEXT()).size(12.0)).truncate());
                            ui.add(egui::Label::new(RichText::new(&p.role).color(TEXT_DIM()).size(11.0)).truncate());
                        });
                    }
                });
            }
        });
    }

    fn detail_search(&mut self, ui: &mut Ui, movie: &Movie, st: &mut DetailState) {
        let result = self.core.status().interactive.get(&movie.id).cloned();
        let running = self.core.is_running(&Command::InteractiveSearch(movie.id));
        ui.horizontal(|ui| {
            if ui.add_enabled(!running, egui::Button::new(if running { "Searching…" } else { "Search again" })).clicked() {
                self.core.run(Command::InteractiveSearch(movie.id));
            }
            egui::ComboBox::from_id_salt("isearch-sort")
                .selected_text(["Best first", "Size", "Age", "Seeders", "Quality", "Score"][st.search_sort as usize])
                .show_ui(ui, |ui| {
                    for (i, l) in ["Best first", "Size", "Age", "Seeders", "Quality", "Score"].iter().enumerate() {
                        ui.selectable_value(&mut st.search_sort, i as u8, *l);
                    }
                });
            ui.checkbox(&mut st.show_rejected, "Show rejected");
            if let Some(r) = &result {
                ui.label(common::dim_text(&format!("{} releases · {} acceptable", r.candidates.len(), r.candidates.iter().filter(|c| c.approved()).count())));
            }
        });
        let Some(r) = result else {
            if running {
                ui.spinner();
            }
            return;
        };
        for e in &r.errors {
            ui.label(RichText::new(e).color(WARN()).size(12.0));
        }
        if r.running || running {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(common::dim_text("Asking your indexers…"));
            });
        }
        let lib = self.core.lib().clone();
        let profile = lib.profile_for(movie).cloned();
        let mut cands: Vec<&crate::release::Candidate> = r.candidates.iter().filter(|c| st.show_rejected || c.approved()).collect();
        match st.search_sort {
            1 => cands.sort_by_key(|c| std::cmp::Reverse(c.release.size)),
            2 => cands.sort_by_key(|c| c.release.publish_date.map(|d| std::cmp::Reverse(d.timestamp()))),
            3 => cands.sort_by_key(|c| std::cmp::Reverse(c.release.seeders.unwrap_or(-1))),
            4 => cands.sort_by_key(|c| std::cmp::Reverse(profile.as_ref().and_then(|p| p.rank(c.parsed.quality.quality)).unwrap_or(0))),
            5 => cands.sort_by_key(|c| std::cmp::Reverse(c.score)),
            _ => {}
        }
        let mut grab = None;
        egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
            egui::Grid::new("isearch").striped(true).spacing([10.0, 5.0]).show(ui, |ui| {
                for h in ["", "Age", "Title", "Indexer", "Size", "Peers", "Language", "Quality", "Score", ""] {
                    ff::title(ui, h, 13.0, LABEL());
                }
                ui.end_row();
                for c in cands {
                    if c.approved() {
                        ff::badge(ui, ff::Badge::Good, 14.0).on_hover_text("Acceptable");
                    } else {
                        let tip = c.rejections.iter().map(|r| format!("• {}", r.reason)).collect::<Vec<_>>().join("\n");
                        ff::badge(ui, if c.permanently_rejected() { ff::Badge::Bad } else { ff::Badge::Warn }, 14.0).on_hover_text(tip);
                    }
                    let age = c.release.age_days();
                    ui.label(if age < 1.0 { format!("{:.0}h", age * 24.0) } else { format!("{:.0}d", age) });
                    let title = sized_left(ui, 340.0, egui::Label::new(RichText::new(&c.release.title).color(if c.approved() { TEXT() } else { TEXT_DIM() }).size(13.0)).truncate().sense(egui::Sense::click()));
                    let mut tip = c.release.title.clone();
                    if !c.format_names.is_empty() {
                        tip.push_str(&format!("\n\nCustom formats: {}", c.format_names.join(", ")));
                    }
                    if !c.release.indexer_flags.is_empty() {
                        tip.push_str(&format!("\nFlags: {}", c.release.indexer_flags.join(", ")));
                    }
                    if !c.rejections.is_empty() {
                        tip.push_str(&format!("\n\n{}", c.rejections.iter().map(|r| format!("• {}", r.reason)).collect::<Vec<_>>().join("\n")));
                    }
                    if title.on_hover_text(tip).clicked() && !c.release.info_url.is_empty() {
                        super::open_url(&c.release.info_url);
                    }
                    ui.label(RichText::new(&c.release.indexer).size(12.0));
                    ui.label(crate::decision::human_size(c.release.size));
                    ui.label(match c.release.protocol {
                        Protocol::Torrent => c.release.seeders.map(|s| format!("{s} / {}", c.release.peers.unwrap_or(0))).unwrap_or_else(|| "?".into()),
                        Protocol::Usenet => "usenet".into(),
                    });
                    ui.label(RichText::new(if c.parsed.languages.is_empty() { movie.original_language.clone() } else { c.parsed.languages.join(", ") }).size(12.0));
                    ff::tag(ui, &crate::quality::full_name(&c.parsed.quality), LABEL());
                    ui.label(RichText::new(format!("{:+}", c.score)).color(if c.score >= 0 { GOOD() } else { BAD() }));
                    let label = if c.approved() { "Grab" } else { "Grab anyway" };
                    if ui.small_button(RichText::new(label).color(if c.approved() { GOOD() } else { WARN() })).clicked() {
                        grab = Some((*c).clone());
                    }
                    ui.end_row();
                }
            });
        });
        if let Some(mut c) = grab {
            c.movie_id = Some(movie.id);
            crate::search::grab_in_background(&self.core, c);
            self.notify("Sending to the download client…");
        }
    }

    pub fn history_list(&mut self, ui: &mut Ui, items: &[History], show_movie: bool) {
        let lib = self.core.lib().clone();
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            egui::Grid::new(if show_movie { "history-all" } else { "history-movie" }).striped(true).spacing([12.0, 6.0]).show(ui, |ui| {
                let heads: &[&str] = if show_movie { &["When", "Event", "Movie", "Release", "Quality", "Details"] } else { &["When", "Event", "Release", "Quality", "Details"] };
                for h in heads {
                    ff::title(ui, h, 13.0, LABEL());
                }
                ui.end_row();
                for h in items {
                    ui.label(RichText::new(common::when(h.date, &self.cfg)).color(TEXT_DIM()).size(12.0));
                    let color = match h.event {
                        HistoryEvent::Grabbed => LABEL(),
                        HistoryEvent::Imported => GOOD(),
                        HistoryEvent::DownloadFailed => BAD(),
                        HistoryEvent::FileDeleted => WARN(),
                        _ => TEXT_DIM(),
                    };
                    ff::tag(ui, h.event.label(), color);
                    if show_movie {
                        let name = lib.movies.get(&h.movie_id).map(|m| m.display()).unwrap_or_else(|| "(removed)".into());
                        if ui.add(egui::Label::new(RichText::new(name).color(TEXT()).strong()).sense(egui::Sense::click()).truncate()).clicked() && lib.movies.contains_key(&h.movie_id) {
                            self.open_movie(h.movie_id);
                        }
                    }
                    sized_left(ui, 320.0, egui::Label::new(RichText::new(&h.source_title).color(TEXT()).size(12.0)).truncate());
                    ui.label(RichText::new(crate::quality::full_name(&h.quality)).size(12.0));
                    let details = h.data.iter().filter(|(_, v)| !v.is_empty()).map(|(k, v)| format!("{k}: {v}")).collect::<Vec<_>>().join("\n");
                    ui.label(common::dim_text("info")).on_hover_text(if details.is_empty() { "—".into() } else { details });
                    ui.end_row();
                }
            });
        });
    }

    // ------------------------------------------------------------ dialogs

    pub fn edit_movies_dialog(&mut self, ctx: &egui::Context, s: &mut EditMovies) -> bool {
        let mut result = None;
        let lib = self.core.lib().clone();
        let close = common::modal(ctx, "edit-movies", 520.0, |ui| {
            common::heading(ui, if s.ids.len() == 1 { "Edit movie" } else { "Edit movies" });
            if s.ids.len() > 1 {
                ui.label(common::dim_text(&format!("{} movies selected. Leave a field as it is to keep each movie's own value.", s.ids.len())));
            }
            w::row(ui, "Monitored", "Monitored movies are searched for and upgraded.", |ui| {
                egui::ComboBox::from_id_salt("ed-mon").selected_text(match s.monitored { Tri::Keep => "No change", Tri::Yes => "Monitored", Tri::No => "Unmonitored" }).show_ui(ui, |ui| {
                    ui.selectable_value(&mut s.monitored, Tri::Keep, "No change");
                    ui.selectable_value(&mut s.monitored, Tri::Yes, "Monitored");
                    ui.selectable_value(&mut s.monitored, Tri::No, "Unmonitored");
                });
            });
            w::row(ui, "Quality profile", "Which qualities are wanted, and when to stop upgrading.", |ui| {
                if s.profile == 0 && ui.small_button("change").clicked() {
                    s.profile = lib.profiles.keys().next().copied().unwrap_or(0);
                }
                if s.profile != 0 {
                    common::profile_combo(ui, "ed-prof", &lib, &mut s.profile);
                }
            });
            w::row(ui, "Minimum availability", "When a movie counts as released enough to search for.", |ui| {
                let mut a = s.availability.unwrap_or_default();
                if s.availability.is_none() {
                    if ui.small_button("change").clicked() {
                        s.availability = Some(a);
                    }
                } else if common::availability_combo(ui, "ed-avail", &mut a) {
                    s.availability = Some(a);
                }
            });
            w::row(ui, "Root folder", "Moving to another root folder can move the files too.", |ui| {
                common::root_combo(ui, "ed-root", &lib, &mut s.root);
            });
            ui.checkbox(&mut s.move_files, "Move the movie folders to the new root folder");
            w::section(ui, "Tags");
            ui.horizontal(|ui| {
                for (i, l) in ["Add", "Remove", "Replace"].iter().enumerate() {
                    if w::pill(ui, l, s.tag_mode == i as u8).clicked() {
                        s.tag_mode = i as u8;
                    }
                }
            });
            common::tags_editor(ui, "ed-tags", &self.core, &mut s.tags, &mut s.new_tag);
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button(RichText::new("Save").color(GOOD()).strong()).clicked() {
                    result = Some(true);
                }
                if ui.button("Cancel").clicked() {
                    result = Some(false);
                }
            });
        });
        if result == Some(true) {
            let core = self.core.clone();
            let (ids, mon, prof, avail, root, mv, tags, mode) = (s.ids.clone(), s.monitored, s.profile, s.availability, s.root.clone(), s.move_files, s.tags.clone(), s.tag_mode);
            std::thread::spawn(move || {
                for id in ids {
                    let Some(mut m) = core.movie(id) else { continue };
                    match mon {
                        Tri::Yes => m.monitored = true,
                        Tri::No => m.monitored = false,
                        Tri::Keep => {}
                    }
                    if prof != 0 {
                        m.quality_profile_id = prof;
                    }
                    if let Some(a) = avail {
                        m.minimum_availability = a;
                    }
                    match mode {
                        0 => {
                            for t in &tags {
                                if !m.tags.contains(t) {
                                    m.tags.push(*t);
                                }
                            }
                        }
                        1 => m.tags.retain(|t| !tags.contains(t)),
                        _ => m.tags = tags.clone(),
                    }
                    let new_root = root.clone();
                    let root_changed = !new_root.is_empty() && new_root != m.root_folder;
                    let m = core.save(m);
                    if root_changed {
                        let folder = std::path::Path::new(&m.path).file_name().map(|n| n.to_os_string()).unwrap_or_else(|| crate::naming::folder_name(&m, &core.cfg()).into());
                        let new_path = std::path::Path::new(&new_root).join(folder).display().to_string();
                        if let Err(e) = crate::library::move_movie(&core, m.id, &new_path, mv) {
                            core.toast(format!("Couldn't move {}: {e}", m.title));
                        }
                    }
                }
                core.toast("Saved.");
            });
        }
        result.is_none() && !close
    }

    pub fn manual_import_dialog(&mut self, ctx: &egui::Context, s: &mut ManualImport) -> bool {
        if let Some(rows) = common::take(&s.scanning) {
            s.scanning = None;
            s.rows = rows.into_iter().map(|d| (d.approved() || d.movie_id.is_some(), d)).collect();
            s.movie_search = vec![String::new(); s.rows.len()];
        }
        let mut result = None;
        let lib = self.core.lib().clone();
        let close = common::modal(ctx, "manual-import", 860.0, |ui| {
            common::heading(ui, "Manual import");
            ui.horizontal(|ui| {
                ui.label(RichText::new("Folder").color(LABEL()).strong());
                w::folder_field(ui, &mut s.folder, 460.0);
                let busy = s.scanning.is_some();
                if ui.add_enabled(!busy && !s.folder.trim().is_empty(), egui::Button::new(if busy { "Scanning…" } else { "Scan" })).clicked() {
                    let core = self.core.clone();
                    let (folder, hint) = (std::path::PathBuf::from(s.folder.trim()), s.movie_hint);
                    s.scanning = Some(common::spawn(ctx, move || crate::import::manual_candidates(&core, &folder, hint)));
                }
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("How").color(LABEL()).strong());
                for (i, l) in ["Move", "Copy", "Hardlink (or copy)"].iter().enumerate() {
                    if w::pill(ui, l, s.mode == i as u8).clicked() {
                        s.mode = i as u8;
                    }
                }
            });
            ff::divider(ui);
            if s.rows.is_empty() {
                ui.label(common::dim_text("Pick a folder (or a single file) and press Scan."));
            }
            egui::ScrollArea::vertical().max_height(380.0).show(ui, |ui| {
                egui::Grid::new("mi-grid").striped(true).spacing([10.0, 6.0]).show(ui, |ui| {
                    for h in ["", "File", "Movie", "Quality", "Size", "Problems"] {
                        ff::title(ui, h, 13.0, LABEL());
                    }
                    ui.end_row();
                    for (i, (on, d)) in s.rows.iter_mut().enumerate() {
                        ui.checkbox(on, "");
                        ui.add(egui::Label::new(RichText::new(d.path.file_name().unwrap_or_default().to_string_lossy()).color(TEXT()).size(12.0)).truncate()).on_hover_text(d.path.display().to_string());
                        let current = d.movie_id.and_then(|id| lib.movies.get(&id)).map(|m| m.display()).unwrap_or_else(|| "Choose movie…".into());
                        egui::ComboBox::from_id_salt(("mi-movie", i)).selected_text(current).width(220.0).show_ui(ui, |ui| {
                            ui.add(egui::TextEdit::singleline(&mut s.movie_search[i]).hint_text("filter"));
                            let q = s.movie_search[i].to_lowercase();
                            for m in lib.movies.values().filter(|m| q.is_empty() || m.title.to_lowercase().contains(&q)).take(60) {
                                if ui.selectable_label(d.movie_id == Some(m.id), m.display()).clicked() {
                                    d.movie_id = Some(m.id);
                                    *on = true;
                                }
                            }
                        });
                        egui::ComboBox::from_id_salt(("mi-q", i)).selected_text(crate::quality::name(d.quality.quality)).width(120.0).show_ui(ui, |ui| {
                            for q in crate::quality::ALL {
                                ui.selectable_value(&mut d.quality.quality, q.id, q.name);
                            }
                        });
                        ui.label(crate::decision::human_size(d.size));
                        let problems: Vec<&String> = d.rejections.iter().filter(|r| !r.starts_with("Couldn't tell")).collect();
                        if problems.is_empty() {
                            ui.label(RichText::new("OK").color(GOOD()));
                        } else {
                            ui.label(RichText::new("check").color(WARN())).on_hover_text(problems.iter().map(|p| format!("• {p}")).collect::<Vec<_>>().join("\n"));
                        }
                        ui.end_row();
                    }
                });
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let n = s.rows.iter().filter(|(on, d)| *on && d.movie_id.is_some()).count();
                if ui.add_enabled(n > 0, egui::Button::new(RichText::new(format!("Import {n}")).color(GOOD()).strong())).clicked() {
                    result = Some(true);
                }
                if ui.button("Close").clicked() {
                    result = Some(false);
                }
            });
        });
        if result == Some(true) {
            let core = self.core.clone();
            let mode = match s.mode {
                0 => crate::import::Transfer::Move,
                1 => crate::import::Transfer::Copy,
                _ => crate::import::Transfer::HardlinkOrCopy,
            };
            let rows: Vec<crate::import::ImportDecision> = s.rows.iter().filter(|(on, d)| *on && d.movie_id.is_some()).map(|(_, d)| d.clone()).collect();
            std::thread::spawn(move || {
                let mut ok = 0;
                for mut d in rows {
                    d.rejections.clear();
                    // Re-check upgrade status against the chosen movie.
                    let lib = core.lib().clone();
                    if let Some(m) = d.movie_id.and_then(|id| lib.movies.get(&id)) {
                        d.is_upgrade = lib.file_for(m).is_some();
                    }
                    match crate::import::import(&core, &d, mode, None) {
                        Ok(_) => ok += 1,
                        Err(e) => core.toast(format!("Couldn't import {}: {e}", d.path.display())),
                    }
                }
                core.toast(format!("Imported {ok} file(s)."));
            });
        }
        result.is_none() && !close
    }
}

/// A shaded materia orb; colour cycles like FFVII's materia types.
fn materia_orb(ui: &mut Ui, i: usize, score: i32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(16.0), egui::Sense::hover());
    let colors = [
        Color32::from_rgb(60, 200, 90),
        Color32::from_rgb(230, 200, 50),
        Color32::from_rgb(58, 138, 230),
        Color32::from_rgb(200, 60, 200),
        Color32::from_rgb(220, 70, 60),
    ];
    let base = if score < 0 { Color32::from_rgb(220, 70, 60) } else { colors[i % colors.len()] };
    let p = ui.painter();
    let c = rect.center();
    p.circle_filled(c + Vec2::new(1.0, 1.5), 7.5, Color32::from_black_alpha(140));
    p.circle(c, 7.0, base, egui::Stroke::new(1.0, Color32::from_rgb(30, 32, 50)));
    p.circle_filled(c + Vec2::new(-2.5, -2.5), 2.5, Color32::from_white_alpha(200));
}

/// A label in a fixed-width, left-aligned cell (for grid columns).
fn sized_left(ui: &mut Ui, width: f32, label: egui::Label) -> egui::Response {
    ui.allocate_ui_with_layout(egui::vec2(width, 18.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.set_width(width);
        ui.add(label)
    })
    .inner
}
