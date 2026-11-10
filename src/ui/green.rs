//! Box of X layout (ported from Jenova via Lifestream; see the project doc
//! claude/themes/green-dashboard.md): a home screen with a glowing jelly orb
//! and a curved column of ring nodes wired to slanted menu tabs; each entry
//! opens a full page with a slanted title bar and a framed panel.

use std::f32::consts::TAU;

use eframe::egui::{
    self, epaint::QuadraticBezierShape, pos2, Color32, Key, LayerId, Pos2, Rect, Sense, Shape, Stroke,
    Vec2,
};

use super::theme::{self as ff, pal};
use super::{App, Page, MENU};

fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(r, g, b, a)
}

/// Background: dark green fade, faint grid rings and spokes around `hub`.
pub fn paint_background(ctx: &egui::Context, hub: Pos2) {
    let screen = ctx.content_rect();
    let p = ctx.layer_painter(LayerId::background());
    p.rect_filled(screen, 0.0, ff::BACKDROP());
    let far = screen.width().max(screen.height()) * 1.1;
    ff::radial_disc(&p, hub, far, &[
        (0.0, Color32::from_rgb(22, 92, 16)),
        (0.25, Color32::from_rgb(10, 54, 8)),
        (0.55, Color32::from_rgb(4, 28, 4)),
        (1.0, Color32::from_rgb(1, 10, 1)),
    ]);
    let line = Stroke::new(1.4, rgba(120, 220, 60, 34));
    for k in 1..=6 {
        let rx = 230.0 * k as f32 * 0.85 + 60.0;
        p.add(Shape::ellipse_stroke(hub, Vec2::new(rx, rx * 0.92), line));
    }
    for i in 0..12 {
        let a = i as f32 / 12.0 * TAU + 0.2;
        p.line_segment([hub, hub + Vec2::new(a.cos(), a.sin()) * far], line);
    }
}

/// The big animated orb with an emblem in it.
fn paint_big_orb(painter: &egui::Painter, c: Pos2, r: f32, t: f32) {
    // Outer haze and shells.
    painter.add(Shape::circle_stroke(c, r * 1.22, Stroke::new(2.0, rgba(120, 220, 50, 80))));
    let spin = t * 0.35;
    let arc: Vec<Pos2> = (0..=40)
        .map(|i| {
            let a = spin + i as f32 / 40.0 * 1.6;
            c + Vec2::new(a.cos(), a.sin()) * r * 1.12
        })
        .collect();
    painter.add(Shape::line(arc, Stroke::new(r * 0.07, rgba(140, 240, 60, 70))));
    let w = (t * 0.7).sin();
    let w2 = (t * 0.55 + 1.3).sin();
    painter.add(Shape::ellipse_filled(
        c + Vec2::new(w * 4.0, w2 * 3.0),
        Vec2::new(r * (1.04 + 0.03 * w), r * (0.98 - 0.03 * w)),
        rgba(60, 170, 24, 70),
    ));
    painter.add(Shape::ellipse_stroke(
        c + Vec2::new(w * 4.0, w2 * 3.0),
        Vec2::new(r * (1.04 + 0.03 * w), r * (0.98 - 0.03 * w)),
        Stroke::new(3.0, rgba(160, 255, 80, 110)),
    ));
    ff::paint_orb(painter, c, r * 0.84, true, t * 0.15);
    // Translucent jelly lobes drifting over the core.
    for (i, (ox, oy, sx, sy)) in [(-0.5, 0.25, 0.42, 0.3), (0.55, -0.1, 0.32, 0.45), (0.1, 0.62, 0.5, 0.22)]
        .iter()
        .enumerate()
    {
        let ph = t * (0.5 + i as f32 * 0.17) + i as f32;
        let center = c + Vec2::new(*ox + 0.05 * ph.sin(), *oy + 0.05 * ph.cos()) * r;
        painter.add(Shape::ellipse_filled(center, Vec2::new(sx * r, sy * r), rgba(140, 240, 60, 45)));
        painter.add(Shape::ellipse_stroke(center, Vec2::new(sx * r, sy * r), Stroke::new(1.5, rgba(200, 255, 120, 70))));
    }
    // Emblem: a Turk's sunglasses, in pale light.
    let s = r * 0.46;
    let p = |x: f32, y: f32| c + Vec2::new(x * s, y * s);
    let pale = rgba(255, 255, 230, 225);
    let lens = [(-1.05, -0.30), (-0.14, -0.30), (-0.22, 0.16), (-0.42, 0.30), (-0.86, 0.30), (-1.0, 0.12)];
    for side in [-1.0, 1.0] {
        let pts: Vec<Pos2> = lens.iter().map(|&(x, y)| p(x * side, y)).collect();
        painter.add(Shape::convex_polygon(pts, rgba(255, 255, 230, 200), Stroke::new((r * 0.02).max(1.2), pale)));
        // A glint across each lens.
        let (gx0, gx1) = if side < 0.0 { (-0.78, -0.56) } else { (0.42, 0.58) };
        painter.line_segment([p(gx0, -0.16), p(gx1, 0.10)], Stroke::new((r * 0.03).max(1.5), rgba(70, 140, 25, 140)));
    }
    // Brow bar and bridge.
    painter.line_segment([p(-1.16, -0.33), p(1.16, -0.33)], Stroke::new((r * 0.04).max(2.0), pale));
    painter.add(QuadraticBezierShape::from_points_stroke(
        [p(-0.14, -0.22), p(0.0, -0.06), p(0.14, -0.22)],
        false,
        Color32::TRANSPARENT,
        Stroke::new((r * 0.03).max(1.5), pale),
    ));
}

/// Ring node beside each tab.
fn paint_node(painter: &egui::Painter, c: Pos2, r: f32, lit: bool) {
    let glow = if lit { 140 } else { 70 };
    ff::radial_disc(painter, c, r * 1.45, &[(0.0, rgba(150, 255, 60, glow)), (0.7, rgba(150, 255, 60, glow / 3)), (1.0, Color32::TRANSPARENT)]);
    ff::radial_disc(painter, c, r, &[
        (0.0, Color32::from_rgb(4, 20, 3)),
        (0.5, Color32::from_rgb(10, 40, 6)),
        (0.78, Color32::from_rgb(44, 138, 18)),
        (0.88, if lit { Color32::from_rgb(220, 255, 110) } else { Color32::from_rgb(141, 232, 58) }),
        (1.0, Color32::from_rgb(28, 90, 10)),
    ]);
    painter.circle_stroke(c, r * 0.36, Stroke::new(1.5, rgba(160, 255, 80, 90)));
}

/// Slanted tab with its label; `lit` = selected (yellow-green fill).
pub fn paint_tab(painter: &egui::Painter, rect: Rect, label: &str, lit: bool, size: f32) {
    let outer = ff::chamfer_points(rect, 16.0);
    let inner = ff::chamfer_points(rect.shrink(2.0), 15.0);
    painter.add(Shape::convex_polygon(outer, if lit { Color32::from_rgb(242, 255, 154) } else { rgba(150, 240, 70, 150) }, Stroke::NONE));
    let (top, bottom) = if lit {
        (Color32::from_rgb(226, 255, 106), Color32::from_rgb(140, 200, 28))
    } else {
        (Color32::from_rgb(36, 112, 14), Color32::from_rgb(12, 52, 6))
    };
    // Vertical gradient inside the clipped outline.
    let mut mesh = egui::Mesh::default();
    for p in &inner {
        let v = ((p.y - rect.top()) / rect.height()).clamp(0.0, 1.0);
        let mix = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * v) as u8;
        mesh.colored_vertex(*p, Color32::from_rgb(mix(top.r(), bottom.r()), mix(top.g(), bottom.g()), mix(top.b(), bottom.b())));
    }
    for i in 1..inner.len() as u32 - 1 {
        mesh.add_triangle(0, i, i + 1);
    }
    painter.add(Shape::mesh(mesh));
    let font = ff::display_font(size);
    let pos = pos2(rect.left() + 26.0, rect.center().y);
    let text = label.to_uppercase();
    if lit {
        painter.text(pos, egui::Align2::LEFT_CENTER, &text, font, pal().on_accent);
    } else {
        painter.text(pos + Vec2::new(0.0, 1.5), egui::Align2::LEFT_CENTER, &text, font.clone(), rgba(0, 0, 0, 160));
        painter.text(pos, egui::Align2::LEFT_CENTER, &text, font, Color32::from_rgb(190, 242, 126));
    }
}

/// A round hint button like "ENTER  SELECT". Returns its response.
fn hint(ui: &mut egui::Ui, at: Pos2, key: &str, label: &str) -> egui::Response {
    let r = 24.0;
    let c = at + Vec2::new(r, 0.0);
    let rect = Rect::from_center_size(c + Vec2::new(55.0, 0.0), Vec2::new(r * 2.0 + 120.0, r * 2.0 + 4.0));
    let resp = ui.interact(rect, ui.id().with(("hint", key)), Sense::click());
    let p = ui.painter();
    paint_node(p, c, r, resp.hovered());
    p.text(c, egui::Align2::CENTER_CENTER, key, ff::display_font(10.0), Color32::from_rgb(216, 255, 154));
    p.text(c + Vec2::new(r + 12.0, 0.0), egui::Align2::LEFT_CENTER, label, ff::display_font(17.0), Color32::from_rgb(190, 242, 126));
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// Bordered status box with a small orb.
fn status_box(ui: &mut egui::Ui, rect: Rect, top: &str, bottom: &str, badge: ff::Badge) {
    let p = ui.painter();
    p.rect_filled(rect, 0.0, rgba(0, 10, 0, 200));
    p.rect_stroke(rect, 0.0, Stroke::new(1.0, rgba(170, 255, 90, 140)), egui::StrokeKind::Inside);
    // Small orb light: lit when all is well, red-tinted badge otherwise.
    if badge == ff::Badge::Good {
        ff::paint_orb(p, pos2(rect.left() + 26.0, rect.center().y), 12.0, true, 0.0);
    } else {
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_center_size(pos2(rect.left() + 26.0, rect.center().y), Vec2::splat(26.0))));
        ff::badge(&mut child, badge, 24.0);
    }
    let p = ui.painter();
    let x = rect.left() + 50.0;
    let room = rect.right() - x - 10.0;
    // Shorten with an ellipsis rather than cutting text off mid-letter.
    let fit = |text: &str, size: f32| -> String {
        let text = text.to_uppercase();
        let per_char = size * 0.78;
        let max = (room / per_char).floor() as usize;
        if text.chars().count() <= max {
            text
        } else {
            let cut: String = text.chars().take(max.saturating_sub(1)).collect();
            format!("{}…", cut.trim_end())
        }
    };
    p.text(pos2(x, rect.center().y - 9.0), egui::Align2::LEFT_CENTER, fit(top, 10.5), ff::display_font(10.5), Color32::from_rgb(168, 224, 112));
    p.text(pos2(x, rect.center().y + 10.0), egui::Align2::LEFT_CENTER, fit(bottom, 12.5), ff::display_font(12.5), Color32::from_rgb(240, 255, 216));
}


impl App {
    /// Back to the dashboard, with the cursor on the page just left.
    pub(crate) fn green_back(&mut self) {
        self.green_in_page = false;
        self.detail = None;
        if let Some(i) = MENU.iter().position(|m| m.0 == self.page) {
            self.green_cursor = i;
        }
    }

    pub(crate) fn green_open(&mut self, page: Page) {
        self.go(page);
        self.green_in_page = true;
    }

    /// (top line, bottom line, badge) for the status box.
    fn green_status(&self) -> (String, String, ff::Badge) {
        let (status, warn) = self.status_line();
        let secs = self.core.status().started.map(|s| s.elapsed().as_secs()).unwrap_or(0);
        let mode = if self.cfg.advanced_mode { "Advanced" } else { "Simple" };
        (
            format!("Turks v{} · {mode} mode · up {:02}:{:02}:{:02}", crate::APP_VERSION, secs / 3600, (secs / 60) % 60, secs % 60),
            status,
            if warn { ff::Badge::Warn } else { ff::Badge::Good },
        )
    }

    /// Esc or the mouse's back button: close the top-most thing first
    /// (dialog, then movie details), then return to the dashboard.
    fn green_step_back(&mut self) {
        if self.dialog.is_some() {
            self.dialog = None;
        } else if self.page == Page::Movies && self.detail.is_some() {
            self.detail = None;
        } else {
            self.green_back();
        }
    }

    pub(crate) fn green_layout(&mut self, ui: &mut egui::Ui) {
        let full = ui.max_rect();
        let ctx = ui.ctx().clone();
        let t = ctx.input(|i| i.time) as f32;
        let main;
        // Modal windows (first-run, update) own the keyboard while open.
        let blocked = self.first_run.is_some() || self.update_prompt_open();
        if self.green_in_page {
            paint_background(&ctx, pos2(full.left() + full.width() * 0.2, full.top() + full.height() * 0.15));
            main = self.green_page(ui, full, blocked);
        } else {
            let r = (full.height() * 0.24).min(full.width() * 0.16).max(90.0);
            let orb_c = pos2(full.left() + full.width() * 0.3, full.center().y - 24.0);
            paint_background(&ctx, orb_c);
            self.green_home(ui, full, orb_c, r, t, blocked);
            ctx.request_repaint_after(std::time::Duration::from_millis(33));
            main = full;
        }
        self.show_dialog(&ctx);
        self.first_run_wizard(&ctx);
        self.update_prompt(&ctx);
        self.toasts(&ctx, pos2(main.right() - 14.0, main.top() + 14.0));
    }

    fn green_home(&mut self, ui: &mut egui::Ui, full: Rect, orb_c: Pos2, r: f32, t: f32, blocked: bool) {
        paint_big_orb(ui.painter(), orb_c, r, t);
        let n = MENU.len();
        self.green_cursor = self.green_cursor.min(n - 1);
        let row_h = ((full.height() - 160.0) / n as f32).clamp(40.0, 66.0);
        let node_r = (row_h * 0.42).min(28.0);
        let arc_r = (r * 1.5).max(n as f32 * row_h * 0.62);
        let tab_w = (full.right() - orb_c.x - arc_r - 140.0).clamp(220.0, 460.0);

        let typing = ui.ctx().egui_wants_keyboard_input();
        if !typing && !blocked {
            let (up, down, enter) = ui.input(|i| (i.key_pressed(Key::ArrowUp), i.key_pressed(Key::ArrowDown), i.key_pressed(Key::Enter)));
            if up {
                self.green_cursor = (self.green_cursor + n - 1) % n;
            }
            if down {
                self.green_cursor = (self.green_cursor + 1) % n;
            }
            if enter {
                self.green_open(MENU[self.green_cursor].0);
            }
        }

        let mut open = None;
        for (i, (page, label)) in MENU.iter().enumerate() {
            let dy = (i as f32 - (n as f32 - 1.0) / 2.0) * row_h;
            let dx = (arc_r * arc_r - dy * dy).max(0.0).sqrt();
            let node_c = pos2(orb_c.x + dx, orb_c.y + dy);
            let tab = Rect::from_min_size(pos2(node_c.x + node_r + 26.0, node_c.y - row_h * 0.38), Vec2::new(tab_w, row_h * 0.76));
            let hit = Rect::from_min_max(pos2(node_c.x - node_r, tab.top()), tab.right_bottom());
            let resp = ui.interact(hit, ui.id().with(("green_menu", i)), Sense::click());
            if resp.hovered() {
                // Only a moving mouse takes the highlight, so a pointer resting
                // over a tab doesn't fight the arrow keys.
                if ui.input(|i| i.pointer.is_moving()) {
                    self.green_cursor = i;
                }
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if resp.clicked() {
                open = Some(*page);
            }
            let lit = self.green_cursor == i;
            let p = ui.painter();
            let wire = Stroke::new(2.0, rgba(150, 255, 70, if lit { 230 } else { 140 }));
            let a = node_c + Vec2::new(node_r, 0.0);
            let b = pos2(tab.left() - 6.0, node_c.y);
            p.line_segment([a, b], wire);
            p.line_segment([pos2(a.x + 8.0, a.y), pos2(a.x + 14.0, a.y - row_h * 0.3)], Stroke::new(1.0, rgba(150, 255, 70, 90)));
            paint_node(p, node_c, node_r, lit);
            paint_tab(p, tab, label, lit, (row_h * 0.42).min(25.0));
        }
        if let Some(page) = open {
            self.green_open(page);
        }

        let (top, bottom, badge) = self.green_status();
        let box_rect = Rect::from_min_size(pos2(full.left() + 6.0, full.bottom() - 64.0), Vec2::new((full.width() * 0.5).min(600.0), 54.0));
        status_box(ui, box_rect, &top, &bottom, badge);
        if ui.interact(box_rect, ui.id().with("green_home_status"), Sense::click()).on_hover_text("Open System").clicked() {
            self.green_open(Page::System);
        }
        // Simple / Advanced switch (the FFVII look keeps it in the side window).
        let mode = if self.cfg.advanced_mode { "ADVANCED" } else { "SIMPLE" };
        if hint(ui, pos2(box_rect.right() + 24.0, full.bottom() - 36.0), "MODE", mode)
            .on_hover_text("Simple shows the settings most people need. Advanced shows everything Radarr has.")
            .clicked()
        {
            self.cfg.advanced_mode = !self.cfg.advanced_mode;
            self.changed();
        }
        if hint(ui, pos2(full.right() - 210.0, full.bottom() - 36.0), "ENTER", "SELECT").clicked() {
            self.green_open(MENU[self.green_cursor].0);
        }
    }

    /// Returns the content panel's rect (toasts anchor to it).
    fn green_page(&mut self, ui: &mut egui::Ui, full: Rect, blocked: bool) -> Rect {
        // While typing, Esc just leaves the text box (egui handles that).
        let typing = ui.ctx().egui_wants_keyboard_input();
        let esc = !typing && !blocked && ui.input(|i| i.key_pressed(Key::Escape));
        let mouse_back = !blocked && ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Extra1));
        if esc || mouse_back {
            self.green_step_back();
            if !self.green_in_page {
                return full;
            }
        }
        let label = match (self.page, &self.detail) {
            (Page::Movies, Some(d)) => self.core.lib().movies.get(&d.id).map(|m| m.title.clone()).unwrap_or_else(|| "Movies".into()),
            _ => MENU.iter().find(|m| m.0 == self.page).map(|m| m.1).unwrap_or("Movies").to_string(),
        };

        let bar = Rect::from_min_size(full.min, Vec2::new((full.width() * 0.42).clamp(300.0, 560.0), 56.0));
        let p = ui.painter();
        let slant = 22.0;
        p.add(Shape::convex_polygon(
            vec![pos2(bar.left() + slant, bar.top()), bar.right_top(), pos2(bar.right() - slant, bar.bottom()), bar.left_bottom()],
            pal().accent,
            Stroke::NONE,
        ));
        // Long movie titles shrink to fit the bar.
        let text = label.to_uppercase();
        let mut size = 28.0;
        while size > 14.0 && ui.painter().layout_no_wrap(text.clone(), ff::display_font(size), Color32::WHITE).size().x > bar.width() - 60.0 {
            size -= 1.0;
        }
        ui.painter().text(pos2(bar.left() + 34.0, bar.center().y), egui::Align2::LEFT_CENTER, text, ff::display_font(size), pal().on_accent);

        let (top, bottom, badge) = self.green_status();
        let sb_w = (full.width() - bar.width() - 30.0).clamp(0.0, 560.0);
        if sb_w > 200.0 {
            let r = Rect::from_min_size(pos2(full.right() - sb_w, full.top() + 2.0), Vec2::new(sb_w, 52.0));
            status_box(ui, r, &top, &bottom, badge);
            // The status box doubles as a shortcut to System.
            if self.page != Page::System && ui.interact(r, ui.id().with("green_status"), Sense::click()).on_hover_text("Open System").clicked() {
                self.go(Page::System);
            }
        }

        let frame = Rect::from_min_max(pos2(full.left(), bar.bottom() + 18.0), pos2(full.right(), full.bottom() - 64.0));
        ff::set_hide_headings(true);
        ff::window_at(ui, frame, "green_main", |ui| self.render_page_body(ui));
        ff::set_hide_headings(false);

        let back_label = if self.page == Page::Movies && self.detail.is_some() { "MOVIES" } else { "BACK" };
        if hint(ui, pos2(full.right() - 190.0, full.bottom() - 30.0), "ESC", back_label).clicked() {
            self.green_step_back();
        }
        frame
    }
}
