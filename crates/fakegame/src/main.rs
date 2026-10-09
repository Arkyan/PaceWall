//! Faux jeu en ligne de commande : crée la zone `LMU_Data` et y écrit une course simulée.
//! Variables : `FAKE_LAP_SECONDS` (durée d'un tour), `FAKE_FROZEN_EVENTS=1` (compteurs figés), `FAKE_YELLOW=1`
//! (jaune régulier), `FAKE_SCENARIO=1` (scénario qui déclenche toutes les alertes vocales), `FAKE_SESSION=race|practice|quali`
//! (session réaliste ; `FAKE_SPEED`, `FAKE_LAPS`).

fn main() {
    let mut opts = fakegame::Options::default();
    if let Some(v) = std::env::var("FAKE_LAP_SECONDS").ok().and_then(|v| v.parse().ok()) {
        opts.lap_seconds = v;
    }
    opts.frozen_events = std::env::var_os("FAKE_FROZEN_EVENTS").is_some();
    opts.yellow = std::env::var_os("FAKE_YELLOW").is_some();
    opts.scenario = std::env::var_os("FAKE_SCENARIO").is_some();
    // FAKE_SESSION=race|practice|quali : session réaliste (FAKE_SPEED = accélération, FAKE_LAPS = tours de course).
    if let Some(kind) = std::env::var("FAKE_SESSION").ok().and_then(|s| fakegame::SessionKind::parse(&s)) {
        let mut cfg = fakegame::SessionConfig { kind, ..Default::default() };
        if let Some(v) = std::env::var("FAKE_SPEED").ok().and_then(|v| v.parse().ok()) {
            cfg.speed = v;
        }
        if let Some(v) = std::env::var("FAKE_LAPS").ok().and_then(|v| v.parse().ok()) {
            cfg.laps = v;
        }
        opts.realistic = Some(cfg);
    }
    if opts.scenario && std::env::var_os("FAKE_LAP_SECONDS").is_none() {
        opts.lap_seconds = 10.0;
    }
    println!("fakegame : LMU_Data créée. Ctrl+C pour arrêter.");
    if let Err(e) = fakegame::run(opts, &|| false) {
        eprintln!("fakegame : {e}");
    }
}
