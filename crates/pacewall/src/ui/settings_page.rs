//! Réglages : overlay, unités, seuils de couleur, apparence, raccourcis, serveur OBS, lancement avec Windows, fichiers.

use super::kit;
use crate::theme::*;
use crate::hotkeys;
use crate::install;
use crate::widgets;
use crate::win;
use eframe::egui::{self, vec2, Margin, RichText, Sense, Stroke, StrokeKind, Ui};
use pwcore::{db, layout, settings};
use serde_json::json;

pub struct State {
    monitors: Vec<win::Monitor>,
    /// Installation : raccourci du bureau, suppression des données à la désinstallation, confirmation, erreur éventuelle.
    desktop_link: bool,
    purge: bool,
    confirm_uninstall: bool,
    install_error: Option<String>,
    /// Raccourci en cours de saisie (clé du réglage), message d'aide, et suivi des boutons de volant.
    capturing: Option<&'static str>,
    capture_hint: String,
    pads: hotkeys::Pads,
}

impl Default for State {
    fn default() -> Self {
        Self { monitors: Vec::new(), desktop_link: true, purge: false, confirm_uninstall: false, install_error: None, capturing: None, capture_hint: String::new(), pads: hotkeys::Pads::default() }
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
                self.units(ui);
                ui.add_space(14.0);
                self.thresholds(ui);
                ui.add_space(14.0);
                self.appearance(ui);
                ui.add_space(14.0);
                self.shortcuts(ui);
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
            ui.label(RichText::new("Le jeu doit être en fenêtré ou sans bordure. Le raccourci pour l'afficher ou la fermer se règle plus bas.").size(12.5).color(MUTED));
        });
    }

    /// Une ligne « libellé + liste de choix » enregistrée dans les réglages.
    fn choice(ui: &mut Ui, label: &str, key: &str, default: &str, choices: &[(&str, &str)]) {
        let current = settings::str_of(key, default);
        let shown = choices.iter().find(|c| c.0 == current).map_or("", |c| c.1);
        ui.horizontal(|ui| {
            ui.allocate_ui_with_layout(vec2(190.0, 20.0), egui::Layout::left_to_right(egui::Align::Center), |ui| ui.label(label));
            egui::ComboBox::from_id_salt(key).width(210.0).selected_text(shown).show_ui(ui, |ui| {
                for (value, name) in choices {
                    if ui.selectable_label(current == *value, *name).clicked() {
                        let _ = settings::set(key, json!(value));
                    }
                }
            });
        });
    }

    fn units(&mut self, ui: &mut Ui) {
        kit::panel_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            kit::section(ui, "Unités");
            ui.add_space(6.0);
            Self::choice(ui, "Vitesse", "unit_speed", "kmh", &[("kmh", "km/h"), ("mph", "mph")]);
            Self::choice(ui, "Température", "unit_temp", "c", &[("c", "°C"), ("f", "°F")]);
            Self::choice(ui, "Pression des pneus", "unit_pressure", "kpa", &[("kpa", "kPa"), ("bar", "bar"), ("psi", "psi")]);
            Self::choice(ui, "Carburant", "unit_fuel", "l", &[("l", "Litres"), ("gal", "Gallons (US)")]);
            ui.label(RichText::new("Valables pour tous les widgets de l'overlay du joueur. Un widget réglé sur une unité précise la garde.").size(12.5).color(MUTED));
        });
    }

    fn thresholds(&mut self, ui: &mut Ui) {
        kit::panel_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            kit::section(ui, "Seuils de couleur");
            ui.add_space(6.0);
            let row = |ui: &mut Ui, label: &str, key: &str, default: i64, range: std::ops::RangeInclusive<i64>, unit: &str| {
                let mut v = settings::int_of(key, default);
                ui.horizontal(|ui| {
                    ui.allocate_ui_with_layout(vec2(330.0, 20.0), egui::Layout::left_to_right(egui::Align::Center), |ui| ui.label(label));
                    if ui.add(egui::DragValue::new(&mut v).range(range).suffix(unit)).changed() {
                        let _ = settings::set(key, json!(v));
                    }
                });
            };
            row(ui, "Carburant et énergie : orange en dessous de", "thr_fuel_warn", 6, 1..=30, " tours");
            row(ui, "Carburant et énergie : rouge en dessous de", "thr_fuel_crit", 3, 0..=30, " tours");
            row(ui, "État des pneus : orange en dessous de", "thr_wear_warn", 55, 0..=100, " %");
            row(ui, "État des pneus : rouge en dessous de", "thr_wear_crit", 30, 0..=100, " %");
            row(ui, "Pneu tout bleu ou tout rouge à partir d'un écart de", "thr_temp_window", 25, 5..=60, " °C");
            ui.label(RichText::new("L'écart de température se mesure par rapport à la température idéale donnée par le jeu.").size(12.5).color(MUTED));
        });
    }

    fn appearance(&mut self, ui: &mut Ui) {
        kit::panel_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            kit::section(ui, "Apparence");
            ui.add_space(6.0);
            ui.label("Couleur d'accent de la fenêtre PaceWall");
            let current = settings::str_of("ui_accent", "cyan");
            ui.horizontal(|ui| {
                for (id, name, color) in widgets::ACCENTS {
                    let (rect, resp) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::click());
                    ui.painter().rect_filled(rect.shrink(3.0), 0.0, *color);
                    if current == *id {
                        ui.painter().rect_stroke(rect, 0.0, Stroke::new(2.0, TEXT), StrokeKind::Inside);
                    }
                    if resp.on_hover_text(*name).clicked() {
                        let _ = settings::set("ui_accent", json!(id));
                        crate::theme::apply_visuals(ui.ctx());
                    }
                }
            });
            ui.label(RichText::new("La couleur de chaque widget de l'overlay se règle dans l'éditeur.").size(12.5).color(MUTED));
        });
    }

    /// Saisie d'un raccourci : la prochaine touche (avec Ctrl, Alt ou Maj) ou le prochain bouton de volant.
    fn capture(&mut self, ui: &mut Ui) {
        let Some(key) = self.capturing else { return };
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
        if let Some((pad, button)) = self.pads.pressed(0..16).first().copied() {
            let _ = settings::set(key, json!(hotkeys::joy_text(pad, button)));
            self.capturing = None;
            return;
        }
        let pressed = ui.input(|i| {
            i.events.iter().find_map(|ev| match ev {
                egui::Event::Key { key, pressed: true, repeat: false, modifiers, .. } => Some((*key, *modifiers)),
                _ => None,
            })
        });
        let Some((k, mods)) = pressed else { return };
        if k == egui::Key::Escape {
            self.capturing = None;
            return;
        }
        let name = k.name();
        let function = name.len() >= 2 && name.starts_with('F') && name[1..].chars().all(|c| c.is_ascii_digit());
        let simple = name.len() == 1 && name.chars().all(|c| c.is_ascii_alphanumeric());
        if !function && !simple {
            self.capture_hint = "Touche non prise en charge : lettre, chiffre ou F1 à F12.".into();
            return;
        }
        if simple && !(mods.ctrl || mods.alt) {
            // Une lettre seule serait interceptée partout dans Windows, y compris en tapant du texte.
            self.capture_hint = "Ajoute Ctrl ou Alt à cette touche (ou choisis une touche F1 à F12).".into();
            return;
        }
        let mut text = String::new();
        for (on, label) in [(mods.ctrl, "Ctrl+"), (mods.alt, "Alt+"), (mods.shift, "Maj+")] {
            if on {
                text.push_str(label);
            }
        }
        text.push_str(name);
        let _ = settings::set(key, json!(text));
        self.capturing = None;
    }

    fn shortcuts(&mut self, ui: &mut Ui) {
        self.capture(ui);
        kit::panel_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            kit::section(ui, "Raccourcis");
            ui.add_space(6.0);
            ui.label(RichText::new("Une touche du clavier ou un bouton de volant. Les raccourcis des widgets les masquent ou les réaffichent en roulant.").size(12.5).color(MUTED));
            ui.add_space(4.0);
            let mut rows: Vec<(&'static str, String)> = vec![("hk_overlay", "Afficher / fermer l'overlay".into()), ("hk_voice", "Couper / remettre la voix".into())];
            rows.extend(hotkeys::WIDGET_ACTIONS.iter().map(|(k, name)| (*k, format!("Masquer / afficher : {name}"))));
            egui::Grid::new("shortcuts").num_columns(4).spacing([14.0, 6.0]).show(ui, |ui| {
                for (key, label) in rows {
                    let current = settings::str_of(key, "");
                    ui.label(label);
                    if self.capturing == Some(key) {
                        ui.label(RichText::new("Appuie sur la touche ou le bouton…").color(WARN));
                        if kit::button(ui, "Annuler").clicked() {
                            self.capturing = None;
                        }
                        ui.label("");
                    } else {
                        // Un texte incompris (fichier modifié à la main) est signalé plutôt qu'ignoré en silence.
                        let valid = current.is_empty() || hotkeys::parse(&current).is_some();
                        ui.label(RichText::new(if current.is_empty() { "aucun" } else { current.as_str() }).font(num_font(12.5)).color(if !valid { BAD } else if current.is_empty() { MUTED } else { TEXT }));
                        if kit::button(ui, "Changer").clicked() {
                            self.capturing = Some(key);
                            self.capture_hint.clear();
                            // Les boutons déjà enfoncés ne comptent pas comme une nouvelle pression.
                            let _ = self.pads.pressed(0..16);
                        }
                        if !current.is_empty() && kit::button(ui, "Effacer").clicked() {
                            let _ = settings::set(key, json!(""));
                        } else if current.is_empty() {
                            ui.label("");
                        }
                    }
                    ui.end_row();
                }
            });
            if self.capturing.is_some() && !self.capture_hint.is_empty() {
                ui.label(RichText::new(&self.capture_hint).color(WARN));
            }
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
