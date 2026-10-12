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

/// Une simulation (`pacewall.exe --simulate`) est-elle en cours ? Elle tient un verrou nommé pendant qu'elle tourne.
pub fn simulation_running() -> bool {
    use windows::core::w;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{OpenMutexW, SYNCHRONIZATION_SYNCHRONIZE};
    unsafe {
        match OpenMutexW(SYNCHRONIZATION_SYNCHRONIZE, false, w!("Local\\PaceWallSimulator")) {
            Ok(h) => {
                let _ = CloseHandle(h);
                true
            }
            Err(_) => false,
        }
    }
}

/// Le programme du jeu tourne-t-il ? Un jeu en pause ne publie plus rien mais n'est pas absent : c'est ce qui les distingue.
/// Si la liste des processus est illisible, on suppose qu'il tourne.
pub fn process_alive() -> bool {
    if simulation_running() {
        return true;
    }
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS};
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return true };
        let mut entry = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut found = false;
        if Process32FirstW(snap, &mut entry).is_ok() {
            loop {
                let len = entry.szExeFile.iter().position(|c| *c == 0).unwrap_or(entry.szExeFile.len());
                let name = String::from_utf16_lossy(&entry.szExeFile[..len]).to_lowercase();
                // « fakegame » : le faux jeu des essais de développement.
                if name == "le mans ultimate.exe" || name == "lmu.exe" || name == "fakegame.exe" {
                    found = true;
                    break;
                }
                if Process32NextW(snap, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snap);
        found
    }
}

/// Vérifie `process_alive` au plus une fois par seconde.
pub struct AliveCheck {
    last: Instant,
    alive: bool,
}

impl Default for AliveCheck {
    fn default() -> Self {
        Self { last: Instant::now(), alive: true }
    }
}

impl AliveCheck {
    pub fn alive(&mut self) -> bool {
        if self.last.elapsed() >= Duration::from_secs(1) {
            self.last = Instant::now();
            self.alive = process_alive();
        }
        self.alive
    }

    /// À appeler quand le jeu vient d'être (re)trouvé.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

pub struct Live {
    reader: Option<Reader>,
    strategy: Strategy,
    pulse: Pulse,
    alive: AliveCheck,
    /// Le jeu a publié du neuf que personne n'a encore lu (la lecture complète n'est faite que sur demande).
    pending: bool,
    last_change: Instant,
    last_open: Instant,
    reported_absent: bool,
    /// Voie des stands du circuit en cours (relue toutes les 5 s : l'enregistreur peut venir de l'apprendre).
    pit_lane: Option<(String, Option<crate::pitlane::PitLane>, Instant)>,
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
            alive: AliveCheck::default(),
            pending: false,
            last_change: Instant::now(),
            last_open: Instant::now() - Duration::from_secs(10),
            reported_absent: false,
            pit_lane: None,
        }
    }

    /// À appeler à la cadence voulue. `with_grid` : lire aussi le classement complet (plus coûteux).
    pub fn poll(&mut self, with_grid: bool) -> Update {
        self.poll_if(with_grid, true)
    }

    /// Comme `poll`, mais la lecture complète (coûteuse) n'a lieu que si `want` : sinon on constate seulement que le jeu
    /// publie, et les nouveautés sont lues au prochain appel avec `want`.
    pub fn poll_if(&mut self, with_grid: bool, want: bool) -> Update {
        if self.reader.is_none() {
            // Tant que le jeu n'a pas créé sa zone, on n'essaie que toutes les 2 s.
            if self.last_open.elapsed() < Duration::from_secs(2) {
                return Update::Unchanged;
            }
            self.last_open = Instant::now();
            self.reader = Reader::open();
            self.pulse.reset();
            self.alive.reset();
            self.strategy = Strategy::default();
            if self.reader.is_none() {
                return self.absent();
            }
        }
        let reader = self.reader.as_ref().unwrap();
        if self.pulse.changed(reader) {
            self.last_change = Instant::now();
            self.reported_absent = false;
            self.pending = true;
        }
        if self.pending && want {
            self.pending = false;
            let mut fields = flatten::read_all(reader, with_grid);
            self.strategy.update(&mut fields);
            // Prévisions météo : ajoutées seulement si un widget ou une alerte les a demandées.
            let forecast = crate::forecast::fields(&fields);
            fields.extend(forecast);
            // État de la voiture et durée d'arrêt (service web du jeu) : mêmes conditions.
            fields.extend(crate::carinfo::fields());
            // Voie des stands apprise pour ce circuit : de quoi calculer le temps réellement perdu par un arrêt.
            let track = fields.iter().find(|f| f.key == "session.track").and_then(|f| f.value.as_str()).unwrap_or("").to_string();
            if self.pit_lane.as_ref().is_none_or(|(t, _, at)| *t != track || at.elapsed() > Duration::from_secs(5)) {
                let lane = if track.is_empty() { None } else { crate::pitlane::load(&track) };
                self.pit_lane = Some((track, lane, Instant::now()));
            }
            if let Some((_, Some(lane), _)) = &self.pit_lane {
                if lane.known() {
                    for (key, label, v) in [("pit.lane_entry", "Entrée des stands", lane.entry), ("pit.lane_exit", "Sortie des stands", lane.exit), ("pit.lane_speed", "Vitesse du limiteur", lane.speed())] {
                        fields.push(crate::flatten::Field { key: key.into(), unit: if key.ends_with("speed") { "m/s" } else { "m" }, label, value: serde_json::json!(v) });
                    }
                }
            }
            return Update::Data(Data::new(fields));
        }
        // Plus rien ne bouge : jeu en pause (on garde la zone et les dernières données) ou jeu fermé (on la lâche).
        if self.last_change.elapsed() > Duration::from_secs(3) && !self.alive.alive() {
            self.reader = None;
            self.pending = false;
            return self.absent();
        }
        Update::Unchanged
    }

    /// Temps écoulé depuis la dernière donnée nouvelle du jeu (en pause, plus rien ne bouge).
    pub fn idle(&self) -> Duration {
        self.last_change.elapsed()
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
    process_alive() && Reader::open().is_some()
}
