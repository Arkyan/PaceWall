//! État de la voiture et arrêt au stand, lus dans le service web local de Le Mans Ultimate (comme les prévisions
//! météo, et comme le fait TinyPedal) : ces données ne sont pas dans la mémoire partagée.
//!
//! - `/rest/strategy/pitstop-estimate` : durée de l'arrêt estimée par le jeu (carburant, pneus, réparations).
//! - `/rest/garage/UIScreen/RepairAndRefuel` : dégâts aérodynamiques, dégâts des suspensions, épaisseur des freins.
//!
//! Rien n'est demandé au jeu tant que personne ne s'y intéresse : un widget appelle `touch()`, et un fil d'exécution
//! interroge alors le service toutes les 2 secondes. Sans réponse, les champs sont simplement absents.

use crate::flatten::Field;
use serde_json::{json, Value};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const WHEELS: [&str; 4] = ["fl", "fr", "rl", "rr"];

/// Ce que le jeu a répondu à la dernière interrogation.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Info {
    /// Durée de l'arrêt (secondes), et la part due aux réparations.
    pub stop_time: Option<f64>,
    pub repair_time: Option<f64>,
    /// Dégâts aérodynamiques, de 0 (intacte) à 1.
    pub aero: Option<f64>,
    /// Dégâts de chaque suspension, de 0 à 1.
    pub suspension: Option<[f64; 4]>,
    /// Épaisseur restante de chaque frein, en mètres.
    pub brakes: Option<[f64; 4]>,
}

struct State {
    touched: Option<Instant>,
    started: bool,
    info: Info,
    /// Plus grande épaisseur vue pour chaque frein : la référence « frein neuf » du relais en cours.
    brake_max: [f64; 4],
}

static STATE: Mutex<State> = Mutex::new(State { touched: None, started: false, info: Info { stop_time: None, repair_time: None, aero: None, suspension: None, brakes: None }, brake_max: [0.0; 4] });

/// À appeler par quiconque affiche ces données : tant qu'on l'appelle, elles sont tenues à jour.
pub fn touch() {
    let mut s = STATE.lock().unwrap_or_else(|e| e.into_inner());
    s.touched = Some(Instant::now());
    if !s.started {
        s.started = true;
        drop(s);
        std::thread::spawn(worker);
    }
}

fn wanted() -> bool {
    STATE.lock().unwrap_or_else(|e| e.into_inner()).touched.is_some_and(|t| t.elapsed() < Duration::from_secs(30))
}

fn worker() {
    loop {
        std::thread::sleep(Duration::from_secs(2));
        if !wanted() {
            continue;
        }
        let port = crate::forecast::port();
        let mut info = Info::default();
        if let Some(text) = crate::forecast::http_get(port, "/rest/strategy/pitstop-estimate") {
            parse_pitstop(&text, &mut info);
        }
        if let Some(text) = crate::forecast::http_get(port, "/rest/garage/UIScreen/RepairAndRefuel") {
            parse_repair(&text, &mut info);
        }
        let mut s = STATE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(b) = info.brakes {
            for i in 0..4 {
                // Frein plus épais qu'avant : il vient d'être changé, c'est la nouvelle référence.
                s.brake_max[i] = s.brake_max[i].max(b[i]);
            }
        }
        s.info = info;
    }
}

/// `{ "total": 38.5, "damage": 12.0, … }`
fn parse_pitstop(text: &str, info: &mut Info) {
    let Ok(json) = serde_json::from_str::<Value>(text) else { return };
    info.stop_time = json.get("total").and_then(Value::as_f64).filter(|v| *v >= 0.0);
    info.repair_time = json.get("damage").and_then(Value::as_f64).filter(|v| *v >= 0.0);
}

/// `{ "wearables": { "body": { "aero": 0.1 }, "brakes": [m, m, m, m], "suspension": [0, 0.2, 0, 0] }, … }`
fn parse_repair(text: &str, info: &mut Info) {
    let Ok(json) = serde_json::from_str::<Value>(text) else { return };
    let Some(w) = json.get("wearables") else { return };
    let four = |v: Option<&Value>| -> Option<[f64; 4]> {
        let a = v?.as_array()?;
        Some([a.first()?.as_f64()?, a.get(1)?.as_f64()?, a.get(2)?.as_f64()?, a.get(3)?.as_f64()?])
    };
    info.aero = w.get("body").and_then(|b| b.get("aero")).and_then(Value::as_f64).filter(|v| *v >= 0.0);
    info.suspension = four(w.get("suspension"));
    info.brakes = four(w.get("brakes"));
}

/// Les champs à ajouter aux données lues dans le jeu. Vide si personne ne les a demandés.
pub fn fields() -> Vec<Field> {
    let (info, brake_max) = {
        let s = STATE.lock().unwrap_or_else(|e| e.into_inner());
        if !s.touched.is_some_and(|t| t.elapsed() < Duration::from_secs(30)) {
            return Vec::new();
        }
        (s.info.clone(), s.brake_max)
    };
    build(&info, &brake_max)
}

/// Champs d'un état donné. `brake_max` : épaisseur de référence de chaque frein (0 si inconnue).
pub fn build(info: &Info, brake_max: &[f64; 4]) -> Vec<Field> {
    let mut out = Vec::new();
    let mut put = |key: String, unit: &'static str, label: &'static str, v: f64| out.push(Field { key, unit, label, value: json!((v * 100.0).round() / 100.0) });
    if let Some(t) = info.stop_time {
        put("pit.stop_time".into(), "s", "Durée de l'arrêt estimée", t);
    }
    if let Some(t) = info.repair_time {
        put("pit.repair_time".into(), "s", "Dont réparations", t);
    }
    if let Some(a) = info.aero {
        put("car.aero_damage".into(), "%", "Dégâts aérodynamiques", a.clamp(0.0, 1.0) * 100.0);
    }
    if let Some(s) = info.suspension {
        for (i, w) in WHEELS.iter().enumerate() {
            put(format!("car.suspension.{w}"), "%", "Dégâts de suspension", s[i].clamp(0.0, 1.0) * 100.0);
        }
    }
    if let Some(b) = info.brakes {
        for (i, w) in WHEELS.iter().enumerate() {
            put(format!("car.brake_mm.{w}"), "mm", "Épaisseur du frein", b[i] * 1000.0);
            if brake_max[i] > 0.0 {
                put(format!("car.brake_left.{w}"), "%", "Frein restant (par rapport au plus épais vu)", (b[i] / brake_max[i]).clamp(0.0, 1.0) * 100.0);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_game_answers() {
        let mut info = Info::default();
        parse_pitstop(r#"{"total": 38.5, "damage": 12.0, "fuel": 20.0}"#, &mut info);
        parse_repair(r#"{"wearables": {"body": {"aero": 0.25}, "brakes": [0.030, 0.030, 0.021, 0.028], "suspension": [0, 0.4, 0, 0]}, "pitMenu": {}}"#, &mut info);
        assert_eq!((info.stop_time, info.repair_time, info.aero), (Some(38.5), Some(12.0), Some(0.25)));
        let fields = build(&info, &[0.030, 0.030, 0.030, 0.0]);
        let get = |k: &str| fields.iter().find(|f| f.key == k).and_then(|f| f.value.as_f64());
        assert_eq!(get("pit.stop_time"), Some(38.5));
        assert_eq!(get("car.aero_damage"), Some(25.0));
        assert_eq!(get("car.suspension.fr"), Some(40.0));
        assert_eq!(get("car.brake_mm.rl"), Some(21.0));
        assert_eq!(get("car.brake_left.rl"), Some(70.0));
        assert_eq!(get("car.brake_left.rr"), None); // pas de référence pour ce frein
        // Réponses absentes ou illisibles : aucun champ, pas d'erreur.
        let mut empty = Info::default();
        parse_pitstop("pas du json", &mut empty);
        parse_repair("{}", &mut empty);
        assert!(build(&empty, &[0.0; 4]).is_empty());
    }
}
