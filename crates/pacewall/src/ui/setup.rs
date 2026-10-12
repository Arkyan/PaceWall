//! Fenêtre d'installation : ce que montre `PaceWall-Setup-x.y.z.exe` (le même programme que PaceWall, sous un autre nom).
//! Première installation, mise à jour d'une version déjà installée, ou réinstallation. En mode automatique
//! (`--update`, lancé par PaceWall après un téléchargement), l'installation démarre seule et PaceWall se relance.

use super::kit;
use crate::install::{self, Options};
use crate::theme::*;
use crate::update;
use eframe::egui::{self, vec2, Align2, Frame, Margin, RichText, Sense};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::time::Duration;

enum Step {
    Ask,
    Working(Receiver<Result<PathBuf, String>>),
    Done(PathBuf),
    Failed(String),
}

struct Setup {
    step: Step,
    /// Mode automatique : pas de question, PaceWall se relance à la fin.
    auto: bool,
    installed: Option<String>,
    desktop: bool,
    autostart: bool,
    launch: bool,
}

pub fn run(auto: bool) {
    if !crate::win::single_instance("PaceWallSetup") {
        return;
    }
    let installed = install::installed_version();
    // Mise à jour : on reprend les choix de l'installation en place. Première installation : raccourci sur le bureau.
    let current = install::current_options();
    let setup = Setup { step: Step::Ask, auto, desktop: if installed.is_some() { current.desktop_shortcut } else { true }, autostart: current.autostart, launch: true, installed };
    let icon = egui::IconData { rgba: super::icon_rgba(), width: 64, height: 64 };
    let viewport = egui::ViewportBuilder::default().with_title("Installation de PaceWall").with_inner_size([560.0, 430.0]).with_resizable(false).with_maximize_button(false).with_icon(icon);
    let options = eframe::NativeOptions { viewport, centered: true, ..Default::default() };
    let _ = eframe::run_native(
        "Installation de PaceWall",
        options,
        Box::new(move |cc| {
            crate::theme::install(&cc.egui_ctx);
            Ok(Box::new(setup))
        }),
    );
}

impl Setup {
    fn start(&mut self) {
        let (tx, rx) = channel();
        let opts = Options { desktop_shortcut: self.desktop, autostart: self.autostart };
        std::thread::spawn(move || {
            let _ = tx.send(install::install(opts));
        });
        self.step = Step::Working(rx);
    }

    /// Ce que fait cette installation, en une phrase.
    fn headline(&self) -> (String, &'static str) {
        match &self.installed {
            None => ("Installer PaceWall sur cet ordinateur".into(), "Installer"),
            Some(v) if update::newer(update::VERSION, v) => (format!("Mettre à jour PaceWall : {v} → {}", update::VERSION), "Mettre à jour"),
            Some(v) if update::newer(v, update::VERSION) => (format!("Une version plus récente ({v}) est déjà installée"), "Installer quand même"),
            Some(_) => ("Cette version est déjà installée".into(), "Réinstaller"),
        }
    }
}

fn launch(exe: &std::path::Path) {
    let _ = std::process::Command::new(exe).spawn();
}

impl eframe::App for Setup {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if self.auto && matches!(self.step, Step::Ask) {
            self.start();
        }
        if let Step::Working(rx) = &self.step {
            match rx.try_recv() {
                Ok(Ok(exe)) => self.step = Step::Done(exe),
                Ok(Err(e)) => self.step = Step::Failed(e),
                Err(TryRecvError::Disconnected) => self.step = Step::Failed("l'installation s'est interrompue".into()),
                Err(TryRecvError::Empty) => ctx.request_repaint_after(Duration::from_millis(100)),
            }
        }
        // Mode automatique : dès que c'est fait, PaceWall repart et cette fenêtre se ferme.
        if let (true, Step::Done(exe)) = (self.auto, &self.step) {
            launch(exe);
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        egui::CentralPanel::default().frame(Frame::NONE.fill(BG).inner_margin(Margin::same(28))).show(ui, |ui| {
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(34.0, 34.0), Sense::hover());
                ui.painter().rect_filled(r, 0.0, PANEL2);
                ui.painter().rect_filled(egui::Rect::from_min_size(r.min, vec2(4.0, 34.0)), 0.0, ui_accent());
                ui.painter().text(r.center() + vec2(1.0, 0.0), Align2::CENTER_CENTER, "PW", num_font(14.0), TEXT);
                ui.add_space(4.0);
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        ui.label(RichText::new("PACE").size(20.0).strong());
                        ui.label(RichText::new("WALL").size(20.0).strong().color(ui_accent()));
                    });
                    ui.label(RichText::new(format!("Version {}", update::version_label())).size(12.5).color(MUTED));
                });
            });
            ui.add_space(22.0);

            let (headline, action) = self.headline();
            match &self.step {
                Step::Ask => {
                    ui.label(RichText::new(headline).size(17.0).strong());
                    ui.add_space(4.0);
                    ui.label(RichText::new(format!("Dossier : {}", install::install_dir().display())).font(num_font(12.0)).color(MUTED));
                    ui.label(RichText::new("Pour ton compte Windows uniquement : aucun droit administrateur demandé. Tes réglages, mises en page et historique sont conservés.").size(12.5).color(MUTED));
                    ui.add_space(16.0);
                    ui.checkbox(&mut self.desktop, "Raccourci sur le bureau");
                    ui.checkbox(&mut self.autostart, "Lancer PaceWall avec Windows (dans la zone de notification)");
                    ui.checkbox(&mut self.launch, "Ouvrir PaceWall à la fin de l'installation");
                    ui.add_space(20.0);
                    let mut go = false;
                    let mut try_it = false;
                    ui.horizontal(|ui| {
                        go = kit::primary(ui, action).clicked();
                        if kit::button(ui, "Annuler").clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        if self.installed.is_none() {
                            try_it = kit::button(ui, "Essayer sans installer").on_hover_text("Lance PaceWall depuis ce fichier, sans rien copier sur l'ordinateur").clicked();
                        }
                    });
                    if go {
                        self.start();
                    }
                    if try_it {
                        if let Ok(me) = std::env::current_exe() {
                            let _ = std::process::Command::new(me).arg("--run").spawn();
                        }
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                }
                Step::Working(_) => {
                    ui.label(RichText::new(headline).size(17.0).strong());
                    ui.add_space(16.0);
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Installation en cours…");
                    });
                }
                Step::Done(exe) => {
                    let exe = exe.clone();
                    ui.label(RichText::new("PaceWall est installé").size(17.0).strong().color(OK));
                    ui.add_space(4.0);
                    ui.label(RichText::new("Tu le retrouves dans le menu Démarrer. Pour le retirer : Paramètres Windows > Applications installées, ou Réglages dans PaceWall.").size(12.5).color(MUTED));
                    ui.add_space(16.0);
                    ui.checkbox(&mut self.launch, "Ouvrir PaceWall maintenant");
                    ui.add_space(20.0);
                    if kit::primary(ui, "Terminer").clicked() {
                        if self.launch {
                            launch(&exe);
                        }
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                }
                Step::Failed(e) => {
                    ui.label(RichText::new("L'installation n'a pas abouti").size(17.0).strong().color(BAD));
                    ui.add_space(4.0);
                    ui.label(RichText::new(e).size(13.0));
                    ui.label(RichText::new("Ferme PaceWall s'il est encore ouvert (icône près de l'horloge > Quitter), puis réessaie.").size(12.5).color(MUTED));
                    ui.add_space(20.0);
                    let mut retry = false;
                    ui.horizontal(|ui| {
                        retry = kit::primary(ui, "Réessayer").clicked();
                        if kit::button(ui, "Fermer").clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                    if retry {
                        self.auto = false;
                        self.start();
                    }
                }
            }
        });
    }
}
