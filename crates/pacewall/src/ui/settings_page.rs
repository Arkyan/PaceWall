//! Réglages : overlay, serveur OBS, lancement avec Windows, emplacements des fichiers.

use super::kit;
use crate::theme::*;
use crate::install;
use crate::win;
use eframe::egui::{self, Margin, RichText, Ui};
use pwcore::{db, layout, settings};
use serde_json::json;

pub struct State {
    monitors: Vec<win::Monitor>,
    /// Installation : raccourci du bureau, suppression des données à la désinstallation, confirmation, erreur éventuelle.
    desktop_link: bool,
    purge: bool,
    confirm_uninstall: bool,
    install_error: Option<String>,
}

impl Default for State {
    fn default() -> Self {
        Self { monitors: Vec::new(), desktop_link: true, purge: false, confirm_uninstall: false, install_error: None }
    }
}

impl State {
    pub fn refresh(&mut self) {
        self.monitors = win::monitors();
    }

    pub fn show(&mut self, ui: &mut Ui) {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            egui::Frame::NONE.inner_margin(Margin::symmetric(30, 26)).show(ui, |ui| {
                kit::page_title(ui, "Réglages", "Les changements sont enregistrés tout de suite.");
                ui.set_max_width(760.0);
                self.overlay(ui);
                ui.add_space(14.0);
                self.obs(ui);
                ui.add_space(14.0);
                self.app(ui);
                ui.add_space(14.0);
                self.files(ui);
            });
        });
    }

    fn overlay(&mut self, ui: &mut Ui) {
        kit::panel_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            kit::section(ui, "Overlay joueur");
            ui.add_space(6.0);
            let mut auto = settings::bool_of("overlay_auto", true);
            if ui.checkbox(&mut auto, "S'ouvre automatiquement avec le jeu et se ferme avec lui").changed() {
                let _ = settings::set("overlay_auto", json!(auto));
            }
            let mut fg = settings::bool_of("overlay_fg_only", true);
            if ui.checkbox(&mut fg, "Masquée quand le jeu n'est pas au premier plan").changed() {
                let _ = settings::set("overlay_fg_only", json!(fg));
            }
            let mut idle = settings::bool_of("overlay_hide_idle", true);
            if ui.checkbox(&mut idle, "Masquée en pause, dans les menus et hors piste").changed() {
                let _ = settings::set("overlay_hide_idle", json!(idle));
            }
            ui.add_space(8.0);
            if self.monitors.is_empty() {
                self.monitors = win::monitors();
            }
            let current = settings::int_of("overlay_monitor", 0).max(0) as usize;
            let label = |i: usize, m: &win::Monitor| format!("Écran {}{} · {}×{}", i + 1, if m.primary { " (principal)" } else { "" }, m.w, m.h);
            let text = self.monitors.get(current).map(|m| label(current, m)).unwrap_or_else(|| "Écran 1".into());
            ui.horizontal(|ui| {
                ui.label("Écran de l'overlay");
                egui::ComboBox::from_id_salt("monitor").selected_text(text).show_ui(ui, |ui| {
                    for (i, m) in self.monitors.iter().enumerate() {
                        if ui.selectable_label(i == current, label(i, m)).clicked() {
                            let _ = settings::set("overlay_monitor", json!(i));
                        }
                    }
                });
            });
            ui.add_space(6.0);
            ui.label(RichText::new("Raccourci pour l'afficher ou la masquer : Ctrl + Alt + O. Le jeu doit être en fenêtré ou sans bordure.").size(12.5).color(MUTED));
        });
    }

    fn obs(&mut self, ui: &mut Ui) {
        kit::panel_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            kit::section(ui, "OBS");
            ui.add_space(6.0);
            let mut on = settings::bool_of("obs_server", false);
            if ui.checkbox(&mut on, "Serveur web pour OBS (source navigateur)").changed() {
                let _ = settings::set("obs_server", json!(on));
            }
            ui.label(RichText::new("Désactivé, il ne consomme rien. Activé, il ne lit le jeu que si une source OBS est ouverte.").size(12.5).color(MUTED));
            ui.add_space(8.0);
            let mut port = settings::int_of("obs_port", 8470);
            ui.horizontal(|ui| {
                ui.label("Port");
                if ui.add(egui::DragValue::new(&mut port).range(1024..=65535)).changed() {
                    let _ = settings::set("obs_port", json!(port));
                }
                ui.label(RichText::new("(redémarrer le serveur pour l'appliquer)").size(12.0).color(MUTED));
            });
            if on {
                let url = format!("http://127.0.0.1:{port}/overlay/obs");
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&url).font(num_font(12.5)));
                    if kit::button(ui, "Copier").clicked() {
                        ui.ctx().copy_text(url);
                    }
                });
            }
        });
    }

    fn app(&mut self, ui: &mut Ui) {
        kit::panel_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            kit::section(ui, "Application");
            ui.add_space(6.0);
            let mut auto = settings::bool_of("autostart", false);
            if ui.checkbox(&mut auto, "Lancer PaceWall avec Windows (dans la zone de notification)").changed() {
                let _ = settings::set("autostart", json!(auto));
                win::set_autostart(auto);
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if kit::button(ui, "Revoir le tutoriel").clicked() {
                    super::request_tutorial();
                }
                ui.label(RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION"))).size(12.5).color(MUTED));
            });
            ui.add_space(10.0);
            self.installation(ui);
        });
    }

    /// Installer ou désinstaller PaceWall (raccourcis, démarrage avec Windows, entrée « Applications installées »).
    fn installation(&mut self, ui: &mut Ui) {
        ui.label(RichText::new("INSTALLATION").size(11.0).color(MUTED));
        if install::running_installed() {
            ui.label(RichText::new(format!("Installé dans {}", install::install_dir().display())).font(num_font(12.0)));
            ui.add_space(4.0);
            ui.checkbox(&mut self.purge, "Supprimer aussi mes données (historique, mises en page, réglages)");
            if self.confirm_uninstall {
                ui.label(RichText::new("PaceWall va se fermer et se désinstaller.").color(WARN));
                ui.horizontal(|ui| {
                    if kit::button(ui, "Confirmer la désinstallation").clicked() {
                        install::uninstall(self.purge);
                        std::process::exit(0);
                    }
                    if kit::button(ui, "Annuler").clicked() {
                        self.confirm_uninstall = false;
                    }
                });
            } else if kit::button(ui, "Désinstaller PaceWall").clicked() {
                self.confirm_uninstall = true;
            }
        } else {
            ui.label(RichText::new("Cette copie de PaceWall n'est pas installée : elle tourne depuis son dossier actuel.").size(12.5).color(MUTED));
            ui.add_space(4.0);
            ui.checkbox(&mut self.desktop_link, "Ajouter un raccourci sur le bureau");
            let mut autostart = settings::bool_of("autostart", false);
            if ui.checkbox(&mut autostart, "Lancer avec Windows après l'installation").changed() {
                let _ = settings::set("autostart", json!(autostart));
            }
            ui.add_space(4.0);
            if kit::button(ui, "Installer PaceWall").clicked() {
                match install::install(install::Options { desktop_shortcut: self.desktop_link, autostart }) {
                    Ok(exe) => {
                        // La copie installée prend le relais : elle ouvre PaceWall, cette copie se ferme.
                        let _ = std::process::Command::new(exe).spawn();
                        std::process::exit(0);
                    }
                    Err(e) => self.install_error = Some(e),
                }
            }
            if let Some(e) = &self.install_error {
                ui.label(RichText::new(format!("Installation impossible : {e}")).color(BAD));
            }
        }
    }

    fn files(&mut self, ui: &mut Ui) {
        kit::panel_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            kit::section(ui, "Fichiers");
            ui.add_space(6.0);
            let dir = db::path().parent().map(|p| p.to_path_buf()).unwrap_or_default();
            for (label, path) in [("Historique", db::path()), ("Mises en page", layout::path()), ("Réglages", settings::path())] {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(label.to_uppercase()).size(11.0).color(MUTED));
                    ui.label(RichText::new(path.display().to_string()).font(num_font(12.0)));
                });
            }
            ui.add_space(6.0);
            if kit::button(ui, "Ouvrir le dossier").clicked() {
                let _ = std::fs::create_dir_all(&dir);
                let _ = std::process::Command::new("explorer.exe").arg(&dir).spawn();
            }
        });
    }
}
