//! « Le jeu publie-t-il de nouvelles données ? » Les compteurs d'événements de LMU restent figés (relevé réel avec `--probe` :
//! 0 changement en 20 s), on s'appuie donc sur trois témoins : ces compteurs au cas où, le temps de session (≈ 5 Hz)
//! et le temps propre de la voiture du joueur (≈ 20 Hz). Un seul des trois qui bouge suffit.

use lmu::reader::Reader;

pub struct Pulse {
    events: u32,
    session_et: f64,
    car_et: f64,
    car_index: Option<usize>,
}

impl Default for Pulse {
    fn default() -> Self {
        Self::new()
    }
}

impl Pulse {
    pub fn new() -> Self {
        Self { events: u32::MAX, session_et: f64::NAN, car_et: f64::NAN, car_index: None }
    }

    /// Repart de zéro (jeu rouvert) : le premier appel à `changed` renvoie `true`.
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// `true` si quelque chose a bougé depuis le dernier appel.
    pub fn changed(&mut self, r: &Reader) -> bool {
        let events = r.events().SME_UPDATE_TELEMETRY;
        let session_et = r.scoring_info().mCurrentET;
        let session_moved = session_et != self.session_et;
        // L'index de la voiture du joueur n'est recherché que quand le classement a bougé (≈ 5 fois par seconde).
        if session_moved || self.car_index.is_none() {
            self.car_index = r.player_indexes().map(|p| p.1);
        }
        let car_et = self.car_index.and_then(|i| r.telemetry(i)).map(|t| t.mElapsedTime).unwrap_or(f64::NAN);
        let moved = events != self.events || session_moved || (car_et.is_finite() && car_et != self.car_et);
        self.events = events;
        self.session_et = session_et;
        self.car_et = car_et;
        moved
    }
}
