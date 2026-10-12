//! Premier lancement : sept étapes pour préparer le jeu, choisir ses widgets et ses unités, régler les alertes vocales,
//! brancher OBS et, si PaceWall n'est pas encore installé, l'installer. Une installation neuve n'affiche aucun widget :
//! c'est ici que l'utilisateur compose son écran.
//! Revoir le tutoriel : Réglages > Application.

use super::{kit, Page};
use crate::install;
use crate::theme::*;
use eframe::egui::{self, Align, Layout, RichText, Ui};
use pwcore::{layout, settings};
use serde_json::json;

/// Ce que le tutoriel demande à la fenêtre.
pub enum Action {
    /// Ouvrir une page (le tutoriel reste ouvert au-dessus).
    Open(Page),
    /// La mise en page vient d'être écrite : l'éditeur doit la relire.
    LayoutChanged,
}

pub struct Tutorial {
    pub open: bool,
    step: usize,
    desktop: bool,
    autostart: bool,
    message: Option<String>,
    /// Widgets cochés à l'étape « Choisis tes widgets » (même ordre que `widgets::KINDS`).
    picked: Vec<bool>,
    /// Reprendre les mêmes widgets pour OBS.
    obs_too: bool,
    /// L'utilisateur a déjà une mise en page : on ne la remplace que s'il le demande.
    replace: bool,
    /// Cette session du tutoriel a écrit la mise en page (elle peut donc la réécrire si on revient en arrière).
    wrote: bool,
}

const STEPS: usize = 7;
/// Les widgets cochés d'office pour une première installation.
const ESSENTIAL: [&str; 6] = ["standings", "timing", "fuel", "tyres", "session", "flags"];

fn has_layout() -> bool {
    layout::load().targets.get("player").is_some_and(|l| !l.widgets.is_empty())
}

/// Cases cochées au départ : les widgets déjà en place, ou la sélection « Essentiel ».
fn initial_pick() -> Vec<bool> {
    let current = layout::load().targets.remove("player").unwrap_or_default();
    crate::widgets::KINDS.iter().map(|(kind, _)| if current.widgets.is_empty() { ESSENTIAL.contains(kind) } else { current.widgets.iter().any(|w| w.kind == *kind) }).collect()
}

impl Tutorial {
    /// Ouvert au premier lancement (tant que « tutorial_done » n'est pas enregistré).
    pub fn new() -> Self {
        // PACEWALL_TUTORIAL_STEP=2 : ouvre directement une étape (essais de mise en page).
        let step = std::env::var("PACEWALL_TUTORIAL_STEP").ok().and_then(|v| v.parse::<usize>().ok()).map_or(0, |s| s.min(STEPS - 1));
        Self { open: !settings::bool_of("tutorial_done", false), step, desktop: true, autostart: false, message: None, picked: initial_pick(), obs_too: true, replace: false, wrote: false }
    }

    pub fn restart(&mut self) {
        self.open = true;
        self.step = 0;
        self.message = None;
        self.picked = initial_pick();
        self.replace = false;
        self.wrote = false;
    }

    fn finish(&mut self) {
        let _ = settings::set("tutorial_done", json!(true));
        self.open = false;
    }

    pub fn show(&mut self, ctx: &egui::Context) -> Option<Action> {
        if !self.open {
            return None;
        }
        let mut action = None;
        let mut leave = false;
        let modal = egui::Modal::new(egui::Id::new("pw-tutorial")).show(ctx, |ui| {
            ui.set_width(520.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("ÉTAPE {} SUR {STEPS}", self.step + 1)).size(11.5).color(ui_accent()));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if kit::button(ui, "Passer").clicked() {
                        leave = true;
                    }
                });
            });
            ui.add_space(6.0);
            match self.step {
                0 => self.welcome(ui),
                1 => self.game(ui),
                2 => self.widgets(ui),
                3 => self.units(ui),
                4 => self.alerts(ui, &mut action),
                5 => self.obs(ui, &mut action),
                _ => self.install(ui),
            }
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                if self.step > 0 && kit::button(ui, "Précédent").clicked() {
                    self.step -= 1;
                    self.message = None;
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let last = self.step + 1 == STEPS;
                    if kit::primary(ui, if last { "Terminer" } else { "Suivant" }).clicked() {
                        if last {
                            leave = true;
                        } else {
                            // En quittant l'étape des widgets, la sélection devient la mise en page.
                            if self.step == 2 && self.apply_widgets() {
                                action = Some(Action::LayoutChanged);
                            }
                            self.step += 1;
                            self.message = None;
                        }
                    }
                });
            });
        });
        // Échap ferme aussi le tutoriel (il reste à revoir dans Réglages). Un clic à côté ne le ferme pas : trop facile
        // à faire par erreur au premier lancement.
        let _ = modal;
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            leave = true;
        }
        if leave {
            self.finish();
        }
        action
    }

    fn title(ui: &mut Ui, text: &str) {
        ui.label(RichText::new(text).size(20.0).strong());
        ui.add_space(8.0);
    }

    fn line(ui: &mut Ui, text: &str) {
        ui.label(RichText::new(text).size(13.5));
        ui.add_space(4.0);
    }

    fn welcome(&mut self, ui: &mut Ui) {
        Self::title(ui, "Bienvenue dans PaceWall");
        Self::line(ui, "PaceWall affiche par-dessus Le Mans Ultimate les infos utiles en course : carburant et énergie, pneus, chrono, classement, carte du circuit, drapeaux.");
        Self::line(ui, "Un ingénieur de course te parle (carburant bas, pneus froids, jaune, drapeau bleu…), et les mêmes overlays peuvent aller dans OBS pour ton stream.");
        Self::line(ui, "Tout est léger : quand tu ne roules pas, PaceWall ne consomme presque rien. Ce petit tour prend une minute.");
    }

    fn game(&mut self, ui: &mut Ui) {
        Self::title(ui, "Prépare Le Mans Ultimate");
        Self::line(ui, "1. Dans le jeu : Réglages > Gameplay > « Enable Plugins » activé. C'est ce qui permet à PaceWall de lire les données.");
        Self::line(ui, "2. Affichage en mode fenêtré ou sans bordure (pas en plein écran exclusif) : sinon l'overlay ne peut pas s'afficher par-dessus.");
        Self::line(ui, "3. Lance PaceWall avant ou après le jeu, peu importe : l'overlay s'ouvre toute seule quand le jeu démarre et se ferme avec lui.");
        ui.add_space(4.0);
        ui.label(RichText::new("Astuce : Ctrl + Alt + O affiche ou masque l'overlay à tout moment.").size(12.5).color(MUTED));
    }

    /// Écrit la mise en page choisie. Renvoie `true` si le fichier a été modifié.
    fn apply_widgets(&mut self) -> bool {
        // Une mise en page existante (tutoriel revu depuis les Réglages) n'est remplacée que sur demande.
        if has_layout() && !self.replace && !self.wrote {
            return false;
        }
        let kinds: Vec<&str> = crate::widgets::KINDS.iter().zip(&self.picked).filter(|(_, on)| **on).map(|((kind, _), _)| *kind).collect();
        let chosen = layout::from_kinds(&kinds);
        let mut all = layout::load();
        if self.obs_too {
            all.targets.insert("obs".into(), chosen.clone());
        }
        all.targets.insert("player".into(), chosen);
        self.wrote = layout::save(&all).is_ok();
        self.wrote
    }

    fn widgets(&mut self, ui: &mut Ui) {
        Self::title(ui, "Choisis tes widgets");
        Self::line(ui, "Coche ce que tu veux voir par-dessus le jeu. Chaque widget est posé à un emplacement libre ; tu le déplaceras et le régleras ensuite dans l'onglet Overlays.");
        ui.horizontal(|ui| {
            if kit::button(ui, "Essentiel").clicked() {
                self.picked = crate::widgets::KINDS.iter().map(|(kind, _)| ESSENTIAL.contains(kind)).collect();
            }
            if kit::button(ui, "Tout").clicked() {
                self.picked.iter_mut().for_each(|p| *p = true);
            }
            if kit::button(ui, "Aucun").clicked() {
                self.picked.iter_mut().for_each(|p| *p = false);
            }
            ui.label(RichText::new(format!("{} choisi(s)", self.picked.iter().filter(|p| **p).count())).size(12.5).color(MUTED));
        });
        ui.add_space(6.0);
        egui::Grid::new("pw-tutorial-widgets").num_columns(2).spacing([28.0, 5.0]).show(ui, |ui| {
            for (i, (_, name)) in crate::widgets::KINDS.iter().enumerate() {
                ui.checkbox(&mut self.picked[i], *name);
                if i % 2 == 1 {
                    ui.end_row();
                }
            }
        });
        ui.add_space(8.0);
        ui.checkbox(&mut self.obs_too, "Mettre les mêmes widgets dans OBS (pour le stream)");
        if has_layout() && !self.wrote {
            ui.checkbox(&mut self.replace, "Remplacer ma mise en page actuelle par cette sélection");
            if !self.replace {
                ui.label(RichText::new("Tu as déjà placé des widgets : sans cette case, rien n'est modifié.").size(12.5).color(MUTED));
            }
        }
    }

    fn units(&mut self, ui: &mut Ui) {
        Self::title(ui, "Tes unités");
        Self::line(ui, "Elles servent dans tous les widgets, et se changent plus tard dans Réglages.");
        let choices: [(&str, &str, &[(&str, &str)]); 4] = [
            ("unit_speed", "Vitesse", &[("kmh", "km/h"), ("mph", "mph")]),
            ("unit_temp", "Température", &[("c", "°C"), ("f", "°F")]),
            ("unit_pressure", "Pression des pneus", &[("kpa", "kPa"), ("bar", "bar"), ("psi", "psi")]),
            ("unit_fuel", "Carburant", &[("l", "litres"), ("gal", "gallons")]),
        ];
        egui::Grid::new("pw-tutorial-units").num_columns(2).spacing([24.0, 8.0]).show(ui, |ui| {
            for (key, label, options) in choices {
                ui.label(RichText::new(label).size(13.5));
                let current = settings::str_of(key, options[0].0);
                ui.horizontal(|ui| {
                    for (value, text) in options {
                        if ui.selectable_label(current == *value, *text).clicked() && current != *value {
                            let _ = settings::set(key, json!(value));
                        }
                    }
                });
                ui.end_row();
            }
        });
    }

    fn alerts(&mut self, ui: &mut Ui, action: &mut Option<Action>) {
        Self::title(ui, "Ton ingénieur de course");
        Self::line(ui, "L'onglet Alertes règle ce qu'il dit : carburant, énergie, pneus, drapeaux, dernier tour, voiture derrière… Chaque alerte s'active, se coupe, se réécoute.");
        Self::line(ui, "Tu choisis sa voix (homme, femme ou voix Windows) et son volume, et tu peux écrire tes propres phrases.");
        ui.add_space(2.0);
        let mut on = settings::bool_of("voice_enabled", true);
        if ui.checkbox(&mut on, "Activer les alertes vocales").changed() {
            let _ = settings::set("voice_enabled", json!(on));
        }
        let voice = settings::str_of("voice_engine", "engineer_m");
        ui.horizontal(|ui| {
            ui.label(RichText::new("Voix").size(13.5));
            for (value, text) in [("engineer_m", "Ingénieur"), ("engineer_f", "Ingénieure"), ("windows", "Voix Windows")] {
                if ui.selectable_label(voice == value, text).clicked() && voice != value {
                    let _ = settings::set("voice_engine", json!(value));
                }
            }
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if kit::button(ui, "Ouvrir l'onglet Alertes").clicked() {
                *action = Some(Action::Open(Page::Alerts));
            }
            if kit::button(ui, "Tester la voix").clicked() {
                let engine = crate::speech::Engine::from_setting(&settings::str_of("voice_engine", "engineer_m"));
                crate::speech::Speaker::global().say("test", "Radio check. Alertes vocales activées.", settings::int_of("voice_volume", 80) as u32, engine, None);
            }
        });
    }

    fn obs(&mut self, ui: &mut Ui, action: &mut Option<Action>) {
        Self::title(ui, "Pour OBS (facultatif)");
        Self::line(ui, "Si tu streames, active le serveur OBS dans Réglages, puis ajoute dans OBS une « Source navigateur » de 1920 × 1080 avec l'adresse :");
        ui.label(RichText::new("http://127.0.0.1:8470/overlay/obs").font(num_font(14.0)).color(ui_accent()));
        ui.add_space(4.0);
        Self::line(ui, "Tu y verras les mêmes widgets, sans avoir besoin de capturer le jeu. Désactivé, le serveur ne consomme rien.");
        if kit::button(ui, "Ouvrir les Réglages").clicked() {
            *action = Some(Action::Open(Page::Settings));
        }
    }

    fn install(&mut self, ui: &mut Ui) {
        if install::running_installed() || install::is_installed() {
            Self::title(ui, "Tout est prêt");
            Self::line(ui, "PaceWall est installé. Il tourne dans la zone de notification (près de l'horloge) : clic gauche pour ouvrir la fenêtre, clic droit pour le menu.");
            Self::line(ui, "Bonne course !");
            return;
        }
        Self::title(ui, "Installer PaceWall");
        Self::line(ui, "Pour l'installer, PaceWall se copie dans ton dossier de programmes, ajoute un raccourci au menu Démarrer et se déclare à Windows (désinstallation propre depuis « Applications installées »). Aucun droit administrateur n'est nécessaire.");
        ui.checkbox(&mut self.desktop, "Ajouter un raccourci sur le bureau");
        ui.checkbox(&mut self.autostart, "Lancer PaceWall avec Windows (dans la zone de notification)");
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if kit::primary(ui, "Installer maintenant").clicked() {
                match install::install(install::Options { desktop_shortcut: self.desktop, autostart: self.autostart }) {
                    Ok(exe) => {
                        let _ = settings::set("tutorial_done", json!(true));
                        // La copie installée prend le relais : elle ouvre PaceWall, cette copie se ferme.
                        let _ = std::process::Command::new(exe).spawn();
                        std::process::exit(0);
                    }
                    Err(e) => self.message = Some(e),
                }
            }
            ui.label(RichText::new("Facultatif : tu peux aussi garder le .exe où il est.").size(12.0).color(MUTED));
        });
        if let Some(m) = &self.message {
            ui.add_space(4.0);
            ui.label(RichText::new(format!("Installation impossible : {m}")).color(BAD));
        }
    }
}
