//! Réglages de l'application (`settings.json`). Lus et écrits par l'interface et par le processus de fond.

use serde_json::{json, Map, Value};
use std::path::PathBuf;

pub fn path() -> PathBuf {
    crate::db::path().with_file_name("settings.json")
}

fn defaults() -> Map<String, Value> {
    let Value::Object(m) = json!({
        "overlay_auto": true,          // ouvrir / fermer l'overlay avec le jeu
        "overlay_fg_only": true,       // masquer l'overlay quand le jeu n'est pas au premier plan
        "overlay_hide_idle": true,     // masquer l'overlay en pause, au menu ou hors piste
        "voice_enabled": true,         // alertes vocales
        "voice_engine": "engineer_m", // "engineer_m" (homme) ou "engineer_f" (femme) : clips d'ingénieur ; "windows" : voix de synthèse
        "voice_volume": 80,            // volume de la voix (0 à 100)
        "voice_fuel": true,            // carburant et énergie
        "voice_tyres": true,           // pneus froids, chauds, usés, crevés
        "voice_flags": true,           // drapeau jaune, full course yellow
        "voice_weather": true,         // pluie
        "yellow_sector_order": "natural", // ordre des secteurs dans les drapeaux jaunes du jeu : "natural" (1, 2, 3) ou "game" (3, 1, 2)
        "replay_record": true,         // enregistrer les courses pour les revoir (position et classement de toutes les voitures)
        "replay_keep": 15,             // nombre de replays conservés (les plus anciens sont supprimés)
        "editor_background": "",       // image de fond de l'éditeur (capture du jeu) : chemin, vide = aucune
        "tutorial_done": false,        // le tutoriel de premier lancement a été vu
        "overlay_monitor": 0,          // index de l'écran (0 = principal)
        "autostart": false,            // lancer avec Windows
        "obs_server": false,           // serveur web pour OBS (source navigateur)
        "obs_port": 8470,
        "obs_fps": 30,                 // images par seconde au plus pour les widgets d'OBS (5 à 60)
        "lmu_api_port": 6397,          // port du service web local de Le Mans Ultimate (prévisions météo)
        // Unités (communes à tous les widgets)
        "unit_speed": "kmh",           // "kmh" ou "mph"
        "unit_temp": "c",              // "c" ou "f"
        "unit_pressure": "kpa",        // "kpa", "bar" ou "psi"
        "unit_fuel": "l",              // "l" (litres) ou "gal" (gallons américains)
        // Seuils de couleur
        "thr_fuel_warn": 6,            // tours de carburant restants : orange en dessous
        "thr_fuel_crit": 3,            // … rouge en dessous
        "thr_wear_warn": 55,           // état des pneus (%) : orange en dessous
        "thr_wear_crit": 30,           // … rouge en dessous
        "thr_temp_window": 25,         // écart à la température idéale (°C) où le pneu est tout bleu ou tout rouge
        "ui_accent": "cyan",           // couleur d'accent de la fenêtre PaceWall
        // Raccourcis : touche (« Ctrl+Alt+O ») ou bouton de volant (« Joy1:B7 »), vide = aucun
        "hk_overlay": "Ctrl+Alt+O",    // afficher / fermer l'overlay
        "hk_voice": "",                // couper / remettre la voix
        "hk_fuel": "", "hk_tyres": "", "hk_timing": "", "hk_inputs": "", "hk_standings": "", "hk_session": "",
        "hk_trackmap": "", "hk_flags": "", "hk_relative": "", "hk_pit": "", "hk_radar": "", "hk_forecast": "",
    }) else { unreachable!() };
    m
}

/// Réglages : valeurs par défaut complétées par le fichier.
pub fn all() -> Value {
    let mut merged = defaults();
    if let Ok(text) = std::fs::read_to_string(path()) {
        // Un fichier modifié avec le Bloc-notes peut commencer par un BOM.
        if let Ok(Value::Object(saved)) = serde_json::from_str::<Value>(text.trim_start_matches('\u{feff}')) {
            for (k, v) in saved {
                if merged.get(&k).is_some_and(|d| std::mem::discriminant(d) == std::mem::discriminant(&v)) {
                    merged.insert(k, v);
                }
            }
        }
    }
    Value::Object(merged)
}

pub fn str_of(key: &str, default: &str) -> String {
    all().get(key).and_then(Value::as_str).unwrap_or(default).to_string()
}

pub fn bool_of(key: &str, default: bool) -> bool {
    all().get(key).and_then(Value::as_bool).unwrap_or(default)
}

pub fn int_of(key: &str, default: i64) -> i64 {
    all().get(key).and_then(Value::as_i64).unwrap_or(default)
}

/// Enregistre une clé connue (les autres sont ignorées).
pub fn set(key: &str, value: Value) -> std::io::Result<()> {
    let mut current = match all() {
        Value::Object(m) => m,
        _ => defaults(),
    };
    if current.contains_key(key) {
        current.insert(key.to_string(), value);
    }
    let p = path();
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(p, serde_json::to_string_pretty(&Value::Object(current))?)
}

/// Date de dernière modification du fichier : le processus de fond s'en sert pour détecter un changement.
pub fn modified() -> Option<std::time::SystemTime> {
    std::fs::metadata(path()).and_then(|m| m.modified()).ok()
}
