//! Règles des alertes vocales : carburant et énergie, pneus (froids, chauds, usés, crevés), drapeaux jaunes et pluie.
//! Ne fait que décider quoi dire ; la parole elle-même est dans `speech`. Chaque règle a son délai de relance pour ne
//! jamais répéter une alerte en boucle.

use pwcore::Data;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Une phrase à dire : `key` désigne le clip de l'ingénieur, `text` sert à la voix Windows (et au journal).
#[derive(Clone, Debug, PartialEq)]
pub struct Phrase {
    pub key: &'static str,
    pub text: String,
}

fn say(key: &'static str, text: impl Into<String>) -> Phrase {
    Phrase { key, text: text.into() }
}

/// Ce que le pilote a choisi d'entendre (réglages « voice_* »).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Config {
    pub enabled: bool,
    /// `true` : clips de l'ingénieur de course ; `false` : voix de synthèse de Windows.
    pub engineer: bool,
    pub volume: u32,
    pub fuel: bool,
    pub tyres: bool,
    pub flags: bool,
    pub weather: bool,
}

impl Config {
    pub fn load() -> Self {
        use pwcore::settings::{bool_of, int_of};
        Self {
            enabled: bool_of("voice_enabled", true),
            engineer: pwcore::settings::str_of("voice_engine", "engineer") != "windows",
            volume: int_of("voice_volume", 80).clamp(0, 100) as u32,
            fuel: bool_of("voice_fuel", true),
            tyres: bool_of("voice_tyres", true),
            flags: bool_of("voice_flags", true),
            weather: bool_of("voice_weather", true),
        }
    }
}

/// Seuils de carburant (tours restants) : une annonce à chaque seuil franchi.
const FUEL_STEPS: [f64; 4] = [5.0, 3.0, 2.0, 1.0];
const FUEL_KEYS: [&str; 4] = ["fuel_5", "fuel_3", "fuel_2", "fuel_1"];
const ENERGY_STEPS: [f64; 3] = [3.0, 2.0, 1.0];
const ENERGY_KEYS: [&str; 3] = ["energy_3", "energy_2", "energy_1"];

#[derive(Default)]
pub struct Alerts {
    fuel_stage: usize,
    energy_stage: usize,
    cold_since: Option<Instant>,
    hot_since: Option<Instant>,
    worn_said: bool,
    last_phase: Option<i64>,
    yellow_before: bool,
    raining_before: bool,
    last_said: HashMap<&'static str, Instant>,
}

/// Nombre de seuils franchis (tours restants sous le seuil), avec une marge de 0,5 tour pour ne pas osciller.
fn stage(steps: &[f64], left: f64, current: usize) -> usize {
    let crossed = steps.iter().filter(|s| left < **s).count();
    if crossed >= current {
        crossed
    } else {
        // Le niveau ne redescend que si on a nettement dépassé le seuil (ravitaillement).
        steps.iter().filter(|s| left < **s + 0.5).count().min(current)
    }
}

fn laps_text(n: f64) -> String {
    let n = n.round() as i64;
    if n <= 1 {
        "un tour".into()
    } else {
        format!("{n} tours")
    }
}

impl Alerts {
    /// À appeler avec chaque nouvelle lecture du jeu. Renvoie les phrases à dire (souvent aucune).
    pub fn update(&mut self, data: &Data, cfg: &Config, now: Instant) -> Vec<Phrase> {
        let mut say: Vec<Phrase> = Vec::new();
        if !cfg.enabled {
            return say;
        }
        // Une alerte n'a de sens qu'en piste : pas au garage, pas dans les menus.
        let on_track = data.flag("session.realtime");
        let speed = data.f("player.speed", 0.0);

        if cfg.flags {
            let phase = data.f("session.phase", -1.0) as i64;
            let yellow = data.f("session.yellow", 0.0) as i64 > 0;
            if let Some(before) = self.last_phase {
                if phase == 6 && before != 6 {
                    say.push(self::say("fcy", "Full course jaune"));
                } else if before == 6 && phase == 5 {
                    say.push(self::say("green", "Course relancée, drapeau vert"));
                }
            }
            if yellow && !self.yellow_before && phase != 6 && self.ready("yellow", now, 15) {
                say.push(self::say("yellow", "Drapeau jaune"));
            }
            self.last_phase = Some(phase);
            self.yellow_before = yellow;
        }

        if cfg.weather {
            let raining = data.f("weather.rain", 0.0) > 3.0;
            if raining && !self.raining_before && self.ready("rain", now, 120) {
                say.push(self::say("rain", "Pluie détectée"));
            }
            self.raining_before = raining;
        }

        if !on_track {
            return say;
        }

        if cfg.fuel {
            if let Some(left) = data.num("strategy.fuel_laps_left").filter(|v| v.is_finite() && *v >= 0.0) {
                let next = stage(&FUEL_STEPS, left, self.fuel_stage);
                if next > self.fuel_stage {
                    say.push(self::say(FUEL_KEYS[next - 1], format!("Carburant : {}", laps_text(FUEL_STEPS[next - 1]))));
                }
                self.fuel_stage = next;
            }
            if let Some(left) = data.num("strategy.energy_laps_left").filter(|v| v.is_finite() && *v >= 0.0) {
                let next = stage(&ENERGY_STEPS, left, self.energy_stage);
                if next > self.energy_stage {
                    say.push(self::say(ENERGY_KEYS[next - 1], format!("Énergie : {}", laps_text(ENERGY_STEPS[next - 1]))));
                }
                self.energy_stage = next;
            }
        }

        if cfg.tyres {
            let names = ["fl", "fr", "rl", "rr"];
            let temps: Vec<f64> = names.iter().filter_map(|n| data.num(&format!("tyre.{n}.carcass"))).filter(|t| t.is_finite()).collect();
            let optimal: Vec<f64> = names.iter().filter_map(|n| data.num(&format!("tyre.{n}.optimal"))).filter(|t| *t > 0.0).collect();
            if !temps.is_empty() && speed > 50.0 {
                let avg = temps.iter().sum::<f64>() / temps.len() as f64;
                let opt = if optimal.is_empty() { 88.0 } else { optimal.iter().sum::<f64>() / optimal.len() as f64 };
                // Froids : 8 s de suite sous la fenêtre ; chauds : 5 s de suite au-dessus. Une relance par minute ou plus.
                let cold = avg < opt - 15.0;
                let hot = avg > opt + 25.0;
                self.cold_since = if cold { self.cold_since.or(Some(now)) } else { None };
                self.hot_since = if hot { self.hot_since.or(Some(now)) } else { None };
                if self.cold_since.is_some_and(|t| now.duration_since(t) >= Duration::from_secs(8)) && self.ready("cold", now, 120) {
                    say.push(self::say("tyres_cold", "Pneus froids"));
                }
                if self.hot_since.is_some_and(|t| now.duration_since(t) >= Duration::from_secs(5)) && self.ready("hot", now, 60) {
                    say.push(self::say("tyres_hot", "Pneus trop chauds"));
                }
            } else {
                self.cold_since = None;
                self.hot_since = None;
            }
            // État des pneus : 100 % neufs, on prévient sous 25 % puis on se réarme quand ils sont changés.
            let states: Vec<f64> = names.iter().filter_map(|n| data.num(&format!("tyre.{n}.wear"))).collect();
            if let Some(min) = states.iter().cloned().reduce(f64::min) {
                if min < 25.0 && !self.worn_said {
                    self.worn_said = true;
                    say.push(self::say("tyres_worn", "Pneus usés"));
                } else if min > 60.0 {
                    self.worn_said = false;
                }
            }
            if names.iter().any(|n| data.flag(&format!("tyre.{n}.flat"))) && self.ready("flat", now, 30) {
                say.push(self::say("flat", "Crevaison"));
            }
        }
        say
    }

    /// `true` si `key` n'a pas été dite depuis `secs` secondes (et la note comme dite maintenant).
    fn ready(&mut self, key: &'static str, now: Instant, secs: u64) -> bool {
        let ok = self.last_said.get(key).is_none_or(|t| now.duration_since(*t) >= Duration::from_secs(secs));
        if ok {
            self.last_said.insert(key, now);
        }
        ok
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pwcore::flatten::Field;
    use serde_json::{json, Value};

    fn data(values: &[(&str, Value)]) -> Data {
        Data::new(values.iter().map(|(k, v)| Field { key: (*k).to_string(), unit: "", label: "", value: v.clone() }).collect())
    }

    fn texts(v: Vec<Phrase>) -> Vec<String> {
        v.into_iter().map(|p| p.text).collect()
    }

    fn cfg() -> Config {
        Config { enabled: true, engineer: true, volume: 80, fuel: true, tyres: true, flags: true, weather: true }
    }

    #[test]
    fn fuel_announced_once_per_threshold() {
        let mut a = Alerts::default();
        let t = Instant::now();
        let at = |left: f64| data(&[("session.realtime", json!(true)), ("strategy.fuel_laps_left", json!(left))]);
        assert!(a.update(&at(8.0), &cfg(), t).is_empty());
        assert_eq!(texts(a.update(&at(4.9), &cfg(), t)), vec!["Carburant : 5 tours"]);
        assert!(a.update(&at(4.2), &cfg(), t).is_empty());
        assert_eq!(texts(a.update(&at(2.9), &cfg(), t)), vec!["Carburant : 3 tours"]);
        // Ravitaillement : le niveau se réarme, la prochaine descente est de nouveau annoncée.
        assert!(a.update(&at(12.0), &cfg(), t).is_empty());
        assert_eq!(texts(a.update(&at(4.0), &cfg(), t)), vec!["Carburant : 5 tours"]);
    }

    #[test]
    fn nothing_off_track_or_disabled() {
        let mut a = Alerts::default();
        let t = Instant::now();
        let d = data(&[("session.realtime", json!(false)), ("strategy.fuel_laps_left", json!(0.5))]);
        assert!(a.update(&d, &cfg(), t).is_empty());
        let d = data(&[("session.realtime", json!(true)), ("strategy.fuel_laps_left", json!(0.5))]);
        assert!(a.update(&d, &Config { enabled: false, ..cfg() }, t).is_empty());
    }

    #[test]
    fn yellow_flag_and_safety_car() {
        let mut a = Alerts::default();
        let t = Instant::now();
        let d = |phase: i64, yellow: i64| data(&[("session.realtime", json!(true)), ("session.phase", json!(phase)), ("session.yellow", json!(yellow))]);
        assert!(a.update(&d(5, 0), &cfg(), t).is_empty());
        assert_eq!(texts(a.update(&d(5, 1), &cfg(), t)), vec!["Drapeau jaune"]);
        // Tant que le jaune dure, pas de répétition.
        assert!(a.update(&d(5, 1), &cfg(), t + Duration::from_secs(1)).is_empty());
        assert_eq!(texts(a.update(&d(6, 1), &cfg(), t + Duration::from_secs(2))), vec!["Full course jaune"]);
        assert_eq!(texts(a.update(&d(5, 0), &cfg(), t + Duration::from_secs(40))), vec!["Course relancée, drapeau vert"]);
    }

    #[test]
    fn cold_tyres_need_eight_seconds() {
        let mut a = Alerts::default();
        let t = Instant::now();
        let d = data(&[
            ("session.realtime", json!(true)),
            ("player.speed", json!(120.0)),
            ("tyre.fl.carcass", json!(60.0)),
            ("tyre.fr.carcass", json!(60.0)),
            ("tyre.rl.carcass", json!(60.0)),
            ("tyre.rr.carcass", json!(60.0)),
            ("tyre.fl.optimal", json!(88.0)),
        ]);
        assert!(a.update(&d, &cfg(), t).is_empty());
        assert!(a.update(&d, &cfg(), t + Duration::from_secs(5)).is_empty());
        assert_eq!(texts(a.update(&d, &cfg(), t + Duration::from_secs(9))), vec!["Pneus froids"]);
        // Délai de relance de 2 minutes.
        assert!(a.update(&d, &cfg(), t + Duration::from_secs(30)).is_empty());
    }
}
