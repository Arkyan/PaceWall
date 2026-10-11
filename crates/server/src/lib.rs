//! Serveur web pour OBS (source navigateur). Démarré seulement si l'option est activée dans PaceWall.
//!
//! - `/overlay/obs` (et `/overlay/player`) : une page qui affiche les widgets de la mise en page.
//! - `/ws` : la page y demande des images (`{"frames": …}`) ; le serveur lui envoie chaque widget déjà dessiné.
//!
//! Les widgets ne sont PAS réécrits en HTML : ils sont dessinés ici par le même code que l'overlay du joueur (fourni
//! par l'application sous la forme d'un `Painter`), puis envoyés en images. OBS montre donc exactement la même chose.
//! Une image n'est renvoyée que si elle a changé, et le jeu n'est lu que si une page est ouverte.
//!
//! `/ws` sert aussi les données brutes par abonnement à des préfixes de clés (`{"sub": […], "hz": n}`), pour des
//! outils externes.

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::Html;
use axum::routing::get;
use axum::Router;
use pwcore::layout::{self, WidgetSpec};
use pwcore::live::{Live, Update};
use pwcore::{settings, Data};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, watch};

const MAX_HZ: u32 = 60;
pub const DEFAULT_PORT: u16 = 8470;

/// Port d'écoute : `PACEWALL_PORT` si défini, sinon le réglage `obs_port`.
pub fn port() -> u16 {
    std::env::var("PACEWALL_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or_else(|| settings::int_of("obs_port", DEFAULT_PORT as i64) as u16)
}

/// Un widget dessiné : `rgba` contient `w × h` pixels RGBA (non prémultipliés). `w == 0` : rien à montrer.
pub struct Frame {
    pub w: u32,
    pub h: u32,
    pub rgba: Vec<u8>,
}

/// Ce qui dessine les widgets : l'application fournit celui de l'overlay du joueur.
pub trait Painter {
    /// Images par seconde voulues pour le widget `index` (peut dépendre de son état : un drapeau qui clignote).
    fn hz(&self, index: usize, spec: &WidgetSpec) -> f32;
    /// Dessine le widget `index` de la mise en page. `unit` : pixels par unité de widget (hauteur de la page / 1080).
    fn paint(&mut self, index: usize, spec: &WidgetSpec, data: &Data, unit: f32, now: f64) -> Frame;
    /// Le widget bouge tout seul (graphe, radar, clignotement) : à redessiner même sans nouvelle donnée.
    fn animated(&self, index: usize, spec: &WidgetSpec) -> bool;
    /// La mise en page a changé : oublier l'état des widgets.
    fn reset(&mut self);
}

/// Fabrique de `Painter` : appelée dans le fil d'exécution de chaque page ouverte.
pub type PainterFactory = fn() -> Box<dyn Painter>;

struct Snapshot {
    game: bool,
    data: Arc<Data>,
}

/// Ce que veut un client : une cadence de lecture du jeu, avec ou sans le classement complet.
#[derive(Clone, Default)]
struct Demand {
    /// Préfixes de clés (données brutes). Vide pour une page d'images.
    prefixes: Vec<String>,
    hz: u32,
    grid: bool,
}

struct Shared {
    tx: watch::Sender<Arc<Snapshot>>,
    demands: Mutex<HashMap<u64, Demand>>,
    next_id: AtomicU64,
    layout_rev: watch::Sender<u64>,
    painter: Option<PainterFactory>,
}

/// Démarre le serveur et ne rend la main qu'à son arrêt (bloquant : à lancer dans un thread dédié).
/// Passer `stop` à `true` l'arrête proprement. Sans `painter`, les pages restent vides.
pub fn run(port: u16, stop: Arc<AtomicBool>, painter: Option<PainterFactory>) {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime tokio")
        .block_on(serve(port, stop, painter));
}

async fn serve(port: u16, stop: Arc<AtomicBool>, painter: Option<PainterFactory>) {
    let (tx, _) = watch::channel(Arc::new(Snapshot { game: false, data: Arc::new(Data::default()) }));
    let shared = Arc::new(Shared {
        tx,
        demands: Mutex::new(HashMap::new()),
        next_id: AtomicU64::new(1),
        layout_rev: watch::channel(0).0,
        painter,
    });

    let (producer, producer_stop) = (shared.clone(), stop.clone());
    std::thread::spawn(move || produce(producer, producer_stop));

    let app = Router::new()
        .route("/overlay/:target", get(|| async { Html(include_str!("../overlay/overlay.html")) }))
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
            let hz = d.values().map(|d| d.hz).max().unwrap_or(0);
            let grid = d.values().any(|d| d.grid || d.prefixes.iter().any(|p| "grid.".starts_with(p.as_str()) || p.starts_with("grid.")));
            (hz.min(MAX_HZ), grid)
        };
        if hz == 0 {
            std::thread::sleep(Duration::from_millis(500));
            continue;
        }
        match live.poll(want_grid) {
            Update::Data(data) => {
                game_on = true;
                shared.tx.send_replace(Arc::new(Snapshot { game: true, data: Arc::new(data) }));
            }
            Update::NoGame => {
                if game_on || !shared.tx.borrow().data.is_empty() {
                    game_on = false;
                    shared.tx.send_replace(Arc::new(Snapshot { game: false, data: Arc::new(Data::default()) }));
                }
            }
            Update::Unchanged => {}
        }
        std::thread::sleep(Duration::from_millis(1000 / hz as u64));
    }
}

async fn upgrade(ws: WebSocketUpgrade, State(shared): State<Arc<Shared>>) -> axum::response::Response {
    ws.on_upgrade(move |socket| client(socket, shared))
}

/// Ce qu'une page d'images demande.
#[derive(Clone)]
struct FramesRequest {
    /// Mise en page : `obs` ou `player`.
    target: String,
    /// Hauteur de la page en pixels réels : les widgets sont dessinés à cette échelle.
    height: f32,
    /// N'afficher que ces widgets (identifiant ou type). Vide : tous.
    only: Vec<String>,
    /// Données de démonstration, sans le jeu.
    demo: bool,
}

async fn client(mut socket: WebSocket, shared: Arc<Shared>) {
    let id = shared.next_id.fetch_add(1, Ordering::Relaxed);
    let mut rx = shared.tx.subscribe();
    let mut demand = Demand::default();
    let mut sent: HashMap<String, Value> = HashMap::new();
    let mut last_game: Option<bool> = None;
    let mut last_send = Instant::now() - Duration::from_secs(1);
    let mut dirty = false;
    // Page d'images : son fil de dessin, et le canal par lequel il nous passe les messages à envoyer.
    let mut frames: Option<(Arc<AtomicBool>, mpsc::Receiver<Message>)> = None;

    loop {
        let period = Duration::from_millis(1000 / demand.hz.clamp(1, MAX_HZ) as u64);
        let deadline = dirty.then(|| tokio::time::Instant::from_std(last_send + period));

        tokio::select! {
            msg = socket.recv() => {
                let Some(Ok(Message::Text(text))) = msg else {
                    match msg { Some(Ok(_)) => continue, _ => break }
                };
                let Ok(v) = serde_json::from_str::<Value>(&text) else { continue };
                if let Some(f) = v.get("frames") {
                    // (Re)lance le dessin pour cette page : à l'ouverture, et quand sa taille change.
                    if let Some((alive, _)) = frames.take() {
                        alive.store(false, Ordering::Relaxed);
                    }
                    if let Some(factory) = shared.painter {
                        let request = FramesRequest {
                            target: if f["target"].as_str() == Some("player") { "player".into() } else { "obs".into() },
                            height: (f["height"].as_f64().unwrap_or(1080.0) as f32).clamp(120.0, 4320.0),
                            only: f["only"].as_array().map(|a| a.iter().filter_map(|s| s.as_str().map(String::from)).collect()).unwrap_or_default(),
                            demo: f["demo"].as_bool().unwrap_or(false),
                        };
                        let alive = Arc::new(AtomicBool::new(true));
                        let (out, inbox) = mpsc::channel(24);
                        let (shared, flag) = (shared.clone(), alive.clone());
                        // Un identifiant à part : ce fil enregistre sa propre demande de lecture du jeu.
                        let render_id = shared.next_id.fetch_add(1, Ordering::Relaxed);
                        std::thread::spawn(move || render(shared, render_id, request, factory, out, flag));
                        frames = Some((alive, inbox));
                    }
                } else {
                    demand.prefixes = v["sub"].as_array().map(|a| a.iter().filter_map(|s| s.as_str().map(String::from)).collect()).unwrap_or_default();
                    demand.hz = if demand.prefixes.is_empty() { 0 } else { v["hz"].as_u64().unwrap_or(10).clamp(1, MAX_HZ as u64) as u32 };
                    shared.demands.lock().unwrap().insert(id, demand.clone());
                    sent.clear();
                    last_game = None;
                    dirty = true;
                }
            }
            frame = async { match frames.as_mut() { Some((_, inbox)) => inbox.recv().await, None => std::future::pending().await } } => {
                match frame {
                    Some(message) => {
                        if socket.send(message).await.is_err() { break; }
                    }
                    None => frames = None,
                }
            }
            changed = rx.changed() => {
                if changed.is_err() { break; }
                dirty = true;
            }
            _ = async { match deadline { Some(t) => tokio::time::sleep_until(t).await, None => std::future::pending().await } } => {}
        }

        // Sans abonnement aux données brutes il n'y a rien à envoyer ici : ne jamais rester « sale », sinon la
        // minuterie ci-dessus est déjà échue et la boucle tourne à vide.
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
    if let Some((alive, _)) = frames {
        alive.store(false, Ordering::Relaxed);
    }
    shared.demands.lock().unwrap().remove(&id);
}

/// Widgets à dessiner pour une page : ceux de sa mise en page, filtrés par `only`.
fn widgets_of(request: &FramesRequest) -> Vec<WidgetSpec> {
    let mut list = layout::load_target(&request.target).widgets;
    if !request.only.is_empty() {
        list.retain(|w| request.only.contains(&w.id) || request.only.contains(&w.kind));
    }
    list
}

/// Empreinte rapide d'une image, pour ne pas renvoyer deux fois la même.
fn fingerprint(f: &Frame) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325 ^ ((f.w as u64) << 32 | f.h as u64);
    let mut chunks = f.rgba.chunks_exact(8);
    for c in &mut chunks {
        h = (h ^ u64::from_le_bytes(c.try_into().unwrap())).wrapping_mul(0x0000_0100_0000_01b3).rotate_left(23);
    }
    for b in chunks.remainder() {
        h = (h ^ *b as u64).wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Message binaire d'une image : type (1), index du widget, position (fractions de la page), opacité, taille, pixels.
fn frame_message(index: usize, spec: &WidgetSpec, f: &Frame) -> Message {
    let mut b = Vec::with_capacity(19 + f.rgba.len());
    b.push(1u8);
    b.extend_from_slice(&(index as u16).to_le_bytes());
    b.extend_from_slice(&spec.x.to_le_bytes());
    b.extend_from_slice(&spec.y.to_le_bytes());
    // L'opacité du widget est déjà dans l'image (elle est dessinée comme dans l'overlay du joueur).
    b.extend_from_slice(&1.0f32.to_le_bytes());
    b.extend_from_slice(&(f.w as u16).to_le_bytes());
    b.extend_from_slice(&(f.h as u16).to_le_bytes());
    b.extend_from_slice(&f.rgba);
    Message::Binary(b)
}

/// Fil de dessin d'une page : redessine chaque widget à sa cadence quand les données changent (ou s'il s'anime), et
/// n'envoie que les images qui ont changé. S'arrête quand la page se ferme ou redemande des images.
fn render(shared: Arc<Shared>, id: u64, request: FramesRequest, factory: PainterFactory, out: mpsc::Sender<Message>, alive: Arc<AtomicBool>) {
    let mut painter = factory();
    let mut rx = shared.tx.subscribe();
    let mut layout_rx = shared.layout_rev.subscribe();
    let unit = request.height / 1080.0;
    // Cadence maximale des images pour OBS (réglage « obs_fps », 30 par défaut) : un flux vidéo n'a pas besoin de plus,
    // et chaque image coûte un dessin et une lecture du jeu.
    let cap = settings::int_of("obs_fps", 30).clamp(5, MAX_HZ as i64) as f32;
    let start = Instant::now();
    let far = Instant::now() - Duration::from_secs(60);

    let mut widgets: Vec<WidgetSpec> = Vec::new();
    // Par widget : dernier dessin, version des données dessinée, empreinte de la dernière image envoyée.
    let mut state: Vec<(Instant, u64, Option<u64>)> = Vec::new();
    let mut reload = true;
    let mut version = 1u64;
    let mut snap = rx.borrow_and_update().clone();
    let mut shown = false;

    while alive.load(Ordering::Relaxed) && !out.is_closed() {
        if layout_rx.has_changed().unwrap_or(false) {
            layout_rx.borrow_and_update();
            reload = true;
        }
        if reload {
            reload = false;
            widgets = widgets_of(&request);
            painter.reset();
            state = vec![(far, 0, None); widgets.len()];
            shown = false;
            // Le jeu doit être lu à la cadence du widget le plus rapide (pas du tout en démonstration).
            let hz = widgets.iter().enumerate().map(|(i, w)| painter.hz(i, w).min(cap).ceil() as u32).max().unwrap_or(1).clamp(1, MAX_HZ);
            let demand = Demand { prefixes: Vec::new(), hz: if request.demo { 0 } else { hz }, grid: true };
            shared.demands.lock().unwrap().insert(id, demand);
            if out.blocking_send(Message::Text(json!({ "t": "reset", "count": widgets.len() }).to_string())).is_err() {
                break;
            }
        }
        if rx.has_changed().unwrap_or(false) {
            snap = rx.borrow_and_update().clone();
            version += 1;
        }
        let now = start.elapsed().as_secs_f64();
        let (visible, data) = if request.demo { (true, Arc::new(pwcore::demo::data(now))) } else { (snap.game, snap.data.clone()) };
        if !visible {
            // Sans jeu, la page reste vide (OBS reste propre).
            if shown {
                shown = false;
                for s in &mut state {
                    s.2 = None;
                }
                let _ = out.blocking_send(Message::Text(r#"{"t":"hide"}"#.into()));
            }
            std::thread::sleep(Duration::from_millis(200));
            continue;
        }
        shown = true;

        let tick = Instant::now();
        let mut wait = Duration::from_millis(200);
        for (i, spec) in widgets.iter().enumerate() {
            let period = Duration::from_secs_f32(1.0 / painter.hz(i, spec).clamp(0.2, cap));
            let since = tick.duration_since(state[i].0);
            let moving = request.demo || painter.animated(i, spec);
            if since < period {
                if moving || state[i].1 != version {
                    wait = wait.min(period - since);
                }
                continue;
            }
            if state[i].1 == version && !moving {
                continue;
            }
            state[i].0 = tick;
            state[i].1 = version;
            let frame = painter.paint(i, spec, &data, unit, now);
            let print = fingerprint(&frame);
            // File pleine (page trop lente) : l'image est abandonnée, elle sera renvoyée au prochain dessin.
            if state[i].2 != Some(print) && out.try_send(frame_message(i, spec, &frame)).is_ok() {
                state[i].2 = Some(print);
            }
            if moving {
                wait = wait.min(period);
            }
        }
        std::thread::sleep(wait.max(Duration::from_millis(2)));
    }
    shared.demands.lock().unwrap().remove(&id);
}

/// Message de mise à jour des données brutes : seulement ce qui a changé depuis le dernier envoi à ce client.
fn diff(snap: &Snapshot, demand: &Demand, sent: &mut HashMap<String, Value>, last_game: &mut Option<bool>) -> Option<String> {
    let mut changed = Map::new();
    let mut meta = Map::new();
    let fields = &snap.data.fields;
    let mut seen: Vec<&str> = Vec::with_capacity(fields.len());

    for f in fields {
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

    let removed: Vec<String> = if fields.is_empty() {
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
