//! Processus de fond : icône dans la zone de notification, raccourci global, surveillance du jeu,
//! enregistrement de l'historique et serveur OBS à la demande. Aucune interface graphique n'est chargée ici :
//! la fenêtre principale (`--ui`) et l'overlay (`--overlay`) sont des processus séparés, lancés quand il le faut.

use crate::win;
use crate::hotkeys::{self, Binding};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use pwcore::{live, recorder, settings};
use serde_json::json;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use windows::Win32::UI::WindowsAndMessaging::{DispatchMessageW, MsgWaitForMultipleObjects, PeekMessageW, PostQuitMessage, TranslateMessage, MSG, PM_REMOVE, QS_ALLINPUT, WM_QUIT};

fn tray_icon() -> Option<Icon> {
    let bytes = include_bytes!("../icons/tray.png");
    let mut reader = png::Decoder::new(std::io::Cursor::new(&bytes[..])).read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    buf.truncate(info.buffer_size());
    Icon::from_rgba(buf, info.width, info.height).ok()
}

/// Serveur web d'OBS : un thread qu'on démarre et arrête selon le réglage.
struct Obs {
    stop: Option<Arc<AtomicBool>>,
}

impl Obs {
    fn apply(&mut self, wanted: bool) {
        match (wanted, self.stop.is_some()) {
            (true, false) => {
                let stop = Arc::new(AtomicBool::new(false));
                let (flag, port) = (stop.clone(), server::port());
                std::thread::spawn(move || server::run(port, flag));
                self.stop = Some(stop);
            }
            (false, true) => {
                if let Some(s) = self.stop.take() {
                    s.store(true, Ordering::Relaxed);
                }
            }
            _ => {}
        }
    }
}

pub fn run(minimized: bool) {
    if !win::single_instance("PaceWallDaemon") {
        // Déjà lancé : on ouvre simplement la fenêtre principale.
        if !minimized {
            win::open_ui(None);
        }
        return;
    }

    // L'historique s'enregistre en permanence, overlay ouverte ou non (2 lectures par seconde pendant une session).
    std::thread::spawn(recorder::run);

    // Menu de l'icône.
    let open = MenuItem::with_id("ui", "Ouvrir PaceWall", true, None);
    let overlay = MenuItem::with_id("overlay", "Afficher / masquer l'overlay (Ctrl+Alt+O)", true, None);
    let auto = CheckMenuItem::with_id("auto", "Overlay automatique avec le jeu", true, settings::bool_of("overlay_auto", true), None);
    let obs_item = CheckMenuItem::with_id("obs", "Serveur OBS", true, settings::bool_of("obs_server", false), None);
    let quit = MenuItem::with_id("quit", "Quitter", true, None);
    let menu = Menu::new();
    let _ = menu.append_items(&[&open, &overlay, &auto, &obs_item, &PredefinedMenuItem::separator(), &quit]);

    MenuEvent::set_event_handler(Some(|e: MenuEvent| match e.id.0.as_str() {
        "ui" => win::open_ui(None),
        "overlay" => win::toggle_overlay(),
        "auto" => {
            let _ = settings::set("overlay_auto", json!(!settings::bool_of("overlay_auto", true)));
        }
        "obs" => {
            let _ = settings::set("obs_server", json!(!settings::bool_of("obs_server", false)));
        }
        "quit" => {
            win::close_window(win::OVERLAY_TITLE);
            win::close_window(win::UI_TITLE);
            unsafe { PostQuitMessage(0) };
        }
        _ => {}
    }));
    TrayIconEvent::set_event_handler(Some(|e: TrayIconEvent| {
        if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
            win::open_ui(None);
        }
    }));

    let mut builder = TrayIconBuilder::new().with_menu(Box::new(menu)).with_tooltip("PaceWall").with_menu_on_left_click(false);
    if let Some(icon) = tray_icon() {
        builder = builder.with_icon(icon);
    }
    let _tray: Option<TrayIcon> = builder.build().ok();

    // Raccourci pour afficher / fermer l'overlay : une touche (raccourci global) ou un bouton de volant (lu ci-dessous).
    let manager = GlobalHotKeyManager::new().ok();
    let mut overlay_key = hotkeys::parse(&settings::str_of("hk_overlay", "Ctrl+Alt+O"));
    if let (Some(m), Some(Binding::Key(k))) = (&manager, overlay_key) {
        let _ = m.register(k);
    }
    let mut pads = hotkeys::Pads::default();
    GlobalHotKeyEvent::set_event_handler(Some(|e: GlobalHotKeyEvent| {
        if e.state == HotKeyState::Pressed {
            win::toggle_overlay();
        }
    }));

    let mut obs = Obs { stop: None };
    obs.apply(settings::bool_of("obs_server", false));
    win::set_autostart(settings::bool_of("autostart", false));
    let mut settings_mtime: Option<SystemTime> = settings::modified();
    let mut game_was_running = false;
    let mut next_watch = Instant::now();

    // Boucle de messages (nécessaire à l'icône et au raccourci), réveillée une fois par seconde.
    // Lancement normal : on ouvre la fenêtre principale. Au démarrage de Windows (`--minimized`), on reste dans l'icône.
    if !minimized {
        win::open_ui(None);
    }

    'main: loop {
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

        // Réglages modifiés par la fenêtre principale : on les applique (démarrage auto, serveur OBS, icône).
        let mtime = settings::modified();
        if mtime != settings_mtime {
            settings_mtime = mtime;
            obs.apply(settings::bool_of("obs_server", false));
            win::set_autostart(settings::bool_of("autostart", false));
            auto.set_checked(settings::bool_of("overlay_auto", true));
            obs_item.set_checked(settings::bool_of("obs_server", false));
            let wanted = hotkeys::parse(&settings::str_of("hk_overlay", "Ctrl+Alt+O"));
            if wanted != overlay_key {
                if let Some(m) = &manager {
                    if let Some(Binding::Key(k)) = overlay_key {
                        let _ = m.unregister(k);
                    }
                    if let Some(Binding::Key(k)) = wanted {
                        let _ = m.register(k);
                    }
                }
                overlay_key = wanted;
            }
        }
        // Bouton de volant : lu vingt fois par seconde, seulement s'il sert de raccourci.
        let joy = if let Some(Binding::Joy(pad, button)) = overlay_key {
            if pads.pressed(std::iter::once(pad)).contains(&(pad, button)) {
                win::toggle_overlay();
            }
            true
        } else {
            false
        };

        // Le jeu apparaît ou disparaît : l'overlay s'ouvre ou se ferme avec lui (si l'option est active).
        if Instant::now() >= next_watch {
            next_watch = Instant::now() + Duration::from_secs(3);
            let running = live::game_running();
            if running != game_was_running {
                game_was_running = running;
                if settings::bool_of("overlay_auto", true) {
                    if running {
                        win::open_overlay();
                    } else {
                        win::close_window(win::OVERLAY_TITLE);
                    }
                }
            }
        }

        unsafe {
            MsgWaitForMultipleObjects(None, false, if joy { 50 } else { 1000 }, QS_ALLINPUT);
        }
    }
    obs.apply(false);
}
