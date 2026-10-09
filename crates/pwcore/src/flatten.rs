//! Mise à plat des données LMU en paires clé / valeur (`player.fuel`, `session.rain`, `grid.03.driver`…).
//! Les widgets et l'explorateur s'abonnent à des préfixes de clés ; les unités sont déjà converties
//! (km/h, °C, %, secondes) pour que les overlays n'aient aucun calcul à refaire.

use lmu::reader::{c_str, Reader};
use lmu::*;
use serde_json::{json, Value};

pub struct Field {
    pub key: String,
    pub unit: &'static str,
    pub label: &'static str,
    pub value: Value,
}

pub struct Out(pub Vec<Field>);

impl Out {
    pub fn num(&mut self, key: impl Into<String>, unit: &'static str, label: &'static str, v: f64) {
        // 3 décimales : évite d'envoyer du bruit de virgule flottante.
        let v = if v.is_finite() { (v * 1000.0).round() / 1000.0 } else { 0.0 };
        self.0.push(Field { key: key.into(), unit, label, value: json!(v) });
    }
    fn int(&mut self, key: impl Into<String>, label: &'static str, v: i64) {
        self.0.push(Field { key: key.into(), unit: "", label, value: json!(v) });
    }
    fn flag(&mut self, key: impl Into<String>, label: &'static str, v: bool) {
        self.0.push(Field { key: key.into(), unit: "", label, value: json!(v) });
    }
    pub fn text(&mut self, key: impl Into<String>, label: &'static str, v: String) {
        self.0.push(Field { key: key.into(), unit: "", label, value: json!(v) });
    }
}

const WHEELS: [&str; 4] = ["fl", "fr", "rl", "rr"];
const KELVIN: f64 = 273.15;

/// Lit le jeu et renvoie les champs. Le classement complet (`grid.*`) n'est lu que si quelqu'un l'a demandé.
pub fn read_all(r: &Reader, with_grid: bool) -> Vec<Field> {
    let mut o = Out(Vec::with_capacity(256));

    let info = r.scoring_info();
    session(&mut o, r, &info);

    if let Some((si, ti)) = r.player_indexes() {
        if let (Some(s), Some(t)) = (r.vehicle_scoring(si), r.telemetry(ti)) {
            player(&mut o, &s, &t);
        }
    }

    let n = if with_grid { (info.mNumVehicles.max(0) as usize).min(MAX_MAPPED_VEHICLES) } else { 0 };
    for i in 0..n {
        if let Some(s) = r.vehicle_scoring(i) {
            grid(&mut o, i, &s);
        }
    }
    o.0
}

fn session(o: &mut Out, r: &Reader, i: &ScoringInfo) {
    o.int("game.version", "Version du jeu", r.game_version() as i64);
    o.text("session.track", "Circuit", c_str(&i.mTrackName));
    o.int("session.type", "Type de session", i.mSession as i64);
    o.int("session.phase", "Phase", i.mGamePhase as i64);
    o.int("session.yellow", "Jaune (FCY)", i.mYellowFlagState as i64);
    o.flag("session.realtime", "En piste", i.mInRealtime != 0);
    o.num("session.time", "s", "Temps de session", i.mCurrentET);
    o.num("session.remaining", "s", "Temps restant", i.mSessionTimeRemaining as f64);
    o.int("session.max_laps", "Tours max", i.mMaxLaps as i64);
    o.num("session.track_length", "m", "Longueur du tour", i.mLapDist);
    o.int("session.vehicles", "Voitures", i.mNumVehicles as i64);
    o.num("session.time_of_day", "s", "Heure du jeu", i.mTimeOfDay as f64);
    o.num("weather.ambient", "°C", "Air", i.mAmbientTemp);
    o.num("weather.track", "°C", "Piste", i.mTrackTemp);
    o.num("weather.rain", "%", "Pluie", i.mRaining * 100.0);
    o.num("weather.cloud_dark", "%", "Obscurité des nuages", i.mDarkCloud * 100.0);
    o.num("weather.wetness", "%", "Humidité de la piste", i.mAvgPathWetness * 100.0);
    o.int("weather.clouds", "Couverture nuageuse", i.mCloudCoverage as i64);
    o.int("weather.grip_level", "Gomme sur la piste", i.mTrackGripLevel as i64);
    let wind = i.mWind;
    o.num("weather.wind", "m/s", "Vent", (wind.x * wind.x + wind.y * wind.y + wind.z * wind.z).sqrt());
    // Jaune local par secteur. Le jeu ne dit pas dans quel ordre il range ses trois secteurs : par défaut le premier
    // élément est le secteur 1 ; le réglage « yellow_sector_order » = "game" applique l'ordre de `mSector` (0 = secteur 3,
    // 1 = secteur 1, 2 = secteur 2) si l'essai en jeu montre que c'est lui. Lu une seule fois au lancement.
    static GAME_ORDER: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    let game_order = *GAME_ORDER.get_or_init(|| crate::settings::str_of("yellow_sector_order", "natural") == "game");
    for (n, f) in i.mSectorFlag.iter().enumerate() {
        let sector = if game_order { [3, 1, 2][n.min(2)] } else { n + 1 };
        o.flag(format!("session.sector_yellow.{sector}"), "Jaune de secteur", *f != 0);
    }
    o.text("player.name", "Pilote", c_str(&i.mPlayerName));
}

fn player(o: &mut Out, s: &VehicleScoring, t: &VehicleTelemetry) {
    let v = t.mLocalVel;
    let speed = (v.x * v.x + v.y * v.y + v.z * v.z).sqrt() * 3.6;
    o.text("player.car", "Voiture", c_str(&t.mVehicleName));
    o.text("player.model", "Modèle", c_str(&t.mVehicleModel));
    o.int("player.class_id", "Classe (id)", t.mVehicleClass as i64);
    o.text("player.class", "Classe", c_str(&s.mVehicleClass));
    o.num("player.speed", "km/h", "Vitesse", speed);
    o.int("player.gear", "Rapport", t.mGear as i64);
    o.int("player.gear_max", "Rapports max", t.mMaxGears as i64);
    o.num("player.rpm", "tr/min", "Régime", t.mEngineRPM);
    o.num("player.rpm_max", "tr/min", "Régime max", t.mEngineMaxRPM);
    o.num("player.water_temp", "°C", "Eau moteur", t.mEngineWaterTemp);
    o.num("player.oil_temp", "°C", "Huile moteur", t.mEngineOilTemp);
    o.num("player.throttle", "%", "Accélérateur", t.mFilteredThrottle * 100.0);
    o.num("player.brake", "%", "Frein", t.mFilteredBrake * 100.0);
    o.num("player.clutch", "%", "Embrayage", t.mFilteredClutch * 100.0);
    o.num("player.steering", "%", "Volant", t.mFilteredSteering * 100.0);

    o.num("fuel.level", "L", "Carburant", t.mFuel);
    o.num("fuel.capacity", "L", "Réservoir", t.mFuelCapacity);
    let cap = t.mFuelCapacity;
    o.num("fuel.percent", "%", "Carburant restant", if cap > 0.0 { t.mFuel / cap * 100.0 } else { 0.0 });
    // ⚠️ Plage à confirmer avec le jeu : on suppose une fraction 0..1.
    o.num("energy.virtual", "%", "Énergie virtuelle", t.mVirtualEnergy as f64 * 100.0);
    o.num("energy.battery", "%", "Batterie", t.mBatteryChargeFraction * 100.0);
    o.num("energy.state_of_charge", "%", "État de charge", t.mStateOfCharge as f64);
    o.num("energy.regen", "kW", "Récupération", t.mRegen as f64);
    o.int("energy.motor_state", "État moteur électrique", t.mElectricBoostMotorState as i64);

    o.int("lap.number", "Tour", t.mLapNumber as i64);
    o.num("lap.delta_best", "s", "Delta meilleur tour", t.mDeltaBest);
    o.flag("lap.invalid", "Tour invalidé", t.mLapInvalidated != 0);
    o.num("lap.start", "s", "Début du tour", t.mLapStartET);
    o.int("lap.sector", "Secteur (brut)", t.mCurrentSector as i64);
    o.num("gap.car_ahead", "s", "Voiture devant", t.mTimeGapCarAhead as f64);
    o.num("gap.car_behind", "s", "Voiture derrière", t.mTimeGapCarBehind as f64);
    o.num("gap.place_ahead", "s", "Place devant", t.mTimeGapPlaceAhead as f64);
    o.num("gap.place_behind", "s", "Place derrière", t.mTimeGapPlaceBehind as f64);

    o.int("aids.tc", "TC", t.mTC as i64);
    o.int("aids.tc_max", "TC max", t.mTCMax as i64);
    o.int("aids.abs", "ABS", t.mABS as i64);
    o.int("aids.abs_max", "ABS max", t.mABSMax as i64);
    o.int("aids.motor_map", "Carto moteur", t.mMotorMap as i64);
    o.int("aids.migration", "Migration de frein", t.mMigration as i64);
    o.num("aids.brake_bias", "%", "Répartition arrière", t.mRearBrakeBias * 100.0);
    o.flag("aids.tc_active", "TC actif", t.mTCActive != 0);
    o.flag("aids.abs_active", "ABS actif", t.mABSActive != 0);
    o.flag("car.speed_limiter", "Limiteur actif", t.mSpeedLimiterActive != 0);
    o.flag("car.headlights", "Phares", t.mHeadlights != 0);
    o.flag("car.overheating", "Surchauffe", t.mOverheating != 0);
    o.int("car.wipers", "Essuie-glaces", t.mWiperState as i64);
    o.int("pit.scheduled_stops", "Arrêts prévus", t.mScheduledStops as i64);

    o.text("tyre.front.compound", "Gomme avant", c_str(&t.mFrontTireCompoundName));
    o.text("tyre.rear.compound", "Gomme arrière", c_str(&t.mRearTireCompoundName));
    let wheels = t.mWheels;
    for (i, w) in wheels.iter().enumerate() {
        let n = WHEELS[i];
        let temps = w.mTemperature;
        o.num(format!("tyre.{n}.temp_left"), "°C", "Surface gauche", temps[0] - KELVIN);
        o.num(format!("tyre.{n}.temp_center"), "°C", "Surface centre", temps[1] - KELVIN);
        o.num(format!("tyre.{n}.temp_right"), "°C", "Surface droite", temps[2] - KELVIN);
        o.num(format!("tyre.{n}.carcass"), "°C", "Carcasse", w.mTireCarcassTemperature - KELVIN);
        let inner = w.mTireInnerLayerTemperature;
        o.num(format!("tyre.{n}.inner"), "°C", "Couche interne (moyenne)", (inner[0] + inner[1] + inner[2]) / 3.0 - KELVIN);
        o.num(format!("tyre.{n}.optimal"), "°C", "Optimale", w.mOptimalTemp as f64);
        o.num(format!("tyre.{n}.pressure"), "kPa", "Pression", w.mPressure);
        // État du pneu comme le donne le jeu : 100 % = neuf, tombe vers 0 % quand il est usé.
        o.num(format!("tyre.{n}.wear"), "%", "État du pneu (100 = neuf)", w.mWear * 100.0);
        o.num(format!("tyre.{n}.grip_slide"), "%", "Glissement", w.mGripFract * 100.0);
        o.flag(format!("tyre.{n}.flat"), "Crevé", w.mFlat != 0);
        o.num(format!("brake.{n}.temp"), "°C", "Frein", w.mBrakeTemp);
    }

    o.int("position.place", "Position", s.mPlace as i64);
    o.int("position.laps", "Tours terminés", s.mTotalLaps as i64);
    o.num("position.lap_dist", "m", "Distance dans le tour", s.mLapDist);
    o.num("position.last_lap", "s", "Dernier tour", s.mLastLapTime);
    o.num("position.best_lap", "s", "Meilleur tour", s.mBestLapTime);
    o.num("position.behind_next", "s", "Retard sur la voiture devant", s.mTimeBehindNext);
    o.num("position.behind_leader", "s", "Retard sur le leader", s.mTimeBehindLeader);
    // Drapeaux du joueur : bleu (mFlag = 6), état d'arrivée (0 en course, 1 arrivé, 2 abandon, 3 disqualifié).
    o.flag("player.blue_flag", "Drapeau bleu", s.mFlag == 6);
    o.int("player.finish_status", "État d'arrivée", s.mFinishStatus as i64);
    o.flag("pit.in_pits", "Aux stands", s.mInPits != 0);
    o.int("pit.state", "État du stand", s.mPitState as i64);
    o.int("pit.stops", "Arrêts effectués", s.mNumPitstops as i64);
    o.int("pit.penalties", "Pénalités", s.mNumPenalties as i64);
}

fn grid(o: &mut Out, i: usize, s: &VehicleScoring) {
    let p = format!("grid.{i:03}");
    o.text(format!("{p}.driver"), "Pilote", c_str(&s.mDriverName));
    o.text(format!("{p}.class"), "Classe", c_str(&s.mVehicleClass));
    o.int(format!("{p}.place"), "Position", s.mPlace as i64);
    o.int(format!("{p}.laps"), "Tours", s.mTotalLaps as i64);
    o.num(format!("{p}.last_lap"), "s", "Dernier tour", s.mLastLapTime);
    o.num(format!("{p}.best_lap"), "s", "Meilleur tour", s.mBestLapTime);
    o.num(format!("{p}.behind_next"), "s", "Retard devant", s.mTimeBehindNext);
    o.num(format!("{p}.behind_leader"), "s", "Retard leader", s.mTimeBehindLeader);
    o.flag(format!("{p}.in_pits"), "Aux stands", s.mInPits != 0);
    o.flag(format!("{p}.is_player"), "Joueur", s.mIsPlayer != 0);
    // Position dans le monde (mètres) pour la carte du circuit.
    let pos = s.mPos;
    o.num(format!("{p}.x"), "m", "Position X", pos.x);
    o.num(format!("{p}.z"), "m", "Position Z", pos.z);
    o.num(format!("{p}.lap_dist"), "m", "Distance dans le tour", s.mLapDist);
}
