//! Tutoriel de premier lancement : six étapes pour préparer le jeu, placer ses overlays, régler les alertes vocales,
//! brancher OBS et, si PaceWall n'est pas encore installé, l'installer (raccourcis, démarrage avec Windows).
//! Revoir le tutoriel : Réglages > Application.

use super::{kit, Page};
use crate::install;
use crate::theme::*;
use eframe::egui::{self, Align, Layout, RichText, Ui};
use pwcore::settings;
use serde_json::json;

/// Ce que le tutoriel demande à la fenêtre.
pub enum Action {
    /// Ouvrir une page (le tutoriel reste ouvert au-dessus).
    Open(Page),
}

pub struct Tutorial {
    pub open: bool,
    step: usize,
    desktop: bool,
    autostart: bool,
    message: Option<String>,
}

const STEPS: usize = 6;

impl Tutorial {
    /// Ouvert au premier lancement (tant que « tutorial_done » n'est pas enregistré).
    pub fn new() -> Self {
        Self { open: !settings::bool_of("tutorial_done", false), step: 0, desktop: true, autostart: false, message: None }
    }

    pub fn restart(&mut self) {
        self.open = true;
        self.step = 0;
        self.message = None;
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
                2 => self.overlays(ui, &mut action),
                3 => self.alerts(ui, &mut action),
                4 => self.obs(ui, &mut action),
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

    fn overlays(&mut self, ui: &mut Ui, action: &mut Option<Action>) {
        Self::title(ui, "Place tes overlays");
        Self::line(ui, "Dans l'onglet Overlays, ajoute les widgets que tu veux (bouton WIDGETS), glisse-les où tu veux sur l'aperçu, puis Enregistrer.");
        Self::line(ui, "Clique un widget pour régler sa taille, son opacité, sa couleur, son titre. Chaque widget reste dans l'écran et la carte du circuit a sa propre zone.");
        Self::line(ui, "Tu as deux mises en page : « Joueur » (par-dessus le jeu) et « OBS » (pour ton stream).");
        ui.add_space(4.0);
        if kit::button(ui, "Ouvrir l'onglet Overlays").clicked() {
            *action = Some(Action::Open(Page::Overlays));
        }
    }

    fn alerts(&mut self, ui: &mut Ui, action: &mut Option<Action>) {
        Self::title(ui, "Ton ingénieur de course");
        Self::line(ui, "L'onglet Alertes règle ce qu'il dit : carburant, énergie, pneus, drapeaux, dernier tour, voiture derrière… Chaque alerte s'active, se coupe, se réécoute.");
        Self::line(ui, "Tu choisis sa voix (homme, femme ou voix Windows) et son volume, et tu peux écrire tes propres phrases.");
        Self::line(ui, "Pour tout essayer sans le jeu, lance la simulation en bas de cet onglet.");
        ui.add_space(4.0);
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
