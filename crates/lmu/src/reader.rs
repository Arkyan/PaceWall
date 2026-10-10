//! Lecture de la zone `LMU_Data` en lecture seule, sans jamais copier les ~320 Ko en entier :
//! on ne lit que la voiture du joueur, l'en-tête de session ou une ligne du classement.

#![cfg(windows)]

use crate::data::*;
use core::ptr::addr_of;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Memory::{
    MapViewOfFile, OpenFileMappingW, UnmapViewOfFile, FILE_MAP_READ, MEMORY_MAPPED_VIEW_ADDRESS,
};

/// Texte C (octets terminés par 0) vers `String`.
pub fn c_str(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

pub struct Reader {
    handle: HANDLE,
    view: MEMORY_MAPPED_VIEW_ADDRESS,
}

// La vue est en lecture seule et on ne la lit que par copie.
unsafe impl Send for Reader {}

impl Reader {
    /// `None` tant que le jeu n'a pas créé la zone.
    pub fn open() -> Option<Reader> {
        let name: Vec<u16> = SHARED_MEMORY_FILE.encode_utf16().chain(Some(0)).collect();
        unsafe {
            let handle = OpenFileMappingW(FILE_MAP_READ.0, false, PCWSTR(name.as_ptr())).ok()?;
            let view = MapViewOfFile(handle, FILE_MAP_READ, 0, 0, 0);
            if view.Value.is_null() {
                let _ = CloseHandle(handle);
                return None;
            }
            Some(Reader { handle, view })
        }
    }

    fn base(&self) -> *const ObjectOut {
        self.view.Value as *const ObjectOut
    }

    pub fn events(&self) -> Event {
        unsafe { addr_of!((*self.base()).generic.events).read_unaligned() }
    }

    pub fn game_version(&self) -> i32 {
        unsafe { addr_of!((*self.base()).generic.gameVersion).read_unaligned() }
    }

    pub fn scoring_info(&self) -> ScoringInfo {
        unsafe { addr_of!((*self.base()).scoring.scoringInfo).read_unaligned() }
    }

    /// Ligne du classement `index` (0 à `mNumVehicles - 1`).
    pub fn vehicle_scoring(&self, index: usize) -> Option<VehicleScoring> {
        if index >= MAX_MAPPED_VEHICLES {
            return None;
        }
        unsafe {
            let first = addr_of!((*self.base()).scoring.vehScoringInfo) as *const VehicleScoring;
            Some(first.add(index).read_unaligned())
        }
    }

    /// Index du joueur dans le classement : la ligne dont `mIsPlayer` vaut 1.
    /// Ne lit que ce octet pour chaque ligne (pas de copie de 584 octets).
    pub fn player_scoring_index(&self) -> Option<usize> {
        let count = (self.scoring_info_count() as usize).min(MAX_MAPPED_VEHICLES);
        unsafe {
            let first = addr_of!((*self.base()).scoring.vehScoringInfo) as *const VehicleScoring;
            (0..count).find(|&i| addr_of!((*first.add(i)).mIsPlayer).read_unaligned() != 0)
        }
    }

    fn scoring_info_count(&self) -> i32 {
        unsafe { addr_of!((*self.base()).scoring.scoringInfo.mNumVehicles).read_unaligned() }
    }

    /// Index de télémétrie de la voiture dont l'identifiant (`mID`) est `id`.
    /// Les index du classement et de la télémétrie peuvent différer : on associe par `mID`.
    pub fn telemetry_index_by_id(&self, id: i32) -> Option<usize> {
        let count = (self.active_vehicles() as usize).min(MAX_MAPPED_VEHICLES);
        unsafe {
            let first = addr_of!((*self.base()).telemetry.telemInfo) as *const VehicleTelemetry;
            (0..count).find(|&i| addr_of!((*first.add(i)).mID).read_unaligned() == id)
        }
    }

    /// Joueur local : (index classement, index télémétrie).
    pub fn player_indexes(&self) -> Option<(usize, usize)> {
        let scoring = self.player_scoring_index()?;
        let id = unsafe {
            let first = addr_of!((*self.base()).scoring.vehScoringInfo) as *const VehicleScoring;
            addr_of!((*first.add(scoring)).mID).read_unaligned()
        };
        Some((scoring, self.telemetry_index_by_id(id)?))
    }

    pub fn active_vehicles(&self) -> u8 {
        unsafe { addr_of!((*self.base()).telemetry.activeVehicles).read_unaligned() }
    }

    /// Télémétrie d'une voiture (les adversaires ont aussi une entrée).
    pub fn telemetry(&self, index: usize) -> Option<VehicleTelemetry> {
        if index >= MAX_MAPPED_VEHICLES {
            return None;
        }
        unsafe {
            let first = addr_of!((*self.base()).telemetry.telemInfo) as *const VehicleTelemetry;
            Some(first.add(index).read_unaligned())
        }
    }

    /// Position, cap et vitesse de toutes les voitures, lus dans leur télémétrie (plus fraîche et plus précise que le
    /// classement). Ne lit que ces champs, pas les ~1,9 Ko de chaque voiture.
    pub fn poses(&self) -> Vec<Pose> {
        let count = (self.active_vehicles() as usize).min(MAX_MAPPED_VEHICLES);
        unsafe {
            let first = addr_of!((*self.base()).telemetry.telemInfo) as *const VehicleTelemetry;
            (0..count)
                .map(|i| {
                    let t = first.add(i);
                    let pos = addr_of!((*t).mPos).read_unaligned();
                    let vel = addr_of!((*t).mLocalVel).read_unaligned();
                    let ori = addr_of!((*t).mOri).read_unaligned();
                    Pose {
                        id: addr_of!((*t).mID).read_unaligned(),
                        elapsed: addr_of!((*t).mElapsedTime).read_unaligned(),
                        x: pos.x,
                        z: pos.z,
                        // Même formule que TinyPedal : angle de la troisième ligne de la matrice d'orientation.
                        yaw: ori[2].x.atan2(ori[2].z),
                        speed: (vel.x * vel.x + vel.y * vel.y + vel.z * vel.z).sqrt(),
                    }
                })
                .collect()
        }
    }

    pub fn player_telemetry(&self) -> Option<VehicleTelemetry> {
        self.telemetry(self.player_indexes()?.1)
    }
}

impl Drop for Reader {
    fn drop(&mut self) {
        unsafe {
            let _ = UnmapViewOfFile(self.view);
            let _ = CloseHandle(self.handle);
        }
    }
}

/// Position (monde, mètres), cap (radians) et vitesse (m/s) d'une voiture à l'instant `elapsed` de sa télémétrie.
#[derive(Clone, Copy, Debug)]
pub struct Pose {
    pub id: i32,
    pub elapsed: f64,
    pub x: f64,
    pub z: f64,
    pub yaw: f64,
    pub speed: f64,
}
