//! Faux jeu : crée la zone `LMU_Data` et y écrit une course simulée. Permet de développer les overlays et d'essayer
//! PaceWall (alertes vocales comprises) sans Le Mans Ultimate. Utilisé par le programme `fakegame` et par
//! `pacewall.exe --simulate`.

/// Réglages de la simulation.
#[derive(Clone, Copy, Debug)]
pub struct Options {
    /// Durée d'un tour en secondes.
    pub lap_seconds: f64,
    /// Compteurs d'événements figés, comme dans le vrai jeu (voir `pacewall.exe --probe`).
    pub frozen_events: bool,
    /// Drapeau jaune de 4 s toutes les 12 s (essai simple des alertes).
    pub yellow: bool,
    /// Scénario complet de 90 s qui déclenche chaque alerte vocale : carburant, énergie, pneus froids puis chauds puis
    /// usés, jaune, full course yellow, pluie, crevaison. Il se répète.
    pub scenario: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self { lap_seconds: 100.0, frozen_events: true, yellow: false, scenario: false }
    }
}

/// Durée d'un cycle du scénario, en secondes.
pub const SCENARIO_CYCLE: f64 = 90.0;

/// Ce que le scénario impose à l'instant `c` (secondes depuis le début du cycle).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scene {
    pub fuel_laps_start: f64,
    pub carcass_c: f64,
    pub wear: f64,
    pub yellow: bool,
    pub phase: u8,
    pub rain: f64,
    pub flat: bool,
}

/// Déroulé du scénario : 0-20 s pneus froids, 20-50 s réchauffe, 50-65 s trop chauds, puis normaux ; carburant qui
/// baisse par paliers de tours, jaune à 30 s, full course yellow à 50 s, pluie à 70 s, crevaison à 75 s, usure critique à 75 s.
pub fn scene(c: f64) -> Scene {
    let carcass_c = if c < 20.0 {
        60.0
    } else if c < 40.0 {
        60.0 + (c - 20.0) / 20.0 * 30.0
    } else if c < 50.0 {
        90.0
    } else if c < 65.0 {
        125.0
    } else {
        90.0
    };
    Scene {
        fuel_laps_start: 5.6,
        carcass_c,
        wear: (1.0 - 0.9 * c / SCENARIO_CYCLE).max(0.0),
        yellow: (30.0..38.0).contains(&c) || (50.0..60.0).contains(&c),
        phase: if (50.0..60.0).contains(&c) { 6 } else { 5 },
        rain: if (70.0..85.0).contains(&c) { 0.15 } else { 0.0 },
        flat: (75.0..78.0).contains(&c),
    }
}

#[cfg(windows)]
pub fn run(opts: Options, stop: &dyn Fn() -> bool) -> Result<(), String> {
    use core::mem::{size_of, zeroed};
    use core::ptr::addr_of_mut;
    use lmu::*;
    use std::{thread, time::{Duration, Instant}};
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows::Win32::System::Memory::{CreateFileMappingW, MapViewOfFile, FILE_MAP_ALL_ACCESS, PAGE_READWRITE};

    fn put(dst: &mut [u8], text: &str) {
        let n = text.len().min(dst.len() - 1);
        dst[..n].copy_from_slice(&text.as_bytes()[..n]);
        dst[n] = 0;
    }

    // Garde-fou : si la zone existe déjà (vrai jeu ou autre simulation), on ne l'écrase surtout pas.
    if lmu::reader::Reader::open().is_some() {
        return Err("La zone LMU_Data existe déjà : le jeu (ou une autre simulation) tourne.".into());
    }

    let size = size_of::<ObjectOut>();
    let name: Vec<u16> = SHARED_MEMORY_FILE.encode_utf16().chain(Some(0)).collect();
    let base = unsafe {
        let handle = CreateFileMappingW(INVALID_HANDLE_VALUE, None, PAGE_READWRITE, 0, size as u32, PCWSTR(name.as_ptr()))
            .map_err(|e| format!("création de la zone impossible : {e}"))?;
        let view = MapViewOfFile(handle, FILE_MAP_ALL_ACCESS, 0, 0, size);
        if view.Value.is_null() {
            return Err("projection de la zone impossible".into());
        }
        view.Value as *mut ObjectOut
    };

    let lap_seconds = opts.lap_seconds;
    let start = Instant::now();
    let (mut telem_count, mut scor_count) = (0u32, 0u32);
    let drivers = ["Toi", "A. Martin", "B. Dupont", "C. Rossi", "D. Keller"];

    while !stop() {
        let t = start.elapsed().as_secs_f64();
        let lap_time = lap_seconds;
        let lap = (t / lap_time) as i32;
        let progress = (t % lap_time) / lap_time;
        let sc = if opts.scenario { Some(scene(t % SCENARIO_CYCLE)) } else { None };
        telem_count += 1;
        scor_count += 1;

        unsafe {
            // Télémétrie du joueur (index 0)
            let mut v: VehicleTelemetry = zeroed();
            v.mID = 0;
            v.mElapsedTime = t;
            v.mLapNumber = lap;
            v.mLapStartET = lap as f64 * lap_time;
            put(&mut v.mVehicleName, "Toyota GR010 #7");
            put(&mut v.mTrackName, "Circuit de la Sarthe");
            let speed = 60.0 + 20.0 * (progress * 12.56).sin();
            v.mLocalVel = Vect3 { x: 0.0, y: 0.0, z: -speed };
            v.mGear = 4 + (2.0 * (progress * 12.56).sin()).round() as i32;
            v.mEngineRPM = 6500.0 + 1500.0 * (progress * 12.56).sin();
            v.mEngineMaxRPM = 8000.0;
            v.mEngineWaterTemp = 88.0;
            v.mEngineOilTemp = 104.0;
            v.mFilteredThrottle = ((progress * 12.56).sin() * 0.5 + 0.5).clamp(0.0, 1.0);
            v.mFilteredBrake = ((progress * 12.56).cos() * 0.5 - 0.1).clamp(0.0, 1.0);
            v.mFilteredSteering = (progress * 25.0).sin() * 0.4;
            v.mFuelCapacity = 90.0;
            v.mFuel = match sc {
                // Scénario : 3,6 L par tour, on part de 5,6 tours et on « ravitaille » à chaque nouveau cycle.
                Some(_) => (3.6 * (5.6 - ((t % SCENARIO_CYCLE) / lap_time).min(5.6))).max(0.0),
                None => (90.0 - 3.6 * (t / lap_time)).max(0.0),
            };
            v.mVirtualEnergy = match sc {
                Some(_) => (0.5 - 0.09 * ((t % SCENARIO_CYCLE) / lap_time) as f32).max(0.0),
                None => (1.0 - 0.035 * (t / lap_time) as f32).max(0.0),
            };
            v.mDeltaBest = 0.3 * (progress * 6.28).sin();
            v.mTimeGapCarAhead = 2.1;
            v.mTimeGapCarBehind = 3.4;
            v.mBatteryChargeFraction = 0.6;
            v.mMaxGears = 7;
            v.mRearBrakeBias = 0.54;
            v.mTC = 3;
            v.mTCMax = 8;
            v.mABS = 2;
            v.mABSMax = 11;
            put(&mut v.mFrontTireCompoundName, "Medium");
            put(&mut v.mRearTireCompoundName, "Medium");
            for (i, w) in v.mWheels.iter_mut().enumerate() {
                let front = i < 2;
                w.mBrakeTemp = 350.0 + 250.0 * v.mFilteredBrake + if front { 40.0 } else { 0.0 };
                w.mPressure = 165.0 + i as f64;
                w.mTemperature = [80.0 + 273.15, 85.0 + 273.15, 90.0 + 273.15];
                w.mTireCarcassTemperature = sc.map_or(82.0, |s| s.carcass_c) + 273.15;
                // mWear : bande de gomme restante, 1 = neuve.
                w.mWear = sc.map_or((1.0 - 0.012 * (t / lap_time)).max(0.0), |s| s.wear);
                w.mFlat = (i == 0 && sc.is_some_and(|s| s.flat)) as u8;
                w.mOptimalTemp = 88.0;
                w.mCompoundType = 1;
            }
            addr_of_mut!((*base).telemetry.telemInfo).cast::<VehicleTelemetry>().write_unaligned(v);
            addr_of_mut!((*base).telemetry.activeVehicles).write_unaligned(drivers.len() as u8);
            addr_of_mut!((*base).telemetry.playerVehicleIdx).write_unaligned(0);
            addr_of_mut!((*base).telemetry.playerHasVehicle).write_unaligned(1);

            // Classement
            let mut info: ScoringInfo = zeroed();
            put(&mut info.mTrackName, "Circuit de la Sarthe");
            info.mSession = 10;
            info.mCurrentET = t;
            info.mEndET = 24.0 * 3600.0;
            info.mMaxLaps = 0;
            info.mLapDist = 4000.0;
            info.mNumVehicles = drivers.len() as i32;
            info.mGamePhase = sc.map_or(5, |s| s.phase);
            // Jaune local dans le secteur 2 (ordre naturel : l'élément 1 du tableau). Le full course yellow passe par la phase 6.
            info.mSectorFlag = [0, (sc.map_or(opts.yellow && (t as u64 % 12) >= 8, |s| s.yellow)) as u8, 0];
            info.mInRealtime = 1;
            put(&mut info.mPlayerName, "Toi");
            info.mRaining = sc.map_or(0.0, |s| s.rain);
            info.mAmbientTemp = 21.0;
            info.mTrackTemp = 32.0;
            info.mSessionTimeRemaining = (24.0 * 3600.0 - t) as f32;
            addr_of_mut!((*base).scoring.scoringInfo).write_unaligned(info);

            for (i, name) in drivers.iter().enumerate() {
                let mut s: VehicleScoring = zeroed();
                s.mID = i as i32;
                put(&mut s.mDriverName, name);
                put(&mut s.mVehicleName, "Hypercar");
                put(&mut s.mVehicleClass, "Hypercar");
                s.mTotalLaps = lap as i16;
                s.mLapDist = (progress * 4000.0 - i as f64 * 40.0).rem_euclid(4000.0);
                // Secteurs comme dans le jeu : 1 = secteur 1, 2 = secteur 2, 0 = secteur 3.
                s.mSector = if s.mLapDist < 1240.0 { 1 } else if s.mLapDist < 2680.0 { 2 } else { 0 };
                // Position dans le monde : un circuit fermé de forme quelconque, pour tester la carte.
                let a = s.mLapDist / 4000.0 * std::f64::consts::TAU;
                s.mPos = Vect3 { x: 500.0 * a.cos() + 130.0 * (3.0 * a).cos(), y: 0.0, z: 300.0 * a.sin() + 90.0 * (2.0 * a).sin() };
                s.mBestLapTime = lap_seconds - 0.6 + i as f64 * 0.3;
                s.mLastLapTime = lap_seconds - 0.2 + i as f64 * 0.3 + 0.4 * (lap as f64 * 1.7).sin();
                s.mIsPlayer = (i == 0) as u8;
                s.mPlace = i as u8 + 1;
                s.mTimeBehindNext = 1.5;
                s.mTimeBehindLeader = i as f64 * 1.5;
                addr_of_mut!((*base).scoring.vehScoringInfo).cast::<VehicleScoring>().add(i).write_unaligned(s);
            }

            if !opts.frozen_events {
                addr_of_mut!((*base).generic.events.SME_UPDATE_TELEMETRY).write_unaligned(telem_count);
                addr_of_mut!((*base).generic.events.SME_UPDATE_SCORING).write_unaligned(scor_count);
            }
        }
        thread::sleep(Duration::from_millis(16));
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn run(_: Options, _: &dyn Fn() -> bool) -> Result<(), String> {
    Err("fakegame ne fonctionne que sous Windows.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scenario_covers_every_alert() {
        // Pneus froids au début, chauds à 55 s, usure critique (< 25 % restant) avant la fin du cycle.
        assert!(scene(10.0).carcass_c < 70.0);
        assert!(scene(55.0).carcass_c > 110.0);
        assert!(scene(80.0).wear < 0.25);
        assert!(scene(33.0).yellow && scene(33.0).phase == 5);
        assert_eq!(scene(55.0).phase, 6);
        assert!(scene(72.0).rain > 0.05);
        assert!(scene(76.0).flat);
        assert!(!scene(5.0).yellow && !scene(5.0).flat);
    }
}
