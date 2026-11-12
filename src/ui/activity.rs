//! Activity: Queue, History, Blocklist.

use eframe::egui::{self, Color32, RichText, Ui};

use super::common::{self, Dialog};
use super::theme::{self as ff, BAD, GOOD, LABEL, TEXT, TEXT_DIM, WARN};
use super::widgets as w;
use super::App;
use crate::core::Command;
use crate::model::*;

#[derive(Default)]
pub struct ActivityState {
    pub tab: u8,
    pub history_page: i64,
    pub history_filter: Option<HistoryEvent>,
    pub history: Option<(i64, Option<HistoryEvent>, Vec<History>, i64)>,
    pub show_finished: bool,
}

impl App {
    pub fn page_activity(&mut self, ui: &mut Ui) {
        let (queue_n, block_n) = {
            let lib = self.core.lib();
            (lib.active_downloads().count(), lib.blocklist.len())
        };
        ui.horizontal(|ui| {
            ff::title(ui, "Activity", 26.0, TEXT());
            ui.add_space(10.0);
            for (i, l) in [format!("Queue {queue_n}"), "History".into(), format!("Blocklist {block_n}")].iter().enumerate() {
                if w::pill(ui, l, self.activity.tab == i as u8).clicked() {
                    self.activity.tab = i as u8;
                    self.activity.history = None;
                }
            }
        });
        ff::divider(ui);
        match self.activity.tab {
            0 => self.queue(ui),
            1 => self.history_tab(ui),
            _ => self.blocklist_tab(ui),
        }
    }

    fn queue(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            if ui.button("Refresh").clicked() {
                self.core.run(Command::RefreshDownloads);
            }
            ui.checkbox(&mut self.activity.show_finished, "Show imported and failed");
            let errors: Vec<String> = {
                let s = self.core.status();
                let lib = self.core.lib();
                s.client_items.iter().filter_map(|(id, (_, r))| r.as_ref().err().map(|e| format!("{}: {e}", lib.clients.get(id).map(|c| c.name.as_str()).unwrap_or("client")))).collect()
            };
            for e in errors {
                ui.label(RichText::new(e).color(BAD()).size(12.0));
            }
        });
        let lib = self.core.lib().clone();
        let mut list: Vec<&Download> = lib.downloads.values().filter(|d| self.activity.show_finished || !d.state.finished()).collect();
        list.sort_by_key(|d| std::cmp::Reverse(d.added));
        if list.is_empty() {
            ui.add_space(20.0);
            ui.label(common::dim_text("Nothing is downloading."));
        }
        let pending: Vec<&Pending> = lib.pending.values().collect();
        let mut remove = None;
        let mut import = None;
        let mut open = None;
        let mut drop_pending = None;
        let mut grab_pending = None;
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            for (i, d) in list.iter().enumerate() {
                let movie = lib.movies.get(&d.movie_id);
                ui.horizontal(|ui| {
                    if i == 0 {
                        ff::paint_hand(ui.painter(), ui.cursor().min + egui::vec2(24.0, 22.0), 1.0);
                    }
                    ui.add_space(30.0);
                    ui.vertical(|ui| {
                        ui.set_width((ui.available_width() - 330.0).max(240.0));
                        let name = movie.map(|m| m.display()).unwrap_or_else(|| "(unknown movie)".into());
                        if ff::title(ui, &name, 18.0, TEXT()).interact(egui::Sense::click()).clicked() && movie.is_some() {
                            open = Some(d.movie_id);
                        }
                        ui.add(egui::Label::new(RichText::new(&d.title).color(TEXT_DIM()).size(12.0)).truncate()).on_hover_text(&d.title);
                        ui.horizontal(|ui| {
                            common::quality_chip(ui, &d.quality);
                            ui.label(RichText::new(format!("{} · {}", d.protocol.label(), d.indexer)).color(TEXT_DIM()).size(12.0));
                            if let Some(c) = lib.clients.get(&d.client_id) {
                                ui.label(RichText::new(format!("→ {}", c.name)).color(TEXT_DIM()).size(12.0));
                            }
                        });
                        for m in &d.messages {
                            ui.add(egui::Label::new(RichText::new(m).color(WARN()).size(12.0)).wrap());
                        }
                    });
                    ui.vertical(|ui| {
                        ui.set_width(300.0);
                        let frac = if d.size > 0 { 1.0 - d.remaining as f32 / d.size as f32 } else { 0.0 };
                        let color = match d.state {
                            DownloadState::Failed => BAD(),
                            DownloadState::Warning => WARN(),
                            DownloadState::Imported => GOOD(),
                            _ => Color32::WHITE,
                        };
                        ui.horizontal(|ui| {
                            ff::stat_label(ui, "MP");
                            ui.label(RichText::new(format!("{:.0}%", frac * 100.0)).color(TEXT()));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ff::tag(ui, d.state.label(), color);
                            });
                        });
                        ff::stat_bar(ui, frac, 290.0, Color32::from_rgb(70, 130, 230), color);
                        ui.label(RichText::new(format!("{} of {} · {}", crate::decision::human_size(d.size.saturating_sub(d.remaining)), crate::decision::human_size(d.size), crate::core::human_rate_eta(d.eta_secs))).color(TEXT_DIM()).size(12.0));
                        ui.horizontal(|ui| {
                            if matches!(d.state, DownloadState::Warning | DownloadState::ImportPending) && ui.small_button("Manual import").clicked() {
                                import = Some((d.output_path.clone(), d.movie_id));
                            }
                            if !d.state.finished() && ui.small_button(RichText::new("Remove").color(BAD())).clicked() {
                                remove = Some((d.id, d.title.clone()));
                            }
                        });
                    });
                });
                ff::divider(ui);
            }
            if !pending.is_empty() {
                w::section(ui, "Waiting on delay profiles");
                for p in pending {
                    ui.horizontal(|ui| {
                        let name = lib.movies.get(&p.movie_id).map(|m| m.display()).unwrap_or_default();
                        ui.label(RichText::new(name).color(TEXT()).strong());
                        ui.add(egui::Label::new(RichText::new(&p.release.title).color(TEXT_DIM()).size(12.0)).truncate());
                        if let Some(t) = p.release_at {
                            ui.label(RichText::new(format!("until {}", common::when(Some(t), &self.cfg))).color(LABEL()).size(12.0));
                        }
                        if ui.small_button("Grab now").clicked() {
                            grab_pending = Some(p.clone());
                        }
                        if ui.small_button("Drop").clicked() {
                            drop_pending = Some(p.id);
                        }
                    });
                }
            }
        });
        if let Some((id, title)) = remove {
            self.dialog = Some(Dialog::RemoveQueue { id, title, from_client: true, blocklist: false, search: false });
        }
        if let Some((path, mid)) = import {
            self.dialog = Some(Dialog::ManualImport(super::movies::ManualImport { folder: path, movie_hint: Some(mid), rows: vec![], mode: 2, scanning: None, movie_search: vec![] }));
        }
        if let Some(id) = open {
            self.open_movie(id);
        }
        if let Some(id) = drop_pending {
            self.core.remove::<Pending>(id);
        }
        if let Some(p) = grab_pending {
            self.core.remove::<Pending>(p.id);
            let lib = self.core.lib().clone();
            let c = crate::decision::candidate(&lib, p.release, Some(p.movie_id));
            crate::search::grab_in_background(&self.core, c);
        }
    }

    fn history_tab(&mut self, ui: &mut Ui) {
        let page_size = 50;
        ui.horizontal(|ui| {
            let label = self.activity.history_filter.map(|e| e.label()).unwrap_or("All events");
            egui::ComboBox::from_id_salt("hist-filter").selected_text(label).show_ui(ui, |ui| {
                let mut f = self.activity.history_filter;
                ui.selectable_value(&mut f, None, "All events");
                for e in [HistoryEvent::Grabbed, HistoryEvent::Imported, HistoryEvent::DownloadFailed, HistoryEvent::FileDeleted, HistoryEvent::FileRenamed] {
                    ui.selectable_value(&mut f, Some(e), e.label());
                }
                if f != self.activity.history_filter {
                    self.activity.history_filter = f;
                    self.activity.history_page = 0;
                }
            });
            let page = self.activity.history_page;
            let last = self.activity.history.as_ref().map(|h| ((h.3 + page_size - 1) / page_size - 1).max(0)).unwrap_or(0);
            if ui.add_enabled(page > 0, egui::Button::new("‹ Newer")).clicked() {
                self.activity.history_page = page - 1;
            }
            if ui.add_enabled(page < last, egui::Button::new("Older ›")).clicked() {
                self.activity.history_page = page + 1;
            }
            if ui.button("Refresh").clicked() {
                self.activity.history = None;
            }
            if ui.button(RichText::new("Clear history").color(BAD())).clicked() {
                self.dialog = Some(Dialog::Confirm { title: "Clear history".into(), text: "Delete every history entry? Downloads and files aren't touched.".into(), action: common::Confirm::ClearHistory });
            }
        });
        let key = (self.activity.history_page, self.activity.history_filter);
        let stale = self.activity.history.as_ref().map(|(p, f, _, _)| (*p, *f) != key).unwrap_or(true);
        if stale {
            let items = self.core.db.history_page(key.0 * page_size, page_size, key.1);
            let total = self.core.db.history_count(key.1);
            self.activity.history = Some((key.0, key.1, items, total));
            // History got shorter (cleared, or entries removed): back to its last page.
            if key.0 > 0 && key.0 * page_size >= total {
                self.activity.history_page = ((total + page_size - 1) / page_size - 1).max(0);
                ui.ctx().request_repaint();
            }
        }
        let (_, _, items, total) = self.activity.history.clone().unwrap();
        ui.label(common::dim_text(&format!("Page {} of {}", key.0 + 1, (total as f64 / page_size as f64).ceil().max(1.0))));
        self.history_list(ui, &items, true);
    }

    fn blocklist_tab(&mut self, ui: &mut Ui) {
        let lib = self.core.lib().clone();
        ui.horizontal(|ui| {
            ui.add(egui::Label::new(common::dim_text("Releases here are never grabbed again for that movie.")).wrap());
            if !lib.blocklist.is_empty() && ui.button(RichText::new("Clear blocklist").color(BAD())).clicked() {
                self.dialog = Some(Dialog::Confirm { title: "Clear blocklist".into(), text: "Remove every release from the blocklist?".into(), action: common::Confirm::ClearBlocklist });
            }
        });
        let mut remove = None;
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            egui::Grid::new("blocklist").striped(true).spacing([12.0, 6.0]).show(ui, |ui| {
                for h in ["Movie", "Release", "Quality", "Indexer", "Reason", "Date", ""] {
                    ff::title(ui, h, 13.0, LABEL());
                }
                ui.end_row();
                let mut items: Vec<&Blocklist> = lib.blocklist.values().collect();
                items.sort_by_key(|b| std::cmp::Reverse(b.date));
                for b in items {
                    ui.label(lib.movies.get(&b.movie_id).map(|m| m.display()).unwrap_or_default());
                    sized_left(ui, 300.0, egui::Label::new(RichText::new(&b.source_title).size(12.0)).truncate());
                    ui.label(crate::quality::full_name(&b.quality));
                    ui.label(&b.indexer);
                    ui.add(egui::Label::new(RichText::new(&b.message).color(TEXT_DIM()).size(12.0)).truncate());
                    ui.label(common::when(b.date, &self.cfg));
                    if ui.small_button("Remove").clicked() {
                        remove = Some(b.id);
                    }
                    ui.end_row();
                }
            });
        });
        if let Some(id) = remove {
            self.core.remove::<Blocklist>(id);
        }
    }
}

/// A label in a fixed-width, left-aligned cell (for grid columns).
fn sized_left(ui: &mut Ui, width: f32, label: egui::Label) -> egui::Response {
    ui.allocate_ui_with_layout(egui::vec2(width, 18.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.set_width(width);
        ui.add(label)
    })
    .inner
}
