//! Characters tab: the full set of a character's parts (`pedset`), every
//! drawable of its outfits and wardrobe with its variants.
use eframe::egui;
use sixng_core::pedset::PartSet;
use std::sync::atomic::{AtomicBool, Ordering};

/// Start with the section open (`--full-set`, for screenshots).
pub static OPEN: AtomicBool = AtomicBool::new(false);

/// The "Full set" section. Returns the outfit to open when an outfit number
/// is clicked.
pub fn ui(ui: &mut egui::Ui, set: &PartSet, current: usize) -> Option<usize> {
    let mut open = None;
    let variants: usize = set.parts.iter().map(|p| p.variants.len()).sum();
    let listed = set.outfits.iter().filter(|o| o.listed).count();
    let other = set.outfits.len() - listed;
    let head = format!("Full set: {} parts, {} variants", set.parts.len(), variants);
    egui::CollapsingHeader::new(head).id_salt("ped_full_set").default_open(OPEN.load(Ordering::Relaxed)).show(ui, |ui| {
        let mut what = format!("Every drawable of the {listed} listed outfits");
        if other > 0 {
            what += &format!(" and of {other} other variation{} of the wardrobe", if other == 1 { "" } else { "s" });
        }
        what += ", with each texture and colour variant (wardrobe item). The numbers are the outfits that wear it; click one to show that outfit.";
        ui.label(egui::RichText::new(what).weak().small());
        if set.complete {
            ui.label(egui::RichText::new("Complete: every item resolved to a model.").small());
        } else {
            ui.label(egui::RichText::new(format!("{} item{} resolved to no model (listed under the outfit below).", set.unresolved, if set.unresolved == 1 { "" } else { "s" })).small().color(egui::Color32::from_rgb(220, 160, 60)));
        }
        ui.horizontal(|ui| {
            if ui.button("Copy as JSON").on_hover_text("The same list as `sixng ped-parts <model> --out <dir>` writes to parts.json").clicked() {
                if let Ok(s) = serde_json::to_string_pretty(set) {
                    ui.ctx().copy_text(s);
                }
            }
            if ui.button("Copy names").on_hover_text("The drawable names, one per line").clicked() {
                ui.ctx().copy_text(set.parts.iter().map(|p| p.drawable.as_str()).collect::<Vec<_>>().join("\n"));
            }
        });
        for kind in ["body", "hair", "clothing", "accessory"] {
            let parts: Vec<_> = set.parts.iter().filter(|p| p.kind == kind).collect();
            if parts.is_empty() {
                continue;
            }
            egui::CollapsingHeader::new(format!("{kind} ({})", parts.len())).id_salt(("ped_set_kind", kind)).default_open(kind == "clothing").show(ui, |ui| {
                egui::Grid::new(("ped_set_grid", kind)).num_columns(3).striped(true).show(ui, |ui| {
                    for p in parts {
                        ui.label(&p.slot);
                        let name = egui::RichText::new(&p.drawable).monospace().small();
                        let hover = if p.strands { format!("found by {}; a strand-hair placeholder: the hair is the groom of the item's asset", p.found_by) } else { format!("found by {}", p.found_by) };
                        ui.label(name).on_hover_text(hover);
                        ui.horizontal_wrapped(|ui| {
                            for (vi, v) in p.variants.iter().enumerate() {
                                if p.variants.len() > 1 {
                                    let mut tip = if v.item.is_empty() { "no wardrobe item".to_string() } else { format!("item {}", v.item) };
                                    if let Some(c) = v.colour_tag {
                                        tip += &format!(", {c}");
                                    }
                                    if let Some(t) = &v.tint {
                                        tip += &format!(", tint {}:{}", t.palette, t.rows[0]);
                                    }
                                    if let Some(pr) = &v.preset {
                                        tip += &format!(", preset {pr}");
                                    }
                                    ui.label(egui::RichText::new(format!("v{}", vi + 1)).weak()).on_hover_text(tip);
                                }
                                for &o in &v.outfits {
                                    let so = &set.outfits[o];
                                    if so.listed {
                                        let b = ui.selectable_label(o == current, format!("{}", o + 1)).on_hover_text(&so.label);
                                        if b.clicked() && o != current {
                                            open = Some(o);
                                        }
                                    } else {
                                        ui.label(egui::RichText::new(format!("w:{}", so.id)).weak().small()).on_hover_text(format!("{} (a variation of the wardrobe no listed outfit uses)", so.label));
                                    }
                                }
                            }
                        });
                        ui.end_row();
                    }
                });
            });
        }
        let missing: Vec<_> = set.outfits.iter().enumerate().filter(|(_, o)| !o.unresolved.is_empty()).collect();
        if !missing.is_empty() {
            egui::CollapsingHeader::new("not found").id_salt("ped_set_missing").show(ui, |ui| {
                for (i, o) in missing {
                    let who = if o.listed { format!("outfit {}", i + 1) } else { format!("variation {}", o.id) };
                    for u in &o.unresolved {
                        ui.label(egui::RichText::new(format!("{who}: {u}")).small());
                    }
                }
            });
        }
    });
    open
}
