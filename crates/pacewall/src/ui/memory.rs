//! Mémoire du jeu : la référence de tout ce que Le Mans Ultimate publie dans `LMU_Data`, champ par champ, avec
//! recherche. Le contenu (`memory.tsv`) est généré par `tools/doc/memoire_partagee.py`, comme `docs/memoire-partagee.md`.

use crate::theme::*;
use eframe::egui::{self, vec2, Align, Layout, Margin, RichText, Ui};
use std::sync::OnceLock;

struct Field {
    offset: &'static str,
    name: &'static str,
    ty: &'static str,
    size: &'static str,
    unit: &'static str,
    desc: &'static str,
    /// Champ ajouté par Le Mans Ultimate (absent de rFactor 2).
    lmu: bool,
    /// Nom, type, unité et description en minuscules, pour la recherche.
    search: String,
}

struct Group {
    name: &'static str,
    cpp: &'static str,
    size: &'static str,
    title: &'static str,
    note: &'static str,
    fields: Vec<Field>,
}

fn groups() -> &'static [Group] {
    static GROUPS: OnceLock<Vec<Group>> = OnceLock::new();
    GROUPS.get_or_init(|| {
        let mut out: Vec<Group> = Vec::new();
        for line in include_str!("memory.tsv").lines() {
            let c: Vec<&'static str> = line.split('\t').collect();
            match (c.first().copied(), c.len()) {
                (Some("S"), 6) => out.push(Group { name: c[1], cpp: c[2], size: c[3], title: c[4], note: c[5], fields: Vec::new() }),
                (Some("F"), 8) => {
                    if let Some(g) = out.last_mut() {
                        let search = format!("{} {} {} {}", c[2], c[3], c[5], c[6]).to_lowercase();
                        g.fields.push(Field { offset: c[1], name: c[2], ty: c[3], size: c[4], unit: c[5], desc: c[6], lmu: c[7] == "1", search });
                    }
                }
                _ => {}
            }
        }
        out
    })
}

#[derive(Default)]
pub struct State {
    filter: String,
    /// Structure affichée seule (`None` : toutes).
    only: Option<&'static str>,
    lmu_only: bool,
}

/// Largeur des colonnes fixes : décalage, champ, type, octets, unité. La signification prend le reste.
const COLS: [f32; 5] = [58.0, 236.0, 196.0, 56.0, 96.0];

fn cell(ui: &mut Ui, width: f32, right: bool, add: impl FnOnce(&mut Ui)) {
    let layout = if right { Layout::top_down(Align::RIGHT) } else { Layout::top_down(Align::LEFT) };
    ui.allocate_ui_with_layout(vec2(width, 0.0), layout, |ui| {
        ui.set_width(width);
        add(ui);
    });
}

fn header(ui: &mut Ui) {
    ui.horizontal_top(|ui| {
        let head = |ui: &mut Ui, text: &str| {
            ui.label(RichText::new(text).size(10.5).color(MUTED));
        };
        cell(ui, COLS[0], true, |ui| head(ui, "DÉCALAGE"));
        cell(ui, COLS[1], false, |ui| head(ui, "CHAMP"));
        cell(ui, COLS[2], false, |ui| head(ui, "TYPE"));
        cell(ui, COLS[3], true, |ui| head(ui, "OCTETS"));
        cell(ui, COLS[4], false, |ui| head(ui, "UNITÉ"));
        head(ui, "SIGNIFICATION");
    });
}

fn row(ui: &mut Ui, f: &Field) {
    ui.horizontal_top(|ui| {
        cell(ui, COLS[0], true, |ui| {
            ui.label(RichText::new(f.offset).font(num_font(12.5)).color(MUTED));
        });
        cell(ui, COLS[1], false, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                ui.label(RichText::new(f.name).font(num_font(12.5)));
                if f.lmu {
                    ui.label(RichText::new("LMU").size(9.5).color(ui_accent())).on_hover_text("Champ ajouté par Le Mans Ultimate (absent de rFactor 2)");
                }
            });
        });
        cell(ui, COLS[2], false, |ui| {
            ui.label(RichText::new(f.ty).font(num_font(11.5)).color(MUTED));
        });
        cell(ui, COLS[3], true, |ui| {
            ui.label(RichText::new(f.size).font(num_font(12.5)).color(MUTED));
        });
        cell(ui, COLS[4], false, |ui| {
            ui.label(RichText::new(f.unit).size(12.5));
        });
        ui.add(egui::Label::new(RichText::new(f.desc).size(12.5)).wrap());
    });
}

impl State {
    pub fn show(&mut self, ui: &mut Ui) {
        let all = groups();
        let total: usize = all.iter().map(|g| g.fields.len()).sum();
        egui::Frame::NONE.inner_margin(Margin::symmetric(24, 18)).show(ui, |ui| {
            ui.label(RichText::new("MÉMOIRE DU JEU").size(18.0).strong());
            ui.label(RichText::new(format!("Tout ce que Le Mans Ultimate publie dans sa zone mémoire « LMU_Data » : {total} champs, à l'octet près.")).color(MUTED));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.filter).hint_text("Chercher (ex. fuel, pneu, kelvin, mPlace)…").desired_width(280.0));
                let selected = self.only.and_then(|n| all.iter().find(|g| g.name == n)).map_or("Toutes les structures", |g| g.title);
                egui::ComboBox::from_id_salt("memory_struct").width(220.0).selected_text(selected).show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.only, None, "Toutes les structures");
                    for g in all {
                        ui.selectable_value(&mut self.only, Some(g.name), g.title);
                    }
                });
                ui.checkbox(&mut self.lmu_only, "Propres à LMU");
            });
            ui.add_space(4.0);
            ui.label(
                RichText::new("Alignement sur 4 octets. Décalages comptés depuis le début de chaque structure. Classement et télémétrie n'ont pas le même ordre : relier les voitures par mID.")
                    .size(12.0)
                    .color(MUTED),
            );
            ui.add_space(8.0);

            let q = self.filter.trim().to_lowercase();
            let narrowed = !q.is_empty() || self.lmu_only || self.only.is_some();
            let mut shown = 0;
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                for g in all.iter().filter(|g| self.only.is_none_or(|n| n == g.name)) {
                    let rows: Vec<&Field> = g.fields.iter().filter(|f| (!self.lmu_only || f.lmu) && (q.is_empty() || f.search.contains(&q))).collect();
                    if rows.is_empty() {
                        continue;
                    }
                    shown += rows.len();
                    let title = format!("{}  ·  {}  ·  {} octets  ·  {} champs", g.title.to_uppercase(), g.cpp, g.size, rows.len());
                    // Une recherche ouvre tout ce qui correspond ; sinon seules les structures de données de course sont dépliées.
                    let open = narrowed || matches!(g.name, "ObjectOut" | "ScoringInfo");
                    egui::CollapsingHeader::new(RichText::new(title).size(12.5).color(MUTED)).id_salt(("memory", g.name, narrowed)).default_open(open).show(ui, |ui| {
                        ui.label(RichText::new(g.note).size(12.0).color(MUTED));
                        ui.add_space(4.0);
                        header(ui);
                        ui.separator();
                        for f in rows {
                            row(ui, f);
                            ui.add_space(2.0);
                        }
                        ui.add_space(8.0);
                    });
                }
                if shown == 0 {
                    ui.add_space(40.0);
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("Aucun champ ne correspond.").color(MUTED));
                    });
                }
            });
        });
    }
}
