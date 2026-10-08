//! Premier jalon : vérifier que la mémoire partagée « LMU_Data » est ouverte par le jeu.
//! Les structures complètes (télémétrie, scoring) viennent ensuite, en `#[repr(C, packed(4))]`.

#[cfg(windows)]
fn main() {
    use std::{thread, time::Duration};
    use windows::core::w;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Memory::{
        MapViewOfFile, OpenFileMappingW, UnmapViewOfFile, FILE_MAP_READ,
    };

    println!("PaceWall collector : en attente de Le Mans Ultimate (Ctrl+C pour quitter)");
    loop {
        // Le jeu crée la zone ; tant qu'elle n'existe pas on dort longtemps (coût quasi nul).
        let handle = unsafe { OpenFileMappingW(FILE_MAP_READ.0, false, w!("LMU_Data")) };
        match handle {
            Ok(h) => {
                let view = unsafe { MapViewOfFile(h, FILE_MAP_READ, 0, 0, 0) };
                if view.Value.is_null() {
                    println!("Zone ouverte mais illisible");
                } else {
                    println!("LMU_Data ouverte : le jeu partage ses données");
                    unsafe {
                        let _ = UnmapViewOfFile(view);
                    }
                }
                unsafe {
                    let _ = CloseHandle(h);
                }
                thread::sleep(Duration::from_secs(2));
            }
            Err(_) => thread::sleep(Duration::from_secs(3)),
        }
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("PaceWall ne fonctionne que sous Windows.");
}
