//! Publie la session simulée (`sim`) dans la zone mémoire `LMU_Data`, comme le ferait le jeu : télémétrie de la voiture du
//! joueur, état de la session, une ligne de classement par voiture. Comme dans le vrai jeu, les compteurs d'événements
//! restent figés : PaceWall repère l'activité grâce au temps de session et au temps de la voiture.

use crate::sim::{Class, Config, Kind, Pit, Sim, CLASSES};
use core::mem::{size_of, zeroed};
use core::ptr::addr_of_mut;
use lmu::*;
use std::time::{Duration, Instant};
use windows::core::PCWSTR;
use windows::Win32::Foundation::INVALID_HANDLE_VALUE;
use windows::Win32::System::Memory::{CreateFileMappingW, MapViewOfFile, FILE_MAP_ALL_ACCESS, PAGE_READWRITE};

fn put(dst: &mut [u8], text: &str) {
    let n = text.len().min(dst.len() - 1);
    dst[..n].copy_from_slice(&text.as_bytes()[..n]);
    dst[n] = 0;
}

const KELVIN: f64 = 273.15;

pub fn run(cfg: Config, stop: &dyn Fn() -> bool) -> Result<(), String> {
    // Garde-fou : jamais d'écriture dans la zone d'un vrai jeu.
    if crate::real_game_running() {
        return Err("Le Mans Ultimate tourne : la simulation ne peut pas écrire dans sa zone mémoire.".into());
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

    let mut sim = Sim::new(cfg);
    let mut last = Instant::now();
    // Session terminée (drapeau à damier, fin du temps) : on laisse 12 s pour voir l'arrivée, puis la simulation s'arrête.
    let mut over_since: Option<Instant> = None;
    while !stop() {
        let now = Instant::now();
        let real_dt = now.duration_since(last).as_secs_f64().min(0.25);
        last = now;
        // Au-delà de 8×, l'enregistrement de la carte de PaceWall voit des « téléportations » (plus de 100 m entre deux lectures).
        sim.step(real_dt * cfg.speed.clamp(1.0, 8.0));
        unsafe { publish(base, &sim) };
        if sim.phase == 8 && over_since.get_or_insert(now).elapsed() > Duration::from_secs(12) {
            break;
        }
        std::thread::sleep(Duration::from_millis(16));
    }
    Ok(())
}

fn gear_for(speed_kmh: f64) -> i32 {
    const UP: [f64; 7] = [0.0, 70.0, 110.0, 150.0, 195.0, 245.0, 290.0];
    UP.iter().rposition(|t| speed_kmh >= *t).map_or(1, |i| i as i32 + 1)
}

unsafe fn publish(base: *mut ObjectOut, sim: &Sim) {
    let p = sim.player();
    let class: &Class = &CLASSES[p.class];
    let track = &sim.track;
    let len = track.len;
    let kmh = p.v * 3.6;

    // --- télémétrie du joueur (index 0)
    let mut v: VehicleTelemetry = zeroed();
    v.mID = 0;
    v.mElapsedTime = sim.t;
    v.mLapNumber = p.laps as i32;
    v.mLapStartET = p.lap_start;
    put(&mut v.mVehicleName, &format!("{} #{}", p.model, p.number));
    put(&mut v.mTrackName, "Circuit de la Sarthe (simulé)");
    v.mLocalVel = Vect3 { x: 0.0, y: 0.0, z: -p.v };
    let gear = if p.v < 0.5 { 0 } else { gear_for(kmh) };
    v.mGear = gear;
    // Régime : monte dans chaque rapport, retombe au passage.
    let (lo, hi) = (match gear { 0 | 1 => 0.0, g => [0.0, 70.0, 110.0, 150.0, 195.0, 245.0, 290.0][(g as usize - 1).min(6)] }, [70.0, 110.0, 150.0, 195.0, 245.0, 290.0, 340.0][(gear.max(1) as usize - 1).min(6)]);
    let in_gear = ((kmh - lo) / (hi - lo)).clamp(0.0, 1.0);
    v.mEngineRPM = 4200.0 + 3500.0 * in_gear;
    v.mEngineMaxRPM = 8000.0;
    v.mEngineWaterTemp = 88.0 + 4.0 * p.throttle;
    v.mEngineOilTemp = 104.0 + 5.0 * p.throttle;
    v.mFilteredThrottle = p.throttle;
    v.mFilteredBrake = p.brake;
    v.mFilteredSteering = (track.kappa_at(p.s) * 40.0).clamp(-1.0, 1.0) * 0.55;
    v.mFuelCapacity = class.tank;
    v.mFuel = p.fuel;
    v.mVirtualEnergy = (p.energy / 100.0) as f32;
    let last_vs_best = if p.best_lap > 0.0 && p.last_lap > 0.0 { (p.last_lap - p.best_lap).clamp(-2.0, 2.0) } else { 0.0 };
    v.mDeltaBest = last_vs_best * ((p.s / len) * 3.14).sin();
    v.mTimeGapCarAhead = sim.gap_to_neighbour(0, true) as f32;
    v.mTimeGapCarBehind = sim.gap_to_neighbour(0, false) as f32;
    v.mBatteryChargeFraction = 0.55 + 0.3 * (1.0 - p.brake);
    v.mMaxGears = 7;
    v.mRearBrakeBias = 0.54;
    v.mTC = 3;
    v.mTCMax = 8;
    v.mABS = 2;
    v.mABSMax = 11;
    v.mSpeedLimiterAvailable = 1;
    v.mSpeedLimiterActive = matches!(p.pit, Pit::Lane | Pit::Out) as u8;
    let compound = if sim.weather_rain > 0.08 { "Wet" } else { "Medium" };
    put(&mut v.mFrontTireCompoundName, compound);
    put(&mut v.mRearTireCompoundName, compound);
    for (i, w) in v.mWheels.iter_mut().enumerate() {
        let front = i < 2;
        // Les roues côté extérieur du virage chauffent un peu plus.
        let side = if i % 2 == 0 { 1.0 } else { -1.0 };
        let load = (track.kappa_at(p.s) * side).clamp(-0.02, 0.02) * 120.0;
        let carcass = p.carcass + load + if front { 1.5 } else { 0.0 };
        w.mBrakeTemp = p.brake_temp + if front { 60.0 } else { 0.0 };
        w.mPressure = 160.0 + (carcass - 60.0) * 0.55 + i as f64 * 0.8;
        w.mTemperature = [carcass + 4.0 + KELVIN, carcass + 9.0 + KELVIN, carcass + 5.0 + KELVIN];
        w.mTireCarcassTemperature = carcass + KELVIN;
        w.mTireInnerLayerTemperature = [carcass - 3.0 + KELVIN; 3];
        // `mWear` : bande de gomme restante, 1 = neuve.
        w.mWear = (1.0 - p.wear - if front { 0.008 } else { 0.0 }).clamp(0.0, 1.0);
        w.mFlat = (p.flat && i == 0) as u8;
        w.mOptimalTemp = 88.0;
        w.mCompoundType = if sim.weather_rain > 0.08 { 3 } else { 1 };
    }
    // Position et cap du joueur, puis une télémétrie minimale pour chaque autre voiture (le vrai jeu en donne une
    // par voiture) : identifiant, instant, position, cap, vitesse. Les voitures sont décalées de part et d'autre de
    // la trajectoire pour que le radar ait quelque chose à montrer.
    let pose = |c: &crate::sim::Car, v: &mut VehicleTelemetry| {
        let (a, b) = (track.pos(c.s), track.pos(c.s + 1.0));
        let (dx, dz) = (b[0] - a[0], b[1] - a[1]);
        let n = (dx * dx + dz * dz).sqrt().max(1e-6);
        let (dx, dz) = (dx / n, dz / n);
        let side = if c.id == 0 { 0.0 } else if c.id % 2 == 0 { 1.7 } else { -1.7 };
        v.mPos = Vect3 { x: a[0] - dz * side, y: 0.0, z: a[1] + dx * side };
        // Cap tel que TinyPedal le relit : atan2(mOri[2].x, mOri[2].z) dans le repère (x, −z).
        v.mOri[2] = Vect3 { x: dx, y: 0.0, z: -dz };
        v.mLocalVel = Vect3 { x: 0.0, y: 0.0, z: -c.v };
        v.mElapsedTime = sim.t;
    };
    pose(p, &mut v);
    addr_of_mut!((*base).telemetry.telemInfo).cast::<VehicleTelemetry>().write_unaligned(v);
    for (k, c) in sim.cars.iter().enumerate().skip(1).take(103) {
        let mut o: VehicleTelemetry = zeroed();
        o.mID = c.id as i32;
        pose(c, &mut o);
        addr_of_mut!((*base).telemetry.telemInfo).cast::<VehicleTelemetry>().add(k).write_unaligned(o);
    }
    addr_of_mut!((*base).telemetry.activeVehicles).write_unaligned(sim.cars.len().min(104) as u8);
    addr_of_mut!((*base).telemetry.playerVehicleIdx).write_unaligned(0);
    addr_of_mut!((*base).telemetry.playerHasVehicle).write_unaligned(1);

    // --- session
    let order = sim.order();
    let race = sim.cfg.kind == Kind::Race;
    let mut info: ScoringInfo = zeroed();
    put(&mut info.mTrackName, "Circuit de la Sarthe (simulé)");
    info.mSession = sim.cfg.kind.code();
    info.mCurrentET = sim.t;
    info.mEndET = if sim.duration > 0.0 { sim.duration } else { 24.0 * 3600.0 };
    info.mMaxLaps = sim.race_laps() as i32;
    info.mLapDist = len;
    info.mNumVehicles = sim.cars.len() as i32;
    info.mGamePhase = sim.phase;
    // Dernier tour : le leader est dans son dernier tour de course (état 5 du jeu).
    let leader_laps = order.first().map_or(0, |&i| sim.cars[i].laps);
    info.mYellowFlagState = if race && sim.race_laps() > 0 && leader_laps + 1 == sim.race_laps() && !sim.leader_done { 5 } else { 0 };
    info.mSectorFlag = [sim.sector_flags[0] as u8, sim.sector_flags[1] as u8, sim.sector_flags[2] as u8];
    info.mInRealtime = 1;
    put(&mut info.mPlayerName, "Toi");
    info.mRaining = sim.weather_rain;
    info.mAvgPathWetness = sim.wetness;
    info.mAmbientTemp = 21.0 - 4.0 * sim.weather_rain;
    info.mTrackTemp = 33.0 - 9.0 * sim.wetness;
    info.mSessionTimeRemaining = sim.remaining() as f32;
    addr_of_mut!((*base).scoring.scoringInfo).write_unaligned(info);

    // --- classement
    for (rank, &i) in order.iter().enumerate().take(104) {
        let c = &sim.cars[i];
        let cl = &CLASSES[c.class];
        let mut s: VehicleScoring = zeroed();
        s.mID = c.id as i32;
        put(&mut s.mDriverName, &c.name);
        put(&mut s.mVehicleName, &format!("{} #{}", c.model, c.number));
        put(&mut s.mVehicleClass, cl.name);
        s.mTotalLaps = c.laps as i16;
        s.mLapDist = c.s.rem_euclid(len);
        s.mSector = track.sector_code(c.s);
        let pos = track.pos(c.s);
        s.mPos = Vect3 { x: pos[0], y: 0.0, z: pos[1] };
        s.mBestLapTime = c.best_lap;
        s.mLastLapTime = c.last_lap;
        s.mTimeIntoLap = if c.started { sim.t - c.lap_start } else { 0.0 };
        s.mEstimatedLapTime = if c.best_lap > 0.0 { c.best_lap } else { track.reference_lap() };
        s.mCurSector1 = c.cur_sectors[0];
        s.mCurSector2 = c.cur_sectors[1];
        s.mLastSector1 = c.last_sectors[0];
        s.mLastSector2 = c.last_sectors[1];
        s.mIsPlayer = (c.id == 0) as u8;
        s.mPlace = (rank + 1) as u8;
        s.mInPits = (c.pit != Pit::Track) as u8;
        s.mNumPitstops = c.stops as i16;
        s.mFlag = if c.id == 0 && sim.blue_flag(0) { 6 } else { 0 };
        s.mFinishStatus = if c.finished { 1 } else { 0 };
        // Retard sur la voiture devant et sur le leader : en temps (course) ou en différence de meilleur tour.
        if rank > 0 {
            let ahead = &sim.cars[order[rank - 1]];
            let leader = &sim.cars[order[0]];
            if race {
                let v = c.v.max(25.0);
                s.mTimeBehindNext = ((sim.total(ahead) - sim.total(c)) / v).max(0.0);
                s.mTimeBehindLeader = ((sim.total(leader) - sim.total(c)) / v).max(0.0);
            } else if c.best_lap > 0.0 && ahead.best_lap > 0.0 {
                s.mTimeBehindNext = c.best_lap - ahead.best_lap;
                s.mTimeBehindLeader = c.best_lap - leader.best_lap;
            }
        }
        addr_of_mut!((*base).scoring.vehScoringInfo).cast::<VehicleScoring>().add(rank).write_unaligned(s);
    }
}
