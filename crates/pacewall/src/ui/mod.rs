//! Fenêtre principale (egui) : accueil, éditeur d'overlays, données en direct, historique, réglages.
//! Lancée à la demande par le processus de fond ; fermée, elle libère toute sa mémoire.

mod canvas;
mod editor;
mod explorer;
mod history;
mod home;
mod kit;
mod settings_page;

use crate::theme::*;
use crate::win;
use eframe::egui::{self, pos2, vec2, Align2, Color32, FontId, Frame, Rect, Sense, Stroke};
use pwcore::live::{Live, Update};
use pwcore::Data;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Home,
    Overlays,
    Data,
    History,
    Settings,
}

impl Page {
    fn from_arg(s: &str) -> Page {
        match s {
            "overlays" => Page::Overlays,
            "data" => Page::Data,
            "history" => Page::History,
            "settings" => Page::Settings,
            _ => Page::Home,
        }
    }
}

/// Lecture du jeu pour les pages qui en ont besoin (aucune lecture tant qu'aucune page ne la demande).
pub struct Feed {
    live: Live,
    pub data: Data,
    pub game: bool,
}

impl Feed {
    /// `true` si de nouvelles données sont arrivées.
    pub fn poll(&mut self, with_grid: bool) -> bool {
        match self.live.poll(with_grid) {
            Update::Data(d) => {
                self.data = d;
                self.game = true;
                true
            }
            Update::NoGame => {
                self.data = Data::default();
                self.game = false;
                true
            }
            Update::Unchanged => false,
        }
    }
}

struct App {
    page: Page,
    feed: Feed,
    start: Instant,
    home: home::State,
    editor: editor::State,
    explorer: explorer::State,
    history: history::State,
    settings: settings_page::State,
    dbg_last: f64,
}

pub fn run(page: Option<String>) {
    if !win::single_instance("PaceWallUi") {
        win::focus_window(win::UI_TITLE);
        return;
    }
    let icon = egui::IconData { rgba: icon_rgba(), width: 64, height: 64 };
    let viewport = egui::ViewportBuilder::default()
        .with_title(win::UI_TITLE)
        .with_inner_size([1400.0, 860.0])
        .with_min_inner_size([980.0, 620.0])
        .with_icon(icon);
    let options = eframe::NativeOptions { viewport, centered: true, ..Default::default() };
    let first = page.as_deref().map(Page::from_arg).unwrap_or(Page::Home);
    let _ = eframe::run_native(
        win::UI_TITLE,
        options,
        Box::new(move |cc| {
            crate::theme::install(&cc.egui_ctx);
            Ok(Box::new(App::new(first)))
        }),
    );
}

fn icon_rgba() -> Vec<u8> {
    let bytes = include_bytes!("../../icons/tray.png");
    let mut reader = match png::Decoder::new(std::io::Cursor::new(&bytes[..])).read_info() {
        Ok(r) => r,
        Err(_) => return vec![0; 64 * 64 * 4],
    };
    let mut buf = vec![0; reader.output_buffer_size()];
    match reader.next_frame(&mut buf) {
        Ok(info) => {
            buf.truncate(info.buffer_size());
            buf
        }
        Err(_) => vec![0; 64 * 64 * 4],
    }
}

impl App {
    fn new(page: Page) -> Self {
        let mut app = Self {
            page,
            feed: Feed { live: Live::new(), data: Data::default(), game: false },
            start: Instant::now(),
            home: home::State::default(),
            editor: editor::State::default(),
            explorer: explorer::State::default(),
            history: history::State::default(),
            settings: settings_page::State::default(),
            dbg_last: 0.0,
        };
        app.enter(page);
        app
    }

    /// Chargements propres à chaque page, faits quand on y arrive.
    fn enter(&mut self, page: Page) {
        self.page = page;
        match page {
            Page::Home => self.home.refresh(),
            Page::History => self.history.reload(),
            Page::Settings => self.settings.refresh(),
            _ => {}
        }
    }

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            ui.add_space(16.0);
            let (r, _) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::hover());
            ui.painter().rect_filled(r, 0.0, BG);
            ui.painter().rect_filled(Rect::from_min_size(r.min, vec2(3.0, 26.0)), 0.0, ACCENT);
            ui.painter().text(r.center() + vec2(1.0, 0.0), Align2::CENTER_CENTER, "PW", num_font(11.0), TEXT);
            ui.label(egui::RichText::new("PACE").size(15.0).strong());
            ui.add_space(-8.0);
            ui.label(egui::RichText::new("WALL").size(15.0).strong().color(ACCENT));
        });
        ui.add_space(18.0);

        let items = [
            (Page::Home, "Accueil"),
            (Page::Overlays, "Overlays"),
            (Page::Data, "Données en direct"),
            (Page::History, "Historique"),
            (Page::Settings, "Réglages"),
        ];
        let mut goto = None;
        for (page, label) in items {
            let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 38.0), Sense::click());
            let on = self.page == page;
            if on || resp.hovered() {
                ui.painter().rect_filled(rect, 0.0, ACCENT.gamma_multiply(0.10));
            }
            if on {
                ui.painter().rect_filled(Rect::from_min_size(rect.min, vec2(3.0, rect.height())), 0.0, ACCENT);
            }
            let color = if on { TEXT } else { MUTED };
            ui.painter().text(pos2(rect.min.x + 22.0, rect.center().y), Align2::LEFT_CENTER, label, FontId::proportional(14.5), color);
            if resp.clicked() {
                goto = Some(page);
            }
        }
        if let Some(p) = goto {
            self.enter(p);
        }

        // État en pied de barre (le premier élément ajouté est celui du bas).
        ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
            ui.add_space(18.0);
            let game = pwcore::live::game_running();
            let overlay = win::overlay_running();
            for (label, on, yes, no, off_color) in [("Jeu", game, "connecté", "absent", BAD), ("Overlay", overlay, "affichée", "fermée", MUTED)] {
                Frame::NONE.inner_margin(egui::Margin::symmetric(18, 0)).show(ui, |ui| {
                    kit::dot(ui, if on { OK } else { off_color }, label, if on { yes } else { no });
                });
            }
        });
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let now = self.start.elapsed().as_secs_f64();
        if std::env::var_os("PACEWALL_DEBUG_FPS").is_some() && now - self.dbg_last >= 1.0 {
            self.dbg_last = now;
            eprintln!("t={now:.1} passes={} frames={}", ui.ctx().cumulative_pass_nr(), ui.ctx().cumulative_frame_nr());
        }
        // Pied de barre rafraîchi une fois par seconde même sur une page statique.
        ui.ctx().request_repaint_after(Duration::from_secs(1));

        egui::Panel::left("nav").exact_size(216.0).resizable(false).frame(Frame::NONE.fill(PANEL).stroke(Stroke::new(1.0, LINE))).show(ui, |ui| self.sidebar(ui));
        egui::CentralPanel::default().frame(Frame::NONE.fill(BG).inner_margin(egui::Margin::same(0))).show(ui, |ui| match self.page {
            Page::Home => self.home.show(ui, &mut self.feed),
            Page::Overlays => self.editor.show(ui, &mut self.feed, now),
            Page::Data => self.explorer.show(ui, &mut self.feed),
            Page::History => self.history.show(ui),
            Page::Settings => self.settings.show(ui),
        });
        if let Some(p) = self.take_request() {
            self.enter(p);
        }
        let _ = Color32::TRANSPARENT;
    }
}

// Demande de changement de page émise par une page (ex. bouton « Modifier » de l'accueil).
thread_local! {
    static REQUEST: std::cell::Cell<Option<Page>> = const { std::cell::Cell::new(None) };
}

pub fn goto(page: Page) {
    REQUEST.with(|r| r.set(Some(page)));
}

impl App {
    fn take_request(&self) -> Option<Page> {
        REQUEST.with(|r| r.take())
    }
}
