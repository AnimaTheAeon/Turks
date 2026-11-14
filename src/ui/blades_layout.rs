//! Blades layout for Turks: one blade per menu page. Movie details open
//! inside the Movies blade. Drawing lives in `blades` (shared with Lifestream
//! and Jenova).

use eframe::egui::{self, pos2, Align, Color32, Layout, UiBuilder};

use super::blades::{self, Blade, Status};
use super::theme as ff;
use super::{App, Page, MENU};

fn colors(page: Page) -> [Color32; 4] {
    match page {
        Page::Movies => blades::BLUE,
        Page::Add | Page::Discover | Page::Collections => blades::GREEN,
        Page::Calendar => blades::ORANGE,
        Page::Activity => blades::TEAL,
        Page::Settings => blades::PURPLE,
        Page::System => blades::RED,
    }
}

impl App {
    pub(crate) fn blades_layout(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        // Modal windows (first-run, update) own the keyboard while open.
        let blocked = self.first_run.is_some() || self.update_prompt_open();
        let cur = MENU.iter().position(|m| m.0 == self.page.menu()).unwrap_or(0);
        let list: Vec<Blade> = MENU.iter().map(|(p, l)| Blade { label: l, colors: colors(*p) }).collect();
        let (status, warn) = self.status_line();
        let line: String = if status.chars().count() > 64 { status.chars().take(62).collect::<String>() + "…" } else { status };
        let (chip, dot) = if warn { ("Needs a look", blades::DOT_WARN) } else { ("All good", blades::DOT_GOOD) };
        let f = blades::show(ui, &mut self.blades, &list, cur, false, &Status { chip, dot, line: &line }, blocked || self.dialog.is_some());

        if !blocked {
            if let Some(i) = f.go {
                self.go(MENU[i].0);
            }
            if f.chip_clicked && self.page != Page::System {
                self.go(Page::System);
            }
            // Esc or the mouse's back button: a dialog, then movie details, then back a blade.
            if f.esc || f.back {
                if self.dialog.is_some() {
                    self.dialog = None;
                } else if self.page == Page::Movies && self.detail.is_some() {
                    self.detail = None;
                } else if f.back && cur > 0 {
                    self.go(MENU[cur - 1].0);
                } else if f.esc && cur > 0 {
                    self.go(MENU[0].0);
                }
            }
        }

        let mut child = ui.new_child(UiBuilder::new().id_salt(("blade_body", cur)).max_rect(f.body).layout(Layout::top_down(Align::Min)));
        child.set_clip_rect(f.body.expand(4.0).intersect(f.visible));
        child.multiply_opacity(f.fade);
        ff::set_hide_headings(true);
        self.render_page_body(&mut child);
        ff::set_hide_headings(false);

        self.show_dialog(&ctx);
        self.first_run_wizard(&ctx);
        self.update_prompt(&ctx);
        self.toasts(&ctx, pos2(f.body.right() - 14.0, f.body.top() + 14.0));
    }
}
