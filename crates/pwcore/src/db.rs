//! Historique : sessions et tours dans une base SQLite locale.
//! Emplacement : `%APPDATA%\PaceWall\history.db` (ou la variable `PACEWALL_DB`).

use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS sessions (
    id           INTEGER PRIMARY KEY,
    started_at   INTEGER NOT NULL,
    ended_at     INTEGER,
    track        TEXT NOT NULL,
    car          TEXT NOT NULL,
    class        TEXT NOT NULL,
    session_type INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS laps (
    id          INTEGER PRIMARY KEY,
    session_id  INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    lap         INTEGER NOT NULL,
    time        REAL NOT NULL,
    valid       INTEGER NOT NULL,
    pit         INTEGER NOT NULL,
    fuel_used   REAL,
    energy_used REAL,
    wear_used   REAL,
    wear_end    REAL,
    tyre_temp   REAL,
    brake_temp  REAL,
    ambient     REAL,
    track_temp  REAL,
    rain        REAL,
    place       INTEGER,
    compound    TEXT,
    created_at  INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS laps_session ON laps(session_id, lap);
";

pub fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

pub fn path() -> PathBuf {
    if let Ok(p) = std::env::var("PACEWALL_DB") {
        return PathBuf::from(p);
    }
    let base = std::env::var("APPDATA").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("."));
    base.join("PaceWall").join("history.db")
}

pub fn open() -> rusqlite::Result<Connection> {
    let path = path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let conn = Connection::open(path)?;
    // WAL : le serveur lit pendant que l'enregistreur écrit, sans se bloquer.
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.execute_batch(SCHEMA)?;
    Ok(conn)
}

/// Sessions restées « en cours » (plantage, arrêt brutal) : on les clôture, et on retire les vides.
pub fn close_stale(conn: &Connection) {
    let _ = conn.execute(
        "UPDATE sessions SET ended_at = COALESCE((SELECT MAX(created_at) FROM laps WHERE session_id = sessions.id), started_at)
         WHERE ended_at IS NULL",
        [],
    );
    let _ = conn.execute("DELETE FROM sessions WHERE id NOT IN (SELECT DISTINCT session_id FROM laps)", []);
}

pub struct NewSession<'a> {
    pub track: &'a str,
    pub car: &'a str,
    pub class: &'a str,
    pub session_type: i32,
}

pub fn begin_session(conn: &Connection, s: &NewSession) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO sessions (started_at, track, car, class, session_type) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![now(), s.track, s.car, s.class, s.session_type],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Clôture la session ; sans aucun tour, elle est supprimée.
pub fn end_session(conn: &Connection, id: i64) {
    let _ = conn.execute("UPDATE sessions SET ended_at = ?1 WHERE id = ?2", params![now(), id]);
    let _ = conn.execute(
        "DELETE FROM sessions WHERE id = ?1 AND NOT EXISTS (SELECT 1 FROM laps WHERE session_id = ?1)",
        params![id],
    );
}

pub struct NewLap<'a> {
    pub session_id: i64,
    pub lap: i32,
    pub time: f64,
    pub valid: bool,
    pub pit: bool,
    pub fuel_used: Option<f64>,
    pub energy_used: Option<f64>,
    pub wear_used: Option<f64>,
    pub wear_end: f64,
    pub tyre_temp: f64,
    pub brake_temp: f64,
    pub ambient: f64,
    pub track_temp: f64,
    pub rain: f64,
    pub place: i32,
    pub compound: &'a str,
}

pub fn add_lap(conn: &Connection, l: &NewLap) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO laps (session_id, lap, time, valid, pit, fuel_used, energy_used, wear_used, wear_end,
                           tyre_temp, brake_temp, ambient, track_temp, rain, place, compound, created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)",
        params![
            l.session_id, l.lap, l.time, l.valid, l.pit, l.fuel_used, l.energy_used, l.wear_used, l.wear_end,
            l.tyre_temp, l.brake_temp, l.ambient, l.track_temp, l.rain, l.place, l.compound, now()
        ],
    )?;
    Ok(())
}

pub fn list_sessions(conn: &Connection) -> rusqlite::Result<Vec<Value>> {
    let mut q = conn.prepare(
        "SELECT s.id, s.started_at, s.ended_at, s.track, s.car, s.class, s.session_type,
                COUNT(l.id), MIN(CASE WHEN l.valid = 1 AND l.pit = 0 THEN l.time END)
         FROM sessions s LEFT JOIN laps l ON l.session_id = s.id
         GROUP BY s.id ORDER BY s.started_at DESC LIMIT 300",
    )?;
    let rows = q.query_map([], |r| {
        Ok(json!({
            "id": r.get::<_, i64>(0)?, "started": r.get::<_, i64>(1)?, "ended": r.get::<_, Option<i64>>(2)?,
            "track": r.get::<_, String>(3)?, "car": r.get::<_, String>(4)?, "class": r.get::<_, String>(5)?,
            "type": r.get::<_, i64>(6)?, "laps": r.get::<_, i64>(7)?, "best": r.get::<_, Option<f64>>(8)?,
        }))
    })?;
    rows.collect()
}

pub fn laps(conn: &Connection, session_id: i64) -> rusqlite::Result<Vec<Value>> {
    let mut q = conn.prepare(
        "SELECT lap, time, valid, pit, fuel_used, energy_used, wear_used, wear_end, tyre_temp, brake_temp,
                ambient, track_temp, rain, place, compound
         FROM laps WHERE session_id = ?1 ORDER BY lap",
    )?;
    let rows = q.query_map([session_id], |r| {
        Ok(json!({
            "lap": r.get::<_, i64>(0)?, "time": r.get::<_, f64>(1)?, "valid": r.get::<_, bool>(2)?,
            "pit": r.get::<_, bool>(3)?, "fuel": r.get::<_, Option<f64>>(4)?, "energy": r.get::<_, Option<f64>>(5)?,
            "wear": r.get::<_, Option<f64>>(6)?, "wear_end": r.get::<_, f64>(7)?,
            "tyre_temp": r.get::<_, f64>(8)?, "brake_temp": r.get::<_, f64>(9)?,
            "ambient": r.get::<_, f64>(10)?, "track_temp": r.get::<_, f64>(11)?, "rain": r.get::<_, f64>(12)?,
            "place": r.get::<_, i64>(13)?, "compound": r.get::<_, String>(14)?,
        }))
    })?;
    rows.collect()
}

pub fn delete_session(conn: &Connection, id: i64) -> rusqlite::Result<usize> {
    conn.execute("DELETE FROM sessions WHERE id = ?1", [id])
}
