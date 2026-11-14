//! The FFVII party-menu window: main window on the left, command menu on the
//! right, info box under it, status bar along the bottom.

mod activity;
mod add;
mod blades;
mod blades_layout;
mod calendar;
mod common;
mod green;
mod images;
mod movies;
mod settings;
mod system;
pub mod theme;
mod widgets;

use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui::{self, pos2, Rect, RichText, Vec2};

use crate::config::Config;
use crate::core::Core;
use crate::instance::Incoming;
use crate::model::Id;
use theme::{self as ff, LABEL, TEXT, TEXT_DIM};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Page {
    Movies,
    Add,
    Discover,
    Collections,
    Calendar,
    Activity,
    Settings,
    System,
}

impl Page {
    /// The menu entry a page belongs to: Discover and Collections are tabs
    /// of Add Movie.
    pub fn menu(self) -> Page {
        match self {
            Page::Discover | Page::Collections => Page::Add,
            p => p,
        }
    }
}

const MENU: &[(Page, &str)] = &[
    (Page::Movies, "Movies"),
    (Page::Add, "Add Movie"),
    (Page::Calendar, "Calendar"),
    (Page::Activity, "Activity"),
    (Page::Settings, "Settings"),
    (Page::System, "System"),
];

pub struct App {
    pub core: Core,
    pub cfg: Config,
    cfg_dirty: Option<Instant>,
    pub page: Page,
    menu_touched: bool,
    pub images: images::Images,
    pub movies: movies::MoviesState,
    pub detail: Option<movies::DetailState>,
    pub add: add::AddState,
    pub discover: add::DiscoverState,
    pub collections: add::CollectionsState,
    pub activity: activity::ActivityState,
    pub calendar: calendar::CalendarState,
    pub settings: settings::SettingsState,
    pub system: system::SystemState,
    pub dialog: Option<common::Dialog>,
    notice: Option<(String, Instant)>,
    incoming: Receiver<Incoming>,
    start_hidden: bool,
    frames: u64,
    #[cfg(windows)]
    tray: Option<crate::tray::Tray>,
    pub updater: Arc<crate::updater::Updater>,
    pub manual_update_check: bool,
    quitting_for_update: bool,
    pub restore_pending: bool,
    pub first_run: Option<common::FirstRun>,
    /// Box of X: a page is open (false = the home screen with the orb).
    pub green_in_page: bool,
    /// Box of X: highlighted menu entry on the home screen.
    pub green_cursor: usize,
    /// Box of X transitions: zooming into the orb to open a page, the page
    /// coming out of the glow, and both reversed on BACK.
    pub green_zoom: Option<(Page, std::time::Instant)>,
    pub green_reveal: Option<std::time::Instant>,
    pub green_leave: Option<std::time::Instant>,
    pub green_unzoom: Option<std::time::Instant>,
    /// Blades theme: slide state between frames.
    pub blades: blades::State,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, core: Core, incoming: Receiver<Incoming>, start_hidden: bool, startup_notice: Option<String>) -> Self {
        configure_fonts(&cc.egui_ctx);
        ff::set_theme(ff::Theme::from_key(&core.cfg().theme));
        widgets::init_icons(&cc.egui_ctx);
        ff::apply_style(&cc.egui_ctx);
        let repaint_ctx = cc.egui_ctx.clone();
        core.set_repaint(move || repaint_ctx.request_repaint());
        #[cfg(windows)]
        let tray = crate::tray::Tray::new(core.clone(), cc.egui_ctx.clone());
        let updater = Arc::new(crate::updater::Updater::default());
        let cfg = core.cfg();
        if cfg.check_updates {
            crate::updater::check_in_background(Arc::clone(&updater), cc.egui_ctx.clone());
        }
        let first_run = (!cfg.setup_done).then(common::FirstRun::default);
        Self {
            images: images::Images::new(&cc.egui_ctx),
            cfg,
            cfg_dirty: None,
            page: Page::Movies,
            menu_touched: false,
            movies: Default::default(),
            detail: None,
            add: Default::default(),
            discover: Default::default(),
            collections: Default::default(),
            activity: Default::default(),
            calendar: Default::default(),
            settings: Default::default(),
            system: Default::default(),
            dialog: None,
            notice: startup_notice.map(|n| (n, Instant::now())),
            incoming,
            start_hidden,
            frames: 0,
            #[cfg(windows)]
            tray,
            updater,
            manual_update_check: false,
            quitting_for_update: false,
            restore_pending: false,
            first_run,
            green_in_page: false,
            green_cursor: 0,
            green_zoom: None,
            green_reveal: None,
            green_leave: None,
            green_unzoom: None,
            blades: Default::default(),
            core,
        }
    }

    pub fn notify(&mut self, msg: impl Into<String>) {
        self.notice = Some((msg.into(), Instant::now()));
    }

    /// Marks settings as changed; they're saved shortly after the last edit.
    pub fn changed(&mut self) {
        self.cfg_dirty = Some(Instant::now());
    }

    fn flush_cfg(&mut self, force: bool) {
        if let Some(t) = self.cfg_dirty {
            if force || t.elapsed() > Duration::from_millis(600) {
                let old = self.core.cfg();
                // Keep values the engine changed meanwhile.
                self.cfg.last_backup = old.last_backup;
                self.core.set_cfg(self.cfg.clone());
                self.core.update_task_intervals();
                #[cfg(windows)]
                if old.start_with_windows != self.cfg.start_with_windows {
                    crate::winint::set_autostart(self.cfg.start_with_windows);
                }
                self.cfg_dirty = None;
            }
        }
    }

    pub fn go(&mut self, page: Page) {
        self.page = page;
        self.menu_touched = true;
        self.detail = None;
    }

    pub fn open_movie(&mut self, id: Id) {
        self.page = Page::Movies;
        self.menu_touched = true;
        self.detail = Some(movies::DetailState::new(id));
    }

    fn handle_incoming(&mut self, ctx: &egui::Context) {
        while let Ok(msg) = self.incoming.try_recv() {
            match msg {
                Incoming::Show | Incoming::Open(_) => {
                    #[cfg(windows)]
                    crate::tray::show_window(ctx);
                    #[cfg(not(windows))]
                    let _ = ctx;
                }
                Incoming::Replace => {
                    #[cfg(windows)]
                    crate::tray::QUIT.store(true, std::sync::atomic::Ordering::SeqCst);
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
    }

    fn status_line(&self) -> (String, bool) {
        let running = self.core.running_commands();
        let visible: Vec<_> = running.iter().filter(|c| !matches!(c.command, crate::core::Command::RefreshDownloads | crate::core::Command::CheckHealth)).collect();
        if let Some(c) = visible.first() {
            let more = if visible.len() > 1 { format!(" (+{} more)", visible.len() - 1) } else { String::new() };
            let msg = if c.message.is_empty() { String::new() } else { format!(": {}", c.message) };
            return (format!("{}{msg}{more}", c.command.label()), false);
        }
        let health = self.core.status().health.clone();
        if let Some(h) = health.iter().max_by_key(|h| h.level) {
            if h.level >= crate::core::HealthLevel::Warning {
                return (h.message.clone(), true);
            }
        }
        let lib = self.core.lib();
        let dl = lib.active_downloads().count();
        (format!("{} movies · {} downloading · {} missing", lib.movies.len(), dl, lib.missing_count()), false)
    }

    /// The current page's contents; shared by both themes' layouts.
    pub(crate) fn render_page_body(&mut self, ui: &mut egui::Ui) {
        match self.page {
            Page::Movies => {
                if self.detail.is_some() {
                    self.page_detail(ui)
                } else {
                    self.page_movies(ui)
                }
            }
            Page::Add => self.page_add(ui),
            Page::Discover => self.page_discover(ui),
            Page::Collections => self.page_collections(ui),
            Page::Calendar => self.page_calendar(ui),
            Page::Activity => self.page_activity(ui),
            Page::Settings => self.page_settings(ui),
            Page::System => self.page_system(ui),
        }
    }

    /// Switches the look when the Theme setting changed. Switching to Box of X
    /// keeps you on the page you were on (Settings) instead of the home screen;
    /// done once here, never per frame, so BACK still works afterwards.
    fn sync_theme(&mut self, ctx: &egui::Context) {
        let want = ff::Theme::from_key(&self.cfg.theme);
        if want != ff::theme() {
            ff::set_theme(want);
            ff::apply_style(ctx);
            self.green_in_page = true;
            self.menu_touched = true;
        }
    }

    fn layout(&mut self, ui: &mut egui::Ui) {
        if ff::is_green() {
            return self.green_layout(ui);
        }
        if ff::is_blades() {
            return self.blades_layout(ui);
        }
        let full = ui.max_rect();
        let gap = 12.0;
        let side_w = 220.0_f32.min(full.width() * 0.28);
        let bar_h = 48.0;
        let main = Rect::from_min_max(full.min, pos2(full.right() - side_w - gap, full.bottom() - bar_h - gap));
        let menu_h = MENU.len() as f32 * (ff::MENU_ITEM_HEIGHT + 6.0) + ff::WINDOW_PADDING as f32 * 2.0;
        let menu = Rect::from_min_size(pos2(main.right() + gap, full.top()), Vec2::new(side_w, menu_h));
        let info_h = 186.0;
        let info = Rect::from_min_max(pos2(menu.left(), main.bottom() - info_h), pos2(full.right(), main.bottom()));
        let bar = Rect::from_min_max(pos2(full.left(), full.bottom() - bar_h), full.max);

        let current = self.page.menu();
        let mut goto = None;
        ff::window_at(ui, menu, "menu", |ui| {
            for (page, label) in MENU {
                let r = ff::menu_item(ui, label, self.menu_touched && current == *page, true);
                if r.clicked() {
                    goto = Some(*page);
                }
            }
        });
        if let Some(p) = goto {
            self.go(p);
        }

        let (movies, missing, free) = {
            let lib = self.core.lib();
            let free: u64 = lib.root_folders.values().filter_map(|r| crate::import::free_space(std::path::Path::new(&r.path))).max().unwrap_or(0);
            (lib.movies.len(), lib.missing_count(), free)
        };
        let secs = self.core.status().started.map(|s| s.elapsed().as_secs()).unwrap_or(0);
        let time = format!("{:02}:{:02}:{:02}", secs / 3600, (secs / 60) % 60, secs % 60);
        let mut mode_clicked = false;
        let advanced = self.cfg.advanced_mode;
        let mut go_wanted = false;
        ff::window_at(ui, info, "info", |ui| {
            info_row(ui, "Time", &time, TEXT());
            info_row(ui, "Movies", &movies.to_string(), TEXT());
            let r = ui.horizontal(|ui| {
                ff::stat_label(ui, "Missing");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ff::title(ui, &missing.to_string(), 17.0, if missing > 0 { ff::BAD() } else { TEXT() });
                });
            });
            if r.response.interact(egui::Sense::click()).on_hover_text("Open Wanted › Missing").clicked() {
                go_wanted = true;
            }
            info_row(ui, "Disk", &if free > 0 { crate::decision::human_size(free) } else { "—".into() }, TEXT());
            ui.add_space(4.0);
            let label = if advanced { "MODE: ADVANCED" } else { "MODE: SIMPLE" };
            mode_clicked = ui
                .add_sized([ui.available_width(), 26.0], egui::Button::new(RichText::new(label).color(LABEL()).strong()))
                .on_hover_text("Simple shows the settings most people need. Advanced shows everything Radarr has.")
                .clicked();
        });
        if go_wanted {
            self.go(Page::Movies);
            self.movies.filter = movies::Filter::Missing;
        }
        if mode_clicked {
            self.cfg.advanced_mode = !advanced;
            self.changed();
        }

        let (status, warn) = self.status_line();
        ff::window_at(ui, bar, "status", |ui| {
            ui.add_space(-4.0);
            ui.horizontal_centered(|ui| {
                let badge = if warn { ff::Badge::Warn } else { ff::Badge::Good };
                ff::badge(ui, badge, 18.0);
                ui.add_space(4.0);
                ff::title(ui, &status, 16.0, if warn { ff::WARN() } else { TEXT() });
            });
        });

        ff::window_at(ui, main, "main", |ui| self.render_page_body(ui));

        let ctx = ui.ctx().clone();
        self.show_dialog(&ctx);
        self.first_run_wizard(&ctx);
        self.update_prompt(&ctx);
        self.toasts(&ctx, pos2(main.right() - 14.0, main.top() + 14.0));
    }

    fn toasts(&mut self, ctx: &egui::Context, anchor: egui::Pos2) {
        let mut msgs: Vec<String> = Vec::new();
        if let Some((t, at)) = &self.notice {
            if at.elapsed() < Duration::from_secs(6) {
                msgs.push(t.clone());
            }
        }
        for (t, at) in self.core.status().toasts.iter() {
            if at.elapsed() < Duration::from_secs(6) {
                msgs.push(t.clone());
            }
        }
        let mut y = anchor.y;
        for (i, m) in msgs.iter().enumerate() {
            let area = egui::Area::new(egui::Id::new(("toast", i)))
                .order(egui::Order::Foreground)
                .pivot(egui::Align2::RIGHT_TOP)
                .fixed_pos(pos2(anchor.x, y))
                .interactable(false)
                .show(ctx, |ui| {
                    ui.set_max_width(380.0);
                    ff::window(ui, |ui| {
                        ui.set_width(340.0);
                        ui.add(egui::Label::new(RichText::new(m).color(TEXT()).strong()).wrap());
                    })
                });
            y += area.response.rect.height() + 10.0;
        }
    }

    fn watch_updater(&mut self, ctx: &egui::Context) {
        use crate::updater::State;
        match self.updater.state() {
            State::Restarting if !self.quitting_for_update => {
                self.quitting_for_update = true;
                #[cfg(windows)]
                crate::tray::QUIT.store(true, std::sync::atomic::Ordering::SeqCst);
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            State::Idle if self.manual_update_check => {
                self.manual_update_check = false;
                let err = self.updater.last_error.lock().ok().and_then(|e| e.clone());
                match err {
                    Some(e) => self.notify(format!("Couldn't check for updates: {e}")),
                    None => self.notify(format!("You're on the latest version (v{}).", crate::updater::APP_VERSION)),
                }
            }
            _ => {}
        }
    }

    /// Is the update window showing?
    pub(crate) fn update_prompt_open(&self) -> bool {
        use crate::updater::State;
        match self.updater.state() {
            State::Available(_) => !self.updater.dismissed.load(std::sync::atomic::Ordering::SeqCst),
            State::Downloading(_) | State::Restarting | State::Failed(_) => true,
            _ => false,
        }
    }

    fn update_prompt(&mut self, ctx: &egui::Context) {
        use crate::updater::State;
        use std::sync::atomic::Ordering::SeqCst;
        let state = self.updater.state();
        if !self.update_prompt_open() {
            return;
        }
        let mut install = None;
        let mut later = false;
        egui::Modal::new(egui::Id::new("update_prompt")).frame(egui::Frame::NONE).backdrop_color(egui::Color32::from_black_alpha(150)).show(ctx, |ui| {
            ff::window(ui, |ui| {
                ui.set_width(460.0);
                match &state {
                    State::Available(r) => {
                        ff::title(ui, "Update available", 24.0, TEXT());
                        ff::divider(ui);
                        ui.horizontal(|ui| {
                            ff::stat_label(ui, "CURRENT");
                            ui.label(RichText::new(format!("v{}", crate::updater::APP_VERSION)).strong());
                            ui.add_space(16.0);
                            ff::stat_label(ui, "NEW");
                            ui.label(RichText::new(format!("v{}", r.version)).strong().color(ff::GOOD()));
                        });
                        if !r.notes.trim().is_empty() {
                            ui.add_space(6.0);
                            egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                                ui.add(egui::Label::new(RichText::new(r.notes.trim()).color(TEXT_DIM())).wrap());
                            });
                        }
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            if ui.button(RichText::new("Update now").color(ff::GOOD()).strong()).clicked() {
                                install = Some(r.clone());
                            }
                            if ui.button("Later").clicked() {
                                later = true;
                            }
                            if !r.page_url.is_empty() {
                                ui.hyperlink_to("What's new", &r.page_url);
                            }
                        });
                    }
                    State::Downloading(r) => {
                        ff::title(ui, &format!("Downloading v{}", r.version), 22.0, TEXT());
                        let done = self.updater.downloaded.load(SeqCst);
                        let total = self.updater.total.load(SeqCst).max(1);
                        ff::stat_bar(ui, done as f32 / total as f32, 420.0, ff::LABEL(), egui::Color32::WHITE);
                        ui.label(dim(&format!("{} of {}", crate::decision::human_size(done), crate::decision::human_size(total))));
                    }
                    State::Restarting => {
                        ff::title(ui, "Installing update…", 22.0, TEXT());
                        ui.label(dim("Turks will reopen in a moment."));
                    }
                    State::Failed(e) => {
                        ff::title(ui, "Update didn't work", 22.0, ff::BAD());
                        ui.add(egui::Label::new(RichText::new(e).color(TEXT())).wrap());
                        if ui.button("OK").clicked() {
                            later = true;
                        }
                    }
                    _ => {}
                }
            });
        });
        if let Some(r) = install {
            crate::updater::install_in_background(Arc::clone(&self.updater), r, ctx.clone());
        }
        if later {
            self.updater.dismissed.store(true, SeqCst);
            if matches!(state, State::Failed(_)) {
                crate::updater::reset(&self.updater);
            }
        }
    }
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.frames += 1;
        if self.frames == 2 && self.start_hidden {
            #[cfg(windows)]
            if self.tray.is_some() {
                crate::tray::hide_window(ctx);
            }
        }
        self.handle_incoming(ctx);
        self.images.pump(ctx);
        #[cfg(windows)]
        if let Some(t) = self.tray.as_mut() {
            let (dl, missing) = {
                let lib = self.core.lib();
                (lib.active_downloads().filter(|d| d.state == crate::model::DownloadState::Downloading).count(), lib.missing_count())
            };
            t.update(dl, missing);
        }
        if ctx.input(|i| i.viewport().close_requested()) {
            #[cfg(windows)]
            {
                let quitting = crate::tray::QUIT.load(std::sync::atomic::Ordering::SeqCst);
                if !quitting && self.cfg.close_to_tray && self.tray.is_some() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                    crate::tray::hide_window(ctx);
                }
            }
        }
        self.watch_updater(ctx);
        self.flush_cfg(false);
        ctx.request_repaint_after(Duration::from_millis(1000));
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.sync_theme(&ctx);
        // Box of X paints its own background (fade and grid) behind the panel.
        let fill = if ff::is_green() { egui::Color32::TRANSPARENT } else { ff::BACKDROP() };
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(fill).inner_margin(egui::Margin::same(if ff::is_blades() { 0 } else { 14 })))
            .show(ui, |ui| self.layout(ui));
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.flush_cfg(true);
        self.core.shutdown();
    }
}

fn info_row(ui: &mut egui::Ui, label: &str, value: &str, color: egui::Color32) {
    ui.horizontal(|ui| {
        ff::stat_label(ui, label);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ff::title(ui, value, 17.0, color);
        });
    });
}

fn configure_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert("Inter".to_owned(), Arc::new(egui::FontData::from_static(include_bytes!("../../resources/Inter-Regular.ttf"))));
    fonts.font_data.insert("Nunito".to_owned(), Arc::new(egui::FontData::from_static(include_bytes!("../../resources/Nunito-700.ttf"))));
    if let Some(p) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
        p.insert(0, "Inter".to_owned());
        p.insert(0, "Nunito".to_owned());
        // Arrows and a few symbols only exist in the monospace font.
        p.push("Hack".to_owned());
    }
    if let Some(m) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
        m.push("Inter".to_owned());
    }
    // Blocky display face for the Box of X theme.
    fonts.font_data.insert("Orbitron".to_owned(), Arc::new(egui::FontData::from_static(include_bytes!("../../resources/Orbitron-800.ttf"))));
    fonts.families.insert(
        egui::FontFamily::Name("display".into()),
        vec!["Orbitron".to_owned(), "Nunito".to_owned(), "Inter".to_owned(), "Hack".to_owned()],
    );
    ctx.set_fonts(fonts);
}

pub fn open_path(path: &str) {
    if path.trim().is_empty() {
        return;
    }
    #[cfg(windows)]
    let _ = std::process::Command::new("explorer").arg(path).spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(path).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let _ = std::process::Command::new("xdg-open").arg(path).spawn();
}

pub fn open_url(url: &str) {
    #[cfg(windows)]
    let _ = std::process::Command::new("cmd").args(["/C", "start", "", url]).spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
}

pub fn dim(text: &str) -> RichText {
    RichText::new(text).color(TEXT_DIM())
}
