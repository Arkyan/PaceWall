//! Voie des stands de chaque circuit, apprise en roulant (comme TinyPedal) : où on y entre, où on en sort, et la
//! vitesse du limiteur. De quoi calculer le temps de traversée, donc le temps réellement perdu par un arrêt.
//!
//! - Entrée et sortie : la distance dans le tour au moment où le jeu nous dit « aux stands », puis « en piste ».
//! - Vitesse : celle du limiteur, relevée quand on roule à vitesse stable, pied au plancher, limiteur actif.
//!
//! Enregistré à côté de la carte du circuit (`tracks\<circuit>.pit.json`) et réutilisé aux sessions suivantes.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Vitesse supposée tant que le limiteur n'a pas été relevé : 60 km/h, la limite de Le Mans Ultimate.
const DEFAULT_SPEED: f64 = 60.0 / 3.6;

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PitLane {
    /// Distance dans le tour (mètres) à l'entrée et à la sortie de la voie. 0 = pas encore relevée.
    pub entry: f64,
    pub exit: f64,
    /// Vitesse du limiteur en m/s. 0 = pas encore relevée.
    #[serde(default)]
    pub speed: f64,
}

impl PitLane {
    pub fn known(&self) -> bool {
        self.entry > 0.0 && self.exit > 0.0
    }

    /// Longueur de la voie le long du tour (la sortie est souvent après la ligne, donc « derrière » l'entrée).
    pub fn length(&self, track_length: f64) -> f64 {
        if !self.known() {
            return 0.0;
        }
        let d = self.exit - self.entry;
        if d < 0.0 { d + track_length } else { d }
    }

    pub fn speed(&self) -> f64 {
        if self.speed > 1.0 { self.speed } else { DEFAULT_SPEED }
    }

    /// Temps pour traverser la voie au limiteur, sans s'arrêter.
    pub fn pass_time(&self, track_length: f64) -> Option<f64> {
        self.known().then(|| self.length(track_length) / self.speed())
    }
}

fn file(track: &str) -> PathBuf {
    let safe: String = track.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' }).collect();
    crate::trackmap::dir().join(format!("{safe}.pit.json"))
}

pub fn load(track: &str) -> Option<PitLane> {
    let text = std::fs::read_to_string(file(track)).ok()?;
    serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()
}

pub fn save(track: &str, lane: &PitLane) -> std::io::Result<()> {
    std::fs::create_dir_all(crate::trackmap::dir())?;
    std::fs::write(file(track), serde_json::to_string(lane)?)
}

/// Ce que l'enregistreur voit de la voiture du joueur à chaque lecture.
pub struct Sample {
    pub in_pits: bool,
    pub lap_dist: f64,
    /// m/s
    pub speed: f64,
    pub limiter: bool,
    /// 0 à 1
    pub throttle: f64,
    pub brake: f64,
}

/// Apprend la voie des stands du circuit en cours.
pub struct Learner {
    track: String,
    lane: PitLane,
    last_in: Option<bool>,
    last_speed: f64,
    last_pos: f64,
}

impl Learner {
    pub fn new(track: &str) -> Self {
        Self { track: track.to_string(), lane: load(track).unwrap_or_default(), last_in: None, last_speed: 0.0, last_pos: f64::NAN }
    }

    pub fn track(&self) -> &str {
        &self.track
    }

    /// À appeler à chaque lecture. Renvoie `true` quand la voie vient d'être enregistrée.
    pub fn update(&mut self, s: &Sample) -> bool {
        let before = self.lane;
        if self.last_in != Some(s.in_pits) {
            // On ne note le passage que si on roule : un retour au garage (touche Échap) n'est pas une entrée aux stands.
            if self.last_in.is_some() && s.speed > 1.0 {
                if s.in_pits {
                    self.lane.entry = s.lap_dist.max(0.0);
                } else {
                    self.lane.exit = s.lap_dist.max(0.0);
                }
            }
            self.last_in = Some(s.in_pits);
        }
        // Vitesse du limiteur : à fond, sans freiner, et vitesse quasi stable d'une lecture à l'autre.
        if s.in_pits && s.limiter && s.lap_dist != self.last_pos {
            let gain = s.speed - self.last_speed;
            if s.throttle > 0.95 && s.brake < 0.01 && s.speed > 1.0 && (0.0..0.1).contains(&gain) {
                self.lane.speed = s.speed;
            }
            self.last_speed = s.speed;
            self.last_pos = s.lap_dist;
        }
        // On n'écrit le fichier que si quelque chose d'utile a changé (pas pour un centimètre par seconde de vitesse).
        let changed = self.lane.entry != before.entry || self.lane.exit != before.exit || (self.lane.speed - before.speed).abs() > 0.1;
        changed && self.lane.known() && save(&self.track, &self.lane).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lane_length_wraps_around_the_start_line() {
        let lane = PitLane { entry: 5600.0, exit: 250.0, speed: 0.0 };
        assert_eq!(lane.length(5900.0), 550.0);
        // Sans vitesse relevée : 60 km/h.
        assert!((lane.pass_time(5900.0).unwrap() - 33.0).abs() < 1e-9);
        assert_eq!(PitLane { entry: 100.0, exit: 400.0, speed: 20.0 }.pass_time(5900.0), Some(15.0));
        assert_eq!(PitLane::default().pass_time(5900.0), None);
    }
}
