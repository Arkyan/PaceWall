//! Identité visuelle « télémétrie pro » : fond quasi noir, angles vifs, chiffres en police mono, accent cyan.
//! Polices : Bahnschrift (libellés) et Consolas (chiffres), déjà présentes dans Windows, rien à embarquer.

use eframe::egui::{self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke};
use std::sync::Arc;

pub const BG: Color32 = Color32::from_rgb(7, 9, 13);
pub const PANEL: Color32 = Color32::from_rgb(13, 17, 23);
pub const PANEL2: Color32 = Color32::from_rgb(18, 24, 32);
pub const LINE: Color32 = Color32::from_rgb(31, 40, 53);
pub const TEXT: Color32 = Color32::from_rgb(232, 237, 244);
pub const MUTED: Color32 = Color32::from_rgb(127, 138, 154);
pub const ACCENT: Color32 = Color32::from_rgb(25, 211, 243);
pub const ACCENT_INK: Color32 = Color32::from_rgb(3, 16, 20);

/// Couleur d'accent de la fenêtre PaceWall (réglage « ui_accent »). Les widgets gardent chacun la leur.
static UI_ACCENT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0x19D3F3);

pub fn ui_accent() -> Color32 {
    let v = UI_ACCENT.load(std::sync::atomic::Ordering::Relaxed);
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

/// Applique la couleur d'accent choisie dans les réglages (identifiant de `widgets::ACCENTS`).
pub fn load_ui_accent() {
    let id = pwcore::settings::str_of("ui_accent", "cyan");
    let c = crate::widgets::ACCENTS.iter().find(|a| a.0 == id).map(|a| a.2).unwrap_or(ACCENT);
    UI_ACCENT.store((c.r() as u32) << 16 | (c.g() as u32) << 8 | c.b() as u32, std::sync::atomic::Ordering::Relaxed);
}
pub const OK: Color32 = Color32::from_rgb(56, 224, 123);
pub const WARN: Color32 = Color32::from_rgb(255, 176, 32);
pub const BAD: Color32 = Color32::from_rgb(255, 77, 94);
pub const BLUE: Color32 = Color32::from_rgb(79, 157, 255);

pub fn ui_family() -> FontFamily {
    FontFamily::Name("ui".into())
}

pub fn num_family() -> FontFamily {
    FontFamily::Name("num".into())
}

pub fn ui_font(size: f32) -> FontId {
    FontId::new(size, ui_family())
}

pub fn num_font(size: f32) -> FontId {
    FontId::new(size, num_family())
}

fn load(path: &str, y_offset_factor: f32) -> Option<FontData> {
    // Bahnschrift place ses glyphes trop haut dans la ligne : on les redescend pour qu'ils soient centrés
    // sur les icônes, cases à cocher et boutons.
    std::fs::read(path).ok().map(|d| FontData::from_owned(d).tweak(egui::FontTweak { y_offset_factor, ..Default::default() }))
}

/// Polices, couleurs et formes. À appeler une fois au démarrage de chaque fenêtre.
pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".into());
    for (name, file, fallback) in [("pw-ui", "bahnschrift.ttf", FontFamily::Proportional), ("pw-num", "consola.ttf", FontFamily::Monospace)] {
        let family = if name == "pw-ui" { ui_family() } else { num_family() };
        let mut chain = vec![];
        if let Some(data) = load(&format!(r"{windir}\Fonts\{file}"), if name == "pw-ui" { 0.12 } else { 0.0 }) {
            fonts.font_data.insert(name.into(), Arc::new(data));
            chain.push(name.to_string());
        }
        // Repli sur les polices intégrées si celle de Windows manque.
        chain.extend(fonts.families.get(&fallback).cloned().unwrap_or_default());
        fonts.families.insert(family, chain);
    }
    // Les textes de l'interface utilisent Bahnschrift partout.
    if let Some(ui) = fonts.families.get(&ui_family()).cloned() {
        fonts.families.insert(FontFamily::Proportional, ui);
    }
    if let Some(num) = fonts.families.get(&num_family()).cloned() {
        fonts.families.insert(FontFamily::Monospace, num);
    }
    ctx.set_fonts(fonts);
    apply_visuals(ctx);
}

/// Couleurs et formes de la fenêtre ; à rappeler quand la couleur d'accent change.
pub fn apply_visuals(ctx: &egui::Context) {
    load_ui_accent();
    let accent = ui_accent();
    let mut v = egui::Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = PANEL;
    v.extreme_bg_color = PANEL2;
    v.faint_bg_color = PANEL;
    v.override_text_color = Some(TEXT);
    v.selection.bg_fill = accent.gamma_multiply(0.35);
    v.selection.stroke = Stroke::new(1.0, accent);
    v.hyperlink_color = accent;
    v.window_stroke = Stroke::new(1.0, LINE);
    let r = CornerRadius::same(2);
    for w in [&mut v.widgets.noninteractive, &mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
        w.corner_radius = r;
    }
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
    v.widgets.inactive.bg_fill = PANEL2;
    v.widgets.inactive.weak_bg_fill = PANEL2;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, LINE);
    v.widgets.hovered.bg_fill = PANEL2;
    v.widgets.hovered.weak_bg_fill = PANEL2;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, accent);
    v.widgets.active.bg_stroke = Stroke::new(1.0, accent);
    ctx.set_visuals(v);

    ctx.global_style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(8.0, 8.0);
        s.spacing.button_padding = egui::vec2(12.0, 6.0);
    });
}

/// Teinte, saturation, luminosité (0..360, 0..1, 0..1) vers une couleur.
pub fn hsl(h: f32, s: f32, l: f32) -> Color32 {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = (h.rem_euclid(360.0)) / 60.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r, g, b) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    Color32::from_rgb(((r + m) * 255.0) as u8, ((g + m) * 255.0) as u8, ((b + m) * 255.0) as u8)
}
