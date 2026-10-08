//! Valeurs de stratégie calculées côté serveur (donc identiques pour toutes les overlays, et conservées
//! si une overlay est rechargée) : consommation moyenne, tours restants, quantité à ajouter.
//!
//! La consommation d'un tour n'est retenue que si le tour est complet (on a vu son début) et sans arrêt
//! aux stands. La moyenne porte sur les 3 derniers tours propres.

use crate::flatten::{Field, Out};
use std::collections::VecDeque;

const KEPT_LAPS: usize = 3;

#[derive(Default)]
pub struct Strategy {
    lap: Option<i64>,
    fuel0: f64,
    energy0: f64,
    pit: bool,
    partial: bool,
    fuel: VecDeque<f64>,
    energy: VecDeque<f64>,
}

fn num(fields: &[Field], key: &str) -> Option<f64> {
    fields.iter().find(|f| f.key == key).and_then(|f| f.value.as_f64())
}

fn flag(fields: &[Field], key: &str) -> bool {
    fields.iter().find(|f| f.key == key).and_then(|f| f.value.as_bool()).unwrap_or(false)
}

fn mean(v: &VecDeque<f64>) -> Option<f64> {
    (!v.is_empty()).then(|| v.iter().sum::<f64>() / v.len() as f64)
}

fn push(v: &mut VecDeque<f64>, x: f64) {
    if v.len() == KEPT_LAPS {
        v.pop_front();
    }
    v.push_back(x);
}

impl Strategy {
    /// Met à jour l'état avec les champs lus, puis ajoute les champs `strategy.*`.
    pub fn update(&mut self, fields: &mut Vec<Field>) {
        let (Some(lap), Some(fuel)) = (num(fields, "lap.number"), num(fields, "fuel.level")) else { return };
        let lap = lap as i64;
        let energy = num(fields, "energy.virtual").unwrap_or(0.0);
        let in_pits = flag(fields, "pit.in_pits");
        let lap_dist = num(fields, "position.lap_dist").unwrap_or(0.0);

        match self.lap {
            // Première lecture, ou retour en arrière (nouvelle session) : on repart de zéro.
            None => self.reset(lap, fuel, energy, lap_dist, in_pits),
            Some(prev) if lap < prev => self.reset(lap, fuel, energy, lap_dist, in_pits),
            Some(prev) if lap > prev => {
                if !self.partial && !self.pit {
                    let used = self.fuel0 - fuel;
                    if used > 0.05 {
                        push(&mut self.fuel, used);
                    }
                    let used = self.energy0 - energy;
                    if energy > 0.0 && used > 0.05 {
                        push(&mut self.energy, used);
                    }
                }
                self.lap = Some(lap);
                self.fuel0 = fuel;
                self.energy0 = energy;
                self.pit = in_pits;
                self.partial = false;
            }
            _ => {}
        }
        self.pit |= in_pits;

        let mut o = Out(Vec::new());
        let fuel_per_lap = mean(&self.fuel);
        let energy_per_lap = mean(&self.energy);
        let mut laps_left: Option<(f64, &str)> = None;

        if let Some(per) = fuel_per_lap {
            let left = fuel / per;
            o.num("strategy.fuel_per_lap", "L", "Conso carburant par tour", per);
            o.num("strategy.fuel_laps_left", "tours", "Tours de carburant", left);
            laps_left = Some((left, "fuel"));
        }
        if let Some(per) = energy_per_lap {
            let left = energy / per;
            o.num("strategy.energy_per_lap", "%", "Conso énergie par tour", per);
            o.num("strategy.energy_laps_left", "tours", "Tours d'énergie", left);
            if laps_left.is_none_or(|(l, _)| left < l) {
                laps_left = Some((left, "energy"));
            }
        }
        if let Some((left, which)) = laps_left {
            o.num("strategy.laps_left", "tours", "Tours restants (limitant)", left);
            o.text("strategy.limiting", "Limitant", which.to_string());
        }

        // Tours restants de la course : par nombre de tours, sinon par temps restant / durée d'un tour.
        let max_laps = num(fields, "session.max_laps").unwrap_or(0.0);
        let done = num(fields, "position.laps").unwrap_or(0.0);
        let lap_time = num(fields, "position.last_lap").filter(|t| *t > 0.0).or(num(fields, "position.best_lap").filter(|t| *t > 0.0));
        let race_left = if max_laps > 0.0 && max_laps < 10_000.0 {
            Some((max_laps - done).max(0.0))
        } else {
            match (num(fields, "session.remaining"), lap_time) {
                (Some(rem), Some(t)) if rem > 0.0 && rem < 1.0e6 => Some((rem / t).ceil()),
                _ => None,
            }
        };
        if let Some(race_left) = race_left {
            o.num("strategy.race_laps_left", "tours", "Tours restants de la course", race_left);
            if let Some(per) = fuel_per_lap {
                o.num("strategy.fuel_to_add", "L", "Carburant à ajouter", (race_left * per - fuel).max(0.0));
            }
            if let Some(per) = energy_per_lap {
                o.num("strategy.energy_to_add", "%", "Énergie à ajouter", (race_left * per - energy).max(0.0));
            }
        }
        fields.extend(o.0);
    }

    fn reset(&mut self, lap: i64, fuel: f64, energy: f64, lap_dist: f64, in_pits: bool) {
        self.fuel.clear();
        self.energy.clear();
        self.lap = Some(lap);
        self.fuel0 = fuel;
        self.energy0 = energy;
        self.pit = in_pits;
        // On se branche en cours de tour : ce tour-là est incomplet.
        self.partial = lap_dist > 300.0;
    }
}
