//! Modèle d'une session de course fictive mais réaliste : un circuit de ~5,9 km tracé comme un vrai (lignes droites,
//! virages de rayons variés, chicanes), 32 voitures de trois classes avec leur rythme et leur régularité, des tours
//! chronométrés, de la consommation, des pneus qui s'usent et chauffent, des arrêts aux stands, des drapeaux jaunes locaux,
//! un full course yellow, de la pluie et des drapeaux bleus.
//!
//! Le modèle ne dépend de rien : il avance par pas de temps (`step`) et se lit par des accesseurs ; `lib.rs` le publie
//! dans la zone mémoire du « jeu ». Toutes les durées sont en secondes de jeu : une accélération (`speed`) fait avancer
//! le temps de jeu plus vite que l'horloge, pour qu'une course entière tienne en quelques minutes.

use std::f64::consts::PI;

/// Type de session simulée.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Practice,
    Qualifying,
    Race,
}

impl Kind {
    /// Code de session du jeu (`mSession`).
    pub fn code(self) -> i32 {
        match self {
            Kind::Practice => 1,
            Kind::Qualifying => 5,
            Kind::Race => 10,
        }
    }

    pub fn parse(s: &str) -> Option<Kind> {
        match s {
            "practice" | "essais" => Some(Kind::Practice),
            "quali" | "qualifying" | "qualifs" => Some(Kind::Qualifying),
            "race" | "course" => Some(Kind::Race),
            _ => None,
        }
    }
}

/// Réglages de la session simulée.
#[derive(Clone, Copy, Debug)]
pub struct Config {
    pub kind: Kind,
    /// Accélération du temps de jeu par rapport à l'horloge.
    pub speed: f64,
    /// Nombre de tours de la course.
    pub laps: u32,
    pub seed: u64,
    /// De la pluie arrive en cours de session.
    pub rain: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self { kind: Kind::Race, speed: 6.0, laps: 18, seed: 7, rain: true }
    }
}

/// Générateur pseudo-aléatoire minimal (splitmix64) : résultats reproductibles avec la même graine.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(0x1234_5678))
    }

    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Nombre dans [0, 1).
    pub fn f(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    pub fn range(&mut self, a: f64, b: f64) -> f64 {
        a + (b - a) * self.f()
    }

    /// Loi normale centrée réduite (Box-Muller).
    pub fn gauss(&mut self) -> f64 {
        let (u, v) = (self.f().max(1e-12), self.f());
        (-2.0 * u.ln()).sqrt() * (2.0 * PI * v).cos()
    }
}

/* ------------------------------------------------------------------ circuit */

pub struct Track {
    /// Ligne centrale, un point tous les `ds` mètres.
    pub pts: Vec<[f64; 2]>,
    pub ds: f64,
    pub len: f64,
    /// Courbure signée (1/m) en chaque point.
    pub kappa: Vec<f64>,
    /// Vitesse de référence (m/s) d'une Hypercar en chaque point : limite en virage, freinage et accélération.
    pub profile: Vec<f64>,
    /// Fin des secteurs 1 et 2 (mètres).
    pub sectors: [f64; 2],
    /// Entrée de la voie des stands, emplacement du stand, sortie (mètres).
    pub pit_in: f64,
    pub pit_box: f64,
    pub pit_out: f64,
}

/// Points de contrôle d'un circuit fictif (mètres, repère quelconque) : grande ligne droite, enchaînement rapide,
/// épingle, chicane et retour.
const CONTROL: [(f64, f64); 26] = [
    (0.0, 0.0), (700.0, 0.0), (1300.0, 20.0), (1650.0, 150.0), (1850.0, 420.0), (1820.0, 740.0), (1600.0, 930.0),
    (1300.0, 900.0), (1120.0, 1000.0), (1180.0, 1200.0), (1050.0, 1380.0), (1180.0, 1580.0), (1480.0, 1680.0),
    (1560.0, 1960.0), (1300.0, 2150.0), (900.0, 2100.0), (640.0, 1900.0), (330.0, 1980.0), (-60.0, 1900.0),
    (-330.0, 1650.0), (-320.0, 1300.0), (-560.0, 1050.0), (-620.0, 700.0), (-430.0, 380.0), (-200.0, 180.0), (-60.0, 40.0),
];

impl Track {
    pub fn build(target_len: f64) -> Self {
        // Catmull-Rom fermé, échantillonné fin, puis remis à l'échelle et ré-échantillonné tous les ~10 m.
        let n = CONTROL.len();
        let mut dense: Vec<[f64; 2]> = Vec::new();
        for i in 0..n {
            let (p0, p1, p2, p3) = (CONTROL[(i + n - 1) % n], CONTROL[i], CONTROL[(i + 1) % n], CONTROL[(i + 2) % n]);
            for k in 0..40 {
                let t = k as f64 / 40.0;
                let (t2, t3) = (t * t, t * t * t);
                let f = |a: f64, b: f64, c: f64, d: f64| 0.5 * ((2.0 * b) + (-a + c) * t + (2.0 * a - 5.0 * b + 4.0 * c - d) * t2 + (-a + 3.0 * b - 3.0 * c + d) * t3);
                dense.push([f(p0.0, p1.0, p2.0, p3.0), f(p0.1, p1.1, p2.1, p3.1)]);
            }
        }
        let dist = |a: [f64; 2], b: [f64; 2]| ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt();
        let raw_len: f64 = (0..dense.len()).map(|i| dist(dense[i], dense[(i + 1) % dense.len()])).sum();
        let scale = target_len / raw_len;
        for p in dense.iter_mut() {
            *p = [p[0] * scale, p[1] * scale];
        }
        // Ré-échantillonnage uniforme.
        let count = (target_len / 10.0).round() as usize;
        let ds = target_len / count as f64;
        let mut pts = Vec::with_capacity(count);
        let (mut acc, mut idx) = (0.0, 0usize);
        let mut seg_len = dist(dense[0], dense[1 % dense.len()]);
        for k in 0..count {
            let want = k as f64 * ds;
            while acc + seg_len < want && idx < dense.len() - 1 {
                acc += seg_len;
                idx += 1;
                seg_len = dist(dense[idx], dense[(idx + 1) % dense.len()]);
            }
            let (a, b) = (dense[idx], dense[(idx + 1) % dense.len()]);
            let t = if seg_len > 0.0 { ((want - acc) / seg_len).clamp(0.0, 1.0) } else { 0.0 };
            pts.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]);
        }
        let len = target_len;
        // Courbure signée, lissée sur trois points.
        let m = pts.len();
        let raw_k: Vec<f64> = (0..m)
            .map(|i| {
                let (a, b, c) = (pts[(i + m - 1) % m], pts[i], pts[(i + 1) % m]);
                let (v1, v2) = ([b[0] - a[0], b[1] - a[1]], [c[0] - b[0], c[1] - b[1]]);
                let cross = v1[0] * v2[1] - v1[1] * v2[0];
                let dot = v1[0] * v2[0] + v1[1] * v2[1];
                cross.atan2(dot) / ds
            })
            .collect();
        let kappa: Vec<f64> = (0..m).map(|i| (raw_k[(i + m - 1) % m] + raw_k[i] + raw_k[(i + 1) % m]) / 3.0).collect();
        // Profil de vitesse : limite en virage, puis passes avant et arrière pour freinage et accélération.
        let (a_lat, a_acc, a_brake, vtop) = (23.0f64, 7.5f64, 19.0f64, 94.0f64);
        let mut v: Vec<f64> = kappa.iter().map(|k| (a_lat / k.abs().max(1e-4)).sqrt().min(vtop)).collect();
        for _ in 0..3 {
            for i in (0..m * 2).rev() {
                let (a, b) = (i % m, (i + 1) % m);
                v[a] = v[a].min((v[b] * v[b] + 2.0 * a_brake * ds).sqrt());
            }
            for i in 0..m * 2 {
                let (a, b) = (i % m, (i + 1) % m);
                v[b] = v[b].min((v[a] * v[a] + 2.0 * a_acc * ds).sqrt());
            }
        }
        Self { pts, ds, len, kappa, profile: v, sectors: [len * 0.31, len * 0.67], pit_in: len - 320.0, pit_box: len - 130.0, pit_out: 260.0 }
    }

    fn index(&self, s: f64) -> (usize, f64) {
        let x = s.rem_euclid(self.len) / self.ds;
        let i = x.floor() as usize % self.pts.len();
        (i, x - x.floor())
    }

    /// Position dans le monde (x, z) à la distance `s` du tour.
    pub fn pos(&self, s: f64) -> [f64; 2] {
        let (i, t) = self.index(s);
        let (a, b) = (self.pts[i], self.pts[(i + 1) % self.pts.len()]);
        [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
    }

    pub fn kappa_at(&self, s: f64) -> f64 {
        self.kappa[self.index(s).0]
    }

    pub fn profile_at(&self, s: f64) -> f64 {
        let (i, t) = self.index(s);
        self.profile[i] + (self.profile[(i + 1) % self.profile.len()] - self.profile[i]) * t
    }

    /// Secteur du jeu (1, 2 ou 0 pour le secteur 3) à la distance `s`.
    pub fn sector_code(&self, s: f64) -> i8 {
        let s = s.rem_euclid(self.len);
        if s < self.sectors[0] {
            1
        } else if s < self.sectors[1] {
            2
        } else {
            0
        }
    }

    /// Temps d'un tour d'une Hypercar au rythme de référence (secondes), d'après le profil.
    pub fn reference_lap(&self) -> f64 {
        self.profile.iter().map(|v| self.ds / v).sum()
    }
}

/* ------------------------------------------------------------------ voitures */

pub struct Class {
    pub name: &'static str,
    /// Vitesse par rapport à l'Hypercar.
    pub scale: f64,
    pub tank: f64,
    /// Litres par kilomètre.
    pub l_per_km: f64,
    /// Énergie virtuelle consommée (% par km) ; 0 si la classe n'en a pas.
    pub energy_per_km: f64,
    /// Tours entre deux arrêts.
    pub stint: f64,
    pub models: &'static [&'static str],
}

pub const CLASSES: [Class; 3] = [
    Class { name: "Hypercar", scale: 1.0, tank: 90.0, l_per_km: 1.00, energy_per_km: 1.25, stint: 12.0, models: &["Toyota GR010", "Ferrari 499P", "Porsche 963", "Cadillac V-Series.R", "Peugeot 9X8", "BMW M Hybrid V8", "Alpine A424"] },
    Class { name: "LMP2", scale: 0.935, tank: 75.0, l_per_km: 0.82, energy_per_km: 0.0, stint: 14.0, models: &["Oreca 07"] },
    Class { name: "LMGT3", scale: 0.865, tank: 100.0, l_per_km: 0.92, energy_per_km: 0.0, stint: 11.0, models: &["Ferrari 296 GT3", "Porsche 911 GT3 R", "McLaren 720S GT3", "BMW M4 GT3", "Aston Martin Vantage", "Corvette Z06 GT3.R", "Lexus RC F GT3"] },
];

const DRIVERS: [&str; 34] = [
    "Toi", "A. Martin", "B. Dupont", "C. Rossi", "D. Keller", "E. Sato", "F. Novak", "G. Silva", "H. Moreau", "I. Weber", "J. Costa",
    "K. Berg", "L. Fontaine", "M. Haas", "N. Ortega", "O. Lindqvist", "P. Marchand", "Q. Bianchi", "R. Dubois", "S. Kovacs",
    "T. Laurent", "U. Schmidt", "V. Ferrari", "W. Gomez", "X. Petit", "Y. Hoffman", "Z. Navarro", "A. Roux", "B. Vidal",
    "C. Lambert", "D. Morel", "E. Girard", "F. Bernard", "G. Muller",
];

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Pit {
    /// Sur la piste.
    Track,
    /// Dans la voie des stands, en route vers son stand.
    Lane,
    /// À l'arrêt dans son stand (secondes restantes).
    Box(f64),
    /// Repart vers la sortie de la voie des stands.
    Out,
}

pub struct Car {
    pub id: usize,
    pub name: String,
    pub model: String,
    pub class: usize,
    pub number: u32,
    /// Rythme de base (1 = référence de la classe) : le pilote.
    pub skill: f64,
    /// Rythme du tour en cours (variation tour à tour).
    pub lap_pace: f64,
    /// Distance parcourue dans le tour (m).
    pub s: f64,
    pub v: f64,
    pub prev_v: f64,
    /// Tours terminés.
    pub laps: u32,
    pub lap_start: f64,
    pub last_lap: f64,
    pub best_lap: f64,
    pub fuel: f64,
    pub energy: f64,
    /// Usure des pneus, 0 (neufs) à 1.
    pub wear: f64,
    pub carcass: f64,
    pub pit: Pit,
    pub stops: u32,
    pub lap_in_pit: bool,
    pub finished: bool,
    /// A franchi la ligne de départ (course : avant, la voiture est sur la grille).
    pub started: bool,
    pub flat: bool,
    pub stint_start_lap: u32,
    /// Position actuelle de la voiture (calculée à chaque pas).
    pub x: f64,
    pub z: f64,
    pub throttle: f64,
    pub brake: f64,
    pub brake_temp: f64,
    pub sector_mark: f64,
    pub last_sector: i8,
}

/// Événement planifié de la session.
#[derive(Clone, Copy, Debug)]
pub enum Event {
    /// Jaune local dans un secteur (1 à 3), durée.
    Yellow(u8, f64),
    /// Full course yellow, durée.
    Fcy(f64),
    /// Début de la pluie.
    Rain,
}

pub struct Sim {
    pub cfg: Config,
    pub track: Track,
    pub cars: Vec<Car>,
    /// Temps de jeu écoulé (s).
    pub t: f64,
    pub weather_rain: f64,
    pub wetness: f64,
    pub phase: u8,
    pub sector_flags: [bool; 3],
    /// Durée de la session (s) pour essais et qualifs ; 0 en course.
    pub duration: f64,
    pub leader_done: bool,
    pub finish_t: f64,
    /// Instants de début des événements et événement en cours (jusqu'à `until`).
    events: Vec<(f64, Event)>,
    active_yellow: Vec<(usize, f64)>,
    fcy_until: f64,
    rain_from: Option<f64>,
    rng: Rng,
    /// Compte à rebours de départ (s de jeu avant le feu vert), course seulement.
    pub start_t: f64,
}

impl Sim {
    pub fn new(cfg: Config) -> Self {
        let track = Track::build(5900.0);
        let mut rng = Rng::new(cfg.seed);
        let ref_lap = track.reference_lap();
        let mut cars: Vec<Car> = Vec::new();
        // 10 Hypercar, 8 LMP2, 14 LMGT3 ; la voiture du joueur (n° 7, Hypercar) est la première.
        let layout = [(0usize, 10usize), (1, 8), (2, 14)];
        for (class, count) in layout {
            for k in 0..count {
                let id = cars.len();
                let models = CLASSES[class].models;
                let number = if id == 0 { 7 } else { 2 + (id as u32 * 3) % 97 };
                cars.push(Car {
                    id,
                    name: DRIVERS[id % DRIVERS.len()].to_string(),
                    model: models[k % models.len()].to_string(),
                    class,
                    number,
                    skill: if id == 0 { 0.996 } else { 1.0 - rng.range(0.0, 0.016) },
                    lap_pace: 1.0,
                    s: 0.0,
                    v: 0.0,
                    prev_v: 0.0,
                    laps: 0,
                    lap_start: 0.0,
                    last_lap: 0.0,
                    best_lap: 0.0,
                    fuel: CLASSES[class].tank,
                    energy: 100.0,
                    wear: 0.0,
                    carcass: 62.0,
                    pit: Pit::Track,
                    stops: 0,
                    lap_in_pit: false,
                    finished: false,
                    started: false,
                    flat: false,
                    stint_start_lap: 0,
                    x: 0.0,
                    z: 0.0,
                    throttle: 0.0,
                    brake: 0.0,
                    brake_temp: 350.0,
                    sector_mark: 0.0,
                    last_sector: 1,
                });
            }
        }
        let race = cfg.kind == Kind::Race;
        // Grille de départ : les classes dans l'ordre, le joueur en 4e position, 9 m entre les voitures.
        let mut order: Vec<usize> = (0..cars.len()).collect();
        order.sort_by(|&a, &b| {
            let key = |c: &Car| (c.class, if c.id == 0 { 3.5 } else { c.id as f64 });
            key(&cars[a]).partial_cmp(&key(&cars[b])).unwrap()
        });
        for (slot, &i) in order.iter().enumerate() {
            if race {
                // Sur la grille, derrière la ligne : distance négative, qui devient positive au franchissement.
                cars[i].s = -(20.0 + slot as f64 * 9.0);
                cars[i].lap_in_pit = false;
            } else {
                // Essais et qualifs : les voitures sont déjà en piste, réparties sur le tour.
                cars[i].s = rng.range(0.0, track.len);
                cars[i].v = track.profile_at(cars[i].s) * CLASSES[cars[i].class].scale * 0.9;
                cars[i].stint_start_lap = 0;
                cars[i].started = true;
            }
            cars[i].lap_pace = 1.0;
            cars[i].laps = 0;
        }
        // Événements.
        let race_secs = cfg.laps as f64 * ref_lap * 1.12;
        let mut events = Vec::new();
        if race {
            for k in 0..3 {
                let at = race_secs * (0.18 + 0.25 * k as f64) + rng.range(-40.0, 40.0);
                events.push((at, Event::Yellow(1 + (rng.f() * 3.0) as u8 % 3, rng.range(30.0, 55.0))));
            }
            events.push((race_secs * 0.55, Event::Fcy(75.0)));
        } else {
            for k in 0..2 {
                events.push((300.0 + 420.0 * k as f64 + rng.range(-60.0, 60.0), Event::Yellow(1 + (rng.f() * 3.0) as u8 % 3, rng.range(25.0, 45.0))));
            }
        }
        if cfg.rain {
            events.push((race_secs * 0.62, Event::Rain));
        }
        events.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        let duration = match cfg.kind {
            Kind::Race => 0.0,
            Kind::Qualifying => 900.0,
            Kind::Practice => 3600.0,
        };
        let mut sim = Self {
            cfg,
            track,
            cars,
            t: 0.0,
            weather_rain: 0.0,
            wetness: 0.0,
            phase: if race { 4 } else { 5 },
            sector_flags: [false; 3],
            duration,
            leader_done: false,
            finish_t: 0.0,
            events,
            active_yellow: Vec::new(),
            fcy_until: 0.0,
            rain_from: None,
            rng,
            start_t: if race { 14.0 } else { 0.0 },
        };
        sim.update_positions();
        sim
    }

    pub fn player(&self) -> &Car {
        &self.cars[0]
    }

    /// Nombre de tours de la course (0 en essais et qualifs).
    pub fn race_laps(&self) -> u32 {
        if self.cfg.kind == Kind::Race {
            self.cfg.laps
        } else {
            0
        }
    }

    /// Distance totale parcourue (m).
    pub fn total(&self, c: &Car) -> f64 {
        c.laps as f64 * self.track.len + c.s
    }

    /// Classement : par distance en course, par meilleur tour sinon. Renvoie les indices de voitures, du 1er au dernier.
    pub fn order(&self) -> Vec<usize> {
        let mut idx: Vec<usize> = (0..self.cars.len()).collect();
        if self.cfg.kind == Kind::Race {
            idx.sort_by(|&a, &b| self.total(&self.cars[b]).partial_cmp(&self.total(&self.cars[a])).unwrap());
        } else {
            let key = |c: &Car| if c.best_lap > 0.0 { c.best_lap } else { 1.0e9 + c.id as f64 };
            idx.sort_by(|&a, &b| key(&self.cars[a]).partial_cmp(&key(&self.cars[b])).unwrap());
        }
        idx
    }

    pub fn fcy(&self) -> bool {
        self.t < self.fcy_until
    }

    fn update_positions(&mut self) {
        for c in self.cars.iter_mut() {
            let p = self.track.pos(c.s);
            c.x = p[0];
            c.z = p[1];
        }
    }

    /// Avance la session de `dt` secondes de jeu (découpé en pas de 0,1 s).
    pub fn step(&mut self, dt: f64) {
        let mut left = dt;
        while left > 1e-9 {
            let h = left.min(0.1);
            self.micro_step(h);
            left -= h;
        }
        self.update_positions();
    }

    fn micro_step(&mut self, dt: f64) {
        self.t += dt;
        let t = self.t;
        // Départ : feux, puis feu vert.
        if self.cfg.kind == Kind::Race && self.phase == 4 && t >= self.start_t {
            self.phase = 5;
        }
        // Événements.
        while let Some(&(at, ev)) = self.events.first() {
            if at > t {
                break;
            }
            self.events.remove(0);
            match ev {
                Event::Yellow(sector, secs) => {
                    let k = (sector as usize - 1).min(2);
                    self.sector_flags[k] = true;
                    self.active_yellow.push((k, t + secs));
                }
                Event::Fcy(secs) => self.fcy_until = t + secs,
                Event::Rain => self.rain_from = Some(t),
            }
        }
        self.active_yellow.retain(|&(_, until)| until > t);
        self.sector_flags = [false; 3];
        for &(k, _) in &self.active_yellow {
            self.sector_flags[k] = true;
        }
        let fcy = self.fcy();
        if self.cfg.kind == Kind::Race && self.phase != 4 && !self.leader_done {
            self.phase = if fcy { 6 } else { 5 };
        }
        // Météo : la pluie monte en deux minutes, la piste se mouille avec retard.
        let target_rain = match self.rain_from {
            Some(from) => (0.35 * ((t - from) / 120.0).clamp(0.0, 1.0)).max(0.0),
            None => 0.0,
        };
        self.weather_rain += (target_rain - self.weather_rain) * (dt * 0.05).min(1.0);
        let target_wet = (self.weather_rain * 2.4).min(0.85);
        self.wetness += (target_wet - self.wetness) * (dt * 0.01).min(1.0);

        let len = self.track.len;
        let ref_lap = self.track.reference_lap();
        let race = self.cfg.kind == Kind::Race;
        let (yellow, wet, leader_done) = (self.sector_flags, self.wetness, self.leader_done);
        let racing = !race || self.phase != 4;
        let end_laps = self.race_laps();
        let mut new_leader_done = leader_done;
        let mut finishers: Vec<usize> = Vec::new();

        for i in 0..self.cars.len() {
            let class = &CLASSES[self.cars[i].class];
            let track = &self.track;
            let c = &mut self.cars[i];
            c.prev_v = c.v;
            // Vitesse visée.
            let mut target = track.profile_at(c.s) * class.scale * c.skill * c.lap_pace;
            // Pneus usés et piste mouillée ralentissent.
            target *= 1.0 - 0.045 * c.wear.powi(2) - 0.09 * wet;
            if c.flat {
                target *= 0.55;
            }
            // Jaune local : on lève le pied dans le secteur concerné.
            let sector_idx = match track.sector_code(c.s) {
                1 => 0,
                2 => 1,
                _ => 2,
            };
            if yellow[sector_idx] && racing {
                target *= 0.82;
            }
            if fcy && !c.finished {
                target = target.min(24.0);
            }
            match c.pit {
                Pit::Lane | Pit::Out => target = target.min(22.0),
                Pit::Box(_) => target = 0.0,
                Pit::Track => {}
            }
            if !racing {
                target = 0.0;
            }
            if c.finished {
                target = target.min(38.0);
            }
            // Dynamique : accélération et freinage limités.
            let (a_acc, a_brake) = (7.5, 19.0);
            let dv = (target - c.v).clamp(-a_brake * dt, a_acc * dt);
            c.v = (c.v + dv).max(0.0);
            let ds = c.v * dt;
            c.s += ds;
            // Consommation, usure, température.
            let km = ds / 1000.0;
            let push = (c.v / 90.0).clamp(0.0, 1.0);
            let accel = (c.v - c.prev_v) / dt.max(1e-6);
            c.throttle = if accel > -0.5 { (0.35 + accel / 6.0).clamp(0.0, 1.0) } else { 0.0 };
            c.brake = if accel < -1.5 { (-accel / 16.0).clamp(0.0, 1.0) } else { 0.0 };
            c.fuel = (c.fuel - km * class.l_per_km * (0.82 + 0.36 * c.throttle)).max(0.0);
            if class.energy_per_km > 0.0 {
                c.energy = (c.energy - km * class.energy_per_km * (0.85 + 0.3 * c.throttle)).max(0.0);
            }
            let lat = (track.kappa_at(c.s).abs() * c.v * c.v / 25.0).clamp(0.0, 1.4);
            c.wear = (c.wear + dt * 0.000095 * (0.5 + lat + 0.4 * push) * (1.0 + 0.4 * wet)).min(1.0);
            let target_temp = 74.0 + 26.0 * (0.55 * lat + 0.45 * push) - 22.0 * wet;
            c.carcass += (target_temp - c.carcass) * (dt * 0.03).min(1.0);
            c.brake_temp += ((300.0 + 520.0 * c.brake) - c.brake_temp) * (dt * 0.8).min(1.0);
            // Crevaison très rare.
            if !c.flat && self.rng.f() < dt * 0.000002 && i != 0 {
                c.flat = true;
            }

            // Franchissement de la ligne de départ : début du premier tour, aucun tour terminé.
            if !c.started && c.s >= 0.0 {
                c.started = true;
                c.lap_start = t;
            }
            // Passage de la ligne.
            if c.s >= len {
                c.s -= len;
                let lap_time = t - c.lap_start;
                c.laps += 1;
                {
                    c.last_lap = lap_time;
                    if !c.lap_in_pit && lap_time > ref_lap * 0.8 && (c.best_lap <= 0.0 || lap_time < c.best_lap) {
                        c.best_lap = lap_time;
                    }
                }
                c.lap_start = t;
                // Rythme du tour suivant : régularité propre à chaque pilote.
                c.lap_pace = 1.0 + self.rng.gauss() * 0.0035;
                if c.pit == Pit::Track {
                    c.lap_in_pit = false;
                }
                if race && end_laps > 0 && c.laps >= end_laps && !new_leader_done {
                    new_leader_done = true;
                }
                if new_leader_done && race && !c.finished {
                    finishers.push(i);
                }
            }
            c.sector_mark = c.s;
            c.last_sector = track.sector_code(c.s);

            // Arrêts aux stands.
            let stint = (c.laps - c.stint_start_lap) as f64;
            let low_fuel = c.fuel < class.l_per_km * (len / 1000.0) * 1.6;
            let low_energy = class.energy_per_km > 0.0 && c.energy < class.energy_per_km * (len / 1000.0) * 1.6;
            let worn = c.wear > 0.68;
            let planned = stint >= class.stint + (c.id as f64 % 3.0) - 1.0;
            let remaining_laps = end_laps.saturating_sub(c.laps);
            let want_stop = !c.finished && (race && remaining_laps > 1 || !race) && (low_fuel || low_energy || worn || c.flat || planned) && c.stops < 6;
            match c.pit {
                Pit::Track => {
                    if want_stop && c.s >= track.pit_in && c.s < track.pit_in + 40.0 {
                        c.pit = Pit::Lane;
                        c.lap_in_pit = true;
                    }
                }
                Pit::Lane => {
                    if c.s >= track.pit_box && c.s < track.pit_box + 30.0 {
                        c.s = track.pit_box;
                        c.v = 0.0;
                        // Arrêt : ravitaillement et pneus ; 24 à 38 s.
                        c.pit = Pit::Box(self.rng.range(24.0, 38.0));
                        c.stops += 1;
                    }
                }
                Pit::Box(left) => {
                    let left = left - dt;
                    if left <= 0.0 {
                        let class_tank = class.tank;
                        c.fuel = class_tank;
                        c.energy = 100.0;
                        c.wear = 0.0;
                        c.carcass = 58.0;
                        c.flat = false;
                        c.stint_start_lap = c.laps;
                        c.pit = Pit::Out;
                    } else {
                        c.pit = Pit::Box(left);
                    }
                }
                Pit::Out => {
                    if c.s >= track.pit_out && c.s < len * 0.5 {
                        c.pit = Pit::Track;
                    }
                }
            }
        }
        if new_leader_done && !self.leader_done {
            self.leader_done = true;
            self.finish_t = t;
            self.phase = 8;
        }
        for i in finishers {
            self.cars[i].finished = true;
        }
        // Qualifs : fin du temps réglementaire.
        if self.cfg.kind == Kind::Qualifying && self.duration > 0.0 && t >= self.duration && self.phase != 8 {
            self.phase = 8;
            for c in self.cars.iter_mut() {
                c.finished = true;
            }
        }
    }

    /// Temps restant de la session (s) : essais et qualifs ; en course, un grand nombre de secondes.
    pub fn remaining(&self) -> f64 {
        if self.duration > 0.0 {
            (self.duration - self.t).max(0.0)
        } else {
            (self.cfg.laps as f64 * self.track.reference_lap() * 1.2 - self.t).max(0.0)
        }
    }

    /// Drapeau bleu pour la voiture `i` : une voiture plus rapide (classe supérieure, ou qui la prend en tour) arrive
    /// derrière elle, à moins de 2,5 s.
    pub fn blue_flag(&self, i: usize) -> bool {
        let me = &self.cars[i];
        let v = me.v.max(25.0);
        let my_total = self.total(me);
        self.cars.iter().any(|o| {
            if o.id == me.id || o.pit != Pit::Track || !o.started {
                return false;
            }
            let faster = o.class < me.class || self.total(o) > my_total + self.track.len;
            let behind = (me.s - o.s).rem_euclid(self.track.len);
            faster && behind < 160.0 && behind / v < 2.5
        })
    }

    /// Écart en secondes (positif) entre la voiture `i` et la plus proche voiture devant ou derrière elle sur la piste.
    pub fn gap_to_neighbour(&self, i: usize, ahead: bool) -> f64 {
        let me = &self.cars[i];
        let v = me.v.max(20.0);
        let mut best = f64::MAX;
        for o in &self.cars {
            if o.id == me.id {
                continue;
            }
            let d = if ahead { (o.s - me.s).rem_euclid(self.track.len) } else { (me.s - o.s).rem_euclid(self.track.len) };
            if d > 0.5 && d < best {
                best = d;
            }
        }
        if best == f64::MAX {
            0.0
        } else {
            best / v
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_is_realistic() {
        let t = Track::build(5900.0);
        assert!((t.len - 5900.0).abs() < 1.0);
        // Un tour d'Hypercar entre 1:30 et 2:10.
        let lap = t.reference_lap();
        assert!((80.0..130.0).contains(&lap), "tour de référence {lap}");
        // Vitesse mini dans les virages lents, maxi sur les lignes droites.
        let vmin = t.profile.iter().cloned().fold(f64::MAX, f64::min);
        let vmax = t.profile.iter().cloned().fold(0.0, f64::max);
        assert!(vmin < 40.0 && vmax > 70.0, "{vmin} {vmax}");
        // Le tracé est fermé : le dernier point est proche du premier.
        let (a, b) = (t.pts[0], t.pts[t.pts.len() - 1]);
        assert!(((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt() < 30.0);
    }

    #[test]
    fn race_runs_with_pit_stops_and_a_finish() {
        let mut sim = Sim::new(Config { laps: 12, rain: false, ..Config::default() });
        // 3 heures de jeu au maximum pour finir 12 tours.
        for _ in 0..(3 * 3600 * 10) {
            sim.step(0.1);
            if sim.leader_done && sim.cars.iter().all(|c| c.finished) {
                break;
            }
        }
        assert!(sim.leader_done, "la course doit se terminer");
        // Classes : l'Hypercar le plus rapide devant le meilleur LMGT3.
        let order = sim.order();
        let best_hyper = order.iter().position(|&i| sim.cars[i].class == 0).unwrap();
        let best_gt3 = order.iter().position(|&i| sim.cars[i].class == 2).unwrap();
        assert!(best_hyper < best_gt3);
        // Au moins une voiture est passée aux stands et personne n'a un réservoir négatif.
        assert!(sim.cars.iter().any(|c| c.stops > 0));
        assert!(sim.cars.iter().all(|c| c.fuel >= 0.0 && c.wear <= 1.0));
        // Tours chronométrés plausibles.
        let lap = sim.player().best_lap;
        let r = sim.track.reference_lap();
        assert!(lap > r * 0.9 && lap < r * 1.4, "meilleur tour {lap} pour une référence de {r}");
    }

    #[test]
    fn fcy_and_yellow_events_happen() {
        let mut sim = Sim::new(Config { laps: 10, rain: true, ..Config::default() });
        let (mut fcy_seen, mut yellow_seen, mut rain_seen) = (false, false, false);
        for _ in 0..(2 * 3600 * 10) {
            sim.step(0.1);
            fcy_seen |= sim.fcy();
            yellow_seen |= sim.sector_flags.iter().any(|y| *y);
            rain_seen |= sim.weather_rain > 0.05;
            if sim.leader_done {
                break;
            }
        }
        assert!(fcy_seen && yellow_seen && rain_seen);
    }

    #[test]
    fn same_seed_same_race() {
        let run = || {
            let mut sim = Sim::new(Config { laps: 3, rain: false, ..Config::default() });
            for _ in 0..4000 {
                sim.step(0.1);
            }
            sim.cars.iter().map(|c| (c.s * 10.0) as i64).sum::<i64>()
        };
        assert_eq!(run(), run());
    }

    /// Bilan d'une course complète, pour juger du réalisme : `cargo test -p fakegame report -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn report() {
        let mut sim = Sim::new(Config { laps: 18, ..Config::default() });
        let mut log = Vec::new();
        let mut last_phase = sim.phase;
        for _ in 0..(3 * 3600 * 10) {
            sim.step(0.1);
            if sim.phase != last_phase {
                log.push(format!("t={:.0}s phase {} -> {}", sim.t, last_phase, sim.phase));
                last_phase = sim.phase;
            }
            if sim.leader_done && sim.cars.iter().all(|c| c.finished) {
                break;
            }
        }
        println!("tour de référence {:.1} s ; fin à t={:.0} s ({:.0} min de jeu)", sim.track.reference_lap(), sim.t, sim.t / 60.0);
        for l in &log {
            println!("{l}");
        }
        for (rank, &i) in sim.order().iter().enumerate().take(12) {
            let c = &sim.cars[i];
            println!("P{:<2} {:<12} {:<9} #{:<3} tours {:>2} arrêts {} meilleur {:.1} s dernier {:.1} s", rank + 1, c.name, CLASSES[c.class].name, c.number, c.laps, c.stops, c.best_lap, c.last_lap);
        }
        for (k, cl) in CLASSES.iter().enumerate() {
            let best = sim.cars.iter().filter(|c| c.class == k).map(|c| c.best_lap).fold(f64::MAX, f64::min);
            let stops: Vec<u32> = sim.cars.iter().filter(|c| c.class == k).map(|c| c.stops).collect();
            println!("{:<9} meilleur tour {:.1} s, arrêts {:?}", cl.name, best, stops);
        }
    }
}
