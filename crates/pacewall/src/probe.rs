//! `pacewall.exe --probe` : relève pendant 20 s ce que le jeu publie vraiment, et ouvre le résultat dans le Bloc-notes.
//! Sert à comprendre un overlay qui clignote ou qui ne se rafraîchit pas : à quelle cadence chaque signal bouge-t-il ?

use lmu::reader::Reader;
use std::fmt::Write as _;
use std::time::{Duration, Instant};

/// Un signal surveillé : nombre de changements et plus longue pause entre deux changements.
struct Signal {
    name: &'static str,
    last: f64,
    last_change: Instant,
    changes: u32,
    longest: Duration,
}

impl Signal {
    fn new(name: &'static str) -> Self {
        Self { name, last: f64::NAN, last_change: Instant::now(), changes: 0, longest: Duration::ZERO }
    }

    fn feed(&mut self, v: f64) {
        if v != self.last {
            if !self.last.is_nan() {
                self.changes += 1;
                self.longest = self.longest.max(self.last_change.elapsed());
            }
            self.last = v;
            self.last_change = Instant::now();
        }
    }
}

pub fn run() {
    let mut out = String::new();
    let path = std::env::temp_dir().join("pw-probe.txt");
    let _ = writeln!(out, "PaceWall — relevé du jeu (20 s, lecture toutes les 50 ms)\n");
    match Reader::open() {
        None => {
            let _ = writeln!(out, "Le jeu n'a pas créé la zone mémoire LMU_Data.\nLance LMU (Enable Plugins activé), mets-toi en piste et relance cette commande.");
        }
        Some(reader) => {
            let _ = writeln!(out, "Version du jeu : {}", reader.game_version());
            let mut sigs = [
                Signal::new("événement télémétrie"),
                Signal::new("événement scoring"),
                Signal::new("temps de session (scoring)"),
                Signal::new("temps voiture joueur (télémétrie)"),
                Signal::new("vitesse joueur"),
            ];
            let start = Instant::now();
            let mut next_line = Duration::from_secs(2);
            let mut last_values = String::new();
            while start.elapsed() < Duration::from_secs(20) {
                let ev = reader.events();
                let info = reader.scoring_info();
                sigs[0].feed(ev.SME_UPDATE_TELEMETRY as f64);
                sigs[1].feed(ev.SME_UPDATE_SCORING as f64);
                sigs[2].feed(info.mCurrentET);
                if let Some(t) = reader.player_telemetry() {
                    sigs[3].feed(t.mElapsedTime);
                    let v = t.mLocalVel;
                    sigs[4].feed(((v.x * v.x + v.y * v.y + v.z * v.z) as f64).sqrt() * 3.6);
                }
                if start.elapsed() >= next_line {
                    next_line += Duration::from_secs(2);
                    let (et, e_tel, e_sco) = (info.mCurrentET, ev.SME_UPDATE_TELEMETRY, ev.SME_UPDATE_SCORING);
                    last_values = format!("événements télémétrie={e_tel} scoring={e_sco}, temps session={et:.2}");
                    let _ = writeln!(out, "{:>3} s : {}", start.elapsed().as_secs(), last_values);
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            let _ = last_values;
            let _ = writeln!(out, "\nChangements sur 20 s (et plus longue pause entre deux changements) :");
            for s in &sigs {
                let _ = writeln!(out, "  {:<36} {:>5} changements, pause max {:>5} ms", s.name, s.changes, s.longest.as_millis());
            }
            let _ = writeln!(out, "\nUn signal à 0 changement ne bouge pas : l'overlay ne peut pas s'en servir pour savoir que le jeu publie.");
        }
    }
    let _ = std::fs::write(&path, out);
    let _ = std::process::Command::new("notepad").arg(&path).spawn();
}
