//! Dessin des widgets pour OBS : exactement le même code que l'overlay du joueur (`widgets` + `soft`), rendu dans une
//! image que le serveur web envoie à la page. Il n'y a donc qu'un seul dessin des widgets dans PaceWall.

use crate::draw::{Measure, D};
use crate::soft::{Fonts, SoftCanvas};
use crate::widgets::{self, WState};
use pwcore::layout::WidgetSpec;
use pwcore::Data;
use server::{Frame, Painter};
use tiny_skia::Pixmap;

struct ObsPainter {
    fonts: Fonts,
    /// État de chaque widget de la mise en page (graphe des inputs, carte chargée, radar…).
    states: Vec<WState>,
    /// Image de travail de chaque widget, gardée d'un dessin à l'autre (créer une image à chaque fois coûte cher).
    images: Vec<Option<Pixmap>>,
}

pub fn factory() -> Box<dyn Painter> {
    Box::new(ObsPainter { fonts: Fonts::load(), states: Vec::new(), images: Vec::new() })
}

/// Table de « dé-prémultiplication » : pour une opacité `a` et une composante prémultipliée `v`, la composante simple.
fn unpremultiply_table() -> &'static [[u8; 256]; 256] {
    static TABLE: std::sync::OnceLock<Box<[[u8; 256]; 256]>> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        let mut t = Box::new([[0u8; 256]; 256]);
        for a in 1..256u32 {
            for v in 0..256u32 {
                t[a as usize][v as usize] = ((v * 255 + a / 2) / a).min(255) as u8;
            }
        }
        t
    })
}

impl Painter for ObsPainter {
    fn hz(&self, index: usize, spec: &WidgetSpec) -> f32 {
        match self.states.get(index) {
            Some(state) => widgets::hz_state(&spec.kind, spec, state),
            None => widgets::hz(&spec.kind, spec),
        }
    }

    fn animated(&self, index: usize, spec: &WidgetSpec) -> bool {
        (spec.kind == "inputs" && spec.flag("graph", true)) || spec.kind == "radar" || self.states.get(index).is_some_and(|s| s.animating())
    }

    fn reset(&mut self) {
        self.states.clear();
        self.images.clear();
    }

    fn paint(&mut self, index: usize, spec: &WidgetSpec, data: &Data, unit: f32, now: f64) -> Frame {
        if self.states.len() <= index {
            self.states.resize_with(index + 1, WState::default);
        }
        let empty = Frame { w: 0, h: 0, rgba: Vec::new() };
        let scale = spec.scale * unit;
        let bg = widgets::bg(spec);
        let (accent, bar, titles) = (widgets::accent(spec), widgets::bar(spec), widgets::titles(spec));
        // Mesure : la taille du widget dépend de ses réglages et de ses données.
        let (bw, bh) = {
            let mut measure = Measure;
            let d = D::new(&mut measure, (0.0, 0.0), scale, 1.0, bg).styled(accent, bar).titled(titles);
            // La carte du circuit garde sa carte chargée entre deux images : la relire à chaque mesure coûterait cher.
            if spec.kind == "trackmap" {
                widgets::draw(&spec.kind, &d, spec, data, &mut self.states[index], now)
            } else {
                widgets::draw(&spec.kind, &d, spec, data, &mut WState::default(), now)
            }
        };
        let (pw, ph) = ((bw * scale).ceil() as u32 + 2, (bh * scale).ceil() as u32 + 2);
        if pw <= 2 || ph <= 2 || pw > 4096 || ph > 4096 {
            return empty;
        }
        if self.images.len() <= index {
            self.images.resize_with(index + 1, || None);
        }
        // Même taille que la dernière fois : on réutilise l'image, simplement effacée.
        let pixmap = match &mut self.images[index] {
            Some(p) if p.width() == pw && p.height() == ph => {
                p.data_mut().fill(0);
                p
            }
            slot => {
                let Some(fresh) = Pixmap::new(pw, ph) else { return empty };
                slot.insert(fresh)
            }
        };
        {
            let mut canvas = SoftCanvas { pm: pixmap.as_mut(), fonts: &mut self.fonts };
            // Même appel que l'overlay du joueur, opacité du widget comprise.
            let d = D::new(&mut canvas, (0.0, 0.0), scale, spec.opacity, bg).styled(accent, bar).titled(titles);
            widgets::draw(&spec.kind, &d, spec, data, &mut self.states[index], now);
        }
        // tiny-skia produit du RGBA prémultiplié ; la page attend du RGBA simple. La division est lue dans une table.
        let table = unpremultiply_table();
        let src = pixmap.data();
        let mut rgba = vec![0u8; src.len()];
        let mut any = false;
        for (out, px) in rgba.chunks_exact_mut(4).zip(src.chunks_exact(4)) {
            let a = px[3];
            if a == 0 {
                continue; // déjà à zéro
            }
            any = true;
            if a == 255 {
                out.copy_from_slice(px);
            } else {
                let row = &table[a as usize];
                out.copy_from_slice(&[row[px[0] as usize], row[px[1] as usize], row[px[2] as usize], a]);
            }
        }
        // Widget qui ne montre rien en ce moment (radar sans voisin, aucun drapeau) : pas d'image du tout.
        if !any {
            return empty;
        }
        Frame { w: pw, h: ph, rgba }
    }
}

#[cfg(test)]
mod bench {
    use super::*;
    use pwcore::flatten::Field;
    use serde_json::json;
    use std::time::Instant;

    /// Données de démonstration, plus cinq voitures autour du joueur avec leur cap (pour le radar).
    fn data(t: f64) -> Data {
        let mut f = pwcore::demo::data(t).fields;
        let me = f.iter().position(|x| x.key == "grid.003.x").unwrap();
        let (mx, mz) = (f[me].value.as_f64().unwrap(), f[me + 1].value.as_f64().unwrap());
        let mut put = |k: String, v: serde_json::Value| f.push(Field { key: k, unit: "", label: "", value: v });
        put("grid.003.yaw".into(), json!(0.3));
        for (n, (dx, dz)) in [(2.5, 1.0), (-2.6, -6.0), (0.5, 12.0), (3.0, -14.0), (-1.0, 17.0)].iter().enumerate() {
            let i = [0, 1, 2, 4, 5][n];
            put(format!("grid.{i:03}.x"), json!(mx + dx + (t * 3.0).sin()));
            put(format!("grid.{i:03}.z"), json!(mz + dz));
            put(format!("grid.{i:03}.yaw"), json!(0.3 + 0.05 * n as f64));
        }
        Data::new(f)
    }

    /// Décomposition du coût : mesure, création de l'image, dessin, conversion.
    #[test]
    #[ignore]
    fn cost_breakdown() {
        let mut fonts = Fonts::load();
        for kind in ["standings", "radar", "trackmap", "inputs"] {
            let mut spec = WidgetSpec::new(kind, kind, 0.0, 0.0);
            spec.opts.insert("autohide".into(), json!(false));
            let mut st = WState::default();
            let d0 = data(1.0);
            let (accent, bar, titles, bg) = (widgets::accent(&spec), widgets::bar(&spec), widgets::titles(&spec), widgets::bg(&spec));
            let n = 300;
            let (mut t_measure, mut t_new, mut t_draw, mut t_conv) = (0u128, 0u128, 0u128, 0u128);
            for k in 0..n + 100 {
                let now = k as f64 * 0.033;
                let d_ = data(now);
                let t = Instant::now();
                let (bw, bh) = {
                    let mut m = Measure;
                    let d = D::new(&mut m, (0.0, 0.0), 1.0, 1.0, bg).styled(accent, bar).titled(titles);
                    widgets::draw(kind, &d, &spec, &d_, &mut WState::default(), now)
                };
                let a = t.elapsed().as_micros();
                let t = Instant::now();
                let mut pm = Pixmap::new(bw.ceil() as u32 + 2, bh.ceil() as u32 + 2).unwrap();
                let b = t.elapsed().as_micros();
                let t = Instant::now();
                {
                    let mut c = SoftCanvas { pm: pm.as_mut(), fonts: &mut fonts };
                    let d = D::new(&mut c, (0.0, 0.0), 1.0, 1.0, bg).styled(accent, bar).titled(titles);
                    widgets::draw(kind, &d, &spec, &d_, &mut st, now);
                }
                let c = t.elapsed().as_micros();
                let t = Instant::now();
                let mut rgba = pm.take();
                for px in rgba.chunks_exact_mut(4) {
                    let al = px[3] as u32;
                    if al > 0 && al < 255 {
                        px[0] = ((px[0] as u32 * 255 + al / 2) / al).min(255) as u8;
                        px[1] = ((px[1] as u32 * 255 + al / 2) / al).min(255) as u8;
                        px[2] = ((px[2] as u32 * 255 + al / 2) / al).min(255) as u8;
                    }
                }
                let e = t.elapsed().as_micros();
                if k >= 100 {
                    t_measure += a; t_new += b; t_draw += c; t_conv += e;
                }
                let _ = &d0;
            }
            let n = n as u128;
            println!("{kind:10} mesure {:4} | image {:4} | dessin {:4} | conversion {:4} µs", t_measure / n, t_new / n, t_draw / n, t_conv / n);
        }
    }

    /// Aperçu de l'arrêt au stand et du repère de sortie sur la carte, en images (`PACEWALL_SHOT_DIR`).
    #[test]
    #[ignore]
    fn pit_preview() {
        let Ok(dir) = std::env::var("PACEWALL_SHOT_DIR") else { return };
        let mut fonts = Fonts::load();
        let mut f = data(1.0).fields;
        let mut put = |k: &str, v: serde_json::Value| f.push(Field { key: k.into(), unit: "", label: "", value: v });
        for (k, v) in [("pit.lane_entry", 3700.0), ("pit.lane_exit", 300.0), ("pit.lane_speed", 16.7), ("pit.stop_time", 43.5), ("pit.repair_time", 14.0), ("pit.menu.count", 4.0)] {
            put(k, json!(v));
        }
        for (i, (n, v)) in [("FUEL", "+35 L"), ("VIRTUAL ENERGY", "80 %"), ("TIRES", "Medium x4"), ("DAMAGE", "Repair all")].iter().enumerate() {
            put(&format!("pit.menu.{i}.name"), json!(n));
            put(&format!("pit.menu.{i}.value"), json!(v));
        }
        let data = Data::new(f);
        for kind in ["pit", "trackmap"] {
            let mut spec = WidgetSpec::new(kind, kind, 0.0, 0.0);
            spec.opts.insert("pitexit".into(), json!("always"));
            let (accent, bar, titles, bg) = (widgets::accent(&spec), widgets::bar(&spec), widgets::titles(&spec), widgets::bg(&spec));
            let mut st = WState::default();
            let (w, h) = {
                let mut m = Measure;
                let d = D::new(&mut m, (0.0, 0.0), 1.0, 1.0, bg).styled(accent, bar).titled(titles);
                widgets::draw(kind, &d, &spec, &data, &mut st, 1.0)
            };
            let mut pm = Pixmap::new((w * 2.0).ceil() as u32 + 2, (h * 2.0).ceil() as u32 + 2).unwrap();
            {
                let mut c = SoftCanvas { pm: pm.as_mut(), fonts: &mut fonts };
                let d = D::new(&mut c, (0.0, 0.0), 2.0, 1.0, bg).styled(accent, bar).titled(titles);
                widgets::draw(kind, &d, &spec, &data, &mut st, 1.0);
            }
            pm.save_png(format!("{dir}/pit_{kind}.png")).unwrap();
        }
    }

    /// Mesure du coût de dessin de chaque widget (µs par image, à l'échelle 1080p) :
    /// `cargo test --release -p pacewall bench -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn cost_per_widget() {
        let mut painter = ObsPainter { fonts: Fonts::load(), states: Vec::new(), images: Vec::new() };
        for (i, kind) in ["radar", "trackmap", "inputs", "standings", "relative", "fuel", "tyres", "timing", "flags", "session", "pit", "forecast"].iter().enumerate() {
            let mut spec = WidgetSpec::new(kind, kind, 0.0, 0.0);
            if *kind == "radar" {
                spec.opts.insert("autohide".into(), json!(false));
            }
            let n = 300;
            // Quelques images d'abord : cartes chargées, caches remplis, historique du graphe plein.
            for k in 0..200 {
                painter.paint(i, &spec, &data(k as f64 * 0.05), 1.0, k as f64 * 0.05);
            }
            let datas: Vec<Data> = (0..n).map(|k| data(10.0 + k as f64 * 0.033)).collect();
            let start = Instant::now();
            let mut pixels = 0;
            for (k, d) in datas.iter().enumerate() {
                let f = painter.paint(i, &spec, d, 1.0, 10.0 + k as f64 * 0.033);
                pixels = f.w * f.h;
            }
            println!("{kind:10} {:6.0} µs par image ({pixels} pixels)", start.elapsed().as_micros() as f64 / n as f64);
        }
    }
}
