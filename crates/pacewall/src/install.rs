//! Installation et désinstallation, sans installateur externe : PaceWall s'installe lui-même.
//! - installation : copie de l'exécutable dans `%LOCALAPPDATA%\Programs\PaceWall`, raccourci du menu Démarrer (et du bureau
//!   si demandé), démarrage avec Windows si demandé, entrée « Applications installées » de Windows ;
//! - désinstallation : arrête PaceWall, retire raccourcis, démarrage automatique et entrée de Windows, puis supprime
//!   le dossier d'installation (et, si demandé, les données de `%APPDATA%\PaceWall`).
//!
//! Tout se fait pour l'utilisateur courant : aucun droit administrateur n'est nécessaire.

use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use windows::core::{Interface, PCWSTR};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoTaskMemFree, IPersistFile, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::Shell::{FOLDERID_Desktop, FOLDERID_Programs, IShellLinkW, SHGetKnownFolderPath, ShellLink, KF_FLAG_DEFAULT};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\PaceWall";

/// Ce que l'utilisateur a choisi à l'installation.
#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub desktop_shortcut: bool,
    pub autostart: bool,
}

pub fn install_dir() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    base.join("Programs").join("PaceWall")
}

pub fn installed_exe() -> PathBuf {
    install_dir().join("pacewall.exe")
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(x), Ok(y)) => x == y,
        _ => a.to_string_lossy().eq_ignore_ascii_case(&b.to_string_lossy()),
    }
}

/// Ce programme est-il celui qui est installé ?
pub fn running_installed() -> bool {
    std::env::current_exe().is_ok_and(|exe| same_file(&exe, &installed_exe()))
}

/// PaceWall est-il installé (même si on lance une autre copie) ?
pub fn is_installed() -> bool {
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};
    installed_exe().is_file() && RegKey::predef(HKEY_CURRENT_USER).open_subkey(UNINSTALL_KEY).is_ok()
}

fn known_folder(id: &windows::core::GUID) -> Option<PathBuf> {
    unsafe {
        let p = SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None).ok()?;
        let s = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as *const _));
        s.map(PathBuf::from)
    }
}

fn start_menu_shortcut() -> Option<PathBuf> {
    known_folder(&FOLDERID_Programs).map(|d| d.join("PaceWall.lnk"))
}

fn desktop_shortcut() -> Option<PathBuf> {
    known_folder(&FOLDERID_Desktop).map(|d| d.join("PaceWall.lnk"))
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn create_shortcut(link: &Path, target: &Path) -> Result<(), String> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let shell: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).map_err(|e| e.to_string())?;
        let t = wide(&target.to_string_lossy());
        shell.SetPath(PCWSTR(t.as_ptr())).map_err(|e| e.to_string())?;
        let dir = wide(&target.parent().unwrap_or(Path::new("")).to_string_lossy());
        let _ = shell.SetWorkingDirectory(PCWSTR(dir.as_ptr()));
        let desc = wide("PaceWall : overlays de course pour Le Mans Ultimate");
        let _ = shell.SetDescription(PCWSTR(desc.as_ptr()));
        let _ = shell.SetIconLocation(PCWSTR(t.as_ptr()), 0);
        let file: IPersistFile = shell.cast().map_err(|e| e.to_string())?;
        let l = wide(&link.to_string_lossy());
        file.Save(PCWSTR(l.as_ptr()), true).map_err(|e| e.to_string())
    }
}

/// Arrête les autres processus PaceWall (fond, overlay, fenêtre) pour pouvoir remplacer ou supprimer l'exécutable.
fn stop_others() {
    let me = std::process::id();
    let _ = Command::new("taskkill").args(["/F", "/IM", "pacewall.exe", "/FI", &format!("PID ne {me}")]).creation_flags(CREATE_NO_WINDOW).output();
    std::thread::sleep(std::time::Duration::from_millis(700));
}

/// Installe PaceWall pour l'utilisateur courant et renvoie le chemin de l'exécutable installé.
pub fn install(opts: Options) -> Result<PathBuf, String> {
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};
    let me = std::env::current_exe().map_err(|e| e.to_string())?;
    let (dir, target) = (install_dir(), installed_exe());
    stop_others();
    std::fs::create_dir_all(&dir).map_err(|e| format!("dossier d'installation : {e}"))?;
    if !same_file(&me, &target) {
        std::fs::copy(&me, &target).map_err(|e| format!("copie du programme : {e}"))?;
    }
    if let Some(link) = start_menu_shortcut() {
        create_shortcut(&link, &target)?;
    }
    if let Some(link) = desktop_shortcut() {
        if opts.desktop_shortcut {
            create_shortcut(&link, &target)?;
        } else {
            let _ = std::fs::remove_file(link);
        }
    }
    // Démarrage avec Windows : le réglage est enregistré, et la clé « Run » pointe sur la copie installée.
    let _ = pwcore::settings::set("autostart", serde_json::json!(opts.autostart));
    if let Ok((run, _)) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(r"Software\Microsoft\Windows\CurrentVersion\Run") {
        if opts.autostart {
            let _ = run.set_value("PaceWall", &format!("\"{}\" --minimized", target.display()));
        } else {
            let _ = run.delete_value("PaceWall");
        }
    }
    // Entrée « Applications installées » : nom, version, dossier, commande de désinstallation.
    let (key, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(UNINSTALL_KEY).map_err(|e| e.to_string())?;
    let _ = key.set_value("DisplayName", &"PaceWall");
    let _ = key.set_value("DisplayVersion", &env!("CARGO_PKG_VERSION"));
    let _ = key.set_value("Publisher", &"PaceWall");
    let _ = key.set_value("InstallLocation", &dir.to_string_lossy().to_string());
    let _ = key.set_value("DisplayIcon", &target.to_string_lossy().to_string());
    let _ = key.set_value("UninstallString", &format!("\"{}\" --uninstall", target.display()));
    let _ = key.set_value("NoModify", &1u32);
    let _ = key.set_value("NoRepair", &1u32);
    Ok(target)
}

/// Désinstalle PaceWall. `purge` supprime aussi l'historique, les mises en page et les réglages.
pub fn uninstall(purge: bool) {
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};
    stop_others();
    for link in [start_menu_shortcut(), desktop_shortcut()].into_iter().flatten() {
        let _ = std::fs::remove_file(link);
    }
    if let Ok(run) = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(r"Software\Microsoft\Windows\CurrentVersion\Run", winreg::enums::KEY_SET_VALUE) {
        let _ = run.delete_value("PaceWall");
    }
    let _ = RegKey::predef(HKEY_CURRENT_USER).delete_subkey_all(UNINSTALL_KEY);
    if purge {
        if let Some(data) = pwcore::settings::path().parent() {
            let _ = std::fs::remove_dir_all(data);
        }
    } else {
        let _ = pwcore::settings::set("autostart", serde_json::json!(false));
    }
    // Le programme en cours ne peut pas se supprimer lui-même : une commande différée le fait quand il est fermé.
    let dir = install_dir();
    // `raw_arg` : la ligne est transmise telle quelle à cmd (les guillemets internes ne sont pas échappés à la Rust).
    let line = format!("/C \"ping 127.0.0.1 -n 4 >nul & rmdir /s /q \"{}\"\"", dir.display());
    let _ = Command::new("cmd").raw_arg(line).creation_flags(CREATE_NO_WINDOW | 0x0000_0200).spawn();
}
