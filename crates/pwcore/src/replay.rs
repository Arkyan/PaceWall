//! Replays de course : pendant une session, l'enregistreur note deux fois par seconde la position et le classement
//! de toutes les voitures. Le fichier (`replays\<session>.pwr`) est un flux compact compressé par blocs d'une minute :
//! une session de deux heures pèse quelques mégaoctets, et rien ne tourne en dehors de l'écriture d'un bloc par minute.
//!
//! Format : `PWR1`, un en-tête, puis des blocs `[longueur u32][données compressées]`. Chaque bloc contient des
//! enregistrements : 0 = liste des pilotes, 1 = une image de la course, 2 = liste des données de la voiture du joueur
//! (les deux listes sont réécrites quand elles changent et au début de chaque bloc).

use crate::data::Data;
use crate::flatten::Field;
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

const MAGIC: &[u8; 4] = b"PWR2";
const FRAMES_PER_CHUNK: u32 = 120;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Header {
    pub track: String,
    pub session_type: i32,
    pub track_length: f32,
    pub max_laps: i32,
    pub player: String,
    pub car: String,
    pub class: String,
    pub started_at: i64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Car {
    pub idx: u8,
    pub place: u8,
    pub laps: u16,
    pub in_pits: bool,
    pub is_player: bool,
    pub x: f32,
    pub z: f32,
    pub lap_dist: f32,
    pub last_lap: f32,
    pub best_lap: f32,
    pub behind_next: f32,
    pub behind_leader: f32,
    /// Dérivés à la lecture (non enregistrés) : arrêts aux stands déjà faits, place au départ du replay.
    pub stops: u8,
    pub start_place: u8,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Frame {
    pub et: f64,
    pub phase: u8,
    pub yellow: i8,
    pub remaining: f32,
    pub rain: f32,
    pub track_temp: f32,
    pub ambient: f32,
    /// Bit n = jaune dans le secteur n + 1.
    pub sectors: u8,
    pub cars: Vec<Car>,
    /// Données de la voiture du joueur, dans l'ordre du `Schema` (nombres, booléens et entiers en f32).
    pub nums: Vec<f32>,
    pub texts: Vec<String>,
}

/// Nature d'une donnée du joueur.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Num = 0,
    Int = 1,
    Bool = 2,
    Text = 3,
}

/// Liste des données du joueur enregistrées (clé et nature), dans l'ordre des valeurs de chaque image.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Schema(pub Vec<(String, Kind)>);

impl Schema {
    fn count(&self, text: bool) -> usize {
        self.0.iter().filter(|(_, k)| (*k == Kind::Text) == text).count()
    }
}

/// Nom et classe de chaque voiture, dans l'ordre des index.
pub type Roster = Vec<(String, String)>;

/* ---------- écriture ---------- */

fn put_str(b: &mut Vec<u8>, s: &str) {
    let bytes = s.as_bytes();
    let n = bytes.len().min(255);
    b.push(n as u8);
    b.extend_from_slice(&bytes[..n]);
}

pub struct Writer {
    file: std::fs::File,
    chunk: Vec<u8>,
    frames: u32,
    roster: Roster,
    schema: Schema,
    pub total: u32,
}

impl Writer {
    pub fn create(path: &Path, h: &Header) -> std::io::Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut file = std::fs::File::create(path)?;
        let mut head = Vec::new();
        head.extend_from_slice(MAGIC);
        put_str(&mut head, &h.track);
        head.extend_from_slice(&h.session_type.to_le_bytes());
        head.extend_from_slice(&h.track_length.to_le_bytes());
        head.extend_from_slice(&h.max_laps.to_le_bytes());
        put_str(&mut head, &h.player);
        put_str(&mut head, &h.car);
        put_str(&mut head, &h.class);
        head.extend_from_slice(&h.started_at.to_le_bytes());
        file.write_all(&head)?;
        Ok(Self { file, chunk: Vec::new(), frames: 0, roster: Vec::new(), schema: Schema::default(), total: 0 })
    }

    pub fn push(&mut self, f: &Frame, roster: &Roster, schema: &Schema) {
        if *roster != self.roster {
            self.chunk.push(0);
            self.chunk.push(roster.len().min(255) as u8);
            for (name, class) in roster.iter().take(255) {
                put_str(&mut self.chunk, name);
                put_str(&mut self.chunk, class);
            }
            self.roster = roster.clone();
        }
        if *schema != self.schema {
            self.chunk.push(2);
            self.chunk.extend_from_slice(&(schema.0.len().min(u16::MAX as usize) as u16).to_le_bytes());
            for (key, kind) in &schema.0 {
                put_str(&mut self.chunk, key);
                self.chunk.push(*kind as u8);
            }
            self.schema = schema.clone();
        }
        let b = &mut self.chunk;
        b.push(1);
        b.extend_from_slice(&f.et.to_le_bytes());
        b.push(f.phase);
        b.push(f.yellow as u8);
        b.extend_from_slice(&f.remaining.to_le_bytes());
        b.extend_from_slice(&f.rain.to_le_bytes());
        b.extend_from_slice(&f.track_temp.to_le_bytes());
        b.extend_from_slice(&f.ambient.to_le_bytes());
        b.push(f.sectors);
        b.push(f.cars.len().min(255) as u8);
        for c in f.cars.iter().take(255) {
            b.push(c.idx);
            b.push(c.place);
            b.extend_from_slice(&c.laps.to_le_bytes());
            b.push(c.in_pits as u8 | (c.is_player as u8) << 1);
            for v in [c.x, c.z, c.lap_dist, c.last_lap, c.best_lap, c.behind_next, c.behind_leader] {
                b.extend_from_slice(&v.to_le_bytes());
            }
        }
        for v in &f.nums {
            b.extend_from_slice(&v.to_le_bytes());
        }
        for t in &f.texts {
            put_str(b, t);
        }
        self.frames += 1;
        self.total += 1;
        if self.frames >= FRAMES_PER_CHUNK {
            self.flush();
        }
    }

    /// Compresse et écrit le bloc en cours.
    pub fn flush(&mut self) {
        if self.chunk.is_empty() {
            return;
        }
        let packed = miniz_oxide::deflate::compress_to_vec(&self.chunk, 4);
        let _ = self.file.write_all(&(packed.len() as u32).to_le_bytes());
        let _ = self.file.write_all(&packed);
        self.chunk.clear();
        self.frames = 0;
        // Le pilote qui rouvre un bloc doit retrouver la liste : on la réécrit au début du bloc suivant.
        self.roster.clear();
        self.schema = Schema::default();
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        self.flush();
    }
}

/* ---------- emplacement ---------- */

pub fn dir() -> PathBuf {
    crate::db::path().with_file_name("replays")
}

pub fn path_of(session_id: i64) -> PathBuf {
    dir().join(format!("{session_id}.pwr"))
}

pub fn exists(session_id: i64) -> bool {
    // Pas de test sur la taille : tant que le fichier est ouvert en écriture, Windows peut encore en annoncer 0 octet.
    path_of(session_id).is_file()
}

pub fn delete(session_id: i64) {
    let _ = std::fs::remove_file(path_of(session_id));
}

/// Ne garde que les `keep` replays les plus récents.
pub fn prune(keep: usize) {
    let Ok(read) = std::fs::read_dir(dir()) else { return };
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = read
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "pwr"))
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    files.sort_by(|a, b| b.0.cmp(&a.0));
    for (_, p) in files.into_iter().skip(keep) {
        let _ = std::fs::remove_file(p);
    }
}

/* ---------- lecture ---------- */

struct Cursor<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let s = self.b.get(self.at..self.at + n)?;
        self.at += n;
        Some(s)
    }
    fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|s| s[0])
    }
    fn u16(&mut self) -> Option<u16> {
        self.take(2).map(|s| u16::from_le_bytes([s[0], s[1]]))
    }
    fn i32(&mut self) -> Option<i32> {
        self.take(4).map(|s| i32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    }
    fn f32(&mut self) -> Option<f32> {
        self.take(4).map(|s| f32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    }
    fn f64(&mut self) -> Option<f64> {
        self.take(8).map(|s| f64::from_le_bytes(s.try_into().unwrap()))
    }
    fn i64(&mut self) -> Option<i64> {
        self.take(8).map(|s| i64::from_le_bytes(s.try_into().unwrap()))
    }
    fn string(&mut self) -> Option<String> {
        let n = self.u8()? as usize;
        self.take(n).map(|s| String::from_utf8_lossy(s).into_owned())
    }
}

/// Une ligne du classement rejoué.
#[derive(Clone, Debug)]
pub struct Row {
    /// Identité de la voiture dans le replay (stable d'une image à l'autre).
    pub idx: u8,
    pub place: u8,
    pub name: String,
    pub class: String,
    pub laps: u16,
    pub last: f64,
    pub best: f64,
    pub gap_leader: f64,
    pub gap_next: f64,
    pub in_pits: bool,
    pub is_player: bool,
    pub stops: u8,
    pub start_place: u8,
}

/// Un tour bouclé par une voiture.
#[derive(Clone, Debug)]
pub struct LapRec {
    pub lap: u16,
    pub time: f64,
    /// Instant de fin du tour, en secondes depuis le début du replay.
    pub t: f64,
    pub place: u8,
    /// Le tour est passé par les stands.
    pub pit: bool,
    /// Tour validé par le jeu. Un tour invalidé n'a pas de chrono (`time` vaut 0).
    pub valid: bool,
}

#[derive(Clone, Debug)]
pub struct Status {
    pub phase: u8,
    pub yellow: i8,
    pub remaining: f64,
    pub rain: f64,
    pub track_temp: f64,
    pub ambient: f64,
    pub sectors: u8,
    pub leader_laps: u16,
}

/// Tracé du circuit paramétré par la distance parcourue : sert à faire rouler les voitures le long de la piste
/// entre deux images, au lieu de couper les virages en ligne droite.
struct Racing {
    /// Points de la carte, repère (x, -z).
    points: Vec<[f32; 2]>,
    /// Distance cumulée jusqu'à chaque point (le dernier élément ferme la boucle).
    cum: Vec<f32>,
}

impl Racing {
    fn new(points: Vec<[f32; 2]>) -> Option<Self> {
        if points.len() < 8 {
            return None;
        }
        let mut cum = Vec::with_capacity(points.len() + 1);
        let mut total = 0.0f32;
        for i in 0..points.len() {
            cum.push(total);
            let (a, b) = (points[i], points[(i + 1) % points.len()]);
            total += ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
        }
        cum.push(total);
        (total > 1.0).then_some(Self { points, cum })
    }

    /// Position (x, z du jeu) à la fraction `f` du tour.
    fn at(&self, f: f64) -> (f32, f32) {
        let total = *self.cum.last().unwrap();
        let d = f.rem_euclid(1.0) as f32 * total;
        let n = self.points.len();
        let i = self.cum.partition_point(|c| *c <= d).saturating_sub(1).min(n - 1);
        let span = (self.cum[i + 1] - self.cum[i]).max(1e-6);
        let t = ((d - self.cum[i]) / span).clamp(0.0, 1.0);
        let (a, b) = (self.points[i], self.points[(i + 1) % n]);
        (a[0] + (b[0] - a[0]) * t, -(a[1] + (b[1] - a[1]) * t))
    }
}

pub struct Replay {
    pub header: Header,
    /// Tracé du circuit, si sa carte a été enregistrée.
    path: Option<Racing>,
    frames: Vec<Frame>,
    /// Index de la liste de pilotes valable pour chaque image.
    roster_of: Vec<u16>,
    rosters: Vec<Arc<Roster>>,
    schema_of: Vec<u16>,
    schemas: Vec<Arc<Schema>>,
    t0: f64,
    /// Par voiture : tours bouclés, et passages aux stands (entrée, sortie) en secondes depuis le début.
    laps: std::collections::HashMap<u8, Vec<LapRec>>,
    pits: std::collections::HashMap<u8, Vec<(f64, f64)>>,
}

impl Replay {
    pub fn load(path: &Path) -> std::io::Result<Self> {
        let mut raw = Vec::new();
        std::fs::File::open(path)?.read_to_end(&mut raw)?;
        let bad = || std::io::Error::new(std::io::ErrorKind::InvalidData, "replay illisible");
        let mut c = Cursor { b: &raw, at: 0 };
        if c.take(4) != Some(&MAGIC[..]) {
            return Err(bad());
        }
        let header = (|| {
            Some(Header {
                track: c.string()?,
                session_type: c.i32()?,
                track_length: c.f32()?,
                max_laps: c.i32()?,
                player: c.string()?,
                car: c.string()?,
                class: c.string()?,
                started_at: c.i64()?,
            })
        })()
        .ok_or_else(bad)?;

        let mut frames = Vec::new();
        let mut roster_of = Vec::new();
        let mut rosters: Vec<Arc<Roster>> = vec![Arc::new(Vec::new())];
        let mut current = 0u16;
        let mut schema_of = Vec::new();
        let mut schemas: Vec<Arc<Schema>> = vec![Arc::new(Schema::default())];
        let mut current_schema = 0u16;
        // Un bloc coupé (arrêt brutal) est simplement ignoré.
        while let Some(len) = c.i32() {
            let Some(packed) = c.take(len.max(0) as usize) else { break };
            let Ok(data) = miniz_oxide::inflate::decompress_to_vec(packed) else { break };
            let mut d = Cursor { b: &data, at: 0 };
            while let Some(tag) = d.u8() {
                if tag == 0 {
                    let Some(n) = d.u8() else { break };
                    let mut r = Vec::new();
                    for _ in 0..n {
                        let (Some(name), Some(class)) = (d.string(), d.string()) else { break };
                        r.push((name, class));
                    }
                    rosters.push(Arc::new(r));
                    current = (rosters.len() - 1) as u16;
                } else if tag == 2 {
                    let Some(n) = d.u16() else { break };
                    let mut keys = Vec::with_capacity(n as usize);
                    for _ in 0..n {
                        let (Some(key), Some(kind)) = (d.string(), d.u8()) else { break };
                        let kind = match kind {
                            1 => Kind::Int,
                            2 => Kind::Bool,
                            3 => Kind::Text,
                            _ => Kind::Num,
                        };
                        keys.push((key, kind));
                    }
                    schemas.push(Arc::new(Schema(keys)));
                    current_schema = (schemas.len() - 1) as u16;
                } else {
                    let schema = schemas[current_schema as usize].clone();
                    let f = (|| {
                        let et = d.f64()?;
                        let (phase, yellow) = (d.u8()?, d.u8()? as i8);
                        let (remaining, rain, track_temp, ambient) = (d.f32()?, d.f32()?, d.f32()?, d.f32()?);
                        let sectors = d.u8()?;
                        let n = d.u8()?;
                        let mut cars = Vec::with_capacity(n as usize);
                        for _ in 0..n {
                            let (idx, place, laps, flags) = (d.u8()?, d.u8()?, d.u16()?, d.u8()?);
                            cars.push(Car {
                                idx,
                                place,
                                laps,
                                in_pits: flags & 1 != 0,
                                is_player: flags & 2 != 0,
                                x: d.f32()?,
                                z: d.f32()?,
                                lap_dist: d.f32()?,
                                last_lap: d.f32()?,
                                best_lap: d.f32()?,
                                behind_next: d.f32()?,
                                behind_leader: d.f32()?,
                                stops: 0,
                                start_place: 0,
                            });
                        }
                        let mut nums = Vec::new();
                        for _ in 0..schema.count(false) {
                            nums.push(d.f32()?);
                        }
                        let mut texts = Vec::new();
                        for _ in 0..schema.count(true) {
                            texts.push(d.string()?);
                        }
                        Some(Frame { et, phase, yellow, remaining, rain, track_temp, ambient, sectors, cars, nums, texts })
                    })();
                    let Some(f) = f else { break };
                    frames.push(f);
                    roster_of.push(current);
                    schema_of.push(current_schema);
                }
            }
        }
        // Arrêts aux stands (entrées au stand cumulées) et place de départ de chaque voiture.
        let mut stops = [0u8; 256];
        let mut was_in = [false; 256];
        let mut start = [0u8; 256];
        let mut seen = [false; 256];
        let mut last_laps = [0u16; 256];
        let mut lap_pit = [false; 256];
        // Instant du dernier passage de la ligne de chaque voiture (pour chronométrer les tours sans temps officiel).
        let mut lap_begin = [f64::NAN; 256];
        let mut laps: std::collections::HashMap<u8, Vec<LapRec>> = Default::default();
        let mut pits: std::collections::HashMap<u8, Vec<(f64, f64)>> = Default::default();
        let first_et = frames.first().map_or(0.0, |f| f.et);
        let last_et = frames.last().map_or(0.0, |f| f.et);
        for f in &mut frames {
            let rel = f.et - first_et;
            for c in &mut f.cars {
                let i = c.idx as usize;
                lap_pit[i] |= c.in_pits;
                if seen[i] && c.laps > last_laps[i] {
                    // Le jeu ne donne pas de chrono pour un tour invalidé : il est gardé sans temps.
                    let valid = c.last_lap > 0.0;
                    let time = if valid { c.last_lap as f64 } else { 0.0 };
                    laps.entry(c.idx).or_default().push(LapRec { lap: c.laps, time, t: rel, place: c.place, pit: lap_pit[i], valid });
                    lap_begin[i] = rel;
                    lap_pit[i] = c.in_pits;
                }
                last_laps[i] = c.laps;
                seen[i] = true;
                if c.in_pits && !was_in[i] {
                    pits.entry(c.idx).or_default().push((rel, last_et - first_et));
                } else if !c.in_pits && was_in[i] {
                    if let Some(p) = pits.get_mut(&c.idx).and_then(|v| v.last_mut()) {
                        p.1 = rel;
                    }
                }
                if start[i] == 0 {
                    start[i] = c.place;
                }
                if c.in_pits && !was_in[i] {
                    stops[i] = stops[i].saturating_add(1);
                }
                was_in[i] = c.in_pits;
                c.stops = stops[i];
                c.start_place = start[i];
            }
        }
        let t0 = frames.first().map_or(0.0, |f| f.et);
        let path = crate::trackmap::load(&header.track).and_then(|m| Racing::new(m.points));
        Ok(Self { header, path, frames, roster_of, rosters, schema_of, schemas, t0, laps, pits })
    }

    pub fn is_empty(&self) -> bool {
        self.frames.len() < 2
    }

    /// Durée enregistrée, en secondes.
    pub fn duration(&self) -> f64 {
        self.frames.last().map_or(0.0, |f| f.et - self.t0).max(0.0)
    }

    /// Classement complet à l'instant `t` (secondes depuis le début), trié par place.
    pub fn rows_at(&self, t: f64) -> Vec<Row> {
        let Some(f) = self.frame_at(t) else { return Vec::new() };
        let roster = &self.rosters[self.roster_of[self.index_at(t)] as usize];
        let mut rows: Vec<Row> = f
            .cars
            .iter()
            .map(|c| {
                let (name, class) = roster.get(c.idx as usize).cloned().unwrap_or_default();
                Row {
                    idx: c.idx,
                    place: c.place,
                    name,
                    class,
                    laps: c.laps,
                    last: c.last_lap as f64,
                    best: c.best_lap as f64,
                    gap_leader: c.behind_leader as f64,
                    gap_next: c.behind_next as f64,
                    in_pits: c.in_pits,
                    is_player: c.is_player,
                    stops: c.stops,
                    start_place: c.start_place,
                }
            })
            .collect();
        rows.sort_by_key(|r| r.place);
        rows
    }

    /// Tours bouclés par une voiture pendant le replay.
    pub fn laps_of(&self, idx: u8) -> &[LapRec] {
        self.laps.get(&idx).map_or(&[], |v| v.as_slice())
    }

    /// Passages aux stands d'une voiture : (entrée, sortie) en secondes depuis le début.
    pub fn pits_of(&self, idx: u8) -> &[(f64, f64)] {
        self.pits.get(&idx).map_or(&[], |v| v.as_slice())
    }

    /// Courbe d'une donnée de la voiture du joueur sur tout le replay : (secondes, valeur), au plus ~1500 points.
    pub fn series(&self, key: &str) -> Vec<[f64; 2]> {
        // Position de la clé parmi les nombres de chaque liste de données.
        let pos: Vec<Option<usize>> = self
            .schemas
            .iter()
            .map(|s| s.0.iter().filter(|(_, k)| *k != Kind::Text).position(|(name, _)| name == key))
            .collect();
        let step = (self.frames.len() / 1500).max(1);
        self.frames
            .iter()
            .enumerate()
            .step_by(step)
            .filter_map(|(i, f)| {
                let p = pos[self.schema_of[i] as usize]?;
                Some([f.et - self.t0, *f.nums.get(p)? as f64])
            })
            .collect()
    }

    /// Informations de session à l'instant `t`.
    pub fn status_at(&self, t: f64) -> Option<Status> {
        let f = self.frame_at(t)?;
        Some(Status {
            phase: f.phase,
            yellow: f.yellow,
            remaining: f.remaining as f64,
            rain: f.rain as f64,
            track_temp: f.track_temp as f64,
            ambient: f.ambient as f64,
            sectors: f.sectors,
            leader_laps: f.cars.iter().map(|c| c.laps).max().unwrap_or(0),
        })
    }

    fn index_at(&self, t: f64) -> usize {
        let target = self.t0 + t.clamp(0.0, self.duration());
        self.frames.partition_point(|f| f.et <= target).saturating_sub(1).min(self.frames.len().saturating_sub(1))
    }

    fn frame_at(&self, t: f64) -> Option<&Frame> {
        self.frames.get(self.index_at(t))
    }

    /// Les valeurs de la course à l'instant `t` (secondes depuis le début du replay), avec les positions interpolées
    /// entre deux images pour que les voitures glissent au lieu de sauter.
    pub fn data_at(&self, t: f64) -> Data {
        let mut out: Vec<Field> = Vec::with_capacity(160);
        let mut put = |key: String, value: Value| out.push(Field { key, unit: "", label: "", value });
        let Some(last) = self.frames.len().checked_sub(1) else { return Data::default() };
        let target = self.t0 + t.clamp(0.0, self.duration());
        let i = self.frames.partition_point(|f| f.et <= target).saturating_sub(1).min(last);
        let a = &self.frames[i];
        let b = &self.frames[(i + 1).min(last)];
        let span = b.et - a.et;
        let k = if span > 1e-6 { ((target - a.et) / span).clamp(0.0, 1.0) as f32 } else { 0.0 };
        let roster = &self.rosters[self.roster_of[i] as usize];
        let h = &self.header;

        put("session.track".into(), json!(h.track));
        put("session.type".into(), json!(h.session_type));
        put("session.phase".into(), json!(a.phase));
        put("session.yellow".into(), json!(a.yellow));
        put("session.realtime".into(), json!(true));
        put("session.time".into(), json!(target));
        put("session.remaining".into(), json!(a.remaining));
        put("session.max_laps".into(), json!(h.max_laps));
        put("session.track_length".into(), json!(h.track_length));
        put("session.vehicles".into(), json!(a.cars.len()));
        put("weather.rain".into(), json!(a.rain));
        put("weather.ambient".into(), json!(a.ambient));
        put("weather.track".into(), json!(a.track_temp));
        for n in 0..3 {
            put(format!("session.sector_yellow.{}", n + 1), json!(a.sectors >> n & 1 == 1));
        }
        put("player.name".into(), json!(h.player));
        if !player_recorded_header(self, i) {
            put("player.car".into(), json!(h.car));
            put("player.class".into(), json!(h.class));
        }
        put("demo.map".into(), json!(false));

        // Voiture du joueur : toutes ses données (carburant, pneus, pédales, tours…), nombres interpolés entre deux images.
        let schema = &self.schemas[self.schema_of[i] as usize];
        let player_recorded = !schema.0.is_empty() && a.nums.len() == schema.count(false);
        if player_recorded {
            let same = self.schema_of[(i + 1).min(last)] == self.schema_of[i] && b.nums.len() == a.nums.len();
            let (mut ni, mut ti) = (0, 0);
            for (key, kind) in &schema.0 {
                match kind {
                    Kind::Text => {
                        put(key.clone(), json!(a.texts.get(ti).cloned().unwrap_or_default()));
                        ti += 1;
                    }
                    _ => {
                        let (va, vb) = (a.nums[ni], if same { b.nums[ni] } else { a.nums[ni] });
                        ni += 1;
                        put(
                            key.clone(),
                            match kind {
                                Kind::Bool => json!(va != 0.0),
                                Kind::Int => json!(va as i64),
                                _ => json!(va + (vb - va) * k),
                            },
                        );
                    }
                }
            }
        }

        for (slot, ca) in a.cars.iter().enumerate() {
            let cb = b.cars.iter().find(|c| c.idx == ca.idx).unwrap_or(ca);
            // Pas d'interpolation si la voiture a « sauté » (retour au stand, téléportation).
            let near = (cb.x - ca.x).abs() < 150.0 && (cb.z - ca.z).abs() < 150.0;
            let lerp = |p: f32, q: f32| if near { p + (q - p) * k } else { p };
            // En piste : la voiture avance le long du tracé, de sa distance dans le tour à celle de l'image suivante
            // (elle suit donc les virages, même quand les images sont espacées). Aux stands, ou sans carte : ligne droite.
            let length = h.track_length as f64;
            let on_track = !ca.in_pits && !cb.in_pits && length > 100.0;
            let (x, z) = match &self.path {
                Some(path) if on_track => {
                    let mut ahead = (cb.lap_dist - ca.lap_dist) as f64;
                    if ahead < -length * 0.5 {
                        ahead += length; // passage de la ligne
                    }
                    // Recul ou bond énorme : retour au stand, remise en piste… on reste sur l'image courante.
                    let ahead = if (0.0..length * 0.5).contains(&ahead) { ahead } else { 0.0 };
                    path.at((ca.lap_dist as f64 + ahead * k as f64) / length)
                }
                _ => (lerp(ca.x, cb.x), lerp(ca.z, cb.z)),
            };
            let (name, class) = roster.get(ca.idx as usize).cloned().unwrap_or_default();
            let p = format!("grid.{slot:03}");
            put(format!("{p}.driver"), json!(name));
            put(format!("{p}.class"), json!(class));
            put(format!("{p}.place"), json!(ca.place));
            put(format!("{p}.laps"), json!(ca.laps));
            put(format!("{p}.last_lap"), json!(ca.last_lap));
            put(format!("{p}.best_lap"), json!(ca.best_lap));
            put(format!("{p}.behind_next"), json!(ca.behind_next));
            put(format!("{p}.behind_leader"), json!(ca.behind_leader));
            put(format!("{p}.in_pits"), json!(ca.in_pits));
            put(format!("{p}.is_player"), json!(ca.is_player));
            put(format!("{p}.x"), json!(x));
            put(format!("{p}.z"), json!(z));
            put(format!("{p}.lap_dist"), json!(lerp(ca.lap_dist, cb.lap_dist)));
            if ca.is_player && !player_recorded {
                put("position.place".into(), json!(ca.place));
                put("position.laps".into(), json!(ca.laps));
                put("position.lap_dist".into(), json!(ca.lap_dist));
                put("position.last_lap".into(), json!(ca.last_lap));
                put("position.best_lap".into(), json!(ca.best_lap));
                put("position.behind_next".into(), json!(ca.behind_next));
                put("position.behind_leader".into(), json!(ca.behind_leader));
                put("lap.number".into(), json!(ca.laps as i64 + 1));
                put("pit.in_pits".into(), json!(ca.in_pits));
            }
        }
        Data::new(out)
    }
}

fn player_recorded_header(r: &Replay, i: usize) -> bool {
    !r.schemas[r.schema_of[i] as usize].0.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(et: f64, x: f32) -> Frame {
        let car = |idx: u8, x: f32, player: bool| Car { idx, place: idx + 1, laps: 2, is_player: player, x, z: 5.0, lap_dist: x, ..Default::default() };
        Frame { et, phase: 5, sectors: 0b010, cars: vec![car(0, x, true), car(1, x + 10.0, false)], nums: vec![x, 1.0, 7.0], texts: vec!["Soft".into()], ..Default::default() }
    }

    #[test]
    fn write_then_read_and_interpolate() {
        let path = std::env::temp_dir().join("pw-replay-test.pwr");
        let h = Header { track: "Le Mans".into(), player: "Moi".into(), ..Default::default() };
        let schema = Schema(vec![("player.speed".into(), Kind::Num), ("fuel.low".into(), Kind::Bool), ("lap.number".into(), Kind::Int), ("tyre.front.compound".into(), Kind::Text)]);
        let roster: Roster = vec![("Moi".into(), "Hypercar".into()), ("Lui".into(), "LMGT3".into())];
        {
            let mut w = Writer::create(&path, &h).unwrap();
            for n in 0..300 {
                w.push(&frame(100.0 + n as f64 * 0.5, n as f32 * 10.0), &roster, &schema);
            }
        } // Drop : dernier bloc écrit.
        let r = Replay::load(&path).unwrap();
        assert_eq!(r.header, h);
        assert_eq!(r.frames.len(), 300);
        assert!((r.duration() - 149.5).abs() < 1e-6);
        // À 0,25 s : entre les images 0 (x = 0) et 1 (x = 10).
        let d = r.data_at(0.25);
        assert_eq!(d.text("session.track"), "Le Mans");
        assert!((d.f("grid.000.x", -1.0) - 5.0).abs() < 1e-3);
        assert_eq!(d.text("grid.001.driver"), "Lui");
        assert_eq!(d.text("grid.001.class"), "LMGT3");
        assert!(d.flag("session.sector_yellow.2") && !d.flag("session.sector_yellow.1"));
        assert_eq!(d.f("lap.number", 0.0), 7.0);
        assert!((d.f("player.speed", -1.0) - 5.0).abs() < 1e-3 && d.flag("fuel.low") && d.text("tyre.front.compound") == "Soft");
        assert_eq!(r.series("player.speed").len(), 300);
        assert!(r.laps_of(0).is_empty() && r.pits_of(0).is_empty());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn invalid_laps_are_kept_without_a_time() {
        let path = std::env::temp_dir().join("pw-replay-laps.pwr");
        let roster: Roster = vec![("Moi".into(), "Hypercar".into())];
        {
            let mut w = Writer::create(&path, &Header::default()).unwrap();
            for n in 0..60u16 {
                // Tour 1 bouclé à l'image 20 (chrono officiel), tour 2 à l'image 40 (invalidé : pas de chrono).
                let (laps, last) = if n < 20 { (0, 0.0) } else if n < 40 { (1, 95.5) } else { (2, -1.0) };
                let car = Car { idx: 0, place: 1, laps, last_lap: last, is_player: true, ..Default::default() };
                w.push(&Frame { et: n as f64 * 0.5, cars: vec![car], ..Default::default() }, &roster, &Schema::default());
            }
        }
        let r = Replay::load(&path).unwrap();
        let laps = r.laps_of(0);
        assert_eq!(laps.len(), 2);
        assert!(laps[0].valid && (laps[0].time - 95.5).abs() < 1e-6);
        assert!(!laps[1].valid && laps[1].time == 0.0);
        let _ = std::fs::remove_file(path);
    }
}
