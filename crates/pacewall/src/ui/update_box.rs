//! Mises à jour dans la fenêtre principale : le bandeau de l'accueil (« version x.y.z disponible ») et le bloc des
//! réglages (recherche automatique, recherche à la demande). Le téléchargement et la recherche se font dans un fil
//! à part : la fenêtre ne se fige pas.

use super::kit;
use crate::theme::*;
use crate::update::{self, Release};
use eframe::egui::{RichText, Ui};
use pwcore::settings;
use serde_json::json;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::time::{Duration, Instant, SystemTime};

#[derive(Default)]
pub struct UpdateBox {
    /// Mise à jour connue (relue sur le disque toutes les 30 s : c'est le processus de fond qui cherche).
    release: Option<Release>,
    read_at: Option<Instant>,
    download: Option<Receiver<Result<PathBuf, String>>>,
    check: Option<Receiver<Result<Option<Release>, String>>>,
    message: String,
    /// La recherche faite à l'ouverture de la fenêtre a déjà été lancée.
    asked: bool,
    /// Recherche silencieuse (à l'ouverture) : pas de message « à jour » ni d'erreur si elle échoue.
    quiet: bool,
}

impl UpdateBox {
    fn refresh(&mut self, ui: &Ui) {
        if self.read_at.is_none_or(|t| t.elapsed() > Duration::from_secs(30)) {
            self.read_at = Some(Instant::now());
            self.release = update::available();
        }
        if let Some(rx) = &self.check {
            match rx.try_recv() {
                Ok(Ok(found)) => {
                    self.message = if found.is_some() || self.quiet { String::new() } else { "PaceWall est à jour.".into() };
                    self.release = found;
                    self.check = None;
                }
                Ok(Err(e)) => {
                    // Sans connexion, la recherche automatique ne dit rien : seule une recherche demandée affiche l'erreur.
                    self.message = if self.quiet { String::new() } else { format!("Recherche impossible : {e}") };
                    self.check = None;
                }
                Err(TryRecvError::Disconnected) => self.check = None,
                Err(TryRecvError::Empty) => ui.ctx().request_repaint_after(Duration::from_millis(200)),
            }
        }
        if let Some(rx) = &self.download {
            match rx.try_recv() {
                Ok(Ok(setup)) => {
                    self.download = None;
                    // L'installeur ferme cette fenêtre et le reste de PaceWall, puis relance la nouvelle version.
                    if let Err(e) = update::apply(&setup) {
                        self.message = format!("Lancement impossible : {e}");
                    }
                }
                Ok(Err(e)) => {
                    self.message = format!("Mise à jour impossible : {e}");
                    self.download = None;
                }
                Err(TryRecvError::Disconnected) => self.download = None,
                Err(TryRecvError::Empty) => ui.ctx().request_repaint_after(Duration::from_millis(200)),
            }
        }
    }

    fn install_button(&mut self, ui: &mut Ui, release: &Release) {
        if self.download.is_some() {
            ui.spinner();
            ui.label(RichText::new("Téléchargement…").color(MUTED));
        } else if kit::primary(ui, "Installer la mise à jour").on_hover_text("Télécharge la nouvelle version, ferme PaceWall, l'installe et le relance. Tes données sont conservées.").clicked() {
            let (tx, rx) = channel();
            let release = release.clone();
            std::thread::spawn(move || {
                let _ = tx.send(update::download(&release));
            });
            self.download = Some(rx);
            self.message.clear();
        }
    }

    fn search(&mut self, quiet: bool) {
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let _ = tx.send(update::check());
        });
        self.check = Some(rx);
        self.quiet = quiet;
        self.message.clear();
    }

    /// Bandeau de l'accueil : rien tant qu'il n'y a pas de mise à jour.
    pub fn banner(&mut self, ui: &mut Ui) {
        // À l'ouverture de la fenêtre, on cherche tout de suite si la dernière recherche date de plus d'une demi-heure :
        // la mise à jour est proposée dès l'accueil, sans attendre le passage du processus de fond.
        if !self.asked {
            self.asked = true;
            let recent = update::last_check().and_then(|t| SystemTime::now().duration_since(t).ok()).is_some_and(|d| d < Duration::from_secs(1800));
            if update::configured() && settings::bool_of("update_check", true) && !recent {
                self.search(true);
            }
        }
        self.refresh(ui);
        let Some(release) = self.release.clone() else { return };
        kit::panel_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("PaceWall {} est disponible", release.version)).size(15.0).strong().color(ui_accent()));
                ui.label(RichText::new(format!("(tu as la {})", update::VERSION)).size(12.5).color(MUTED));
            });
            if !release.notes.is_empty() {
                ui.label(RichText::new(&release.notes).size(12.5));
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| self.install_button(ui, &release));
            if !self.message.is_empty() {
                ui.label(RichText::new(&self.message).size(12.5).color(BAD));
            }
        });
        ui.add_space(14.0);
    }

    /// Bloc des réglages.
    pub fn settings(&mut self, ui: &mut Ui) {
        self.refresh(ui);
        ui.label(RichText::new("MISES À JOUR").size(11.0).color(MUTED));
        if !update::configured() {
            ui.label(RichText::new("Cette copie de PaceWall ne sait pas où chercher les nouvelles versions : pour mettre à jour, relance le fichier d'installation de la nouvelle version.").size(12.5).color(MUTED));
            return;
        }
        let mut auto = settings::bool_of("update_check", true);
        if ui.checkbox(&mut auto, "Chercher les nouvelles versions automatiquement").changed() {
            let _ = settings::set("update_check", json!(auto));
        }
        ui.label(RichText::new("Une requête au démarrage, puis deux par jour. Rien n'est installé sans ton accord.").size(12.5).color(MUTED));
        ui.add_space(4.0);
        let release = self.release.clone();
        ui.horizontal(|ui| {
            if let Some(release) = &release {
                ui.label(RichText::new(format!("Version {} disponible", release.version)).color(ui_accent()));
                self.install_button(ui, release);
            } else if self.check.is_some() {
                ui.spinner();
                ui.label(RichText::new("Recherche…").color(MUTED));
            } else if kit::button(ui, "Chercher maintenant").clicked() {
                self.search(false);
            }
        });
        if !self.message.is_empty() {
            ui.label(RichText::new(&self.message).size(12.5).color(MUTED));
        } else if let Some(ago) = update::last_check().and_then(|t| SystemTime::now().duration_since(t).ok()) {
            let text = match ago.as_secs() {
                s if s < 3600 => "Dernière recherche : il y a moins d'une heure.".to_string(),
                s if s < 86_400 => format!("Dernière recherche : il y a {} h.", s / 3600),
                s => format!("Dernière recherche : il y a {} j.", s / 86_400),
            };
            ui.label(RichText::new(text).size(12.5).color(MUTED));
        }
    }
}
