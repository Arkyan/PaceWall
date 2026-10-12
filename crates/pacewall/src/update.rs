//! Mises à jour : PaceWall regarde s'il existe une version plus récente, la télécharge à la demande et la lance.
//!
//! Chaque version publiée sur GitHub contient deux fichiers : `PaceWall-Setup-x.y.z.exe` et `latest.json`
//! (`{ "version", "file", "sha256", "notes" }`), fabriqués par `tools/release.ps1`. L'adresse
//! `…/releases/latest/download/latest.json` désigne toujours la dernière version.
//!
//! Rien n'est installé sans un clic de l'utilisateur. Le fichier téléchargé n'est lancé que si son empreinte SHA-256
//! est celle annoncée. Le téléchargement passe par Windows (HTTPS du système) : aucune bibliothèque réseau embarquée.

use pwcore::settings;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use windows::core::PCWSTR;
use windows::Win32::System::Com::Urlmon::URLDownloadToFileW;

/// Dépôt GitHub où sont publiées les versions, sous la forme « compte/depot ». Vide : pas de recherche en ligne.
pub const REPO: &str = "Arkyan/PaceWall";


pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Version et révision du code, pour l'affichage : « 0.9.0 (35ce712) ».
pub fn version_label() -> String {
    match option_env!("PACEWALL_COMMIT") {
        Some(c) if !c.is_empty() => format!("{VERSION} ({c})"),
        _ => VERSION.to_string(),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    pub version: String,
    pub file: String,
    pub sha256: String,
    pub notes: String,
}

impl Release {
    fn from_json(v: &Value) -> Option<Release> {
        let text = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
        Some(Release { version: text("version")?, file: text("file")?, sha256: text("sha256")?.to_lowercase(), notes: text("notes").unwrap_or_default() })
    }
}

/// D'où viennent les versions : le dépôt GitHub, ou `PACEWALL_UPDATE_URL` (adresse ou dossier local, pour les essais).
fn source() -> Option<String> {
    if let Ok(v) = std::env::var("PACEWALL_UPDATE_URL") {
        return Some(v.trim_end_matches(['/', '\\']).to_string());
    }
    (!REPO.is_empty()).then(|| format!("https://github.com/{REPO}/releases/latest/download"))
}

/// La recherche de mises à jour en ligne est-elle possible avec cette copie de PaceWall ?
pub fn configured() -> bool {
    source().is_some()
}

fn numbers(version: &str) -> Vec<u32> {
    version.trim_start_matches('v').split(['.', '-']).map(|p| p.parse().unwrap_or(0)).collect()
}

/// `a` est-elle une version plus récente que `b` ? (« 0.10.0 » est plus récente que « 0.9.3 ».)
pub fn newer(a: &str, b: &str) -> bool {
    numbers(a) > numbers(b)
}

fn state_path() -> PathBuf {
    settings::path().with_file_name("update.json")
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// Récupère un fichier de la source dans `dest`.
fn fetch(name: &str, dest: &Path) -> Result<(), String> {
    let base = source().ok_or("mises à jour en ligne non configurées")?;
    if Path::new(&base).is_dir() {
        return std::fs::copy(Path::new(&base).join(name), dest).map(|_| ()).map_err(|e| e.to_string());
    }
    // Le paramètre ajouté évite que Windows ne resserve une ancienne réponse de son cache.
    let stamp = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let (url, file) = (wide(&format!("{base}/{name}?t={stamp}")), wide(&dest.to_string_lossy()));
    unsafe { URLDownloadToFileW(None, PCWSTR(url.as_ptr()), PCWSTR(file.as_ptr()), 0, None) }.map_err(|e| format!("téléchargement impossible ({})", e.message()))
}

/// Interroge la source. Renvoie la dernière version publiée si elle est plus récente que celle-ci.
pub fn check() -> Result<Option<Release>, String> {
    let tmp = std::env::temp_dir().join("pw-latest.json");
    fetch("latest.json", &tmp)?;
    let text = std::fs::read_to_string(&tmp).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&tmp);
    let value: Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|_| "réponse illisible".to_string())?;
    let release = Release::from_json(&value).ok_or("réponse incomplète")?;
    // Gardé sur le disque : la fenêtre principale et l'icône lisent ce fichier sans refaire de requête.
    let _ = std::fs::write(state_path(), json!({ "version": release.version, "file": release.file, "sha256": release.sha256, "notes": release.notes }).to_string());
    Ok(newer(&release.version, VERSION).then_some(release))
}

/// Mise à jour trouvée lors de la dernière recherche (sans requête).
pub fn available() -> Option<Release> {
    let value: Value = serde_json::from_str(&std::fs::read_to_string(state_path()).ok()?).ok()?;
    Release::from_json(&value).filter(|r| newer(&r.version, VERSION))
}

/// Date de la dernière recherche réussie.
pub fn last_check() -> Option<SystemTime> {
    std::fs::metadata(state_path()).and_then(|m| m.modified()).ok()
}

pub fn sha256_of(path: &Path) -> Result<String, String> {
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher).map_err(|e| e.to_string())?;
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Télécharge l'installeur de `release` et vérifie son empreinte. Renvoie son chemin.
pub fn download(release: &Release) -> Result<PathBuf, String> {
    // Le nom vient d'un fichier distant : on n'en garde que la partie « nom de fichier », et il doit être un .exe.
    let name = Path::new(&release.file).file_name().and_then(|n| n.to_str()).filter(|n| n.to_lowercase().ends_with(".exe")).ok_or("nom de fichier inattendu")?;
    let dest = std::env::temp_dir().join(name);
    fetch(name, &dest)?;
    let found = sha256_of(&dest)?;
    if found != release.sha256 {
        let _ = std::fs::remove_file(&dest);
        return Err("le fichier téléchargé ne correspond pas à la version annoncée (empreinte différente)".into());
    }
    Ok(dest)
}

/// Lance l'installeur téléchargé en mode mise à jour : il ferme PaceWall, remplace le programme et le relance.
pub fn apply(setup: &Path) -> Result<(), String> {
    std::process::Command::new(setup).arg("--update").spawn().map(|_| ()).map_err(|e| e.to_string())
}

/// Fil de fond du processus principal : une recherche peu après le démarrage, puis deux fois par jour.
pub fn watch() {
    std::thread::sleep(Duration::from_secs(5));
    loop {
        if configured() && settings::bool_of("update_check", true) {
            let _ = check();
        }
        std::thread::sleep(Duration::from_secs(12 * 3600));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions_by_numbers() {
        assert!(newer("0.10.0", "0.9.3"));
        assert!(newer("v1.0.0", "0.99.99"));
        assert!(newer("0.9.1", "0.9"));
        assert!(!newer("0.9.0", "0.9.0"));
        assert!(!newer("0.8.9", "0.9.0"));
    }

    #[test]
    fn reads_a_release_and_checks_the_download() {
        // Source locale : un dossier avec latest.json et un faux installeur.
        let dir = std::env::temp_dir().join(format!("pw-update-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("PaceWall-Setup-99.0.0.exe"), b"faux installeur").unwrap();
        let hash = sha256_of(&dir.join("PaceWall-Setup-99.0.0.exe")).unwrap();
        assert_eq!(hash.len(), 64);
        let release = Release::from_json(&json!({ "version": "99.0.0", "file": "PaceWall-Setup-99.0.0.exe", "sha256": hash.to_uppercase(), "notes": "Essai" })).unwrap();
        assert_eq!(release.sha256, hash);
        // Téléchargement depuis la source (ici un dossier) : accepté si l'empreinte est la bonne, refusé et effacé sinon.
        std::env::set_var("PACEWALL_UPDATE_URL", &dir);
        let got = download(&release).expect("téléchargement");
        assert_eq!(std::fs::read(&got).unwrap(), b"faux installeur");
        let _ = std::fs::remove_file(&got);
        // Fichier remplacé sur le serveur après l'annonce : l'empreinte ne correspond plus, il est effacé.
        std::fs::write(dir.join("PaceWall-Setup-99.0.0.exe"), b"autre contenu").unwrap();
        assert!(download(&release).is_err() && !got.exists());
        std::env::remove_var("PACEWALL_UPDATE_URL");
        // Un nom de fichier qui sort du dossier ou qui n'est pas un programme est refusé avant tout téléchargement.
        assert!(download(&Release { file: "..\\..\\notes.txt".into(), ..release.clone() }).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
