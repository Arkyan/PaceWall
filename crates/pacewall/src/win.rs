//! Petites fonctions Windows : instance unique, recherche et fermeture de fenêtres, écrans, démarrage automatique.

use std::os::windows::process::CommandExt;
use std::process::Command;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{GetLastError, BOOL, ERROR_ALREADY_EXISTS, HWND, LPARAM, RECT, TRUE, WPARAM};
use windows::Win32::Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowW, GetWindowLongPtrW, IsIconic, PostMessageW, SetForegroundWindow, SetWindowLongPtrW, ShowWindow, GWL_EXSTYLE, SW_RESTORE, WM_CLOSE,
};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const MONITORINFOF_PRIMARY: u32 = 1;

pub const OVERLAY_TITLE: &str = "PaceWall Overlay";
pub const UI_TITLE: &str = "PaceWall";

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// `true` si on est la première instance de ce nom (le verrou vit tant que le processus tourne).
pub fn single_instance(name: &str) -> bool {
    let n = wide(&format!("Local\\{name}"));
    unsafe {
        let handle = CreateMutexW(None, false, PCWSTR(n.as_ptr()));
        // Le handle est volontairement conservé jusqu'à la fin du processus.
        let already = handle.is_err() || GetLastError() == ERROR_ALREADY_EXISTS;
        std::mem::forget(handle);
        !already
    }
}

fn find(title: &str) -> Option<HWND> {
    let t = wide(title);
    unsafe { FindWindowW(PCWSTR::null(), PCWSTR(t.as_ptr())).ok().filter(|h| !h.0.is_null()) }
}

pub fn window_open(title: &str) -> bool {
    find(title).is_some()
}

pub fn close_window(title: &str) {
    if let Some(h) = find(title) {
        unsafe {
            let _ = PostMessageW(h, WM_CLOSE, WPARAM(0), LPARAM(0));
        }
    }
}

/// Amène une fenêtre existante au premier plan. `false` si elle n'existe pas.
pub fn focus_window(title: &str) -> bool {
    let Some(h) = find(title) else { return false };
    unsafe {
        if IsIconic(h).as_bool() {
            let _ = ShowWindow(h, SW_RESTORE);
        }
        let _ = SetForegroundWindow(h);
    }
    true
}

pub fn overlay_running() -> bool {
    window_open(OVERLAY_TITLE)
}

/// Relance ce même programme dans un autre mode (`--overlay`, `--ui`), sans fenêtre de console.
pub fn spawn_self(args: &[&str]) {
    if let Ok(exe) = std::env::current_exe() {
        let _ = Command::new(exe).args(args).creation_flags(CREATE_NO_WINDOW).spawn();
    }
}

pub fn open_overlay() {
    if !overlay_running() {
        spawn_self(&["--overlay"]);
    }
}

pub fn toggle_overlay() {
    if overlay_running() {
        close_window(OVERLAY_TITLE);
    } else {
        spawn_self(&["--overlay"]);
    }
}

/// Ouvre la fenêtre principale, ou la ramène devant si elle existe déjà. `page` : section à afficher.
pub fn open_ui(page: Option<&str>) {
    if focus_window(UI_TITLE) {
        return;
    }
    match page {
        Some(p) => spawn_self(&["--ui", p]),
        None => spawn_self(&["--ui"]),
    }
}

/// À appeler tout au début d'un processus à interface : coordonnées en pixels réels, par écran.
pub fn set_dpi_aware() {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
}

#[derive(Clone, Debug)]
pub struct Monitor {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub primary: bool,
    /// Échelle d'affichage (1.0 = 100 %).
    pub scale: f32,
}

/// Écrans, le principal en premier.
pub fn monitors() -> Vec<Monitor> {
    unsafe extern "system" fn cb(hm: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
        let list = &mut *(data.0 as *mut Vec<Monitor>);
        let mut info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        if GetMonitorInfoW(hm, &mut info).as_bool() {
            let (mut dx, mut _dy) = (96u32, 96u32);
            let _ = GetDpiForMonitor(hm, MDT_EFFECTIVE_DPI, &mut dx, &mut _dy);
            let r = info.rcMonitor;
            list.push(Monitor {
                x: r.left,
                y: r.top,
                w: r.right - r.left,
                h: r.bottom - r.top,
                primary: info.dwFlags & MONITORINFOF_PRIMARY != 0,
                scale: dx as f32 / 96.0,
            });
        }
        TRUE
    }
    let mut list: Vec<Monitor> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(None, None, Some(cb), LPARAM(&mut list as *mut _ as isize));
    }
    list.sort_by_key(|m| !m.primary);
    list
}

/* ---------- Lancement avec Windows ---------- */

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";

pub fn set_autostart(on: bool) {
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};
    let Ok((key, _)) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(RUN_KEY) else { return };
    if on {
        if let Ok(exe) = std::env::current_exe() {
            let _ = key.set_value("PaceWall", &format!("\"{}\" --minimized", exe.display()));
        }
    } else {
        let _ = key.delete_value("PaceWall");
    }
}

/// Rend une fenêtre insensible aux clics et sans focus, sans le style « en couches » que winit ajoute pour
/// l'insensibilité aux clics et qui empêche l'affichage du rendu OpenGL transparent.
/// `layered` : conserve ou non `WS_EX_LAYERED` (à tester selon le cas).
pub fn click_through(title: &str, layered: bool) -> bool {
    const WS_EX_TRANSPARENT: isize = 0x20;
    const WS_EX_LAYERED: isize = 0x80000;
    const WS_EX_NOACTIVATE: isize = 0x0800_0000;
    const WS_EX_TOOLWINDOW: isize = 0x80;
    let Some(h) = find(title) else { return false };
    unsafe {
        let mut ex = GetWindowLongPtrW(h, GWL_EXSTYLE);
        ex |= WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW;
        if layered {
            ex |= WS_EX_LAYERED;
        } else {
            ex &= !WS_EX_LAYERED;
        }
        SetWindowLongPtrW(h, GWL_EXSTYLE, ex);
    }
    true
}
