//! Dessin des widgets dans egui (aperçu de l'éditeur) : même code que l'overlay, autre surface.

use crate::draw::{Canvas, Font, HAlign};
use crate::theme;
use eframe::egui::{pos2, vec2, Align2, Color32, Painter, Rect, Shape, Stroke};

pub struct EguiCanvas<'a> {
    pub painter: &'a Painter,
}

impl Canvas for EguiCanvas<'_> {
    fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: Color32) {
        self.painter.rect_filled(Rect::from_min_size(pos2(x, y), vec2(w, h)), 0.0, color);
    }

    fn polygon(&mut self, pts: &[(f32, f32)], fill: Color32, stroke: Option<(f32, Color32)>) {
        let v = pts.iter().map(|&(x, y)| pos2(x, y)).collect();
        let st = stroke.map(|(w, c)| Stroke::new(w, c)).unwrap_or(Stroke::NONE);
        self.painter.add(Shape::convex_polygon(v, fill, st));
    }

    fn polyline(&mut self, pts: &[(f32, f32)], width: f32, color: Color32) {
        let v = pts.iter().map(|&(x, y)| pos2(x, y)).collect();
        self.painter.add(Shape::line(v, Stroke::new(width, color)));
    }

    fn text(&mut self, x: f32, y: f32, s: &str, size: f32, font: Font, align: HAlign, color: Color32) -> f32 {
        let id = if font == Font::Num { theme::num_font(size) } else { theme::ui_font(size) };
        let a = match align {
            HAlign::Left => Align2::LEFT_TOP,
            HAlign::Center => Align2::CENTER_TOP,
            HAlign::Right => Align2::RIGHT_TOP,
        };
        self.painter.text(pos2(x, y), a, s, id, color).width()
    }
}
