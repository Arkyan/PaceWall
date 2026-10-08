//! Historique : sessions enregistrées, statistiques, courbes et tableau des tours.

use super::kit;
use crate::draw;
use crate::theme::*;
use eframe::egui::{self, Margin, RichText, Sense, Ui, vec2};
use egui_plot::{Line, Plot, PlotPoints, Points};
use pwcore::db;

struct Session {
    id: i64,
    started: i64,
    ended: Option<i64>,
    track: String,
    car: String,
    class: String,
    kind: i64,
    laps: i64,
    best: Option<f64>,
}

struct Lap {
    lap: i64,
    time: f64,
    valid: bool,
    pit: bool,
    fuel: Option<f64>,
    energy: Option<f64>,
    wear: Option<f64>,
    wear_end: f64,
    tyre_temp: f64,
    brake_temp: f64,
    track_temp: f64,
    rain: f64,
    place: i64,
    compound: String,
}

#[derive(Default)]
pub struct State {
    sessions: Vec<Session>,
    selected: Option<i64>,
    laps: Vec<Lap>,
    confirm_delete: bool,
    error: Option<String>,
}

fn session_name(t: i64) -> &'static str {
    match t {
        0 => "Test",
        1..=4 => "Essais libres",
        5..=8 => "Qualifications",
        9 => "Warm-up",
        10..=13 => "Course",
        _ => "Session",
    }
}

fn date(ts: i64) -> String {
    // Jour et heure UTC suffisent pour s'y retrouver ; pas de dépendance de fuseau horaire.
    let days = ts.div_euclid(86_400);
    let secs = ts.rem_euclid(86_400);
    let (y, m, d) = civil(days);
    format!("{d:02}/{m:02}/{y} {:02}:{:02}", secs / 3600, secs % 3600 / 60)
}

/// Jours depuis 1970-01-01 vers (année, mois, jour).
fn civil(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn duration(a: i64, b: Option<i64>) -> String {
    match b {
        None => "en cours".into(),
        Some(b) => {
            let m = ((b - a) / 60).max(0);
            if m >= 60 { format!("{} h {:02}", m / 60, m % 60) } else { format!("{m} min") }
        }
    }
}

impl State {
    pub fn reload(&mut self) {
        self.error = None;
        let conn = match db::open() {
            Ok(c) => c,
            Err(e) => {
                self.error = Some(format!("Base inaccessible : {e}"));
                return;
            }
        };
        self.sessions = db::list_sessions(&conn)
            .unwrap_or_default()
            .into_iter()
            .map(|s| Session {
                id: s["id"].as_i64().unwrap_or(0),
                started: s["started"].as_i64().unwrap_or(0),
                ended: s["ended"].as_i64(),
                track: s["track"].as_str().unwrap_or("").into(),
                car: s["car"].as_str().unwrap_or("").into(),
                class: s["class"].as_str().unwrap_or("").into(),
                kind: s["type"].as_i64().unwrap_or(-1),
                laps: s["laps"].as_i64().unwrap_or(0),
                best: s["best"].as_f64(),
            })
            .collect();
        if let Some(id) = self.selected {
            if self.sessions.iter().any(|s| s.id == id) {
                self.load_laps(id);
            } else {
                self.selected = None;
                self.laps.clear();
            }
        }
    }

    fn load_laps(&mut self, id: i64) {
        self.selected = Some(id);
        self.confirm_delete = false;
        self.laps = db::open()
            .ok()
            .and_then(|c| db::laps(&c, id).ok())
            .unwrap_or_default()
            .into_iter()
            .map(|l| Lap {
                lap: l["lap"].as_i64().unwrap_or(0),
                time: l["time"].as_f64().unwrap_or(0.0),
                valid: l["valid"].as_bool().unwrap_or(true),
                pit: l["pit"].as_bool().unwrap_or(false),
                fuel: l["fuel"].as_f64(),
                energy: l["energy"].as_f64(),
                wear: l["wear"].as_f64(),
                wear_end: l["wear_end"].as_f64().unwrap_or(0.0),
                tyre_temp: l["tyre_temp"].as_f64().unwrap_or(0.0),
                brake_temp: l["brake_temp"].as_f64().unwrap_or(0.0),
                track_temp: l["track_temp"].as_f64().unwrap_or(0.0),
                rain: l["rain"].as_f64().unwrap_or(0.0),
                place: l["place"].as_i64().unwrap_or(0),
                compound: l["compound"].as_str().unwrap_or("").into(),
            })
            .collect();
    }

    pub fn show(&mut self, ui: &mut Ui) {
        if let Some(e) = &self.error {
            ui.label(RichText::new(e).color(BAD));
            return;
        }
        egui::Panel::left("history_list").exact_size(310.0).resizable(false).frame(egui::Frame::NONE.fill(PANEL).stroke(egui::Stroke::new(1.0, LINE))).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add_space(14.0);
                ui.label(RichText::new("SESSIONS").size(12.0).color(MUTED));
                if kit::button(ui, "Actualiser").clicked() {
                    self.reload();
                }
            });
            ui.add_space(4.0);
            if self.sessions.is_empty() {
                ui.add_space(20.0);
                ui.label(RichText::new("  Aucune session enregistrée.\n  Les tours s'enregistrent automatiquement\n  pendant que PaceWall tourne.").color(MUTED));
            }
            let mut pick = None;
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                for s in &self.sessions {
                    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 58.0), Sense::click());
                    let on = self.selected == Some(s.id);
                    if on || resp.hovered() {
                        ui.painter().rect_filled(rect, 0.0, ACCENT.gamma_multiply(0.10));
                    }
                    if on {
                        ui.painter().rect_filled(egui::Rect::from_min_size(rect.min, vec2(3.0, rect.height())), 0.0, ACCENT);
                    }
                    let p = ui.painter();
                    p.text(rect.min + vec2(16.0, 10.0), egui::Align2::LEFT_TOP, &s.track, egui::FontId::proportional(14.5), TEXT);
                    let line = format!("{} · {} tours{} · {}", session_name(s.kind), s.laps, s.best.map(|b| format!(" · {}", draw::lap(Some(b)))).unwrap_or_default(), date(s.started));
                    p.text(rect.min + vec2(16.0, 32.0), egui::Align2::LEFT_TOP, line, num_font(11.0), MUTED);
                    if resp.clicked() {
                        pick = Some(s.id);
                    }
                }
            });
            if let Some(id) = pick {
                self.load_laps(id);
            }
        });

        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(BG)).show(ui, |ui| {
            let Some(id) = self.selected else {
                ui.centered_and_justified(|ui| {
                    ui.label(RichText::new("Choisis une session à gauche.").color(MUTED));
                });
                return;
            };
            let Some(found) = self.sessions.iter().find(|s| s.id == id) else { return };
            // Copie des champs affichés : la fermeture ci-dessous modifie `self`.
            let session = Session { id: found.id, started: found.started, ended: found.ended, track: found.track.clone(), car: found.car.clone(), class: found.class.clone(), kind: found.kind, laps: found.laps, best: found.best };
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                egui::Frame::NONE.inner_margin(Margin::symmetric(24, 18)).show(ui, |ui| {
                    let mut delete = false;
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(RichText::new(&session.track).size(20.0).strong());
                            ui.label(RichText::new(format!("{} · {} ({}) · {} · {}", session_name(session.kind), session.car, session.class, date(session.started), duration(session.started, session.ended))).color(MUTED));
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                            if self.confirm_delete {
                                if kit::button(ui, "Confirmer la suppression").clicked() {
                                    delete = true;
                                }
                                if kit::button(ui, "Annuler").clicked() {
                                    self.confirm_delete = false;
                                }
                            } else if kit::button(ui, "Supprimer").clicked() {
                                self.confirm_delete = true;
                            }
                        });
                    });
                    if delete {
                        if let Ok(c) = db::open() {
                            let _ = db::delete_session(&c, id);
                        }
                        self.selected = None;
                        self.reload();
                        return;
                    }
                    ui.add_space(12.0);
                    self.stats(ui);
                    ui.add_space(12.0);
                    self.charts(ui);
                    ui.add_space(12.0);
                    self.table(ui);
                });
            });
        });
    }

    fn stats(&self, ui: &mut Ui) {
        let clean: Vec<&Lap> = self.laps.iter().filter(|l| l.valid && !l.pit).collect();
        let best = clean.iter().map(|l| l.time).fold(f64::INFINITY, f64::min);
        let mean = if clean.is_empty() { None } else { Some(clean.iter().map(|l| l.time).sum::<f64>() / clean.len() as f64) };
        let sd = mean.filter(|_| clean.len() > 1).map(|m| (clean.iter().map(|l| (l.time - m).powi(2)).sum::<f64>() / clean.len() as f64).sqrt());
        let avg = |f: &dyn Fn(&Lap) -> Option<f64>| {
            let v: Vec<f64> = self.laps.iter().filter_map(f).collect();
            if v.is_empty() { None } else { Some(v.iter().sum::<f64>() / v.len() as f64) }
        };
        ui.horizontal_wrapped(|ui| {
            kit::stat(ui, "Tours", &self.laps.len().to_string(), "");
            kit::stat(ui, "Meilleur", &draw::lap(best.is_finite().then_some(best)), "");
            kit::stat(ui, "Moyenne", &draw::lap(mean), "");
            kit::stat(ui, "Régularité", &sd.map(|s| format!("± {s:.2}")).unwrap_or_else(|| "—".into()), "s");
            kit::stat(ui, "Carburant / tour", &draw::n(avg(&|l| l.fuel), 2), "L");
            kit::stat(ui, "Énergie / tour", &draw::n(avg(&|l| l.energy), 1), "%");
            kit::stat(ui, "Usure pneus / tour", &draw::n(avg(&|l| l.wear), 2), "%");
        });
    }

    fn charts(&self, ui: &mut Ui) {
        let series = |f: &dyn Fn(&Lap) -> Option<f64>| -> Vec<[f64; 2]> { self.laps.iter().filter_map(|l| f(l).map(|v| [l.lap as f64, v])).collect() };
        let charts: [(&str, Vec<[f64; 2]>, egui::Color32); 4] = [
            ("Temps au tour (s)", series(&|l| (l.valid && !l.pit).then_some(l.time)), ACCENT),
            ("Carburant par tour (L)", series(&|l| l.fuel), BLUE),
            ("Usure des pneus (%)", series(&|l| Some(l.wear_end)), OK),
            ("Température des pneus (°C)", series(&|l| Some(l.tyre_temp)), BAD),
        ];
        let w = ((ui.available_width() - 12.0) / 2.0).max(280.0);
        ui.horizontal_wrapped(|ui| {
            for (i, (title, pts, color)) in charts.iter().enumerate() {
                kit::panel_frame().show(ui, |ui| {
                    ui.set_width(w - 30.0);
                    kit::section(ui, title);
                    Plot::new(("plot", i))
                        .height(150.0)
                        .allow_zoom(false)
                        .allow_drag(false)
                        .allow_scroll(false)
                        .allow_boxed_zoom(false)
                        .show_grid(true)
                        .show(ui, |p| {
                            if pts.is_empty() {
                                return;
                            }
                            p.line(Line::new(*title, PlotPoints::from(pts.clone())).color(*color).width(2.0));
                            p.points(Points::new(*title, PlotPoints::from(pts.clone())).color(*color).radius(2.5));
                        });
                });
            }
        });
    }

    fn table(&self, ui: &mut Ui) {
        kit::panel_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            kit::section(ui, "Tours");
            egui::Grid::new("laps").num_columns(12).spacing([18.0, 4.0]).striped(true).show(ui, |ui| {
                for h in ["Tour", "Temps", "Carbu (L)", "Énergie (%)", "Usure/tour", "Usure (%)", "Pneus °C", "Freins °C", "Piste °C", "Pluie", "Pos.", "Gomme"] {
                    ui.label(RichText::new(h.to_uppercase()).size(10.5).color(MUTED));
                }
                ui.end_row();
                let best = self.laps.iter().filter(|l| l.valid && !l.pit).map(|l| l.time).fold(f64::INFINITY, f64::min);
                for l in &self.laps {
                    let dim = !l.valid || l.pit;
                    let color = if (l.time - best).abs() < 1e-9 && !dim { OK } else if dim { MUTED } else { TEXT };
                    let tags = format!("{}{}{}", l.lap, if l.pit { "  stand" } else { "" }, if !l.valid { "  invalide" } else { "" });
                    let cell = |ui: &mut Ui, s: String| {
                        ui.label(RichText::new(s).font(num_font(12.5)).color(color));
                    };
                    cell(ui, tags);
                    cell(ui, draw::lap(Some(l.time)));
                    cell(ui, draw::n(l.fuel, 2));
                    cell(ui, draw::n(l.energy, 1));
                    cell(ui, draw::n(l.wear, 2));
                    cell(ui, draw::n(Some(l.wear_end), 1));
                    cell(ui, draw::n(Some(l.tyre_temp), 0));
                    cell(ui, draw::n(Some(l.brake_temp), 0));
                    cell(ui, draw::n(Some(l.track_temp), 0));
                    cell(ui, if l.rain > 0.01 { format!("{:.0} %", l.rain * 100.0) } else { "—".into() });
                    cell(ui, l.place.to_string());
                    cell(ui, l.compound.clone());
                    ui.end_row();
                }
            });
        });
    }
}
