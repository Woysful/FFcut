//! A ready-made effect (echo, limiter, LUT...): its controls, drawn from its entry in `builtin::DEFS`.
//! Shared by the audio and the video stacks.

use super::widgets;
use eframe::egui;
use ffcut_core::builtin::{self, Ctl, Def, Param};
use ffcut_core::BuiltinFx;
use std::hash::Hash;

/// One line about the effect for its card's tooltip.
pub fn tip(fx: &BuiltinFx) -> Option<&'static str> {
    builtin::def(&fx.id).map(|d| d.desc)
}

fn control(ui: &mut egui::Ui, id: egui::Id, d: &Def, p: &Param, fx: &mut BuiltinFx) {
    let mut v = d.value(fx, p.key);
    let before = v;
    match p.ctl {
        Ctl::Num { min, max, unit, decimals, log, pct } => {
            let s = egui::Slider::new(&mut v, min..=max).logarithmic(log);
            let s = if pct {
                s.custom_formatter(|n, _| format!("{:.0} %", n * 100.0))
                    .custom_parser(|s| s.trim().trim_end_matches('%').trim().parse::<f64>().ok().map(|n| n / 100.0))
            } else {
                s.suffix(unit).fixed_decimals(decimals)
            };
            let r = widgets::value(ui, s);
            widgets::resettable(r, &mut v, p.default);
        }
        Ctl::Int { min, max, unit } => {
            let mut i = v as i64;
            let r = widgets::value(ui, egui::Slider::new(&mut i, min..=max).suffix(unit));
            widgets::resettable(r, &mut i, p.default as i64);
            v = i as f64;
        }
        Ctl::Choice(options) => {
            let cur = options.get(v as usize).copied().unwrap_or_default();
            widgets::Combo::from_id_salt(id).selected_text(cur).width(ui.available_width()).show_list(ui, |ui| {
                for (i, o) in options.iter().enumerate() {
                    if ui.selectable_label(v as usize == i, *o).clicked() {
                        v = i as f64;
                    }
                }
            });
        }
        Ctl::Toggle => {
            let mut b = v >= 0.5;
            ui.checkbox(&mut b, "");
            v = b as u8 as f64;
        }
        Ctl::File(ext) => {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Browse…").clicked() {
                    let mut dlg = rfd::FileDialog::new().add_filter(p.label, ext).add_filter("All files", &["*"]);
                    if let Some(dir) = std::path::Path::new(fx.file.trim()).parent().filter(|d| d.is_dir()) {
                        dlg = dlg.set_directory(dir);
                    }
                    if let Some(f) = dlg.pick_file() {
                        fx.file = f.to_string_lossy().into_owned();
                    }
                }
                ui.add(egui::TextEdit::singleline(&mut fx.file).desired_width(ui.available_width()).hint_text("No file"));
            });
            return;
        }
    }
    if v != before {
        fx.set(p.key, v);
    }
}

/// The settings of a ready-made effect.
pub fn body(ui: &mut egui::Ui, id: impl Hash + Copy + std::fmt::Debug, fx: &mut BuiltinFx) {
    let Some(d) = builtin::def(&fx.id) else {
        widgets::warn(ui, format!("This version of FFcut does not know the effect “{}”: it is left out.", fx.id));
        return;
    };
    widgets::grid(("builtin", id)).show(ui, |ui| {
        for p in d.params {
            let l = ui.label(p.label);
            if !p.tip.is_empty() {
                l.on_hover_text(p.tip);
            }
            control(ui, egui::Id::new(("builtin-ctl", id, p.key)), d, p, fx);
            ui.end_row();
        }
    });
    if d.params.iter().any(|p| matches!(p.ctl, Ctl::File(_))) {
        let file = fx.file.trim();
        if file.is_empty() {
            widgets::hint(ui, "Choose a file: until then the effect does nothing.");
        } else if !std::path::Path::new(file).is_file() {
            widgets::warn(ui, "This file is not there: the preview and the export will fail.");
        }
    }
    let fresh = d.new_fx();
    let changed = d.params.iter().any(|p| d.value(fx, p.key) != d.value(&fresh, p.key));
    if changed && ui.small_button("Reset").on_hover_text("Every control back to its default").clicked() {
        fx.vals = fresh.vals;
    }
}
