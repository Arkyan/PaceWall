//! Accueil : état du jeu, de l'overlay, adresse OBS et dernière session.

use super::{goto, kit, Feed, Page};
use crate::draw;
use crate::theme::*;
use crate::win;
use eframe::egui::{self, Margin, RichText, Ui};
use pwcore::{db, settings};
use serde_json::json;
use std::time::{Duration, Instant};

#[derive(Default)]
pub struct State {
    /// Largeur d'une carte, recalculée à chaque image selon la largeur disponible.
    w: f32,
    last: Option<LastSession>,
    checked: Option<Instant>,
}

struct LastSession {
    track: String,
    car: String,
    laps: i64,
    best: Option<f64>,
}

impl State {
    pub fn refresh(&mut self) {
        self.checked = Some(Instant::now());
        self.last = db::open().ok().and_then(|c| db::list_sessions(&c).ok()).and_then(|v| v.into_iter().next()).map(|s| LastSession {
            track: s["track"].as_str().unwrap_or("").into(),
            car: s["car"].as_str().unwrap_or("").into(),
            laps: s["laps"].as_i64().unwrap_or(0),
            best: s["best"].as_f64(),
        });
    }

    pub fn show(&mut self, ui: &mut Ui, feed: &mut Feed) {
        // Lecture légère du jeu : 1 fois par seconde suffit pour cette page.
        feed.poll(false);
        ui.ctx().request_repaint_after(Duration::from_secs(1));
        if self.checked.is_none_or(|t| t.elapsed() > Duration::from_secs(10)) {
            self.refresh();
        }

        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            egui::Frame::NONE.inner_margin(Margin::symmetric(30, 26)).show(ui, |ui| {
                kit::page_title(ui, "Accueil", "Overlays de course pour Le Mans Ultimate.");
                // Les cartes se répartissent sur 1 à 4 colonnes selon la largeur : jamais de débordement.
                let gap = 14.0;
                let avail = ui.available_width();
                let cols = (((avail + gap) / (300.0 + gap)).floor() as usize).clamp(1, 4);
                self.w = (avail - gap * (cols as f32 - 1.0)) / cols as f32;
                ui.spacing_mut().item_spacing = egui::vec2(gap, gap);
                let rows = 4_usize.div_ceil(cols);
                for row in 0..rows {
                    ui.horizontal_top(|ui| {
                        for i in (row * cols)..((row + 1) * cols).min(4) {
                            match i {
                                0 => self.game_card(ui, feed),
                                1 => self.overlay_card(ui),
                                2 => self.obs_card(ui),
                                _ => self.last_card(ui),
                            }
                        }
                    });
                }
            });
        });
    }

    fn card(ui: &mut Ui, w: f32, title: &str, add: impl FnOnce(&mut Ui)) {
        kit::panel_frame().show(ui, |ui| {
            ui.set_width((w - 30.0).max(200.0));
            ui.set_min_height(190.0);
            ui.vertical(|ui| {
                kit::section(ui, title);
                ui.add_space(6.0);
                add(ui);
            });
        });
    }

    fn game_card(&mut self, ui: &mut Ui, feed: &Feed) {
        Self::card(ui, self.w, "Jeu", |ui| {
            if feed.game {
                ui.label(RichText::new("Jeu connecté").font(num_font(22.0)).color(OK));
                ui.add_space(8.0);
                let d = &feed.data;
                kit::kv(ui, "Circuit", d.text("session.track"));
                kit::kv(ui, "Voiture", d.text("player.car"));
                kit::kv(ui, "Position", &d.num("position.place").map(|p| format!("P{p:.0}")).unwrap_or_else(|| "—".into()));
                kit::kv(ui, "Tour", &draw::n(d.num("lap.number"), 0));
                kit::kv(ui, "Carburant", &d.num("fuel.level").map(|v| format!("{v:.1} L")).unwrap_or_else(|| "—".into()));
            } else {
                ui.label(RichText::new("Jeu absent").font(num_font(22.0)));
                ui.add_space(8.0);
                ui.label(RichText::new("Lance Le Mans Ultimate. Dans Réglages > Gameplay, active Enable Plugins, et passe le jeu en fenêtré ou sans bordure pour voir l'overlay par-dessus.").color(MUTED));
            }
        });
    }

    fn overlay_card(&mut self, ui: &mut Ui) {
        Self::card(ui, self.w, "Overlay joueur", |ui| {
            let on = win::overlay_running();
            ui.label(RichText::new(if on { "Affichée" } else { "Fermée" }).font(num_font(22.0)).color(if on { OK } else { TEXT }));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if kit::primary(ui, if on { "Masquer" } else { "Afficher" }).clicked() {
                    win::toggle_overlay();
                }
                if kit::button(ui, "Modifier").clicked() {
                    goto(Page::Overlays);
                }
            });
            ui.add_space(6.0);
            let mut auto = settings::bool_of("overlay_auto", true);
            if ui.checkbox(&mut auto, "S'ouvre et se ferme avec le jeu").changed() {
                let _ = settings::set("overlay_auto", json!(auto));
            }
            ui.add_space(4.0);
            ui.label(RichText::new("Raccourci : Ctrl + Alt + O. Fermée, l'overlay ne consomme rien.").size(12.0).color(MUTED));
        });
    }

    fn obs_card(&mut self, ui: &mut Ui) {
        Self::card(ui, self.w, "OBS", |ui| {
            let on = settings::bool_of("obs_server", false);
            if on {
                let url = format!("http://127.0.0.1:{}/overlay/obs", settings::int_of("obs_port", 8470));
                ui.label(RichText::new("Source navigateur, 1920 × 1080 :").color(MUTED));
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&url).font(num_font(12.5)));
                });
                if kit::button(ui, "Copier l'adresse").clicked() {
                    ui.ctx().copy_text(url);
                }
                ui.add_space(4.0);
                ui.label(RichText::new("Un seul widget par source : ajoute ?only=fuel à l'adresse.").size(12.0).color(MUTED));
            } else {
                ui.label(RichText::new("Le serveur OBS est désactivé : il ne consomme rien.").color(MUTED));
                ui.add_space(6.0);
                if kit::button(ui, "Activer").clicked() {
                    let _ = settings::set("obs_server", json!(true));
                }
            }
        });
    }

    fn last_card(&mut self, ui: &mut Ui) {
        Self::card(ui, self.w, "Dernière session", |ui| {
            match &self.last {
                Some(s) => {
                    kit::kv(ui, "Circuit", &s.track);
                    kit::kv(ui, "Voiture", &s.car);
                    kit::kv(ui, "Tours", &s.laps.to_string());
                    kit::kv(ui, "Meilleur", &draw::lap(s.best));
                }
                None => {
                    ui.label(RichText::new("Aucune session enregistrée pour l'instant.").color(MUTED));
                }
            }
            ui.add_space(8.0);
            if kit::button(ui, "Ouvrir l'historique").clicked() {
                goto(Page::History);
            }
        });
    }
}
