//! PaceWall : overlays de course pour Le Mans Ultimate.
//!
//! Un seul programme, trois modes pour consommer le moins possible :
//! - sans argument : processus de fond (icône, raccourci, surveillance du jeu, historique, serveur OBS) ;
//!   aucune interface chargée, quelques Mo ;
//! - `--ui` : la fenêtre principale, lancée à la demande et qui disparaît quand on la ferme ;
//! - `--overlay` : l'overlay native par-dessus le jeu, lancée avec le jeu.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod draw;
mod overlay;
mod soft;
mod theme;
mod widgets;
mod win;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--overlay") {
        overlay::run();
    } else {
        eprintln!("PaceWall : mode non disponible pour l'instant");
    }
}
