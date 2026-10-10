//! Serveur web pour OBS (source navigateur). Démarré seulement si l'option est activée dans PaceWall.
//!
//! - `/overlay/obs` (et `/overlay/player`) : les overlays en HTML, avec les mêmes widgets que l'overlay native.
//! - `/ws` : les données du jeu, par abonnement à des préfixes de clés, en ne renvoyant que ce qui a changé.
//! - `/api/layouts` : les mises en page enregistrées par l'éditeur de PaceWall (lecture seule).
//!
//! Le jeu n'est lu que si au moins un client est abonné, à la cadence du plus exigeant.

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::http::header;
use axum::response::Html;
use axum::routing::get;
use axum::Router;
use pwcore::flatten::Field;
use pwcore::live::{Live, Update};
use pwcore::{layout, settings};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::watch;

const MAX_HZ: u32 = 60;
pub const DEFAULT_PORT: u16 = 8470;

/// Port d'écoute : `PACEWALL_PORT` si défini, sinon le réglage `obs_port`.
pub fn port() -> u16 {
    std::env::var("PACEWALL_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or_else(|| settings::int_of("obs_port", DEFAULT_PORT as i64) as u16)
}

struct Snapshot {
    game: bool,
    fields: Vec<Field>,
}

/// Ce que veut un client : des préfixes de clés et une cadence.
#[derive(Clone, Default)]
struct Demand {
    prefixes: Vec<String>,
    hz: u32,
}

struct Shared {
    tx: watch::Sender<Arc<Snapshot>>,
    demands: Mutex<HashMap<u64, Demand>>,
    next_id: AtomicU64,
    layout_rev: watch::Sender<u64>,
}

/// Démarre le serveur et ne rend la main qu'à son arrêt (bloquant : à lancer dans un thread dédié).
/// Passer `stop` à `true` l'arrête proprement.
pub fn run(port: u16, stop: Arc<AtomicBool>) {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime tokio")
        .block_on(serve(port, stop));
}

async fn serve(port: u16, stop: Arc<AtomicBool>) {
    let (tx, _) = watch::channel(Arc::new(Snapshot { game: false, fields: Vec::new() }));
    let shared = Arc::new(Shared {
        tx,
        demands: Mutex::new(HashMap::new()),
        next_id: AtomicU64::new(1),
        layout_rev: watch::channel(0).0,
    });

    let (producer, producer_stop) = (shared.clone(), stop.clone());
    std::thread::spawn(move || produce(producer, producer_stop));

    let app = Router::new()
        .route("/overlay/:target", get(|| async { Html(include_str!("../overlay/overlay.html")) }))
        .route("/assets/runtime.js", get(|| async { asset("text/javascript", include_str!("../overlay/runtime.js")) }))
        .route("/assets/widgets.js", get(|| async { asset("text/javascript", include_str!("../overlay/widgets.js")) }))
        .route("/assets/overlay.css", get(|| async { asset("text/css", include_str!("../overlay/overlay.css")) }))
        .route("/api/layouts", get(api_layouts_get))
        .route("/api/track/:name", get(api_track))
        .route("/api/prefs", get(api_prefs))
        .route("/ws", get(upgrade))
        .with_state(shared);

    let addr = format!("127.0.0.1:{port}");
    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Serveur OBS : port {port} indisponible ({e})");
            return;
        }
    };
    let shutdown = async move {
        while !stop.load(Ordering::Relaxed) {
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    };
    let _ = axum::serve(listener, app).with_graceful_shutdown(shutdown).await;
}

/// Lecture du jeu pour les clients web. Ne tourne que si quelqu'un écoute.
fn produce(shared: Arc<Shared>, stop: Arc<AtomicBool>) {
    let mut live = Live::new();
    let mut layout_mtime = layout::modified();
    let mut game_on = false;

    while !stop.load(Ordering::Relaxed) {
        // L'éditeur enregistre la mise en page dans un fichier : on prévient les pages ouvertes.
        let mtime = layout::modified();
        if mtime != layout_mtime {
            layout_mtime = mtime;
            shared.layout_rev.send_modify(|r| *r += 1);
        }

        let (hz, want_grid) = {
            let d = shared.demands.lock().unwrap();
            // Un client sans abonnement ne justifie aucune lecture du jeu.
            let hz = d.values().filter(|d| !d.prefixes.is_empty()).map(|d| d.hz).max().unwrap_or(0);
            let grid = d.values().any(|d| d.prefixes.iter().any(|p| "grid.".starts_with(p.as_str()) || p.starts_with("grid.")));
            (hz.min(MAX_HZ), grid)
        };
        if hz == 0 {
            std::thread::sleep(Duration::from_millis(500));
            continue;
        }
        match live.poll(want_grid) {
            Update::Data(data) => {
                game_on = true;
                shared.tx.send_replace(Arc::new(Snapshot { game: true, fields: data.fields }));
            }
            Update::NoGame => {
                if game_on || !shared.tx.borrow().fields.is_empty() {
                    game_on = false;
                    shared.tx.send_replace(Arc::new(Snapshot { game: false, fields: Vec::new() }));
                }
            }
            Update::Unchanged => {}
        }
        std::thread::sleep(Duration::from_millis(1000 / hz as u64));
    }
}

fn asset(content_type: &'static str, body: &'static str) -> ([(header::HeaderName, String); 2], &'static str) {
    ([(header::CONTENT_TYPE, format!("{content_type}; charset=utf-8")), (header::CACHE_CONTROL, "no-cache".into())], body)
}

/// Carte d'un circuit, enregistrée par PaceWall pendant un tour propre (404 tant qu'elle n'existe pas).
async fn api_track(Path(name): Path<String>) -> Result<([(header::HeaderName, &'static str); 1], String), StatusCode> {
    let map = pwcore::trackmap::load(&name).ok_or(StatusCode::NOT_FOUND)?;
    let json = serde_json::to_string(&map).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(([(header::CONTENT_TYPE, "application/json")], json))
}

/// Unités et seuils de couleur choisis dans les réglages (les mêmes que l'overlay du joueur).
async fn api_prefs() -> ([(header::HeaderName, &'static str); 1], String) {
    let all = settings::all();
    let keys = ["unit_speed", "unit_temp", "unit_pressure", "unit_fuel", "thr_fuel_warn", "thr_fuel_crit", "thr_wear_warn", "thr_wear_crit", "thr_temp_window"];
    let prefs: Map<String, Value> = keys.iter().filter_map(|k| all.get(*k).map(|v| (k.to_string(), v.clone()))).collect();
    ([(header::CONTENT_TYPE, "application/json")], Value::Object(prefs).to_string())
}

/// Mises en page des overlays (joueur et OBS), telles que l'éditeur les a enregistrées.
async fn api_layouts_get() -> ([(header::HeaderName, &'static str); 1], String) {
    let text = std::fs::read_to_string(layout::path()).unwrap_or_else(|_| "{}".into());
    ([(header::CONTENT_TYPE, "application/json")], text)
}

async fn upgrade(ws: WebSocketUpgrade, State(shared): State<Arc<Shared>>) -> axum::response::Response {
    ws.on_upgrade(move |socket| client(socket, shared))
}

async fn client(mut socket: WebSocket, shared: Arc<Shared>) {
    let id = shared.next_id.fetch_add(1, Ordering::Relaxed);
    let mut rx = shared.tx.subscribe();
    let mut layout_rx = shared.layout_rev.subscribe();
    let mut demand = Demand::default();
    let mut sent: HashMap<String, Value> = HashMap::new();
    let mut last_game: Option<bool> = None;
    let mut last_send = Instant::now() - Duration::from_secs(1);
    let mut dirty = false;

    loop {
        let period = Duration::from_millis(1000 / demand.hz.clamp(1, MAX_HZ) as u64);
        let deadline = dirty.then(|| tokio::time::Instant::from_std(last_send + period));

        tokio::select! {
            msg = socket.recv() => {
                let Some(Ok(Message::Text(text))) = msg else {
                    match msg { Some(Ok(_)) => continue, _ => break }
                };
                if let Ok(v) = serde_json::from_str::<Value>(&text) {
                    demand.prefixes = v["sub"].as_array().map(|a| a.iter().filter_map(|s| s.as_str().map(String::from)).collect()).unwrap_or_default();
                    demand.hz = v["hz"].as_u64().unwrap_or(10).clamp(1, MAX_HZ as u64) as u32;
                    shared.demands.lock().unwrap().insert(id, demand.clone());
                    sent.clear();
                    last_game = None;
                    dirty = true;
                }
            }
            changed = layout_rx.changed() => {
                if changed.is_err() || socket.send(Message::Text(r#"{"t":"layout"}"#.into())).await.is_err() { break; }
            }
            changed = rx.changed() => {
                if changed.is_err() { break; }
                dirty = true;
            }
            _ = async { match deadline { Some(t) => tokio::time::sleep_until(t).await, None => std::future::pending().await } } => {}
        }

        // Sans abonnement il n'y a rien à envoyer : ne jamais rester « sale », sinon la minuterie
        // ci-dessus est déjà échue et la boucle tourne à vide.
        if demand.prefixes.is_empty() {
            dirty = false;
        }
        if dirty && last_send.elapsed() >= period {
            let snap = rx.borrow().clone();
            if let Some(text) = diff(&snap, &demand, &mut sent, &mut last_game) {
                if socket.send(Message::Text(text)).await.is_err() {
                    break;
                }
            }
            last_send = Instant::now();
            dirty = false;
        }
    }
    shared.demands.lock().unwrap().remove(&id);
}

/// Message de mise à jour : seulement ce qui a changé depuis le dernier envoi à ce client.
fn diff(snap: &Snapshot, demand: &Demand, sent: &mut HashMap<String, Value>, last_game: &mut Option<bool>) -> Option<String> {
    let mut changed = Map::new();
    let mut meta = Map::new();
    let mut seen: Vec<&str> = Vec::with_capacity(snap.fields.len());

    for f in &snap.fields {
        if !demand.prefixes.iter().any(|p| f.key.starts_with(p.as_str())) {
            continue;
        }
        seen.push(&f.key);
        match sent.get(&f.key) {
            Some(old) if *old == f.value => {}
            Some(_) => {
                changed.insert(f.key.clone(), f.value.clone());
                sent.insert(f.key.clone(), f.value.clone());
            }
            None => {
                meta.insert(f.key.clone(), json!([f.unit, f.label]));
                changed.insert(f.key.clone(), f.value.clone());
                sent.insert(f.key.clone(), f.value.clone());
            }
        }
    }

    let removed: Vec<String> = if snap.fields.is_empty() {
        let all = sent.keys().cloned().collect();
        sent.clear();
        all
    } else {
        let gone: Vec<String> = sent.keys().filter(|k| !seen.contains(&k.as_str())).cloned().collect();
        for k in &gone {
            sent.remove(k);
        }
        gone
    };

    let game_changed = *last_game != Some(snap.game);
    *last_game = Some(snap.game);
    if changed.is_empty() && removed.is_empty() && !game_changed {
        return None;
    }
    let mut msg = json!({ "t": "u", "game": snap.game, "d": changed });
    if !meta.is_empty() {
        msg["m"] = Value::Object(meta);
    }
    if !removed.is_empty() {
        msg["x"] = json!(removed);
    }
    Some(msg.to_string())
}
