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

pub const KINDS: [(&str, &str); 13] = [
    ("fuel", "Carburant et énergie"),
    ("tyres", "Pneus"),
    ("timing", "Chrono et écarts"),
    ("inputs", "Pilotage"),
    ("standings", "Classement"),
    ("session", "Session et météo"),
    ("trackmap", "Carte du circuit"),
    ("flags", "Drapeaux"),
    ("relative", "Relatif"),
    ("pit", "Arrêt au stand"),
    ("radar", "Radar de proximité"),
    ("forecast", "Prévisions météo"),
    ("damage", "État de la voiture"),
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
            Opt { id: "target", label: "Objectif de consommation (+1 tour)", kind: Bool(true) },
        ],
        "tyres" => &[
            Opt { id: "pressure", label: "Pression", kind: Choice(&[("auto", "Unité des réglages"), ("kpa", "kPa"), ("bar", "bar"), ("psi", "psi"), ("off", "Masquer")], "auto") },
            Opt { id: "brakes", label: "Températures des freins", kind: Bool(false) },
            Opt { id: "tempsrc", label: "Température affichée", kind: Choice(&[("carcass", "Carcasse"), ("inner", "Couche interne"), ("surface", "Surface (moyenne)")], "carcass") },
            Opt { id: "tempoffset", label: "Décalage de température (°C)", kind: Num { min: -10.0, max: 10.0, step: 0.5, def: 0.0 } },
            Opt { id: "wearmode", label: "Affichage de l'usure", kind: Choice(&[("state", "État du pneu (100 → 0 %)"), ("used", "Usure (0 → 100 %)")], "state") },
            Opt { id: "unit", label: "Unité de température", kind: Choice(&[("auto", "Unité des réglages"), ("c", "°C"), ("f", "°F")], "auto") },
        ],
        "timing" => &[
            Opt { id: "gaps", label: "Écarts devant / derrière", kind: Bool(true) },
            Opt { id: "place", label: "Place affichée", kind: Choice(&[("class", "En classe"), ("overall", "Au général")], "class") },
        ],
        "flags" => &[
            Opt { id: "anim", label: "Clignotement (jaune, bleu, rouge)", kind: Bool(true) },
            Opt { id: "blueclass", label: "Bleu « LMP » quand une classe plus rapide arrive", kind: Bool(true) },
        ],
        "inputs" => &[
            Opt { id: "fps", label: "Fluidité (images/s)", kind: Choice(&[("10", "10"), ("15", "15"), ("20", "20"), ("30", "30")], "20") },
            Opt { id: "gauges", label: "Régime, rapport et vitesse", kind: Bool(true) },
            Opt { id: "pedals", label: "Pédales (barres)", kind: Bool(true) },
            Opt { id: "graph", label: "Graphe des inputs", kind: Bool(true) },
            Opt { id: "window", label: "Durée du graphe", kind: Choice(&[("5", "5 s"), ("10", "10 s"), ("20", "20 s")], "10") },
            Opt { id: "steering", label: "Volant dans le graphe", kind: Bool(true) },
            Opt { id: "clutch", label: "Embrayage", kind: Bool(false) },
            Opt { id: "mph", label: "Toujours en mph (sinon : unité des réglages)", kind: Bool(false) },
        ],
        "standings" => &[
            Opt { id: "ahead", label: "Voitures devant", kind: Num { min: 0.0, max: 8.0, step: 1.0, def: 3.0 } },
            Opt { id: "behind", label: "Voitures derrière", kind: Num { min: 0.0, max: 8.0, step: 1.0, def: 3.0 } },
            Opt { id: "columns", label: "Colonne de droite", kind: Choice(&[("auto", "Auto (écarts en course, chronos sinon)"), ("gaps", "Toujours les écarts"), ("laps", "Toujours les chronos")], "auto") },
            Opt { id: "scope", label: "Classement", kind: Choice(&[("overall", "Général"), ("class", "Par classe (ma classe)")], "overall") },
        ],
        "session" => &[Opt { id: "weather", label: "Météo", kind: Bool(true) }],
        "trackmap" => &[
            Opt { id: "size", label: "Largeur de la zone", kind: Num { min: 100.0, max: 1000.0, step: 10.0, def: 300.0 } },
            Opt { id: "height", label: "Hauteur de la zone", kind: Num { min: 100.0, max: 1000.0, step: 10.0, def: 220.0 } },
            Opt { id: "rotate", label: "Orientation", kind: Choice(&[("auto", "Auto (circuit le plus grand)"), ("0", "0°"), ("90", "90°"), ("180", "180°"), ("270", "270°")], "auto") },
            Opt { id: "others", label: "Afficher les autres voitures", kind: Bool(true) },
            Opt { id: "fps", label: "Fluidité (images/s)", kind: Choice(&[("10", "10"), ("20", "20"), ("30", "30")], "20") },
            Opt { id: "sectors", label: "Limites des secteurs", kind: Bool(true) },
            Opt { id: "pitexit", label: "Repère de sortie des stands", kind: Choice(&[("request", "Arrêt demandé ou en cours"), ("always", "Toujours"), ("off", "Jamais")], "request") },
            Opt { id: "numbers", label: "Numéro dans les pastilles", kind: Choice(&[("none", "Aucun"), ("class", "Place en classe"), ("overall", "Place au général")], "none") },
        ],
        "relative" => &[
            Opt { id: "ahead", label: "Voitures devant", kind: Num { min: 0.0, max: 6.0, step: 1.0, def: 3.0 } },
            Opt { id: "behind", label: "Voitures derrière", kind: Num { min: 0.0, max: 6.0, step: 1.0, def: 3.0 } },
            Opt { id: "place", label: "Place affichée", kind: Choice(&[("class", "En classe"), ("overall", "Au général")], "class") },
        ],
        "pit" => &[
            Opt { id: "lane", label: "Voie des stands tant que le circuit n'est pas appris (s)", kind: Num { min: 5.0, max: 90.0, step: 1.0, def: 25.0 } },
            Opt { id: "stop", label: "Durée d'arrêt si le jeu ne la donne pas (s)", kind: Num { min: 0.0, max: 120.0, step: 1.0, def: 35.0 } },
            Opt { id: "menu", label: "Afficher le menu des stands du jeu", kind: Bool(true) },
        ],
        "damage" => &[Opt { id: "brakes", label: "Usure des freins", kind: Bool(true) }],
        "radar" => &[
            Opt { id: "range", label: "Rayon affiché (m)", kind: Num { min: 8.0, max: 40.0, step: 1.0, def: 20.0 } },
            Opt { id: "autohide", label: "Masquer quand personne n'est proche", kind: Bool(true) },
            Opt { id: "show", label: "Apparaît quand une voiture est à moins de (m)", kind: Num { min: 10.0, max: 80.0, step: 5.0, def: 35.0 } },
            Opt { id: "fps", label: "Fluidité (images/s)", kind: Choice(&[("15", "15"), ("30", "30"), ("60", "60 (le plus fluide)")], "60") },
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
        "flags" => 4.0,
        "relative" => 4.0,
        "radar" => spec.num("fps", 60.0) as f32,
        _ => 1.0,
    }
}

/// Cadence selon l'état du widget : un panneau de drapeau qui clignote doit être redessiné plus souvent qu'un
/// panneau fixe (ou vide).
pub fn hz_state(kind: &str, spec: &WidgetSpec, st: &WState) -> f32 {
    if kind == "flags" && st.animating {
        15.0
    } else {
        hz(kind, spec)
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
    /// Dernière phase de course vue et instant jusqu'auquel le drapeau vert reste allumé (après une relance).
    flag_phase: Option<i64>,
    green_until: f64,
    /// Orientation automatique de la carte, calculée une fois par circuit et par zone : (circuit, points, zone) → degrés.
    map_angle: Option<((Option<String>, usize, u32, u32), f32)>,
    /// Radar : dernière position du joueur et direction de marche (repère de la carte), pour orienter les voitures voisines.
    radar_last: Option<(f32, f32)>,
    radar_dir: Option<(f32, f32)>,
    /// Radar : (temps du jeu, horloge du widget) à la dernière mesure de la voiture du joueur.
    radar_clock: Option<(f64, f64)>,
    /// Instant d'entrée dans la voie des stands (chronomètre de l'arrêt en cours).
    pit_since: Option<f64>,
    /// Durée d'arrêt retenue (et si elle vient du jeu), figée à l'entrée des stands.
    pit_stop: Option<(f64, bool)>,
    /// Le widget s'anime en ce moment (drapeau qui flotte) : il doit être redessiné même sans nouvelle donnée.
    animating: bool,
}

impl WState {
    pub fn animating(&self) -> bool {
        self.animating
    }
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
        "flags" => flags(d, spec, data, st, now),
        "relative" => relative(d, spec, data),
        "pit" => pit(d, spec, data, st, now),
        "radar" => radar(d, spec, data, st, now),
        "forecast" => forecast(d, spec, data),
        "damage" => damage(d, spec, data),
        _ => (0.0, 0.0),
    }
}

fn lap_color(laps: Option<f64>) -> Color32 {
    let p = crate::prefs::get();
    match laps {
        Some(l) if l < p.fuel_crit => BAD,
        Some(l) if l < p.fuel_warn => WARN,
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
    target: bool,
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
    d.text(w - 16.0, top + 60.0, &add_txt, 14.0, TEXT, Font::Num, HAlign::Right);

    // Objectif : la consommation à tenir pour boucler un tour de plus avec ce qu'il reste.
    if target {
        let text = match (level, per, laps) {
            (Some(lv), Some(p), Some(l)) if p > 0.0 && lv > 0.0 => {
                let goal = lv / (l.floor() + 1.0);
                format!("+1 tour : {goal:.2} {unit}/tour ({:.0} % de moins)", (1.0 - goal / p).max(0.0) * 100.0)
            }
            _ => "+1 tour : —".into(),
        };
        d.text(x0, top + 80.0, &text, 12.0, MUTED, Font::Num, HAlign::Left);
    }
}

fn fuel(d: &D, spec: &WidgetSpec, data: &Data) -> (f32, f32) {
    let w = 300.0;
    let energy = spec.flag("energy", true);
    let margin = spec.num("margin", 1.0);
    let target = spec.flag("target", true);
    let prefs = crate::prefs::get();
    // Litres ou gallons : le niveau, la consommation et la quantité à ajouter sont convertis ensemble.
    let vol = |v: Option<f64>| v.map(|v| prefs.fuel(v).0);
    let block = 92.0 + if d.titles() { 16.0 } else { 0.0 } + if target { 20.0 } else { 0.0 };
    let h = if energy { block * 2.0 + 6.0 } else { block };
    d.card(w, h);
    let lim = data.text("strategy.limiting");
    resource(
        d,
        0.0,
        w,
        "Carburant",
        prefs.fuel(0.0).1,
        vol(data.num("fuel.level")),
        data.num("fuel.percent"),
        vol(data.num("strategy.fuel_per_lap")),
        data.num("strategy.fuel_laps_left"),
        vol(data.num("strategy.fuel_to_add")),
        lim == "fuel",
        margin,
        target,
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
            target,
        );
    }
    (w, h)
}

/* ---------- Pneus ---------- */

fn tyres(d: &D, spec: &WidgetSpec, data: &Data) -> (f32, f32) {
    let w = 262.0;
    let brakes = spec.flag("brakes", false);
    let prefs = crate::prefs::get();
    // « auto » : l'unité choisie dans les réglages ; une unité imposée sur ce widget garde la priorité.
    let fahrenheit = match spec.choice("unit", "auto") {
        "c" => false,
        "f" => true,
        _ => prefs.fahrenheit,
    };
    let pressure = spec.choice("pressure", "auto");
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
        let win = prefs.temp_window as f32;
        let hue = if dlt < 0.0 { 130.0 + (-dlt / win).min(1.0) * 80.0 } else { 130.0 - (dlt / win).min(1.0) * 130.0 };
        let tint = if t.is_finite() { hsl(hue, 0.7, 0.52) } else { MUTED };
        d.fill(x, y, cw, 3.0, tint);
        let ttxt = if t.is_finite() { format!("{:.0}°", temp(t)) } else { "—".into() };
        d.text(x + cw / 2.0, y + 8.0, &ttxt, 24.0, TEXT, Font::Num, HAlign::Center);
        // `wear` = état du pneu en % comme le donne le jeu (100 = neuf) ; on peut aussi l'afficher en usure (0 = neuf).
        let shown = if worn { 100.0 - wear } else { wear };
        let wf = (shown / 100.0) as f32;
        d.fill(x + 8.0, y + 42.0, cw - 16.0, 3.0, Color32::from_white_alpha(26));
        d.fill(x + 8.0, y + 42.0, (cw - 16.0) * wf.clamp(0.0, 1.0), 3.0, if wear < prefs.wear_crit { BAD } else if wear < prefs.wear_warn { WARN } else { OK });
        let p = data.f(&format!("tyre.{n}.pressure"), 0.0);
        let ptxt = match pressure {
            "off" => String::new(),
            _ if p <= 0.0 => String::new(),
            "bar" => format!("{:.2} bar · ", p / 100.0),
            "psi" => format!("{:.1} psi · ", p * 0.145038),
            "kpa" => format!("{p:.0} kPa · "),
            _ => format!("{} · ", prefs.pressure(p)),
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
    // Place en classe par défaut (celle qui compte en multi-classes) ; au général si demandé ou si le classement manque.
    let in_class = if spec.choice("place", "class") == "class" { near_cars(data).iter().find(|c| c.me).map(|c| c.rank as f64) } else { None };
    let place = in_class.or(data.num("position.place")).map(|p| format!("P{p:.0}")).unwrap_or_else(|| "P—".into());
    d.text(44.0, 14.0, &place, 28.0, ACCENT_INK, Font::Num, HAlign::Center);
    let delta = data.num("lap.delta_best");
    let dc = match delta {
        Some(v) if v <= 0.0 => OK,
        Some(_) => BAD,
        None => MUTED,
    };
    d.text(w - 16.0, 12.0, &draw::signed(delta, 2), 30.0, dc, Font::Num, HAlign::Right);
    d.sep(16.0, 58.0, w - 32.0);

    let fmt_gap = |v: Option<f64>| v.filter(|v| *v > 0.0).map(|v| format!("{v:.1} s")).unwrap_or_else(|| "—".into());
    let (ahead, behind) = neighbour_gaps(data);
    let mut list: Vec<(&str, String)> = vec![
        ("Tour", data.num("lap.number").map(|v| format!("{v:.0}")).unwrap_or_else(|| "—".into())),
        ("Dernier", draw::lap(data.num("position.last_lap"))),
        ("Meilleur", draw::lap(data.num("position.best_lap"))),
    ];
    if gaps {
        list.push(("Devant", fmt_gap(ahead)));
        list.push(("Derrière", fmt_gap(behind)));
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
    // Trois blocs au choix : régime / rapport / vitesse, pédales, graphe. Le widget se resserre autour de ceux gardés
    // (si tout est décoché, on garde le premier pour ne pas afficher une carte vide).
    let (pedals_on, graph) = (spec.flag("pedals", true), spec.flag("graph", true));
    let gauges = spec.flag("gauges", true) || (!pedals_on && !graph);
    let clutch = spec.flag("clutch", false);
    let steering = spec.flag("steering", true);
    let win = spec.num("window", 10.0).max(1.0);
    let thr = data.f("player.throttle", 0.0) as f32 / 100.0;
    let brk = data.f("player.brake", 0.0) as f32 / 100.0;
    let clu = data.f("player.clutch", 0.0) as f32 / 100.0;
    let mut pedals = vec![(brk, BAD), (thr, OK)];
    if clutch {
        pedals.push((clu, BLUE));
    }
    let pedals_w = pedals.len() as f32 * 17.0 - 5.0;
    // Largeur : étroite si seules les pédales sont affichées, entière dès qu'il y a le graphe ou les chiffres.
    let w = if pedals_on && !gauges && !graph { 32.0 + pedals_w } else { 262.0 };
    // Haut du widget : la barre de régime (avec les chiffres), puis pédales et chiffres côte à côte.
    let top = pedals_on || gauges;
    let cy = if gauges { 28.0 } else { 12.0 };
    let block = if !top {
        0.0
    } else if pedals_on {
        cy + 86.0
    } else {
        cy + 62.0
    };
    // Pédales et graphe sans les chiffres : le graphe se place à côté des pédales, sur toute leur hauteur.
    let side = pedals_on && graph && !gauges;
    let (gx, gy, gw, gh) = if side {
        (16.0 + pedals_w + 14.0, 12.0, w - 32.0 - pedals_w - 14.0, 84.0)
    } else {
        (16.0, if top { block + 6.0 } else { 12.0 }, w - 32.0, 52.0)
    };
    let h = if side {
        12.0 + 84.0 + 12.0
    } else if graph {
        gy + gh + 10.0
    } else {
        block + 6.0
    };
    d.card(w, h);

    if gauges {
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
    }
    if pedals_on {
        // Pédales : segments verticaux.
        for (i, (v, color)) in pedals.iter().enumerate() {
            let x = 16.0 + i as f32 * 17.0;
            let segs = 14;
            for s in 0..segs {
                let y = cy + (segs - 1 - s) as f32 * 6.0;
                let lit = (s as f32 + 1.0) / segs as f32 <= v.clamp(0.0, 1.0) + 0.0001;
                d.fill(x, y, 12.0, 4.0, if lit { *color } else { Color32::from_white_alpha(26) });
            }
        }
    }
    if gauges {
        let px = if pedals_on { 16.0 + pedals_w + 19.0 } else { 16.0 };
        let gear = data.f("player.gear", 0.0) as i64;
        let g = if gear < 0 { "R".to_string() } else if gear == 0 { "N".to_string() } else { gear.to_string() };
        d.text(px + 24.0, cy + 2.0, &g, 54.0, d.accent(), Font::Num, HAlign::Center);
        let kmh = data.f("player.speed", 0.0);
        let (v, unit) = if spec.flag("mph", false) { (kmh * 0.621371, "mph") } else { crate::prefs::get().speed(kmh) };
        let sx = px + 62.0;
        let vw = d.text(sx, cy + 10.0, &format!("{v:.0}"), 24.0, TEXT, Font::Num, HAlign::Left);
        d.text(sx + vw + 4.0, cy + 20.0, unit, 11.0, MUTED, Font::Ui, HAlign::Left);
    }

    if graph {
        // Historique glissant ; un point par image dessinée (le rythme est fixé par `fps`).
        st.hist.push_back((now, [thr, brk, data.f("player.steering", 0.0) as f32, clu]));
        while st.hist.front().is_some_and(|(t, _)| now - t > win + 0.2) {
            st.hist.pop_front();
        }
        if top && !side {
            d.sep(16.0, block, w - 32.0);
        }
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

pub(crate) fn class_color(c: &str) -> Color32 {
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
    /// Temps écoulé dans le tour et temps au tour estimés par le jeu (0 s'ils manquent).
    time_into: f64,
    est_lap: f64,
}

/// Écarts avec la voiture qui roule juste devant et juste derrière sur la piste, en secondes (voir `near_cars`).
/// Sans classement disponible, on se rabat sur les écarts donnés par le jeu.
fn neighbour_gaps(data: &Data) -> (Option<f64>, Option<f64>) {
    let positive = |v: Option<f64>| v.filter(|v| *v > 0.0);
    let cars = near_cars(data);
    if let Some(me) = cars.iter().find(|c| c.me) {
        let on_track = |c: &&Near| !c.me && (!c.pit || me.pit);
        let ahead = cars.iter().filter(on_track).filter(|c| c.secs > 0.0).map(|c| c.secs).fold(f64::MAX, f64::min);
        let behind = cars.iter().filter(on_track).filter(|c| c.secs <= 0.0).map(|c| -c.secs).fold(f64::MAX, f64::min);
        let some = |v: f64| (v < f64::MAX).then(|| v.max(0.1));
        if ahead < f64::MAX || behind < f64::MAX {
            return (some(ahead), some(behind));
        }
    }
    (
        positive(data.num("gap.car_ahead")).or_else(|| positive(data.num("position.behind_next"))),
        positive(data.num("gap.car_behind")),
    )
}

/// Les voitures du classement, rangées par place.
fn grid_cars(data: &Data) -> Vec<Car> {
    let length = data.f("session.track_length", 0.0).max(1.0);
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
                time_into: data.f(&format!("{p}.time_into"), 0.0),
                est_lap: data.f(&format!("{p}.est_lap"), 0.0),
            })
        })
        .collect();
    all.sort_by_key(|c| c.place);
    all
}

/// Retard, en secondes, de `behind` sur `ahead` (classé devant lui), calculé comme TinyPedal : la différence de leurs
/// temps écoulés dans le tour, tels que le jeu les estime ; si celui de devant vient de passer la ligne, on ajoute
/// un tour de celui de derrière. `None` si le jeu ne donne pas ces temps (ancien replay, démonstration).
fn interval(ahead: &Car, behind: &Car, length: f64) -> Option<f64> {
    if ahead.est_lap <= 1.0 || behind.est_lap <= 1.0 {
        return None;
    }
    let lap_diff = (ahead.dist - behind.dist) / length.max(1.0);
    if lap_diff < 0.0 {
        return Some(0.0); // ordre pas encore à jour (départ lancé, dépassement en cours)
    }
    let gap = ahead.time_into - behind.time_into;
    Some(if gap >= 0.0 { gap } else { gap + behind.est_lap })
}

/// Écart en secondes entre le joueur et la voiture `j` (positif si elle est derrière lui).
/// D'abord d'après les temps dans le tour ; sinon la somme des retards « sur la voiture devant » donnés par le jeu ;
/// en dernier recours la distance divisée par la vitesse.
fn gap_seconds(all: &[Car], me: usize, j: usize, speed: f64, length: f64) -> f64 {
    let timed = if j < me { interval(&all[j], &all[me], length).map(|g| -g) } else { interval(&all[me], &all[j], length) };
    if let Some(g) = timed {
        return g;
    }
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
    let all = grid_cars(data);
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
                    draw::signed(Some(gap_seconds(&all, me, j, speed, length)), 1)
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
    // Les drapeaux ont leur propre widget (« Drapeaux »).
    let top = 0.0;
    // Circuit et type de session seulement si demandés.
    let hdr = if d.titles() { 16.0 } else { 0.0 };
    let h = 10.0 + top + hdr + 46.0 + if weather { 52.0 } else { 6.0 };
    d.card(w, h);
    let y = 10.0 + top;
    if d.titles() {
        d.label(16.0, y, data.text("session.track"), MUTED);
        d.label_right(w - 16.0, y, session_name(data.f("session.type", -1.0) as i64), d.accent());
    }
    d.text(16.0, y + hdr, &draw::hms(data.num("session.remaining")), 34.0, TEXT, Font::Num, HAlign::Left);
    let max = data.f("session.max_laps", 0.0);
    let left = data.num("strategy.race_laps_left");
    let laps = if max > 0.0 && max < 10_000.0 {
        format!("Tour {:.0} / {max:.0}", data.f("lap.number", 0.0).min(max))
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
            ("Air", draw::n(data.num("weather.ambient").map(|v| crate::prefs::get().temp(v)), 0) + "°"),
            ("Piste", draw::n(data.num("weather.track").map(|v| crate::prefs::get().temp(v)), 0) + "°"),
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

/* ---------- Drapeaux ---------- */

/// Un drapeau montré au pilote : une couleur, et le secteur pour le jaune. Aucun autre texte.
struct Flag {
    /// Le panneau clignote (jaune local, bleu, rouge), comme au bord de la piste.
    blink: bool,
    fill: Color32,
    /// Texte du bandeau : seulement le secteur du jaune (« S2 »).
    label: String,
    kind: FlagKind,
}

#[derive(PartialEq, Clone, Copy)]
enum FlagKind {
    Plain,
    Black,
    Checkered,
}

/// Le drapeau (ou les deux plus importants s'il y en a plusieurs) affiché en grand, rien quand il n'y en a aucun :
/// lisible d'un coup d'œil en roulant. Ordre d'importance : rouge, neutralisation, noir, jaune, bleu, blanc, vert, damier.
fn flags(d: &D, spec: &WidgetSpec, data: &Data, st: &mut WState, now: f64) -> (f32, f32) {
    let (w, h) = (290.0, 104.0);
    let phase = data.f("session.phase", 5.0) as i64;
    let ystate = data.f("session.yellow", 0.0) as i64;
    let finish = data.f("player.finish_status", 0.0) as i64;
    // Vert : allumé 8 s après une relance (retour en phase de course après formation, neutralisation ou arrêt).
    if let Some(before) = st.flag_phase {
        if phase == 5 && before != 5 && before >= 0 {
            st.green_until = now + 8.0;
        }
    }
    st.flag_phase = Some(phase);

    let dark = Color32::from_rgb(17, 17, 17);
    let mut active: Vec<Flag> = Vec::new();
    let mut add = |fill: Color32, label: &str, _ink: Color32, kind: FlagKind| active.push(Flag { blink: fill == BAD || fill == BLUE || !label.is_empty(), fill, label: label.into(), kind });
    if phase == 7 || ystate == 7 {
        add(BAD, "", Color32::WHITE, FlagKind::Plain);
    }
    if phase == 6 {
        // Neutralisation (full course yellow, safety car) : orange, pour ne pas la confondre avec le jaune d'un secteur.
        add(WARN, "", dark, FlagKind::Plain);
    }
    if finish == 3 {
        add(Color32::from_rgb(12, 12, 12), "", Color32::WHITE, FlagKind::Black);
    }
    let sectors: Vec<usize> = (1..=3).filter(|s| data.flag(&format!("session.sector_yellow.{s}"))).collect();
    if !sectors.is_empty() {
        // « S2 », « S13 » : le S une seule fois, pour tenir sur un demi-panneau.
        let list: String = sectors.iter().map(|s| s.to_string()).collect();
        add(Color32::from_rgb(255, 214, 51), &format!("S{list}"), dark, FlagKind::Plain);
    }
    // Bleu : c'est le jeu qui le déclenche. On regarde ensuite qui arrive derrière : une voiture d'une classe plus
    // rapide donne un bleu avec le sigle de sa classe, sinon c'est un bleu simple (on nous prend un tour).
    if data.flag("player.blue_flag") {
        let tag = if spec.flag("blueclass", true) { faster_class_behind(data).unwrap_or("") } else { "" };
        add(BLUE, tag, Color32::WHITE, FlagKind::Plain);
    }
    if ystate == 5 {
        add(Color32::from_rgb(240, 244, 250), "", dark, FlagKind::Plain);
    }
    if now < st.green_until {
        add(OK, "", dark, FlagKind::Plain);
    }
    if phase == 8 || finish == 1 {
        add(Color32::WHITE, "", dark, FlagKind::Checkered);
    }

    if active.is_empty() {
        // Rien à montrer en jeu. Dans l'éditeur, un repère permet de retrouver et de placer le widget.
        if d.is_preview() {
            let edge = Color32::from_white_alpha(40);
            d.fill(0.0, 0.0, w, 1.5, edge);
            d.fill(0.0, h - 1.5, w, 1.5, edge);
            d.fill(0.0, 0.0, 1.5, h, edge);
            d.fill(w - 1.5, 0.0, 1.5, h, edge);
            d.text(w / 2.0, h / 2.0 - 9.0, "Drapeaux (aucun en ce moment)", 14.0, MUTED, Font::Ui, HAlign::Center);
        }
        st.animating = false;
        return (w, h);
    }
    let shown = &active[..active.len().min(2)];
    // Clignotement : allumé 0,45 s, atténué 0,3 s. Le widget n'est redessiné en continu que s'il clignote.
    let blinking = spec.flag("anim", true) && shown.iter().any(|f| f.blink);
    st.animating = blinking;
    let dim = blinking && (now % 0.75) > 0.45;
    // Un ou deux panneaux lumineux côte à côte. Le dessin ne change qu'au clignotement : il est gardé en mémoire.
    let gap = 8.0;
    let pw = (w - gap * (shown.len() as f32 - 1.0)) / shown.len() as f32;
    let tag = {
        use std::hash::{Hash, Hasher};
        let mut hs = std::collections::hash_map::DefaultHasher::new();
        for f in shown {
            (f.fill.to_array(), &f.label, f.kind as u8, f.blink && dim).hash(&mut hs);
        }
        ("flags", shown.len(), d.cache_tag()).hash(&mut hs);
        hs.finish()
    };
    d.layer(tag, 0.0, 0.0, w, h, &mut |dl| {
        for (i, f) in shown.iter().enumerate() {
            led_panel(dl, i as f32 * (pw + gap), pw, h, f, f.blink && dim);
        }
    });
    (w, h)
}

/// Rapidité d'une classe et son sigle sur le panneau (plus le nombre est grand, plus la classe est rapide).
/// `None` pour une classe inconnue : on ne la compare alors à aucune autre.
fn class_speed(class: &str) -> Option<(u8, &'static str)> {
    let c = class.to_lowercase();
    if c.contains("hyper") {
        Some((5, "HYP"))
    } else if c.contains("lmp2") || c.contains("lmp3") || c.contains("lmp") {
        Some((if c.contains("lmp3") { 3 } else { 4 }, "LMP"))
    } else if c.contains("gte") {
        Some((2, "GTE"))
    } else if c.contains("gt3") {
        Some((1, "GT"))
    } else {
        None
    }
}

/// Sigle de la classe de la voiture qui arrive juste derrière nous sur la piste, si cette classe est plus rapide que
/// la nôtre : toujours « LMP », quelle que soit cette classe. `None` si elle est de notre classe, plus lente, inconnue, ou s'il n'y a personne
/// à moins de 15 secondes.
pub(crate) fn faster_class_behind(data: &Data) -> Option<&'static str> {
    let cars = near_cars(data);
    let me = cars.iter().find(|c| c.me)?;
    let behind = cars.iter().filter(|c| !c.me && !c.pit && c.secs < 0.0 && c.secs > -15.0).max_by(|a, b| a.secs.total_cmp(&b.secs))?;
    let ((theirs, tag), (mine, _)) = (class_speed(&behind.class)?, class_speed(&me.class)?);
    // Le panneau affiche toujours « LMP », quelle que soit la classe plus rapide qui arrive.
    let _ = tag;
    (theirs > mine).then_some("LMP")
}

/// Lettres du panneau en points : 5 × 7, une ligne par rangée (bit de gauche = colonne de gauche).
fn glyph(c: char) -> [u8; 7] {
    match c {
        'S' => [0b01110, 0b10001, 0b10000, 0b01110, 0b00001, 0b10001, 0b01110],
        '1' => [0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
        '2' => [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111],
        '3' => [0b01110, 0b10001, 0b00001, 0b00110, 0b00001, 0b10001, 0b01110],
        'L' => [0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111],
        'M' => [0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001],
        'P' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000],
        'H' => [0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
        'Y' => [0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100],
        'G' => [0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01111],
        'T' => [0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100],
        'E' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111],
        _ => [0; 7],
    }
}

/// Un panneau lumineux comme ceux du bord de piste : une grille de points de la couleur du drapeau sur fond noir.
/// Le secteur du jaune s'écrit en points dans une fenêtre sombre au milieu.
fn led_panel(d: &D, x: f32, pw: f32, h: f32, f: &Flag, dim: bool) {
    let pitch = 8.0_f32;
    let (cols, rows) = ((pw / pitch).floor() as i32, (h / pitch).floor() as i32);
    let (ox, oy) = (x + (pw - cols as f32 * pitch) / 2.0 + pitch / 2.0, (h - rows as f32 * pitch) / 2.0 + pitch / 2.0);
    // Boîtier.
    d.fill(x, 0.0, pw, h, Color32::from_rgb(9, 10, 12));
    let bezel = Color32::from_rgb(46, 51, 60);
    d.fill(x, 0.0, pw, 1.5, bezel);
    d.fill(x, h - 1.5, pw, 1.5, bezel);
    d.fill(x, 0.0, 1.5, h, bezel);
    d.fill(x + pw - 1.5, 0.0, 1.5, h, bezel);

    // Texte : une case par point, avec une marge d'un point autour (la « fenêtre » sombre).
    let chars: Vec<char> = f.label.chars().collect();
    let text_cols = if chars.is_empty() { 0 } else { chars.len() as i32 * 6 - 1 };
    let (tx, ty) = ((cols - text_cols) / 2, (rows - 7) / 2);
    let in_window = |c: i32, r: i32| text_cols > 0 && c >= tx - 2 && c < tx + text_cols + 2 && r >= ty - 1 && r < ty + 8;
    let in_glyph = |c: i32, r: i32| {
        let (gc, gr) = (c - tx, r - ty);
        if text_cols == 0 || gc < 0 || gc >= text_cols || !(0..7).contains(&gr) || gc % 6 == 5 {
            return false;
        }
        glyph(chars[(gc / 6) as usize])[gr as usize] >> (4 - gc % 6) & 1 == 1
    };

    let off = Color32::from_rgb(26, 28, 33);
    let level = if dim { 0.22 } else { 1.0 };
    let tone = |c: Color32, k: f32| Color32::from_rgb((c.r() as f32 * k) as u8, (c.g() as f32 * k) as u8, (c.b() as f32 * k) as u8);
    for r in 0..rows {
        for c in 0..cols {
            let lit = match f.kind {
                // Damier : pavés de 2 × 2 points.
                FlagKind::Checkered => (c / 2 + r / 2) % 2 == 0,
                // Noir : seul le tour du panneau est allumé.
                FlagKind::Black => c == 0 || r == 0 || c == cols - 1 || r == rows - 1,
                FlagKind::Plain => {
                    if in_window(c, r) {
                        in_glyph(c, r)
                    } else {
                        true
                    }
                }
            };
            let (px, py) = (ox + c as f32 * pitch, oy + r as f32 * pitch);
            if lit {
                let color = if f.kind == FlagKind::Black { Color32::from_rgb(236, 240, 246) } else { f.fill };
                // Halo léger, puis le point.
                d.circle(px, py, 3.9, tone(color, level).gamma_multiply(0.22), None);
                d.circle(px, py, 2.7, tone(color, level), None);
            } else {
                d.circle(px, py, 2.2, off, None);
            }
        }
    }
}

/* ---------- Carte du circuit ---------- */

/// Position d'une voiture sur la carte : (x, -z), repère de TinyPedal.
struct MapCar {
    idx: usize,
    driver: String,
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

/// Angle (degrés, 0 à 179) qui donne au circuit la plus grande taille possible dans une zone de `aw` × `ah`.
fn best_angle(points: &[[f32; 2]], aw: f32, ah: f32) -> f32 {
    let (mut best, mut best_scale) = (0.0f32, 0.0f32);
    for deg in 0..180 {
        let (sin, cos) = (deg as f32).to_radians().sin_cos();
        let (mut min, mut max) = ([f32::MAX; 2], [f32::MIN; 2]);
        for p in points {
            let r = [p[0] * cos - p[1] * sin, p[0] * sin + p[1] * cos];
            for a in 0..2 {
                min[a] = min[a].min(r[a]);
                max[a] = max[a].max(r[a]);
            }
        }
        let scale = (aw / (max[0] - min[0]).max(1.0)).min(ah / (max[1] - min[1]).max(1.0));
        // Strictement meilleur : à taille égale on garde l'angle le plus petit (carte « droite » si possible).
        if scale > best_scale * 1.002 {
            best_scale = scale;
            best = deg as f32;
        }
    }
    best
}

fn trackmap_widget(d: &D, spec: &WidgetSpec, data: &Data, st: &mut WState, now: f64) -> (f32, f32) {
    // Zone choisie par le pilote : sa taille ne change jamais, c'est le circuit qui s'y adapte (et se tourne
    // pour y être le plus grand possible).
    let w = spec.num("size", 300.0).clamp(100.0, 1000.0) as f32;
    let h = spec.num("height", 220.0).clamp(100.0, 1000.0) as f32;
    if d.measuring() {
        return (w, h);
    }
    let k = (w.min(h * 1.4) / 300.0).clamp(0.7, 1.4); // épaisseur du tracé et taille des voitures suivent la zone
    let (ax, ay, aw, ah) = (24.0, 24.0, w - 48.0, h - 48.0);
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

    // Orientation : automatique (la plus grande dans la zone) ou imposée.
    let rotate = spec.choice("rotate", "auto");
    let angle_deg: f32 = if rotate == "auto" {
        let count = st.map.as_ref().and_then(|(_, m)| m.as_ref()).map_or(0, |m| m.points.len());
        let tag = (st.map.as_ref().map(|(k, _)| k.clone()), count, aw.to_bits(), ah.to_bits());
        match &st.map_angle {
            Some((t, a)) if *t == tag => *a,
            _ => {
                let a = st.map.as_ref().and_then(|(_, m)| m.as_ref()).map_or(0.0, |m| best_angle(&m.points, aw, ah));
                st.map_angle = Some((tag, a));
                a
            }
        }
    } else {
        rotate.parse().unwrap_or(0.0)
    };
    // Repère de sortie des stands (comme TinyPedal) : une voiture qui se trouve à cet endroit maintenant sera à
    // notre hauteur quand on ressortira.
    let pit_mode = spec.choice("pitexit", "request");
    let pit_exit = if pit_mode == "always" || (pit_mode == "request" && (data.flag("pit.in_pits") || data.f("pit.state", 0.0) >= 1.0)) {
        pwcore::carinfo::touch();
        Some(pit_plan(data, st, now, 25.0, 35.0))
    } else {
        None
    };
    let map = st.map.as_ref().and_then(|(_, m)| m.as_ref());

    let (sin, cos) = angle_deg.to_radians().sin_cos();
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
                driver: data.text(&format!("{p}.driver")).to_string(),
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
    // (Dans un replay, les positions sont déjà interpolées : on ne prolonge rien.)
    let extrapolate = spec.flag("extrapolate", true);
    for c in cars.iter_mut().filter(|_| extrapolate) {
        let e = st.car_pos.entry(c.idx).or_insert((c.xy[0], c.xy[1], now, 0.0, 0.0));
        if (c.xy[0], c.xy[1]) != (e.0, e.1) {
            let dt = (now - e.2) as f32;
            let (dx, dy) = (c.xy[0] - e.0, c.xy[1] - e.1);
            // Saut brutal (retour au stand, téléportation) ou trou dans les mesures : pas de vitesse.
            let (vx, vy) = if dt > 0.005 && dt < 1.0 && dx.abs() + dy.abs() < 150.0 { (dx / dt, dy / dt) } else { (0.0, 0.0) };
            // Vitesse impossible (plus de 430 km/h) : c'est un saut, pas un déplacement.
            let (vx, vy) = if vx.abs() + vy.abs() > 120.0 { (0.0, 0.0) } else { (vx, vy) };
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
    // Voitures suivies (écran de replay) : entourées, et dessinées au-dessus des autres.
    let (focus, focus2) = (spec.choice("focus", "").to_string(), spec.choice("focus2", "").to_string());
    let followed = |c: &MapCar| !c.driver.is_empty() && (c.driver == focus || c.driver == focus2);
    cars.sort_by_key(|c| (c.me, followed(c), -c.place));

    let outline = Color32::from_rgba_unmultiplied(6, 8, 12, 230);
    let dot = |c: &MapCar, x: f32, y: f32| {
        let num = match mode.as_str() {
            "none" => 0,
            "overall" => c.place,
            _ => c.rank,
        };
        let label = (num > 0).then(|| num.to_string());
        if followed(c) {
            let ring = if c.driver == focus { WARN } else { Color32::from_rgb(214, 104, 255) };
            d.circle(x, y, 17.0 * k, Color32::TRANSPARENT, Some((2.5 * k, ring)));
        }
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
        // Le chiffre du repère est le temps total de l'arrêt, voie des stands comprise.
        if let (Some(plan), Some(m)) = (&pit_exit, map) {
            if let Some(ghost) = plan.ghost {
                let length = data.f("session.track_length", 0.0).max(1.0);
                let (x, y) = to_screen(rot(trackmap::at(m, ghost / length)));
                d.circle(x, y, 10.0 * k, Color32::from_rgba_unmultiplied(8, 11, 16, 200), Some((2.0 * k, WARN)));
                d.text(x, y - 5.5 * k, &format!("{:.0}", plan.lane + plan.stop), 9.5 * k, WARN, Font::Num, HAlign::Center);
            }
        }
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

#[cfg(test)]
mod gap_tests {
    use super::neighbour_gaps;
    use pwcore::flatten::Field;
    use pwcore::Data;
    use serde_json::json;

    fn data(cars: &[(f64, bool, bool)], speed: f64) -> Data {
        let mut f = vec![
            Field { key: "session.track_length".into(), unit: "", label: "", value: json!(5000.0) },
            Field { key: "session.vehicles".into(), unit: "", label: "", value: json!(cars.len()) },
            Field { key: "player.speed".into(), unit: "", label: "", value: json!(speed) },
        ];
        for (i, (dist, me, pit)) in cars.iter().enumerate() {
            for (k, v) in [("lap_dist", json!(dist)), ("is_player", json!(me)), ("in_pits", json!(pit))] {
                f.push(Field { key: format!("grid.{i:03}.{k}"), unit: "", label: "", value: v });
            }
        }
        Data::new(f)
    }

    #[test]
    fn pit_plan_follows_the_learned_lane() {
        use super::{pit_plan, WState};
        // Circuit de 5 km bouclé en 100 s (50 m/s). Voie des stands de 4700 m à 250 m (550 m) à 20 m/s : 27,5 s.
        let scene = |dist: f64, in_pits: bool, lane: bool| {
            let mut f = data(&[(dist, true, in_pits)], 180.0).fields;
            let mut put = |k: &str, v: serde_json::Value| f.push(Field { key: k.into(), unit: "", label: "", value: v });
            put("grid.000.est_lap", json!(100.0));
            put("position.lap_dist", json!(dist));
            put("pit.in_pits", json!(in_pits));
            put("pit.stop_time", json!(40.0));
            if lane {
                put("pit.lane_entry", json!(4700.0));
                put("pit.lane_exit", json!(250.0));
                put("pit.lane_speed", json!(20.0));
            }
            Data::new(f)
        };
        let mut st = WState::default();
        let p = pit_plan(&scene(1000.0, false, true), &mut st, 0.0, 25.0, 35.0);
        assert!(p.stop_from_game && p.lane_measured);
        assert!((p.lane - 27.5).abs() < 1e-9);
        // En piste, ces 550 m prennent 11 s : l'arrêt coûte 27,5 + 40 − 11.
        assert!((p.loss - 56.5).abs() < 1e-9);
        // On ressort dans 74 s (jusqu'à l'entrée) + 67,5 s : la voiture qui sera alors à la sortie est à 3175 m.
        assert!((p.ghost.unwrap() - 3175.0).abs() < 1e-6);
        // Dans la voie depuis 10 s : il reste 57,5 s.
        pit_plan(&scene(4800.0, true, true), &mut st, 100.0, 25.0, 35.0);
        let p = pit_plan(&scene(4900.0, true, true), &mut st, 110.0, 25.0, 35.0);
        assert!((p.ghost.unwrap() - 2375.0).abs() < 1e-6);
        // Pendant l'arrêt, le jeu n'annonce plus que 5 s : on garde les 40 s annoncées à l'entrée.
        let mut shrunk = scene(4900.0, true, true).fields;
        shrunk.iter_mut().find(|f| f.key == "pit.stop_time").unwrap().value = json!(5.0);
        let p = pit_plan(&Data::new(shrunk), &mut st, 110.0, 25.0, 35.0);
        assert!((p.stop - 40.0).abs() < 1e-9 && (p.ghost.unwrap() - 2375.0).abs() < 1e-6);
        // Arrêt plus long que prévu : on ne ressort jamais avant d'avoir parcouru le reste de la voie.
        let p = pit_plan(&scene(4900.0, true, true), &mut st, 400.0, 25.0, 35.0);
        assert!((p.ghost.unwrap() - (250.0 - 17.5 * 50.0 + 5000.0)).abs() < 1e-6);
        // Circuit pas encore appris : les réglages du widget, et pas de point de sortie.
        let p = pit_plan(&scene(1000.0, false, false), &mut WState::default(), 0.0, 25.0, 35.0);
        assert!(!p.lane_measured && p.ghost.is_none() && (p.loss - 65.0).abs() < 1e-9);
    }

    #[test]
    fn blue_flag_names_a_faster_class_behind() {
        use super::faster_class_behind;
        // Moi à 1000 m, 180 km/h ; une voiture 60 m derrière (1,2 s).
        let scene = |class: &str, my_class: &str| {
            let mut d = data(&[(1000.0, true, false), (940.0, false, false)], 180.0);
            let mut f = std::mem::take(&mut d.fields);
            for (i, c) in [(0, my_class), (1, class)] {
                f.push(Field { key: format!("grid.{i:03}.class"), unit: "", label: "", value: json!(c) });
            }
            Data::new(f)
        };
        assert_eq!(faster_class_behind(&scene("LMP2", "LMGT3")), Some("LMP"));
        assert_eq!(faster_class_behind(&scene("Hypercar", "LMP2")), Some("LMP")); // toujours « LMP »
        assert_eq!(faster_class_behind(&scene("LMGT3", "LMGT3")), None); // même classe : bleu simple
        assert_eq!(faster_class_behind(&scene("LMGT3", "Hypercar")), None); // classe plus lente
        assert_eq!(faster_class_behind(&scene("Mystère", "LMGT3")), None); // classe inconnue
    }

    #[test]
    fn gaps_follow_the_cars_on_track() {
        // Joueur à 1000 m à 180 km/h (50 m/s) : une voiture 200 m devant, une 4900 m « devant » (donc 100 m derrière),
        // une aux stands juste devant (ignorée).
        let d = data(&[(1000.0, true, false), (1200.0, false, false), (900.0, false, false), (1010.0, false, true)], 180.0);
        let (ahead, behind) = neighbour_gaps(&d);
        assert!((ahead.unwrap() - 4.0).abs() < 1e-6);
        assert!((behind.unwrap() - 2.0).abs() < 1e-6);
        // Avec les temps dans le tour du jeu (méthode de TinyPedal) : 3 s devant, et une voiture qui vient de passer
        // la ligne (2 s dans son tour) alors qu'on est à 98 s d'un tour de 100 s : 4 s devant, pas 96 derrière.
        let mut d = data(&[(4900.0, true, false), (100.0, false, false), (4990.0, false, false)], 180.0);
        let mut f = std::mem::take(&mut d.fields);
        for (i, t) in [(0, 98.0), (1, 2.0), (2, 99.5)] {
            f.push(Field { key: format!("grid.{i:03}.time_into"), unit: "", label: "", value: json!(t) });
            f.push(Field { key: format!("grid.{i:03}.est_lap"), unit: "", label: "", value: json!(100.0) });
        }
        let (ahead, behind) = neighbour_gaps(&Data::new(f));
        assert!((ahead.unwrap() - 1.5).abs() < 1e-6 && behind.is_none());
        // Seul en piste : rien à afficher.
        assert_eq!(neighbour_gaps(&data(&[(1000.0, true, false)], 180.0)), (None, None));
    }
}

/* ---------- Relatif ---------- */

/// Nom court : « J. Dupont ».
fn short_name(name: &str) -> String {
    let parts: Vec<&str> = name.split_whitespace().collect();
    if parts.len() > 1 {
        format!("{}. {}", parts[0].chars().next().unwrap_or(' '), parts[1..].join(" "))
    } else {
        name.to_string()
    }
}

/// Une voiture vue depuis celle du joueur.
struct Near {
    name: String,
    class: String,
    place: i64,
    rank: i64,
    /// Écart en secondes : positif devant le joueur, négatif derrière.
    secs: f64,
    /// Tours d'avance sur le joueur au classement : +1 = il nous a pris un tour, −1 = on lui en a pris un.
    laps: i64,
    pit: bool,
    me: bool,
}

/// Les voitures du classement, situées par rapport au joueur sur la piste. Vide si le joueur est introuvable.
fn near_cars(data: &Data) -> Vec<Near> {
    let length = data.f("session.track_length", 0.0);
    let n = (data.f("session.vehicles", 0.0) as usize).min(104);
    let Some(me) = (0..n).find(|i| data.flag(&format!("grid.{i:03}.is_player"))) else { return Vec::new() };
    if length < 100.0 {
        return Vec::new();
    }
    let total = |i: usize| data.f(&format!("grid.{i:03}.laps"), 0.0) * length + data.f(&format!("grid.{i:03}.lap_dist"), 0.0);
    let (my_dist, my_total) = (data.f(&format!("grid.{me:03}.lap_dist"), 0.0), total(me));
    // Écart en temps, comme TinyPedal : différence des temps écoulés dans le tour (estimés par le jeu), ramenée à
    // plus ou moins un demi-tour du joueur. Sans ces temps : distance / vitesse la plus élevée des deux voitures.
    let (my_time, my_lap) = (data.f(&format!("grid.{me:03}.time_into"), 0.0), data.f(&format!("grid.{me:03}.est_lap"), 0.0));
    let my_speed = data.f("player.speed", 0.0) / 3.6;
    let in_garage = data.flag(&format!("grid.{me:03}.garage"));
    let mut cars: Vec<Near> = (0..n)
        .filter_map(|i| {
            let p = format!("grid.{i:03}");
            let dist = data.num(&format!("{p}.lap_dist"))?;
            // Une voiture rangée au garage n'est pas sur la piste (sauf si on y est aussi).
            if i != me && data.flag(&format!("{p}.garage")) && !in_garage {
                return None;
            }
            let mut ahead = (dist - my_dist).rem_euclid(length);
            if ahead > length / 2.0 {
                ahead -= length;
            }
            let secs = match data.num(&format!("{p}.time_into")) {
                Some(t) if my_lap > 1.0 => {
                    let d = (t - my_time).rem_euclid(my_lap);
                    if d > my_lap / 2.0 { d - my_lap } else { d }
                }
                _ => ahead / my_speed.max(data.f(&format!("{p}.speed"), 0.0)).max(15.0),
            };
            Some(Near {
                name: data.text(&format!("{p}.driver")).to_string(),
                class: data.text(&format!("{p}.class")).to_string(),
                place: data.f(&format!("{p}.place"), 0.0) as i64,
                rank: 0,
                secs: if i == me { 0.0 } else { secs },
                laps: ((total(i) - my_total - ahead) / length).round() as i64,
                pit: data.flag(&format!("{p}.in_pits")),
                me: i == me,
            })
        })
        .collect();
    // Place en classe.
    let mut order: Vec<usize> = (0..cars.len()).collect();
    order.sort_by_key(|&i| cars[i].place);
    let mut seen: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    for i in order {
        let c = seen.entry(cars[i].class.clone()).or_insert(0);
        *c += 1;
        cars[i].rank = *c;
    }
    cars
}

fn relative(d: &D, spec: &WidgetSpec, data: &Data) -> (f32, f32) {
    let w = 300.0;
    let (n_ahead, n_behind) = (spec.num("ahead", 3.0) as usize, spec.num("behind", 3.0) as usize);
    let rows = n_ahead + n_behind + 1;
    let class_place = spec.choice("place", "class") == "class";
    let top = if d.titles() { 30.0 } else { 8.0 };
    let h = top + rows as f32 * 24.0 + 10.0;
    d.card(w, h);
    if d.titles() {
        d.label(16.0, 10.0, "Relatif", MUTED);
        d.label_right(w - 16.0, 10.0, "sur la piste", d.accent());
    }
    let cars = near_cars(data);
    let Some(me) = cars.iter().find(|c| c.me) else { return (w, h) };
    // Les voitures aux stands ne comptent pas, sauf si on y est aussi.
    let on_track = |c: &&Near| !c.me && (!c.pit || me.pit);
    let mut front: Vec<&Near> = cars.iter().filter(on_track).filter(|c| c.secs > 0.0).collect();
    front.sort_by(|a, b| a.secs.total_cmp(&b.secs));
    front.truncate(n_ahead);
    front.reverse();
    let mut back: Vec<&Near> = cars.iter().filter(on_track).filter(|c| c.secs <= 0.0).collect();
    back.sort_by(|a, b| b.secs.total_cmp(&a.secs));
    back.truncate(n_behind);

    // Le joueur reste à sa ligne, même s'il manque des voitures devant.
    let first = n_ahead - front.len();
    for (row, c) in front.iter().chain(std::iter::once(&me)).chain(back.iter()).enumerate() {
        let y = top + (first + row) as f32 * 24.0;
        if c.me {
            d.fill(8.0, y, w - 8.0, 22.0, d.accent().gamma_multiply(0.22));
        }
        d.fill(8.0, y + 2.0, 3.0, 18.0, class_color(&c.class));
        let pos = if class_place { c.rank } else { c.place };
        d.text(42.0, y + 3.0, &pos.to_string(), 14.0, TEXT, Font::Num, HAlign::Right);
        // Rouge : il nous a pris un tour (à laisser passer). Bleu : on lui a pris un tour. Gris : aux stands.
        let color = if c.pit {
            MUTED
        } else if c.laps > 0 {
            BAD
        } else if c.laps < 0 {
            BLUE
        } else {
            TEXT
        };
        d.text(50.0, y + 3.0, &short_name(&c.name), 14.0, color, Font::Ui, HAlign::Left);
        if !c.me {
            // Même convention que le classement : « + » derrière nous, « − » devant.
            d.text(w - 16.0, y + 3.0, &draw::signed(Some(-c.secs), 1), 13.0, MUTED, Font::Num, HAlign::Right);
        }
    }
    (w, h)
}

/* ---------- Arrêt au stand ---------- */

/// Ce qu'un arrêt au stand coûterait maintenant.
struct PitPlan {
    /// Durée de l'arrêt lui-même, et si elle vient du jeu (sinon du réglage du widget).
    stop: f64,
    stop_from_game: bool,
    /// Temps de traversée de la voie des stands, et s'il a été mesuré sur ce circuit (sinon réglage du widget).
    lane: f64,
    lane_measured: bool,
    /// Temps perdu par rapport à une voiture restée en piste.
    loss: f64,
    /// Distance dans le tour d'une voiture qui, si elle y est maintenant, sera à notre hauteur quand on ressortira.
    ghost: Option<f64>,
    /// Vitesse moyenne sur un tour (m/s), pour convertir des distances en temps.
    pace: f64,
}

/// Calcule le coût d'un arrêt, à la façon de TinyPedal : la voie des stands est celle apprise sur ce circuit
/// (longueur ÷ vitesse du limiteur), l'arrêt est celui que le jeu estime. `lane_opt` et `stop_opt` : réglages du widget,
/// utilisés tant que le circuit n'est pas appris ou que le jeu ne donne pas son estimation.
fn pit_plan(data: &Data, st: &mut WState, now: f64, lane_opt: f64, stop_opt: f64) -> PitPlan {
    let game_stop = data.num("pit.stop_time").filter(|v| *v > 0.0);
    let current = (game_stop.unwrap_or(stop_opt), game_stop.is_some());
    // Une fois dans la voie, on garde la durée annoncée à l'entrée (comme TinyPedal) : pendant l'arrêt, l'estimation
    // du jeu fond à mesure que le travail avance, et le point de sortie prévu se mettrait à nous suivre.
    let (stop, stop_from_game) = if data.flag("pit.in_pits") {
        *st.pit_stop.get_or_insert(current)
    } else {
        st.pit_stop = Some(current);
        current
    };
    let length = data.f("session.track_length", 0.0);
    let n = (data.f("session.vehicles", 0.0) as usize).min(104);
    let me = (0..n).find(|i| data.flag(&format!("grid.{i:03}.is_player")));
    // Temps au tour : celui que le jeu estime pour nous, sinon notre meilleur ou notre dernier tour.
    let lap_time = me
        .and_then(|i| data.num(&format!("grid.{i:03}.est_lap")))
        .filter(|v| *v > 1.0)
        .or_else(|| data.num("position.best_lap").filter(|v| *v > 1.0))
        .or_else(|| data.num("position.last_lap").filter(|v| *v > 1.0));
    let in_pits = data.flag("pit.in_pits");
    // Chronomètre de l'arrêt en cours : depuis l'instant où on est entré dans la voie.
    if in_pits {
        st.pit_since.get_or_insert(now);
    } else {
        st.pit_since = None;
    }
    let (entry, exit, speed) = (data.num("pit.lane_entry"), data.num("pit.lane_exit"), data.f("pit.lane_speed", 60.0 / 3.6).max(1.0));
    match (entry, exit, lap_time) {
        (Some(entry), Some(exit), Some(lap_time)) if length > 100.0 => {
            let pace = length / lap_time;
            let lane_length = (exit - entry).rem_euclid(length);
            let lane = lane_length / speed;
            // Pendant qu'on traverse la voie et qu'on s'arrête, une voiture en piste parcourt la même portion à son rythme.
            let loss = (lane + stop - lane_length / pace).max(0.0);
            let my_dist = data.f("position.lap_dist", 0.0);
            // Temps avant de ressortir : jusqu'à l'entrée, puis la voie et l'arrêt ; une fois dedans, ce qu'il en reste.
            let remaining = match st.pit_since {
                Some(since) => (lane + stop - (now - since)).max((exit - my_dist).rem_euclid(length).min(lane_length) / speed),
                None => (entry - my_dist).rem_euclid(length) / pace + lane + stop,
            };
            PitPlan { stop, stop_from_game, lane, lane_measured: true, loss, ghost: Some((exit - remaining * pace).rem_euclid(length)), pace }
        }
        _ => PitPlan { stop, stop_from_game, lane: lane_opt, lane_measured: false, loss: lane_opt + stop, ghost: None, pace: lap_time.filter(|_| length > 100.0).map_or(50.0, |t| length / t) },
    }
}

fn pit(d: &D, spec: &WidgetSpec, data: &Data, st: &mut WState, now: f64) -> (f32, f32) {
    let w = 300.0;
    pwcore::carinfo::touch();
    let plan = pit_plan(data, st, now, spec.num("lane", 25.0), spec.num("stop", 35.0));
    // Menu des stands du jeu : ce qui est programmé pour le prochain arrêt.
    let menu: Vec<(String, String)> = if spec.flag("menu", true) {
        (0..(data.f("pit.menu.count", 0.0) as usize).min(6)).map(|i| (data.text(&format!("pit.menu.{i}.name")).to_string(), data.text(&format!("pit.menu.{i}.value")).to_string())).collect()
    } else {
        Vec::new()
    };
    let hdr = if d.titles() { 18.0 } else { 0.0 };
    let menu_h = if menu.is_empty() { 0.0 } else { 10.0 + menu.len() as f32 * 17.0 };
    let h = hdr + 138.0 + menu_h;
    d.card(w, h);
    if d.titles() {
        d.label(16.0, 10.0, "Arrêt au stand", MUTED);
        d.label_right(w - 16.0, 10.0, &format!("{:.0} s perdues", plan.loss), d.accent());
    }
    let y0 = hdr + 10.0;
    let is_race = (10..=13).contains(&(data.f("session.type", -1.0) as i64));

    // Retard de chaque voiture sur le leader : somme des écarts entre voitures qui se suivent au classement
    // (d'après les temps dans le tour, tours de retard compris ; sinon l'écart donné par le jeu).
    let length = data.f("session.track_length", 0.0).max(1.0);
    let cars = grid_cars(data);
    let mut behind = Vec::with_capacity(cars.len());
    let mut sum = 0.0;
    for i in 0..cars.len() {
        if i > 0 {
            sum += match interval(&cars[i - 1], &cars[i], length) {
                Some(g) => g + ((cars[i - 1].dist - cars[i].dist) / length).floor().max(0.0) * cars[i].est_lap,
                None => cars[i].behind_next,
            };
        }
        behind.push(sum);
    }
    let me = cars.iter().position(|c| c.me);
    let (Some(me), true) = (me, is_race) else {
        d.text(16.0, y0 + 6.0, "Estimation en course uniquement", 13.0, MUTED, Font::Ui, HAlign::Left);
        return (w, h);
    };
    // Après l'arrêt : notre retard sur le leader augmente du temps perdu ; on se replace parmi les autres.
    let mine = behind[me] + plan.loss;
    let others = || (0..cars.len()).filter(|&i| i != me);
    let place = 1 + others().filter(|&i| behind[i] < mine).count();
    let class_place = 1 + others().filter(|&i| behind[i] < mine && cars[i].class == cars[me].class).count();
    let lost = place as i64 - cars[me].place;
    d.text(16.0, y0, &format!("P{place}"), 30.0, TEXT, Font::Num, HAlign::Left);
    d.text(w - 16.0, y0 + 2.0, &format!("P{class_place} en classe"), 13.0, MUTED, Font::Num, HAlign::Right);
    d.text(w - 16.0, y0 + 20.0, &if lost > 0 { format!("{lost} place(s) perdue(s)") } else { "aucune place perdue".into() }, 12.0, if lost > 0 { WARN } else { OK }, Font::Ui, HAlign::Right);
    d.sep(16.0, y0 + 42.0, w - 32.0);

    // Qui sera devant et derrière nous à la sortie. Circuit appris : les voitures qui roulent autour du point de la
    // piste où on ressort (trafic réel, voitures à un tour comprises). Sinon : nos voisines au classement.
    let (front, back): (Option<(usize, f64)>, Option<(usize, f64)>) = match plan.ghost {
        Some(ghost) => {
            let rel = |i: usize| {
                let d = (cars[i].dist.rem_euclid(length) - ghost).rem_euclid(length);
                (if d > length / 2.0 { d - length } else { d }) / plan.pace
            };
            let on_track: Vec<(usize, f64)> = others().filter(|&i| !cars[i].pit).map(|i| (i, rel(i))).collect();
            (
                on_track.iter().filter(|c| c.1 > 0.0).min_by(|a, b| a.1.total_cmp(&b.1)).copied(),
                on_track.iter().filter(|c| c.1 <= 0.0).max_by(|a, b| a.1.total_cmp(&b.1)).map(|c| (c.0, -c.1)),
            )
        }
        None => (
            others().filter(|&i| behind[i] < mine).max_by(|a, b| behind[*a].total_cmp(&behind[*b])).map(|i| (i, mine - behind[i])),
            others().filter(|&i| behind[i] >= mine).min_by(|a, b| behind[*a].total_cmp(&behind[*b])).map(|i| (i, behind[i] - mine)),
        ),
    };
    let line = |y: f32, label: &str, who: Option<(usize, f64)>| {
        d.label(16.0, y + 4.0, label, MUTED);
        match who {
            Some((i, gap)) => {
                d.text(86.0, y + 1.0, &short_name(&cars[i].name), 14.0, TEXT, Font::Ui, HAlign::Left);
                // Moins de 3 s d'écart à la sortie : on ressort dans le trafic.
                d.text(w - 16.0, y + 1.0, &format!("{gap:.1} s"), 14.0, if gap < 3.0 { WARN } else { TEXT }, Font::Num, HAlign::Right);
            }
            None => {
                d.text(w - 16.0, y + 1.0, "personne", 13.0, MUTED, Font::Ui, HAlign::Right);
            }
        }
    };
    line(y0 + 48.0, "Devant", front);
    line(y0 + 70.0, "Derrière", back);
    // D'où vient le temps perdu : arrêt estimé par le jeu ou réglé, voie des stands mesurée sur ce circuit ou réglée.
    let repair = data.num("pit.repair_time").filter(|v| *v >= 1.0);
    let stop_txt = match (plan.stop_from_game, repair) {
        (true, Some(r)) => format!("Arrêt {:.0} s (répar. {r:.0})", plan.stop),
        (true, None) => format!("Arrêt {:.0} s", plan.stop),
        (false, _) => format!("Arrêt {:.0} s (réglage)", plan.stop),
    };
    let lane_txt = if plan.lane_measured { format!("voie {:.0} s", plan.lane) } else { format!("voie {:.0} s (réglage)", plan.lane) };
    d.text(16.0, y0 + 96.0, &format!("{stop_txt} · {lane_txt}"), 11.5, MUTED, Font::Num, HAlign::Left);
    if !menu.is_empty() {
        let top = y0 + 118.0;
        d.sep(16.0, top, w - 32.0);
        for (i, (name, value)) in menu.iter().enumerate() {
            let y = top + 8.0 + i as f32 * 17.0;
            d.label(16.0, y + 2.0, name, MUTED);
            d.text(w - 16.0, y, value, 12.5, TEXT, Font::Num, HAlign::Right);
        }
    }
    (w, h)
}

/* ---------- Radar de proximité ---------- */

/// Y a-t-il une voiture à notre hauteur, à gauche et à droite ? D'après la position et le cap de la télémétrie
/// (même repère et même rotation que le radar). `None` si le jeu ne donne pas le cap, ou si on est aux stands.
pub(crate) fn cars_beside(data: &Data) -> Option<(bool, bool)> {
    let n = (data.f("session.vehicles", 0.0) as usize).min(104);
    let me = (0..n).find(|i| data.flag(&format!("grid.{i:03}.is_player")))?;
    let p = format!("grid.{me:03}");
    if data.flag(&format!("{p}.in_pits")) {
        return None;
    }
    let (mx, my, yaw) = (data.num(&format!("{p}.x"))? as f32, -(data.num(&format!("{p}.z"))? as f32), data.num(&format!("{p}.yaw"))?);
    let (s, c) = ((yaw - std::f64::consts::PI) as f32).sin_cos();
    let (mut left, mut right) = (false, false);
    for i in (0..n).filter(|i| *i != me) {
        let p = format!("grid.{i:03}");
        let (Some(x), Some(z)) = (data.num(&format!("{p}.x")), data.num(&format!("{p}.z"))) else { continue };
        if data.flag(&format!("{p}.in_pits")) {
            continue;
        }
        let (rx, ry) = (x as f32 - mx, -(z as f32) - my);
        let (side, fwd) = (c * rx - s * ry, -(c * ry + s * rx));
        // Mêmes limites que le trait rouge du radar : à notre hauteur, et dans la voie d'à côté.
        if fwd.abs() < 4.9 && side.abs() > 0.8 && side.abs() < 4.6 {
            if side < 0.0 {
                left = true;
            } else {
                right = true;
            }
        }
    }
    Some((left, right))
}

fn radar(d: &D, spec: &WidgetSpec, data: &Data, st: &mut WState, now: f64) -> (f32, f32) {
    let size = 200.0_f32;
    if d.measuring() {
        return (size, size);
    }
    let range = spec.num("range", 20.0).clamp(8.0, 40.0) as f32;
    // Le radar apparaît avant qu'une voiture n'entre dans le cercle : on la voit arriver.
    let show = (spec.num("show", 35.0) as f32).max(range);
    let (cx, cy) = (size / 2.0, size / 2.0);
    let radius = size / 2.0 - 6.0;
    let scale = radius / range; // unités du widget par mètre

    // Comme TinyPedal : position et cap de chaque voiture lus dans sa télémétrie (repère x, −z).
    // Fluidité : le jeu ne publie ces positions qu'une vingtaine de fois par seconde, et pas au même instant pour
    // toutes les voitures. On garde pour chacune sa dernière mesure et sa vitesse, puis à chaque image on avance
    // toutes les voitures (la nôtre comprise) jusqu'à l'instant présent : le radar glisse au lieu de sauter.
    let n = (data.f("session.vehicles", 0.0) as usize).min(104);
    let mut me: Option<(usize, Option<f64>)> = None;
    let mut seen: Vec<(usize, Option<f64>)> = Vec::new();
    for i in 0..n {
        let p = format!("grid.{i:03}");
        let (Some(x), Some(z)) = (data.num(&format!("{p}.x")), data.num(&format!("{p}.z"))) else { continue };
        let (x, y) = (x as f32, -z as f32);
        // Instant de la mesure : celui du jeu s'il est connu, sinon l'horloge du widget.
        let t = data.num(&format!("{p}.t")).unwrap_or(now);
        let yaw = data.num(&format!("{p}.yaw"));
        let player = data.flag(&format!("{p}.is_player"));
        let e = st.car_pos.entry(i).or_insert((x, y, t, 0.0, 0.0));
        if (x, y) != (e.0, e.1) {
            let dt = (t - e.2) as f32;
            let (dx, dy) = (x - e.0, y - e.1);
            let ok = dt > 0.005 && dt < 1.0 && (dx.abs() + dy.abs()) / dt < 120.0;
            *e = (x, y, t, if ok { dx / dt } else { 0.0 }, if ok { dy / dt } else { 0.0 });
            if player {
                // Repère entre le temps du jeu et notre horloge, pris à chaque nouvelle mesure de notre voiture.
                st.radar_clock = Some((t, now));
            }
        }
        if player {
            me = Some((i, yaw));
        } else if !data.flag(&format!("{p}.in_pits")) {
            seen.push((i, yaw));
        }
    }
    // Instant présent, en temps du jeu (au plus 0,35 s après notre dernière mesure : au-delà, on n'invente plus rien).
    let game_now = st.radar_clock.map_or(now, |(t, wall)| t + (now - wall).clamp(0.0, 0.35));
    let at = |st: &WState, i: usize| -> (f32, f32) {
        let e = st.car_pos[&i];
        let lead = ((game_now - e.2) as f32).clamp(0.0, 0.35);
        (e.0 + e.3 * lead, e.1 + e.4 * lead)
    };
    let others: Vec<(f32, f32, Option<f64>)> = seen.iter().map(|(i, yaw)| { let p = at(st, *i); (p.0, p.1, *yaw) }).collect();
    let me = me.map(|(i, yaw)| { let p = at(st, i); (p.0, p.1, yaw) });
    // Voitures voisines dans le repère du joueur : (à droite, devant) en mètres, et leur angle par rapport à nous.
    let mut near: Vec<(f32, f32, f32)> = Vec::new();
    if let Some((mx, my, yaw)) = me {
        // Sans cap donné par le jeu (replay, démonstration) : la direction du dernier déplacement, gardée à l'arrêt.
        if yaw.is_none() {
            if let Some(last) = st.radar_last {
                let (dx, dy) = (mx - last.0, my - last.1);
                let len = (dx * dx + dy * dy).sqrt();
                if len > 0.3 && len < 60.0 {
                    let (nx, ny) = (dx / len, dy / len);
                    st.radar_dir = Some(match st.radar_dir {
                        Some((ox, oy)) => {
                            let (bx, by) = (ox * 0.6 + nx * 0.4, oy * 0.6 + ny * 0.4);
                            let l = (bx * bx + by * by).sqrt().max(1e-3);
                            (bx / l, by / l)
                        }
                        None => (nx, ny),
                    });
                    st.radar_last = Some((mx, my));
                } else if len >= 60.0 {
                    st.radar_last = Some((mx, my));
                }
            } else {
                st.radar_last = Some((mx, my));
            }
        }
        for (x, y, their_yaw) in others {
            let (rx, ry) = (x - mx, y - my);
            let rel = match (yaw, st.radar_dir) {
                // Rotation de TinyPedal : angle = cap du joueur − π ; l'axe vertical pointe vers l'arrière.
                (Some(yaw), _) => {
                    let (s, c) = ((yaw - std::f64::consts::PI) as f32).sin_cos();
                    Some((c * rx - s * ry, -(c * ry + s * rx)))
                }
                (None, Some((fx, fy))) => Some((rx * -fy + ry * fx, rx * fx + ry * fy)),
                _ => None,
            };
            if let Some((right, fwd)) = rel {
                if (right * right + fwd * fwd).sqrt() <= show {
                    let turn = match (yaw, their_yaw) {
                        (Some(a), Some(b)) => (b - a) as f32,
                        _ => 0.0,
                    };
                    near.push((right, fwd, turn));
                }
            }
        }
    }
    if d.is_preview() && near.is_empty() {
        // Aperçu de l'éditeur : trois voitures d'exemple.
        near = vec![(-2.6, 1.2, 0.05), (2.8, -8.5, -0.08), (0.4, 15.0, 0.0)];
    }
    if near.is_empty() && spec.flag("autohide", true) {
        return (size, size);
    }

    // Fond : un disque sombre, des cercles tous les 10 m et un axe, pour juger les distances d'un coup d'œil.
    let fill = ((bg(spec) * 1.5).min(0.9) * 255.0) as u8;
    let faint = Color32::from_white_alpha(26);
    // Ce fond ne change jamais : il est dessiné une fois, puis recopié à chaque image (le radar tourne à 60 images/s).
    let tag = {
        use std::hash::{Hash, Hasher};
        let mut hs = std::collections::hash_map::DefaultHasher::new();
        ("radar", range.to_bits(), fill, d.cache_tag()).hash(&mut hs);
        hs.finish()
    };
    d.layer(tag, 0.0, 0.0, size, size, &mut |dl| {
        dl.circle(cx, cy, radius, Color32::from_rgba_unmultiplied(8, 11, 16, fill), Some((1.2, Color32::from_white_alpha(46))));
        let mut ring = 10.0;
        while ring < range - 0.5 {
            dl.circle(cx, cy, ring * scale, Color32::TRANSPARENT, Some((1.0, faint)));
            ring += 10.0;
        }
        dl.line(&[(cx, cy - radius), (cx, cy + radius)], 1.0, faint);
        dl.line(&[(cx - radius, cy), (cx + radius, cy)], 1.0, faint);
    });

    // Une voiture : une gélule orientée (extrémités arrondies).
    let (car_w, car_l) = (2.0 * scale, 4.7 * scale);
    let half = (car_l - car_w).max(0.0) / 2.0;
    let car = |x: f32, y: f32, turn: f32, color: Color32| {
        // Vers le haut quand la voiture roule dans notre sens ; tournée de son angle par rapport à nous.
        let (dx, dy) = (-turn.sin(), -turn.cos());
        let ends = [(x - dx * half, y - dy * half), (x + dx * half, y + dy * half)];
        d.line(&ends, car_w + 2.4, Color32::from_rgba_unmultiplied(4, 6, 9, 220));
        d.line(&ends, car_w, color);
    };
    // Danger d'une voiture : à notre hauteur (rouge), toute proche (orange), sinon neutre.
    let beside = |c: &(f32, f32, f32)| c.0.abs() > 0.8 && c.0.abs() < 4.6 && c.1.abs() < 4.9;
    let close = |c: &(f32, f32, f32)| c.0.abs() < 4.6 && c.1.abs() < 10.0;
    for c in &near {
        if (c.0 * c.0 + c.1 * c.1).sqrt() > range - 1.0 {
            continue; // hors du cercle : elle a seulement fait apparaître le radar
        }
        let color = if beside(c) { BAD } else if close(c) { WARN } else { Color32::from_rgb(196, 204, 216) };
        car(cx + c.0 * scale, cy - c.1 * scale, c.2, color);
    }
    // Alerte de côté : un trait le long de notre voiture, rouge si quelqu'un est à notre hauteur, orange s'il arrive.
    for dir in [-1.0f32, 1.0] {
        let side: Vec<&(f32, f32, f32)> = near.iter().filter(|c| c.0 * dir > 0.8 && c.0.abs() < 4.6).collect();
        let color = if side.iter().any(|c| beside(c)) { Some(BAD) } else if side.iter().any(|c| close(c)) { Some(WARN) } else { None };
        if let Some(color) = color {
            let x = cx + dir * (car_w / 2.0 + 0.9 * scale);
            d.line(&[(x, cy - car_l * 0.75), (x, cy + car_l * 0.75)], (0.45 * scale).max(2.5), color);
        }
    }
    car(cx, cy, 0.0, d.accent());
    (size, size)
}

/* ---------- Prévisions météo ---------- */

/// Pictogramme du ciel : soleil, nuages, pluie ou orage, dessiné avec des formes simples.
fn sky_icon(d: &D, cx: f32, cy: f32, sky: i64) {
    let sun = Color32::from_rgb(255, 205, 64);
    let cloud = Color32::from_rgb(186, 196, 210);
    let drop = Color32::from_rgb(90, 165, 255);
    let draw_sun = |x: f32, y: f32, r: f32| {
        for i in 0..8 {
            let a = i as f32 * std::f32::consts::FRAC_PI_4;
            d.line(&[(x + a.cos() * (r + 2.5), y + a.sin() * (r + 2.5)), (x + a.cos() * (r + 5.5), y + a.sin() * (r + 5.5))], 1.6, sun);
        }
        d.circle(x, y, r, sun, None);
    };
    let draw_cloud = |x: f32, y: f32, color: Color32| {
        d.circle(x - 6.5, y + 2.0, 5.0, color, None);
        d.circle(x, y - 2.5, 7.0, color, None);
        d.circle(x + 7.0, y + 2.0, 5.5, color, None);
        d.fill(x - 6.5, y + 1.0, 13.5, 6.5, color);
    };
    let rain = |x: f32, y: f32, drops: i32, width: f32| {
        for i in 0..drops {
            let dx = (i as f32 - (drops - 1) as f32 / 2.0) * 6.0;
            d.line(&[(x + dx + 1.5, y + 10.0), (x + dx - 1.5, y + 16.0)], width, drop);
        }
    };
    match sky {
        0 => draw_sun(cx, cy, 7.0),
        1 | 2 => {
            draw_sun(cx - 6.0, cy - 5.0, 5.0);
            draw_cloud(cx + 3.0, cy + 3.0, cloud);
        }
        3 | 4 => draw_cloud(cx, cy, cloud),
        5..=7 => {
            draw_cloud(cx, cy - 3.0, cloud);
            rain(cx, cy - 3.0, 2, 1.6);
        }
        8 | 9 => {
            draw_cloud(cx, cy - 3.0, Color32::from_rgb(140, 150, 166));
            rain(cx, cy - 3.0, 4, 2.0);
        }
        10 => {
            draw_cloud(cx, cy - 3.0, Color32::from_rgb(118, 126, 142));
            d.line(&[(cx + 2.0, cy + 6.0), (cx - 3.0, cy + 12.0), (cx + 1.5, cy + 12.0), (cx - 2.0, cy + 18.0)], 2.0, sun);
        }
        _ => {
            d.text(cx, cy - 8.0, "—", 16.0, MUTED, Font::Num, HAlign::Center);
        }
    }
}

/// Prévisions de la session : cinq étapes (début, 25 %, 50 %, 75 %, fin) avec le ciel, le risque de pluie et la
/// température. L'étape en cours est soulignée, les étapes passées sont estompées.
fn forecast(d: &D, _spec: &WidgetSpec, data: &Data) -> (f32, f32) {
    // Demander les prévisions les fait tenir à jour (une requête au jeu toutes les 30 secondes).
    pwcore::forecast::touch();
    let w = 310.0;
    let hdr = if d.titles() { 18.0 } else { 0.0 };
    let h = hdr + 98.0;
    d.card(w, h);
    if d.titles() {
        d.label(16.0, 10.0, "Prévisions météo", MUTED);
    }
    let count = (data.f("forecast.count", 0.0) as usize).min(5);
    if count == 0 {
        d.text(w / 2.0, hdr + 38.0, "Prévisions indisponibles", 13.0, MUTED, Font::Ui, HAlign::Center);
        return (w, h);
    }
    let prefs = crate::prefs::get();
    let cell = (w - 20.0) / 5.0;
    let steps = ["Début", "25 %", "50 %", "75 %", "Fin"];
    for i in 0..count {
        let p = format!("forecast.{i}");
        let cx = 10.0 + cell * (i as f32 + 0.5);
        let y = hdr + 8.0;
        let current = data.flag(&format!("{p}.current"));
        let minutes = data.num(&format!("{p}.minutes"));
        let past = !current && minutes.is_some_and(|m| m < 0.0);
        // Ligne du haut : « maint. », le délai avant l'étape, ou à défaut sa place dans la session.
        let (when, color) = match minutes {
            _ if current => ("maint.".to_string(), d.accent()),
            Some(m) if m >= 90.0 => (format!("{:.0} h {:02.0}", (m / 60.0).floor(), m % 60.0), MUTED),
            Some(m) if m >= 0.0 => (format!("{m:.0} min"), TEXT),
            Some(_) => ("passé".to_string(), MUTED),
            None => (steps[i].to_string(), MUTED),
        };
        d.text(cx, y, &when, 10.5, color, Font::Num, HAlign::Center);
        if current {
            d.fill(cx - cell / 2.0 + 6.0, y + 16.0, cell - 12.0, 1.5, d.accent());
        }
        sky_icon(d, cx, y + 36.0, data.f(&format!("{p}.sky"), -1.0) as i64);
        let rain = data.f(&format!("{p}.rain"), 0.0);
        let rain_color = if rain >= 60.0 { BLUE } else if rain >= 30.0 { Color32::from_rgb(150, 190, 240) } else { MUTED };
        d.text(cx, y + 56.0, &format!("{rain:.0} %"), 13.0, rain_color, Font::Num, HAlign::Center);
        d.text(cx, y + 73.0, &format!("{:.0}°", prefs.temp(data.f(&format!("{p}.temp"), 0.0))), 11.0, MUTED, Font::Num, HAlign::Center);
        if past {
            // Étape passée : un voile sombre l'estompe.
            d.fill(cx - cell / 2.0 + 2.0, y - 2.0, cell - 4.0, 90.0, Color32::from_rgba_unmultiplied(8, 10, 14, 140));
        }
    }
    (w, h)
}

/* ---------- État de la voiture ---------- */

/// État de la voiture vue de dessus : carrosserie par zones, roues et suspensions, dégâts aérodynamiques, temps de
/// réparation et freins. Les zones viennent de la mémoire partagée ; le reste du service web du jeu.
fn damage(d: &D, spec: &WidgetSpec, data: &Data) -> (f32, f32) {
    pwcore::carinfo::touch();
    let brakes = spec.flag("brakes", true);
    let w = 262.0;
    let hdr = if d.titles() { 18.0 } else { 0.0 };
    let h = hdr + 152.0;
    d.card(w, h);
    if d.titles() {
        d.label(16.0, 10.0, "État de la voiture", MUTED);
    }
    let ok = Color32::from_rgb(58, 66, 80);
    let detached = Color32::from_rgb(214, 104, 255);

    // Schéma : une grille de 3 × 3 dont le centre est l'habitacle ; les huit autres cases sont les zones de carrosserie.
    let (sx, sy) = (34.0, hdr + 14.0);
    let (cw, ch, gap) = (26.0, 38.0, 3.0);
    let cell = |i: usize| (sx + (i % 3) as f32 * (cw + gap), sy + (i / 3) as f32 * (ch + gap));
    let mut zone = 0;
    for i in 0..9 {
        let (x, y) = cell(i);
        if i == 4 {
            d.fill(x, y, cw, ch, Color32::from_rgb(20, 24, 31));
            continue;
        }
        let color = match data.f(&format!("car.dent.{zone}"), 0.0) as i64 {
            0 => ok,
            1 => WARN,
            2 => BAD,
            _ => detached,
        };
        d.fill(x, y, cw, ch, color);
        zone += 1;
    }
    // Roues : la couleur dit l'état de la suspension ; une roue détachée est violette, un pneu crevé a un liseré rouge.
    let body_w = 3.0 * cw + 2.0 * gap;
    for (i, n) in ["fl", "fr", "rl", "rr"].iter().enumerate() {
        let x = if i % 2 == 0 { sx - 12.0 } else { sx + body_w + 5.0 };
        let y = if i < 2 { sy + 4.0 } else { sy + 2.0 * (ch + gap) + 4.0 };
        let susp = data.f(&format!("car.suspension.{n}"), 0.0);
        let color = if data.flag(&format!("tyre.{n}.detached")) {
            detached
        } else if susp >= 60.0 {
            BAD
        } else if susp >= 20.0 {
            WARN
        } else if susp >= 2.0 {
            Color32::from_rgb(214, 190, 96)
        } else {
            ok
        };
        d.fill(x, y, 7.0, 30.0, color);
        if data.flag(&format!("tyre.{n}.flat")) {
            d.fill(x - 2.0, y - 2.0, 11.0, 2.0, BAD);
            d.fill(x - 2.0, y + 30.0, 11.0, 2.0, BAD);
        }
    }

    // Colonne de droite : chiffres.
    let x = 156.0;
    let mut y = hdr + 12.0;
    let mut row = |label: &str, value: String, color: Color32| {
        d.label(x, y + 3.0, label, MUTED);
        d.text(w - 16.0, y, &value, 14.0, color, Font::Num, HAlign::Right);
        y += 24.0;
    };
    match data.num("car.aero_damage") {
        // On affiche ce qu'il reste de l'aérodynamique : 100 % = intacte.
        Some(dmg) => row("Aéro", format!("{:.0} %", 100.0 - dmg), if dmg >= 25.0 { BAD } else if dmg >= 5.0 { WARN } else { OK }),
        None => row("Aéro", "—".into(), MUTED),
    }
    match data.num("pit.repair_time") {
        Some(t) if t >= 1.0 => row("Répar.", format!("{t:.0} s"), WARN),
        Some(_) => row("Répar.", "aucune".into(), MUTED),
        None => row("Répar.", "—".into(), MUTED),
    }
    if brakes {
        y += 4.0;
        d.label(x, y, "Freins", MUTED);
        y += 16.0;
        for (i, n) in ["fl", "fr", "rl", "rr"].iter().enumerate() {
            let (bx, by) = (if i % 2 == 0 { x + 42.0 } else { w - 16.0 }, y + (i / 2) as f32 * 20.0);
            let (text, color) = match data.num(&format!("car.brake_left.{n}")) {
                Some(p) => (format!("{p:.0} %"), if p < 30.0 { BAD } else if p < 50.0 { WARN } else { TEXT }),
                None => ("—".to_string(), MUTED),
            };
            d.text(bx, by, &text, 13.0, color, Font::Num, HAlign::Right);
        }
    }
    (w, h)
}
