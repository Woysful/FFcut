//! An "FFmpeg filter" effect: the searchable list it is picked from and its form, made from the options
//! ffmpeg reports for the filter. Shared by the audio and the video stacks.

use super::widgets;
use crate::catalog::{Catalog, Load};
use crate::theme::{self, size};
use eframe::egui::{self, text::LayoutJob, FontId, RichText, TextFormat};
use ffcut_core::filters::{FilterFx, FilterInfo, Media, OptInfo, OptKind};
use std::hash::Hash;

/// Width of the filter list in the "+ Add effect" menu.
const LIST_W: f32 = 360.0;
const LIST_H: f32 = 320.0;

/// A spinner with a word, or the error, while a list is not in: `None` until it is.
pub fn loaded<T>(ui: &mut egui::Ui, l: Load<T>, what: &str) -> Option<std::sync::Arc<T>> {
    match l {
        Load::Ready(v) => Some(v),
        Load::Loading => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(RichText::new(format!("Reading {what}…")).color(theme::TEXT_LABEL));
            });
            None
        }
        Load::Failed(e) => {
            widgets::error(ui, format!("Could not read {what}: {e}"));
            None
        }
    }
}

/// One row of a [`search_list`].
pub struct Row<'a> {
    pub title: &'a str,
    pub desc: &'a str,
    /// Also searched, not shown (a plugin's file, say).
    pub extra: &'a str,
}

/// A search box over a list with a fixed size, for a submenu of "+ Add effect". Returns the picked row.
pub fn search_list(ui: &mut egui::Ui, id: egui::Id, rows: &[Row], noun: &str) -> Option<usize> {
    let mut q: String = ui.data(|d| d.get_temp(id)).unwrap_or_default();
    let r = ui.add(egui::TextEdit::singleline(&mut q).hint_text(format!("Search {} {noun}…", rows.len())).desired_width(LIST_W));
    if q.is_empty() && !r.has_focus() {
        r.request_focus();
    }
    let ql = q.trim().to_lowercase();
    let low = |s: &str| s.to_lowercase();
    let mut hits: Vec<usize> =
        (0..rows.len()).filter(|&i| ql.is_empty() || [rows[i].title, rows[i].desc, rows[i].extra].iter().any(|s| low(s).contains(&ql))).collect();
    // titles that start with the query first, the rest in the order given
    hits.sort_by_key(|&i| !low(rows[i].title).starts_with(&ql));
    let mut picked = None;
    // a fixed height: the menu is then moved up to fit the window instead of shrinking the list
    egui::ScrollArea::vertical().min_scrolled_height(LIST_H).max_height(LIST_H).show(ui, |ui| {
        ui.set_width(LIST_W);
        ui.set_min_height(LIST_H);
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
        for &i in &hits {
            let row = &rows[i];
            let mut job = LayoutJob::default();
            job.append(row.title, 0.0, TextFormat { font_id: FontId::proportional(size::BODY), color: theme::TEXT_STRONG, ..Default::default() });
            job.append(row.desc, 10.0, TextFormat { font_id: FontId::proportional(size::SMALL), color: theme::TEXT_DIM, ..Default::default() });
            job.wrap = egui::text::TextWrapping::truncate_at_width(LIST_W - 16.0);
            let r = ui.button(job);
            let r = if row.desc.is_empty() { r } else { r.on_hover_text(row.desc) };
            if r.clicked() {
                picked = Some(i);
            }
        }
        if hits.is_empty() {
            widgets::hint(ui, "Nothing matches.");
        }
    });
    if picked.is_some() {
        q.clear();
    }
    ui.data_mut(|d| d.insert_temp(id, q));
    picked
}

/// The searchable list of `media` filters. Returns the picked name.
pub fn picker(ui: &mut egui::Ui, cat: &Catalog, media: Media) -> Option<String> {
    let list = loaded(ui, cat.filters_of(media), "ffmpeg's filters")?;
    let rows: Vec<Row> = list.iter().map(|f: &FilterInfo| Row { title: &f.name, desc: &f.desc, extra: "" }).collect();
    search_list(ui, egui::Id::new(("filter-search", media)), &rows, "filters").map(|i| list[i].name.clone())
}

// ------------------------------------------------------------------ the form

pub(super) fn fnum(v: f64, int: bool) -> String {
    if int {
        return format!("{}", v.round() as i64);
    }
    let s = format!("{v:.6}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" {
        "0".into()
    } else {
        s.to_string()
    }
}

fn truthy(s: &str) -> bool {
    matches!(s.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on" | "y")
}

/// `0xRRGGBB`, `#RRGGBB` (an `@alpha` is ignored) or a few common names.
fn parse_color(s: &str) -> Option<[u8; 3]> {
    let s = s.trim().split('@').next()?.trim().to_ascii_lowercase();
    let named = match s.as_str() {
        "black" => Some([0, 0, 0]),
        "white" => Some([255, 255, 255]),
        "red" => Some([255, 0, 0]),
        "green" => Some([0, 128, 0]),
        "lime" => Some([0, 255, 0]),
        "blue" => Some([0, 0, 255]),
        "yellow" => Some([255, 255, 0]),
        "cyan" => Some([0, 255, 255]),
        "magenta" => Some([255, 0, 255]),
        "gray" | "grey" => Some([128, 128, 128]),
        _ => None,
    };
    if named.is_some() {
        return named;
    }
    let hex = s.strip_prefix("0x").or_else(|| s.strip_prefix('#')).unwrap_or(&s);
    if hex.len() < 6 || !hex.is_char_boundary(6) {
        return None;
    }
    let b = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    Some([b(0)?, b(2)?, b(4)?])
}

/// Two values of option `o` mean the same.
fn same(o: &OptInfo, a: &str, b: &str) -> bool {
    match o.kind {
        OptKind::Bool => truthy(a) == truthy(b),
        OptKind::Int | OptKind::Float if o.choices.is_empty() => match (a.trim().parse::<f64>(), b.trim().parse::<f64>()) {
            (Ok(x), Ok(y)) => (x - y).abs() <= 1e-9 * x.abs().max(1.0),
            _ => a.trim() == b.trim(),
        },
        _ => a.trim() == b.trim(),
    }
}

fn opt_tip(o: &OptInfo) -> String {
    let mut s = if o.help.is_empty() { o.name.clone() } else { o.help.clone() };
    let mut facts = vec![o.type_name.clone()];
    let lim = |v: f64| if v.abs() >= 1e9 { "∞".to_string() } else { fnum(v, false) };
    if let (Some(a), Some(b)) = (o.min, o.max) {
        facts.push(format!("{} … {}", if a <= -1e9 { "-∞".to_string() } else { lim(a) }, lim(b)));
    }
    if let Some(d) = &o.default {
        facts.push(format!("default {}", if d.is_empty() { "empty" } else { d }));
    }
    s.push_str(&format!("\n{}", facts.join(" · ")));
    s
}

/// The control of one option. Returns the new value (`Some(None)` = back to the default).
fn opt_control(ui: &mut egui::Ui, id: egui::Id, o: &OptInfo, shown: &str) -> (egui::Response, Option<Option<String>>) {
    let mut out = None;
    let w = ui.available_width().max(80.0);
    let int = o.kind == OptKind::Int;
    let resp = match o.kind {
        _ if !o.choices.is_empty() && o.kind != OptKind::Flags => {
            widgets::Combo::from_id_salt(id)
                .selected_text(shown)
                .width(w)
                .height(320.0)
                .show_list(ui, |ui| {
                    for c in &o.choices {
                        let r = ui.selectable_label(shown == c.name, &c.name);
                        let r = if c.help.is_empty() { r } else { r.on_hover_text(&c.help) };
                        if r.clicked() {
                            out = Some(Some(c.name.clone()));
                        }
                    }
                })
                .response
        }
        OptKind::Flags => {
            let on: Vec<&str> = shown.split('+').map(str::trim).filter(|s| !s.is_empty()).collect();
            let label = if on.is_empty() { "none".to_string() } else { on.join("+") };
            widgets::menu_button(ui, label, |ui| {
                for c in &o.choices {
                    let mut b = on.contains(&c.name.as_str());
                    let r = ui.checkbox(&mut b, &c.name);
                    let r = if c.help.is_empty() { r } else { r.on_hover_text(&c.help) };
                    if r.changed() {
                        let next: Vec<&str> = o.choices.iter().map(|x| x.name.as_str()).filter(|n| if *n == c.name { b } else { on.contains(n) }).collect();
                        out = Some(Some(next.join("+")));
                    }
                }
            })
        }
        OptKind::Bool => {
            let mut b = truthy(shown);
            let r = ui.checkbox(&mut b, if shown.eq_ignore_ascii_case("auto") || shown == "-1" { "auto" } else { "" });
            if r.changed() {
                out = Some(Some(if b { "1" } else { "0" }.to_string()));
            }
            r
        }
        OptKind::Int | OptKind::Float if shown.trim().parse::<f64>().is_ok() || shown.trim().is_empty() => {
            let (lo, hi) = (o.min.unwrap_or(f64::MIN), o.max.unwrap_or(f64::MAX));
            let mut v = shown.trim().parse::<f64>().unwrap_or(0.0_f64.clamp(lo, hi.max(lo)));
            let span = hi - lo;
            let small = span.is_finite() && span > 0.0 && if int { span <= 64.0 } else { span <= 1000.0 };
            let r = if small {
                let s = egui::Slider::new(&mut v, lo..=hi);
                let s = if int { s.integer() } else { s.max_decimals(4) };
                widgets::value(ui, s)
            } else {
                let speed = if span.is_finite() && span < 1e5 { span / 300.0 } else { (v.abs() * 0.01).max(if int { 1.0 } else { 0.01 }) };
                let d = egui::DragValue::new(&mut v).range(lo..=hi).speed(speed);
                let d = if int { d.fixed_decimals(0) } else { d.max_decimals(6) };
                widgets::num(ui, d)
            };
            if r.changed() {
                out = Some(Some(fnum(v, int)));
            }
            r
        }
        OptKind::Color => {
            ui.horizontal(|ui| {
                if let Some(mut c) = parse_color(shown) {
                    if ui.color_edit_button_srgb(&mut c).changed() {
                        out = Some(Some(format!("0x{:02X}{:02X}{:02X}", c[0], c[1], c[2])));
                    }
                }
                text_field(ui, o, shown, &mut out)
            })
            .inner
        }
        _ => text_field(ui, o, shown, &mut out),
    };
    (resp, out)
}

fn text_field(ui: &mut egui::Ui, o: &OptInfo, shown: &str, out: &mut Option<Option<String>>) -> egui::Response {
    let mut s = shown.to_string();
    let r = ui.add(egui::TextEdit::singleline(&mut s).desired_width(ui.available_width()).hint_text(o.default.as_deref().unwrap_or("")));
    if r.changed() {
        *out = Some(if s.is_empty() { None } else { Some(s) });
    }
    r
}

/// The settings of an "FFmpeg filter" effect.
pub fn body(ui: &mut egui::Ui, id: impl Hash + Copy + std::fmt::Debug, f: &mut FilterFx, cat: &Catalog) {
    let help = match cat.help(&f.name) {
        Load::Ready(h) => Some(h),
        Load::Loading => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(RichText::new("Reading the options…").color(theme::TEXT_LABEL));
            });
            None
        }
        Load::Failed(e) => {
            widgets::error(ui, e);
            None
        }
    };
    let known = |k: &str| help.as_ref().is_some_and(|h| h.options.iter().any(|o| o.name == k));
    if let Some(h) = &help {
        if !h.desc.is_empty() {
            widgets::hint(ui, &h.desc);
        }
        if h.options.is_empty() {
            widgets::hint(ui, "This filter has no settings.");
        } else {
            widgets::grid(("filter", id)).show(ui, |ui| {
                for o in &h.options {
                    let cur = f.get(&o.name).map(str::to_string);
                    let shown = cur.clone().or_else(|| o.default.clone()).unwrap_or_default();
                    let label = RichText::new(&o.name).color(if cur.is_some() { theme::TEXT_STRONG } else { theme::TEXT_LABEL });
                    ui.label(label).on_hover_text(opt_tip(o));
                    let (resp, new) = opt_control(ui, egui::Id::new(("filter-opt", id, &o.name)), o, &shown);
                    let mut new = new;
                    if cur.is_some() {
                        egui::Popup::context_menu(&resp).style(theme::menu_style).show(|ui| {
                            if ui.button("Reset to default").clicked() {
                                new = Some(None);
                                ui.close();
                            }
                        });
                    }
                    if let Some(v) = new {
                        let v = v.filter(|v| !o.default.as_deref().is_some_and(|d| same(o, v, d)));
                        f.set(&o.name, v);
                    }
                    ui.end_row();
                }
            });
        }
    }
    // values this ffmpeg does not know (set with another version): visible, so they can be removed
    if help.is_some() || !f.opts.is_empty() {
        let unknown: Vec<(String, String)> = f.opts.iter().filter(|(k, _)| !known(k)).cloned().collect();
        for (k, v) in unknown {
            ui.horizontal(|ui| {
                let warn = if help.is_some() { "not an option of this ffmpeg" } else { "set" };
                ui.label(RichText::new(format!("{k} = {v}")).color(if help.is_some() { theme::WARN } else { theme::TEXT_LABEL })).on_hover_text(warn);
                if ui.small_button("✕").on_hover_text("Remove this value").clicked() {
                    f.set(&k, None);
                }
            });
        }
    }
    ui.horizontal(|ui| {
        ui.hyperlink_to(RichText::new("ffmpeg documentation").size(size::HINT), format!("https://ffmpeg.org/ffmpeg-filters.html#{}", f.name));
        if !f.opts.is_empty() && ui.small_button("Reset").on_hover_text("Every option back to ffmpeg's default").clicked() {
            f.opts.clear();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_and_colours() {
        assert_eq!(fnum(0.5, false), "0.5");
        assert_eq!(fnum(2.0, false), "2");
        assert_eq!(fnum(-0.0000001, false), "0");
        assert_eq!(fnum(2.6, true), "3");
        assert_eq!(parse_color("0xFF8000@0.5"), Some([255, 128, 0]));
        assert_eq!(parse_color("#00ff00"), Some([0, 255, 0]));
        assert_eq!(parse_color("Black"), Some([0, 0, 0]));
        assert_eq!(parse_color("nope"), None);
        assert_eq!(parse_color("0xé1234567"), None);
    }

    #[test]
    fn defaults_compare_by_meaning() {
        let o = |kind| OptInfo { name: "x".into(), kind, type_name: String::new(), help: String::new(), min: None, max: None, default: None, choices: vec![] };
        assert!(same(&o(OptKind::Bool), "false", "0"));
        assert!(same(&o(OptKind::Float), "1.0", "1"));
        assert!(!same(&o(OptKind::Float), "1.1", "1"));
        assert!(!same(&o(OptKind::Text), "1.0", "1"));
    }
}
