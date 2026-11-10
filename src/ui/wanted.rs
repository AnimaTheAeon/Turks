//! Wanted: missing movies and cutoff unmet (upgrades).

use eframe::egui::{self, RichText, Ui};

use super::common;
use super::theme::{self as ff, LABEL, TEXT, TEXT_DIM, WARN};
use super::widgets as w;
use super::App;
use crate::core::Command;
use crate::model::*;

#[derive(Default)]
pub struct WantedState {
    pub tab: u8,
    pub include_unavailable: bool,
}

impl App {
    pub fn page_wanted(&mut self, ui: &mut Ui) {
        let lib = self.core.lib().clone();
        let delay = self.cfg.availability_delay_days;
        let missing: Vec<Movie> = {
            let mut v: Vec<Movie> = lib
                .movies
                .values()
                .filter(|m| m.monitored && m.movie_file_id.is_none() && (self.wanted.include_unavailable || m.is_available(delay)))
                .cloned()
                .collect();
            v.sort_by_key(|m| std::cmp::Reverse(m.digital_release.or(m.physical_release).or(m.in_cinemas)));
            v
        };
        let cutoff = crate::library::cutoff_unmet(&self.core);
        ui.horizontal(|ui| {
            ff::title(ui, "Wanted", 26.0, TEXT);
            ui.add_space(10.0);
            if w::pill(ui, &format!("Missing {}", missing.len()), self.wanted.tab == 0).clicked() {
                self.wanted.tab = 0;
            }
            if w::pill(ui, &format!("Cutoff unmet {}", cutoff.len()), self.wanted.tab == 1).clicked() {
                self.wanted.tab = 1;
            }
        });
        ui.horizontal(|ui| {
            if self.wanted.tab == 0 {
                if ui.button("Search all missing").clicked() {
                    self.core.run(Command::MissingSearch);
                }
                ui.checkbox(&mut self.wanted.include_unavailable, "Include movies that aren't available yet");
            } else if ui.button("Search all cutoff unmet").clicked() {
                self.core.run(Command::CutoffUnmetSearch);
            }
        });
        ff::divider(ui);
        let list = if self.wanted.tab == 0 { missing } else { cutoff };
        if list.is_empty() {
            ui.label(common::dim_text(if self.wanted.tab == 0 { "Nothing is missing. Nice." } else { "Every file meets its profile's cutoff." }));
            return;
        }
        let mut open = None;
        let mut search = None;
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            egui::Grid::new("wanted").striped(true).spacing([14.0, 6.0]).show(ui, |ui| {
                for h in ["Movie", "Profile", "Released", "Current", "Last searched", ""] {
                    ff::title(ui, h, 13.0, LABEL);
                }
                ui.end_row();
                for m in &list {
                    if ui.add(egui::Label::new(RichText::new(m.display()).color(TEXT).strong()).sense(egui::Sense::click())).clicked() {
                        open = Some(m.id);
                    }
                    ui.label(lib.profile_for(m).map(|p| p.name.clone()).unwrap_or_default());
                    let rel = m.digital_release.or(m.physical_release).or(m.in_cinemas);
                    ui.label(RichText::new(common::date_label(rel, &self.cfg)).color(if m.is_available(delay) { TEXT } else { WARN }));
                    ui.label(lib.file_for(m).map(|f| crate::quality::full_name(&f.quality)).unwrap_or_else(|| "—".into()));
                    ui.label(RichText::new(common::when(m.last_search, &self.cfg)).color(TEXT_DIM));
                    if ui.small_button("Search").clicked() {
                        search = Some(m.id);
                    }
                    ui.end_row();
                }
            });
        });
        if let Some(id) = open {
            self.open_movie(id);
        }
        if let Some(id) = search {
            self.core.run(Command::SearchMovies(vec![id]));
        }
    }
}
