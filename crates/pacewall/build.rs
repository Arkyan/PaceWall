//! Donne au programme son identité Windows (icône, version, nom du produit dans les propriétés du fichier) et note
//! la révision du code dont il est issu.

use std::process::Command;

fn main() {
    // Révision git courte, affichée à côté de la version. Absente si le code n'est pas dans un dépôt.
    let commit = Command::new("git").args(["rev-parse", "--short", "HEAD"]).output().ok().filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
    println!("cargo:rustc-env=PACEWALL_COMMIT={commit}");
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/refs/heads");
    println!("cargo:rerun-if-changed=icons/icon.ico");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("icons/icon.ico");
        res.set("ProductName", "PaceWall");
        res.set("FileDescription", "PaceWall");
        res.set("CompanyName", "PaceWall");
        res.set("LegalCopyright", "PaceWall");
        res.set("OriginalFilename", "pacewall.exe");
        // Sans les outils de ressources de Windows, le programme se compile quand même (sans icône ni version).
        if let Err(e) = res.compile() {
            println!("cargo:warning=ressources Windows non intégrées : {e}");
        }
    }
}
