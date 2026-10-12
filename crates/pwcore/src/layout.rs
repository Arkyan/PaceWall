//! Mises en page des overlays (`layouts.json`) : une pour le joueur (overlay native) et une pour OBS (navigateur).
//! Positions en fraction de l'écran, tailles relatives à une hauteur de 1080 px : elles suivent la résolution.
//! Le même fichier est lu par l'overlay native et par les pages web d'OBS.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WidgetSpec {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub x: f32,
    pub y: f32,
    #[serde(default = "one")]
    pub scale: f32,
    #[serde(default = "one")]
    pub opacity: f32,
    #[serde(default)]
    pub opts: Map<String, Value>,
}

fn one() -> f32 {
    1.0
}

impl WidgetSpec {
    pub fn new(id: &str, kind: &str, x: f32, y: f32) -> Self {
        Self { id: id.into(), kind: kind.into(), x, y, scale: 1.0, opacity: 1.0, opts: Map::new() }
    }
    pub fn flag(&self, key: &str, default: bool) -> bool {
        self.opts.get(key).and_then(Value::as_bool).unwrap_or(default)
    }
    pub fn num(&self, key: &str, default: f64) -> f64 {
        match self.opts.get(key) {
            Some(Value::Number(n)) => n.as_f64().unwrap_or(default),
            Some(Value::String(s)) => s.parse().unwrap_or(default),
            _ => default,
        }
    }
    pub fn choice<'a>(&'a self, key: &str, default: &'a str) -> &'a str {
        self.opts.get(key).and_then(Value::as_str).unwrap_or(default)
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Layout {
    #[serde(default)]
    pub widgets: Vec<WidgetSpec>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Layouts {
    #[serde(default = "one_u32")]
    pub version: u32,
    #[serde(default)]
    pub targets: BTreeMap<String, Layout>,
}

fn one_u32() -> u32 {
    1
}

impl Default for Layouts {
    fn default() -> Self {
        Self { version: 1, targets: BTreeMap::new() }
    }
}

/// Emplacement conseillé de chaque type de widget (fractions d'un écran 16:9) : là où le configurateur de premier
/// lancement le pose. Les widgets ne se chevauchent pas ; chacun se déplace ensuite dans l'éditeur.
pub fn suggested(kind: &str) -> (f32, f32) {
    match kind {
        "standings" => (0.012, 0.05),
        "relative" => (0.012, 0.47),
        "timing" => (0.012, 0.76),
        "session" => (0.42, 0.02),
        "flags" => (0.455, 0.14),
        "trackmap" => (0.80, 0.03),
        "forecast" => (0.80, 0.34),
        "damage" => (0.85, 0.56),
        "pit" => (0.83, 0.78),
        "fuel" => (0.19, 0.80),
        "inputs" => (0.40, 0.82),
        "tyres" => (0.64, 0.78),
        "radar" => (0.445, 0.56),
        _ => (0.40, 0.40),
    }
}

/// Mise en page faite des types de widgets choisis, chacun à son emplacement conseillé.
pub fn from_kinds(kinds: &[&str]) -> Layout {
    Layout { widgets: kinds.iter().map(|k| { let (x, y) = suggested(k); WidgetSpec::new(k, k, x, y) }).collect() }
}

pub fn path() -> PathBuf {
    crate::db::path().with_file_name("layouts.json")
}

pub fn load() -> Layouts {
    std::fs::read_to_string(path()).ok().and_then(|t| serde_json::from_str(t.trim_start_matches('\u{feff}')).ok()).unwrap_or_default()
}

/// Mise en page d'une cible (`player` ou `obs`). Vide tant que l'utilisateur n'a rien choisi : une installation neuve
/// n'affiche aucun widget, c'est le configurateur de premier lancement qui les propose.
pub fn load_target(target: &str) -> Layout {
    load().targets.remove(target).unwrap_or_default()
}

pub fn save(layouts: &Layouts) -> std::io::Result<()> {
    let p = path();
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(p, serde_json::to_string(layouts)?)
}

pub fn modified() -> Option<std::time::SystemTime> {
    std::fs::metadata(path()).and_then(|m| m.modified()).ok()
}
