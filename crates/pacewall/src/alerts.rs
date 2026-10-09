//! Règles des alertes vocales : carburant et énergie, pneus (froids, chauds, usés, crevés), drapeaux jaunes et pluie.
//! Ne fait que décider quoi dire ; la parole elle-même est dans `speech`. Chaque règle a son délai de relance pour ne
//! jamais répéter une alerte en boucle.

use crate::speech::Engine;
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
    /// Voix : ingénieur (homme ou femme) ou synthèse Windows.
    pub engine: Engine,
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
            engine: Engine::from_setting(&pwcore::settings::str_of("voice_engine", "engineer_m")),
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
    /// Secteurs en jaune à la lecture précédente (secteurs 1, 2, 3).
    yellow_before: [bool; 3],
    blue_before: bool,
    last_lap_before: bool,
    /// Dernier instant en neutralisation : la relance a sa propre annonce, pas besoin de « fin du jaune » en plus.
    fcy_seen: Option<Instant>,
    last_best: Option<f64>,
    rear_before: Option<f64>,
    limiter_since: Option<Instant>,
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
            // Jaune local : le jeu indique quels secteurs sont concernés.
            let yellow = [1, 2, 3].map(|s| data.flag(&format!("session.sector_yellow.{s}")));
            if let Some(before) = self.last_phase {
                if phase == 6 && before != 6 {
                    say.push(self::say("fcy", "Full course yellow"));
                } else if before == 6 && phase == 5 {
                    say.push(self::say("green", "Course relancée, drapeau vert"));
                }
            }
            // Annonce quand un secteur passe au jaune : « Drapeau jaune, secteur 2 » (ou sans précision s'il y en a plusieurs).
            let appeared = (0..3).any(|i| yellow[i] && !self.yellow_before[i]);
            if appeared && phase != 6 && self.ready("yellow", now, 6) {
                let sectors: Vec<usize> = (0..3).filter(|i| yellow[*i]).map(|i| i + 1).collect();
                match sectors.as_slice() {
                    [1] => say.push(self::say("yellow_s1", "Drapeau jaune, secteur un")),
                    [2] => say.push(self::say("yellow_s2", "Drapeau jaune, secteur deux")),
                    [3] => say.push(self::say("yellow_s3", "Drapeau jaune, secteur trois")),
                    _ => say.push(self::say("yellow", "Drapeau jaune")),
                }
            }
            self.last_phase = Some(phase);
            // Fin du jaune : tous les secteurs sont repassés au vert (hors neutralisation et sa relance).
            if phase == 6 {
                self.fcy_seen = Some(now);
            }
            let after_fcy = self.fcy_seen.is_some_and(|t| now.duration_since(t) < Duration::from_secs(15));
            if self.yellow_before.iter().any(|y| *y) && !yellow.iter().any(|y| *y) && phase != 6 && !after_fcy && self.ready("yellow_end", now, 6) {
                say.push(self::say("yellow_end", "Fin du jaune"));
            }
            // Dernier tour : état « dernier tour » du jeu, ou tour courant = nombre de tours de la course.
            let max_laps = data.f("session.max_laps", 0.0);
            let last_lap = data.f("session.yellow", 0.0) as i64 == 5 || (max_laps > 0.0 && max_laps < 10_000.0 && data.f("lap.number", 0.0) >= max_laps);
            if last_lap && !self.last_lap_before && self.ready("last_lap", now, 60) {
                say.push(self::say("last_lap", "Dernier tour"));
            }
            self.last_lap_before = last_lap;
            self.yellow_before = yellow;
            // Drapeau bleu : une voiture plus rapide arrive, on prévient quand il apparaît (pas en boucle).
            let blue = data.flag("player.blue_flag");
            if blue && !self.blue_before && self.ready("blue", now, 12) {
                say.push(self::say("blue", "Drapeau bleu"));
            }
            self.blue_before = blue;
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

        // Meilleur tour personnel : le meilleur temps de la session baisse.
        if let Some(best) = data.num("position.best_lap").filter(|v| *v > 0.0) {
            if self.last_best.is_some_and(|prev| best < prev - 0.0005) && self.ready("best_lap", now, 10) {
                say.push(self::say("best_lap", "Meilleur tour personnel"));
            }
            self.last_best = Some(best);
        }
        // Voiture derrière qui se rapproche (course) : l'écart passe sous une seconde après avoir été plus grand.
        let is_race = (10..=13).contains(&(data.f("session.type", -1.0) as i64));
        match data.num("gap.car_behind").filter(|v| *v > 0.0) {
            Some(gap) => {
                if is_race && speed > 50.0 && gap < 1.0 && self.rear_before.is_some_and(|prev| prev >= 1.3) && self.ready("rear_close", now, 45) {
                    say.push(self::say("rear_close", "Voiture derrière, elle se rapproche"));
                }
                self.rear_before = Some(gap);
            }
            None => self.rear_before = None,
        }
        // Limiteur de vitesse : aux stands, à plus de 70 km/h depuis une seconde, sans le limiteur.
        if data.flag("pit.in_pits") && !data.flag("car.speed_limiter") && speed > 70.0 {
            let since = *self.limiter_since.get_or_insert(now);
            if now.duration_since(since) >= Duration::from_secs(1) && self.ready("pit_limiter", now, 20) {
                say.push(self::say("pit_limiter", "Active le limiteur"));
            }
        } else {
            self.limiter_since = None;
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
        Config { enabled: true, engine: Engine::Male, volume: 80, fuel: true, tyres: true, flags: true, weather: true }
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
        let d = |phase: i64, s1: bool, s2: bool, s3: bool| {
            data(&[
                ("session.realtime", json!(true)),
                ("session.phase", json!(phase)),
                ("session.sector_yellow.1", json!(s1)),
                ("session.sector_yellow.2", json!(s2)),
                ("session.sector_yellow.3", json!(s3)),
            ])
        };
        assert!(a.update(&d(5, false, false, false), &cfg(), t).is_empty());
        // Un seul secteur : on le nomme.
        assert_eq!(texts(a.update(&d(5, false, true, false), &cfg(), t)), vec!["Drapeau jaune, secteur deux"]);
        // Tant que le jaune dure, pas de répétition.
        assert!(a.update(&d(5, false, true, false), &cfg(), t + Duration::from_secs(1)).is_empty());
        // Un autre secteur passe au jaune : plusieurs secteurs, annonce sans précision.
        assert_eq!(texts(a.update(&d(5, true, true, false), &cfg(), t + Duration::from_secs(10))), vec!["Drapeau jaune"]);
        assert_eq!(texts(a.update(&d(6, false, false, false), &cfg(), t + Duration::from_secs(20))), vec!["Full course yellow"]);
        assert_eq!(texts(a.update(&d(5, false, false, false), &cfg(), t + Duration::from_secs(50))), vec!["Course relancée, drapeau vert"]);
    }

    #[test]
    fn blue_flag_announced_once_per_appearance() {
        let mut a = Alerts::default();
        let t = Instant::now();
        let d = |blue: bool| data(&[("session.realtime", json!(true)), ("player.blue_flag", json!(blue))]);
        assert!(a.update(&d(false), &cfg(), t).is_empty());
        assert_eq!(texts(a.update(&d(true), &cfg(), t)), vec!["Drapeau bleu"]);
        // Il reste affiché : pas de répétition ; il disparaît puis revient après le délai : nouvelle annonce.
        assert!(a.update(&d(true), &cfg(), t + Duration::from_secs(3)).is_empty());
        assert!(a.update(&d(false), &cfg(), t + Duration::from_secs(5)).is_empty());
        assert_eq!(texts(a.update(&d(true), &cfg(), t + Duration::from_secs(20))), vec!["Drapeau bleu"]);
    }

    #[test]
    fn race_alerts() {
        let mut a = Alerts::default();
        let t = Instant::now();
        let base = |extra: &[(&str, Value)]| {
            let mut v = vec![("session.realtime", json!(true)), ("player.speed", json!(150.0)), ("session.type", json!(10))];
            v.extend(extra.iter().cloned());
            data(&v)
        };
        // Meilleur tour : annoncé seulement quand il baisse.
        assert!(a.update(&base(&[("position.best_lap", json!(100.0))]), &cfg(), t).is_empty());
        assert_eq!(texts(a.update(&base(&[("position.best_lap", json!(99.2))]), &cfg(), t + Duration::from_secs(1))), vec!["Meilleur tour personnel"]);
        assert!(a.update(&base(&[("position.best_lap", json!(99.2))]), &cfg(), t + Duration::from_secs(2)).is_empty());
        // Voiture derrière : plus de 1,3 s puis moins d'une seconde.
        assert!(a.update(&base(&[("gap.car_behind", json!(2.0))]), &cfg(), t + Duration::from_secs(3)).is_empty());
        assert_eq!(texts(a.update(&base(&[("gap.car_behind", json!(0.8))]), &cfg(), t + Duration::from_secs(4))), vec!["Voiture derrière, elle se rapproche"]);
        assert!(a.update(&base(&[("gap.car_behind", json!(0.7))]), &cfg(), t + Duration::from_secs(5)).is_empty());
        // Dernier tour : état 5 du jeu.
        assert_eq!(texts(a.update(&base(&[("session.yellow", json!(5))]), &cfg(), t + Duration::from_secs(6))), vec!["Dernier tour"]);
        // Limiteur : aux stands, vite, sans limiteur, pendant plus d'une seconde.
        let pit = base(&[("pit.in_pits", json!(true)), ("car.speed_limiter", json!(false))]);
        assert!(a.update(&pit, &cfg(), t + Duration::from_secs(10)).is_empty());
        assert_eq!(texts(a.update(&pit, &cfg(), t + Duration::from_secs(12))), vec!["Active le limiteur"]);
    }

    #[test]
    fn yellow_end_announced() {
        let mut a = Alerts::default();
        let t = Instant::now();
        let d = |s2: bool| data(&[("session.realtime", json!(true)), ("session.phase", json!(5)), ("session.sector_yellow.2", json!(s2))]);
        assert_eq!(texts(a.update(&d(true), &cfg(), t)), vec!["Drapeau jaune, secteur deux"]);
        assert_eq!(texts(a.update(&d(false), &cfg(), t + Duration::from_secs(10))), vec!["Fin du jaune"]);
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
