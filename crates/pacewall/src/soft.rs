//! Rendu logiciel des widgets, sans GPU : tiny-skia pour les formes, ab_glyph pour le texte (polices Windows).
//! C'est ce qui dessine l'overlay : une petite image par widget, envoyée à une fenêtre Windows transparente.

use crate::draw::{Canvas, Font, HAlign};
use ab_glyph::{Font as _, FontArc, OutlineCurve, Point};
use eframe::egui::Color32;
use std::collections::HashMap;
use tiny_skia::{FillRule, LineCap, LineJoin, Paint, Path, PathBuilder, PixmapMut, Rect, Stroke, Transform};

/// Masque de transparence d'un caractère déjà dessiné à une taille donnée.
struct GlyphBmp {
    w: i32,
    h: i32,
    /// Décalage du coin haut-gauche du masque par rapport au point d'écriture (x) et à la ligne de base (y).
    ox: i32,
    oy: i32,
    alpha: Vec<u8>,
}

pub struct Fonts {
    ui: Vec<FontArc>,
    num: Vec<FontArc>,
    cache: HashMap<(bool, char), Option<(Path, f32)>>,
    bitmaps: HashMap<(bool, char, u32), Option<GlyphBmp>>,
}

fn load(name: &str) -> Option<FontArc> {
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".into());
    let bytes = std::fs::read(format!(r"{windir}\Fonts\{name}")).ok()?;
    FontArc::try_from_vec(bytes).ok()
}

impl Fonts {
    /// Bahnschrift pour les libellés, Consolas pour les chiffres ; Segoe UI et Arial en repli (glyphes absents).
    pub fn load() -> Self {
        let fallback: Vec<FontArc> = ["segoeui.ttf", "arial.ttf"].iter().filter_map(|n| load(n)).collect();
        let mut ui: Vec<FontArc> = load("bahnschrift.ttf").into_iter().collect();
        let mut num: Vec<FontArc> = load("consola.ttf").into_iter().collect();
        ui.extend(fallback.iter().cloned());
        num.extend(fallback.iter().cloned());
        Self { ui, num, cache: HashMap::new(), bitmaps: HashMap::new() }
    }

    fn chain(&self, num: bool) -> &[FontArc] {
        if num {
            &self.num
        } else {
            &self.ui
        }
    }

    /// Contour d'un caractère en unités de police (y vers le bas, origine sur la ligne de base) et son avance.
    fn glyph(&mut self, num: bool, ch: char) -> Option<&(Path, f32)> {
        if !self.cache.contains_key(&(num, ch)) {
            let entry = self.build(num, ch);
            self.cache.insert((num, ch), entry);
        }
        self.cache.get(&(num, ch)).and_then(|e| e.as_ref())
    }

    fn build(&self, num: bool, ch: char) -> Option<(Path, f32)> {
        for font in self.chain(num) {
            let id = font.glyph_id(ch);
            if id.0 == 0 && ch != ' ' {
                continue;
            }
            let upm = font.units_per_em().unwrap_or(1000.0);
            let advance = font.h_advance_unscaled(id) / upm;
            let mut pb = PathBuilder::new();
            if let Some(outline) = font.outline(id) {
                let mut last: Option<Point> = None;
                let p = |pt: Point| (pt.x / upm, pt.y / upm);
                for curve in &outline.curves {
                    let start = match curve {
                        OutlineCurve::Line(a, _) | OutlineCurve::Quad(a, _, _) | OutlineCurve::Cubic(a, _, _, _) => *a,
                    };
                    if last.is_none_or(|l| (l.x - start.x).abs() > 1e-3 || (l.y - start.y).abs() > 1e-3) {
                        if last.is_some() {
                            pb.close();
                        }
                        let (x, y) = p(start);
                        pb.move_to(x, y);
                    }
                    match curve {
                        OutlineCurve::Line(_, b) => {
                            let (x, y) = p(*b);
                            pb.line_to(x, y);
                            last = Some(*b);
                        }
                        OutlineCurve::Quad(_, b, c) => {
                            let ((x1, y1), (x, y)) = (p(*b), p(*c));
                            pb.quad_to(x1, y1, x, y);
                            last = Some(*c);
                        }
                        OutlineCurve::Cubic(_, b, c, d) => {
                            let ((x1, y1), (x2, y2), (x, y)) = (p(*b), p(*c), p(*d));
                            pb.cubic_to(x1, y1, x2, y2, x, y);
                            last = Some(*d);
                        }
                    }
                }
                if last.is_some() {
                    pb.close();
                }
            }
            // Un espace n'a pas de contour : on garde seulement son avance.
            let path = pb.finish().or_else(|| PathBuilder::from_rect(Rect::from_xywh(0.0, 0.0, 0.001, 0.001)?).into())?;
            return Some((path, advance));
        }
        None
    }

    /// Masque du caractère à la taille `size` (en pixels), mis en cache : on ne rasterise qu'une fois.
    fn bitmap(&mut self, num: bool, ch: char, size: f32) -> Option<&GlyphBmp> {
        let key = (num, ch, (size * 4.0).round() as u32);
        if !self.bitmaps.contains_key(&key) {
            let bmp = self.glyph(num, ch).map(|(p, _)| p.clone()).and_then(|path| {
                let b = path.bounds();
                // Contours en unités de police, axe vertical vers le haut : on retourne vers le bas.
                let (x0, x1) = ((b.left() * size).floor() as i32 - 1, (b.right() * size).ceil() as i32 + 1);
                // L'axe vertical est inversé : le point le plus haut du dessin (bottom) devient le plus petit y.
                let (y0, y1) = ((-b.bottom() * size).floor() as i32 - 1, (-b.top() * size).ceil() as i32 + 1);
                let (w, h) = (x1 - x0, y1 - y0);
                if w <= 0 || h <= 0 || w > 512 || h > 512 {
                    return None;
                }
                let mut pm = tiny_skia::Pixmap::new(w as u32, h as u32)?;
                let mut p = Paint::default();
                p.set_color_rgba8(255, 255, 255, 255);
                p.anti_alias = true;
                pm.fill_path(&path, &p, FillRule::Winding, Transform::from_row(size, 0.0, 0.0, -size, -x0 as f32, -y0 as f32), None);
                let alpha = pm.data().chunks_exact(4).map(|px| px[3]).collect();
                Some(GlyphBmp { w, h, ox: x0, oy: y0, alpha })
            });
            self.bitmaps.insert(key, bmp);
        }
        self.bitmaps.get(&key).and_then(|b| b.as_ref())
    }

    fn ascent(&self, num: bool) -> f32 {
        self.chain(num).first().map(|f| f.ascent_unscaled() / f.units_per_em().unwrap_or(1000.0)).unwrap_or(0.9)
    }

    pub fn width(&mut self, num: bool, s: &str, size: f32) -> f32 {
        s.chars().filter_map(|c| self.glyph(num, c).map(|g| g.1 * size)).sum()
    }
}

/// Image en cours de dessin : octets RGBA prémultipliés (tiny-skia).
pub struct SoftCanvas<'a> {
    pub pm: PixmapMut<'a>,
    pub fonts: &'a mut Fonts,
}

fn paint(c: Color32) -> Paint<'static> {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    let mut p = Paint::default();
    p.set_color_rgba8(r, g, b, a);
    p.anti_alias = true;
    p
}

fn path_of(pts: &[(f32, f32)], close: bool) -> Option<Path> {
    let mut pb = PathBuilder::new();
    let (first, rest) = pts.split_first()?;
    pb.move_to(first.0, first.1);
    for &(x, y) in rest {
        pb.line_to(x, y);
    }
    if close {
        pb.close();
    }
    pb.finish()
}

impl Canvas for SoftCanvas<'_> {
    fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: Color32) {
        // Rectangles alignés sur les pixels, sans lissage : bords nets pour les jauges et les traits fins,
        // et on évite le remplissage lissé de tiny-skia qui échoue sur certaines tailles.
        let (x0, y0) = (x.round(), y.round());
        let (x1, y1) = ((x + w).round(), (y + h).round());
        let (w, h) = ((x1 - x0).max(if w > 0.0 { 1.0 } else { 0.0 }), (y1 - y0).max(if h > 0.0 { 1.0 } else { 0.0 }));
        if let Some(r) = Rect::from_xywh(x0, y0, w, h) {
            let mut p = paint(color);
            p.anti_alias = false;
            self.pm.fill_rect(r, &p, Transform::identity(), None);
        }
    }

    fn polygon(&mut self, pts: &[(f32, f32)], fill: Color32, stroke: Option<(f32, Color32)>) {
        let Some(path) = path_of(pts, true) else { return };
        self.pm.fill_path(&path, &paint(fill), FillRule::Winding, Transform::identity(), None);
        if let Some((width, color)) = stroke {
            let st = Stroke { width, ..Stroke::default() };
            self.pm.stroke_path(&path, &paint(color), &st, Transform::identity(), None);
        }
    }

    fn polyline(&mut self, pts: &[(f32, f32)], width: f32, color: Color32) {
        let Some(path) = path_of(pts, false) else { return };
        let st = Stroke { width, line_cap: LineCap::Round, line_join: LineJoin::Round, ..Stroke::default() };
        self.pm.stroke_path(&path, &paint(color), &st, Transform::identity(), None);
    }

    fn text(&mut self, x: f32, y: f32, s: &str, size: f32, font: Font, align: HAlign, color: Color32) -> f32 {
        let num = font == Font::Num;
        let width = self.fonts.width(num, s, size);
        let left = match align {
            HAlign::Left => x,
            HAlign::Center => x - width / 2.0,
            HAlign::Right => x - width,
        };
        // La ligne de base est à l'ascendante sous le haut du texte.
        let base = (y + self.fonts.ascent(num) * size).round() as i32;
        let (pw, ph) = (self.pm.width() as i32, self.pm.height() as i32);
        let [cr, cg, cb, ca] = color.to_array(); // prémultiplié
        let mut pen = left;
        for ch in s.chars() {
            let advance = match self.fonts.glyph(num, ch) {
                Some(g) => g.1 * size,
                None => continue,
            };
            if ch != ' ' {
                if let Some(g) = self.fonts.bitmap(num, ch, size) {
                    let (gx, gy) = (pen.round() as i32 + g.ox, base + g.oy);
                    let data = self.pm.data_mut();
                    for j in 0..g.h {
                        let py = gy + j;
                        if py < 0 || py >= ph {
                            continue;
                        }
                        for i in 0..g.w {
                            let px = gx + i;
                            if px < 0 || px >= pw {
                                continue;
                            }
                            let m = g.alpha[(j * g.w + i) as usize] as u32;
                            if m == 0 {
                                continue;
                            }
                            // Source prémultipliée × masque, composée sur l'image (source-over).
                            let sa = ca as u32 * m / 255;
                            let inv = 255 - sa;
                            let o = ((py * pw + px) * 4) as usize;
                            data[o] = (cr as u32 * m / 255 + data[o] as u32 * inv / 255).min(255) as u8;
                            data[o + 1] = (cg as u32 * m / 255 + data[o + 1] as u32 * inv / 255).min(255) as u8;
                            data[o + 2] = (cb as u32 * m / 255 + data[o + 2] as u32 * inv / 255).min(255) as u8;
                            data[o + 3] = (sa + data[o + 3] as u32 * inv / 255).min(255) as u8;
                        }
                    }
                }
            }
            pen += advance;
        }
        width
    }
}
