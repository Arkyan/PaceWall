//! Onglet Alertes : quoi dire, dans quelle situation, avec quelle voix ; chaque alerte s'active, se désactive, se réécoute
//! et peut avoir sa phrase. Aussi : la simulation, pour tout essayer sans le jeu.

use super::kit;
use crate::speech::{Engine, Speaker};
use crate::theme::*;
use crate::win;
use eframe::egui::{self, Margin, RichText, Ui};
use pwcore::settings;
use pwcore::voice_cfg::{self, AlertCfg, AlertDef, AlertSet, CATALOG, GROUPS};
use serde_json::json;

pub struct State {
    set: AlertSet,
    loaded: bool,
    /// Simulation choisie : 0 scénario d'alertes, 1 course, 2 qualifications, 3 essais libres ; accélération et tours.
    sim_kind: usize,
    sim_speed: f32,
    sim_laps: f32,
}

impl Default for State {
    fn default() -> Self {
        Self { set: AlertSet::new(), loaded: false, sim_kind: 1, sim_speed: 6.0, sim_laps: 18.0 }
    }
}

fn engine() -> Engine {
    Engine::from_setting(&settings::str_of("voice_engine", "engineer_m"))
}

fn volume() -> u32 {
    settings::int_of("voice_volume", 80).clamp(0, 100) as u32
}

impl State {
    pub fn refresh(&mut self) {
        self.set = voice_cfg::load();
        self.loaded = true;
    }

    fn save(&self) {
        let _ = voice_cfg::save(&self.set);
    }

    pub fn show(&mut self, ui: &mut Ui) {
        if !self.loaded {
            self.refresh();
        }
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            egui::Frame::NONE.inner_margin(Margin::symmetric(30, 26)).show(ui, |ui| {
                kit::page_title(ui, "Alertes vocales", "Ce que ton ingénieur te dit, et dans quelles situations.");
                ui.set_max_width(780.0);
                self.general(ui);
                for group in GROUPS {
                    ui.add_space(14.0);
                    self.group(ui, group);
                }
                ui.add_space(14.0);
                self.simulation(ui);
            });
        });
    }

    /// Activation générale, voix, volume et essai.
    fn general(&mut self, ui: &mut Ui) {
        kit::panel_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            kit::section(ui, "Général");
            ui.add_space(6.0);
            let mut on = settings::bool_of("voice_enabled", true);
            if ui.checkbox(&mut on, "Activer les alertes vocales (pendant que tu roules)").changed() {
                let _ = settings::set("voice_enabled", json!(on));
            }
            ui.add_enabled_ui(on, |ui| {
                ui.add_space(6.0);
                ui.label(RichText::new("Voix").size(12.0).color(MUTED));
                let current = settings::str_of("voice_engine", "engineer_m");
                let name = |v: &str| match v {
                    "windows" => "Voix de synthèse Windows",
                    "engineer_f" => "Ingénieure de course (voix de femme)",
                    _ => "Ingénieur de course (voix d'homme)",
                };
                egui::ComboBox::from_id_salt("voice_engine").selected_text(name(&current)).width(320.0).show_ui(ui, |ui| {
                    for v in ["engineer_m", "engineer_f", "windows"] {
                        if ui.selectable_label(current == v || (v == "engineer_m" && current == "engineer"), name(v)).clicked() {
                            let _ = settings::set("voice_engine", json!(v));
                        }
                    }
                });
                ui.add_space(6.0);
                let mut vol = volume() as f32;
                ui.label(RichText::new("Volume de la voix").size(12.0).color(MUTED));
                ui.spacing_mut().slider_width = (ui.available_width() - 200.0).clamp(100.0, 320.0);
                if ui.add(egui::Slider::new(&mut vol, 0.0..=100.0).step_by(5.0).suffix(" %")).changed() {
                    let _ = settings::set("voice_volume", json!(vol.round() as i64));
                }
                ui.add_space(6.0);
                if kit::button(ui, "Tester la voix").clicked() {
                    Speaker::global().say("test", "Radio check. Alertes vocales activées.", volume(), engine(), None);
                }
            });
            ui.add_space(4.0);
            ui.label(RichText::new("La voix d'ingénieur est enregistrée dans PaceWall (hors ligne, sans coût pendant la course). La voix Windows sert de secours et de voix pour les phrases personnalisées : installe une voix française dans Paramètres > Heure et langue > Voix.").size(11.5).color(MUTED));
        });
    }

    fn group(&mut self, ui: &mut Ui, group: &str) {
        kit::panel_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            kit::section(ui, group);
            for def in CATALOG.iter().filter(|a| a.group == group) {
                ui.add_space(8.0);
                self.row(ui, def);
            }
        });
    }

    /// Une alerte : case, choix de la phrase, écoute, phrase personnalisée.
    fn row(&mut self, ui: &mut Ui, def: &AlertDef) {
        let mut cfg = self.set.get(def.id).cloned().unwrap_or_default();
        let before = cfg.clone();
        ui.checkbox(&mut cfg.on, RichText::new(def.label).size(14.0));
        ui.add_enabled_ui(cfg.on, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.add_space(24.0);
                let shown = match cfg.variant.as_str() {
                    "0" => "Phrase 1".to_string(),
                    "1" => "Phrase 2".to_string(),
                    _ => "Alterner les phrases".to_string(),
                };
                ui.add_enabled_ui(cfg.text.trim().is_empty(), |ui| {
                    egui::ComboBox::from_id_salt(("variant", def.id)).selected_text(shown).width(170.0).show_ui(ui, |ui| {
                        ui.selectable_value(&mut cfg.variant, "auto".to_string(), "Alterner les phrases");
                        ui.selectable_value(&mut cfg.variant, "0".to_string(), format!("1 · {}", def.phrases[0]));
                        ui.selectable_value(&mut cfg.variant, "1".to_string(), format!("2 · {}", def.phrases[1]));
                    });
                });
                if kit::button(ui, "Écouter").clicked() {
                    self.listen(def, &cfg);
                }
                // Le champ de phrase personnalisée n'est montré qu'à la demande (ou s'il y en a déjà une).
                let open_id = egui::Id::new(("custom_open", def.id));
                let mut open = ui.data_mut(|d| d.get_temp::<bool>(open_id)).unwrap_or(!cfg.text.trim().is_empty());
                let label = if open { "Masquer ma phrase" } else { "Ma phrase…" };
                if kit::button(ui, label).clicked() {
                    open = !open;
                    ui.data_mut(|d| d.insert_temp(open_id, open));
                }
                if cfg.text.trim().is_empty() {
                    ui.label(RichText::new(def.phrases[cfg.fixed_variant().unwrap_or(0).min(1)]).size(11.5).color(MUTED));
                }
            });
            let open = ui.data_mut(|d| d.get_temp::<bool>(egui::Id::new(("custom_open", def.id)))).unwrap_or(!cfg.text.trim().is_empty());
            if open {
                ui.horizontal(|ui| {
                    ui.add_space(24.0);
                    let width = (ui.available_width() - 8.0).clamp(160.0, 560.0);
                    ui.add(egui::TextEdit::singleline(&mut cfg.text).hint_text("Ta phrase à toi (lue par la voix Windows)…").desired_width(width));
                });
            }
        });
        if cfg != before {
            if cfg == AlertCfg::default() {
                self.set.remove(def.id);
            } else {
                self.set.insert(def.id.to_string(), cfg);
            }
            self.save();
        }
    }

    /// Joue l'alerte telle qu'elle sera dite : clip choisi, ou phrase personnalisée à la voix Windows.
    fn listen(&self, def: &AlertDef, cfg: &AlertCfg) {
        let text = cfg.text.trim();
        if !text.is_empty() {
            Speaker::global().say("", text, volume(), Engine::Windows, None);
        } else {
            let fallback = def.phrases[cfg.fixed_variant().unwrap_or(0).min(1)];
            Speaker::global().say(def.clips[0], fallback, volume(), engine(), cfg.fixed_variant());
        }
    }

    /// Simulation : un faux jeu intégré qui déroule toutes les alertes, pour essayer PaceWall sans Le Mans Ultimate.
    fn simulation(&mut self, ui: &mut Ui) {
        kit::panel_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            kit::section(ui, "Simulation (sans le jeu)");
            ui.add_space(6.0);
            let simulating = pwcore::live::simulation_running();
            let game = !simulating && pwcore::live::game_running();
            ui.label(RichText::new("Lance un faux Le Mans Ultimate pour tout essayer sans le jeu. Le scénario d'alertes déclenche chaque alerte en 90 s ; la course, les qualifs et les essais simulent une vraie session : 32 voitures sur trois classes, tours chronométrés, stands, jaunes locaux, full course yellow, drapeaux bleus et pluie.").size(12.0).color(MUTED));
            ui.add_space(6.0);
            ui.add_enabled_ui(!simulating && !game, |ui| {
                ui.label(RichText::new("Type de simulation").size(12.0).color(MUTED));
                let kinds = ["Scénario d'alertes (90 s, déclenche toutes les alertes)", "Course réaliste (32 voitures, stands, jaunes, full course yellow, pluie)", "Qualifications (tours lancés, classement par meilleur tour)", "Essais libres"];
                egui::ComboBox::from_id_salt("sim_kind").selected_text(kinds[self.sim_kind]).width(520.0).show_ui(ui, |ui| {
                    for (i, k) in kinds.iter().enumerate() {
                        ui.selectable_value(&mut self.sim_kind, i, *k);
                    }
                });
                if self.sim_kind > 0 {
                    ui.add_space(4.0);
                    ui.spacing_mut().slider_width = 220.0;
                    ui.add(egui::Slider::new(&mut self.sim_speed, 1.0..=8.0).step_by(1.0).text("Accélération du temps").suffix(" ×"));
                    if self.sim_kind == 1 {
                        ui.add(egui::Slider::new(&mut self.sim_laps, 5.0..=40.0).step_by(1.0).text("Tours de course"));
                    }
                }
            });
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
                    let session = ["", "race", "quali", "practice"][self.sim_kind];
                    if session.is_empty() {
                        win::spawn_self(&["--simulate"]);
                    } else {
                        let (speed, laps) = (format!("{}", self.sim_speed.round()), format!("{}", self.sim_laps.round()));
                        win::spawn_self(&["--simulate", "--session", session, "--speed", &speed, "--laps", &laps]);
                    }
                }
            });
            // Raison d'un échec de lancement (écrite par la simulation) : visible dix secondes.
            if !simulating {
                let log = std::env::temp_dir().join("pw-simulation.log");
                let recent = std::fs::metadata(&log).and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|d| d.as_secs() < 10);
                if recent {
                    if let Ok(text) = std::fs::read_to_string(&log) {
                        ui.add_space(4.0);
                        ui.label(RichText::new(format!("La simulation ne s'est pas lancée : {text}")).color(BAD));
                    }
                }
            }
            ui.add_space(4.0);
            ui.label(RichText::new("Pour que l'overlay s'ouvre toute seule, PaceWall doit tourner en arrière-plan (icône près de l'horloge).").size(11.5).color(MUTED));
        });
        // Le statut se met à jour tout seul pendant que la page est ouverte.
        ui.ctx().request_repaint_after(std::time::Duration::from_secs(1));
    }
}
