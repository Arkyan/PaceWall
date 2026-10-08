//! Faux jeu : crée la zone `LMU_Data` et y écrit une course simulée (un tour de 100 s).
//! Permet de développer le collecteur et les overlays sans Le Mans Ultimate.

#[cfg(windows)]
fn main() {
    use core::mem::{size_of, zeroed};
    use core::ptr::addr_of_mut;
    use lmu::*;
    use std::{thread, time::{Duration, Instant}};
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows::Win32::System::Memory::{
        CreateFileMappingW, MapViewOfFile, FILE_MAP_ALL_ACCESS, PAGE_READWRITE,
    };

    fn put(dst: &mut [u8], text: &str) {
        let n = text.len().min(dst.len() - 1);
        dst[..n].copy_from_slice(&text.as_bytes()[..n]);
        dst[n] = 0;
    }

    let size = size_of::<ObjectOut>();
    let name: Vec<u16> = SHARED_MEMORY_FILE.encode_utf16().chain(Some(0)).collect();
    let base = unsafe {
        let handle = CreateFileMappingW(
            INVALID_HANDLE_VALUE,
            None,
            PAGE_READWRITE,
            0,
            size as u32,
            PCWSTR(name.as_ptr()),
        )
        .expect("création de la zone impossible");
        let view = MapViewOfFile(handle, FILE_MAP_ALL_ACCESS, 0, 0, size);
        assert!(!view.Value.is_null(), "projection impossible");
        view.Value as *mut ObjectOut
    };

    println!("fakegame : LMU_Data créée ({size} octets). Ctrl+C pour arrêter.");
    let start = Instant::now();
    let (mut telem_count, mut scor_count) = (0u32, 0u32);
    let drivers = ["Toi", "A. Martin", "B. Dupont", "C. Rossi", "D. Keller"];

    loop {
        let t = start.elapsed().as_secs_f64();
        let lap_time = 100.0;
        let lap = (t / lap_time) as i32;
        let progress = (t % lap_time) / lap_time;
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
            v.mFuel = (90.0 - 3.6 * (t / lap_time)).max(0.0);
            v.mVirtualEnergy = (1.0 - 0.035 * (t / lap_time) as f32).max(0.0);
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
                w.mTireCarcassTemperature = 82.0 + 273.15;
                w.mWear = (0.012 * (t / lap_time)).min(1.0);
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
            info.mLapDist = 13626.0;
            info.mNumVehicles = drivers.len() as i32;
            info.mGamePhase = 5;
            info.mInRealtime = 1;
            put(&mut info.mPlayerName, "Toi");
            info.mRaining = 0.0;
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
                s.mLapDist = progress * 13626.0 - i as f64 * 40.0;
                s.mBestLapTime = 99.4 + i as f64 * 0.3;
                s.mLastLapTime = 99.8 + i as f64 * 0.3;
                s.mIsPlayer = (i == 0) as u8;
                s.mPlace = i as u8 + 1;
                s.mTimeBehindNext = 1.5;
                s.mTimeBehindLeader = i as f64 * 1.5;
                addr_of_mut!((*base).scoring.vehScoringInfo).cast::<VehicleScoring>().add(i).write_unaligned(s);
            }

            addr_of_mut!((*base).generic.events.SME_UPDATE_TELEMETRY).write_unaligned(telem_count);
            addr_of_mut!((*base).generic.events.SME_UPDATE_SCORING).write_unaligned(scor_count);
        }
        thread::sleep(Duration::from_millis(16));
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("fakegame ne fonctionne que sous Windows.");
}
