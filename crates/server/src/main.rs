//! Serveur OBS autonome (sans interface) : `cargo run -p server`. Dans l'application, il est lancé par le
//! processus de fond quand l'option « OBS » est activée.

use std::sync::{atomic::AtomicBool, Arc};

fn main() {
    let port = server::port();
    println!("Serveur OBS : http://127.0.0.1:{port}/overlay/obs");
    server::run(port, Arc::new(AtomicBool::new(false)));
}
