//! Calendar: cinema, digital and physical release dates by month or week.

use chrono::{Datelike, Duration, NaiveDate};
use eframe::egui::{self, Color32, RichText, Sense, Ui, Vec2};

use super::common;
use super::theme::{self as ff, BAD, GOOD, LABEL, TEXT, TEXT_DIM, WARN};
use super::widgets as w;
use super::App;
use crate::model::Movie;

#[derive(Default)]
pub struct CalendarState {
    pub anchor: Option<NaiveDate>,
    pub show_unmonitored: bool,
}

#[derive(Clone, Copy)]
enum Kind {
    Cinema,
    Digital,
    Physical,
}

impl App {
    pub fn page_calendar(&mut self, ui: &mut Ui) {
        let today = chrono::Local::now().date_naive();
        let anchor = *self.calendar.anchor.get_or_insert(today);
        let week = self.cfg.calendar_week_view;
        let first_dow = self.cfg.first_day_of_week as i64;
        let (start, end) = if week {
            let back = (anchor.weekday().num_days_from_sunday() as i64 - first_dow).rem_euclid(7);
            let s = anchor - Duration::days(back);
            (s, s + Duration::days(6))
        } else {
            let first = anchor.with_day(1).unwrap();
            let back = (first.weekday().num_days_from_sunday() as i64 - first_dow).rem_euclid(7);
            let s = first - Duration::days(back);
            (s, s + Duration::days(41))
        };
        ui.horizontal(|ui| {
            ff::title(ui, "Calendar", 26.0, TEXT);
            ui.add_space(10.0);
            if ui.button("‹").clicked() {
                self.calendar.anchor = Some(if week { anchor - Duration::days(7) } else { (anchor.with_day(1).unwrap() - Duration::days(1)).with_day(1).unwrap() });
            }
            if ui.button("Today").clicked() {
                self.calendar.anchor = Some(today);
            }
            if ui.button("›").clicked() {
                self.calendar.anchor = Some(if week { anchor + Duration::days(7) } else { (anchor.with_day(28).unwrap() + Duration::days(7)).with_day(1).unwrap() });
            }
            ff::title(ui, &if week { format!("{} – {}", start.format("%b %-d"), end.format("%b %-d, %Y")) } else { anchor.format("%B %Y").to_string() }, 20.0, WARN);
            ui.add_space(10.0);
            if w::pill(ui, "Month", !week).clicked() {
                self.cfg.calendar_week_view = false;
                self.changed();
            }
            if w::pill(ui, "Week", week).clicked() {
                self.cfg.calendar_week_view = true;
                self.changed();
            }
            ui.checkbox(&mut self.calendar.show_unmonitored, "Unmonitored");
        });
        ui.horizontal(|ui| {
            for (c, l) in [(LABEL, "In cinemas"), (WARN, "Digital"), (Color32::from_rgb(200, 140, 255), "Physical")] {
                ff::tag(ui, l, c);
            }
            ui.label(common::dim_text("· green = downloaded, red = missing"));
            if self.cfg.api_enabled && ui.small_button("Copy iCal link").on_hover_text("Subscribe to this in your phone or desktop calendar (Turks' API must be reachable from that device: Settings › General › Allow other computers)").clicked() {
                let url = format!("http://{}:{}{}/feed/v3/calendar/turks.ics?apikey={}", local_ip(), self.cfg.api_port, self.cfg.url_base, self.cfg.api_key);
                let _ = arboard::Clipboard::new().and_then(|mut c| c.set_text(url));
                self.notify("Calendar link copied.");
            }
        });
        ff::divider(ui);

        let lib = self.core.lib().clone();
        let mut events: Vec<(NaiveDate, Kind, Movie)> = Vec::new();
        for m in lib.movies.values().filter(|m| m.monitored || self.calendar.show_unmonitored) {
            for (d, k) in [(m.in_cinemas, Kind::Cinema), (m.digital_release, Kind::Digital), (m.physical_release, Kind::Physical)] {
                if let Some(d) = d {
                    if d >= start && d <= end {
                        events.push((d, k, m.clone()));
                    }
                }
            }
        }
        let days = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
        let col_w = (ui.available_width() - 6.0 * 6.0) / 7.0;
        let rows = if week { 1 } else { 6 };
        let cell_h = if week { (ui.available_height() - 30.0).max(200.0) } else { ((ui.available_height() - 30.0) / 6.0).max(80.0) };
        let mut open = None;
        ui.horizontal(|ui| {
            for i in 0..7 {
                let d = days[((i + first_dow) % 7) as usize];
                ui.add_sized([col_w, 18.0], egui::Label::new(RichText::new(d).color(LABEL).strong()));
            }
        });
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            for r in 0..rows {
                ui.horizontal(|ui| {
                    for c in 0..7 {
                        let day = start + Duration::days((r * 7 + c) as i64);
                        let (rect, _) = ui.allocate_exact_size(Vec2::new(col_w, cell_h), Sense::hover());
                        let in_month = week || day.month() == anchor.month();
                        let fill = if day == today { Color32::from_rgba_premultiplied(64, 112, 210, 90) } else { Color32::from_rgba_premultiplied(8, 12, 52, if in_month { 120 } else { 50 }) };
                        ui.painter().rect(rect, egui::CornerRadius::same(4), fill, egui::Stroke::new(1.0, Color32::from_rgba_premultiplied(150, 160, 220, 60)), egui::StrokeKind::Inside);
                        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink(4.0)).layout(egui::Layout::top_down(egui::Align::Min)));
                        child.label(RichText::new(day.day().to_string()).color(if in_month { TEXT } else { TEXT_DIM }).strong().size(12.0));
                        for (_, k, m) in events.iter().filter(|e| e.0 == day) {
                            let color = match k {
                                Kind::Cinema => LABEL,
                                Kind::Digital => WARN,
                                Kind::Physical => Color32::from_rgb(200, 140, 255),
                            };
                            let has = m.movie_file_id.is_some();
                            let resp = child
                                .horizontal(|ui| {
                                    let (dot, _) = ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
                                    ui.painter().circle_filled(dot.center(), 4.0, if has { GOOD } else if day <= today { BAD } else { color });
                                    ui.add(egui::Label::new(RichText::new(&m.title).color(color).size(12.0)).truncate());
                                })
                                .response
                                .interact(Sense::click());
                            let kind = match k {
                                Kind::Cinema => "In cinemas",
                                Kind::Digital => "Digital release",
                                Kind::Physical => "Physical release",
                            };
                            if resp.on_hover_text(format!("{}\n{kind}\n{}", m.display(), if has { "Downloaded" } else { "No file yet" })).clicked() {
                                open = Some(m.id);
                            }
                        }
                    }
                });
                ui.add_space(4.0);
            }
        });
        if let Some(id) = open {
            self.open_movie(id);
        }
    }
}

/// Best guess at this PC's LAN address (for the calendar link).
fn local_ip() -> String {
    std::net::UdpSocket::bind("0.0.0.0:0")
        .and_then(|s| {
            s.connect("8.8.8.8:80")?;
            s.local_addr()
        })
        .map(|a| a.ip().to_string())
        .unwrap_or_else(|_| "localhost".into())
}
