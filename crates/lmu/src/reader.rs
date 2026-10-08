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

    pub fn player_index(&self) -> Option<usize> {
        unsafe {
            let has = addr_of!((*self.base()).telemetry.playerHasVehicle).read_unaligned();
            let idx = addr_of!((*self.base()).telemetry.playerVehicleIdx).read_unaligned() as usize;
            (has != 0 && idx < MAX_MAPPED_VEHICLES).then_some(idx)
        }
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

    pub fn player_telemetry(&self) -> Option<VehicleTelemetry> {
        self.telemetry(self.player_index()?)
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
