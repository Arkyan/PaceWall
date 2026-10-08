//! Enregistreur d'historique. Tourne en permanence (même sans overlay ouverte), mais très peu :
//! 2 lectures par seconde de quelques champs, une écriture SQLite par tour terminé.
//!
//! Un tour est enregistré quand le nombre de tours terminés du joueur augmente. Le tour en cours au
//! moment où on se branche est ignoré (« partiel »), sauf si on démarre dans les premiers mètres.

use crate::db::{self, NewLap, NewSession};
use crate::pulse::Pulse;
use crate::trackmap::{self, Builder, SectorLearner};
use lmu::reader::{c_str, Reader};
use lmu::{ScoringInfo, VehicleScoring, VehicleTelemetry};
use rusqlite::Connection;
use std::time::{Duration, Instant};

struct Session {
    id: i64,
    track: String,
    session_type: i32,
    start_event: u32,
    last_et: f64,
    last_laps: i16,
    partial: bool,
    // Au début du tour courant
    fuel0: f64,
    energy0: f64,
    wear0: f64,
    // Cumuls sur le tour courant
    tyre_sum: f64,
    brake_sum: f64,
    samples: u32,
    pit: bool,
    invalid: bool,
}

pub fn run() {
    let conn = match db::open() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Historique désactivé : base inaccessible ({e})");
            return;
        }
    };
    db::close_stale(&conn);

    let mut reader: Option<Reader> = None;
    let mut session: Option<Session> = None;
    // Carte du circuit : (circuit, constructeur tant qu'elle n'existe pas encore).
    let mut map: Option<MapState> = None;
    let mut building = false;
    let mut pulse = Pulse::new();
    let mut last_change = Instant::now();

    loop {
        if reader.is_none() {
            reader = Reader::open();
            pulse.reset();
            if reader.is_none() {
                std::thread::sleep(Duration::from_secs(3));
                continue;
            }
        }
        let r = reader.as_ref().unwrap();
        if pulse.changed(r) {
            last_change = Instant::now();
            building = sample(&conn, r, r.events().SME_START_SESSION, &mut session, &mut map);
        } else if last_change.elapsed() > Duration::from_secs(3) {
            // Jeu fermé ou en pause longue : on clôt la session.
            if let Some(s) = session.take() {
                db::end_session(&conn, s.id);
            }
            reader = None;
            std::thread::sleep(Duration::from_secs(2));
            continue;
        }
        // Tant que la carte du circuit se construit, on échantillonne plus souvent (10 fois par seconde).
        std::thread::sleep(Duration::from_millis(if building { 100 } else { 500 }));
    }
}

fn averages(t: &VehicleTelemetry) -> (f64, f64, f64) {
    let wheels = t.mWheels;
    let (mut tyre, mut brake, mut wear) = (0.0, 0.0, 0.0);
    for w in &wheels {
        let temps = w.mTemperature;
        tyre += (temps[0] + temps[1] + temps[2]) / 3.0 - 273.15;
        brake += w.mBrakeTemp;
        wear += w.mWear * 100.0;
    }
    (tyre / 4.0, brake / 4.0, wear / 4.0)
}

/// État de la carte du circuit en cours : constructeur tant qu'elle n'existe pas, relevé des secteurs tant qu'ils manquent.
struct MapState {
    track: String,
    builder: Option<Builder>,
    sectors: SectorLearner,
    need_sectors: bool,
}

/// Renvoie `true` tant que la carte du circuit est en construction.
fn sample(conn: &Connection, r: &Reader, start_event: u32, session: &mut Option<Session>, map: &mut Option<MapState>) -> bool {
    let Some((si, ti)) = r.player_indexes() else { return false };
    let (Some(s), Some(t)) = (r.vehicle_scoring(si), r.telemetry(ti)) else { return false };
    let info = r.scoring_info();
    let track = c_str(&info.mTrackName);
    let session_type = info.mSession;
    let et = info.mCurrentET;

    let changed = session.as_ref().is_none_or(|x| {
        x.track != track || x.session_type != session_type || x.start_event != start_event || et < x.last_et - 5.0
    });
    if changed {
        if let Some(old) = session.take() {
            db::end_session(conn, old.id);
        }
        *session = begin(conn, &track, &info, &s, &t, start_event);
    }
    // Carte du circuit : on la construit au premier passage, puis elle est réutilisée.
    if map.as_ref().is_none_or(|m| m.track != track) {
        let existing = trackmap::load(&track);
        *map = Some(MapState {
            builder: existing.is_none().then(|| Builder::new(&track, info.mLapDist)),
            need_sectors: existing.as_ref().is_none_or(|m| m.sectors.len() < 2),
            sectors: SectorLearner::default(),
            track: track.clone(),
        });
    }
    let mut building = false;
    if let Some(m) = map.as_mut() {
        if let Some(b) = m.builder.as_mut() {
            building = true;
            let pos = s.mPos;
            if let Some(done) = b.update(s.mTotalLaps as i64, s.mLapDist, pos.x, pos.z, s.mInPits != 0, s.mLastLapTime, info.mCurrentET) {
                if trackmap::save(&done).is_ok() {
                    m.builder = None;
                    building = false;
                }
            }
        }
        // Limites des secteurs : relevées sur un tour propre, ajoutées à la carte dès qu'elle existe.
        if m.need_sectors {
            if let Some(limits) = m.sectors.update(s.mSector, s.mLapDist, info.mLapDist, s.mInPits != 0) {
                if let Some(mut saved) = trackmap::load(&m.track) {
                    saved.sectors = limits.to_vec();
                    if trackmap::save(&saved).is_ok() {
                        m.need_sectors = false;
                    }
                }
            }
        }
    }

    let Some(cur) = session.as_mut() else { return building };
    cur.last_et = et;

    let (tyre, brake, wear) = averages(&t);
    cur.tyre_sum += tyre;
    cur.brake_sum += brake;
    cur.samples += 1;
    cur.pit |= s.mInPits != 0;
    cur.invalid |= t.mLapInvalidated != 0;

    let laps = s.mTotalLaps;
    if laps < cur.last_laps {
        cur.last_laps = laps; // retour au stand / remise à zéro
        return building;
    }
    if laps > cur.last_laps {
        let time = s.mLastLapTime;
        if !cur.partial && time > 0.0 && cur.samples > 0 {
            let n = cur.samples as f64;
            let fuel = t.mFuel;
            let energy = t.mVirtualEnergy as f64 * 100.0;
            let clean = !cur.pit;
            let lap = NewLap {
                session_id: cur.id,
                lap: laps as i32,
                time,
                valid: !cur.invalid,
                pit: cur.pit,
                // Après un arrêt, carburant et usure ne sont plus comparables : on ne les garde pas.
                fuel_used: clean.then(|| cur.fuel0 - fuel).filter(|v| *v >= 0.0),
                energy_used: clean.then(|| cur.energy0 - energy).filter(|v| *v >= 0.0),
                wear_used: clean.then(|| cur.wear0 - wear).filter(|v| *v >= 0.0),
                wear_end: wear,
                tyre_temp: cur.tyre_sum / n,
                brake_temp: cur.brake_sum / n,
                ambient: info.mAmbientTemp,
                track_temp: info.mTrackTemp,
                rain: info.mRaining,
                place: s.mPlace as i32,
                compound: &c_str(&t.mFrontTireCompoundName),
            };
            if let Err(e) = db::add_lap(conn, &lap) {
                eprintln!("Tour non enregistré : {e}");
            }
        }
        cur.last_laps = laps;
        cur.partial = false;
        cur.fuel0 = t.mFuel;
        cur.energy0 = t.mVirtualEnergy as f64 * 100.0;
        cur.wear0 = wear;
        cur.tyre_sum = 0.0;
        cur.brake_sum = 0.0;
        cur.samples = 0;
        cur.pit = s.mInPits != 0;
        cur.invalid = false;
    }
    building
}

fn begin(
    conn: &Connection,
    track: &str,
    info: &ScoringInfo,
    s: &VehicleScoring,
    t: &VehicleTelemetry,
    start_event: u32,
) -> Option<Session> {
    let car = c_str(&t.mVehicleName);
    let class = c_str(&s.mVehicleClass);
    let id = db::begin_session(conn, &NewSession { track, car: &car, class: &class, session_type: info.mSession }).ok()?;
    let (_, _, wear) = averages(t);
    Some(Session {
        id,
        track: track.to_string(),
        session_type: info.mSession,
        start_event,
        last_et: info.mCurrentET,
        last_laps: s.mTotalLaps,
        // On se branche en cours de tour, sauf dans les premiers mètres : ce tour-là est incomplet.
        partial: s.mLapDist > 300.0,
        fuel0: t.mFuel,
        energy0: t.mVirtualEnergy as f64 * 100.0,
        wear0: wear,
        tyre_sum: 0.0,
        brake_sum: 0.0,
        samples: 0,
        pit: s.mInPits != 0,
        invalid: false,
    })
}
