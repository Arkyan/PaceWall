//! Serveur local PaceWall.
//!
//! - Un thread lit la mémoire partagée de LMU, seulement quand au moins un client écoute, à la cadence
//!   du client le plus exigeant, et sans lire le classement si personne ne s'y abonne.
//! - Chaque client WebSocket s'abonne à des préfixes de clés et ne reçoit que les valeurs qui ont changé.
//! - La page `/` est l'explorateur de données (`explorer.html`), `/history` l'historique des sessions.
//! - `/overlay/player` et `/overlay/obs` : les overlays ; `/editor` : leur éditeur ; `/api/layouts` : les mises en page.
//! - Un second thread (`recorder`) enregistre les tours dans SQLite, overlay ouverte ou non.

mod db;
mod flatten;
mod recorder;
mod strategy;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::Json;
use axum::response::Html;
use axum::routing::{delete, get};
use axum::Router;
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::watch;

use flatten::Field;

const DEFAULT_PORT: u16 = 8470;
const MAX_HZ: u32 = 60;

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
    db: Mutex<Option<rusqlite::Connection>>,
    layout_rev: watch::Sender<u64>,
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let port = std::env::var("PACEWALL_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(DEFAULT_PORT);
    let (tx, _) = watch::channel(Arc::new(Snapshot { game: false, fields: Vec::new() }));
    let shared = Arc::new(Shared {
        tx,
        demands: Mutex::new(HashMap::new()),
        next_id: AtomicU64::new(1),
        db: Mutex::new(db::open().ok()),
        layout_rev: watch::channel(0).0,
    });

    let producer = shared.clone();
    std::thread::spawn(move || produce(producer));
    #[cfg(windows)]
    std::thread::spawn(recorder::run);

    let app = Router::new()
        .route("/", get(|| async { Html(include_str!("explorer.html")) }))
        .route("/history", get(|| async { Html(include_str!("history.html")) }))
        .route("/overlay/:target", get(|| async { Html(include_str!("../overlay/overlay.html")) }))
        .route("/editor", get(|| async { Html(include_str!("../overlay/editor.html")) }))
        .route("/assets/runtime.js", get(|| async { asset("text/javascript", include_str!("../overlay/runtime.js")) }))
        .route("/assets/widgets.js", get(|| async { asset("text/javascript", include_str!("../overlay/widgets.js")) }))
        .route("/assets/overlay.css", get(|| async { asset("text/css", include_str!("../overlay/overlay.css")) }))
        .route("/api/layouts", get(api_layouts_get).put(api_layouts_put))
        .route("/ws", get(upgrade))
        .route("/api/sessions", get(api_sessions))
        .route("/api/sessions/:id/laps", get(api_laps))
        .route("/api/sessions/:id", delete(api_delete))
        .with_state(shared);

    let addr = format!("127.0.0.1:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await.expect("port déjà utilisé ?");
    println!("PaceWall : http://{addr}
  explorateur  /   historique  /history   éditeur  /editor
  overlays  /overlay/player  /overlay/obs
  base : {}", db::path().display());
    axum::serve(listener, app).await.unwrap();
}

/// Lecture du jeu. Ne tourne que si quelqu'un écoute.
#[cfg(windows)]
fn produce(shared: Arc<Shared>) {
    use lmu::reader::Reader;

    let mut reader: Option<Reader> = None;
    let mut strategy = strategy::Strategy::default();
    let mut last_events = u32::MAX;
    let mut last_change = Instant::now();
    let mut game_live = false;

    loop {
        let (hz, want_grid) = {
            let d = shared.demands.lock().unwrap();
            let hz = d.values().map(|d| d.hz).max().unwrap_or(0);
            let grid = d.values().any(|d| d.prefixes.iter().any(|p| "grid.".starts_with(p.as_str()) || p.starts_with("grid.")));
            (hz.min(MAX_HZ), grid)
        };
        if hz == 0 {
            // Personne n'écoute : on ne touche pas à la mémoire du jeu.
            std::thread::sleep(Duration::from_millis(500));
            continue;
        }

        if reader.is_none() {
            reader = Reader::open();
            last_events = u32::MAX;
            strategy = strategy::Strategy::default();
            if reader.is_none() {
                publish(&shared, false, Vec::new(), &mut game_live);
                std::thread::sleep(Duration::from_secs(2));
                continue;
            }
        }
        let r = reader.as_ref().unwrap();

        let events = r.events().SME_UPDATE_TELEMETRY;
        if events != last_events {
            last_events = events;
            last_change = Instant::now();
            let mut fields = flatten::read_all(r, want_grid);
            strategy.update(&mut fields);
            publish(&shared, true, fields, &mut game_live);
        } else if last_change.elapsed() > Duration::from_secs(3) {
            // Jeu en pause, dans les menus ou fermé : on ralentit, et on referme si la zone a disparu.
            publish(&shared, false, Vec::new(), &mut game_live);
            reader = None;
            std::thread::sleep(Duration::from_secs(1));
            continue;
        }
        std::thread::sleep(Duration::from_millis(1000 / hz as u64));
    }
}

#[cfg(not(windows))]
fn produce(_shared: Arc<Shared>) {}

fn publish(shared: &Shared, game: bool, fields: Vec<Field>, live: &mut bool) {
    if !game && !*live && fields.is_empty() && shared.tx.borrow().fields.is_empty() {
        // Rien n'a changé depuis le dernier « jeu absent » : on évite de réveiller les clients.
        return;
    }
    *live = game;
    shared.tx.send_replace(Arc::new(Snapshot { game, fields }));
}

fn asset(content_type: &'static str, body: &'static str) -> ([(header::HeaderName, String); 2], &'static str) {
    ([(header::CONTENT_TYPE, format!("{content_type}; charset=utf-8")), (header::CACHE_CONTROL, "no-cache".into())], body)
}

fn layouts_path() -> std::path::PathBuf {
    db::path().with_file_name("layouts.json")
}

/// Mises en page des overlays (joueur et OBS), telles que l'éditeur les a enregistrées.
async fn api_layouts_get() -> ([(header::HeaderName, &'static str); 1], String) {
    let text = std::fs::read_to_string(layouts_path()).unwrap_or_else(|_| "{}".into());
    ([(header::CONTENT_TYPE, "application/json")], text)
}

async fn api_layouts_put(State(shared): State<Arc<Shared>>, body: String) -> StatusCode {
    if body.len() > 256 * 1024 || serde_json::from_str::<Value>(&body).is_err() {
        return StatusCode::BAD_REQUEST;
    }
    let path = layouts_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if std::fs::write(path, body).is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR;
    }
    // Prévient les overlays ouvertes : elles rechargent leur mise en page.
    shared.layout_rev.send_modify(|r| *r += 1);
    StatusCode::NO_CONTENT
}

type ApiResult = Result<Json<Value>, StatusCode>;

fn with_db<T>(shared: &Shared, f: impl FnOnce(&rusqlite::Connection) -> rusqlite::Result<T>) -> Result<T, StatusCode> {
    let guard = shared.db.lock().unwrap();
    let conn = guard.as_ref().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    f(conn).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn api_sessions(State(shared): State<Arc<Shared>>) -> ApiResult {
    with_db(&shared, db::list_sessions).map(|v| Json(Value::Array(v)))
}

async fn api_laps(State(shared): State<Arc<Shared>>, Path(id): Path<i64>) -> ApiResult {
    with_db(&shared, |c| db::laps(c, id)).map(|v| Json(Value::Array(v)))
}

async fn api_delete(State(shared): State<Arc<Shared>>, Path(id): Path<i64>) -> StatusCode {
    match with_db(&shared, |c| db::delete_session(c, id)) {
        Ok(_) => StatusCode::NO_CONTENT,
        Err(code) => code,
    }
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

        if dirty && last_send.elapsed() >= period && !demand.prefixes.is_empty() {
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
