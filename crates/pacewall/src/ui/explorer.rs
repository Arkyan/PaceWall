//! Données en direct : toutes les valeurs lues dans le jeu, groupées, avec recherche.
//! Sert à comprendre ce que LMU expose et à vérifier les overlays.

use super::{kit, Feed};
use crate::theme::*;
use eframe::egui::{self, Margin, RichText, Ui};
use pwcore::Data;
use serde_json::Value;
use std::time::Duration;

pub struct State {
    filter: String,
    hz: u32,
    frozen: bool,
    grid: bool,
    snapshot: Data,
}

impl Default for State {
    fn default() -> Self {
        Self { filter: String::new(), hz: 5, frozen: false, grid: false, snapshot: Data::default() }
    }
}

fn group_title(key: &str) -> &'static str {
    match key {
        "game" => "Jeu",
        "session" => "Session",
        "weather" => "Météo",
        "player" => "Pilote",
        "fuel" => "Carburant",
        "energy" => "Énergie",
        "lap" => "Tour",
        "gap" => "Écarts",
        "aids" => "Aides",
        "car" => "Voiture",
        "pit" => "Stands",
        "tyre" => "Pneus",
        "brake" => "Freins",
        "position" => "Classement du joueur",
        "strategy" => "Stratégie",
        "grid" => "Classement complet",
        _ => "Autres",
    }
}

const ORDER: [&str; 16] = ["game", "session", "weather", "player", "fuel", "energy", "strategy", "lap", "gap", "aids", "car", "pit", "tyre", "brake", "position", "grid"];

fn show_value(v: &Value, unit: &str) -> String {
    match v {
        Value::Bool(b) => if *b { "oui".into() } else { "non".into() },
        Value::Number(n) => {
            let f = n.as_f64().unwrap_or(0.0);
            let s = kit::num(f);
            if unit.is_empty() { s } else { format!("{s} {unit}") }
        }
        Value::String(s) => if s.is_empty() { "—".into() } else { s.clone() },
        other => other.to_string(),
    }
}

impl State {
    pub fn show(&mut self, ui: &mut Ui, feed: &mut Feed) {
        // La lecture ne se fait que sur cette page, à la cadence choisie.
        if !self.frozen {
            // Service web du jeu (arrêt au stand, état de la voiture, prévisions) : demandé tant que la page est ouverte.
            pwcore::carinfo::touch();
            pwcore::forecast::touch();
            feed.poll(self.grid);
            ui.ctx().request_repaint_after(Duration::from_millis(1000 / self.hz as u64));
        }
        // En mode figé, l'instantané est sorti de `self` le temps de l'affichage (évite un double emprunt).
        let frozen_data = if self.frozen { Some(std::mem::take(&mut self.snapshot)) } else { None };
        let data: &Data = frozen_data.as_ref().unwrap_or(&feed.data);

        egui::Frame::NONE.inner_margin(Margin::symmetric(24, 18)).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("DONNÉES EN DIRECT").size(18.0).strong());
                ui.add_space(10.0);
                let (color, text) = if feed.game { (OK, "Jeu connecté") } else { (BAD, "Jeu absent") };
                ui.label(RichText::new(text.to_uppercase()).size(11.5).color(color));
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.filter).hint_text("Filtrer (ex. tyre, fuel, temp)…").desired_width(260.0));
                egui::ComboBox::from_id_salt("hz").selected_text(format!("{} Hz", self.hz)).show_ui(ui, |ui| {
                    for h in [1u32, 2, 5, 10, 30] {
                        ui.selectable_value(&mut self.hz, h, format!("{h} Hz"));
                    }
                });
                ui.checkbox(&mut self.grid, "Classement complet");
                let label = if self.frozen { "Reprendre" } else { "Figer" };
                if kit::button(ui, label).clicked() {
                    if !self.frozen {
                        // L'instantané est celui du dernier affichage : on le reconstruit champ par champ.
                        let fields = feed
                            .data
                            .fields
                            .iter()
                            .map(|f| pwcore::flatten::Field { key: f.key.clone(), unit: f.unit, label: f.label, value: f.value.clone() })
                            .collect();
                        self.snapshot = Data::new(fields);
                    }
                    self.frozen = !self.frozen;
                }
                ui.label(RichText::new(format!("{} valeurs", data.fields.len())).color(MUTED));
            });
            ui.add_space(8.0);

            if data.is_empty() {
                ui.add_space(40.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("En attente de données…").color(MUTED));
                    ui.label(RichText::new("Lance Le Mans Ultimate (ou fakegame pour tester).").size(12.5).color(MUTED));
                });
                return;
            }

            let q = self.filter.trim().to_lowercase();
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                for prefix in ORDER {
                    let rows: Vec<_> = data
                        .fields
                        .iter()
                        .filter(|f| f.key.split('.').next() == Some(prefix))
                        .filter(|f| q.is_empty() || f.key.to_lowercase().contains(&q) || f.label.to_lowercase().contains(&q))
                        .collect();
                    if rows.is_empty() {
                        continue;
                    }
                    let title = format!("{}  ·  {}", group_title(prefix).to_uppercase(), rows.len());
                    egui::CollapsingHeader::new(RichText::new(title).size(12.5).color(MUTED)).id_salt(prefix).default_open(prefix != "grid" || !q.is_empty()).show(ui, |ui| {
                        egui::Grid::new(("grid", prefix)).num_columns(3).spacing([18.0, 3.0]).striped(true).show(ui, |ui| {
                            for f in rows {
                                ui.label(RichText::new(f.label).size(12.5));
                                ui.label(RichText::new(&f.key).font(num_font(11.5)).color(MUTED));
                                ui.label(RichText::new(show_value(&f.value, f.unit)).font(num_font(13.0)));
                                ui.end_row();
                            }
                        });
                    });
                }
            });
        });
        if let Some(d) = frozen_data {
            self.snapshot = d;
        }
    }
}
