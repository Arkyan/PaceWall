//! Serveur OBS autonome (sans interface) : `cargo run -p server`. Il ne sait pas dessiner les widgets (c'est
//! l'application qui fournit le dessin) : il ne sert ici que les données brutes sur `/ws`. Dans l'application, il est
//! lancé par le processus de fond quand l'option « OBS » est activée.

use std::sync::{atomic::AtomicBool, Arc};

fn main() {
    let port = server::port();
    println!("Serveur de données : ws://127.0.0.1:{port}/ws");
    server::run(port, Arc::new(AtomicBool::new(false)), None);
}
