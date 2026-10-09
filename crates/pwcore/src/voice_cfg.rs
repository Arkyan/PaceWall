//! Réglages des alertes vocales, une par une (`alerts.json`) : activée ou non, phrase choisie, phrase personnalisée.
//! Le catalogue décrit chaque alerte et les phrases enregistrées de l'ingénieur ; l'interface (onglet Alertes) et
//! l'overlay (qui parle) lisent le même fichier.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Une alerte du catalogue.
pub struct AlertDef {
    /// Identifiant enregistré dans `alerts.json`.
    pub id: &'static str,
    pub group: &'static str,
    pub label: &'static str,
    /// Clés des clips de l'ingénieur qu'elle peut jouer (le premier sert à l'essai).
    pub clips: &'static [&'static str],
    /// Phrases enregistrées (variantes 1 et 2), pour les afficher et les choisir.
    pub phrases: &'static [&'static str],
}

pub const GROUPS: [&str; 5] = ["Carburant", "Énergie", "Pneus", "Drapeaux", "Météo"];

pub const CATALOG: &[AlertDef] = &[
    AlertDef { id: "fuel_5", group: "Carburant", label: "Cinq tours de carburant", clips: &["fuel_5"], phrases: &["Carburant, il reste cinq tours.", "Cinq tours de carburant."] },
    AlertDef { id: "fuel_3", group: "Carburant", label: "Trois tours de carburant", clips: &["fuel_3"], phrases: &["Attention carburant, trois tours restants.", "Trois tours de carburant, prépare ton arrêt."] },
    AlertDef { id: "fuel_2", group: "Carburant", label: "Deux tours de carburant", clips: &["fuel_2"], phrases: &["Carburant critique, deux tours.", "Plus que deux tours de carburant, rentre au stand."] },
    AlertDef { id: "fuel_1", group: "Carburant", label: "Dernier tour de carburant", clips: &["fuel_1"], phrases: &["Dernier tour de carburant, rentre au stand maintenant !", "Un tour de carburant, rentre immédiatement !"] },
    AlertDef { id: "energy_3", group: "Énergie", label: "Trois tours d'énergie", clips: &["energy_3"], phrases: &["Énergie, trois tours restants.", "Attention à l'énergie, trois tours."] },
    AlertDef { id: "energy_2", group: "Énergie", label: "Deux tours d'énergie", clips: &["energy_2"], phrases: &["Énergie critique, deux tours.", "Plus que deux tours d'énergie."] },
    AlertDef { id: "energy_1", group: "Énergie", label: "Dernier tour d'énergie", clips: &["energy_1"], phrases: &["Dernier tour d'énergie, rentre au stand !", "Un tour d'énergie, rentre maintenant !"] },
    AlertDef { id: "tyres_cold", group: "Pneus", label: "Pneus froids", clips: &["tyres_cold"], phrases: &["Pneus froids, fais-les monter en température.", "Attention, tes pneus sont froids."] },
    AlertDef { id: "tyres_hot", group: "Pneus", label: "Pneus trop chauds", clips: &["tyres_hot"], phrases: &["Pneus trop chauds, ménage-les.", "Température des pneus trop élevée, lève un peu le pied."] },
    AlertDef { id: "tyres_worn", group: "Pneus", label: "Pneus usés", clips: &["tyres_worn"], phrases: &["Pneus usés, pense à ton arrêt.", "Les pneus sont en fin de vie."] },
    AlertDef { id: "flat", group: "Pneus", label: "Crevaison", clips: &["flat"], phrases: &["Crevaison ! Rentre au stand !", "Tu as une crevaison, rentre doucement !"] },
    AlertDef { id: "yellow", group: "Drapeaux", label: "Drapeau jaune (avec le secteur)", clips: &["yellow_s2", "yellow", "yellow_s1", "yellow_s3"], phrases: &["Drapeau jaune, secteur N.", "Jaune dans le secteur N, lève le pied."] },
    AlertDef { id: "fcy", group: "Drapeaux", label: "Full course yellow", clips: &["fcy"], phrases: &["Full course yellow.", "Full course yellow, respecte les écarts et reste concentré."] },
    AlertDef { id: "green", group: "Drapeaux", label: "Relance, drapeau vert", clips: &["green"], phrases: &["Course relancée, drapeau vert.", "Feu vert, c'est reparti, bonne relance."] },
    AlertDef { id: "rain", group: "Météo", label: "Pluie détectée", clips: &["rain"], phrases: &["Pluie détectée.", "Il commence à pleuvoir, prépare tes pneus pluie."] },
];

/// Réglage d'une alerte.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AlertCfg {
    /// Alerte active.
    #[serde(default = "yes")]
    pub on: bool,
    /// « auto » : les phrases alternent ; « 0 » ou « 1 » : toujours la même.
    #[serde(default = "auto")]
    pub variant: String,
    /// Phrase personnalisée : si elle n'est pas vide, elle est lue par la voix Windows (les clips ne sont pas modifiables).
    #[serde(default)]
    pub text: String,
}

fn yes() -> bool {
    true
}

fn auto() -> String {
    "auto".into()
}

impl Default for AlertCfg {
    fn default() -> Self {
        Self { on: true, variant: auto(), text: String::new() }
    }
}

impl AlertCfg {
    /// Variante imposée (0 ou 1), ou `None` pour alterner.
    pub fn fixed_variant(&self) -> Option<usize> {
        self.variant.parse().ok()
    }
}

pub type AlertSet = HashMap<String, AlertCfg>;

pub fn path() -> PathBuf {
    crate::db::path().with_file_name("alerts.json")
}

pub fn load() -> AlertSet {
    std::fs::read_to_string(path())
        .ok()
        .and_then(|t| serde_json::from_str(t.trim_start_matches('\u{feff}')).ok())
        .unwrap_or_default()
}

pub fn save(set: &AlertSet) -> std::io::Result<()> {
    let p = path();
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(p, serde_json::to_string_pretty(set)?)
}

/// Identifiant du catalogue qui joue le clip `key` (« yellow_s2 » → « yellow »).
pub fn id_of_key(key: &str) -> Option<&'static str> {
    CATALOG.iter().find(|a| a.clips.contains(&key)).map(|a| a.id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_clip_key_has_an_alert() {
        for key in ["fuel_5", "energy_1", "tyres_cold", "flat", "yellow_s3", "yellow", "fcy", "green", "rain"] {
            assert!(id_of_key(key).is_some(), "{key}");
        }
        assert_eq!(id_of_key("yellow_s1"), Some("yellow"));
    }

    #[test]
    fn partial_file_gets_defaults() {
        let set: AlertSet = serde_json::from_str(r#"{"rain":{"on":false}}"#).unwrap();
        assert!(!set["rain"].on);
        assert_eq!(set["rain"].variant, "auto");
        assert_eq!(set["rain"].fixed_variant(), None);
        assert_eq!(AlertCfg { variant: "1".into(), ..Default::default() }.fixed_variant(), Some(1));
    }
}
