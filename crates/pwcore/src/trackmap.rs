//! Cartes des circuits : la trajectoire d'un tour, vue de dessus, enregistrée une fois puis réutilisée.
//!
//! Le jeu donne la position de chaque voiture (mètres, repère du monde) et la distance parcourue dans le tour.
//! On range les positions du joueur par distance de tour ; quand tout le tour est couvert, on en tire une boucle
//! de points régulièrement espacés, sauvegardée dans `%APPDATA%\PaceWall\tracks\<circuit>.json`.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TrackMap {
    pub track: String,
    /// Longueur du tour en mètres.
    pub length: f64,
    /// Boucle fermée dans l'ordre du tour : `[x, z]` en mètres.
    pub points: Vec<[f32; 2]>,
}

pub fn dir() -> PathBuf {
    crate::db::path().with_file_name("tracks")
}

fn file(track: &str) -> PathBuf {
    let safe: String = track.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' }).collect();
    dir().join(format!("{safe}.json"))
}

pub fn exists(track: &str) -> bool {
    file(track).is_file()
}

pub fn load(track: &str) -> Option<TrackMap> {
    let text = std::fs::read_to_string(file(track)).ok()?;
    let map: TrackMap = serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()?;
    (map.points.len() >= 50).then_some(map)
}

pub fn save(map: &TrackMap) -> std::io::Result<()> {
    std::fs::create_dir_all(dir())?;
    std::fs::write(file(&map.track), serde_json::to_string(map)?)
}

/// Un circuit fictif pour les aperçus (éditeur, démo).
pub fn demo() -> TrackMap {
    let n = 360;
    let points = (0..n)
        .map(|i| {
            let a = i as f32 / n as f32 * std::f32::consts::TAU;
            [520.0 * a.cos() + 130.0 * (3.0 * a).cos(), 300.0 * a.sin() + 90.0 * (2.0 * a).sin()]
        })
        .collect();
    TrackMap { track: "demo".into(), length: 4000.0, points }
}

/// Position d'un point à la fraction `f` (0..1) du tour sur la carte.
pub fn at(map: &TrackMap, f: f64) -> [f32; 2] {
    let n = map.points.len();
    let x = f.rem_euclid(1.0) * n as f64;
    let (i, t) = (x.floor() as usize % n, (x - x.floor()) as f32);
    let (a, b) = (map.points[i], map.points[(i + 1) % n]);
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}

/// Distance minimale entre deux points enregistrés (mètres de tour).
const MIN_NODE: f64 = 10.0;

/// Construit une carte pendant que le joueur roule, à la façon de TinyPedal : on enregistre un tour complet
/// commencé à la ligne, sans passage aux stands ni téléportation, et on ne le garde qu'après la ligne suivante
/// si le tour a été validé (temps du dernier tour positif).
pub struct Builder {
    pub track: String,
    length: f64,
    coords: Vec<[f32; 2]>,
    candidate: Option<Vec<[f32; 2]>>,
    validating_since: Option<f64>,
    recording: bool,
    pit_lap: bool,
    last_laps: i64,
    pos_recorded: f64,
    pos_last: f64,
}

impl Builder {
    pub fn new(track: &str, length: f64) -> Self {
        Self {
            track: track.to_string(),
            length,
            coords: Vec::new(),
            candidate: None,
            validating_since: None,
            recording: false,
            pit_lap: false,
            last_laps: i64::MAX,
            pos_recorded: 0.0,
            pos_last: 0.0,
        }
    }

    /// À appeler à chaque lecture. `x` et `z` : position du monde du jeu ; la carte est dans le repère
    /// (x, -z), comme TinyPedal. Renvoie la carte quand elle est prête à être sauvegardée.
    pub fn update(&mut self, laps: i64, lap_dist: f64, x: f64, z: f64, in_pits: bool, last_lap_time: f64, elapsed: f64) -> Option<TrackMap> {
        self.pit_lap |= in_pits;

        // Passage de la ligne : le tour qui vient de finir devient candidat s'il était propre, et un nouveau tour commence.
        if self.last_laps != laps && lap_dist > 0.0 && lap_dist < 200.0 {
            if !self.pit_lap && self.last_laps < laps && self.coords.len() >= 50 {
                self.candidate = Some(std::mem::take(&mut self.coords));
                self.validating_since = Some(elapsed);
            }
            self.coords.clear();
            self.pos_recorded = 0.0;
            self.pos_last = 0.0;
            self.recording = true;
            self.pit_lap = false;
            self.last_laps = laps;
        }

        // Validation après la ligne : tour valide au bout d'une seconde, sinon abandon après 8 s.
        if let Some(t0) = self.validating_since {
            let timer = elapsed - t0;
            if timer > 8.0 {
                self.candidate = None;
                self.validating_since = None;
            } else if timer > 1.0 && last_lap_time > 0.0 {
                self.validating_since = None;
                if let Some(points) = self.candidate.take() {
                    return Some(TrackMap { track: self.track.clone(), length: self.length, points });
                }
            }
        }

        if self.recording && lap_dist >= 0.0 && lap_dist != self.pos_last {
            let delta = lap_dist - self.pos_recorded;
            if delta > 100.0 {
                // Téléportation (retour au stand, remise en piste) : ce tour ne servira pas.
                self.recording = false;
                self.coords.clear();
            } else if delta >= MIN_NODE {
                self.coords.push([x as f32, -z as f32]);
                self.pos_recorded = lap_dist;
            }
            self.pos_last = lap_dist;
        }
        None
    }

    /// Avancement approximatif du tour en cours d'enregistrement (0..1), pour l'afficher.
    pub fn progress(&self) -> f32 {
        if self.recording && self.length > 0.0 {
            (self.pos_recorded / self.length).clamp(0.0, 1.0) as f32
        } else {
            0.0
        }
    }
}
