//! PaceWall : overlays de course pour Le Mans Ultimate.
//!
//! Un seul programme, trois modes pour consommer le moins possible :
//! - sans argument : processus de fond (icône, raccourci, surveillance du jeu, historique, serveur OBS) ;
//!   aucune interface chargée, quelques Mo ;
//! - `--ui` : la fenêtre principale, lancée à la demande et qui disparaît quand on la ferme ;
//! - `--overlay` : l'overlay native par-dessus le jeu, lancée avec le jeu ;
//! - `--probe` : relevé de 20 s de ce que le jeu publie (diagnostic).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod alerts;
mod daemon;
mod draw;
mod install;
mod overlay;
mod probe;
mod soft;
mod speech;
mod ui;
mod voice_clips;
mod theme;
mod widgets;
mod win;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let has = |f: &str| args.iter().any(|a| a == f);
    if has("--install") {
        // Installation sans interface : `pacewall.exe --install [--desktop] [--autostart]`.
        if let Ok(exe) = install::install(install::Options { desktop_shortcut: has("--desktop"), autostart: has("--autostart") }) {
            let _ = std::process::Command::new(exe).spawn();
        }
    } else if has("--uninstall") {
        // Désinstallation : `pacewall.exe --uninstall [--purge]` (--purge supprime aussi les données).
        install::uninstall(has("--purge"));
    } else if has("--simulate") {
        // Faux jeu intégré : le verrou nommé dit au reste de PaceWall qu'une simulation tourne (voir Réglages).
        let log = std::env::temp_dir().join("pw-simulation.log");
        let _ = std::fs::remove_file(&log);
        if !win::single_instance("PaceWallSimulator") {
            let _ = std::fs::write(&log, "Une simulation tourne déjà (ou vient de s'arrêter : attends une seconde).");
        } else {
            let stop = win::simulator_stop_signal();
            // `--simulate` : scénario d'alertes (90 s). `--session race|practice|quali [--speed N] [--laps N]` : session réaliste.
            let value = |flag: &str| args.iter().skip_while(|a| *a != flag).nth(1).cloned();
            let realistic = value("--session").and_then(|s| fakegame::SessionKind::parse(&s)).map(|kind| {
                let mut cfg = fakegame::SessionConfig { kind, ..Default::default() };
                if let Some(v) = value("--speed").and_then(|v| v.parse().ok()) {
                    cfg.speed = v;
                }
                if let Some(v) = value("--laps").and_then(|v| v.parse().ok()) {
                    cfg.laps = v;
                }
                cfg
            });
            let opts = fakegame::Options { lap_seconds: 10.0, frozen_events: true, yellow: false, scenario: realistic.is_none(), realistic };
            if let Err(e) = fakegame::run(opts, &stop) {
                let _ = std::fs::write(&log, e);
            }
        }
    } else if has("--say") {
        // `pacewall.exe --say "texte"` : essai de la voix des alertes.
        let text = args.iter().skip_while(|a| *a != "--say").nth(1).cloned().unwrap_or_else(|| "Alertes vocales PaceWall".into());
        // `--say test` joue le clip de l'ingénieur ; tout autre texte est lu par la voix Windows.
        let engine = speech::Engine::from_setting(&pwcore::settings::str_of("voice_engine", "engineer_m"));
        speech::Speaker::global().say(&text, &text, pwcore::settings::int_of("voice_volume", 80) as u32, engine, None);
        std::thread::sleep(std::time::Duration::from_secs(6));
    } else if has("--probe") {
        probe::run();
    } else if has("--overlay") {
        overlay::run();
    } else if has("--ui") {
        // `--ui overlays` ouvre directement une page (accueil, overlays, data, history, settings).
        let page = args.iter().skip_while(|a| *a != "--ui").nth(1).filter(|a| !a.starts_with("--")).cloned();
        ui::run(page);
    } else {
        daemon::run(has("--minimized"));
    }
}
