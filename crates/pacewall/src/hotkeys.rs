//! Raccourcis réglables : une touche du clavier (« Ctrl+Alt+O ») ou un bouton de volant / manette (« Joy1:B7 »).
//! Le clavier passe par des raccourcis globaux de Windows (aucun coût) ; les boutons sont lus une vingtaine de fois
//! par seconde, et seulement sur les périphériques réellement utilisés par un raccourci.

use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use std::str::FromStr;
use windows::Win32::Media::Multimedia::{joyGetPosEx, JOYINFOEX, JOY_RETURNBUTTONS};

/// Actions de l'overlay : (clé du réglage, libellé).
pub const WIDGET_ACTIONS: &[(&str, &str)] = &[
    ("hk_fuel", "Carburant et énergie"),
    ("hk_tyres", "Pneus"),
    ("hk_timing", "Chrono et écarts"),
    ("hk_inputs", "Pilotage"),
    ("hk_standings", "Classement"),
    ("hk_session", "Session et météo"),
    ("hk_trackmap", "Carte du circuit"),
    ("hk_flags", "Drapeaux"),
    ("hk_relative", "Relatif"),
    ("hk_pit", "Arrêt au stand"),
    ("hk_radar", "Radar de proximité"),
];

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Binding {
    Key(HotKey),
    /// Périphérique (0 à 15) et bouton (0 à 31).
    Joy(u32, u32),
}

/// Lit un raccourci enregistré. `None` s'il est vide ou incompris.
pub fn parse(text: &str) -> Option<Binding> {
    let text = text.trim();
    if let Some(rest) = text.strip_prefix("Joy") {
        let (pad, button) = rest.split_once(":B")?;
        let (pad, button): (u32, u32) = (pad.parse().ok()?, button.parse().ok()?);
        if !(1..=16).contains(&pad) || !(1..=32).contains(&button) {
            return None;
        }
        return Some(Binding::Joy(pad - 1, button - 1));
    }
    let mut mods = Modifiers::empty();
    let mut code = None;
    for part in text.split('+').map(str::trim).filter(|p| !p.is_empty()) {
        match part.to_lowercase().as_str() {
            "ctrl" => mods |= Modifiers::CONTROL,
            "alt" => mods |= Modifiers::ALT,
            "maj" | "shift" => mods |= Modifiers::SHIFT,
            key => {
                let name = match key.len() {
                    1 if key.chars().all(|c| c.is_ascii_alphabetic()) => format!("Key{}", key.to_uppercase()),
                    1 if key.chars().all(|c| c.is_ascii_digit()) => format!("Digit{key}"),
                    _ => key.to_uppercase(), // F1 à F12
                };
                code = Code::from_str(&name).ok();
            }
        }
    }
    code.map(|c| Binding::Key(HotKey::new((!mods.is_empty()).then_some(mods), c)))
}

/// Texte d'un bouton de volant : périphériques et boutons numérotés à partir de 1.
pub fn joy_text(pad: u32, button: u32) -> String {
    format!("Joy{}:B{}", pad + 1, button + 1)
}

/// Boutons enfoncés d'un périphérique (un bit par bouton), ou `None` s'il n'est pas branché.
fn buttons(pad: u32) -> Option<u32> {
    let mut info = JOYINFOEX { dwSize: std::mem::size_of::<JOYINFOEX>() as u32, dwFlags: JOY_RETURNBUTTONS as u32, ..Default::default() };
    (unsafe { joyGetPosEx(pad, &mut info) } == 0).then_some(info.dwButtons)
}

/// Suit les boutons des volants et manettes pour repérer ceux qu'on vient d'enfoncer.
#[derive(Default)]
pub struct Pads {
    last: [u32; 16],
}

impl Pads {
    /// Boutons qui viennent d'être enfoncés sur les périphériques demandés : (périphérique, bouton).
    pub fn pressed(&mut self, pads: impl Iterator<Item = u32>) -> Vec<(u32, u32)> {
        let mut out = Vec::new();
        for pad in pads.filter(|p| *p < 16) {
            let now = buttons(pad).unwrap_or(0);
            let fresh = now & !self.last[pad as usize];
            self.last[pad as usize] = now;
            for b in 0..32 {
                if fresh & (1 << b) != 0 {
                    out.push((pad, b));
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_keys_and_buttons() {
        assert_eq!(parse("Ctrl+Alt+O"), Some(Binding::Key(HotKey::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyO))));
        assert_eq!(parse("F7"), Some(Binding::Key(HotKey::new(None, Code::F7))));
        assert_eq!(parse("Maj+3"), Some(Binding::Key(HotKey::new(Some(Modifiers::SHIFT), Code::Digit3))));
        assert_eq!(parse("Joy2:B7"), Some(Binding::Joy(1, 6)));
        assert_eq!(parse(&joy_text(0, 11)), Some(Binding::Joy(0, 11)));
        assert_eq!(parse(""), None);
        assert_eq!(parse("Joy0:B1"), None);
    }
}
