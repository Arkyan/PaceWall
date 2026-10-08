//! Éditeur d'overlays : on place et on règle les widgets sur un aperçu 16:9, pour le joueur ou pour OBS.
//! L'aperçu dessine les mêmes widgets que l'overlay, avec des données de démonstration ou celles du jeu.

use super::canvas::EguiCanvas;
use super::{kit, Feed};
use crate::draw::D;
use crate::theme::*;
use crate::widgets::{self, OptKind, WState};
use eframe::egui::{self, vec2, Color32, Frame, Id, Margin, Rect, RichText, Sense, Stroke, StrokeKind, Ui};
use pwcore::layout::{self, Layout, Layouts, WidgetSpec};
use pwcore::Data;
use serde_json::json;
use std::collections::HashMap;
use std::time::Duration;

pub struct State {
    layouts: Layouts,
    target: String,
    selected: Option<String>,
    dirty: bool,
    live: bool,
    states: HashMap<String, WState>,
    status: String,
}

impl Default for State {
    fn default() -> Self {
        Self { layouts: layout::load(), target: "player".into(), selected: None, dirty: false, live: false, states: HashMap::new(), status: String::new() }
    }
}

fn target_label(t: &str) -> &'static str {
    if t == "player" { "Joueur" } else { "OBS" }
}

impl State {
    fn cur(&mut self) -> &mut Layout {
        let t = self.target.clone();
        self.layouts.targets.entry(t).or_insert_with(layout::default_layout)
    }

    pub fn show(&mut self, ui: &mut Ui, feed: &mut Feed, now: f64) {
        // Aperçu animé : ~25 images par seconde tant que cette page est ouverte.
        ui.ctx().request_repaint_after(Duration::from_millis(40));
        if self.live {
            feed.poll(true);
        }
        let use_demo = !self.live || !feed.game;
        let demo;
        let data: &Data = if use_demo {
            demo = pwcore::demo::data(now);
            &demo
        } else {
            &feed.data
        };
        let live_missing = self.live && !feed.game;

        let bar = Frame::NONE.fill(PANEL).stroke(Stroke::new(1.0, LINE)).inner_margin(Margin::symmetric(14, 8));
        egui::Panel::top("ed_top").frame(bar).show(ui, |ui| self.top_bar(ui, live_missing));
        egui::Panel::left("ed_left").exact_size(210.0).resizable(false).frame(Frame::NONE.fill(PANEL).stroke(Stroke::new(1.0, LINE)).inner_margin(Margin::same(14))).show(ui, |ui| self.left(ui));
        egui::Panel::right("ed_right").exact_size(264.0).resizable(false).frame(Frame::NONE.fill(PANEL).stroke(Stroke::new(1.0, LINE)).inner_margin(Margin::same(14))).show(ui, |ui| self.inspector(ui));
        egui::CentralPanel::default().frame(Frame::NONE.fill(BG).inner_margin(Margin::same(16))).show(ui, |ui| self.stage(ui, data, now));
    }

    fn top_bar(&mut self, ui: &mut Ui, live_missing: bool) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("CIBLE").size(11.0).color(MUTED));
            for t in ["player", "obs"] {
                if ui.selectable_label(self.target == t, target_label(t).to_uppercase()).clicked() && self.target != t {
                    self.target = t.into();
                    self.selected = None;
                }
            }
            ui.add_space(18.0);
            ui.label(RichText::new("DONNÉES").size(11.0).color(MUTED));
            ui.selectable_value(&mut self.live, false, "DÉMO");
            ui.selectable_value(&mut self.live, true, "JEU EN DIRECT");
            if live_missing {
                ui.label(RichText::new("jeu absent : démo").size(12.0).color(WARN));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_enabled_ui(self.dirty, |ui| {
                    if kit::primary(ui, "Enregistrer").clicked() {
                        self.status = match layout::save(&self.layouts) {
                            Ok(()) => {
                                self.dirty = false;
                                "Enregistré".into()
                            }
                            Err(e) => format!("Échec : {e}"),
                        };
                    }
                });
                let (text, color) = if self.dirty { ("Modifications non enregistrées".to_string(), WARN) } else { (if self.status.is_empty() { "Enregistré".to_string() } else { self.status.clone() }, MUTED) };
                ui.label(RichText::new(text).size(12.5).color(color));
            });
        });
    }

    fn left(&mut self, ui: &mut Ui) {
        kit::section(ui, "Ajouter un widget");
        for (kind, name) in widgets::KINDS {
            if ui.add_sized([ui.available_width(), 30.0], egui::Button::new(format!("+  {name}"))).clicked() {
                let layout = self.cur();
                let n = layout.widgets.iter().filter(|w| w.kind == kind).count();
                let mut id = if n == 0 { kind.to_string() } else { format!("{kind}-{}", n + 1) };
                while layout.widgets.iter().any(|w| w.id == id) {
                    id.push('x');
                }
                layout.widgets.push(WidgetSpec::new(&id, kind, 0.3, 0.3));
                self.selected = Some(id);
                self.dirty = true;
            }
        }
        ui.add_space(14.0);
        kit::section(ui, "Sur l'écran");
        let items: Vec<(String, String)> = self.cur().widgets.iter().map(|w| (w.id.clone(), format!("{}{}", widgets::name(&w.kind), if w.id != w.kind { format!(" ({})", w.id) } else { String::new() }))).collect();
        for (id, label) in items {
            if ui.selectable_label(self.selected.as_deref() == Some(&id), label).clicked() {
                self.selected = Some(id);
            }
        }
        ui.add_space(14.0);
        let other = if self.target == "player" { "obs" } else { "player" };
        if kit::button(ui, &format!("Copier vers {}", target_label(other))).clicked() {
            let copy = self.cur().clone();
            self.layouts.targets.insert(other.into(), copy);
            self.dirty = true;
            self.status = format!("Mise en page copiée vers {}", target_label(other));
        }
        ui.add_space(10.0);
        let hint = if self.target == "obs" {
            "OBS : source navigateur 1920×1080 sur l'adresse indiquée dans Réglages."
        } else {
            "Joueur : l'overlay s'affiche par-dessus le jeu (fenêtré ou sans bordure)."
        };
        ui.label(RichText::new(hint).size(12.0).color(MUTED));
    }

    fn stage(&mut self, ui: &mut Ui, data: &Data, now: f64) {
        let avail = ui.available_rect_before_wrap();
        let (mut w, mut h) = (avail.width(), avail.width() * 9.0 / 16.0);
        if h > avail.height() {
            h = avail.height();
            w = h * 16.0 / 9.0;
        }
        let stage = Rect::from_center_size(avail.center(), vec2(w, h));
        ui.allocate_rect(avail, Sense::hover());
        let painter = ui.painter_at(stage);
        painter.rect_filled(stage, 0.0, Color32::from_rgb(14, 19, 27));
        for i in 1..20 {
            let x = stage.min.x + stage.width() * i as f32 / 20.0;
            painter.line_segment([egui::pos2(x, stage.min.y), egui::pos2(x, stage.max.y)], Stroke::new(1.0, Color32::from_rgba_unmultiplied(25, 211, 243, 12)));
        }
        for i in 1..12 {
            let y = stage.min.y + stage.height() * i as f32 / 12.0;
            painter.line_segment([egui::pos2(stage.min.x, y), egui::pos2(stage.max.x, y)], Stroke::new(1.0, Color32::from_rgba_unmultiplied(25, 211, 243, 12)));
        }
        painter.rect_stroke(stage, 0.0, Stroke::new(1.0, LINE), StrokeKind::Inside);

        let unit = stage.height() / 1080.0;
        let n = self.cur().widgets.len();
        for i in 0..n {
            let spec = self.cur().widgets[i].clone();
            let origin = stage.min + vec2(spec.x * stage.width(), spec.y * stage.height());
            let scale = spec.scale * unit;
            let st = self.states.entry(spec.id.clone()).or_default();
            let (bw, bh) = {
                let mut canvas = EguiCanvas { painter: &painter };
                let d = D::new(&mut canvas, (origin.x, origin.y), scale, spec.opacity, widgets::bg(&spec));
                widgets::draw(&spec.kind, &d, &spec, data, st, now)
            };
            let rect = Rect::from_min_size(origin, vec2(bw, bh) * scale);
            let resp = ui.interact(rect, Id::new(("pw-widget", &spec.id)), Sense::click_and_drag());
            if resp.clicked() || resp.drag_started() {
                self.selected = Some(spec.id.clone());
            }
            if resp.dragged() {
                let d = resp.drag_delta();
                let w = &mut self.cur().widgets[i];
                w.x = (w.x + d.x / stage.width()).clamp(0.0, 0.98);
                w.y = (w.y + d.y / stage.height()).clamp(0.0, 0.98);
                self.dirty = true;
            }
            if resp.drag_stopped() {
                // Aimantation légère (grille de 0,25 %) une fois le widget lâché.
                let w = &mut self.cur().widgets[i];
                w.x = (w.x * 400.0).round() / 400.0;
                w.y = (w.y * 400.0).round() / 400.0;
            }
            if resp.hovered() || resp.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
            }
            if self.selected.as_deref() == Some(spec.id.as_str()) {
                painter.rect_stroke(rect.expand(3.0), 0.0, Stroke::new(2.0, ACCENT), StrokeKind::Outside);
            }
        }
    }

    fn inspector(&mut self, ui: &mut Ui) {
        let Some(id) = self.selected.clone() else {
            kit::section(ui, "Réglages");
            ui.label(RichText::new("Clique un widget dans l'aperçu pour le régler.\n\nGlisse-le pour le déplacer.").color(MUTED));
            return;
        };
        let target = self.target.clone();
        let layout = self.layouts.targets.entry(target).or_insert_with(layout::default_layout);
        let Some(idx) = layout.widgets.iter().position(|w| w.id == id) else {
            self.selected = None;
            return;
        };

        let mut changed = false;
        let (mut duplicate, mut remove) = (false, false);
        {
            let spec = &mut layout.widgets[idx];
            kit::section(ui, widgets::name(&spec.kind));
            ui.add_space(4.0);
            changed |= ui.add(egui::Slider::new(&mut spec.scale, 0.5..=2.5).text("Taille").fixed_decimals(2)).changed();
            changed |= ui.add(egui::Slider::new(&mut spec.opacity, 0.1..=1.0).text("Opacité").fixed_decimals(2)).changed();
            let mut bg = spec.num("bg", 78.0) as f32;
            if ui.add(egui::Slider::new(&mut bg, 0.0..=100.0).text("Fond (%)").step_by(5.0)).changed() {
                spec.opts.insert("bg".into(), json!(bg.round()));
                changed = true;
            }
            ui.add_space(6.0);
            for opt in widgets::options(&spec.kind) {
                match &opt.kind {
                    OptKind::Bool(def) => {
                        let mut v = spec.flag(opt.id, *def);
                        if ui.checkbox(&mut v, opt.label).changed() {
                            spec.opts.insert(opt.id.into(), json!(v));
                            changed = true;
                        }
                    }
                    OptKind::Num { min, max, step, def } => {
                        let mut v = spec.num(opt.id, *def);
                        if ui.add(egui::Slider::new(&mut v, *min..=*max).step_by(*step).text(opt.label)).changed() {
                            spec.opts.insert(opt.id.into(), json!(v));
                            changed = true;
                        }
                    }
                    OptKind::Choice(choices, def) => {
                        let cur = spec.choice(opt.id, def).to_string();
                        let shown = choices.iter().find(|(v, _)| *v == cur).map(|(_, l)| *l).unwrap_or("");
                        ui.horizontal(|ui| {
                            ui.label(opt.label);
                            egui::ComboBox::from_id_salt(("opt", opt.id)).selected_text(shown).show_ui(ui, |ui| {
                                for (value, label) in choices.iter() {
                                    if ui.selectable_label(cur == *value, *label).clicked() {
                                        spec.opts.insert(opt.id.into(), json!(value));
                                        changed = true;
                                    }
                                }
                            });
                        });
                    }
                }
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                duplicate = kit::button(ui, "Dupliquer").clicked();
                remove = kit::button(ui, "Supprimer").clicked();
            });
            ui.add_space(8.0);
            ui.label(RichText::new(format!("Position : {:.0} % × {:.0} %", spec.x * 100.0, spec.y * 100.0)).size(12.0).color(MUTED));
        }

        if duplicate {
            let mut copy = layout.widgets[idx].clone();
            let mut new_id = format!("{}-copie", copy.id);
            while layout.widgets.iter().any(|w| w.id == new_id) {
                new_id.push('2');
            }
            copy.id = new_id.clone();
            copy.x = (copy.x + 0.03).min(0.9);
            copy.y = (copy.y + 0.03).min(0.9);
            layout.widgets.push(copy);
            self.selected = Some(new_id);
            changed = true;
        }
        if remove {
            layout.widgets.remove(idx);
            self.selected = None;
            changed = true;
        }
        if changed {
            self.dirty = true;
        }
    }
}
