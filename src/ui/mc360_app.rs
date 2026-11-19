//! The MC360 look for Turks: one skin blade per menu section, each with a
//! menu of that section's pages and tabs. Picking an entry opens the page on
//! the blade, drawn exactly as in the other looks.

use super::mc360::{Blade, Event, Info, Item, Mc360, Tint};
use super::theme as ff;
use super::{App, Page, MENU};
use eframe::egui;

const LOOKS: [ff::Theme; 5] = [ff::Theme::FfMenu, ff::Theme::GreenDash, ff::Theme::Blades, ff::Theme::Glass, ff::Theme::Mc360];
const SYSTEM_TABS: [(&str, &str, &str); 6] = [
    ("Status", "info", "Health checks and versions."),
    ("Tasks", "task", "Scheduled jobs, with Run now."),
    ("Backup", "disk", "Make and restore backups."),
    ("Updates", "refresh", "Check now and Update now."),
    ("Events", "bell", "What the app has been doing."),
    ("Parse", "search", "Test how a name is read."),
];

fn page_no(p: Page) -> u32 {
    match p.menu() {
        Page::Movies => 0,
        Page::Add | Page::Discover | Page::Collections => 1,
        Page::Calendar => 2,
        Page::Activity => 3,
        Page::Settings => 4,
        Page::System => 5,
    }
}

fn settings_icon(name: &str) -> (&'static str, &'static str) {
    match name {
        "Media Management" => ("folder", "Naming, folders, hardlinks."),
        "Indexers" => ("search", "Synced from Materia."),
        "Download Clients" => ("plug", "Lifestream connection."),
        "Connect" => ("bell", "Where alerts go."),
        "Tags" => ("tag", "Tags and what uses them."),
        "General" => ("gear", "Port, sign-in, backups."),
        "UI" => ("palette", "Look, sounds and motion."),
        _ => ("gear", ""),
    }
}

impl App {
    fn mc360_blades(&self) -> Vec<(Page, Blade)> {
        let advanced = self.cfg.advanced_mode;
        MENU.iter()
            .map(|(page, _)| {
                let n = page_no(*page) * 100;
                let (key, tint, items) = match page {
                    Page::Movies => ("movies", Tint::Blue, vec![Item::new(n, "Library", "film", "Your movies.")]),
                    Page::Add => (
                        "add movie",
                        Tint::Green,
                        vec![
                            Item::new(n, "Search", "search", "Find a movie by name."),
                            Item::new(n + 1, "Discover", "eye", "Popular and upcoming."),
                            Item::new(n + 2, "Collections", "apps", "Fill in film series."),
                        ],
                    ),
                    Page::Calendar => ("calendar", Tint::Orange, vec![Item::new(n, "Calendar", "cal", "Releases by day.")]),
                    Page::Activity => (
                        "activity",
                        Tint::Red,
                        vec![
                            Item::new(n, "Queue", "queue", "Downloads in progress."),
                            Item::new(n + 1, "History", "clock", "Grabs and imports."),
                            Item::new(n + 2, "Blocklist", "block", "Releases that won't be grabbed again."),
                        ],
                    ),
                    Page::Settings => (
                        "settings",
                        Tint::Purple,
                        super::settings::SECTIONS
                            .iter()
                            .enumerate()
                            .filter(|(_, (_, adv))| advanced || !adv)
                            .map(|(i, (name, _))| {
                                let (icon, hint) = settings_icon(name);
                                let hint = if hint.is_empty() { format!("{name} settings.") } else { hint.to_string() };
                                Item::new(n + i as u32, name, icon, &hint)
                            })
                            .collect(),
                    ),
                    _ => (
                        "system",
                        Tint::Grey,
                        SYSTEM_TABS.iter().enumerate().map(|(i, (l, ic, h))| Item::new(n + i as u32, l, ic, h)).collect(),
                    ),
                };
                (*page, Blade { key: key.into(), tint, items })
            })
            .collect()
    }

    /// The entry that matches what's showing now.
    fn mc360_current(&self) -> u32 {
        let n = page_no(self.page) * 100;
        n + match self.page {
            Page::Discover => 1,
            Page::Collections => 2,
            Page::Activity => self.activity.tab as u32,
            Page::Settings => self.settings.section as u32,
            Page::System => self.system.tab as u32,
            _ => 0,
        }
    }

    pub(crate) fn mc360_layout(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let defs = self.mc360_blades();
        let blades: Vec<Blade> = defs.iter().map(|d| d.1.clone()).collect();
        let cur_b = MENU.iter().position(|m| m.0 == self.page.menu()).unwrap_or(0);
        let mut mc = self.mc.take().unwrap_or_else(|| {
            let mut m = Mc360::new(Some(crate::config::data_dir().join("mc360.txt")));
            m.home(cur_b);
            m
        });
        // The page changed somewhere else (a link, the status chip): follow it.
        let want = self.mc360_current();
        if mc.tier() == 2 && (mc.blade() != cur_b || mc.item() != Some(want)) {
            mc.show_page(cur_b, want, &blades[cur_b].items);
        }

        // Esc and the mouse's back button close a dialog first.
        let blocked = self.first_run.is_some() || self.update_prompt_open();
        let typing = ctx.egui_wants_keyboard_input();
        let esc = !typing && ui.input(|i| i.key_pressed(egui::Key::Escape));
        let mback = ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Extra1));
        let mut swallowed = false;
        if (esc || mback) && !blocked && self.dialog.is_some() {
            self.dialog = None;
            swallowed = true;
            ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        }

        let (status, _warn) = self.status_line();
        let mut card: Vec<String> = status.split(" · ").map(str::to_string).collect();
        card.truncate(3);
        let look_names: Vec<&str> = LOOKS.iter().map(|t| t.label()).collect();
        let ver = crate::updater::APP_VERSION.to_string();
        let info = Info {
            app: "Turks",
            ver: &ver,
            plat: "Windows",
            icon: "film",
            card,
            looks: &look_names,
            look: 4,
            popup_open: swallowed || blocked || self.dialog.is_some(),
            app_calm: ff::calm(),
            checking: self.manual_update_check,
        };
        // On a movie's details, Esc goes back to the list first.
        let in_detail = self.page == Page::Movies && self.detail.is_some();
        let mut close_detail = false;
        let mut before_back = || {
            if in_detail {
                close_detail = true;
                true
            } else {
                false
            }
        };
        let events = mc.ui(ui, &blades, &info, &mut before_back, &mut |child| {
            ff::set_hide_headings(true);
            self.render_page_body(child);
            ff::set_hide_headings(false);
        });
        if close_detail {
            self.detail = None;
        }
        for ev in events {
            match ev {
                Event::Open(b, item) => {
                    let page = defs[b].0;
                    let sub = item % 100;
                    match page {
                        Page::Add => self.go(match sub {
                            1 => Page::Discover,
                            2 => Page::Collections,
                            _ => Page::Add,
                        }),
                        Page::Activity => {
                            self.go(page);
                            self.activity.tab = sub as u8;
                        }
                        Page::Settings => {
                            self.go(page);
                            self.settings.section = sub as usize;
                        }
                        Page::System => {
                            self.go(page);
                            self.system.tab = sub as u8;
                        }
                        p => self.go(p),
                    }
                }
                Event::Home(_) => {}
                Event::SetLook(i) => {
                    self.cfg.theme = LOOKS[i.min(LOOKS.len() - 1)].key().to_string();
                    self.changed();
                }
                Event::CheckUpdates => {
                    self.manual_update_check = true;
                    crate::updater::check_in_background(std::sync::Arc::clone(&self.updater), ctx.clone());
                }
            }
        }
        self.mc = Some(mc);
        self.show_dialog(&ctx);
        self.first_run_wizard(&ctx);
        let full = ui.max_rect();
        self.toasts(&ctx, egui::pos2(full.right() - 40.0, full.top() + 90.0));
    }
}
