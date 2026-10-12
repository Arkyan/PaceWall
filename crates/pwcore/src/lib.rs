#![recursion_limit = "256"]
//! Cœur de PaceWall, partagé par le processus de fond, l'interface, l'overlay native et le serveur OBS.
//!
//! - `live` : lit la mémoire partagée de Le Mans Ultimate et produit des `Data` (clé → valeur, unités converties).
//! - `strategy` : consommation moyenne, tours restants, quantité à ajouter.
//! - `recorder` / `db` : historique des tours (SQLite).
//! - `settings` / `layout` : réglages et mises en page des overlays.

pub mod carinfo;
pub mod data;
pub mod db;
pub mod demo;
pub mod flatten;
pub mod forecast;
pub mod layout;
pub mod pitlane;
#[cfg(windows)]
pub mod live;
#[cfg(windows)]
pub mod pulse;
pub mod recorder;
pub mod replay;
pub mod settings;
pub mod strategy;
pub mod trackmap;
pub mod voice_cfg;

pub use data::Data;
