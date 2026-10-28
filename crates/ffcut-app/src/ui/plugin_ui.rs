//! A plugin effect (LADSPA, LV2, frei0r): the searchable list it is picked from and its form, made from the
//! controls the plugin declares. Shared by the audio and the video stacks.

use super::filter_ui::{fnum, loaded, search_list, Row};
use super::widgets;
use crate::catalog::{Catalog, Load};
use crate::theme::{self, size};
use eframe::egui::{self, RichText};
use ffcut_core::plugins::{same_value, Control, CtlKind, PluginFx, PluginInfo, PluginKind};
use std::hash::Hash;

/// The searchable list of the usable plugins of these kinds. Returns the new effect.
pub fn picker(ui: &mut egui::Ui, cat: &Catalog, kinds: &[PluginKind]) -> Option<PluginFx> {
    let scan = loaded(ui, cat.plugins(), "the plugins")?;
    for k in kinds.iter().filter(|k| cat.lacks_host(**k) == Some(true)) {
        widgets::warn(ui, format!("Your ffmpeg was built without {} support: these plugins cannot run.", k.label()));
    }
    let list: Vec<&PluginInfo> = scan.plugins.iter().filter(|p| p.usable() && kinds.contains(&p.kind)).collect();
    let names: Vec<String> = kinds.iter().map(|k| k.label().to_string()).collect();
    if list.is_empty() {
        let what = names.join(" or ");
        let install = if kinds.contains(&PluginKind::Frei0r) { "frei0r-plugins" } else { "swh-plugins, x42-plugins or lsp-plugins" };
        widgets::hint(ui, format!("No {what} plugins found. Install some (for example {install}) or add their folder in Settings → Plugins."));
        return None;
    }
    let descs: Vec<String> = list
        .iter()
        .map(|p| match (kinds.len() > 1, p.desc.is_empty()) {
            (true, true) => p.kind.label().to_string(),
            (true, false) => format!("{} · {}", p.kind.label(), p.desc),
            (false, _) => p.desc.clone(),
        })
        .collect();
    let extras: Vec<String> = list.iter().map(|p| format!("{} {} {}", p.id, p.label, p.file.display())).collect();
    let rows: Vec<Row> = list.iter().enumerate().map(|(i, p)| Row { title: &p.name, desc: &descs[i], extra: &extras[i] }).collect();
    let picked = search_list(ui, egui::Id::new(("plugin-search", kinds)), &rows, "plugins");
    picked.map(|i| list[i].new_fx())
}

fn ctl_tip(p: &PluginInfo, c: &Control) -> String {
    let mut s = if c.help.is_empty() { c.name.clone() } else { format!("{}\n{}", c.name, c.help) };
    let mut facts = vec![format!("“{}”", c.key)];
    if let (Some(a), Some(b)) = (c.min, c.max) {
        facts.push(format!("{} … {}", fnum(a, false), fnum(b, false)));
    }
    match &c.default {
        Some(d) => facts.push(format!("default {}", if d.is_empty() { "empty" } else { d })),
        None => facts.push("no default".into()),
    }
    s.push_str(&format!("\n{}", facts.join(" · ")));
    if c.sample_rate {
        s.push_str("\nThe plugin reads this as a fraction of the sample rate.");
    }
    if p.kind == PluginKind::Frei0r && c.kind == CtlKind::Float {
        s.push_str("\nfrei0r values usually go from 0 to 1; type a number to go beyond.");
    }
    s
}

fn floats(s: &str) -> Vec<f64> {
    s.split('/').map(|v| v.trim().parse::<f64>().unwrap_or(0.0)).collect()
}

/// The control of one plugin parameter. Returns the new value.
fn control(ui: &mut egui::Ui, id: egui::Id, kind: PluginKind, c: &Control, shown: &str) -> (egui::Response, Option<String>) {
    let mut out = None;
    let int = c.kind == CtlKind::Int;
    let resp = match c.kind {
        CtlKind::Toggle => {
            let mut b = kind.toggle_value(shown);
            let r = ui.checkbox(&mut b, "");
            if r.changed() {
                out = Some(kind.toggle_text(b).to_string());
            }
            r
        }
        CtlKind::Choice => {
            let label = c.choices.iter().find(|(v, _)| same_value(v, shown)).map_or(shown, |(_, l)| l.as_str());
            egui::ComboBox::from_id_salt(id)
                .selected_text(label)
                .width(ui.available_width().max(80.0))
                .height(320.0)
                .show_ui(ui, |ui| {
                    for (v, l) in &c.choices {
                        if ui.selectable_label(same_value(v, shown), if l.is_empty() { v } else { l }).clicked() {
                            out = Some(v.clone());
                        }
                    }
                })
                .response
        }
        CtlKind::Color => {
            let v = floats(shown);
            let mut rgb = [0, 1, 2].map(|i| v.get(i).copied().unwrap_or(0.0).clamp(0.0, 1.0) as f32);
            let r = ui.color_edit_button_rgb(&mut rgb);
            if r.changed() {
                out = Some(rgb.iter().map(|x| fnum(*x as f64, false)).collect::<Vec<_>>().join("/"));
            }
            r
        }
        CtlKind::Position => {
            let v = floats(shown);
            let (mut x, mut y) = (v.first().copied().unwrap_or(0.0), v.get(1).copied().unwrap_or(0.0));
            let r = ui.horizontal(|ui| {
                let a = widgets::num(ui, egui::DragValue::new(&mut x).speed(0.005).max_decimals(4).prefix("x "));
                let b = widgets::num(ui, egui::DragValue::new(&mut y).speed(0.005).max_decimals(4).prefix("y "));
                if a.changed() || b.changed() {
                    out = Some(format!("{}/{}", fnum(x, false), fnum(y, false)));
                }
                a | b
            });
            r.inner
        }
        CtlKind::Text => {
            let mut s = shown.to_string();
            let r = ui.add(egui::TextEdit::singleline(&mut s).desired_width(ui.available_width()));
            if r.changed() {
                out = Some(s);
            }
            r
        }
        CtlKind::Float | CtlKind::Int => {
            let (lo, hi) = (c.min.unwrap_or(f64::MIN), c.max.unwrap_or(f64::MAX));
            let mut v = shown.trim().parse::<f64>().unwrap_or(0.0_f64.clamp(lo, hi.max(lo)));
            let span = hi - lo;
            let r = if c.min.is_some() && c.max.is_some() && span > 0.0 && span.is_finite() {
                let s = egui::Slider::new(&mut v, lo..=hi).clamping(if c.strict { egui::SliderClamping::Always } else { egui::SliderClamping::Never });
                let s = if int { s.integer() } else { s.max_decimals(4) };
                let s = if c.log && lo > 0.0 { s.logarithmic(true) } else { s };
                widgets::value(ui, s)
            } else {
                let speed = (v.abs() * 0.01).max(if int { 1.0 } else { 0.01 });
                let d = egui::DragValue::new(&mut v).range(lo..=hi).speed(speed);
                widgets::num(ui, if int { d.fixed_decimals(0) } else { d.max_decimals(6) })
            };
            if r.changed() {
                out = Some(fnum(v, int));
            }
            r
        }
    };
    (resp, out)
}

/// What a plugin's channels do to a track.
fn channels_note(p: &PluginInfo) -> Option<String> {
    match (p.kind, p.inputs, p.outputs) {
        (PluginKind::Frei0r, ..) | (PluginKind::Ladspa, 1, 1) => None,
        (PluginKind::Lv2, 1, 1) => Some("A mono plugin: ffmpeg mixes the track down to mono for it.".into()),
        (_, 2, 2) => Some("A stereo plugin: a mono track becomes stereo.".into()),
        (_, i, o) => Some(format!("Takes {i} channel(s) and gives {o}: ffmpeg converts the track to fit.")),
    }
}

/// The settings of a plugin effect.
pub fn body(ui: &mut egui::Ui, id: impl Hash + Copy + std::fmt::Debug, fx: &mut PluginFx, cat: &Catalog) {
    let info = match cat.plugin(fx) {
        Load::Ready(i) => (*i).clone(),
        Load::Loading => {
            loaded(ui, Load::<()>::Loading, "the plugins");
            return;
        }
        Load::Failed(e) => {
            widgets::error(ui, format!("Could not read the plugins: {e}"));
            None
        }
    };
    let Some(p) = info else {
        widgets::warn(ui, format!("This {} plugin is not installed here, or its folder is not in Settings → Plugins: the export will fail.", fx.kind.label()));
        widgets::hint(ui, &fx.id);
        for (k, v) in fx.params.clone() {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{k} = {v}")).color(theme::TEXT_LABEL));
                if ui.small_button("✕").on_hover_text("Remove this value").clicked() {
                    fx.set(&k, None);
                }
            });
        }
        return;
    };
    if !p.desc.is_empty() {
        widgets::hint(ui, &p.desc);
    }
    if cat.lacks_host(p.kind) == Some(true) {
        widgets::warn(ui, format!("Your ffmpeg was built without {} support: the export will fail.", p.kind.label()));
    }
    if let Some(why) = &p.problem {
        widgets::warn(ui, format!("ffmpeg cannot use this plugin as an effect: it {why}."));
    }
    if let Some(n) = channels_note(&p) {
        widgets::hint(ui, n);
    }
    if p.controls.is_empty() {
        widgets::hint(ui, "This plugin has no settings.");
    } else {
        widgets::grid(("plugin", id)).show(ui, |ui| {
            for c in &p.controls {
                let set = fx.get(&c.key).is_some();
                let label = RichText::new(&c.name).color(if set { theme::TEXT_STRONG } else { theme::TEXT_LABEL });
                ui.add(egui::Label::new(label).truncate()).on_hover_text(ctl_tip(&p, c));
                let shown = p.value(fx, c);
                let (resp, new) = control(ui, egui::Id::new(("plugin-ctl", id, &c.key)), p.kind, c, &shown);
                let mut new = new.map(Some);
                if set && c.default.is_some() {
                    egui::Popup::context_menu(&resp).style(theme::menu_style).show(|ui| {
                        if ui.button("Reset to default").clicked() {
                            new = Some(None);
                            ui.close();
                        }
                    });
                }
                if let Some(v) = new {
                    p.set(fx, &c.key, v);
                }
                ui.end_row();
            }
        });
    }
    let unknown: Vec<(String, String)> = fx.params.iter().filter(|(k, _)| p.control(k).is_none()).cloned().collect();
    for (k, v) in unknown {
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{k} = {v}")).color(theme::WARN)).on_hover_text("not a control of this plugin");
            if ui.small_button("✕").on_hover_text("Remove this value").clicked() {
                fx.set(&k, None);
            }
        });
    }
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("{} · {}", p.kind.label(), p.file.display())).size(size::HINT).color(theme::TEXT_DIM)).on_hover_text(&p.id);
        if fx.params != p.new_fx().params && ui.small_button("Reset").on_hover_text("Every control back to the plugin's default").clicked() {
            p.reset(fx);
        }
    });
}
