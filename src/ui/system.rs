//! System: Status, Tasks, Backup, Updates, Events.

use eframe::egui::{self, RichText, Ui};

use super::common::{self, Dialog};
use super::theme::{self as ff, BAD, GOOD, LABEL, TEXT, TEXT_DIM, WARN};
use super::widgets as w;
use super::App;
use crate::core::{Core, HealthLevel};

#[derive(Default)]
pub struct SystemState {
    pub tab: u8,
    pub log_filter: u8,
    pub parse: String,
}

impl App {
    pub fn page_system(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ff::title(ui, "System", 26.0, TEXT);
            ui.add_space(10.0);
            for (i, l) in ["Status", "Tasks", "Backup", "Updates", "Events", "Parse"].iter().enumerate() {
                if w::pill(ui, l, self.system.tab == i as u8).clicked() {
                    self.system.tab = i as u8;
                }
            }
        });
        ff::divider(ui);
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| match self.system.tab {
            0 => self.sys_status(ui),
            1 => self.sys_tasks(ui),
            2 => self.sys_backup(ui),
            3 => self.sys_updates(ui),
            4 => self.sys_events(ui),
            _ => self.sys_parse(ui),
        });
    }

    fn sys_status(&mut self, ui: &mut Ui) {
        w::section(ui, "Health");
        ui.horizontal(|ui| {
            if ui.button("Check now").clicked() {
                self.core.run(crate::core::Command::CheckHealth);
            }
        });
        let health = self.core.status().health.clone();
        if health.is_empty() {
            ui.horizontal(|ui| {
                ff::badge(ui, ff::Badge::Good, 16.0);
                ui.label(RichText::new("Everything looks good.").color(GOOD));
            });
        }
        for h in health {
            ui.horizontal(|ui| {
                ff::badge(ui, match h.level { HealthLevel::Error => ff::Badge::Bad, HealthLevel::Warning => ff::Badge::Warn, HealthLevel::Notice => ff::Badge::Good }, 16.0);
                ui.label(RichText::new(&h.source).color(LABEL).strong());
                ui.add(egui::Label::new(RichText::new(&h.message).color(TEXT)).wrap());
            });
        }
        w::section(ui, "Disk space");
        let roots: Vec<String> = self.core.lib().root_folders.values().map(|r| r.path.clone()).collect();
        for r in roots {
            let free = crate::import::free_space(std::path::Path::new(&r)).unwrap_or(0);
            let total = fs2::total_space(&r).unwrap_or(0).max(1);
            ui.horizontal(|ui| {
                ui.add_sized([260.0, 18.0], egui::Label::new(RichText::new(&r).color(TEXT)).truncate());
                ff::stat_bar(ui, 1.0 - free as f32 / total as f32, 240.0, egui::Color32::from_rgb(70, 130, 230), if free < total / 10 { BAD } else { GOOD });
                ui.label(common::dim_text(&format!("{} free of {}", crate::decision::human_size(free), crate::decision::human_size(total))));
            });
        }
        w::section(ui, "About");
        let data = crate::config::data_dir();
        let rows = [
            ("Version", format!("Turks {}", crate::updater::APP_VERSION)),
            ("Data folder", data.display().to_string()),
            ("Database", data.join("turks.db").display().to_string()),
            ("API", if self.cfg.api_enabled { format!("http://{}:{}{}", if self.cfg.api_bind_all { "0.0.0.0" } else { "localhost" }, self.cfg.api_port, self.cfg.url_base) } else { "off".into() }),
            ("ffprobe", crate::mediainfo::ffprobe_path(&self.cfg.ffprobe_path).unwrap_or_else(|| "not found (media info from file names)".into())),
            ("Movies / files", { let l = self.core.lib(); format!("{} / {}", l.movies.len(), l.files.len()) }),
        ];
        for (k, v) in rows {
            ui.horizontal(|ui| {
                ui.add_sized([140.0, 18.0], egui::Label::new(RichText::new(k).color(LABEL).strong()));
                ui.label(RichText::new(v).color(TEXT));
            });
        }
        ui.horizontal(|ui| {
            if ui.button("Open data folder").clicked() {
                super::open_path(&data.display().to_string());
            }
            ui.hyperlink_to("Based on Radarr (GPLv3)", "https://github.com/Radarr/Radarr");
        });
    }

    fn sys_tasks(&mut self, ui: &mut Ui) {
        w::section(ui, "Scheduled");
        let tasks = self.core.status().tasks.clone();
        egui::Grid::new("tasks").striped(true).spacing([16.0, 6.0]).show(ui, |ui| {
            for h in ["Task", "Every", "Last run", "Took", "Next run", ""] {
                ff::title(ui, h, 13.0, LABEL);
            }
            ui.end_row();
            for t in tasks {
                ui.label(RichText::new(t.name).color(TEXT).strong());
                let mins = t.interval.as_secs() / 60;
                ui.label(if mins == 0 { format!("{}s", t.interval.as_secs()) } else if mins < 120 { format!("{mins} min") } else { format!("{} h", mins / 60) });
                ui.label(common::when(t.last_run, &self.cfg));
                ui.label(t.last_duration.map(|d| format!("{:.1}s", d.as_secs_f32())).unwrap_or_default());
                let next = Core::next_run(&t);
                let secs = (next - chrono::Utc::now()).num_seconds();
                ui.label(if secs <= 0 { "now".into() } else if secs < 3600 { format!("in {} min", secs / 60 + 1) } else { format!("in {} h", secs / 3600) });
                let running = self.core.is_running(&t.command);
                if ui.add_enabled(!running, egui::Button::new(if running { "Running…" } else { "Run now" })).clicked() {
                    self.core.run(t.command.clone());
                }
                ui.end_row();
            }
        });
        w::section(ui, "Recent commands");
        let cmds: Vec<_> = self.core.status().commands.iter().rev().take(40).cloned().collect();
        egui::Grid::new("cmds").striped(true).spacing([16.0, 6.0]).show(ui, |ui| {
            for c in cmds {
                if c.finished.is_none() {
                    ui.spinner();
                } else {
                    ff::badge(ui, if c.failed { ff::Badge::Bad } else { ff::Badge::Good }, 14.0);
                }
                ui.label(RichText::new(c.command.label()).color(TEXT));
                ui.label(common::when(Some(c.started), &self.cfg));
                ui.add(egui::Label::new(RichText::new(&c.message).color(if c.failed { BAD } else { TEXT_DIM }).size(12.0)).truncate());
                ui.end_row();
            }
        });
    }

    fn sys_backup(&mut self, ui: &mut Ui) {
        if self.restore_pending {
            ui.label(RichText::new("A restore is waiting. Restart Turks to finish it.").color(WARN).strong());
        }
        ui.horizontal(|ui| {
            if ui.button(RichText::new("Back up now").color(GOOD)).clicked() {
                let core = self.core.clone();
                std::thread::spawn(move || match crate::backup::create(&core, "manual") {
                    Ok(p) => core.toast(format!("Backup saved: {}", p.display())),
                    Err(e) => core.toast(format!("Backup failed: {e}")),
                });
            }
            if ui.button("Restore from file…").clicked() {
                if let Some(p) = rfd::FileDialog::new().add_filter("Turks backup", &["zip"]).pick_file() {
                    self.dialog = Some(Dialog::Confirm { title: "Restore backup".into(), text: format!("Replace your current library and settings with {}? This happens the next time Turks starts.", p.display()), action: common::Confirm::RestoreBackup(p) });
                }
            }
            if ui.button("Open backup folder").clicked() {
                let _ = std::fs::create_dir_all(&self.cfg.backup_dir);
                super::open_path(&self.cfg.backup_dir);
            }
        });
        ff::divider(ui);
        let list = crate::backup::list(&self.cfg);
        if list.is_empty() {
            ui.label(common::dim_text("No backups yet."));
        }
        let mut action = None;
        egui::Grid::new("backups").striped(true).spacing([16.0, 6.0]).show(ui, |ui| {
            for (p, size, t) in list {
                let name = p.file_name().unwrap_or_default().to_string_lossy().to_string();
                ui.label(RichText::new(&name).color(TEXT));
                ui.label(crate::decision::human_size(size));
                let dt: chrono::DateTime<chrono::Utc> = t.into();
                ui.label(common::when(Some(dt), &self.cfg));
                if ui.small_button("Restore").clicked() {
                    action = Some((true, p.clone()));
                }
                if ui.small_button(RichText::new("Delete").color(BAD)).clicked() {
                    action = Some((false, p.clone()));
                }
                ui.end_row();
            }
        });
        if let Some((restore, p)) = action {
            self.dialog = Some(if restore {
                Dialog::Confirm { title: "Restore backup".into(), text: format!("Restore {}? It's applied the next time Turks starts.", p.display()), action: common::Confirm::RestoreBackup(p) }
            } else {
                Dialog::Confirm { title: "Delete backup".into(), text: format!("Delete {}?", p.display()), action: common::Confirm::DeleteBackup(p) }
            });
        }
    }

    fn sys_updates(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ff::stat_label(ui, "Installed");
            ui.label(RichText::new(format!("v{}", crate::updater::APP_VERSION)).color(TEXT).strong());
        });
        ui.add_space(6.0);
        match self.updater.state() {
            crate::updater::State::Checking => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Checking GitHub…");
                });
            }
            crate::updater::State::Available(r) => {
                ui.label(RichText::new(format!("v{} is available.", r.version)).color(GOOD).strong());
                if ui.button("Show update").clicked() {
                    self.updater.dismissed.store(false, std::sync::atomic::Ordering::SeqCst);
                }
            }
            _ => {
                if ui.button("Check for updates").clicked() {
                    self.manual_update_check = true;
                    crate::updater::check_in_background(std::sync::Arc::clone(&self.updater), ui.ctx().clone());
                }
            }
        }
        ui.add_space(6.0);
        ui.hyperlink_to("Release notes on GitHub", "https://github.com/AnimaTheAeon/Turks/releases");
    }

    fn sys_events(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            for (i, l) in ["All", "Warnings", "Errors"].iter().enumerate() {
                if w::pill(ui, l, self.system.log_filter == i as u8).clicked() {
                    self.system.log_filter = i as u8;
                }
            }
            if ui.button("Clear").clicked() {
                crate::applog::clear();
            }
            if ui.button("Open log file").clicked() {
                super::open_path(&crate::config::data_dir().join("turks.log").display().to_string());
            }
        });
        ff::divider(ui);
        let filter = self.system.log_filter;
        crate::applog::with_lines(|lines| {
            for l in lines.iter().rev() {
                use crate::applog::Level;
                if filter == 1 && l.level == Level::Info || filter == 2 && l.level != Level::Error {
                    continue;
                }
                let color = match l.level {
                    Level::Info => TEXT,
                    Level::Warn => WARN,
                    Level::Error => BAD,
                };
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&l.time).color(TEXT_DIM).size(12.0));
                    ui.add(egui::Label::new(RichText::new(&l.text).color(color).size(13.0)).wrap());
                });
            }
        });
    }

    /// Radarr's "Test parsing": what Turks makes of a release name.
    fn sys_parse(&mut self, ui: &mut Ui) {
        ui.label(common::dim_text("Paste a release or file name to see how Turks reads it."));
        ui.add(egui::TextEdit::singleline(&mut self.system.parse).hint_text("The.Matrix.1999.1080p.BluRay.x264-GROUP").desired_width(f32::INFINITY));
        let t = self.system.parse.trim().to_string();
        if t.is_empty() {
            return;
        }
        ff::divider(ui);
        let Some(p) = crate::parser::parse_movie_title(&t, false) else {
            ui.label(RichText::new("Turks couldn't read a movie title and year from that.").color(BAD));
            return;
        };
        let lib = self.core.lib().clone();
        let release = crate::release::Release { title: t.clone(), ..Default::default() };
        let movie_id = crate::decision::map_movie(&lib, &p, &release, None);
        let movie = movie_id.and_then(|id| lib.movies.get(&id));
        let fmts = crate::formats::matching(lib.formats.values(), &crate::decision::input_for_release(&p, &release, movie));
        let mi = crate::parser::media_info_from_name(&t);
        let score = movie.and_then(|m| lib.profile_for(m)).map(|pr| pr.score_of(&fmts.iter().map(|f| f.0).collect::<Vec<_>>()));
        let rows: Vec<(&str, String)> = vec![
            ("Title", p.titles.join(" / ")),
            ("Year", if p.year > 0 { p.year.to_string() } else { "—".into() }),
            ("Quality", crate::quality::full_name(&p.quality)),
            ("Release group", p.release_group.clone()),
            ("Edition", p.edition.clone()),
            ("Languages", if p.languages.is_empty() { "not stated (original language)".into() } else { p.languages.join(", ") }),
            ("Video / audio", format!("{} {} {} · {} {}", mi.video_codec, mi.video_dynamic_range, mi.resolution, mi.audio_codec, if mi.audio_channels > 0.0 { format!("{:.1}", mi.audio_channels) } else { String::new() })),
            ("IMDb / TMDb id", format!("{} / {}", if p.imdb_id.is_empty() { "—" } else { &p.imdb_id }, if p.tmdb_id > 0 { p.tmdb_id.to_string() } else { "—".into() })),
            ("Hardcoded subs", if p.hardcoded_subs.is_empty() { "no".into() } else { p.hardcoded_subs.clone() }),
            ("Matches movie", movie.map(|m| m.display()).unwrap_or_else(|| "nothing in your library".into())),
            ("Custom formats", if fmts.is_empty() { "none".into() } else { fmts.iter().map(|f| f.1.clone()).collect::<Vec<_>>().join(", ") }),
            ("Format score", score.map(|s| format!("{s:+}")).unwrap_or_else(|| "—".into())),
        ];
        for (k, v) in rows {
            ui.horizontal(|ui| {
                ui.add_sized([140.0, 18.0], egui::Label::new(RichText::new(k).color(LABEL).strong()));
                ui.label(RichText::new(v).color(TEXT));
            });
        }
    }
}
