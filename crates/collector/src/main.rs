//! Collecteur : lit `LMU_Data` et affiche l'essentiel de la voiture du joueur.
//! Sans jeu, il dort longtemps entre deux essais (coût quasi nul).

#[cfg(windows)]
fn main() {
    use lmu::reader::{c_str, Reader};
    use std::{thread, time::Duration};

    println!("PaceWall collector : en attente de Le Mans Ultimate (Ctrl+C pour quitter)");
    loop {
        let Some(reader) = Reader::open() else {
            thread::sleep(Duration::from_secs(3));
            continue;
        };
        println!("LMU_Data ouverte (version du jeu {})", reader.game_version());
        let mut last_update = u32::MAX;
        let mut idle = 0;
        loop {
            let update = reader.events().SME_UPDATE_TELEMETRY;
            if update == last_update {
                // Le jeu ne publie plus : fermé ou en pause. On referme la zone après ~5 s.
                idle += 1;
                if idle > 50 {
                    println!("Plus de données, retour à l'attente");
                    break;
                }
                thread::sleep(Duration::from_millis(100));
                continue;
            }
            idle = 0;
            last_update = update;

            if let Some(v) = reader.player_telemetry() {
                let info = reader.scoring_info();
                let speed = v.mLocalVel.z.abs() * 3.6;
                // Champs d'une structure `packed` : toujours copiés dans des locales avant affichage.
                let (gear, rpm, fuel) = (v.mGear, v.mEngineRPM, v.mFuel);
                let energy = v.mVirtualEnergy * 100.0;
                let wear = v.mWheels[0].mWear * 100.0;
                let (track, tyre) = (c_str(&info.mTrackName), c_str(&v.mFrontTireCompoundName));
                println!(
                    "{track} | {speed:>3.0} km/h | rapport {gear} | {rpm:>5.0} tr/min | carbu {fuel:>5.1} L | énergie {energy:>5.1} % | usure pneus {wear:.1} % | {tyre}"
                );
            }
            // 4 fois par seconde suffit pour ce test ; les overlays choisiront leur cadence par widget.
            thread::sleep(Duration::from_millis(250));
        }
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("PaceWall ne fonctionne que sous Windows.");
}
