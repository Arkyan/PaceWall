//! Fenêtre principale (egui) : accueil, éditeur d'overlays, données en direct, historique, réglages.
//! Lancée à la demande par le processus de fond ; fermée, elle libère toute sa mémoire.

mod alerts_page;
mod canvas;
mod editor;
mod explorer;
mod history;
mod replay;
mod home;
mod kit;
mod memory;
mod settings_page;
mod setup;
mod tutorial;
mod update_box;

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
    Memory,
    History,
    Alerts,
    Settings,
}

impl Page {
    fn from_arg(s: &str) -> Page {
        match s {
            "overlays" => Page::Overlays,
            "data" => Page::Data,
            "memory" => Page::Memory,
            "history" => Page::History,
            "alerts" => Page::Alerts,
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
    /// Referme la lecture du jeu (la zone mémoire n'est plus tenue ouverte par cette fenêtre).
    pub fn release(&mut self) {
        self.live = Live::new();
        self.data = Data::default();
        self.game = false;
    }

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
    memory: memory::State,
    history: history::State,
    settings: settings_page::State,
    alerts: alerts_page::State,
    tutorial: tutorial::Tutorial,
    dbg_last: f64,
    sized: u8,
}

/// Fenêtre d'installation (voir `setup.rs`). `auto` : mise à jour sans question.
pub fn setup(auto: bool) {
    setup::run(auto);
}

pub fn run(page: Option<String>) {
    if !win::single_instance("PaceWallUi") {
        win::focus_window(win::UI_TITLE);
        return;
    }
    let icon = egui::IconData { rgba: icon_rgba(), width: 64, height: 64 };
    // PACEWALL_UI_SIZE=1000x640 : taille de départ imposée (tests de mise en page).
    let size = std::env::var("PACEWALL_UI_SIZE")
        .ok()
        .and_then(|v| {
            let (w, h) = v.split_once('x')?;
            Some([w.parse::<f32>().ok()?, h.parse::<f32>().ok()?])
        })
        .unwrap_or([1400.0, 860.0]);
    let viewport = egui::ViewportBuilder::default()
        .with_title(win::UI_TITLE)
        .with_inner_size(size)
        .with_min_inner_size([760.0, 520.0])
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
            memory: memory::State::default(),
            history: history::State::default(),
            settings: settings_page::State::default(),
            alerts: alerts_page::State::default(),
            tutorial: tutorial::Tutorial::new(),
            dbg_last: 0.0,
            sized: 0,
        };
        app.enter(page);
        app
    }

    /// Chargements propres à chaque page, faits quand on y arrive.
    fn enter(&mut self, page: Page) {
        self.page = page;
        // Les pages qui ne lisent pas le jeu libèrent sa zone mémoire : sinon elle survit à la fin du jeu (ou d'une simulation).
        if !matches!(page, Page::Home | Page::Overlays | Page::Data) {
            self.feed.release();
        }
        match page {
            Page::Home => self.home.refresh(),
            Page::History => self.history.reload(),
            Page::Alerts => self.alerts.refresh(),
            Page::Settings => self.settings.refresh(),
            _ => {}
        }
    }

    /// `wide` : barre complète (libellés) ; sinon barre réduite à des icônes, pour les petites fenêtres.
    fn sidebar(&mut self, ui: &mut egui::Ui, wide: bool) {
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            ui.add_space(if wide { 16.0 } else { 19.0 });
            let (r, _) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::hover());
            ui.painter().rect_filled(r, 0.0, BG);
            ui.painter().rect_filled(Rect::from_min_size(r.min, vec2(3.0, 26.0)), 0.0, ui_accent());
            ui.painter().text(r.center() + vec2(1.0, 0.0), Align2::CENTER_CENTER, "PW", num_font(11.0), TEXT);
            if wide {
                ui.label(egui::RichText::new("PACE").size(15.0).strong());
                ui.add_space(-8.0);
                ui.label(egui::RichText::new("WALL").size(15.0).strong().color(ui_accent()));
            }
        });
        ui.add_space(18.0);

        let items = [
            (Page::Home, "Accueil"),
            (Page::Overlays, "Overlays"),
            (Page::Data, "Données en direct"),
            (Page::Memory, "Mémoire du jeu"),
            (Page::History, "Historique"),
            (Page::Alerts, "Alertes"),
            (Page::Settings, "Réglages"),
        ];
        let mut goto = None;
        for (page, label) in items {
            let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::click());
            let on = self.page == page;
            if on || resp.hovered() {
                ui.painter().rect_filled(rect, 0.0, ui_accent().gamma_multiply(0.10));
            }
            if on {
                ui.painter().rect_filled(Rect::from_min_size(rect.min, vec2(3.0, rect.height())), 0.0, ui_accent());
            }
            let color = if on { ui_accent() } else { MUTED };
            let icon_box = Rect::from_center_size(pos2(rect.min.x + if wide { 30.0 } else { rect.width() / 2.0 }, rect.center().y), vec2(18.0, 18.0));
            icon(ui.painter(), icon_box, page, color);
            if wide {
                ui.painter().text(pos2(rect.min.x + 52.0, rect.center().y), Align2::LEFT_CENTER, label, FontId::proportional(14.5), if on { TEXT } else { MUTED });
            }
            let resp = if wide { resp } else { resp.on_hover_text(label) };
            if resp.clicked() {
                goto = Some(page);
            }
        }
        if let Some(p) = goto {
            self.enter(p);
        }

        // État en pied de barre (le premier élément ajouté est celui du bas).
        let game = pwcore::live::game_running();
        let overlay = win::overlay_running();
        ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
            ui.add_space(18.0);
            for (label, on, yes, no, off_color) in [("Jeu", game, "connecté", "absent", BAD), ("Overlay", overlay, "affichée", "fermée", MUTED)] {
                if wide {
                    Frame::NONE.inner_margin(egui::Margin::symmetric(18, 0)).show(ui, |ui| {
                        kit::dot(ui, if on { OK } else { off_color }, label, if on { yes } else { no });
                    });
                } else {
                    ui.horizontal(|ui| {
                        ui.add_space(26.0);
                        let (r, resp) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
                        ui.painter().rect_filled(r, 0.0, if on { OK } else { off_color });
                        resp.on_hover_text(format!("{label} : {}", if on { yes } else { no }));
                    });
                    ui.add_space(6.0);
                }
            }
        });
    }
}

/// Icônes de la barre latérale, tracées à la main (24 × 24 ramenés à `r`).
fn icon(p: &egui::Painter, r: Rect, page: Page, color: Color32) {
    let st = Stroke::new(1.7, color);
    let pt = |x: f32, y: f32| pos2(r.min.x + x / 24.0 * r.width(), r.min.y + y / 24.0 * r.height());
    let line = |pts: &[(f32, f32)]| {
        p.add(egui::Shape::line(pts.iter().map(|&(x, y)| pt(x, y)).collect(), st));
    };
    match page {
        Page::Home => {
            line(&[(3.0, 11.0), (12.0, 4.0), (21.0, 11.0)]);
            line(&[(5.0, 10.0), (5.0, 20.0), (19.0, 20.0), (19.0, 10.0)]);
        }
        Page::Overlays => {
            line(&[(3.0, 4.0), (21.0, 4.0), (21.0, 17.0), (3.0, 17.0), (3.0, 4.0)]);
            line(&[(7.0, 20.0), (17.0, 20.0)]);
            line(&[(7.0, 9.0), (12.0, 9.0)]);
            line(&[(7.0, 13.0), (16.0, 13.0)]);
        }
        Page::Data => {
            line(&[(3.0, 4.0), (3.0, 20.0), (21.0, 20.0)]);
            line(&[(7.0, 15.0), (11.0, 10.0), (14.0, 13.0), (19.0, 6.0)]);
        }
        Page::Memory => {
            // Barrette de mémoire : un boîtier et ses broches.
            line(&[(4.0, 7.0), (20.0, 7.0), (20.0, 17.0), (4.0, 17.0), (4.0, 7.0)]);
            for x in [8.0, 12.0, 16.0] {
                line(&[(x, 3.0), (x, 7.0)]);
                line(&[(x, 17.0), (x, 21.0)]);
            }
        }
        Page::History => {
            p.circle_stroke(pt(12.0, 12.0), 9.0 / 24.0 * r.width(), st);
            line(&[(12.0, 7.0), (12.0, 12.0), (15.0, 14.0)]);
        }
        Page::Alerts => {
            // Haut-parleur avec deux ondes.
            line(&[(4.0, 9.0), (8.0, 9.0), (13.0, 5.0), (13.0, 19.0), (8.0, 15.0), (4.0, 15.0), (4.0, 9.0)]);
            line(&[(16.0, 9.0), (18.0, 12.0), (16.0, 15.0)]);
            line(&[(19.0, 6.0), (22.0, 12.0), (19.0, 18.0)]);
        }
        Page::Settings => {
            line(&[(4.0, 7.0), (14.0, 7.0)]);
            line(&[(18.0, 7.0), (20.0, 7.0)]);
            line(&[(4.0, 17.0), (6.0, 17.0)]);
            line(&[(10.0, 17.0), (20.0, 17.0)]);
            p.circle_stroke(pt(16.0, 7.0), 2.0 / 24.0 * r.width(), st);
            p.circle_stroke(pt(8.0, 17.0), 2.0 / 24.0 * r.width(), st);
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let now = self.start.elapsed().as_secs_f64();
        if std::env::var_os("PACEWALL_DEBUG_FPS").is_some() && now - self.dbg_last >= 1.0 {
            self.dbg_last = now;
            eprintln!("t={now:.1} passes={} frames={}", ui.ctx().cumulative_pass_nr(), ui.ctx().cumulative_frame_nr());
        }
        // Premier affichage : on connaît enfin la taille de l'écran, la fenêtre s'y adapte (image 1) puis se centre
        // (image 2, une fois la nouvelle taille appliquée : sinon elle déborde de l'écran).
        if self.sized < 2 && std::env::var_os("PACEWALL_UI_SIZE").is_none() {
            if self.sized == 0 {
                if let Some(mon) = ui.ctx().input(|i| i.viewport().monitor_size) {
                    let size = vec2((mon.x * 0.92).min(1400.0), (mon.y * 0.88).min(860.0));
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::InnerSize(size));
                }
            } else if let Some(cmd) = egui::ViewportCommand::center_on_screen(ui.ctx()) {
                ui.ctx().send_viewport_cmd(cmd);
            }
            self.sized += 1;
            ui.ctx().request_repaint();
        }
        // Pied de barre rafraîchi une fois par seconde même sur une page statique.
        ui.ctx().request_repaint_after(Duration::from_secs(1));

        // Barre latérale complète si la fenêtre est assez large, réduite à des icônes sinon.
        let wide = ui.ctx().content_rect().width() >= 1180.0;
        egui::Panel::left("nav")
            .exact_size(if wide { 216.0 } else { 64.0 })
            .resizable(false)
            .frame(Frame::NONE.fill(PANEL).stroke(Stroke::new(1.0, LINE)))
            .show(ui, |ui| self.sidebar(ui, wide));
        egui::CentralPanel::default().frame(Frame::NONE.fill(BG).inner_margin(egui::Margin::same(0))).show(ui, |ui| match self.page {
            Page::Home => self.home.show(ui, &mut self.feed),
            Page::Overlays => self.editor.show(ui, &mut self.feed, now),
            Page::Data => self.explorer.show(ui, &mut self.feed),
            Page::Memory => self.memory.show(ui),
            Page::History => self.history.show(ui),
            Page::Alerts => self.alerts.show(ui),
            Page::Settings => self.settings.show(ui),
        });
        if let Some(p) = self.take_request() {
            self.enter(p);
        }
        // Tutoriel : ouvert au premier lancement, ou à la demande depuis Réglages.
        if TUTORIAL_REQUEST.with(|r| r.replace(false)) {
            self.tutorial.restart();
        }
        if let Some(tutorial::Action::Open(page)) = self.tutorial.show(ui.ctx()) {
            self.enter(page);
        }
    }
}

// Demande de réaffichage du tutoriel, émise par les Réglages.
thread_local! {
    static TUTORIAL_REQUEST: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

pub fn request_tutorial() {
    TUTORIAL_REQUEST.with(|r| r.set(true));
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
