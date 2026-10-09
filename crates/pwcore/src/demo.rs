//! Données factices pour l'éditeur d'overlays et les aperçus quand le jeu n'est pas lancé.

use crate::data::Data;
use crate::flatten::Field;
use serde_json::{json, Value};

fn field(key: impl Into<String>, value: Value) -> Field {
    Field { key: key.into(), unit: "", label: "", value }
}

/// `t` : secondes écoulées (fait bouger vitesse, régime, pédales et delta).
pub fn data(t: f64) -> Data {
    let wave = (t * 1.3).sin();
    let mut f: Vec<Field> = Vec::with_capacity(160);
    let mut put = |k: &str, v: Value| f.push(field(k, v));

    put("game.version", json!(1));
    put("session.track", json!("Circuit de la Sarthe"));
    put("session.type", json!(10));
    // Drapeaux de démonstration : jaune dans le secteur 2, bleu qui clignote.
    put("session.sector_yellow.1", json!(false));
    put("session.sector_yellow.2", json!(true));
    put("session.sector_yellow.3", json!(false));
    put("player.blue_flag", json!(((t * 0.5) as i64) % 2 == 0));
    put("session.phase", json!(5));
    put("session.yellow", json!(0));
    put("session.remaining", json!(5.0 * 3600.0 + 1234.0 - t));
    put("session.max_laps", json!(0));
    put("session.track_length", json!(4000.0));
    put("demo.map", json!(true));
    put("session.vehicles", json!(12));
    put("weather.ambient", json!(21.0));
    put("weather.track", json!(32.0));
    put("weather.rain", json!(0.0));
    put("player.car", json!("Toyota GR010 #7"));
    put("player.speed", json!((200.0 + 60.0 * wave).round()));
    put("player.gear", json!(4 + (1.5 * wave).round() as i64));
    put("player.rpm", json!((6800.0 + 1100.0 * wave).round()));
    put("player.rpm_max", json!(8000.0));
    put("player.throttle", json!((55.0 + 60.0 * wave).clamp(0.0, 100.0).round()));
    put("player.brake", json!((-20.0 - 80.0 * wave).clamp(0.0, 100.0).round()));
    put("player.clutch", json!(0.0));
    put("player.steering", json!((40.0 * (t * 0.7).sin()).round()));
    put("fuel.level", json!(52.4));
    put("fuel.capacity", json!(90.0));
    put("fuel.percent", json!(58.0));
    put("energy.virtual", json!(61.0));
    put("strategy.fuel_per_lap", json!(3.61));
    put("strategy.fuel_laps_left", json!(14.5));
    put("strategy.energy_per_lap", json!(3.5));
    put("strategy.energy_laps_left", json!(17.4));
    put("strategy.limiting", json!("fuel"));
    put("strategy.race_laps_left", json!(48.0));
    put("strategy.fuel_to_add", json!(121.0));
    put("strategy.energy_to_add", json!(108.0));
    put("lap.number", json!(14));
    put("lap.delta_best", json!(((0.35 * (t * 0.4).sin()) * 100.0).round() / 100.0));
    put("gap.car_ahead", json!(2.4));
    put("gap.car_behind", json!(3.8));
    put("position.place", json!(4));
    put("position.laps", json!(13));
    put("position.last_lap", json!(207.412));
    put("position.best_lap", json!(206.988));
    put("tyre.front.compound", json!("Medium"));
    put("tyre.rear.compound", json!("Medium"));
    for (i, n) in ["fl", "fr", "rl", "rr"].iter().enumerate() {
        let i = i as f64;
        put(&format!("tyre.{n}.temp_left"), json!(80.0 + i));
        put(&format!("tyre.{n}.temp_center"), json!(86.0 + i));
        put(&format!("tyre.{n}.temp_right"), json!(92.0 + i));
        put(&format!("tyre.{n}.carcass"), json!(84.0 + i));
        put(&format!("tyre.{n}.inner"), json!(82.0 + i));
        put(&format!("tyre.{n}.optimal"), json!(88.0));
        put(&format!("tyre.{n}.pressure"), json!(165.0 + i));
        put(&format!("tyre.{n}.wear"), json!(88.0 - i * 2.0));
        put(&format!("brake.{n}.temp"), json!(420.0 + i * 20.0));
    }
    let names = ["A. Martin", "B. Dupont", "C. Rossi", "Toi", "E. Keller", "F. Sato", "G. Novak", "H. Silva", "I. Moreau", "J. Weber", "K. Costa", "L. Berg"];
    let classes = ["Hypercar", "Hypercar", "Hypercar", "Hypercar", "LMP2", "Hypercar", "LMGT3", "LMP2", "LMGT3", "LMGT3", "LMP2", "LMGT3"];
    for (i, name) in names.iter().enumerate() {
        let p = format!("grid.{i:03}");
        put(&format!("{p}.driver"), json!(name));
        put(&format!("{p}.class"), json!(classes[i]));
        put(&format!("{p}.place"), json!(i + 1));
        put(&format!("{p}.laps"), json!(14));
        put(&format!("{p}.behind_leader"), json!(i as f64 * 4.3));
        put(&format!("{p}.behind_next"), json!(if i == 0 { 0.0 } else { 3.1 + (i as f64 * 0.7).sin() }));
        put(&format!("{p}.best_lap"), json!(206.9 + i as f64 * 0.37));
        put(&format!("{p}.in_pits"), json!(false));
        put(&format!("{p}.is_player"), json!(*name == "Toi"));
        // Position sur le circuit de démonstration : les voitures avancent le long de la boucle.
        let frac = (t * 0.02 + 0.5 - i as f64 * 0.04).rem_euclid(1.0);
        let pt = crate::trackmap::at(&crate::trackmap::demo(), frac);
        put(&format!("{p}.x"), json!(pt[0]));
        put(&format!("{p}.z"), json!(-pt[1]));
        put(&format!("{p}.lap_dist"), json!(frac * 4000.0));
    }
    Data::new(f)
}
