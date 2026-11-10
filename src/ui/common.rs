//! Shared UI pieces: dialogs, posters, pickers and the first-run wizard.

use std::sync::{Arc, Mutex};

use eframe::egui::{self, Color32, RichText, Ui, Vec2};

use super::theme::{self as ff, BAD, GOOD, LABEL, TEXT, TEXT_DIM, WARN};
use super::widgets as w;
use super::App;
use crate::db::Library;
use crate::model::*;

/// Result of work done on a background thread.
pub type Task<T> = Arc<Mutex<Option<T>>>;

pub fn spawn<T: Send + 'static>(ctx: &egui::Context, f: impl FnOnce() -> T + Send + 'static) -> Task<T> {
    let slot: Task<T> = Arc::new(Mutex::new(None));
    let s2 = Arc::clone(&slot);
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let r = f();
        *s2.lock().unwrap() = Some(r);
        ctx.request_repaint();
    });
    slot
}

pub fn take<T>(t: &Option<Task<T>>) -> Option<T> {
    t.as_ref().and_then(|t| t.lock().unwrap().take())
}

/// A centered FF window over a dimmed backdrop. Returns true when it should close.
pub fn modal(ctx: &egui::Context, id: &str, width: f32, add: impl FnOnce(&mut Ui)) -> bool {
    let r = egui::Modal::new(egui::Id::new(id)).frame(egui::Frame::NONE).backdrop_color(Color32::from_black_alpha(160)).show(ctx, |ui| {
        ff::window(ui, |ui| {
            ui.set_width(width);
            add(ui)
        })
    });
    r.should_close()
}

pub fn heading(ui: &mut Ui, text: &str) {
    ff::title(ui, text, 24.0, TEXT);
    ff::divider(ui);
}

// ---------------------------------------------------------------- posters

/// A poster with the FF window's silver border; initials when there's no image.
pub fn poster(ui: &mut Ui, tex: Option<egui::TextureId>, initials: &str, size: Vec2, accent: Color32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
    let painter = ui.painter();
    painter.rect_filled(rect.translate(Vec2::new(3.0, 4.0)), egui::CornerRadius::same(4), Color32::from_black_alpha(120));
    match tex {
        Some(t) => {
            painter.image(t, rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
        }
        None => {
            let mut mesh = egui::Mesh::default();
            let (a, b) = (Color32::from_rgb(46, 86, 190), Color32::from_rgb(6, 8, 58));
            mesh.colored_vertex(rect.left_top(), a);
            mesh.colored_vertex(rect.right_top(), Color32::from_rgb(24, 40, 146));
            mesh.colored_vertex(rect.right_bottom(), b);
            mesh.colored_vertex(rect.left_bottom(), Color32::from_rgb(18, 30, 120));
            mesh.add_triangle(0, 1, 2);
            mesh.add_triangle(0, 2, 3);
            painter.add(egui::Shape::mesh(mesh));
            ff::shadow_text(ui, rect.center(), egui::Align2::CENTER_CENTER, initials, egui::FontId::proportional((size.x * 0.28).max(14.0)), TEXT);
        }
    }
    let painter = ui.painter();
    painter.rect_stroke(rect, egui::CornerRadius::same(4), egui::Stroke::new(2.0, ff::BORDER_LIGHT), egui::StrokeKind::Inside);
    if accent != Color32::TRANSPARENT {
        let bar = egui::Rect::from_min_size(egui::pos2(rect.left() + 2.0, rect.bottom() - 6.0), Vec2::new(rect.width() - 4.0, 4.0));
        painter.rect_filled(bar, egui::CornerRadius::same(1), accent);
    }
    if resp.hovered() {
        painter.rect_stroke(rect.expand(2.0), egui::CornerRadius::same(5), egui::Stroke::new(2.0, LABEL), egui::StrokeKind::Outside);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

// ---------------------------------------------------------------- movie status

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MovieState {
    Downloaded,
    Downloading,
    Missing,
    NotAvailable,
    Unmonitored,
    CutoffUnmet,
}

impl MovieState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Downloaded => "DOWNLOADED",
            Self::Downloading => "DOWNLOADING",
            Self::Missing => "MISSING",
            Self::NotAvailable => "NOT AVAILABLE",
            Self::Unmonitored => "UNMONITORED",
            Self::CutoffUnmet => "UPGRADE",
        }
    }
    pub fn color(self) -> Color32 {
        match self {
            Self::Downloaded => GOOD,
            Self::Downloading => WARN,
            Self::Missing => BAD,
            Self::NotAvailable => LABEL,
            Self::Unmonitored => Color32::from_rgb(150, 156, 180),
            Self::CutoffUnmet => WARN,
        }
    }
}

pub fn movie_state(lib: &Library, m: &Movie, delay: i64) -> MovieState {
    if lib.active_downloads().any(|d| d.movie_id == m.id && d.state != DownloadState::Failed) {
        return MovieState::Downloading;
    }
    if let Some(f) = lib.file_for(m) {
        if m.monitored {
            if let Some(p) = lib.profile_for(m) {
                let fmts: Vec<Id> = crate::decision::file_formats(lib, f, m).into_iter().map(|x| x.0).collect();
                if crate::decision::cutoff_not_met(p, &f.quality, &fmts, None) {
                    return MovieState::CutoffUnmet;
                }
            }
        }
        return MovieState::Downloaded;
    }
    if !m.monitored {
        return MovieState::Unmonitored;
    }
    if m.is_available(delay) {
        MovieState::Missing
    } else {
        MovieState::NotAvailable
    }
}

// ---------------------------------------------------------------- pickers

pub fn profile_combo(ui: &mut Ui, id: &str, lib: &Library, value: &mut Id) -> bool {
    let mut changed = false;
    let current = lib.profiles.get(value).map(|p| p.name.clone()).unwrap_or_else(|| "Choose…".into());
    egui::ComboBox::from_id_salt(id).selected_text(current).width(200.0).show_ui(ui, |ui| {
        for p in lib.profiles.values() {
            changed |= ui.selectable_value(value, p.id, &p.name).changed();
        }
    });
    changed
}

pub fn root_combo(ui: &mut Ui, id: &str, lib: &Library, value: &mut String) -> bool {
    let mut changed = false;
    let text = if value.is_empty() { "Choose…".to_string() } else { value.clone() };
    egui::ComboBox::from_id_salt(id).selected_text(text).width(260.0).show_ui(ui, |ui| {
        for r in lib.root_folders.values() {
            let free = crate::import::free_space(std::path::Path::new(&r.path)).map(|f| format!("  ({} free)", crate::decision::human_size(f))).unwrap_or_default();
            changed |= ui.selectable_value(value, r.path.clone(), format!("{}{free}", r.path)).changed();
        }
    });
    changed
}

pub fn availability_combo(ui: &mut Ui, id: &str, value: &mut Availability) -> bool {
    let mut changed = false;
    egui::ComboBox::from_id_salt(id).selected_text(value.label()).show_ui(ui, |ui| {
        for a in Availability::ALL {
            changed |= ui.selectable_value(value, a, a.label()).changed();
        }
    });
    changed
}

pub fn monitor_combo(ui: &mut Ui, id: &str, value: &mut ListMonitor) -> bool {
    let label = |m: ListMonitor| match m {
        ListMonitor::MovieOnly => "Movie only",
        ListMonitor::MovieAndCollection => "Movie and collection",
        ListMonitor::None => "None",
    };
    let mut changed = false;
    egui::ComboBox::from_id_salt(id).selected_text(label(*value)).show_ui(ui, |ui| {
        for m in [ListMonitor::MovieOnly, ListMonitor::MovieAndCollection, ListMonitor::None] {
            changed |= ui.selectable_value(value, m, label(m)).changed();
        }
    });
    changed
}

/// Tag chips with an add box. New labels create tags.
pub fn tags_editor(ui: &mut Ui, id: &str, core: &crate::core::Core, tags: &mut Vec<Id>, new_tag: &mut String) -> bool {
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        let lib = core.lib().clone();
        let mut remove = None;
        for t in tags.iter() {
            let label = lib.tag_label(*t);
            if ui.add(egui::Button::new(RichText::new(format!("{label}  ×")).color(LABEL).size(13.0)).corner_radius(10.0)).on_hover_text("Remove tag").clicked() {
                remove = Some(*t);
            }
        }
        if let Some(r) = remove {
            tags.retain(|x| *x != r);
            changed = true;
        }
        let resp = ui.add(egui::TextEdit::singleline(new_tag).hint_text("add tag").desired_width(110.0).id_salt(id));
        let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if (enter || ui.small_button("+").clicked()) && !new_tag.trim().is_empty() {
            let label = new_tag.trim().to_lowercase();
            let tid = match lib.tag_id(&label) {
                Some(t) => t,
                None => core.save(Tag { id: 0, label }).id,
            };
            if !tags.contains(&tid) {
                tags.push(tid);
                changed = true;
            }
            new_tag.clear();
        }
        // Existing tags as quick picks.
        let others: Vec<&Tag> = lib.tags.values().filter(|t| !tags.contains(&t.id)).take(8).collect();
        if !others.is_empty() && new_tag.is_empty() {
            egui::ComboBox::from_id_salt(format!("{id}-pick")).selected_text("existing…").width(90.0).show_ui(ui, |ui| {
                for t in others {
                    if ui.selectable_label(false, &t.label).clicked() {
                        tags.push(t.id);
                        changed = true;
                    }
                }
            });
        }
    });
    changed
}

pub fn result_line(ui: &mut Ui, r: &Option<Result<String, String>>) {
    match r {
        Some(Ok(m)) => {
            ui.horizontal(|ui| {
                ff::badge(ui, ff::Badge::Good, 16.0);
                ui.add(egui::Label::new(RichText::new(m).color(GOOD)).wrap());
            });
        }
        Some(Err(e)) => {
            ui.horizontal(|ui| {
                ff::badge(ui, ff::Badge::Bad, 16.0);
                ui.add(egui::Label::new(RichText::new(e).color(BAD)).wrap());
            });
        }
        None => {}
    }
}

pub fn date_label(d: Option<chrono::NaiveDate>, cfg: &crate::config::Config) -> String {
    match d {
        Some(d) => d.format(&cfg.short_date_format).to_string(),
        None => "—".into(),
    }
}

pub fn when(t: Option<chrono::DateTime<chrono::Utc>>, cfg: &crate::config::Config) -> String {
    let Some(t) = t else { return "—".into() };
    let local = t.with_timezone(&chrono::Local);
    if cfg.show_relative_dates {
        let ago = chrono::Utc::now() - t;
        if ago.num_seconds() < 60 && ago.num_seconds() >= 0 {
            return "just now".into();
        }
        if ago.num_minutes() < 60 && ago.num_minutes() >= 0 {
            return format!("{} min ago", ago.num_minutes());
        }
        if ago.num_hours() < 24 && ago.num_hours() >= 0 {
            return format!("{} h ago", ago.num_hours());
        }
        if ago.num_days() < 7 && ago.num_days() >= 0 {
            return format!("{} days ago", ago.num_days());
        }
    }
    let time = if cfg.time_24h { local.format("%H:%M") } else { local.format("%-I:%M %p") };
    format!("{} {time}", local.format(&cfg.short_date_format))
}

pub fn runtime(mins: i32, cfg: &crate::config::Config) -> String {
    if mins <= 0 {
        return String::new();
    }
    if cfg.runtime_hours_minutes {
        format!("{}h {}m", mins / 60, mins % 60)
    } else {
        format!("{mins} min")
    }
}

// ---------------------------------------------------------------- dialogs

pub enum Dialog {
    Confirm { title: String, text: String, action: Confirm },
    DeleteMovies { ids: Vec<Id>, delete_files: bool, exclusion: bool },
    EditMovies(super::movies::EditMovies),
    ManualImport(super::movies::ManualImport),
    Rename { ids: Vec<Id>, preview: Vec<crate::library::RenamePreview> },
    AddMovie(super::add::AddDialog),
    RemoveQueue { id: Id, title: String, from_client: bool, blocklist: bool, search: bool },
    Settings(super::settings::Editor),
}

pub enum Confirm {
    DeleteFile(Id),
    ClearHistory,
    ClearBlocklist,
    RestoreBackup(std::path::PathBuf),
    DeleteBackup(std::path::PathBuf),
}

impl App {
    pub fn show_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.dialog.take() else { return };
        let keep = match &mut d {
            Dialog::Confirm { title, text, action } => {
                let mut result = None;
                let close = modal(ctx, "confirm", 440.0, |ui| {
                    heading(ui, title);
                    ui.add(egui::Label::new(RichText::new(text.as_str()).color(TEXT)).wrap());
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if ui.button(RichText::new("Yes").color(BAD).strong()).clicked() {
                            result = Some(true);
                        }
                        if ui.button("Cancel").clicked() {
                            result = Some(false);
                        }
                    });
                });
                if result == Some(true) {
                    self.run_confirm(action);
                }
                result.is_none() && !close
            }
            Dialog::DeleteMovies { ids, delete_files, exclusion } => {
                let mut result = None;
                let names: Vec<String> = {
                    let lib = self.core.lib();
                    ids.iter().filter_map(|i| lib.movies.get(i).map(|m| m.display())).take(6).collect()
                };
                let close = modal(ctx, "delete-movies", 460.0, |ui| {
                    heading(ui, if ids.len() == 1 { "Delete movie" } else { "Delete movies" });
                    for n in &names {
                        ui.label(RichText::new(n).color(TEXT).strong());
                    }
                    if ids.len() > names.len() {
                        ui.label(super::dim(&format!("…and {} more", ids.len() - names.len())));
                    }
                    ui.add_space(6.0);
                    ui.checkbox(delete_files, "Delete the movie folders and files too");
                    if *delete_files {
                        ui.label(RichText::new("Files go to the recycle bin if one is set, otherwise they're deleted for good.").color(WARN));
                    }
                    ui.checkbox(exclusion, "Add to the import list exclusions (so lists don't add it back)");
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button(RichText::new("Delete").color(BAD).strong()).clicked() {
                            result = Some(true);
                        }
                        if ui.button("Cancel").clicked() {
                            result = Some(false);
                        }
                    });
                });
                if result == Some(true) {
                    let core = self.core.clone();
                    let (ids, del, ex) = (ids.clone(), *delete_files, *exclusion);
                    std::thread::spawn(move || {
                        for id in ids {
                            if let Err(e) = crate::library::delete_movie(&core, id, del, ex) {
                                core.toast(format!("Couldn't delete: {e}"));
                            }
                        }
                    });
                    self.detail = None;
                    self.movies.selected.clear();
                }
                result.is_none() && !close
            }
            Dialog::EditMovies(state) => self.edit_movies_dialog(ctx, state),
            Dialog::ManualImport(state) => self.manual_import_dialog(ctx, state),
            Dialog::Rename { ids, preview } => {
                let mut result = None;
                let close = modal(ctx, "rename", 720.0, |ui| {
                    heading(ui, "Rename files");
                    if preview.is_empty() {
                        ui.label(super::dim("Every file already matches your naming settings."));
                    } else {
                        egui::ScrollArea::vertical().max_height(380.0).show(ui, |ui| {
                            for p in preview.iter() {
                                ui.label(RichText::new(format!("− {}", p.existing)).color(BAD));
                                ui.label(RichText::new(format!("+ {}", p.new)).color(GOOD));
                                ff::divider(ui);
                            }
                        });
                    }
                    ui.horizontal(|ui| {
                        if !preview.is_empty() && ui.button(RichText::new("Rename").color(GOOD).strong()).clicked() {
                            result = Some(true);
                        }
                        if ui.button("Close").clicked() {
                            result = Some(false);
                        }
                    });
                });
                if result == Some(true) {
                    self.core.run(crate::core::Command::RenameMovies(ids.clone()));
                }
                result.is_none() && !close
            }
            Dialog::AddMovie(state) => self.add_movie_dialog(ctx, state),
            Dialog::RemoveQueue { id, title, from_client, blocklist, search } => {
                let mut result = None;
                let close = modal(ctx, "remove-queue", 460.0, |ui| {
                    heading(ui, "Remove from queue");
                    ui.add(egui::Label::new(RichText::new(title.as_str()).color(TEXT).strong()).wrap());
                    ui.add_space(6.0);
                    ui.checkbox(from_client, "Remove it from the download client (and delete its data)");
                    ui.checkbox(blocklist, "Add the release to the blocklist");
                    if *blocklist {
                        ui.checkbox(search, "Search for a replacement");
                    }
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button(RichText::new("Remove").color(BAD).strong()).clicked() {
                            result = Some(true);
                        }
                        if ui.button("Cancel").clicked() {
                            result = Some(false);
                        }
                    });
                });
                if result == Some(true) {
                    let core = self.core.clone();
                    let (id, fc, bl, s) = (*id, *from_client, *blocklist, *search && *blocklist);
                    std::thread::spawn(move || {
                        if let Err(e) = crate::tracked::remove_from_queue(&core, id, fc, bl, s) {
                            core.toast(format!("Couldn't remove it: {e}"));
                        }
                    });
                }
                result.is_none() && !close
            }
            Dialog::Settings(editor) => self.settings_editor(ctx, editor),
        };
        if keep && self.dialog.is_none() {
            self.dialog = Some(d);
        }
    }

    fn run_confirm(&mut self, action: &Confirm) {
        match action {
            Confirm::DeleteFile(id) => {
                let core = self.core.clone();
                let id = *id;
                std::thread::spawn(move || {
                    if let Err(e) = crate::library::delete_file(&core, id, "Manual") {
                        core.toast(format!("Couldn't delete the file: {e}"));
                    }
                });
            }
            Confirm::ClearHistory => self.core.db.clear_history(),
            Confirm::ClearBlocklist => {
                let ids: Vec<Id> = self.core.lib().blocklist.keys().copied().collect();
                for id in ids {
                    self.core.remove::<Blocklist>(id);
                }
            }
            Confirm::RestoreBackup(p) => match crate::backup::stage_restore(p) {
                Ok(()) => {
                    self.restore_pending = true;
                    self.notify("Backup staged. Restart Turks to finish restoring it.");
                }
                Err(e) => self.notify(format!("Couldn't restore: {e}")),
            },
            Confirm::DeleteBackup(p) => {
                let _ = std::fs::remove_file(p);
            }
        }
    }
}

// ---------------------------------------------------------------- first run

#[derive(Default)]
pub struct FirstRun {
    pub step: u8,
    pub key: String,
    pub root: String,
    pub test: Option<Task<Result<(), String>>>,
    pub test_result: Option<Result<String, String>>,
}

impl App {
    pub fn first_run_wizard(&mut self, ctx: &egui::Context) {
        let Some(mut fr) = self.first_run.take() else { return };
        if let Some(r) = take(&fr.test) {
            fr.test = None;
            fr.test_result = Some(r.map(|_| "TMDb key works.".into()));
            if fr.test_result.as_ref().map(|r| r.is_ok()).unwrap_or(false) {
                self.cfg.tmdb_api_key = fr.key.trim().to_string();
                self.changed();
                fr.step = 1;
            }
        }
        let mut done = false;
        modal(ctx, "first-run", 560.0, |ui| {
            match fr.step {
                0 => {
                    heading(ui, "Welcome to Turks");
                    ui.add(egui::Label::new(RichText::new("Turks keeps your movie collection: it finds releases on your indexers, sends them to your download client, then renames and files them in your library.").color(TEXT)).wrap());
                    ui.add_space(8.0);
                    w::section(ui, "1. TMDb API key");
                    ui.add(egui::Label::new(super::dim("Movie info and posters come from TMDb. A key is free: sign up at themoviedb.org, then Settings › API › Create. Either the API key or the read access token works.")).wrap());
                    ui.hyperlink_to("Get a TMDb key", "https://www.themoviedb.org/settings/api");
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(&mut fr.key).desired_width(360.0).hint_text("paste your key"));
                        let busy = fr.test.is_some();
                        if ui.add_enabled(!busy && !fr.key.trim().is_empty(), egui::Button::new(if busy { "Checking…" } else { "Check" })).clicked() {
                            let t = crate::tmdb::Tmdb::new(&fr.key, "en", "US");
                            fr.test = Some(spawn(ctx, move || t.test()));
                        }
                    });
                    result_line(ui, &fr.test_result);
                    ui.add_space(6.0);
                    if ui.small_button("Skip for now").clicked() {
                        fr.step = 1;
                    }
                }
                _ => {
                    heading(ui, "Where do your movies live?");
                    ui.add(egui::Label::new(super::dim("Pick the folder that holds (or will hold) your movie folders, like D:\\Media\\Movies. You can add more later in Settings › Media Management.")).wrap());
                    ui.add_space(6.0);
                    w::folder_field(ui, &mut fr.root, 340.0);
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.add_enabled(!fr.root.trim().is_empty(), egui::Button::new(RichText::new("Finish").color(GOOD).strong())).clicked() {
                            let path = fr.root.trim().to_string();
                            if !self.core.lib().root_folders.values().any(|r| r.path == path) {
                                self.core.save(RootFolder { id: 0, path });
                            }
                            done = true;
                        }
                        if ui.button("Skip").clicked() {
                            done = true;
                        }
                    });
                    ui.add_space(4.0);
                    ui.label(super::dim("Next: add an indexer and a download client in Settings."));
                }
            }
        });
        if done {
            self.cfg.setup_done = true;
            self.changed();
            self.core.run(crate::core::Command::CheckHealth);
        } else {
            self.first_run = Some(fr);
        }
    }
}

pub fn quality_chip(ui: &mut Ui, qm: &QualityModel) {
    ff::tag(ui, &crate::quality::full_name(qm), LABEL);
}

pub fn dim_text(t: &str) -> RichText {
    RichText::new(t).color(TEXT_DIM)
}
