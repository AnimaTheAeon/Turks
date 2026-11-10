//! Settings: Radarr's twelve sections, with Shinra help icons on everything.

use eframe::egui::{self, RichText, Ui};

use super::common::{self, Dialog, Task};
use super::theme::{self as ff, BAD, GOOD, LABEL, TEXT, TEXT_DIM, WARN};
use super::widgets as w;
use super::App;
use crate::config::{ColonReplacement, FileDate, Propers, RescanAfterRefresh};
use crate::model::*;

const SECTIONS: &[(&str, bool)] = &[
    ("Media Management", false),
    ("Profiles", false),
    ("Quality", true),
    ("Custom Formats", false),
    ("Indexers", false),
    ("Download Clients", false),
    ("Import Lists", false),
    ("Connect", false),
    ("Metadata", false),
    ("Tags", false),
    ("General", false),
    ("UI", false),
];

#[derive(Default)]
pub struct SettingsState {
    pub section: usize,
    pub new_root: String,
    pub tmdb_test: Option<Task<Result<(), String>>>,
    pub tmdb_result: Option<Result<String, String>>,
}

pub enum Editor {
    Indexer { item: Indexer, test: Option<Task<Result<String, String>>>, result: Option<Result<String, String>>, new_tag: String, cats: String },
    Client { item: DownloadClient, test: Option<Task<Result<String, String>>>, result: Option<Result<String, String>>, new_tag: String },
    Notification { item: Notification, test: Option<Task<Result<String, String>>>, result: Option<Result<String, String>>, new_tag: String },
    List { item: ImportList, test: Option<Task<Result<String, String>>>, result: Option<Result<String, String>>, new_tag: String },
    Profile { item: QualityProfile },
    Format { item: CustomFormat, new_kind: usize },
    ImportFormats { text: String, error: Option<String> },
    Delay { item: DelayProfile, new_tag: String },
    Restriction { item: ReleaseProfile, required: String, ignored: String, new_tag: String },
    RemotePath { item: RemotePathMapping },
    Prowlarr { url: String, key: String, task: Option<Task<Result<Vec<Indexer>, String>>>, found: Vec<(bool, Indexer)>, error: Option<String> },
}

fn text_row(ui: &mut Ui, label: &str, help: &str, value: &mut String) -> bool {
    w::row(ui, label, help, |ui| ui.add(egui::TextEdit::singleline(value).desired_width(250.0)).changed())
}

fn secret_row(ui: &mut Ui, label: &str, help: &str, value: &mut String) -> bool {
    w::row(ui, label, help, |ui| ui.add(egui::TextEdit::singleline(value).password(true).desired_width(250.0)).changed())
}

fn toggle_row(ui: &mut Ui, label: &str, help: &str, value: &mut bool) -> bool {
    w::row(ui, label, help, |ui| w::toggle(ui, value))
}

fn num_row<T: egui::emath::Numeric>(ui: &mut Ui, label: &str, help: &str, value: &mut T, range: std::ops::RangeInclusive<T>, suffix: &str) -> bool {
    w::row(ui, label, help, |ui| ui.add(egui::DragValue::new(value).range(range).suffix(suffix)).changed())
}

fn card(ui: &mut Ui, title: &str, sub: &str, ok: Option<bool>) -> (egui::Response, bool) {
    // Fixed size up front so horizontal_wrapped can wrap cards onto new rows.
    let (rect, r) = ui.allocate_exact_size(egui::vec2(250.0, 58.0), egui::Sense::click());
    let hovered = r.hovered();
    ui.painter().rect(
        rect,
        egui::CornerRadius::same(if ff::is_green() { 1 } else { 5 }),
        if ff::is_green() { egui::Color32::from_rgba_premultiplied(2, 18, 2, 160) } else { egui::Color32::from_rgba_premultiplied(8, 12, 52, 150) },
        egui::Stroke::new(1.0, if hovered { LABEL() } else { ff::BORDER_MID() }),
        egui::StrokeKind::Inside,
    );
    let mut delete = false;
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(egui::vec2(9.0, 6.0))).layout(egui::Layout::top_down(egui::Align::Min)));
    child.horizontal(|ui| {
        if let Some(ok) = ok {
            ff::badge(ui, if ok { ff::Badge::Good } else { ff::Badge::Bad }, 14.0);
        }
        ui.add(egui::Label::new(RichText::new(title).color(TEXT()).strong().size(15.0)).truncate());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button(RichText::new("×").color(BAD())).on_hover_text("Delete").clicked() {
                delete = true;
            }
        });
    });
    child.add(egui::Label::new(RichText::new(sub).color(TEXT_DIM()).size(12.0)).truncate());
    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    (r.on_hover_text(sub), delete)
}

fn add_card(ui: &mut Ui, label: &str) -> bool {
    ui.add_sized([250.0, 54.0], egui::Button::new(RichText::new(format!("+  {label}")).color(LABEL()).strong())).clicked()
}

impl App {
    pub fn page_settings(&mut self, ui: &mut Ui) {
        let advanced = self.cfg.advanced_mode;
        let mut flip = false;
        ui.horizontal(|ui| {
            ff::title(ui, "Settings", 26.0, TEXT());
            if ff::is_green() {
                ui.label(common::dim_text(if advanced { "Advanced mode: showing everything" } else { "Simple mode: the settings most people need" }));
                let label = if advanced { "MODE: ADVANCED" } else { "MODE: SIMPLE" };
                flip = ui
                    .button(RichText::new(label).color(LABEL()).strong())
                    .on_hover_text("Simple shows the settings most people need. Advanced shows everything Radarr has.")
                    .clicked();
            } else {
                ui.label(common::dim_text(if advanced { "Advanced mode: showing everything" } else { "Simple mode: the settings most people need (switch with MODE in the side window)" }));
            }
        });
        if flip {
            self.cfg.advanced_mode = !advanced;
            self.changed();
        }
        ff::divider(ui);
        let avail = ui.available_rect_before_wrap();
        let nav_w = 196.0;
        let nav = egui::Rect::from_min_size(avail.min, egui::vec2(nav_w, avail.height()));
        let body = egui::Rect::from_min_max(egui::pos2(avail.left() + nav_w + 12.0, avail.top()), avail.max);
        let mut nav_ui = ui.new_child(egui::UiBuilder::new().max_rect(nav).layout(egui::Layout::top_down(egui::Align::Min)));
        egui::ScrollArea::vertical().id_salt("settings-nav").show(&mut nav_ui, |ui| {
            for (i, (name, adv)) in SECTIONS.iter().enumerate() {
                if *adv && !advanced {
                    continue;
                }
                let r = ff::sub_menu_item(ui, name, self.settings.section == i);
                if r.clicked() {
                    self.settings.section = i;
                }
            }
        });
        if SECTIONS[self.settings.section].1 && !advanced {
            self.settings.section = 0;
        }
        let mut body_ui = ui.new_child(egui::UiBuilder::new().max_rect(body).layout(egui::Layout::top_down(egui::Align::Min)));
        egui::ScrollArea::vertical().id_salt(("settings-body", self.settings.section)).auto_shrink(false).show(&mut body_ui, |ui| {
            ui.set_width(body.width() - 16.0);
            match self.settings.section {
                0 => self.s_media(ui),
                1 => self.s_profiles(ui),
                2 => self.s_quality(ui),
                3 => self.s_formats(ui),
                4 => self.s_indexers(ui),
                5 => self.s_clients(ui),
                6 => self.s_lists(ui),
                7 => self.s_connect(ui),
                8 => self.s_metadata(ui),
                9 => self.s_tags(ui),
                10 => self.s_general(ui),
                _ => self.s_ui(ui),
            }
        });
        ui.allocate_rect(avail, egui::Sense::hover());
    }

    // ------------------------------------------------------------ media management

    fn s_media(&mut self, ui: &mut Ui) {
        let adv = self.cfg.advanced_mode;
        let mut ch = false;
        w::section_help(ui, "Root folders", "Folders that hold your movie folders. Each movie lives in its own folder inside one of these.");
        let roots: Vec<RootFolder> = self.core.lib().root_folders.values().cloned().collect();
        let mut remove = None;
        for r in &roots {
            ui.horizontal(|ui| {
                let ok = std::path::Path::new(&r.path).is_dir();
                ff::badge(ui, if ok { ff::Badge::Good } else { ff::Badge::Bad }, 14.0);
                ui.label(RichText::new(&r.path).color(TEXT()).strong());
                if let Some(f) = crate::import::free_space(std::path::Path::new(&r.path)) {
                    ui.label(common::dim_text(&format!("{} free", crate::decision::human_size(f))));
                }
                let unmapped = crate::import::unmapped_folders(&self.core.lib(), std::path::Path::new(&r.path)).len();
                if unmapped > 0 && ui.small_button(format!("{unmapped} unmapped folders → import")).clicked() {
                    self.add.root = r.path.clone();
                    self.add.tab = 1;
                    self.go(super::Page::Add);
                }
                if ui.small_button(RichText::new("Remove").color(BAD())).clicked() {
                    remove = Some(r.id);
                }
            });
        }
        if let Some(id) = remove {
            self.core.remove::<RootFolder>(id);
        }
        ui.horizontal(|ui| {
            w::folder_field(ui, &mut self.settings.new_root, 320.0);
            if ui.add_enabled(!self.settings.new_root.trim().is_empty(), egui::Button::new("Add root folder")).clicked() {
                let p = self.settings.new_root.trim().to_string();
                let _ = std::fs::create_dir_all(&p);
                self.core.save(RootFolder { id: 0, path: p });
                self.settings.new_root.clear();
            }
        });

        w::section_help(ui, "Movie naming", "How Turks names folders and files when it imports. Uses Radarr's tokens.");
        let c = &mut self.cfg;
        ch |= toggle_row(ui, "Rename movies", "Rename imported files using the format below. Off keeps the downloaded file name.", &mut c.rename_movies);
        ch |= toggle_row(ui, "Replace illegal characters", "Swap characters Windows can't use (like ? and *) for safe ones instead of dropping them.", &mut c.replace_illegal_chars);
        ch |= w::row(ui, "Colon replacement", "What a ':' in a title becomes. Smart turns 'Title: Sub' into 'Title - Sub'.", |ui| {
            let mut changed = false;
            egui::ComboBox::from_id_salt("colon").selected_text(c.colon_replacement.label()).show_ui(ui, |ui| {
                for o in ColonReplacement::ALL {
                    changed |= ui.selectable_value(&mut c.colon_replacement, o, o.label()).changed();
                }
            });
            changed
        });
        ch |= w::row(ui, "Movie file format", "Tokens: {Movie Title} {Movie CleanTitle} {Release Year} {Quality Full} {Edition Tags} {MediaInfo VideoCodec} {MediaInfo AudioCodec} {MediaInfo VideoDynamicRangeType} {Release Group} {Custom Formats} {ImdbId} {TmdbId}. {Movie.Title} uses dots, {MOVIE TITLE} upper case.", |ui| {
            ui.add(egui::TextEdit::singleline(&mut c.standard_movie_format).desired_width(360.0)).changed()
        });
        ui.label(RichText::new(format!("Example: {}", crate::naming::example(c, false))).color(GOOD()).size(12.0));
        ch |= w::row(ui, "Movie folder format", "Folder for each movie. Add {imdb-{ImdbId}} or {tmdb-{TmdbId}} for Plex/Jellyfin id matching.", |ui| {
            ui.add(egui::TextEdit::singleline(&mut c.movie_folder_format).desired_width(360.0)).changed()
        });
        ui.label(RichText::new(format!("Example: {}", crate::naming::example(c, true))).color(GOOD()).size(12.0));

        w::section_help(ui, "Importing", "What happens when a download finishes.");
        ch |= toggle_row(ui, "Use hardlinks instead of copy", "For torrents on the same drive: the file appears in your library without using extra space, and keeps seeding. Falls back to copying across drives.", &mut c.use_hardlinks);
        ch |= toggle_row(ui, "Import extra files", "Bring subtitles and other extras along with the movie.", &mut c.import_extra_files);
        if c.import_extra_files {
            ch |= text_row(ui, "Extra file extensions", "Comma separated, e.g. srt,ass,nfo", &mut c.extra_file_extensions);
        }
        ch |= toggle_row(ui, "Analyse video files", "Read codec, HDR and audio info with ffprobe when it's available.", &mut c.analyse_video_files);
        if adv {
            ch |= toggle_row(ui, "Skip free space check", "Import even when the root folder looks full (for drives that report free space wrong).", &mut c.skip_free_space_check);
            ch |= num_row(ui, "Minimum free space", "Keep at least this much free on the library drive.", &mut c.minimum_free_space_mb, 0..=1_000_000, " MB");
            ch |= num_row(ui, "Minimum file age", "Wait until a file hasn't changed for this long before importing it.", &mut c.minimum_age_for_import_secs, 0..=3600, " s");
        }

        w::section_help(ui, "File management", "Upgrades, deletions and the recycle bin.");
        ch |= toggle_row(ui, "Unmonitor deleted movies", "When a movie's file disappears from disk, stop monitoring it so it isn't downloaded again.", &mut c.unmonitor_deleted_movies);
        ch |= w::row(ui, "Propers and repacks", "Prefer and upgrade: grab fixed releases automatically. Do not upgrade: prefer them but don't replace files. Do not prefer: rely on custom formats instead.", |ui| {
            let mut changed = false;
            egui::ComboBox::from_id_salt("propers").selected_text(c.propers.label()).show_ui(ui, |ui| {
                for p in Propers::ALL {
                    changed |= ui.selectable_value(&mut c.propers, p, p.label()).changed();
                }
            });
            changed
        });
        ch |= w::row(ui, "Recycle bin", "Deleted and upgraded files go here instead of being deleted. Leave empty to delete for good.", |ui| w::folder_field(ui, &mut c.recycle_bin, 220.0));
        ch |= num_row(ui, "Recycle bin cleanup", "Delete files from the recycle bin after this many days (0 = never).", &mut c.recycle_bin_cleanup_days, 0..=365, " days");
        if adv {
            ch |= toggle_row(ui, "Create empty movie folders", "Make a folder for each movie as soon as it's added.", &mut c.create_empty_folders);
            ch |= toggle_row(ui, "Delete empty folders", "Remove download folders left empty after an import.", &mut c.delete_empty_folders);
            ch |= w::row(ui, "Change file date", "Set the file's date to the cinema or physical release date.", |ui| {
                let mut changed = false;
                let label = |d: FileDate| match d {
                    FileDate::None => "Don't change",
                    FileDate::InCinemas => "In cinemas date",
                    FileDate::PhysicalRelease => "Physical release date",
                };
                egui::ComboBox::from_id_salt("filedate").selected_text(label(c.file_date)).show_ui(ui, |ui| {
                    for d in [FileDate::None, FileDate::InCinemas, FileDate::PhysicalRelease] {
                        changed |= ui.selectable_value(&mut c.file_date, d, label(d)).changed();
                    }
                });
                changed
            });
            ch |= w::row(ui, "Rescan folder after refresh", "Check the movie folder for changes after refreshing its info.", |ui| {
                let mut changed = false;
                let label = |d: RescanAfterRefresh| match d {
                    RescanAfterRefresh::Always => "Always",
                    RescanAfterRefresh::AfterManual => "After manual refresh",
                    RescanAfterRefresh::Never => "Never",
                };
                egui::ComboBox::from_id_salt("rescan").selected_text(label(c.rescan_after_refresh)).show_ui(ui, |ui| {
                    for d in [RescanAfterRefresh::Always, RescanAfterRefresh::AfterManual, RescanAfterRefresh::Never] {
                        changed |= ui.selectable_value(&mut c.rescan_after_refresh, d, label(d)).changed();
                    }
                });
                changed
            });
        }
        if ch {
            self.changed();
        }
    }

    // ------------------------------------------------------------ profiles

    fn s_profiles(&mut self, ui: &mut Ui) {
        let lib = self.core.lib().clone();
        w::section_help(ui, "Quality profiles", "Which qualities a movie may be downloaded in, which one is good enough (the cutoff), and how custom formats are scored.");
        let mut open = None;
        let mut delete = None;
        ui.horizontal_wrapped(|ui| {
            for p in lib.profiles.values() {
                let allowed: Vec<String> = p.items.iter().rev().filter(|i| i.allowed).take(4).map(|i| if i.name.is_empty() { crate::quality::name(i.id).to_string() } else { i.name.clone() }).collect();
                let used = lib.movies.values().filter(|m| m.quality_profile_id == p.id).count();
                let (r, del) = card(ui, &p.name, &format!("{} · {used} movies", allowed.join(", ")), None);
                if r.clicked() {
                    open = Some(p.clone());
                }
                if del {
                    if used > 0 {
                        self.notify(format!("'{}' is used by {used} movies; move them to another profile first.", p.name));
                    } else {
                        delete = Some(p.id);
                    }
                }
            }
            if add_card(ui, "Add profile") {
                let mut p = crate::quality::default_profile("New profile", crate::quality::id::BLURAY1080P, &[crate::quality::id::WEBDL1080P, crate::quality::id::WEBRIP1080P, crate::quality::id::BLURAY1080P]);
                p.id = 0;
                open = Some(p);
            }
        });
        if let Some(id) = delete {
            self.core.remove::<QualityProfile>(id);
        }
        if let Some(p) = open {
            self.dialog = Some(Dialog::Settings(Editor::Profile { item: p }));
        }

        w::section_help(ui, "Delay profiles", "Wait a while before grabbing, so a better release (or your preferred protocol) has time to show up.");
        let mut open = None;
        let mut delete = None;
        ui.horizontal_wrapped(|ui| {
            let mut list: Vec<&DelayProfile> = lib.delay_profiles.values().collect();
            list.sort_by_key(|d| d.order);
            for d in list {
                let tags = if d.tags.is_empty() { "All movies".to_string() } else { d.tags.iter().map(|t| lib.tag_label(*t)).collect::<Vec<_>>().join(", ") };
                let (r, del) = card(ui, &format!("Prefer {}", d.preferred.label()), &format!("Usenet {} min · Torrent {} min · {tags}", d.usenet_delay, d.torrent_delay), None);
                if r.clicked() {
                    open = Some(d.clone());
                }
                if del && !d.tags.is_empty() {
                    delete = Some(d.id);
                }
            }
            if add_card(ui, "Add delay profile") {
                open = Some(DelayProfile { order: lib.delay_profiles.len() as i32, ..Default::default() });
            }
        });
        if let Some(id) = delete {
            self.core.remove::<DelayProfile>(id);
        }
        if let Some(d) = open {
            self.dialog = Some(Dialog::Settings(Editor::Delay { item: d, new_tag: String::new() }));
        }

        w::section_help(ui, "Release profiles", "Words a release must contain, or must not contain. Wrap a term in slashes for a regex: /\\bHC\\b/");
        let mut open = None;
        let mut delete = None;
        ui.horizontal_wrapped(|ui| {
            for r in lib.release_profiles.values() {
                let (resp, del) = card(ui, if r.name.is_empty() { "Release profile" } else { &r.name }, &format!("Must: {} · Not: {}", r.required.join(", "), r.ignored.join(", ")), Some(r.enabled));
                if resp.clicked() {
                    open = Some(r.clone());
                }
                if del {
                    delete = Some(r.id);
                }
            }
            if add_card(ui, "Add release profile") {
                open = Some(ReleaseProfile { enabled: true, ..Default::default() });
            }
        });
        if let Some(id) = delete {
            self.core.remove::<ReleaseProfile>(id);
        }
        if let Some(r) = open {
            let (req, ign) = (r.required.join(", "), r.ignored.join(", "));
            self.dialog = Some(Dialog::Settings(Editor::Restriction { item: r, required: req, ignored: ign, new_tag: String::new() }));
        }
    }

    // ------------------------------------------------------------ quality sizes

    fn s_quality(&mut self, ui: &mut Ui) {
        w::section_help(ui, "Quality sizes", "Smallest, preferred and largest size for each quality, in MB per minute of runtime. Releases outside these are rejected; preferred breaks ties.");
        let sizes: Vec<QualitySize> = {
            let lib = self.core.lib();
            crate::quality::ALL.iter().map(|q| lib.size_for(q.id)).collect()
        };
        let mut save = Vec::new();
        egui::Grid::new("sizes").striped(true).spacing([14.0, 6.0]).show(ui, |ui| {
            for h in ["Quality", "Min", "Preferred", "Max", "2-hour movie"] {
                ff::title(ui, h, 13.0, LABEL());
            }
            ui.end_row();
            for mut s in sizes {
                ui.label(RichText::new(crate::quality::name(s.quality)).color(TEXT()));
                let mut changed = ui.add(egui::DragValue::new(&mut s.min).range(0.0..=2000.0).speed(0.5)).changed();
                let mut pref = s.preferred.unwrap_or(0.0);
                if ui.add(egui::DragValue::new(&mut pref).range(0.0..=2000.0).speed(0.5).custom_formatter(|n, _| if n <= 0.0 { "Unlimited".into() } else { format!("{n:.1}") })).changed() {
                    s.preferred = (pref > 0.0).then_some(pref);
                    changed = true;
                }
                let mut max = s.max.unwrap_or(0.0);
                if ui.add(egui::DragValue::new(&mut max).range(0.0..=2000.0).speed(0.5).custom_formatter(|n, _| if n <= 0.0 { "Unlimited".into() } else { format!("{n:.1}") })).changed() {
                    s.max = (max > 0.0).then_some(max);
                    changed = true;
                }
                let gb = |mb: f64| format!("{:.1} GB", mb * 120.0 / 1024.0);
                ui.label(common::dim_text(&format!("{} – {}", gb(s.min), s.max.map(gb).unwrap_or_else(|| "∞".into()))));
                ui.end_row();
                if changed {
                    save.push(s);
                }
            }
        });
        if ui.button("Reset to Radarr's defaults").clicked() {
            save = crate::quality::default_sizes();
        }
        if !save.is_empty() {
            self.core.save_many(save);
        }
    }

    // ------------------------------------------------------------ custom formats

    fn s_formats(&mut self, ui: &mut Ui) {
        let lib = self.core.lib().clone();
        w::section_help(ui, "Custom formats", "Named patterns (HDR, Atmos, x265, a favourite group…) that quality profiles score. Import TRaSH Guides or Radarr JSON straight in.");
        let mut open = None;
        let mut delete = None;
        let mut export = None;
        ui.horizontal_wrapped(|ui| {
            let mut list: Vec<&CustomFormat> = lib.formats.values().collect();
            list.sort_by_key(|a| a.name.to_lowercase());
            for f in list {
                let (r, del) = card(ui, &f.name, &f.specs.iter().map(|s| s.kind.label()).collect::<Vec<_>>().join(", "), None);
                if r.clicked() {
                    open = Some(f.clone());
                }
                r.context_menu(|ui| {
                    if ui.button("Copy as JSON").clicked() {
                        export = Some(f.clone());
                        ui.close();
                    }
                    if ui.button("Clone").clicked() {
                        open = Some(CustomFormat { id: 0, name: format!("{} copy", f.name), ..f.clone() });
                        ui.close();
                    }
                });
                if del {
                    delete = Some(f.id);
                }
            }
            if add_card(ui, "Add custom format") {
                open = Some(CustomFormat { id: 0, name: String::new(), include_when_renaming: false, specs: vec![] });
            }
            if add_card(ui, "Import JSON") {
                self.dialog = Some(Dialog::Settings(Editor::ImportFormats { text: String::new(), error: None }));
            }
        });
        ui.label(common::dim_text("Right-click a format to copy it as JSON or clone it."));
        if let Some(id) = delete {
            self.core.remove::<CustomFormat>(id);
            let profiles: Vec<QualityProfile> = self.core.lib().profiles.values().cloned().collect();
            for mut p in profiles {
                p.format_items.retain(|f| f.format_id != id);
                self.core.save(p);
            }
        }
        if let Some(f) = export {
            let json = serde_json::to_string_pretty(&crate::formats::export_json(&f)).unwrap_or_default();
            if arboard::Clipboard::new().and_then(|mut c| c.set_text(json)).is_ok() {
                self.notify("Copied to the clipboard.");
            }
        }
        if let Some(f) = open {
            self.dialog = Some(Dialog::Settings(Editor::Format { item: f, new_kind: 0 }));
        }
    }

    // ------------------------------------------------------------ indexers

    fn s_indexers(&mut self, ui: &mut Ui) {
        let lib = self.core.lib().clone();
        let adv = self.cfg.advanced_mode;
        w::section_help(ui, "Indexers", "Where Turks finds releases. Torznab for torrent sites (Prowlarr, Jackett), Newznab for usenet.");
        let mut open = None;
        let mut delete = None;
        ui.horizontal_wrapped(|ui| {
            for ix in lib.indexers.values() {
                let flags = [(ix.enable_rss, "RSS"), (ix.enable_auto_search, "Auto"), (ix.enable_interactive_search, "Interactive")].iter().filter(|x| x.0).map(|x| x.1).collect::<Vec<_>>().join(" · ");
                let failing = self.core.status().indexer_failures.contains_key(&ix.id);
                let (r, del) = card(ui, &ix.name, &format!("{} · {flags} · priority {}", ix.kind.protocol().label(), ix.priority), Some(!failing));
                if r.clicked() {
                    open = Some(ix.clone());
                }
                if del {
                    delete = Some(ix.id);
                }
            }
            if add_card(ui, "Add Torznab (torrent)") {
                open = Some(Indexer { kind: IndexerKind::Torznab, ..Default::default() });
            }
            if add_card(ui, "Add Newznab (usenet)") {
                open = Some(Indexer { kind: IndexerKind::Newznab, ..Default::default() });
            }
            if add_card(ui, "Import from Prowlarr") {
                self.dialog = Some(Dialog::Settings(Editor::Prowlarr { url: "http://localhost:9696".into(), key: String::new(), task: None, found: vec![], error: None }));
            }
        });
        ui.label(common::dim_text(&format!("Prowlarr can also push indexers to Turks: in Prowlarr add an app of type Radarr with server http://localhost:{} and the API key from Settings › General.", self.cfg.api_port)));
        if let Some(id) = delete {
            self.core.remove::<Indexer>(id);
        }
        if let Some(ix) = open {
            let cats = ix.categories.iter().map(|c| c.to_string()).collect::<Vec<_>>().join(",");
            self.dialog = Some(Dialog::Settings(Editor::Indexer { item: ix, test: None, result: None, new_tag: String::new(), cats }));
        }

        w::section_help(ui, "Options", "Apply to every indexer.");
        let c = &mut self.cfg;
        let mut ch = false;
        ch |= num_row(ui, "RSS sync interval", "How often Turks checks indexers' latest releases. 0 isn't allowed; minimum 10 minutes.", &mut c.rss_sync_interval_minutes, 10..=720, " min");
        ch |= num_row(ui, "Availability delay", "Days before or after the release date to start searching (negative = earlier).", &mut c.availability_delay_days, -365..=365, " days");
        ch |= toggle_row(ui, "Allow hardcoded subs", "Grab releases with subtitles burned into the picture.", &mut c.allow_hardcoded_subs);
        if adv {
            ch |= num_row(ui, "Minimum age (usenet)", "Wait this long after a usenet post before grabbing it.", &mut c.minimum_age_minutes, 0..=1440, " min");
            ch |= num_row(ui, "Retention (usenet)", "Ignore usenet posts older than this (0 = no limit).", &mut c.retention_days, 0..=10000, " days");
            ch |= num_row(ui, "Maximum size", "Ignore any release bigger than this (0 = no limit).", &mut c.maximum_size_mb, 0..=1_000_000, " MB");
            ch |= toggle_row(ui, "Prefer indexer flags", "Prefer freeleech and similar releases when everything else ties.", &mut c.prefer_indexer_flags);
            if !c.allow_hardcoded_subs {
                ch |= text_row(ui, "Whitelisted hardcoded subs", "Burned-in subtitle tags that are OK anyway, comma separated (e.g. 'korsub').", &mut c.whitelisted_hardcoded_subs);
            }
        }
        if ch {
            self.changed();
        }
    }

    // ------------------------------------------------------------ download clients

    fn s_clients(&mut self, ui: &mut Ui) {
        let lib = self.core.lib().clone();
        w::section_help(ui, "Download clients", "Where releases are sent. Lifestream speaks the Transmission protocol, so pick 'Transmission / Lifestream' for it.");
        let mut open = None;
        let mut delete = None;
        ui.horizontal_wrapped(|ui| {
            for c in lib.clients.values() {
                let ok = self.core.status().client_items.get(&c.id).map(|(_, r)| r.is_ok());
                let (r, del) = card(ui, &c.name, &format!("{} · {}:{} · priority {}{}", c.kind.label(), c.host, c.port, c.priority, if c.enabled { "" } else { " · OFF" }), ok);
                if r.clicked() {
                    open = Some(c.clone());
                }
                if del {
                    delete = Some(c.id);
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            for k in ClientKind::ALL {
                if ui.button(format!("+ {}", k.label())).clicked() {
                    open = Some(DownloadClient { name: k.label().split(' ').next().unwrap_or("Client").to_string(), kind: k, port: k.default_port(), ..Default::default() });
                }
            }
        });
        if let Some(id) = delete {
            self.core.remove::<DownloadClient>(id);
        }
        if let Some(c) = open {
            self.dialog = Some(Dialog::Settings(Editor::Client { item: c, test: None, result: None, new_tag: String::new() }));
        }
        w::section_help(ui, "Completed download handling", "Turks imports finished downloads by itself.");
        let c = &mut self.cfg;
        let mut ch = false;
        ch |= toggle_row(ui, "Enable", "Import completed downloads automatically.", &mut c.completed_download_handling);
        ch |= toggle_row(ui, "Redownload failed", "When a download fails, blocklist it and search for another release.", &mut c.redownload_failed);
        if c.redownload_failed {
            ch |= toggle_row(ui, "…from interactive search too", "Also replace failed releases you picked by hand.", &mut c.redownload_failed_from_interactive);
        }
        if self.cfg.advanced_mode {
            ch |= num_row(ui, "Check every", "How often to ask download clients for progress.", &mut self.cfg.check_downloads_seconds, 15..=600, " s");
        }
        if ch {
            self.changed();
        }
        w::section_help(ui, "Remote path mappings", "If a download client runs on another computer (or in Docker), tell Turks how its paths look from here. Example: host 'nas', remote '/downloads/', local '\\\\nas\\downloads\\'.");
        let mut open = None;
        let mut delete = None;
        for m in lib.remote_paths.values() {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{}  {}  →  {}", m.host, m.remote, m.local)).color(TEXT()));
                if ui.small_button("Edit").clicked() {
                    open = Some(m.clone());
                }
                if ui.small_button(RichText::new("Remove").color(BAD())).clicked() {
                    delete = Some(m.id);
                }
            });
        }
        if ui.button("+ Add mapping").clicked() {
            open = Some(RemotePathMapping::default());
        }
        if let Some(id) = delete {
            self.core.remove::<RemotePathMapping>(id);
        }
        if let Some(m) = open {
            self.dialog = Some(Dialog::Settings(Editor::RemotePath { item: m }));
        }
    }

    // ------------------------------------------------------------ import lists

    fn s_lists(&mut self, ui: &mut Ui) {
        let lib = self.core.lib().clone();
        w::section_help(ui, "Import lists", "Lists that add movies automatically: TMDb lists and people, IMDb lists, Trakt, Plex watchlist, RSS, or another Turks/Radarr.");
        let mut open = None;
        let mut delete = None;
        ui.horizontal_wrapped(|ui| {
            for l in lib.lists.values() {
                let (r, del) = card(ui, &l.name, &format!("{} · {}{}", l.kind.label(), if l.auto_add { "auto add" } else { "manual" }, l.last_sync.map(|t| format!(" · synced {}", common::when(Some(t), &self.cfg))).unwrap_or_default()), Some(l.enabled));
                if r.clicked() {
                    open = Some(l.clone());
                }
                if del {
                    delete = Some(l.id);
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            for k in ListKind::templates() {
                if ui.button(format!("+ {}", k.label())).clicked() {
                    let d = self.default_add_options();
                    open = Some(ImportList { name: k.label().into(), kind: k, root_folder: d.root_folder, quality_profile_id: d.quality_profile_id, ..Default::default() });
                }
            }
        });
        if let Some(id) = delete {
            self.core.remove::<ImportList>(id);
        }
        if let Some(l) = open {
            self.dialog = Some(Dialog::Settings(Editor::List { item: l, test: None, result: None, new_tag: String::new() }));
        }
        ui.horizontal(|ui| {
            if ui.button("Sync lists now").clicked() {
                self.core.run(crate::core::Command::ImportListSync);
            }
        });
        let c = &mut self.cfg;
        let mut ch = false;
        ch |= num_row(ui, "Sync every", "How often lists are checked.", &mut c.list_sync_interval_hours, 6..=168, " h");
        ch |= w::row(ui, "Clean library", "What to do with movies that are no longer on any list. Careful: removing can delete files.", |ui| {
            let mut changed = false;
            let opts = [("Disabled", "Do nothing"), ("LogOnly", "Just log it"), ("KeepAndUnmonitor", "Keep and unmonitor"), ("RemoveAndKeep", "Remove, keep files"), ("RemoveAndDelete", "Remove and delete files")];
            let cur = opts.iter().find(|o| o.0 == c.list_sync_clean_library).map(|o| o.1).unwrap_or("Do nothing");
            egui::ComboBox::from_id_salt("clean-lib").selected_text(cur).show_ui(ui, |ui| {
                for (k, l) in opts {
                    if ui.selectable_label(c.list_sync_clean_library == k, l).clicked() {
                        c.list_sync_clean_library = k.into();
                        changed = true;
                    }
                }
            });
            changed
        });
        if ch {
            self.changed();
        }
        w::section_help(ui, "Exclusions", "Movies lists won't add again (you deleted them with 'add exclusion').");
        let mut remove = None;
        for e in lib.exclusions.values() {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{} ({})", e.title, e.year)).color(TEXT()));
                if ui.small_button("Remove").clicked() {
                    remove = Some(e.id);
                }
            });
        }
        if lib.exclusions.is_empty() {
            ui.label(common::dim_text("None."));
        }
        if let Some(id) = remove {
            self.core.remove::<ListExclusion>(id);
        }
    }

    // ------------------------------------------------------------ connect

    fn s_connect(&mut self, ui: &mut Ui) {
        let lib = self.core.lib().clone();
        w::section_help(ui, "Connections", "Messages when things happen (Discord, Telegram, ntfy…), library refreshes for Plex/Jellyfin/Emby/Kodi, webhooks and scripts.");
        let mut open = None;
        let mut delete = None;
        ui.horizontal_wrapped(|ui| {
            for n in lib.notifications.values() {
                let (r, del) = card(ui, &n.name, n.kind.label(), None);
                if r.clicked() {
                    open = Some(n.clone());
                }
                if del {
                    delete = Some(n.id);
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            for k in NotifyKind::templates() {
                if ui.button(format!("+ {}", k.label())).clicked() {
                    let media = k.is_media_server();
                    open = Some(Notification { name: k.label().into(), on_grab: !media, on_health_issue: !media, on_manual_interaction: !media, on_app_update: !media, on_rename: media, on_movie_delete: media, on_movie_file_delete: media, kind: k, ..Default::default() });
                }
            }
        });
        if let Some(id) = delete {
            self.core.remove::<Notification>(id);
        }
        if let Some(n) = open {
            self.dialog = Some(Dialog::Settings(Editor::Notification { item: n, test: None, result: None, new_tag: String::new() }));
        }
    }

    // ------------------------------------------------------------ metadata

    fn s_metadata(&mut self, ui: &mut Ui) {
        w::section_help(ui, "Kodi / Emby / Jellyfin metadata", "Write movie.nfo files and artwork next to your movies so media servers pick up the same info.");
        let c = &mut self.cfg;
        let mut ch = false;
        ch |= toggle_row(ui, "Write metadata", "Create an .nfo for each movie when it's imported or refreshed.", &mut c.kodi_metadata);
        if c.kodi_metadata {
            ch |= toggle_row(ui, "Movie images", "Save poster.jpg and fanart.jpg in the movie folder.", &mut c.kodi_movie_images);
            ch |= toggle_row(ui, "Collection name", "Include the TMDb collection as the movie's set.", &mut c.kodi_collection_name);
            ch |= toggle_row(ui, "TMDb URL in .nfo", "Add the TMDb link at the end (helps Kodi's scraper).", &mut c.kodi_url_in_nfo);
        }
        if ch {
            self.changed();
        }
        ui.add_space(10.0);
        ui.label(common::dim_text("Subtitles: point Jenova's watch folder at your movie root folder and it will pick up new imports on its own."));
    }

    // ------------------------------------------------------------ tags

    fn s_tags(&mut self, ui: &mut Ui) {
        let lib = self.core.lib().clone();
        w::section_help(ui, "Tags", "Tags connect movies to indexers, download clients, delay/release profiles and connections that have the same tag.");
        let mut delete = None;
        egui::Grid::new("tags").striped(true).spacing([16.0, 6.0]).show(ui, |ui| {
            for h in ["Tag", "Movies", "Indexers", "Clients", "Connections", "Lists", ""] {
                ff::title(ui, h, 13.0, LABEL());
            }
            ui.end_row();
            for t in lib.tags.values() {
                let m = lib.movies.values().filter(|x| x.tags.contains(&t.id)).count();
                let i = lib.indexers.values().filter(|x| x.tags.contains(&t.id)).count();
                let c = lib.clients.values().filter(|x| x.tags.contains(&t.id)).count();
                let n = lib.notifications.values().filter(|x| x.tags.contains(&t.id)).count();
                let l = lib.lists.values().filter(|x| x.tags.contains(&t.id)).count();
                ui.label(RichText::new(&t.label).color(TEXT()).strong());
                for v in [m, i, c, n, l] {
                    ui.label(v.to_string());
                }
                if m + i + c + n + l == 0 && ui.small_button("Delete").clicked() {
                    delete = Some(t.id);
                } else {
                    ui.label("");
                }
                ui.end_row();
            }
        });
        if lib.tags.is_empty() {
            ui.label(common::dim_text("No tags yet. Add them on a movie or when adding one."));
        }
        if let Some(id) = delete {
            self.core.remove::<Tag>(id);
        }
    }

    // ------------------------------------------------------------ general

    fn s_general(&mut self, ui: &mut Ui) {
        if let Some(r) = common::take(&self.settings.tmdb_test) {
            self.settings.tmdb_test = None;
            self.settings.tmdb_result = Some(r.map(|_| "TMDb key works.".into()));
        }
        let adv = self.cfg.advanced_mode;
        let mut ch = false;
        w::section_help(ui, "Metadata (TMDb)", "Movie info and artwork come from The Movie Database.");
        ch |= secret_row(ui, "TMDb API key", "Free at themoviedb.org › Settings › API. The v3 key or the v4 read access token both work.", &mut self.cfg.tmdb_api_key);
        ui.horizontal(|ui| {
            if ui.add_enabled(self.settings.tmdb_test.is_none(), egui::Button::new("Test key")).clicked() {
                let t = crate::tmdb::Tmdb::new(&self.cfg.tmdb_api_key, "en", "US");
                self.settings.tmdb_test = Some(common::spawn(ui.ctx(), move || t.test()));
            }
            ui.hyperlink_to("Get a key", "https://www.themoviedb.org/settings/api");
        });
        common::result_line(ui, &self.settings.tmdb_result);
        let c = &mut self.cfg;
        ch |= text_row(ui, "Metadata language", "Language for titles and overviews (e.g. en, de, fr, pt-BR).", &mut c.metadata_language);
        ch |= text_row(ui, "Certification country", "Country for age ratings and release dates (e.g. US, GB, DE).", &mut c.certification_country);

        w::section_help(ui, "API", "Lets Prowlarr, Overseerr/Jellyseerr and other apps talk to Turks the way they'd talk to Radarr.");
        ch |= toggle_row(ui, "Enable API", "Changes take effect after a restart.", &mut c.api_enabled);
        ch |= num_row(ui, "Port", "7878 is Radarr's port; change it if Radarr is also running.", &mut c.api_port, 1024..=65535, "");
        ch |= toggle_row(ui, "Allow other computers", "Listen on the network, not just this PC. Only turn on if apps on other machines need it.", &mut c.api_bind_all);
        let mut regen = false;
        w::row(ui, "API key", "Apps need this key to connect.", |ui| {
            ui.add(egui::TextEdit::singleline(&mut c.api_key.clone()).desired_width(250.0));
            if ui.small_button("Copy").clicked() {
                let _ = arboard::Clipboard::new().and_then(|mut cb| cb.set_text(c.api_key.clone()));
            }
            if ui.small_button("New").clicked() {
                regen = true;
            }
        });
        if regen {
            c.api_key = crate::config::random_key();
            ch = true;
        }
        if adv {
            ch |= text_row(ui, "URL base", "For reverse proxies, e.g. /turks", &mut c.url_base);
        }

        w::section_help(ui, "Backups", "Turks backs up its settings and database on a schedule.");
        ch |= w::row(ui, "Backup folder", "Where backups are saved.", |ui| w::folder_field(ui, &mut c.backup_dir, 220.0));
        ch |= num_row(ui, "Interval", "Days between automatic backups.", &mut c.backup_interval_days, 1..=7, " days");
        ch |= num_row(ui, "Retention", "Automatic backups older than this are deleted.", &mut c.backup_retention_days, 1..=90, " days");

        w::section_help(ui, "App", "");
        ch |= toggle_row(ui, "Start with Windows", "Start in the tray when you sign in.", &mut c.start_with_windows);
        ch |= toggle_row(ui, "Close to tray", "The close button hides Turks in the tray so it keeps working.", &mut c.close_to_tray);
        ch |= toggle_row(ui, "Check for updates", "Look for a new version on GitHub when Turks starts.", &mut c.check_updates);
        if adv {
            ch |= w::row(ui, "ffprobe path", "Optional. ffprobe.exe reads real codec/HDR/audio info. Leave empty to look next to turks.exe and on PATH.", |ui| {
                let mut changed = false;
                if ui.button("Browse").clicked() {
                    if let Some(p) = rfd::FileDialog::new().pick_file() {
                        c.ffprobe_path = p.display().to_string();
                        changed = true;
                    }
                }
                changed |= ui.add(egui::TextEdit::singleline(&mut c.ffprobe_path).desired_width(200.0)).changed();
                changed
            });
        }
        if ch {
            self.changed();
        }
    }

    fn s_ui(&mut self, ui: &mut Ui) {
        let c = &mut self.cfg;
        let mut ch = false;
        w::section_help(ui, "Look", "");
        ch |= w::row(ui, "Theme", "FFVII menu: blue windows and the pointing hand. Box of X: a green dashboard with a glowing orb; pick a page from the home screen, and BACK (Esc or the mouse's back button) returns to it.", |ui| {
            let mut changed = false;
            let cur = ff::Theme::from_key(&c.theme);
            egui::ComboBox::from_id_salt("theme").selected_text(cur.label()).show_ui(ui, |ui| {
                for t in [ff::Theme::FfMenu, ff::Theme::GreenDash] {
                    if ui.selectable_label(cur == t, t.label()).clicked() && cur != t {
                        c.theme = t.key().to_string();
                        changed = true;
                    }
                }
            });
            changed
        });
        w::section_help(ui, "Calendar and dates", "");
        ch |= w::row(ui, "First day of week", "", |ui| {
            let mut changed = false;
            egui::ComboBox::from_id_salt("fdow").selected_text(if c.first_day_of_week == 1 { "Monday" } else { "Sunday" }).show_ui(ui, |ui| {
                changed |= ui.selectable_value(&mut c.first_day_of_week, 0, "Sunday").changed();
                changed |= ui.selectable_value(&mut c.first_day_of_week, 1, "Monday").changed();
            });
            changed
        });
        ch |= w::row(ui, "Short date format", "", |ui| {
            let mut changed = false;
            let opts = [("%b %-d %Y", "Mar 25 2026"), ("%d %b %Y", "25 Mar 2026"), ("%m/%d/%Y", "03/25/2026"), ("%d/%m/%Y", "25/03/2026"), ("%Y-%m-%d", "2026-03-25")];
            let cur = opts.iter().find(|o| o.0 == c.short_date_format).map(|o| o.1).unwrap_or("Mar 25 2026");
            egui::ComboBox::from_id_salt("sdf").selected_text(cur).show_ui(ui, |ui| {
                for (f, l) in opts {
                    if ui.selectable_label(c.short_date_format == f, l).clicked() {
                        c.short_date_format = f.into();
                        changed = true;
                    }
                }
            });
            changed
        });
        ch |= toggle_row(ui, "24-hour time", "", &mut c.time_24h);
        ch |= toggle_row(ui, "Relative dates", "Show '3 h ago' instead of a date for recent things.", &mut c.show_relative_dates);
        ch |= toggle_row(ui, "Runtime as hours and minutes", "2h 16m instead of 136 min.", &mut c.runtime_hours_minutes);
        w::section_help(ui, "Movies", "");
        ch |= w::row(ui, "Poster size", "Width of posters in the Movies grid.", |ui| ui.add(egui::Slider::new(&mut c.poster_size, 100.0..=260.0).suffix(" px")).changed());
        if ch {
            self.changed();
        }
    }

    // ------------------------------------------------------------ editors

    pub fn settings_editor(&mut self, ctx: &egui::Context, ed: &mut Editor) -> bool {
        let lib = self.core.lib().clone();
        let mut save = false;
        let mut cancel = false;
        let footer = |ui: &mut Ui, save: &mut bool, cancel: &mut bool| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button(RichText::new("Save").color(GOOD()).strong()).clicked() {
                    *save = true;
                }
                if ui.button("Cancel").clicked() {
                    *cancel = true;
                }
            });
        };
        let close = match ed {
            Editor::Indexer { item, test, result, new_tag, cats } => {
                if let Some(r) = common::take(test) {
                    *test = None;
                    *result = Some(r);
                }
                common::modal(ctx, "ed-indexer", 560.0, |ui| {
                    common::heading(ui, &format!("{} indexer", if item.kind == IndexerKind::Torznab { "Torznab" } else { "Newznab" }));
                    text_row(ui, "Name", "", &mut item.name);
                    text_row(ui, "URL", "The indexer's address, e.g. http://localhost:9696/1/ for a Prowlarr indexer.", &mut item.url);
                    text_row(ui, "API path", "Usually /api", &mut item.api_path);
                    secret_row(ui, "API key", "", &mut item.api_key);
                    if text_row(ui, "Categories", "Newznab category ids, comma separated. 2000 = Movies, 2040 = HD, 2045 = UHD.", cats) {
                        item.categories = cats.split(',').filter_map(|c| c.trim().parse().ok()).collect();
                    }
                    toggle_row(ui, "RSS", "Check this indexer's newest releases on every RSS sync.", &mut item.enable_rss);
                    toggle_row(ui, "Automatic search", "Use it when Turks searches by itself.", &mut item.enable_auto_search);
                    toggle_row(ui, "Interactive search", "Use it when you search by hand.", &mut item.enable_interactive_search);
                    num_row(ui, "Priority", "1 is highest. Used to break ties between equal releases.", &mut item.priority, 1..=50, "");
                    if item.kind == IndexerKind::Torznab {
                        num_row(ui, "Minimum seeders", "Skip torrents with fewer seeders.", &mut item.minimum_seeders, 0..=1000, "");
                        let mut ratio = item.seed_ratio.unwrap_or(0.0);
                        if num_row(ui, "Seed ratio", "Sent to the download client. 0 = use the client's setting.", &mut ratio, 0.0..=100.0, "") {
                            item.seed_ratio = (ratio > 0.0).then_some(ratio);
                        }
                    }
                    w::row(ui, "Download client", "Always use this client for this indexer.", |ui| {
                        let cur = lib.clients.get(&item.download_client_id).map(|c| c.name.clone()).unwrap_or_else(|| "Any".into());
                        egui::ComboBox::from_id_salt("ix-client").selected_text(cur).show_ui(ui, |ui| {
                            ui.selectable_value(&mut item.download_client_id, 0, "Any");
                            for c in lib.clients.values() {
                                ui.selectable_value(&mut item.download_client_id, c.id, &c.name);
                            }
                        });
                    });
                    w::row(ui, "Tags", "Only used for movies with one of these tags (none = all movies).", |_| {});
                    common::tags_editor(ui, "ix-tags", &self.core, &mut item.tags, new_tag);
                    ui.horizontal(|ui| {
                        if ui.add_enabled(test.is_none(), egui::Button::new(if test.is_some() { "Testing…" } else { "Test" })).clicked() {
                            let ix = item.clone();
                            *test = Some(common::spawn(ctx, move || crate::indexers::test(&ix)));
                        }
                    });
                    common::result_line(ui, result);
                    footer(ui, &mut save, &mut cancel);
                })
            }
            Editor::Client { item, test, result, new_tag } => {
                if let Some(r) = common::take(test) {
                    *test = None;
                    *result = Some(r);
                }
                common::modal(ctx, "ed-client", 560.0, |ui| {
                    common::heading(ui, item.kind.label());
                    text_row(ui, "Name", "", &mut item.name);
                    toggle_row(ui, "Enabled", "", &mut item.enabled);
                    if item.kind.is_blackhole() {
                        w::row(ui, "Drop folder", "Turks saves .torrent/.nzb files here for your client to pick up.", |ui| w::folder_field(ui, &mut item.drop_folder, 220.0));
                        w::row(ui, "Watch folder", "Where your client puts finished downloads.", |ui| w::folder_field(ui, &mut item.watch_folder, 220.0));
                        if item.kind == ClientKind::TorrentBlackhole {
                            toggle_row(ui, "Save magnet files", "Save magnet links as .magnet files (your client must support them).", &mut item.save_magnet_files);
                        }
                    } else {
                        text_row(ui, "Host", "localhost if it runs on this PC.", &mut item.host);
                        num_row(ui, "Port", "", &mut item.port, 1..=65535, "");
                        toggle_row(ui, "Use SSL", "", &mut item.use_ssl);
                        text_row(ui, "URL base", "Only if the client's web UI isn't at the root, e.g. /transmission", &mut item.url_base);
                        if matches!(item.kind, ClientKind::Sabnzbd) {
                            secret_row(ui, "API key", "SABnzbd › Config › General.", &mut item.api_key);
                        } else {
                            if item.kind != ClientKind::Deluge {
                                text_row(ui, "Username", "", &mut item.username);
                            }
                            secret_row(ui, "Password", "", &mut item.password);
                        }
                        if item.kind != ClientKind::DownloadStation {
                            text_row(ui, "Category", "Turks only looks at downloads in this category/label, so it won't touch your other downloads.", &mut item.category);
                        }
                        if matches!(item.kind, ClientKind::Transmission | ClientKind::QBittorrent | ClientKind::Deluge) {
                            text_row(ui, "Directory", "Optional download folder for Turks' downloads.", &mut item.directory);
                        }
                        if item.kind == ClientKind::DownloadStation {
                            text_row(ui, "Destination", "Shared folder on the NAS, e.g. downloads/movies (no leading slash). Add a remote path mapping so Turks can reach it over the network.", &mut item.directory);
                        }
                        toggle_row(ui, "Add paused", "", &mut item.add_paused);
                    }
                    num_row(ui, "Priority", "1 is used first when several clients of the same type are set up.", &mut item.priority, 1..=50, "");
                    toggle_row(ui, "Remove completed", "Remove downloads from the client once imported (torrents: after seeding goals are met).", &mut item.remove_completed);
                    toggle_row(ui, "Remove failed", "Remove failed downloads from the client.", &mut item.remove_failed);
                    w::row(ui, "Tags", "Only used for movies with one of these tags.", |_| {});
                    common::tags_editor(ui, "cl-tags", &self.core, &mut item.tags, new_tag);
                    if ui.add_enabled(test.is_none(), egui::Button::new(if test.is_some() { "Testing…" } else { "Test" })).clicked() {
                        let c = item.clone();
                        *test = Some(common::spawn(ctx, move || crate::clients::test(&c)));
                    }
                    common::result_line(ui, result);
                    footer(ui, &mut save, &mut cancel);
                })
            }
            Editor::Notification { item, test, result, new_tag } => {
                if let Some(r) = common::take(test) {
                    *test = None;
                    *result = Some(r);
                }
                common::modal(ctx, "ed-notify", 580.0, |ui| {
                    common::heading(ui, item.kind.label());
                    text_row(ui, "Name", "", &mut item.name);
                    match &mut item.kind {
                        NotifyKind::Discord { webhook, username } => {
                            secret_row(ui, "Webhook URL", "Discord › channel settings › Integrations › Webhooks.", webhook);
                            text_row(ui, "Username", "", username);
                        }
                        NotifyKind::Telegram { bot_token, chat_id, silent } => {
                            secret_row(ui, "Bot token", "From @BotFather.", bot_token);
                            text_row(ui, "Chat id", "", chat_id);
                            toggle_row(ui, "Send silently", "", silent);
                        }
                        NotifyKind::Pushover { user_key, api_token, devices, priority } => {
                            secret_row(ui, "User key", "", user_key);
                            secret_row(ui, "API token", "", api_token);
                            text_row(ui, "Devices", "Comma separated; empty = all.", devices);
                            num_row(ui, "Priority", "-2 to 2", priority, -2..=2, "");
                        }
                        NotifyKind::Ntfy { server, topic, token, priority } => {
                            text_row(ui, "Server", "", server);
                            text_row(ui, "Topic", "", topic);
                            secret_row(ui, "Access token", "Optional.", token);
                            num_row(ui, "Priority", "1 to 5", priority, 1..=5, "");
                        }
                        NotifyKind::Gotify { server, app_token, priority } => {
                            text_row(ui, "Server", "", server);
                            secret_row(ui, "App token", "", app_token);
                            num_row(ui, "Priority", "", priority, 0..=10, "");
                        }
                        NotifyKind::Webhook { url, method_put, username, password } => {
                            text_row(ui, "URL", "Receives Radarr-style JSON.", url);
                            toggle_row(ui, "Use PUT", "Off sends POST.", method_put);
                            text_row(ui, "Username", "", username);
                            secret_row(ui, "Password", "", password);
                        }
                        NotifyKind::Email { server, port, require_tls, username, password, from, to } => {
                            text_row(ui, "SMTP server", "", server);
                            num_row(ui, "Port", "", port, 1..=65535, "");
                            toggle_row(ui, "Encrypted", "TLS/STARTTLS.", require_tls);
                            text_row(ui, "Username", "", username);
                            secret_row(ui, "Password", "", password);
                            text_row(ui, "From", "", from);
                            text_row(ui, "To", "Comma separated.", to);
                        }
                        NotifyKind::Plex { host, port, token, use_ssl } => {
                            text_row(ui, "Host", "", host);
                            num_row(ui, "Port", "", port, 1..=65535, "");
                            secret_row(ui, "Plex token", "See Plex's 'Finding an authentication token' help page.", token);
                            toggle_row(ui, "Use SSL", "", use_ssl);
                        }
                        NotifyKind::Emby { host, port, api_key, use_ssl, .. } => {
                            text_row(ui, "Host", "", host);
                            num_row(ui, "Port", "", port, 1..=65535, "");
                            secret_row(ui, "API key", "Dashboard › API Keys.", api_key);
                            toggle_row(ui, "Use SSL", "", use_ssl);
                        }
                        NotifyKind::Kodi { host, port, username, password } => {
                            text_row(ui, "Host", "", host);
                            num_row(ui, "Port", "", port, 1..=65535, "");
                            text_row(ui, "Username", "", username);
                            secret_row(ui, "Password", "", password);
                        }
                        NotifyKind::Slack { webhook, channel } => {
                            secret_row(ui, "Webhook URL", "Slack › Apps › Incoming Webhooks.", webhook);
                            text_row(ui, "Channel", "Optional override, e.g. #movies", channel);
                        }
                        NotifyKind::Pushbullet { api_key, device } => {
                            secret_row(ui, "Access token", "Pushbullet › Settings › Account.", api_key);
                            text_row(ui, "Device id", "Optional; empty sends to all devices.", device);
                        }
                        NotifyKind::Apprise { server, urls, tag } => {
                            text_row(ui, "Apprise API server", "Your Apprise API, e.g. http://nas:8000", server);
                            text_row(ui, "Apprise URLs", "Space separated (or leave empty and use a tag).", urls);
                            text_row(ui, "Tag", "Send to the Apprise config's URLs with this tag.", tag);
                        }
                        NotifyKind::CustomScript { path, arguments } => {
                            w::row(ui, "Script", "Gets radarr_* environment variables like Radarr's custom scripts, so existing scripts work.", |ui| {
                                if ui.button("Browse").clicked() {
                                    if let Some(p) = rfd::FileDialog::new().pick_file() {
                                        *path = p.display().to_string();
                                    }
                                }
                                ui.add(egui::TextEdit::singleline(path).desired_width(200.0));
                            });
                            text_row(ui, "Arguments", "", arguments);
                        }
                    }
                    w::section(ui, "When");
                    ui.horizontal_wrapped(|ui| {
                        for (v, l) in [
                            (&mut item.on_grab, "Grab"),
                            (&mut item.on_download, "Import"),
                            (&mut item.on_upgrade, "Upgrade"),
                            (&mut item.on_rename, "Rename"),
                            (&mut item.on_movie_added, "Movie added"),
                            (&mut item.on_movie_delete, "Movie deleted"),
                            (&mut item.on_movie_file_delete, "File deleted"),
                            (&mut item.on_health_issue, "Health issue"),
                            (&mut item.on_health_restored, "Health restored"),
                            (&mut item.on_manual_interaction, "Needs attention"),
                            (&mut item.on_app_update, "App updated"),
                        ] {
                            ui.checkbox(v, l);
                        }
                    });
                    if item.on_health_issue {
                        ui.checkbox(&mut item.include_health_warnings, "Include warnings (not just errors)");
                    }
                    w::row(ui, "Tags", "Only for movies with one of these tags.", |_| {});
                    common::tags_editor(ui, "nt-tags", &self.core, &mut item.tags, new_tag);
                    if ui.add_enabled(test.is_none(), egui::Button::new(if test.is_some() { "Sending…" } else { "Test" })).clicked() {
                        let n = item.clone();
                        *test = Some(common::spawn(ctx, move || crate::notify::send(&n, &crate::notify::Event::Test).map(|_| "Sent.".to_string())));
                    }
                    common::result_line(ui, result);
                    footer(ui, &mut save, &mut cancel);
                })
            }
            Editor::List { item, test, result, new_tag } => {
                if let Some(r) = common::take(test) {
                    *test = None;
                    *result = Some(r);
                }
                common::modal(ctx, "ed-list", 580.0, |ui| {
                    common::heading(ui, item.kind.label());
                    text_row(ui, "Name", "", &mut item.name);
                    toggle_row(ui, "Enabled", "", &mut item.enabled);
                    toggle_row(ui, "Add automatically", "Add movies from this list without asking.", &mut item.auto_add);
                    match &mut item.kind {
                        ListKind::TmdbList { list_id } => {
                            text_row(ui, "List id", "The number in the list's TMDb URL.", list_id);
                        }
                        ListKind::TmdbCollection { collection_id } => {
                            text_row(ui, "Collection id", "", collection_id);
                        }
                        ListKind::TmdbPerson { person_id, cast, crew } => {
                            text_row(ui, "Person id", "From the person's TMDb URL.", person_id);
                            toggle_row(ui, "As cast", "", cast);
                            toggle_row(ui, "As director/writer/producer", "", crew);
                        }
                        ListKind::TmdbCompany { company_id } => {
                            text_row(ui, "Company id", "", company_id);
                        }
                        ListKind::TmdbKeyword { keyword_id } => {
                            text_row(ui, "Keyword id", "", keyword_id);
                        }
                        ListKind::TmdbPopular { which } => {
                            w::row(ui, "List", "", |ui| {
                                egui::ComboBox::from_id_salt("popular").selected_text(which.clone()).show_ui(ui, |ui| {
                                    for o in ["popular", "top_rated", "now_playing", "upcoming", "trending"] {
                                        ui.selectable_value(which, o.to_string(), o);
                                    }
                                });
                            });
                        }
                        ListKind::ImdbList { list_id } => {
                            text_row(ui, "List id", "ls123456789 (the list must be public).", list_id);
                        }
                        ListKind::TraktList { client_id, username, list } => {
                            secret_row(ui, "Trakt client id", "Create an app at trakt.tv/oauth/applications.", client_id);
                            text_row(ui, "Username", "", username);
                            text_row(ui, "List", "'watchlist' or a list's slug.", list);
                        }
                        ListKind::RssList { url } => {
                            text_row(ui, "RSS URL", "", url);
                        }
                        ListKind::PlexWatchlist { rss_url } => {
                            text_row(ui, "Watchlist RSS URL", "Plex › Settings › Watchlist › RSS feed.", rss_url);
                        }
                        ListKind::TurksList { url, api_key } => {
                            text_row(ui, "URL", "", url);
                            secret_row(ui, "API key", "", api_key);
                        }
                    }
                    w::row(ui, "Root folder", "", |ui| {
                        common::root_combo(ui, "li-root2", &lib, &mut item.root_folder);
                    });
                    w::row(ui, "Quality profile", "", |ui| {
                        common::profile_combo(ui, "li-prof2", &lib, &mut item.quality_profile_id);
                    });
                    w::row(ui, "Monitor", "", |ui| {
                        common::monitor_combo(ui, "li-mon", &mut item.monitor);
                    });
                    w::row(ui, "Minimum availability", "", |ui| {
                        common::availability_combo(ui, "li-avail2", &mut item.minimum_availability);
                    });
                    toggle_row(ui, "Search when added", "", &mut item.search_on_add);
                    w::row(ui, "Tags", "Given to movies this list adds.", |_| {});
                    common::tags_editor(ui, "li-tags", &self.core, &mut item.tags, new_tag);
                    if ui.add_enabled(test.is_none(), egui::Button::new(if test.is_some() { "Fetching…" } else { "Test" })).clicked() {
                        let core = self.core.clone();
                        let l = item.clone();
                        *test = Some(common::spawn(ctx, move || crate::lists::fetch(&core, &l).map(|h| format!("The list has {} movies.", h.len()))));
                    }
                    common::result_line(ui, result);
                    footer(ui, &mut save, &mut cancel);
                })
            }
            Editor::Profile { item } => common::modal(ctx, "ed-profile", 640.0, |ui| {
                common::heading(ui, "Quality profile");
                text_row(ui, "Name", "", &mut item.name);
                toggle_row(ui, "Upgrades allowed", "Off: stop once any allowed quality is downloaded.", &mut item.upgrade_allowed);
                if item.upgrade_allowed {
                    w::row(ui, "Upgrade until", "Once a file reaches this quality, Turks stops upgrading it.", |ui| {
                        let name = |i: &ProfileItem| if i.name.is_empty() { crate::quality::name(i.id).to_string() } else { i.name.clone() };
                        let cur = item.items.iter().find(|i| i.id == item.cutoff).map(name).unwrap_or_default();
                        egui::ComboBox::from_id_salt("cutoff").selected_text(cur).show_ui(ui, |ui| {
                            for i in item.items.iter().filter(|i| i.allowed) {
                                ui.selectable_value(&mut item.cutoff, i.id, name(i));
                            }
                        });
                    });
                }
                w::row(ui, "Language", "Original = the movie's own language. Any = no language check.", |ui| {
                    egui::ComboBox::from_id_salt("plang").selected_text(item.language.clone()).show_ui(ui, |ui| {
                        for l in ["Any", "Original"].iter().map(|s| s.to_string()).chain(crate::parser::language::LANGUAGES.iter().map(|s| s.to_string())) {
                            ui.selectable_value(&mut item.language, l.clone(), l);
                        }
                    });
                });
                w::section_help(ui, "Qualities", "Tick the ones you want. Higher in the list is preferred.");
                egui::ScrollArea::vertical().id_salt("pq").max_height(220.0).show(ui, |ui| {
                    let n = item.items.len();
                    let mut swap = None;
                    for idx in (0..n).rev() {
                        let it = &mut item.items[idx];
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut it.allowed, "");
                            let label = if it.name.is_empty() { crate::quality::name(it.id).to_string() } else { format!("{} ({})", it.name, it.qualities.iter().map(|q| crate::quality::name(*q)).collect::<Vec<_>>().join(", ")) };
                            ui.label(RichText::new(label).color(if it.allowed { TEXT() } else { TEXT_DIM() }));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if idx > 0 && ui.small_button("▼").clicked() {
                                    swap = Some((idx, idx - 1));
                                }
                                if idx + 1 < n && ui.small_button("▲").clicked() {
                                    swap = Some((idx, idx + 1));
                                }
                            });
                        });
                    }
                    if let Some((a, b)) = swap {
                        item.items.swap(a, b);
                    }
                });
                w::section_help(ui, "Custom format scores", "Positive scores are preferred, negative avoided. Releases below the minimum score are rejected.");
                num_row(ui, "Minimum score", "", &mut item.min_format_score, -100000..=100000, "");
                num_row(ui, "Upgrade until score", "", &mut item.cutoff_format_score, -100000..=100000, "");
                num_row(ui, "Minimum upgrade increase", "An upgrade must beat the current score by at least this much.", &mut item.min_upgrade_format_score, 1..=100000, "");
                egui::ScrollArea::vertical().id_salt("pf").max_height(180.0).show(ui, |ui| {
                    for f in lib.formats.values() {
                        let pos = item.format_items.iter().position(|x| x.format_id == f.id);
                        let mut score = pos.map(|p| item.format_items[p].score).unwrap_or(0);
                        ui.horizontal(|ui| {
                            ui.add_sized([260.0, 18.0], egui::Label::new(RichText::new(&f.name).color(TEXT())));
                            if ui.add(egui::DragValue::new(&mut score).range(-100000..=100000)).changed() {
                                match pos {
                                    Some(p) => item.format_items[p].score = score,
                                    None => item.format_items.push(FormatScore { format_id: f.id, score }),
                                }
                            }
                        });
                    }
                    if lib.formats.is_empty() {
                        ui.label(common::dim_text("No custom formats yet."));
                    }
                });
                footer(ui, &mut save, &mut cancel);
            }),
            Editor::Format { item, new_kind } => common::modal(ctx, "ed-format", 640.0, |ui| {
                common::heading(ui, "Custom format");
                text_row(ui, "Name", "", &mut item.name);
                toggle_row(ui, "Include in {Custom Formats}", "Show this format's name where the naming format uses {Custom Formats}.", &mut item.include_when_renaming);
                w::section_help(ui, "Conditions", "Within one type, at least one condition must match and every 'required' one must. Every type must pass. 'Negate' flips a condition.");
                let mut remove = None;
                egui::ScrollArea::vertical().id_salt("fspecs").max_height(300.0).show(ui, |ui| {
                    for (i, s) in item.specs.iter_mut().enumerate() {
                        ui.group(|ui| {
                            ui.horizontal(|ui| {
                                ff::tag(ui, s.kind.label(), LABEL());
                                ui.add(egui::TextEdit::singleline(&mut s.name).hint_text("name").desired_width(140.0));
                                ui.checkbox(&mut s.negate, "Negate");
                                ui.checkbox(&mut s.required, "Required");
                                if ui.small_button(RichText::new("×").color(BAD())).clicked() {
                                    remove = Some(i);
                                }
                            });
                            match &mut s.kind {
                                CfKind::ReleaseTitle { pattern } | CfKind::ReleaseGroup { pattern } | CfKind::Edition { pattern } => {
                                    ui.add(egui::TextEdit::singleline(pattern).hint_text("regular expression").desired_width(480.0).font(egui::TextStyle::Monospace));
                                    if !pattern.is_empty() && fancy_regex::Regex::new(pattern).is_err() {
                                        ui.label(RichText::new("That regex doesn't compile.").color(BAD()).size(12.0));
                                    }
                                }
                                CfKind::Source { source } => {
                                    egui::ComboBox::from_id_salt(("src", i)).selected_text(crate::quality::source::NAMES[*source as usize % 10]).show_ui(ui, |ui| {
                                        for (v, n) in crate::quality::source::NAMES.iter().enumerate() {
                                            ui.selectable_value(source, v as i32, *n);
                                        }
                                    });
                                }
                                CfKind::Resolution { resolution } => {
                                    egui::ComboBox::from_id_salt(("res", i)).selected_text(format!("{resolution}p")).show_ui(ui, |ui| {
                                        for r in [360, 480, 540, 576, 720, 1080, 2160] {
                                            ui.selectable_value(resolution, r, format!("{r}p"));
                                        }
                                    });
                                }
                                CfKind::QualityModifier { modifier } => {
                                    egui::ComboBox::from_id_salt(("mod", i)).selected_text(crate::quality::modifier::NAMES[*modifier as usize % 6]).show_ui(ui, |ui| {
                                        for (v, n) in crate::quality::modifier::NAMES.iter().enumerate() {
                                            ui.selectable_value(modifier, v as i32, *n);
                                        }
                                    });
                                }
                                CfKind::Size { min_gb, max_gb } => {
                                    ui.horizontal(|ui| {
                                        ui.label("More than");
                                        ui.add(egui::DragValue::new(min_gb).range(0.0..=1000.0).suffix(" GB"));
                                        ui.label("up to");
                                        ui.add(egui::DragValue::new(max_gb).range(0.0..=1000.0).suffix(" GB"));
                                    });
                                }
                                CfKind::Language { language, except } => {
                                    ui.horizontal(|ui| {
                                        egui::ComboBox::from_id_salt(("lang", i)).selected_text(language.clone()).show_ui(ui, |ui| {
                                            for l in ["Original".to_string()].into_iter().chain(crate::parser::language::LANGUAGES.iter().map(|s| s.to_string())) {
                                                ui.selectable_value(language, l.clone(), l);
                                            }
                                        });
                                        ui.checkbox(except, "Any language except this");
                                    });
                                }
                                CfKind::IndexerFlag { flag } => {
                                    egui::ComboBox::from_id_salt(("flag", i)).selected_text(flag.clone()).show_ui(ui, |ui| {
                                        for (f, _) in crate::formats::INDEXER_FLAGS {
                                            ui.selectable_value(flag, f.to_string(), *f);
                                        }
                                    });
                                }
                                CfKind::Year { min, max } => {
                                    ui.horizontal(|ui| {
                                        ui.add(egui::DragValue::new(min).range(1880..=2100));
                                        ui.label("to");
                                        ui.add(egui::DragValue::new(max).range(1880..=2100));
                                    });
                                }
                            }
                        });
                    }
                });
                if let Some(i) = remove {
                    item.specs.remove(i);
                }
                ui.horizontal(|ui| {
                    let templates = CfKind::templates();
                    egui::ComboBox::from_id_salt("newkind").selected_text(templates[*new_kind].label()).show_ui(ui, |ui| {
                        for (i, t) in templates.iter().enumerate() {
                            ui.selectable_value(new_kind, i, t.label());
                        }
                    });
                    if ui.button("+ Add condition").clicked() {
                        item.specs.push(CfSpec { name: String::new(), kind: CfKind::templates().remove(*new_kind), negate: false, required: false });
                    }
                });
                footer(ui, &mut save, &mut cancel);
            }),
            Editor::ImportFormats { text, error } => common::modal(ctx, "ed-import-cf", 640.0, |ui| {
                common::heading(ui, "Import custom formats");
                ui.label(common::dim_text("Paste JSON from the TRaSH Guides or a Radarr export: one format or a list of them."));
                egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                    ui.add(egui::TextEdit::multiline(text).desired_width(600.0).desired_rows(14).font(egui::TextStyle::Monospace));
                });
                if let Some(e) = error {
                    ui.label(RichText::new(e.as_str()).color(BAD()));
                }
                ui.horizontal(|ui| {
                    if ui.button(RichText::new("Import").color(GOOD()).strong()).clicked() {
                        save = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            }),
            Editor::Delay { item, new_tag } => common::modal(ctx, "ed-delay", 520.0, |ui| {
                common::heading(ui, "Delay profile");
                toggle_row(ui, "Usenet", "", &mut item.enable_usenet);
                toggle_row(ui, "Torrent", "", &mut item.enable_torrent);
                w::row(ui, "Preferred protocol", "Wins when two releases are otherwise equal.", |ui| {
                    egui::ComboBox::from_id_salt("pref").selected_text(item.preferred.label()).show_ui(ui, |ui| {
                        ui.selectable_value(&mut item.preferred, Protocol::Usenet, "Usenet");
                        ui.selectable_value(&mut item.preferred, Protocol::Torrent, "Torrent");
                    });
                });
                num_row(ui, "Usenet delay", "Minutes to wait after a usenet release appears.", &mut item.usenet_delay, 0..=100000, " min");
                num_row(ui, "Torrent delay", "Minutes to wait after a torrent appears.", &mut item.torrent_delay, 0..=100000, " min");
                toggle_row(ui, "Skip delay for best quality", "Grab right away if it's the profile's top quality.", &mut item.bypass_if_highest_quality);
                toggle_row(ui, "Skip delay above score", "Grab right away if its custom format score is high enough.", &mut item.bypass_if_above_score);
                if item.bypass_if_above_score {
                    num_row(ui, "…score", "", &mut item.minimum_score, -100000..=100000, "");
                }
                w::row(ui, "Tags", "Movies with these tags use this profile. The profile without tags is the default.", |_| {});
                common::tags_editor(ui, "dl-tags", &self.core, &mut item.tags, new_tag);
                footer(ui, &mut save, &mut cancel);
            }),
            Editor::Restriction { item, required, ignored, new_tag } => common::modal(ctx, "ed-restr", 540.0, |ui| {
                common::heading(ui, "Release profile");
                text_row(ui, "Name", "", &mut item.name);
                toggle_row(ui, "Enabled", "", &mut item.enabled);
                text_row(ui, "Must contain", "Comma separated; a release needs at least one.", required);
                text_row(ui, "Must not contain", "Comma separated; any match rejects the release.", ignored);
                w::row(ui, "Indexer", "", |ui| {
                    let cur = lib.indexers.get(&item.indexer_id).map(|i| i.name.clone()).unwrap_or_else(|| "Any".into());
                    egui::ComboBox::from_id_salt("rp-ix").selected_text(cur).show_ui(ui, |ui| {
                        ui.selectable_value(&mut item.indexer_id, 0, "Any");
                        for i in lib.indexers.values() {
                            ui.selectable_value(&mut item.indexer_id, i.id, &i.name);
                        }
                    });
                });
                w::row(ui, "Tags", "", |_| {});
                common::tags_editor(ui, "rp-tags", &self.core, &mut item.tags, new_tag);
                footer(ui, &mut save, &mut cancel);
            }),
            Editor::RemotePath { item } => common::modal(ctx, "ed-rpm", 520.0, |ui| {
                common::heading(ui, "Remote path mapping");
                text_row(ui, "Host", "The download client's host, exactly as entered there.", &mut item.host);
                text_row(ui, "Remote path", "The folder as the download client sees it.", &mut item.remote);
                w::row(ui, "Local path", "The same folder as Turks sees it.", |ui| w::folder_field(ui, &mut item.local, 220.0));
                footer(ui, &mut save, &mut cancel);
            }),
            Editor::Prowlarr { url, key, task, found, error } => {
                if let Some(r) = common::take(task) {
                    *task = None;
                    match r {
                        Ok(list) => {
                            *found = list.into_iter().map(|i| (true, i)).collect();
                            *error = None;
                        }
                        Err(e) => *error = Some(e),
                    }
                }
                common::modal(ctx, "ed-prowlarr", 560.0, |ui| {
                    common::heading(ui, "Import from Prowlarr");
                    text_row(ui, "Prowlarr URL", "", url);
                    secret_row(ui, "API key", "Prowlarr › Settings › General.", key);
                    if ui.add_enabled(task.is_none(), egui::Button::new(if task.is_some() { "Loading…" } else { "Load indexers" })).clicked() {
                        let (u, k) = (url.clone(), key.clone());
                        *task = Some(common::spawn(ctx, move || crate::indexers::from_prowlarr(&u, &k)));
                    }
                    if let Some(e) = error {
                        ui.label(RichText::new(e.as_str()).color(BAD()));
                    }
                    for (on, ix) in found.iter_mut() {
                        ui.checkbox(on, format!("{} ({})", ix.name, ix.kind.protocol().label()));
                    }
                    ui.horizontal(|ui| {
                        if !found.is_empty() && ui.button(RichText::new("Add selected").color(GOOD()).strong()).clicked() {
                            save = true;
                        }
                        if ui.button("Cancel").clicked() {
                            cancel = true;
                        }
                    });
                })
            }
        };
        if save {
            let core = self.core.clone();
            match ed {
                Editor::Indexer { item, .. } => {
                    if item.name.trim().is_empty() {
                        item.name = "Indexer".into();
                    }
                    core.save(item.clone());
                }
                Editor::Client { item, .. } => {
                    core.save(item.clone());
                    core.run(crate::core::Command::RefreshDownloads);
                }
                Editor::Notification { item, .. } => {
                    core.save(item.clone());
                }
                Editor::List { item, .. } => {
                    core.save(item.clone());
                }
                Editor::Profile { item } => {
                    if !item.items.iter().any(|i| i.allowed) {
                        self.notify("A profile needs at least one quality ticked.");
                        return true;
                    }
                    if !item.items.iter().any(|i| i.allowed && i.id == item.cutoff) {
                        item.cutoff = item.items.iter().rev().find(|i| i.allowed).map(|i| i.id).unwrap_or(0);
                    }
                    core.save(item.clone());
                }
                Editor::Format { item, .. } => {
                    if item.name.trim().is_empty() {
                        self.notify("Give the format a name.");
                        return true;
                    }
                    core.save(item.clone());
                }
                Editor::ImportFormats { text, error } => match crate::formats::import_json(text) {
                    Ok(list) => {
                        let n = list.len();
                        for mut f in list {
                            // Replace a format with the same name.
                            if let Some(existing) = core.lib().formats.values().find(|x| x.name == f.name) {
                                f.id = existing.id;
                            }
                            core.save(f);
                        }
                        self.notify(format!("Imported {n} custom format(s). Give them scores in your quality profiles."));
                    }
                    Err(e) => {
                        *error = Some(e);
                        return true;
                    }
                },
                Editor::Delay { item, .. } => {
                    core.save(item.clone());
                }
                Editor::Restriction { item, required, ignored, .. } => {
                    let split = |s: &str| s.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect::<Vec<_>>();
                    item.required = split(required);
                    item.ignored = split(ignored);
                    core.save(item.clone());
                }
                Editor::RemotePath { item } => {
                    core.save(item.clone());
                }
                Editor::Prowlarr { found, .. } => {
                    let mut n = 0;
                    for (on, ix) in found.iter() {
                        if !on {
                            continue;
                        }
                        let existing = core.lib().indexers.values().find(|i| i.prowlarr_id.is_some() && i.prowlarr_id == ix.prowlarr_id && i.url == ix.url).map(|i| i.id);
                        core.save(Indexer { id: existing.unwrap_or(0), ..ix.clone() });
                        n += 1;
                    }
                    self.notify(format!("Added {n} indexers from Prowlarr."));
                }
            }
            return false;
        }
        let _ = WARN();
        !(cancel || close)
    }
}
