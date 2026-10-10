//! Petits composants d'interface partagés par les pages.

use crate::theme::*;
use eframe::egui::{self, vec2, Color32, Frame, Margin, RichText, Sense, Stroke, Ui};

pub fn panel_frame() -> Frame {
    Frame::NONE.fill(PANEL).stroke(Stroke::new(1.0, LINE)).inner_margin(Margin::same(14))
}

/// Titre de section : petit carré cyan et libellé en capitales espacées.
pub fn section(ui: &mut Ui, text: &str) {
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(6.0, 6.0), Sense::hover());
        ui.painter().rect_filled(r, 0.0, ui_accent());
        ui.label(RichText::new(text.to_uppercase()).size(11.5).color(MUTED));
    });
    ui.add_space(2.0);
}

pub fn page_title(ui: &mut Ui, title: &str, sub: &str) {
    ui.label(RichText::new(title.to_uppercase()).size(22.0).strong());
    if !sub.is_empty() {
        ui.label(RichText::new(sub).color(MUTED));
    }
    ui.add_space(10.0);
}

/// Ligne « libellé … valeur ».
pub fn kv(ui: &mut Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label.to_uppercase()).size(11.0).color(MUTED));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(value).font(num_font(13.5)));
        });
    });
}

/// Pastille d'état : carré coloré et libellé.
pub fn dot(ui: &mut Ui, color: Color32, label: &str, value: &str) {
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(8.0, 8.0), Sense::hover());
        ui.painter().rect_filled(r, 0.0, color);
        ui.label(RichText::new(label.to_uppercase()).size(11.0).color(MUTED));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(value).font(num_font(12.5)));
        });
    });
}

/// Carte de chiffre : libellé et grande valeur.
/// Cadre de statistique de largeur extérieure `width` (marges et contour compris) : permet de les ranger en grille.
pub fn stat_sized(ui: &mut Ui, width: f32, label: &str, value: &str, unit: &str) {
    Frame::NONE.fill(PANEL).stroke(Stroke::new(1.0, LINE)).inner_margin(Margin::symmetric(14, 10)).show(ui, |ui| {
        ui.set_width((width - 30.0).max(60.0));
        ui.label(RichText::new(label.to_uppercase()).size(10.5).color(MUTED));
        ui.horizontal(|ui| {
            ui.label(RichText::new(value).font(num_font(21.0)));
            if !unit.is_empty() {
                ui.label(RichText::new(unit).size(12.0).color(MUTED));
            }
        });
    });
}

/// Bouton plein d'accent (action principale).
pub fn primary(ui: &mut Ui, text: &str) -> egui::Response {
    ui.add(egui::Button::new(RichText::new(text.to_uppercase()).size(12.0).strong().color(ACCENT_INK)).fill(ui_accent()).stroke(Stroke::new(1.0, ui_accent())))
}

pub fn button(ui: &mut Ui, text: &str) -> egui::Response {
    ui.add(egui::Button::new(RichText::new(text.to_uppercase()).size(12.0)))
}

/// Écrit un nombre de façon compacte (entier sans décimale).
pub fn num(v: f64) -> String {
    if v.fract().abs() < 1e-9 {
        format!("{v:.0}")
    } else if v.abs() >= 100.0 {
        format!("{v:.1}")
    } else {
        format!("{v:.2}")
    }
}
