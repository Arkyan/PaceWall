//! Replay d'une course : la carte en grand en haut (avec la fiche des voitures suivies), et en dessous le classement
//! dynamique, les relais de chaque voiture ou la télémétrie du joueur.
//! Rien ne tourne en dehors de cette page : la lecture se fait à la demande, image par image.

use super::canvas::EguiCanvas;
use super::kit;
use crate::draw::{self, D};
use crate::theme::*;
use crate::widgets::{self, WState};
use eframe::egui::{self, vec2, Align2, Color32, Frame, Key, Margin, Rect, RichText, Sense, Stroke, StrokeKind, Ui};
use egui_plot::{Line, Plot, PlotPoints, Points, VLine};
use pwcore::layout::{self, WidgetSpec};
use pwcore::replay::{Replay, Row};
use serde_json::{json, Map, Value};
use std::time::Duration;

const SPEEDS: [f64; 6] = [1.0, 2.0, 4.0, 8.0, 16.0, 32.0];
const ROW_H: f32 = 28.0;
const PURPLE: Color32 = Color32::from_rgb(168, 92, 242);
/// Couleurs des deux voitures suivies (les mêmes que leurs anneaux sur la carte).
const COLOR_A: Color32 = WARN;
const COLOR_B: Color32 = Color32::from_rgb(214, 104, 255);

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Standings,
    Stints,
    Telemetry,
}

/// Une courbe de télémétrie : (nom, couleur, points).
type Curve = (&'static str, Color32, Vec<[f64; 2]>);

pub struct View {
    replay: Replay,
    map: WidgetSpec,
    map_state: WState,
    t: f64,
    playing: bool,
    speed: usize,
    tab: Tab,
    /// Classe affichée dans le tableau (`None` = classement général).
    class: Option<String>,
    /// Voitures suivies (identité dans le replay) : la principale, et celle à laquelle on la compare.
    focus: Option<u8>,
    focus_b: Option<u8>,
    /// Tri du tableau : (colonne, décroissant). `None` = ordre du classement.
    sort: Option<(usize, bool)>,
    /// Courbes de la voiture du joueur, calculées une fois à l'ouverture.
    telemetry: Vec<(&'static str, Vec<Curve>)>,
}

fn clock(s: f64) -> String {
    let s = s.max(0.0) as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s % 3600 / 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

/// Retard sur une voiture : en secondes, ou en tours si on a au moins un tour de moins.
fn gap_text(secs: f64, laps_down: u16) -> String {
    if laps_down > 0 {
        format!("+{laps_down} T")
    } else if secs > 0.0 {
        format!("+{secs:.1}")
    } else {
        "—".into()
    }
}

fn lap_text(v: f64) -> String {
    draw::lap((v > 0.0).then_some(v))
}

fn places_text(r: &Row) -> (String, Color32) {
    match r.start_place as i32 - r.place as i32 {
        0 => ("=".to_string(), MUTED),
        g if g > 0 => (format!("+{g}"), OK),
        g => (format!("−{}", -g), BAD),
    }
}

fn plot(id: &str, w: f32, h: f32) -> Plot<'_> {
    Plot::new(id).width(w).height(h).allow_zoom(false).allow_drag(false).allow_scroll(false).allow_boxed_zoom(false).show_grid(true)
}

impl View {
    pub fn open(session_id: i64) -> Result<Self, String> {
        let replay = Replay::load(&pwcore::replay::path_of(session_id)).map_err(|e| e.to_string())?;
        if replay.is_empty() {
            return Err("Ce replay est vide.".into());
        }
        // La carte reprend les réglages de ton widget (numéros, secteurs, orientation…) ; seule sa zone s'adapte à la page.
        let mut opts: Map<String, Value> = layout::load()
            .targets
            .get("player")
            .and_then(|l| l.widgets.iter().find(|w| w.kind == "trackmap"))
            .map(|w| w.opts.clone())
            .unwrap_or_default();
        opts.insert("bg".into(), json!(0));
        // Les positions du replay sont déjà interpolées : la carte ne doit pas prolonger le mouvement (sinon, après un
        // retour en arrière, les voitures à l'arrêt restent décalées).
        opts.insert("extrapolate".into(), json!(false));
        opts.entry("sectors").or_insert(json!(true));
        opts.entry("numbers").or_insert(json!("class"));
        let map = WidgetSpec { id: "replay-map".into(), kind: "trackmap".into(), x: 0.0, y: 0.0, scale: 1.0, opacity: 1.0, opts };

        let wheels = [("AVG", "fl", ui_accent()), ("AVD", "fr", BLUE), ("ARG", "rl", OK), ("ARD", "rr", WARN)];
        let per_wheel = |field: &str| -> Vec<Curve> { wheels.iter().map(|(name, w, c)| (*name, *c, replay.series(&format!("tyre.{w}.{field}")))).collect() };
        let telemetry = vec![
            ("Carburant (L)", vec![("Carburant", BLUE, replay.series("fuel.level"))]),
            ("Énergie virtuelle (%)", vec![("Énergie", OK, replay.series("energy.virtual"))]),
            ("État des pneus (%)", per_wheel("wear")),
            ("Température de carcasse (°C)", per_wheel("carcass")),
        ];
        let mut view = Self { replay, map, map_state: WState::default(), t: 0.0, playing: true, speed: 2, tab: Tab::Standings, class: None, focus: None, focus_b: None, sort: None, telemetry };
        // Développement : état de départ imposé (PACEWALL_REPLAY = "instant,onglet,voiture suivie,voiture comparée").
        if let Ok(spec) = std::env::var("PACEWALL_REPLAY") {
            let mut parts = spec.split(',').map(|s| s.trim().parse::<f64>().ok());
            if let Some(Some(t)) = parts.next() {
                view.t = t;
                view.playing = false;
            }
            view.tab = match parts.next().flatten().map(|v| v as i32) {
                Some(1) => Tab::Stints,
                Some(2) => Tab::Telemetry,
                _ => Tab::Standings,
            };
            view.focus = parts.next().flatten().map(|v| v as u8);
            view.focus_b = parts.next().flatten().map(|v| v as u8);
        }
        Ok(view)
    }

    /// Raccourcis : espace = lecture/pause, ←/→ = 10 s (1 min avec Maj), ↑/↓ = vitesse, Début/Fin,
    /// Page préc./suiv. = tour précédent/suivant de la voiture suivie (ou du joueur).
    fn keyboard(&mut self, ui: &Ui, rows: &[Row], duration: f64) {
        if ui.memory(|m| m.focused().is_some()) {
            return;
        }
        let (space, left, right, up, down, home, end, prev, next, shift) = ui.input(|i| {
            (i.key_pressed(Key::Space), i.key_pressed(Key::ArrowLeft), i.key_pressed(Key::ArrowRight), i.key_pressed(Key::ArrowUp), i.key_pressed(Key::ArrowDown), i.key_pressed(Key::Home), i.key_pressed(Key::End), i.key_pressed(Key::PageUp), i.key_pressed(Key::PageDown), i.modifiers.shift)
        });
        let step = if shift { 60.0 } else { 10.0 };
        if space {
            if self.t >= duration {
                self.t = 0.0;
            }
            self.playing = !self.playing;
        }
        if left {
            self.t -= step;
        }
        if right {
            self.t += step;
        }
        if up {
            self.speed = (self.speed + 1).min(SPEEDS.len() - 1);
        }
        if down {
            self.speed = self.speed.saturating_sub(1);
        }
        if home {
            self.t = 0.0;
        }
        if end {
            self.t = duration;
            self.playing = false;
        }
        if prev || next {
            let car = self.focus.or_else(|| rows.iter().find(|r| r.is_player).map(|r| r.idx));
            if let Some(idx) = car {
                let laps = self.replay.laps_of(idx);
                let target = if next { laps.iter().map(|l| l.t).find(|t| *t > self.t + 0.5) } else { laps.iter().map(|l| l.t).filter(|t| *t < self.t - 2.0).next_back() };
                self.t = target.unwrap_or(if next { duration } else { 0.0 });
            }
        }
        self.t = self.t.clamp(0.0, duration);
    }

    /// Dessine la page. Renvoie `true` quand on demande à revenir à l'historique.
    pub fn show(&mut self, ui: &mut Ui) -> bool {
        let mut back = false;
        let duration = self.replay.duration();
        let (dt, now) = ui.input(|i| (i.stable_dt as f64, i.time));
        if self.playing {
            self.t = (self.t + dt.min(0.1) * SPEEDS[self.speed]).min(duration);
            if self.t >= duration {
                self.playing = false;
            }
            ui.ctx().request_repaint_after(Duration::from_millis(40));
        }
        let rows = self.replay.rows_at(self.t);
        self.keyboard(ui, &rows, duration);

        let bar = Frame::NONE.fill(PANEL).stroke(Stroke::new(1.0, LINE)).inner_margin(Margin::symmetric(14, 8));
        egui::Panel::top("replay_top").frame(bar).show(ui, |ui| {
            ui.horizontal(|ui| {
                if kit::button(ui, "Historique").clicked() {
                    back = true;
                }
                ui.add_space(10.0);
                let h = &self.replay.header;
                ui.label(RichText::new(format!("{} · {}", h.track, h.car)).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new("Espace : pause · Flèches gauche/droite : 10 s (Maj : 1 min) · Haut/bas : vitesse · Page préc./suiv. : tour").size(11.5).color(MUTED));
                });
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let label = if self.playing { "Pause" } else if self.t >= duration { "Rejouer" } else { "Lecture" };
                if kit::primary(ui, label).clicked() {
                    if self.t >= duration {
                        self.t = 0.0;
                    }
                    self.playing = !self.playing;
                }
                ui.add_space(8.0);
                ui.label(RichText::new("VITESSE").size(11.0).color(MUTED));
                for (i, s) in SPEEDS.iter().enumerate() {
                    if ui.selectable_label(self.speed == i, format!("×{s}")).clicked() {
                        self.speed = i;
                    }
                }
                ui.add_space(10.0);
                ui.label(RichText::new(format!("{} / {}", clock(self.t), clock(duration))).font(num_font(13.0)));
            });
            ui.spacing_mut().slider_width = (ui.available_width() - 20.0).max(100.0);
            if ui.add(egui::Slider::new(&mut self.t, 0.0..=duration).show_value(false)).changed() {
                // On a pris la barre : on se met en pause pour que l'image ne parte pas tout de suite.
                self.playing = false;
            }
        });

        egui::CentralPanel::default().frame(Frame::NONE.fill(BG).inner_margin(Margin::same(16))).show(ui, |ui| {
            // Une voiture suivie qui a quitté le classement n'est plus suivie.
            for f in [&mut self.focus, &mut self.focus_b] {
                if f.is_some_and(|i| !rows.iter().any(|r| r.idx == i)) {
                    *f = None;
                }
            }
            let width = ui.available_width();
            let top_h = (ui.available_height() * 0.46).clamp(200.0, 520.0);
            let (top, _) = ui.allocate_exact_size(vec2(width, top_h), Sense::hover());
            // La fiche des voitures suivies prend le tiers droit ; sinon la carte occupe toute la largeur.
            let card_w = if self.focus.is_some() { (width * 0.36).clamp(300.0, 520.0) } else { 0.0 };
            let map_rect = Rect::from_min_max(top.min, egui::pos2(top.max.x - card_w - if card_w > 0.0 { 10.0 } else { 0.0 }, top.max.y));
            self.draw_map(ui, map_rect, &rows, now);
            if card_w > 0.0 {
                self.card(ui, Rect::from_min_max(egui::pos2(top.max.x - card_w, top.min.y), top.max), &rows);
            }

            ui.add_space(10.0);
            self.tabs(ui, &rows);
            ui.add_space(6.0);
            match self.tab {
                Tab::Standings => self.table(ui, &rows),
                Tab::Stints => self.stints(ui, &rows, duration),
                Tab::Telemetry => self.telemetry(ui),
            }
        });
        back
    }

    /// La carte, avec l'état de la course en haut à gauche.
    fn draw_map(&mut self, ui: &mut Ui, rect: Rect, rows: &[Row], now: f64) {
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, PANEL);
        painter.rect_stroke(rect, 0.0, Stroke::new(1.0, LINE), StrokeKind::Inside);

        // Zone de la carte : toute la place disponible (ses proportions suivent celles du cadre), le circuit s'y adapte.
        let inner = rect.shrink(10.0);
        let h_units = 400.0_f32;
        let w_units = (h_units * inner.width() / inner.height()).clamp(100.0, 1000.0);
        let scale = (inner.height() / h_units).min(inner.width() / w_units);
        // La zone est arrondie : sans ça, l'orientation automatique serait recalculée à chaque pixel de redimensionnement.
        self.map.opts.insert("size".into(), json!((w_units / 10.0).round() * 10.0));
        self.map.opts.insert("height".into(), json!(h_units));
        let name_of = |idx: Option<u8>| idx.and_then(|i| rows.iter().find(|r| r.idx == i)).map(|r| r.name.clone()).unwrap_or_default();
        self.map.opts.insert("focus".into(), json!(name_of(self.focus)));
        self.map.opts.insert("focus2".into(), json!(name_of(self.focus_b)));
        let origin = inner.center() - vec2(w_units, h_units) * scale / 2.0;
        let data = self.replay.data_at(self.t);
        {
            let mut canvas = EguiCanvas { painter: &painter };
            let d = D::new(&mut canvas, (origin.x, origin.y), scale, 1.0, 0.0).styled(widgets::accent(&self.map), false).titled(false);
            widgets::draw("trackmap", &d, &self.map, &data, &mut self.map_state, now);
        }

        // État de la course.
        let Some(s) = self.replay.status_at(self.t) else { return };
        let max = self.replay.header.max_laps;
        let race = self.replay.header.session_type >= 10;
        let lap = if max > 0 && max < 10_000 { format!("Tour {} / {}", (s.leader_laps as i32 + 1).min(max), max) } else { format!("Tour {}", s.leader_laps + 1) };
        let (state, color) = match (s.phase, s.yellow) {
            (8, _) => ("Terminée", MUTED),
            (7, _) => ("Course arrêtée", BAD),
            (6, _) | (_, 1..=4) => ("Full course yellow", WARN),
            (5, _) if s.sectors & 7 != 0 => ("Jaune local", WARN),
            _ => ("Drapeau vert", OK),
        };
        let mut y = rect.min.y + 14.0;
        let x = rect.min.x + 16.0;
        painter.text(egui::pos2(x, y), Align2::LEFT_TOP, if race { lap } else { "Session".to_string() }, num_font(20.0), TEXT);
        y += 30.0;
        painter.text(egui::pos2(x, y), Align2::LEFT_TOP, format!("Temps restant  {}", draw::hms(Some(s.remaining))), num_font(12.5), MUTED);
        y += 20.0;
        painter.text(egui::pos2(x, y), Align2::LEFT_TOP, state.to_uppercase(), ui_font(12.0), color);
        y += 22.0;
        painter.text(egui::pos2(x, y), Align2::LEFT_TOP, format!("Air {:.0}°  Piste {:.0}°  Pluie {:.0} %", s.ambient, s.track_temp, s.rain), num_font(12.0), MUTED);
    }

    /// Fiche de la voiture suivie, ou comparaison des deux voitures suivies.
    fn card(&mut self, ui: &mut Ui, rect: Rect, rows: &[Row]) {
        ui.painter().rect_filled(rect, 0.0, PANEL);
        ui.painter().rect_stroke(rect, 0.0, Stroke::new(1.0, LINE), StrokeKind::Inside);
        let find = |idx: Option<u8>| idx.and_then(|i| rows.iter().find(|r| r.idx == i));
        let Some(a) = find(self.focus) else { return };
        let b = find(self.focus_b);
        let mut clear = false;
        ui.scope_builder(egui::UiBuilder::new().max_rect(rect.shrink(14.0)), |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&a.name).size(16.0).strong().color(COLOR_A));
                if let Some(b) = b {
                    ui.label(RichText::new("contre").color(MUTED));
                    ui.label(RichText::new(&b.name).size(16.0).strong().color(COLOR_B));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if kit::button(ui, "Fermer").clicked() {
                        clear = true;
                    }
                });
            });
            // Les statistiques : une colonne par voiture.
            let stat = |r: &Row| -> [String; 8] {
                // Tours invalidés déjà bouclés à cet instant du replay.
                let invalid = self.replay.laps_of(r.idx).iter().filter(|l| !l.valid && l.t <= self.t).count();
                [format!("P{}", r.place), r.class.clone(), format!("{}", r.laps), format!("{invalid}"), lap_text(r.best), lap_text(r.last), format!("{}", r.stops), places_text(r).0]
            };
            let labels = ["Position", "Classe", "Tours", "Tours non valides", "Meilleur tour", "Dernier tour", "Arrêts aux stands", "Places gagnées"];
            let (sa, sb) = (stat(a), b.map(stat));
            ui.add_space(4.0);
            egui::Grid::new("replay_card").num_columns(3).spacing([18.0, 3.0]).show(ui, |ui| {
                for (i, label) in labels.iter().enumerate() {
                    ui.label(RichText::new(label.to_uppercase()).size(10.5).color(MUTED));
                    ui.label(RichText::new(&sa[i]).font(num_font(12.5)));
                    if let Some(sb) = &sb {
                        ui.label(RichText::new(&sb[i]).font(num_font(12.5)));
                    }
                    ui.end_row();
                }
            });
            if let Some(b) = b {
                // Écart entre les deux : différence de leurs retards sur le leader (même tour), sinon en tours.
                let text = if a.laps != b.laps {
                    let (front, n) = if a.laps > b.laps { (&a.name, a.laps - b.laps) } else { (&b.name, b.laps - a.laps) };
                    format!("{front} a {n} tour(s) d'avance")
                } else {
                    let gap = b.gap_leader - a.gap_leader;
                    format!("{} devant de {:.1} s", if gap >= 0.0 { &a.name } else { &b.name }, gap.abs())
                };
                ui.add_space(2.0);
                ui.label(RichText::new(text).color(TEXT));
            } else {
                ui.add_space(2.0);
                ui.label(RichText::new("Ctrl + clic sur une autre ligne pour comparer.").size(11.5).color(MUTED));
            }

            // Chronos tour par tour (les tours passés par les stands sont laissés de côté pour garder l'échelle lisible).
            ui.add_space(6.0);
            let series = |idx: u8| -> Vec<[f64; 2]> { self.replay.laps_of(idx).iter().filter(|l| !l.pit && l.time > 0.0).map(|l| [l.lap as f64, l.time]).collect() };
            let (pa, pb) = (series(a.idx), b.map(|b| series(b.idx)));
            let (w, h) = (ui.available_width(), ui.available_height().max(60.0));
            if pa.is_empty() && pb.as_ref().is_none_or(|p| p.is_empty()) {
                ui.label(RichText::new("Pas encore de tour bouclé dans ce replay.").color(MUTED));
            } else {
                let (cur_a, cur_b) = (a.laps as f64, b.map(|b| b.laps as f64));
                plot("replay_laps", w, h).show(ui, |p| {
                    for (name, pts, color, cur) in [(a.name.as_str(), Some(&pa), COLOR_A, Some(cur_a)), (b.map_or("", |b| b.name.as_str()), pb.as_ref(), COLOR_B, cur_b)] {
                        let Some(pts) = pts.filter(|p| !p.is_empty()) else { continue };
                        p.line(Line::new(name, PlotPoints::from(pts.clone())).color(color).width(2.0));
                        p.points(Points::new(name, PlotPoints::from(pts.clone())).color(color).radius(2.5));
                        if let Some(c) = cur {
                            p.vline(VLine::new(name, c).color(color.gamma_multiply(0.45)).width(1.0));
                        }
                    }
                });
            }
        });
        if clear {
            self.focus = None;
            self.focus_b = None;
        }
    }

    fn tabs(&mut self, ui: &mut Ui, rows: &[Row]) {
        let mut classes: Vec<String> = Vec::new();
        for r in rows {
            if !r.class.is_empty() && !classes.contains(&r.class) {
                classes.push(r.class.clone());
            }
        }
        if classes.len() < 2 {
            self.class = None;
        }
        ui.horizontal(|ui| {
            for (tab, name) in [(Tab::Standings, "CLASSEMENT"), (Tab::Stints, "RELAIS ET STANDS"), (Tab::Telemetry, "MA TÉLÉMÉTRIE")] {
                if ui.selectable_label(self.tab == tab, name).clicked() {
                    self.tab = tab;
                }
            }
            if self.tab != Tab::Telemetry && classes.len() >= 2 {
                ui.add_space(16.0);
                ui.label(RichText::new("CLASSE").size(11.0).color(MUTED));
                if ui.selectable_label(self.class.is_none(), "TOUTES").clicked() {
                    self.class = None;
                }
                for c in classes {
                    if ui.selectable_label(self.class.as_deref() == Some(c.as_str()), c.to_uppercase()).clicked() {
                        self.class = Some(c);
                    }
                }
            }
            if self.tab == Tab::Standings && self.focus.is_none() {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new("Clique une ligne pour suivre une voiture · un titre de colonne pour trier").size(11.5).color(MUTED));
                });
            }
        });
    }

    fn table(&mut self, ui: &mut Ui, rows: &[Row]) {
        let race = self.replay.header.session_type >= 10;
        // (titre, poids de largeur, aligné à droite)
        let cols: Vec<(&str, f32, bool)> = if race {
            vec![("POS", 0.55, false), ("PILOTE", 2.6, false), ("CLASSE", 1.2, false), ("TOURS", 0.8, true), ("ÉCART LEADER", 1.4, true), ("DEVANT", 1.1, true), ("DERNIER", 1.3, true), ("MEILLEUR", 1.3, true), ("ARRÊTS", 0.8, true), ("+/−", 0.7, true)]
        } else {
            vec![("POS", 0.55, false), ("PILOTE", 2.8, false), ("CLASSE", 1.3, false), ("TOURS", 0.8, true), ("MEILLEUR", 1.4, true), ("ÉCART", 1.2, true), ("DERNIER", 1.4, true), ("ARRÊTS", 0.8, true)]
        };

        // Lignes affichées : le général, ou une classe (les écarts sont alors recalculés entre voitures de la classe).
        let shown: Vec<&Row> = rows.iter().filter(|r| self.class.as_ref().is_none_or(|c| &r.class == c)).collect();
        let leader_laps = shown.first().map_or(0, |r| r.laps);
        let leader_gap = shown.first().map_or(0.0, |r| r.gap_leader);
        let overall_best = shown.iter().map(|r| r.best).filter(|b| *b > 0.0).fold(f64::INFINITY, f64::min);
        let by_class = self.class.is_some();

        // Chaque ligne : ses cellules (texte, couleur) et, par colonne, la valeur qui sert au tri.
        struct LineOut<'a> {
            row: &'a Row,
            rank: usize,
            cells: Vec<(String, Color32)>,
            keys: Vec<f64>,
        }
        let far = |v: f64| if v > 0.0 { v } else { f64::MAX };
        let mut lines: Vec<LineOut> = shown
            .iter()
            .enumerate()
            .map(|(n, r)| {
                let down = leader_laps.saturating_sub(r.laps);
                let prev = if n > 0 { Some(shown[n - 1]) } else { None };
                let last_ok = r.last > 0.0 && r.best > 0.0 && (r.last - r.best).abs() < 1e-3;
                let best_color = if r.best > 0.0 && (r.best - overall_best).abs() < 1e-3 { PURPLE } else { TEXT };
                let (gain, gain_color) = places_text(r);
                // Écart avec la voiture de devant : celui du jeu au général, calculé entre voitures de la classe sinon.
                let interval_secs = if !by_class { r.gap_next } else { prev.filter(|p| p.laps == r.laps).map_or(0.0, |p| r.gap_leader - p.gap_leader) };
                let interval = match prev {
                    Some(p) if by_class && p.laps != r.laps => gap_text(0.0, p.laps.saturating_sub(r.laps)),
                    _ => gap_text(interval_secs, 0),
                };
                let leader_secs = if by_class { (r.gap_leader - leader_gap).max(0.0) } else { r.gap_leader };
                let leader_text = if n == 0 { "Leader".to_string() } else { gap_text(leader_secs, down) };
                let best_gap = if r.best > 0.0 && overall_best.is_finite() { format!("+{:.3}", r.best - overall_best) } else { "—".into() };
                let place = if by_class { n + 1 } else { r.place as usize };
                let pit = (if r.in_pits { "STAND".to_string() } else { format!("{}", r.stops) }, if r.in_pits { WARN } else { MUTED });
                let last = (lap_text(r.last), if last_ok { PURPLE } else { TEXT });
                let gain_key = r.place as f64 - r.start_place as f64;
                let (cells, keys) = if race {
                    (
                        vec![(format!("{place}"), TEXT), (r.name.clone(), TEXT), (r.class.clone(), MUTED), (format!("{}", r.laps), TEXT), (leader_text, if n == 0 { OK } else { TEXT }), (interval, TEXT), last, (lap_text(r.best), best_color), pit, (gain, gain_color)],
                        vec![n as f64, 0.0, 0.0, -(r.laps as f64), n as f64, far(interval_secs), far(r.last), far(r.best), r.stops as f64, gain_key],
                    )
                } else {
                    (
                        vec![(format!("{place}"), TEXT), (r.name.clone(), TEXT), (r.class.clone(), MUTED), (format!("{}", r.laps), TEXT), (lap_text(r.best), best_color), (if n == 0 { "Référence".into() } else { best_gap }, MUTED), last, pit],
                        vec![n as f64, 0.0, 0.0, -(r.laps as f64), far(r.best), far(r.best), far(r.last), r.stops as f64],
                    )
                };
                LineOut { row: r, rank: n, cells, keys }
            })
            .collect();
        if let Some((col, desc)) = self.sort.filter(|(c, _)| *c < cols.len()) {
            // Pilote et classe se trient par ordre alphabétique, le reste par valeur ; à égalité, l'ordre du classement.
            lines.sort_by(|a, b| {
                let o = if col == 1 || col == 2 { a.cells[col].0.to_lowercase().cmp(&b.cells[col].0.to_lowercase()) } else { a.keys[col].total_cmp(&b.keys[col]) };
                (if desc { o.reverse() } else { o }).then(a.rank.cmp(&b.rank))
            });
        }

        let total_w: f32 = cols.iter().map(|c| c.1).sum();
        let width = ui.available_width() - 14.0;
        let widths: Vec<f32> = cols.iter().map(|c| c.1 / total_w * width).collect();

        // En-tête : un clic trie par cette colonne, un second inverse, un troisième revient au classement.
        let (head, _) = ui.allocate_exact_size(vec2(width, 24.0), Sense::hover());
        ui.painter().rect_filled(head, 0.0, PANEL2);
        let mut x = head.min.x;
        for (i, (title, _, right)) in cols.iter().enumerate() {
            let cell = Rect::from_min_size(egui::pos2(x, head.min.y), vec2(widths[i], head.height()));
            let resp = ui.interact(cell, ui.id().with(("replay_head", i)), Sense::click());
            let active = self.sort.filter(|(c, _)| *c == i);
            if resp.clicked() {
                self.sort = match active {
                    None => Some((i, false)),
                    Some((_, false)) => Some((i, true)),
                    Some((_, true)) => None,
                };
            }
            if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            let label = match active {
                Some((_, false)) => format!("{title} +"),
                Some((_, true)) => format!("{title} −"),
                None => title.to_string(),
            };
            let color = if active.is_some() { ui_accent() } else if resp.hovered() { TEXT } else { MUTED };
            let (pos, al) = if *right { (egui::pos2(x + widths[i] - 10.0, head.center().y), Align2::RIGHT_CENTER) } else { (egui::pos2(x + 10.0, head.center().y), Align2::LEFT_CENTER) };
            ui.painter().text(pos, al, label, ui_font(10.5), color);
            x += widths[i];
        }

        let ctrl = ui.input(|i| i.modifiers.ctrl || i.modifiers.command);
        let mut pick: Option<u8> = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for (n, line) in lines.iter().enumerate() {
                let r = line.row;
                let (rect, resp) = ui.allocate_exact_size(vec2(width, ROW_H), Sense::click());
                if resp.clicked() {
                    pick = Some(r.idx);
                }
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                let p = ui.painter();
                let followed = if self.focus == Some(r.idx) { Some(COLOR_A) } else if self.focus_b == Some(r.idx) { Some(COLOR_B) } else { None };
                if let Some(c) = followed {
                    p.rect_filled(rect, 0.0, c.gamma_multiply(0.16));
                } else if r.is_player {
                    p.rect_filled(rect, 0.0, ui_accent().gamma_multiply(0.14));
                } else if resp.hovered() {
                    p.rect_filled(rect, 0.0, Color32::from_white_alpha(12));
                } else if n % 2 == 1 {
                    p.rect_filled(rect, 0.0, Color32::from_white_alpha(5));
                }
                p.rect_filled(Rect::from_min_size(rect.min, vec2(3.0, ROW_H)), 0.0, widgets::class_color(&r.class));
                let mut x = rect.min.x;
                for (i, ((text, color), (_, _, right))) in line.cells.iter().zip(cols.iter()).enumerate() {
                    let font = if i == 1 || i == 2 { ui_font(13.0) } else { num_font(12.5) };
                    let (pos, al) = if *right { (egui::pos2(x + widths[i] - 10.0, rect.center().y), Align2::RIGHT_CENTER) } else { (egui::pos2(x + 10.0, rect.center().y), Align2::LEFT_CENTER) };
                    // Les noms longs sont coupés à la largeur de la colonne.
                    let clipped = p.with_clip_rect(Rect::from_min_size(egui::pos2(x, rect.min.y), vec2(widths[i], ROW_H)));
                    clipped.text(pos, al, text, font, *color);
                    x += widths[i];
                }
            }
        });
        if let Some(idx) = pick {
            if ctrl && self.focus.is_some() && self.focus != Some(idx) {
                // Ctrl + clic : la voiture à comparer (un second Ctrl + clic la retire).
                self.focus_b = if self.focus_b == Some(idx) { None } else { Some(idx) };
            } else if self.focus == Some(idx) {
                self.focus = self.focus_b.take();
            } else {
                if self.focus_b == Some(idx) {
                    self.focus_b = None;
                }
                self.focus = Some(idx);
            }
        }
    }

    /// Relais et stands : une frise par voiture sur toute la durée du replay. Un clic place la lecture à cet instant.
    fn stints(&mut self, ui: &mut Ui, rows: &[Row], duration: f64) {
        let shown: Vec<&Row> = rows.iter().filter(|r| self.class.as_ref().is_none_or(|c| &r.class == c)).collect();
        let width = ui.available_width() - 14.0;
        let name_w = (width * 0.22).clamp(150.0, 280.0);
        let info_w = 150.0;
        let bar_w = (width - name_w - info_w).max(80.0);
        let (head, _) = ui.allocate_exact_size(vec2(width, 24.0), Sense::hover());
        let p = ui.painter();
        p.rect_filled(head, 0.0, PANEL2);
        p.text(egui::pos2(head.min.x + 10.0, head.center().y), Align2::LEFT_CENTER, "PILOTE", ui_font(10.5), MUTED);
        p.text(egui::pos2(head.min.x + name_w, head.center().y), Align2::LEFT_CENTER, "RELAIS (PLEIN) ET ARRÊTS AUX STANDS (ORANGE) · CLIC = ALLER À CET INSTANT", ui_font(10.5), MUTED);
        p.text(egui::pos2(head.max.x - 10.0, head.center().y), Align2::RIGHT_CENTER, "ARRÊTS · TEMPS AU STAND", ui_font(10.5), MUTED);

        let mut seek = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for (n, r) in shown.iter().enumerate() {
                let (rect, _) = ui.allocate_exact_size(vec2(width, ROW_H), Sense::hover());
                let bar = Rect::from_min_size(egui::pos2(rect.min.x + name_w, rect.min.y + 7.0), vec2(bar_w, ROW_H - 14.0));
                let resp = ui.interact(bar, ui.id().with(("replay_stint", r.idx)), Sense::click_and_drag());
                let p = ui.painter();
                if r.is_player {
                    p.rect_filled(rect, 0.0, ui_accent().gamma_multiply(0.14));
                } else if n % 2 == 1 {
                    p.rect_filled(rect, 0.0, Color32::from_white_alpha(5));
                }
                let class = widgets::class_color(&r.class);
                p.rect_filled(Rect::from_min_size(rect.min, vec2(3.0, ROW_H)), 0.0, class);
                let place = if self.class.is_some() { n + 1 } else { r.place as usize };
                let clipped = p.with_clip_rect(Rect::from_min_size(rect.min, vec2(name_w - 6.0, ROW_H)));
                clipped.text(egui::pos2(rect.min.x + 10.0, rect.center().y), Align2::LEFT_CENTER, format!("{place}  {}", r.name), ui_font(13.0), TEXT);

                // Relais en couleur de classe, arrêts en orange par-dessus.
                let to_x = |t: f64| bar.min.x + (t / duration.max(1.0)).clamp(0.0, 1.0) as f32 * bar.width();
                p.rect_filled(bar, 0.0, class.gamma_multiply(0.55));
                let pits = self.replay.pits_of(r.idx);
                let mut in_pit = 0.0;
                for (a, b) in pits {
                    in_pit += b - a;
                    let x0 = to_x(*a);
                    p.rect_filled(Rect::from_min_max(egui::pos2(x0, bar.min.y - 2.0), egui::pos2(to_x(*b).max(x0 + 2.0), bar.max.y + 2.0)), 0.0, WARN);
                }
                // Tours bouclés : petits traits.
                for l in self.replay.laps_of(r.idx) {
                    let x = to_x(l.t);
                    p.line_segment([egui::pos2(x, bar.min.y), egui::pos2(x, bar.max.y)], Stroke::new(1.0, Color32::from_black_alpha(110)));
                }
                let cursor = to_x(self.t);
                p.line_segment([egui::pos2(cursor, rect.min.y), egui::pos2(cursor, rect.max.y)], Stroke::new(1.5, TEXT));
                p.text(egui::pos2(rect.max.x - 10.0, rect.center().y), Align2::RIGHT_CENTER, format!("{} · {}", pits.len(), clock(in_pit)), num_font(12.5), if pits.is_empty() { MUTED } else { TEXT });

                if let Some(pos) = resp.interact_pointer_pos().filter(|_| resp.clicked() || resp.dragged()) {
                    seek = Some(((pos.x - bar.min.x) / bar.width()).clamp(0.0, 1.0) as f64 * duration);
                }
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
            }
        });
        if let Some(t) = seek {
            self.t = t;
            self.playing = false;
        }
    }

    /// Télémétrie du joueur sur toute la course, avec le repère de lecture.
    fn telemetry(&self, ui: &mut Ui) {
        if self.telemetry.iter().all(|(_, curves)| curves.iter().all(|c| c.2.is_empty())) {
            ui.add_space(20.0);
            ui.label(RichText::new("Ce replay ne contient pas la télémétrie de ta voiture (enregistré avant cette fonction).").color(MUTED));
            return;
        }
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let avail = ui.available_width() - 14.0;
            let cols = if avail >= 900.0 { 2 } else { 1 };
            let gap = 12.0;
            ui.spacing_mut().item_spacing = vec2(gap, gap);
            let w = ((avail - gap * (cols as f32 - 1.0)) / cols as f32 - 30.0).floor().max(160.0);
            for (r, chunk) in self.telemetry.chunks(cols).enumerate() {
                ui.horizontal(|ui| {
                    for (j, (title, curves)) in chunk.iter().enumerate() {
                        kit::panel_frame().show(ui, |ui| {
                            ui.vertical(|ui| {
                                ui.set_width(w);
                                ui.horizontal(|ui| {
                                    kit::section(ui, title);
                                    if curves.len() > 1 {
                                        for (name, color, _) in curves {
                                            ui.label(RichText::new(*name).size(11.0).color(*color));
                                        }
                                    }
                                });
                                plot(&format!("replay_tel_{}", r * cols + j), w, 150.0).x_axis_formatter(|m, _| clock(m.value)).show(ui, |p| {
                                    for (name, color, pts) in curves {
                                        if !pts.is_empty() {
                                            p.line(Line::new(*name, PlotPoints::from(pts.clone())).color(*color).width(1.8));
                                        }
                                    }
                                    p.vline(VLine::new("Lecture", self.t).color(TEXT).width(1.0));
                                });
                            });
                        });
                    }
                });
            }
        });
    }
}
