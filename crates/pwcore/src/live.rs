//! Lecture du jeu : une instance par consommateur (overlay, interface, serveur OBS).
//! Ne copie jamais la zone mémoire en entier : voir `lmu::reader`.

use crate::data::Data;
use crate::{flatten, strategy::Strategy};
use crate::pulse::Pulse;
use lmu::reader::Reader;
use std::time::{Duration, Instant};

pub enum Update {
    /// Nouvelles données (le compteur de mises à jour du jeu a bougé).
    Data(Data),
    /// Jeu absent, fermé ou en pause longue.
    NoGame,
    /// Rien de nouveau depuis le dernier appel.
    Unchanged,
}

pub struct Live {
    reader: Option<Reader>,
    strategy: Strategy,
    pulse: Pulse,
    last_change: Instant,
    last_open: Instant,
    reported_absent: bool,
}

impl Default for Live {
    fn default() -> Self {
        Self::new()
    }
}

impl Live {
    pub fn new() -> Self {
        Self {
            reader: None,
            strategy: Strategy::default(),
            pulse: Pulse::new(),
            last_change: Instant::now(),
            last_open: Instant::now() - Duration::from_secs(10),
            reported_absent: false,
        }
    }

    /// À appeler à la cadence voulue. `with_grid` : lire aussi le classement complet (plus coûteux).
    pub fn poll(&mut self, with_grid: bool) -> Update {
        if self.reader.is_none() {
            // Tant que le jeu n'a pas créé sa zone, on n'essaie que toutes les 2 s.
            if self.last_open.elapsed() < Duration::from_secs(2) {
                return Update::Unchanged;
            }
            self.last_open = Instant::now();
            self.reader = Reader::open();
            self.pulse.reset();
            self.strategy = Strategy::default();
            if self.reader.is_none() {
                return self.absent();
            }
        }
        let reader = self.reader.as_ref().unwrap();
        if self.pulse.changed(reader) {
            self.last_change = Instant::now();
            self.reported_absent = false;
            let mut fields = flatten::read_all(reader, with_grid);
            self.strategy.update(&mut fields);
            return Update::Data(Data::new(fields));
        }
        if self.last_change.elapsed() > Duration::from_secs(3) {
            self.reader = None;
            return self.absent();
        }
        Update::Unchanged
    }

    fn absent(&mut self) -> Update {
        // On ne le signale qu'une fois : les consommateurs n'ont pas à se réveiller pour rien.
        if self.reported_absent {
            Update::Unchanged
        } else {
            self.reported_absent = true;
            Update::NoGame
        }
    }
}

/// Le jeu a-t-il créé sa zone mémoire ? (test d'existence, quasi gratuit)
pub fn game_running() -> bool {
    Reader::open().is_some()
}
