//! Réglages : overlay, serveur OBS, lancement avec Windows, emplacements des fichiers.

use super::kit;
use crate::theme::*;
use crate::win;
use eframe::egui::{self, Margin, RichText, Ui};
use pwcore::{db, layout, settings};
use serde_json::json;

#[derive(Default)]
pub struct State {
    monitors: Vec<win::Monitor>,
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
                self.voice(ui);
                ui.add_space(14.0);
                self.simulation(ui);
                ui.add_space(14.0);
                self.obs(ui);
                ui.add_space(14.0);
                self.app(ui);
                ui.add_space(14.0);
                self.files(ui);
            });
        });
    }

    /// Alertes vocales : quoi dire, à quel volume, et un bouton pour entendre la voix.
    fn voice(&mut self, ui: &mut Ui) {
        kit::panel_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            kit::section(ui, "Alertes vocales");
            ui.add_space(6.0);
            let mut on = settings::bool_of("voice_enabled", true);
            if ui.checkbox(&mut on, "Activer les alertes vocales (pendant que tu roules)").changed() {
                let _ = settings::set("voice_enabled", json!(on));
            }
            ui.add_enabled_ui(on, |ui| {
                for (key, label) in [
                    ("voice_fuel", "Carburant et énergie : 5, 3, 2 puis 1 tour restant"),
                    ("voice_tyres", "Pneus : froids, trop chauds, usés, crevaison"),
                    ("voice_flags", "Drapeau jaune, full course jaune et reprise"),
                    ("voice_weather", "Pluie détectée"),
                ] {
                    let mut v = settings::bool_of(key, true);
                    if ui.checkbox(&mut v, label).changed() {
                        let _ = settings::set(key, json!(v));
                    }
                }
                ui.add_space(6.0);
                ui.label(RichText::new("Voix").size(12.0).color(MUTED));
                let engine = settings::str_of("voice_engine", "engineer");
                let shown = if engine == "windows" { "Voix de synthèse Windows" } else { "Ingénieur de course (voix naturelle)" };
                egui::ComboBox::from_id_salt("voice_engine").selected_text(shown).width(300.0).show_ui(ui, |ui| {
                    if ui.selectable_label(engine != "windows", "Ingénieur de course (voix naturelle)").clicked() {
                        let _ = settings::set("voice_engine", json!("engineer"));
                    }
                    if ui.selectable_label(engine == "windows", "Voix de synthèse Windows").clicked() {
                        let _ = settings::set("voice_engine", json!("windows"));
                    }
                });
                ui.add_space(6.0);
                let mut volume = settings::int_of("voice_volume", 80).clamp(0, 100) as f32;
                ui.label(RichText::new("Volume de la voix").size(12.0).color(MUTED));
                ui.spacing_mut().slider_width = (ui.available_width() - 200.0).clamp(100.0, 320.0);
                if ui.add(egui::Slider::new(&mut volume, 0.0..=100.0).step_by(5.0).suffix(" %")).changed() {
                    let _ = settings::set("voice_volume", json!(volume.round() as i64));
                }
                ui.add_space(6.0);
                if kit::button(ui, "Tester la voix").clicked() {
                    crate::speech::Speaker::global().say("test", "Radio check. Alertes vocales activées.", volume.round() as u32, settings::str_of("voice_engine", "engineer") != "windows");
                }
            });
            ui.add_space(4.0);
            ui.label(RichText::new("La voix d'ingénieur est enregistrée dans PaceWall (hors ligne, sans coût pendant la course). La voix Windows sert de secours : installe une voix française dans Paramètres > Heure et langue > Voix.").size(11.5).color(MUTED));
        });
    }

    /// Simulation : un faux jeu intégré qui déroule toutes les alertes, pour essayer PaceWall sans Le Mans Ultimate.
    fn simulation(&mut self, ui: &mut Ui) {
        kit::panel_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            kit::section(ui, "Simulation (sans le jeu)");
            ui.add_space(6.0);
            let simulating = pwcore::live::simulation_running();
            let game = !simulating && pwcore::live::game_running();
            ui.label(RichText::new("Lance un faux Le Mans Ultimate : les overlays, la carte et les alertes vocales se déclenchent comme en course (un cycle de 90 s qui se répète : pneus froids puis chauds puis usés, carburant, drapeau jaune, full course jaune, pluie, crevaison).").size(12.0).color(MUTED));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if simulating {
                    if kit::button(ui, "Arrêter la simulation").clicked() {
                        win::stop_simulator();
                    }
                    ui.label(RichText::new("Simulation en cours").color(OK));
                } else if game {
                    ui.add_enabled(false, egui::Button::new("Lancer la simulation"));
                    ui.label(RichText::new("Le jeu tourne : simulation impossible").color(MUTED));
                } else if kit::button(ui, "Lancer la simulation").clicked() {
                    win::spawn_self(&["--simulate"]);
                }
            });
            ui.add_space(4.0);
            ui.label(RichText::new("Pour que l'overlay s'ouvre toute seule, PaceWall doit tourner en arrière-plan (icône près de l'horloge).").size(11.5).color(MUTED));
        });
        // Le statut se met à jour tout seul pendant que la page est ouverte.
        ui.ctx().request_repaint_after(std::time::Duration::from_secs(1));
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
            ui.label(RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION"))).size(12.5).color(MUTED));
        });
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
