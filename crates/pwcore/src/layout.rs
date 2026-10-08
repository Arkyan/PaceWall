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

pub fn default_layout() -> Layout {
    let w = |id: &str, x: f32, y: f32| WidgetSpec::new(id, id, x, y);
    Layout {
        widgets: vec![
            w("standings", 0.012, 0.05),
            w("timing", 0.012, 0.42),
            w("fuel", 0.012, 0.66),
            w("tyres", 0.18, 0.66),
            w("session", 0.42, 0.02),
            w("inputs", 0.40, 0.76),
        ],
    }
}

pub fn path() -> PathBuf {
    crate::db::path().with_file_name("layouts.json")
}

pub fn load() -> Layouts {
    std::fs::read_to_string(path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

/// Mise en page d'une cible (`player` ou `obs`), ou celle par défaut.
pub fn load_target(target: &str) -> Layout {
    load().targets.remove(target).unwrap_or_else(default_layout)
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
