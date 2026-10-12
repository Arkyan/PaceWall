//! Prévisions météo de la session. Elles ne sont pas dans la mémoire partagée du jeu : Le Mans Ultimate les expose par
//! son petit service web local (`/rest/sessions/weather`, le même que lit TinyPedal). Chaque session (essais, qualifs,
//! course) a cinq étapes : début, 25 %, 50 %, 75 % et fin, avec le ciel, la température et le risque de pluie.
//!
//! Rien n'est demandé au jeu tant que personne ne s'y intéresse : un widget ou une alerte appelle `touch()`, et un fil
//! d'exécution interroge alors le service toutes les 30 secondes. Sans réponse, les prévisions restent simplement vides.

use crate::flatten::Field;
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Une étape des prévisions.
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    /// Type de ciel du jeu : 0 dégagé, 1-2 quelques nuages, 3-4 couvert, 5-7 pluie faible, 8-9 pluie, 10 orage.
    pub sky: i64,
    pub temp: f64,
    /// Risque de pluie en pourcentage.
    pub rain: f64,
}

const NODES: [&str; 5] = ["START", "NODE_25", "NODE_50", "NODE_75", "FINISH"];
const SESSIONS: [&str; 3] = ["PRACTICE", "QUALIFY", "RACE"];

struct State {
    touched: Option<Instant>,
    started: bool,
    /// Prévisions des essais, des qualifications et de la course.
    sessions: Option<[Vec<Node>; 3]>,
}

static STATE: Mutex<State> = Mutex::new(State { touched: None, started: false, sessions: None });

/// À appeler par quiconque affiche ou utilise les prévisions : tant qu'on l'appelle, elles sont tenues à jour.
pub fn touch() {
    let mut s = STATE.lock().unwrap_or_else(|e| e.into_inner());
    s.touched = Some(Instant::now());
    if !s.started {
        s.started = true;
        drop(s);
        std::thread::spawn(worker);
    }
}

fn wanted() -> bool {
    STATE.lock().unwrap_or_else(|e| e.into_inner()).touched.is_some_and(|t| t.elapsed() < Duration::from_secs(90))
}

fn worker() {
    let mut last: Option<Instant> = None;
    loop {
        std::thread::sleep(Duration::from_secs(1));
        if !wanted() || last.is_some_and(|t| t.elapsed() < Duration::from_secs(30)) {
            continue;
        }
        last = Some(Instant::now());
        let found = fetch().and_then(|text| parse(&text));
        STATE.lock().unwrap_or_else(|e| e.into_inner()).sessions = found;
    }
}

/// Texte JSON des prévisions : le service web du jeu, ou un fichier d'essai (`PACEWALL_FORECAST_FILE`).
fn fetch() -> Option<String> {
    if let Ok(path) = std::env::var("PACEWALL_FORECAST_FILE") {
        return std::fs::read_to_string(path).ok();
    }
    http_get(port(), "/rest/sessions/weather")
}

/// Port du service web local du jeu (réglage `lmu_api_port`).
pub(crate) fn port() -> u16 {
    crate::settings::int_of("lmu_api_port", 6397).clamp(1, 65535) as u16
}

/// Requête HTTP minimale vers la machine locale (aucune dépendance) : délais courts, la réponse entière ou rien.
pub(crate) fn http_get(port: u16, path: &str) -> Option<String> {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_millis(400)).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(2))).ok()?;
    stream.set_write_timeout(Some(Duration::from_secs(1))).ok()?;
    write!(stream, "GET {path} HTTP/1.1\r\nHost: localhost:{port}\r\nAccept: application/json\r\nConnection: close\r\n\r\n").ok()?;
    let mut raw = Vec::new();
    // Une réponse coupée par le délai reste exploitable si elle est complète.
    let _ = stream.read_to_end(&mut raw);
    let split = raw.windows(4).position(|w| w == b"\r\n\r\n")?;
    let head = String::from_utf8_lossy(&raw[..split]).to_lowercase();
    if !head.starts_with("http/1.") || !head.lines().next()?.contains(" 200") {
        return None;
    }
    let body = &raw[split + 4..];
    let body = if head.contains("transfer-encoding: chunked") { unchunk(body)? } else { body.to_vec() };
    String::from_utf8(body).ok()
}

/// Recolle un corps envoyé par morceaux (« chunked »).
fn unchunk(mut b: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    loop {
        let end = b.windows(2).position(|w| w == b"\r\n")?;
        let size = usize::from_str_radix(std::str::from_utf8(&b[..end]).ok()?.split(';').next()?.trim(), 16).ok()?;
        b = &b[end + 2..];
        if size == 0 {
            return Some(out);
        }
        out.extend_from_slice(b.get(..size)?);
        b = b.get(size + 2..)?;
    }
}

/// Lit la réponse du jeu : `{ "RACE": { "START": { "WNV_SKY": { "currentValue": 3 }, … }, … }, … }`.
fn parse(text: &str) -> Option<[Vec<Node>; 3]> {
    let json: Value = serde_json::from_str(text).ok()?;
    let session = |name: &str| -> Vec<Node> {
        NODES
            .iter()
            .filter_map(|node| {
                let n = json.get(name)?.get(node)?;
                let value = |key: &str| n.get(key)?.get("currentValue")?.as_f64();
                Some(Node { sky: value("WNV_SKY")? as i64, temp: value("WNV_TEMPERATURE")?, rain: value("WNV_RAIN_CHANCE")?.clamp(0.0, 100.0) })
            })
            .collect()
    };
    let all = [session(SESSIONS[0]), session(SESSIONS[1]), session(SESSIONS[2])];
    all.iter().any(|s| !s.is_empty()).then_some(all)
}

/// Les prévisions de la session en cours, en champs `forecast.*` à ajouter aux données lues dans le jeu.
/// Vide si personne ne les a demandées ou si le jeu ne les a pas données.
pub fn fields(current: &[Field]) -> Vec<Field> {
    let sessions = {
        let s = STATE.lock().unwrap_or_else(|e| e.into_inner());
        if !s.touched.is_some_and(|t| t.elapsed() < Duration::from_secs(90)) {
            return Vec::new();
        }
        match &s.sessions {
            Some(all) => all.clone(),
            None => return Vec::new(),
        }
    };
    let num = |key: &str| current.iter().find(|f| f.key == key).and_then(|f| f.value.as_f64());
    let nodes = match num("session.type").unwrap_or(-1.0) as i64 {
        5..=8 => &sessions[1],
        10..=13 => &sessions[2],
        _ => &sessions[0],
    };
    build(nodes, num("session.time"), num("session.remaining"))
}

/// Champs d'une liste d'étapes. `elapsed` et `remaining` (secondes) situent l'étape en cours et le délai des suivantes.
pub fn build(nodes: &[Node], elapsed: Option<f64>, remaining: Option<f64>) -> Vec<Field> {
    let mut out = Vec::new();
    let mut put = |key: String, unit: &'static str, label: &'static str, value: Value| out.push(Field { key, unit, label, value });
    put("forecast.count".into(), "", "Étapes de prévision", json!(nodes.len()));
    // Durée de la session, si elle est limitée en temps : chaque étape couvre un cinquième de cette durée.
    let total = match (elapsed, remaining) {
        (Some(e), Some(r)) if e >= 0.0 && r > 0.0 && r < 200.0 * 3600.0 => Some(e + r),
        _ => None,
    };
    for (i, n) in nodes.iter().enumerate() {
        let p = format!("forecast.{i}");
        put(format!("{p}.sky"), "", "Ciel", json!(n.sky));
        put(format!("{p}.temp"), "°C", "Température", json!(n.temp));
        put(format!("{p}.rain"), "%", "Risque de pluie", json!(n.rain));
        if let (Some(total), Some(e)) = (total, elapsed) {
            let (start, end) = (total * i as f64 * 0.2, total * (i + 1) as f64 * 0.2);
            // Minutes avant le début de l'étape (négatif : déjà commencée).
            put(format!("{p}.minutes"), "min", "Dans", json!(((start - e) / 60.0 * 10.0).round() / 10.0));
            put(format!("{p}.current"), "", "Étape en cours", json!(e >= start && e < end));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_game_answer_and_places_the_steps() {
        let node = |sky: i64, rain: f64| json!({ "WNV_SKY": { "currentValue": sky }, "WNV_TEMPERATURE": { "currentValue": 21.0 }, "WNV_RAIN_CHANCE": { "currentValue": rain } });
        let text = json!({ "RACE": { "START": node(0, 0.0), "NODE_25": node(3, 20.0), "NODE_50": node(6, 70.0), "NODE_75": node(8, 90.0), "FINISH": node(2, 10.0) } }).to_string();
        let all = parse(&text).expect("lecture");
        assert!(all[0].is_empty() && all[2].len() == 5);
        assert_eq!(all[2][2], Node { sky: 6, temp: 21.0, rain: 70.0 });
        // Course d'une heure, 15 minutes écoulées : on est dans la deuxième étape (12 à 24 min), la pluie arrive dans 9 min.
        let fields = build(&all[2], Some(900.0), Some(2700.0));
        let get = |key: &str| fields.iter().find(|f| f.key == key).map(|f| f.value.clone());
        assert_eq!(get("forecast.count"), Some(json!(5)));
        assert_eq!(get("forecast.1.current"), Some(json!(true)));
        assert_eq!(get("forecast.0.current"), Some(json!(false)));
        assert_eq!(get("forecast.2.minutes"), Some(json!(9.0)));
        assert_eq!(get("forecast.2.rain"), Some(json!(70.0)));
        // Course au nombre de tours (pas de durée) : pas de délai, mais les étapes sont là.
        let fields = build(&all[2], Some(900.0), None);
        assert!(fields.iter().all(|f| !f.key.ends_with(".minutes")) && fields.len() == 16);
        assert!(parse("pas du json").is_none());
        // Corps envoyé par morceaux.
        assert_eq!(unchunk(b"4\r\nWiki\r\n5\r\npedia\r\n0\r\n\r\n"), Some(b"Wikipedia".to_vec()));
    }
}
