//! An "FFmpeg filter" effect: the searchable list it is picked from and its form, made from the options
//! ffmpeg reports for the filter. Shared by the audio and the video stacks.

use super::widgets;
use crate::catalog::{Catalog, Load};
use crate::theme::{self, size};
use eframe::egui::{self, text::LayoutJob, FontId, RichText, TextFormat};
use ffcut_core::filters::{FilterFx, FilterInfo, Media, OptInfo, OptKind};
use ffcut_core::plugins::{PluginFx, PluginInfo, PluginKind};
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
                ui.label(RichText::new(format!("Reading {what}…")).color(theme::text_label()));
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
    /// A favorite (filled star); `None` = the row has no star.
    pub fav: Option<bool>,
}

/// What a click on a row of a list in "+ Add effect" was for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RowAct {
    Pick,
    /// The star: add to / remove from the favorites.
    Star,
}

/// A search box over a list with a fixed size, for a submenu of "+ Add effect". Returns the clicked row.
pub fn search_list(ui: &mut egui::Ui, id: egui::Id, rows: &[Row], noun: &str) -> Option<(usize, RowAct)> {
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
    let mut clicked = None;
    // a fixed height: the menu is then moved up to fit the window instead of shrinking the list
    egui::ScrollArea::vertical().min_scrolled_height(LIST_H).max_height(LIST_H).show(ui, |ui| {
        ui.set_width(LIST_W);
        ui.set_min_height(LIST_H);
        for &i in &hits {
            if let Some(a) = row(ui, rows[i].title, rows[i].desc, rows[i].fav) {
                clicked = Some((i, a));
            }
        }
        if hits.is_empty() {
            widgets::hint(ui, "Nothing matches.");
        }
    });
    if clicked.is_some_and(|(_, a)| a == RowAct::Pick) {
        q.clear();
    }
    ui.data_mut(|d| d.insert_temp(id, q));
    clicked
}

/// Width of the star at the right of a row.
const STAR_W: f32 = 24.0;

/// One entry of a list in "+ Add effect": the title in full colour, then a dim description, cut to the
/// width. With `fav`, a star at the right end (shown while the pointer is on the row, and always for a
/// favorite) adds it to the favorites or takes it out.
fn row(ui: &mut egui::Ui, title: &str, desc: &str, fav: Option<bool>) -> Option<RowAct> {
    let pad = ui.spacing().button_padding;
    let w = ui.available_width();
    let star_w = if fav.is_some() { STAR_W } else { 0.0 };
    let mut job = LayoutJob::default();
    job.append(title, 0.0, TextFormat { font_id: FontId::proportional(size::BODY), color: theme::text_strong(), ..Default::default() });
    job.append(desc, 10.0, TextFormat { font_id: FontId::proportional(size::SMALL), color: theme::text_dim(), ..Default::default() });
    job.wrap = egui::text::TextWrapping::truncate_at_width(w - 2.0 * pad.x - star_w);
    let galley = ui.painter().layout_job(job);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, galley.size().y + 2.0 * pad.y), egui::Sense::click());
    // added after the row, so it is the one on top that takes the click
    let star = fav.map(|on| {
        (on, ui.interact(egui::Rect::from_min_max(egui::pos2(rect.right() - star_w, rect.top()), rect.max), resp.id.with("star"), egui::Sense::click()))
    });
    let hovered = resp.hovered() || star.as_ref().is_some_and(|(_, s)| s.hovered());
    if ui.is_rect_visible(rect) {
        let v = &ui.visuals().widgets.hovered;
        if hovered {
            ui.painter().rect_filled(rect, v.corner_radius, v.weak_bg_fill);
        }
        ui.painter().galley(egui::pos2(rect.left() + pad.x, rect.center().y - galley.size().y / 2.0), galley, theme::text_strong());
        if let Some((on, s)) = &star {
            if *on || hovered {
                let color = match (*on, s.hovered()) {
                    (true, _) => theme::accent(),
                    (false, true) => theme::text_strong(),
                    (false, false) => theme::text_dim(),
                };
                paint_star(ui.painter(), s.rect.center(), 6.5, *on, color);
            }
        }
    }
    let resp = if desc.is_empty() { resp } else { resp.on_hover_text(desc) };
    if let Some((on, s)) = star {
        if s.on_hover_text(if on { "Remove from the favorites" } else { "Add to the favorites: it shows at the top of this menu" }).clicked() {
            return Some(RowAct::Star);
        }
    }
    resp.clicked().then_some(RowAct::Pick)
}

/// A five-pointed star of outer radius `r`: filled, or an outline.
fn paint_star(p: &egui::Painter, c: egui::Pos2, r: f32, filled: bool, color: egui::Color32) {
    let pt = |i: usize, rad: f32| {
        let a = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.0;
        c + rad * egui::vec2(a.cos(), a.sin())
    };
    let outer: Vec<egui::Pos2> = (0..5).map(|i| pt(2 * i, r)).collect();
    let inner: Vec<egui::Pos2> = (0..5).map(|i| pt(2 * i + 1, r * 0.45)).collect();
    if filled {
        // the outline is concave, which egui does not fill: the inner pentagon and the five points
        p.add(egui::Shape::convex_polygon(inner.clone(), color, egui::Stroke::NONE));
        for i in 0..5 {
            p.add(egui::Shape::convex_polygon(vec![outer[i], inner[i], inner[(i + 4) % 5]], color, egui::Stroke::NONE));
        }
    } else {
        let path: Vec<egui::Pos2> = (0..5).flat_map(|i| [outer[i], inner[i]]).collect();
        p.add(egui::Shape::closed_line(path, egui::Stroke::new(1.2, color)));
    }
}

/// Every word of the query is in one of the texts (`words` are lower case).
fn matches(texts: &[&str], words: &[String]) -> bool {
    words.iter().all(|w| texts.iter().any(|t| t.to_lowercase().contains(w.as_str())))
}

/// A ready-made effect of the "+ Add effect" menu.
pub struct Entry<'a, T> {
    /// Names it in the favorites (stable, unlike the title).
    pub key: String,
    pub group: &'static str,
    pub title: &'static str,
    pub desc: &'static str,
    /// Also searched, not shown (the ffmpeg filters behind it, other names).
    pub keywords: &'static str,
    pub make: Box<dyn Fn() -> T + 'a>,
}

/// What was picked in the "+ Add effect" menu.
pub enum Pick<T> {
    Effect(T),
    /// An ffmpeg filter, by name.
    Filter(String),
    Plugin(PluginFx),
}

/// Height of the list of the "+ Add effect" menu.
const MENU_H: f32 = 420.0;

fn media_key(m: Media) -> &'static str {
    match m {
        Media::Audio => "audio",
        Media::Video => "video",
    }
}

/// How the favorites (`Settings::favorite_effects`) name an entry, a filter and a plugin.
fn effect_key(m: Media, key: &str) -> String {
    format!("{}:effect:{key}", media_key(m))
}
fn filter_key(m: Media, name: &str) -> String {
    format!("{}:filter:{name}", media_key(m))
}
pub(super) fn plugin_key(p: &PluginInfo) -> String {
    let m = if p.kind == PluginKind::Frei0r { Media::Video } else { Media::Audio };
    format!("{}:plugin:{}:{}:{}", media_key(m), p.kind.label(), p.id, p.label)
}

/// Adds `key` to the favorites, or takes it out.
pub(super) fn toggle_fav(favs: &mut Vec<String>, key: String) {
    match favs.iter().position(|k| *k == key) {
        Some(i) => {
            favs.remove(i);
        }
        None => favs.push(key),
    }
}

/// An entry of the "Favorites" group.
enum Fav<'a> {
    Effect(usize),
    Filter(&'a FilterInfo),
    Plugin(&'a PluginInfo),
}

/// The inside of "+ Add effect": a search field, the favorites, the ready-made effects under their
/// `groups` headings, then the ffmpeg filters of `media` that FFcut has no controls for, then the
/// `plugins` submenu. The search looks at all of them; Enter takes the first entry. A star on a row
/// moves it to the favorites at the top (`favs`, kept in the settings).
pub fn add_menu<T>(
    ui: &mut egui::Ui,
    entries: &[Entry<T>],
    groups: &[&str],
    cat: &Catalog,
    media: Media,
    plugins: &[PluginKind],
    favs: &mut Vec<String>,
) -> Option<Pick<T>> {
    let id = egui::Id::new(("add-effect", media));
    let frame = ui.ctx().cumulative_frame_nr();
    // the menu was closed in between: start with an empty search, and type into it right away
    let last: Option<u64> = ui.data(|d| d.get_temp(id.with("shown")));
    let opened = !last.is_some_and(|l| l + 1 >= frame);
    ui.data_mut(|d| d.insert_temp(id.with("shown"), frame));
    let mut q: String = if opened { String::new() } else { ui.data(|d| d.get_temp(id)).unwrap_or_default() };
    let search = ui.add(egui::TextEdit::singleline(&mut q).hint_text("Search effects and ffmpeg filters…").desired_width(LIST_W));
    if opened {
        search.request_focus();
    }
    let enter = search.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
    let words: Vec<String> = q.split_whitespace().map(str::to_lowercase).collect();
    let first = words.first().map_or("", String::as_str);
    let is_fav = |key: &str| favs.iter().any(|k| k == key);

    let entry_keys: Vec<String> = entries.iter().map(|e| effect_key(media, &e.key)).collect();
    let hits: Vec<usize> = (0..entries.len())
        .filter(|&i| !is_fav(&entry_keys[i]) && matches(&[entries[i].title, entries[i].desc, entries[i].keywords, entries[i].group], &words))
        .collect();
    let filters = cat.filters_of(media);
    let list: &[FilterInfo] = if let Load::Ready(l) = &filters { l } else { &[] };
    let mut found: Vec<&FilterInfo> = list
        .iter()
        .filter(|f| ffcut_core::filters::in_picker(&f.name) && !is_fav(&filter_key(media, &f.name)) && matches(&[&f.name, &f.desc], &words))
        .collect();
    // names that start with the query first, the rest in ffmpeg's (alphabetical) order
    found.sort_by_key(|f| !f.name.starts_with(first));
    let scan = if let Load::Ready(s) = cat.plugins() { Some(s) } else { None };
    let usable: Vec<&PluginInfo> = scan.as_ref().map(|s| s.plugins.iter().filter(|p| p.usable() && plugins.contains(&p.kind)).collect()).unwrap_or_default();

    // the favorites in the order they were starred; one whose effect, filter or plugin is gone is skipped
    let fav_rows: Vec<(Fav, String)> = favs
        .iter()
        .filter_map(|k| {
            let f = if let Some(i) = entry_keys.iter().position(|e| e == k) {
                let e = &entries[i];
                matches(&[e.title, e.desc, e.keywords, e.group], &words).then_some(Fav::Effect(i))
            } else if let Some(f) = list.iter().find(|f| filter_key(media, &f.name) == *k) {
                matches(&[&f.name, &f.desc], &words).then_some(Fav::Filter(f))
            } else {
                let p = usable.iter().find(|p| plugin_key(p) == *k)?;
                matches(&[&p.name, &p.desc, p.kind.label()], &words).then_some(Fav::Plugin(p))
            }?;
            Some((f, k.clone()))
        })
        .collect();
    let pick_fav = |f: &Fav| match f {
        Fav::Effect(i) => Pick::Effect((entries[*i].make)()),
        Fav::Filter(f) => Pick::Filter(f.name.clone()),
        Fav::Plugin(p) => Pick::Plugin(p.new_fx()),
    };

    let mut picked: Option<Pick<T>> = None;
    let mut star: Option<String> = None;
    if enter {
        picked = match (fav_rows.first(), hits.first(), found.first()) {
            (Some((f, _)), _, _) => Some(pick_fav(f)),
            (None, Some(&i), _) => Some(Pick::Effect((entries[i].make)())),
            (None, None, Some(f)) => Some(Pick::Filter(f.name.clone())),
            _ => None,
        };
    }
    // a fixed height: the menu is then moved up to fit the window instead of shrinking the list
    egui::ScrollArea::vertical().id_salt(id.with("list")).min_scrolled_height(MENU_H).max_height(MENU_H).show(ui, |ui| {
        ui.set_width(LIST_W);
        ui.set_min_height(MENU_H);
        if !fav_rows.is_empty() {
            widgets::menu_indent(ui, |ui| widgets::caption(ui, "Favorites"));
            for (f, key) in &fav_rows {
                let (title, desc): (&str, String) = match f {
                    Fav::Effect(i) => (entries[*i].title, entries[*i].desc.to_string()),
                    Fav::Filter(f) => (&f.name, format!("ffmpeg filter · {}", f.desc)),
                    Fav::Plugin(p) if p.desc.is_empty() => (&p.name, p.kind.label().to_string()),
                    Fav::Plugin(p) => (&p.name, format!("{} · {}", p.kind.label(), p.desc)),
                };
                match row(ui, title, &desc, Some(true)) {
                    Some(RowAct::Pick) => picked = Some(pick_fav(f)),
                    Some(RowAct::Star) => star = Some(key.clone()),
                    None => {}
                }
            }
            ui.add_space(4.0);
        }
        for g in groups {
            let in_group: Vec<usize> = hits.iter().copied().filter(|&i| entries[i].group == *g).collect();
            if in_group.is_empty() {
                continue;
            }
            widgets::menu_indent(ui, |ui| widgets::caption(ui, g));
            for i in in_group {
                match row(ui, entries[i].title, entries[i].desc, Some(false)) {
                    Some(RowAct::Pick) => picked = Some(Pick::Effect((entries[i].make)())),
                    Some(RowAct::Star) => star = Some(entry_keys[i].clone()),
                    None => {}
                }
            }
            ui.add_space(4.0);
        }
        match &filters {
            Load::Ready(_) if !found.is_empty() => {
                widgets::menu_indent(ui, |ui| widgets::caption(ui, &format!("FFmpeg filters · {}", found.len())))
                    .on_hover_text("Any other filter of your ffmpeg, with a form made from the options it reports");
                for f in &found {
                    match row(ui, &f.name, &f.desc, Some(false)) {
                        Some(RowAct::Pick) => picked = Some(Pick::Filter(f.name.clone())),
                        Some(RowAct::Star) => star = Some(filter_key(media, &f.name)),
                        None => {}
                    }
                }
            }
            Load::Ready(_) => {}
            l => {
                loaded(ui, l.clone(), "ffmpeg's filters");
            }
        }
        if fav_rows.is_empty() && hits.is_empty() && found.is_empty() && matches!(filters, Load::Ready(_)) {
            widgets::hint(ui, "Nothing matches.");
        }
    });
    ui.separator();
    let tip = if plugins.contains(&PluginKind::Frei0r) {
        "A frei0r plugin installed on this computer (ffmpeg runs it)"
    } else {
        "A LADSPA or LV2 plugin installed on this computer (ffmpeg runs it)"
    };
    widgets::form_submenu(ui, "Plugin", |ui| {
        if let Some(p) = super::plugin_ui::picker(ui, cat, plugins, favs) {
            picked = Some(Pick::Plugin(p));
        }
    })
    .on_hover_text(tip);
    if let Some(k) = star {
        toggle_fav(favs, k);
    }
    if picked.is_some() {
        q.clear();
    }
    ui.data_mut(|d| d.insert_temp(id, q));
    picked
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
                ui.label(RichText::new("Reading the options…").color(theme::text_label()));
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
                    let label = RichText::new(&o.name).color(if cur.is_some() { theme::text_strong() } else { theme::text_label() });
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
                ui.label(RichText::new(format!("{k} = {v}")).color(if help.is_some() { theme::warn() } else { theme::text_label() })).on_hover_text(warn);
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
