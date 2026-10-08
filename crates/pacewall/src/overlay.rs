//! Overlay joueur native, sans GPU ni toolkit : une petite fenêtre Windows transparente par widget.
//!
//! - Chaque widget est dessiné par le processeur (tiny-skia) dans une image de sa taille, puis envoyée à une
//!   fenêtre « en couches » (transparence par pixel, insensible aux clics, sans focus, au premier plan).
//!   Le coût est proportionnel à la surface des widgets, pas à celle de l'écran.
//! - Elle lit le jeu directement (aucun serveur) et ne redessine un widget qu'à sa cadence.
//! - Le processus est lancé quand le jeu démarre et fermé quand il s'arrête.
//!
//! Le jeu doit être en fenêtré ou sans bordure pour qu'elle s'affiche par-dessus.

use crate::draw::{Measure, D};
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
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, MsgWaitForMultipleObjects, PeekMessageW, PostQuitMessage, RegisterClassW, SetWindowPos,
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
    let start = Instant::now();
    // Diagnostic : PACEWALL_DEBUG=1 écrit des compteurs dans %TEMP%\pw-overlay.log.
    let debug = std::env::var_os("PACEWALL_DEBUG").is_some();
    let (mut loops, mut polls, mut renders, mut dbg_at) = (0u32, 0u32, 0u32, Instant::now());
    // Durées cumulées (ms) : lecture, mesure, dessin, envoi à la fenêtre.
    let mut t_poll = Duration::ZERO;
    let (mut t_measure, mut t_draw, mut t_present) = (Duration::ZERO, Duration::ZERO, Duration::ZERO);

    'main: loop {
        loops += 1;
        if debug && dbg_at.elapsed() >= Duration::from_secs(5) {
            let s = dbg_at.elapsed().as_secs_f32();
            let ms = |d: Duration| d.as_secs_f32() * 1000.0 / s;
            let line = format!("{:.0}s: boucles/s {:.0}, lectures/s {:.0}, rendus/s {:.0} | ms par seconde : lecture {:.1}, mesure {:.1}, dessin {:.1}, envoi {:.1}
", start.elapsed().as_secs_f32(), loops as f32 / s, polls as f32 / s, renders as f32 / s, ms(t_poll), ms(t_measure), ms(t_draw), ms(t_present));
            (t_poll, t_measure, t_draw, t_present) = (Duration::ZERO, Duration::ZERO, Duration::ZERO, Duration::ZERO);
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
            let mtime = layout::modified();
            if mtime != layout_mtime {
                layout_mtime = mtime;
                layout = load_layout();
                panels.clear();
                rendered.clear();
            }
        }

        let with_grid = layout.widgets.iter().any(|w| w.kind == "standings" || w.kind == "trackmap");
        let t = Instant::now();
        let update = live.poll(with_grid);
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

        let mut sleep = Duration::from_millis(500);
        if let (true, Some(mon)) = (game, mon.clone()) {
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
                let period = Duration::from_secs_f32(1.0 / widgets::hz(&spec.kind, spec).max(0.2));
                let due = now.duration_since(panel.last) >= period;
                // Le graphe des inputs avance avec le temps ; les autres widgets n'ont rien à refaire sans nouvelle donnée.
                let time_driven = spec.kind == "inputs" && spec.flag("graph", true);
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
                    let d = D::new(&mut measure, (0.0, 0.0), scale, spec.opacity, bg).styled(widgets::accent(spec), widgets::bar(spec));
                    widgets::draw(&spec.kind, &d, spec, &data, &mut probe, time)
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
                    bytes.fill(0);
                    if let Some(pm) = PixmapMut::from_bytes(bytes, pw as u32, ph as u32) {
                        let mut canvas = SoftCanvas { pm, fonts: &mut fonts };
                        let d = D::new(&mut canvas, (0.0, 0.0), scale, spec.opacity, bg).styled(widgets::accent(spec), widgets::bar(spec));
                        widgets::draw(&spec.kind, &d, spec, &data, &mut panel.state, time);
                    }
                    // tiny-skia écrit en RGBA prémultiplié ; la fenêtre attend du BGRA prémultiplié.
                    let bytes = std::slice::from_raw_parts_mut(panel.bits, (pw * ph * 4) as usize);
                    for px in bytes.chunks_exact_mut(4) {
                        px.swap(0, 2);
                    }
                }
                t_draw += t.elapsed();
                let t = Instant::now();
                let x = mon.x + (spec.x * mon.w as f32).round() as i32;
                let y = mon.y + (spec.y * mon.h as f32).round() as i32;
                panel.present(x, y);
                t_present += t.elapsed();
                sleep = sleep.min(period);
            }
        } else {
            for p in &mut panels {
                p.hide();
            }
            rendered.clear();
        }

        // Certains jeux repassent devant : on réaffirme le premier plan de temps en temps (sans prendre le focus).
        if game && now >= next_topmost {
            next_topmost = now + Duration::from_secs(3);
            for p in &panels {
                unsafe {
                    let _ = SetWindowPos(p.hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
                }
            }
        }

        // Veille jusqu'à la prochaine échéance, réveillée par un message (fermeture).
        unsafe {
            MsgWaitForMultipleObjects(None, false, sleep.as_millis().clamp(1, 1000) as u32, QS_ALLINPUT);
        }
    }
}
