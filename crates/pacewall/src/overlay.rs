//! Overlay joueur native, sans GPU ni toolkit : une petite fenêtre Windows transparente par widget.
//!
//! - Chaque widget est dessiné par le processeur (tiny-skia) dans une image de sa taille, puis envoyée à une
//!   fenêtre « en couches » (transparence par pixel, insensible aux clics, sans focus, au premier plan).
//!   Le coût est proportionnel à la surface des widgets, pas à celle de l'écran.
//! - Elle lit le jeu directement (aucun serveur) et ne redessine un widget qu'à sa cadence.
//! - Le processus est lancé quand le jeu démarre et fermé quand il s'arrête.
//!
//! Le jeu doit être en fenêtré ou sans bordure pour qu'elle s'affiche par-dessus.

use crate::alerts;
use crate::speech;
use crate::draw::{Measure, D};
use crate::hotkeys::{self, Binding};
use crate::soft::{Fonts, SoftCanvas};
use crate::{widgets, win};
use pwcore::layout::{self, Layout};
use pwcore::live::{Live, Update};
use pwcore::{settings, Data};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tiny_skia::PixmapMut;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, SelectObject, AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION,
    DIB_RGB_COLORS, HBITMAP, HDC, HGDIOBJ,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetWindowLongW, GWL_EXSTYLE, MsgWaitForMultipleObjects, PeekMessageW, PostQuitMessage, RegisterClassW, SetWindowPos,
    ShowWindow, TranslateMessage, UpdateLayeredWindow, HWND_TOPMOST, MSG, PM_REMOVE, QS_ALLINPUT, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SW_HIDE, SW_SHOWNA,
    ULW_ALPHA, WM_CLOSE, WM_NCHITTEST, WM_QUIT, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};

const CLASS: &str = "PaceWallOverlayPanel";

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        // Demande de fermeture (menu, interface, jeu fermé) : on arrête tout le processus.
        WM_CLOSE => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        // Les clics traversent (HTTRANSPARENT), en plus du style WS_EX_TRANSPARENT.
        WM_NCHITTEST => LRESULT(-1),
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn register_class() {
    unsafe {
        let class = wide(CLASS);
        let hinst = GetModuleHandleW(PCWSTR::null()).unwrap_or_default();
        let wc = WNDCLASSW { lpfnWndProc: Some(wndproc), hInstance: hinst.into(), lpszClassName: PCWSTR(class.as_ptr()), ..Default::default() };
        RegisterClassW(&wc);
    }
}

/// Une fenêtre transparente et son image (DIB 32 bits, accessible directement en mémoire).
struct Panel {
    hwnd: HWND,
    dc: HDC,
    bmp: HBITMAP,
    bits: *mut u8,
    w: i32,
    h: i32,
    shown: bool,
    state: widgets::WState,
    last: Instant,
}

impl Panel {
    fn new() -> Option<Self> {
        unsafe {
            let class = wide(CLASS);
            let title = wide(win::OVERLAY_TITLE);
            let hinst = GetModuleHandleW(PCWSTR::null()).ok()?;
            let hwnd = CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
                PCWSTR(class.as_ptr()),
                PCWSTR(title.as_ptr()),
                WS_POPUP,
                0,
                0,
                1,
                1,
                None,
                None,
                hinst,
                None,
            )
            .ok()?;
            let dc = CreateCompatibleDC(None);
            Some(Self { hwnd, dc, bmp: HBITMAP::default(), bits: std::ptr::null_mut(), w: 0, h: 0, shown: false, state: widgets::WState::default(), last: Instant::now() - Duration::from_secs(10) })
        }
    }

    /// Alloue l'image si la taille a changé.
    fn ensure(&mut self, w: i32, h: i32) {
        if w == self.w && h == self.h && !self.bits.is_null() {
            return;
        }
        unsafe {
            if !self.bmp.is_invalid() {
                let _ = DeleteObject(HGDIOBJ(self.bmp.0));
            }
            let mut bmi = BITMAPINFO::default();
            bmi.bmiHeader = BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h, // image du haut vers le bas
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            };
            let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
            if let Ok(bmp) = CreateDIBSection(self.dc, &bmi, DIB_RGB_COLORS, &mut bits, None, 0) {
                SelectObject(self.dc, HGDIOBJ(bmp.0));
                self.bmp = bmp;
                self.bits = bits as *mut u8;
                self.w = w;
                self.h = h;
            }
        }
    }

    /// Envoie l'image à la fenêtre, à la position `(x, y)` de l'écran.
    fn present(&mut self, x: i32, y: i32) {
        unsafe {
            let pos = POINT { x, y };
            let size = SIZE { cx: self.w, cy: self.h };
            let src = POINT { x: 0, y: 0 };
            let blend = BLENDFUNCTION { BlendOp: AC_SRC_OVER as u8, BlendFlags: 0, SourceConstantAlpha: 255, AlphaFormat: AC_SRC_ALPHA as u8 };
            let _ = UpdateLayeredWindow(self.hwnd, None, Some(&pos), Some(&size), self.dc, Some(&src), COLORREF(0), Some(&blend), ULW_ALPHA);
            if !self.shown {
                let _ = ShowWindow(self.hwnd, SW_SHOWNA);
                self.shown = true;
            }
        }
    }

    fn hide(&mut self) {
        if self.shown {
            unsafe {
                let _ = ShowWindow(self.hwnd, SW_HIDE);
            }
            self.shown = false;
        }
    }
}

impl Drop for Panel {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.hwnd);
            if !self.bmp.is_invalid() {
                let _ = DeleteObject(HGDIOBJ(self.bmp.0));
            }
            let _ = DeleteDC(self.dc);
        }
    }
}

/// Mise en page du joueur. `PACEWALL_ONLY=inputs,fuel` (ou `none`) limite les widgets : mesures de performance.
fn load_layout() -> Layout {
    let mut l = layout::load_target("player");
    if let Ok(only) = std::env::var("PACEWALL_ONLY") {
        let keep: Vec<&str> = only.split(',').collect();
        l.widgets.retain(|w| keep.contains(&w.kind.as_str()));
    }
    l
}

fn pick_monitor() -> Option<win::Monitor> {
    let monitors = win::monitors();
    let idx = settings::int_of("overlay_monitor", 0).max(0) as usize;
    monitors.get(idx).or(monitors.first()).cloned()
}

/// Journal des apparitions et disparitions de l'overlay (%TEMP%\pw-overlay-events.log) : sert à comprendre un clignotement.
fn log_event(text: &str) {
    use std::io::Write;
    let path = std::env::temp_dir().join("pw-overlay-events.log");
    if std::fs::metadata(&path).map(|m| m.len() > 100_000).unwrap_or(false) {
        let _ = std::fs::remove_file(&path);
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let _ = writeln!(f, "{:02}:{:02}:{:02} {}", secs / 3600 % 24, secs / 60 % 60, secs % 60, text);
    }
}

pub fn run() {
    if !win::single_instance("PaceWallOverlay") {
        return;
    }
    win::set_dpi_aware();
    // Sans cela, Windows réveille le processus toutes les 15,6 ms au mieux : impossible de tenir 20 ou 30 images par seconde.
    unsafe {
        windows::Win32::Media::timeBeginPeriod(1);
    }
    register_class();

    let mut fonts = Fonts::load();
    let mut live = Live::new();
    let mut data = Data::default();
    let mut game = false;
    let mut data_version = 0u64;

    let mut layout: Layout = load_layout();
    let mut layout_mtime = layout::modified();
    let mut panels: Vec<Panel> = Vec::new();
    let mut rendered: HashMap<usize, u64> = HashMap::new();
    let mut next_check = Instant::now() + Duration::from_secs(2);
    // Écran choisi dans les réglages ; tout est calculé en pixels réels. Relu toutes les 2 s seulement.
    let mut mon = pick_monitor();
    let mut next_topmost = Instant::now() + Duration::from_secs(3);
    // Visibilité : jeu au premier plan, en piste et pas en pause (réglages « overlay_fg_only » et « overlay_hide_idle »).
    let mut fg_only = settings::bool_of("overlay_fg_only", true);
    let mut hide_idle = settings::bool_of("overlay_hide_idle", true);
    let mut fg_ok = true;
    let mut was_visible = false;
    // Alertes vocales : règles (alerts) et voix (speech) ; relues avec les autres réglages toutes les 2 s.
    let mut voice = alerts::Config::load();
    // Réglages alerte par alerte (onglet Alertes) : activée, phrase choisie, phrase personnalisée.
    let mut per_alert = pwcore::voice_cfg::load();
    let mut alerts = alerts::Alerts::default();
    let mut next_alert = Instant::now();
    let mut next_fg = Instant::now();
    // Raccourcis de l'overlay : masquer ou réafficher un type de widget, couper ou remettre la voix.
    let manager = global_hotkey::GlobalHotKeyManager::new().ok();
    let mut shortcuts = Shortcuts::default();
    shortcuts.load(manager.as_ref());
    let mut pads = hotkeys::Pads::default();
    let mut hidden: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut muted = false;
    let start = Instant::now();
    // Diagnostic : PACEWALL_DEBUG=1 écrit des compteurs dans %TEMP%\pw-overlay.log.
    let debug = std::env::var_os("PACEWALL_DEBUG").is_some();
    let (mut loops, mut polls, mut renders, mut dbg_at) = (0u32, 0u32, 0u32, Instant::now());
    // Durées cumulées (ms) : lecture, mesure, dessin, envoi à la fenêtre.
    let mut t_poll = Duration::ZERO;
    let (mut t_measure, mut t_draw, mut t_present) = (Duration::ZERO, Duration::ZERO, Duration::ZERO);
    let (mut t_fill, mut t_widget, mut t_swap) = (Duration::ZERO, Duration::ZERO, Duration::ZERO);

    'main: loop {
        loops += 1;
        if debug && dbg_at.elapsed() >= Duration::from_secs(5) {
            let s = dbg_at.elapsed().as_secs_f32();
            let ms = |d: Duration| d.as_secs_f32() * 1000.0 / s;
            let line = format!("{:.0}s: boucles/s {:.0}, lectures/s {:.0}, rendus/s {:.0} | ms par seconde : lecture {:.1}, mesure {:.1}, dessin {:.1}, envoi {:.1}
", start.elapsed().as_secs_f32(), loops as f32 / s, polls as f32 / s, renders as f32 / s, ms(t_poll), ms(t_measure), ms(t_draw), ms(t_present));
            (t_poll, t_measure, t_draw, t_present) = (Duration::ZERO, Duration::ZERO, Duration::ZERO, Duration::ZERO);
            let line = format!("{line} | fill {:.1} widget {:.1} swap {:.1}", ms(t_fill), ms(t_widget), ms(t_swap));
            (t_fill, t_widget, t_swap) = (Duration::ZERO, Duration::ZERO, Duration::ZERO);
            let _ = std::fs::write(std::env::temp_dir().join("pw-overlay.log"), line);
            (loops, polls, renders, dbg_at) = (0, 0, 0, Instant::now());
        }
        // Messages : WM_QUIT quand on demande la fermeture.
        unsafe {
            let mut msg = MSG::default();
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                if msg.message == WM_QUIT {
                    break 'main;
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }

        let now = Instant::now();
        // Mise en page modifiée dans l'éditeur : on reconstruit les fenêtres.
        if now >= next_check {
            next_check = now + Duration::from_secs(2);
            mon = pick_monitor();
            fg_only = settings::bool_of("overlay_fg_only", true);
            hide_idle = settings::bool_of("overlay_hide_idle", true);
            voice = alerts::Config::load();
            per_alert = pwcore::voice_cfg::load();
            shortcuts.load(manager.as_ref());
            let mtime = layout::modified();
            if mtime != layout_mtime {
                layout_mtime = mtime;
                layout = load_layout();
                panels.clear();
                rendered.clear();
            }
        }

        // Raccourcis enfoncés depuis le dernier passage (touches, puis boutons de volant).
        let mut fired: Vec<Action> = Vec::new();
        while let Ok(ev) = global_hotkey::GlobalHotKeyEvent::receiver().try_recv() {
            if ev.state == global_hotkey::HotKeyState::Pressed {
                fired.extend(shortcuts.keys.iter().filter(|(id, _)| *id == ev.id).map(|(_, a)| a.clone()));
            }
        }
        if !shortcuts.buttons.is_empty() {
            for hit in pads.pressed(shortcuts.buttons.iter().map(|(pad, _, _)| *pad)) {
                fired.extend(shortcuts.buttons.iter().filter(|(pad, b, _)| (*pad, *b) == hit).map(|(_, _, a)| a.clone()));
            }
        }
        for action in fired {
            match action {
                Action::Voice => muted = !muted,
                Action::Widget(kind) => {
                    if !hidden.remove(&kind) {
                        hidden.insert(kind);
                    }
                    rendered.clear();
                }
            }
        }

        // Le radar parlé et le bleu « LMP » ont besoin des autres voitures, même sans widget qui les affiche.
        let voice_on = |id: &str| voice.enabled && !muted && per_alert.get(id).is_none_or(|c| c.on);
        let (radar_voice, grid_voice) = (voice_on("radar"), voice_on("radar") || voice_on("blue_lmp"));
        // L'alerte « pluie prévue » a besoin des prévisions du jeu, même sans le widget à l'écran.
        if voice.weather && voice_on("rain_soon") {
            pwcore::forecast::touch();
        }
        let with_grid = grid_voice || layout.widgets.iter().any(|w| matches!(w.kind.as_str(), "standings" | "trackmap" | "relative" | "pit" | "radar" | "flags") || w.kind == "timing");
        // On ne lit le jeu en entier que si un widget doit être redessiné (le jeu publie plus vite que les widgets n'en ont besoin).
        let alert_due = voice.enabled && !muted && now >= next_alert;
        let want = alert_due
            || panels.len() < layout.widgets.len()
            || layout.widgets.iter().zip(panels.iter()).any(|(spec, panel)| {
                now.duration_since(panel.last) >= Duration::from_secs_f32(1.0 / widgets::hz_state(&spec.kind, spec, &panel.state).max(0.2))
            });
        let t = Instant::now();
        let update = live.poll_if(with_grid, want);
        t_poll += t.elapsed();
        match update {
            Update::Data(d) => {
                polls += 1;
                data = d;
                if !game {
                    log_event("jeu détecté : overlay affichée");
                }
                game = true;
                data_version += 1;
                if voice.enabled && !muted {
                    // Le radar parlé doit réagir vite : dix vérifications par seconde au lieu de quatre.
                    next_alert = now + Duration::from_millis(if radar_voice { 100 } else { 250 });
                    for phrase in alerts.update(&data, &voice, now) {
                        let acfg = pwcore::voice_cfg::id_of_key(phrase.key).and_then(|id| per_alert.get(id)).cloned().unwrap_or_default();
                        if !acfg.on {
                            continue;
                        }
                        // Phrase personnalisée : lue par la voix Windows. Sinon le clip de l'ingénieur (phrase choisie ou alternée).
                        let custom = acfg.text.trim().to_string();
                        if custom.is_empty() {
                            log_event(&format!("alerte vocale : {}", phrase.text));
                            speech::Speaker::global().say(phrase.key, &phrase.text, voice.volume, voice.engine, acfg.fixed_variant());
                        } else {
                            log_event(&format!("alerte vocale (phrase personnalisée) : {custom}"));
                            speech::Speaker::global().say("", &custom, voice.volume, speech::Engine::Windows, None);
                        }
                    }
                }
            }
            Update::NoGame => {
                if game {
                    log_event("jeu perdu : overlay masquée");
                }
                game = false;
                data = Data::default();
            }
            Update::Unchanged => {}
        }

        // Premier plan : interrogé 4 fois par seconde seulement.
        if now >= next_fg {
            next_fg = now + Duration::from_millis(250);
            fg_ok = (!fg_only || win::game_in_front()) && !win::overlay_hidden_for_capture();
        }
        // En pause ou au menu, plus rien ne bouge (temps du jeu figé) ou « en piste » est faux : on masque.
        let driving = !hide_idle || (data.flag("session.realtime") && live.idle() < Duration::from_millis(1500));
        let visible = game && fg_ok && driving;
        if visible != was_visible {
            was_visible = visible;
            log_event(&format!(
                "{} (jeu {}, premier plan {}, en piste {}, inactif depuis {} ms)",
                if visible { "overlay affichée" } else { "overlay masquée" },
                game,
                fg_ok,
                data.flag("session.realtime"),
                live.idle().as_millis()
            ));
        }

        let mut sleep = Duration::from_millis(if game { 250 } else { 500 });
        if voice.enabled && !muted && game {
            sleep = sleep.min(next_alert.saturating_duration_since(now).max(Duration::from_millis(20)));
        }
        if let (true, Some(mon)) = (visible, mon.clone()) {
            while panels.len() < layout.widgets.len() {
                match Panel::new() {
                    Some(p) => panels.push(p),
                    None => break,
                }
            }
            let unit = mon.h as f32 / 1080.0;
            let time = start.elapsed().as_secs_f64();
            sleep = Duration::from_millis(200);

            for (i, spec) in layout.widgets.iter().enumerate() {
                let Some(panel) = panels.get_mut(i) else { continue };
                // Type de widget masqué par un raccourci.
                if hidden.contains(&spec.kind) {
                    panel.hide();
                    continue;
                }
                let period = Duration::from_secs_f32(1.0 / widgets::hz_state(&spec.kind, spec, &panel.state).max(0.2));
                let due = now.duration_since(panel.last) >= period;
                // Le graphe des inputs avance avec le temps ; les autres widgets n'ont rien à refaire sans nouvelle donnée.
                let time_driven = (spec.kind == "inputs" && spec.flag("graph", true)) || spec.kind == "radar" || panel.state.animating();
                let fresh = rendered.get(&i).copied() != Some(data_version);
                if !due || !(fresh || time_driven) {
                    sleep = sleep.min(period.saturating_sub(now.duration_since(panel.last)).max(Duration::from_millis(5)));
                    continue;
                }
                panel.last = now;
                renders += 1;
                rendered.insert(i, data_version);

                let scale = spec.scale * unit;
                let bg = widgets::bg(spec);
                // Mesure : la taille du widget varie (lignes de classement, bandeau jaune…).
                let t = Instant::now();
                let mut probe = widgets::WState::default();
                let (bw, bh) = {
                    let mut measure = Measure;
                    let d = D::new(&mut measure, (0.0, 0.0), scale, spec.opacity, bg).styled(widgets::accent(spec), widgets::bar(spec)).titled(widgets::titles(spec));
                    // La carte du circuit garde sa carte chargée entre deux images : la relire à chaque mesure coûterait cher.
                    if spec.kind == "trackmap" {
                        widgets::draw(&spec.kind, &d, spec, &data, &mut panel.state, time)
                    } else {
                        widgets::draw(&spec.kind, &d, spec, &data, &mut probe, time)
                    }
                };
                t_measure += t.elapsed();
                let (pw, ph) = ((bw * scale).ceil() as i32 + 2, (bh * scale).ceil() as i32 + 2);
                if pw <= 2 || ph <= 2 {
                    continue;
                }
                panel.ensure(pw, ph);
                if panel.bits.is_null() {
                    continue;
                }
                let t = Instant::now();
                unsafe {
                    let bytes = std::slice::from_raw_parts_mut(panel.bits, (pw * ph * 4) as usize);
                    let tf = Instant::now();
                    bytes.fill(0);
                    t_fill += tf.elapsed();
                    let tw = Instant::now();
                    if let Some(pm) = PixmapMut::from_bytes(bytes, pw as u32, ph as u32) {
                        let mut canvas = SoftCanvas { pm, fonts: &mut fonts };
                        let d = D::new(&mut canvas, (0.0, 0.0), scale, spec.opacity, bg).styled(widgets::accent(spec), widgets::bar(spec)).titled(widgets::titles(spec));
                        widgets::draw(&spec.kind, &d, spec, &data, &mut panel.state, time);
                    }
                    t_widget += tw.elapsed();
                    let ts = Instant::now();
                    // tiny-skia écrit en RGBA prémultiplié ; la fenêtre attend du BGRA prémultiplié.
                    let bytes = std::slice::from_raw_parts_mut(panel.bits, (pw * ph * 4) as usize);
                    for px in bytes.chunks_exact_mut(4) {
                        px.swap(0, 2);
                    }
                    t_swap += ts.elapsed();
                }
                t_draw += t.elapsed();
                let t = Instant::now();
                let x = mon.x + (spec.x * mon.w as f32).round() as i32;
                let y = mon.y + (spec.y * mon.h as f32).round() as i32;
                panel.present(x, y);
                t_present += t.elapsed();
                // Prochaine image dans une période à partir du début de celle-ci (et non de la fin) : cadence tenue.
                sleep = sleep.min(period.saturating_sub(panel.last.elapsed()).max(Duration::from_millis(1)));
            }
        } else {
            for p in &mut panels {
                p.hide();
            }
            rendered.clear();
        }

        // Certains jeux repassent devant : on réaffirme le premier plan de temps en temps (sans prendre le focus).
        if visible && now >= next_topmost {
            next_topmost = now + Duration::from_secs(3);
            for p in &panels {
                unsafe {
                    // Seulement si la fenêtre a réellement perdu son rang : réaffirmer à chaque fois fait scintiller par-dessus un jeu.
                    let ex = GetWindowLongW(p.hwnd, GWL_EXSTYLE) as u32;
                    if ex & WS_EX_TOPMOST.0 == 0 {
                        let _ = SetWindowPos(p.hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
                    }
                }
            }
        }

        // Un bouton de volant sert de raccourci : on revient le lire au moins vingt fois par seconde.
        if !shortcuts.buttons.is_empty() {
            sleep = sleep.min(Duration::from_millis(50));
        }
        // Veille jusqu'à la prochaine échéance, réveillée par un message (fermeture).
        unsafe {
            MsgWaitForMultipleObjects(None, false, sleep.as_millis().clamp(1, 1000) as u32, QS_ALLINPUT);
        }
    }
    unsafe {
        windows::Win32::Media::timeEndPeriod(1);
    }
}

#[derive(Clone)]
enum Action {
    Voice,
    /// Masquer ou réafficher tous les widgets de ce type.
    Widget(String),
}

/// Raccourcis de l'overlay tels qu'enregistrés dans les réglages.
#[derive(Default)]
struct Shortcuts {
    /// Texte des réglages au dernier chargement (pour ne rien refaire s'ils n'ont pas changé).
    raw: Vec<String>,
    registered: Vec<global_hotkey::hotkey::HotKey>,
    keys: Vec<(u32, Action)>,
    buttons: Vec<(u32, u32, Action)>,
}

impl Shortcuts {
    fn load(&mut self, manager: Option<&global_hotkey::GlobalHotKeyManager>) {
        let all = settings::all();
        let text = |k: &str| all.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
        let mut wanted: Vec<(String, Action)> = vec![(text("hk_voice"), Action::Voice)];
        for (key, _) in hotkeys::WIDGET_ACTIONS {
            wanted.push((text(key), Action::Widget(key.trim_start_matches("hk_").to_string())));
        }
        let raw: Vec<String> = wanted.iter().map(|(t, _)| t.clone()).collect();
        if raw == self.raw {
            return;
        }
        self.raw = raw;
        if let Some(m) = manager {
            for k in self.registered.drain(..) {
                let _ = m.unregister(k);
            }
        }
        self.keys.clear();
        self.buttons.clear();
        for (t, action) in wanted {
            match hotkeys::parse(&t) {
                Some(Binding::Key(k)) => {
                    if let Some(m) = manager {
                        if m.register(k).is_ok() {
                            self.registered.push(k);
                            self.keys.push((k.id(), action));
                        }
                    }
                }
                Some(Binding::Joy(pad, button)) => self.buttons.push((pad, button, action)),
                None => {}
            }
        }
    }
}
