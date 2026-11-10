//! System tray icon with quick actions (Windows).

#![cfg(windows)]

use std::sync::atomic::{AtomicBool, Ordering};

use tray_icon::menu::{IsMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

use crate::core::{Command, Core};

pub static QUIT: AtomicBool = AtomicBool::new(false);

pub fn show_window(ctx: &eframe::egui::Context) {
    use eframe::egui::ViewportCommand;
    ctx.send_viewport_cmd(ViewportCommand::Visible(true));
    ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
    ctx.send_viewport_cmd(ViewportCommand::Focus);
    ctx.request_repaint();
}

pub fn hide_window(ctx: &eframe::egui::Context) {
    ctx.send_viewport_cmd(eframe::egui::ViewportCommand::Visible(false));
}

pub struct Tray {
    icon: TrayIcon,
    status: MenuItem,
    last_tip: String,
}

struct Ids {
    rss: MenuId,
    missing: MenuId,
    downloads: MenuId,
    show: MenuId,
    quit: MenuId,
}

impl Tray {
    pub fn new(core: Core, ctx: eframe::egui::Context) -> Option<Self> {
        let status = MenuItem::new("Turks", false, None);
        let rss = MenuItem::new("RSS sync now", true, None);
        let missing = MenuItem::new("Search all missing", true, None);
        let downloads = MenuItem::new("Check downloads", true, None);
        let show = MenuItem::new("Show window", true, None);
        let quit = MenuItem::new("Quit", true, None);
        let menu = Menu::new();
        let (s1, s2) = (PredefinedMenuItem::separator(), PredefinedMenuItem::separator());
        let items: [&dyn IsMenuItem; 8] = [&status, &s1, &rss, &missing, &downloads, &show, &s2, &quit];
        menu.append_items(&items).ok()?;
        let ids = Ids { rss: rss.id().clone(), missing: missing.id().clone(), downloads: downloads.id().clone(), show: show.id().clone(), quit: quit.id().clone() };

        let img = image::load_from_memory(include_bytes!("../resources/icon_64.png")).ok()?.to_rgba8();
        let (w, h) = img.dimensions();
        let icon = tray_icon::Icon::from_rgba(img.into_raw(), w, h).ok()?;
        let tray = TrayIconBuilder::new().with_menu(Box::new(menu)).with_tooltip("Turks").with_icon(icon).with_menu_on_left_click(false).build().ok()?;

        let ctx2 = ctx.clone();
        TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                show_window(&ctx2);
            }
        }));
        MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
            if e.id == ids.rss {
                core.run(Command::RssSync);
            } else if e.id == ids.missing {
                core.run(Command::MissingSearch);
            } else if e.id == ids.downloads {
                core.run(Command::RefreshDownloads);
            } else if e.id == ids.show {
                show_window(&ctx);
            } else if e.id == ids.quit {
                QUIT.store(true, Ordering::SeqCst);
                show_window(&ctx);
                ctx.send_viewport_cmd(eframe::egui::ViewportCommand::Close);
            }
            ctx.request_repaint();
        }));
        Some(Self { icon: tray, status, last_tip: String::new() })
    }

    pub fn update(&mut self, downloading: usize, missing: usize) {
        let tip = format!("Turks · {downloading} downloading · {missing} missing");
        if tip != self.last_tip {
            let _ = self.icon.set_tooltip(Some(&tip));
            self.status.set_text(format!("{downloading} downloading · {missing} missing"));
            self.last_tip = tip;
        }
    }
}
