//! Préférences communes à tous les widgets : unités et seuils de couleur (onglet Réglages).
//! Lues dans `settings.json`, gardées en mémoire, et relues au plus une fois par seconde si le fichier a changé :
//! les widgets peuvent donc les consulter à chaque image sans rien coûter.

use pwcore::settings;
use std::sync::Mutex;
use std::time::{Instant, SystemTime};

#[derive(Clone, Copy, PartialEq)]
pub enum Pressure {
    Kpa,
    Bar,
    Psi,
}

#[derive(Clone, Copy)]
pub struct Prefs {
    pub mph: bool,
    pub fahrenheit: bool,
    pub gallons: bool,
    pub pressure: Pressure,
    /// Tours de carburant (ou d'énergie) restants : orange en dessous de `fuel_warn`, rouge en dessous de `fuel_crit`.
    pub fuel_warn: f64,
    pub fuel_crit: f64,
    /// État des pneus en % : orange en dessous de `wear_warn`, rouge en dessous de `wear_crit`.
    pub wear_warn: f64,
    pub wear_crit: f64,
    /// Écart à la température idéale (°C) à partir duquel un pneu est tout bleu ou tout rouge.
    pub temp_window: f64,
}

impl Prefs {
    fn load() -> Self {
        let all = settings::all();
        let text = |k: &str, d: &str| all.get(k).and_then(|v| v.as_str()).unwrap_or(d).to_string();
        let num = |k: &str, d: f64| all.get(k).and_then(|v| v.as_f64()).unwrap_or(d);
        Self {
            mph: text("unit_speed", "kmh") == "mph",
            fahrenheit: text("unit_temp", "c") == "f",
            gallons: text("unit_fuel", "l") == "gal",
            pressure: match text("unit_pressure", "kpa").as_str() {
                "bar" => Pressure::Bar,
                "psi" => Pressure::Psi,
                _ => Pressure::Kpa,
            },
            fuel_warn: num("thr_fuel_warn", 6.0),
            fuel_crit: num("thr_fuel_crit", 3.0),
            wear_warn: num("thr_wear_warn", 55.0),
            wear_crit: num("thr_wear_crit", 30.0),
            temp_window: num("thr_temp_window", 25.0).max(1.0),
        }
    }

    /// Vitesse dans l'unité choisie, avec son nom.
    pub fn speed(&self, kmh: f64) -> (f64, &'static str) {
        if self.mph { (kmh * 0.621_371, "mph") } else { (kmh, "km/h") }
    }

    pub fn temp(&self, celsius: f64) -> f64 {
        if self.fahrenheit { celsius * 9.0 / 5.0 + 32.0 } else { celsius }
    }

    /// Volume de carburant dans l'unité choisie (litres ou gallons américains), avec son nom.
    pub fn fuel(&self, litres: f64) -> (f64, &'static str) {
        if self.gallons { (litres * 0.264_172, "gal") } else { (litres, "L") }
    }

    pub fn pressure(&self, kpa: f64) -> String {
        match self.pressure {
            Pressure::Bar => format!("{:.2} bar", kpa / 100.0),
            Pressure::Psi => format!("{:.1} psi", kpa * 0.145_038),
            Pressure::Kpa => format!("{kpa:.0} kPa"),
        }
    }
}

struct Cache {
    prefs: Prefs,
    checked: Instant,
    stamp: Option<SystemTime>,
}

static CACHE: Mutex<Option<Cache>> = Mutex::new(None);

pub fn get() -> Prefs {
    let mut guard = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    match guard.as_mut() {
        Some(c) => {
            if c.checked.elapsed().as_secs_f32() >= 1.0 {
                c.checked = Instant::now();
                let stamp = settings::modified();
                if stamp != c.stamp {
                    c.stamp = stamp;
                    c.prefs = Prefs::load();
                }
            }
            c.prefs
        }
        None => {
            let prefs = Prefs::load();
            *guard = Some(Cache { prefs, checked: Instant::now(), stamp: settings::modified() });
            prefs
        }
    }
}
