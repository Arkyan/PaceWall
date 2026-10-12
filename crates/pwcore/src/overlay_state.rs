//! Ce que le joueur voit de son overlay, publié par l'overlay pour que les widgets d'OBS fassent pareil : affichée ou
//! masquée (menus, pause, jeu en arrière-plan), et les types de widgets masqués par un raccourci.
//! Un petit fichier, réécrit seulement quand l'état change.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct State {
    pub visible: bool,
    #[serde(default)]
    pub hidden: Vec<String>,
}

fn path() -> PathBuf {
    crate::db::path().with_file_name("overlay-state.json")
}

pub fn publish(state: &State) {
    if let Ok(text) = serde_json::to_string(state) {
        let _ = std::fs::write(path(), text);
    }
}

pub fn read() -> Option<State> {
    serde_json::from_str(&std::fs::read_to_string(path()).ok()?).ok()
}
