//! Primitives de dessin pour les widgets : coordonnées en unités de 1080 px de hauteur d'écran,
//! mises à l'échelle (`s`) et à l'opacité (`a`) du widget.
//!
//! Le dessin passe par le trait `Canvas`, avec deux implémentations : egui (aperçu de l'éditeur) et un rendu
//! logiciel (overlay : de petites fenêtres Windows transparentes, sans GPU). Les widgets sont écrits une seule fois.

use crate::theme::ACCENT;
use eframe::egui::Color32;
use std::cell::RefCell;

#[derive(Clone, Copy, PartialEq)]
pub enum Font {
    Ui,
    Num,
}

#[derive(Clone, Copy, PartialEq)]
pub enum HAlign {
    Left,
    Center,
    Right,
}

/// Surface de dessin, en pixels. Origine en haut à gauche.
pub trait Canvas {
    fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: Color32);
    /// Polygone convexe ; `stroke` : (épaisseur, couleur) du contour.
    fn polygon(&mut self, pts: &[(f32, f32)], fill: Color32, stroke: Option<(f32, Color32)>);
    fn polyline(&mut self, pts: &[(f32, f32)], width: f32, color: Color32);
    /// Texte d'une ligne dont le haut est à `y` ; renvoie sa largeur en pixels.
    fn text(&mut self, x: f32, y: f32, s: &str, size: f32, font: Font, align: HAlign, color: Color32) -> f32;
    fn circle(&mut self, x: f32, y: f32, r: f32, fill: Color32, stroke: Option<(f32, Color32)>);
    /// `true` pour le canvas de mesure : un widget peut alors renvoyer sa taille sans rien calculer.
    fn measuring(&self) -> bool {
        false
    }
}

/// Canvas qui ne dessine rien : sert à mesurer la taille d'un widget avant de lui allouer une image.
pub struct Measure;

impl Canvas for Measure {
    fn rect(&mut self, _: f32, _: f32, _: f32, _: f32, _: Color32) {}
    fn polygon(&mut self, _: &[(f32, f32)], _: Color32, _: Option<(f32, Color32)>) {}
    fn polyline(&mut self, _: &[(f32, f32)], _: f32, _: Color32) {}
    fn text(&mut self, _: f32, _: f32, s: &str, size: f32, _: Font, _: HAlign, _: Color32) -> f32 {
        s.chars().count() as f32 * size * 0.55
    }
    fn circle(&mut self, _: f32, _: f32, _: f32, _: Color32, _: Option<(f32, Color32)>) {}
    fn measuring(&self) -> bool {
        true
    }
}

pub struct D<'a> {
    c: RefCell<&'a mut dyn Canvas>,
    /// Coin haut-gauche du widget, en pixels.
    o: (f32, f32),
    /// Échelle : taille du widget × (hauteur écran / 1080).
    s: f32,
    /// Opacité globale du widget (0..1).
    a: f32,
    /// Opacité du fond de la carte (0..1).
    bg: f32,
}

impl<'a> D<'a> {
    pub fn new(canvas: &'a mut dyn Canvas, origin: (f32, f32), scale: f32, opacity: f32, bg: f32) -> Self {
        Self { c: RefCell::new(canvas), o: origin, s: scale, a: opacity, bg }
    }

    fn col(&self, c: Color32) -> Color32 {
        if self.a >= 0.999 {
            c
        } else {
            c.gamma_multiply(self.a)
        }
    }

    fn pt(&self, x: f32, y: f32) -> (f32, f32) {
        (self.o.0 + x * self.s, self.o.1 + y * self.s)
    }

    pub fn fill(&self, x: f32, y: f32, w: f32, h: f32, color: Color32) {
        let (px, py) = self.pt(x, y);
        self.c.borrow_mut().rect(px, py, w * self.s, h * self.s, self.col(color));
    }

    pub fn line(&self, pts: &[(f32, f32)], width: f32, color: Color32) {
        if pts.len() >= 2 {
            let v: Vec<(f32, f32)> = pts.iter().map(|&(x, y)| self.pt(x, y)).collect();
            self.c.borrow_mut().polyline(&v, width * self.s, self.col(color));
        }
    }

    /// Texte ; renvoie la largeur occupée en unités de base.
    pub fn text(&self, x: f32, y: f32, s: &str, size: f32, color: Color32, font: Font, align: HAlign) -> f32 {
        let (px, py) = self.pt(x, y);
        let w = self.c.borrow_mut().text(px, py, s, size * self.s, font, align, self.col(color));
        w / self.s
    }

    pub fn circle(&self, x: f32, y: f32, r: f32, fill: Color32, stroke: Option<(f32, Color32)>) {
        let (px, py) = self.pt(x, y);
        self.c.borrow_mut().circle(px, py, r * self.s, self.col(fill), stroke.map(|(w, c)| (w * self.s, self.col(c))));
    }

    pub fn measuring(&self) -> bool {
        self.c.borrow().measuring()
    }

    pub fn label(&self, x: f32, y: f32, s: &str, color: Color32) -> f32 {
        self.text(x, y, &s.to_uppercase(), 10.5, color, Font::Ui, HAlign::Left)
    }

    pub fn label_right(&self, x: f32, y: f32, s: &str, color: Color32) -> f32 {
        self.text(x, y, &s.to_uppercase(), 10.5, color, Font::Ui, HAlign::Right)
    }

    /// Carte : fond sombre, trait d'accent à gauche, coin haut-droit coupé.
    pub fn card(&self, w: f32, h: f32) {
        let cut = 14.0;
        let pts: Vec<(f32, f32)> = [(0.0, 0.0), (w - cut, 0.0), (w, cut), (w, h), (0.0, h)].iter().map(|&(x, y)| self.pt(x, y)).collect();
        let fill = Color32::from_rgba_unmultiplied(8, 11, 16, (self.bg * 255.0) as u8);
        self.c.borrow_mut().polygon(&pts, self.col(fill), Some((1.0, self.col(Color32::from_white_alpha(23)))));
        self.fill(0.0, 0.0, 3.0, h, ACCENT);
    }

    /// Jauge segmentée de `w` × `h`, remplie à `frac` (0..1).
    pub fn bar(&self, x: f32, y: f32, w: f32, h: f32, frac: f32, color: Color32) {
        let (seg, gap) = (6.0, 2.0);
        let n = ((w + gap) / (seg + gap)).floor() as usize;
        let lit = frac.clamp(0.0, 1.0) * w;
        for i in 0..n {
            let sx = i as f32 * (seg + gap);
            self.fill(x + sx, y, seg, h, Color32::from_white_alpha(26));
            if lit > sx {
                self.fill(x + sx, y, seg.min(lit - sx), h, color);
            }
        }
    }

    pub fn sep(&self, x: f32, y: f32, w: f32) {
        self.fill(x, y, w, 1.0, Color32::from_white_alpha(23));
    }
}

pub fn n(v: Option<f64>, dec: usize) -> String {
    match v {
        Some(v) if v.is_finite() => format!("{v:.dec$}"),
        _ => "—".into(),
    }
}

pub fn signed(v: Option<f64>, dec: usize) -> String {
    match v {
        Some(v) if v.is_finite() => format!("{}{:.dec$}", if v > 0.0 { "+" } else if v < 0.0 { "−" } else { "" }, v.abs()),
        _ => "—".into(),
    }
}

pub fn lap(v: Option<f64>) -> String {
    match v {
        Some(s) if s > 0.0 => {
            let m = (s / 60.0).floor();
            format!("{}:{:06.3}", m as i64, s - m * 60.0)
        }
        _ => "—".into(),
    }
}

pub fn hms(v: Option<f64>) -> String {
    match v {
        Some(s) if (0.0..1.0e7).contains(&s) => {
            let s = s as i64;
            let (h, m) = (s / 3600, s % 3600 / 60);
            if h > 0 {
                format!("{h}:{m:02}:{:02}", s % 60)
            } else {
                format!("{m}:{:02}", s % 60)
            }
        }
        _ => "—".into(),
    }
}
