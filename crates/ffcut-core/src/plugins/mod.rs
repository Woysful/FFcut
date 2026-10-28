//! LADSPA and LV2 (audio) and frei0r (video) plugins, hosted by ffmpeg's `ladspa`, `lv2` and `frei0r` filters:
//! where they are installed, what they are called and which controls they have, and the filter text of a
//! [`PluginFx`].
//!
//! LV2 plugins describe themselves in Turtle files, read here. LADSPA and frei0r plugins only tell
//! about themselves when loaded, so their libraries are opened in a child process (`--probe-plugins`, see
//! [`probe_main`]): a plugin that crashes takes down the probe, never the editor.
//!
//! ffmpeg finds frei0r and LV2 plugins by itself, in folders given by `FREI0R_PATH` and `LV2_PATH`.
//! [`Dirs::env`] sets those to the folders listed here, so ffmpeg (and the player) see the same plugins.
//! VST, VST3, CLAP and OFX plugins cannot be hosted by ffmpeg.

mod lv2;
mod native;
pub mod ttl;

use crate::filters::opt_value;
use crate::fmtnum::num;
pub use crate::model::plugin::{PluginFx, PluginKind};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::mpsc::{channel, RecvTimeoutError};
use std::sync::OnceLock;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CtlKind {
    Float,
    Int,
    /// On / off.
    Toggle,
    /// One of `choices` (LV2 enumerations).
    Choice,
    /// frei0r: `r/g/b`, each 0..1.
    Color,
    /// frei0r: `x/y`.
    Position,
    Text,
}

/// One input control of a plugin.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Control {
    /// How the filter text names it: `c0` (LADSPA), the port symbol (LV2), the parameter number (frei0r).
    pub key: String,
    pub name: String,
    #[serde(default)]
    pub help: String,
    pub kind: CtlKind,
    pub min: Option<f64>,
    pub max: Option<f64>,
    /// The value the plugin starts with, as filter text; `None` = it has none, a value must be given.
    pub default: Option<String>,
    /// Values outside `min`..`max` are refused (LADSPA); otherwise the range is only a guide (frei0r).
    #[serde(default)]
    pub strict: bool,
    #[serde(default)]
    pub log: bool,
    /// LADSPA: the range is a fraction of the sample rate (ffmpeg passes the value as it is).
    #[serde(default)]
    pub sample_rate: bool,
    /// (value, label) for `Choice`.
    #[serde(default)]
    pub choices: Vec<(String, String)>,
}

/// One installed plugin.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginInfo {
    pub kind: PluginKind,
    /// As in [`PluginFx::id`].
    pub id: String,
    #[serde(default)]
    pub label: String,
    pub name: String,
    /// Maker, explanation, or both.
    #[serde(default)]
    pub desc: String,
    /// The library (LADSPA, frei0r) or the bundle folder (LV2).
    pub file: PathBuf,
    /// Audio inputs and outputs (frei0r: 1 and 1).
    pub inputs: u32,
    pub outputs: u32,
    pub controls: Vec<Control>,
    /// Why it cannot be used as an effect (a generator, an instrument, a feature ffmpeg lacks...).
    #[serde(default)]
    pub problem: Option<String>,
}

impl PluginKind {
    /// How a [`CtlKind::Toggle`] is written.
    pub fn toggle_text(self, on: bool) -> &'static str {
        match (self, on) {
            (PluginKind::Frei0r, true) => "y",
            (PluginKind::Frei0r, false) => "n",
            (_, true) => "1",
            (_, false) => "0",
        }
    }
    pub fn toggle_value(self, s: &str) -> bool {
        match self {
            PluginKind::Frei0r => s.trim() == "y",
            _ => s.trim().parse::<f64>().is_ok_and(|v| v >= 0.5),
        }
    }
}

impl Control {
    /// Something to start with when the plugin has no default: the low end of the range, or 0 inside it.
    pub fn fallback(&self) -> String {
        match self.kind {
            CtlKind::Choice => self.choices.first().map(|c| c.0.clone()).unwrap_or_else(|| "0".into()),
            CtlKind::Color => "0/0/0".into(),
            CtlKind::Position => "0/0".into(),
            CtlKind::Text => String::new(),
            _ => num(0.0_f64.max(self.min.unwrap_or(f64::MIN)).min(self.max.unwrap_or(f64::MAX)), 6),
        }
    }
}

impl PluginInfo {
    /// The effect, with a value for every control that has no default (ffmpeg refuses to start without).
    pub fn new_fx(&self) -> PluginFx {
        let params = self.controls.iter().filter(|c| c.default.is_none()).map(|c| (c.key.clone(), c.fallback())).collect();
        PluginFx { kind: self.kind, id: self.id.clone(), label: self.label.clone(), name: self.name.clone(), params }
    }
    pub fn is(&self, fx: &PluginFx) -> bool {
        self.kind == fx.kind && self.id == fx.id && self.label == fx.label
    }
    pub fn control(&self, key: &str) -> Option<&Control> {
        self.controls.iter().find(|c| c.key == key)
    }
    pub fn usable(&self) -> bool {
        self.problem.is_none()
    }

    /// What control `c` of `fx` is at: the value set, else the default, else what [`Self::new_fx`] would set.
    pub fn value(&self, fx: &PluginFx, c: &Control) -> String {
        fx.get(&c.key).map(str::to_string).or_else(|| c.default.clone()).unwrap_or_else(|| c.fallback())
    }

    /// Sets control `key` of `fx` (`None` = back to its default) so that the filter text stays right:
    /// a value equal to the default is not written, a control without a default keeps a value, and frei0r
    /// values (which ffmpeg takes by position) are written out up to the last one that differs.
    pub fn set(&self, fx: &mut PluginFx, key: &str, value: Option<String>) {
        let Some(c) = self.control(key) else {
            fx.set(key, value);
            return;
        };
        let is_default = |v: &str| c.default.as_deref().is_some_and(|d| same_value(v, d));
        let value = match value {
            None if c.default.is_none() => Some(c.fallback()),
            Some(v) if is_default(&v) => None,
            v => v,
        };
        fx.set(key, value);
        if self.kind == PluginKind::Frei0r {
            let differs = |c: &Control| fx.get(&c.key).is_some_and(|v| !c.default.as_deref().is_some_and(|d| same_value(v, d)));
            let last = self.controls.iter().rposition(differs);
            fx.params = match last {
                Some(l) => self.controls[..=l].iter().map(|c| (c.key.clone(), self.value(fx, c))).collect(),
                None => Vec::new(),
            };
        }
    }

    /// Every control back to its default.
    pub fn reset(&self, fx: &mut PluginFx) {
        fx.params = self.new_fx().params;
    }
}

/// Two values of a control mean the same (`1` and `1.0`).
pub fn same_value(a: &str, b: &str) -> bool {
    match (a.trim().parse::<f64>(), b.trim().parse::<f64>()) {
        (Ok(x), Ok(y)) => (x - y).abs() <= 1e-9 * x.abs().max(1.0),
        _ => a.trim() == b.trim(),
    }
}

// ------------------------------------------------------------------ filter text

fn is_number(s: &str) -> bool {
    s.trim().parse::<f64>().is_ok_and(f64::is_finite)
}

fn is_symbol(s: &str) -> bool {
    let mut c = s.chars();
    c.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_') && c.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// A frei0r parameter as `av_get_token` reads it between `|`.
fn frei0r_token(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if matches!(c, '\\' | '\'' | '|' | ' ' | '\t' | '\n') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// ffmpeg opens a LADSPA library as given only when its name starts with `/` or `.` (otherwise it looks in
/// its own folders). On Windows a path with a drive becomes a device path, `//./C:/...`, which starts with one.
fn ladspa_file(path: &str, windows: bool) -> String {
    if !windows {
        return path.to_string();
    }
    let p = path.replace('\\', "/");
    let b = p.as_bytes();
    if b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':' {
        format!("//./{p}")
    } else {
        p
    }
}

/// `ladspa=...`, `lv2=...` or `frei0r=...`; `None` when the effect cannot be written (no plugin set).
/// Values that are not what the control takes are left out, so the plugin keeps its default.
pub fn plugin_text(p: &PluginFx) -> Option<String> {
    if p.id.trim().is_empty() {
        return None;
    }
    let ctl = |valid: &dyn Fn(&str) -> bool| -> Vec<String> {
        p.params.iter().filter(|(k, v)| valid(k) && is_number(v)).map(|(k, v)| format!("{k}={}", v.trim())).collect()
    };
    let (head, controls) = match p.kind {
        PluginKind::Ladspa => {
            let label = if p.label.is_empty() { String::new() } else { format!(":plugin={}", opt_value(&p.label)) };
            let valid = |k: &str| k.strip_prefix('c').is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
            (format!("ladspa=file={}{label}", opt_value(&ladspa_file(&p.id, cfg!(windows)))), ctl(&valid))
        }
        PluginKind::Lv2 => (format!("lv2=plugin={}", opt_value(&p.id)), ctl(&is_symbol)),
        PluginKind::Frei0r => {
            if p.id.contains(['/', '\\', ':']) {
                return None;
            }
            // positional: the values up to the last one set (the editor fills the ones before it)
            let mut vals = Vec::new();
            for i in 0.. {
                let Some(v) = p.get(&i.to_string()) else { break };
                vals.push(frei0r_token(v));
            }
            let head = format!("frei0r=filter_name={}", opt_value(&p.id));
            return Some(if vals.is_empty() { head } else { format!("{head}:filter_params={}", opt_value(&vals.join("|"))) });
        }
    };
    let text = if controls.is_empty() { head } else { format!("{head}:controls={}", opt_value(&controls.join("|"))) };
    Some(format!("{text},{NAMED_LAYOUT}"))
}

/// ffmpeg's ladspa and lv2 filters give their output a layout of "N channels" with no named order, which
/// encoders such as AAC refuse. aformat alone only relabels the link, not the frames passing through it
/// (the next filter then fails with "Channel layout change is not supported"), so aresample writes them
/// out again in the named layout with as many channels.
const NAMED_LAYOUT: &str = "aresample,aformat=channel_layouts=mono|stereo|3.0|quad|5.0|5.1|6.1|7.1";

// ------------------------------------------------------------------ where plugins are

/// Library file names: what ffmpeg adds to a frei0r name.
pub const LIB_EXT: &str = std::env::consts::DLL_EXTENSION;

/// The environment as it was when FFcut started (before [`Dirs::env`] was applied to it).
fn inherited(var: &str) -> Option<OsString> {
    static FIRST: OnceLock<HashMap<&'static str, Option<OsString>>> = OnceLock::new();
    FIRST.get_or_init(|| ["LADSPA_PATH", "LV2_PATH", "FREI0R_PATH"].into_iter().map(|v| (v, std::env::var_os(v))).collect()).get(var).cloned().flatten()
}

fn env_list(var: &str) -> Vec<PathBuf> {
    inherited(var).map(|v| std::env::split_paths(&v).filter(|p| !p.as_os_str().is_empty()).collect()).unwrap_or_default()
}

/// The folders searched, in the order ffmpeg searches them (a frei0r name found twice is the first one).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Dirs {
    pub ladspa: Vec<PathBuf>,
    pub lv2: Vec<PathBuf>,
    pub frei0r: Vec<PathBuf>,
}

fn push_unique(v: &mut Vec<PathBuf>, p: PathBuf) {
    if !v.contains(&p) {
        v.push(p);
    }
}

impl Dirs {
    /// The user's folders (each with its subfolders, so one folder can hold every kind), then the usual ones.
    pub fn new(user: &[PathBuf]) -> Self {
        let mut mine = Vec::new();
        for d in user.iter().filter(|d| d.is_dir()) {
            push_unique(&mut mine, d.clone());
            let mut subs: Vec<PathBuf> =
                std::fs::read_dir(d).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.is_dir() && !p.join("manifest.ttl").is_file()).collect();
            subs.sort();
            for s in subs {
                push_unique(&mut mine, s);
            }
        }
        let home = dirs_home();
        let mut d = Dirs { ladspa: mine.clone(), lv2: mine.clone(), frei0r: mine };
        let add = |v: &mut Vec<PathBuf>, list: Vec<PathBuf>| list.into_iter().for_each(|p| push_unique(v, p));
        add(&mut d.ladspa, env_list("LADSPA_PATH"));
        add(&mut d.frei0r, env_list("FREI0R_PATH"));
        let lv2_env = env_list("LV2_PATH");
        if cfg!(windows) {
            let var = |v: &str| std::env::var_os(v).map(PathBuf::from);
            if lv2_env.is_empty() {
                add(&mut d.lv2, [var("APPDATA").map(|p| p.join("LV2")), var("COMMONPROGRAMFILES").map(|p| p.join("LV2"))].into_iter().flatten().collect());
            }
        } else {
            let h = |s: &str| home.as_ref().map(|h| h.join(s));
            add(&mut d.ladspa, [h(".ladspa"), h(".ladspa/lib")].into_iter().flatten().collect());
            add(&mut d.ladspa, ["/usr/local/lib/ladspa", "/usr/lib/ladspa", "/usr/local/lib64/ladspa", "/usr/lib64/ladspa"].map(PathBuf::from).to_vec());
            add(&mut d.frei0r, h(".frei0r-1/lib").into_iter().collect());
            add(
                &mut d.frei0r,
                ["/usr/local/lib/frei0r-1", "/usr/lib/frei0r-1", "/usr/local/lib64/frei0r-1", "/usr/lib64/frei0r-1"].map(PathBuf::from).to_vec(),
            );
            if lv2_env.is_empty() {
                if cfg!(target_os = "macos") {
                    add(&mut d.lv2, [h("Library/Audio/Plug-Ins/LV2"), h(".lv2")].into_iter().flatten().collect());
                    add(&mut d.lv2, ["/Library/Audio/Plug-Ins/LV2", "/usr/local/lib/lv2", "/usr/lib/lv2"].map(PathBuf::from).to_vec());
                } else {
                    add(&mut d.lv2, h(".lv2").into_iter().collect());
                    add(&mut d.lv2, ["/usr/local/lib/lv2", "/usr/lib/lv2"].map(PathBuf::from).to_vec());
                }
            }
        }
        add(&mut d.lv2, lv2_env);
        d
    }

    /// `LV2_PATH` and `FREI0R_PATH` for ffmpeg and the player: these folders, in this order.
    pub fn env(&self) -> Vec<(&'static str, OsString)> {
        let join = |v: &[PathBuf]| std::env::join_paths(v).unwrap_or_default();
        vec![("LV2_PATH", join(&self.lv2)), ("FREI0R_PATH", join(&self.frei0r))]
    }

    /// Gives every ffmpeg started from now on (see `run::command`) these folders.
    pub fn apply_to_children(&self) {
        crate::run::set_child_env(self.env());
    }
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).filter(|h| !h.is_empty()).map(PathBuf::from)
}

/// Library files directly in `dir`, sorted.
fn libs_in(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|e| e.eq_ignore_ascii_case(LIB_EXT)))
        .collect();
    v.sort();
    v
}

// ------------------------------------------------------------------ scanning

/// What a scan found.
#[derive(Debug, Clone, Default)]
pub struct Scan {
    /// Sorted by kind, then name.
    pub plugins: Vec<PluginInfo>,
    /// Files and bundles that could not be read, with the reason.
    pub errors: Vec<(PathBuf, String)>,
}

/// Opens the library at `path` in this process and reads its LADSPA or frei0r plugins (none: not a plugin).
/// This runs the library's code: the editor goes through [`scan`], which does it in a child process.
pub use native::probe_file;

/// Finds every plugin of `dirs`. LADSPA and frei0r libraries are opened by `exe --probe-plugins`.
pub fn scan(exe: &Path, dirs: &Dirs) -> Scan {
    let mut out = lv2_plugins(dirs);
    scan_libs(exe, dirs, &mut out);
    out.plugins.sort_by_cached_key(|p| (p.kind as u8, p.name.to_lowercase()));
    out
}

/// The LV2 plugins of `dirs` (read from their bundles, nothing is loaded).
pub fn lv2_plugins(dirs: &Dirs) -> Scan {
    let mut out = Scan::default();
    let mut uris = HashSet::new();
    for d in &dirs.lv2 {
        let mut bundles: Vec<PathBuf> =
            std::fs::read_dir(d).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.join("manifest.ttl").is_file()).collect();
        bundles.sort();
        for b in bundles {
            match lv2::read_bundle(&b) {
                Ok(list) => out.plugins.extend(list.into_iter().filter(|p| uris.insert(p.id.clone()))),
                Err(e) => out.errors.push((b, e)),
            }
        }
    }
    out
}

/// The LADSPA and frei0r part of [`scan`].
fn scan_libs(exe: &Path, dirs: &Dirs, out: &mut Scan) {
    let mut files = Vec::new();
    for d in dirs.ladspa.iter().chain(&dirs.frei0r) {
        for f in libs_in(d) {
            let key = f.canonicalize().unwrap_or_else(|_| f.clone());
            if !files.iter().any(|(k, _): &(PathBuf, PathBuf)| *k == key) {
                files.push((key, f));
            }
        }
    }
    let files: Vec<PathBuf> = files.into_iter().map(|(_, f)| f).collect();
    let (found, errors) = probe_in_child(exe, &files);
    out.errors.extend(errors);
    // a frei0r name is looked up in the frei0r folders, in order: the first library of that name wins
    let rank = |p: &PluginInfo| dirs.frei0r.iter().position(|d| p.file.parent() == Some(d.as_path())).unwrap_or(usize::MAX);
    let mut found = found;
    found.sort_by_key(|p| (p.kind != PluginKind::Frei0r, rank(p)));
    let mut names = HashSet::new();
    for p in found {
        if p.kind == PluginKind::Frei0r && (rank(&p) == usize::MAX || !names.insert(p.id.clone())) {
            continue;
        }
        out.plugins.push(p);
    }
}

/// Lines of the probe start with this, so that whatever a plugin prints is told apart.
const MARK: &str = "\u{1}ffcut\t";
/// A library that takes longer than this to load is given up.
const PROBE_TIMEOUT: Duration = Duration::from_secs(20);

/// The `--probe-plugins FILE...` mode: opens each library and prints what is in it, one line per file.
pub fn probe_main(files: &[OsString]) -> i32 {
    let out = std::io::stdout();
    for (i, f) in files.iter().enumerate() {
        let say = |s: String| {
            let mut o = out.lock();
            let _ = writeln!(o, "{MARK}{s}");
            let _ = o.flush();
        };
        say(format!("begin\t{i}"));
        match native::probe_file(Path::new(f)) {
            Ok(list) => say(format!("done\t{i}\t{}", serde_json::to_string(&list).unwrap_or_else(|_| "[]".into()))),
            Err(e) => say(format!("fail\t{i}\t{}", e.replace(['\n', '\t'], " "))),
        }
    }
    0
}

enum Line {
    Begin(usize),
    Done(usize, Vec<PluginInfo>),
    Fail(usize, String),
}

fn parse_line(l: &str) -> Option<Line> {
    let mut it = l.strip_prefix(MARK)?.splitn(3, '\t');
    let (what, i) = (it.next()?, it.next()?.parse().ok()?);
    Some(match what {
        "begin" => Line::Begin(i),
        "done" => Line::Done(i, serde_json::from_str(it.next()?).ok()?),
        "fail" => Line::Fail(i, it.next().unwrap_or("").to_string()),
        _ => return None,
    })
}

/// Runs the probe over `files`; a library that crashes or hangs it is reported and the probe goes on after it.
fn probe_in_child(exe: &Path, files: &[PathBuf]) -> (Vec<PluginInfo>, Vec<(PathBuf, String)>) {
    let (mut found, mut errors) = (Vec::new(), Vec::new());
    let mut from = 0;
    while from < files.len() {
        let rest = &files[from..];
        let child = crate::run::command(exe).arg("--probe-plugins").args(rest).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn();
        let mut child: Child = match child {
            Ok(c) => c,
            Err(e) => {
                errors.extend(rest.iter().map(|f| (f.clone(), format!("cannot start the plugin probe: {e}"))));
                break;
            }
        };
        let (tx, rx) = channel();
        let stdout = child.stdout.take().expect("piped");
        std::thread::spawn(move || {
            for l in BufReader::new(stdout).lines().map_while(Result::ok) {
                if let Some(l) = parse_line(&l) {
                    if tx.send(l).is_err() {
                        break;
                    }
                }
            }
        });
        let (mut current, mut next) = (None, 0);
        let lost = loop {
            match rx.recv_timeout(PROBE_TIMEOUT) {
                Ok(Line::Begin(i)) => current = Some(i),
                Ok(Line::Done(i, list)) => {
                    found.extend(list);
                    (current, next) = (None, i + 1);
                }
                Ok(Line::Fail(i, e)) => {
                    errors.push((rest[i.min(rest.len() - 1)].clone(), e));
                    (current, next) = (None, i + 1);
                }
                Err(RecvTimeoutError::Timeout) => {
                    let _ = child.kill();
                    break current.map(|i| (i, "did not finish loading"));
                }
                Err(RecvTimeoutError::Disconnected) => break current.map(|i| (i, "crashed while loading")),
            }
        };
        let _ = child.wait();
        match lost {
            Some((i, why)) if i < rest.len() => {
                errors.push((rest[i].clone(), why.to_string()));
                from += i + 1;
            }
            // ended without a word about the remaining files: give up on them rather than loop
            _ if next < rest.len() => {
                errors.extend(rest[next..].iter().map(|f| (f.clone(), "the plugin probe stopped early".to_string())));
                break;
            }
            _ => break,
        }
    }
    (found, errors)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fx(kind: PluginKind, id: &str, label: &str, params: &[(&str, &str)]) -> PluginFx {
        PluginFx { kind, id: id.into(), label: label.into(), name: String::new(), params: params.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect() }
    }

    #[test]
    fn filter_text_of_each_kind() {
        let l = fx(PluginKind::Ladspa, "/usr/lib/ladspa/amp_1181.so", "amp_mono", &[("c0", "2"), ("bad", "1"), ("c1", "x")]);
        assert_eq!(
            plugin_text(&l).unwrap(),
            "ladspa=file=/usr/lib/ladspa/amp_1181.so:plugin=amp_mono:controls='c0=2',aresample,aformat=channel_layouts=mono|stereo|3.0|quad|5.0|5.1|6.1|7.1"
        );
        let l = fx(PluginKind::Ladspa, "/opt/my plugins/a.so", "a", &[]);
        assert_eq!(
            plugin_text(&l).unwrap(),
            "ladspa=file='/opt/my plugins/a.so':plugin=a,aresample,aformat=channel_layouts=mono|stereo|3.0|quad|5.0|5.1|6.1|7.1"
        );
        let v = fx(PluginKind::Lv2, "http://plugin.org.uk/swh-plugins/sc4", "", &[("attack", "10"), ("ratio", "4.5")]);
        assert_eq!(
            plugin_text(&v).unwrap(),
            r"lv2=plugin='http\://plugin.org.uk/swh-plugins/sc4':controls='attack=10|ratio=4.5',aresample,aformat=channel_layouts=mono|stereo|3.0|quad|5.0|5.1|6.1|7.1"
        );
        let f = fx(PluginKind::Frei0r, "glow", "", &[("0", "0.5"), ("1", "y"), ("3", "skipped")]);
        assert_eq!(plugin_text(&f).unwrap(), "frei0r=filter_name=glow:filter_params=0.5|y");
        let f = fx(PluginKind::Frei0r, "x", "", &[("0", "a b|c")]);
        assert_eq!(plugin_text(&f).unwrap(), r"frei0r=filter_name=x:filter_params='a\\ b\\|c'");
        assert!(plugin_text(&fx(PluginKind::Frei0r, "../x", "", &[])).is_none());
        assert!(plugin_text(&fx(PluginKind::Lv2, " ", "", &[])).is_none());
    }

    #[test]
    fn windows_ladspa_paths_start_with_a_slash() {
        assert_eq!(ladspa_file(r"C:\Plugins\amp.dll", true), "//./C:/Plugins/amp.dll");
        assert_eq!(ladspa_file(r"\\server\share\amp.dll", true), "//server/share/amp.dll");
        assert_eq!(ladspa_file("/usr/lib/ladspa/amp.so", false), "/usr/lib/ladspa/amp.so");
    }

    #[test]
    fn controls_without_a_default_get_a_value() {
        let c = |key: &str, min, max, default: Option<&str>| Control { min, max, strict: true, ..ctl(key, CtlKind::Float, default) };
        let p = info(
            PluginKind::Ladspa,
            vec![c("c0", Some(1.0), Some(5.0), None), c("c1", None, None, Some("3")), c("c2", Some(-5.0), Some(-1.0), None), c("c3", None, None, None)],
        );
        let f = p.new_fx();
        assert_eq!(f.params, [("c0".to_string(), "1".to_string()), ("c2".into(), "-1".into()), ("c3".into(), "0".into())]);
        assert!(p.is(&f));
    }

    fn ctl(key: &str, kind: CtlKind, default: Option<&str>) -> Control {
        Control {
            key: key.into(),
            name: key.into(),
            help: String::new(),
            kind,
            min: None,
            max: None,
            default: default.map(str::to_string),
            strict: false,
            log: false,
            sample_rate: false,
            choices: vec![],
        }
    }

    fn info(kind: PluginKind, controls: Vec<Control>) -> PluginInfo {
        PluginInfo {
            kind,
            id: "p".into(),
            label: String::new(),
            name: "P".into(),
            desc: String::new(),
            file: "p".into(),
            inputs: 1,
            outputs: 1,
            controls,
            problem: None,
        }
    }

    #[test]
    fn setting_controls_keeps_the_text_right() {
        let p = info(PluginKind::Lv2, vec![ctl("gain", CtlKind::Float, Some("0")), ctl("need", CtlKind::Float, None)]);
        let mut f = p.new_fx();
        assert_eq!(f.get("need"), Some("0"));
        p.set(&mut f, "gain", Some("2".into()));
        p.set(&mut f, "need", None);
        assert_eq!((f.get("gain"), f.get("need")), (Some("2"), Some("0")));
        p.set(&mut f, "gain", Some("0.0".into()));
        assert_eq!(f.get("gain"), None);
        assert_eq!(p.value(&f, &p.controls[0]), "0");

        let p = info(PluginKind::Frei0r, vec![ctl("0", CtlKind::Float, Some("0.5")), ctl("1", CtlKind::Toggle, Some("n")), ctl("2", CtlKind::Text, Some("a"))]);
        let mut f = p.new_fx();
        p.set(&mut f, "1", Some("y".into()));
        assert_eq!(plugin_text(&f).unwrap(), "frei0r=filter_name=p:filter_params=0.5|y");
        p.set(&mut f, "2", Some("b".into()));
        p.set(&mut f, "1", None);
        assert_eq!(plugin_text(&f).unwrap(), "frei0r=filter_name=p:filter_params=0.5|n|b");
        p.set(&mut f, "2", Some("a".into()));
        assert!(f.params.is_empty());
    }

    #[test]
    fn probe_lines() {
        assert!(matches!(parse_line(&format!("{MARK}begin\t3")), Some(Line::Begin(3))));
        assert!(matches!(parse_line(&format!("{MARK}done\t1\t[]")), Some(Line::Done(1, v)) if v.is_empty()));
        assert!(matches!(parse_line(&format!("{MARK}fail\t2\tno")), Some(Line::Fail(2, e)) if e == "no"));
        assert!(parse_line("begin\t1").is_none());
    }

    #[test]
    fn user_folders_come_first_with_their_subfolders() {
        let root = std::env::temp_dir().join(format!("ffcut-dirs-{}", std::process::id()));
        std::fs::create_dir_all(root.join("frei0r")).unwrap();
        std::fs::create_dir_all(root.join("b.lv2")).unwrap();
        std::fs::write(root.join("b.lv2/manifest.ttl"), "").unwrap();
        let d = Dirs::new(&[root.clone(), root.join("missing")]);
        assert_eq!(&d.frei0r[..2], [root.clone(), root.join("frei0r")]);
        assert_eq!(&d.lv2[..2], [root.clone(), root.join("frei0r")]);
        assert!(!d.ladspa.contains(&root.join("missing")));
        let env = d.env();
        assert_eq!(env[0].0, "LV2_PATH");
        assert_eq!(std::env::split_paths(&env[1].1).next(), Some(root.clone()));
        let _ = std::fs::remove_dir_all(root);
    }
}
