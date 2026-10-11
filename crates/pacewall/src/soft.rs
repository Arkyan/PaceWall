//! Rendu logiciel des widgets, sans GPU : tiny-skia pour les formes, ab_glyph pour le texte (polices Windows).
//! C'est ce qui dessine l'overlay : une petite image par widget, envoyée à une fenêtre Windows transparente.

use crate::draw::{Canvas, Font, HAlign};
use ab_glyph::{Font as _, FontArc, OutlineCurve, Point};
use eframe::egui::Color32;
use std::collections::HashMap;
use tiny_skia::{FillRule, Paint, Path, PathBuilder, Pixmap, PixmapMut, Rect, Transform};

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
    /// Couches pré-dessinées (tracé des circuits) : position du coin et image.
    layers: HashMap<u64, (i32, i32, Pixmap)>,
    /// Masque de travail des traits (réutilisé d'un trait à l'autre pour ne rien allouer).
    mask: Vec<u8>,
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
        Self { ui, num, cache: HashMap::new(), bitmaps: HashMap::new(), layers: HashMap::new(), mask: Vec::new() }
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

/// Division par 255 exacte et arrondie, sans division (les compositions en font des centaines de milliers par image).
#[inline(always)]
fn div255(v: u32) -> u32 {
    let v = v + 128;
    (v + (v >> 8)) >> 8
}

/// Pose une couleur prémultipliée sur un pixel prémultiplié (« source par-dessus »).
#[inline(always)]
fn over(px: &mut [u8], [r, g, b, a]: [u8; 4]) {
    if a == 255 || px[3] == 0 {
        // Source opaque, ou pixel encore vide : rien à mélanger.
        px.copy_from_slice(&[r, g, b, a]);
    } else if a != 0 {
        let inv = 255 - a as u32;
        px[0] = (r as u32 + div255(px[0] as u32 * inv)) as u8;
        px[1] = (g as u32 + div255(px[1] as u32 * inv)) as u8;
        px[2] = (b as u32 + div255(px[2] as u32 * inv)) as u8;
        px[3] = (a as u32 + div255(px[3] as u32 * inv)) as u8;
    }
}

/// Comme `over`, pour une couleur couverte à `cov` / 255 (bords lissés, masque d'un caractère).
#[inline(always)]
fn over_cov(px: &mut [u8], [r, g, b, a]: [u8; 4], cov: u32) {
    if cov >= 255 {
        over(px, [r, g, b, a]);
    } else if cov > 0 {
        over(px, [div255(r as u32 * cov) as u8, div255(g as u32 * cov) as u8, div255(b as u32 * cov) as u8, div255(a as u32 * cov) as u8]);
    }
}

impl SoftCanvas<'_> {
    /// Aplat rectangulaire écrit directement en mémoire (source-over, couleur prémultipliée) : bien plus léger
    /// que le pipeline de tiny-skia pour les centaines de petits rectangles d'un widget.
    fn blend_rect(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, color: Color32) {
        let (pw, ph) = (self.pm.width() as i32, self.pm.height() as i32);
        let (x0, x1) = (x0.max(0), x1.min(pw));
        let (y0, y1) = (y0.max(0), y1.min(ph));
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        let c = color.to_array();
        if c[3] == 0 {
            return;
        }
        let data = self.pm.data_mut();
        for y in y0..y1 {
            let row = (y * pw) as usize * 4;
            for px in data[row + x0 as usize * 4..row + x1 as usize * 4].chunks_exact_mut(4) {
                over(px, c);
            }
        }
    }

    /// Trait d'un pixel d'épaisseur entre deux points (Bresenham), pour les contours.
    fn hairline(&mut self, (x0, y0): (f32, f32), (x1, y1): (f32, f32), color: Color32) {
        let (mut x, mut y) = (x0.round() as i32, y0.round() as i32);
        let (xe, ye) = (x1.round() as i32, y1.round() as i32);
        let (dx, dy) = ((xe - x).abs(), -(ye - y).abs());
        let (sx, sy) = (if x < xe { 1 } else { -1 }, if y < ye { 1 } else { -1 });
        let mut err = dx + dy;
        loop {
            self.blend_rect(x, y, x + 1, y + 1, color);
            if x == xe && y == ye {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }
}

impl Canvas for SoftCanvas<'_> {
    fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: Color32) {
        // Alignés sur les pixels, sans lissage : bords nets pour les jauges et les traits fins.
        let (x0, y0) = (x.round() as i32, y.round() as i32);
        let x1 = (x + w).round() as i32;
        let y1 = (y + h).round() as i32;
        let x1 = if w > 0.0 { x1.max(x0 + 1) } else { x0 };
        let y1 = if h > 0.0 { y1.max(y0 + 1) } else { y0 };
        self.blend_rect(x0, y0, x1, y1, color);
    }

    fn polygon(&mut self, pts: &[(f32, f32)], fill: Color32, stroke: Option<(f32, Color32)>) {
        // Polygone convexe rempli ligne par ligne (une plage par ligne de pixels), sans lissage.
        let (min_y, max_y) = pts.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| (lo.min(p.1), hi.max(p.1)));
        for y in (min_y.floor() as i32)..(max_y.ceil() as i32) {
            let cy = y as f32 + 0.5;
            let (mut lo, mut hi) = (f32::MAX, f32::MIN);
            for i in 0..pts.len() {
                let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
                if (a.1 <= cy && b.1 > cy) || (b.1 <= cy && a.1 > cy) {
                    let x = a.0 + (cy - a.1) / (b.1 - a.1) * (b.0 - a.0);
                    lo = lo.min(x);
                    hi = hi.max(x);
                }
            }
            if lo <= hi {
                self.blend_rect(lo.round() as i32, y, hi.round() as i32, y + 1, fill);
            }
        }
        if let Some((_, color)) = stroke {
            for i in 0..pts.len() {
                let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
                self.hairline(a, b, color);
            }
        }
    }

    fn circle(&mut self, x: f32, y: f32, r: f32, fill: Color32, stroke: Option<(f32, Color32)>) {
        // Disque et anneau tramés par la distance au centre : le bord est lissé sur un pixel. Bien plus léger que de
        // construire un chemin et de le remplir, pour les dizaines de pastilles d'une carte ou d'un radar.
        let r = r.max(0.5);
        let (fill, ring) = (fill.to_array(), stroke.map(|(w, c)| (w.max(0.0), c.to_array())));
        let (outer, inner) = match ring {
            Some((w, _)) => (r + w / 2.0, (r - w / 2.0).max(0.0)),
            None => (r, r),
        };
        let (pw, ph) = (self.pm.width() as i32, self.pm.height() as i32);
        let (x0, x1) = (((x - outer - 1.0).floor() as i32).max(0), ((x + outer + 1.0).ceil() as i32).min(pw));
        let (y0, y1) = (((y - outer - 1.0).floor() as i32).max(0), ((y + outer + 1.0).ceil() as i32).min(ph));
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        let far2 = (outer + 0.5) * (outer + 0.5);
        // À l'intérieur de ce rayon, le pixel est entièrement dans le disque et hors de l'anneau : pas de racine carrée.
        let core = (if ring.is_some() { inner } else { r } - 0.5).max(0.0);
        let core2 = core * core;
        let data = self.pm.data_mut();
        for py in y0..y1 {
            let dy = py as f32 + 0.5 - y;
            let row = (py * pw) as usize * 4;
            for px in x0..x1 {
                let dx = px as f32 + 0.5 - x;
                let d2 = dx * dx + dy * dy;
                if d2 >= far2 {
                    continue;
                }
                let o = row + px as usize * 4;
                if d2 <= core2 {
                    over(&mut data[o..o + 4], fill);
                    continue;
                }
                let d = d2.sqrt();
                if fill[3] != 0 {
                    over_cov(&mut data[o..o + 4], fill, ((r + 0.5 - d).clamp(0.0, 1.0) * 255.0) as u32);
                }
                if let Some((_, color)) = ring {
                    let cov = (outer + 0.5 - d).clamp(0.0, 1.0) * (d - inner + 0.5).clamp(0.0, 1.0);
                    over_cov(&mut data[o..o + 4], color, (cov * 255.0) as u32);
                }
            }
        }
    }

    fn polyline(&mut self, pts: &[(f32, f32)], width: f32, color: Color32) {
        // Trait à bouts et jonctions arrondis : chaque segment est une « gélule » tramée par la distance au segment.
        // Les couvertures sont d'abord réunies dans un masque (le maximum, pour qu'un trait translucide ne se
        // superpose pas à lui-même aux jonctions), puis la couleur est posée une seule fois.
        if pts.len() < 2 {
            return;
        }
        let half = (width / 2.0).max(0.5);
        let reach = half + 1.0;
        let (pw, ph) = (self.pm.width() as i32, self.pm.height() as i32);
        let (mut lo, mut hi) = ((f32::MAX, f32::MAX), (f32::MIN, f32::MIN));
        for p in pts {
            lo = (lo.0.min(p.0), lo.1.min(p.1));
            hi = (hi.0.max(p.0), hi.1.max(p.1));
        }
        let (bx0, bx1) = (((lo.0 - reach).floor() as i32).max(0), ((hi.0 + reach).ceil() as i32).min(pw));
        let (by0, by1) = (((lo.1 - reach).floor() as i32).max(0), ((hi.1 + reach).ceil() as i32).min(ph));
        if bx0 >= bx1 || by0 >= by1 {
            return;
        }
        let (bw, bh) = ((bx1 - bx0) as usize, (by1 - by0) as usize);
        let mut mask = std::mem::take(&mut self.fonts.mask);
        mask.clear();
        mask.resize(bw * bh, 0);
        let (full2, far2) = ((half - 0.5).max(0.0).powi(2), (half + 0.5).powi(2));
        for seg in pts.windows(2) {
            let ((ax, ay), (bx, by)) = (seg[0], seg[1]);
            let (dx, dy) = (bx - ax, by - ay);
            let len2 = dx * dx + dy * dy;
            let (x0, x1) = (((ax.min(bx) - reach).floor() as i32).max(bx0), ((ax.max(bx) + reach).ceil() as i32).min(bx1));
            let (y0, y1) = (((ay.min(by) - reach).floor() as i32).max(by0), ((ay.max(by) + reach).ceil() as i32).min(by1));
            for py in y0..y1 {
                let cy = py as f32 + 0.5 - ay;
                let row = (py - by0) as usize * bw;
                for px in x0..x1 {
                    let cx = px as f32 + 0.5 - ax;
                    // Point du segment le plus proche du centre du pixel.
                    let t = if len2 > 1e-6 { ((cx * dx + cy * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
                    let (ex, ey) = (cx - dx * t, cy - dy * t);
                    let d2 = ex * ex + ey * ey;
                    if d2 >= far2 {
                        continue;
                    }
                    let cov = if d2 <= full2 { 255 } else { ((half + 0.5 - d2.sqrt()).clamp(0.0, 1.0) * 255.0) as u8 };
                    let m = &mut mask[row + (px - bx0) as usize];
                    if cov > *m {
                        *m = cov;
                    }
                }
            }
        }
        let c = color.to_array();
        let data = self.pm.data_mut();
        for (j, line) in mask.chunks_exact(bw).enumerate() {
            let row = ((by0 + j as i32) * pw + bx0) as usize * 4;
            for (i, m) in line.iter().enumerate() {
                if *m != 0 {
                    let o = row + i * 4;
                    over_cov(&mut data[o..o + 4], c, *m as u32);
                }
            }
        }
        self.fonts.mask = mask;
    }

    fn layer(&mut self, key: u64, rect: (f32, f32, f32, f32), f: &mut dyn FnMut(&mut dyn Canvas, (f32, f32))) {
        let (ix, iy) = (rect.0.floor() as i32, rect.1.floor() as i32);
        let (iw, ih) = (rect.2.ceil() as i64 + 2, rect.3.ceil() as i64 + 2);
        if iw <= 0 || ih <= 0 || iw > 4096 || ih > 4096 {
            return f(self, (0.0, 0.0));
        }
        if !self.fonts.layers.contains_key(&key) {
            if self.fonts.layers.len() > 12 {
                self.fonts.layers.clear();
            }
            let Some(mut img) = Pixmap::new(iw as u32, ih as u32) else { return f(self, (0.0, 0.0)) };
            {
                let mut child = SoftCanvas { pm: img.as_mut(), fonts: &mut *self.fonts };
                f(&mut child, (ix as f32, iy as f32));
            }
            self.fonts.layers.insert(key, (ix, iy, img));
        }
        if let Some((lx, ly, img)) = self.fonts.layers.get(&key) {
            // Recopie « source par-dessus » en pixels prémultipliés. Les couches sont surtout transparentes : on saute
            // les pixels vides, bien plus vite que le pipeline général pour une grande image.
            let (dw, dh) = (self.pm.width() as i32, self.pm.height() as i32);
            let (sw, sh) = (img.width() as i32, img.height() as i32);
            let src = img.data();
            let dst = self.pm.data_mut();
            for sy in 0..sh {
                let dy = ly + sy;
                if dy < 0 || dy >= dh {
                    continue;
                }
                for sx in 0..sw {
                    let dx = lx + sx;
                    if dx < 0 || dx >= dw {
                        continue;
                    }
                    let si = ((sy * sw + sx) * 4) as usize;
                    if src[si + 3] == 0 {
                        continue;
                    }
                    let di = ((dy * dw + dx) * 4) as usize;
                    over(&mut dst[di..di + 4], [src[si], src[si + 1], src[si + 2], src[si + 3]]);
                }
            }
        }
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
        let color = color.to_array(); // prémultiplié
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
                            let o = ((py * pw + px) * 4) as usize;
                            over_cov(&mut data[o..o + 4], color, m);
                        }
                    }
                }
            }
            pen += advance;
        }
        width
    }
}
