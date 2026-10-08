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
mod overlay;
mod probe;
mod soft;
mod speech;
mod ui;
mod theme;
mod widgets;
mod win;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let has = |f: &str| args.iter().any(|a| a == f);
    if has("--simulate") {
        // Faux jeu intégré : le verrou nommé dit au reste de PaceWall qu'une simulation tourne (voir Réglages).
        if win::single_instance("PaceWallSimulator") {
            let stop = win::simulator_stop_signal();
            let opts = fakegame::Options { lap_seconds: 10.0, frozen_events: true, yellow: false, scenario: true };
            let _ = fakegame::run(opts, &stop);
        }
    } else if has("--say") {
        // `pacewall.exe --say "texte"` : essai de la voix des alertes.
        let text = args.iter().skip_while(|a| *a != "--say").nth(1).cloned().unwrap_or_else(|| "Alertes vocales PaceWall".into());
        speech::Speaker::global().say(&text, pwcore::settings::int_of("voice_volume", 80) as u32);
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
