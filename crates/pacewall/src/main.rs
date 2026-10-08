//! PaceWall : overlays de course pour Le Mans Ultimate.
//!
//! Un seul programme, trois modes pour consommer le moins possible :
//! - sans argument : processus de fond (icône, raccourci, surveillance du jeu, historique, serveur OBS) ;
//!   aucune interface chargée, quelques Mo ;
//! - `--ui` : la fenêtre principale, lancée à la demande et qui disparaît quand on la ferme ;
//! - `--overlay` : l'overlay native par-dessus le jeu, lancée avec le jeu ;
//! - `--probe` : relevé de 20 s de ce que le jeu publie (diagnostic).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod daemon;
mod draw;
mod overlay;
mod probe;
mod soft;
mod ui;
mod theme;
mod widgets;
mod win;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let has = |f: &str| args.iter().any(|a| a == f);
    if has("--probe") {
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
