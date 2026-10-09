//! Widgets natifs : carburant et énergie, pneus, chrono, pilotage, classement, session et météo.
//! Chaque widget se dessine dans un repère de 1080 px de haut (voir `draw::D`) et renvoie sa taille.
//! Même catalogue, mêmes réglages et mêmes clés de données que les widgets HTML d'OBS (`server/overlay/widgets.js`).

use crate::draw::{self, Font, HAlign, D};
use crate::theme::*;
use eframe::egui::Color32;
use pwcore::layout::WidgetSpec;
use pwcore::trackmap::{self, TrackMap};
use pwcore::Data;
use std::collections::VecDeque;

pub const KINDS: [(&str, &str); 7] = [
    ("fuel", "Carburant et énergie"),
    ("tyres", "Pneus"),
    ("timing", "Chrono et écarts"),
    ("inputs", "Pilotage"),
    ("standings", "Classement"),
    ("session", "Session et météo"),
    ("trackmap", "Carte du circuit"),
];

pub fn name(kind: &str) -> &'static str {
    KINDS.iter().find(|(k, _)| *k == kind).map(|(_, n)| *n).unwrap_or("Widget")
}

/* ---------- Réglages ---------- */

pub enum OptKind {
    Bool(bool),
    Num { min: f64, max: f64, step: f64, def: f64 },
    Choice(&'static [(&'static str, &'static str)], &'static str),
}

pub struct Opt {
    pub id: &'static str,
    pub label: &'static str,
    pub kind: OptKind,
}

const NO_OPTS: &[Opt] = &[];

pub fn options(kind: &str) -> &'static [Opt] {
    use OptKind::*;
    match kind {
        "fuel" => &[
            Opt { id: "energy", label: "Afficher l'énergie virtuelle", kind: Bool(true) },
            Opt { id: "margin", label: "Marge de sécurité (tours)", kind: Num { min: 0.0, max: 3.0, step: 0.5, def: 1.0 } },
        ],
        "tyres" => &[
            Opt { id: "pressure", label: "Pression", kind: Choice(&[("kpa", "kPa"), ("bar", "bar"), ("psi", "psi"), ("off", "Masquer")], "kpa") },
            Opt { id: "brakes", label: "Températures des freins", kind: Bool(false) },
            Opt { id: "tempsrc", label: "Température affichée", kind: Choice(&[("carcass", "Carcasse"), ("inner", "Couche interne"), ("surface", "Surface (moyenne)")], "carcass") },
            Opt { id: "tempoffset", label: "Décalage de température (°C)", kind: Num { min: -10.0, max: 10.0, step: 0.5, def: 0.0 } },
            Opt { id: "wearmode", label: "Affichage de l'usure", kind: Choice(&[("state", "État du pneu (100 → 0 %)"), ("used", "Usure (0 → 100 %)")], "state") },
            Opt { id: "unit", label: "Unité de température", kind: Choice(&[("c", "°C"), ("f", "°F")], "c") },
        ],
        "timing" => &[Opt { id: "gaps", label: "Écarts devant / derrière", kind: Bool(true) }],
        "inputs" => &[
            Opt { id: "fps", label: "Fluidité (images/s)", kind: Choice(&[("10", "10"), ("15", "15"), ("20", "20"), ("30", "30")], "20") },
            Opt { id: "graph", label: "Graphe des inputs", kind: Bool(true) },
            Opt { id: "window", label: "Durée du graphe", kind: Choice(&[("5", "5 s"), ("10", "10 s"), ("20", "20 s")], "10") },
            Opt { id: "steering", label: "Volant dans le graphe", kind: Bool(true) },
            Opt { id: "clutch", label: "Embrayage", kind: Bool(false) },
            Opt { id: "mph", label: "Vitesse en mph", kind: Bool(false) },
        ],
        "standings" => &[
            Opt { id: "ahead", label: "Voitures devant", kind: Num { min: 0.0, max: 8.0, step: 1.0, def: 3.0 } },
            Opt { id: "behind", label: "Voitures derrière", kind: Num { min: 0.0, max: 8.0, step: 1.0, def: 3.0 } },
            Opt { id: "columns", label: "Colonne de droite", kind: Choice(&[("auto", "Auto (écarts en course, chronos sinon)"), ("gaps", "Toujours les écarts"), ("laps", "Toujours les chronos")], "auto") },
            Opt { id: "scope", label: "Classement", kind: Choice(&[("overall", "Général"), ("class", "Par classe (ma classe)")], "overall") },
        ],
        "session" => &[Opt { id: "weather", label: "Météo", kind: Bool(true) }],
        "trackmap" => &[
            Opt { id: "size", label: "Taille de la carte", kind: Num { min: 160.0, max: 460.0, step: 10.0, def: 300.0 } },
            Opt { id: "rotate", label: "Rotation", kind: Choice(&[("0", "0°"), ("90", "90°"), ("180", "180°"), ("270", "270°")], "0") },
            Opt { id: "others", label: "Afficher les autres voitures", kind: Bool(true) },
            Opt { id: "fps", label: "Fluidité (images/s)", kind: Choice(&[("10", "10"), ("20", "20"), ("30", "30")], "20") },
            Opt { id: "sectors", label: "Limites des secteurs", kind: Bool(true) },
            Opt { id: "numbers", label: "Numéro dans les pastilles", kind: Choice(&[("none", "Aucun"), ("class", "Place en classe"), ("overall", "Place au général")], "none") },
        ],
        _ => NO_OPTS,
    }
}

/// Couleurs d'accent proposées : (identifiant enregistré, libellé, couleur). Les mêmes sont dans runtime.js.
pub const ACCENTS: &[(&str, &str, Color32)] = &[
    ("cyan", "Cyan", Color32::from_rgb(25, 211, 243)),
    ("white", "Blanc", Color32::from_rgb(232, 237, 244)),
    ("orange", "Orange", Color32::from_rgb(255, 150, 40)),
    ("green", "Vert", Color32::from_rgb(56, 224, 123)),
    ("red", "Rouge", Color32::from_rgb(255, 77, 94)),
    ("violet", "Violet", Color32::from_rgb(168, 112, 255)),
    ("pink", "Rose", Color32::from_rgb(255, 106, 190)),
    ("yellow", "Jaune", Color32::from_rgb(255, 214, 51)),
];

pub fn accent(spec: &WidgetSpec) -> Color32 {
    let id = spec.choice("accent", "cyan");
    ACCENTS.iter().find(|a| a.0 == id).map(|a| a.2).unwrap_or(ACCENT)
}

/// Barre latérale d'accent de la carte (réglage commun).
pub fn bar(spec: &WidgetSpec) -> bool {
    spec.flag("bar", false)
}

/// Titres des widgets (réglage commun) : masqués par défaut.
pub fn titles(spec: &WidgetSpec) -> bool {
    spec.flag("titles", false)
}

/// Opacité du fond en pourcentage : réglage commun à tous les widgets.
pub fn bg(spec: &WidgetSpec) -> f32 {
    (spec.num("bg", 55.0) / 100.0) as f32
}

/// Cadence de rafraîchissement voulue (images par seconde) : elle fixe le rythme de l'overlay.
pub fn hz(kind: &str, spec: &WidgetSpec) -> f32 {
    match kind {
        "inputs" => spec.num("fps", 20.0) as f32,
        "timing" => 4.0,
        "fuel" => 2.0,
        "tyres" => 1.0,
        "standings" => 1.0,
        "trackmap" => spec.num("fps", 20.0) as f32,
        _ => 1.0,
    }
}

/* ---------- État par widget (graphe des inputs) ---------- */

#[derive(Default)]
pub struct WState {
    hist: VecDeque<(f64, [f32; 4])>,
    /// Carte du circuit chargée : (circuit, carte). Rechargée au changement de circuit, ou toutes les 3 s tant qu'elle manque.
    map: Option<(String, Option<TrackMap>)>,
    map_checked: f64,
    /// Dernière position connue de chaque voiture de la carte : (x, y, instant, vitesse x, vitesse y), pour la lisser.
    car_pos: std::collections::HashMap<usize, (f32, f32, f64, f32, f32)>,
}

pub fn draw(kind: &str, d: &D, spec: &WidgetSpec, data: &Data, st: &mut WState, now: f64) -> (f32, f32) {
    match kind {
        "fuel" => fuel(d, spec, data),
        "tyres" => tyres(d, spec, data),
        "timing" => timing(d, spec, data),
        "inputs" => inputs(d, spec, data, st, now),
        "standings" => standings(d, spec, data),
        "session" => session(d, spec, data),
        "trackmap" => trackmap_widget(d, spec, data, st, now),
        _ => (0.0, 0.0),
    }
}

fn lap_color(laps: Option<f64>) -> Color32 {
    match laps {
        Some(l) if l < 3.0 => BAD,
        Some(l) if l < 6.0 => WARN,
        _ => TEXT,
    }
}

fn bar_color(frac: f32) -> Color32 {
    if frac < 0.15 {
        BAD
    } else if frac < 0.30 {
        WARN
    } else {
        OK
    }
}

/* ---------- Carburant et énergie ---------- */

#[allow(clippy::too_many_arguments)]
fn resource(
    d: &D,
    y0: f32,
    w: f32,
    title: &str,
    unit: &str,
    level: Option<f64>,
    pct: Option<f64>,
    per: Option<f64>,
    laps: Option<f64>,
    add: Option<f64>,
    limiting: bool,
    margin: f64,
) {
    let x0 = 16.0;
    // Titre seulement si demandé ; sinon les valeurs, avec leur unité, suffisent.
    let t = if d.titles() { 16.0 } else { 0.0 };
    if d.titles() {
        d.label(x0, y0 + 10.0, title, MUTED);
        if limiting {
            d.label_right(w - 16.0, y0 + 10.0, "limitant", d.accent());
        }
    }
    let top = y0 + 8.0 + t;
    let bw = d.text(x0, top, &draw::n(level, 1), 34.0, TEXT, Font::Num, HAlign::Left);
    d.text(x0 + bw + 6.0, top + 18.0, unit, 13.0, MUTED, Font::Ui, HAlign::Left);
    if let Some(p) = pct {
        d.text(w - 16.0, top + 12.0, &format!("{p:.0} %"), 14.0, MUTED, Font::Num, HAlign::Right);
    }
    let frac = (pct.unwrap_or(0.0) / 100.0) as f32;
    d.bar(x0, top + 44.0, w - 32.0, 8.0, frac, bar_color(frac));

    // Tours restants, consommation par tour, quantité à ajouter : chacun avec son unité.
    let laps_txt = laps.map(|l| format!("{l:.1} tours")).unwrap_or_else(|| "—".into());
    let per_txt = per.map(|p| format!("{p:.2} {unit}/tour")).unwrap_or_else(|| "—".into());
    let add_txt = match (add, per) {
        (Some(a), p) => format!("+{:.1} {unit}", a + margin * p.unwrap_or(0.0)),
        _ => "—".into(),
    };
    let cols = [x0, x0 + 100.0, x0 + 200.0];
    let lim_color = if limiting { d.accent() } else { lap_color(laps) };
    d.text(cols[0], top + 60.0, &laps_txt, 14.0, if limiting { lim_color } else { lap_color(laps) }, Font::Num, HAlign::Left);
    d.text(cols[1], top + 60.0, &per_txt, 14.0, MUTED, Font::Num, HAlign::Left);
    d.text(cols[2], top + 60.0, &add_txt, 14.0, TEXT, Font::Num, HAlign::Left);
}

fn fuel(d: &D, spec: &WidgetSpec, data: &Data) -> (f32, f32) {
    let w = 300.0;
    let energy = spec.flag("energy", true);
    let margin = spec.num("margin", 1.0);
    let block = 92.0 + if d.titles() { 16.0 } else { 0.0 };
    let h = if energy { block * 2.0 + 6.0 } else { block };
    d.card(w, h);
    let lim = data.text("strategy.limiting");
    resource(
        d,
        0.0,
        w,
        "Carburant",
        "L",
        data.num("fuel.level"),
        data.num("fuel.percent"),
        data.num("strategy.fuel_per_lap"),
        data.num("strategy.fuel_laps_left"),
        data.num("strategy.fuel_to_add"),
        lim == "fuel",
        margin,
    );
    if energy {
        d.sep(16.0, block + 1.0, w - 32.0);
        resource(
            d,
            block + 6.0,
            w,
            "Énergie virtuelle",
            "%",
            data.num("energy.virtual"),
            data.num("energy.virtual"),
            data.num("strategy.energy_per_lap"),
            data.num("strategy.energy_laps_left"),
            data.num("strategy.energy_to_add"),
            lim == "energy",
            margin,
        );
    }
    (w, h)
}

/* ---------- Pneus ---------- */

fn tyres(d: &D, spec: &WidgetSpec, data: &Data) -> (f32, f32) {
    let w = 262.0;
    let brakes = spec.flag("brakes", false);
    let fahrenheit = spec.choice("unit", "c") == "f";
    let pressure = spec.choice("pressure", "kpa");
    let source = spec.choice("tempsrc", "carcass");
    let offset = spec.num("tempoffset", 0.0);
    let worn = spec.choice("wearmode", "state") == "used";
    // Titre et gomme seulement si demandés : la grille remonte alors en haut du widget.
    let hdr = if d.titles() { 18.0 } else { 0.0 };
    let base = 10.0 + hdr;
    let h = base + 134.0 + 12.0 + if brakes { 40.0 } else { 0.0 };
    d.card(w, h);
    if d.titles() {
        d.label(16.0, 10.0, "Pneus", MUTED);
        let comp = if !data.text("tyre.front.compound").is_empty() { data.text("tyre.front.compound") } else { data.text("tyre.rear.compound") };
        d.label_right(w - 16.0, 10.0, comp, d.accent());
    }

    let names = ["fl", "fr", "rl", "rr"];
    let cw = (w - 30.0 - 8.0) / 2.0;
    let temp = |c: f64| if fahrenheit { c * 9.0 / 5.0 + 32.0 } else { c };
    for (i, n) in names.iter().enumerate() {
        let x = 16.0 + (i % 2) as f32 * (cw + 8.0);
        let y = base + (i / 2) as f32 * 70.0;
        let t = offset
            + match source {
                "surface" => (data.f(&format!("tyre.{n}.temp_left"), f64::NAN) + data.f(&format!("tyre.{n}.temp_center"), f64::NAN) + data.f(&format!("tyre.{n}.temp_right"), f64::NAN)) / 3.0,
                "inner" => data.f(&format!("tyre.{n}.inner"), f64::NAN),
                _ => data.f(&format!("tyre.{n}.carcass"), f64::NAN),
            };
        let opt = {
            let o = data.f(&format!("tyre.{n}.optimal"), 0.0);
            if o > 0.0 {
                o
            } else {
                88.0
            }
        };
        let wear = data.f(&format!("tyre.{n}.wear"), 0.0);
        // Bleu = froid, vert = fenêtre idéale, rouge = trop chaud.
        let dlt = (t - opt) as f32;
        let hue = if dlt < 0.0 { 130.0 + (-dlt / 25.0).min(1.0) * 80.0 } else { 130.0 - (dlt / 25.0).min(1.0) * 130.0 };
        let tint = if t.is_finite() { hsl(hue, 0.7, 0.52) } else { MUTED };
        d.fill(x, y, cw, 3.0, tint);
        let ttxt = if t.is_finite() { format!("{:.0}°", temp(t)) } else { "—".into() };
        d.text(x + cw / 2.0, y + 8.0, &ttxt, 24.0, TEXT, Font::Num, HAlign::Center);
        // `wear` = état du pneu en % comme le donne le jeu (100 = neuf) ; on peut aussi l'afficher en usure (0 = neuf).
        let shown = if worn { 100.0 - wear } else { wear };
        let wf = (shown / 100.0) as f32;
        d.fill(x + 8.0, y + 42.0, cw - 16.0, 3.0, Color32::from_white_alpha(26));
        d.fill(x + 8.0, y + 42.0, (cw - 16.0) * wf.clamp(0.0, 1.0), 3.0, if wear < 30.0 { BAD } else if wear < 55.0 { WARN } else { OK });
        let p = data.f(&format!("tyre.{n}.pressure"), 0.0);
        let ptxt = match pressure {
            "off" => String::new(),
            _ if p <= 0.0 => String::new(),
            "bar" => format!("{:.2} bar · ", p / 100.0),
            "psi" => format!("{:.1} psi · ", p * 0.145038),
            _ => format!("{p:.0} kPa · "),
        };
        d.text(x + cw / 2.0, y + 49.0, &format!("{ptxt}{shown:.0} %"), 11.0, MUTED, Font::Num, HAlign::Center);
    }
    if brakes {
        let bw = (w - 30.0) / 4.0;
        for (i, n) in names.iter().enumerate() {
            let x = 16.0 + i as f32 * bw;
            d.text(x + bw / 2.0, base + 148.0, &n.to_uppercase(), 10.5, MUTED, Font::Ui, HAlign::Center);
            d.text(x + bw / 2.0, base + 160.0, &format!("{:.0}°", temp(data.f(&format!("brake.{n}.temp"), 0.0))), 14.0, TEXT, Font::Num, HAlign::Center);
        }
    }
    (w, h)
}

/* ---------- Chrono ---------- */

fn timing(d: &D, spec: &WidgetSpec, data: &Data) -> (f32, f32) {
    let w = 280.0;
    let gaps = spec.flag("gaps", true);
    let rows = if gaps { 5 } else { 3 };
    let h = 66.0 + rows as f32 * 22.0 + 8.0;
    d.card(w, h);
    d.fill(16.0, 10.0, 56.0, 38.0, d.accent());
    let place = data.num("position.place").map(|p| format!("P{p:.0}")).unwrap_or_else(|| "P—".into());
    d.text(44.0, 14.0, &place, 28.0, ACCENT_INK, Font::Num, HAlign::Center);
    let delta = data.num("lap.delta_best");
    let dc = match delta {
        Some(v) if v <= 0.0 => OK,
        Some(_) => BAD,
        None => MUTED,
    };
    d.text(w - 16.0, 12.0, &draw::signed(delta, 2), 30.0, dc, Font::Num, HAlign::Right);
    d.sep(16.0, 58.0, w - 32.0);

    let gap = |k: &str| data.num(k).filter(|v| *v > 0.0).map(|v| format!("{v:.1} s")).unwrap_or_else(|| "—".into());
    let mut list: Vec<(&str, String)> = vec![
        ("Tour", data.num("lap.number").map(|v| format!("{v:.0}")).unwrap_or_else(|| "—".into())),
        ("Dernier", draw::lap(data.num("position.last_lap"))),
        ("Meilleur", draw::lap(data.num("position.best_lap"))),
    ];
    if gaps {
        list.push(("Devant", gap("gap.car_ahead")));
        list.push(("Derrière", gap("gap.car_behind")));
    }
    for (i, (l, v)) in list.iter().enumerate() {
        let y = 66.0 + i as f32 * 22.0;
        d.label(16.0, y + 4.0, l, MUTED);
        d.text(w - 16.0, y + 1.0, v, 14.0, TEXT, Font::Num, HAlign::Right);
    }
    (w, h)
}

/* ---------- Pilotage ---------- */

fn inputs(d: &D, spec: &WidgetSpec, data: &Data, st: &mut WState, now: f64) -> (f32, f32) {
    let w = 262.0;
    let graph = spec.flag("graph", true);
    let clutch = spec.flag("clutch", false);
    let steering = spec.flag("steering", true);
    let win = spec.num("window", 10.0).max(1.0);
    let h = if graph { 114.0 + 56.0 + 12.0 } else { 120.0 };
    d.card(w, h);

    let rpm = data.f("player.rpm", 0.0);
    let max = {
        let m = data.f("player.rpm_max", 8000.0);
        if m > 0.0 {
            m
        } else {
            8000.0
        }
    };
    let f = (rpm / max) as f32;
    d.bar(16.0, 12.0, w - 32.0, 6.0, f, if f > 0.95 { BAD } else if f > 0.85 { WARN } else { OK });

    // Pédales : segments verticaux.
    let thr = data.f("player.throttle", 0.0) as f32 / 100.0;
    let brk = data.f("player.brake", 0.0) as f32 / 100.0;
    let clu = data.f("player.clutch", 0.0) as f32 / 100.0;
    let mut pedals = vec![(brk, BAD), (thr, OK)];
    if clutch {
        pedals.push((clu, BLUE));
    }
    for (i, (v, color)) in pedals.iter().enumerate() {
        let x = 16.0 + i as f32 * 17.0;
        let segs = 14;
        for s in 0..segs {
            let y = 28.0 + (segs - 1 - s) as f32 * 6.0;
            let lit = (s as f32 + 1.0) / segs as f32 <= v.clamp(0.0, 1.0) + 0.0001;
            d.fill(x, y, 12.0, 4.0, if lit { *color } else { Color32::from_white_alpha(26) });
        }
    }
    let px = 16.0 + pedals.len() as f32 * 17.0 + 14.0;
    let gear = data.f("player.gear", 0.0) as i64;
    let g = if gear < 0 { "R".to_string() } else if gear == 0 { "N".to_string() } else { gear.to_string() };
    d.text(px + 24.0, 30.0, &g, 54.0, d.accent(), Font::Num, HAlign::Center);
    let kmh = data.f("player.speed", 0.0);
    let (v, unit) = if spec.flag("mph", false) { (kmh * 0.621371, "mph") } else { (kmh, "km/h") };
    let sx = px + 62.0;
    let vw = d.text(sx, 38.0, &format!("{v:.0}"), 24.0, TEXT, Font::Num, HAlign::Left);
    d.text(sx + vw + 4.0, 48.0, unit, 11.0, MUTED, Font::Ui, HAlign::Left);

    if graph {
        // Historique glissant ; un point par image dessinée (le rythme est fixé par `fps`).
        st.hist.push_back((now, [thr, brk, data.f("player.steering", 0.0) as f32, clu]));
        while st.hist.front().is_some_and(|(t, _)| now - t > win + 0.2) {
            st.hist.pop_front();
        }
        let (gx, gy, gw, gh) = (16.0, 120.0, w - 32.0, 52.0);
        d.sep(16.0, 114.0, w - 32.0);
        for k in [0.0, 0.5, 1.0] {
            d.fill(gx, gy + k * (gh - 1.0), gw, 1.0, Color32::from_white_alpha(23));
        }
        let pts = |idx: usize, map: &dyn Fn(f32) -> f32| -> Vec<(f32, f32)> {
            st.hist.iter().map(|(t, v)| (gx + gw - ((now - t) / win) as f32 * gw, map(v[idx]))).collect()
        };
        let pct = |v: f32| gy + gh - 2.0 - v.clamp(0.0, 1.0) * (gh - 4.0);
        if steering {
            d.line(&pts(2, &|v| gy + gh / 2.0 - (v / 100.0).clamp(-1.0, 1.0) * (gh / 2.0 - 2.0)), 1.5, Color32::from_white_alpha(140));
        }
        if clutch {
            d.line(&pts(3, &pct), 2.0, BLUE);
        }
        d.line(&pts(0, &pct), 2.5, OK);
        d.line(&pts(1, &pct), 2.5, BAD);
    }
    (w, h)
}

/* ---------- Classement ---------- */

fn class_color(c: &str) -> Color32 {
    let c = c.to_lowercase();
    if c.contains("hyper") {
        Color32::from_rgb(229, 57, 59)
    } else if c.contains("lmp2") {
        Color32::from_rgb(45, 127, 249)
    } else if c.contains("gt3") {
        Color32::from_rgb(47, 191, 113)
    } else if c.contains("lmp3") {
        Color32::from_rgb(168, 92, 242)
    } else if c.contains("gte") {
        Color32::from_rgb(242, 169, 59)
    } else {
        Color32::from_rgb(123, 132, 148)
    }
}

struct Car {
    place: i64,
    name: String,
    class: String,
    /// Distance totale parcourue (tours × longueur + distance dans le tour), en mètres.
    dist: f64,
    /// Retard sur la voiture juste devant, en secondes (donné par le jeu).
    behind_next: f64,
    best: f64,
    pit: bool,
    me: bool,
}

/// Écart en secondes entre le joueur et la voiture `j` (positif si elle est derrière lui).
/// On additionne les retards « sur la voiture devant » donnés par le jeu ; s'ils manquent, on estime d'après la distance.
fn gap_seconds(all: &[Car], me: usize, j: usize, speed: f64) -> f64 {
    let chain = if j < me {
        -all[j + 1..=me].iter().map(|c| c.behind_next).sum::<f64>()
    } else {
        all[me + 1..=j].iter().map(|c| c.behind_next).sum::<f64>()
    };
    let dist = all[me].dist - all[j].dist;
    if chain.abs() > 0.05 || dist.abs() < 5.0 {
        chain
    } else {
        // Le jeu ne donne rien : distance qui nous sépare / vitesse (au moins 15 m/s pour ne pas exploser à l'arrêt).
        dist / speed.max(15.0)
    }
}

fn standings(d: &D, spec: &WidgetSpec, data: &Data) -> (f32, f32) {
    let w = 300.0;
    let ahead = spec.num("ahead", 3.0) as usize;
    let behind = spec.num("behind", 3.0) as usize;
    let rows = ahead + behind + 1;
    // Ancien réglage « classOnly » conservé pour les mises en page déjà enregistrées.
    let class_only = spec.choice("scope", if spec.flag("classOnly", false) { "class" } else { "overall" }) == "class";
    // Colonne de droite : en course les écarts avec les autres pilotes, aux essais et en qualifs les meilleurs tours.
    let is_race = (10..=13).contains(&(data.f("session.type", -1.0) as i64));
    let show_gaps = match spec.choice("columns", "auto") {
        "gaps" => true,
        "laps" => false,
        _ => is_race,
    };
    // En-tête (titre et nature de la colonne) seulement si demandé.
    let top = if d.titles() { 30.0 } else { 8.0 };
    let h = top + rows as f32 * 24.0 + 10.0;
    d.card(w, h);
    if d.titles() {
        d.label(16.0, 10.0, "Classement", MUTED);
        d.label_right(w - 16.0, 10.0, if show_gaps { "écarts" } else { "meilleur tour" }, d.accent());
    }

    let length = data.f("session.track_length", 0.0).max(1.0);
    let speed = data.f("player.speed", 0.0) / 3.6;
    let n = (data.f("session.vehicles", 0.0) as usize).min(104);
    let mut all: Vec<Car> = (0..n)
        .filter_map(|i| {
            let p = format!("grid.{i:03}");
            let place = data.num(&format!("{p}.place"))? as i64;
            Some(Car {
                place,
                name: data.text(&format!("{p}.driver")).to_string(),
                class: data.text(&format!("{p}.class")).to_string(),
                dist: data.f(&format!("{p}.laps"), 0.0) * length + data.f(&format!("{p}.lap_dist"), 0.0),
                behind_next: data.f(&format!("{p}.behind_next"), 0.0).max(0.0),
                best: data.f(&format!("{p}.best_lap"), 0.0),
                pit: data.flag(&format!("{p}.in_pits")),
                me: data.flag(&format!("{p}.is_player")),
            })
        })
        .collect();
    all.sort_by_key(|c| c.place);
    let me_idx = all.iter().position(|c| c.me);
    // Liste affichée : tous les pilotes ou ceux de ma classe, avec leur rang dans la liste complète pour les écarts.
    let my_class = me_idx.map(|i| all[i].class.clone());
    let shown: Vec<usize> = (0..all.len()).filter(|&i| !class_only || my_class.as_ref().is_none_or(|c| &all[i].class == c)).collect();
    let idx = shown.iter().position(|&i| Some(i) == me_idx).unwrap_or(0);
    let mut start = idx.saturating_sub(ahead);
    start = start.min(shown.len().saturating_sub(rows));
    for (row, &j) in shown.iter().skip(start).take(rows).enumerate() {
        let c = &all[j];
        let y = top + row as f32 * 24.0;
        if c.me {
            d.fill(8.0, y, w - 8.0, 22.0, d.accent().gamma_multiply(0.22));
        }
        d.fill(8.0, y + 2.0, 3.0, 18.0, class_color(&c.class));
        let pos = if class_only { start + row + 1 } else { c.place as usize };
        d.text(42.0, y + 3.0, &pos.to_string(), 14.0, TEXT, Font::Num, HAlign::Right);
        let short = {
            let parts: Vec<&str> = c.name.split_whitespace().collect();
            if parts.len() > 1 {
                format!("{}. {}", parts[0].chars().next().unwrap_or(' '), parts[1..].join(" "))
            } else {
                c.name.clone()
            }
        };
        d.text(50.0, y + 3.0, &short, 14.0, if c.pit { MUTED } else { TEXT }, Font::Ui, HAlign::Left);
        if show_gaps {
            if let (Some(me), false) = (me_idx, c.me) {
                let behind_dist = all[me].dist - c.dist;
                let text = if behind_dist.abs() >= length {
                    // Au moins un tour d'écart : « +1 T » (il est derrière) ou « −1 T » (il est devant).
                    format!("{}{} T", if behind_dist > 0.0 { "+" } else { "−" }, (behind_dist.abs() / length).floor())
                } else {
                    draw::signed(Some(gap_seconds(&all, me, j, speed)), 1)
                };
                d.text(w - 16.0, y + 3.0, &text, 13.0, MUTED, Font::Num, HAlign::Right);
            }
        } else {
            let best = draw::lap((c.best > 0.0).then_some(c.best));
            d.text(w - 16.0, y + 3.0, &best, 13.0, if c.me { TEXT } else { MUTED }, Font::Num, HAlign::Right);
        }
    }
    (w, h)
}

/* ---------- Session et météo ---------- */

fn session_name(t: i64) -> &'static str {
    match t {
        0 => "Test",
        1..=4 => "Essais",
        5..=8 => "Qualifs",
        9 => "Warm-up",
        10..=13 => "Course",
        _ => "",
    }
}

fn session(d: &D, spec: &WidgetSpec, data: &Data) -> (f32, f32) {
    let w = 280.0;
    let weather = spec.flag("weather", true);
    let phase = data.f("session.phase", 5.0) as i64;
    // Secteurs en jaune (1, 2, 3) : le bandeau les nomme.
    let yellow_sectors: Vec<usize> = (1..=3).filter(|s| data.flag(&format!("session.sector_yellow.{s}"))).collect();
    let flag = phase == 6 || !yellow_sectors.is_empty();
    let top = if flag { 36.0 } else { 0.0 };
    // Circuit et type de session seulement si demandés.
    let hdr = if d.titles() { 16.0 } else { 0.0 };
    let h = 10.0 + top + hdr + 46.0 + if weather { 52.0 } else { 6.0 };
    d.card(w, h);
    if flag {
        d.fill(16.0, 10.0, w - 32.0, 24.0, WARN);
        let banner = if phase == 6 {
            "FULL COURSE YELLOW / SAFETY CAR".to_string()
        } else {
            let list: Vec<String> = yellow_sectors.iter().map(|s| format!("S{s}")).collect();
            format!("JAUNE · {}", list.join(" "))
        };
        d.text(w / 2.0, 14.0, &banner, 12.0, Color32::from_rgb(17, 17, 17), Font::Ui, HAlign::Center);
    }
    let y = 10.0 + top;
    if d.titles() {
        d.label(16.0, y, data.text("session.track"), MUTED);
        d.label_right(w - 16.0, y, session_name(data.f("session.type", -1.0) as i64), d.accent());
    }
    d.text(16.0, y + hdr, &draw::hms(data.num("session.remaining")), 34.0, TEXT, Font::Num, HAlign::Left);
    let max = data.f("session.max_laps", 0.0);
    let left = data.num("strategy.race_laps_left");
    let laps = if max > 0.0 && max < 10_000.0 {
        format!("Tour {:.0} / {max:.0}", data.f("lap.number", 0.0))
    } else if let Some(l) = left {
        format!("≈ {l:.0} tours")
    } else {
        String::new()
    };
    d.text(w - 16.0, y + hdr + 14.0, &laps, 14.0, MUTED, Font::Num, HAlign::Right);
    if weather {
        let wy = y + hdr + 50.0;
        d.sep(16.0, wy - 6.0, w - 32.0);
        let rain = data.f("weather.rain", 0.0);
        let cols = [
            ("Air", draw::n(data.num("weather.ambient"), 0) + "°"),
            ("Piste", draw::n(data.num("weather.track"), 0) + "°"),
            ("Pluie", if rain > 1.0 { format!("{rain:.0} %") } else { "—".into() }),
        ];
        for (i, (l, v)) in cols.iter().enumerate() {
            let x = 16.0 + i as f32 * 86.0;
            d.label(x, wy, l, MUTED);
            d.text(x, wy + 12.0, v, 15.0, TEXT, Font::Num, HAlign::Left);
        }
    }
    (w, h)
}

/* ---------- Carte du circuit ---------- */

/// Position d'une voiture sur la carte : (x, -z), repère de TinyPedal.
struct MapCar {
    idx: usize,
    xy: [f32; 2],
    lap_dist: f64,
    class: String,
    me: bool,
    pit: bool,
    place: i64,
    rank: i64,
}

/// Lissage de Chaikin d'une boucle fermée : adoucit les angles des points enregistrés.
fn smooth_loop(p: &[[f32; 2]], rounds: u32) -> Vec<[f32; 2]> {
    let mut cur = p.to_vec();
    for _ in 0..rounds {
        let n = cur.len();
        if n < 3 {
            break;
        }
        let mut next = Vec::with_capacity(n * 2);
        for i in 0..n {
            let (a, b) = (cur[i], cur[(i + 1) % n]);
            next.push([0.75 * a[0] + 0.25 * b[0], 0.75 * a[1] + 0.25 * b[1]]);
            next.push([0.25 * a[0] + 0.75 * b[0], 0.25 * a[1] + 0.75 * b[1]]);
        }
        cur = next;
    }
    cur
}

fn trackmap_widget(d: &D, spec: &WidgetSpec, data: &Data, st: &mut WState, now: f64) -> (f32, f32) {
    let size = spec.num("size", 300.0).clamp(160.0, 460.0) as f32;
    let k = (size / 300.0).clamp(0.7, 1.4); // épaisseur du tracé et taille des voitures suivent la taille
    let w = size;
    let track = data.text("session.track").to_string();

    // Carte : enregistrée pendant un tour propre (voir pwcore::trackmap), lue sur le disque.
    let demo = data.flag("demo.map");
    let key = if demo { "demo".to_string() } else { track.clone() };
    let reload = match &st.map {
        Some((k, m)) => *k != key || (m.is_none() && now - st.map_checked > 3.0),
        None => true,
    };
    if reload {
        st.map_checked = now;
        let loaded = if demo { Some(trackmap::demo()) } else if track.is_empty() { None } else { trackmap::load(&track) };
        st.map = Some((key, loaded));
    }
    let map = st.map.as_ref().and_then(|(_, m)| m.as_ref());

    let angle = (spec.num("rotate", 0.0) as f32).to_radians();
    let (sin, cos) = angle.sin_cos();
    let rot = |p: [f32; 2]| [p[0] * cos - p[1] * sin, p[0] * sin + p[1] * cos];
    let pts: Vec<[f32; 2]> = map.map(|m| m.points.iter().map(|&p| rot(p)).collect()).unwrap_or_default();
    let (mut min, mut max) = ([f32::MAX; 2], [f32::MIN; 2]);
    for p in &pts {
        for a in 0..2 {
            min[a] = min[a].min(p[a]);
            max[a] = max[a].max(p[a]);
        }
    }
    let (bw, bh) = ((max[0] - min[0]).max(1.0), (max[1] - min[1]).max(1.0));

    // La carte prend la forme du circuit : plus de grand vide pour un tracé allongé.
    let (ax, ay, aw) = (24.0, 24.0, w - 48.0);
    let aspect = if pts.is_empty() { 1.0 } else { (bh / bw).clamp(0.5, 1.15) };
    let ah = aw * aspect;
    let h = ay + ah + 24.0;
    if d.measuring() {
        return (w, h);
    }
    d.card(w, h);

    let n = (data.f("session.vehicles", 0.0) as usize).min(104);
    let show_others = spec.flag("others", true);
    let mode = spec.choice("numbers", "none").to_string();
    let mut cars: Vec<MapCar> = (0..n)
        .filter_map(|i| {
            let p = format!("grid.{i:03}");
            let (x, z) = (data.num(&format!("{p}.x"))?, data.num(&format!("{p}.z"))?);
            Some(MapCar {
                idx: i,
                xy: [x as f32, -z as f32],
                lap_dist: data.f(&format!("{p}.lap_dist"), 0.0),
                class: data.text(&format!("{p}.class")).to_string(),
                me: data.flag(&format!("{p}.is_player")),
                pit: data.flag(&format!("{p}.in_pits")),
                place: data.f(&format!("{p}.place"), 0.0) as i64,
                rank: 0,
            })
        })
        .collect();
    // Fluidité : le jeu ne publie la position des voitures que ~5 fois par seconde. Entre deux mesures on prolonge
    // le mouvement à la vitesse observée (au plus 0,25 s), ce qui évite les sauts.
    for c in cars.iter_mut() {
        let e = st.car_pos.entry(c.idx).or_insert((c.xy[0], c.xy[1], now, 0.0, 0.0));
        if (c.xy[0], c.xy[1]) != (e.0, e.1) {
            let dt = (now - e.2) as f32;
            let (dx, dy) = (c.xy[0] - e.0, c.xy[1] - e.1);
            // Saut brutal (retour au stand, téléportation) ou trou dans les mesures : pas de vitesse.
            let (vx, vy) = if dt > 0.005 && dt < 1.0 && dx.abs() + dy.abs() < 150.0 { (dx / dt, dy / dt) } else { (0.0, 0.0) };
            *e = (c.xy[0], c.xy[1], now, vx, vy);
        }
        let ahead = ((now - e.2) as f32).clamp(0.0, 0.25);
        c.xy = [e.0 + e.3 * ahead, e.1 + e.4 * ahead];
    }
    // Place en classe : rang parmi les voitures de la même classe, dans l'ordre du classement général.
    let mut order: Vec<usize> = (0..cars.len()).collect();
    order.sort_by_key(|&i| cars[i].place);
    let mut seen: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    for i in order {
        let c = seen.entry(cars[i].class.clone()).or_insert(0);
        *c += 1;
        cars[i].rank = *c;
    }
    cars.retain(|c| c.me || show_others);
    // Les premiers au-dessus des suivants, le joueur tout en haut.
    cars.sort_by_key(|c| (c.me, -c.place));

    let outline = Color32::from_rgba_unmultiplied(6, 8, 12, 230);
    let dot = |c: &MapCar, x: f32, y: f32| {
        let num = match mode.as_str() {
            "none" => 0,
            "overall" => c.place,
            _ => c.rank,
        };
        let label = (num > 0).then(|| num.to_string());
        if c.me {
            d.circle(x, y, 14.0 * k, Color32::from_rgba_unmultiplied(25, 211, 243, 55), None);
            d.circle(x, y, 9.0 * k, d.accent(), Some((2.0 * k, TEXT)));
            if let Some(l) = &label {
                d.text(x, y - 6.0 * k, l, 12.0 * k, ACCENT_INK, Font::Num, HAlign::Center);
            }
        } else {
            let col = if c.pit { MUTED } else { class_color(&c.class) };
            let r = if label.is_some() { 7.0 * k } else { 4.6 * k };
            d.circle(x, y, r, col, Some((1.4, outline)));
            if let Some(l) = &label {
                d.text(x, y - 4.8 * k, l, 9.5 * k, Color32::WHITE, Font::Num, HAlign::Center);
            }
        }
    };
    // Route : liseré clair, asphalte sombre, filet central.
    let road = |d: &D, line: &[(f32, f32)]| {
        d.line(line, 12.0 * k, Color32::from_rgb(112, 125, 146));
        d.line(line, 8.0 * k, Color32::from_rgb(30, 36, 46));
        d.line(line, 1.2, Color32::from_white_alpha(38));
    };
    // Ligne de départ en travers de la route, et chevron pour le sens de roulage.
    let start = |d: &D, at: (f32, f32), dir: (f32, f32)| {
        let len = (dir.0 * dir.0 + dir.1 * dir.1).sqrt().max(0.001);
        let (tx, ty) = (dir.0 / len, dir.1 / len);
        let (nx, ny) = (-ty * 8.0 * k, tx * 8.0 * k);
        d.line(&[(at.0 - nx, at.1 - ny), (at.0 + nx, at.1 + ny)], 3.5 * k, TEXT);
        let tip = (at.0 + tx * 15.0 * k, at.1 + ty * 15.0 * k);
        let back = 5.0 * k;
        d.line(&[(tip.0 - tx * back - ty * back, tip.1 - ty * back + tx * back), tip, (tip.0 - tx * back + ty * back, tip.1 - ty * back - tx * back)], 2.0, d.accent());
    };

    if map.is_some() && pts.len() > 2 {
        let scale = (aw / bw).min(ah / bh);
        let (ox, oy) = (ax + (aw - bw * scale) / 2.0, ay + (ah - bh * scale) / 2.0);
        let to_screen = |p: [f32; 2]| (ox + (p[0] - min[0]) * scale, oy + (p[1] - min[1]) * scale);

        // Le tracé (route, départ, secteurs) ne change pas : il est dessiné une fois puis recopié à chaque image.
        let tag = {
            use std::hash::{Hash, Hasher};
            let mut hs = std::collections::hash_map::DefaultHasher::new();
            let first = pts.first().map(|p| (p[0].to_bits(), p[1].to_bits()));
            let sectors: Vec<u64> = map.map(|m| m.sectors.iter().map(|s| s.to_bits()).collect()).unwrap_or_default();
            ("map", &st.map.as_ref().map(|(name, _)| name.clone()), pts.len(), first, sectors, spec.flag("sectors", true), w.to_bits(), h.to_bits(), d.cache_tag()).hash(&mut hs);
            hs.finish()
        };
        d.layer(tag, 0.0, 0.0, w, h, &mut |dl| {
            let mut line: Vec<(f32, f32)> = smooth_loop(&pts, 1).into_iter().map(to_screen).collect();
            line.push(line[0]);
            road(dl, &line);
            let (a, b) = (to_screen(pts[0]), to_screen(pts[1]));
            start(dl, a, (b.0 - a.0, b.1 - a.1));
            // Limites des secteurs : un trait en travers de la route et le numéro du secteur au milieu.
            if spec.flag("sectors", true) {
                if let Some(map) = map.filter(|m| m.sectors.len() >= 2) {
                    let n = pts.len();
                    let ticks: Vec<f64> = vec![0.0, map.sectors[0], map.sectors[1], 1.0];
                    for f in map.sectors.iter().take(2) {
                        let i = ((f * n as f64) as usize).min(n - 1);
                        let (p, before, after) = (to_screen(pts[i]), to_screen(pts[(i + n - 2) % n]), to_screen(pts[(i + 2) % n]));
                        let (dx, dy) = (after.0 - before.0, after.1 - before.1);
                        let len = (dx * dx + dy * dy).sqrt().max(0.001);
                        let (nx, ny) = (-dy / len * 7.5 * k, dx / len * 7.5 * k);
                        dl.line(&[(p.0 - nx, p.1 - ny), (p.0 + nx, p.1 + ny)], 2.5 * k, Color32::from_rgb(255, 214, 51));
                    }
                    for s in 0..3 {
                        let mid = (ticks[s] + ticks[s + 1]) / 2.0;
                        let i = ((mid * n as f64) as usize).min(n - 1);
                        let (p, before, after) = (to_screen(pts[i]), to_screen(pts[(i + n - 2) % n]), to_screen(pts[(i + 2) % n]));
                        let (dx, dy) = (after.0 - before.0, after.1 - before.1);
                        let len = (dx * dx + dy * dy).sqrt().max(0.001);
                        // Le numéro est posé du côté extérieur de la route, hors du tracé.
                        let (nx, ny) = (-dy / len * 17.0 * k, dx / len * 17.0 * k);
                        let (cx, cy) = (ox + bw * scale / 2.0, oy + bh * scale / 2.0);
                        let sign = if (p.0 + nx - cx).powi(2) + (p.1 + ny - cy).powi(2) > (p.0 - nx - cx).powi(2) + (p.1 - ny - cy).powi(2) { 1.0 } else { -1.0 };
                        dl.text(p.0 + sign * nx, p.1 + sign * ny - 5.5 * k, &format!("S{}", s + 1), 10.0 * k, Color32::from_rgb(255, 214, 51), Font::Num, HAlign::Center);
                    }
                }
            }
        });
        // Les autres d'abord, le joueur par-dessus.
        for c in &cars {
            let (x, y) = to_screen(rot(c.xy));
            dot(c, x, y);
        }
    } else {
        // Pas encore de carte : un cercle, avec les voitures placées selon leur distance dans le tour.
        let r = (aw.min(ah) / 2.0 - 8.0).max(20.0);
        let (cx, cy) = (ax + aw / 2.0, ay + ah / 2.0);
        let circle: Vec<(f32, f32)> = (0..=72)
            .map(|i| {
                let a = i as f32 / 72.0 * std::f32::consts::TAU;
                (cx + r * a.cos(), cy + r * a.sin())
            })
            .collect();
        {
            use std::hash::{Hash, Hasher};
            let mut hs = std::collections::hash_map::DefaultHasher::new();
            ("circle", w.to_bits(), h.to_bits(), d.cache_tag()).hash(&mut hs);
            d.layer(hs.finish(), 0.0, 0.0, w, h, &mut |dl| {
                road(dl, &circle);
                start(dl, (cx, cy - r), (1.0, 0.0));
            });
        }
        let length = data.f("session.track_length", 0.0).max(1.0);
        for c in &cars {
            let a = -std::f32::consts::FRAC_PI_2 + (c.lap_dist / length) as f32 * std::f32::consts::TAU;
            dot(c, cx + r * a.cos(), cy + r * a.sin());
        }
        d.text(w / 2.0, cy - 6.0, "Un tour propre trace la carte", 10.5, MUTED, Font::Ui, HAlign::Center);
    }
    (w, h)
}
