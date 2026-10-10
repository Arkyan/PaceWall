//! Enregistreur d'historique. Tourne en permanence (même sans overlay ouverte), mais très peu :
//! 2 lectures par seconde de quelques champs, une écriture SQLite par tour terminé.
//!
//! Un tour est enregistré quand le nombre de tours terminés du joueur augmente. Le tour en cours au
//! moment où on se branche est ignoré (« partiel »), sauf si on démarre dans les premiers mètres.

use crate::db::{self, NewLap, NewSession};
use crate::live::AliveCheck;
use crate::pulse::Pulse;
use crate::replay::{self, Car, Frame, Kind, Schema};
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
    /// Temps de session au dernier passage de la ligne (pour chronométrer un tour que le jeu n'a pas validé).
    lap_start: f64,
    /// Temps cumulés à la fin des secteurs 1 et 2 du tour en cours, tels que le jeu les a validés (0 = pas de temps).
    cur_s1: f64,
    cur_s2: f64,
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
    /// Replay en cours d'écriture et instant de la dernière image enregistrée.
    replay: Option<replay::Writer>,
    last_frame: f64,
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
    let mut alive = AliveCheck::default();
    let mut last_change = Instant::now();

    loop {
        if reader.is_none() {
            reader = Reader::open();
            pulse.reset();
            alive.reset();
            if reader.is_none() {
                std::thread::sleep(Duration::from_secs(3));
                continue;
            }
        }
        let r = reader.as_ref().unwrap();
        if pulse.changed(r) {
            last_change = Instant::now();
            building = sample(&conn, r, r.events().SME_START_SESSION, &mut session, &mut map);
        } else if last_change.elapsed() > Duration::from_secs(3) && !alive.alive() {
            // Jeu fermé : on clôt la session. (En pause, la session reste ouverte : le jeu reprendra où il en était.)
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

fn laps_unchanged(now: i16, before: i16) -> bool {
    now == before
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

    // Replay : une image toutes les 0,5 s de session, toutes les voitures.
    if let Some(w) = cur.replay.as_mut() {
        if et >= cur.last_frame + 0.45 || et < cur.last_frame {
            cur.last_frame = et;
            let (frame, roster, schema) = capture(r, &info);
            w.push(&frame, &roster, &schema);
        }
    }

    let (tyre, brake, wear) = averages(&t);
    cur.tyre_sum += tyre;
    cur.brake_sum += brake;
    cur.samples += 1;
    cur.pit |= s.mInPits != 0;
    cur.invalid |= t.mLapInvalidated != 0;

    // Secteurs du tour en cours : relevés pendant le tour (au passage de la ligne, le jeu les a déjà remis à zéro).
    if laps_unchanged(s.mTotalLaps, cur.last_laps) {
        if s.mCurSector1 > 0.0 {
            cur.cur_s1 = s.mCurSector1;
        }
        if s.mCurSector2 > 0.0 {
            cur.cur_s2 = s.mCurSector2;
        }
    }

    let laps = s.mTotalLaps;
    if laps < cur.last_laps {
        cur.last_laps = laps; // retour au stand / remise à zéro
        return building;
    }
    if laps > cur.last_laps {
        // Un tour invalidé n'a pas de chrono officiel (le jeu renvoie une valeur négative ou nulle) : on l'enregistre
        // quand même, avec la durée mesurée entre les deux passages de la ligne, et marqué « non valide ».
        let official = s.mLastLapTime;
        // Le début du nouveau tour, donné par le jeu, est exact ; à défaut, l'instant où on a vu le compteur changer.
        let crossing = if t.mLapStartET > cur.lap_start { t.mLapStartET } else { et };
        let time = if official > 0.0 { official } else { crossing - cur.lap_start };
        cur.invalid |= official <= 0.0;
        // Temps par secteur : uniquement ceux que le jeu a validés. Le secteur 3 n'existe que si le tour a un chrono.
        let (c1, c2) = (cur.cur_s1, cur.cur_s2);
        let sectors = [(c1 > 0.0).then_some(c1), (c1 > 0.0 && c2 > c1).then(|| c2 - c1), (official > 0.0 && c2 > 0.0 && official > c2).then(|| official - c2)];
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
                sectors,
            };
            if let Err(e) = db::add_lap(conn, &lap) {
                eprintln!("Tour non enregistré : {e}");
            }
        }
        cur.last_laps = laps;
        cur.partial = false;
        cur.lap_start = crossing;
        cur.cur_s1 = 0.0;
        cur.cur_s2 = 0.0;
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

/// Ouvre le fichier de replay de la session (sauf si l'option est coupée).
fn open_replay(id: i64, track: &str, info: &ScoringInfo, s: &VehicleScoring, t: &VehicleTelemetry) -> Option<replay::Writer> {
    if !crate::settings::bool_of("replay_record", true) {
        return None;
    }
    let header = replay::Header {
        track: track.to_string(),
        session_type: info.mSession,
        track_length: info.mLapDist as f32,
        max_laps: info.mMaxLaps,
        player: c_str(&info.mPlayerName),
        car: c_str(&t.mVehicleName),
        class: c_str(&s.mVehicleClass),
        started_at: db::now(),
    };
    let w = replay::Writer::create(&replay::path_of(id), &header).ok()?;
    replay::prune(crate::settings::int_of("replay_keep", 15).clamp(1, 200) as usize);
    Some(w)
}

/// Une image de la course : toutes les voitures du classement.
fn capture(r: &Reader, info: &ScoringInfo) -> (Frame, replay::Roster, Schema) {
    let n = (info.mNumVehicles.max(0) as usize).min(lmu::MAX_MAPPED_VEHICLES);
    let mut cars = Vec::with_capacity(n);
    let mut roster: replay::Roster = Vec::new();
    for i in 0..n {
        let Some(v) = r.vehicle_scoring(i) else { continue };
        // Identité de la voiture : son identifiant de jeu (`mID`), stable toute la session. L'ordre du tableau, lui,
        // peut changer (certaines sources le rangent par place) : s'en servir mélangerait les voitures entre elles.
        let id = v.mID.clamp(0, 255) as usize;
        if roster.len() <= id {
            roster.resize(id + 1, (String::new(), String::new()));
        }
        roster[id] = (c_str(&v.mDriverName), c_str(&v.mVehicleClass));
        let pos = v.mPos;
        cars.push(Car {
            idx: id as u8,
            place: v.mPlace.clamp(0, 255) as u8,
            laps: v.mTotalLaps.clamp(0, i16::MAX) as u16,
            in_pits: v.mInPits != 0,
            is_player: v.mIsPlayer != 0,
            x: pos.x as f32,
            z: pos.z as f32,
            lap_dist: v.mLapDist as f32,
            last_lap: v.mLastLapTime as f32,
            best_lap: v.mBestLapTime as f32,
            behind_next: v.mTimeBehindNext as f32,
            behind_leader: v.mTimeBehindLeader as f32, stops: 0, start_place: 0,
        });
    }
    let flags = info.mSectorFlag;
    let sectors = flags.iter().take(3).enumerate().fold(0u8, |m, (i, f)| m | ((*f == 1) as u8) << i);
    // Voiture du joueur : tout ce que lisent les widgets (carburant, pneus, pédales, tours, écarts…).
    let (mut keys, mut nums, mut texts) = (Vec::new(), Vec::new(), Vec::new());
    for f in crate::flatten::read_all(r, false) {
        if ["session.", "weather.", "game.", "grid."].iter().any(|p| f.key.starts_with(p)) {
            continue;
        }
        match &f.value {
            serde_json::Value::Bool(b) => {
                keys.push((f.key, Kind::Bool));
                nums.push(*b as u8 as f32);
            }
            serde_json::Value::Number(n) => {
                keys.push((f.key, if n.is_i64() { Kind::Int } else { Kind::Num }));
                nums.push(n.as_f64().unwrap_or(0.0) as f32);
            }
            serde_json::Value::String(s) => {
                keys.push((f.key, Kind::Text));
                texts.push(s.clone());
            }
            _ => {}
        }
    }
    let frame = Frame {
        et: info.mCurrentET,
        phase: info.mGamePhase,
        yellow: info.mYellowFlagState,
        remaining: info.mSessionTimeRemaining as f32,
        rain: (info.mRaining * 100.0) as f32,
        track_temp: info.mTrackTemp as f32,
        ambient: info.mAmbientTemp as f32,
        sectors,
        cars,
        nums,
        texts,
    };
    (frame, roster, Schema(keys))
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
        // (Au stand, ou sur la grille avant le feu vert, on voit le tour depuis son début : il compte.)
        partial: s.mLapDist > 300.0 && s.mInPits == 0 && !(s.mTotalLaps == 0 && info.mGamePhase < 5),
        lap_start: t.mLapStartET,
        cur_s1: 0.0,
        cur_s2: 0.0,
        fuel0: t.mFuel,
        energy0: t.mVirtualEnergy as f64 * 100.0,
        wear0: wear,
        tyre_sum: 0.0,
        brake_sum: 0.0,
        samples: 0,
        pit: s.mInPits != 0,
        invalid: false,
        replay: open_replay(id, track, info, &s, &t),
        last_frame: f64::MIN,
    })
}
